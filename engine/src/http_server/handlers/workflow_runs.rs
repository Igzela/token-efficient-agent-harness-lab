use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, require_store, ApiError,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::{
    WorkflowRunActionApiRequest, WorkflowRunApprovalApiRequest, WorkflowRunCreateApiRequest,
    WorkflowRunEventApiRequest, AXUM_API_SCHEMA_VERSION,
};

pub(crate) async fn api_create_workflow_run(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    Json(request): Json<WorkflowRunCreateApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read")?;
    if request.plan_id.trim().is_empty() {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "plan_id_required",
            "plan_id is required",
        ));
    }
    let store = require_store(&state)?;
    match store.create_workflow_run_from_plan(&request.plan_id, &context.api_key_id) {
        Ok(run) => Ok((cors_headers(), Json(json_response("run", run)))),
        Err(e) if e.starts_with("plan not found:") => Err(ApiError::with_code(
            StatusCode::NOT_FOUND,
            "plan_not_found",
            "plan not found",
        )),
        Err(e) => Err(internal_error(e)),
    }
}

pub(crate) async fn api_workflow_runs(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read")?;
    let store = require_store(&state)?;
    let limit = query_i64(&params, "limit", 100).clamp(0, 500);
    let offset = query_i64(&params, "offset", 0).max(0);
    let search = params.get("search").map(String::as_str);
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "runs": store.search_workflow_runs(limit, offset, search).map_err(internal_error)?,
        })),
    ))
}

pub(crate) async fn api_workflow_run_detail(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    AxumPath(run_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read")?;
    let store = require_store(&state)?;
    match store.get_workflow_run(&run_id).map_err(internal_error)? {
        Some(run) => Ok((cors_headers(), Json(json_response("run", run)))),
        None => Err(not_found()),
    }
}

pub(crate) async fn api_create_workflow_run_event(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    AxumPath(run_id): AxumPath<String>,
    Json(request): Json<WorkflowRunEventApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read")?;
    if request.event_type.trim().is_empty() {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "event_type_required",
            "event_type is required",
        ));
    }
    let store = require_store(&state)?;
    match store.append_workflow_run_event(
        &run_id,
        request.node_id.as_deref(),
        &request.event_type,
        &request.details.unwrap_or(serde_json::Value::Null),
        &context.api_key_id,
    ) {
        Ok(event) => Ok((cors_headers(), Json(json_response("event", event)))),
        Err(e) if e.starts_with("workflow run not found:") => Err(not_found()),
        Err(e) => Err(internal_error(e)),
    }
}

pub(crate) async fn api_workflow_run_events(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    AxumPath(run_id): AxumPath<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read")?;
    let store = require_store(&state)?;
    let limit = query_i64(&params, "limit", 100).clamp(0, 500);
    match store.workflow_run_events(&run_id, limit) {
        Ok(events) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "events": events,
            })),
        )),
        Err(e) if e.starts_with("workflow run not found:") => Err(not_found()),
        Err(e) => Err(internal_error(e)),
    }
}

pub(crate) async fn api_create_workflow_run_approval(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    AxumPath(run_id): AxumPath<String>,
    Json(request): Json<WorkflowRunApprovalApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read")?;
    if request.node_id.trim().is_empty() {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "node_id_required",
            "node_id is required",
        ));
    }
    let store = require_store(&state)?;
    match store.record_workflow_run_approval(
        &run_id,
        &request.node_id,
        &request.decision,
        &context.api_key_id,
        request.reason.as_deref(),
    ) {
        Ok(approval) => Ok((cors_headers(), Json(json_response("approval", approval)))),
        Err(e) if e.starts_with("workflow run not found:") => Err(not_found()),
        Err(e) if e.starts_with("invalid workflow approval decision:") => Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "invalid_approval_decision",
            "invalid approval decision",
        )),
        Err(e) => Err(internal_error(e)),
    }
}

pub(crate) async fn api_workflow_run_approvals(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    AxumPath(run_id): AxumPath<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read")?;
    let store = require_store(&state)?;
    let limit = query_i64(&params, "limit", 100).clamp(0, 500);
    match store.workflow_run_approvals(&run_id, limit) {
        Ok(approvals) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "approvals": approvals,
            })),
        )),
        Err(e) if e.starts_with("workflow run not found:") => Err(not_found()),
        Err(e) => Err(internal_error(e)),
    }
}

pub(crate) async fn api_resume_workflow_run(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    AxumPath(run_id): AxumPath<String>,
    Json(request): Json<WorkflowRunActionApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read")?;
    let store = require_store(&state)?;
    match store.request_workflow_run_resume(&run_id, &context.api_key_id, request.reason.as_deref())
    {
        Ok(run) => Ok((cors_headers(), Json(json_response("run", run)))),
        Err(e) if e.starts_with("workflow run not found:") => Err(not_found()),
        Err(e) => Err(internal_error(e)),
    }
}

pub(crate) async fn api_cancel_workflow_run(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    AxumPath(run_id): AxumPath<String>,
    Json(request): Json<WorkflowRunActionApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read")?;
    let store = require_store(&state)?;
    match store.request_workflow_run_cancel(&run_id, &context.api_key_id, request.reason.as_deref())
    {
        Ok(run) => Ok((cors_headers(), Json(json_response("run", run)))),
        Err(e) if e.starts_with("workflow run not found:") => Err(not_found()),
        Err(e) => Err(internal_error(e)),
    }
}

fn query_i64(params: &std::collections::HashMap<String, String>, key: &str, default: i64) -> i64 {
    params
        .get(key)
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(default)
}

fn json_response(key: &str, value: serde_json::Value) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert(
        "schema_version".to_string(),
        serde_json::Value::String(AXUM_API_SCHEMA_VERSION.to_string()),
    );
    map.insert(key.to_string(), value);
    serde_json::Value::Object(map)
}

fn not_found() -> ApiError {
    ApiError::with_code(
        StatusCode::NOT_FOUND,
        "workflow_run_not_found",
        "workflow run not found",
    )
}
