use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::http_server::middleware::{authorize, cors_headers, ApiError};
use crate::http_server::state::AxumApiState;
use crate::http_server::{openapi_document, AXUM_API_SCHEMA_VERSION};

pub(crate) async fn api_health(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "health:read")?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "status": "healthy",
            "tenant_id": context.tenant_id,
        })),
    ))
}

pub(crate) async fn api_ready(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "health:read")?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "status": "ready",
            "tenant_id": context.tenant_id,
        })),
    ))
}

pub(crate) async fn api_openapi(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "health:read")?;
    Ok((cors_headers(), Json(openapi_document())))
}
