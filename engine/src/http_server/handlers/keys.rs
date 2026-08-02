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
use crate::infrastructure::auth::{APIKey, TenantResolver, LOCAL_BOOTSTRAP_API_KEY_ID};
use crate::storage::local_product_store::{
    validate_managed_acceptance_role_scopes, LocalProductStore, ALL_MANAGED_ACCEPTANCE_SCOPES,
    SCOPE_IDENTITY_DELEGATE,
};

fn requests_managed_acceptance_scope(scopes: &[String]) -> bool {
    scopes
        .iter()
        .any(|scope| ALL_MANAGED_ACCEPTANCE_SCOPES.contains(&scope.as_str()))
}

fn fail_closed_resolver_after_durable_key_uncertainty(resolver: &mut TenantResolver, key_id: &str) {
    resolver.remove_api_key(key_id);
}

async fn run_store_operation<T, F>(operation: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| internal_error(format!("key authority worker failed: {error}")))?
        .map_err(internal_error)
}

fn resolver_api_key(
    resolver: &Arc<std::sync::Mutex<TenantResolver>>,
    key_id: &str,
) -> Result<APIKey, ApiError> {
    resolver
        .lock()
        .map_err(|_| {
            ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "auth unavailable",
            )
        })?
        .api_key(key_id)
        .ok_or_else(|| ApiError::new(axum::http::StatusCode::NOT_FOUND, "key not found"))
}

fn resolver_remove_key(
    resolver: &Arc<std::sync::Mutex<TenantResolver>>,
    key_id: &str,
) -> Result<(), ApiError> {
    let mut guard = resolver.lock().map_err(|_| {
        ApiError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "auth unavailable",
        )
    })?;
    fail_closed_resolver_after_durable_key_uncertainty(&mut guard, key_id);
    Ok(())
}

fn fail_closed_missing_scoped_key(
    resolver: &Arc<std::sync::Mutex<TenantResolver>>,
    key_id: &str,
    tenant_id: &str,
) -> Result<(), ApiError> {
    let same_tenant = resolver
        .lock()
        .map_err(|_| {
            ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "auth unavailable",
            )
        })?
        .api_key(key_id)
        .is_some_and(|key| key.tenant_id == tenant_id);
    if same_tenant {
        resolver_remove_key(resolver, key_id)?;
    }
    Ok(())
}

fn resolver_update_key_scopes(
    resolver: &Arc<std::sync::Mutex<TenantResolver>>,
    key_id: &str,
    scopes: std::collections::HashSet<String>,
) -> Result<(), ApiError> {
    let mut guard = resolver.lock().map_err(|_| {
        ApiError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "auth unavailable",
        )
    })?;
    guard
        .update_api_key_scopes(key_id, scopes)
        .map_err(internal_error)
}

fn require_bootstrap_for_managed_delegation(
    store: &LocalProductStore,
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
    if matches!(role, "reviewer" | "output_operator") || requests_managed_acceptance_scope(scopes) {
        if context.api_key_id != LOCAL_BOOTSTRAP_API_KEY_ID
            || !context.scopes.contains(SCOPE_IDENTITY_DELEGATE)
        {
            return Err(ApiError::new(
                axum::http::StatusCode::FORBIDDEN,
                "managed-acceptance scope delegation requires the canonical bootstrap authority",
            ));
        }
        store
            .authenticate_bootstrap_identity_delegation_principal(&context.tenant_id, None)
            .map_err(|_| {
                ApiError::new(
                    axum::http::StatusCode::FORBIDDEN,
                    "managed-acceptance scope delegation requires the canonical bootstrap authority",
                )
            })?;
    }
    Ok(())
}

fn require_key_target_authority(
    store: &LocalProductStore,
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
    if matches!(target_role, Some("reviewer" | "output_operator")) {
        if context.api_key_id != LOCAL_BOOTSTRAP_API_KEY_ID
            || !context.scopes.contains(SCOPE_IDENTITY_DELEGATE)
        {
            return Err(ApiError::new(
                axum::http::StatusCode::FORBIDDEN,
                "managed identity mutation requires the canonical bootstrap authority",
            ));
        }
        store
            .authenticate_bootstrap_identity_delegation_principal(&context.tenant_id, None)
            .map_err(|_| {
                ApiError::new(
                    axum::http::StatusCode::FORBIDDEN,
                    "managed identity mutation requires the canonical bootstrap authority",
                )
            })?;
    }
    Ok(())
}

fn require_managed_actor_key_mutation_allowed(
    store: &LocalProductStore,
    context: &ApiRequestContext,
) -> Result<(), ApiError> {
    let actor_role = store
        .get_api_key_metadata_for_tenant(&context.api_key_id, &context.tenant_id)
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
    let context = authorize(&state, &headers, "team:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let keys = run_store_operation({
        let store = Arc::clone(&store);
        let tenant_id = context.tenant_id.clone();
        move || store.list_api_key_metadata_for_tenant(&tenant_id, 100)
    })
    .await?;
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
    let bootstrap_store = Arc::clone(&store);
    let bootstrap_context = context.clone();
    let bootstrap_role = request.role.clone();
    let bootstrap_scopes = request.scopes.clone();
    tokio::task::spawn_blocking(move || {
        require_bootstrap_for_managed_delegation(
            &bootstrap_store,
            &bootstrap_context,
            &bootstrap_role,
            &bootstrap_scopes,
        )
    })
    .await
    .map_err(|error| internal_error(format!("bootstrap authority worker failed: {error}")))??;
    validate_managed_acceptance_role_scopes(&request.role, &request.scopes)
        .map_err(|error| ApiError::new(axum::http::StatusCode::BAD_REQUEST, error))?;
    let actor_store = Arc::clone(&store);
    let actor_context = context.clone();
    tokio::task::spawn_blocking(move || {
        require_managed_actor_key_mutation_allowed(&actor_store, &actor_context)
    })
    .await
    .map_err(|error| internal_error(format!("key authority worker failed: {error}")))??;

    let resolver = state.tenant_resolver.as_ref().cloned().ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "auth unavailable",
        )
    })?;
    let (key, raw_key) = {
        let guard = resolver.lock().map_err(|_| {
            ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "auth unavailable",
            )
        })?;
        let scopes_set: std::collections::HashSet<String> =
            request.scopes.iter().cloned().collect();
        let now = state.now();
        guard
            .prepare_api_key(
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
            store.record_api_key_metadata_with_expiry_for_tenant(
                &context.tenant_id,
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
            return Err(internal_error(format!(
                "key metadata worker failed: {error}"
            )));
        }
    };

    if let Err(error) = persist_result {
        return Err(internal_error(error));
    }
    resolver
        .lock()
        .map_err(|_| {
            ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "auth unavailable",
            )
        })?
        .add_api_key(key.clone());

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
    let actor_store = Arc::clone(&store);
    let actor_context = context.clone();
    tokio::task::spawn_blocking(move || {
        require_managed_actor_key_mutation_allowed(&actor_store, &actor_context)
    })
    .await
    .map_err(|error| internal_error(format!("key authority worker failed: {error}")))??;
    let resolver = state.tenant_resolver.as_ref().cloned().ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "auth unavailable",
        )
    })?;
    let metadata = run_store_operation({
        let store = Arc::clone(&store);
        let key_id = key_id.clone();
        let tenant_id = context.tenant_id.clone();
        move || store.get_api_key_metadata_for_tenant(&key_id, &tenant_id)
    })
    .await?;
    let metadata = match metadata {
        Some(metadata) => metadata,
        None => {
            fail_closed_missing_scoped_key(&resolver, &key_id, &context.tenant_id)?;
            return Err(ApiError::new(
                axum::http::StatusCode::NOT_FOUND,
                "key not found",
            ));
        }
    };
    let role = metadata
        .get("role")
        .and_then(|value| value.as_str().map(str::to_string));

    let target = resolver_api_key(&resolver, &key_id)?;
    let target_context = context.clone();
    let target_store = Arc::clone(&store);
    let target_role = role.clone();
    tokio::task::spawn_blocking(move || {
        require_key_target_authority(
            &target_store,
            &target_context,
            &target,
            target_role.as_deref(),
        )
    })
    .await
    .map_err(|error| internal_error(format!("key authority worker failed: {error}")))??;
    let revoked = run_store_operation({
        let store = Arc::clone(&store);
        let key_id = key_id.clone();
        let tenant_id = context.tenant_id.clone();
        let actor_key_id = context.api_key_id.clone();
        move || store.revoke_api_key_metadata_for_tenant(&key_id, &tenant_id, &actor_key_id)
    })
    .await;
    let mut guard = resolver.lock().map_err(|_| {
        ApiError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "auth unavailable",
        )
    })?;
    let revoked = match revoked {
        Ok(revoked) => revoked,
        Err(error) => {
            // A commit error has unknown durable outcome. Fail closed in the
            // in-memory resolver instead of restoring a key that may already
            // be revoked in the canonical store.
            fail_closed_resolver_after_durable_key_uncertainty(&mut guard, &key_id);
            return Err(error);
        }
    };

    if !revoked {
        fail_closed_resolver_after_durable_key_uncertainty(&mut guard, &key_id);
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "key not found or already revoked",
        ));
    }
    guard.remove_api_key(&key_id);

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
    let actor_store = Arc::clone(&store);
    let actor_context = context.clone();
    tokio::task::spawn_blocking(move || {
        require_managed_actor_key_mutation_allowed(&actor_store, &actor_context)
    })
    .await
    .map_err(|error| internal_error(format!("key authority worker failed: {error}")))??;

    let old_key = run_store_operation({
        let store = Arc::clone(&store);
        let key_id = key_id.clone();
        let tenant_id = context.tenant_id.clone();
        move || store.get_api_key_metadata_for_tenant(&key_id, &tenant_id)
    })
    .await?
    .ok_or_else(|| ApiError::new(axum::http::StatusCode::NOT_FOUND, "key not found"))?;

    let user_id = old_key["user_id"]
        .as_str()
        .ok_or_else(|| {
            ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "invalid key metadata",
            )
        })?
        .to_string();
    let role = old_key["role"]
        .as_str()
        .ok_or_else(|| {
            ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "invalid key metadata",
            )
        })?
        .to_string();
    let resolver = state.tenant_resolver.as_ref().cloned().ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "auth unavailable",
        )
    })?;
    let target = resolver_api_key(&resolver, &key_id)?;
    let target_store = Arc::clone(&store);
    let target_context = context.clone();
    let target_role = role.clone();
    let target_for_check = target.clone();
    tokio::task::spawn_blocking(move || {
        require_key_target_authority(
            &target_store,
            &target_context,
            &target_for_check,
            Some(target_role.as_str()),
        )
    })
    .await
    .map_err(|error| internal_error(format!("key authority worker failed: {error}")))??;
    let scopes: Vec<String> = old_key["scopes"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let bootstrap_store = Arc::clone(&store);
    let bootstrap_context = context.clone();
    let bootstrap_role = role.clone();
    let bootstrap_scopes = scopes.clone();
    tokio::task::spawn_blocking(move || {
        require_bootstrap_for_managed_delegation(
            &bootstrap_store,
            &bootstrap_context,
            &bootstrap_role,
            &bootstrap_scopes,
        )
    })
    .await
    .map_err(|error| internal_error(format!("bootstrap authority worker failed: {error}")))??;
    validate_managed_acceptance_role_scopes(&role, &scopes)
        .map_err(|error| ApiError::new(axum::http::StatusCode::BAD_REQUEST, error))?;
    let expires_at = old_key["expires_at"].as_f64();
    let scopes_set: std::collections::HashSet<String> = scopes.iter().cloned().collect();
    let now = state.now();
    let (new_key, raw_key) = {
        let mut guard = resolver.lock().map_err(|_| {
            ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "auth unavailable",
            )
        })?;
        if old_key["revoked_at"].as_str().is_some() {
            guard.remove_api_key(&key_id);
            return Err(ApiError::new(
                axum::http::StatusCode::NOT_FOUND,
                "key not found or already revoked",
            ));
        }
        guard
            .prepare_api_key(&context.tenant_id, Some(scopes_set), expires_at, now)
            .map_err(|e| ApiError::new(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e))?
    };

    let rotate_result = run_store_operation({
        let store = Arc::clone(&store);
        let key_id = key_id.clone();
        let tenant_id = context.tenant_id.clone();
        let new_key_id = new_key.key_id.clone();
        let user_id = user_id.clone();
        let role = role.clone();
        let scopes = scopes.clone();
        let expires_at = new_key.expires_at;
        let actor_key_id = context.api_key_id.clone();
        move || {
            store.rotate_api_key_metadata_for_tenant(
                &key_id,
                &tenant_id,
                &new_key_id,
                &user_id,
                &role,
                &scopes,
                expires_at,
                &actor_key_id,
            )
        }
    })
    .await;
    let mut guard = resolver.lock().map_err(|_| {
        ApiError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "auth unavailable",
        )
    })?;
    match rotate_result {
        Ok(true) => {
            guard.remove_api_key(&key_id);
            guard.add_api_key(new_key.clone());
        }
        Ok(false) => {
            fail_closed_resolver_after_durable_key_uncertainty(&mut guard, &key_id);
            return Err(ApiError::new(
                axum::http::StatusCode::NOT_FOUND,
                "key not found or already revoked",
            ));
        }
        Err(error) => {
            fail_closed_resolver_after_durable_key_uncertainty(&mut guard, &key_id);
            return Err(error);
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
    let actor_store = Arc::clone(&store);
    let actor_context = context.clone();
    tokio::task::spawn_blocking(move || {
        require_managed_actor_key_mutation_allowed(&actor_store, &actor_context)
    })
    .await
    .map_err(|error| internal_error(format!("key authority worker failed: {error}")))??;
    let resolver = state.tenant_resolver.as_ref().cloned().ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "auth unavailable",
        )
    })?;
    let metadata = run_store_operation({
        let store = Arc::clone(&store);
        let key_id = key_id.clone();
        let tenant_id = context.tenant_id.clone();
        move || store.get_api_key_metadata_for_tenant(&key_id, &tenant_id)
    })
    .await?;
    let metadata = match metadata {
        Some(metadata) => metadata,
        None => {
            fail_closed_missing_scoped_key(&resolver, &key_id, &context.tenant_id)?;
            return Err(ApiError::new(
                axum::http::StatusCode::NOT_FOUND,
                "key not found",
            ));
        }
    };
    let role = metadata
        .get("role")
        .and_then(|value| value.as_str().map(str::to_string));

    let target = resolver_api_key(&resolver, &key_id)?;
    let target_store = Arc::clone(&store);
    let target_context = context.clone();
    let target_role = role.clone();
    tokio::task::spawn_blocking(move || {
        require_key_target_authority(
            &target_store,
            &target_context,
            &target,
            target_role.as_deref(),
        )
    })
    .await
    .map_err(|error| internal_error(format!("key authority worker failed: {error}")))??;
    let deleted = run_store_operation({
        let store = Arc::clone(&store);
        let key_id = key_id.clone();
        let tenant_id = context.tenant_id.clone();
        let actor_key_id = context.api_key_id.clone();
        move || store.delete_api_key_metadata_for_tenant(&key_id, &tenant_id, &actor_key_id)
    })
    .await;
    let mut guard = resolver.lock().map_err(|_| {
        ApiError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "auth unavailable",
        )
    })?;
    let deleted = match deleted {
        Ok(deleted) => deleted,
        Err(error) => {
            // Keep the resolver fail-closed when the durable outcome is
            // unknown; restoring the key could resurrect revoked authority.
            fail_closed_resolver_after_durable_key_uncertainty(&mut guard, &key_id);
            return Err(error);
        }
    };

    if !deleted {
        fail_closed_resolver_after_durable_key_uncertainty(&mut guard, &key_id);
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "key not found",
        ));
    }
    guard.remove_api_key(&key_id);

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
    let actor_store = Arc::clone(&store);
    let actor_context = context.clone();
    tokio::task::spawn_blocking(move || {
        require_managed_actor_key_mutation_allowed(&actor_store, &actor_context)
    })
    .await
    .map_err(|error| internal_error(format!("key authority worker failed: {error}")))??;

    let old_key = run_store_operation({
        let store = Arc::clone(&store);
        let key_id = key_id.clone();
        let tenant_id = context.tenant_id.clone();
        move || store.get_api_key_metadata_for_tenant(&key_id, &tenant_id)
    })
    .await?
    .ok_or_else(|| ApiError::new(axum::http::StatusCode::NOT_FOUND, "key not found"))?;
    let role = old_key["role"]
        .as_str()
        .ok_or_else(|| {
            ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "invalid key metadata",
            )
        })?
        .to_string();
    let resolver = state.tenant_resolver.as_ref().cloned().ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "auth unavailable",
        )
    })?;
    let target = resolver_api_key(&resolver, &key_id)?;
    let target_store = Arc::clone(&store);
    let target_context = context.clone();
    let target_role = role.clone();
    let target_for_check = target.clone();
    tokio::task::spawn_blocking(move || {
        require_key_target_authority(
            &target_store,
            &target_context,
            &target_for_check,
            Some(target_role.as_str()),
        )
    })
    .await
    .map_err(|error| internal_error(format!("key authority worker failed: {error}")))??;
    let bootstrap_store = Arc::clone(&store);
    let bootstrap_context = context.clone();
    let bootstrap_role = role.clone();
    let bootstrap_scopes = request.scopes.clone();
    tokio::task::spawn_blocking(move || {
        require_bootstrap_for_managed_delegation(
            &bootstrap_store,
            &bootstrap_context,
            &bootstrap_role,
            &bootstrap_scopes,
        )
    })
    .await
    .map_err(|error| internal_error(format!("bootstrap authority worker failed: {error}")))??;
    validate_managed_acceptance_role_scopes(&role, &request.scopes)
        .map_err(|error| ApiError::new(axum::http::StatusCode::BAD_REQUEST, error))?;

    let old_scopes = target.scopes.clone();
    let scopes: std::collections::HashSet<String> = request.scopes.iter().cloned().collect();
    {
        let guard = resolver.lock().map_err(|_| {
            ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "auth unavailable",
            )
        })?;
        guard
            .validate_api_key_scopes(&key_id, &scopes)
            .map_err(|error| ApiError::new(axum::http::StatusCode::BAD_REQUEST, error))?;
    }

    let updated = run_store_operation({
        let store = Arc::clone(&store);
        let key_id = key_id.clone();
        let tenant_id = context.tenant_id.clone();
        let requested_scopes = request.scopes.clone();
        let actor_key_id = context.api_key_id.clone();
        move || {
            store.update_api_key_scopes_for_tenant(
                &key_id,
                &tenant_id,
                &requested_scopes,
                &actor_key_id,
            )
        }
    })
    .await?;
    if !updated {
        resolver_remove_key(&resolver, &key_id)?;
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "key not found",
        ));
    }
    if resolver_update_key_scopes(&resolver, &key_id, scopes.clone()).is_err() {
        // The resolver was validated above; if its update nevertheless
        // fails after the store commit, restore the canonical old value
        // when possible, and always remove the in-memory authority.
        let rollback_scopes = old_scopes.iter().cloned().collect::<Vec<_>>();
        let rollback_store = Arc::clone(&store);
        let rollback_key_id = key_id.clone();
        let rollback_tenant_id = context.tenant_id.clone();
        let rollback_actor_key_id = context.api_key_id.clone();
        let _ = run_store_operation(move || {
            rollback_store.update_api_key_scopes_for_tenant(
                &rollback_key_id,
                &rollback_tenant_id,
                &rollback_scopes,
                &rollback_actor_key_id,
            )
        })
        .await;
        resolver_remove_key(&resolver, &key_id)?;
        return Err(ApiError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "auth state update failed",
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

#[cfg(test)]
mod tests {
    use super::fail_closed_resolver_after_durable_key_uncertainty;
    use crate::infrastructure::auth::{Tenant, TenantResolver};
    use std::collections::HashSet;

    #[test]
    fn durable_key_uncertainty_removes_resolver_authority() {
        let mut resolver = TenantResolver::new();
        resolver.add_tenant(Tenant {
            tenant_id: "local".into(),
            name: "Local".into(),
            scopes: HashSet::from(["team:read".to_string()]),
            rate_limit: None,
        });
        let (key, _) = resolver
            .create_api_key(
                "local",
                Some(HashSet::from(["team:read".to_string()])),
                None,
                1.0,
            )
            .unwrap();
        assert!(resolver.api_key(&key.key_id).is_some());

        fail_closed_resolver_after_durable_key_uncertainty(&mut resolver, &key.key_id);

        assert!(resolver.api_key(&key.key_id).is_none());
    }
}
