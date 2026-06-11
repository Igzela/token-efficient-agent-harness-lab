use serde::{Deserialize, Serialize};

pub const AUTO_ADJUSTMENT_GUARD_DECISION_SCHEMA_VERSION: &str = "auto_adjustment_guard_decision.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutoAdjustmentGuardDecision {
    pub schema_version: String,
    pub allowed: bool,
    pub mode: String,
    pub reasons: Vec<String>,
    pub blocked_reasons: Vec<String>,
    pub env_gate: bool,
    pub dry_run: bool,
    pub max_adjustments_remaining: u32,
    pub safety_invariants: Vec<String>,
}

pub struct AutoAdjustmentGuard;

impl AutoAdjustmentGuard {
    pub fn from_env() -> AutoAdjustmentGuardDecision {
        Self::from_env_values(
            std::env::var("ACP_ENABLE_AUTO_ADJUSTMENT").ok().as_deref(),
            std::env::var("ACP_AUTO_ADJUSTMENT_DRY_RUN").ok().as_deref(),
        )
    }

    pub fn from_env_values(
        enable_auto_adjustment: Option<&str>,
        dry_run: Option<&str>,
    ) -> AutoAdjustmentGuardDecision {
        let env_gate = enable_auto_adjustment == Some("1");
        let dry_run_enabled = dry_run == Some("1");
        let mut reasons = vec![
            "no live policy mutation is available in this PR".to_string(),
            "POST apply endpoint is not implemented".to_string(),
            "rollback endpoint is not implemented".to_string(),
        ];
        let mut blocked_reasons = vec![
            "active automatic adjustment is not approved".to_string(),
            "active mode is reserved for future human approval".to_string(),
        ];

        let (allowed, mode) = if !env_gate {
            blocked_reasons.push("ACP_ENABLE_AUTO_ADJUSTMENT is not set to 1".to_string());
            (false, "disabled")
        } else if dry_run_enabled {
            reasons.push("ACP_AUTO_ADJUSTMENT_DRY_RUN=1 enables dry-run decisions".to_string());
            (true, "dry_run")
        } else {
            blocked_reasons
                .push("ACP_AUTO_ADJUSTMENT_DRY_RUN=1 is required for Phase 5 dry-run".to_string());
            (false, "disabled")
        };

        AutoAdjustmentGuardDecision {
            schema_version: AUTO_ADJUSTMENT_GUARD_DECISION_SCHEMA_VERSION.to_string(),
            allowed,
            mode: mode.to_string(),
            reasons,
            blocked_reasons,
            env_gate,
            dry_run: dry_run_enabled && env_gate,
            max_adjustments_remaining: 0,
            safety_invariants: vec![
                "default_off".to_string(),
                "dry_run_only".to_string(),
                "no_post_apply_endpoint".to_string(),
                "no_rollback_endpoint".to_string(),
                "no_active_policy_mutation".to_string(),
                "no_provider_cli_auth_security_deploy_boundary_expansion".to_string(),
                "no_target_repository_write".to_string(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_by_default() {
        let decision = AutoAdjustmentGuard::from_env_values(None, None);
        assert!(!decision.allowed);
        assert_eq!(decision.mode, "disabled");
        assert!(!decision.env_gate);
        assert!(!decision.dry_run);
    }

    #[test]
    fn dry_run_requires_enable_gate_and_dry_run_env() {
        let decision = AutoAdjustmentGuard::from_env_values(Some("1"), Some("1"));
        assert!(decision.allowed);
        assert_eq!(decision.mode, "dry_run");
        assert!(decision.env_gate);
        assert!(decision.dry_run);
    }

    #[test]
    fn enable_without_dry_run_still_blocks_active_apply() {
        let decision = AutoAdjustmentGuard::from_env_values(Some("1"), None);
        assert!(!decision.allowed);
        assert_eq!(decision.mode, "disabled");
        assert!(decision
            .blocked_reasons
            .iter()
            .any(|reason| reason.contains("not approved")));
        assert_eq!(decision.max_adjustments_remaining, 0);
    }
}
