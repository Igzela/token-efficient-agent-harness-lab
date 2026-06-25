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

