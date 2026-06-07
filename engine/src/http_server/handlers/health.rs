use axum::extract::{Extension, State};
use axum::http::{HeaderMap, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::http_server::middleware::{authorize, cors_headers, ApiError, RequestId};
use crate::http_server::state::AxumApiState;
use crate::http_server::{openapi_document, AXUM_API_SCHEMA_VERSION};

pub(crate) async fn api_health(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "health:read", uri.path(), &request_id.0)?;

    // Deep health: check DB connectivity
    let db_ok = state.local_store.as_ref().map_or(true, |store| {
        store.check_integrity().map_or(false, |r| r.status == "ok")
    });

    // Deep health: check scheduler liveness (last tick within 30s if enabled)
    let scheduler_ok = state.scheduler.as_ref().map_or(true, |sched| {
        if let Ok(guard) = sched.lock() {
            let status = guard.status();
            if let Some(last_tick) = status.get("last_tick_at").and_then(|v| v.as_str()) {
                // Parse timestamp and check if within 30 seconds
                chrono::NaiveDateTime::parse_from_str(last_tick, "%Y-%m-%dT%H:%M:%SZ")
                    .ok()
                    .map(|t| {
                        let now = chrono::Utc::now().naive_utc();
                        let diff = now.signed_duration_since(t);
                        diff.num_seconds() < 30
                    })
                    .unwrap_or(false)
            } else {
                // No tick yet — scheduler just started, consider ok
                true
            }
        } else {
            false
        }
    });

    let overall = if db_ok && scheduler_ok {
        "healthy"
    } else if db_ok {
        "degraded"
    } else {
        "unhealthy"
    };

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "status": overall,
            "checks": {
                "db": if db_ok { "ok" } else { "error" },
                "scheduler": if scheduler_ok { "ok" } else { "stale" },
            },
            "tenant_id": context.tenant_id,
            "request_id": context.request_id,
        })),
    ))
}

pub(crate) async fn api_ready(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "health:read", uri.path(), &request_id.0)?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "status": "ready",
            "tenant_id": context.tenant_id,
            "request_id": context.request_id,
        })),
    ))
}

pub(crate) async fn api_openapi(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "health:read", uri.path(), &request_id.0)?;
    Ok((cors_headers(), Json(openapi_document())))
}
