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
