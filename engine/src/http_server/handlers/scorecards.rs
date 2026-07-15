use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, require_store, ApiError, RequestId,
};
use crate::http_server::state::AxumApiState;
use crate::storage::local_product_store::{
    BudgetAutoPausePolicy, ReplayProductionProfile, ReplayProductionRequest,
};

#[derive(Debug, Deserialize)]
pub(crate) struct ScorecardQuery {
    run_id: Option<String>,
    dispatch_id: Option<String>,
    scenario_id: Option<String>,
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RegressionQuery {
    scenario_id: Option<String>,
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RegressionTrendQuery {
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BudgetEvidenceQuery {
    kind: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OfflineReplayQuery {
    status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UsageObservationQuery {
    run_id: String,
    limit: Option<i64>,
}

pub(crate) async fn api_usage_observations(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    Query(query): Query<UsageObservationQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let run = store
        .get_workflow_run(&query.run_id)
        .map_err(internal_error)?
        .ok_or_else(|| {
            ApiError::with_code(StatusCode::NOT_FOUND, "run_not_found", "run not found")
        })?;
    if run.get("tenant_id").and_then(serde_json::Value::as_str) != Some(context.tenant_id.as_str())
    {
        return Err(ApiError::with_code(
            StatusCode::NOT_FOUND,
            "run_not_found",
            "run not found",
        ));
    }
    let limit = query.limit.unwrap_or(64).clamp(1, 64);
    let observations = store
        .normalized_usage_observations_for_run(&query.run_id, limit)
        .map_err(internal_error)?;
    let count = observations.len();
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": "normalized_usage_read.v1",
            "run_id": query.run_id,
            "observations": observations,
            "count": count,
            "limit": limit,
            "read_only": true,
            "metadata_only": true,
            "raw_provider_content": "excluded",
        })),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OfflineReplayGenerateRequest {
    replay: ReplayProductionRequest,
    confirm_generation: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReplayProductionProfileRequest {
    profile: ReplayProductionProfile,
    confirm_profile: bool,
}

pub(crate) async fn api_replay_production_profile(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let profile = require_store(&state)?
        .replay_production_profile()
        .map_err(internal_error)?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": "offline_replay_production_profile_read.v1",
            "configured": profile.is_some(),
            "profile": profile,
            "provider_calls": "disabled",
            "mutation_authority": "none",
        })),
    ))
}

pub(crate) async fn api_configure_replay_production_profile(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    Json(request): Json<ReplayProductionProfileRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if state.tenant_resolver.is_none() {
        return Err(ApiError::with_code(
            StatusCode::FORBIDDEN,
            "auth_required_for_replay_profile",
            "replay production profile mutation requires configured authentication",
        ));
    }
    let context = authorize(&state, &headers, "team:admin", uri.path(), &request_id.0)?;
    if !request.confirm_profile {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "replay_profile_confirmation_required",
            "confirm_profile must be true",
        ));
    }
    let configured = require_store(&state)?
        .configure_replay_production_profile(&request.profile, &context.api_key_id)
        .map_err(|error| {
            ApiError::with_code(StatusCode::BAD_REQUEST, "replay_profile_rejected", error)
        })?;
    Ok((cors_headers(), Json(json!({"configured": configured}))))
}

pub(crate) async fn api_generate_offline_replay(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    Json(request): Json<OfflineReplayGenerateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    if !request.confirm_generation {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "offline_replay_confirmation_required",
            "confirm_generation must be true",
        ));
    }
    let result = require_store(&state)?
        .produce_offline_replay(&request.replay, &context.api_key_id)
        .map_err(|error| {
            ApiError::with_code(
                StatusCode::BAD_REQUEST,
                "offline_replay_generation_rejected",
                error,
            )
        })?;
    Ok((cors_headers(), Json(json!({"producer": result}))))
}

#[derive(Debug, Deserialize)]
pub(crate) struct BudgetAutoPauseRequest {
    run_id: String,
    confirm_auto_pause: bool,
    #[serde(default)]
    policy: BudgetAutoPausePolicy,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BudgetPauseRecoveryRequest {
    recovery: String,
    reason: String,
    confirm_recovery: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BudgetRecomputeRequest {
    run_id: String,
    confirm_recompute: bool,
}

pub(crate) async fn api_recompute_budget_evidence(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    Json(request): Json<BudgetRecomputeRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    if !request.confirm_recompute {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "budget_recompute_confirmation_required",
            "confirm_recompute must be true",
        ));
    }
    let store = require_store(&state)?;
    let run = store
        .get_workflow_run(&request.run_id)
        .map_err(internal_error)?
        .ok_or_else(|| {
            ApiError::with_code(StatusCode::NOT_FOUND, "run_not_found", "run not found")
        })?;
    if run.get("tenant_id").and_then(serde_json::Value::as_str) != Some(context.tenant_id.as_str())
    {
        return Err(ApiError::with_code(
            StatusCode::NOT_FOUND,
            "run_not_found",
            "run not found",
        ));
    }
    let result = store
        .produce_budget_intelligence_for_run(&request.run_id, &context.api_key_id)
        .map_err(internal_error)?;
    let observations = store
        .normalized_usage_observations_for_run(&request.run_id, 64)
        .map_err(internal_error)?;
    Ok((
        cors_headers(),
        Json(json!({"producer": result, "usage_observations": observations})),
    ))
}

pub(crate) async fn api_scorecard_artifacts(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    Query(query): Query<ScorecardQuery>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let artifacts = match (
        query.run_id.as_deref(),
        query.dispatch_id.as_deref(),
        query.scenario_id.as_deref(),
    ) {
        (Some(run_id), None, None) => store
            .native_scorecard_artifacts_by_run(run_id, limit)
            .map_err(internal_error)?,
        (None, Some(dispatch_id), None) => store
            .native_scorecard_artifacts_by_dispatch(dispatch_id, limit)
            .map_err(internal_error)?,
        (None, None, Some(scenario_id)) => store
            .scorecard_artifacts_by_scenario(scenario_id, limit)
            .map_err(internal_error)?,
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) | (_, Some(_), Some(_)) => {
            return Err(ApiError::with_code(
                StatusCode::BAD_REQUEST,
                "invalid_scorecard_query",
                "provide only one of run_id, dispatch_id, or scenario_id",
            ));
        }
        (None, None, None) => {
            return Err(ApiError::with_code(
                StatusCode::BAD_REQUEST,
                "invalid_scorecard_query",
                "run_id, dispatch_id, or scenario_id is required",
            ));
        }
    };
    let comparison = if let Some(scenario_id) = query.scenario_id.as_deref() {
        Some(
            store
                .scorecard_comparison_by_scenario(scenario_id)
                .map_err(|error| {
                    ApiError::with_code(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "incomparable_scorecards",
                        error,
                    )
                })?,
        )
    } else {
        None
    };

    Ok((
        cors_headers(),
        Json(json!({
            "metadata_only": true,
            "read_only": true,
            "target_repository_writes": "disabled",
            "artifacts": artifacts,
            "comparison": comparison,
        })),
    ))
}

pub(crate) async fn api_scorecard_artifact_detail(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    Path(artifact_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let artifact = store
        .get_native_scorecard_artifact(&artifact_id)
        .map_err(internal_error)?
        .ok_or_else(|| {
            ApiError::with_code(
                StatusCode::NOT_FOUND,
                "native_scorecard_artifact_not_found",
                "native scorecard artifact not found",
            )
        })?;
    Ok((
        cors_headers(),
        Json(json!({
            "metadata_only": true,
            "read_only": true,
            "target_repository_writes": "disabled",
            "artifact": artifact,
        })),
    ))
}

pub(crate) async fn api_regression_artifacts(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    Query(query): Query<RegressionQuery>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let artifacts = if let Some(scenario_id) = query.scenario_id.as_deref() {
        store
            .regression_report_artifacts_by_scenario(scenario_id, limit)
            .map_err(internal_error)?
    } else {
        store
            .regression_report_artifacts(limit)
            .map_err(internal_error)?
    };
    Ok((
        cors_headers(),
        Json(regression_response(json!({"artifacts": artifacts}))),
    ))
}

pub(crate) async fn api_regression_artifact_detail(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    Path(artifact_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let artifact = store
        .get_regression_report_artifact(&artifact_id)
        .map_err(internal_error)?
        .ok_or_else(|| {
            ApiError::with_code(
                StatusCode::NOT_FOUND,
                "regression_artifact_not_found",
                "regression artifact not found",
            )
        })?;
    Ok((
        cors_headers(),
        Json(regression_response(json!({"artifact": artifact}))),
    ))
}

pub(crate) async fn api_regression_trend(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    Path(scenario_id): Path<String>,
    Query(query): Query<RegressionTrendQuery>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let trend = store
        .regression_report_trend(&scenario_id, query.limit.unwrap_or(50))
        .map_err(internal_error)?;
    Ok((
        cors_headers(),
        Json(regression_response(json!({"trend": trend}))),
    ))
}

fn regression_response(payload: serde_json::Value) -> serde_json::Value {
    let mut response = json!({
        "metadata_only": true,
        "read_only": true,
        "report_only": true,
        "provider_calls": "disabled",
        "mutation_authority": "none",
        "target_repository_writes": "disabled",
    });
    response
        .as_object_mut()
        .expect("regression response is object")
        .extend(payload.as_object().cloned().unwrap_or_default());
    response
}

pub(crate) async fn api_budget_evidence_artifacts(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    Query(query): Query<BudgetEvidenceQuery>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let kind = query.kind.as_deref();
    if kind.is_some_and(|value| !matches!(value, "forecast" | "anomaly")) {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "invalid_budget_evidence_query",
            "kind must be forecast or anomaly",
        ));
    }
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let offset = query.offset.unwrap_or(0).clamp(0, 10_000);
    let artifacts = store
        .budget_evidence_artifacts(kind, limit, offset)
        .map_err(internal_error)?;
    Ok((
        cors_headers(),
        Json(regression_response(json!({
            "artifacts": artifacts,
            "kind": kind,
            "limit": limit,
            "offset": offset,
        }))),
    ))
}

pub(crate) async fn api_offline_replay_artifacts(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    Query(query): Query<OfflineReplayQuery>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let status = query.status.as_deref();
    if status.is_some_and(|value| {
        !matches!(
            value,
            "sufficient"
                | "insufficient_evidence"
                | "incompatible_cohort"
                | "stale_evidence"
                | "tampered_evidence"
                | "uncalibrated_evidence"
                | "out_of_distribution"
        )
    }) {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "invalid_offline_replay_status",
            "status is not a supported offline replay status",
        ));
    }
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let offset = query.offset.unwrap_or(0).clamp(0, 10_000);
    let artifacts = store
        .offline_replay_artifacts(status, limit, offset)
        .map_err(internal_error)?;
    Ok((
        cors_headers(),
        Json(regression_response(json!({
            "schema_version": "offline_replay_read.v1",
            "artifacts": artifacts,
            "status": status,
            "limit": limit,
            "offset": offset,
            "empty": artifacts.is_empty(),
            "read_only": true,
            "metadata_only": true,
            "mutation_authority": "none",
        }))),
    ))
}

pub(crate) async fn api_offline_replay_artifact_detail(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    Path(artifact_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let artifact = store
        .get_offline_replay_artifact(&artifact_id)
        .map_err(internal_error)?
        .ok_or_else(|| {
            ApiError::with_code(
                StatusCode::NOT_FOUND,
                "offline_replay_artifact_not_found",
                "offline replay artifact not found",
            )
        })?;
    Ok((
        cors_headers(),
        Json(regression_response(json!({
            "schema_version": "offline_replay_read.v1",
            "artifact": artifact,
            "read_only": true,
            "metadata_only": true,
            "mutation_authority": "none",
        }))),
    ))
}

pub(crate) async fn api_budget_evidence_artifact_detail(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    Path(artifact_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let artifact = store
        .get_budget_evidence_artifact(&artifact_id)
        .map_err(internal_error)?
        .ok_or_else(|| {
            ApiError::with_code(
                StatusCode::NOT_FOUND,
                "budget_evidence_artifact_not_found",
                "budget evidence artifact not found",
            )
        })?;
    Ok((
        cors_headers(),
        Json(regression_response(json!({"artifact": artifact}))),
    ))
}

pub(crate) async fn api_apply_budget_auto_pause(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    Path(artifact_id): Path<String>,
    Json(request): Json<BudgetAutoPauseRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    if !request.confirm_auto_pause {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "budget_auto_pause_confirmation_required",
            "confirm_auto_pause must be true",
        ));
    }
    let decision = require_store(&state)?
        .apply_budget_auto_pause(
            &artifact_id,
            &request.run_id,
            &request.policy,
            &context.api_key_id,
        )
        .map_err(|error| {
            ApiError::with_code(StatusCode::CONFLICT, "budget_auto_pause_rejected", error)
        })?;
    Ok((
        cors_headers(),
        Json(json!({"schema_version": "axum_api.v1", "decision": decision})),
    ))
}

pub(crate) async fn api_recover_budget_auto_pause(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    Path(run_id): Path<String>,
    Json(request): Json<BudgetPauseRecoveryRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    if !request.confirm_recovery {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "budget_pause_recovery_confirmation_required",
            "confirm_recovery must be true",
        ));
    }
    let decision = require_store(&state)?
        .recover_budget_auto_pause(
            &run_id,
            &request.recovery,
            &request.reason,
            &context.api_key_id,
        )
        .map_err(|error| {
            ApiError::with_code(
                StatusCode::CONFLICT,
                "budget_pause_recovery_rejected",
                error,
            )
        })?;
    Ok((
        cors_headers(),
        Json(json!({"schema_version": "axum_api.v1", "decision": decision})),
    ))
}
