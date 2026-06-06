use axum::extract::{Extension, State};
use axum::http::{HeaderMap, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::http_server::middleware::{RequestId, 
    authorize, backup_dir_for_state, cors_headers, internal_error, ApiError,
};
use crate::http_server::state::AxumApiState;
use crate::http_server::AXUM_API_SCHEMA_VERSION;
use crate::provider::config::provider_pricing_from_env;
use crate::storage::backup_manager::BackupManager;

pub(crate) async fn api_metrics(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "health:read", uri.path(), &request_id.0)?;

    let mut dispatch_count = 0;
    let mut plan_count = 0;
    let mut workflow_run_count = 0;
    let mut audit_event_count = 0;
    let mut api_key_count = 0;
    let mut backup_count = 0;
    let mut total_reserved_cost = 0.0;
    let mut total_estimated_cost_usd = 0.0;
    let mut total_input_tokens = 0;
    let mut total_output_tokens = 0;
    let mut estimated_cost_available = false;
    let mut latest_backup_created_at = serde_json::Value::Null;
    let pricing_configured = provider_pricing_from_env().configured();

    if let Some(store) = &state.local_store {
        let stats = store.stats().map_err(internal_error)?;
        dispatch_count = stats["dispatches"].as_i64().unwrap_or(0);
        plan_count = stats["plans"].as_i64().unwrap_or(0);
        workflow_run_count = stats["workflow_runs"].as_i64().unwrap_or(0);
        audit_event_count = stats["audit_events"].as_i64().unwrap_or(0);
        api_key_count = stats["api_keys"].as_i64().unwrap_or(0);

        let costs = store.cost_summary().map_err(internal_error)?;
        total_reserved_cost = costs["total_reserved_cost"].as_f64().unwrap_or(0.0);
        total_estimated_cost_usd = costs["total_estimated_cost_usd"].as_f64().unwrap_or(0.0);
        total_input_tokens = costs["total_input_tokens"].as_i64().unwrap_or(0);
        total_output_tokens = costs["total_output_tokens"].as_i64().unwrap_or(0);
        estimated_cost_available = costs["estimated_cost_available"].as_bool().unwrap_or(false);

        if !store.is_memory() {
            let backup_dir = backup_dir_for_state(&state, store.db_path());
            let manager = BackupManager::new(&backup_dir).map_err(internal_error)?;
            let backups = manager.list_backups().map_err(internal_error)?;
            backup_count = backups.len() as i64;
            latest_backup_created_at = backups
                .iter()
                .max_by(|a, b| a.created_at.cmp(&b.created_at))
                .map(|b| json!(b.created_at))
                .unwrap_or(serde_json::Value::Null);
        }
    }

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "executor_type": state.executor_type(),
            "auth_required": state.tenant_resolver.is_some(),
            "provider_enabled": state.provider_enabled(),
            "local_store": state.local_store.is_some(),
            "dispatch_count": dispatch_count,
            "plan_count": plan_count,
            "workflow_run_count": workflow_run_count,
            "audit_event_count": audit_event_count,
            "api_key_count": api_key_count,
            "backup_count": backup_count,
            "latest_backup_created_at": latest_backup_created_at,
            "total_reserved_cost": total_reserved_cost,
            "total_estimated_cost_usd": total_estimated_cost_usd,
            "total_input_tokens": total_input_tokens,
            "total_output_tokens": total_output_tokens,
            "pricing_configured": pricing_configured,
            "estimated_cost_available": estimated_cost_available,
            "boundaries": crate::storage::local_product_store::local_boundaries(
                state.executor_type(),
                state.provider_enabled(),
            ),
        })),
    ))
}
