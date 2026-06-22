use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{
    ContextualPolicyPromotion, ContextualPolicyPromotionVerdict, ObjectiveProfile,
    PromotedAdaptivePolicy, CONTEXTUAL_POLICY_PROMOTION_SCHEMA_VERSION,
};
use crate::provider::redaction::contains_sensitive_patterns;
use crate::trusted_local::EffectiveExecutionGates;

const MAX_EVIDENCE: usize = 10_000;
const MAX_ID_BYTES: usize = 160;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveAutoPromotionEvidence {
    pub observation_id: String,
    pub run_id: String,
    pub task_class: String,
    pub objective: ObjectiveProfile,
    pub candidate_id: String,
    pub sequence: u64,
    pub success: bool,
    pub quality_score: f64,
    pub cost_usd: f64,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveAutoPromotionRequest {
    pub task_class: String,
    pub objective: ObjectiveProfile,
    pub risk_level: String,
    pub candidate_id: String,
    pub baseline_candidate_id: String,
    pub expected_active_policy_hash: Option<String>,
    pub rollout_percentage: u8,
}

impl AdaptiveAutoPromotionRequest {
    pub fn from_env(
        task_class: &str,
        objective: ObjectiveProfile,
        risk_level: &str,
        candidate_id: &str,
        baseline_candidate_id: &str,
        expected_active_policy_hash: Option<String>,
    ) -> Self {
        Self {
            task_class: task_class.to_string(),
            objective,
            risk_level: risk_level.to_string(),
            candidate_id: candidate_id.to_string(),
            baseline_candidate_id: baseline_candidate_id.to_string(),
            expected_active_policy_hash,
            rollout_percentage: env_u8("ACP_ADAPTIVE_AUTO_PROMOTION_ROLLOUT_PERCENTAGE")
                .unwrap_or(10),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveAutoPromotionPolicy {
    pub min_samples_per_candidate: usize,
    pub min_confidence: f64,
    pub min_quality_delta: f64,
    pub min_cost_reduction: f64,
    pub min_latency_reduction_ms: f64,
    pub max_failure_rate_delta: f64,
    pub max_evidence_age_sequences: u64,
}

impl Default for AdaptiveAutoPromotionPolicy {
    fn default() -> Self {
        Self {
            min_samples_per_candidate: 30,
            min_confidence: 0.85,
            min_quality_delta: 0.0,
            min_cost_reduction: 0.0,
            min_latency_reduction_ms: 0.0,
            max_failure_rate_delta: 0.02,
            max_evidence_age_sequences: 1_000,
        }
    }
}

impl AdaptiveAutoPromotionPolicy {
    pub fn from_env() -> Self {
        let defaults = Self::default();
        Self {
            min_samples_per_candidate: env_usize("ACP_ADAPTIVE_AUTO_PROMOTION_MIN_SAMPLES")
                .unwrap_or(defaults.min_samples_per_candidate),
            min_confidence: env_f64("ACP_ADAPTIVE_AUTO_PROMOTION_MIN_CONFIDENCE")
                .unwrap_or(defaults.min_confidence),
            min_quality_delta: env_f64("ACP_ADAPTIVE_AUTO_PROMOTION_MIN_QUALITY_DELTA")
                .unwrap_or(defaults.min_quality_delta),
            min_cost_reduction: env_f64("ACP_ADAPTIVE_AUTO_PROMOTION_MIN_COST_REDUCTION")
                .unwrap_or(defaults.min_cost_reduction),
            min_latency_reduction_ms: env_f64(
                "ACP_ADAPTIVE_AUTO_PROMOTION_MIN_LATENCY_REDUCTION_MS",
            )
            .unwrap_or(defaults.min_latency_reduction_ms),
            max_failure_rate_delta: env_f64("ACP_ADAPTIVE_AUTO_PROMOTION_MAX_FAILURE_RATE_DELTA")
                .unwrap_or(defaults.max_failure_rate_delta),
            max_evidence_age_sequences: env_u64(
                "ACP_ADAPTIVE_AUTO_PROMOTION_MAX_EVIDENCE_AGE_SEQUENCES",
            )
            .unwrap_or(defaults.max_evidence_age_sequences),
        }
    }

    pub fn validation_errors(&self) -> Vec<String> {
        validate_policy(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdaptiveAutoPromotionGate {
    enabled: bool,
    active: bool,
    killed: bool,
}

impl AdaptiveAutoPromotionGate {
    pub fn from_env() -> Self {
        let gates = EffectiveExecutionGates::from_env();
        Self::from_flags(
            gates.auto_promotion_enabled,
            gates.auto_promotion_active,
            env_enabled("ACP_ADAPTIVE_AUTO_PROMOTION_KILL_SWITCH"),
        )
    }

    pub fn from_flags(enabled: bool, active: bool, killed: bool) -> Self {
        Self {
            enabled,
            active,
            killed,
        }
    }

    pub fn is_configured(self) -> bool {
        self.enabled || self.active || self.killed
    }
}

pub struct AdaptiveAutoPromotionController;

impl AdaptiveAutoPromotionController {
    pub fn evaluate(
        request: &AdaptiveAutoPromotionRequest,
        evidence: &[AdaptiveAutoPromotionEvidence],
        active_policy: Option<&PromotedAdaptivePolicy>,
        policy: &AdaptiveAutoPromotionPolicy,
        gate: &AdaptiveAutoPromotionGate,
    ) -> ContextualPolicyPromotionVerdict {
        let mut blocked_reasons = validate_request(request);
        blocked_reasons.extend(validate_policy(policy));
        if !gate.enabled || !gate.active {
            blocked_reasons.push("adaptive_auto_promotion_gates_disabled".to_string());
        }
        if gate.killed {
            blocked_reasons.push("adaptive_auto_promotion_kill_switch_active".to_string());
        }
        if matches!(request.risk_level.as_str(), "high" | "critical") {
            blocked_reasons.push("high_risk_context_excluded".to_string());
        }
        validate_active_policy(request, active_policy, &mut blocked_reasons);

        if evidence.len() > MAX_EVIDENCE {
            blocked_reasons.push("evidence_limit_exceeded".to_string());
        }
        let latest_sequence = evidence
            .iter()
            .map(|observation| observation.sequence)
            .max()
            .unwrap_or_default();
        let minimum_sequence = latest_sequence.saturating_sub(policy.max_evidence_age_sequences);
        let mut seen_observations = BTreeSet::new();
        let mut seen_run_candidates = BTreeSet::new();
        let mut candidate = Vec::new();
        let mut baseline = Vec::new();
        for observation in evidence {
            let relevant = observation.task_class == request.task_class
                && observation.objective == request.objective
                && matches!(
                    observation.candidate_id.as_str(),
                    id if id == request.candidate_id || id == request.baseline_candidate_id
                );
            if !relevant {
                continue;
            }
            blocked_reasons.extend(validate_evidence(observation));
            if !seen_observations.insert(observation.observation_id.clone()) {
                blocked_reasons.push("duplicate_observation_id".to_string());
            }
            if !seen_run_candidates
                .insert((observation.run_id.clone(), observation.candidate_id.clone()))
            {
                blocked_reasons.push("duplicate_run_candidate_observation".to_string());
            }
            if observation.sequence < minimum_sequence {
                continue;
            }
            if observation.candidate_id == request.candidate_id {
                candidate.push(observation);
            } else {
                baseline.push(observation);
            }
        }

        if candidate.is_empty() {
            blocked_reasons.push("candidate_evidence_missing".to_string());
        }
        if baseline.is_empty() {
            blocked_reasons.push("baseline_evidence_missing".to_string());
        }
        if (!evidence.is_empty() && candidate.is_empty())
            || (!evidence.is_empty() && baseline.is_empty())
        {
            blocked_reasons.push("fresh_evidence_missing".to_string());
        }
        if candidate.len() < policy.min_samples_per_candidate
            || baseline.len() < policy.min_samples_per_candidate
        {
            blocked_reasons.push("minimum_sample_count_not_met".to_string());
        }

        let confidence = confidence(candidate.len().min(baseline.len()));
        if confidence < policy.min_confidence {
            blocked_reasons.push("minimum_confidence_not_met".to_string());
        }
        let candidate_metrics = Metrics::from_evidence(&candidate);
        let baseline_metrics = Metrics::from_evidence(&baseline);
        let mean_quality_delta = candidate_metrics.quality - baseline_metrics.quality;
        let mean_cost_reduction = baseline_metrics.cost - candidate_metrics.cost;
        let mean_latency_reduction = baseline_metrics.latency - candidate_metrics.latency;
        let failure_rate_delta = candidate_metrics.failure_rate - baseline_metrics.failure_rate;
        if mean_quality_delta < policy.min_quality_delta {
            blocked_reasons.push("quality_regression_detected".to_string());
        }
        if mean_cost_reduction < policy.min_cost_reduction {
            blocked_reasons.push("cost_regression_detected".to_string());
        }
        if mean_latency_reduction < policy.min_latency_reduction_ms {
            blocked_reasons.push("latency_regression_detected".to_string());
        }
        if failure_rate_delta > policy.max_failure_rate_delta {
            blocked_reasons.push("failure_rate_regression_detected".to_string());
        }
        blocked_reasons.sort();
        blocked_reasons.dedup();

        if !blocked_reasons.is_empty() {
            return ContextualPolicyPromotionVerdict {
                schema_version: CONTEXTUAL_POLICY_PROMOTION_SCHEMA_VERSION.to_string(),
                eligible: false,
                blocked_reasons,
                policy: None,
            };
        }

        let promotion = ContextualPolicyPromotion {
            schema_version: CONTEXTUAL_POLICY_PROMOTION_SCHEMA_VERSION.to_string(),
            task_class: request.task_class.clone(),
            objective: request.objective,
            candidate_id: request.candidate_id.clone(),
            baseline_candidate_id: request.baseline_candidate_id.clone(),
            sample_count: candidate.len(),
            confidence,
            mean_quality_delta,
            mean_cost_reduction,
            failure_rate_delta,
            evidence_run_ids: candidate
                .iter()
                .chain(baseline.iter())
                .map(|observation| observation.run_id.clone())
                .collect(),
            risk_level: request.risk_level.clone(),
            confirm_adaptive_policy_promotion: false,
        };
        ContextualPolicyPromotionVerdict {
            schema_version: CONTEXTUAL_POLICY_PROMOTION_SCHEMA_VERSION.to_string(),
            eligible: true,
            blocked_reasons: Vec::new(),
            policy: Some(PromotedAdaptivePolicy::new_auto(
                &promotion,
                mean_latency_reduction,
                request.rollout_percentage,
                active_policy.map(|active| active.policy_hash.clone()),
            )),
        }
    }
}

#[derive(Default)]
struct Metrics {
    quality: f64,
    cost: f64,
    latency: f64,
    failure_rate: f64,
}

impl Metrics {
    fn from_evidence(evidence: &[&AdaptiveAutoPromotionEvidence]) -> Self {
        if evidence.is_empty() {
            return Self::default();
        }
        let denominator = evidence.len() as f64;
        Self {
            quality: evidence
                .iter()
                .map(|observation| observation.quality_score)
                .sum::<f64>()
                / denominator,
            cost: evidence
                .iter()
                .map(|observation| observation.cost_usd)
                .sum::<f64>()
                / denominator,
            latency: evidence
                .iter()
                .map(|observation| observation.latency_ms as f64)
                .sum::<f64>()
                / denominator,
            failure_rate: evidence
                .iter()
                .filter(|observation| !observation.success)
                .count() as f64
                / denominator,
        }
    }
}

fn validate_request(request: &AdaptiveAutoPromotionRequest) -> Vec<String> {
    let mut violations = Vec::new();
    if [
        &request.task_class,
        &request.risk_level,
        &request.candidate_id,
        &request.baseline_candidate_id,
    ]
    .iter()
    .any(|value| !valid_id(value))
    {
        violations.push("invalid_auto_promotion_identity".to_string());
    }
    if request.candidate_id == request.baseline_candidate_id {
        violations.push("candidate_matches_baseline".to_string());
    }
    if !matches!(
        request.risk_level.as_str(),
        "low" | "medium" | "high" | "critical"
    ) {
        violations.push("invalid_risk_level".to_string());
    }
    if !(1..=100).contains(&request.rollout_percentage) {
        violations.push("invalid_rollout_percentage".to_string());
    }
    if request
        .expected_active_policy_hash
        .as_ref()
        .is_some_and(|hash| !valid_hash(hash))
    {
        violations.push("invalid_expected_policy_hash".to_string());
    }
    if contains_sensitive_patterns(&serde_json::to_string(request).unwrap_or_default()) {
        violations.push("sensitive_pattern_detected".to_string());
    }
    violations
}

fn validate_policy(policy: &AdaptiveAutoPromotionPolicy) -> Vec<String> {
    let mut violations = Vec::new();
    if policy.min_samples_per_candidate == 0 {
        violations.push("invalid_minimum_samples".to_string());
    }
    if !normalized(policy.min_confidence) {
        violations.push("invalid_minimum_confidence".to_string());
    }
    if ![
        policy.min_quality_delta,
        policy.min_cost_reduction,
        policy.min_latency_reduction_ms,
        policy.max_failure_rate_delta,
    ]
    .iter()
    .all(|value| value.is_finite())
    {
        violations.push("invalid_auto_promotion_threshold".to_string());
    }
    if policy.max_evidence_age_sequences == 0 {
        violations.push("invalid_evidence_freshness_window".to_string());
    }
    violations
}

fn validate_active_policy(
    request: &AdaptiveAutoPromotionRequest,
    active_policy: Option<&PromotedAdaptivePolicy>,
    blocked_reasons: &mut Vec<String>,
) {
    match active_policy {
        Some(active) => {
            if request.expected_active_policy_hash.as_deref() != Some(active.policy_hash.as_str()) {
                blocked_reasons.push("active_policy_hash_stale".to_string());
            }
            if active.candidate_id != request.baseline_candidate_id {
                blocked_reasons.push("active_policy_baseline_mismatch".to_string());
            }
        }
        None if request.expected_active_policy_hash.is_some() => {
            blocked_reasons.push("active_policy_missing".to_string());
        }
        None => {}
    }
}

fn validate_evidence(observation: &AdaptiveAutoPromotionEvidence) -> Vec<String> {
    let mut violations = Vec::new();
    if [
        &observation.observation_id,
        &observation.run_id,
        &observation.task_class,
        &observation.candidate_id,
    ]
    .iter()
    .any(|value| !valid_id(value))
        || observation.sequence == 0
    {
        violations.push("invalid_auto_promotion_evidence_identity".to_string());
    }
    if !normalized(observation.quality_score)
        || !observation.cost_usd.is_finite()
        || observation.cost_usd < 0.0
    {
        violations.push("invalid_auto_promotion_evidence_metrics".to_string());
    }
    if contains_sensitive_patterns(&serde_json::to_string(observation).unwrap_or_default()) {
        violations.push("sensitive_pattern_detected".to_string());
    }
    violations
}

fn confidence(samples: usize) -> f64 {
    if samples == 0 {
        0.0
    } else {
        samples as f64 / (samples as f64 + 1.0)
    }
}

fn normalized(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn env_enabled(key: &str) -> bool {
    std::env::var(key)
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn env_f64(key: &str) -> Option<f64> {
    std::env::var(key).ok()?.parse().ok()
}

fn env_u64(key: &str) -> Option<u64> {
    std::env::var(key).ok()?.parse().ok()
}

fn env_usize(key: &str) -> Option<usize> {
    std::env::var(key).ok()?.parse().ok()
}

fn env_u8(key: &str) -> Option<u8> {
    std::env::var(key).ok()?.parse().ok()
}

#[cfg(test)]
mod policy_validation_tests {
    use super::*;

    #[test]
    fn validation_errors_reuse_runtime_policy_rules() {
        let policy = AdaptiveAutoPromotionPolicy {
            min_samples_per_candidate: 0,
            min_confidence: 2.0,
            min_quality_delta: f64::NAN,
            min_cost_reduction: 0.0,
            min_latency_reduction_ms: 0.0,
            max_failure_rate_delta: 0.0,
            max_evidence_age_sequences: 0,
        };

        assert_eq!(
            policy.validation_errors(),
            vec![
                "invalid_minimum_samples",
                "invalid_minimum_confidence",
                "invalid_auto_promotion_threshold",
                "invalid_evidence_freshness_window",
            ]
        );
        assert!(AdaptiveAutoPromotionPolicy::default()
            .validation_errors()
            .is_empty());
    }
}
