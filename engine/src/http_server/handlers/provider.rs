use axum::extract::{Extension, Query, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, require_store, ApiError, RequestId,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::{ProviderEndpointConfigApiRequest, AXUM_API_SCHEMA_VERSION};
use crate::provider::adaptive_execution::{
    adaptive_runtime_hash_from_configs, build_adaptive_provider_runtime_from_configs,
    parse_adaptive_provider_endpoints_json, AdaptiveProviderEndpointConfig,
    ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON,
};

const ADAPTIVE_PROVIDER_ENDPOINTS_CONFIG_KEY: &str = "adaptive_provider_endpoints";

pub(crate) async fn api_provider_health(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "health:read", uri.path(), &request_id.0)?;
    let Some(provider) = &state.provider else {
        return Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "status": "noop",
                "message": "no provider configured",
            })),
        ));
    };
    let enabled = provider.is_enabled();
    let provider_id = provider.provider_id();
    if enabled {
        Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "status": "ok",
                "provider_id": provider_id,
                "enabled": true,
            })),
        ))
    } else {
        Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "status": "error",
                "provider_id": provider_id,
                "enabled": false,
                "message": "provider is disabled",
            })),
        ))
    }
}

pub(crate) async fn api_provider_endpoints(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "config:read", uri.path(), &request_id.0)?;
    let store = require_store(&state)?;
    let config = store.config_snapshot().map_err(internal_error)?;
    let local_endpoints = config
        .get(ADAPTIVE_PROVIDER_ENDPOINTS_CONFIG_KEY)
        .and_then(|value| {
            serde_json::from_value::<Vec<AdaptiveProviderEndpointConfig>>(value.clone()).ok()
        });
    let env_endpoints = std::env::var(ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .and_then(|value| parse_adaptive_provider_endpoints_json(&value).ok());
    let (source, endpoints) = if let Some(endpoints) = local_endpoints {
        ("local_config", endpoints)
    } else if let Some(endpoints) = env_endpoints {
        ("environment", endpoints)
    } else {
        ("none", Vec::new())
    };
    let (local_runtime_ready, local_config_error_code) = if source == "local_config" {
        local_config_runtime_status(&state, &store, &endpoints)
    } else {
        (false, None)
    };
    let completion_executor_configured =
        state.adaptive_provider_executor.is_some() || local_runtime_ready;
    let completion_registry_configured =
        state.adaptive_registry_snapshot.is_some() || local_runtime_ready;

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "source": source,
            "endpoints": endpoints,
            "runtime": {
                "executor_configured": state.adaptive_provider_executor.is_some(),
                "registry_configured": state.adaptive_registry_snapshot.is_some(),
                "workflow_executor_configured": state.adaptive_provider_executor.is_some(),
                "workflow_registry_configured": state.adaptive_registry_snapshot.is_some(),
                "completion_executor_configured": completion_executor_configured,
                "completion_registry_configured": completion_registry_configured,
                "local_config_apply_requires_restart": source == "local_config" && !local_runtime_ready,
                "local_config_applies_to_completion_api": local_runtime_ready,
                "local_config_error_code": local_config_error_code,
            },
            "safety": {
                "raw_secrets_allowed": false,
                "credential_storage": "env_reference_only",
                "supported_provider_types": ["stub", "openai_compatible", "anthropic"],
            },
        })),
    ))
}

pub(crate) async fn api_put_provider_endpoints(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Json(request): Json<ProviderEndpointConfigApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "config:admin", uri.path(), &request_id.0)?;
    if request.confirm_provider_endpoint_config != Some(true) {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "provider_endpoint_config_not_confirmed",
            "confirm_provider_endpoint_config must be true",
        ));
    }
    let raw = serde_json::to_string(&request.endpoints).map_err(|_| {
        ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "invalid_endpoint_config_json",
            "adaptive endpoint config must be serializable",
        )
    })?;
    let endpoints = parse_adaptive_provider_endpoints_json(&raw)
        .map_err(|error| ApiError::with_code(StatusCode::BAD_REQUEST, error.code, error.message))?;
    let config_hash = adaptive_runtime_hash_from_configs(&endpoints)
        .map_err(|error| ApiError::with_code(StatusCode::BAD_REQUEST, error.code, error.message))?;
    let store = require_store(&state)?;
    let (executor, registry_snapshot) = build_adaptive_provider_runtime_from_configs(
        &endpoints,
        &store,
        &state.circuit_breaker_registry,
    )
    .map_err(|error| ApiError::with_code(StatusCode::BAD_REQUEST, error.code, error.message))?;
    let updated = store
        .set_config_value(
            ADAPTIVE_PROVIDER_ENDPOINTS_CONFIG_KEY,
            json!(endpoints),
            &context.api_key_id,
        )
        .map_err(internal_error)?;
    state.install_adaptive_local_config_runtime(config_hash, executor, registry_snapshot);
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "source": "local_config",
            "endpoints": updated["value"],
            "runtime": {
                "executor_configured": state.adaptive_provider_executor.is_some(),
                "registry_configured": state.adaptive_registry_snapshot.is_some(),
                "workflow_executor_configured": state.adaptive_provider_executor.is_some(),
                "workflow_registry_configured": state.adaptive_registry_snapshot.is_some(),
                "completion_executor_configured": true,
                "completion_registry_configured": true,
                "local_config_apply_requires_restart": false,
                "local_config_applies_to_completion_api": true,
                "local_config_error_code": null,
            },
            "safety": {
                "raw_secrets_allowed": false,
                "credential_storage": "env_reference_only",
                "supported_provider_types": ["stub", "openai_compatible", "anthropic"],
            },
        })),
    ))
}

fn local_config_runtime_status(
    state: &AxumApiState,
    store: &std::sync::Arc<crate::storage::local_product_store::LocalProductStore>,
    endpoints: &[AdaptiveProviderEndpointConfig],
) -> (bool, Option<String>) {
    let config_hash = match adaptive_runtime_hash_from_configs(endpoints) {
        Ok(hash) => hash,
        Err(error) => return (false, Some(error.code)),
    };
    if state
        .adaptive_local_config_runtime_for_hash(&config_hash)
        .is_some()
    {
        return (true, None);
    }
    match build_adaptive_provider_runtime_from_configs(
        endpoints,
        store,
        &state.circuit_breaker_registry,
    ) {
        Ok((executor, registry_snapshot)) => {
            state.install_adaptive_local_config_runtime(config_hash, executor, registry_snapshot);
            (true, None)
        }
        Err(error) => (false, Some(error.code)),
    }
}

pub(crate) async fn api_provider_audit(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "audit:read", uri.path(), &request_id.0)?;
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
    let events = store
        .provider_audit_events_with_offset(limit, offset)
        .map_err(internal_error)?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "events": events,
        })),
    ))
}
