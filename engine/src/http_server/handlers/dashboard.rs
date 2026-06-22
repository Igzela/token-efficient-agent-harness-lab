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
use crate::trusted_local::EffectiveExecutionGates;

const ADAPTIVE_FUSION_OPERATOR_STATUS_SCHEMA_VERSION: &str = "adaptive_fusion_operator_status.v1";

pub(crate) async fn api_dashboard(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "health:read", uri.path(), &request_id.0)?;
    let exec_type = state.executor_type();
    let execution_gates = EffectiveExecutionGates::from_env();
    let prov_enabled = state.provider_enabled()
        || (execution_gates.provider_execution && state.adaptive_provider_executor.is_some());
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
    if let Some(object) = body.as_object_mut() {
        object.insert("cli".to_string(), json!(state.cli_capability()));
        object.insert(
            "adaptive_fusion".to_string(),
            adaptive_fusion_operator_status(&state).map_err(internal_error)?,
        );
    }
    Ok((cors_headers(), Json(body)))
}

fn adaptive_fusion_operator_status(state: &AxumApiState) -> Result<serde_json::Value, String> {
    let effective_gates = EffectiveExecutionGates::from_env();
    let provider_execution = effective_gates.provider_execution;
    let adaptive_execution = effective_gates.adaptive_execution;
    let auth = state.tenant_resolver.is_some();
    let fusion_kill_switch = env_enabled("ACP_ADAPTIVE_FUSION_KILL_SWITCH");
    let executor_configured = state.adaptive_provider_executor.is_some();
    let registry_configured = state.adaptive_registry_snapshot.is_some();
    let default_routing_enabled = effective_gates.default_routing;

    let (active_policy_count, snapshot_count, active_snapshot_count) =
        if let Some(store) = &state.local_store {
            let policies = store.active_adaptive_fusion_policies()?;
            let snapshots = store.adaptive_fusion_policy_snapshots()?;
            let active_snapshots = snapshots
                .iter()
                .filter(|snapshot| snapshot.status == "active")
                .count();
            (policies.len(), snapshots.len(), active_snapshots)
        } else {
            (0, 0, 0)
        };

    Ok(json!({
        "schema_version": ADAPTIVE_FUSION_OPERATOR_STATUS_SCHEMA_VERSION,
        "trusted_local_profile": effective_gates.profile,
        "completion_api": {
            "available": true,
            "ready_for_live_completion": provider_execution
                && adaptive_execution
                && auth
                && executor_configured
                && registry_configured
                && !fusion_kill_switch,
            "executor_configured": executor_configured,
            "registry_configured": registry_configured,
            "default_routing_enabled": default_routing_enabled,
        },
        "gates": {
            "provider_execution": provider_execution,
            "adaptive_execution": adaptive_execution,
            "auth": auth,
            "fusion_kill_switch": fusion_kill_switch,
            "experiments_enabled": effective_gates.experiments_enabled,
            "experiments_active": effective_gates.experiments_active,
            "experiments_paused": env_enabled("ACP_ADAPTIVE_EXPERIMENTS_PAUSED"),
            "experiments_kill_switch": env_enabled("ACP_ADAPTIVE_EXPERIMENTS_KILL_SWITCH"),
            "auto_promotion_enabled": effective_gates.auto_promotion_enabled,
            "auto_promotion_active": effective_gates.auto_promotion_active,
            "auto_promotion_kill_switch": env_enabled("ACP_ADAPTIVE_AUTO_PROMOTION_KILL_SWITCH"),
        },
        "policy": {
            "active_policy_count": active_policy_count,
            "snapshot_count": snapshot_count,
            "active_snapshot_count": active_snapshot_count,
            "live_execution_authority": false,
            "requires_explicit_adaptive_plan": true,
        },
    }))
}

fn env_enabled(key: &str) -> bool {
    std::env::var(key)
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}
