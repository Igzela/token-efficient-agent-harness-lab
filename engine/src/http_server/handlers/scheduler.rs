use axum::extract::{Extension, State};
use axum::http::{HeaderMap, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::http_server::middleware::{authorize, cors_headers, ApiError, RequestId};
use crate::http_server::state::AxumApiState;
use crate::http_server::AXUM_API_SCHEMA_VERSION;

pub(crate) async fn api_scheduler_status(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "health:read", uri.path(), &request_id.0)?;
    let status = match &state.scheduler {
        Some(scheduler) => {
            let guard = scheduler.lock().map_err(|e| {
                crate::http_server::middleware::internal_error(format!("scheduler lock: {e}"))
            })?;
            guard.status()
        }
        None => {
            json!({
                "schema_version": "scheduler.v1",
                "running": false,
                "enabled": false,
                "message": "scheduler not enabled (set ACP_ENABLE_SCHEDULER=1)",
            })
        }
    };
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "tenant_id": context.tenant_id,
            "request_id": context.request_id,
            "scheduler": status,
        })),
    ))
}
