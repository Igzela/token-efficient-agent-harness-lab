use axum::extract::{Extension, Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, require_store, ApiError, RequestId,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::AXUM_API_SCHEMA_VERSION;

#[derive(Debug, Deserialize)]
pub(crate) struct DecisionsQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub search: Option<String>,
    pub run_id: Option<String>,
}

pub(crate) async fn api_decisions(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Query(params): Query<DecisionsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;

    let limit = params.limit.unwrap_or(100).min(500) as i64;
    let offset = i64::try_from(params.offset.unwrap_or(0)).unwrap_or(i64::MAX);

    let decisions = if let Some(ref run_id) = params.run_id {
        store
            .get_decisions_for_run_with_offset(run_id, limit, offset)
            .map_err(internal_error)?
    } else {
        store
            .search_decisions(limit, offset, params.search.as_deref())
            .map_err(internal_error)?
    };
    let total = store
        .count_decisions(params.run_id.as_deref(), params.search.as_deref())
        .map_err(internal_error)?;

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "tenant_id": context.tenant_id,
            "request_id": context.request_id,
            "decisions": decisions.iter().map(|d| d.to_value()).collect::<Vec<_>>(),
            "total": total,
            "limit": limit,
            "offset": offset,
        })),
    ))
}

pub(crate) async fn api_decision_detail(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(decision_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;

    let decision = store
        .get_decision_by_id(&decision_id)
        .map_err(internal_error)?;

    match decision {
        Some(record) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "tenant_id": context.tenant_id,
                "request_id": context.request_id,
                "decision": record.to_value(),
            })),
        )
            .into_response()),
        None => Err(ApiError::with_code(
            StatusCode::NOT_FOUND,
            "decision_not_found",
            format!("decision {decision_id} not found"),
        )),
    }
}

pub(crate) async fn api_decision_stats(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;

    let stats = store.decision_log_stats().map_err(internal_error)?;

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "tenant_id": context.tenant_id,
            "request_id": context.request_id,
            "stats": {
                "total_decisions": stats.total_decisions,
                "by_action": stats.by_action,
                "avg_confidence": stats.avg_confidence,
            },
        })),
    ))
}
