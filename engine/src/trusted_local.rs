use serde::{Deserialize, Serialize};

use crate::infrastructure::auth::validate_token_shape;
use crate::provider::adaptive_execution::{
    parse_adaptive_provider_endpoints_json, ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON,
};

pub const TRUSTED_LOCAL_PROFILE_SCHEMA_VERSION: &str = "trusted_local_profile.v1";
pub const ACP_TRUSTED_LOCAL_PROFILE: &str = "ACP_TRUSTED_LOCAL_PROFILE";

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
pub struct EffectiveExecutionGates {
    pub profile: TrustedLocalProfileStatus,
    pub provider_execution: bool,
    pub adaptive_execution: bool,
    pub default_routing: bool,
    pub experiments_enabled: bool,
    pub experiments_active: bool,
    pub auto_promotion_enabled: bool,
    pub auto_promotion_active: bool,
}

impl EffectiveExecutionGates {
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup<F>(lookup: F) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        let profile = TrustedLocalProfileStatus::from_lookup(&lookup);
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
            profile,
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

        let endpoint_configs = lookup(ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON)
            .filter(|value| !value.trim().is_empty())
            .and_then(|value| parse_adaptive_provider_endpoints_json(&value).ok());
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
