use axum::extract::{Extension, Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::Path;

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, require_store, ApiError, RequestId,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::{
    SupervisedPatchWorkspaceCreateRequest, SupervisedPatchWorkspaceVerifyRequest,
    TargetRepoOutputRequest, AXUM_API_SCHEMA_VERSION,
};
use crate::node_executor::CommandNodeExecutor;
use crate::provider::redaction::redact_sensitive_patterns;
use crate::storage::local_product_store::{LocalProductStore, TargetOutputClaim};
use crate::target_repo_output::{
    create_or_reuse_github_pull_request, export_patch, github_repository_for_remote,
    prepare_git_worktree, push_approved_branch, remove_git_worktree, BranchPublishRequest,
    GitHubPullRequestConfig, GitHubPullRequestRequest, TargetRepoOutputConfig,
};
use crate::tool_policy_executor::ToolPolicyNodeExecutor;

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
    let effective_branch = request
        .branch_name
        .clone()
        .unwrap_or_else(|| format!("acp/{artifact_id}"));
    let effective_remote = request
        .remote
        .clone()
        .unwrap_or_else(|| "origin".to_string());
    let effective_commit_message = request
        .commit_message
        .clone()
        .unwrap_or_else(|| format!("feat: apply approved artifact {artifact_id}"));
    let effective_pr_title = request
        .pr_title
        .clone()
        .unwrap_or_else(|| format!("Apply approved artifact {artifact_id}"));
    let request_binding = json!({
        "schema_version": "target_repo_output_request.v1",
        "artifact_id": artifact_id,
        "workspace_id": artifact.get("workspace_id"),
        "run_id": request.run_id,
        "target_id": artifact.get("target_id"),
        "mode": request.mode,
        "patch_hash": artifact.get("patch_hash"),
        "source_revision": artifact.get("source_revision"),
        "approval_id": binding.get("approving_approval").and_then(|value| value.get("approval_id")),
        "branch_name": if request.mode == "push_branch" { Some(effective_branch.as_str()) } else { None },
        "remote": if request.mode == "push_branch" { Some(effective_remote.as_str()) } else { None },
        "commit_message": if request.mode == "push_branch" { Some(effective_commit_message.as_str()) } else { None },
        "pr_title": if request.mode == "push_branch" { Some(effective_pr_title.as_str()) } else { None },
        "create_pull_request": request.create_pull_request == Some(true),
    });
    let request_sha256 = hex::encode(Sha256::digest(
        serde_json::to_vec(&request_binding).map_err(|error| internal_error(error.to_string()))?,
    ));
    if artifact.get("target_output_receipt").is_some() {
        match store
            .claim_target_output(
                &artifact_id,
                &request_binding,
                &request_sha256,
                &context.api_key_id,
            )
            .map_err(target_output_receipt_error)?
        {
            TargetOutputClaim::Reused(output) => {
                let _ = store.append_audit(
                    &context.api_key_id,
                    "supervised_patch.target_output_reused",
                    &artifact_id,
                    &json!({"request_sha256": request_sha256, "receipt_state": "completed"}),
                );
                return Ok((
                    cors_headers(),
                    Json(json!({
                        "schema_version": AXUM_API_SCHEMA_VERSION,
                        "output": output,
                        "approval_binding": binding,
                        "reused": true,
                    })),
                ));
            }
            TargetOutputClaim::ReconciliationRequired(state) => {
                return Err(ApiError::with_code(
                    StatusCode::CONFLICT,
                    "target_output_reconciliation_required",
                    format!("target output is {state}; automatic delivery is refused"),
                ));
            }
            TargetOutputClaim::Claimed => {
                return Err(internal_error(
                    "existing target output receipt unexpectedly produced a new claim".to_string(),
                ));
            }
        }
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
            let branch_name = effective_branch;
            let remote = effective_remote;
            let commit_message = effective_commit_message;
            let pr_title = effective_pr_title;
            let pr_body = build_pr_body(&artifact);
            let publish_branch = branch_name.clone();
            let publish_remote = remote.clone();
            let publish_title = pr_title.clone();
            let publish_body = pr_body.clone();
            let pending_pull_request = if request.create_pull_request == Some(true) {
                let github_config = GitHubPullRequestConfig::from_env();
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
                if let Err(error) = github_config.require_repository(&repository) {
                    audit_target_output_failure(
                        &store,
                        &context.api_key_id,
                        &artifact_id,
                        &request.mode,
                        "pull_request_preflight_failed",
                    );
                    return Err(target_output_error(error));
                }
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
                        expected_head_sha: None,
                    },
                ))
            } else {
                None
            };
            match store
                .claim_target_output(
                    &artifact_id,
                    &request_binding,
                    &request_sha256,
                    &context.api_key_id,
                )
                .map_err(target_output_receipt_error)?
            {
                TargetOutputClaim::Claimed => {}
                TargetOutputClaim::Reused(output) => {
                    let _ = store.append_audit(
                        &context.api_key_id,
                        "supervised_patch.target_output_reused",
                        &artifact_id,
                        &json!({"request_sha256": request_sha256, "receipt_state": "completed"}),
                    );
                    return Ok((
                        cors_headers(),
                        Json(json!({
                            "schema_version": AXUM_API_SCHEMA_VERSION,
                            "output": output,
                            "approval_binding": binding,
                            "reused": true,
                        })),
                    ));
                }
                TargetOutputClaim::ReconciliationRequired(state) => {
                    return Err(ApiError::with_code(
                        StatusCode::CONFLICT,
                        "target_output_reconciliation_required",
                        format!("target output is {state}; automatic delivery is refused"),
                    ));
                }
            }
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
                    let _ = store.mark_target_output_outcome_unknown(
                        &artifact_id,
                        &request_binding,
                        &request_sha256,
                        &context.api_key_id,
                        "branch_push_failed",
                    );
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
            let published_commit_sha = output.commit_sha.clone();
            let mut output =
                serde_json::to_value(output).map_err(|error| internal_error(error.to_string()))?;
            if let Some((github_config, mut pull_request_request)) = pending_pull_request {
                pull_request_request.expected_head_sha = Some(published_commit_sha);
                let pull_request = match create_or_reuse_github_pull_request(
                    &github_config,
                    &pull_request_request,
                )
                .await
                {
                    Ok(pull_request) => pull_request,
                    Err(error) => {
                        let _ = store.mark_target_output_outcome_unknown(
                            &artifact_id,
                            &request_binding,
                            &request_sha256,
                            &context.api_key_id,
                            "pull_request_failed",
                        );
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

    if request.mode == "push_branch" {
        if let Err(error) = store.record_target_output_receipt(
            &artifact_id,
            &request_binding,
            &request_sha256,
            &output,
            &context.api_key_id,
        ) {
            let _ = store.mark_target_output_outcome_unknown(
                &artifact_id,
                &request_binding,
                &request_sha256,
                &context.api_key_id,
                "local_finalize_failed",
            );
            return Err(internal_error(error));
        }
    } else {
        let _ = store.append_audit(
            &context.api_key_id,
            "supervised_patch.target_output_success",
            &artifact_id,
            &json!({
                "run_id": request.run_id,
                "mode": request.mode,
                "patch_hash": expected_patch_hash,
                "kill_path": "ACP_TARGET_REPO_OUTPUT_KILL_SWITCH=1",
            }),
        );
    }

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "output": output,
            "approval_binding": binding,
            "integrity": integrity,
            "reused": false,
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

fn target_output_receipt_error(error: String) -> ApiError {
    let code = if error.contains("does not match durable receipt") {
        "target_output_request_mismatch"
    } else {
        "target_output_receipt_invalid"
    };
    ApiError::with_code(StatusCode::CONFLICT, code, &error)
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
    let max_executor_timeout_ms = repair_executor
        .map(|_| crate::cli::CliConfig::from_env().timeout_ms)
        .unwrap_or(0)
        .max(timeout_ms);
    validate_managed_scheduler_lease(&state, max_executor_timeout_ms)?;
    let resume = request
        .resume_run_id
        .as_deref()
        .map(|run_id| managed_resume_binding(&store, run_id, &workspace_id))
        .transpose()?;
    let prior_verification = workspace.get("verification");
    let mut verification_attempts = if resume.is_some() {
        prior_verification
            .and_then(|value| value.get("verification_attempts"))
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let mut repair_attempts = if resume.is_some() {
        prior_verification
            .and_then(|value| value.get("repair_attempts"))
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let mut next_repair_attempt = 1;
    let mut execution = match resume.as_ref() {
        Some(binding) if binding.operation == "verify" => {
            if binding.attempt < attempt || binding.attempt > attempt + max_repair_attempts {
                return Err(ApiError::with_code(
                    StatusCode::BAD_REQUEST,
                    "invalid_verification_resume",
                    "managed verification attempt is not reachable with the requested repair bounds",
                ));
            }
            next_repair_attempt = binding.attempt.saturating_sub(attempt) + 1;
            let execution = run_workspace_verification(
                store.clone(),
                &context.api_key_id,
                workspace_id.clone(),
                command.clone(),
                workspace_path.clone(),
                timeout_ms,
                binding.attempt,
                Some(&binding.run_id),
            )
            .await?;
            upsert_verification_attempt(&mut verification_attempts, binding.attempt, &execution);
            execution
        }
        Some(binding) if binding.operation == "repair" => {
            if repair_executor.is_none()
                || binding.attempt == 0
                || binding.attempt > max_repair_attempts
            {
                return Err(ApiError::with_code(
                    StatusCode::BAD_REQUEST,
                    "invalid_verification_resume",
                    "managed repair attempt is not enabled by the requested repair configuration",
                ));
            }
            next_repair_attempt = binding.attempt + 1;
            let repair_execution = run_cli_repair(
                store.clone(),
                &context.api_key_id,
                workspace_id.clone(),
                repair_executor.unwrap_or_default().to_string(),
                command.clone(),
                "resume exact managed repair".to_string(),
                workspace_path.clone(),
                binding.attempt,
                Some(&binding.node),
                Some(&binding.run_id),
            )
            .await?;
            upsert_verification_attempt(&mut repair_attempts, binding.attempt, &repair_execution);
            if repair_execution.output.status == "completed" {
                let verification_attempt = attempt + binding.attempt;
                let verification = run_workspace_verification(
                    store.clone(),
                    &context.api_key_id,
                    workspace_id.clone(),
                    command.clone(),
                    workspace_path.clone(),
                    timeout_ms,
                    verification_attempt,
                    None,
                )
                .await?;
                upsert_verification_attempt(
                    &mut verification_attempts,
                    verification_attempt,
                    &verification,
                );
                verification
            } else {
                repair_execution
            }
        }
        Some(_) => {
            return Err(ApiError::with_code(
                StatusCode::BAD_REQUEST,
                "invalid_verification_resume",
                "managed run has an unsupported supervised-patch operation",
            ));
        }
        None => {
            let execution = run_workspace_verification(
                store.clone(),
                &context.api_key_id,
                workspace_id.clone(),
                command.clone(),
                workspace_path.clone(),
                timeout_ms,
                attempt,
                None,
            )
            .await?;
            upsert_verification_attempt(&mut verification_attempts, attempt, &execution);
            execution
        }
    };
    let mut approval_required = execution.output.status == "awaiting_approval";

    for repair_attempt in next_repair_attempt..=max_repair_attempts {
        if execution.output.status == "completed" || approval_required {
            break;
        }
        let failure_summary = execution
            .output
            .error_message
            .as_deref()
            .or(execution.output.output.as_deref())
            .unwrap_or("verification command failed");
        let repair_execution = run_cli_repair(
            store.clone(),
            &context.api_key_id,
            workspace_id.clone(),
            repair_executor.unwrap_or_default().to_string(),
            command.clone(),
            redact_sensitive_patterns(failure_summary),
            workspace_path.clone(),
            repair_attempt,
            None,
            None,
        )
        .await?;
        upsert_verification_attempt(&mut repair_attempts, repair_attempt, &repair_execution);
        if repair_execution.output.status == "awaiting_approval" {
            execution = repair_execution;
            approval_required = true;
            break;
        }
        if repair_execution.output.status != "completed" {
            execution = repair_execution;
            break;
        }
        let verification_attempt = attempt + repair_attempt;
        execution = run_workspace_verification(
            store.clone(),
            &context.api_key_id,
            workspace_id.clone(),
            command.clone(),
            workspace_path.clone(),
            timeout_ms,
            verification_attempt,
            None,
        )
        .await?;
        upsert_verification_attempt(&mut verification_attempts, verification_attempt, &execution);
        approval_required = execution.output.status == "awaiting_approval";
    }

    let output = &execution.output;
    let passed = output.status == "completed";
    let verification = json!({
        "schema_version": "workspace_verification.v1",
        "status": if approval_required { "approval_required" } else if passed { "evidence_recorded" } else { "verification_failed" },
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
        "managed_run_id": execution.run_id,
        "managed_node_id": execution.node_id,
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
    store: std::sync::Arc<LocalProductStore>,
    actor: &str,
    workspace_id: String,
    command: String,
    workspace_path: String,
    timeout_ms: u64,
    attempt: u64,
    resume_run_id: Option<&str>,
) -> Result<ManagedToolExecution, ApiError> {
    let node_metadata = json!({
        "profile_id": "supervised_patch_verification",
        "command": command,
        "workspace_path": workspace_path,
        "workspace_root": workspace_path,
        "executor_timeout_ms": timeout_ms,
    });
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
    let executor = ToolPolicyNodeExecutor::command(std::sync::Arc::new(executor), store.clone());
    run_managed_tool_node(
        store,
        executor,
        actor,
        workspace_id,
        "verify",
        attempt,
        node_metadata,
        resume_run_id,
    )
    .await
}

async fn run_cli_repair(
    store: std::sync::Arc<LocalProductStore>,
    actor: &str,
    workspace_id: String,
    executor_type: String,
    command: String,
    failure_summary: String,
    workspace_path: String,
    attempt: u64,
    resume_node: Option<&serde_json::Value>,
    resume_run_id: Option<&str>,
) -> Result<ManagedToolExecution, ApiError> {
    let cli_config = crate::cli::CliConfig::from_env();
    let inner = crate::cli::CliNodeExecutor::from_config_for(&cli_config, &executor_type)
        .ok_or_else(|| {
            ApiError::with_code(
                StatusCode::BAD_REQUEST,
                "cli_not_available",
                "CLI repair requires ACP_ENABLE_CLI_EXECUTION=1 and an available CLI binary",
            )
        })?;
    let redacted_failure = redact_sensitive_patterns(&failure_summary);
    let default_prompt = format!(
        "Fix the repository in the current workspace so this verification command passes.\n\
         Command: {command}\n\
         Failure: {redacted_failure}\n\
         Make the smallest correct change. Do not commit, push, or modify files outside the workspace."
    );
    let prompt = resume_node
        .and_then(|node| node.get("prompt"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(&default_prompt)
        .to_string();
    let failure_summary_sha256 = resume_node
        .and_then(|node| node.get("failure_summary_sha256"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| hex::encode(Sha256::digest(redacted_failure.as_bytes())));
    let tool_name = executor_type.clone();
    let node_metadata = json!({
        "profile_id": "supervised_patch_repair",
        "executor": executor_type,
        "prompt": prompt,
        "verification_command": command,
        "failure_summary_sha256": failure_summary_sha256,
        "workspace_path": workspace_path,
        "workspace_root": workspace_path,
        "executor_timeout_ms": cli_config.timeout_ms,
    });
    let executor =
        ToolPolicyNodeExecutor::cli(std::sync::Arc::new(inner), store.clone(), tool_name);
    run_managed_tool_node(
        store,
        executor,
        actor,
        workspace_id,
        "repair",
        attempt,
        node_metadata,
        resume_run_id,
    )
    .await
}

#[derive(Debug)]
struct ManagedToolExecution {
    output: crate::node_executor::NodeExecutionOutput,
    run_id: String,
    node_id: String,
}

#[derive(Debug)]
struct ManagedResumeBinding {
    run_id: String,
    operation: String,
    attempt: u64,
    node: serde_json::Value,
}

fn managed_resume_binding(
    store: &LocalProductStore,
    run_id: &str,
    workspace_id: &str,
) -> Result<ManagedResumeBinding, ApiError> {
    let run = store
        .get_workflow_run(run_id)
        .map_err(internal_error)?
        .ok_or_else(|| {
            ApiError::with_code(
                StatusCode::BAD_REQUEST,
                "invalid_verification_resume",
                "managed verification run was not found",
            )
        })?;
    let nodes = run
        .get("nodes")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| internal_error("managed verification run has no nodes".to_string()))?;
    if nodes.len() != 1 {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "invalid_verification_resume",
            "managed verification run must contain exactly one node",
        ));
    }
    let binding = nodes[0]
        .get("managed_supervised_patch")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            ApiError::with_code(
                StatusCode::BAD_REQUEST,
                "invalid_verification_resume",
                "run is not a managed supervised-patch execution",
            )
        })?;
    if binding
        .get("workspace_id")
        .and_then(serde_json::Value::as_str)
        != Some(workspace_id)
    {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "invalid_verification_resume",
            "managed verification run belongs to another workspace",
        ));
    }
    let operation = binding
        .get("operation")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| internal_error("managed verification operation is missing".to_string()))?;
    let attempt = binding
        .get("attempt")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| internal_error("managed verification attempt is missing".to_string()))?;
    Ok(ManagedResumeBinding {
        run_id: run_id.to_string(),
        operation: operation.to_string(),
        attempt,
        node: nodes[0].clone(),
    })
}

async fn run_managed_tool_node(
    store: std::sync::Arc<LocalProductStore>,
    executor: ToolPolicyNodeExecutor<'static>,
    actor: &str,
    workspace_id: String,
    operation: &str,
    attempt: u64,
    mut node_metadata: serde_json::Value,
    resume_run_id: Option<&str>,
) -> Result<ManagedToolExecution, ApiError> {
    let binding_sha256 = crate::tool_policy_executor::managed_tool_binding_sha256(
        &workspace_id,
        operation,
        attempt,
        &node_metadata,
    )
    .map_err(internal_error)?;
    let binding = json!({
        "schema_version": "managed_supervised_patch.v1",
        "workspace_id": workspace_id,
        "operation": operation,
        "attempt": attempt,
        "binding_sha256": binding_sha256,
        "content_excluded": true,
    });
    node_metadata
        .as_object_mut()
        .ok_or_else(|| internal_error("managed tool metadata must be an object".to_string()))?
        .insert("managed_supervised_patch".to_string(), binding.clone());
    let node_id = format!("supervised-{operation}-{attempt}");

    let run_id = if let Some(resume_run_id) = resume_run_id {
        let run = store
            .get_workflow_run(resume_run_id)
            .map_err(internal_error)?
            .ok_or_else(|| internal_error("managed verification run disappeared".to_string()))?;
        let node = run
            .get("nodes")
            .and_then(serde_json::Value::as_array)
            .and_then(|nodes| nodes.first())
            .ok_or_else(|| internal_error("managed verification node disappeared".to_string()))?;
        let stored_binding = node
            .get("managed_supervised_patch")
            .and_then(serde_json::Value::as_object);
        let stored_binding_sha256 = crate::tool_policy_executor::managed_tool_binding_sha256(
            &workspace_id,
            operation,
            attempt,
            node,
        )
        .map_err(internal_error)?;
        if node.get("node_id").and_then(serde_json::Value::as_str) != Some(node_id.as_str())
            || stored_binding
                .and_then(|value| value.get("schema_version"))
                .and_then(serde_json::Value::as_str)
                != Some("managed_supervised_patch.v1")
            || stored_binding
                .and_then(|value| value.get("workspace_id"))
                .and_then(serde_json::Value::as_str)
                != Some(workspace_id.as_str())
            || stored_binding
                .and_then(|value| value.get("operation"))
                .and_then(serde_json::Value::as_str)
                != Some(operation)
            || stored_binding
                .and_then(|value| value.get("attempt"))
                .and_then(serde_json::Value::as_u64)
                != Some(attempt)
            || stored_binding
                .and_then(|value| value.get("binding_sha256"))
                .and_then(serde_json::Value::as_str)
                != Some(binding_sha256.as_str())
            || stored_binding_sha256 != binding_sha256
        {
            return Err(ApiError::with_code(
                StatusCode::CONFLICT,
                "verification_resume_binding_changed",
                "managed verification input no longer matches its approved binding",
            ));
        }
        if matches!(
            node.get("status").and_then(serde_json::Value::as_str),
            Some("completed" | "failed" | "awaiting_approval")
        ) {
            let output = node
                .get("result")
                .map(node_output_from_value)
                .transpose()?
                .ok_or_else(|| {
                    internal_error("managed verification result is missing".to_string())
                })?;
            return Ok(ManagedToolExecution {
                output,
                run_id: resume_run_id.to_string(),
                node_id,
            });
        }
        resume_run_id.to_string()
    } else {
        store
            .ensure_managed_supervised_patch_run(
                &workspace_id,
                operation,
                attempt,
                &binding_sha256,
                &node_metadata,
                actor,
            )
            .map_err(managed_run_error)?
    };

    if let Some(execution) = persisted_managed_tool_execution(&store, &run_id, &node_id)? {
        return Ok(execution);
    }

    let actor = actor.to_string();
    let tick_store = store.clone();
    let tick_run_id = run_id.clone();
    let tick = tokio::task::spawn_blocking(move || {
        tick_store.tick_managed_supervised_patch_with_executor(&tick_run_id, &actor, &executor)
    })
    .await
    .map_err(|error| internal_error(error.to_string()))?
    .map_err(internal_error)?;
    if tick.get("node_id").and_then(serde_json::Value::as_str) != Some(node_id.as_str()) {
        if let Some(execution) = persisted_managed_tool_execution(&store, &run_id, &node_id)? {
            return Ok(execution);
        }
        return Err(ApiError::with_code(
            StatusCode::CONFLICT,
            "verification_in_progress",
            "the canonical managed verification is pending or executing",
        ));
    }
    let output = tick
        .get("result")
        .map(node_output_from_value)
        .transpose()?
        .ok_or_else(|| internal_error("managed verification tick result is missing".to_string()))?;
    let run_id = tick
        .get("run")
        .and_then(|run| run.get("run_id"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| internal_error("managed verification tick run id is missing".to_string()))?
        .to_string();
    Ok(ManagedToolExecution {
        output,
        run_id,
        node_id,
    })
}

fn managed_run_error(error: String) -> ApiError {
    if error.contains("binding changed") {
        ApiError::with_code(
            StatusCode::CONFLICT,
            "verification_binding_changed",
            "the canonical managed verification is bound to different inputs",
        )
    } else {
        internal_error(error)
    }
}

fn persisted_managed_tool_execution(
    store: &LocalProductStore,
    run_id: &str,
    node_id: &str,
) -> Result<Option<ManagedToolExecution>, ApiError> {
    let run = store
        .get_workflow_run(run_id)
        .map_err(internal_error)?
        .ok_or_else(|| internal_error("managed verification run disappeared".to_string()))?;
    let node = run
        .get("nodes")
        .and_then(serde_json::Value::as_array)
        .and_then(|nodes| nodes.first())
        .filter(|node| node.get("node_id").and_then(serde_json::Value::as_str) == Some(node_id))
        .ok_or_else(|| internal_error("managed verification node disappeared".to_string()))?;
    match node.get("status").and_then(serde_json::Value::as_str) {
        Some("completed" | "failed" | "awaiting_approval") => {
            let output = node
                .get("result")
                .map(node_output_from_value)
                .transpose()?
                .ok_or_else(|| {
                    internal_error("managed verification result is missing".to_string())
                })?;
            Ok(Some(ManagedToolExecution {
                output,
                run_id: run_id.to_string(),
                node_id: node_id.to_string(),
            }))
        }
        Some("running") => Err(ApiError::with_code(
            StatusCode::CONFLICT,
            "verification_in_progress",
            "the canonical managed verification is already executing",
        )),
        Some("pending") => Ok(None),
        Some(status) => Err(internal_error(format!(
            "managed verification has unsupported persisted status: {status}"
        ))),
        None => Err(internal_error(
            "managed verification node status is missing".to_string(),
        )),
    }
}

fn validate_managed_scheduler_lease(
    state: &AxumApiState,
    max_execution_timeout_ms: u64,
) -> Result<(), ApiError> {
    let Some(scheduler) = state.scheduler.as_ref() else {
        return Ok(());
    };
    let status = scheduler
        .lock()
        .map_err(|_| internal_error("scheduler state lock is poisoned".to_string()))?
        .status();
    let config = status
        .get("config")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| internal_error("scheduler status is missing config".to_string()))?;
    let lease_timeout_ms = config
        .get("lease_timeout_ms")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            internal_error("scheduler status is missing lease_timeout_ms".to_string())
        })?;
    let interval_ms = config
        .get("interval_ms")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| internal_error("scheduler status is missing interval_ms".to_string()))?;
    let required_lease_ms = max_execution_timeout_ms.saturating_add(interval_ms.max(1_000));
    if lease_timeout_ms < required_lease_ms {
        return Err(ApiError::with_code(
            StatusCode::CONFLICT,
            "unsafe_managed_execution_lease",
            format!(
                "scheduler lease timeout {lease_timeout_ms}ms must be at least {required_lease_ms}ms for the bounded managed execution timeout"
            ),
        ));
    }
    Ok(())
}

fn node_output_from_value(
    value: &serde_json::Value,
) -> Result<crate::node_executor::NodeExecutionOutput, ApiError> {
    let required = |field: &str| {
        value
            .get(field)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| internal_error(format!("managed execution result missing {field}")))
    };
    Ok(crate::node_executor::NodeExecutionOutput {
        status: required("status")?,
        executor_type: required("executor_type")?,
        output: value
            .get("output")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        error_domain: value
            .get("error_domain")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        error_message: value
            .get("error_message")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        input_tokens: value
            .get("input_tokens")
            .and_then(serde_json::Value::as_i64),
        output_tokens: value
            .get("output_tokens")
            .and_then(serde_json::Value::as_i64),
        estimated_cost: value
            .get("estimated_cost")
            .and_then(serde_json::Value::as_f64),
        latency_ms: value.get("latency_ms").and_then(serde_json::Value::as_i64),
        process_outcome: value
            .get("process_outcome")
            .cloned()
            .filter(|outcome| !outcome.is_null())
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| internal_error(format!("invalid process_outcome: {error}")))?,
    })
}

fn verification_attempt_value(attempt: u64, execution: &ManagedToolExecution) -> serde_json::Value {
    let output = &execution.output;
    json!({
        "attempt": attempt,
        "status": output.status,
        "executor_type": output.executor_type,
        "output": output.output.as_deref().map(redact_sensitive_patterns),
        "error_domain": output.error_domain,
        "error_message": output.error_message.as_deref().map(redact_sensitive_patterns),
        "latency_ms": output.latency_ms,
        "process_outcome": output.process_outcome,
        "managed_run_id": execution.run_id,
        "managed_node_id": execution.node_id,
    })
}

fn upsert_verification_attempt(
    attempts: &mut Vec<serde_json::Value>,
    attempt: u64,
    execution: &ManagedToolExecution,
) {
    let value = verification_attempt_value(attempt, execution);
    if let Some(existing) = attempts.iter_mut().find(|candidate| {
        candidate
            .get("managed_run_id")
            .and_then(serde_json::Value::as_str)
            == Some(execution.run_id.as_str())
    }) {
        *existing = value;
    } else {
        attempts.push(value);
    }
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
