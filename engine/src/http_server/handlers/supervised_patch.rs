use axum::extract::{Extension, Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;
use std::path::Path;

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, require_store, ApiError, RequestId,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::{
    SupervisedPatchWorkspaceCreateRequest, SupervisedPatchWorkspaceVerifyRequest,
    TargetRepoOutputRequest, AXUM_API_SCHEMA_VERSION,
};
use crate::node_executor::{CommandNodeExecutor, NodeExecutionInput, NodeExecutor};
use crate::provider::redaction::redact_sensitive_patterns;
use crate::target_repo_output::{
    create_or_reuse_github_pull_request, export_patch, github_repository_for_remote,
    prepare_git_worktree, push_approved_branch, remove_git_worktree, BranchPublishRequest,
    GitHubPullRequestConfig, GitHubPullRequestRequest, TargetRepoOutputConfig,
};

pub(crate) async fn api_supervised_patch_workspaces(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let limit = query_i64(&params, "limit", 100).clamp(0, 500);
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "metadata_only": true,
            "execution_authority": "disabled",
            "verification_execution_authority": "allowlisted_commands",
            "workspaces": store.supervised_patch_workspaces(limit).map_err(internal_error)?,
        })),
    ))
}

pub(crate) async fn api_supervised_patch_workspace_detail(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(workspace_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    match store
        .get_supervised_patch_workspace(&workspace_id)
        .map_err(internal_error)?
    {
        Some(workspace) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "metadata_only": true,
                "execution_authority": "disabled",
                "verification_execution_authority": "allowlisted_commands",
                "workspace": workspace,
            })),
        )),
        None => Err(not_found("supervised_patch_workspace_not_found")),
    }
}

pub(crate) async fn api_supervised_patch_artifacts(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let limit = query_i64(&params, "limit", 100).clamp(0, 500);
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "metadata_only": true,
            "execution_authority": "disabled",
            "artifacts": store.supervised_patch_artifacts(limit).map_err(internal_error)?,
        })),
    ))
}

pub(crate) async fn api_supervised_patch_artifact_detail(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(artifact_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    match store
        .get_supervised_patch_artifact(&artifact_id)
        .map_err(internal_error)?
    {
        Some(artifact) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "metadata_only": true,
                "execution_authority": "disabled",
                "artifact": artifact,
            })),
        )),
        None => Err(not_found("supervised_patch_artifact_not_found")),
    }
}

fn query_i64(params: &std::collections::HashMap<String, String>, key: &str, default: i64) -> i64 {
    params
        .get(key)
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(default)
}

fn uuid_v4_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:016x}", t)
}

fn not_found(code: &str) -> ApiError {
    ApiError::with_code(
        StatusCode::NOT_FOUND,
        code,
        "supervised patch metadata not found",
    )
}

pub(crate) async fn api_export_supervised_patch(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(artifact_id): AxumPath<String>,
    Json(request): Json<serde_json::Value>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;

    let run_id = request.get("run_id").and_then(|v| v.as_str()).unwrap_or("");

    let binding = store
        .validate_approval_binding(run_id, &artifact_id)
        .map_err(internal_error)?;

    if !binding["export_eligible"].as_bool().unwrap_or(false) {
        return Err(ApiError::with_code(
            StatusCode::FORBIDDEN,
            "export_not_approved",
            "artifact export requires valid approval binding",
        ));
    }

    let artifact = store
        .get_supervised_patch_artifact(&artifact_id)
        .map_err(internal_error)?
        .ok_or_else(|| not_found("artifact_not_found"))?;

    let integrity = store
        .validate_artifact_integrity(&artifact_id)
        .map_err(internal_error)?;

    if !integrity["integrity_ok"].as_bool().unwrap_or(false) {
        return Err(ApiError::with_code(
            StatusCode::CONFLICT,
            "integrity_check_failed",
            "artifact integrity check failed",
        ));
    }

    let _ = store.append_audit(
        &context.api_key_id,
        "supervised_patch.export",
        &artifact_id,
        &json!({
            "run_id": run_id,
            "export_eligible": true,
            "integrity_ok": true,
        }),
    );

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "export": {
                "artifact_id": artifact_id,
                "artifact": artifact,
                "approval_binding": binding,
                "integrity": integrity,
                "exported_by": context.api_key_id,
                "exported_at": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            },
        })),
    ))
}

pub(crate) async fn api_create_supervised_patch_workspace(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Json(request): Json<SupervisedPatchWorkspaceCreateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let workspace_mode = request.workspace_mode.as_deref().unwrap_or("copy");
    let required_scope = if workspace_mode == "git_worktree" {
        "dispatch:execute"
    } else {
        "dispatch:read"
    };
    let context = authorize(&state, &headers, required_scope, uri.path(), &request_id.0)?;
    let store = require_store(&state)?;

    let unique_id = format!(
        "ws-{}-{}",
        chrono::Utc::now().timestamp_millis(),
        uuid_v4_simple()
    );
    let config = TargetRepoOutputConfig::from_env();
    let (workspace_dir, source_revision, git) = match workspace_mode {
        "copy" => (
            store
                .create_workspace_directory(&unique_id, &request.target_repo_path)
                .map_err(internal_error)?,
            request.source_revision.clone(),
            serde_json::Value::Null,
        ),
        "git_worktree" => {
            let db_dir = store
                .db_path()
                .parent()
                .ok_or_else(|| internal_error("store has no parent directory".to_string()))?;
            let workspace_path = db_dir.join("workspaces").join(&unique_id);
            let prepared = match prepare_git_worktree(
                &config,
                Path::new(&request.target_repo_path),
                &workspace_path,
                &request.source_revision,
            ) {
                Ok(prepared) => prepared,
                Err(error) => {
                    audit_target_output_failure(
                        &store,
                        &context.api_key_id,
                        &unique_id,
                        "prepare_git_worktree",
                        "worktree_prepare_failed",
                    );
                    return Err(target_output_error(error));
                }
            };
            (
                prepared.workspace_path.clone(),
                prepared.source_revision.clone(),
                json!({
                    "default_branch": prepared.default_branch,
                    "source_revision": prepared.source_revision,
                }),
            )
        }
        _ => {
            return Err(ApiError::with_code(
                StatusCode::BAD_REQUEST,
                "invalid_workspace_mode",
                "workspace_mode must be copy or git_worktree",
            ));
        }
    };

    let workspace_request = json!({
        "run_id": request.run_id,
        "plan_id": request.plan_id,
        "target_id": request.target_id,
        "target_repo_path": request.target_repo_path,
        "workspace_path": workspace_dir,
        "source_revision": source_revision,
        "source_tree_hash": request.source_tree_hash,
        "workspace_mode": workspace_mode,
        "git": git,
        "status": "workspace_created",
    });

    match store.record_supervised_patch_workspace(&workspace_request, &context.api_key_id) {
        Ok(workspace) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "workspace": workspace,
            })),
        )),
        Err(e) => {
            if workspace_mode == "git_worktree" {
                let _ = remove_git_worktree(
                    &config,
                    Path::new(
                        workspace_request["target_repo_path"]
                            .as_str()
                            .unwrap_or_default(),
                    ),
                    Path::new(
                        workspace_request["workspace_path"]
                            .as_str()
                            .unwrap_or_default(),
                    ),
                );
            }
            Err(internal_error(e))
        }
    }
}

pub(crate) async fn api_target_repo_output(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(artifact_id): AxumPath<String>,
    Json(request): Json<TargetRepoOutputRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    let store = require_store(&state)?;
    if request.confirm_target_output != Some(true) {
        let _ = store.append_audit(
            &context.api_key_id,
            "supervised_patch.target_output_denied",
            &artifact_id,
            &json!({"reason": "confirmation_required", "mode": request.mode}),
        );
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "target_output_confirmation_required",
            "confirm_target_output=true is required",
        ));
    }
    let config = TargetRepoOutputConfig::from_env();
    if let Err(error) = config.require_enabled() {
        let _ = store.append_audit(
            &context.api_key_id,
            "supervised_patch.target_output_denied",
            &artifact_id,
            &json!({"reason": error, "mode": request.mode}),
        );
        return Err(target_output_error(error));
    }

    let artifact = store
        .get_supervised_patch_artifact(&artifact_id)
        .map_err(internal_error)?
        .ok_or_else(|| not_found("artifact_not_found"))?;
    let workspace_id = artifact
        .get("workspace_id")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let workspace = store
        .get_supervised_patch_workspace(workspace_id)
        .map_err(internal_error)?
        .ok_or_else(|| not_found("workspace_not_found"))?;
    if workspace
        .get("workspace_mode")
        .and_then(|value| value.as_str())
        != Some("git_worktree")
    {
        audit_target_output_failure(
            &store,
            &context.api_key_id,
            &artifact_id,
            &request.mode,
            "git_worktree_required",
        );
        return Err(ApiError::with_code(
            StatusCode::CONFLICT,
            "git_worktree_required",
            "target repo output requires a controlled git_worktree workspace",
        ));
    }

    let binding = store
        .validate_approval_binding(&request.run_id, &artifact_id)
        .map_err(internal_error)?;
    if !binding["export_eligible"].as_bool().unwrap_or(false) {
        let _ = store.append_audit(
            &context.api_key_id,
            "supervised_patch.target_output_denied",
            &artifact_id,
            &json!({"reason": "approval_binding_required", "mode": request.mode}),
        );
        return Err(ApiError::with_code(
            StatusCode::FORBIDDEN,
            "target_output_not_approved",
            "target repo output requires valid approval binding",
        ));
    }
    let integrity = store
        .validate_artifact_integrity(&artifact_id)
        .map_err(internal_error)?;
    if !integrity["integrity_ok"].as_bool().unwrap_or(false) {
        audit_target_output_failure(
            &store,
            &context.api_key_id,
            &artifact_id,
            &request.mode,
            "integrity_check_failed",
        );
        return Err(ApiError::with_code(
            StatusCode::CONFLICT,
            "integrity_check_failed",
            "artifact integrity check failed",
        ));
    }
    if artifact
        .get("secret_scan_status")
        .and_then(|value| value.as_str())
        != Some("passed")
        || artifact
            .get("redaction_status")
            .and_then(|value| value.as_str())
            != Some("redacted")
    {
        audit_target_output_failure(
            &store,
            &context.api_key_id,
            &artifact_id,
            &request.mode,
            "artifact_secret_scan_failed",
        );
        return Err(ApiError::with_code(
            StatusCode::CONFLICT,
            "artifact_secret_scan_failed",
            "target repo output requires passed secret scan and redacted artifact",
        ));
    }
    if !verification_evidence_ready(&artifact) {
        audit_target_output_failure(
            &store,
            &context.api_key_id,
            &artifact_id,
            &request.mode,
            "verification_evidence_required",
        );
        return Err(ApiError::with_code(
            StatusCode::CONFLICT,
            "verification_evidence_required",
            "target repo output requires recorded workflow verification evidence",
        ));
    }

    let workspace_path = Path::new(
        workspace
            .get("workspace_path")
            .and_then(|value| value.as_str())
            .unwrap_or(""),
    );
    let source_revision = artifact
        .get("source_revision")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let expected_patch_hash = artifact
        .get("patch_hash")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let output = match request.mode.as_str() {
        "export_patch" => {
            let output = match export_patch(&config, workspace_path, source_revision) {
                Ok(output) => output,
                Err(error) => {
                    audit_target_output_failure(
                        &store,
                        &context.api_key_id,
                        &artifact_id,
                        &request.mode,
                        "patch_export_failed",
                    );
                    return Err(target_output_error(error));
                }
            };
            if output.patch_hash != expected_patch_hash {
                audit_target_output_failure(
                    &store,
                    &context.api_key_id,
                    &artifact_id,
                    &request.mode,
                    "approved_patch_changed",
                );
                return Err(ApiError::with_code(
                    StatusCode::CONFLICT,
                    "approved_patch_changed",
                    "workspace patch no longer matches approved artifact",
                ));
            }
            serde_json::to_value(output).map_err(|error| internal_error(error.to_string()))?
        }
        "push_branch" => {
            let branch_name = request
                .branch_name
                .unwrap_or_else(|| format!("acp/{artifact_id}"));
            let remote = request.remote.unwrap_or_else(|| "origin".to_string());
            let commit_message = request
                .commit_message
                .unwrap_or_else(|| format!("feat: apply approved artifact {artifact_id}"));
            let pr_title = request
                .pr_title
                .unwrap_or_else(|| format!("Apply approved artifact {artifact_id}"));
            let pr_body = build_pr_body(&artifact);
            let publish_branch = branch_name.clone();
            let publish_remote = remote.clone();
            let publish_title = pr_title.clone();
            let publish_body = pr_body.clone();
            let pending_pull_request = if request.create_pull_request == Some(true) {
                let github_config = GitHubPullRequestConfig::from_env();
                if let Err(error) = github_config.require_enabled() {
                    audit_target_output_failure(
                        &store,
                        &context.api_key_id,
                        &artifact_id,
                        &request.mode,
                        "pull_request_preflight_failed",
                    );
                    return Err(target_output_error(error));
                }
                let repository =
                    match github_repository_for_remote(&config, workspace_path, &publish_remote) {
                        Ok(repository) => repository,
                        Err(error) => {
                            audit_target_output_failure(
                                &store,
                                &context.api_key_id,
                                &artifact_id,
                                &request.mode,
                                "pull_request_preflight_failed",
                            );
                            return Err(target_output_error(error));
                        }
                    };
                let base_branch = match workspace
                    .get("git")
                    .and_then(|value| value.get("default_branch"))
                    .and_then(|value| value.as_str())
                {
                    Some(base_branch) => base_branch.to_string(),
                    None => {
                        audit_target_output_failure(
                            &store,
                            &context.api_key_id,
                            &artifact_id,
                            &request.mode,
                            "pull_request_preflight_failed",
                        );
                        return Err(ApiError::with_code(
                            StatusCode::CONFLICT,
                            "default_branch_missing",
                            "workspace is missing the target default branch",
                        ));
                    }
                };
                Some((
                    github_config,
                    GitHubPullRequestRequest {
                        repository,
                        head_branch: publish_branch,
                        base_branch,
                        title: publish_title,
                        body: publish_body,
                    },
                ))
            } else {
                None
            };
            let output = match push_approved_branch(
                &config,
                BranchPublishRequest {
                    target_repo_path: Path::new(
                        workspace
                            .get("target_repo_path")
                            .and_then(|value| value.as_str())
                            .unwrap_or(""),
                    )
                    .to_path_buf(),
                    workspace_path: workspace_path.to_path_buf(),
                    source_revision: source_revision.to_string(),
                    expected_patch_hash: expected_patch_hash.to_string(),
                    branch_name,
                    remote,
                    commit_message,
                    pr_title,
                    pr_body,
                },
            ) {
                Ok(output) => output,
                Err(error) => {
                    audit_target_output_failure(
                        &store,
                        &context.api_key_id,
                        &artifact_id,
                        &request.mode,
                        "branch_push_failed",
                    );
                    return Err(target_output_error(error));
                }
            };
            let mut output =
                serde_json::to_value(output).map_err(|error| internal_error(error.to_string()))?;
            if let Some((github_config, pull_request_request)) = pending_pull_request {
                let pull_request = match create_or_reuse_github_pull_request(
                    &github_config,
                    &pull_request_request,
                )
                .await
                {
                    Ok(pull_request) => pull_request,
                    Err(error) => {
                        audit_target_output_failure(
                            &store,
                            &context.api_key_id,
                            &artifact_id,
                            &request.mode,
                            "pull_request_failed",
                        );
                        return Err(target_output_error(error));
                    }
                };
                if let Some(object) = output.as_object_mut() {
                    object.insert(
                        "pull_request".to_string(),
                        serde_json::to_value(pull_request)
                            .map_err(|error| internal_error(error.to_string()))?,
                    );
                }
            }
            output
        }
        _ => {
            audit_target_output_failure(
                &store,
                &context.api_key_id,
                &artifact_id,
                &request.mode,
                "invalid_target_output_mode",
            );
            return Err(ApiError::with_code(
                StatusCode::BAD_REQUEST,
                "invalid_target_output_mode",
                "mode must be export_patch or push_branch",
            ));
        }
    };

    let _ = store.append_audit(
        &context.api_key_id,
        "supervised_patch.target_output_success",
        &artifact_id,
        &json!({
            "run_id": request.run_id,
            "mode": request.mode,
            "patch_hash": expected_patch_hash,
            "branch_name": output.get("branch_name"),
            "commit_sha": output.get("commit_sha"),
            "approval_id": binding.get("approving_approval").and_then(|value| value.get("approval_id")),
            "kill_path": "ACP_TARGET_REPO_OUTPUT_KILL_SWITCH=1",
        }),
    );

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "output": output,
            "approval_binding": binding,
            "integrity": integrity,
        })),
    ))
}

fn audit_target_output_failure(
    store: &crate::storage::local_product_store::LocalProductStore,
    actor: &str,
    artifact_id: &str,
    mode: &str,
    reason: &str,
) {
    let _ = store.append_audit(
        actor,
        "supervised_patch.target_output_failure",
        artifact_id,
        &json!({
            "mode": mode,
            "reason": reason,
            "kill_path": "ACP_TARGET_REPO_OUTPUT_KILL_SWITCH=1",
        }),
    );
}

fn build_pr_body(artifact: &serde_json::Value) -> String {
    let artifact_id = artifact
        .get("artifact_id")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    let patch_hash = artifact
        .get("patch_hash")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    let source_revision = artifact
        .get("source_revision")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    let changed_files = artifact
        .get("changed_files")
        .and_then(|value| value.as_array())
        .map(|files| {
            files
                .iter()
                .filter_map(|value| value.as_str())
                .map(|file| format!("- `{file}`"))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    let verification = artifact
        .get("evidence_bundle")
        .and_then(|value| value.get("verification"))
        .and_then(|value| value.get("status"))
        .and_then(|value| value.as_str())
        .unwrap_or("not_run");
    format!(
        "Artifact: `{artifact_id}`\n\nPatch: `{patch_hash}`\n\nSource: `{source_revision}`\n\nVerification: `{verification}`\n\nChanged files:\n{changed_files}"
    )
}

fn verification_evidence_ready(artifact: &serde_json::Value) -> bool {
    artifact
        .get("evidence_bundle")
        .and_then(|value| value.get("verification"))
        .and_then(|value| value.get("status"))
        .and_then(|value| value.as_str())
        == Some("evidence_recorded")
}

fn target_output_error(error: String) -> ApiError {
    let (status, code) =
        if error.contains("ACP_ENABLE_TARGET_REPO_OUTPUT") || error.contains("kill switch") {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "target_repo_output_disabled",
            )
        } else if error.contains("changed")
            || error.contains("hash")
            || error.contains("already exists")
        {
            (StatusCode::CONFLICT, "target_repo_output_conflict")
        } else {
            (StatusCode::BAD_REQUEST, "target_repo_output_invalid")
        };
    ApiError::with_code(status, code, &error)
}

pub(crate) async fn api_cleanup_supervised_patch_workspace(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(workspace_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let store = require_store(&state)?;
    let workspace = store
        .get_supervised_patch_workspace(&workspace_id)
        .map_err(internal_error)?
        .ok_or_else(|| not_found("workspace_not_found"))?;
    let required_scope = if workspace
        .get("workspace_mode")
        .and_then(|value| value.as_str())
        == Some("git_worktree")
    {
        "dispatch:execute"
    } else {
        "dispatch:read"
    };
    let context = authorize(&state, &headers, required_scope, uri.path(), &request_id.0)?;
    match store.cleanup_workspace(&workspace_id, &context.api_key_id) {
        Ok(workspace) => {
            let _ = store.append_audit(
                &context.api_key_id,
                "supervised_patch.cleanup",
                &workspace_id,
                &json!({"status": "cleaned"}),
            );
            Ok((
                cors_headers(),
                Json(json!({
                    "schema_version": AXUM_API_SCHEMA_VERSION,
                    "workspace": workspace,
                })),
            ))
        }
        Err(e) if e.contains("not found") => Err(not_found("workspace_not_found")),
        Err(e) if e.contains("invalid") => Err(ApiError::with_code(
            StatusCode::CONFLICT,
            "invalid_status_transition",
            &e,
        )),
        Err(e) => Err(internal_error(e)),
    }
}

pub(crate) async fn api_quarantine_supervised_patch_workspace(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(workspace_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    match store.quarantine_workspace(&workspace_id, &context.api_key_id) {
        Ok(workspace) => {
            let _ = store.append_audit(
                &context.api_key_id,
                "supervised_patch.quarantine",
                &workspace_id,
                &json!({"status": "quarantined"}),
            );
            Ok((
                cors_headers(),
                Json(json!({
                    "schema_version": AXUM_API_SCHEMA_VERSION,
                    "workspace": workspace,
                })),
            ))
        }
        Err(e) if e.contains("not found") => Err(not_found("workspace_not_found")),
        Err(e) if e.contains("invalid") => Err(ApiError::with_code(
            StatusCode::CONFLICT,
            "invalid_status_transition",
            &e,
        )),
        Err(e) => Err(internal_error(e)),
    }
}

pub(crate) async fn api_verify_supervised_patch_workspace(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(workspace_id): AxumPath<String>,
    Json(request): Json<SupervisedPatchWorkspaceVerifyRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    if request.confirm_verification != Some(true) {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "verification_confirmation_required",
            "confirm_verification=true is required",
        ));
    }
    let store = require_store(&state)?;
    let workspace = store
        .get_supervised_patch_workspace(&workspace_id)
        .map_err(internal_error)?
        .ok_or_else(|| not_found("workspace_not_found"))?;
    let command = request.command.trim().to_string();
    if command.is_empty() {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "verification_command_required",
            "command is required",
        ));
    }
    let command_argv: Vec<String> = command.split_whitespace().map(str::to_string).collect();
    let timeout_ms = request.timeout_ms.unwrap_or(120_000).clamp(1_000, 600_000);
    let attempt = request.attempt.unwrap_or(1).clamp(1, 3);
    let workspace_path = workspace
        .get("workspace_path")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let run_id = workspace
        .get("run_id")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let workflow_id = workspace
        .get("plan_id")
        .and_then(|value| value.as_str())
        .unwrap_or("workspace_verification")
        .to_string();
    let repair_executor = request.repair_executor.as_deref();
    if let Some(executor) = repair_executor {
        if !matches!(executor, "codex_cli" | "claude_code_cli") {
            return Err(ApiError::with_code(
                StatusCode::BAD_REQUEST,
                "invalid_repair_executor",
                "repair_executor must be codex_cli or claude_code_cli",
            ));
        }
    }
    let max_repair_attempts = if repair_executor.is_some() {
        request.max_repair_attempts.unwrap_or(1).clamp(1, 2)
    } else {
        0
    };
    let mut verification_attempts = Vec::new();
    let mut repair_attempts = Vec::new();
    let mut output = run_workspace_verification(
        command.clone(),
        workspace_path.clone(),
        run_id.clone(),
        workflow_id.clone(),
        timeout_ms,
        attempt,
    )
    .await?;
    verification_attempts.push(verification_attempt_value(attempt, &output));

    for repair_attempt in 1..=max_repair_attempts {
        if output.status == "completed" {
            break;
        }
        let failure_summary = output
            .error_message
            .as_deref()
            .or(output.output.as_deref())
            .unwrap_or("verification command failed");
        let repair_output = run_cli_repair(
            repair_executor.unwrap_or_default().to_string(),
            command.clone(),
            redact_sensitive_patterns(failure_summary),
            workspace_path.clone(),
            run_id.clone(),
            workflow_id.clone(),
            repair_attempt,
        )
        .await?;
        repair_attempts.push(verification_attempt_value(repair_attempt, &repair_output));
        if repair_output.status != "completed" {
            break;
        }
        let verification_attempt = attempt + repair_attempt;
        output = run_workspace_verification(
            command.clone(),
            workspace_path.clone(),
            run_id.clone(),
            workflow_id.clone(),
            timeout_ms,
            verification_attempt,
        )
        .await?;
        verification_attempts.push(verification_attempt_value(verification_attempt, &output));
    }

    let passed = output.status == "completed";
    let verification = json!({
        "schema_version": "workspace_verification.v1",
        "status": if passed { "evidence_recorded" } else { "verification_failed" },
        "command": command_argv,
        "result_status": output.status,
        "executor_type": output.executor_type,
        "output": output.output.as_deref().map(redact_sensitive_patterns),
        "error_domain": output.error_domain,
        "error_message": output.error_message.as_deref().map(redact_sensitive_patterns),
        "latency_ms": output.latency_ms,
        "timeout_ms": timeout_ms,
        "attempt": attempt,
        "verification_attempts": verification_attempts,
        "repair_attempts": repair_attempts,
        "recorded_at": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    });
    store
        .record_workspace_verification(&workspace_id, &verification, &context.api_key_id)
        .map_err(internal_error)?;

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "verification": verification,
        })),
    ))
}

async fn run_workspace_verification(
    command: String,
    workspace_path: String,
    run_id: String,
    workflow_id: String,
    timeout_ms: u64,
    attempt: u64,
) -> Result<crate::node_executor::NodeExecutionOutput, ApiError> {
    let input = NodeExecutionInput {
        node_id: format!("verify-{run_id}-{attempt}"),
        task_type: "workspace_verification".to_string(),
        run_id,
        workflow_id,
        node_metadata: json!({
            "command": command,
            "workspace_path": workspace_path,
            "workspace_root": workspace_path,
        }),
    };
    let executor = CommandNodeExecutor {
        timeout_ms,
        allowed_commands: Vec::new(),
        allowed_binaries: [
            "cargo", "bun", "node", "npm", "pnpm", "yarn", "uv", "python", "python3", "go", "make",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        env_vars: Vec::new(),
    };
    tokio::task::spawn_blocking(move || executor.execute_node(&input))
        .await
        .map_err(|error| internal_error(error.to_string()))
}

async fn run_cli_repair(
    executor_type: String,
    command: String,
    failure_summary: String,
    workspace_path: String,
    run_id: String,
    workflow_id: String,
    attempt: u64,
) -> Result<crate::node_executor::NodeExecutionOutput, ApiError> {
    let cli_config = crate::cli::CliConfig::from_env();
    let executor = crate::cli::CliNodeExecutor::from_config(&cli_config).ok_or_else(|| {
        ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "cli_not_available",
            "CLI repair requires ACP_ENABLE_CLI_EXECUTION=1 and an available CLI binary",
        )
    })?;
    let prompt = format!(
        "Fix the repository in the current workspace so this verification command passes.\n\
         Command: {command}\n\
         Failure: {failure_summary}\n\
         Make the smallest correct change. Do not commit, push, or modify files outside the workspace."
    );
    let input = NodeExecutionInput {
        node_id: format!("repair-{run_id}-{attempt}"),
        task_type: "workspace_repair".to_string(),
        run_id,
        workflow_id,
        node_metadata: json!({
            "executor": executor_type,
            "prompt": prompt,
            "workspace_path": workspace_path,
        }),
    };
    tokio::task::spawn_blocking(move || executor.execute_node(&input))
        .await
        .map_err(|error| internal_error(error.to_string()))
}

fn verification_attempt_value(
    attempt: u64,
    output: &crate::node_executor::NodeExecutionOutput,
) -> serde_json::Value {
    json!({
        "attempt": attempt,
        "status": output.status,
        "executor_type": output.executor_type,
        "output": output.output.as_deref().map(redact_sensitive_patterns),
        "error_domain": output.error_domain,
        "error_message": output.error_message.as_deref().map(redact_sensitive_patterns),
        "latency_ms": output.latency_ms,
    })
}

pub(crate) async fn api_capture_supervised_patch(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(workspace_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let store = require_store(&state)?;
    let workspace = store
        .get_supervised_patch_workspace(&workspace_id)
        .map_err(internal_error)?
        .ok_or_else(|| not_found("workspace_not_found"))?;
    let required_scope = if workspace
        .get("workspace_mode")
        .and_then(|value| value.as_str())
        == Some("git_worktree")
    {
        "dispatch:execute"
    } else {
        "dispatch:read"
    };
    let context = authorize(&state, &headers, required_scope, uri.path(), &request_id.0)?;
    match store.capture_patch(&workspace_id, &context.api_key_id) {
        Ok(artifact) => {
            let _ = store.append_audit(
                &context.api_key_id,
                "supervised_patch.capture",
                &workspace_id,
                &json!({
                    "artifact_id": artifact.get("artifact_id"),
                    "changed_files_count": artifact.get("changed_files").and_then(|v| v.as_array()).map(|a| a.len()),
                    "secret_scan_status": artifact.get("secret_scan_status"),
                    "patch_hash": artifact.get("patch_hash"),
                }),
            );
            Ok((
                cors_headers(),
                Json(json!({
                    "schema_version": AXUM_API_SCHEMA_VERSION,
                    "artifact": artifact,
                })),
            ))
        }
        Err(e) if e.contains("not found") => Err(not_found("workspace_not_found")),
        Err(e) if e.contains("no files") => Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "no_files_to_capture",
            &e,
        )),
        Err(e) => Err(internal_error(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::verification_evidence_ready;
    use serde_json::json;

    #[test]
    fn target_output_requires_recorded_verification_evidence() {
        assert!(!verification_evidence_ready(&json!({
            "evidence_bundle": {"verification": {"status": "not_run"}}
        })));
        assert!(verification_evidence_ready(&json!({
            "evidence_bundle": {"verification": {"status": "evidence_recorded"}}
        })));
    }
}
