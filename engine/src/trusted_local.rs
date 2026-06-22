use serde::{Deserialize, Serialize};

pub const TRUSTED_LOCAL_PROFILE_SCHEMA_VERSION: &str = "trusted_local_profile.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrustedLocalProfileInput {
    pub requested: bool,
    pub auth_configured: bool,
    pub endpoint_configured: bool,
    pub credentials_available: bool,
    pub pricing_configured: bool,
    pub per_dispatch_cost_cap_configured: bool,
    pub daily_cost_cap_configured: bool,
    pub fusion_kill_switch: bool,
    pub experiments_paused: bool,
    pub experiments_kill_switch: bool,
    pub auto_promotion_kill_switch: bool,
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

impl TrustedLocalProfileStatus {
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
        let adaptive_execution = ready && !input.fusion_kill_switch;

        Self {
            schema_version: TRUSTED_LOCAL_PROFILE_SCHEMA_VERSION.to_string(),
            requested: input.requested,
            ready,
            blockers,
            capabilities: TrustedLocalCapabilities {
                provider_execution: ready,
                adaptive_execution,
                default_routing: adaptive_execution,
                experiments: adaptive_execution
                    && !input.experiments_paused
                    && !input.experiments_kill_switch,
                auto_promotion: adaptive_execution && !input.auto_promotion_kill_switch,
            },
        }
    }
}
