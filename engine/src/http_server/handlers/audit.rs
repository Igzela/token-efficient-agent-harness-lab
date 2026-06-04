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
use crate::provider::redaction::redact_audit_fields;

pub(crate) async fn api_audit(
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
    let search = params.get("search").map(String::as_str);
    let redact = params
        .get("redact")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let mut events = store
        .search_audit_events(limit, offset, search)
        .map_err(internal_error)?;
    if redact {
        for event in &mut events {
            if let Some(details) = event.get_mut("details") {
                *details = redact_audit_fields(details);
            }
        }
    }
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "redacted": redact,
            "events": events,
        })),
    ))
}
