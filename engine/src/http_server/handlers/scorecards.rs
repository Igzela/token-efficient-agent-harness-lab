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
    let artifacts = store
        .budget_evidence_artifacts(kind, query.limit.unwrap_or(50))
        .map_err(internal_error)?;
    Ok((
        cors_headers(),
        Json(regression_response(json!({"artifacts": artifacts}))),
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
