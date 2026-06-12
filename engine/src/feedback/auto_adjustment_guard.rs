use serde::{Deserialize, Serialize};

use crate::infrastructure::structured_events;

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
            std::env::var("ACP_AUTO_ADJUSTMENT_ACTIVE").ok().as_deref(),
        )
    }

    pub fn from_env_values(
        enable_auto_adjustment: Option<&str>,
        dry_run: Option<&str>,
        active: Option<&str>,
    ) -> AutoAdjustmentGuardDecision {
        let env_gate = enable_auto_adjustment == Some("1");
        let dry_run_enabled = dry_run == Some("1");
        let active_gate = active == Some("1");
        let mut reasons = vec!["safe tier-map changes only".to_string()];
        let mut blocked_reasons = Vec::new();

        let (allowed, mode) = if !env_gate {
            blocked_reasons.push("ACP_ENABLE_AUTO_ADJUSTMENT is not set to 1".to_string());
            (false, "disabled")
        } else if dry_run_enabled {
            reasons.push("ACP_AUTO_ADJUSTMENT_DRY_RUN=1 enables dry-run decisions".to_string());
            (true, "dry_run")
        } else if active_gate {
            reasons.push("ACP_AUTO_ADJUSTMENT_ACTIVE=1 enables active apply".to_string());
            (true, "active")
        } else {
            blocked_reasons.push("ACP_AUTO_ADJUSTMENT_ACTIVE is not set to 1".to_string());
            (false, "disabled")
        };

        let decision = AutoAdjustmentGuardDecision {
            schema_version: AUTO_ADJUSTMENT_GUARD_DECISION_SCHEMA_VERSION.to_string(),
            allowed,
            mode: mode.to_string(),
            reasons,
            blocked_reasons,
            env_gate,
            dry_run: dry_run_enabled && env_gate,
            max_adjustments_remaining: u32::from(mode == "active"),
            safety_invariants: vec![
                "default_off".to_string(),
                "dry_run_blocks_active_apply".to_string(),
                "two_env_gates_for_active_apply".to_string(),
                "one_adjustment_per_request".to_string(),
                "persistent_snapshot_before_mutation".to_string(),
                "rollback_hash_validation".to_string(),
                "no_provider_cli_auth_security_deploy_boundary_expansion".to_string(),
                "no_target_repository_write".to_string(),
            ],
        };

        structured_events::log_auto_adjustment_guard(
            decision.allowed,
            &decision.mode,
            decision.env_gate,
            decision.dry_run,
        );

        decision
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_by_default() {
        let decision = AutoAdjustmentGuard::from_env_values(None, None, None);
        assert!(!decision.allowed);
        assert_eq!(decision.mode, "disabled");
        assert!(!decision.env_gate);
        assert!(!decision.dry_run);
    }

    #[test]
    fn dry_run_requires_enable_gate_and_dry_run_env() {
        let decision = AutoAdjustmentGuard::from_env_values(Some("1"), Some("1"), None);
        assert!(decision.allowed);
        assert_eq!(decision.mode, "dry_run");
        assert!(decision.env_gate);
        assert!(decision.dry_run);
    }

    #[test]
    fn enable_without_dry_run_still_blocks_active_apply() {
        let decision = AutoAdjustmentGuard::from_env_values(Some("1"), None, None);
        assert!(!decision.allowed);
        assert_eq!(decision.mode, "disabled");
        assert!(decision
            .blocked_reasons
            .iter()
            .any(|reason| reason.contains("ACP_AUTO_ADJUSTMENT_ACTIVE")));
        assert_eq!(decision.max_adjustments_remaining, 0);
    }

    #[test]
    fn active_requires_enable_and_active_without_dry_run() {
        let decision = AutoAdjustmentGuard::from_env_values(Some("1"), None, Some("1"));
        assert!(decision.allowed);
        assert_eq!(decision.mode, "active");
        assert_eq!(decision.max_adjustments_remaining, 1);

        let dry_run_wins = AutoAdjustmentGuard::from_env_values(Some("1"), Some("1"), Some("1"));
        assert!(dry_run_wins.allowed);
        assert_eq!(dry_run_wins.mode, "dry_run");
        assert_eq!(dry_run_wins.max_adjustments_remaining, 0);
    }
}
