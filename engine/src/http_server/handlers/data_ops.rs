use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, require_store, ApiError,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::{ImportApiRequest, AXUM_API_SCHEMA_VERSION};
use crate::storage::local_product_store::local_boundaries;

pub(crate) async fn api_config(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "config:read")?;
    let store = require_store(&state)?;
    let exec_type = state.executor_type();
    let prov_enabled = state.provider_enabled();
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "config": store.config_snapshot().map_err(internal_error)?,
            "boundaries": local_boundaries(exec_type, prov_enabled),
        })),
    ))
}

pub(crate) async fn api_export(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "export:read")?;
    let store = require_store(&state)?;
    let exec_type = state.executor_type();
    let prov_enabled = state.provider_enabled();
    Ok((
        cors_headers(),
        Json(
            store
                .export_snapshot(exec_type, prov_enabled)
                .map_err(internal_error)?,
        ),
    ))
}

pub(crate) async fn api_integrity(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "health:read")?;
    let store = require_store(&state)?;
    let report = store.check_integrity().map_err(internal_error)?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "integrity": {
                "status": report.status,
                "schema_version": report.schema_version,
                "tables": report.tables.iter().map(|t| json!({
                    "name": t.name,
                    "row_count": t.row_count,
                    "status": t.status,
                })).collect::<Vec<_>>(),
            },
        })),
    ))
}

pub(crate) async fn api_import(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    Json(request): Json<ImportApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if state.tenant_resolver.is_none() {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "admin auth is required for import",
        ));
    }
    let context = authorize(&state, &headers, "config:admin")?;
    if request.confirm_import != Some(true) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "confirm_import must be true",
        ));
    }
    let store = require_store(&state)?;
    let result = store
        .import_snapshot(&request.snapshot)
        .map_err(internal_error)?;
    store
        .append_audit(
            &context.api_key_id,
            "data.import",
            "local_product_store",
            &json!({
                "imported": {
                    "dispatches": result.imported.dispatches,
                    "plans": result.imported.plans,
                    "workflow_runs": result.imported.workflow_runs,
                    "config": result.imported.config,
                    "team": result.imported.team,
                    "audit": result.imported.audit,
                },
                "error_count": result.errors.len(),
            }),
        )
        .map_err(internal_error)?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "imported": {
                "dispatches": result.imported.dispatches,
                "plans": result.imported.plans,
                "workflow_runs": result.imported.workflow_runs,
                "config": result.imported.config,
                "team": result.imported.team,
                "audit": result.imported.audit,
            },
            "errors": result.errors,
        })),
    ))
}
