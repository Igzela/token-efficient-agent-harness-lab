use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, require_store, ApiError,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::AXUM_API_SCHEMA_VERSION;

pub(crate) async fn api_provider_health(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "health:read")?;
    let Some(provider) = &state.provider else {
        return Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "status": "noop",
                "message": "no provider configured",
            })),
        ));
    };
    let enabled = provider.is_enabled();
    let provider_id = provider.provider_id();
    if enabled {
        Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "status": "ok",
                "provider_id": provider_id,
                "enabled": true,
            })),
        ))
    } else {
        Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "status": "error",
                "provider_id": provider_id,
                "enabled": false,
                "message": "provider is disabled",
            })),
        ))
    }
}

pub(crate) async fn api_provider_audit(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "audit:read")?;
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
    let events = store
        .provider_audit_events_with_offset(limit, offset)
        .map_err(internal_error)?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "events": events,
        })),
    ))
}
