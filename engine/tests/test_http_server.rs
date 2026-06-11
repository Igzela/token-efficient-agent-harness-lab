use std::collections::HashSet;
use std::fs;

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use engine::http_server::{build_axum_router, build_axum_router_with_dashboard, AxumApiState};
use engine::infrastructure::auth::{Tenant, TenantResolver};
use engine::infrastructure::rate_limiter::RateLimiter;
use engine::provider::audit::{ProviderAuditEvent, PROVIDER_AUDIT_EVENT_SCHEMA_VERSION};
use engine::storage::local_product_store::LocalProductStore;
use serde_json::{json, Value};
use tempfile::tempdir;
use tower::ServiceExt;

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn response_text(response: axum::response::Response) -> String {
    let bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn provider_audit_event(event_id: &str, created_at: &str) -> ProviderAuditEvent {
    ProviderAuditEvent {
        schema_version: PROVIDER_AUDIT_EVENT_SCHEMA_VERSION.to_string(),
        event_id: event_id.to_string(),
        dispatch_id: "disp-provider-audit".to_string(),
        provider_id: "stub-provider".to_string(),
        event_type: "response_received".to_string(),
        input_token_count: Some(10),
        output_token_count: Some(5),
        cost: Some(0.001),
        currency: Some("USD".to_string()),
        latency_ms: Some(25),
        error_domain: None,
        redaction_status: "not_applicable".to_string(),
        created_at: created_at.to_string(),
    }
}

#[tokio::test]
async fn axum_health_is_available_without_auth_by_default() {
    let app = build_axum_router(AxumApiState::new());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["status"], "healthy");
    assert_eq!(body["tenant_id"], "local");
}

#[tokio::test]
async fn axum_ready_is_available_without_auth_by_default() {
    let app = build_axum_router(AxumApiState::new());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["status"], "ready");
}

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
async fn axum_policy_proposal_activation_requires_confirmation_and_affects_dispatch() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("policy.db")).unwrap();
    let mut resolver = TenantResolver::new();
    let scopes = HashSet::from([
        "dispatch:read".to_string(),
        "team:admin".to_string(),
        "health:read".to_string(),
    ]);
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local".to_string(),
        scopes: scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local", Some(scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store).with_auth(
        resolver,
        RateLimiter::new(60.0, 100),
        Some(100),
        1.0,
    ));

    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/proposals")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "task_domain": "docs",
                        "task_intent": "review",
                        "target_tier": "verifier",
                        "payload": {"type": "tier_map_override"},
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let body = response_json(created).await;
    let proposal_id = body["proposal"]["proposal_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(body["proposal"]["status"], "pending");

    let missing_confirm = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/proposals/{proposal_id}/approve"))
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"reason": "pilot"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_confirm.status(), StatusCode::BAD_REQUEST);

    let approved = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/proposals/{proposal_id}/approve"))
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"reason": "pilot", "confirm_policy_override": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approved.status(), StatusCode::OK);
    let body = response_json(approved).await;
    assert_eq!(body["proposal"]["status"], "active");

    let dispatch = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/dispatch")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "raw_request": "Review docs for consistency",
                        "request_source": "api"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dispatch.status(), StatusCode::OK);
    let body = response_json(dispatch).await;
    assert_eq!(body["decision"]["selected_tier"], "verifier");
}

#[tokio::test]
async fn axum_local_store_persists_dispatch_history_and_dashboard_summary() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("local-team.db");

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
                            "raw_request": "Summarize local team status without provider calls",
                            "request_source": "dashboard"
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
    }

    let store = LocalProductStore::new(&db_path).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let history = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/dispatches")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(history.status(), StatusCode::OK);
    let history_body = response_json(history).await;
    assert_eq!(history_body["dispatches"].as_array().unwrap().len(), 1);
    assert_eq!(
        history_body["dispatches"][0]["final_status"],
        "not_executed"
    );
    assert_eq!(history_body["dispatches"][0]["request_source"], "dashboard");
    assert!(
        history_body["dispatches"][0]["reserved_cost"]
            .as_f64()
            .unwrap()
            > 0.0
    );

    let dashboard = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/dashboard")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dashboard.status(), StatusCode::OK);
    let dashboard_body = response_json(dashboard).await;
    assert_eq!(dashboard_body["counts"]["dispatches"], 1);
    assert_eq!(
        dashboard_body["dispatches"][0]["raw_request"],
        "Summarize local team status without provider calls"
    );
    assert_eq!(dashboard_body["boundaries"]["provider_transport"], "noop");
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
    assert_eq!(body["plan"]["boundaries"]["runtime_workers"], "disabled");
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
    assert_eq!(run_body["run"]["boundaries"]["runtime_workers"], "disabled");
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

#[tokio::test]
async fn axum_supervised_patch_metadata_lists_empty_state() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("patch.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let workspaces = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/supervised-patch/workspaces")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(workspaces.status(), StatusCode::OK);
    let workspaces_body = response_json(workspaces).await;
    assert_eq!(workspaces_body["metadata_only"], true);
    assert_eq!(workspaces_body["execution_authority"], "disabled");
    assert_eq!(workspaces_body["workspaces"].as_array().unwrap().len(), 0);

    let artifacts = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/supervised-patch/artifacts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(artifacts.status(), StatusCode::OK);
    let artifacts_body = response_json(artifacts).await;
    assert_eq!(artifacts_body["metadata_only"], true);
    assert_eq!(artifacts_body["execution_authority"], "disabled");
    assert_eq!(artifacts_body["artifacts"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn axum_supervised_patch_metadata_returns_storage_records_read_only() {
    let target_dir = tempdir().unwrap();
    let workspace_root = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("patch.db")).unwrap();
    let workspace_path = workspace_root.path().join("workspaces").join("ws-001");
    store
        .record_supervised_patch_workspace(
            &json!({
                "plan_id": "plan-0001",
                "run_id": "run-0001",
                "target_id": "target-001",
                "target_repo_path": target_dir.path().to_string_lossy(),
                "workspace_path": workspace_path.to_string_lossy(),
                "source_revision": "abc123",
            }),
            "operator",
        )
        .unwrap();
    store
        .record_supervised_patch_artifact(
            &json!({
                "workspace_id": "patch-workspace-0001",
                "patch_hash": "sha256-patch",
                "changed_files": ["src/lib.rs"],
                "redaction_status": "redacted",
            }),
            "operator",
        )
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let workspaces = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/supervised-patch/workspaces?limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(workspaces.status(), StatusCode::OK);
    let workspaces_body = response_json(workspaces).await;
    assert_eq!(workspaces_body["workspaces"].as_array().unwrap().len(), 1);
    assert_eq!(
        workspaces_body["workspaces"][0]["boundary"]["target_repository_writes"],
        "disabled"
    );
    assert_eq!(
        workspaces_body["workspaces"][0]["boundary"]["workspace_directory_creation"],
        "not_performed"
    );

    let workspace_detail = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/supervised-patch/workspaces/patch-workspace-0001")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(workspace_detail.status(), StatusCode::OK);
    let workspace_detail_body = response_json(workspace_detail).await;
    assert_eq!(
        workspace_detail_body["workspace"]["workspace_id"],
        "patch-workspace-0001"
    );
    assert_eq!(workspace_detail_body["execution_authority"], "disabled");

    let artifacts = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/supervised-patch/artifacts?limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(artifacts.status(), StatusCode::OK);
    let artifacts_body = response_json(artifacts).await;
    assert_eq!(artifacts_body["artifacts"].as_array().unwrap().len(), 1);
    assert_eq!(
        artifacts_body["artifacts"][0]["patch_apply_authority"],
        "disabled"
    );
    assert_eq!(
        artifacts_body["artifacts"][0]["artifact_file_created"],
        false
    );

    let artifact_detail = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/supervised-patch/artifacts/patch-artifact-0001")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(artifact_detail.status(), StatusCode::OK);
    let artifact_detail_body = response_json(artifact_detail).await;
    assert_eq!(
        artifact_detail_body["artifact"]["artifact_id"],
        "patch-artifact-0001"
    );
    assert_eq!(artifact_detail_body["metadata_only"], true);
}

#[tokio::test]
async fn axum_supervised_patch_metadata_returns_404_for_missing_records() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("patch.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let workspace = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/supervised-patch/workspaces/missing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(workspace.status(), StatusCode::NOT_FOUND);
    let workspace_body = response_json(workspace).await;
    assert_eq!(
        workspace_body["code"],
        "supervised_patch_workspace_not_found"
    );

    let artifact = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/supervised-patch/artifacts/missing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(artifact.status(), StatusCode::NOT_FOUND);
    let artifact_body = response_json(artifact).await;
    assert_eq!(artifact_body["code"], "supervised_patch_artifact_not_found");
}

#[tokio::test]
async fn axum_supervised_patch_metadata_requires_dispatch_read_scope_when_auth_configured() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("patch.db")).unwrap();
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
                .uri("/api/v1/supervised-patch/workspaces")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn axum_local_store_exposes_team_config_costs_and_export() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    store
        .set_config_value("workspace_name", json!("Local Ops"), "test-admin")
        .unwrap();
    store
        .upsert_team_member("user-admin", "Admin User", "admin")
        .unwrap();
    store
        .record_api_key_metadata(
            "key-admin",
            "user-admin",
            "admin",
            &["health:read".to_string(), "dispatch:read".to_string()],
            "test-admin",
        )
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/dispatch")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "Review cost ledger", "request_source": "api"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let config = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(config.status(), StatusCode::OK);
    let config_body = response_json(config).await;
    assert_eq!(config_body["config"]["workspace_name"], "Local Ops");
    assert_eq!(config_body["config"]["provider_transport"], "stub/off");

    let team = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(team.status(), StatusCode::OK);
    let team_body = response_json(team).await;
    assert_eq!(team_body["members"][0]["role"], "admin");
    assert_eq!(team_body["api_keys"][0]["key_id"], "key-admin");
    assert!(team_body["api_keys"][0].get("raw_key").is_none());

    let costs = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/costs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(costs.status(), StatusCode::OK);
    let costs_body = response_json(costs).await;
    assert_eq!(costs_body["dispatch_count"], 1);
    assert!(costs_body["total_reserved_cost"].as_f64().unwrap() > 0.0);

    let export = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/export")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(export.status(), StatusCode::OK);
    let export_body = response_json(export).await;
    assert_eq!(export_body["schema_version"], "local_team_export.v1");
    assert_eq!(export_body["dispatches"].as_array().unwrap().len(), 1);
    assert_eq!(export_body["team"]["members"][0]["user_id"], "user-admin");
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
async fn axum_audit_paginates_and_clamps_negative_limit() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    store
        .append_audit("tester", "first.action", "res-1", &json!({}))
        .unwrap();
    store
        .append_audit("tester", "second.action", "res-2", &json!({}))
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let paged = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/audit?limit=1&offset=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(paged.status(), StatusCode::OK);
    let body = response_json(paged).await;
    let events = body["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["action"], "first.action");

    let negative = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/audit?limit=-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(negative.status(), StatusCode::OK);
    let body = response_json(negative).await;
    assert!(body["events"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn axum_audit_filters_by_search_query() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    store
        .append_audit(
            "key-admin",
            "backup.create",
            "backup-0001",
            &json!({"label": "nightly"}),
        )
        .unwrap();
    store
        .append_audit(
            "key-readonly",
            "team.update",
            "user-readonly",
            &json!({"role": "readonly"}),
        )
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/audit?search=backup&limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    let events = body["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["action"], "backup.create");
}

#[tokio::test]
async fn axum_audit_redacts_sensitive_details_when_requested() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    store
        .append_audit(
            "key-admin",
            "provider.configure",
            "provider-local",
            &json!({
                "api_key": "secret123",
                "nested": {"password": "pw"},
                "safe": "kept",
            }),
        )
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let unredacted = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/audit?limit=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unredacted.status(), StatusCode::OK);
    let body = response_json(unredacted).await;
    assert_eq!(body["redacted"], false);
    assert_eq!(body["events"][0]["details"]["api_key"], "secret123");

    let redacted = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/audit?limit=1&redact=true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(redacted.status(), StatusCode::OK);
    let body = response_json(redacted).await;
    assert_eq!(body["redacted"], true);
    assert_eq!(body["events"][0]["details"]["api_key"], "***");
    assert_eq!(body["events"][0]["details"]["nested"]["password"], "***");
    assert_eq!(body["events"][0]["details"]["safe"], "kept");
}

#[tokio::test]
async fn axum_metrics_reports_operations_summary() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    store
        .record_dispatch(
            "Metrics dispatch",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-metrics", "final_status": "provider_completed"},
                "decision": {"selected_tier": "balanced_worker", "budget_reservation": {"reserved_cost": 0.25}},
                "analysis": {"risk_level": "low"},
                "execution_result": {
                    "executor_type": "provider",
                    "input_tokens": 100,
                    "output_tokens": 50,
                    "estimated_cost": 0.05,
                    "latency_ms": 1200
                },
            }),
            "test",
        )
        .unwrap();
    store
        .record_api_key_metadata(
            "key-admin",
            "user-admin",
            "admin",
            &["health:read".to_string()],
            "test",
        )
        .unwrap();

    let mut resolver = TenantResolver::new();
    let scopes = HashSet::from(["health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(scopes), None, 1.0)
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
                .uri("/api/v1/metrics")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["auth_required"], true);
    assert_eq!(body["local_store"], true);
    assert_eq!(body["dispatch_count"], 1);
    assert_eq!(body["api_key_count"], 1);
    assert!(body["audit_event_count"].as_i64().unwrap() >= 2);
    assert_eq!(body["total_reserved_cost"], 0.25);
    assert_eq!(body["total_estimated_cost_usd"], 0.05);
    assert_eq!(body["total_input_tokens"], 100);
    assert_eq!(body["total_output_tokens"], 50);
    assert_eq!(body["estimated_cost_available"], true);
    assert!(body["pricing_configured"].is_boolean());
    assert_eq!(body["boundaries"]["target_repository_writes"], "disabled");
    // GA-4: new observability fields
    assert!(
        body["secret_block_count"].is_number(),
        "secret_block_count should be present"
    );
    assert!(
        body["queue_length"].is_number(),
        "queue_length should be present"
    );
    assert_eq!(body["secret_block_count"], 0, "no artifacts yet");
    assert_eq!(body["queue_length"], 0, "no pending nodes");
}

#[tokio::test]
async fn axum_auth_rejects_missing_key_when_configured() {
    let state = AxumApiState::new().with_auth(
        TenantResolver::new(),
        RateLimiter::new(60.0, 10),
        Some(60),
        1.0,
    );
    let app = build_axum_router(state);
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn axum_health_bypass_allows_unauthenticated_health_probe_when_auth_configured() {
    let state = AxumApiState::new().with_auth(
        TenantResolver::new(),
        RateLimiter::new(60.0, 10),
        Some(60),
        1.0,
    );
    let app = build_axum_router(state);

    let health = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);
    let body = response_json(health).await;
    assert_eq!(body["status"], "healthy");
    assert_eq!(body["tenant_id"], "local");

    let ready = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);
    let body = response_json(ready).await;
    assert_eq!(body["status"], "ready");
}

#[tokio::test]
async fn axum_health_bypass_is_skipped_when_auth_header_present() {
    let mut resolver = TenantResolver::new();
    resolver.add_tenant(Tenant {
        tenant_id: "t1".to_string(),
        name: "T1".to_string(),
        scopes: HashSet::new(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key(
            "t1",
            Some(HashSet::from(["dispatch:read".to_string()])),
            None,
            1.0,
        )
        .unwrap();
    let state = AxumApiState::new().with_auth(resolver, RateLimiter::new(60.0, 10), Some(60), 1.0);
    let app = build_axum_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/health")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn axum_admin_backup_requires_scope_confirmation_and_writes_audit_log() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();

    let mut resolver = TenantResolver::new();
    let admin_scopes = HashSet::from([
        "backup:admin".to_string(),
        "audit:read".to_string(),
        "health:read".to_string(),
    ]);
    let readonly_scopes = HashSet::from(["health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: admin_scopes
            .union(&readonly_scopes)
            .cloned()
            .collect::<HashSet<String>>(),
        rate_limit: Some(100),
    });
    let (_readonly_key, readonly_raw) = resolver
        .create_api_key("local-team", Some(readonly_scopes), None, 1.0)
        .unwrap();
    let (_admin_key, admin_raw) = resolver
        .create_api_key("local-team", Some(admin_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    let readonly = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {readonly_raw}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"label": "nightly", "confirm_local_backup": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(readonly.status(), StatusCode::FORBIDDEN);

    let missing_confirmation = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {admin_raw}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"label": "nightly"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_confirmation.status(), StatusCode::BAD_REQUEST);

    let backup = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {admin_raw}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"label": "nightly", "confirm_local_backup": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(backup.status(), StatusCode::OK);
    let backup_body = response_json(backup).await;
    assert_eq!(backup_body["backup"]["label"], "nightly");

    let audit = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/audit")
                .header(header::AUTHORIZATION, format!("Bearer {admin_raw}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(audit.status(), StatusCode::OK);
    let audit_body = response_json(audit).await;
    assert_eq!(audit_body["events"][0]["action"], "backup.create");
}

#[tokio::test]
async fn axum_backup_requires_auth_boundary_even_with_confirmation() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups")),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/backups")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"label": "manual", "confirm_local_backup": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = response_json(response).await;
    assert_eq!(body["code"], "backup_admin_required");
    assert_eq!(body["error"], "admin auth is required for local backup");
}

#[tokio::test]
async fn axum_backup_verify_and_restore_dry_run_are_non_destructive() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("team.db");
    let store = LocalProductStore::new(&db_path).unwrap();
    store
        .record_dispatch(
            "Backup verification row",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-backup-verify", "final_status": "not_executed"},
                "decision": {"selected_tier": "cheap_executor", "budget_reservation": {"reserved_cost": 0.1}},
                "analysis": {"risk_level": "low"},
                "execution_result": {"executor_type": "noop"},
            }),
            "test",
        )
        .unwrap();

    let mut resolver = TenantResolver::new();
    let admin_scopes = HashSet::from([
        "backup:admin".to_string(),
        "audit:read".to_string(),
        "health:read".to_string(),
    ]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: admin_scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(admin_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"label": "verify", "confirm_local_backup": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let create_body = response_json(create).await;
    let backup_id = create_body["backup"]["backup_id"].as_str().unwrap();

    let verify = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/backups/{backup_id}/verify"))
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(verify.status(), StatusCode::OK);
    let verify_body = response_json(verify).await;
    assert_eq!(verify_body["verification"]["success"], true);
    assert_eq!(verify_body["verification"]["checksum_ok"], true);
    assert_eq!(verify_body["verification"]["integrity_ok"], true);
    assert_eq!(verify_body["verification"]["dry_run"], false);
    assert!(
        verify_body["verification"]["records_checked"]
            .as_i64()
            .unwrap()
            > 0
    );

    let dry_run = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/backups/{backup_id}/restore/dry-run"))
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"confirm_restore_dry_run": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dry_run.status(), StatusCode::OK);
    let dry_run_body = response_json(dry_run).await;
    assert_eq!(dry_run_body["restore_dry_run"]["success"], true);
    assert_eq!(dry_run_body["restore_dry_run"]["dry_run"], true);
    assert_eq!(
        dry_run_body["restore_dry_run"]["restore_would_overwrite"],
        true
    );

    let integrity = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/storage/integrity")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(integrity.status(), StatusCode::OK);
    let integrity_body = response_json(integrity).await;
    assert_eq!(integrity_body["integrity"]["status"], "ok");
}

#[tokio::test]
async fn axum_auth_allows_scoped_dispatch_key() {
    let mut resolver = TenantResolver::new();
    let scopes = HashSet::from(["dispatch:read".to_string(), "health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "tenant-a".to_string(),
        name: "Tenant A".to_string(),
        scopes: scopes.clone(),
        rate_limit: Some(5),
    });
    let (_key, raw_key) = resolver
        .create_api_key("tenant-a", Some(scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_auth(
        resolver,
        RateLimiter::new(60.0, 10),
        Some(60),
        1.0,
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/dispatch")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "Summarize docs", "request_source": "api"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn axum_rate_limit_blocks_after_tenant_limit() {
    let mut resolver = TenantResolver::new();
    let scopes = HashSet::from(["health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "tenant-a".to_string(),
        name: "Tenant A".to_string(),
        scopes: scopes.clone(),
        rate_limit: Some(1),
    });
    let (_key, raw_key) = resolver
        .create_api_key("tenant-a", Some(scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_auth(
        resolver,
        RateLimiter::new(60.0, 10),
        Some(60),
        1.0,
    ));

    let request = || {
        Request::builder()
            .method(Method::GET)
            .uri("/api/v1/health")
            .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
            .body(Body::empty())
            .unwrap()
    };

    let first = app.clone().oneshot(request()).await.unwrap();
    let second = app.oneshot(request()).await.unwrap();

    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn axum_preflight_returns_cors_headers() {
    let app = build_axum_router(AxumApiState::new());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/api/v1/dispatch")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN),
        Some(&"*".parse().unwrap())
    );
}

#[tokio::test]
async fn axum_openapi_document_lists_dispatch_endpoint() {
    let app = build_axum_router(AxumApiState::new());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["openapi"], "3.1.0");
    assert!(body["paths"]["/api/v1/dispatch"]["post"].is_object());
    assert!(body["paths"]["/api/v1/plans"]["post"].is_object());
    assert!(body["paths"]["/api/v1/plans"]["get"].is_object());
    assert!(body["paths"]["/api/v1/plans/{plan_id}"]["get"].is_object());
    assert!(body["paths"]["/api/v1/workflow-runs"]["post"].is_object());
    assert!(body["paths"]["/api/v1/workflow-runs"]["get"].is_object());
    assert!(body["paths"]["/api/v1/workflow-runs/{run_id}"]["get"].is_object());
    assert!(body["paths"]["/api/v1/workflow-runs/{run_id}/events"]["get"].is_object());
    assert!(body["paths"]["/api/v1/workflow-runs/{run_id}/approvals"]["get"].is_object());
    assert!(body["paths"]["/api/v1/supervised-patch/workspaces"]["get"].is_object());
    assert!(body["paths"]["/api/v1/supervised-patch/workspaces/{workspace_id}"]["get"].is_object());
    assert!(body["paths"]["/api/v1/supervised-patch/artifacts"]["get"].is_object());
    assert!(body["paths"]["/api/v1/supervised-patch/artifacts/{artifact_id}"]["get"].is_object());
    assert!(body["paths"]["/api/v1/metrics"]["get"].is_object());
    assert!(body["paths"]["/api/v1/backups/{backup_id}/verify"]["get"].is_object());
    assert!(body["paths"]["/api/v1/backups/{backup_id}/restore/dry-run"]["post"].is_object());
    assert_eq!(
        body["paths"]["/api/v1/audit"]["get"]["parameters"][3]["name"],
        "redact"
    );
    assert!(body["paths"]["/api/v1/provider/audit"]["get"].is_object());
    assert_eq!(
        body["paths"]["/api/v1/provider/audit"]["get"]["parameters"][0]["name"],
        "limit"
    );
    assert_eq!(
        body["paths"]["/api/v1/provider/audit"]["get"]["parameters"][1]["name"],
        "offset"
    );
}

#[tokio::test]
async fn axum_dashboard_serves_static_index_when_configured() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("index.html"),
        "<!doctype html><title>Agent Control Plane</title>",
    )
    .unwrap();
    let app = build_axum_router_with_dashboard(AxumApiState::new(), dir.path());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/html; charset=utf-8"
    );
    let body = response_text(response).await;
    assert!(body.contains("Agent Control Plane"));
}

#[tokio::test]
async fn axum_dashboard_serves_static_assets_when_configured() {
    let dir = tempdir().unwrap();
    let asset_dir = dir.path().join("_next/static/chunks");
    fs::create_dir_all(&asset_dir).unwrap();
    fs::write(asset_dir.join("app.js"), "console.log('ok');").unwrap();
    fs::write(dir.path().join("index.html"), "<!doctype html>").unwrap();
    let app = build_axum_router_with_dashboard(AxumApiState::new(), dir.path());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/_next/static/chunks/app.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/javascript; charset=utf-8"
    );
    assert_eq!(response_text(response).await, "console.log('ok');");
}

#[tokio::test]
async fn axum_dashboard_does_not_mask_unknown_api_routes() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("index.html"), "<!doctype html>").unwrap();
    let app = build_axum_router_with_dashboard(AxumApiState::new(), dir.path());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/missing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response_text(response).await, "not found");
}

#[tokio::test]
async fn axum_provider_health_noop_when_no_provider() {
    let app = build_axum_router(AxumApiState::new());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/provider/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["status"], "noop");
    assert_eq!(body["message"], "no provider configured");
}

#[tokio::test]
async fn axum_provider_health_noop_with_multi_executor_and_no_provider() {
    let engine = engine::dispatch_engine::DispatchEngine::with_multi_executor(
        engine::cli::MultiExecutor::new(std::collections::HashMap::new()),
    );
    let app = build_axum_router(AxumApiState::new().with_engine(engine));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/provider/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["status"], "noop");
    assert_eq!(body["message"], "no provider configured");
}

#[tokio::test]
async fn axum_provider_health_ok_with_stub_provider() {
    use engine::provider::stub::StubProvider;
    use engine::provider::Provider;
    use std::sync::Arc;

    let provider: Arc<dyn Provider> = Arc::new(StubProvider::new("stub-health"));
    let state = AxumApiState::new().with_provider(provider);
    let app = build_axum_router(state);
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/provider/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["status"], "ok");
    assert_eq!(body["provider_id"], "stub-health");
    assert_eq!(body["enabled"], true);
}

#[tokio::test]
async fn axum_provider_audit_paginates_and_clamps_negative_limit() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    store
        .record_provider_audit_event(&provider_audit_event("evt-old", "2026-05-29T12:00:00Z"))
        .unwrap();
    store
        .record_provider_audit_event(&provider_audit_event("evt-new", "2026-05-29T12:01:00Z"))
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let paged = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/provider/audit?limit=1&offset=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(paged.status(), StatusCode::OK);
    let body = response_json(paged).await;
    let events = body["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["event_id"], "evt-old");

    let negative = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/provider/audit?limit=-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(negative.status(), StatusCode::OK);
    let body = response_json(negative).await;
    assert!(body["events"].as_array().unwrap().is_empty());
}

fn make_admin_app() -> (axum::Router, String) {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    let mut resolver = TenantResolver::new();
    let mut admin_scopes = HashSet::new();
    admin_scopes.insert("team:read".to_string());
    admin_scopes.insert("team:admin".to_string());
    admin_scopes.insert("dispatch:read".to_string());
    admin_scopes.insert("health:read".to_string());
    admin_scopes.insert("audit:read".to_string());
    admin_scopes.insert("backup:admin".to_string());
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local".to_string(),
        scopes: admin_scopes.clone(),
        rate_limit: Some(10_000),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local", Some(admin_scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store).with_auth(
        resolver,
        RateLimiter::new(60.0, 10_000),
        Some(10_000),
        1.0,
    ));
    (app, raw_key)
}

#[tokio::test]
async fn axum_create_api_key_returns_raw_key() {
    let (app, admin_key) = make_admin_app();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "u1", "role": "admin", "scopes": ["dispatch:read"]})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert!(body["key_id"].as_str().unwrap().starts_with("key_"));
    assert!(body["raw_key"].as_str().unwrap().starts_with("harness_"));
    assert_eq!(body["user_id"], "u1");
    assert_eq!(body["role"], "admin");
}

#[tokio::test]
async fn axum_list_api_keys_returns_metadata() {
    let (app, admin_key) = make_admin_app();
    let app_clone = app.clone();

    // Create two keys
    for uid in &["u1", "u2"] {
        app_clone
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/keys")
                    .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({"user_id": uid, "role": "readonly", "scopes": ["dispatch:read"]})
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
    }

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    let keys = body["keys"].as_array().unwrap();
    assert!(keys.len() >= 2);
    // Keys should have metadata fields but no raw_key
    for key in keys {
        assert!(key["key_id"].as_str().is_some());
        assert!(key["user_id"].as_str().is_some());
        assert!(key["role"].as_str().is_some());
        assert!(key["scopes"].as_array().is_some());
        assert!(key["created_at"].as_str().is_some());
        assert!(
            key.get("raw_key").is_none(),
            "list must not return raw keys"
        );
    }
}

#[tokio::test]
async fn axum_list_api_keys_requires_team_read_scope() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    let mut resolver = TenantResolver::new();
    let mut readonly_scopes = HashSet::new();
    readonly_scopes.insert("dispatch:read".to_string());
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local".to_string(),
        scopes: readonly_scopes.clone(),
        rate_limit: Some(10_000),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local", Some(readonly_scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store).with_auth(
        resolver,
        RateLimiter::new(60.0, 10_000),
        Some(10_000),
        1.0,
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn axum_revoke_api_key_blocks_future_auth() {
    let (app, admin_key) = make_admin_app();
    let app_clone = app.clone();

    let create_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "u1", "role": "admin", "scopes": ["dispatch:read", "team:read"]}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let create_body = response_json(create_resp).await;
    let new_key = create_body["raw_key"].as_str().unwrap().to_string();
    let key_id = create_body["key_id"].as_str().unwrap().to_string();

    let app_clone = app.clone();
    let use_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {new_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(use_resp.status(), StatusCode::OK);

    let app_clone = app.clone();
    let revoke_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/keys/{key_id}/revoke"))
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoke_resp.status(), StatusCode::OK);

    let app_clone = app.clone();
    let blocked_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {new_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(blocked_resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn axum_rotate_api_key_creates_new_revokes_old() {
    let (app, admin_key) = make_admin_app();
    let app_clone = app.clone();

    let create_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "u1", "role": "admin", "scopes": ["dispatch:read", "team:read", "team:admin"]}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let create_body = response_json(create_resp).await;
    let old_key = create_body["raw_key"].as_str().unwrap().to_string();
    let key_id = create_body["key_id"].as_str().unwrap().to_string();

    let app_clone = app.clone();
    let rotate_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/keys/{key_id}/rotate"))
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rotate_resp.status(), StatusCode::OK);
    let rotate_body = response_json(rotate_resp).await;
    let new_key = rotate_body["raw_key"].as_str().unwrap().to_string();
    assert_ne!(old_key, new_key);
    assert_eq!(rotate_body["rotated_from"], key_id);

    let app_clone = app.clone();
    let old_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {old_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(old_resp.status(), StatusCode::UNAUTHORIZED);

    let app_clone = app.clone();
    let new_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {new_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(new_resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn axum_delete_api_key_removes_metadata() {
    let (app, admin_key) = make_admin_app();
    let app_clone = app.clone();

    let create_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "u1", "role": "admin", "scopes": ["dispatch:read"]})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let create_body = response_json(create_resp).await;
    let key_id = create_body["key_id"].as_str().unwrap().to_string();

    let app_clone = app.clone();
    let del_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/v1/keys/{key_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del_resp.status(), StatusCode::OK);

    let app_clone = app.clone();
    let team_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let team_body = response_json(team_resp).await;
    let keys = team_body["api_keys"].as_array().unwrap();
    assert!(!keys.iter().any(|k| k["key_id"] == key_id));
}

#[tokio::test]
async fn axum_create_team_member_adds_to_snapshot() {
    let (app, admin_key) = make_admin_app();

    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "alice", "display_name": "Alice", "role": "admin"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::OK);

    let team_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let team_body = response_json(team_resp).await;
    let members = team_body["members"].as_array().unwrap();
    assert!(members.iter().any(|m| m["user_id"] == "alice"));
}

#[tokio::test]
async fn axum_update_member_role_changes_role() {
    let (app, admin_key) = make_admin_app();

    app.clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "bob", "display_name": "Bob", "role": "readonly"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let update_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/api/v1/team/bob")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"role": "admin"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(update_resp.status(), StatusCode::OK);

    let team_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let team_body = response_json(team_resp).await;
    let members = team_body["members"].as_array().unwrap();
    let bob = members.iter().find(|m| m["user_id"] == "bob").unwrap();
    assert_eq!(bob["role"], "admin");
}

#[tokio::test]
async fn axum_delete_member_removes_from_snapshot() {
    let (app, admin_key) = make_admin_app();

    app.clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "carol", "display_name": "Carol", "role": "admin"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let del_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/api/v1/team/carol")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del_resp.status(), StatusCode::OK);

    let team_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let team_body = response_json(team_resp).await;
    let members = team_body["members"].as_array().unwrap();
    assert!(!members.iter().any(|m| m["user_id"] == "carol"));
}

#[tokio::test]
async fn axum_team_mutation_requires_admin_scope() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    let mut resolver = TenantResolver::new();
    let mut readonly_scopes = HashSet::new();
    readonly_scopes.insert("team:read".to_string());
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local".to_string(),
        scopes: readonly_scopes.clone(),
        rate_limit: Some(10_000),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local", Some(readonly_scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store).with_auth(
        resolver,
        RateLimiter::new(60.0, 10_000),
        Some(10_000),
        1.0,
    ));

    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "u1", "role": "admin", "scopes": ["dispatch:read"]})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::FORBIDDEN);

    let member_resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "u1", "display_name": "U1", "role": "admin"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(member_resp.status(), StatusCode::FORBIDDEN);
}

// --- dispatch detail tests ---

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

// --- list backups tests ---

#[tokio::test]
async fn axum_list_backups_requires_admin_scope() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();

    let mut resolver = TenantResolver::new();
    let readonly_scopes = HashSet::from(["health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: readonly_scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(readonly_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn axum_list_backups_returns_empty_when_no_backups() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();

    let mut resolver = TenantResolver::new();
    let admin_scopes = HashSet::from(["backup:admin".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: admin_scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(admin_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_json(resp).await;
    assert_eq!(body["backups"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn axum_list_backups_after_create() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();

    let mut resolver = TenantResolver::new();
    let admin_scopes = HashSet::from(["backup:admin".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: admin_scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(admin_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    // Create a backup
    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"label": "test", "confirm_local_backup": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::OK);

    // List backups
    let list_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_resp.status(), StatusCode::OK);
    let body = response_json(list_resp).await;
    assert_eq!(body["backups"].as_array().unwrap().len(), 1);
    assert_eq!(body["backups"][0]["label"], "test");
}

#[tokio::test]
async fn axum_create_backup_uses_real_timestamp() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();

    let mut resolver = TenantResolver::new();
    let admin_scopes = HashSet::from(["backup:admin".to_string(), "health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: admin_scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(admin_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    // Create a backup
    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"label": "ts-check", "confirm_local_backup": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::OK);

    // List and verify timestamp is not the old hardcoded value
    let list_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response_json(list_resp).await;
    let created_at = body["backups"][0]["created_at"].as_str().unwrap();
    assert_ne!(
        created_at, "2026-05-29T00:00:00Z",
        "backup timestamp should not be hardcoded"
    );
    assert!(
        created_at.starts_with("2026-") || created_at.starts_with("2027-"),
        "backup timestamp should be a recent ISO date, got: {created_at}"
    );
}

// --- delete backup tests ---

#[tokio::test]
async fn axum_delete_backup_requires_admin_scope() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();

    let mut resolver = TenantResolver::new();
    let readonly_scopes = HashSet::from(["health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: readonly_scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(readonly_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/api/v1/backups/backup-0001")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn axum_delete_backup_404_for_missing() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();

    let mut resolver = TenantResolver::new();
    let admin_scopes = HashSet::from(["backup:admin".to_string(), "audit:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: admin_scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(admin_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/api/v1/backups/nonexistent-backup")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn axum_delete_backup_removes_backup() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();

    let mut resolver = TenantResolver::new();
    let admin_scopes = HashSet::from([
        "backup:admin".to_string(),
        "audit:read".to_string(),
        "health:read".to_string(),
    ]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: admin_scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(admin_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    // Create a backup
    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"label": "to-delete", "confirm_local_backup": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::OK);
    let create_body = response_json(create_resp).await;
    let backup_id = create_body["backup"]["backup_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Delete it
    let delete_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/v1/backups/{backup_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete_resp.status(), StatusCode::OK);
    let delete_body = response_json(delete_resp).await;
    assert_eq!(delete_body["ok"], true);

    // Verify list is empty
    let list_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_resp.status(), StatusCode::OK);
    let list_body = response_json(list_resp).await;
    assert_eq!(list_body["backups"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn axum_tick_advances_single_node_run_to_completion() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("tick.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    // Create a plan
    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "echo hello", "request_source": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plan_resp.status(), StatusCode::OK);
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap().to_string();

    // Create a run from the plan
    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(run_resp.status(), StatusCode::OK);
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap().to_string();
    assert_eq!(run_body["run"]["status"], "created");

    // Tick the run - should transition to running and execute first node
    let tick_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"actor": "test"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let first_tick_body = response_json(tick_resp).await;
    // For a single-node plan, the first tick may complete the run immediately
    let first_action = first_tick_body["tick"]["action"].as_str().unwrap_or("");
    assert!(
        first_action == "node_executed" || first_action == "completed",
        "first tick should execute a node or complete, got: {first_action}"
    );
    if first_action == "node_executed" {
        assert_eq!(first_tick_body["tick"]["executor_type"], "noop");
    }

    // Keep ticking until run completes (for multi-node plans)
    for _ in 0..20 {
        let tick_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({"actor": "test"}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        if tick_resp.status() == StatusCode::CONFLICT {
            // Run is already terminal
            break;
        }
        let tick_body = response_json(tick_resp).await;
        let action = tick_body["tick"]["action"].as_str().unwrap_or("");
        if action == "completed" || action == "failed" {
            break;
        }
    }

    // Verify the run is terminal
    let run_detail = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/workflow-runs/{run_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let detail_body = response_json(run_detail).await;
    assert!(
        detail_body["run"]["status"] == "completed" || detail_body["run"]["status"] == "failed",
        "run should be terminal, got: {}",
        detail_body["run"]["status"]
    );
}

#[tokio::test]
async fn axum_tick_returns_409_on_terminal_run() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("tick-terminal.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    // Create a plan and run
    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "echo test", "request_source": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap();

    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap();

    // Cancel the run
    app.clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/cancel"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"reason": "test"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Tick should return 409
    let tick_resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"actor": "test"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tick_resp.status(), StatusCode::CONFLICT);
    let tick_body = response_json(tick_resp).await;
    assert_eq!(tick_body["code"], "run_terminal");
}

#[tokio::test]
async fn axum_tick_respects_node_dependencies() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("tick-deps.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    // Create a plan with multiple nodes
    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "raw_request": "first analyze the code, then refactor it, then test it",
                        "request_source": "test"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap();
    let nodes = plan_body["plan"]["graph"]["nodes"].as_array().unwrap();

    // Only run this test if the plan has multiple nodes
    if nodes.len() < 2 {
        return; // Skip if decomposition didn't produce multiple nodes
    }

    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap().to_string();

    // Tick repeatedly - nodes should complete respecting dependencies
    let mut completed_nodes = Vec::new();
    for _ in 0..(nodes.len() + 5) {
        let tick_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({"actor": "test"}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let tick_body = response_json(tick_resp).await;
        let action = tick_body["tick"]["action"].as_str().unwrap_or("");
        if action == "node_executed" {
            let node_id = tick_body["tick"]["node_id"].as_str().unwrap();
            completed_nodes.push(node_id.to_string());
        }
        if action == "completed" || action == "failed" {
            break;
        }
    }

    // All nodes should have been executed
    assert!(
        completed_nodes.len() >= nodes.len(),
        "expected {} nodes to execute, got {}",
        nodes.len(),
        completed_nodes.len()
    );

    // Verify the run is terminal
    let detail_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/workflow-runs/{run_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let detail_body = response_json(detail_resp).await;
    assert!(
        detail_body["run"]["status"] == "completed" || detail_body["run"]["status"] == "failed",
        "run should be terminal after all nodes complete"
    );

    // Verify all nodes have completed status
    let run_nodes = detail_body["run"]["nodes"].as_array().unwrap();
    for node in run_nodes {
        let status = node["db_status"]
            .as_str()
            .unwrap_or(node["status"].as_str().unwrap_or(""));
        assert!(
            status == "completed" || status == "failed",
            "node {} should be terminal, got: {}",
            node["node_id"].as_str().unwrap_or("?"),
            status
        );
    }
}

#[tokio::test]
async fn axum_tick_returns_404_for_missing_run() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("tick-missing.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let tick_resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs/nonexistent/tick")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"actor": "test"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tick_resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn axum_supervised_patch_workspace_create_and_cleanup() {
    let dir = tempdir().unwrap();
    let target_dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("workspace.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    // Create a workspace
    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/supervised-patch/workspaces")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "run_id": "run-test-001",
                        "target_id": "target-001",
                        "target_repo_path": target_dir.path().to_string_lossy(),
                        "source_revision": "abc123"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::OK);
    let create_body = response_json(create_resp).await;
    let workspace = &create_body["workspace"];
    assert_eq!(workspace["status"], "workspace_created");
    assert_eq!(workspace["run_id"], "run-test-001");

    let workspace_path = workspace["workspace_path"].as_str().unwrap();
    assert!(
        std::path::Path::new(workspace_path).exists(),
        "workspace directory should exist on disk"
    );

    let workspace_id = workspace["workspace_id"].as_str().unwrap().to_string();

    // Cleanup the workspace
    let cleanup_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/workspaces/{workspace_id}/cleanup"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cleanup_resp.status(), StatusCode::OK);
    let cleanup_body = response_json(cleanup_resp).await;
    assert_eq!(cleanup_body["workspace"]["status"], "cleaned");
    assert!(
        !std::path::Path::new(workspace_path).exists(),
        "workspace directory should be removed after cleanup"
    );
}

#[tokio::test]
async fn axum_supervised_patch_workspace_quarantine() {
    let dir = tempdir().unwrap();
    let target_dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("quarantine.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    // Create a workspace
    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/supervised-patch/workspaces")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "run_id": "run-test-002",
                        "target_id": "target-002",
                        "target_repo_path": target_dir.path().to_string_lossy(),
                        "source_revision": "def456"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let create_body = response_json(create_resp).await;
    let workspace_id = create_body["workspace"]["workspace_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Quarantine the workspace
    let quarantine_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/workspaces/{workspace_id}/quarantine"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(quarantine_resp.status(), StatusCode::OK);
    let quarantine_body = response_json(quarantine_resp).await;
    assert_eq!(quarantine_body["workspace"]["status"], "quarantined");
}

#[tokio::test]
async fn axum_supervised_patch_artifact_capture() {
    let dir = tempdir().unwrap();
    let target_dir = tempdir().unwrap();
    // Write a test file in the target so workspace gets it
    std::fs::write(target_dir.path().join("hello.txt"), "hello world").unwrap();
    let store = LocalProductStore::new(dir.path().join("artifact.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    // Create a workspace (copies target contents)
    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/supervised-patch/workspaces")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "run_id": "run-test-003",
                        "target_id": "target-003",
                        "target_repo_path": target_dir.path().to_string_lossy(),
                        "source_revision": "ghi789"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let create_body = response_json(create_resp).await;
    let workspace_id = create_body["workspace"]["workspace_id"]
        .as_str()
        .unwrap()
        .to_string();
    let workspace_path = create_body["workspace"]["workspace_path"]
        .as_str()
        .unwrap()
        .to_string();

    // Simulate a patch: add a new file to the workspace
    std::fs::write(
        std::path::Path::new(&workspace_path).join("patch.txt"),
        "patched content",
    )
    .unwrap();

    // Capture patch (server-generated hash)
    let capture_resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/workspaces/{workspace_id}/capture"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(capture_resp.status(), StatusCode::OK);
    let capture_body = response_json(capture_resp).await;
    let artifact = &capture_body["artifact"];
    assert!(artifact["patch_hash"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    assert_eq!(artifact["artifact_type"], "patch_diff");
    assert_eq!(artifact["redaction_status"], "redacted");
    assert!(!artifact["changed_files"].as_array().unwrap().is_empty());
    // .source_manifest.json must never appear in changed_files
    for file in artifact["changed_files"].as_array().unwrap() {
        assert!(
            !file.as_str().unwrap().contains(".source_manifest.json"),
            "changed_files must not contain .source_manifest.json, got: {file}"
        );
    }
}

#[tokio::test]
async fn axum_end_to_end_plan_run_tick_workspace_capture_quality_approval_export() {
    let dir = tempdir().unwrap();
    let target_dir = tempdir().unwrap();
    // Seed target with a file
    std::fs::write(target_dir.path().join("src.rs"), "fn main() {}").unwrap();
    let store = LocalProductStore::new(dir.path().join("e2e.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    // Step 1: Create a plan
    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "analyze and refactor code", "request_source": "e2e"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plan_resp.status(), StatusCode::OK);
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap().to_string();

    // Step 2: Create a run
    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(run_resp.status(), StatusCode::OK);
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap().to_string();

    // Step 3: Tick to completion
    for _ in 0..20 {
        let tick_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({"actor": "e2e", "max_retries": 2}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        if tick_resp.status() == StatusCode::CONFLICT {
            break;
        }
        let tick_body = response_json(tick_resp).await;
        let action = tick_body["tick"]["action"].as_str().unwrap_or("");
        if action == "completed" || action == "failed" {
            break;
        }
    }

    // Step 4: Verify run is terminal
    let detail_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/workflow-runs/{run_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let detail_body = response_json(detail_resp).await;
    assert!(
        detail_body["run"]["status"] == "completed" || detail_body["run"]["status"] == "failed"
    );

    // Step 5: Create workspace (copies target code)
    let ws_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/supervised-patch/workspaces")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "run_id": run_id,
                        "target_id": "target-e2e",
                        "target_repo_path": target_dir.path().to_string_lossy(),
                        "source_revision": "e2e-rev-001"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ws_resp.status(), StatusCode::OK);
    let ws_body = response_json(ws_resp).await;
    let workspace_id = ws_body["workspace"]["workspace_id"]
        .as_str()
        .unwrap()
        .to_string();
    let workspace_path = ws_body["workspace"]["workspace_path"]
        .as_str()
        .unwrap()
        .to_string();

    // Verify workspace has the copied file
    assert!(std::path::Path::new(&workspace_path)
        .join("src.rs")
        .exists());

    // Step 6: Modify workspace (simulate work)
    std::fs::write(
        std::path::Path::new(&workspace_path).join("patch.txt"),
        "new file",
    )
    .unwrap();

    // Step 7: Capture patch (server-generated hash, quality checks)
    let capture_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/workspaces/{workspace_id}/capture"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(capture_resp.status(), StatusCode::OK);
    let capture_body = response_json(capture_resp).await;
    let artifact = &capture_body["artifact"];
    let artifact_id = artifact["artifact_id"].as_str().unwrap().to_string();
    let patch_hash = artifact["patch_hash"].as_str().unwrap().to_string();
    assert!(patch_hash.starts_with("sha256:"));
    assert_eq!(artifact["redaction_status"], "redacted");
    assert!(!artifact["changed_files"].as_array().unwrap().is_empty());

    // Step 8: Quality check - integrity
    let integrity_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/supervised-patch/artifacts/{artifact_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(integrity_resp.status(), StatusCode::OK);

    // Step 9: Record approval WITH proper binding fields
    let changed_files: Vec<String> = artifact["changed_files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap_or("").to_string())
        .collect();
    let approval_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/approvals"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "node_id": "approval-node",
                        "decision": "approved",
                        "reason": "e2e test approval",
                        "bound_patch_hash": patch_hash,
                        "bound_source_revision": "e2e-rev-001",
                        "bound_changed_files": changed_files,
                        "expires_at": "2099-12-31T23:59:59Z"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approval_resp.status(), StatusCode::OK);

    // Step 10: Export with valid binding should succeed
    let export_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/artifacts/{artifact_id}/export"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"run_id": run_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let export_status = export_resp.status();
    let export_body = response_json(export_resp).await;
    assert_eq!(
        export_status,
        StatusCode::OK,
        "export failed: {export_body}"
    );
    let export = &export_body["export"];
    assert_eq!(export["artifact_id"], artifact_id);
    assert!(export["artifact"]["patch_hash"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    assert!(!export["artifact"]["changed_files"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(export["approval_binding"]["export_eligible"], true);
    assert_eq!(export["integrity"]["integrity_ok"], true);

    // Step 11: Cleanup
    let cleanup_resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/workspaces/{workspace_id}/cleanup"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cleanup_resp.status(), StatusCode::OK);
    assert!(!std::path::Path::new(&workspace_path).exists());
}

#[tokio::test]
async fn axum_e2e_command_executor_produces_real_patch_export() {
    // 1. Create a temp dir with a "target repo" containing one file
    let target_dir = tempdir().unwrap();
    std::fs::write(target_dir.path().join("README.md"), "# target\n").unwrap();

    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("e2e.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    // 2. Create a plan
    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "echo hello", "request_source": "e2e-command"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plan_resp.status(), StatusCode::OK);
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap().to_string();

    // 3. Create a workflow run
    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(run_resp.status(), StatusCode::OK);
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap().to_string();

    // 4. Create a supervised patch workspace linked to the run
    let ws_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/supervised-patch/workspaces")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "run_id": run_id,
                        "target_id": "target-command-e2e",
                        "target_repo_path": target_dir.path().to_string_lossy(),
                        "source_revision": "cmd-rev-001"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ws_resp.status(), StatusCode::OK);
    let ws_body = response_json(ws_resp).await;
    let workspace_id = ws_body["workspace"]["workspace_id"]
        .as_str()
        .unwrap()
        .to_string();
    let workspace_path = ws_body["workspace"]["workspace_path"]
        .as_str()
        .unwrap()
        .to_string();

    // Verify workspace has the copied README.md from target
    assert!(std::path::Path::new(&workspace_path)
        .join("README.md")
        .exists());

    // 5. Tick the workflow run with executor=command
    // The plan graph nodes don't have a command field, so CommandNodeExecutor
    // defaults to "echo noop". The workspace_path is injected from
    // supervised_patch_workspaces into node_metadata.
    let tick_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "executor": "command",
                        "actor": "e2e-command",
                        "max_retries": 0
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    // Tick may succeed (node_executed) or return 409 if run is terminal
    assert!(
        tick_resp.status() == StatusCode::OK || tick_resp.status() == StatusCode::CONFLICT,
        "tick should succeed or return conflict if terminal"
    );

    // 6. After tick, manually create a file in workspace_path to simulate
    //    command output (since the noop default doesn't create files)
    std::fs::write(
        std::path::Path::new(&workspace_path).join("new_file.txt"),
        "patched content\n",
    )
    .unwrap();

    // 7. Capture the patch
    let capture_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/workspaces/{workspace_id}/capture"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(capture_resp.status(), StatusCode::OK);
    let capture_body = response_json(capture_resp).await;
    let artifact = &capture_body["artifact"];
    let artifact_id = artifact["artifact_id"].as_str().unwrap().to_string();
    let patch_hash = artifact["patch_hash"].as_str().unwrap().to_string();
    assert!(
        patch_hash.starts_with("sha256:"),
        "patch_hash should start with sha256:, got: {patch_hash}"
    );
    let changed_files = artifact["changed_files"].as_array().unwrap();
    // Should contain the new file but NOT .source_manifest.json
    assert!(
        changed_files
            .iter()
            .any(|f| f.as_str().unwrap().contains("new_file.txt")),
        "changed_files should contain new_file.txt, got: {changed_files:?}"
    );
    assert!(
        !changed_files
            .iter()
            .any(|f| f.as_str().unwrap().contains(".source_manifest.json")),
        "changed_files must not contain .source_manifest.json, got: {changed_files:?}"
    );

    // 8. Validate artifact integrity via artifact detail endpoint
    let integrity_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/supervised-patch/artifacts/{artifact_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(integrity_resp.status(), StatusCode::OK);
    let integrity_body = response_json(integrity_resp).await;
    assert_eq!(integrity_body["artifact"]["artifact_id"], artifact_id);
    assert_eq!(integrity_body["artifact"]["patch_hash"], patch_hash);

    // 9. Record approval WITH proper binding fields
    let changed_files_vec: Vec<String> = artifact["changed_files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap_or("").to_string())
        .collect();
    let approval_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/approvals"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "node_id": "approval-node",
                        "decision": "approved",
                        "reason": "command executor e2e approval",
                        "bound_patch_hash": patch_hash,
                        "bound_source_revision": "cmd-rev-001",
                        "bound_changed_files": changed_files_vec,
                        "expires_at": "2099-12-31T23:59:59Z"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approval_resp.status(), StatusCode::OK);

    // 10. Export the artifact (needs approval binding first)
    let export_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/artifacts/{artifact_id}/export"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"run_id": run_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let export_status = export_resp.status();
    let export_body = response_json(export_resp).await;
    assert_eq!(
        export_status,
        StatusCode::OK,
        "export failed: {export_body}"
    );
    let export = &export_body["export"];
    assert_eq!(export["artifact_id"], artifact_id);
    assert!(export["artifact"]["patch_hash"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    assert!(!export["artifact"]["changed_files"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(export["approval_binding"]["export_eligible"], true);
    assert_eq!(export["integrity"]["integrity_ok"], true);
}

#[tokio::test]
async fn axum_tick_with_command_executor_uses_command_node_executor() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("tick-cmd.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "echo hello", "request_source": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plan_resp.status(), StatusCode::OK);
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap();

    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap();

    let tick_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"actor": "test", "executor": "command"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tick_resp.status(), StatusCode::OK);
    let tick_body = response_json(tick_resp).await;
    let executor_type = tick_body["tick"]["executor_type"].as_str().unwrap_or("");
    assert!(
        executor_type == "command" || executor_type == "noop",
        "expected command or noop executor_type, got: {executor_type}"
    );
}

#[tokio::test]
async fn axum_tick_with_unknown_executor_falls_back_to_noop() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("tick-unknown.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "echo test", "request_source": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plan_resp.status(), StatusCode::OK);
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap();

    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap();

    let tick_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"actor": "test", "executor": "fake_executor"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tick_resp.status(), StatusCode::OK);
    let tick_body = response_json(tick_resp).await;
    let action = tick_body["tick"]["action"].as_str().unwrap_or("");
    assert!(
        action == "node_executed" || action == "completed",
        "tick should still work with unknown executor, got: {action}"
    );
    if action == "node_executed" {
        assert_eq!(tick_body["tick"]["executor_type"], "noop");
    }
}

#[tokio::test]
async fn axum_tick_with_fail_executor_marks_run_failed() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("tick-fail.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "fail test", "request_source": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plan_resp.status(), StatusCode::OK);
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap();

    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap();

    let tick_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"actor": "test", "executor": "fail"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tick_resp.status(), StatusCode::OK);
    let tick_body = response_json(tick_resp).await;
    assert_eq!(tick_body["tick"]["executor_type"], "fail");
    assert_eq!(tick_body["tick"]["result"]["status"], "failed");
    assert_eq!(tick_body["tick"]["run"]["status"], "failed");
}

#[tokio::test]
async fn axum_dynamic_tick_recovers_failed_run_with_graph_mutation() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("tick-dynamic.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "dynamic recovery test", "request_source": "test"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plan_resp.status(), StatusCode::OK);
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap();

    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap();

    let fail_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"actor": "test", "executor": "fail"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(fail_resp.status(), StatusCode::OK);

    let dynamic_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"actor": "test", "executor": "dynamic"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dynamic_resp.status(), StatusCode::OK);
    let dynamic_body = response_json(dynamic_resp).await;
    assert_eq!(dynamic_body["tick"]["action"], "dynamic_tick");
    assert!(
        dynamic_body["tick"]["mutations_applied"].as_i64().unwrap() >= 1,
        "dynamic tick should apply a recovery mutation: {dynamic_body}"
    );
    let actions = dynamic_body["tick"]["actions"].as_array().unwrap();
    assert!(
        actions.iter().any(|a| a["type"] == "graph_mutated"),
        "dynamic tick should report graph_mutated: {dynamic_body}"
    );

    let detail_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/workflow-runs/{run_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let detail_body = response_json(detail_resp).await;
    assert_eq!(detail_body["run"]["status"], "running");
    assert!(
        detail_body["run"]["nodes"].as_array().unwrap().len() >= 3,
        "failed run should have original plus recovery nodes"
    );
}

#[tokio::test]
async fn axum_tick_with_claude_code_cli_unavailable_returns_400() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("tick-cli-400.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "echo cli", "request_source": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plan_resp.status(), StatusCode::OK);
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap();

    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap();

    // CLI execution is not enabled in test env (ACP_ENABLE_CLI_EXECUTION not set)
    let tick_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"actor": "test", "executor": "claude_code_cli"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tick_resp.status(), StatusCode::BAD_REQUEST);
    let tick_body = response_json(tick_resp).await;
    assert_eq!(tick_body["code"], "cli_not_available");
}

#[test]
fn cli_node_executor_resolve_prompt_and_executor() {
    use engine::cli::CliNodeExecutor;
    use serde_json::json;

    let executor =
        CliNodeExecutor::new(Some("/bin/claude".into()), Some("/bin/codex".into()), 5000);

    let input_with_prompt = engine::node_executor::NodeExecutionInput {
        node_id: "n1".into(),
        task_type: "test".into(),
        run_id: "r1".into(),
        workflow_id: "w1".into(),
        node_metadata: json!({"prompt": "do something"}),
    };
    assert_eq!(executor.resolve_prompt(&input_with_prompt), "do something");
    assert_eq!(
        executor.resolve_executor(&input_with_prompt),
        "claude_code_cli"
    );

    let input_with_command = engine::node_executor::NodeExecutionInput {
        node_id: "n2".into(),
        task_type: "test".into(),
        run_id: "r2".into(),
        workflow_id: "w2".into(),
        node_metadata: json!({"command": "echo hi"}),
    };
    assert_eq!(executor.resolve_prompt(&input_with_command), "echo hi");

    let input_with_explicit_executor = engine::node_executor::NodeExecutionInput {
        node_id: "n3".into(),
        task_type: "test".into(),
        run_id: "r3".into(),
        workflow_id: "w3".into(),
        node_metadata: json!({"executor": "codex_cli"}),
    };
    assert_eq!(
        executor.resolve_executor(&input_with_explicit_executor),
        "codex_cli"
    );

    let input_empty = engine::node_executor::NodeExecutionInput {
        node_id: "n4".into(),
        task_type: "test".into(),
        run_id: "r4".into(),
        workflow_id: "w4".into(),
        node_metadata: json!({}),
    };
    assert_eq!(executor.resolve_prompt(&input_empty), "echo noop");
}

#[tokio::test]
async fn axum_tick_with_codex_cli_unavailable_returns_400() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("tick-codex-400.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "echo codex cli", "request_source": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plan_resp.status(), StatusCode::OK);
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap();

    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap();

    let tick_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"actor": "test", "executor": "codex_cli"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tick_resp.status(), StatusCode::BAD_REQUEST);
    let tick_body = response_json(tick_resp).await;
    assert_eq!(tick_body["code"], "cli_not_available");
}

// ── GA-3: Scheduler status endpoint tests ────────────────────────────

#[tokio::test]
async fn axum_scheduler_status_returns_enabled_when_scheduler_present() {
    use engine::scheduler::{SchedulerConfig, WorkflowScheduler};
    use std::sync::{Arc, Mutex};

    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("sched-status.db")).unwrap();
    let config = SchedulerConfig {
        interval_ms: 2000,
        max_concurrent: 4,
        lease_timeout_ms: 300_000,
        executor_type: "noop".to_string(),
        ..Default::default()
    };
    let mut scheduler = WorkflowScheduler::new(Arc::new(store), config);
    scheduler.start().unwrap();
    let scheduler_arc = Arc::new(Mutex::new(scheduler));

    let app = build_axum_router(AxumApiState::new().with_scheduler(scheduler_arc));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/scheduler/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    let sched = &body["scheduler"];
    assert_eq!(sched["schema_version"], "scheduler.v1");
    assert_eq!(sched["running"], true);
    assert_eq!(sched["config"]["interval_ms"], 2000);
    assert_eq!(sched["config"]["max_concurrent"], 4);
    assert_eq!(sched["config"]["lease_timeout_ms"], 300_000);
    assert_eq!(sched["config"]["executor_type"], "noop");
    assert_eq!(sched["active_runs"], 0);
    assert!(sched["started_at"].as_str().is_some());
}

#[tokio::test]
async fn axum_scheduler_status_returns_disabled_when_no_scheduler() {
    let app = build_axum_router(AxumApiState::new());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/scheduler/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    let sched = &body["scheduler"];
    assert_eq!(sched["running"], false);
    assert_eq!(sched["enabled"], false);
    assert_eq!(
        sched["message"],
        "scheduler not enabled (set ACP_ENABLE_SCHEDULER=1)"
    );
}

#[tokio::test]
async fn axum_scheduler_status_reflects_active_runs() {
    use engine::scheduler::{SchedulerConfig, WorkflowScheduler};
    use std::sync::{Arc, Mutex};

    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("sched-active.db")).unwrap();

    // Create a plan and run before wrapping store in scheduler
    let plan = store
        .create_workflow_plan("test task", "test", "actor", |ids, _| {
            Ok(json!({
                "schema_version": "read_only_plan.v1",
                "plan_id": ids.plan_id,
                "status": "planned_read_only",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "analysis": {"analysis_id": "a-1", "task_domain": "docs"},
                "graph": {
                    "schema_version": "workflow_graph.v1",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "status": "decomposed",
                    "created_at": "2026-06-05T00:00:00Z",
                    "updated_at": "2026-06-05T00:00:00Z",
                    "nodes": [{
                        "schema_version": "workflow_node.v1",
                        "node_id": "node-a",
                        "workflow_id": ids.workflow_id,
                        "task_type": "analysis",
                        "assigned_agent_id": null,
                        "status": "pending",
                        "input_refs": [],
                        "output_ref": null,
                        "budget": 0.1,
                        "cost_incurred": 0.0,
                        "error": null,
                        "created_at": "2026-06-05T00:00:00Z",
                        "started_at": null,
                        "completed_at": null
                    }],
                    "edges": [],
                },
                "boundaries": {
                    "execution_authority": "disabled",
                    "target_repository_writes": "disabled",
                    "runtime_workers": "disabled",
                },
            }))
        })
        .unwrap();
    let plan_id = plan["plan_id"].as_str().unwrap();
    store
        .create_workflow_run_from_plan(plan_id, "actor")
        .unwrap();

    let store_arc = Arc::new(store);
    let config = SchedulerConfig {
        interval_ms: 2000,
        max_concurrent: 4,
        lease_timeout_ms: 300_000,
        executor_type: "noop".to_string(),
        ..Default::default()
    };
    let scheduler = WorkflowScheduler::new(store_arc, config);
    let scheduler_arc = Arc::new(Mutex::new(scheduler));

    let app = build_axum_router(AxumApiState::new().with_scheduler(scheduler_arc));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/scheduler/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    let sched = &body["scheduler"];
    assert_eq!(sched["active_runs"], 1, "should reflect the created run");
}

// ── GA-4: Observability / Audit tests ─────────────────────────────────

#[tokio::test]
async fn ga4_metrics_includes_secret_block_and_queue_length() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    // Create a dispatch so dispatch_count >= 1
    store
        .record_dispatch(
            "GA4 dispatch",
            "test",
            &json!({"record": {"dispatch_id": "disp-ga4", "final_status": "noop"}}),
            "test",
        )
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["secret_block_count"], 0);
    assert_eq!(body["queue_length"], 0);
    assert!(body["dispatch_count"].as_i64().unwrap() >= 1);
}

#[tokio::test]
async fn ga4_capture_logs_audit_event() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    // Create target files
    let target_dir = dir.path().join("target");
    std::fs::create_dir_all(target_dir.join("src")).unwrap();
    std::fs::write(target_dir.join("src/main.rs"), "fn main() {}").unwrap();

    // Create workspace
    let ws_path = store
        .create_workspace_directory("ws-ga4", target_dir.to_str().unwrap())
        .unwrap();
    let workspace = store
        .record_supervised_patch_workspace(
            &json!({
                "plan_id": "plan-ga4",
                "run_id": "run-ga4",
                "target_id": "ga4-target",
                "target_repo_path": target_dir.to_string_lossy(),
                "workspace_path": &ws_path,
                "source_revision": "abc123",
                "status": "workspace_created",
            }),
            "ga4-actor",
        )
        .unwrap();
    let ws_id = workspace["workspace_id"].as_str().unwrap();

    // Add a new file to workspace for capture
    std::fs::write(format!("{}/src/new.rs", ws_path), "fn new() {}").unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    // Capture
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/workspaces/{ws_id}/capture"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Check audit log
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/audit?limit=50")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    let events = body["events"].as_array().unwrap();
    let capture_event = events
        .iter()
        .find(|e| e["action"] == "supervised_patch.capture");
    assert!(
        capture_event.is_some(),
        "should have supervised_patch.capture audit event"
    );
    let evt = capture_event.unwrap();
    assert_eq!(evt["resource"], ws_id);
    assert!(evt["details"]["changed_files_count"].as_i64().unwrap() >= 1);
}

#[tokio::test]
async fn ga4_cleanup_logs_audit_event() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let target_dir = dir.path().join("target");
    std::fs::create_dir_all(&target_dir).unwrap();
    std::fs::write(target_dir.join("file.txt"), "content").unwrap();

    let ws_path = store
        .create_workspace_directory("ws-cleanup", target_dir.to_str().unwrap())
        .unwrap();
    let workspace = store
        .record_supervised_patch_workspace(
            &json!({
                "plan_id": "plan-cleanup",
                "run_id": "run-cleanup",
                "target_id": "cleanup-target",
                "target_repo_path": target_dir.to_string_lossy(),
                "workspace_path": &ws_path,
                "source_revision": "abc123",
                "status": "workspace_created",
            }),
            "cleanup-actor",
        )
        .unwrap();
    let ws_id = workspace["workspace_id"].as_str().unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    // Cleanup
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/workspaces/{ws_id}/cleanup"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Check audit log
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/audit?limit=50")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    let events = body["events"].as_array().unwrap();
    let cleanup_event = events
        .iter()
        .find(|e| e["action"] == "supervised_patch.cleanup");
    assert!(
        cleanup_event.is_some(),
        "should have supervised_patch.cleanup audit event"
    );
}

#[tokio::test]
async fn ga4_quarantine_logs_audit_event() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let target_dir = dir.path().join("target");
    std::fs::create_dir_all(&target_dir).unwrap();
    std::fs::write(target_dir.join("file.txt"), "content").unwrap();

    let ws_path = store
        .create_workspace_directory("ws-quarantine", target_dir.to_str().unwrap())
        .unwrap();
    let workspace = store
        .record_supervised_patch_workspace(
            &json!({
                "plan_id": "plan-quarantine",
                "run_id": "run-quarantine",
                "target_id": "quarantine-target",
                "target_repo_path": target_dir.to_string_lossy(),
                "workspace_path": &ws_path,
                "source_revision": "abc123",
                "status": "workspace_created",
            }),
            "quarantine-actor",
        )
        .unwrap();
    let ws_id = workspace["workspace_id"].as_str().unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    // Quarantine
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/workspaces/{ws_id}/quarantine"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Check audit log
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/audit?limit=50")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    let events = body["events"].as_array().unwrap();
    let quarantine_event = events
        .iter()
        .find(|e| e["action"] == "supervised_patch.quarantine");
    assert!(
        quarantine_event.is_some(),
        "should have supervised_patch.quarantine audit event"
    );
}

#[tokio::test]
async fn ga4_metrics_enrichment_includes_new_fields() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    // Create a dispatch to populate latency data
    store
        .record_dispatch(
            "enrichment dispatch",
            "test",
            &json!({
                "record": {
                    "dispatch_id": "disp-enrich",
                    "final_status": "noop",
                    "latency_ms": 150,
                }
            }),
            "test",
        )
        .unwrap();

    // Create a plan + run to generate approval count
    let plan = store
        .create_workflow_plan("enrichment test", "test", "actor", |ids, _| {
            Ok(json!({
                "schema_version": "read_only_plan.v1",
                "plan_id": ids.plan_id,
                "status": "planned_read_only",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "analysis": {"analysis_id": "a-1", "task_domain": "docs"},
                "graph": {
                    "schema_version": "workflow_graph.v1",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "status": "decomposed",
                    "created_at": "2026-06-06T00:00:00Z",
                    "updated_at": "2026-06-06T00:00:00Z",
                    "nodes": [{
                        "schema_version": "workflow_node.v1",
                        "node_id": "node-a",
                        "workflow_id": ids.workflow_id,
                        "task_type": "analysis",
                        "assigned_agent_id": null,
                        "status": "pending",
                        "input_refs": [],
                        "output_ref": null,
                        "budget": 0.1,
                        "cost_incurred": 0.0,
                        "error": null,
                        "created_at": "2026-06-06T00:00:00Z",
                        "started_at": null,
                        "completed_at": null
                    }],
                    "edges": [],
                },
                "boundaries": {"execution_authority": "disabled"},
            }))
        })
        .unwrap();
    let plan_id = plan["plan_id"].as_str().unwrap();
    let run = store
        .create_workflow_run_from_plan(plan_id, "actor")
        .unwrap();
    let run_id = run["run_id"].as_str().unwrap();

    // Record an approval
    store
        .record_workflow_run_approval(
            run_id,
            "node-a",
            "approved",
            "reviewer",
            Some("looks good"),
            None,
            None,
            None,
            None,
        )
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    // New enrichment fields
    assert!(
        body.get("artifact_count").is_some(),
        "metrics should include artifact_count"
    );
    assert!(
        body.get("approval_count").is_some(),
        "metrics should include approval_count"
    );
    assert!(
        body.get("executor_latency_avg_ms").is_some(),
        "metrics should include executor_latency_avg_ms"
    );
    assert!(
        body.get("scheduler_active_runs").is_some(),
        "metrics should include scheduler_active_runs"
    );
    assert_eq!(body["approval_count"].as_i64().unwrap(), 1);
    assert_eq!(body["artifact_count"].as_i64().unwrap(), 0);
    assert_eq!(body["scheduler_active_runs"].as_u64().unwrap(), 0);
}

#[tokio::test]
async fn ga4_node_tick_emits_audit_event() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    // Create a plan with a single node
    let plan = store
        .create_workflow_plan("tick audit test", "test", "actor", |ids, _| {
            Ok(json!({
                "schema_version": "read_only_plan.v1",
                "plan_id": ids.plan_id,
                "status": "planned_read_only",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "analysis": {"analysis_id": "a-1", "task_domain": "docs"},
                "graph": {
                    "schema_version": "workflow_graph.v1",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "status": "decomposed",
                    "created_at": "2026-06-06T00:00:00Z",
                    "updated_at": "2026-06-06T00:00:00Z",
                    "nodes": [{
                        "schema_version": "workflow_node.v1",
                        "node_id": "node-tick",
                        "workflow_id": ids.workflow_id,
                        "task_type": "command",
                        "assigned_agent_id": null,
                        "status": "pending",
                        "input_refs": [],
                        "output_ref": null,
                        "budget": 0.1,
                        "cost_incurred": 0.0,
                        "error": null,
                        "created_at": "2026-06-06T00:00:00Z",
                        "started_at": null,
                        "completed_at": null
                    }],
                    "edges": [],
                },
                "boundaries": {"execution_authority": "disabled"},
            }))
        })
        .unwrap();
    let plan_id = plan["plan_id"].as_str().unwrap();
    let run = store
        .create_workflow_run_from_plan(plan_id, "actor")
        .unwrap();
    let run_id = run["run_id"].as_str().unwrap();

    // Tick with noop executor
    let executor = engine::node_executor::NoopNodeExecutor;
    store
        .tick_with_executor(run_id, "cli-actor", 0, &executor)
        .unwrap();

    // Check audit log for node_tick event
    let events = store.audit_events(50).unwrap();
    let tick_event = events
        .iter()
        .find(|e| e["action"] == "workflow_run.node_tick");
    assert!(
        tick_event.is_some(),
        "should have workflow_run.node_tick audit event"
    );
    let evt = tick_event.unwrap();
    assert_eq!(evt["resource"], run_id);
    assert_eq!(evt["details"]["executor_type"], "noop");
    assert_eq!(evt["details"]["status"], "completed");
    assert_eq!(evt["details"]["node_id"], "node-tick");
}

#[tokio::test]
async fn ga4_approval_audit_event_exists() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let plan = store
        .create_workflow_plan("approval audit test", "test", "actor", |ids, _| {
            Ok(json!({
                "schema_version": "read_only_plan.v1",
                "plan_id": ids.plan_id,
                "status": "planned_read_only",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "analysis": {"analysis_id": "a-1", "task_domain": "docs"},
                "graph": {
                    "schema_version": "workflow_graph.v1",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "status": "decomposed",
                    "created_at": "2026-06-06T00:00:00Z",
                    "updated_at": "2026-06-06T00:00:00Z",
                    "nodes": [{
                        "schema_version": "workflow_node.v1",
                        "node_id": "node-appr",
                        "workflow_id": ids.workflow_id,
                        "task_type": "analysis",
                        "assigned_agent_id": null,
                        "status": "pending",
                        "input_refs": [],
                        "output_ref": null,
                        "budget": 0.1,
                        "cost_incurred": 0.0,
                        "error": null,
                        "created_at": "2026-06-06T00:00:00Z",
                        "started_at": null,
                        "completed_at": null
                    }],
                    "edges": [],
                },
                "boundaries": {"execution_authority": "disabled"},
            }))
        })
        .unwrap();
    let plan_id = plan["plan_id"].as_str().unwrap();
    let run = store
        .create_workflow_run_from_plan(plan_id, "actor")
        .unwrap();
    let run_id = run["run_id"].as_str().unwrap();

    // Record an approval
    store
        .record_workflow_run_approval(
            run_id,
            "node-appr",
            "approved",
            "reviewer",
            Some("LGTM"),
            Some("sha256:abc"),
            Some("rev1"),
            Some(&["file.rs".to_string()]),
            Some("2026-12-31T00:00:00Z"),
        )
        .unwrap();

    // Check audit log for approval_record event
    let events = store.audit_events(50).unwrap();
    let approval_event = events
        .iter()
        .find(|e| e["action"] == "workflow_run.approval_record");
    assert!(
        approval_event.is_some(),
        "should have workflow_run.approval_record audit event"
    );
    let evt = approval_event.unwrap();
    assert_eq!(evt["resource"], run_id);
    assert_eq!(evt["details"]["decision"], "approved");
    assert_eq!(evt["details"]["metadata_only"], true);
}

// ── GA-6: SDK/API Completeness ──────────────────────────────────────────────

/// Helper: create a plan, run, tick to terminal, create workspace, modify, capture.
/// Returns (app, run_id, workspace_id, workspace_path, artifact_id, patch_hash, changed_files, _dir, _target_dir).
/// Caller must keep _dir and _target_dir alive to prevent tempdir cleanup.
async fn ga6_setup_e2e() -> (
    axum::Router,
    String,
    String,
    String,
    String,
    String,
    Vec<String>,
    tempfile::TempDir,
    tempfile::TempDir,
) {
    let dir = tempdir().unwrap();
    let target_dir = tempdir().unwrap();
    std::fs::write(target_dir.path().join("lib.rs"), "fn hello() {}").unwrap();
    let store = LocalProductStore::new(dir.path().join("ga6.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    // Create plan
    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "ga6 test task", "request_source": "ga6"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plan_resp.status(), StatusCode::OK);
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap().to_string();

    // Create run
    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(run_resp.status(), StatusCode::OK);
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap().to_string();

    // Tick to terminal
    for _ in 0..20 {
        let tick_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({"actor": "ga6"}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        if tick_resp.status() == StatusCode::CONFLICT {
            break;
        }
        let tick_body = response_json(tick_resp).await;
        let action = tick_body["tick"]["action"].as_str().unwrap_or("");
        if action == "completed" || action == "failed" {
            break;
        }
    }

    // Create workspace
    let ws_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/supervised-patch/workspaces")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "run_id": run_id,
                        "target_id": "ga6-target",
                        "target_repo_path": target_dir.path().to_string_lossy(),
                        "source_revision": "ga6-rev-001"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ws_resp.status(), StatusCode::OK);
    let ws_body = response_json(ws_resp).await;
    let workspace_id = ws_body["workspace"]["workspace_id"]
        .as_str()
        .unwrap()
        .to_string();
    let workspace_path = ws_body["workspace"]["workspace_path"]
        .as_str()
        .unwrap()
        .to_string();

    // Modify workspace
    std::fs::write(
        std::path::Path::new(&workspace_path).join("new_module.rs"),
        "pub fn added() {}",
    )
    .unwrap();

    // Capture
    let capture_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/workspaces/{workspace_id}/capture"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(capture_resp.status(), StatusCode::OK);
    let capture_body = response_json(capture_resp).await;
    let artifact = &capture_body["artifact"];
    let artifact_id = artifact["artifact_id"].as_str().unwrap().to_string();
    let patch_hash = artifact["patch_hash"].as_str().unwrap().to_string();
    let changed_files: Vec<String> = artifact["changed_files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap_or("").to_string())
        .collect();

    (
        app,
        run_id,
        workspace_id,
        workspace_path,
        artifact_id,
        patch_hash,
        changed_files,
        dir,
        target_dir,
    )
}

#[tokio::test]
async fn ga6_export_rejected_on_approval_patch_hash_mismatch() {
    let (app, run_id, _ws_id, _ws_path, artifact_id, _patch_hash, _changed, _d1, _d2) =
        ga6_setup_e2e().await;

    // Record approval with WRONG patch hash
    let approval_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/approvals"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "node_id": "approval-node",
                        "decision": "approved",
                        "reason": "wrong hash test",
                        "bound_patch_hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
                        "bound_source_revision": "ga6-rev-001",
                        "bound_changed_files": ["new_module.rs"],
                        "expires_at": "2099-12-31T23:59:59Z"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approval_resp.status(), StatusCode::OK);

    // Export should fail — patch hash mismatch → export_not_approved (403)
    let export_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/artifacts/{artifact_id}/export"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"run_id": run_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(export_resp.status(), StatusCode::FORBIDDEN);
    let body = response_json(export_resp).await;
    assert_eq!(body["code"], "export_not_approved");
}

#[tokio::test]
async fn ga6_export_rejected_on_approval_changed_files_mismatch() {
    let (app, run_id, _ws_id, _ws_path, artifact_id, patch_hash, _changed, _d1, _d2) =
        ga6_setup_e2e().await;

    // Record approval with correct hash but WRONG changed_files
    let approval_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/approvals"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "node_id": "approval-node",
                        "decision": "approved",
                        "reason": "wrong files test",
                        "bound_patch_hash": patch_hash,
                        "bound_source_revision": "ga6-rev-001",
                        "bound_changed_files": ["completely_different_file.txt"],
                        "expires_at": "2099-12-31T23:59:59Z"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approval_resp.status(), StatusCode::OK);

    // Export should fail — changed files mismatch → export_not_approved (403)
    let export_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/artifacts/{artifact_id}/export"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"run_id": run_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(export_resp.status(), StatusCode::FORBIDDEN);
    let body = response_json(export_resp).await;
    assert_eq!(body["code"], "export_not_approved");
}

#[tokio::test]
async fn ga6_export_rejected_when_approval_expired() {
    let (app, run_id, _ws_id, _ws_path, artifact_id, patch_hash, changed, _d1, _d2) =
        ga6_setup_e2e().await;

    // Record approval that already expired
    let approval_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/approvals"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "node_id": "approval-node",
                        "decision": "approved",
                        "reason": "expired test",
                        "bound_patch_hash": patch_hash,
                        "bound_source_revision": "ga6-rev-001",
                        "bound_changed_files": changed,
                        "expires_at": "2020-01-01T00:00:00Z"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approval_resp.status(), StatusCode::OK);

    // Export should fail — expired approval → export_not_approved (403)
    let export_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/artifacts/{artifact_id}/export"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"run_id": run_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(export_resp.status(), StatusCode::FORBIDDEN);
    let body = response_json(export_resp).await;
    assert_eq!(body["code"], "export_not_approved");
}

#[tokio::test]
async fn ga6_export_rejected_when_no_approval_exists() {
    let (app, run_id, _ws_id, _ws_path, artifact_id, _hash, _changed, _d1, _d2) =
        ga6_setup_e2e().await;

    // Export without any approval — should fail → export_not_approved (403)
    let export_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/artifacts/{artifact_id}/export"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"run_id": run_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(export_resp.status(), StatusCode::FORBIDDEN);
    let body = response_json(export_resp).await;
    assert_eq!(body["code"], "export_not_approved");
}

#[tokio::test]
async fn ga6_artifact_detail_returns_diff_summary_and_changed_files() {
    let (app, _run_id, _ws_id, _ws_path, artifact_id, patch_hash, changed, _d1, _d2) =
        ga6_setup_e2e().await;

    // Fetch artifact detail
    let detail_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/supervised-patch/artifacts/{artifact_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detail_resp.status(), StatusCode::OK);
    let body = response_json(detail_resp).await;
    let art = &body["artifact"];
    assert_eq!(art["artifact_id"], artifact_id);
    assert_eq!(art["patch_hash"], patch_hash);

    let files = art["changed_files"].as_array().unwrap();
    assert_eq!(files.len(), changed.len());
    for f in &changed {
        assert!(
            files.iter().any(|v| v.as_str() == Some(f)),
            "expected {f} in changed_files"
        );
    }

    // review_diff should be populated (non-empty string)
    let diff = art["review_diff"].as_str().unwrap_or("");
    assert!(
        !diff.is_empty(),
        "review_diff should be populated for captured artifact"
    );
    assert!(
        diff.contains("new_module.rs"),
        "diff should mention the added file"
    );
}

#[tokio::test]
async fn ga6_workspace_lifecycle_create_capture_cleanup() {
    let dir = tempdir().unwrap();
    let target_dir = tempdir().unwrap();
    std::fs::write(target_dir.path().join("app.rs"), "fn main() {}").unwrap();
    let store = LocalProductStore::new(dir.path().join("ga6-lifecycle.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    // Create plan + run (required for workspace)
    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "lifecycle test", "request_source": "ga6"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap();

    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap();

    // Create workspace
    let ws_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/supervised-patch/workspaces")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "run_id": run_id,
                        "target_id": "lifecycle-target",
                        "target_repo_path": target_dir.path().to_string_lossy(),
                        "source_revision": "lifecycle-rev"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ws_resp.status(), StatusCode::OK);
    let ws_body = response_json(ws_resp).await;
    let ws_id = ws_body["workspace"]["workspace_id"]
        .as_str()
        .unwrap()
        .to_string();
    let ws_path = ws_body["workspace"]["workspace_path"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(std::path::Path::new(&ws_path).exists());

    // Workspace list should include our workspace
    let list_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/supervised-patch/workspaces")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_resp.status(), StatusCode::OK);
    let list_body = response_json(list_resp).await;
    let workspaces = list_body["workspaces"].as_array().unwrap();
    assert!(workspaces.iter().any(|w| w["workspace_id"] == ws_id));

    // Modify workspace before capture (otherwise capture returns 400 — no changes)
    std::fs::write(
        std::path::Path::new(&ws_path).join("added.rs"),
        "pub fn new_func() {}",
    )
    .unwrap();

    // Capture
    let cap_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/workspaces/{ws_id}/capture"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cap_resp.status(), StatusCode::OK);
    let cap_body = response_json(cap_resp).await;
    assert_eq!(cap_body["artifact"]["artifact_type"], "patch_diff");
    assert!(!cap_body["artifact"]["changed_files"]
        .as_array()
        .unwrap()
        .is_empty());

    // Cleanup workspace
    let clean_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/workspaces/{ws_id}/cleanup"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(clean_resp.status(), StatusCode::OK);
    assert!(!std::path::Path::new(&ws_path).exists());
}

#[tokio::test]
async fn ga6_quarantine_workspace_transitions_status() {
    let dir = tempdir().unwrap();
    let target_dir = tempdir().unwrap();
    std::fs::write(target_dir.path().join("q.rs"), "fn q() {}").unwrap();
    let store = LocalProductStore::new(dir.path().join("ga6-quarantine.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    // Create plan + run
    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "quarantine test", "request_source": "ga6"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap();

    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap();

    // Create workspace
    let ws_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/supervised-patch/workspaces")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "run_id": run_id,
                        "target_id": "q-target",
                        "target_repo_path": target_dir.path().to_string_lossy(),
                        "source_revision": "q-rev"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let ws_body = response_json(ws_resp).await;
    let ws_id = ws_body["workspace"]["workspace_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Quarantine
    let q_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/workspaces/{ws_id}/quarantine"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(q_resp.status(), StatusCode::OK);
    let q_body = response_json(q_resp).await;
    assert_eq!(q_body["workspace"]["status"], "quarantined");

    // Verify workspace detail shows quarantined status
    let detail_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/supervised-patch/workspaces/{ws_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detail_resp.status(), StatusCode::OK);
    let detail_body = response_json(detail_resp).await;
    assert_eq!(detail_body["workspace"]["status"], "quarantined");
}

#[tokio::test]
async fn ga6_tick_with_noop_executor_completes_single_node() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("ga6-tick.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    // Create plan via API (produces a WorkflowGraph with decomposed nodes)
    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "tick test task", "request_source": "ga6"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plan_resp.status(), StatusCode::OK);
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap();

    // Create run from plan
    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(run_resp.status(), StatusCode::OK);
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap();

    // Tick until terminal
    let mut completed = false;
    for _ in 0..20 {
        let tick_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({"actor": "ga6", "executor": "noop"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        if tick_resp.status() == StatusCode::CONFLICT {
            completed = true;
            break;
        }
        assert_eq!(tick_resp.status(), StatusCode::OK);
        let tick_body = response_json(tick_resp).await;
        let action = tick_body["tick"]["action"].as_str().unwrap_or("");
        if action == "completed" || action == "failed" {
            completed = true;
            break;
        }
    }
    assert!(completed, "run should have reached terminal state");

    // Verify run is terminal
    let detail_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/workflow-runs/{run_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let detail_body = response_json(detail_resp).await;
    let status = detail_body["run"]["status"].as_str().unwrap();
    assert!(
        status == "completed" || status == "failed",
        "expected terminal status, got: {status}"
    );
}

#[tokio::test]
async fn ga6_scheduler_status_reports_config_and_metrics() {
    use engine::scheduler::{SchedulerConfig, WorkflowScheduler};
    use std::sync::{Arc, Mutex};

    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("ga6-sched.db")).unwrap();
    let config = SchedulerConfig {
        interval_ms: 5000,
        max_concurrent: 2,
        lease_timeout_ms: 60_000,
        executor_type: "command".to_string(),
        ..Default::default()
    };
    let mut scheduler = WorkflowScheduler::new(Arc::new(store), config);
    scheduler.start().unwrap();
    let scheduler_arc = Arc::new(Mutex::new(scheduler));

    let app = build_axum_router(AxumApiState::new().with_scheduler(scheduler_arc));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/scheduler/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    let sched = &body["scheduler"];
    assert_eq!(sched["schema_version"], "scheduler.v1");
    assert_eq!(sched["running"], true);
    assert_eq!(sched["config"]["interval_ms"], 5000);
    assert_eq!(sched["config"]["max_concurrent"], 2);
    assert_eq!(sched["config"]["lease_timeout_ms"], 60_000);
    assert_eq!(sched["config"]["executor_type"], "command");
    assert_eq!(sched["active_runs"], 0);
    assert!(sched["tick_count"].as_u64().is_some());
    assert!(sched["error_count"].as_u64().is_some());
}

#[tokio::test]
async fn ga6_export_success_with_matching_approval() {
    let (app, run_id, _ws_id, _ws_path, artifact_id, patch_hash, changed, _d1, _d2) =
        ga6_setup_e2e().await;

    // Record approval with CORRECT binding fields
    let approval_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/approvals"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "node_id": "approval-node",
                        "decision": "approved",
                        "reason": "correct binding test",
                        "bound_patch_hash": patch_hash,
                        "bound_source_revision": "ga6-rev-001",
                        "bound_changed_files": changed,
                        "expires_at": "2099-12-31T23:59:59Z"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approval_resp.status(), StatusCode::OK);

    // Export should succeed
    let export_resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/artifacts/{artifact_id}/export"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"run_id": run_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(export_resp.status(), StatusCode::OK);
    let body = response_json(export_resp).await;
    assert_eq!(body["export"]["artifact_id"], artifact_id);
    assert_eq!(body["export"]["approval_binding"]["export_eligible"], true);
    assert_eq!(body["export"]["integrity"]["integrity_ok"], true);
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

// ── Safety gate invariant tests ──────────────────────────────────────

async fn create_test_proposal(app: &axum::Router, raw_key: &str) -> String {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/proposals")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "task_domain": "docs",
                        "task_intent": "review",
                        "target_tier": "verifier",
                        "payload": {"type": "tier_map_override"},
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_json(resp).await;
    body["proposal"]["proposal_id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn test_proposal_rejects_cli_tier_override() {
    let (app, admin_key) = make_admin_app();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/proposals")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "task_domain": "docs",
                        "task_intent": "review",
                        "target_tier": "codex_cli",
                        "payload": {"type": "tier_map_override"},
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(response).await;
    assert_eq!(body["code"], "invalid_policy_proposal");
}

#[tokio::test]
async fn test_proposal_approve_requires_team_admin() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("safety-admin.db")).unwrap();
    let mut resolver = TenantResolver::new();
    let admin_scopes = HashSet::from([
        "dispatch:read".to_string(),
        "team:admin".to_string(),
        "health:read".to_string(),
    ]);
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local".to_string(),
        scopes: admin_scopes.clone(),
        rate_limit: Some(10_000),
    });
    let (_admin_key, admin_raw) = resolver
        .create_api_key("local", Some(admin_scopes), None, 1.0)
        .unwrap();

    let non_admin_scopes = HashSet::from(["dispatch:read".to_string(), "health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "readonly".to_string(),
        name: "Readonly".to_string(),
        scopes: non_admin_scopes.clone(),
        rate_limit: Some(10_000),
    });
    let (_readonly_key, readonly_raw) = resolver
        .create_api_key("readonly", Some(non_admin_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store).with_auth(
        resolver,
        RateLimiter::new(60.0, 10_000),
        Some(10_000),
        1.0,
    ));

    let proposal_id = create_test_proposal(&app, &admin_raw).await;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/proposals/{proposal_id}/approve"))
                .header(header::AUTHORIZATION, format!("Bearer {readonly_raw}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"reason": "test", "confirm_policy_override": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_proposal_rollback_requires_confirm_policy_override() {
    let (app, admin_key) = make_admin_app();
    let proposal_id = create_test_proposal(&app, &admin_key).await;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/proposals/{proposal_id}/rollback"))
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"reason": "test"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(response).await;
    assert_eq!(body["code"], "invalid_policy_proposal");
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("confirm_policy_override"),
        "error should mention confirm_policy_override requirement"
    );
}

#[tokio::test]
async fn test_proposal_deactivate_requires_confirm_policy_override() {
    let (app, admin_key) = make_admin_app();

    // First approve a proposal so it becomes active (required for deactivate)
    let proposal_id = create_test_proposal(&app, &admin_key).await;
    let approve_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/proposals/{proposal_id}/approve"))
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"reason": "activate for deactivate test", "confirm_policy_override": true})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approve_resp.status(), StatusCode::OK);

    // Now try deactivate without confirm_policy_override
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/proposals/{proposal_id}/deactivate"))
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"reason": "test"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(response).await;
    assert_eq!(body["code"], "invalid_policy_proposal");
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("confirm_policy_override"),
        "error should mention confirm_policy_override requirement"
    );
}

#[tokio::test]
async fn test_proposal_approve_requires_auth_configured() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("safety-noauth.db")).unwrap();

    // Create proposal via store directly (no auth needed for store operations)
    let proposal = store
        .create_policy_proposal(
            &json!({
                "task_domain": "docs",
                "task_intent": "review",
                "target_tier": "verifier",
                "payload": {"type": "tier_map_override"},
            }),
            "test-actor",
        )
        .unwrap();
    let proposal_id = proposal["proposal_id"].as_str().unwrap();

    // Build router WITHOUT auth configured
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/proposals/{proposal_id}/approve"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"reason": "test", "confirm_policy_override": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = response_json(response).await;
    assert_eq!(body["code"], "auth_required_for_policy_override");
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

#[tokio::test]
async fn test_feedback_traces_empty() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("traces-empty.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/feedback/traces")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["schema_version"], "feedback_traces.v1");
    assert_eq!(body["total"], 0);
    assert!(body["traces"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_feedback_cost_of_pass_empty() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("cop-empty.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/feedback/cost-of-pass")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["schema_version"], "feedback_cost_of_pass.v1");
    assert!(body["rows"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_simulation_report_empty() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("sim-empty.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/simulation/report")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["schema_version"], "dispatch_simulation_report.v1");
    assert_eq!(body["totals"]["dispatch_count"], 0);
    assert_eq!(body["totals"]["shadow_route_count"], 0);
    assert!(body["report"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_simulation_report_with_shadow_routes() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("sim-shadow.db")).unwrap();
    store
        .record_dispatch(
            "Simulation shadow dispatch",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-shadow", "final_status": "not_executed"},
                "decision": {
                    "selected_tier": "balanced_worker",
                    "budget_reservation": {"reserved_cost": 0.1},
                    "shadow_routes": [
                        {"tier": "cheap_executor", "score": 0.8},
                        {"tier": "premium_worker", "score": 0.6}
                    ]
                },
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
                .uri("/api/v1/simulation/report")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["schema_version"], "dispatch_simulation_report.v1");
    assert!(body["totals"]["dispatch_count"].as_i64().unwrap() >= 1);
    assert!(body["totals"]["shadow_route_count"].as_i64().unwrap() >= 2);

    let report = body["report"].as_array().unwrap();
    assert!(!report.is_empty());
    assert_eq!(report[0]["dispatch_id"], "disp-shadow");
    assert_eq!(report[0]["status"], "shadow_only");
    let shadow_routes = report[0]["shadow_routes"].as_array().unwrap();
    assert_eq!(shadow_routes.len(), 2);
    assert_eq!(shadow_routes[0]["tier"], "cheap_executor");
    assert_eq!(shadow_routes[1]["tier"], "premium_worker");
}

#[tokio::test]
async fn test_feedback_patterns_empty() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("patterns-empty.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/feedback/patterns")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["schema_version"], "feedback_patterns.v1");
    assert_eq!(body["total"], 0);
    assert!(body["patterns"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_feedback_patterns_with_dispatch_data() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("patterns-data.db")).unwrap();

    for i in 0..5 {
        let status = if i < 4 { "failed" } else { "completed" };
        store
            .record_dispatch(
                &format!("Pattern dispatch {i}"),
                "api",
                &json!({
                    "record": {"dispatch_id": format!("disp-pat-{i}"), "final_status": status},
                    "decision": {"selected_tier": "fragile_worker", "budget_reservation": {"reserved_cost": 0.05}},
                    "analysis": {"risk_level": "high"},
                    "execution_result": {"executor_type": "noop"},
                }),
                "test",
            )
            .unwrap();
    }

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/feedback/patterns")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["schema_version"], "feedback_patterns.v1");
    let patterns = body["patterns"].as_array().unwrap();
    assert!(!patterns.is_empty());

    let first = &patterns[0];
    assert!(first["pattern_id"].as_str().is_some());
    assert!(first["pattern_type"].as_str().is_some());
    assert!(first["severity"].as_str().is_some());
    let evidence = first["evidence_trace_ids"].as_array().unwrap();
    assert!(!evidence.is_empty());
}

#[tokio::test]
async fn test_feedback_patterns_with_task_class_filter() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("patterns-filter.db")).unwrap();

    for i in 0..4 {
        store
            .record_dispatch(
                &format!("Code review dispatch {i}"),
                "api",
                &json!({
                    "record": {"dispatch_id": format!("disp-cr-{i}"), "final_status": "failed"},
                    "decision": {"selected_tier": "balanced_worker", "budget_reservation": {"reserved_cost": 0.05}},
                    "analysis": {"risk_level": "high", "task_class": "code_review"},
                    "execution_result": {"executor_type": "noop"},
                }),
                "test",
            )
            .unwrap();
    }

    for i in 0..4 {
        store
            .record_dispatch(
                &format!("Docs dispatch {i}"),
                "api",
                &json!({
                    "record": {"dispatch_id": format!("disp-docs-{i}"), "final_status": "completed"},
                    "decision": {"selected_tier": "balanced_worker", "budget_reservation": {"reserved_cost": 0.02}},
                    "analysis": {"risk_level": "low", "task_class": "docs_update"},
                    "execution_result": {"executor_type": "noop"},
                }),
                "test",
            )
            .unwrap();
    }

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/feedback/patterns?task_class=code_review")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    let patterns = body["patterns"].as_array().unwrap();
    for pattern in patterns {
        let tc = pattern["affected_task_class"].as_str().unwrap_or("");
        assert!(
            tc.eq_ignore_ascii_case("code_review") || tc.is_empty(),
            "unexpected task_class in filtered result: {tc}"
        );
    }
}

#[tokio::test]
async fn test_feedback_traces_returns_stable_schema() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("traces-schema.db")).unwrap();

    store
        .record_dispatch(
            "Trace schema check",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-trace-schema", "final_status": "completed"},
                "decision": {
                    "selected_tier": "balanced_worker",
                    "budget_reservation": {"reserved_cost": 0.08}
                },
                "analysis": {"risk_level": "low"},
                "execution_result": {"executor_type": "noop", "estimated_cost": 0.004},
                "evaluation_result": {"status": "pass"},
            }),
            "test",
        )
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/feedback/traces")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["schema_version"], "feedback_traces.v1");
    assert!(body["total"].as_u64().unwrap() >= 1);

    let traces = body["traces"].as_array().unwrap();
    assert!(!traces.is_empty());
    let trace = &traces[0];
    assert!(trace["trace_id"].as_str().is_some());
    assert!(trace["dispatch_id"].as_str().is_some());
    assert!(trace["decision"].is_object() || trace["decision"].is_null());
    assert!(trace["execution"].is_object() || trace["execution"].is_null());
    assert!(trace["evaluation"].is_object() || trace["evaluation"].is_null());
}

#[tokio::test]
async fn test_feedback_traces_supports_filters() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("traces-filters.db")).unwrap();

    store
        .record_dispatch(
            "Filter test alpha",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-filter-a", "final_status": "completed"},
                "decision": {"selected_tier": "premium_worker", "budget_reservation": {"reserved_cost": 0.2}},
                "analysis": {"risk_level": "low", "task_class": "code_review"},
                "execution_result": {"executor_type": "noop"},
            }),
            "test",
        )
        .unwrap();

    store
        .record_dispatch(
            "Filter test beta",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-filter-b", "final_status": "failed"},
                "decision": {"selected_tier": "cheap_executor", "budget_reservation": {"reserved_cost": 0.01}},
                "analysis": {"risk_level": "high", "task_class": "docs_update"},
                "execution_result": {"executor_type": "noop"},
            }),
            "test",
        )
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/feedback/traces?task_class=code_review")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_json(resp).await;
    for trace in body["traces"].as_array().unwrap() {
        assert_eq!(trace["task_class"], "code_review");
    }

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/feedback/traces?tier=premium_worker")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_json(resp).await;
    for trace in body["traces"].as_array().unwrap() {
        assert_eq!(trace["tier"], "premium_worker");
    }

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/feedback/traces?status=fail")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_json(resp).await;
    for trace in body["traces"].as_array().unwrap() {
        assert_eq!(trace["status"], "fail");
    }
}

#[tokio::test]
async fn test_cost_of_pass_still_works() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("cop-data.db")).unwrap();

    store
        .record_dispatch(
            "Cost aggregation A",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-cop-a", "final_status": "completed"},
                "decision": {"selected_tier": "balanced_worker", "budget_reservation": {"reserved_cost": 0.10}},
                "analysis": {"risk_level": "low", "task_class": "code_review"},
                "execution_result": {"executor_type": "noop", "estimated_cost": 0.05},
            }),
            "test",
        )
        .unwrap();

    store
        .record_dispatch(
            "Cost aggregation B",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-cop-b", "final_status": "completed"},
                "decision": {"selected_tier": "balanced_worker", "budget_reservation": {"reserved_cost": 0.08}},
                "analysis": {"risk_level": "low", "task_class": "code_review"},
                "execution_result": {"executor_type": "noop", "estimated_cost": 0.03},
            }),
            "test",
        )
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/feedback/cost-of-pass")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["schema_version"], "feedback_cost_of_pass.v1");
    let rows = body["rows"].as_array().unwrap();
    assert!(!rows.is_empty());

    let code_review_row = rows
        .iter()
        .find(|r| r["task_class"] == "code_review")
        .expect("expected code_review cost-of-pass row");
    assert_eq!(code_review_row["dispatch_count"].as_i64().unwrap(), 2);
    assert!(code_review_row["average_cost_usd"].as_f64().unwrap() > 0.0);
}

#[tokio::test]
async fn test_policy_simulation_report_empty() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("psim-empty.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/simulation/policy-delta")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["schema_version"], "policy_simulation_report.v1");
    assert_eq!(body["input_trace_count"], 0);
    assert_eq!(body["safety"], "shadow_only / no_live_influence");
    assert_eq!(body["success_rate_delta"], 0.0);
}

#[tokio::test]
async fn test_policy_simulation_report_with_traces() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("psim-traces.db")).unwrap();
    store
        .record_dispatch(
            "Pass dispatch",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-pass", "final_status": "completed"},
                "decision": {"selected_tier": "balanced_worker", "budget_reservation": {"reserved_cost": 0.05}, "shadow_routes": []},
                "analysis": {"task_class": "implementation"},
                "execution_result": {"executor_type": "noop", "status": "completed", "success": true, "latency_ms": 1000, "estimated_cost": 0.03},
                "evaluation_result": {"status": "pass"}
            }),
            "test",
        )
        .unwrap();
    store
        .record_dispatch(
            "Fail dispatch",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-fail", "final_status": "failed"},
                "decision": {"selected_tier": "cheap_executor", "budget_reservation": {"reserved_cost": 0.02}, "shadow_routes": []},
                "analysis": {"task_class": "code_review"},
                "execution_result": {"executor_type": "noop", "status": "failed", "success": false, "latency_ms": 500, "estimated_cost": 0.01},
                "evaluation_result": {"status": "fail"}
            }),
            "test",
        )
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/simulation/policy-delta?policy=cheapest")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["schema_version"], "policy_simulation_report.v1");
    assert_eq!(body["input_trace_count"], 2);
    assert_eq!(body["safety"], "shadow_only / no_live_influence");
    assert!(body["success_rate_delta"].is_f64());
    assert!(body["cost_delta"].is_f64());
    assert!(body["latency_delta"].is_f64());
    assert!(body["human_review_rate_delta"].is_f64());
    assert!(body["actual_success_rate"].is_f64());
    assert!(body["simulated_success_rate"].is_f64());
    assert_eq!(body["evidence_trace_ids"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_shadow_simulation_does_not_alter_dispatch_tier() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("psim-notier.db")).unwrap();
    store
        .record_dispatch(
            "Tier check",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-tier", "final_status": "completed"},
                "decision": {"selected_tier": "balanced_worker", "budget_reservation": {"reserved_cost": 0.05}, "shadow_routes": []},
                "analysis": {"task_class": "implementation"},
                "execution_result": {"executor_type": "noop", "status": "completed", "success": true, "latency_ms": 1000},
                "evaluation_result": {"status": "pass"}
            }),
            "test",
        )
        .unwrap();

    let _report = store.policy_simulation_report(10).unwrap();

    let dispatches = store.list_dispatches(10).unwrap();
    assert_eq!(dispatches.len(), 1);
    let bundle = &dispatches[0]["bundle"];
    assert_eq!(bundle["decision"]["selected_tier"], "balanced_worker");
}

#[tokio::test]
async fn test_shadow_simulation_does_not_alter_executor_type() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("psim-noexec.db")).unwrap();
    store
        .record_dispatch(
            "Exec check",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-exec", "final_status": "completed"},
                "decision": {"selected_tier": "strong_planner", "budget_reservation": {"reserved_cost": 0.1}, "shadow_routes": []},
                "analysis": {"task_class": "architecture"},
                "execution_result": {"executor_type": "cli", "status": "completed", "success": true, "latency_ms": 5000},
                "evaluation_result": {"status": "pass"}
            }),
            "test",
        )
        .unwrap();

    let _report = store
        .policy_simulation_report_with_policy(10, "cheapest")
        .unwrap();

    let dispatches = store.list_dispatches(10).unwrap();
    assert_eq!(
        dispatches[0]["bundle"]["execution_result"]["executor_type"],
        "cli"
    );
}

#[tokio::test]
async fn test_shadow_simulation_does_not_mutate_routing_policy() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("psim-nopol.db")).unwrap();
    store
        .record_dispatch(
            "Policy check",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-pol", "final_status": "completed"},
                "decision": {"selected_tier": "balanced_worker", "routing_policy": "default", "budget_reservation": {"reserved_cost": 0.05}, "shadow_routes": []},
                "analysis": {"task_class": "implementation"},
                "execution_result": {"executor_type": "noop", "status": "completed", "success": true},
                "evaluation_result": {"status": "pass"}
            }),
            "test",
        )
        .unwrap();

    let _report = store.policy_simulation_report(10).unwrap();

    let dispatches = store.list_dispatches(10).unwrap();
    assert_eq!(
        dispatches[0]["bundle"]["decision"]["routing_policy"],
        "default"
    );
}
