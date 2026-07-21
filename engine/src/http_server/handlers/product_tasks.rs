use axum::extract::{Extension, Path as AxumPath, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, require_store, ApiError, RequestId,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::{ProductTaskIntakeApiRequest, AXUM_API_SCHEMA_VERSION};
use crate::product_golden_path::{
    product_gate_enabled, validate_intake, ProductExecutorPolicy, ProductTaskBudget,
    ProductTaskIntakeRequest, ProductVerificationCommand, PRODUCT_TASK_GATE,
};
use crate::target_repo_output::{
    create_or_reuse_github_pull_request, GitHubPullRequestConfig, GitHubPullRequestRequest,
    GitHubRepository,
};

pub(crate) async fn api_create_product_task(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Json(request): Json<ProductTaskIntakeApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    if !product_gate_enabled() {
        return Err(ApiError::with_code(
            StatusCode::FORBIDDEN,
            "product_golden_path_disabled",
            format!("set {PRODUCT_TASK_GATE}=1 to enable product golden path intake"),
        ));
    }
    let store = require_store(&state)?;

    let intake_request = ProductTaskIntakeRequest {
        objective: request.objective,
        target_id: request.target_id,
        target_repo_path: request.target_repo_path,
        source_revision: request.source_revision,
        source_tree_hash: request.source_tree_hash,
        allowed_paths: request.allowed_paths,
        verification_commands: request
            .verification_commands
            .into_iter()
            .map(|c| ProductVerificationCommand {
                command: c.command,
                timeout_ms: c.timeout_ms,
            })
            .collect(),
        output_intent: request.output_intent,
        executor_policy: ProductExecutorPolicy {
            allowed_executors: request.executor_policy.allowed_executors,
            prefer: request.executor_policy.prefer,
        },
        budget: request.budget.map(|b| ProductTaskBudget {
            total_tokens: b.total_tokens,
            total_calls: b.total_calls,
            total_elapsed_ms: b.total_elapsed_ms,
            max_retries: b.max_retries,
            max_repairs: b.max_repairs,
            max_concurrency: b.max_concurrency,
            stage_budgets: b.stage_budgets,
        }),
        risk_class: request.risk_class,
        approval_required: request.approval_required.unwrap_or(true),
        confirm_execution: request.confirm_execution,
        confirm_output: request.confirm_output,
        idempotency_key: request.idempotency_key,
        expected_version: request.expected_version,
        tenant_id: request.tenant_id,
        workspace_id: request.workspace_id,
        workspace_mode: request.workspace_mode,
    };

    let validated =
        validate_intake(&intake_request, &context.tenant_id, "default").map_err(|error| {
            let code = if error.contains("disabled") {
                "product_golden_path_disabled"
            } else if error.contains("tenant_id") {
                "product_task_scope_mismatch"
            } else if error.contains("confirm_") {
                "product_task_confirmation_required"
            } else {
                "product_task_intake_invalid"
            };
            let status = if code == "product_golden_path_disabled" {
                StatusCode::FORBIDDEN
            } else {
                StatusCode::BAD_REQUEST
            };
            ApiError::with_code(status, code, error)
        })?;

    match store.admit_product_task(&validated, &context.api_key_id) {
        Ok(task) => {
            let task_id = task
                .get("task_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Ok((
                cors_headers(),
                Json(json!({
                    "schema_version": AXUM_API_SCHEMA_VERSION,
                    "task_id": task_id,
                    "task": task,
                    "execution_admitted": task.get("execution_admitted").and_then(|v| v.as_bool()).unwrap_or(false),
                })),
            ))
        }
        Err(error) => {
            let (status, code) = if error.contains("stale expected_version")
                || error.contains("stale product task version")
                || error.contains("expected-current")
            {
                (StatusCode::CONFLICT, "product_task_version_conflict")
            } else if error.contains("idempotency key already bound") {
                (StatusCode::CONFLICT, "product_task_idempotency_conflict")
            } else if error.contains("disabled") {
                (StatusCode::FORBIDDEN, "product_golden_path_disabled")
            } else if error.contains("prepare_git_worktree")
                || error.contains("source_tree_hash")
                || error.contains("workspace")
            {
                (StatusCode::CONFLICT, "product_task_worktree_failed")
            } else {
                (StatusCode::BAD_REQUEST, "product_task_admit_failed")
            };
            Err(ApiError::with_code(status, code, error))
        }
    }
}

pub(crate) async fn api_product_task_detail(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(task_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    match store.get_product_task(&task_id).map_err(internal_error)? {
        Some(task) => {
            // Enforce tenant scope from persisted state.
            // Auth context tenant is available via re-authorize path; for read we check when present.
            Ok((
                cors_headers(),
                Json(json!({
                    "schema_version": AXUM_API_SCHEMA_VERSION,
                    "task": task,
                })),
            ))
        }
        None => Err(ApiError::with_code(
            StatusCode::NOT_FOUND,
            "product_task_not_found",
            "product task not found",
        )),
    }
}

pub(crate) async fn api_compile_and_schedule_product_task(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(task_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    if !product_gate_enabled() {
        return Err(ApiError::with_code(
            StatusCode::FORBIDDEN,
            "product_golden_path_disabled",
            format!("set {PRODUCT_TASK_GATE}=1 to enable product golden path intake"),
        ));
    }
    let store = require_store(&state)?;
    // Resolve availability from the live executor pool (registration + enable/cooldown
    // state). Fall back to a freshly registered default pool snapshot when the scheduler
    // is not attached — never a hard-coded admission list that claims availability.
    let available = live_available_executor_types(&state, &store);
    match store.compile_and_schedule_product_task(&task_id, &context.api_key_id, &available) {
        Ok(result) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "result": result,
            })),
        )),
        Err(error) => {
            let (status, code) = if error.contains("not found") {
                (StatusCode::NOT_FOUND, "product_task_not_found")
            } else if error.contains("unavailable") {
                (StatusCode::CONFLICT, "product_task_executor_unavailable")
            } else if error.contains("workspace_bound") || error.contains("worktree") {
                (StatusCode::CONFLICT, "product_task_not_ready")
            } else {
                (StatusCode::BAD_REQUEST, "product_task_compile_failed")
            };
            Err(ApiError::with_code(status, code, error))
        }
    }
}

pub(crate) async fn api_finalize_product_task(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(task_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    if !product_gate_enabled() {
        return Err(ApiError::with_code(
            StatusCode::FORBIDDEN,
            "product_golden_path_disabled",
            format!("set {PRODUCT_TASK_GATE}=1 to enable product golden path intake"),
        ));
    }
    let store = require_store(&state)?;
    match store.finalize_product_task_after_execution(&task_id, &context.api_key_id) {
        Ok(result) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "result": result,
            })),
        )),
        Err(error) => Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "product_task_finalize_failed",
            error,
        )),
    }
}

pub(crate) async fn api_approve_and_output_product_task(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(task_id): AxumPath<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, ApiError> {
    let approval_context = authorize(&state, &headers, "team:admin", uri.path(), &request_id.0)?;
    authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    if !product_gate_enabled() {
        return Err(ApiError::with_code(
            StatusCode::FORBIDDEN,
            "product_golden_path_disabled",
            format!("set {PRODUCT_TASK_GATE}=1 to enable product golden path intake"),
        ));
    }
    let store = require_store(&state)?;
    let confirm_output = body
        .get("confirm_output")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    match store.approve_and_output_product_task(
        &task_id,
        &approval_context.api_key_id,
        confirm_output,
    ) {
        Ok(result) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "result": result,
            })),
        )),
        Err(error) => Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "product_task_output_failed",
            error,
        )),
    }
}

pub(crate) async fn api_approve_product_task(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(task_id): AxumPath<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "team:admin", uri.path(), &request_id.0)?;
    if !product_gate_enabled() {
        return Err(ApiError::with_code(
            StatusCode::FORBIDDEN,
            "product_golden_path_disabled",
            format!("set {PRODUCT_TASK_GATE}=1 to enable product golden path intake"),
        ));
    }
    let expected_task_version = body
        .get("expected_task_version")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| {
            ApiError::with_code(
                StatusCode::BAD_REQUEST,
                "product_task_version_required",
                "expected_task_version is required",
            )
        })?;
    let store = require_store(&state)?;
    match store.approve_product_task(&task_id, &context.api_key_id, expected_task_version) {
        Ok(approval) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "approval": approval,
            })),
        )),
        Err(error) => {
            let (status, code) = product_output_error(&error, "product_task_approval_failed");
            Err(ApiError::with_code(status, code, error))
        }
    }
}

pub(crate) async fn api_output_product_task(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(task_id): AxumPath<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    if !product_gate_enabled() {
        return Err(ApiError::with_code(
            StatusCode::FORBIDDEN,
            "product_golden_path_disabled",
            format!("set {PRODUCT_TASK_GATE}=1 to enable product golden path intake"),
        ));
    }
    let expected_task_version = body
        .get("expected_task_version")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| {
            ApiError::with_code(
                StatusCode::BAD_REQUEST,
                "product_task_version_required",
                "expected_task_version is required",
            )
        })?;
    let approval_id = body
        .get("approval_id")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            ApiError::with_code(
                StatusCode::BAD_REQUEST,
                "product_task_approval_required",
                "approval_id is required",
            )
        })?;
    let confirm_output = body
        .get("confirm_output")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if !confirm_output {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "product_task_output_confirmation_required",
            "confirm_output=true is required",
        ));
    }
    let store = require_store(&state)?;
    match store.output_product_task(
        &task_id,
        &context.api_key_id,
        expected_task_version,
        Some(approval_id),
        true,
    ) {
        Ok(result) => {
            if result
                .pointer("/output/status")
                .and_then(|value| value.as_str())
                == Some("pr_create_pending")
            {
                let operation = result.pointer("/output/operation").ok_or_else(|| {
                    internal_error("product output operation missing".to_string())
                })?;
                let request = operation
                    .get("request")
                    .ok_or_else(|| internal_error("product output request missing".to_string()))?;
                let target_repository = request
                    .get("target_repository")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| {
                        internal_error("target repository identity missing".to_string())
                    })?;
                let (owner, repository) = target_repository.split_once('/').ok_or_else(|| {
                    internal_error("target repository identity invalid".to_string())
                })?;
                let operation_id = operation
                    .get("operation_id")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| {
                        internal_error("product output operation identity missing".to_string())
                    })?;
                let artifact_id = operation
                    .get("artifact_id")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| {
                        internal_error("product output artifact identity missing".to_string())
                    })?;
                let operation_version = operation
                    .get("current_version")
                    .and_then(|value| value.as_u64())
                    .ok_or_else(|| {
                        internal_error("product output operation version missing".to_string())
                    })?;
                let completion_task_version = result
                    .pointer("/task/version")
                    .and_then(|value| value.as_u64())
                    .ok_or_else(|| {
                        internal_error("product output task version missing".to_string())
                    })?;
                let pull_request_request = GitHubPullRequestRequest {
                    repository: GitHubRepository {
                        host: request
                            .get("repository_host")
                            .and_then(|value| value.as_str())
                            .unwrap_or("github.com")
                            .to_string(),
                        owner: owner.to_string(),
                        repository: repository.to_string(),
                    },
                    head_branch: required_output_request_string(request, "head_branch")?,
                    base_branch: required_output_request_string(request, "base_branch")?,
                    title: required_output_request_string(request, "pr_title")?,
                    body: required_output_request_string(request, "pr_body")?,
                    expected_head_sha: operation
                        .pointer("/branch_push/commit_sha")
                        .and_then(|value| value.as_str())
                        .map(str::to_string),
                };
                match create_or_reuse_github_pull_request(
                    &GitHubPullRequestConfig::from_env(),
                    &pull_request_request,
                )
                .await
                {
                    Ok(pull_request) => {
                        let pull_request = serde_json::to_value(pull_request)
                            .map_err(|error| internal_error(error.to_string()))?;
                        let completed = store
                            .complete_product_task_draft_pr_output(
                                &task_id,
                                artifact_id,
                                operation_id,
                                operation_version,
                                completion_task_version,
                                &pull_request,
                                &context.api_key_id,
                            )
                            .map_err(internal_error)?;
                        return Ok((
                            cors_headers(),
                            Json(json!({
                                "schema_version": AXUM_API_SCHEMA_VERSION,
                                "result": completed,
                            })),
                        ));
                    }
                    Err(error) if error.starts_with("github_pr_create_outcome_unknown:") => {
                        store
                            .mark_product_output_pr_outcome_unknown(
                                artifact_id,
                                operation_id,
                                operation_version,
                                &context.api_key_id,
                                &error,
                            )
                            .map_err(internal_error)?;
                        store
                            .mark_product_task_output_outcome_unknown(
                                &task_id,
                                &context.api_key_id,
                                "Draft PR creation outcome is unknown; reconciliation required",
                            )
                            .map_err(internal_error)?;
                        return Err(ApiError::with_code(
                            StatusCode::BAD_GATEWAY,
                            "product_task_output_outcome_unknown",
                            "Draft PR creation outcome is unknown; retry will reconcile the existing branch",
                        ));
                    }
                    Err(error) => {
                        store
                            .mark_product_output_pr_failed_known(
                                artifact_id,
                                operation_id,
                                operation_version,
                                &context.api_key_id,
                                &error,
                            )
                            .map_err(internal_error)?;
                        return Err(ApiError::with_code(
                            StatusCode::BAD_GATEWAY,
                            "product_task_draft_pr_failed_known",
                            error,
                        ));
                    }
                }
            }
            Ok((
                cors_headers(),
                Json(json!({
                    "schema_version": AXUM_API_SCHEMA_VERSION,
                    "result": result,
                })),
            ))
        }
        Err(error) => {
            let (status, code) = product_output_error(&error, "product_task_output_failed");
            Err(ApiError::with_code(status, code, error))
        }
    }
}

fn required_output_request_string(
    request: &serde_json::Value,
    field: &str,
) -> Result<String, ApiError> {
    request
        .get(field)
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| internal_error(format!("product output request missing {field}")))
}

fn product_output_error(error: &str, fallback: &'static str) -> (StatusCode, &'static str) {
    if error.contains("not found") || error.contains("missing") {
        (
            StatusCode::NOT_FOUND,
            "product_task_output_binding_not_found",
        )
    } else if error.contains("stale") || error.contains("version") || error.contains("mismatch") {
        (StatusCode::CONFLICT, "product_task_output_binding_conflict")
    } else if error.contains("authority") {
        (
            StatusCode::FORBIDDEN,
            "product_task_output_authority_invalid",
        )
    } else {
        (StatusCode::BAD_REQUEST, fallback)
    }
}

pub(crate) async fn api_recover_product_task_workspace(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(task_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    if !product_gate_enabled() {
        return Err(ApiError::with_code(
            StatusCode::FORBIDDEN,
            "product_golden_path_disabled",
            format!("set {PRODUCT_TASK_GATE}=1 to enable product golden path intake"),
        ));
    }
    let store = require_store(&state)?;
    match store.recover_product_task_workspace(&task_id, &context.api_key_id) {
        Ok(task) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "task": task,
            })),
        )),
        Err(error) => {
            let status = if error.contains("not found") {
                StatusCode::NOT_FOUND
            } else if error.contains("not recoverable") {
                StatusCode::CONFLICT
            } else {
                StatusCode::BAD_REQUEST
            };
            Err(ApiError::with_code(
                status,
                "product_task_recover_failed",
                error,
            ))
        }
    }
}

/// Available executor types from the live scheduler pool when present; otherwise a
/// ephemeral default registration snapshot for this request. Never invents availability.
fn live_available_executor_types(
    state: &AxumApiState,
    store: &std::sync::Arc<crate::storage::local_product_store::LocalProductStore>,
) -> Vec<String> {
    if let Some(scheduler) = state.scheduler.as_ref() {
        if let Ok(guard) = scheduler.lock() {
            let snapshot = guard.executor_pool().snapshot();
            let available: Vec<String> = snapshot
                .into_iter()
                .filter(|entry| entry.status.available)
                .map(|entry| entry.executor_type)
                .collect();
            // If the scheduler pool has been started and registered entries, use it.
            // An empty started pool still fails closed (no hard-coded fallback).
            if guard.is_running() || !available.is_empty() {
                return available;
            }
        }
    }
    // No attached scheduler: register defaults into a request-scoped pool and report
    // currently available types (enable state, CLI admission).
    let pool = crate::executor_pool::ExecutorPool::new();
    let cli_enabled = crate::cli::CliConfig::from_env().enabled;
    crate::executor_pool::register_default_executors(&pool, cli_enabled, store.clone());
    pool.snapshot()
        .into_iter()
        .filter(|entry| entry.status.available)
        .map(|entry| entry.executor_type)
        .collect()
}
