use axum::extract::{Extension, Path as AxumPath, State};
use axum::http::{HeaderMap, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;
use std::sync::Arc;

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, require_store, ApiError, ApiRequestContext, RequestId,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::{CreateApiKeyRequest, UpdateKeyScopesRequest, AXUM_API_SCHEMA_VERSION};
use crate::infrastructure::auth::LOCAL_BOOTSTRAP_API_KEY_ID;
use crate::storage::local_product_store::{
    validate_managed_acceptance_role_scopes, LocalProductStore, ALL_MANAGED_ACCEPTANCE_SCOPES,
    SCOPE_IDENTITY_DELEGATE,
};

fn requests_managed_acceptance_scope(scopes: &[String]) -> bool {
    scopes
        .iter()
        .any(|scope| ALL_MANAGED_ACCEPTANCE_SCOPES.contains(&scope.as_str()))
}

fn require_bootstrap_for_managed_delegation(
    context: &ApiRequestContext,
    role: &str,
    scopes: &[String],
) -> Result<(), ApiError> {
    if scopes.iter().any(|scope| scope == SCOPE_IDENTITY_DELEGATE) {
        return Err(ApiError::new(
            axum::http::StatusCode::FORBIDDEN,
            "the bootstrap delegation capability cannot be delegated",
        ));
    }
    if (matches!(role, "reviewer" | "output_operator") || requests_managed_acceptance_scope(scopes))
        && (context.api_key_id != LOCAL_BOOTSTRAP_API_KEY_ID
            || !context.scopes.contains(SCOPE_IDENTITY_DELEGATE))
    {
        return Err(ApiError::new(
            axum::http::StatusCode::FORBIDDEN,
            "managed-acceptance scope delegation requires the canonical bootstrap authority",
        ));
    }
    Ok(())
}

fn require_key_target_authority(
    context: &ApiRequestContext,
    target: &crate::infrastructure::auth::APIKey,
    target_role: Option<&str>,
) -> Result<(), ApiError> {
    if target.key_id == LOCAL_BOOTSTRAP_API_KEY_ID {
        return Err(ApiError::new(
            axum::http::StatusCode::FORBIDDEN,
            "the canonical bootstrap key is immutable",
        ));
    }
    if target.tenant_id != context.tenant_id {
        return Err(ApiError::new(
            axum::http::StatusCode::FORBIDDEN,
            "a key may only be managed within the authenticated tenant",
        ));
    }
    if matches!(target_role, Some("reviewer" | "output_operator"))
        && (context.api_key_id != LOCAL_BOOTSTRAP_API_KEY_ID
            || !context.scopes.contains(SCOPE_IDENTITY_DELEGATE))
    {
        return Err(ApiError::new(
            axum::http::StatusCode::FORBIDDEN,
            "managed identity mutation requires the canonical bootstrap authority",
        ));
    }
    Ok(())
}

fn require_managed_actor_key_mutation_allowed(
    store: &LocalProductStore,
    context: &ApiRequestContext,
) -> Result<(), ApiError> {
    let actor_role = store
        .get_api_key_metadata(&context.api_key_id)
        .map_err(internal_error)?
        .and_then(|metadata| {
            metadata
                .get("role")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        });
    if matches!(actor_role.as_deref(), Some("reviewer" | "output_operator")) {
        return Err(ApiError::new(
            axum::http::StatusCode::FORBIDDEN,
            "managed identities cannot mutate API key authority",
        ));
    }
    if actor_role.is_none()
        && context.api_key_id != LOCAL_BOOTSTRAP_API_KEY_ID
        && context
            .scopes
            .iter()
            .any(|scope| ALL_MANAGED_ACCEPTANCE_SCOPES.contains(&scope.as_str()))
    {
        return Err(ApiError::new(
            axum::http::StatusCode::FORBIDDEN,
            "managed-capability key metadata is required for API key authority mutation",
        ));
    }
    Ok(())
}

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
    require_bootstrap_for_managed_delegation(&context, &request.role, &request.scopes)?;
    validate_managed_acceptance_role_scopes(&request.role, &request.scopes)
        .map_err(|error| ApiError::new(axum::http::StatusCode::BAD_REQUEST, error))?;
    let store = require_store(&state)?;
    require_managed_actor_key_mutation_allowed(&store, &context)?;

    let resolver = state.tenant_resolver.as_ref().cloned().ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "auth unavailable",
        )
    })?;
    let (key, raw_key) = {
        let mut guard = resolver.lock().map_err(|_| {
            ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "auth unavailable",
            )
        })?;
        let scopes_set: std::collections::HashSet<String> =
            request.scopes.iter().cloned().collect();
        let now = state.now();
        guard
            .create_api_key(
                &context.tenant_id,
                Some(scopes_set),
                request.expires_at,
                now,
            )
            .map_err(|e| ApiError::new(axum::http::StatusCode::BAD_REQUEST, e))?
    };

    let key_id = key.key_id.clone();
    let user_id = request.user_id.clone();
    let role = request.role.clone();
    let scopes = request.scopes.clone();
    let actor_key_id = context.api_key_id.clone();
    let persist_result = match tokio::task::spawn_blocking({
        let store = Arc::clone(&store);
        let key_id = key_id.clone();
        move || {
            store.record_api_key_metadata_with_expiry(
                &key_id,
                &user_id,
                &role,
                &scopes,
                key.expires_at,
                &actor_key_id,
            )
        }
    })
    .await
    {
        Ok(result) => result,
        Err(error) => {
            if let Ok(mut guard) = resolver.lock() {
                guard.remove_api_key(&key_id);
            }
            return Err(internal_error(format!(
                "key metadata worker failed: {error}"
            )));
        }
    };

    if let Err(error) = persist_result {
        if let Ok(mut guard) = resolver.lock() {
            guard.remove_api_key(&key_id);
        }
        return Err(internal_error(error));
    }

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
    require_managed_actor_key_mutation_allowed(&store, &context)?;
    let role = store
        .get_api_key_metadata(&key_id)
        .map_err(internal_error)?
        .and_then(|metadata| {
            metadata
                .get("role")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        });

    let resolver = state.tenant_resolver.as_ref().ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "auth unavailable",
        )
    })?;
    let mut guard = resolver.lock().map_err(|_| {
        ApiError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "auth unavailable",
        )
    })?;
    let target = guard
        .api_key(&key_id)
        .ok_or_else(|| ApiError::new(axum::http::StatusCode::NOT_FOUND, "key not found"))?;
    require_key_target_authority(&context, &target, role.as_deref())?;
    guard.remove_api_key(&key_id);

    let revoked = match store.revoke_api_key_metadata(&key_id, &context.api_key_id) {
        Ok(revoked) => revoked,
        Err(error) => {
            guard.add_api_key(target);
            return Err(internal_error(error));
        }
    };

    if !revoked {
        guard.add_api_key(target);
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
    require_managed_actor_key_mutation_allowed(&store, &context)?;

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
    let resolver = state.tenant_resolver.as_ref().ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "auth unavailable",
        )
    })?;
    let mut guard = resolver.lock().map_err(|_| {
        ApiError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "auth unavailable",
        )
    })?;
    let target = guard
        .api_key(&key_id)
        .ok_or_else(|| ApiError::new(axum::http::StatusCode::NOT_FOUND, "key not found"))?;
    require_key_target_authority(&context, &target, Some(role))?;
    let scopes: Vec<String> = old_key["scopes"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    require_bootstrap_for_managed_delegation(&context, role, &scopes)?;
    validate_managed_acceptance_role_scopes(role, &scopes)
        .map_err(|error| ApiError::new(axum::http::StatusCode::BAD_REQUEST, error))?;
    let expires_at = old_key["expires_at"].as_f64();
    if old_key["revoked_at"].as_str().is_some() {
        guard.remove_api_key(&key_id);
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "key not found or already revoked",
        ));
    }

    let scopes_set: std::collections::HashSet<String> = scopes.iter().cloned().collect();
    let now = state.now();
    let (new_key, raw_key) = guard
        .create_api_key(&context.tenant_id, Some(scopes_set), expires_at, now)
        .map_err(|e| ApiError::new(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e))?;

    match store.rotate_api_key_metadata(
        &key_id,
        &new_key.key_id,
        user_id,
        role,
        &scopes,
        new_key.expires_at,
        &context.api_key_id,
    ) {
        Ok(true) => {
            guard.remove_api_key(&key_id);
        }
        Ok(false) => {
            guard.remove_api_key(&new_key.key_id);
            return Err(ApiError::new(
                axum::http::StatusCode::NOT_FOUND,
                "key not found or already revoked",
            ));
        }
        Err(error) => {
            guard.remove_api_key(&new_key.key_id);
            return Err(internal_error(error));
        }
    }

    Ok((
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
    require_managed_actor_key_mutation_allowed(&store, &context)?;
    let role = store
        .get_api_key_metadata(&key_id)
        .map_err(internal_error)?
        .and_then(|metadata| {
            metadata
                .get("role")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        });

    let resolver = state.tenant_resolver.as_ref().ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "auth unavailable",
        )
    })?;
    let mut guard = resolver.lock().map_err(|_| {
        ApiError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "auth unavailable",
        )
    })?;
    let target = guard
        .api_key(&key_id)
        .ok_or_else(|| ApiError::new(axum::http::StatusCode::NOT_FOUND, "key not found"))?;
    require_key_target_authority(&context, &target, role.as_deref())?;
    guard.remove_api_key(&key_id);

    let deleted = match store.delete_api_key_metadata(&key_id, &context.api_key_id) {
        Ok(deleted) => deleted,
        Err(error) => {
            guard.add_api_key(target);
            return Err(internal_error(error));
        }
    };

    if !deleted {
        guard.add_api_key(target);
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
    let context = authorize(&state, &headers, "team:admin", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    require_managed_actor_key_mutation_allowed(&store, &context)?;

    let old_key = store
        .get_api_key_metadata(&key_id)
        .map_err(internal_error)?
        .ok_or_else(|| ApiError::new(axum::http::StatusCode::NOT_FOUND, "key not found"))?;
    let role = old_key["role"].as_str().ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "invalid key metadata",
        )
    })?;
    let resolver = state.tenant_resolver.as_ref().ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "auth unavailable",
        )
    })?;
    let mut guard = resolver.lock().map_err(|_| {
        ApiError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "auth unavailable",
        )
    })?;
    let target = guard
        .api_key(&key_id)
        .ok_or_else(|| ApiError::new(axum::http::StatusCode::NOT_FOUND, "key not found"))?;
    require_key_target_authority(&context, &target, Some(role))?;
    require_bootstrap_for_managed_delegation(&context, role, &request.scopes)?;
    validate_managed_acceptance_role_scopes(role, &request.scopes)
        .map_err(|error| ApiError::new(axum::http::StatusCode::BAD_REQUEST, error))?;

    let old_scopes = target.scopes.clone();
    let scopes: std::collections::HashSet<String> = request.scopes.iter().cloned().collect();
    guard
        .validate_api_key_scopes(&key_id, &scopes)
        .map_err(|error| ApiError::new(axum::http::StatusCode::BAD_REQUEST, error))?;

    guard
        .update_api_key_scopes(&key_id, scopes.clone())
        .map_err(|_| {
            ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "auth state update failed",
            )
        })?;

    let updated = match store.update_api_key_scopes(&key_id, &request.scopes, &context.api_key_id) {
        Ok(updated) => updated,
        Err(error) => {
            let _ = guard.update_api_key_scopes(&key_id, old_scopes);
            return Err(internal_error(error));
        }
    };

    if !updated {
        let _ = guard.update_api_key_scopes(&key_id, old_scopes);
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
