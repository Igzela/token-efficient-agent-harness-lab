use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, require_store, ApiError,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::AXUM_API_SCHEMA_VERSION;

pub(crate) async fn api_supervised_patch_workspaces(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read")?;
    let store = require_store(&state)?;
    let limit = query_i64(&params, "limit", 100).clamp(0, 500);
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "metadata_only": true,
            "execution_authority": "disabled",
            "workspaces": store.supervised_patch_workspaces(limit).map_err(internal_error)?,
        })),
    ))
}

pub(crate) async fn api_supervised_patch_workspace_detail(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    AxumPath(workspace_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read")?;
    let store = require_store(&state)?;
    match store
        .get_supervised_patch_workspace(&workspace_id)
        .map_err(internal_error)?
    {
        Some(workspace) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "metadata_only": true,
                "execution_authority": "disabled",
                "workspace": workspace,
            })),
        )),
        None => Err(not_found("supervised_patch_workspace_not_found")),
    }
}

pub(crate) async fn api_supervised_patch_artifacts(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read")?;
    let store = require_store(&state)?;
    let limit = query_i64(&params, "limit", 100).clamp(0, 500);
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "metadata_only": true,
            "execution_authority": "disabled",
            "artifacts": store.supervised_patch_artifacts(limit).map_err(internal_error)?,
        })),
    ))
}

pub(crate) async fn api_supervised_patch_artifact_detail(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    AxumPath(artifact_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read")?;
    let store = require_store(&state)?;
    match store
        .get_supervised_patch_artifact(&artifact_id)
        .map_err(internal_error)?
    {
        Some(artifact) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "metadata_only": true,
                "execution_authority": "disabled",
                "artifact": artifact,
            })),
        )),
        None => Err(not_found("supervised_patch_artifact_not_found")),
    }
}

fn query_i64(params: &std::collections::HashMap<String, String>, key: &str, default: i64) -> i64 {
    params
        .get(key)
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(default)
}

fn not_found(code: &str) -> ApiError {
    ApiError::with_code(
        StatusCode::NOT_FOUND,
        code,
        "supervised patch metadata not found",
    )
}
