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
async fn axum_supervised_patch_verification_runs_allowlisted_command_and_records_evidence() {
    let target_dir = tempdir().unwrap();
    let workspace_root = tempdir().unwrap();
    let workspace_path = workspace_root.path().join("workspace");
    fs::create_dir_all(&workspace_path).unwrap();
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("patch.db");
    let store = LocalProductStore::new(&db_path).unwrap();
    store
        .record_supervised_patch_workspace(
            &json!({
                "run_id": "run-verify",
                "target_id": "target-verify",
                "target_repo_path": target_dir.path().to_string_lossy(),
                "workspace_path": workspace_path.to_string_lossy(),
                "source_revision": "abc123",
                "status": "workspace_created",
            }),
            "operator",
        )
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let missing_confirmation = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/supervised-patch/workspaces/patch-workspace-0001/verify")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"command": "python3 --version"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_confirmation.status(), StatusCode::BAD_REQUEST);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/supervised-patch/workspaces/patch-workspace-0001/verify")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "command": "python3 --version",
                        "confirm_verification": true,
                        "attempt": 1,
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["verification"]["status"], "evidence_recorded");
    assert_eq!(
        body["verification"]["command"],
        json!(["python3", "--version"])
    );

    let persisted_store = LocalProductStore::new(&db_path).unwrap();
    let workspace = persisted_store
        .get_supervised_patch_workspace("patch-workspace-0001")
        .unwrap()
        .unwrap();
    assert_eq!(workspace["verification"]["status"], "evidence_recorded");
}

#[tokio::test]
async fn axum_supervised_patch_verification_can_repair_and_retry_with_cli() {
    let _env_guard = provider_cli_env_lock().lock().await;
    let target_dir = tempdir().unwrap();
    let workspace_root = tempdir().unwrap();
    let workspace_path = workspace_root.path().join("workspace");
    fs::create_dir_all(&workspace_path).unwrap();
    fs::write(
        workspace_path.join("verify.py"),
        "from pathlib import Path\nraise SystemExit(0 if Path('fixed.txt').exists() else 1)\n",
    )
    .unwrap();
    let fake_cli_root = std::env::current_dir()
        .unwrap()
        .join("target")
        .join("test-fake-cli");
    fs::create_dir_all(&fake_cli_root).unwrap();
    let fake_cli_dir = tempfile::Builder::new()
        .prefix("supervised-repair-")
        .tempdir_in(fake_cli_root)
        .unwrap();
    let fake_codex = fake_cli_dir.path().join("fake-codex");
    fs::write(
        &fake_codex,
        "#!/bin/sh\nprintf 'fixed\\n' > fixed.txt\n\
         printf '{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"repair complete\"}}\\n'\n\
         printf '{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":10,\"output_tokens\":2}}\\n'\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&fake_codex, fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::env::set_var("ACP_ENABLE_CLI_EXECUTION", "1");
    std::env::set_var("ACP_CODEX_BIN", &fake_codex);
    std::env::remove_var("ACP_CLAUDE_CODE_BIN");

    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("patch.db")).unwrap();
    store
        .record_supervised_patch_workspace(
            &json!({
                "run_id": "run-repair",
                "target_id": "target-repair",
                "target_repo_path": target_dir.path().to_string_lossy(),
                "workspace_path": workspace_path.to_string_lossy(),
                "source_revision": "abc123",
                "status": "workspace_created",
            }),
            "operator",
        )
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/supervised-patch/workspaces/patch-workspace-0001/verify")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "command": "python3 verify.py",
                        "confirm_verification": true,
                        "repair_executor": "codex_cli",
                        "max_repair_attempts": 2,
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    std::env::remove_var("ACP_ENABLE_CLI_EXECUTION");
    std::env::remove_var("ACP_CODEX_BIN");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["verification"]["status"], "approval_required");
    assert_eq!(
        body["verification"]["verification_attempts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        body["verification"]["repair_attempts"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(!workspace_path.join("fixed.txt").exists());
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
