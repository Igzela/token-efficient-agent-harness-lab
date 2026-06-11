use serde::{Deserialize, Serialize};

use super::policy_proposer::{ProposalCandidate, SafetyFlags, PROPOSAL_CANDIDATE_SCHEMA_VERSION};
use super::proposal_validator::ProposalValidator;
use crate::model_selector::is_safe_policy_override_tier;

pub const AUTO_ADJUSTMENT_POLICY_DECISION_SCHEMA_VERSION: &str =
    "auto_adjustment_policy_decision.v1";
pub const STRICT_AUTO_ADJUSTMENT_CONFIDENCE: f64 = 0.85;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutoAdjustmentEvidenceSummary {
    pub pattern_ids: Vec<String>,
    pub evidence_trace_ids: Vec<String>,
    pub simulation_scenario_id: Option<String>,
    pub success_rate_delta: Option<f64>,
    pub cost_delta: Option<f64>,
    pub latency_delta: Option<f64>,
    pub human_review_rate_delta: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutoAdjustmentPolicyDecision {
    pub schema_version: String,
    pub candidate_id: String,
    pub eligible: bool,
    pub reasons: Vec<String>,
    pub blocked_reasons: Vec<String>,
    pub confidence: f64,
    pub risk_level: String,
    pub target_tier: String,
    pub policy_key: String,
    pub evidence_summary: AutoAdjustmentEvidenceSummary,
    pub requires_snapshot: bool,
    pub requires_rollback: bool,
}

#[derive(Debug, Clone)]
pub struct AutoAdjustmentPolicy {
    pub strict_confidence_threshold: f64,
    pub max_cost_regression: f64,
    pub max_latency_regression_ms: f64,
}

impl Default for AutoAdjustmentPolicy {
    fn default() -> Self {
        Self {
            strict_confidence_threshold: STRICT_AUTO_ADJUSTMENT_CONFIDENCE,
            max_cost_regression: 0.0,
            max_latency_regression_ms: 0.0,
        }
    }
}

impl AutoAdjustmentPolicy {
    pub fn evaluate(&self, candidate: &ProposalCandidate) -> AutoAdjustmentPolicyDecision {
        let mut reasons = Vec::new();
        let mut blocked_reasons = Vec::new();

        if candidate.schema_version == PROPOSAL_CANDIDATE_SCHEMA_VERSION {
            reasons.push("generated proposal candidate schema accepted".to_string());
        } else {
            blocked_reasons.push(format!(
                "candidate schema_version '{}' is not {}",
                candidate.schema_version, PROPOSAL_CANDIDATE_SCHEMA_VERSION
            ));
        }

        if matches!(candidate.source.as_str(), "pattern_detector" | "simulation") {
            reasons.push(format!(
                "candidate source '{}' is generated",
                candidate.source
            ));
        } else {
            blocked_reasons.push(format!(
                "candidate source '{}' is not an approved generated source",
                candidate.source
            ));
        }

        let validation = ProposalValidator::validate_generated(candidate);
        if validation.valid {
            reasons.push("ProposalValidator accepted candidate".to_string());
        } else {
            blocked_reasons.extend(
                validation
                    .errors
                    .into_iter()
                    .map(|error| format!("ProposalValidator rejected candidate: {error}")),
            );
        }

        if is_safe_policy_override_tier(&candidate.target_tier) {
            reasons.push(format!(
                "target_tier '{}' is in safe override tier set",
                candidate.target_tier
            ));
        } else {
            blocked_reasons.push(format!(
                "target_tier '{}' is not in safe override tier set",
                candidate.target_tier
            ));
        }

        if candidate.confidence >= self.strict_confidence_threshold {
            reasons.push(format!(
                "confidence {:.3} meets strict threshold {:.3}",
                candidate.confidence, self.strict_confidence_threshold
            ));
        } else {
            blocked_reasons.push(format!(
                "confidence {:.3} is below strict threshold {:.3}",
                candidate.confidence, self.strict_confidence_threshold
            ));
        }

        if candidate.evidence.pattern_ids.is_empty()
            || candidate.evidence.evidence_trace_ids.is_empty()
        {
            blocked_reasons
                .push("evidence must include pattern_ids and evidence_trace_ids".to_string());
        } else {
            reasons.push("pattern and trace evidence present".to_string());
        }

        if candidate.evidence.simulation_scenario_id.is_none()
            || candidate.evidence.success_rate_delta.is_none()
            || candidate.evidence.cost_delta.is_none()
            || candidate.evidence.latency_delta.is_none()
            || candidate.evidence.human_review_rate_delta.is_none()
        {
            blocked_reasons.push("complete simulation evidence is required".to_string());
        } else {
            reasons.push("simulation evidence present".to_string());
        }

        if let Some(delta) = candidate.evidence.success_rate_delta {
            if delta < 0.0 {
                blocked_reasons.push(format!(
                    "simulation success_rate_delta {:.3} is a regression",
                    delta
                ));
            }
        }
        if let Some(delta) = candidate.evidence.cost_delta {
            if delta > self.max_cost_regression {
                blocked_reasons.push(format!(
                    "simulation cost_delta {:.4} exceeds allowed regression {:.4}",
                    delta, self.max_cost_regression
                ));
            }
        }
        if let Some(delta) = candidate.evidence.latency_delta {
            if delta > self.max_latency_regression_ms {
                blocked_reasons.push(format!(
                    "simulation latency_delta {:.1} exceeds allowed regression {:.1}",
                    delta, self.max_latency_regression_ms
                ));
            }
        }
        if let Some(delta) = candidate.evidence.human_review_rate_delta {
            if delta > 0.0 {
                blocked_reasons.push(format!(
                    "simulation human_review_rate_delta {:.3} is a regression",
                    delta
                ));
            }
        }

        if candidate.requires_human_approval {
            reasons.push("requires_human_approval remains true".to_string());
        } else {
            blocked_reasons.push("requires_human_approval must remain true".to_string());
        }

        collect_safety_flag_reasons(&candidate.safety_flags, &mut reasons, &mut blocked_reasons);

        if candidate.policy_key.contains("tier_override") {
            reasons.push("adjustment scope is tier_map_override".to_string());
        } else {
            blocked_reasons.push("adjustment scope must be tier_map_override".to_string());
        }

        if policy_key_has_forbidden_boundary(&candidate.policy_key) {
            blocked_reasons.push("policy_key references forbidden boundary scope".to_string());
        }

        AutoAdjustmentPolicyDecision {
            schema_version: AUTO_ADJUSTMENT_POLICY_DECISION_SCHEMA_VERSION.to_string(),
            candidate_id: candidate.candidate_id.clone(),
            eligible: blocked_reasons.is_empty(),
            reasons,
            blocked_reasons,
            confidence: candidate.confidence,
            risk_level: candidate.risk_level.clone(),
            target_tier: candidate.target_tier.clone(),
            policy_key: candidate.policy_key.clone(),
            evidence_summary: AutoAdjustmentEvidenceSummary {
                pattern_ids: candidate.evidence.pattern_ids.clone(),
                evidence_trace_ids: candidate.evidence.evidence_trace_ids.clone(),
                simulation_scenario_id: candidate.evidence.simulation_scenario_id.clone(),
                success_rate_delta: candidate.evidence.success_rate_delta,
                cost_delta: candidate.evidence.cost_delta,
                latency_delta: candidate.evidence.latency_delta,
                human_review_rate_delta: candidate.evidence.human_review_rate_delta,
            },
            requires_snapshot: true,
            requires_rollback: true,
        }
    }
}

fn collect_safety_flag_reasons(
    flags: &SafetyFlags,
    reasons: &mut Vec<String>,
    blocked_reasons: &mut Vec<String>,
) {
    let checks = [
        (
            flags.no_provider_cli_boundary_expansion,
            "no provider/CLI boundary expansion",
        ),
        (flags.no_auth_security_change, "no auth/security change"),
        (flags.no_db_migration_required, "no DB migration required"),
        (
            flags.no_hard_constraint_mutation,
            "no hard constraint mutation",
        ),
        (flags.no_target_repo_write, "no target repository write"),
        (flags.no_destructive_operation, "no destructive operation"),
        (flags.no_auto_activation, "no auto activation"),
    ];

    for (ok, label) in checks {
        if ok {
            reasons.push(label.to_string());
        } else {
            blocked_reasons.push(format!("safety flag failed: {label}"));
        }
    }
}

fn policy_key_has_forbidden_boundary(policy_key: &str) -> bool {
    let value = policy_key.to_ascii_lowercase();
    [
        "provider",
        "cli",
        "auth",
        "security",
        "deploy",
        "release",
        "hard_constraint",
        "target_repo",
        "destructive",
        "db_migration",
    ]
    .iter()
    .any(|term| value.contains(term))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feedback::policy_proposer::{CandidateEvidence, SafetyFlags};

    fn candidate() -> ProposalCandidate {
        ProposalCandidate {
            schema_version: PROPOSAL_CANDIDATE_SCHEMA_VERSION.to_string(),
            candidate_id: "candidate-001".to_string(),
            title: "Route code_generate to balanced_worker".to_string(),
            summary: "Safe dry-run candidate".to_string(),
            task_domain: "code".to_string(),
            task_intent: "generate".to_string(),
            task_class: "code_generate".to_string(),
            policy_key: "task_class_tier_override:code_generate->balanced_worker".to_string(),
            target_tier: "balanced_worker".to_string(),
            source: "pattern_detector".to_string(),
            evidence: CandidateEvidence {
                pattern_ids: vec!["pattern-1".to_string()],
                evidence_trace_ids: vec!["trace-1".to_string(), "trace-2".to_string()],
                simulation_scenario_id: Some("sim-balanced".to_string()),
                actual_success_rate: Some(0.75),
                simulated_success_rate: Some(0.9),
                success_rate_delta: Some(0.15),
                actual_cost: Some(0.10),
                simulated_cost: Some(0.09),
                cost_delta: Some(-0.01),
                actual_latency_ms: Some(1000.0),
                simulated_latency_ms: Some(900.0),
                latency_delta: Some(-100.0),
                actual_human_review_rate: Some(0.2),
                simulated_human_review_rate: Some(0.1),
                human_review_rate_delta: Some(-0.1),
            },
            confidence: 0.9,
            risk_level: "low".to_string(),
            requires_human_approval: true,
            safety_flags: SafetyFlags::all_safe(),
        }
    }

    #[test]
    fn accepts_high_confidence_safe_generated_candidate() {
        let decision = AutoAdjustmentPolicy::default().evaluate(&candidate());
        assert!(decision.eligible, "{:?}", decision.blocked_reasons);
        assert_eq!(
            decision.schema_version,
            AUTO_ADJUSTMENT_POLICY_DECISION_SCHEMA_VERSION
        );
        assert!(decision.requires_snapshot);
        assert!(decision.requires_rollback);
    }

    #[test]
    fn rejects_cli_target_tier() {
        let mut c = candidate();
        c.target_tier = "codex_cli".to_string();
        let decision = AutoAdjustmentPolicy::default().evaluate(&c);
        assert!(!decision.eligible);
        assert!(decision
            .blocked_reasons
            .iter()
            .any(|reason| reason.contains("safe override tier")));
    }

    #[test]
    fn rejects_missing_evidence() {
        let mut c = candidate();
        c.evidence.pattern_ids.clear();
        let decision = AutoAdjustmentPolicy::default().evaluate(&c);
        assert!(!decision.eligible);
        assert!(decision
            .blocked_reasons
            .iter()
            .any(|reason| reason.contains("pattern_ids")));
    }

    #[test]
    fn rejects_weak_confidence() {
        let mut c = candidate();
        c.confidence = 0.84;
        let decision = AutoAdjustmentPolicy::default().evaluate(&c);
        assert!(!decision.eligible);
        assert!(decision
            .blocked_reasons
            .iter()
            .any(|reason| reason.contains("below strict threshold")));
    }

    #[test]
    fn rejects_missing_simulation() {
        let mut c = candidate();
        c.evidence.simulation_scenario_id = None;
        let decision = AutoAdjustmentPolicy::default().evaluate(&c);
        assert!(!decision.eligible);
        assert!(decision
            .blocked_reasons
            .iter()
            .any(|reason| reason.contains("simulation evidence")));
    }

    #[test]
    fn rejects_simulation_regression() {
        let mut c = candidate();
        c.evidence.success_rate_delta = Some(-0.01);
        let decision = AutoAdjustmentPolicy::default().evaluate(&c);
        assert!(!decision.eligible);
        assert!(decision
            .blocked_reasons
            .iter()
            .any(|reason| reason.contains("success_rate_delta")));
    }

    #[test]
    fn rejects_failed_safety_flag() {
        let mut c = candidate();
        c.safety_flags.no_target_repo_write = false;
        let decision = AutoAdjustmentPolicy::default().evaluate(&c);
        assert!(!decision.eligible);
        assert!(decision
            .blocked_reasons
            .iter()
            .any(|reason| reason.contains("target repository write")));
    }
}
