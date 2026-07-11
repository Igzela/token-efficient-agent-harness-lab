use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::http_server::middleware::{authorize, cors_headers, require_store, ApiError, RequestId};
use crate::http_server::state::AxumApiState;
use crate::operator_decision::OperatorDecisionAction;
use crate::storage::local_product_store::BudgetAutoPausePolicy;

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
    if !request.confirm_action {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "operator_decision_confirmation_required",
            "confirm_action must be true",
        ));
    }
    let actor = authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?.api_key_id;
    let store = require_store(&state)?;
    let queue = store
        .operator_decision_queue(
            &request.generated_at,
            request.maximum_freshness_seconds,
            request.limit,
            request.offset,
        )
        .map_err(|error| {
            ApiError::with_code(
                StatusCode::CONFLICT,
                "operator_decision_queue_changed",
                error,
            )
        })?;
    if queue.queue_sha256 != request.queue_sha256 {
        return Err(ApiError::with_code(
            StatusCode::CONFLICT,
            "operator_decision_queue_changed",
            "decision queue hash no longer matches",
        ));
    }
    let item = queue
        .items
        .into_iter()
        .find(|item| item.decision_id == decision_id)
        .ok_or_else(|| {
            ApiError::with_code(
                StatusCode::NOT_FOUND,
                "operator_decision_not_found",
                "decision is not present in the bound queue",
            )
        })?;
    if item.recommended_action.as_ref() != Some(&request.action) {
        return Err(ApiError::with_code(
            StatusCode::CONFLICT,
            "operator_decision_not_ready",
            "decision does not currently recommend this action",
        ));
    }
    let source = item.selected_source.ok_or_else(|| {
        ApiError::with_code(
            StatusCode::CONFLICT,
            "operator_decision_not_ready",
            "decision has no selected source",
        )
    })?;
    let result = match request.action {
        OperatorDecisionAction::Approve | OperatorDecisionAction::Reject => {
            if source.evidence_type != "approval" {
                return Err(unsupported("approval source is required"));
            }
            let approval = store
                .workflow_run_approvals(&item.resource_id, 100)
                .map_err(internal)?
                .into_iter()
                .find(|approval| approval["approval_id"] == source.evidence_id)
                .ok_or_else(|| {
                    ApiError::with_code(
                        StatusCode::CONFLICT,
                        "operator_decision_source_changed",
                        "approval source is no longer available",
                    )
                })?;
            let node_id = approval["node_id"]
                .as_str()
                .ok_or_else(|| internal("approval source has no node id"))?;
            let decision = if matches!(request.action, OperatorDecisionAction::Approve) {
                "approved"
            } else {
                "rejected"
            };
            store
                .record_workflow_run_approval(
                    &item.resource_id,
                    node_id,
                    decision,
                    &actor,
                    request.reason.as_deref(),
                    None,
                    None,
                    None,
                    None,
                )
                .map_err(internal)?
        }
        OperatorDecisionAction::Resume => {
            if source.evidence_type != "workflow" {
                return Err(unsupported("workflow source is required"));
            }
            store
                .request_workflow_run_resume(&item.resource_id, &actor, request.reason.as_deref())
                .map_err(internal)?
        }
        OperatorDecisionAction::Retry => {
            if source.evidence_type != "workflow" {
                return Err(unsupported("workflow source is required"));
            }
            store
                .tick_with_retry(&item.resource_id, &actor, 0)
                .map_err(internal)?
        }
        OperatorDecisionAction::Pause => {
            authorize(
                &state,
                &headers,
                "dispatch:execute",
                uri.path(),
                &request_id.0,
            )?;
            if source.evidence_type != "budget" {
                return Err(unsupported("budget anomaly source is required"));
            }
            let policy = request.budget_policy.as_ref().ok_or_else(|| {
                ApiError::with_code(
                    StatusCode::BAD_REQUEST,
                    "operator_decision_budget_policy_required",
                    "budget_policy is required for a pause",
                )
            })?;
            store
                .apply_budget_auto_pause(&source.evidence_id, &item.resource_id, policy, &actor)
                .map_err(|error| {
                    ApiError::with_code(
                        StatusCode::CONFLICT,
                        "operator_decision_action_rejected",
                        error,
                    )
                })?
        }
        OperatorDecisionAction::Rollback
        | OperatorDecisionAction::Inspect
        | OperatorDecisionAction::Acknowledge => {
            return Err(unsupported(
                "no compatible existing action owner is available",
            ))
        }
    };
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": "operator_decision_action_result.v1",
            "decision_id": decision_id,
            "queue_sha256": request.queue_sha256,
            "action": request.action,
            "owner_result": result,
        })),
    ))
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
