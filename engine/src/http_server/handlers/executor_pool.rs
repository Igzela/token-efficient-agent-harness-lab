use axum::extract::{Extension, State};
use axum::http::{HeaderMap, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::executor_pool::EXECUTOR_POOL_SCHEMA_VERSION;
use crate::http_server::middleware::{authorize, cors_headers, ApiError, RequestId};
use crate::http_server::state::AxumApiState;
use crate::http_server::AXUM_API_SCHEMA_VERSION;

pub(crate) async fn api_executor_pool_status(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "health:read", uri.path(), &request_id.0)?;
    let pool_status = match &state.scheduler {
        Some(scheduler) => {
            let guard = scheduler.lock().map_err(|e| {
                crate::http_server::middleware::internal_error(format!("scheduler lock: {e}"))
            })?;
            let pool = guard.executor_pool();
            let snapshot = pool.snapshot();
            let total_active = pool.total_active();
            let total_capacity = pool.total_capacity();
            json!({
                "schema_version": EXECUTOR_POOL_SCHEMA_VERSION,
                "executors": snapshot,
                "total_active": total_active,
                "total_capacity": total_capacity,
            })
        }
        None => {
            json!({
                "schema_version": EXECUTOR_POOL_SCHEMA_VERSION,
                "executors": [],
                "total_active": 0,
                "total_capacity": 0,
            })
        }
    };
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "tenant_id": context.tenant_id,
            "request_id": context.request_id,
            "executor_pool": pool_status,
        })),
    ))
}
