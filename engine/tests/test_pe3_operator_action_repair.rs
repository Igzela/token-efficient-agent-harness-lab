use std::path::{Path, PathBuf};
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

fn create_run(store: &LocalProductStore, name: &str, nodes: Value, edges: Value) -> String {
    let plan = store
        .create_workflow_plan(name, "pe3-repair", "operator", |ids, _| {
            Ok(json!({
                "status": "planned_read_only",
                "graph": {
                    "nodes": nodes,
                    "edges": edges,
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

fn item_for_action(
    queue: &OperatorDecisionQueue,
    action: OperatorDecisionAction,
) -> OperatorDecisionItem {
    queue
        .items
        .iter()
        .find(|item| item.recommended_action == Some(action))
        .cloned()
        .unwrap()
}

fn action_body(
    queue: &OperatorDecisionQueue,
    action: OperatorDecisionAction,
    generated_at: &str,
) -> Value {
    json!({
        "queue_sha256": queue.queue_sha256,
        "generated_at": generated_at,
        "maximum_freshness_seconds": queue.maximum_freshness_seconds,
        "limit": queue.limit,
        "offset": queue.offset,
        "action": action,
        "confirm_action": true,
        "reason": "operator confirmed"
    })
}

async fn post_action(app: Router, decision_id: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/operator/decisions/{decision_id}/actions"))
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

fn approval_fixture(
    observed_at: &str,
    current_at: &str,
) -> (
    tempfile::TempDir,
    PathBuf,
    Router,
    OperatorDecisionQueue,
    OperatorDecisionItem,
) {
    let directory = tempdir().unwrap();
    let path = directory.path().join("local.db");
    let (store, clock) = make_store(&path, observed_at);
    let run = create_run(&store, "approval", json!([]), json!([]));
    request_approval(&store, &run, "node-z");
    let queue = store
        .operator_decision_queue(observed_at, 300, 100, 0)
        .unwrap();
    let item = item_for_action(&queue, OperatorDecisionAction::Approve);
    clock.set(current_at);
    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    (directory, path, app, queue, item)
}

#[tokio::test]
async fn mutation_accepts_exact_freshness_boundary_and_current_valid_execution() {
    let (_directory, path, app, queue, item) =
        approval_fixture("2026-07-11T00:00:00Z", "2026-07-11T00:05:00Z");
    let (status, body) = post_action(
        app,
        &item.decision_id,
        action_body(&queue, OperatorDecisionAction::Approve, &queue.generated_at),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["action"], "approve");

    let connection = rusqlite::Connection::open(path).unwrap();
    let approved: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM workflow_run_approvals WHERE decision = 'approved'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(approved, 1);
}

#[tokio::test]
async fn mutation_rejects_expired_and_future_queue_timestamps() {
    let (_directory, _path, app, queue, item) =
        approval_fixture("2026-07-11T00:00:00Z", "2026-07-11T00:05:01Z");
    let (status, body) = post_action(
        app,
        &item.decision_id,
        action_body(&queue, OperatorDecisionAction::Approve, &queue.generated_at),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        body["error"]["code"],
        "operator_decision_generated_at_stale"
    );

    let (_directory, _path, app, queue, item) =
        approval_fixture("2026-07-11T00:05:01Z", "2026-07-11T00:05:00Z");
    let (status, body) = post_action(
        app,
        &item.decision_id,
        action_body(&queue, OperatorDecisionAction::Approve, &queue.generated_at),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        body["error"]["code"],
        "operator_decision_generated_at_future"
    );
}

#[tokio::test]
async fn source_resolution_after_read_invalidates_hash_and_decision() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("local.db");
    let (store, clock) = make_store(&path, "2026-07-11T00:00:00Z");
    let run = create_run(&store, "approval", json!([]), json!([]));
    request_approval(&store, &run, "node-z");
    let queue = store
        .operator_decision_queue("2026-07-11T00:00:00Z", 300, 100, 0)
        .unwrap();
    let item = item_for_action(&queue, OperatorDecisionAction::Approve);
    clock.set("2026-07-11T00:01:00Z");
    store
        .record_workflow_run_approval(
            &run,
            "node-z",
            "approved",
            "other-operator",
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let (status, body) = post_action(
        app,
        &item.decision_id,
        action_body(&queue, OperatorDecisionAction::Approve, &queue.generated_at),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "operator_decision_queue_changed");
}

#[tokio::test]
async fn source_change_and_exact_page_order_change_fail_closed() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("local.db");
    let (store, clock) = make_store(&path, "2026-07-11T00:00:00Z");
    let run = create_run(&store, "approval", json!([]), json!([]));
    request_approval(&store, &run, "node-z");
    let queue = store
        .operator_decision_queue("2026-07-11T00:00:00Z", 300, 1, 0)
        .unwrap();
    let item = queue.items[0].clone();

    clock.set("2026-07-11T00:01:00Z");
    request_approval(&store, &run, "node-a");
    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let (status, body) = post_action(
        app,
        &item.decision_id,
        action_body(
            &queue,
            item.recommended_action.unwrap(),
            &queue.generated_at,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "operator_decision_queue_changed");
}

#[tokio::test]
async fn queue_hash_and_decision_id_replay_are_rejected() {
    let (_directory, _path, app, queue, item) =
        approval_fixture("2026-07-11T00:00:00Z", "2026-07-11T00:00:10Z");
    let mut bad_hash = action_body(&queue, OperatorDecisionAction::Approve, &queue.generated_at);
    bad_hash["queue_sha256"] = json!("00".repeat(32));
    let (status, body) = post_action(app.clone(), &item.decision_id, bad_hash).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "operator_decision_queue_changed");

    let (status, body) = post_action(
        app,
        "operator-decision-deadbeef",
        action_body(&queue, OperatorDecisionAction::Approve, &queue.generated_at),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "operator_decision_not_found");
}

#[tokio::test]
async fn reject_uses_its_exact_source_and_resolves_the_pending_approval() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("local.db");
    let (store, clock) = make_store(&path, "2026-07-11T00:00:00Z");
    let run = create_run(&store, "approval", json!([]), json!([]));
    request_approval(&store, &run, "node-z");
    let queue = store
        .operator_decision_queue("2026-07-11T00:00:00Z", 300, 100, 0)
        .unwrap();
    let item = item_for_action(&queue, OperatorDecisionAction::Reject);
    clock.set("2026-07-11T00:00:10Z");
    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let (status, body) = post_action(
        app,
        &item.decision_id,
        action_body(&queue, OperatorDecisionAction::Reject, &queue.generated_at),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let connection = rusqlite::Connection::open(path).unwrap();
    let rejected: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM workflow_run_approvals WHERE decision = 'rejected'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rejected, 1);
}

#[tokio::test]
async fn manual_resume_clears_pause_and_repeated_action_is_fail_closed() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("local.db");
    let (store, clock) = make_store(&path, "2026-07-11T00:00:00Z");
    let run = create_run(&store, "resume", json!([]), json!([]));
    store.update_run_pause_reason(&run, Some("manual")).unwrap();
    let queue = store
        .operator_decision_queue("2026-07-11T00:00:00Z", 300, 100, 0)
        .unwrap();
    let item = item_for_action(&queue, OperatorDecisionAction::Resume);
    clock.set("2026-07-11T00:00:10Z");
    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let body = action_body(&queue, OperatorDecisionAction::Resume, &queue.generated_at);
    let (status, response) = post_action(app.clone(), &item.decision_id, body.clone()).await;
    assert_eq!(status, StatusCode::OK, "{response}");

    let (status, response) = post_action(app, &item.decision_id, body).await;
    assert_eq!(status, StatusCode::CONFLICT, "{response}");

    let connection = rusqlite::Connection::open(path).unwrap();
    let (status, pause_reason): (String, Option<String>) = connection
        .query_row(
            "SELECT status, pause_reason FROM workflow_runs WHERE run_id = ?1",
            [&run],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(status, "running");
    assert!(pause_reason.is_none());
}

#[tokio::test]
async fn blocked_retry_executes_once_then_old_binding_cannot_replay() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("local.db");
    let (store, clock) = make_store(&path, "2026-07-11T00:00:00Z");
    let run = create_run(
        &store,
        "retry",
        json!([{"node_id": "n1", "task_type": "noop", "status": "pending"}]),
        json!([]),
    );
    {
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute(
                "UPDATE workflow_runs SET status = 'blocked', updated_at = ?1 WHERE run_id = ?2",
                rusqlite::params!["2026-07-11T00:00:00Z", run],
            )
            .unwrap();
    }
    let queue = store
        .operator_decision_queue("2026-07-11T00:00:00Z", 300, 100, 0)
        .unwrap();
    let item = item_for_action(&queue, OperatorDecisionAction::Retry);
    clock.set("2026-07-11T00:00:10Z");
    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let body = action_body(&queue, OperatorDecisionAction::Retry, &queue.generated_at);
    let (status, response) = post_action(app.clone(), &item.decision_id, body.clone()).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    let (status, response) = post_action(app, &item.decision_id, body).await;
    assert_eq!(status, StatusCode::CONFLICT, "{response}");

    let connection = rusqlite::Connection::open(path).unwrap();
    let attempt_count: i64 = connection
        .query_row(
            "SELECT attempt_count FROM workflow_run_nodes WHERE run_id = ?1 AND node_id = 'n1'",
            [&run],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(attempt_count, 1);
}
