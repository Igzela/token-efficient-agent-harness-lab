use axum::extract::{Extension, State};
use axum::http::{HeaderMap, Uri};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::feedback::{AdaptiveAutoPromotionPolicy, AdaptiveExperimentPolicy};
use crate::http_server::middleware::{
    authorize, cors_headers, internal_error, ApiError, RequestId,
};
use crate::http_server::state::AxumApiState;
use crate::provider::config::provider_pricing_from_env;
use crate::provider::cost_gate::CostGateConfig;
use crate::storage::local_product_store::local_boundaries;

const ADAPTIVE_FUSION_OPERATOR_STATUS_SCHEMA_VERSION: &str = "adaptive_fusion_operator_status.v1";

pub(crate) async fn api_dashboard(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    uri: Uri,
    Extension(request_id): Extension<RequestId>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "health:read", uri.path(), &request_id.0)?;
    let exec_type = state.executor_type();
    let execution_gates = state.effective_execution_gates();
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
    let workers_running = state
        .scheduler
        .as_ref()
        .and_then(|scheduler| scheduler.lock().ok())
        .is_some_and(|scheduler| scheduler.is_running());
    if workers_running {
        if let Some(boundaries) = body.get_mut("boundaries").and_then(|v| v.as_object_mut()) {
            boundaries.insert("runtime_workers".to_string(), json!("enabled"));
        }
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
    let effective_gates = state.effective_execution_gates();
    let provider_execution = effective_gates.provider_execution;
    let adaptive_execution = effective_gates.adaptive_execution;
    let auth = state.tenant_resolver.is_some();
    let fusion_kill_switch = env_enabled("ACP_ADAPTIVE_FUSION_KILL_SWITCH");
    let executor_configured = state.adaptive_provider_executor.is_some();
    let registry_configured = state.adaptive_registry_snapshot.is_some();
    let storage_configured = state.local_store.is_some();
    let default_routing_enabled = effective_gates.default_routing;
    let experiment_policy = AdaptiveExperimentPolicy::from_env();
    let experiment_policy_blockers = experiment_policy.validation_errors();
    let auto_promotion_policy = AdaptiveAutoPromotionPolicy::from_env();
    let mut auto_promotion_policy_blockers = auto_promotion_policy.validation_errors();
    let auto_promotion_rollout_percentage =
        env_u8("ACP_ADAPTIVE_AUTO_PROMOTION_ROLLOUT_PERCENTAGE", 10);
    if !(1..=100).contains(&auto_promotion_rollout_percentage) {
        auto_promotion_policy_blockers.push("invalid_rollout_percentage".to_string());
    }
    let cost_gate = CostGateConfig::from_env();
    let scheduler = scheduler_operator_status(state)?;
    let scheduler_running = scheduler["running"].as_bool().unwrap_or(false);
    let scheduler_paused = scheduler["paused"].as_bool().unwrap_or(false);
    let scheduler_killed = scheduler["kill_requested"].as_bool().unwrap_or(false);

    let (
        active_policy_count,
        snapshot_count,
        active_snapshot_count,
        observation_count,
        observation_success_count,
        observation_total_cost_usd,
        latest_observation_at,
        today_cost_usd,
    ) = if let Some(store) = &state.local_store {
        let policies = store.active_adaptive_fusion_policies()?;
        let snapshots = store.adaptive_fusion_policy_snapshots()?;
        let active_snapshots = snapshots
            .iter()
            .filter(|snapshot| snapshot.status == "active")
            .count();
        let observations = store.adaptive_observations()?;
        let success_count = observations
            .iter()
            .filter(|observation| observation.success)
            .count();
        let observation_cost = observations
            .iter()
            .map(|observation| observation.cost_usd)
            .sum::<f64>();
        let latest_at = observations
            .iter()
            .map(|observation| observation.created_at.as_str())
            .max()
            .map(str::to_string);
        let today_prefix = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let today_observation_cost = observations
            .iter()
            .filter(|observation| observation.created_at.starts_with(&today_prefix))
            .map(|observation| observation.cost_usd)
            .sum::<f64>();
        let today_cost = store.daily_estimated_cost_usd(&today_prefix)? + today_observation_cost;
        (
            policies.len(),
            snapshots.len(),
            active_snapshots,
            observations.len(),
            success_count,
            observation_cost,
            latest_at,
            today_cost,
        )
    } else {
        (0, 0, 0, 0, 0, 0.0, None, 0.0)
    };
    let provider_execution_active = provider_execution && auth && !fusion_kill_switch;
    let adaptive_execution_active = provider_execution_active && adaptive_execution;
    let completion_ready = provider_execution
        && adaptive_execution
        && auth
        && executor_configured
        && registry_configured
        && storage_configured
        && !fusion_kill_switch;
    let experiments_active = completion_ready
        && effective_gates.experiments_active
        && !env_enabled("ACP_ADAPTIVE_EXPERIMENTS_PAUSED")
        && !env_enabled("ACP_ADAPTIVE_EXPERIMENTS_KILL_SWITCH")
        && experiment_policy_blockers.is_empty();
    let auto_promotion_active = completion_ready
        && effective_gates.auto_promotion_active
        && !env_enabled("ACP_ADAPTIVE_AUTO_PROMOTION_KILL_SWITCH")
        && auto_promotion_policy_blockers.is_empty();
    let task_advancement_active = effective_gates.task_advancement.ready
        && scheduler_running
        && !scheduler_paused
        && !scheduler_killed;
    let daily_cost_remaining_usd = cost_gate
        .daily_cap_usd
        .map(|cap| (cap - today_cost_usd).max(0.0));

    Ok(json!({
        "schema_version": ADAPTIVE_FUSION_OPERATOR_STATUS_SCHEMA_VERSION,
        "trusted_local_profile": effective_gates.profile,
        "trusted_local_task_advancement": effective_gates.task_advancement,
        "completion_api": {
            "available": true,
            "ready_for_live_completion": completion_ready,
            "executor_configured": executor_configured,
            "registry_configured": registry_configured,
            "storage_configured": storage_configured,
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
        "authority": {
            "provider_execution_active": provider_execution_active,
            "adaptive_execution_active": adaptive_execution_active,
            "default_routing_active": default_routing_enabled && completion_ready,
            "experiments_active": experiments_active,
            "auto_promotion_active": auto_promotion_active,
            "task_advancement_active": task_advancement_active,
        },
        "bounds": {
            "per_dispatch_cost_cap_usd": cost_gate.per_dispatch_cap_usd,
            "daily_cost_cap_usd": cost_gate.daily_cap_usd,
            "today_cost_usd": today_cost_usd,
            "daily_cost_remaining_usd": daily_cost_remaining_usd,
            "experiment_traffic_rate": experiment_policy.traffic_rate,
            "experiment_max_cost_usd": experiment_policy.max_cost_usd,
            "experiment_max_total_tokens": experiment_policy.max_total_tokens,
            "experiment_max_calls": experiment_policy.max_calls,
            "experiment_max_elapsed_ms": experiment_policy.max_elapsed_ms,
            "experiment_max_concurrency": experiment_policy.max_concurrency,
            "experiment_policy_valid": experiment_policy_blockers.is_empty(),
            "experiment_policy_blockers": experiment_policy_blockers,
            "auto_promotion_rollout_percentage": auto_promotion_rollout_percentage,
            "auto_promotion_policy_valid": auto_promotion_policy_blockers.is_empty(),
            "auto_promotion_policy_blockers": auto_promotion_policy_blockers,
            "worker_count": effective_gates.task_advancement.worker_count,
            "worker_max_concurrent": effective_gates.task_advancement.max_concurrent,
        },
        "observations": {
            "count": observation_count,
            "success_count": observation_success_count,
            "failure_count": observation_count.saturating_sub(observation_success_count),
            "total_cost_usd": observation_total_cost_usd,
            "latest_at": latest_observation_at,
        },
        "scheduler": scheduler,
    }))
}

fn scheduler_operator_status(state: &AxumApiState) -> Result<serde_json::Value, String> {
    let Some(scheduler) = &state.scheduler else {
        return Ok(json!({
            "enabled": false,
            "running": false,
            "supervised_workers_enabled": false,
            "paused": false,
            "kill_requested": false,
            "worker_count": 0,
            "max_concurrent": 0,
            "executor_type": null,
            "active_runs": 0,
            "tick_count": 0,
            "error_count": 0,
            "last_tick_at": null,
        }));
    };
    let status = scheduler
        .lock()
        .map_err(|_| "scheduler status unavailable".to_string())?
        .status();
    Ok(json!({
        "enabled": true,
        "running": status["running"],
        "supervised_workers_enabled": status["supervised_workers_enabled"],
        "paused": status["paused"],
        "kill_requested": status["kill_requested"],
        "worker_count": status["worker_count"],
        "max_concurrent": status["config"]["max_concurrent"],
        "executor_type": status["config"]["executor_type"],
        "active_runs": status["active_runs"],
        "tick_count": status["tick_count"],
        "error_count": status["error_count"],
        "last_tick_at": status["last_tick_at"],
    }))
}

fn env_enabled(key: &str) -> bool {
    std::env::var(key)
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn env_u8(key: &str, default: u8) -> u8 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<u8>().ok())
        .unwrap_or(default)
}
