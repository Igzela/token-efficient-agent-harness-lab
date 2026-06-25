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
        supervised_workers_enabled: true,
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

