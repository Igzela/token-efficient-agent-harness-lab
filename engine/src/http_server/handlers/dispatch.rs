use axum::extract::{Extension, Path as AxumPath, Query, State};
use axum::http::{HeaderMap, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;
use std::sync::Arc;

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, require_store, ApiError, RequestId,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::{
    AutoAdjustmentApplyRequest, AutoAdjustmentRollbackRequest, DispatchApiRequest,
    PolicyProposalActionRequest, PolicyProposalCreateRequest, AXUM_API_SCHEMA_VERSION,
};
use crate::infrastructure::structured_events;
use crate::provider::cost_gate::{check_cost_gates, CostGateConfig};

pub(crate) async fn api_dispatch(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Json(request): Json<DispatchApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    if request.raw_request.trim().is_empty() {
        return Err(ApiError::with_code(
            axum::http::StatusCode::BAD_REQUEST,
            "raw_request_required",
            "raw_request is required",
        ));
    }

    structured_events::log_dispatch_start(&request_id.0, "", "http", request.raw_request.len());

    let is_provider = state.executor_type() == "provider";
    if is_provider {
        authorize(
            &state,
            &headers,
            "dispatch:execute",
            uri.path(),
            &request_id.0,
        )?;
    }

    let request_source = request.request_source.as_deref().unwrap_or("api");
    let active_policy = if let Some(store) = &state.local_store {
        store.active_routing_policy().map_err(internal_error)?
    } else {
        None
    };

    if is_provider {
        let cost_config = CostGateConfig::from_env();
        if cost_config.is_active() {
            let reserved = state
                .engine
                .preflight_reserved_cost(&request.raw_request, request_source);
            let daily_cost = if let Some(store) = &state.local_store {
                let today_prefix = &crate::http_server::middleware::chrono_free_today()[..10];
                store.daily_estimated_cost_usd(today_prefix).unwrap_or(0.0)
            } else {
                0.0
            };
            let cost_gate_passed = check_cost_gates(&cost_config, reserved, daily_cost).is_ok();
            structured_events::log_cost_gate(
                &request_id.0,
                "",
                reserved,
                daily_cost,
                cost_config.per_dispatch_cap_usd.unwrap_or(0.0),
                cost_config.daily_cap_usd.unwrap_or(0.0),
                cost_gate_passed,
            );
            if !cost_gate_passed {
                let raw = request.raw_request.clone();
                let src = request_source.to_string();
                let eng = Arc::clone(&state.engine);
                let policy = active_policy.clone();
                let bundle = tokio::task::spawn_blocking(move || match policy {
                    Some(policy) => eng.dispatch_with_policy(&raw, &src, policy),
                    None => eng.dispatch(&raw, &src),
                })
                .await
                .map_err(|e| internal_error(e.to_string()))?;
                if let Some(store) = &state.local_store {
                    store
                        .record_dispatch(
                            &request.raw_request,
                            request_source,
                            &bundle,
                            &context.api_key_id,
                        )
                        .map_err(internal_error)?;
                }
                let did = bundle
                    .get("record")
                    .and_then(|r| r.get("dispatch_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let fs = bundle
                    .get("record")
                    .and_then(|r| r.get("final_status"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                structured_events::log_dispatch_complete(&request_id.0, did, fs);
                return Ok((cors_headers(), Json(bundle)));
            }
        }
    }

    let raw = request.raw_request.clone();
    let src = request_source.to_string();
    let eng = Arc::clone(&state.engine);
    let bundle = tokio::task::spawn_blocking(move || match active_policy {
        Some(policy) => eng.dispatch_with_policy(&raw, &src, policy),
        None => eng.dispatch(&raw, &src),
    })
    .await
    .map_err(|e| internal_error(e.to_string()))?;
    if let Some(store) = &state.local_store {
        store
            .record_dispatch(
                &request.raw_request,
                request_source,
                &bundle,
                &context.api_key_id,
            )
            .map_err(internal_error)?;
    }
    let did = bundle
        .get("record")
        .and_then(|r| r.get("dispatch_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let fs = bundle
        .get("record")
        .and_then(|r| r.get("final_status"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    structured_events::log_dispatch_complete(&request_id.0, did, fs);
    Ok((cors_headers(), Json(bundle)))
}

pub(crate) async fn api_dispatches(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(100)
        .clamp(0, 500);
    let offset = params
        .get("offset")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0)
        .max(0);
    let search = params.get("search").map(String::as_str);
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "dispatches": store.search_dispatches(limit, offset, search).map_err(internal_error)?,
        })),
    ))
}

pub(crate) async fn api_dispatch_detail(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(dispatch_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    match store.get_dispatch(&dispatch_id).map_err(internal_error)? {
        Some(dispatch) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "dispatch": dispatch,
            })),
        )),
        None => Err(ApiError::with_code(
            axum::http::StatusCode::NOT_FOUND,
            "dispatch_not_found",
            "dispatch not found",
        )),
    }
}

pub(crate) async fn api_dispatch_metrics(
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
            "metrics": store.dispatch_metrics(limit).map_err(internal_error)?,
            "limit": limit,
        })),
    ))
}

pub(crate) async fn api_feedback_traces(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let limit = query_i64(&params, "limit", 100).clamp(0, 500);
    let offset = query_i64(&params, "offset", 0).max(0);
    let traces = store
        .feedback_traces(
            limit,
            offset,
            params.get("task_class").map(String::as_str),
            params.get("tier").map(String::as_str),
            params.get("status").map(String::as_str),
        )
        .map_err(internal_error)?;
    Ok((cors_headers(), Json(traces)))
}

pub(crate) async fn api_feedback_patterns(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let patterns = store
        .feedback_patterns(
            params.get("task_class").map(String::as_str),
            params.get("tier").map(String::as_str),
        )
        .map_err(internal_error)?;
    Ok((cors_headers(), Json(patterns)))
}

pub(crate) async fn api_feedback_cost_of_pass(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "cost:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let rows = store
        .cost_of_pass(
            params.get("task_class").map(String::as_str),
            params.get("tier").map(String::as_str),
        )
        .map_err(internal_error)?;
    Ok((cors_headers(), Json(rows)))
}

pub(crate) async fn api_simulation_report(
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
        Json(store.simulation_report(limit).map_err(internal_error)?),
    ))
}

pub(crate) async fn api_policy_simulation_report(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let limit = query_i64(&params, "limit", 100).clamp(0, 500);
    let policy = params
        .get("policy")
        .cloned()
        .unwrap_or_else(|| "cheapest".to_string());
    Ok((
        cors_headers(),
        Json(
            store
                .policy_simulation_report_with_policy(limit, &policy)
                .map_err(internal_error)?,
        ),
    ))
}

pub(crate) async fn api_policy_proposals(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let limit = query_i64(&params, "limit", 100).clamp(0, 500);
    let offset = query_i64(&params, "offset", 0).max(0);
    Ok((
        cors_headers(),
        Json(
            store
                .list_policy_proposals(limit, offset, params.get("status").map(String::as_str))
                .map_err(internal_error)?,
        ),
    ))
}

pub(crate) async fn api_create_policy_proposal(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Json(request): Json<PolicyProposalCreateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let request_value = serde_json::to_value(request).map_err(|e| internal_error(e.to_string()))?;
    let proposal = store
        .create_policy_proposal(&request_value, &context.api_key_id)
        .map_err(bad_policy_request)?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "proposal": proposal,
        })),
    ))
}

pub(crate) async fn api_policy_proposal_detail(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(proposal_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    match store
        .get_policy_proposal(&proposal_id)
        .map_err(internal_error)?
    {
        Some(proposal) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "proposal": proposal,
            })),
        )),
        None => Err(ApiError::with_code(
            axum::http::StatusCode::NOT_FOUND,
            "proposal_not_found",
            "proposal not found",
        )),
    }
}

pub(crate) async fn api_approve_policy_proposal(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(proposal_id): AxumPath<String>,
    Json(request): Json<PolicyProposalActionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_auth_for_policy_override(&state)?;
    let context = authorize(&state, &headers, "team:admin", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let actor = request.actor.as_deref().unwrap_or(&context.api_key_id);
    let proposal = store
        .approve_policy_proposal(
            &proposal_id,
            actor,
            request.reason.as_deref(),
            request.confirm_policy_override.unwrap_or(false),
        )
        .map_err(bad_policy_request)?;
    structured_events::log_proposal_action(&proposal_id, "approve", actor);
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "proposal": proposal,
        })),
    ))
}

pub(crate) async fn api_reject_policy_proposal(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(proposal_id): AxumPath<String>,
    Json(request): Json<PolicyProposalActionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_auth_for_policy_override(&state)?;
    let context = authorize(&state, &headers, "team:admin", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let actor = request.actor.as_deref().unwrap_or(&context.api_key_id);
    let proposal = store
        .reject_policy_proposal(&proposal_id, actor, request.reason.as_deref())
        .map_err(bad_policy_request)?;
    structured_events::log_proposal_action(&proposal_id, "reject", actor);
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "proposal": proposal,
        })),
    ))
}

pub(crate) async fn api_deactivate_policy_proposal(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(proposal_id): AxumPath<String>,
    Json(request): Json<PolicyProposalActionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_auth_for_policy_override(&state)?;
    let context = authorize(&state, &headers, "team:admin", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let actor = request.actor.as_deref().unwrap_or(&context.api_key_id);
    let proposal = store
        .deactivate_policy_proposal(
            &proposal_id,
            actor,
            request.reason.as_deref(),
            request.confirm_policy_override.unwrap_or(false),
        )
        .map_err(bad_policy_request)?;
    structured_events::log_proposal_action(&proposal_id, "deactivate", actor);
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "proposal": proposal,
        })),
    ))
}

pub(crate) async fn api_rollback_policy_proposal(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(proposal_id): AxumPath<String>,
    Json(request): Json<PolicyProposalActionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_auth_for_policy_override(&state)?;
    let context = authorize(&state, &headers, "team:admin", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let actor = request.actor.as_deref().unwrap_or(&context.api_key_id);
    let proposal = store
        .rollback_policy_proposal(
            &proposal_id,
            actor,
            request.reason.as_deref(),
            request.confirm_policy_override.unwrap_or(false),
        )
        .map_err(bad_policy_request)?;
    structured_events::log_proposal_action(&proposal_id, "rollback", actor);
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "proposal": proposal,
        })),
    ))
}

pub(crate) async fn api_generated_proposals(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let limit = query_i64(&params, "limit", 50).clamp(0, 500);
    Ok((
        cors_headers(),
        Json(store.generated_proposals(limit).map_err(internal_error)?),
    ))
}

pub(crate) async fn api_auto_adjustments(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let limit = query_i64(&params, "limit", 50).clamp(0, 500);
    Ok((
        cors_headers(),
        Json(
            store
                .auto_adjustments_report(limit)
                .map_err(internal_error)?,
        ),
    ))
}

pub(crate) async fn api_apply_auto_adjustment(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Json(request): Json<AutoAdjustmentApplyRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_auth_for_policy_override(&state)?;
    let context = authorize(&state, &headers, "team:admin", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let actor = request
        .actor
        .clone()
        .unwrap_or_else(|| context.api_key_id.clone());
    let request_value = serde_json::to_value(request).map_err(|e| internal_error(e.to_string()))?;
    let result = store
        .apply_auto_adjustment(&request_value, &actor)
        .map_err(bad_policy_request)?;
    let adj_id = result
        .get("adjustment_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let prop_id = result
        .get("proposal_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let pk = result
        .get("policy_key")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let tt = result
        .get("target_tier")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let accepted = result
        .get("applied")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    structured_events::log_active_apply(adj_id, prop_id, pk, tt, accepted);
    Ok((cors_headers(), Json(result)))
}

pub(crate) async fn api_rollback_auto_adjustment(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(adjustment_id): AxumPath<String>,
    Json(request): Json<AutoAdjustmentRollbackRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_auth_for_policy_override(&state)?;
    let context = authorize(&state, &headers, "team:admin", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let actor = request
        .actor
        .clone()
        .unwrap_or_else(|| context.api_key_id.clone());
    let request_value = serde_json::to_value(request).map_err(|e| internal_error(e.to_string()))?;
    let result = store
        .rollback_auto_adjustment(&adjustment_id, &request_value, &actor)
        .map_err(bad_policy_request)?;
    let prop_id = result
        .get("proposal_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let rolled_back = result
        .get("rolled_back")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let status = result.get("status").and_then(|v| v.as_str()).unwrap_or("");
    structured_events::log_rollback(&adjustment_id, prop_id, rolled_back, status);
    Ok((cors_headers(), Json(result)))
}

fn require_auth_for_policy_override(state: &AxumApiState) -> Result<(), ApiError> {
    if state.tenant_resolver.is_none() {
        return Err(ApiError::with_code(
            axum::http::StatusCode::FORBIDDEN,
            "auth_required_for_policy_override",
            "policy override activation requires configured authentication",
        ));
    }
    Ok(())
}

fn bad_policy_request(error: String) -> ApiError {
    ApiError::with_code(
        axum::http::StatusCode::BAD_REQUEST,
        "invalid_policy_proposal",
        error,
    )
}

fn query_i64(
    params: &std::collections::HashMap<String, String>,
    key: &str,
    default_value: i64,
) -> i64 {
    params
        .get(key)
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(default_value)
}

/// GET /api/v1/regulator/state — read-only snapshot of regulator operational state.
/// No mutation. No secrets. Requires dispatch:read scope.
pub(crate) async fn api_regulator_state(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;

    // Active routing policy (merged tier_map from active proposals)
    let active_policy = store.active_routing_policy().map_err(internal_error)?;

    // Pending proposals
    let pending = store
        .list_policy_proposals(50, 0, Some("pending"))
        .map_err(internal_error)?;
    let pending_count = pending.get("total").and_then(|v| v.as_i64()).unwrap_or(0);

    // Active proposals
    let active = store
        .list_policy_proposals(50, 0, Some("active"))
        .map_err(internal_error)?;
    let active_count = active.get("total").and_then(|v| v.as_i64()).unwrap_or(0);

    // Active auto-adjustments
    let active_adjustments = store.active_auto_adjustments().map_err(internal_error)?;

    // Auto-adjustment report (includes guard mode, blocked reasons)
    let adjustments_report = store.auto_adjustments_report(50).map_err(internal_error)?;

    // Env gate status (read-only, no secrets)
    let env_gate = std::env::var("ACP_ENABLE_AUTO_ADJUSTMENT").ok().as_deref() == Some("1");
    let dry_run = std::env::var("ACP_AUTO_ADJUSTMENT_DRY_RUN").ok().as_deref() == Some("1");
    let active_gate = std::env::var("ACP_AUTO_ADJUSTMENT_ACTIVE").ok().as_deref() == Some("1");

    let mode = if !env_gate {
        "disabled"
    } else if dry_run {
        "dry_run"
    } else if active_gate {
        "active"
    } else {
        "disabled"
    };

    // PostgreSQL backend detection
    let pg_backend = std::env::var("ACP_DATABASE_URL").ok().is_some();

    // Build response
    Ok((
        cors_headers(),
        Json(serde_json::json!({
            "schema_version": "regulator_state.v1",
            "regulator": {
                "mode": mode,
                "env_gate_enabled": env_gate,
                "dry_run_enabled": dry_run,
                "active_gate_enabled": active_gate,
                "pg_backend_detected": pg_backend,
            },
            "active_routing_policy": active_policy,
            "proposals": {
                "pending_count": pending_count,
                "active_count": active_count,
            },
            "auto_adjustments": {
                "active_count": active_adjustments.len(),
                "report": adjustments_report,
            },
            "warnings": build_regulator_warnings(env_gate, dry_run, active_gate, pg_backend),
        })),
    ))
}

fn build_regulator_warnings(
    env_gate: bool,
    dry_run: bool,
    active_gate: bool,
    pg_backend: bool,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if !env_gate {
        warnings
            .push("ACP_ENABLE_AUTO_ADJUSTMENT is not set; auto-adjustment is disabled".to_string());
    }
    if env_gate && !dry_run && !active_gate {
        warnings.push("ACP_AUTO_ADJUSTMENT_ACTIVE is not set; auto-adjustment is in disabled mode despite env gate".to_string());
    }
    if pg_backend && std::env::var("ACP_TEST_DATABASE_URL").ok().is_none() {
        warnings.push("PostgreSQL backend detected but ACP_TEST_DATABASE_URL not set; PG active trial status is BLOCKED".to_string());
    }
    warnings
}
