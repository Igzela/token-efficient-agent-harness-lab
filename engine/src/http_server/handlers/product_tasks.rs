use axum::extract::{Extension, Path as AxumPath, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::http_server::middleware::{
    authorize, authorize_any, cors_headers, internal_error, require_store, ApiError, RequestId,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::{ProductTaskIntakeApiRequest, AXUM_API_SCHEMA_VERSION};
use crate::infrastructure::auth::LOCAL_BOOTSTRAP_API_KEY_ID;
use crate::product_golden_path::{
    product_gate_enabled, product_scheduler_kill_active, validate_intake, ProductExecutorPolicy,
    ProductTaskBudget, ProductTaskIntakeRequest, ProductVerificationCommand,
    ProductVerificationRuntimeAuthority, PRODUCT_TASK_GATE,
};
use crate::storage::local_product_store::product_tasks::{
    public_product_task_projection, public_product_task_result_projection,
};
use crate::storage::local_product_store::{
    SCOPE_DELEGATED_ARTIFACT_CONFIRM, SCOPE_DELEGATED_AUTONOMY, SCOPE_DELEGATED_MANIFEST_APPROVE,
    SCOPE_IDENTITY_DELEGATE, SCOPE_SPEND_AUTHORIZE,
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
        source_kind: request.source_kind,
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
        matrix_binding: None,
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
                    "task": public_product_task_projection(&task),
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
    let context = authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    match store
        .get_product_task_for_tenant(&task_id, &context.tenant_id)
        .map_err(|error| {
            if error.contains("tenant") {
                ApiError::with_code(StatusCode::FORBIDDEN, "product_task_scope_mismatch", error)
            } else {
                internal_error(error)
            }
        })? {
        Some(task) => {
            // Enforce tenant scope from persisted state.
            // Auth context tenant is available via re-authorize path; for read we check when present.
            Ok((
                cors_headers(),
                Json(json!({
                    "schema_version": AXUM_API_SCHEMA_VERSION,
                    "task": public_product_task_projection(&task),
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
    match store
        .get_product_task_for_tenant(&task_id, &context.tenant_id)
        .map_err(|error| {
            if error.contains("tenant") {
                ApiError::with_code(StatusCode::FORBIDDEN, "product_task_scope_mismatch", error)
            } else {
                internal_error(error)
            }
        })? {
        Some(_) => {}
        None => {
            return Err(ApiError::with_code(
                StatusCode::NOT_FOUND,
                "product_task_not_found",
                "product task not found",
            ));
        }
    }
    // Automatic admission requires the actual attached/running scheduler and its live
    // executor pool. A request-scoped registration snapshot cannot consume the run.
    let available = live_available_executor_types(&state).map_err(|error| {
        ApiError::with_code(
            StatusCode::CONFLICT,
            "product_task_scheduler_unavailable",
            error,
        )
    })?;
    match store.compile_and_schedule_product_task_for_tenant(
        &context.tenant_id,
        &task_id,
        &context.api_key_id,
        &available,
    ) {
        Ok(result) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "result": public_product_task_result_projection(&result),
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
    let scheduler_for_samples = state.scheduler.clone();
    let scheduler_for_commit = state.scheduler.clone();
    let authority = move || product_verification_runtime_authority(scheduler_for_samples.as_ref());
    let commit_authority = move |operation: &mut dyn FnMut() -> Result<
        (serde_json::Value, serde_json::Value),
        String,
    >| {
        product_verification_commit_authority(scheduler_for_commit.as_ref(), operation)
    };
    match store.finalize_product_task_after_execution_with_commit_authority_for_tenant(
        &context.tenant_id,
        &task_id,
        &context.api_key_id,
        &authority,
        &commit_authority,
    ) {
        Ok(result) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "result": public_product_task_result_projection(&result),
            })),
        )),
        Err(error) if error.contains("tenant") => Err(ApiError::with_code(
            StatusCode::FORBIDDEN,
            "product_task_scope_mismatch",
            error,
        )),
        Err(error) => Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "product_task_finalize_failed",
            error,
        )),
    }
}

pub(crate) async fn api_prepare_delegated_product_task(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(task_id): AxumPath<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, ApiError> {
    // Manifest/spend approval is a managed reviewer capability. The canonical
    // bootstrap key only owns identity delegation and deliberately does not
    // carry managed-operation scopes.
    let context = authorize(
        &state,
        &headers,
        SCOPE_DELEGATED_MANIFEST_APPROVE,
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
    let delegation: crate::storage::local_product_store::DelegationContract =
        serde_json::from_value(
            body.get("delegation")
                .cloned()
                .ok_or_else(|| delegated_request_error("delegation is required"))?,
        )
        .map_err(|_| delegated_request_error("delegation is malformed"))?;
    let proposal = body
        .get("proposal_manifest")
        .filter(|value| value.is_object())
        .ok_or_else(|| delegated_request_error("proposal_manifest is required"))?;
    let approved_proposal_sha256 = body
        .get("approved_proposal_sha256")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| delegated_request_error("approved_proposal_sha256 is required"))?;
    let attempt_id = body
        .get("attempt_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| delegated_request_error("attempt_id is required"))?;
    let principal = store
        .authenticate_managed_acceptance_principal(
            &context.tenant_id,
            &context.api_key_id,
            None, // wall-clock expiry; never pass cost caps as now_unix
        )
        .map_err(|error| delegated_api_error(&error, "delegated_operator_authentication_failed"))?;
    let available = live_available_executor_types(&state).map_err(|error| {
        ApiError::with_code(
            StatusCode::CONFLICT,
            "delegated_scheduler_unavailable",
            error,
        )
    })?;
    let persisted = store
        .persist_delegation_for_product_task(&principal, &task_id, &delegation)
        .map_err(|error| delegated_api_error(&error, "delegation_persist_failed"))?;
    let task_binding = json!({
        "delegation_id": delegation.delegation_id,
        "product_task_id": persisted.get("product_task_id"),
        "replayed": persisted.get("replayed"),
    });
    let proposal_receipt = store
        .persist_approved_delegated_proposal(
            &delegation.delegation_id,
            proposal,
            approved_proposal_sha256,
        )
        .map_err(|error| delegated_api_error(&error, "delegated_proposal_persist_failed"))?;
    let prepared = store
        .prepare_delegated_managed_product_task(
            &task_id,
            &context.api_key_id,
            &available,
            proposal,
            &delegation,
            attempt_id,
        )
        .map_err(|error| delegated_api_error(&error, "delegated_product_prepare_failed"))?;
    let manifest = prepared
        .get("final_manifest")
        .cloned()
        .ok_or_else(|| internal_error("delegated final manifest missing".to_string()))?;
    let approval = store
        .approve_delegated_manifest(&principal, &delegation.delegation_id, &manifest)
        .map_err(|error| delegated_api_error(&error, "delegated_manifest_approval_failed"))?;
    let spend = store
        .issue_delegated_spend(
            &principal,
            &delegation.delegation_id,
            approval
                .get("approval_receipt_sha256")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| internal_error("delegated approval hash missing".to_string()))?,
            &manifest,
        )
        .map_err(|error| delegated_api_error(&error, "delegated_spend_issue_failed"))?;
    Ok((
        cors_headers(),
        Json(delegated_prepare_response(
            &persisted,
            &task_binding,
            &proposal_receipt,
            &manifest,
            &approval,
            &spend,
            &prepared,
        )),
    ))
}

fn delegated_prepare_response(
    persisted: &serde_json::Value,
    task_binding: &serde_json::Value,
    proposal_receipt: &serde_json::Value,
    manifest: &serde_json::Value,
    approval: &serde_json::Value,
    spend: &serde_json::Value,
    prepared: &serde_json::Value,
) -> serde_json::Value {
    json!({
        "schema_version": AXUM_API_SCHEMA_VERSION,
        "delegation_sha256": persisted.get("delegation_sha256"),
        "delegation_product_task_binding": task_binding,
        "approved_proposal_sha256": proposal_receipt.get("proposal_manifest_sha256"),
        "final_manifest": manifest,
        "manifest_approval_receipt_sha256": approval.get("approval_receipt_sha256"),
        "spend_authorization_id": spend.get("spend_authorization_id"),
        "execution_activated": false,
        "result": public_product_task_result_projection(prepared),
    })
}

pub(crate) async fn api_activate_delegated_product_task(
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
    let delegation_id = body
        .get("delegation_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| delegated_request_error("delegation_id is required"))?;
    let attempt_id = body
        .get("attempt_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| delegated_request_error("attempt_id is required"))?;
    let manifest = body
        .get("final_manifest")
        .filter(|value| value.is_object())
        .ok_or_else(|| delegated_request_error("final_manifest is required"))?;
    let spend_authorization_id = body
        .get("spend_authorization_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| delegated_request_error("spend_authorization_id is required"))?;
    let store = require_store(&state)?;
    let principal = store
        .authenticate_managed_acceptance_principal(
            &context.tenant_id,
            &context.api_key_id,
            None, // wall-clock expiry; never pass cost caps as now_unix
        )
        .map_err(|error| {
            delegated_api_error(&error, "delegated_activator_authentication_failed")
        })?;
    let lease = store
        .admit_delegated_attempt(&principal, delegation_id, attempt_id, manifest)
        .map_err(|error| delegated_api_error(&error, "delegated_attempt_admission_failed"))?;
    let activated = store
        .activate_delegated_managed_product_task(
            &task_id,
            &context.api_key_id,
            manifest,
            spend_authorization_id,
            lease
                .get("attempt_lease_id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| internal_error("delegated lease identity missing".to_string()))?,
        )
        .map_err(|error| delegated_api_error(&error, "delegated_activation_failed"))?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "attempt_lease_id": lease.get("attempt_lease_id"),
            "result": public_product_task_result_projection(&activated),
        })),
    ))
}

pub(crate) async fn api_approve_delegated_product_task(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(task_id): AxumPath<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, ApiError> {
    // Artifact confirmation is a separate managed reviewer capability. Store
    // authority additionally rejects the manifest approver and activator, so
    // callers must use a distinct authenticated reviewer key.
    let context = authorize(
        &state,
        &headers,
        SCOPE_DELEGATED_ARTIFACT_CONFIRM,
        uri.path(),
        &request_id.0,
    )?;
    let expected_task_version = body
        .get("expected_task_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| delegated_request_error("expected_task_version is required"))?;
    let delegation_id = body
        .get("delegation_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| delegated_request_error("delegation_id is required"))?;
    let manifest = body
        .get("final_manifest")
        .filter(|value| value.is_object())
        .ok_or_else(|| delegated_request_error("final_manifest is required"))?;
    let target_main_sha = body
        .get("current_target_main_sha")
        .and_then(serde_json::Value::as_str)
        .filter(|value| value.len() == 40 && value.chars().all(|ch| ch.is_ascii_hexdigit()))
        .ok_or_else(|| delegated_request_error("current_target_main_sha must be 40 hex"))?;
    let store = require_store(&state)?;
    let principal = store
        .authenticate_managed_acceptance_principal(
            &context.tenant_id,
            &context.api_key_id,
            None, // wall-clock expiry; never pass cost caps as now_unix
        )
        .map_err(|error| {
            delegated_api_error(&error, "delegated_confirmer_authentication_failed")
        })?;
    let result = store
        .approve_delegated_product_task(
            &principal,
            &task_id,
            &context.api_key_id,
            expected_task_version,
            delegation_id,
            manifest,
            target_main_sha,
        )
        .map_err(|error| delegated_api_error(&error, "delegated_artifact_approval_failed"))?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "result": public_product_task_result_projection(&result),
        })),
    ))
}

pub(crate) async fn api_terminal_delegated_product_task(
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
    let delegation_id = body
        .get("delegation_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| delegated_request_error("delegation_id is required"))?;
    let attempt_id = body
        .get("attempt_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| delegated_request_error("attempt_id is required"))?;
    let store = require_store(&state)?;
    let principal = store
        .authenticate_managed_acceptance_principal(&context.tenant_id, &context.api_key_id, None)
        .map_err(|error| delegated_api_error(&error, "delegated_terminal_authentication_failed"))?;
    let result = store
        .complete_delegated_product_task_terminal_for_principal(
            &principal,
            delegation_id,
            attempt_id,
            &task_id,
            &context.api_key_id,
        )
        .map_err(|error| delegated_api_error(&error, "delegated_terminal_failed"))?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "result": public_product_task_result_projection(&result),
        })),
    ))
}

pub(crate) async fn api_approve_product_task(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(task_id): AxumPath<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize_any(
        &state,
        &headers,
        &["team:admin", SCOPE_DELEGATED_MANIFEST_APPROVE],
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
    let store = require_store(&state)?;
    match store.approve_product_task_for_tenant(
        &context.tenant_id,
        &task_id,
        &context.api_key_id,
        expected_task_version,
    ) {
        Ok(approval) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "approval": public_product_task_result_projection(&approval),
            })),
        )),
        Err(error) => {
            if error.contains("tenant") {
                return Err(ApiError::with_code(
                    StatusCode::FORBIDDEN,
                    "product_task_scope_mismatch",
                    error,
                ));
            }
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
    match store.output_product_task_for_tenant(
        &context.tenant_id,
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
                // Prefer the current ProductTask version for terminal CAS. The
                // durable operation may still carry the earlier claim version
                // from the first progressive output phase.
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
                    expected_base_sha: operation
                        .get("source_revision")
                        .and_then(|value| value.as_str())
                        .map(str::to_string),
                    expected_head_sha: operation
                        .pointer("/branch_push/commit_sha")
                        .and_then(|value| value.as_str())
                        .map(str::to_string),
                };
                let reconciliation_only = operation
                    .get("claim_action")
                    .and_then(|value| value.as_str())
                    == Some("reconcile_pr_only");
                let pull_request_result = if reconciliation_only {
                    crate::target_repo_output::reconcile_existing_github_pull_request(
                        &GitHubPullRequestConfig::from_env(),
                        &pull_request_request,
                    )
                    .await
                } else {
                    create_or_reuse_github_pull_request(
                        &GitHubPullRequestConfig::from_env(),
                        &pull_request_request,
                    )
                    .await
                };
                match pull_request_result {
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
                                "result": public_product_task_result_projection(&completed),
                            })),
                        ));
                    }
                    Err(error) if error.starts_with("github_pr_create_outcome_unknown:") => {
                        store
                            .mark_product_output_pr_outcome_unknown(
                                artifact_id,
                                operation_id,
                                operation_version,
                                &task_id,
                                approval_id,
                                completion_task_version,
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
                                &task_id,
                                approval_id,
                                completion_task_version,
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
                    "result": public_product_task_result_projection(&result),
                })),
            ))
        }
        Err(error) => {
            if error.contains("tenant") {
                return Err(ApiError::with_code(
                    StatusCode::FORBIDDEN,
                    "product_task_scope_mismatch",
                    error,
                ));
            }
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

fn delegated_request_error(message: &str) -> ApiError {
    ApiError::with_code(
        StatusCode::BAD_REQUEST,
        "delegated_product_request_invalid",
        message,
    )
}

fn delegated_api_error(error: &str, fallback: &'static str) -> ApiError {
    let (status, code) = if error.contains("expired")
        || error.contains("revoked")
        || error.contains("authority")
        || error.contains("principal")
    {
        (StatusCode::FORBIDDEN, "delegated_authority_invalid")
    } else if error.contains("stale")
        || error.contains("mismatch")
        || error.contains("conflict")
        || error.contains("drift")
        || error.contains("replay")
    {
        (StatusCode::CONFLICT, "delegated_state_conflict")
    } else if error.contains("missing") || error.contains("not found") {
        (StatusCode::NOT_FOUND, "delegated_binding_not_found")
    } else {
        (StatusCode::BAD_REQUEST, fallback)
    };
    ApiError::with_code(status, code, error)
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
    match store.recover_product_task_workspace_for_tenant(
        &context.tenant_id,
        &task_id,
        &context.api_key_id,
    ) {
        Ok(task) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "task": public_product_task_projection(&task),
            })),
        )),
        Err(error) => {
            let status = if error.contains("tenant") {
                StatusCode::FORBIDDEN
            } else if error.contains("not found") {
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

pub(crate) async fn api_reconcile_unadmitted_delegated_product_task(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(task_id): AxumPath<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "team:admin", uri.path(), &request_id.0)?;
    if context.api_key_id != LOCAL_BOOTSTRAP_API_KEY_ID
        || !context
            .scopes
            .iter()
            .any(|scope| scope == SCOPE_IDENTITY_DELEGATE)
    {
        return Err(ApiError::with_code(
            StatusCode::FORBIDDEN,
            "bootstrap_identity_authority_required",
            "only the canonical bootstrap identity-delegation authority may rebind an unadmitted delegation",
        ));
    }
    let delegation_id = body
        .get("delegation_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| delegated_request_error("delegation_id is required"))?;
    let reviewer_key_id = body
        .get("reviewer_key_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| delegated_request_error("reviewer_key_id is required"))?;
    let store = require_store(&state)?;
    let reviewer = store
        .authenticate_managed_acceptance_principal_for_tenant(
            &context.tenant_id,
            reviewer_key_id,
            None,
        )
        .map_err(|error| delegated_api_error(&error, "delegated_reviewer_authentication_failed"))?;
    reviewer
        .require_scope(SCOPE_DELEGATED_AUTONOMY)
        .map_err(|error| delegated_api_error(&error, "delegated_reviewer_scope_denied"))?;
    reviewer
        .require_scope(SCOPE_DELEGATED_MANIFEST_APPROVE)
        .map_err(|error| delegated_api_error(&error, "delegated_reviewer_scope_denied"))?;
    reviewer
        .require_scope(SCOPE_SPEND_AUTHORIZE)
        .map_err(|error| delegated_api_error(&error, "delegated_reviewer_scope_denied"))?;
    let bootstrap = store
        .authenticate_bootstrap_identity_delegation_principal(&context.tenant_id, None)
        .map_err(|error| {
            ApiError::with_code(
                StatusCode::FORBIDDEN,
                "bootstrap_identity_authority_required",
                error,
            )
        })?;
    let result = store
        .rebind_unadmitted_delegation_for_bootstrap(&bootstrap, &task_id, delegation_id, &reviewer)
        .map_err(|error| {
            delegated_api_error(&error, "delegated_bootstrap_reconciliation_failed")
        })?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "result": result,
        })),
    ))
}

/// Executor types the attached scheduler can actually route to a worker right now.
/// Pool-routed modes expose admitted live entries; fixed modes expose only their exact
/// configured executor. Fixture/failure scheduler modes cannot admit automatic product work.
fn live_available_executor_types(state: &AxumApiState) -> Result<Vec<String>, String> {
    let scheduler = state
        .scheduler
        .as_ref()
        .ok_or_else(|| "automatic product execution requires an attached scheduler".to_string())?;
    let guard = scheduler
        .lock()
        .map_err(|_| "attached scheduler authority lock is unavailable".to_string())?;
    let status = guard.status();
    let control = guard.control_snapshot()?;
    if !control.running {
        return Err("automatic product execution requires a running scheduler".to_string());
    }
    if control.paused {
        return Err("automatic product execution scheduler is paused".to_string());
    }
    if control.kill_requested || product_scheduler_kill_active() {
        return Err("automatic product execution scheduler kill is active".to_string());
    }
    let registered_available = guard
        .executor_pool()
        .snapshot()
        .into_iter()
        .filter(|entry| entry.status.available)
        .map(|entry| entry.executor_type)
        .collect::<Vec<_>>();
    let configured_executor = status
        .pointer("/config/executor_type")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "attached scheduler executor mode is unavailable".to_string())?;
    let available = scheduler_routable_executor_types(configured_executor, registered_available);
    if available.is_empty() {
        return Err("attached scheduler has no admitted runnable executor".to_string());
    }
    Ok(available)
}

fn scheduler_routable_executor_types(
    configured_executor: &str,
    registered_available: Vec<String>,
) -> Vec<String> {
    if matches!(
        configured_executor,
        "dynamic" | "dynamic_noop" | "dynamic_workflow" | "auto" | "pool"
    ) {
        return registered_available;
    }
    if matches!(configured_executor, "noop" | "stub" | "fail") {
        return Vec::new();
    }
    registered_available
        .into_iter()
        .filter(|executor| executor == configured_executor)
        .collect()
}

fn product_verification_runtime_authority(
    scheduler: Option<&std::sync::Arc<std::sync::Mutex<crate::scheduler::WorkflowScheduler>>>,
) -> Result<ProductVerificationRuntimeAuthority, String> {
    let scheduler = scheduler.ok_or_else(|| {
        "automatic product verification requires an attached scheduler".to_string()
    })?;
    let guard = scheduler
        .try_lock()
        .map_err(|_| "attached scheduler authority lock is unavailable".to_string())?;
    let control = guard.control_snapshot()?;
    Ok(ProductVerificationRuntimeAuthority {
        scheduler_attached: true,
        scheduler_running: control.running,
        scheduler_paused: control.paused,
        scheduler_killed: control.kill_requested,
        global_kill_active: product_scheduler_kill_active(),
        manual_operational_tick: false,
    })
}

fn product_verification_commit_authority(
    scheduler: Option<&std::sync::Arc<std::sync::Mutex<crate::scheduler::WorkflowScheduler>>>,
    operation: &mut dyn FnMut() -> Result<(serde_json::Value, serde_json::Value), String>,
) -> Result<(serde_json::Value, serde_json::Value), String> {
    let scheduler = scheduler.ok_or_else(|| {
        "runtime_authority_unavailable:automatic product verification requires an attached scheduler"
            .to_string()
    })?;
    // Never wait while a caller may already own application-store locks. Once acquired,
    // keep both the scheduler owner and its worker-shared control gate through the database
    // commit so API and worker-observed pause/kill have one linearization order.
    let guard = scheduler.try_lock().map_err(|_| {
        "runtime_authority_unavailable:attached scheduler authority lock is unavailable".to_string()
    })?;
    guard
        .with_control_barrier(|control| {
            let authority = ProductVerificationRuntimeAuthority {
                scheduler_attached: true,
                scheduler_running: control.running,
                scheduler_paused: control.paused,
                scheduler_killed: control.kill_requested,
                global_kill_active: product_scheduler_kill_active(),
                manual_operational_tick: false,
            };
            authority
                .validate()
                .map_err(|reason| format!("runtime_authority_lost:{reason}"))?;
            operation()
        })
        .map_err(|error| {
            if error == "scheduler control gate is unavailable" {
                format!("runtime_authority_unavailable:{error}")
            } else {
                error
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::{SchedulerConfig, WorkflowScheduler};
    use crate::storage::local_product_store::LocalProductStore;
    use std::sync::{Arc, Mutex};

    #[test]
    fn delegated_prepare_response_never_admits_or_exposes_an_attempt_lease() {
        let response = delegated_prepare_response(
            &json!({"delegation_sha256": "d".repeat(64)}),
            &json!({"product_task_id": "ptask-1", "replayed": false}),
            &json!({"proposal_manifest_sha256": "p".repeat(64)}),
            &json!({"schema_version": "managed_final_execution_manifest.v1"}),
            &json!({"approval_receipt_sha256": "a".repeat(64)}),
            &json!({"spend_authorization_id": "spend-1"}),
            &json!({"task": {"status": "workspace_bound"}}),
        );
        assert_eq!(response["execution_activated"], false);
        assert!(response.get("attempt_lease_id").is_none());
        assert!(response.get("attempt_lease_token").is_none());
    }

    #[test]
    fn automatic_product_admission_requires_attached_running_scheduler() {
        let state = AxumApiState::new();
        assert!(live_available_executor_types(&state)
            .unwrap_err()
            .contains("attached scheduler"));

        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(
            LocalProductStore::new(directory.path().join("scheduler-admission.db")).unwrap(),
        );
        let scheduler = WorkflowScheduler::new(Arc::clone(&store), SchedulerConfig::default());
        crate::executor_pool::register_default_executors(scheduler.executor_pool(), false, store);
        assert!(!scheduler.executor_pool().snapshot().is_empty());
        let state = AxumApiState::new().with_scheduler(Arc::new(Mutex::new(scheduler)));
        assert!(live_available_executor_types(&state)
            .unwrap_err()
            .contains("running scheduler"));
    }

    #[test]
    fn automatic_product_verification_does_not_fall_back_to_manual_authority() {
        assert!(product_verification_runtime_authority(None)
            .unwrap_err()
            .contains("attached scheduler"));
    }

    #[test]
    fn automatic_product_verification_fails_closed_on_scheduler_lock_contention() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(
            LocalProductStore::new(directory.path().join("scheduler-authority.db")).unwrap(),
        );
        let scheduler = Arc::new(Mutex::new(WorkflowScheduler::new(
            store,
            SchedulerConfig::default(),
        )));
        let held = scheduler.lock().unwrap();
        assert!(product_verification_runtime_authority(Some(&scheduler))
            .unwrap_err()
            .contains("lock is unavailable"));
        drop(held);
    }

    #[test]
    fn scheduler_pause_blocks_product_admission_and_artifact_commit() {
        let directory = tempfile::tempdir().unwrap();
        let store =
            Arc::new(LocalProductStore::new(directory.path().join("scheduler-paused.db")).unwrap());
        let mut scheduler_value = WorkflowScheduler::new(
            store,
            SchedulerConfig {
                executor_type: "command".to_string(),
                supervised_workers_enabled: true,
                interval_ms: 10_000,
                ..SchedulerConfig::default()
            },
        );
        scheduler_value.pause("test").unwrap();
        scheduler_value.start().unwrap();
        let scheduler = Arc::new(Mutex::new(scheduler_value));
        assert_eq!(scheduler.lock().unwrap().status()["paused"], true);
        let state = AxumApiState::new().with_scheduler(Arc::clone(&scheduler));
        assert!(live_available_executor_types(&state)
            .unwrap_err()
            .contains("scheduler is paused"));
        assert_eq!(
            product_verification_runtime_authority(Some(&scheduler))
                .unwrap()
                .validate(),
            Err("scheduler_paused")
        );
        let invoked = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let invoked_in_operation = Arc::clone(&invoked);
        let mut operation = move || {
            invoked_in_operation.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok((json!({}), json!({})))
        };
        assert!(
            product_verification_commit_authority(Some(&scheduler), &mut operation)
                .unwrap_err()
                .contains("runtime_authority_lost:scheduler_paused")
        );
        assert!(!invoked.load(std::sync::atomic::Ordering::SeqCst));
        scheduler.lock().unwrap().stop().unwrap();
    }

    #[test]
    fn automatic_product_artifact_commit_serializes_scheduler_control() {
        let directory = tempfile::tempdir().unwrap();
        let store =
            Arc::new(LocalProductStore::new(directory.path().join("scheduler-commit.db")).unwrap());
        let store_in_operation = Arc::clone(&store);
        let mut scheduler_value = WorkflowScheduler::new(
            store,
            SchedulerConfig {
                supervised_workers_enabled: true,
                interval_ms: 10_000,
                ..SchedulerConfig::default()
            },
        );
        scheduler_value.start().unwrap();
        let scheduler = Arc::new(Mutex::new(scheduler_value));
        let observed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed_in_operation = Arc::clone(&observed);
        let scheduler_in_operation = Arc::clone(&scheduler);
        let mut operation = move || {
            // The commit body may acquire store locks while scheduler control is held;
            // the authority sampler must therefore remain storage-free.
            store_in_operation.audit_events(1)?;
            observed_in_operation.store(
                scheduler_in_operation.try_lock().is_err(),
                std::sync::atomic::Ordering::SeqCst,
            );
            Ok((json!({"artifact": true}), json!({"task": true})))
        };
        let result = product_verification_commit_authority(Some(&scheduler), &mut operation)
            .expect("commit authority");
        assert_eq!(result.0, json!({"artifact": true}));
        assert!(observed.load(std::sync::atomic::Ordering::SeqCst));
        scheduler.lock().unwrap().stop().unwrap();
    }

    #[test]
    fn fixed_scheduler_modes_only_admit_the_executor_the_worker_consumes() {
        let registered = vec![
            "command".to_string(),
            "codex_cli".to_string(),
            "noop".to_string(),
        ];
        assert!(scheduler_routable_executor_types("noop", registered.clone()).is_empty());
        assert_eq!(
            scheduler_routable_executor_types("codex_cli", registered.clone()),
            vec!["codex_cli"]
        );
        assert_eq!(
            scheduler_routable_executor_types("pool", registered),
            vec!["command", "codex_cli", "noop"]
        );
    }
}
