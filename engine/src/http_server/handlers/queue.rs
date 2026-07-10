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

fn not_found() -> ApiError {
    ApiError::with_code(StatusCode::NOT_FOUND, "not_found", "workflow run not found")
}

#[derive(Debug, Deserialize)]
pub(crate) struct QueueRunsQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PriorityUpdateRequest {
    pub priority: i32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PauseUpdateRequest {
    pub reason: Option<String>,
}

pub(crate) async fn api_queue_status(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "health:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let queue_status = store.get_queue_status().map_err(internal_error)?;

    let (
        backpressure_active,
        effective_concurrency,
        total_active,
        total_capacity,
        max_queued,
        backpressure_enabled,
        backpressure_activation,
    ) = match &state.scheduler {
        Some(scheduler) => {
            let guard = scheduler
                .lock()
                .map_err(|e| internal_error(format!("scheduler lock: {e}")))?;
            let scheduler_status = guard.status();
            let config = scheduler_status.get("config").and_then(|v| v.as_object());
            let pool = guard.executor_pool();
            let ta = pool.total_active();
            let tc = pool.total_capacity();
            (
                scheduler_status
                    .get("backpressure_active")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(ta >= tc),
                tc,
                ta,
                tc,
                config
                    .and_then(|c| c.get("max_queued"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                config
                    .and_then(|c| c.get("backpressure_enabled"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                config
                    .and_then(|c| c.get("backpressure_activation"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0),
            )
        }
        None => (false, 0, 0, 0, 0, false, 0.0),
    };

    let total_queued = queue_status
        .get("total_queued")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as usize;
    let capacity_utilization = if total_capacity > 0 {
        total_active as f64 / total_capacity as f64
    } else {
        0.0
    };
    let queue_depth_ratio = if total_capacity > 0 {
        total_queued as f64 / total_capacity as f64
    } else {
        0.0
    };

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "tenant_id": context.tenant_id,
            "request_id": context.request_id,
            "queue": {
                "total_queued": queue_status.get("total_queued"),
                "total_running": queue_status.get("total_running"),
                "total_paused": queue_status.get("total_paused"),
                "total_completed": queue_status.get("total_completed"),
                "total_failed": queue_status.get("total_failed"),
                "avg_priority": queue_status.get("avg_priority"),
                "overdue_count": queue_status.get("overdue_count"),
                "capacity_utilization": capacity_utilization,
                "queue_depth_ratio": queue_depth_ratio,
                "backpressure_active": backpressure_active,
                "effective_concurrency": effective_concurrency,
                "queue_config": {
                    "max_concurrent": total_capacity,
                    "max_queued": max_queued,
                    "backpressure_enabled": backpressure_enabled,
                    "backpressure_activation": backpressure_activation,
                },
            },
        })),
    ))
}

pub(crate) async fn api_queue_runs(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Query(params): Query<QueueRunsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "health:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let limit = params.limit.unwrap_or(50).min(500);
    let offset = params.offset.unwrap_or(0);
    let db_offset = i64::try_from(offset).unwrap_or(i64::MAX);
    let page = store
        .list_active_workflow_runs_prioritized_page(limit as i64, db_offset)
        .map_err(internal_error)?;
    let total = store.count_active_workflow_runs().map_err(internal_error)?;

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "tenant_id": context.tenant_id,
            "request_id": context.request_id,
            "runs": page,
            "total": total,
            "limit": limit,
            "offset": offset,
        })),
    ))
}

pub(crate) async fn api_update_run_priority(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(run_id): AxumPath<String>,
    Json(request): Json<PriorityUpdateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:write",
        uri.path(),
        &request_id.0,
    )?;
    if request.priority < 1 || request.priority > 10 {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "invalid_priority",
            "priority must be between 1 and 10",
        ));
    }
    let store = require_store(&state)?;
    match store.update_run_priority(&run_id, request.priority as i64) {
        Ok(()) => {}
        Err(e) if e.starts_with("workflow run not found:") => return Err(not_found()),
        Err(e) => return Err(internal_error(e)),
    }

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "tenant_id": context.tenant_id,
            "request_id": context.request_id,
            "ok": true,
            "run_id": run_id,
            "priority": request.priority,
        })),
    ))
}

pub(crate) async fn api_update_run_pause(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(run_id): AxumPath<String>,
    Json(request): Json<PauseUpdateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:write",
        uri.path(),
        &request_id.0,
    )?;
    let store = require_store(&state)?;
    match store.update_run_pause_reason(&run_id, request.reason.as_deref()) {
        Ok(()) => {}
        Err(e) if e.starts_with("workflow run not found:") => return Err(not_found()),
        Err(e) => return Err(internal_error(e)),
    }

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "tenant_id": context.tenant_id,
            "request_id": context.request_id,
            "ok": true,
            "run_id": run_id,
            "pause_reason": request.reason,
        })),
    ))
}

pub(crate) async fn api_queue_tenants(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "health:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let tenants = store.list_tenants_with_quota().map_err(internal_error)?;

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "tenant_id": context.tenant_id,
            "request_id": context.request_id,
            "tenants": tenants,
        })),
    ))
}
