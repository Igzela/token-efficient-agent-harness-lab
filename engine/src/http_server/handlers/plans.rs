use axum::extract::{Extension, Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, require_store, ApiError, RequestId,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::{ReadOnlyPlanApiRequest, AXUM_API_SCHEMA_VERSION};
use crate::provider::adaptive_observation::AdaptiveNodeExecutionConfig;
use crate::read_only_planner::ReadOnlyPlanner;

pub(crate) async fn api_create_plan(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Json(request): Json<ReadOnlyPlanApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let required_scope = if request.adaptive_execution.is_some() {
        "dispatch:execute"
    } else {
        "dispatch:read"
    };
    let context = authorize(&state, &headers, required_scope, uri.path(), &request_id.0)?;
    if request.raw_request.trim().is_empty() {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "raw_request_required",
            "raw_request is required",
        ));
    }
    if request.adaptive_execution.is_some() && request.confirm_adaptive_execution_plan != Some(true)
    {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "adaptive_execution_confirmation_required",
            "confirm_adaptive_execution_plan must be true",
        ));
    }

    let store = require_store(&state)?;
    let request_source = request.request_source.as_deref().unwrap_or("api");
    let adaptive_execution = if let Some(value) = request.adaptive_execution.clone() {
        serde_json::from_value::<AdaptiveNodeExecutionConfig>(value.clone()).map_err(|_| {
            ApiError::with_code(
                StatusCode::BAD_REQUEST,
                "invalid_adaptive_execution",
                "adaptive_execution must contain a valid plan and limits",
            )
        })?;
        Some(value)
    } else {
        None
    };
    let planner = ReadOnlyPlanner::new();
    let plan = store
        .create_workflow_plan(
            &request.raw_request,
            request_source,
            &context.api_key_id,
            |ids, created_at| {
                if let Some(adaptive_execution) = adaptive_execution {
                    Ok(adaptive_execution_plan(
                        ids,
                        &request.raw_request,
                        request_source,
                        created_at,
                        adaptive_execution,
                    ))
                } else {
                    planner.create_plan(ids, &request.raw_request, request_source, created_at)
                }
            },
        )
        .map_err(internal_error)?;

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "plan": plan,
        })),
    ))
}

fn adaptive_execution_plan(
    ids: &crate::read_only_planner::WorkflowPlanIds,
    raw_request: &str,
    request_source: &str,
    created_at: &str,
    adaptive_execution: serde_json::Value,
) -> serde_json::Value {
    json!({
        "schema_version": "read_only_plan.v1",
        "plan_id": ids.plan_id,
        "status": "planned_read_only",
        "workflow_id": ids.workflow_id,
        "dispatch_id": ids.dispatch_id,
        "analysis": {
            "analysis_id": format!("analysis-{}", ids.dispatch_id),
            "task_domain": "adaptive",
            "request_source": request_source,
            "raw_request_snapshot": raw_request,
        },
        "graph": {
            "schema_version": "workflow_graph.v1",
            "workflow_id": ids.workflow_id,
            "dispatch_id": ids.dispatch_id,
            "status": "decomposed",
            "created_at": created_at,
            "updated_at": created_at,
            "nodes": [{
                "node_id": "adaptive-node-1",
                "task_type": "implementation",
                "status": "pending",
                "adaptive_execution": adaptive_execution,
            }],
            "edges": [],
        },
        "boundaries": {
            "execution": "explicit_tick_only",
            "execution_authority": "explicit_tick_only",
            "target_repository_writes": "disabled",
            "runtime_workers": "env_gated_supervised",
        },
        "advisory": {
            "schema_version": "plan_advisory.v1",
            "mode": "explicit_adaptive_execution_plan",
            "requires_executor": "adaptive_provider",
        },
    })
}

pub(crate) async fn api_plans(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
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
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "plans": store.search_workflow_plans(limit, offset, search).map_err(internal_error)?,
        })),
    ))
}

pub(crate) async fn api_plan_detail(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(plan_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    match store.get_workflow_plan(&plan_id).map_err(internal_error)? {
        Some(plan) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "plan": plan,
            })),
        )),
        None => Err(ApiError::with_code(
            StatusCode::NOT_FOUND,
            "plan_not_found",
            "plan not found",
        )),
    }
}
