use axum::extract::{Extension, Path, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashSet;

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, require_store, ApiError, RequestId,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::{
    ToolAllowlistPolicyApiRequest, ToolCapabilityPolicyApiRequest, ToolHookPolicyApiRequest,
    AXUM_API_SCHEMA_VERSION,
};
use crate::provider::redaction::contains_sensitive_patterns;
use crate::workflow::tool_registry::validate_tool_hook_contract;

const MAX_POLICY_IDENTIFIER_BYTES: usize = 256;
const MAX_TOOL_DESCRIPTION_BYTES: usize = 4 * 1024;
const MAX_POLICY_JSON_BYTES: usize = 16 * 1024;
const MAX_TOOL_NAMES: usize = 128;

fn bounded_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_POLICY_IDENTIFIER_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn validate_resource_id(value: &str) -> Result<(), ApiError> {
    if bounded_identifier(value) {
        Ok(())
    } else {
        Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "invalid_tool_policy_identifier",
            "tool policy identifier is invalid or oversized",
        ))
    }
}

fn validate_expected_hash(value: Option<&str>) -> Result<(), ApiError> {
    if value
        .is_none_or(|hash| hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        Ok(())
    } else {
        Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "invalid_tool_policy_hash",
            "expected_current_sha256 must be a 64-character hexadecimal digest",
        ))
    }
}

fn validate_confirmation(confirmed: Option<bool>) -> Result<(), ApiError> {
    if confirmed == Some(true) {
        Ok(())
    } else {
        Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "tool_policy_confirmation_required",
            "confirm_tool_policy must be true",
        ))
    }
}

fn validate_bounded_json<T: Serialize>(value: &T) -> Result<(), ApiError> {
    let bytes = serde_json::to_vec(value).map_err(|error| internal_error(error.to_string()))?;
    if bytes.len() > MAX_POLICY_JSON_BYTES {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "tool_policy_oversized",
            "tool policy request exceeds the bounded JSON size",
        ));
    }
    let rendered = String::from_utf8_lossy(&bytes);
    if contains_sensitive_patterns(&rendered) {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "tool_policy_sensitive_content",
            "tool policy metadata must not contain secret-shaped content",
        ));
    }
    Ok(())
}

fn map_policy_error(error: String) -> ApiError {
    if error.contains("changed concurrently")
        || error.contains("expected_current_sha256")
        || error.contains("does not exist at expected hash")
    {
        ApiError::with_code(StatusCode::CONFLICT, "tool_policy_stale", &error)
    } else if error.starts_with("tool capability is not registered:") {
        ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "tool_capability_not_registered",
            &error,
        )
    } else if error.starts_with("enabled tool hook count exceeds") {
        ApiError::with_code(StatusCode::CONFLICT, "tool_policy_hook_limit", &error)
    } else {
        internal_error(error)
    }
}

fn not_found(kind: &str, resource_id: &str) -> ApiError {
    ApiError::with_code(
        StatusCode::NOT_FOUND,
        "tool_policy_not_found",
        format!("{kind} tool policy resource not found: {resource_id}"),
    )
}

fn response(resource: Value) -> (HeaderMap, Json<Value>) {
    (
        cors_headers(),
        Json(json!({"schema_version": AXUM_API_SCHEMA_VERSION, "resource": resource})),
    )
}

pub(crate) async fn api_tool_capability_policy(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Path(tool_name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    validate_resource_id(&tool_name)?;
    let store = require_store(&state)?;
    store
        .read_tool_capability_policy(&tool_name)
        .map_err(internal_error)?
        .map(response)
        .ok_or_else(|| not_found("capability", &tool_name))
}

pub(crate) async fn api_configure_tool_capability(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Path(tool_name): Path<String>,
    Json(request): Json<ToolCapabilityPolicyApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    validate_resource_id(&tool_name)?;
    validate_confirmation(request.confirm_tool_policy)?;
    validate_expected_hash(request.expected_current_sha256.as_deref())?;
    validate_bounded_json(&request)?;
    if request.description.trim().is_empty()
        || request.description.len() > MAX_TOOL_DESCRIPTION_BYTES
        || !matches!(request.risk_level.as_str(), "low" | "medium" | "high")
    {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "invalid_tool_capability_policy",
            "description and risk_level must satisfy the bounded capability contract",
        ));
    }
    let store = require_store(&state)?;
    let resource = store
        .configure_tool_capability(
            &context.api_key_id,
            &tool_name,
            &request.description,
            request.input_schema.as_ref(),
            request.output_schema.as_ref(),
            request.requires_approval,
            &request.risk_level,
            request.expected_current_sha256.as_deref(),
        )
        .map_err(map_policy_error)?;
    Ok(response(resource))
}

pub(crate) async fn api_tool_allowlist_policy(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Path(profile_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    validate_resource_id(&profile_id)?;
    let store = require_store(&state)?;
    store
        .read_tool_allowlist_policy(&profile_id)
        .map_err(internal_error)?
        .map(response)
        .ok_or_else(|| not_found("allowlist", &profile_id))
}

pub(crate) async fn api_configure_tool_allowlist(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Path(profile_id): Path<String>,
    Json(request): Json<ToolAllowlistPolicyApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    validate_resource_id(&profile_id)?;
    validate_confirmation(request.confirm_tool_policy)?;
    validate_expected_hash(request.expected_current_sha256.as_deref())?;
    validate_bounded_json(&request)?;
    if request.tool_names.len() > MAX_TOOL_NAMES
        || request
            .tool_names
            .iter()
            .any(|tool_name| !bounded_identifier(tool_name))
        || request.tool_names.iter().collect::<HashSet<_>>().len() != request.tool_names.len()
    {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "invalid_tool_allowlist_policy",
            "tool_names must contain at most 128 unique bounded identifiers",
        ));
    }
    let store = require_store(&state)?;
    let resource = store
        .configure_tool_allowlist(
            &context.api_key_id,
            &profile_id,
            &request.tool_names,
            request.expected_current_sha256.as_deref(),
        )
        .map_err(map_policy_error)?;
    Ok(response(resource))
}

pub(crate) async fn api_tool_hook_policy(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Path(hook_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    validate_resource_id(&hook_id)?;
    let store = require_store(&state)?;
    store
        .read_tool_hook_policy(&hook_id)
        .map_err(internal_error)?
        .map(response)
        .ok_or_else(|| not_found("hook", &hook_id))
}

pub(crate) async fn api_configure_tool_hook(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Path(hook_id): Path<String>,
    Json(request): Json<ToolHookPolicyApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    validate_resource_id(&hook_id)?;
    validate_confirmation(request.confirm_tool_policy)?;
    validate_expected_hash(request.expected_current_sha256.as_deref())?;
    validate_bounded_json(&request)?;
    let valid_target = request.tool_name.as_deref().is_none_or(bounded_identifier);
    if !valid_target {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "invalid_tool_hook_policy",
            "hook type, action, and tool target must satisfy the bounded hook contract",
        ));
    }
    validate_tool_hook_contract(
        &request.hook_type,
        request.condition.as_ref(),
        &request.action,
        request.action_config.as_ref(),
    )
    .map_err(|error| {
        ApiError::with_code(StatusCode::BAD_REQUEST, "invalid_tool_hook_policy", error)
    })?;
    let store = require_store(&state)?;
    let resource = store
        .configure_tool_hook(
            &context.api_key_id,
            &hook_id,
            &request.hook_type,
            request.tool_name.as_deref(),
            request.condition.as_ref(),
            &request.action,
            request.action_config.as_ref(),
            request.enabled,
            request.expected_current_sha256.as_deref(),
        )
        .map_err(map_policy_error)?;
    Ok(response(resource))
}
