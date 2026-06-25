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
        supervised_workers_enabled: true,
        ..Default::default()
    };
    let mut scheduler = WorkflowScheduler::new(Arc::new(store), config);
    scheduler.start().unwrap();
    let scheduler_arc = Arc::new(Mutex::new(scheduler));

    let app = build_axum_router(AxumApiState::new().with_scheduler(scheduler_arc));
    let dashboard_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/dashboard")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dashboard_response.status(), StatusCode::OK);
    let dashboard = response_json(dashboard_response).await;
    assert_eq!(dashboard["boundaries"]["runtime_workers"], "enabled");

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

#[tokio::test]
async fn axum_scheduler_control_requires_execute_scope_confirmation_and_audits() {
    use engine::scheduler::{SchedulerConfig, WorkflowScheduler};
    use std::sync::{Arc, Mutex};

    let dir = tempdir().unwrap();
    let store = Arc::new(LocalProductStore::new(dir.path().join("sched-control.db")).unwrap());
    let mut scheduler = WorkflowScheduler::new(
        store.clone(),
        SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            worker_count: 1,
            supervised_workers_enabled: true,
            ..Default::default()
        },
    );
    scheduler.start().unwrap();
    let scheduler = Arc::new(Mutex::new(scheduler));

    let mut resolver = TenantResolver::new();
    let scopes = HashSet::from(["dispatch:execute".to_string(), "health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local".to_string(),
        scopes: scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local", Some(scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store_arc(store.clone())
            .with_scheduler(scheduler)
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    let missing_confirmation = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/scheduler/control")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"action": "pause", "actor": "operator"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_confirmation.status(), StatusCode::BAD_REQUEST);

    let paused = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/scheduler/control")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "action": "pause",
                        "actor": "operator",
                        "confirm_control": true
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(paused.status(), StatusCode::OK);
    let body = response_json(paused).await;
    assert_eq!(body["scheduler"]["paused"], true);

    let killed = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/scheduler/control")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "action": "kill",
                        "actor": "operator",
                        "confirm_control": true
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(killed.status(), StatusCode::OK);
    let body = response_json(killed).await;
    assert_eq!(body["scheduler"]["running"], false);
    assert_eq!(body["scheduler"]["kill_requested"], true);

    let audits = store.audit_events(20).unwrap();
    assert!(audits
        .iter()
        .any(|event| event["action"] == "scheduler.control.pause"));
    assert!(audits
        .iter()
        .any(|event| event["action"] == "scheduler.control.kill"));
}

#[tokio::test]
async fn axum_scheduler_control_rejects_missing_execute_scope() {
    use engine::scheduler::{SchedulerConfig, WorkflowScheduler};
    use std::sync::{Arc, Mutex};

    let dir = tempdir().unwrap();
    let store = Arc::new(LocalProductStore::new(dir.path().join("sched-scope.db")).unwrap());
    let scheduler = Arc::new(Mutex::new(WorkflowScheduler::new(
        store.clone(),
        SchedulerConfig::default(),
    )));
    let mut resolver = TenantResolver::new();
    let scopes = HashSet::from(["health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local".to_string(),
        scopes: scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local", Some(scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store_arc(store)
            .with_scheduler(scheduler)
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/scheduler/control")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"action": "pause", "confirm_control": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

