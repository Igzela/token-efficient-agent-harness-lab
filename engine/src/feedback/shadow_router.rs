use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::offline_evaluation::{
    offline_replay_report_sha256, OfflineCounterfactualEstimate, OfflineObservedFacts,
    OfflinePolicyComparison, OfflineReplayReport, OfflineReplayStatus,
    OFFLINE_REPLAY_SCHEMA_VERSION,
};
use super::run_trace_recorder::RunTrace;

pub const SHADOW_ROUTE_SCHEMA_VERSION: &str = "shadow_route.v1";
pub const SHADOW_REPLAY_COMPARISON_SCHEMA_VERSION: &str = "shadow_replay_comparison.v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowRouteOutput {
    pub schema_version: String,
    pub shadow_route_id: String,
    pub dispatch_id: String,
    pub actual_tier: String,
    pub candidate_tier: String,
    pub candidate_profile: Option<String>,
    pub reason: String,
    pub estimated_cost: Option<f64>,
    pub expected_tradeoff: String,
    pub influence_selected_tier: bool,
    pub influence_executor_type: bool,
    pub influence_retry_path: bool,
    pub influence_routing_policy: bool,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShadowDriftEvidence {
    pub status: String,
    pub reason_codes: Vec<String>,
    pub observed_sample_count: usize,
    pub predicted_sample_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShadowPolicyComparison {
    pub policy_id: String,
    pub policy_version: String,
    pub policy_hash: String,
    pub task_class: String,
    pub observed: OfflineObservedFacts,
    pub predicted: OfflineCounterfactualEstimate,
    pub success_rate_delta: f64,
    pub quality_score_delta: f64,
    pub tool_success_score_delta: f64,
    pub cost_usd_delta: f64,
    pub latency_ms_delta: f64,
    pub total_tokens_delta: f64,
    pub retry_count_delta: f64,
    pub coverage_trace_ids: Vec<String>,
    pub coverage_evidence_content_sha256: Vec<String>,
    pub drift: ShadowDriftEvidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShadowReplayComparison {
    pub schema_version: String,
    pub status: OfflineReplayStatus,
    pub reason_codes: Vec<String>,
    pub policy_id: Option<String>,
    pub policy_version: Option<String>,
    pub policy_hash: Option<String>,
    pub source_trace_ids: Vec<String>,
    pub source_evidence_content_sha256: Vec<String>,
    pub comparisons: Vec<ShadowPolicyComparison>,
    pub shadow_only: bool,
    pub influence_selected_tier: bool,
    pub influence_executor_type: bool,
    pub influence_retry_path: bool,
    pub influence_routing_policy: bool,
    pub content_sha256: String,
}

pub struct ShadowRouter;

impl ShadowRouter {
    pub fn generate_shadow_route(
        trace: &RunTrace,
        candidate_tier: &str,
        reason: &str,
        tradeoff: &str,
    ) -> ShadowRouteOutput {
        ShadowRouteOutput {
            schema_version: SHADOW_ROUTE_SCHEMA_VERSION.to_string(),
            shadow_route_id: format!("shadow-{}-{}", trace.dispatch_id, candidate_tier),
            dispatch_id: trace.dispatch_id.clone(),
            actual_tier: trace.selected_tier.clone(),
            candidate_tier: candidate_tier.to_string(),
            candidate_profile: None,
            reason: reason.to_string(),
            estimated_cost: estimate_tier_cost_usd(
                candidate_tier,
                &trace.selected_tier,
                trace.estimated_cost_usd,
                trace.reserved_cost,
            ),
            expected_tradeoff: tradeoff.to_string(),
            influence_selected_tier: false,
            influence_executor_type: false,
            influence_retry_path: false,
            influence_routing_policy: false,
            source: "shadow_only".to_string(),
        }
    }

    pub fn generate_for_policy(trace: &RunTrace, candidate_tier: &str) -> ShadowRouteOutput {
        if candidate_tier == trace.selected_tier {
            Self::generate_shadow_route(trace, candidate_tier, "same_as_actual", "no change")
        } else if tier_order(candidate_tier) < tier_order(&trace.selected_tier) {
            Self::generate_shadow_route(
                trace,
                candidate_tier,
                "cost_optimization",
                "lower cost, potentially lower quality",
            )
        } else {
            Self::generate_shadow_route(
                trace,
                candidate_tier,
                "quality_upgrade",
                "higher quality, higher cost",
            )
        }
    }

    /// Compare a validated offline replay report for shadow diagnostics only.
    /// This consumes no provider, routing, policy, or production authority.
    pub fn compare_replay_report(
        report: &OfflineReplayReport,
    ) -> Result<ShadowReplayComparison, String> {
        validate_replay_report(report)?;
        let mut comparison = ShadowReplayComparison {
            schema_version: SHADOW_REPLAY_COMPARISON_SCHEMA_VERSION.to_string(),
            status: report.status,
            reason_codes: report.reason_codes.clone(),
            policy_id: None,
            policy_version: None,
            policy_hash: None,
            source_trace_ids: report.source_trace_ids.clone(),
            source_evidence_content_sha256: report.source_evidence_content_sha256.clone(),
            comparisons: Vec::new(),
            shadow_only: true,
            influence_selected_tier: false,
            influence_executor_type: false,
            influence_retry_path: false,
            influence_routing_policy: false,
            content_sha256: String::new(),
        };

        if report.status == OfflineReplayStatus::Sufficient {
            for policy_comparison in &report.comparisons {
                let observed = report
                    .observed_facts
                    .iter()
                    .find(|fact| {
                        fact.task_class == policy_comparison.task_class
                            && fact.candidate_id == policy_comparison.current_observed_candidate_id
                    })
                    .ok_or_else(|| "shadow comparison missing observed facts".to_string())?;
                let predicted = report
                    .counterfactual_estimates
                    .iter()
                    .find(|estimate| {
                        estimate.policy_id == policy_comparison.policy_id
                            && estimate.task_class == policy_comparison.task_class
                    })
                    .ok_or_else(|| {
                        "shadow comparison missing counterfactual estimate".to_string()
                    })?;
                comparison.policy_id = Some(policy_comparison.policy_id.clone());
                comparison.policy_version = Some(policy_comparison.policy_version.clone());
                comparison.policy_hash = Some(policy_comparison.policy_hash.clone());
                comparison.comparisons.push(shadow_policy_comparison(
                    policy_comparison,
                    observed,
                    predicted,
                ));
            }
            if comparison.comparisons.is_empty() {
                comparison.status = OfflineReplayStatus::InsufficientEvidence;
                comparison
                    .reason_codes
                    .push("missing_shadow_comparison_rows".to_string());
            }
        }
        comparison.reason_codes.sort();
        comparison.reason_codes.dedup();
        comparison.content_sha256 = shadow_replay_comparison_sha256(&comparison);
        Ok(comparison)
    }

    pub fn validate_replay_comparison(comparison: &ShadowReplayComparison) -> Result<(), String> {
        if comparison.schema_version != SHADOW_REPLAY_COMPARISON_SCHEMA_VERSION
            || !comparison.shadow_only
            || comparison.influence_selected_tier
            || comparison.influence_executor_type
            || comparison.influence_retry_path
            || comparison.influence_routing_policy
            || !valid_hash(&comparison.content_sha256)
            || shadow_replay_comparison_sha256(comparison) != comparison.content_sha256
        {
            return Err("shadow replay comparison is not hash-valid and shadow-only".to_string());
        }
        Ok(())
    }
}

fn shadow_policy_comparison(
    comparison: &OfflinePolicyComparison,
    observed: &OfflineObservedFacts,
    predicted: &OfflineCounterfactualEstimate,
) -> ShadowPolicyComparison {
    ShadowPolicyComparison {
        policy_id: comparison.policy_id.clone(),
        policy_version: comparison.policy_version.clone(),
        policy_hash: comparison.policy_hash.clone(),
        task_class: comparison.task_class.clone(),
        observed: observed.clone(),
        predicted: predicted.clone(),
        success_rate_delta: comparison.success_rate_delta,
        quality_score_delta: comparison.quality_score_delta,
        tool_success_score_delta: comparison.tool_success_score_delta,
        cost_usd_delta: comparison.cost_usd_delta,
        latency_ms_delta: comparison.latency_ms_delta,
        total_tokens_delta: comparison.total_tokens_delta,
        retry_count_delta: comparison.retry_count_delta,
        coverage_trace_ids: predicted.source_trace_ids.clone(),
        coverage_evidence_content_sha256: predicted.source_evidence_content_sha256.clone(),
        drift: ShadowDriftEvidence {
            status: "observed_comparable_cohort_not_live_candidate".to_string(),
            reason_codes: vec![
                "counterfactual_estimate_is_not_live_candidate_observation".to_string()
            ],
            observed_sample_count: observed.sample_count,
            predicted_sample_count: predicted.sample_count,
        },
    }
}

fn validate_replay_report(report: &OfflineReplayReport) -> Result<(), String> {
    if report.schema_version != OFFLINE_REPLAY_SCHEMA_VERSION
        || offline_replay_report_sha256(report).map_err(|error| error.to_string())?
            != report.content_sha256
        || !report.shadow_only
        || report.influence_selected_tier
        || report.influence_executor_type
        || report.influence_retry_path
        || report.influence_routing_policy
    {
        return Err("shadow input report is not a valid immutable offline report".to_string());
    }
    for policy in std::iter::once(&report.current_policy).chain(report.candidate_policies.iter()) {
        if policy.content_sha256().map_err(|error| error.to_string())? != policy.policy_hash {
            return Err("shadow input report contains an invalid policy hash".to_string());
        }
    }
    Ok(())
}

pub fn shadow_replay_comparison_sha256(comparison: &ShadowReplayComparison) -> String {
    let mut unsigned = comparison.clone();
    unsigned.content_sha256.clear();
    hex::encode(Sha256::digest(
        serde_json::to_vec(&unsigned).expect("shadow comparison is serializable"),
    ))
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn tier_cost_multiplier(tier: &str) -> f64 {
    match tier {
        "cheap_executor" => 0.5,
        "balanced_worker" => 1.0,
        "strong_planner" => 1.5,
        "claude_code_cli" => 2.0,
        "codex_cli" => 1.2,
        _ => 1.0,
    }
}

fn estimate_tier_cost_usd(
    candidate_tier: &str,
    actual_tier: &str,
    actual_cost: Option<f64>,
    reserved_cost: f64,
) -> Option<f64> {
    let base = actual_cost.unwrap_or(reserved_cost);
    if base <= 0.0 {
        return None;
    }
    let actual_mult = tier_cost_multiplier(actual_tier);
    let cand_mult = tier_cost_multiplier(candidate_tier);
    Some(cand_mult / actual_mult * base)
}

fn tier_order(tier: &str) -> u8 {
    match tier {
        "cheap_executor" => 0,
        "codex_cli" => 1,
        "balanced_worker" => 2,
        "strong_planner" => 3,
        "claude_code_cli" => 4,
        _ => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feedback::{OfflinePolicyDefinition, OfflinePolicySelection};
    use serde_json::json;
    use std::collections::BTreeMap;

    fn minimal_trace() -> RunTrace {
        RunTrace {
            schema_version: "feedback_trace.v1".to_string(),
            trace_id: "trace-001".to_string(),
            dispatch_id: "disp-001".to_string(),
            history_id: None,
            created_at: None,
            task_class: "codegen".to_string(),
            task_domain: None,
            task_intent: None,
            selected_tier: "balanced_worker".to_string(),
            selected_profile: None,
            routing_policy: None,
            complexity_score: None,
            constraints: vec![],
            human_review_flag: false,
            retry_policy: None,
            shadow_routes: vec![],
            executor_type: "local".to_string(),
            execution_status: None,
            latency_ms: Some(1000),
            input_tokens: None,
            output_tokens: None,
            estimated_cost_usd: Some(0.05),
            reserved_cost: 0.05,
            total_cost: 0.0,
            retry_count: 0,
            evaluation_status: "pending".to_string(),
            final_status: "pending".to_string(),
            success: true,
            failure_domain: None,
            analysis: json!(null),
            decision: json!(null),
            execution: json!(null),
            evaluation: json!(null),
        }
    }

    #[test]
    fn all_influence_flags_false() {
        let trace = minimal_trace();
        let route = ShadowRouter::generate_for_policy(&trace, "cheap_executor");
        assert!(!route.influence_selected_tier);
        assert!(!route.influence_executor_type);
        assert!(!route.influence_retry_path);
        assert!(!route.influence_routing_policy);
    }

    #[test]
    fn source_is_shadow_only() {
        let trace = minimal_trace();
        let route = ShadowRouter::generate_for_policy(&trace, "strong_planner");
        assert_eq!(route.source, "shadow_only");
    }

    #[test]
    fn schema_version() {
        let trace = minimal_trace();
        let route = ShadowRouter::generate_for_policy(&trace, "cheap_executor");
        assert_eq!(route.schema_version, SHADOW_ROUTE_SCHEMA_VERSION);
    }

    #[test]
    fn same_tier() {
        let trace = minimal_trace();
        let route = ShadowRouter::generate_for_policy(&trace, "balanced_worker");
        assert_eq!(route.reason, "same_as_actual");
        assert_eq!(route.expected_tradeoff, "no change");
        assert_eq!(route.candidate_tier, "balanced_worker");
    }

    #[test]
    fn cheaper_tier() {
        let trace = minimal_trace();
        let route = ShadowRouter::generate_for_policy(&trace, "cheap_executor");
        assert_eq!(route.reason, "cost_optimization");
        assert_eq!(
            route.expected_tradeoff,
            "lower cost, potentially lower quality"
        );
    }

    #[test]
    fn stronger_tier() {
        let trace = minimal_trace();
        let route = ShadowRouter::generate_for_policy(&trace, "strong_planner");
        assert_eq!(route.reason, "quality_upgrade");
        assert_eq!(route.expected_tradeoff, "higher quality, higher cost");
    }

    #[test]
    fn cost_estimation() {
        let trace = minimal_trace();
        let route =
            ShadowRouter::generate_shadow_route(&trace, "cheap_executor", "test", "test tradeoff");
        // cheap_executor=0.5, balanced_worker=1.0, base=0.05
        // 0.5 / 1.0 * 0.05 = 0.025
        let cost = route.estimated_cost.unwrap();
        assert!((cost - 0.025).abs() < 1e-10);
    }

    #[test]
    fn determinism() {
        let trace = minimal_trace();
        let r1 = ShadowRouter::generate_for_policy(&trace, "strong_planner");
        let r2 = ShadowRouter::generate_for_policy(&trace, "strong_planner");
        assert_eq!(r1.shadow_route_id, r2.shadow_route_id);
        assert_eq!(r1.reason, r2.reason);
        assert_eq!(r1.estimated_cost, r2.estimated_cost);
        assert_eq!(r1.schema_version, r2.schema_version);
        assert_eq!(r1.source, r2.source);
    }

    fn replay_report() -> OfflineReplayReport {
        let candidate_definition_sha256 = format!("{:064x}", 3);
        let current_policy = OfflinePolicyDefinition::new(
            "current",
            "v1",
            BTreeMap::from([(
                "codegen".to_string(),
                OfflinePolicySelection {
                    candidate_id: "actual".to_string(),
                    candidate_version: "v1".to_string(),
                    candidate_definition_sha256: candidate_definition_sha256.clone(),
                },
            )]),
        )
        .unwrap();
        let candidate_policy = OfflinePolicyDefinition::new(
            "candidate",
            "v1",
            BTreeMap::from([(
                "codegen".to_string(),
                OfflinePolicySelection {
                    candidate_id: "candidate".to_string(),
                    candidate_version: "v1".to_string(),
                    candidate_definition_sha256: candidate_definition_sha256.clone(),
                },
            )]),
        )
        .unwrap();
        let observed = OfflineObservedFacts {
            task_class: "codegen".to_string(),
            candidate_id: "actual".to_string(),
            candidate_version: "v1".to_string(),
            candidate_definition_sha256: candidate_definition_sha256.clone(),
            member_endpoint_ids: vec!["endpoint-a".to_string()],
            trace_ids: vec!["trace-actual".to_string()],
            evidence_content_sha256: vec![format!("{:064x}", 4)],
            sample_count: 3,
            success_rate: 1.0,
            average_quality_score: 0.8,
            average_tool_success_score: 0.9,
            average_cost_usd: 0.1,
            average_latency_ms: 100.0,
            average_total_tokens: 20.0,
            average_retry_count: 0.0,
        };
        let selection = OfflinePolicySelection {
            candidate_id: "candidate".to_string(),
            candidate_version: "v1".to_string(),
            candidate_definition_sha256: candidate_definition_sha256.clone(),
        };
        let predicted = OfflineCounterfactualEstimate {
            policy_id: "candidate".to_string(),
            policy_version: "v1".to_string(),
            policy_hash: candidate_policy.policy_hash.clone(),
            task_class: "codegen".to_string(),
            selection,
            source_candidate_id: "candidate".to_string(),
            source_candidate_version: "v1".to_string(),
            source_candidate_definition_sha256: candidate_definition_sha256,
            source_trace_ids: vec!["trace-candidate".to_string()],
            source_evidence_content_sha256: vec![format!("{:064x}", 5)],
            sample_count: 3,
            estimated_success_rate: 1.0,
            estimated_quality_score: 0.85,
            estimated_tool_success_score: 0.92,
            estimated_cost_usd: 0.08,
            estimated_latency_ms: 90.0,
            estimated_total_tokens: 18.0,
            estimated_retry_count: 0.0,
            estimation_method: "observed_comparable_candidate_cohort".to_string(),
        };
        let comparison = OfflinePolicyComparison {
            policy_id: "candidate".to_string(),
            policy_version: "v1".to_string(),
            policy_hash: candidate_policy.policy_hash.clone(),
            task_class: "codegen".to_string(),
            current_observed_candidate_id: "actual".to_string(),
            candidate_selection: predicted.selection.clone(),
            current_observed: observed.clone(),
            counterfactual: predicted.clone(),
            success_rate_delta: 0.0,
            quality_score_delta: 0.05,
            tool_success_score_delta: 0.02,
            cost_usd_delta: -0.02,
            latency_ms_delta: -10.0,
            total_tokens_delta: -2.0,
            retry_count_delta: 0.0,
        };
        let mut report = OfflineReplayReport {
            schema_version: OFFLINE_REPLAY_SCHEMA_VERSION.to_string(),
            status: OfflineReplayStatus::Sufficient,
            reason_codes: Vec::new(),
            current_policy,
            candidate_policies: vec![candidate_policy],
            observed_facts: vec![observed],
            counterfactual_estimates: vec![predicted],
            comparisons: vec![comparison],
            outcomes: Vec::new(),
            eligibility_content_sha256: format!("{:064x}", 6),
            replay_judge_calibrations: Vec::new(),
            source_trace_ids: vec!["trace-actual".to_string(), "trace-candidate".to_string()],
            source_evidence_content_sha256: vec![format!("{:064x}", 4), format!("{:064x}", 5)],
            shadow_only: true,
            influence_selected_tier: false,
            influence_executor_type: false,
            influence_retry_path: false,
            influence_routing_policy: false,
            content_sha256: String::new(),
        };
        report.content_sha256 = offline_replay_report_sha256(&report).unwrap();
        report
    }

    #[test]
    fn replay_comparison_is_hash_bound_shadow_only_and_reports_drift_boundary() {
        let report = replay_report();
        let comparison = ShadowRouter::compare_replay_report(&report).unwrap();
        assert_eq!(comparison.status, OfflineReplayStatus::Sufficient);
        assert_eq!(comparison.comparisons.len(), 1);
        assert_eq!(comparison.comparisons[0].observed.sample_count, 3);
        assert_eq!(comparison.comparisons[0].predicted.sample_count, 3);
        assert_eq!(
            comparison.comparisons[0].drift.status,
            "observed_comparable_cohort_not_live_candidate"
        );
        assert!(comparison.shadow_only);
        assert!(!comparison.influence_routing_policy);
        assert_eq!(
            comparison,
            ShadowRouter::compare_replay_report(&report).unwrap()
        );

        let mut tampered = report;
        tampered.content_sha256 = format!("{:064x}", 999);
        assert!(ShadowRouter::compare_replay_report(&tampered).is_err());
    }

    #[test]
    fn replay_comparison_preserves_insufficient_and_ood_outcomes() {
        let mut report = replay_report();
        report.status = OfflineReplayStatus::OutOfDistribution;
        report.reason_codes = vec!["candidate_out_of_distribution".to_string()];
        report.content_sha256 = offline_replay_report_sha256(&report).unwrap();
        let comparison = ShadowRouter::compare_replay_report(&report).unwrap();
        assert_eq!(comparison.status, OfflineReplayStatus::OutOfDistribution);
        assert!(comparison.comparisons.is_empty());
        assert_eq!(
            comparison.reason_codes,
            vec!["candidate_out_of_distribution"]
        );
    }
}
