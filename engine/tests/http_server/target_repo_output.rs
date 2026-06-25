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
async fn axum_target_repo_output_requires_execute_scope() {
    let dir = tempdir().unwrap();
    let target = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("target-output-auth.db")).unwrap();
    let mut resolver = TenantResolver::new();
    resolver.add_tenant(Tenant {
        tenant_id: "target-output".to_string(),
        name: "Target Output".to_string(),
        scopes: HashSet::from(["dispatch:read".to_string(), "dispatch:execute".to_string()]),
        rate_limit: Some(100),
    });
    let (_, raw_key) = resolver
        .create_api_key(
            "target-output",
            Some(HashSet::from(["dispatch:read".to_string()])),
            None,
            1.0,
        )
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store).with_auth(
        resolver,
        RateLimiter::new(60.0, 100),
        Some(100),
        1.0,
    ));

    let workspace_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/supervised-patch/workspaces")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "run_id": "run-missing",
                        "target_id": "target-missing",
                        "target_repo_path": target.path().to_string_lossy(),
                        "source_revision": "HEAD",
                        "workspace_mode": "git_worktree"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(workspace_response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response_json(workspace_response).await["code"],
        "missing_scope"
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/supervised-patch/artifacts/missing/output")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "run_id": "run-missing",
                        "mode": "export_patch",
                        "confirm_target_output": true
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(response_json(response).await["code"], "missing_scope");
}

#[tokio::test]
async fn axum_target_repo_worktree_gate_and_kill_switch_are_audited() {
    let _env_lock = target_repo_output_env_lock().lock().await;
    let _env = TargetRepoOutputEnvGuard;
    std::env::remove_var("ACP_ENABLE_TARGET_REPO_OUTPUT");
    std::env::remove_var("ACP_TARGET_REPO_OUTPUT_KILL_SWITCH");
    let target = tempdir().unwrap();
    git(target.path(), &["init", "-b", "main"]);
    git(target.path(), &["config", "user.name", "ACP Test"]);
    git(
        target.path(),
        &["config", "user.email", "acp-test@example.invalid"],
    );
    std::fs::write(target.path().join("README.md"), "base\n").unwrap();
    git(target.path(), &["add", "README.md"]);
    git(target.path(), &["commit", "-m", "base"]);
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("target-output-gate.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let body = json!({
        "run_id": "run-gate",
        "target_id": "target-gate",
        "target_repo_path": target.path().to_string_lossy(),
        "source_revision": "main",
        "workspace_mode": "git_worktree"
    })
    .to_string();

    let disabled = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/supervised-patch/workspaces")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(disabled.status(), StatusCode::SERVICE_UNAVAILABLE);

    std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
    std::env::set_var("ACP_TARGET_REPO_OUTPUT_KILL_SWITCH", "1");
    let killed = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/supervised-patch/workspaces")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(killed.status(), StatusCode::SERVICE_UNAVAILABLE);

    let audit = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/audit?limit=10&search=target_output_failure")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let audit_body = response_json(audit).await;
    assert_eq!(audit_body["events"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn axum_target_repo_output_exports_and_pushes_only_after_approval() {
    let _env_lock = target_repo_output_env_lock().lock().await;
    let _env = TargetRepoOutputEnvGuard::enable_local_remote();
    std::env::remove_var("ACP_ENABLE_GITHUB_PR_OUTPUT");
    std::env::remove_var("ACP_GITHUB_TOKEN_ENV");
    let target = tempdir().unwrap();
    let remote = tempdir().unwrap();
    git(target.path(), &["init", "-b", "main"]);
    git(target.path(), &["config", "user.name", "ACP Test"]);
    git(
        target.path(),
        &["config", "user.email", "acp-test@example.invalid"],
    );
    std::fs::write(target.path().join("README.md"), "base\n").unwrap();
    git(target.path(), &["add", "README.md"]);
    git(target.path(), &["commit", "-m", "base"]);
    git(remote.path(), &["init", "--bare"]);
    git(remote.path(), &["symbolic-ref", "HEAD", "refs/heads/main"]);
    git(
        target.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );
    git(target.path(), &["push", "-u", "origin", "main"]);
    let main_before = git(target.path(), &["rev-parse", "main"]);

    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("target-output.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let plan = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "raw_request": "Update README and verify output",
                        "request_source": "target-output-test"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plan.status(), StatusCode::OK);
    let plan_id = response_json(plan).await["plan"]["plan_id"]
        .as_str()
        .unwrap()
        .to_string();
    let run = app
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
    assert_eq!(run.status(), StatusCode::OK);
    let run_id = response_json(run).await["run"]["run_id"]
        .as_str()
        .unwrap()
        .to_string();
    for _ in 0..20 {
        let tick = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({"actor": "target-output-test"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        if tick.status() == StatusCode::CONFLICT {
            break;
        }
        let tick_body = response_json(tick).await;
        if tick_body["tick"]["action"] == "completed" {
            break;
        }
    }

    let workspace = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/supervised-patch/workspaces")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "run_id": run_id,
                        "plan_id": plan_id,
                        "target_id": "target-real-git",
                        "target_repo_path": target.path().to_string_lossy(),
                        "source_revision": "main",
                        "workspace_mode": "git_worktree"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let workspace_status = workspace.status();
    let workspace_body = response_json(workspace).await;
    assert_eq!(
        workspace_status,
        StatusCode::OK,
        "workspace failed: {workspace_body}"
    );
    assert_eq!(
        workspace_body["workspace"]["workspace_mode"],
        "git_worktree"
    );
    let workspace_id = workspace_body["workspace"]["workspace_id"]
        .as_str()
        .unwrap()
        .to_string();
    let workspace_path = PathBuf::from(
        workspace_body["workspace"]["workspace_path"]
            .as_str()
            .unwrap(),
    );
    let source_revision = workspace_body["workspace"]["source_revision"]
        .as_str()
        .unwrap()
        .to_string();
    std::fs::write(
        workspace_path.join("README.md"),
        "base\napproved production output\n",
    )
    .unwrap();

    let capture = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/workspaces/{workspace_id}/capture"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let capture_status = capture.status();
    let capture_body = response_json(capture).await;
    assert_eq!(
        capture_status,
        StatusCode::OK,
        "capture failed: {capture_body}"
    );
    let artifact = &capture_body["artifact"];
    let artifact_id = artifact["artifact_id"].as_str().unwrap().to_string();
    let patch_hash = artifact["patch_hash"].as_str().unwrap().to_string();
    let changed_files = artifact["changed_files"].clone();
    assert_eq!(artifact["evidence_bundle"]["patch_hash"], patch_hash);
    assert_eq!(
        artifact["evidence_bundle"]["verification"]["run_id"],
        run_id
    );
    assert_eq!(
        artifact["evidence_bundle"]["verification"]["status"],
        "evidence_recorded"
    );

    let unconfirmed = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/artifacts/{artifact_id}/output"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"run_id": run_id, "mode": "export_patch"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unconfirmed.status(), StatusCode::BAD_REQUEST);

    let unapproved = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/artifacts/{artifact_id}/output"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "run_id": run_id,
                        "mode": "export_patch",
                        "confirm_target_output": true
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unapproved.status(), StatusCode::FORBIDDEN);

    let approval = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/approvals"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "node_id": "target-output-approval",
                        "decision": "approved",
                        "reason": "reviewed diff and evidence",
                        "bound_patch_hash": patch_hash,
                        "bound_source_revision": source_revision,
                        "bound_changed_files": changed_files,
                        "expires_at": "2099-12-31T23:59:59Z"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approval.status(), StatusCode::OK);

    std::fs::write(
        workspace_path.join("README.md"),
        "base\napproved production output\ntampered after approval\n",
    )
    .unwrap();
    let tampered = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/artifacts/{artifact_id}/output"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "run_id": run_id,
                        "mode": "export_patch",
                        "confirm_target_output": true
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        tampered.status(),
        StatusCode::CONFLICT,
        "post-approval workspace mutation must invalidate output"
    );
    std::fs::write(
        workspace_path.join("README.md"),
        "base\napproved production output\n",
    )
    .unwrap();

    let export = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/artifacts/{artifact_id}/output"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "run_id": run_id,
                        "mode": "export_patch",
                        "confirm_target_output": true
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let export_status = export.status();
    let export_body = response_json(export).await;
    assert_eq!(
        export_status,
        StatusCode::OK,
        "export failed: {export_body}"
    );
    assert_eq!(export_body["output"]["patch_hash"], patch_hash);
    assert!(export_body["output"]["patch"]
        .as_str()
        .unwrap()
        .contains("approved production output"));

    let pr_preflight_branch = format!("acp/pr-preflight-{artifact_id}");
    let pr_preflight = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/artifacts/{artifact_id}/output"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "run_id": run_id,
                        "mode": "push_branch",
                        "confirm_target_output": true,
                        "branch_name": pr_preflight_branch,
                        "remote": "origin",
                        "commit_message": "feat: should not push without github gate",
                        "pr_title": "Should not push without github gate",
                        "create_pull_request": true
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(pr_preflight.status(), StatusCode::BAD_REQUEST);
    let missing_branch = Command::new("git")
        .arg("-C")
        .arg(remote.path())
        .args(["rev-parse", &format!("refs/heads/{pr_preflight_branch}")])
        .output()
        .unwrap();
    assert!(
        !missing_branch.status.success(),
        "GitHub PR preflight failure must not push the target branch"
    );

    let push = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/supervised-patch/artifacts/{artifact_id}/output"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "run_id": run_id,
                        "mode": "push_branch",
                        "confirm_target_output": true,
                        "branch_name": format!("acp/{artifact_id}"),
                        "remote": "origin",
                        "commit_message": "feat: apply approved production output",
                        "pr_title": "Apply approved production output"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let push_status = push.status();
    let push_body = response_json(push).await;
    assert_eq!(push_status, StatusCode::OK, "push failed: {push_body}");
    assert_eq!(push_body["output"]["patch_hash"], patch_hash);
    assert_eq!(
        git(target.path(), &["rev-parse", "main"]),
        main_before,
        "target main must remain unchanged"
    );
    assert_eq!(
        git(
            remote.path(),
            &["rev-parse", &format!("refs/heads/acp/{artifact_id}")]
        ),
        push_body["output"]["commit_sha"].as_str().unwrap()
    );

    let audit = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/audit?limit=100&search=target_output")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(audit.status(), StatusCode::OK);
    let audit_body = response_json(audit).await;
    assert!(audit_body["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| {
            event["action"] == "supervised_patch.target_output_success"
                && event["resource"] == artifact_id
        }));
}

