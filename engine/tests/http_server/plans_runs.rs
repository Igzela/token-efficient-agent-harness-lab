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
async fn axum_create_read_only_plan_persists_workflow_graph_without_execution() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("plans.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "raw_request": "Plan a docs migration without execution",
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
    assert_eq!(body["schema_version"], "axum_api.v1");
    assert_eq!(body["plan"]["schema_version"], "read_only_plan.v1");
    assert_eq!(body["plan"]["plan_id"], "plan-0001");
    assert_eq!(body["plan"]["status"], "planned_read_only");
    assert_eq!(body["plan"]["graph"]["status"], "decomposed");
    assert!(!body["plan"]["graph"]["nodes"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(body["plan"]["boundaries"]["execution"], "disabled");
    assert_eq!(
        body["plan"]["boundaries"]["target_repository_writes"],
        "disabled"
    );
    assert_eq!(
        body["plan"]["boundaries"]["runtime_workers"],
        "env_gated_supervised"
    );
    assert_eq!(
        body["plan"]["advisory"]["schema_version"],
        "plan_advisory.v1"
    );
    assert_eq!(body["plan"]["advisory"]["mode"], "recommendation_only");
    assert_eq!(
        body["plan"]["advisory"]["decision"]["execution_allowed"],
        false
    );
    assert_eq!(
        body["plan"]["advisory"]["routing"]["adaptive_routing_available"],
        false
    );
    assert!(body["plan"].get("execution_result").is_none());

    let list = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/plans?search=docs&limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let list_body = response_json(list).await;
    assert_eq!(list_body["plans"].as_array().unwrap().len(), 1);
    assert_eq!(list_body["plans"][0]["plan_id"], "plan-0001");
    assert_eq!(
        list_body["plans"][0]["advisory"]["retry"]["provider_invocation"],
        "not_invoked"
    );
}

#[tokio::test]
async fn axum_get_read_only_plan_by_id_and_missing_id() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("plans.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"raw_request": "Plan only"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);

    let fetched = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/plans/plan-0001")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(fetched.status(), StatusCode::OK);
    let body = response_json(fetched).await;
    assert_eq!(body["plan"]["plan_id"], "plan-0001");

    let missing = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/plans/plan-missing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    let body = response_json(missing).await;
    assert_eq!(body["code"], "plan_not_found");
}

#[tokio::test]
async fn axum_workflow_runs_persist_inert_state_from_plan() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("runs.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let created_plan = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"raw_request": "Plan only"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created_plan.status(), StatusCode::OK);
    let plan_body = response_json(created_plan).await;
    let node_id = plan_body["plan"]["graph"]["nodes"][0]["node_id"]
        .as_str()
        .unwrap()
        .to_string();

    let created_run = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": "plan-0001"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created_run.status(), StatusCode::OK);
    let run_body = response_json(created_run).await;
    assert_eq!(run_body["schema_version"], "axum_api.v1");
    assert_eq!(run_body["run"]["schema_version"], "workflow_run.v1");
    assert_eq!(run_body["run"]["run_id"], "run-0001");
    assert_eq!(run_body["run"]["status"], "created");
    assert_eq!(
        run_body["run"]["boundaries"]["execution_authority"],
        "disabled"
    );
    assert_eq!(
        run_body["run"]["boundaries"]["runtime_workers"],
        "env_gated_supervised"
    );
    assert!(run_body["run"].get("execution_result").is_none());
    assert!(!run_body["run"]["nodes"].as_array().unwrap().is_empty());

    let event = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs/run-0001/events")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "node_id": node_id,
                        "event_type": "node_status_observed",
                        "details": {"status": "ready"}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(event.status(), StatusCode::OK);
    let event_body = response_json(event).await;
    assert_eq!(event_body["event"]["event_type"], "node_status_observed");

    let approval = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs/run-0001/approvals")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "node_id": node_id,
                        "decision": "approved",
                        "reason": "metadata only"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approval.status(), StatusCode::OK);
    let approval_body = response_json(approval).await;
    assert_eq!(approval_body["approval"]["decision"], "approved");

    let resumed = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs/run-0001/resume")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"reason": "metadata resume"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resumed.status(), StatusCode::OK);
    let resumed_body = response_json(resumed).await;
    assert_eq!(resumed_body["run"]["status"], "running");
    assert_eq!(
        resumed_body["run"]["boundaries"]["resume_execution_authority"],
        "disabled"
    );

    let cancelled = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs/run-0001/cancel")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"reason": "metadata cancel"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cancelled.status(), StatusCode::OK);
    let cancelled_body = response_json(cancelled).await;
    assert_eq!(cancelled_body["run"]["status"], "cancelled");
    assert_eq!(
        cancelled_body["run"]["boundaries"]["cancel_execution_authority"],
        "disabled"
    );

    let detail = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/workflow-runs/run-0001")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detail.status(), StatusCode::OK);
    let detail_body = response_json(detail).await;
    assert_eq!(detail_body["run"]["events"].as_array().unwrap().len(), 4);
    assert_eq!(detail_body["run"]["approvals"].as_array().unwrap().len(), 1);

    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/workflow-runs?search=plan-0001")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let list_body = response_json(list).await;
    assert_eq!(list_body["runs"].as_array().unwrap().len(), 1);

    let events = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/workflow-runs/run-0001/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(events.status(), StatusCode::OK);
    let events_body = response_json(events).await;
    assert_eq!(events_body["events"].as_array().unwrap().len(), 4);

    let approvals = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/workflow-runs/run-0001/approvals")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approvals.status(), StatusCode::OK);
    let approvals_body = response_json(approvals).await;
    assert_eq!(approvals_body["approvals"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn axum_workflow_run_child_lists_return_404_for_missing_run() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("runs.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let events = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/workflow-runs/run-missing/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(events.status(), StatusCode::NOT_FOUND);
    let events_body = response_json(events).await;
    assert_eq!(events_body["code"], "workflow_run_not_found");

    let approvals = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/workflow-runs/run-missing/approvals")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approvals.status(), StatusCode::NOT_FOUND);
    let approvals_body = response_json(approvals).await;
    assert_eq!(approvals_body["code"], "workflow_run_not_found");
}

#[tokio::test]
async fn axum_workflow_runs_require_dispatch_read_scope_when_auth_configured() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("runs.db")).unwrap();
    let mut resolver = TenantResolver::new();
    let scopes = HashSet::from(["health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "tenant-a".to_string(),
        name: "Tenant A".to_string(),
        scopes: HashSet::from(["health:read".to_string(), "dispatch:read".to_string()]),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("tenant-a", Some(scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store).with_auth(
        resolver,
        RateLimiter::new(60.0, 100),
        Some(100),
        1.0,
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/workflow-runs")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn axum_plans_require_dispatch_read_scope_when_auth_configured() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("plans.db")).unwrap();
    let mut resolver = TenantResolver::new();
    let scopes = HashSet::from(["health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "tenant-a".to_string(),
        name: "Tenant A".to_string(),
        scopes: HashSet::from(["health:read".to_string(), "dispatch:read".to_string()]),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("tenant-a", Some(scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store).with_auth(
        resolver,
        RateLimiter::new(60.0, 100),
        Some(100),
        1.0,
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"raw_request": "Plan only"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
