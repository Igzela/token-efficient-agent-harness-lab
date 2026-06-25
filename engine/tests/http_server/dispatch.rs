use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use engine::http_server::{
    build_axum_router, build_axum_router_with_dashboard, AxumApiState, CliCapability,
};
use engine::infrastructure::auth::{Tenant, TenantResolver};
use engine::infrastructure::rate_limiter::RateLimiter;
use engine::provider::audit::{ProviderAuditEvent, PROVIDER_AUDIT_EVENT_SCHEMA_VERSION};
use engine::storage::local_product_store::LocalProductStore;
use serde_json::{json, Value};
use tempfile::{tempdir, TempDir};
use tokio::sync::Mutex;
use tower::ServiceExt;

use super::common::*;

#[tokio::test]
async fn axum_dispatch_returns_deterministic_noop_bundle() {
    let app = build_axum_router(AxumApiState::new());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/dispatch")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "raw_request": "Review this file without provider calls",
                        "request_source": "api"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["execution_result"]["executor_type"], "noop");
    assert_eq!(body["execution_result"]["status"], "not_executed");
    assert_eq!(body["record"]["final_status"], "not_executed");
}

#[tokio::test]
async fn axum_dispatch_rejects_empty_request() {
    let app = build_axum_router(AxumApiState::new());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/dispatch")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"raw_request": ""}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(response).await;
    assert_eq!(body["code"], "raw_request_required");
    assert_eq!(body["error"], "raw_request is required");
}

#[tokio::test]
async fn axum_dispatches_filters_by_search_query() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("dispatch.db")).unwrap();
    store
        .record_dispatch(
            "Alpha parser work",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-alpha", "final_status": "not_executed"},
                "decision": {"selected_tier": "balanced_worker", "budget_reservation": {"reserved_cost": 0.1}},
                "analysis": {"risk_level": "low"},
                "execution_result": {"executor_type": "noop"},
            }),
            "test",
        )
        .unwrap();
    store
        .record_dispatch(
            "Beta docs review",
            "dashboard",
            &json!({
                "record": {"dispatch_id": "disp-beta", "final_status": "not_executed"},
                "decision": {"selected_tier": "cheap_executor", "budget_reservation": {"reserved_cost": 0.1}},
                "analysis": {"risk_level": "low"},
                "execution_result": {"executor_type": "noop"},
            }),
            "test",
        )
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/dispatches?search=alpha&limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    let dispatches = body["dispatches"].as_array().unwrap();
    assert_eq!(dispatches.len(), 1);
    assert_eq!(dispatches[0]["dispatch_id"], "disp-alpha");
}

#[tokio::test]
async fn axum_cost_details_clamps_negative_limit() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    store
        .record_dispatch(
            "Cost row",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-cost", "final_status": "not_executed"},
                "decision": {"selected_tier": "balanced_worker", "budget_reservation": {"reserved_cost": 0.1}},
                "analysis": {"risk_level": "low"},
                "execution_result": {"executor_type": "noop"},
            }),
            "test",
        )
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/costs/dispatches?limit=-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert!(body["dispatches"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn axum_dispatch_detail_returns_bundle_for_existing_dispatch() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("dispatch.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    // Create a dispatch first
    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/dispatch")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "test dispatch", "request_source": "api"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::OK);
    let create_body = response_json(create_resp).await;
    let dispatch_id = create_body["record"]["dispatch_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Get detail
    let detail_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/dispatches/{dispatch_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detail_resp.status(), StatusCode::OK);
    let detail_body = response_json(detail_resp).await;
    assert_eq!(detail_body["dispatch"]["dispatch_id"], dispatch_id);
    assert!(detail_body["dispatch"]["bundle"].is_object());
}

#[tokio::test]
async fn axum_dispatch_detail_returns_404_for_missing() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("dispatch.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/dispatches/nonexistent-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn axum_queue_status_returns_200() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("queue.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/queue/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["schema_version"], "axum_api.v1");
    assert!(body["queue"].is_object());
    assert_eq!(body["queue"]["backpressure_active"], false);
}

#[tokio::test]
async fn axum_queue_runs_returns_array() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("queue-runs.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/queue/runs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert!(body["runs"].as_array().is_some());
    assert_eq!(body["limit"], 50);
    assert_eq!(body["offset"], 0);
}

#[tokio::test]
async fn axum_queue_runs_respects_limit_offset() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("queue-page.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/queue/runs?limit=10&offset=5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["limit"], 10);
    assert_eq!(body["offset"], 5);
}

fn create_plan_and_run(store: &LocalProductStore) -> String {
    store
        .create_workflow_plan("Queue test request", "test", "test-actor", |ids, _| {
            Ok(json!({
                "schema_version": "read_only_plan.v1",
                "plan_id": ids.plan_id,
                "status": "planned_read_only",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "analysis": {"analysis_id": "a-1", "task_domain": "test"},
                "graph": {
                    "schema_version": "workflow_graph.v1",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "status": "decomposed",
                    "created_at": "2026-06-07T00:00:00Z",
                    "updated_at": "2026-06-07T00:00:00Z",
                    "nodes": [
                        {"node_id": "node-a", "task_type": "implementation", "status": "pending"}
                    ],
                    "edges": []
                },
                "boundaries": {
                    "execution_authority": "disabled",
                    "target_repository_writes": "disabled",
                    "runtime_workers": "disabled",
                },
            }))
        })
        .expect("failed to create plan");
    let run = store
        .create_workflow_run_from_plan("plan-0001", "test-user")
        .unwrap();
    run["run_id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn axum_queue_update_run_priority() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("queue-pri.db")).unwrap();
    let run_id = create_plan_and_run(&store);
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/v1/queue/runs/{run_id}/priority"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"priority": 3}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["ok"], true);
    assert_eq!(body["priority"], 3);
    assert_eq!(body["run_id"], run_id);
}

#[tokio::test]
async fn axum_queue_update_priority_rejects_invalid() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("queue-pri-invalid.db")).unwrap();
    let run_id = create_plan_and_run(&store);
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/v1/queue/runs/{run_id}/priority"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"priority": 0}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(response).await;
    assert_eq!(body["code"], "invalid_priority");

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/v1/queue/runs/{run_id}/priority"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"priority": 11}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn axum_queue_set_and_clear_pause() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("queue-pause.db")).unwrap();
    let run_id = create_plan_and_run(&store);
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/v1/queue/runs/{run_id}/pause"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"reason": "rate limit"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["pause_reason"], "rate limit");

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/v1/queue/runs/{run_id}/pause"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["pause_reason"], Value::Null);
}

#[tokio::test]
async fn axum_queue_tenants_returns_array() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("queue-tenants.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/queue/tenants")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert!(body["tenants"].as_array().is_some());
}

#[tokio::test]
async fn test_dispatch_metrics_empty_store() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("metrics-empty.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/dispatch-metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["schema_version"], "axum_api.v1");
    assert_eq!(body["metrics"]["totals"]["dispatch_count"], 0);
    assert_eq!(body["metrics"]["totals"]["success_count"], 0);
}

#[tokio::test]
async fn test_dispatch_metrics_with_data() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("metrics-data.db");

    {
        let store = LocalProductStore::new(&db_path).unwrap();
        let app = build_axum_router(AxumApiState::new().with_local_store(store));
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/dispatch")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "raw_request": "Test dispatch for metrics",
                            "request_source": "api"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    let store = LocalProductStore::new(&db_path).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/dispatch-metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert!(
        body["metrics"]["totals"]["dispatch_count"]
            .as_i64()
            .unwrap()
            >= 1
    );
}

