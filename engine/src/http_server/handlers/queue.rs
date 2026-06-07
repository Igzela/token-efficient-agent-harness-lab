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

    let (backpressure_active, effective_concurrency) = match &state.scheduler {
        Some(scheduler) => {
            let guard = scheduler
                .lock()
                .map_err(|e| internal_error(format!("scheduler lock: {e}")))?;
            let pool = guard.executor_pool();
            (
                pool.total_active() >= pool.total_capacity(),
                pool.total_capacity(),
            )
        }
        None => (false, 0),
    };

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "tenant_id": context.tenant_id,
            "request_id": context.request_id,
            "queue_status": queue_status,
            "backpressure_active": backpressure_active,
            "effective_concurrency": effective_concurrency,
            "queue_config": {
                "max_priority": 10,
                "min_priority": 1,
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
    let all_runs = store
        .list_active_workflow_runs_prioritized()
        .map_err(internal_error)?;

    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);
    let total = all_runs.len();
    let page: Vec<_> = all_runs.into_iter().skip(offset).take(limit).collect();

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
    let context = authorize(&state, &headers, "health:read", uri.path(), &request_id.0)?;
    if request.priority < 1 || request.priority > 10 {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "invalid_priority",
            "priority must be between 1 and 10",
        ));
    }
    let store = require_store(&state)?;
    store
        .update_run_priority(&run_id, request.priority as i64)
        .map_err(internal_error)?;

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
    let context = authorize(&state, &headers, "health:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    store
        .update_run_pause_reason(&run_id, request.reason.as_deref())
        .map_err(internal_error)?;

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
