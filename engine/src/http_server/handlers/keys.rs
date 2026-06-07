use axum::extract::{Extension, Path as AxumPath, State};
use axum::http::{HeaderMap, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, require_store, ApiError, RequestId,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::{CreateApiKeyRequest, UpdateKeyScopesRequest, AXUM_API_SCHEMA_VERSION};

pub(crate) async fn api_list_keys(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "team:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let keys = store.list_api_key_metadata(100).map_err(internal_error)?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "keys": keys,
        })),
    ))
}

pub(crate) async fn api_create_key(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Json(request): Json<CreateApiKeyRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "team:admin", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;

    let mut guard = state
        .tenant_resolver
        .as_ref()
        .ok_or_else(|| {
            ApiError::new(
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "auth unavailable",
            )
        })?
        .lock()
        .map_err(|_| {
            ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "auth unavailable",
            )
        })?;

    let scopes_set: std::collections::HashSet<String> = request.scopes.iter().cloned().collect();
    let (key, raw_key) = guard
        .create_api_key(
            &context.tenant_id,
            Some(scopes_set),
            request.expires_at,
            state.now,
        )
        .map_err(|e| ApiError::new(axum::http::StatusCode::BAD_REQUEST, e))?;

    store
        .record_api_key_metadata(
            &key.key_id,
            &request.user_id,
            &request.role,
            &request.scopes,
            &context.api_key_id,
        )
        .map_err(internal_error)?;

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "key_id": key.key_id,
            "raw_key": raw_key,
            "user_id": request.user_id,
            "role": request.role,
            "scopes": request.scopes,
            "created_at": key.created_at,
        })),
    ))
}

pub(crate) async fn api_revoke_key(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(key_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "team:admin", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;

    let revoked = store
        .revoke_api_key_metadata(&key_id, &context.api_key_id)
        .map_err(internal_error)?;

    if let Some(resolver) = &state.tenant_resolver {
        let mut guard = resolver.lock().map_err(|_| {
            ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "auth unavailable",
            )
        })?;
        guard.remove_api_key(&key_id);
    }

    if !revoked {
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "key not found or already revoked",
        ));
    }

    Ok((
        cors_headers(),
        Json(json!({"schema_version": AXUM_API_SCHEMA_VERSION, "ok": true, "key_id": key_id})),
    ))
}

pub(crate) async fn api_rotate_key(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(key_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "team:admin", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;

    let old_key = store
        .get_api_key_metadata(&key_id)
        .map_err(internal_error)?
        .ok_or_else(|| ApiError::new(axum::http::StatusCode::NOT_FOUND, "key not found"))?;

    let user_id = old_key["user_id"].as_str().ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "invalid key metadata",
        )
    })?;
    let role = old_key["role"].as_str().ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "invalid key metadata",
        )
    })?;
    let scopes: Vec<String> = old_key["scopes"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let expires_at = old_key["expires_at"].as_f64();

    store
        .revoke_api_key_metadata(&key_id, &context.api_key_id)
        .map_err(internal_error)?;

    if let Some(resolver) = &state.tenant_resolver {
        let mut guard = resolver.lock().map_err(|_| {
            ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "auth unavailable",
            )
        })?;
        guard.remove_api_key(&key_id);

        let scopes_set: std::collections::HashSet<String> = scopes.iter().cloned().collect();
        let (new_key, raw_key) = guard
            .create_api_key(&context.tenant_id, Some(scopes_set), expires_at, state.now)
            .map_err(|e| ApiError::new(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e))?;

        store
            .record_api_key_metadata(&new_key.key_id, user_id, role, &scopes, &context.api_key_id)
            .map_err(internal_error)?;

        return Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "key_id": new_key.key_id,
                "raw_key": raw_key,
                "user_id": user_id,
                "role": role,
                "scopes": scopes,
                "created_at": new_key.created_at,
                "rotated_from": key_id,
            })),
        ));
    }

    Err(ApiError::new(
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "auth unavailable",
    ))
}

pub(crate) async fn api_delete_key(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(key_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "team:admin", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;

    let deleted = store
        .delete_api_key_metadata(&key_id, &context.api_key_id)
        .map_err(internal_error)?;

    if let Some(resolver) = &state.tenant_resolver {
        let mut guard = resolver.lock().map_err(|_| {
            ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "auth unavailable",
            )
        })?;
        guard.remove_api_key(&key_id);
    }

    if !deleted {
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "key not found",
        ));
    }

    Ok((
        cors_headers(),
        Json(json!({"schema_version": AXUM_API_SCHEMA_VERSION, "ok": true, "key_id": key_id})),
    ))
}

pub(crate) async fn api_update_key_scopes(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(key_id): AxumPath<String>,
    Json(request): Json<UpdateKeyScopesRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let _context = authorize(&state, &headers, "team:admin", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;

    let updated = store
        .update_api_key_scopes(&key_id, &request.scopes, &_context.api_key_id)
        .map_err(internal_error)?;

    if !updated {
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "key not found",
        ));
    }

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "ok": true,
            "key_id": key_id,
            "scopes": request.scopes,
        })),
    ))
}
