use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::Json;
use chrono::DateTime;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::http_server::middleware::{authorize, cors_headers, require_store, ApiError, RequestId};
use crate::http_server::state::AxumApiState;
use crate::operator_decision::{
    OperatorDecisionAction, OperatorDecisionEvidenceReference, OperatorDecisionItem,
    OperatorDecisionOutcome,
};
use crate::storage::local_product_store::{is_execution_owner_conflict, BudgetAutoPausePolicy};

const MAX_MUTATION_QUEUE_AGE_SECONDS: u64 = 300;

#[derive(Debug, Deserialize)]
pub(crate) struct OperatorDecisionActionRequest {
    pub queue_sha256: String,
    pub generated_at: String,
    pub maximum_freshness_seconds: u64,
    pub limit: i64,
    pub offset: i64,
    pub action: OperatorDecisionAction,
    pub confirm_action: bool,
    pub reason: Option<String>,
    pub budget_policy: Option<BudgetAutoPausePolicy>,
}

pub(crate) async fn api_apply_operator_decision_action(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    Path(decision_id): Path<String>,
    Json(request): Json<OperatorDecisionActionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !request.confirm_action && !matches!(request.action, OperatorDecisionAction::Inspect) {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "operator_decision_confirmation_required",
            "confirm_action must be true",
        ));
    }
    if request.maximum_freshness_seconds == 0
        || request.maximum_freshness_seconds > 30 * 24 * 60 * 60
        || request.limit <= 0
        || request.limit > 100
        || request.offset < 0
        || request.offset > 10_000
        || !valid_hash(&request.queue_sha256)
    {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "invalid_operator_decision_action_request",
            "queue hash, freshness, limit, or offset is outside the bounded contract",
        ));
    }
    let actor = authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?.api_key_id;
    let store = require_store(&state)?;
    let current_time = store.operator_decision_now();
    validate_mutation_time(
        &request.generated_at,
        &current_time,
        request.maximum_freshness_seconds,
    )?;

    let bound_queue = store
        .operator_decision_queue(
            &request.generated_at,
            request.maximum_freshness_seconds,
            request.limit,
            request.offset,
        )
        .map_err(queue_changed)?;
    if bound_queue.queue_sha256 != request.queue_sha256 {
        return Err(ApiError::with_code(
            StatusCode::CONFLICT,
            "operator_decision_queue_changed",
            "decision queue hash no longer matches the bound read page",
        ));
    }
    let (bound_position, bound_item) = find_bound_item(&bound_queue.items, &decision_id)?;
    validate_requested_action(&bound_item, &request.action)?;
    let bound_source = bound_item
        .selected_source
        .clone()
        .ok_or_else(not_ready_source)?;

    let current_queue = store
        .operator_decision_queue(
            &current_time,
            request.maximum_freshness_seconds,
            request.limit,
            request.offset,
        )
        .map_err(queue_changed)?;
    let (current_position, current_item) = current_queue
        .items
        .iter()
        .enumerate()
        .find(|(_, item)| item.decision_id == decision_id)
        .map(|(position, item)| (position, item.clone()))
        .ok_or_else(|| {
            ApiError::with_code(
                StatusCode::CONFLICT,
                "operator_decision_current_state_changed",
                "decision is no longer present on the exact current queue page",
            )
        })?;
    validate_current_binding(
        &bound_item,
        &current_item,
        &bound_source,
        &request.action,
        bound_position,
        current_position,
    )?;
    let current_source = current_item
        .selected_source
        .clone()
        .ok_or_else(not_ready_source)?;

    let result = match request.action {
        OperatorDecisionAction::Approve | OperatorDecisionAction::Reject => {
            require_source_kind(&current_source, "approval", "approval source is required")?;
            let approval_id = approval_evidence_id(&current_source, &request.action)?;
            let decision = if matches!(request.action, OperatorDecisionAction::Approve) {
                "approved"
            } else {
                "rejected"
            };
            if store
                .tool_execution_approval_requires_execute_scope(
                    &current_item.resource_id,
                    &approval_id,
                )
                .map_err(internal)?
            {
                authorize(
                    &state,
                    &headers,
                    "dispatch:execute",
                    uri.path(),
                    &request_id.0,
                )?;
            }
            store
                .resolve_requested_workflow_run_approval(
                    &current_item.resource_id,
                    &approval_id,
                    decision,
                    &actor,
                    request.reason.as_deref(),
                )
                .map_err(|error| {
                    ApiError::with_code(
                        StatusCode::CONFLICT,
                        "operator_decision_source_changed",
                        error,
                    )
                })?
        }
        OperatorDecisionAction::Resume => {
            let run = store
                .get_workflow_run(&current_item.resource_id)
                .map_err(internal)?
                .ok_or_else(|| source_changed("workflow run is no longer available"))?;
            let pause_reason = run
                .get("pause_reason")
                .and_then(Value::as_str)
                .ok_or_else(|| source_changed("workflow run is no longer paused"))?
                .to_string();
            if pause_reason.starts_with("budget_auto_pause:") {
                require_source_kind(
                    &current_source,
                    "recovery",
                    "budget recovery source is required",
                )?;
                authorize(
                    &state,
                    &headers,
                    "dispatch:execute",
                    uri.path(),
                    &request_id.0,
                )?;
                let reason = request
                    .reason
                    .as_deref()
                    .map(str::trim)
                    .filter(|reason| !reason.is_empty())
                    .ok_or_else(|| {
                        ApiError::with_code(
                            StatusCode::BAD_REQUEST,
                            "operator_decision_recovery_reason_required",
                            "budget pause recovery requires a bounded operator reason",
                        )
                    })?;
                store
                    .recover_budget_auto_pause(&current_item.resource_id, "resume", reason, &actor)
                    .map_err(internal)?
            } else {
                require_source_kind(&current_source, "workflow", "workflow source is required")?;
                store
                    .update_run_pause_reason(&current_item.resource_id, None)
                    .map_err(operator_owner_conflict)?;
                match store.request_workflow_run_resume(
                    &current_item.resource_id,
                    &actor,
                    request.reason.as_deref(),
                ) {
                    Ok(result) => result,
                    Err(error) => {
                        let compensation = store
                            .update_run_pause_reason(
                                &current_item.resource_id,
                                Some(&pause_reason),
                            )
                            .map_err(|compensation_error| {
                                internal(format!(
                                    "resume failed: {error}; pause compensation failed: {compensation_error}"
                                ))
                            });
                        compensation?;
                        return Err(internal(format!(
                            "resume failed and pause reason was restored: {error}"
                        )));
                    }
                }
            }
        }
        OperatorDecisionAction::Retry => {
            require_source_kind(&current_source, "workflow", "workflow source is required")?;
            let run = store
                .get_workflow_run(&current_item.resource_id)
                .map_err(internal)?
                .ok_or_else(|| source_changed("workflow run is no longer available"))?;
            if run.get("status").and_then(Value::as_str) != Some("blocked") {
                return Err(source_changed("workflow run is no longer retryable"));
            }
            store
                .tick_with_retry(&current_item.resource_id, &actor, 0)
                .map_err(|error| {
                    if is_execution_owner_conflict(&error) {
                        operator_owner_conflict(error)
                    } else {
                        ApiError::with_code(
                            StatusCode::CONFLICT,
                            "operator_decision_action_rejected",
                            error,
                        )
                    }
                })?
        }
        OperatorDecisionAction::Pause => {
            authorize(
                &state,
                &headers,
                "dispatch:execute",
                uri.path(),
                &request_id.0,
            )?;
            require_source_kind(
                &current_source,
                "budget",
                "budget anomaly source is required",
            )?;
            let policy = request.budget_policy.as_ref().ok_or_else(|| {
                ApiError::with_code(
                    StatusCode::BAD_REQUEST,
                    "operator_decision_budget_policy_required",
                    "budget_policy is required for a pause",
                )
            })?;
            store
                .apply_budget_auto_pause(
                    &current_source.evidence_id,
                    &current_item.resource_id,
                    policy,
                    &actor,
                )
                .map_err(|error| {
                    ApiError::with_code(
                        StatusCode::CONFLICT,
                        "operator_decision_action_rejected",
                        error,
                    )
                })?
        }
        OperatorDecisionAction::Rollback => {
            authorize(&state, &headers, "team:admin", uri.path(), &request_id.0)?;
            require_source_kind(
                &current_source,
                "adaptive_policy_snapshot",
                "adaptive policy snapshot source is required",
            )?;
            store
                .rollback_adaptive_fusion_policy(&current_source.evidence_id, true, &actor)
                .map_err(internal)?
        }
        OperatorDecisionAction::Inspect => inspect_source(&store, &current_source)?,
        OperatorDecisionAction::Acknowledge => {
            authorize(
                &state,
                &headers,
                "dispatch:execute",
                uri.path(),
                &request_id.0,
            )?;
            let source_sha256 = current_source
                .content_sha256
                .as_deref()
                .ok_or_else(|| source_changed("acknowledgement source hash is unavailable"))?;
            store
                .acknowledge_operator_source(
                    &decision_id,
                    &current_source.evidence_type,
                    &current_source.evidence_id,
                    source_sha256,
                    request.reason.as_deref(),
                    &actor,
                )
                .map_err(internal)?
        }
    };

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": "operator_decision_action_result.v1",
            "decision_id": decision_id,
            "queue_sha256": request.queue_sha256,
            "current_queue_sha256": current_queue.queue_sha256,
            "current_generated_at": current_time,
            "action": request.action,
            "owner_result": result,
        })),
    ))
}

fn inspect_source(
    store: &crate::storage::local_product_store::LocalProductStore,
    source: &OperatorDecisionEvidenceReference,
) -> Result<Value, ApiError> {
    let value = match source.evidence_type.as_str() {
        "budget_anomaly_finding" => store
            .get_budget_evidence_artifact(&source.evidence_id)
            .map_err(internal)?,
        "token_efficiency_regression_artifact" => store
            .get_regression_report_artifact(&source.evidence_id)
            .map_err(internal)?,
        "policy_proposal" => store
            .get_policy_proposal(&source.evidence_id)
            .map_err(internal)?,
        "scheduler_heartbeat" => store
            .read_heartbeat()
            .map_err(internal)?
            .map(|heartbeat| serde_json::to_value(heartbeat).unwrap_or(Value::Null)),
        _ => return Err(unsupported("source type has no typed inspect owner")),
    };
    value.ok_or_else(|| source_changed("inspect source is no longer available"))
}

fn validate_mutation_time(
    generated_at: &str,
    current_time: &str,
    requested_freshness_seconds: u64,
) -> Result<(), ApiError> {
    let generated = DateTime::parse_from_rfc3339(generated_at).map_err(|_| {
        ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "invalid_operator_decision_generated_at",
            "generated_at must be RFC3339",
        )
    })?;
    let current = DateTime::parse_from_rfc3339(current_time).map_err(|_| {
        ApiError::with_code(
            StatusCode::INTERNAL_SERVER_ERROR,
            "operator_decision_clock_unavailable",
            "store clock did not return RFC3339",
        )
    })?;
    if generated > current {
        return Err(ApiError::with_code(
            StatusCode::CONFLICT,
            "operator_decision_generated_at_future",
            "mutation queue timestamp is later than the store clock",
        ));
    }
    let age_seconds = (current - generated).num_seconds() as u64;
    let allowed_age = requested_freshness_seconds.min(MAX_MUTATION_QUEUE_AGE_SECONDS);
    if age_seconds > allowed_age {
        return Err(ApiError::with_code(
            StatusCode::CONFLICT,
            "operator_decision_generated_at_stale",
            "mutation queue timestamp is older than the allowed action freshness",
        ));
    }
    Ok(())
}

fn find_bound_item(
    items: &[OperatorDecisionItem],
    decision_id: &str,
) -> Result<(usize, OperatorDecisionItem), ApiError> {
    items
        .iter()
        .enumerate()
        .find(|(_, item)| item.decision_id == decision_id)
        .map(|(position, item)| (position, item.clone()))
        .ok_or_else(|| {
            ApiError::with_code(
                StatusCode::NOT_FOUND,
                "operator_decision_not_found",
                "decision is not present in the bound queue page",
            )
        })
}

fn validate_requested_action(
    item: &OperatorDecisionItem,
    action: &OperatorDecisionAction,
) -> Result<(), ApiError> {
    if item.outcome != OperatorDecisionOutcome::Ready
        || item.recommended_action.as_ref() != Some(action)
    {
        return Err(ApiError::with_code(
            StatusCode::CONFLICT,
            "operator_decision_not_ready",
            "decision does not exactly recommend this action",
        ));
    }
    Ok(())
}

fn validate_current_binding(
    bound: &OperatorDecisionItem,
    current: &OperatorDecisionItem,
    bound_source: &OperatorDecisionEvidenceReference,
    action: &OperatorDecisionAction,
    bound_position: usize,
    current_position: usize,
) -> Result<(), ApiError> {
    validate_requested_action(current, action)?;
    if current.conflict_key != bound.conflict_key
        || current.resource_id != bound.resource_id
        || current.selected_source.as_ref() != Some(bound_source)
        || current_position != bound_position
    {
        return Err(ApiError::with_code(
            StatusCode::CONFLICT,
            "operator_decision_current_state_changed",
            "current decision source, resource, conflict binding, or page order differs from the read decision",
        ));
    }
    Ok(())
}

fn approval_evidence_id(
    source: &OperatorDecisionEvidenceReference,
    action: &OperatorDecisionAction,
) -> Result<String, ApiError> {
    let expected_suffix = if matches!(action, OperatorDecisionAction::Approve) {
        ":approve"
    } else {
        ":reject"
    };
    source
        .evidence_id
        .strip_suffix(expected_suffix)
        .map(str::to_string)
        .ok_or_else(|| {
            ApiError::with_code(
                StatusCode::CONFLICT,
                "operator_decision_source_changed",
                "approval source identity does not match the requested action",
            )
        })
}

fn require_source_kind(
    source: &OperatorDecisionEvidenceReference,
    expected: &str,
    message: &str,
) -> Result<(), ApiError> {
    if source.evidence_type != expected {
        return Err(unsupported(message));
    }
    Ok(())
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn not_ready_source() -> ApiError {
    ApiError::with_code(
        StatusCode::CONFLICT,
        "operator_decision_not_ready",
        "decision has no selected source",
    )
}

fn queue_changed(error: impl ToString) -> ApiError {
    ApiError::with_code(
        StatusCode::CONFLICT,
        "operator_decision_queue_changed",
        error.to_string(),
    )
}

fn source_changed(message: &str) -> ApiError {
    ApiError::with_code(
        StatusCode::CONFLICT,
        "operator_decision_source_changed",
        message,
    )
}

fn unsupported(message: &str) -> ApiError {
    ApiError::with_code(
        StatusCode::CONFLICT,
        "operator_decision_action_not_allowlisted",
        message,
    )
}

fn internal(error: impl ToString) -> ApiError {
    ApiError::with_code(
        StatusCode::CONFLICT,
        "operator_decision_action_rejected",
        error.to_string(),
    )
}

fn operator_owner_conflict(error: impl ToString) -> ApiError {
    let error = error.to_string();
    if is_execution_owner_conflict(&error) {
        ApiError::with_code(
            StatusCode::CONFLICT,
            "operator_decision_execution_owner_conflict",
            error,
        )
    } else {
        internal(error)
    }
}
