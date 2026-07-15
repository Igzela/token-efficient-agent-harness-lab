use std::path::Path;
use std::sync::{Arc, Mutex};

use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use engine::http_server::{build_axum_router, AxumApiState};
use engine::operator_decision::{
    OperatorDecisionAction, OperatorDecisionItem, OperatorDecisionQueue,
};
use engine::storage::local_product_store::LocalProductStore;
use serde_json::{json, Value};
use tempfile::tempdir;
use tower::ServiceExt;

#[derive(Clone)]
struct TestClock(Arc<Mutex<String>>);

impl TestClock {
    fn new(now: &str) -> Self {
        Self(Arc::new(Mutex::new(now.to_string())))
    }

    fn set(&self, now: &str) {
        *self.0.lock().unwrap() = now.to_string();
    }

    fn now(&self) -> String {
        self.0.lock().unwrap().clone()
    }
}

fn make_store(path: &Path, now: &str) -> (LocalProductStore, TestClock) {
    let clock = TestClock::new(now);
    let owner = clock.clone();
    let store = LocalProductStore::new_with_clock(path, move || owner.now()).unwrap();
    (store, clock)
}

fn create_run(store: &LocalProductStore) -> String {
    let plan = store
        .create_workflow_plan("queue-order", "pe3-repair", "operator", |ids, _| {
            Ok(json!({
                "status": "planned_read_only",
                "graph": {
                    "nodes": [],
                    "edges": [],
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id
                },
                "analysis": {},
                "boundaries": {"execution_authority": "disabled"}
            }))
        })
        .unwrap();
    store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "operator")
        .unwrap()["run_id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn request_approval(store: &LocalProductStore, run_id: &str, node_id: &str) {
    store
        .record_workflow_run_approval(
            run_id,
            node_id,
            "requested",
            "operator",
            Some("review"),
            None,
            None,
            None,
            Some("2026-07-11T02:00:00Z"),
        )
        .unwrap();
}

fn action_body(queue: &OperatorDecisionQueue, action: OperatorDecisionAction) -> Value {
    json!({
        "queue_sha256": queue.queue_sha256,
        "generated_at": queue.generated_at,
        "maximum_freshness_seconds": queue.maximum_freshness_seconds,
        "limit": queue.limit,
        "offset": queue.offset,
        "action": action,
        "confirm_action": true,
        "reason": "operator confirmed"
    })
}

async fn post_action(app: Router, item: &OperatorDecisionItem, body: Value) -> (StatusCode, Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/operator/decisions/{}/actions",
                    item.decision_id
                ))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

#[tokio::test]
async fn decision_reordering_within_the_same_page_fails_closed() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("local.db");
    let (store, clock) = make_store(&path, "2026-07-11T00:00:00Z");
    let run_id = create_run(&store);

    request_approval(&store, &run_id, "node-z");
    clock.set("2026-07-11T00:01:00Z");
    request_approval(&store, &run_id, "node-a");

    clock.set("2026-07-11T00:00:00Z");
    let bound_queue = store
        .operator_decision_queue("2026-07-11T00:00:00Z", 300, 100, 0)
        .unwrap();
    let target_key = format!("{run_id}:node-z:approval:approve");
    let target = bound_queue
        .items
        .iter()
        .find(|item| item.conflict_key == target_key)
        .cloned()
        .expect("target decision on bound page");
    let bound_position = bound_queue
        .items
        .iter()
        .position(|item| item.decision_id == target.decision_id)
        .unwrap();

    clock.set("2026-07-11T00:01:00Z");
    let current_queue = store
        .operator_decision_queue("2026-07-11T00:01:00Z", 300, 100, 0)
        .unwrap();
    let current_position = current_queue
        .items
        .iter()
        .position(|item| item.decision_id == target.decision_id)
        .unwrap();
    assert_ne!(bound_position, current_position);
    assert!(current_queue
        .items
        .iter()
        .any(|item| item.conflict_key == target_key));

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let (status, response) = post_action(
        app,
        &target,
        action_body(&bound_queue, OperatorDecisionAction::Approve),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{response}");
    assert_eq!(response["code"], "operator_decision_current_state_changed");

    let connection = rusqlite::Connection::open(path).unwrap();
    let approved: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM workflow_run_approvals WHERE decision = 'approved'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(approved, 0);
}

#[tokio::test]
async fn every_action_not_bound_to_the_decision_is_explicitly_fail_closed() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("local.db");
    let (store, _clock) = make_store(&path, "2026-07-11T00:00:00Z");
    let run_id = create_run(&store);
    request_approval(&store, &run_id, "node-z");
    let queue = store
        .operator_decision_queue("2026-07-11T00:00:00Z", 300, 100, 0)
        .unwrap();
    let item = queue
        .items
        .iter()
        .find(|item| item.recommended_action == Some(OperatorDecisionAction::Approve))
        .cloned()
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    for action in [
        OperatorDecisionAction::Rollback,
        OperatorDecisionAction::Inspect,
        OperatorDecisionAction::Acknowledge,
    ] {
        let (status, response) = post_action(app.clone(), &item, action_body(&queue, action)).await;
        assert_eq!(status, StatusCode::CONFLICT, "{response}");
        assert_eq!(response["code"], "operator_decision_not_ready");
    }
}
