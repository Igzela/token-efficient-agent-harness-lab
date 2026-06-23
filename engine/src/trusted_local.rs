use serde::{Deserialize, Serialize};

use crate::infrastructure::auth::validate_token_shape;
use crate::provider::adaptive_execution::{
    adaptive_provider_endpoint_configs_from_sources, AdaptiveProviderEndpointConfig,
    ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON,
};

pub const TRUSTED_LOCAL_PROFILE_SCHEMA_VERSION: &str = "trusted_local_profile.v1";
pub const TRUSTED_LOCAL_TASK_ADVANCEMENT_SCHEMA_VERSION: &str = "trusted_local_task_advancement.v1";
pub const ACP_TRUSTED_LOCAL_PROFILE: &str = "ACP_TRUSTED_LOCAL_PROFILE";
pub const ACP_TRUSTED_LOCAL_TASK_ADVANCEMENT: &str = "ACP_TRUSTED_LOCAL_TASK_ADVANCEMENT";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrustedLocalProfileInput {
    pub requested: bool,
    pub auth_configured: bool,
    pub endpoint_configured: bool,
    pub credentials_available: bool,
    pub pricing_configured: bool,
    pub per_dispatch_cost_cap_configured: bool,
    pub daily_cost_cap_configured: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedLocalCapabilities {
    pub provider_execution: bool,
    pub adaptive_execution: bool,
    pub default_routing: bool,
    pub experiments: bool,
    pub auto_promotion: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedLocalProfileStatus {
    pub schema_version: String,
    pub requested: bool,
    pub ready: bool,
    pub blockers: Vec<String>,
    pub capabilities: TrustedLocalCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedLocalTaskAdvancementStatus {
    pub schema_version: String,
    pub requested: bool,
    pub ready: bool,
    pub blockers: Vec<String>,
    pub executor_type: String,
    pub worker_count: usize,
    pub max_concurrent: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveExecutionGates {
    pub profile: TrustedLocalProfileStatus,
    pub task_advancement: TrustedLocalTaskAdvancementStatus,
    pub provider_execution: bool,
    pub adaptive_execution: bool,
    pub default_routing: bool,
    pub experiments_enabled: bool,
    pub experiments_active: bool,
    pub auto_promotion_enabled: bool,
    pub auto_promotion_active: bool,
    pub scheduler_enabled: bool,
    pub supervised_workers_enabled: bool,
}

impl EffectiveExecutionGates {
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup<F>(lookup: F) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        Self::from_lookup_with_endpoint_configs(lookup, None)
    }

    pub fn from_lookup_with_endpoint_configs<F>(
        lookup: F,
        endpoint_configs: Option<&[AdaptiveProviderEndpointConfig]>,
    ) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        let profile =
            TrustedLocalProfileStatus::from_lookup_with_endpoint_configs(&lookup, endpoint_configs);
        let task_advancement =
            TrustedLocalTaskAdvancementStatus::from_profile_lookup(&profile, &lookup);
        Self {
            provider_execution: env_flag(&lookup, "ACP_ENABLE_PROVIDER_EXECUTION")
                || profile.capabilities.provider_execution,
            adaptive_execution: env_flag(&lookup, "ACP_ENABLE_ADAPTIVE_FUSION_EXECUTION")
                || profile.capabilities.adaptive_execution,
            default_routing: env_flag(&lookup, "ACP_ADAPTIVE_DEFAULT_LIVE_ROUTING")
                || profile.capabilities.default_routing,
            experiments_enabled: env_flag(&lookup, "ACP_ENABLE_ADAPTIVE_EXPERIMENTS")
                || profile.capabilities.experiments,
            experiments_active: env_flag(&lookup, "ACP_ADAPTIVE_EXPERIMENTS_ACTIVE")
                || profile.capabilities.experiments,
            auto_promotion_enabled: env_flag(&lookup, "ACP_ENABLE_ADAPTIVE_AUTO_PROMOTION")
                || profile.capabilities.auto_promotion,
            auto_promotion_active: env_flag(&lookup, "ACP_ADAPTIVE_AUTO_PROMOTION_ACTIVE")
                || profile.capabilities.auto_promotion,
            scheduler_enabled: env_flag(&lookup, "ACP_ENABLE_SCHEDULER") || task_advancement.ready,
            supervised_workers_enabled: env_flag(&lookup, "ACP_ENABLE_SUPERVISED_WORKERS")
                || task_advancement.ready,
            profile,
            task_advancement,
        }
    }
}

impl TrustedLocalTaskAdvancementStatus {
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup<F>(lookup: F) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        let profile = TrustedLocalProfileStatus::from_lookup(&lookup);
        Self::from_profile_lookup(&profile, &lookup)
    }

    fn from_profile_lookup<F>(profile: &TrustedLocalProfileStatus, lookup: &F) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        let requested = env_flag(lookup, ACP_TRUSTED_LOCAL_TASK_ADVANCEMENT);
        let executor_type =
            lookup("ACP_SCHEDULER_EXECUTOR").unwrap_or_else(|| "adaptive_provider".to_string());
        let (worker_count, worker_count_valid) =
            configured_usize(lookup("ACP_SUPERVISED_WORKER_COUNT").as_deref(), 1);
        let (max_concurrent, max_concurrent_valid) =
            configured_usize(lookup("ACP_SCHEDULER_MAX_CONCURRENT").as_deref(), 4);
        let (interval_ms, interval_valid) =
            configured_u64(lookup("ACP_SCHEDULER_INTERVAL_MS").as_deref(), 2_000);
        let (lease_timeout_ms, lease_timeout_valid) =
            configured_u64(lookup("ACP_SCHEDULER_LEASE_TIMEOUT_MS").as_deref(), 300_000);

        let mut blockers = Vec::new();
        if requested {
            if !profile.ready {
                blockers.push("trusted_local_profile_not_ready".to_string());
            }
            if executor_type != "adaptive_provider" {
                blockers.push("scheduler_executor_not_adaptive_provider".to_string());
            }
            if worker_count_valid && max_concurrent_valid && worker_count > max_concurrent {
                blockers.push("worker_count_exceeds_max_concurrent".to_string());
            }
            if !worker_count_valid || worker_count > 32 {
                blockers.push("worker_count_out_of_bounds".to_string());
            }
            if !max_concurrent_valid || max_concurrent > 32 {
                blockers.push("scheduler_max_concurrent_out_of_bounds".to_string());
            }
            if !interval_valid || !(250..=60_000).contains(&interval_ms) {
                blockers.push("scheduler_interval_out_of_bounds".to_string());
            }
            if !lease_timeout_valid || !(1_000..=3_600_000).contains(&lease_timeout_ms) {
                blockers.push("scheduler_lease_timeout_out_of_bounds".to_string());
            }
        }

        Self {
            schema_version: TRUSTED_LOCAL_TASK_ADVANCEMENT_SCHEMA_VERSION.to_string(),
            requested,
            ready: requested && blockers.is_empty(),
            blockers,
            executor_type,
            worker_count,
            max_concurrent,
        }
    }
}

impl TrustedLocalProfileStatus {
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup<F>(lookup: F) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        Self::from_lookup_with_endpoint_configs(lookup, None)
    }

    pub fn from_lookup_with_endpoint_configs<F>(
        lookup: F,
        persisted_endpoint_configs: Option<&[AdaptiveProviderEndpointConfig]>,
    ) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        let requested = env_flag(&lookup, ACP_TRUSTED_LOCAL_PROFILE);
        if !requested {
            return Self::resolve(TrustedLocalProfileInput {
                requested,
                auth_configured: false,
                endpoint_configured: false,
                credentials_available: false,
                pricing_configured: false,
                per_dispatch_cost_cap_configured: false,
                daily_cost_cap_configured: false,
            });
        }

        let env_endpoint_config = lookup(ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON);
        let endpoint_configs = adaptive_provider_endpoint_configs_from_sources(
            env_endpoint_config.as_deref(),
            persisted_endpoint_configs,
        )
        .ok()
        .flatten();
        let endpoint_configured = endpoint_configs
            .as_ref()
            .is_some_and(|configs| !configs.is_empty());
        let credentials_available = endpoint_configs.as_ref().is_some_and(|configs| {
            configs.iter().all(|config| {
                config.provider_type == "stub"
                    || config
                        .credential_env
                        .as_deref()
                        .and_then(&lookup)
                        .is_some_and(|value| !value.trim().is_empty())
            })
        });
        let pricing_configured = endpoint_configs.as_ref().is_some_and(|configs| {
            configs.iter().all(|config| {
                matches!(
                    (config.input_cost_per_1k_usd, config.output_cost_per_1k_usd),
                    (Some(input), Some(output)) if input > 0.0 && output > 0.0
                )
            })
        });
        let auth_configured = env_flag(&lookup, "ACP_REQUIRE_AUTH")
            && lookup("ACP_ADMIN_API_KEY")
                .as_deref()
                .is_some_and(validate_token_shape);

        Self::resolve(TrustedLocalProfileInput {
            requested,
            auth_configured,
            endpoint_configured,
            credentials_available,
            pricing_configured,
            per_dispatch_cost_cap_configured: positive_f64(
                lookup("ACP_COST_PER_DISPATCH_USD").as_deref(),
            ),
            daily_cost_cap_configured: positive_f64(lookup("ACP_COST_DAILY_USD").as_deref()),
        })
    }

    pub fn resolve(input: TrustedLocalProfileInput) -> Self {
        let mut blockers = Vec::new();
        if input.requested {
            if !input.auth_configured {
                blockers.push("auth_not_configured".to_string());
            }
            if !input.daily_cost_cap_configured {
                blockers.push("daily_cost_cap_not_configured".to_string());
            }
            if !input.endpoint_configured {
                blockers.push("endpoint_not_configured".to_string());
            }
            if !input.pricing_configured {
                blockers.push("endpoint_pricing_not_configured".to_string());
            }
            if !input.per_dispatch_cost_cap_configured {
                blockers.push("per_dispatch_cost_cap_not_configured".to_string());
            }
            if !input.credentials_available {
                blockers.push("provider_credential_not_available".to_string());
            }
        }
        let ready = input.requested && blockers.is_empty();

        Self {
            schema_version: TRUSTED_LOCAL_PROFILE_SCHEMA_VERSION.to_string(),
            requested: input.requested,
            ready,
            blockers,
            capabilities: TrustedLocalCapabilities {
                provider_execution: ready,
                adaptive_execution: ready,
                default_routing: ready,
                experiments: ready,
                auto_promotion: ready,
            },
        }
    }
}

fn env_flag<F>(lookup: &F, key: &str) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    lookup(key).is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
}

fn positive_f64(value: Option<&str>) -> bool {
    value
        .and_then(|value| value.trim().parse::<f64>().ok())
        .is_some_and(|value| value.is_finite() && value > 0.0)
}

fn configured_usize(value: Option<&str>, default: usize) -> (usize, bool) {
    match value {
        None => (default, true),
        Some(value) => value
            .trim()
            .parse::<usize>()
            .ok()
            .filter(|value| *value > 0)
            .map_or((0, false), |value| (value, true)),
    }
}

fn configured_u64(value: Option<&str>, default: u64) -> (u64, bool) {
    match value {
        None => (default, true),
        Some(value) => value
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .map_or((0, false), |value| (value, true)),
    }
}
