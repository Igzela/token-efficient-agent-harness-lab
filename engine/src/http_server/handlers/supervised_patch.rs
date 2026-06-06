use axum::extract::{Extension, Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, require_store, ApiError, RequestId,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::{SupervisedPatchWorkspaceCreateRequest, AXUM_API_SCHEMA_VERSION};

pub(crate) async fn api_supervised_patch_workspaces(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
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
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(workspace_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
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
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
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
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(artifact_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
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

fn uuid_v4_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:016x}", t)
}

fn not_found(code: &str) -> ApiError {
    ApiError::with_code(
        StatusCode::NOT_FOUND,
        code,
        "supervised patch metadata not found",
    )
}

pub(crate) async fn api_export_supervised_patch(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(artifact_id): AxumPath<String>,
    Json(request): Json<serde_json::Value>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;

    let run_id = request.get("run_id").and_then(|v| v.as_str()).unwrap_or("");

    let binding = store
        .validate_approval_binding(run_id, &artifact_id)
        .map_err(internal_error)?;

    if !binding["export_eligible"].as_bool().unwrap_or(false) {
        return Err(ApiError::with_code(
            StatusCode::FORBIDDEN,
            "export_not_approved",
            "artifact export requires valid approval binding",
        ));
    }

    let artifact = store
        .get_supervised_patch_artifact(&artifact_id)
        .map_err(internal_error)?
        .ok_or_else(|| not_found("artifact_not_found"))?;

    let integrity = store
        .validate_artifact_integrity(&artifact_id)
        .map_err(internal_error)?;

    if !integrity["integrity_ok"].as_bool().unwrap_or(false) {
        return Err(ApiError::with_code(
            StatusCode::CONFLICT,
            "integrity_check_failed",
            "artifact integrity check failed",
        ));
    }

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "export": {
                "artifact_id": artifact_id,
                "artifact": artifact,
                "approval_binding": binding,
                "integrity": integrity,
                "exported_by": context.api_key_id,
                "exported_at": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            },
        })),
    ))
}

pub(crate) async fn api_create_supervised_patch_workspace(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Json(request): Json<SupervisedPatchWorkspaceCreateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;

    let unique_id = format!(
        "ws-{}-{}",
        chrono::Utc::now().timestamp_millis(),
        &uuid_v4_simple()
    );
    let workspace_dir = store
        .create_workspace_directory(&unique_id, &request.target_repo_path)
        .map_err(internal_error)?;

    let workspace_request = json!({
        "run_id": request.run_id,
        "plan_id": request.plan_id,
        "target_id": request.target_id,
        "target_repo_path": request.target_repo_path,
        "workspace_path": workspace_dir,
        "source_revision": request.source_revision,
        "source_tree_hash": request.source_tree_hash,
        "status": "workspace_created",
    });

    match store.record_supervised_patch_workspace(&workspace_request, &context.api_key_id) {
        Ok(workspace) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "workspace": workspace,
            })),
        )),
        Err(e) => Err(internal_error(e)),
    }
}

pub(crate) async fn api_cleanup_supervised_patch_workspace(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(workspace_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    match store.cleanup_workspace(&workspace_id, &context.api_key_id) {
        Ok(workspace) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "workspace": workspace,
            })),
        )),
        Err(e) if e.contains("not found") => Err(not_found("workspace_not_found")),
        Err(e) if e.contains("invalid") => Err(ApiError::with_code(
            StatusCode::CONFLICT,
            "invalid_status_transition",
            &e,
        )),
        Err(e) => Err(internal_error(e)),
    }
}

pub(crate) async fn api_quarantine_supervised_patch_workspace(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(workspace_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    match store.quarantine_workspace(&workspace_id, &context.api_key_id) {
        Ok(workspace) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "workspace": workspace,
            })),
        )),
        Err(e) if e.contains("not found") => Err(not_found("workspace_not_found")),
        Err(e) if e.contains("invalid") => Err(ApiError::with_code(
            StatusCode::CONFLICT,
            "invalid_status_transition",
            &e,
        )),
        Err(e) => Err(internal_error(e)),
    }
}

pub(crate) async fn api_capture_supervised_patch(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(workspace_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    match store.capture_patch(&workspace_id, &context.api_key_id) {
        Ok(artifact) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "artifact": artifact,
            })),
        )),
        Err(e) if e.contains("not found") => Err(not_found("workspace_not_found")),
        Err(e) if e.contains("no files") => Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "no_files_to_capture",
            &e,
        )),
        Err(e) => Err(internal_error(e)),
    }
}
