use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, require_store, ApiError,
};
use crate::http_server::state::AxumApiState;
use crate::provider::config::provider_pricing_from_env;

pub(crate) async fn api_costs(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "cost:read")?;
    let store = require_store(&state)?;
    let mut costs = store.cost_summary().map_err(internal_error)?;
    if let Some(obj) = costs.as_object_mut() {
        obj.insert(
            "pricing_configured".to_string(),
            serde_json::json!(provider_pricing_from_env().configured()),
        );
    }
    Ok((cors_headers(), Json(costs)))
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
