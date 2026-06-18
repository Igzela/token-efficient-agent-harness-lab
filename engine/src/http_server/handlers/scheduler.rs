use axum::extract::{Extension, State};
use axum::http::{HeaderMap, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, ApiError, RequestId,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::AXUM_API_SCHEMA_VERSION;

#[derive(Debug, Deserialize)]
pub(crate) struct SchedulerControlRequest {
    action: String,
    actor: Option<String>,
    confirm_control: Option<bool>,
}

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

pub(crate) async fn api_scheduler_control(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Json(request): Json<SchedulerControlRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    let store = state.local_store.as_ref().ok_or_else(|| {
        ApiError::with_code(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "local_store_unavailable",
            "local product store is not configured",
        )
    })?;
    let action = request.action.trim().to_ascii_lowercase();
    if request.confirm_control != Some(true) {
        let _ = store.append_audit(
            &context.api_key_id,
            "scheduler.control.denied",
            "supervised-workers",
            &json!({
                "reason": "confirmation_required",
                "requested_action": action,
                "requested_actor": request.actor,
            }),
        );
        return Err(ApiError::with_code(
            axum::http::StatusCode::BAD_REQUEST,
            "scheduler_control_confirmation_required",
            "confirm_control=true is required",
        ));
    }
    if !matches!(action.as_str(), "pause" | "resume" | "kill") {
        return Err(ApiError::with_code(
            axum::http::StatusCode::BAD_REQUEST,
            "invalid_scheduler_control_action",
            "action must be pause, resume, or kill",
        ));
    }
    let scheduler = state.scheduler.as_ref().ok_or_else(|| {
        ApiError::with_code(
            axum::http::StatusCode::CONFLICT,
            "scheduler_not_enabled",
            "scheduler is not enabled",
        )
    })?;
    let mut guard = scheduler
        .lock()
        .map_err(|error| internal_error(format!("scheduler lock: {error}")))?;
    let result = match action.as_str() {
        "pause" => guard.pause(&context.api_key_id),
        "resume" => guard.resume(&context.api_key_id),
        "kill" => guard.kill(&context.api_key_id),
        _ => unreachable!(),
    };
    if let Err(error) = result {
        let _ = store.append_audit(
            &context.api_key_id,
            "scheduler.control.failed",
            "supervised-workers",
            &json!({
                "requested_action": action,
                "requested_actor": request.actor,
                "error": error,
            }),
        );
        return Err(ApiError::with_code(
            axum::http::StatusCode::CONFLICT,
            "scheduler_control_failed",
            &error,
        ));
    }
    let status = guard.status();
    let _ = store.append_audit(
        &context.api_key_id,
        &format!("scheduler.control.{action}"),
        "supervised-workers",
        &json!({
            "requested_actor": request.actor,
            "worker_count": status["worker_count"],
            "running": status["running"],
            "paused": status["paused"],
            "kill_requested": status["kill_requested"],
        }),
    );
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
