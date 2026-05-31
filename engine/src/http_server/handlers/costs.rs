use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, require_store, ApiError,
};
use crate::http_server::state::AxumApiState;

pub(crate) async fn api_costs(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "cost:read")?;
    let store = require_store(&state)?;
    Ok((
        cors_headers(),
        Json(store.cost_summary().map_err(internal_error)?),
    ))
}

pub(crate) async fn api_cost_details(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "cost:read")?;
    let store = require_store(&state)?;
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(50)
        .clamp(0, 500);
    Ok((
        cors_headers(),
        Json(store.dispatch_cost_details(limit).map_err(internal_error)?),
    ))
}
