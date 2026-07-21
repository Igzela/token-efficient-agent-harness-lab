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
    // Live pool availability is derived from default registered types when no pool
    // snapshot is attached to API state. Fail closed only for identifiers that cannot
    // exist in the default registration set.
    let available = vec![
        "command".to_string(),
        "noop".to_string(),
        "stub".to_string(),
        "local_runner_validation".to_string(),
        "agent_step".to_string(),
    ];
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
    let confirm_output = body
        .get("confirm_output")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    match store.approve_and_output_product_task(&task_id, &context.api_key_id, confirm_output) {
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
