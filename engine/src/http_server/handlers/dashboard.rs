use axum::extract::{Extension, State};
use axum::http::{HeaderMap, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, ApiError, RequestId,
};
use crate::http_server::state::AxumApiState;
use crate::provider::config::provider_pricing_from_env;
use crate::storage::local_product_store::local_boundaries;

pub(crate) async fn api_dashboard(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "health:read", uri.path(), &request_id.0)?;
    let exec_type = state.executor_type();
    let prov_enabled = state.provider_enabled();
    let mut body = if let Some(store) = &state.local_store {
        store
            .dashboard_snapshot(20, exec_type, prov_enabled)
            .map_err(internal_error)?
    } else {
        json!({
            "schema_version": "local_dashboard.v1",
            "status": "ready",
            "counts": {
                "dispatches": 0,
                "plans": 0,
                "workflow_runs": 0,
                "team_members": 0,
                "api_keys": 0,
                "audit_events": 0,
            },
            "dispatches": [],
            "team": {"schema_version": "local_team.v1", "members": [], "api_keys": []},
            "config": {},
            "costs": {
                "schema_version": "local_cost_summary.v2",
                "currency": "USD",
                "dispatch_count": 0,
                "total_reserved_cost": 0.0,
                "total_estimated_cost_usd": 0.0,
                "total_input_tokens": 0,
                "total_output_tokens": 0,
                "estimated_cost_available": false,
                "pricing_configured": provider_pricing_from_env().configured(),
                "cost_utilization": 0.0,
                "by_tier": [],
                "daily": [],
            },
            "boundaries": local_boundaries(exec_type, prov_enabled),
        })
    };
    if let Some(costs) = body.get_mut("costs").and_then(|v| v.as_object_mut()) {
        costs.insert(
            "pricing_configured".to_string(),
            json!(provider_pricing_from_env().configured()),
        );
    }
    Ok((cors_headers(), Json(body)))
}
