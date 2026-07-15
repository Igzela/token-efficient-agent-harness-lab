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
use crate::storage::local_product_store::{
    DurableMemoryCreate, DurableMemoryRevision, MemoryRetrievalRequest, MemoryScope,
    ProviderEmbeddingResolutionRequest,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MemoryVersionRequest {
    expected_version: i64,
    run_id: String,
    scope: MemoryScope,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MemoryRevisionApiRequest {
    run_id: String,
    scope: MemoryScope,
    expected_version: i64,
    source_id: String,
    source_sha256: String,
    content: serde_json::Value,
    confidence: f64,
    fresh_until: Option<String>,
    expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MemorySupersedeRequest {
    winner_expected_version: i64,
    loser_memory_id: String,
    loser_expected_version: i64,
    run_id: String,
    scope: MemoryScope,
    confirm_supersede: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MemoryPruneRequest {
    scope: crate::storage::local_product_store::MemoryScope,
    run_id: String,
    confirm_prune: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MemoryDetailQuery {
    run_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MemoryReembedRequest {
    expected_version: i64,
    run_id: String,
    scope: MemoryScope,
    confirm_reembed: bool,
}

pub(crate) async fn api_create_memory(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Json(request): Json<DurableMemoryCreate>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    require_tenant(&context.tenant_id, &request.scope.tenant_id)?;
    let run_id = request.run_id.as_deref().ok_or_else(|| {
        ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "durable_memory_run_binding_required",
            "API memory creation requires an authoritative run_id",
        )
    })?;
    require_run_scope(&state, run_id, &request.scope)?;
    let memory = require_store(&state)?
        .create_durable_memory(&request, &context.api_key_id)
        .map_err(bad_memory_request)?;
    Ok((
        StatusCode::CREATED,
        cors_headers(),
        Json(json!({"memory": memory})),
    ))
}

pub(crate) async fn api_supersede_memory(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(winner_memory_id): AxumPath<String>,
    Json(request): Json<MemorySupersedeRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    if !request.confirm_supersede {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "durable_memory_supersede_confirmation_required",
            "confirm_supersede must be true",
        ));
    }
    require_run_scope(&state, &request.run_id, &request.scope)?;
    require_memory_scope(
        &state,
        &winner_memory_id,
        &request.scope,
        &context.tenant_id,
    )?;
    require_memory_scope(
        &state,
        &request.loser_memory_id,
        &request.scope,
        &context.tenant_id,
    )?;
    let result = require_store(&state)?
        .supersede_durable_memory(
            &winner_memory_id,
            request.winner_expected_version,
            &request.loser_memory_id,
            request.loser_expected_version,
            &context.api_key_id,
        )
        .map_err(bad_memory_request)?;
    Ok((cors_headers(), Json(result)))
}

pub(crate) async fn api_prune_memories(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Json(request): Json<MemoryPruneRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    require_tenant(&context.tenant_id, &request.scope.tenant_id)?;
    require_run_scope(&state, &request.run_id, &request.scope)?;
    if !request.confirm_prune {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "durable_memory_prune_confirmation_required",
            "confirm_prune must be true",
        ));
    }
    let result = require_store(&state)?
        .prune_expired_durable_memories(&request.scope, &context.api_key_id)
        .map_err(bad_memory_request)?;
    Ok((cors_headers(), Json(result)))
}

pub(crate) async fn api_memory_detail(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(memory_id): AxumPath<String>,
    Query(query): Query<MemoryDetailQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    let versions = require_store(&state)?
        .inspect_durable_memory(&memory_id)
        .map_err(bad_memory_request)?;
    if versions.is_empty() {
        return Err(ApiError::with_code(
            StatusCode::NOT_FOUND,
            "durable_memory_not_found",
            "durable memory not found",
        ));
    }
    let scope = memory_scope_from_versions(&versions)?;
    require_run_scope(&state, &query.run_id, &scope)?;
    require_tenant(&context.tenant_id, &scope.tenant_id)?;
    Ok((cors_headers(), Json(json!({"versions": versions}))))
}

pub(crate) async fn api_revise_memory(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(memory_id): AxumPath<String>,
    Json(request): Json<MemoryRevisionApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    require_run_scope(&state, &request.run_id, &request.scope)?;
    require_memory_scope(&state, &memory_id, &request.scope, &context.tenant_id)?;
    let revision = DurableMemoryRevision {
        expected_version: request.expected_version,
        source_id: request.source_id,
        source_sha256: request.source_sha256,
        content: request.content,
        confidence: request.confidence,
        fresh_until: request.fresh_until,
        expires_at: request.expires_at,
    };
    let memory = require_store(&state)?
        .revise_durable_memory(&memory_id, &revision, &context.api_key_id)
        .map_err(bad_memory_request)?;
    Ok((cors_headers(), Json(json!({"memory": memory}))))
}

pub(crate) async fn api_reembed_memory(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(memory_id): AxumPath<String>,
    Json(request): Json<MemoryReembedRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    if !request.confirm_reembed {
        return Err(ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "durable_memory_reembed_confirmation_required",
            "confirm_reembed must be true",
        ));
    }
    require_run_scope(&state, &request.run_id, &request.scope)?;
    require_memory_scope(&state, &memory_id, &request.scope, &context.tenant_id)?;
    let memory = require_store(&state)?
        .reembed_durable_memory(&memory_id, request.expected_version, &context.api_key_id)
        .map_err(bad_memory_request)?;
    Ok((cors_headers(), Json(json!({"memory":memory}))))
}

pub(crate) async fn api_reconcile_memory_embedding(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(memory_id): AxumPath<String>,
    Json(request): Json<ProviderEmbeddingResolutionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    require_tenant(&context.tenant_id, &request.scope.tenant_id)?;
    let run_id = request.run_id.as_deref().ok_or_else(|| {
        ApiError::with_code(
            StatusCode::BAD_REQUEST,
            "provider_embedding_resolution_run_required",
            "provider embedding reconciliation requires an authoritative run_id",
        )
    })?;
    require_run_scope(&state, run_id, &request.scope)?;
    let resolution = require_store(&state)?
        .reconcile_provider_embedding_operation(&memory_id, &request, &context.api_key_id)
        .map_err(bad_memory_request)?;
    Ok((cors_headers(), Json(json!({"resolution":resolution}))))
}

pub(crate) async fn api_invalidate_memory(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(memory_id): AxumPath<String>,
    Json(request): Json<MemoryVersionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    transition_memory(state, headers, uri, request_id, memory_id, request, false).await
}

pub(crate) async fn api_forget_memory(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    AxumPath(memory_id): AxumPath<String>,
    Json(request): Json<MemoryVersionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    transition_memory(state, headers, uri, request_id, memory_id, request, true).await
}

async fn transition_memory(
    state: AxumApiState,
    headers: HeaderMap,
    uri: Uri,
    request_id: RequestId,
    memory_id: String,
    request: MemoryVersionRequest,
    forget: bool,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(
        &state,
        &headers,
        "dispatch:execute",
        uri.path(),
        &request_id.0,
    )?;
    require_run_scope(&state, &request.run_id, &request.scope)?;
    require_memory_scope(&state, &memory_id, &request.scope, &context.tenant_id)?;
    let store = require_store(&state)?;
    let memory = if forget {
        store.forget_durable_memory(&memory_id, request.expected_version, &context.api_key_id)
    } else {
        store.invalidate_durable_memory(&memory_id, request.expected_version, &context.api_key_id)
    }
    .map_err(bad_memory_request)?;
    Ok((cors_headers(), Json(json!({"memory": memory}))))
}

pub(crate) async fn api_retrieve_memory(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
    Json(request): Json<MemoryRetrievalRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read", uri.path(), &request_id.0)?;
    require_tenant(&context.tenant_id, &request.scope.tenant_id)?;
    let run = require_store(&state)?
        .get_workflow_run(&request.run_id)
        .map_err(internal_error)?
        .ok_or_else(|| {
            ApiError::with_code(StatusCode::NOT_FOUND, "run_not_found", "run not found")
        })?;
    require_tenant(
        &context.tenant_id,
        run.get("tenant_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(""),
    )?;
    if run.get("workspace_id").and_then(serde_json::Value::as_str)
        != Some(request.scope.workspace_id.as_str())
    {
        return Err(ApiError::with_code(
            StatusCode::FORBIDDEN,
            "memory_workspace_scope_mismatch",
            "memory workspace scope mismatch",
        ));
    }
    require_retrieval_node_scope(&run, &request)?;
    let result = require_store(&state)?
        .retrieve_durable_memories(&request, &context.api_key_id)
        .map_err(bad_memory_request)?;
    Ok((cors_headers(), Json(json!({"retrieval": result}))))
}

fn require_memory_scope(
    state: &AxumApiState,
    memory_id: &str,
    expected_scope: &MemoryScope,
    tenant_id: &str,
) -> Result<(), ApiError> {
    let versions = require_store(state)?
        .inspect_durable_memory(memory_id)
        .map_err(bad_memory_request)?;
    let stored = memory_scope_from_versions(&versions)?;
    require_tenant(tenant_id, &stored.tenant_id)?;
    if &stored != expected_scope {
        return Err(ApiError::with_code(
            StatusCode::FORBIDDEN,
            "memory_scope_mismatch",
            "durable memory scope does not match the exact requested scope",
        ));
    }
    Ok(())
}

fn memory_scope_from_versions(versions: &[serde_json::Value]) -> Result<MemoryScope, ApiError> {
    versions
        .last()
        .and_then(|value| value.get("scope"))
        .cloned()
        .ok_or_else(|| {
            ApiError::with_code(
                StatusCode::NOT_FOUND,
                "durable_memory_not_found",
                "durable memory not found",
            )
        })
        .and_then(|value| {
            serde_json::from_value(value).map_err(|error| internal_error(error.to_string()))
        })
}

fn require_retrieval_node_scope(
    run: &serde_json::Value,
    request: &MemoryRetrievalRequest,
) -> Result<(), ApiError> {
    let node = run
        .get("nodes")
        .and_then(serde_json::Value::as_array)
        .and_then(|nodes| {
            nodes.iter().find(|node| {
                node.get("node_id").and_then(serde_json::Value::as_str)
                    == Some(request.node_id.as_str())
            })
        })
        .ok_or_else(|| {
            ApiError::with_code(
                StatusCode::FORBIDDEN,
                "memory_node_scope_mismatch",
                "retrieval node is not owned by the bound run",
            )
        })?;
    if let Some(agent_id) = request.scope.agent_id.as_deref() {
        if node
            .get("assigned_agent_id")
            .and_then(serde_json::Value::as_str)
            != Some(agent_id)
        {
            return Err(ApiError::with_code(
                StatusCode::FORBIDDEN,
                "memory_agent_scope_mismatch",
                "retrieval agent is not assigned to the bound node",
            ));
        }
    }
    if request
        .scope
        .task_id
        .as_deref()
        .is_some_and(|task_id| task_id != request.node_id)
    {
        return Err(ApiError::with_code(
            StatusCode::FORBIDDEN,
            "memory_task_scope_mismatch",
            "retrieval task scope must match the bound node",
        ));
    }
    Ok(())
}

fn require_run_scope(
    state: &AxumApiState,
    run_id: &str,
    scope: &crate::storage::local_product_store::MemoryScope,
) -> Result<(), ApiError> {
    let run = require_store(state)?
        .get_workflow_run(run_id)
        .map_err(internal_error)?
        .ok_or_else(|| {
            ApiError::with_code(StatusCode::NOT_FOUND, "run_not_found", "run not found")
        })?;
    require_tenant(
        &scope.tenant_id,
        run.get("tenant_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(""),
    )?;
    if run.get("workspace_id").and_then(serde_json::Value::as_str)
        != Some(scope.workspace_id.as_str())
    {
        return Err(ApiError::with_code(
            StatusCode::FORBIDDEN,
            "memory_workspace_scope_mismatch",
            "memory workspace scope mismatch",
        ));
    }
    Ok(())
}

fn require_tenant(expected: &str, actual: &str) -> Result<(), ApiError> {
    if expected == actual {
        Ok(())
    } else {
        Err(ApiError::with_code(
            StatusCode::FORBIDDEN,
            "memory_tenant_scope_mismatch",
            "memory tenant scope mismatch",
        ))
    }
}

fn bad_memory_request(error: String) -> ApiError {
    let status = if error.contains("not found") {
        StatusCode::NOT_FOUND
    } else if error.contains("scope mismatch") {
        StatusCode::FORBIDDEN
    } else if error.contains("version conflict") {
        StatusCode::CONFLICT
    } else {
        StatusCode::BAD_REQUEST
    };
    ApiError::with_code(status, "durable_memory_request_rejected", error)
}
