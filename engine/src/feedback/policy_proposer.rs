use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::dispatch_decision::{TASK_DOMAINS, TASK_INTENTS};
use crate::infrastructure::structured_events;

use super::pattern_detector::{DetectedPattern, PatternSeverity, PatternType};
use super::policy_simulator::SimulationResult;
use super::policy_snapshot::stable_hash;

pub const PROPOSAL_CANDIDATE_SCHEMA_VERSION: &str = "policy_proposal_candidate.v1";
pub const MIN_CONFIDENCE_THRESHOLD: f64 = 0.5;

// ---------------------------------------------------------------------------
// SafetyFlags
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SafetyFlags {
    pub no_provider_cli_boundary_expansion: bool,
    pub no_auth_security_change: bool,
    pub no_db_migration_required: bool,
    pub no_hard_constraint_mutation: bool,
    pub no_target_repo_write: bool,
    pub no_destructive_operation: bool,
    pub no_auto_activation: bool,
}

impl SafetyFlags {
    pub fn all_safe() -> Self {
        Self {
            no_provider_cli_boundary_expansion: true,
            no_auth_security_change: true,
            no_db_migration_required: true,
            no_hard_constraint_mutation: true,
            no_target_repo_write: true,
            no_destructive_operation: true,
            no_auto_activation: true,
        }
    }
}

// ---------------------------------------------------------------------------
// CandidateEvidence
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateEvidence {
    pub pattern_ids: Vec<String>,
    pub evidence_trace_ids: Vec<String>,
    pub simulation_scenario_id: Option<String>,
    pub actual_success_rate: Option<f64>,
    pub simulated_success_rate: Option<f64>,
    pub success_rate_delta: Option<f64>,
    pub actual_cost: Option<f64>,
    pub simulated_cost: Option<f64>,
    pub cost_delta: Option<f64>,
    pub actual_latency_ms: Option<f64>,
    pub simulated_latency_ms: Option<f64>,
    pub latency_delta: Option<f64>,
    pub actual_human_review_rate: Option<f64>,
    pub simulated_human_review_rate: Option<f64>,
    pub human_review_rate_delta: Option<f64>,
}

// ---------------------------------------------------------------------------
// ProposalCandidate
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalCandidate {
    pub schema_version: String,
    pub candidate_id: String,
    pub title: String,
    pub summary: String,
    pub task_domain: String,
    pub task_intent: String,
    pub task_class: String,
    pub policy_key: String,
    pub target_tier: String,
    pub source: String,
    pub evidence: CandidateEvidence,
    pub confidence: f64,
    pub risk_level: String,
    pub requires_human_approval: bool,
    pub safety_flags: SafetyFlags,
}

// ---------------------------------------------------------------------------
// PolicyProposer
// ---------------------------------------------------------------------------

pub struct PolicyProposer {
    pub min_confidence: f64,
}

impl Default for PolicyProposer {
    fn default() -> Self {
        Self {
            min_confidence: MIN_CONFIDENCE_THRESHOLD,
        }
    }
}

impl PolicyProposer {
    pub fn propose(
        &self,
        patterns: &[DetectedPattern],
        simulation: Option<&SimulationResult>,
    ) -> Vec<ProposalCandidate> {
        if patterns.is_empty() {
            return Vec::new();
        }

        let mut candidates = Vec::new();
        for pattern in patterns {
            match pattern.pattern_type {
                PatternType::TierFailureConcentration => {
                    let Some(ref tier) = pattern.affected_tier else {
                        continue;
                    };
                    if tier != "cheap_executor" && tier != "balanced_worker" {
                        continue;
                    }
                    let Some(ref tc) = pattern.affected_task_class else {
                        // TierFailureConcentration has affected_task_class = None in detector;
                        // we derive it from pattern_id suffix or skip
                        continue;
                    };
                    let Some((domain, intent)) = parse_task_class(tc) else {
                        continue;
                    };
                    let Some(target) = upgrade_tier(tier) else {
                        continue;
                    };

                    let risk_level = severity_to_risk(&pattern.severity);
                    let policy_key = format!("tier_override:{}->{}", tier, target);
                    candidates.push(ProposalCandidate {
                        schema_version: PROPOSAL_CANDIDATE_SCHEMA_VERSION.to_string(),
                        candidate_id: stable_candidate_id(
                            pattern,
                            &domain,
                            &intent,
                            &policy_key,
                            target,
                        ),
                        title: format!("Route {} away from {} due to high failure rate", tc, tier),
                        summary: format!(
                            "Tier '{}' has a failure rate of {:.1}% for task class '{}'. \
                             Upgrading to '{}'.",
                            tier,
                            pattern.rate * 100.0,
                            tc,
                            target
                        ),
                        task_domain: domain,
                        task_intent: intent,
                        task_class: tc.clone(),
                        policy_key,
                        target_tier: target.to_string(),
                        source: "pattern_detector".to_string(),
                        evidence: make_evidence(pattern, simulation),
                        confidence: failure_concentration_confidence(pattern.rate),
                        risk_level,
                        requires_human_approval: true,
                        safety_flags: SafetyFlags::all_safe(),
                    });
                    let c = candidates.last().unwrap();
                    structured_events::log_proposal_generated(
                        &c.candidate_id,
                        "TierFailureConcentration",
                        &c.target_tier,
                        c.confidence,
                        &c.policy_key,
                    );
                }
                PatternType::TaskClassFailureConcentration => {
                    let Some(ref tc) = pattern.affected_task_class else {
                        continue;
                    };
                    let Some((domain, intent)) = parse_task_class(tc) else {
                        continue;
                    };

                    let risk_level = severity_to_risk(&pattern.severity);
                    let policy_key = format!("task_class_tier_override:{}->balanced_worker", tc);
                    candidates.push(ProposalCandidate {
                        schema_version: PROPOSAL_CANDIDATE_SCHEMA_VERSION.to_string(),
                        candidate_id: stable_candidate_id(
                            pattern,
                            &domain,
                            &intent,
                            &policy_key,
                            "balanced_worker",
                        ),
                        title: format!("Route {} to balanced_worker due to high failure rate", tc),
                        summary: format!(
                            "Task class '{}' has a failure rate of {:.1}%. \
                             Routing to balanced_worker.",
                            tc,
                            pattern.rate * 100.0
                        ),
                        task_domain: domain,
                        task_intent: intent,
                        task_class: tc.clone(),
                        policy_key,
                        target_tier: "balanced_worker".to_string(),
                        source: "pattern_detector".to_string(),
                        evidence: make_evidence(pattern, simulation),
                        confidence: failure_concentration_confidence(pattern.rate),
                        risk_level,
                        requires_human_approval: true,
                        safety_flags: SafetyFlags::all_safe(),
                    });
                    let c = candidates.last().unwrap();
                    structured_events::log_proposal_generated(
                        &c.candidate_id,
                        "TaskClassFailureConcentration",
                        &c.target_tier,
                        c.confidence,
                        &c.policy_key,
                    );
                }
                PatternType::HighCostPerPass => {
                    let Some(ref tier) = pattern.affected_tier else {
                        continue;
                    };
                    let Some(ref tc) = pattern.affected_task_class else {
                        continue;
                    };
                    let Some((domain, intent)) = parse_task_class(tc) else {
                        continue;
                    };
                    let Some(target) = downgrade_tier(tier) else {
                        continue;
                    };

                    // Only generate if simulation shows cost improvement
                    let Some(sim) = simulation else {
                        continue;
                    };
                    if sim.cost_delta >= 0.0 {
                        continue;
                    }

                    let policy_key = format!("tier_override:{}->{}", tier, target);
                    candidates.push(ProposalCandidate {
                        schema_version: PROPOSAL_CANDIDATE_SCHEMA_VERSION.to_string(),
                        candidate_id: stable_candidate_id(
                            pattern,
                            &domain,
                            &intent,
                            &policy_key,
                            target,
                        ),
                        title: format!(
                            "Downgrade {} from {} to {} for cost optimization",
                            tc, tier, target
                        ),
                        summary: format!(
                            "Task class '{}' on '{}' has high cost per pass. \
                             Simulation shows ${:.4} cost savings by switching to '{}'.",
                            tc,
                            tier,
                            sim.cost_delta.abs(),
                            target
                        ),
                        task_domain: domain,
                        task_intent: intent,
                        task_class: tc.clone(),
                        policy_key,
                        target_tier: target.to_string(),
                        source: "simulation".to_string(),
                        evidence: make_evidence(pattern, Some(sim)),
                        confidence: high_cost_confidence(&pattern.severity),
                        risk_level: severity_to_risk(&pattern.severity),
                        requires_human_approval: true,
                        safety_flags: SafetyFlags::all_safe(),
                    });
                    let c = candidates.last().unwrap();
                    structured_events::log_proposal_generated(
                        &c.candidate_id,
                        "HighCostPerPass",
                        &c.target_tier,
                        c.confidence,
                        &c.policy_key,
                    );
                }
                _ => {}
            }
        }

        // Filter by confidence threshold
        candidates.retain(|c| c.confidence >= self.min_confidence);

        // Filter by valid domain/intent
        candidates.retain(|c| {
            TASK_DOMAINS.contains(&c.task_domain.as_str())
                && TASK_INTENTS.contains(&c.task_intent.as_str())
        });

        // Deduplicate by policy_key, keep highest confidence
        deduplicate_by_policy_key(candidates)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_task_class(tc: &str) -> Option<(String, String)> {
    let (domain, intent) = tc.split_once('_')?;
    Some((domain.to_string(), intent.to_string()))
}

fn upgrade_tier(tier: &str) -> Option<&str> {
    match tier {
        "cheap_executor" => Some("balanced_worker"),
        "balanced_worker" => Some("strong_planner"),
        _ => None,
    }
}

fn downgrade_tier(tier: &str) -> Option<&str> {
    match tier {
        "strong_planner" => Some("balanced_worker"),
        "balanced_worker" => Some("cheap_executor"),
        _ => None,
    }
}

fn severity_to_risk(severity: &PatternSeverity) -> String {
    match severity {
        PatternSeverity::High => "high".to_string(),
        PatternSeverity::Medium => "medium".to_string(),
        PatternSeverity::Low => "low".to_string(),
    }
}

fn failure_concentration_confidence(rate: f64) -> f64 {
    if rate >= 0.8 {
        (0.85 + ((rate - 0.8) * 0.5)).min(0.95)
    } else {
        rate * 0.8
    }
}

fn high_cost_confidence(severity: &PatternSeverity) -> f64 {
    match severity {
        PatternSeverity::High => 0.9,
        PatternSeverity::Medium | PatternSeverity::Low => 0.6,
    }
}

fn stable_candidate_id(
    pattern: &DetectedPattern,
    task_domain: &str,
    task_intent: &str,
    policy_key: &str,
    target_tier: &str,
) -> String {
    let hash = stable_hash(&json!({
        "schema_version": PROPOSAL_CANDIDATE_SCHEMA_VERSION,
        "pattern_id": pattern.pattern_id,
        "pattern_type": format!("{:?}", pattern.pattern_type),
        "task_domain": task_domain,
        "task_intent": task_intent,
        "policy_key": policy_key,
        "target_tier": target_tier,
        "evidence_trace_ids": pattern.evidence_trace_ids,
    }));
    format!("proposal-{}", &hash[..12])
}

fn make_evidence(
    pattern: &DetectedPattern,
    simulation: Option<&SimulationResult>,
) -> CandidateEvidence {
    CandidateEvidence {
        pattern_ids: vec![pattern.pattern_id.clone()],
        evidence_trace_ids: pattern.evidence_trace_ids.clone(),
        simulation_scenario_id: simulation.map(|s| s.scenario_id.clone()),
        actual_success_rate: simulation.map(|s| s.actual_success_rate),
        simulated_success_rate: simulation.map(|s| s.simulated_success_rate),
        success_rate_delta: simulation.map(|s| s.success_rate_delta),
        actual_cost: simulation.map(|s| s.actual_average_cost),
        simulated_cost: simulation.map(|s| s.simulated_average_cost),
        cost_delta: simulation.map(|s| s.cost_delta),
        actual_latency_ms: simulation.map(|s| s.actual_average_latency_ms),
        simulated_latency_ms: simulation.map(|s| s.simulated_average_latency_ms),
        latency_delta: simulation.map(|s| s.latency_delta),
        actual_human_review_rate: simulation.map(|s| s.actual_human_review_rate),
        simulated_human_review_rate: simulation.map(|s| s.simulated_human_review_rate),
        human_review_rate_delta: simulation.map(|s| s.human_review_rate_delta),
    }
}

fn deduplicate_by_policy_key(candidates: Vec<ProposalCandidate>) -> Vec<ProposalCandidate> {
    let mut best: HashMap<String, ProposalCandidate> = HashMap::new();
    for c in candidates {
        let key = c.policy_key.clone();
        match best.get(&key) {
            Some(existing) if existing.confidence >= c.confidence => {}
            _ => {
                best.insert(key, c);
            }
        }
    }
    best.into_values().collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pattern(
        pattern_type: PatternType,
        tier: Option<&str>,
        task_class: Option<&str>,
        rate: f64,
        severity: PatternSeverity,
        trace_ids: Vec<&str>,
    ) -> DetectedPattern {
        let tier_str = tier.unwrap_or("unknown");
        DetectedPattern {
            schema_version: "feedback_pattern.v1".to_string(),
            pattern_id: format!("pattern-{:?}-{}", pattern_type, tier_str),
            pattern_type,
            affected_tier: tier.map(String::from),
            affected_task_class: task_class.map(String::from),
            count: 0,
            denominator: 0,
            rate,
            evidence_trace_ids: trace_ids.into_iter().map(String::from).collect(),
            severity,
            recommendation_hint: String::new(),
        }
    }

    fn make_simulation(cost_delta: f64) -> SimulationResult {
        SimulationResult {
            schema_version: "policy_simulation_report.v1".to_string(),
            scenario_id: "sim-test".to_string(),
            candidate_policy_id: "policy-test".to_string(),
            input_trace_count: 10,
            actual_success_rate: 0.8,
            simulated_success_rate: 0.9,
            success_rate_delta: 0.1,
            actual_average_cost: 0.10,
            simulated_average_cost: 0.10 + cost_delta,
            cost_delta,
            actual_average_latency_ms: 1000.0,
            simulated_average_latency_ms: 900.0,
            latency_delta: -100.0,
            actual_human_review_rate: 0.2,
            simulated_human_review_rate: 0.1,
            human_review_rate_delta: -0.1,
            assumptions: vec![],
            evidence_trace_ids: vec!["t1".to_string(), "t2".to_string()],
            safety: "shadow_only / no_live_influence".to_string(),
        }
    }

    #[test]
    fn propose_empty_patterns_returns_empty() {
        let proposer = PolicyProposer::default();
        let candidates = proposer.propose(&[], None);
        assert!(candidates.is_empty());
    }

    #[test]
    fn propose_failure_concentration_upgrades_tier() {
        let proposer = PolicyProposer::default();
        let pattern = make_pattern(
            PatternType::TierFailureConcentration,
            Some("cheap_executor"),
            Some("code_generate"),
            0.8,
            PatternSeverity::High,
            vec!["t1", "t2", "t3"],
        );

        let candidates = proposer.propose(&[pattern], None);
        assert_eq!(candidates.len(), 1);

        let c = &candidates[0];
        assert_eq!(c.target_tier, "balanced_worker");
        assert_eq!(c.task_domain, "code");
        assert_eq!(c.task_intent, "generate");
        assert!(c.confidence > 0.0);
        assert_eq!(c.risk_level, "high");
        assert!(c.requires_human_approval);
        assert_eq!(c.safety_flags, SafetyFlags::all_safe());
    }

    #[test]
    fn propose_cost_optimization_with_simulation() {
        let proposer = PolicyProposer::default();
        let pattern = make_pattern(
            PatternType::HighCostPerPass,
            Some("strong_planner"),
            Some("code_debug"),
            0.3,
            PatternSeverity::Medium,
            vec!["t4", "t5"],
        );

        // cost_delta < 0 means simulated is cheaper
        let sim = make_simulation(-0.05);
        let candidates = proposer.propose(&[pattern], Some(&sim));

        assert_eq!(candidates.len(), 1);
        let c = &candidates[0];
        assert_eq!(c.target_tier, "balanced_worker");
        assert_eq!(c.source, "simulation");
        assert!((c.confidence - 0.6).abs() < f64::EPSILON);
    }

    #[test]
    fn propose_suppresses_low_confidence() {
        let proposer = PolicyProposer::default();
        // rate=0.4 * 0.8 = 0.32, below default threshold of 0.5
        let pattern = make_pattern(
            PatternType::TierFailureConcentration,
            Some("cheap_executor"),
            Some("code_generate"),
            0.4,
            PatternSeverity::Low,
            vec!["t1"],
        );

        let candidates = proposer.propose(&[pattern], None);
        assert!(candidates.is_empty());
    }

    #[test]
    fn propose_includes_evidence_trace_ids() {
        let proposer = PolicyProposer::default();
        let pattern = make_pattern(
            PatternType::TaskClassFailureConcentration,
            None,
            Some("code_review"),
            0.75,
            PatternSeverity::High,
            vec!["trace-100", "trace-200", "trace-300"],
        );

        let candidates = proposer.propose(&[pattern], None);
        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].evidence.evidence_trace_ids,
            vec![
                "trace-100".to_string(),
                "trace-200".to_string(),
                "trace-300".to_string()
            ]
        );
    }

    #[test]
    fn propose_includes_simulation_deltas() {
        let proposer = PolicyProposer::default();
        let pattern = make_pattern(
            PatternType::HighCostPerPass,
            Some("strong_planner"),
            Some("code_debug"),
            0.3,
            PatternSeverity::Medium,
            vec!["t1"],
        );

        let sim = make_simulation(-0.03);
        let candidates = proposer.propose(&[pattern], Some(&sim));

        assert_eq!(candidates.len(), 1);
        let e = &candidates[0].evidence;
        assert_eq!(e.simulation_scenario_id, Some("sim-test".to_string()));
        assert!(e.cost_delta.is_some());
        assert!(e.cost_delta.unwrap() < 0.0);
        assert!(e.success_rate_delta.is_some());
        assert!(e.latency_delta.is_some());
        assert!(e.human_review_rate_delta.is_some());
    }
}
