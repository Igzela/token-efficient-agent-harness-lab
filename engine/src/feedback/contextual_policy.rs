use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::adaptive_fusion::{objective_weights, ObjectiveProfile};
use super::offline_evaluation::{CandidateAggregate, TaskClassEvaluation};
use super::policy_snapshot::stable_hash;
use crate::provider::redaction::contains_sensitive_patterns;

pub const CONTEXTUAL_POLICY_SCHEMA_VERSION: &str = "adaptive_contextual_policy.v1";
pub const CONTEXTUAL_POLICY_DECISION_SCHEMA_VERSION: &str = "adaptive_policy_decision.v1";
pub const CONTEXTUAL_POLICY_PROMOTION_SCHEMA_VERSION: &str = "adaptive_policy_promotion.v1";

const MAX_OBSERVATIONS: usize = 10_000;
const MAX_CANDIDATES: usize = 512;
const MAX_ID_BYTES: usize = 160;
const MAX_EVIDENCE_IDS: usize = 10_000;
const MIN_PROMOTION_SAMPLES: usize = 30;
const MIN_PROMOTION_CONFIDENCE: f64 = 0.85;
const MAX_EXPLORATION_RATE: f64 = 0.05;
const SEQUENCE_DECAY_BASE: f64 = 0.985;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextualPolicyRequest {
    pub schema_version: String,
    pub request_id: String,
    pub task_class: String,
    pub objective: ObjectiveProfile,
    pub risk_level: String,
    #[serde(default)]
    pub exploration_seed: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextualBanditObservation {
    pub schema_version: String,
    pub observation_id: String,
    pub run_id: String,
    pub task_class: String,
    pub objective: ObjectiveProfile,
    pub candidate_id: String,
    pub sequence: u64,
    pub success: bool,
    pub quality_score: f64,
    pub tool_success_score: f64,
    pub cost_efficiency_score: f64,
    pub latency_efficiency_score: f64,
    #[serde(default)]
    pub human_score: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextualCandidateScore {
    pub candidate_id: String,
    pub sample_count: usize,
    pub decayed_sample_weight: f64,
    pub utility_score: f64,
    pub quality_score: f64,
    pub success_score: f64,
    pub tool_success_score: f64,
    pub cost_efficiency_score: f64,
    pub latency_efficiency_score: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextualPolicyDecision {
    pub schema_version: String,
    pub task_class: String,
    pub objective: ObjectiveProfile,
    pub selected_candidate_id: String,
    pub baseline_candidate_id: Option<String>,
    pub exploration_assigned: bool,
    pub exploration_rate: f64,
    pub scorecards: Vec<ContextualCandidateScore>,
    pub shadow_only: bool,
    pub live_execution_authority: bool,
    pub requires_explicit_adaptive_plan: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextualPolicyPromotion {
    pub schema_version: String,
    pub task_class: String,
    pub objective: ObjectiveProfile,
    pub candidate_id: String,
    pub baseline_candidate_id: String,
    pub sample_count: usize,
    pub confidence: f64,
    pub mean_quality_delta: f64,
    pub mean_cost_reduction: f64,
    pub failure_rate_delta: f64,
    pub evidence_run_ids: Vec<String>,
    pub risk_level: String,
    pub confirm_adaptive_policy_promotion: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromotedAdaptivePolicy {
    pub schema_version: String,
    pub policy_key: String,
    pub task_class: String,
    pub objective: ObjectiveProfile,
    pub candidate_id: String,
    pub baseline_candidate_id: String,
    pub sample_count: usize,
    pub confidence: f64,
    pub mean_quality_delta: f64,
    pub mean_cost_reduction: f64,
    pub failure_rate_delta: f64,
    pub evidence_run_ids: Vec<String>,
    pub policy_hash: String,
    pub shadow_first: bool,
    pub live_execution_authority: bool,
    pub requires_explicit_adaptive_plan: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextualPolicyPromotionVerdict {
    pub schema_version: String,
    pub eligible: bool,
    pub blocked_reasons: Vec<String>,
    pub policy: Option<PromotedAdaptivePolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextualPolicyError {
    pub code: String,
    pub violations: Vec<String>,
}

impl ContextualPolicyError {
    fn validation(mut violations: Vec<String>) -> Self {
        violations.sort();
        violations.dedup();
        Self {
            code: "contextual_policy_validation_failed".to_string(),
            violations,
        }
    }
}

impl fmt::Display for ContextualPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.code, self.violations)
    }
}

impl std::error::Error for ContextualPolicyError {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdaptiveExplorationGate {
    enabled: bool,
    active: bool,
    killed: bool,
    max_rate: f64,
}

impl AdaptiveExplorationGate {
    pub fn from_env() -> Self {
        Self::from_flags(
            env_enabled("ACP_ENABLE_ADAPTIVE_EXPLORATION"),
            env_enabled("ACP_ADAPTIVE_EXPLORATION_ACTIVE"),
            env_enabled("ACP_ADAPTIVE_EXPLORATION_KILL_SWITCH"),
            MAX_EXPLORATION_RATE,
        )
    }

    pub fn from_flags(enabled: bool, active: bool, killed: bool, max_rate: f64) -> Self {
        Self {
            enabled,
            active,
            killed,
            max_rate: if max_rate.is_finite() {
                max_rate.clamp(0.0, MAX_EXPLORATION_RATE)
            } else {
                0.0
            },
        }
    }

    fn rate_for(self, risk_level: &str) -> f64 {
        if self.enabled && self.active && !self.killed && !high_risk(risk_level) {
            self.max_rate
        } else {
            0.0
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContextualPolicyPromotionGate {
    enabled: bool,
    active: bool,
}

impl ContextualPolicyPromotionGate {
    pub fn from_env() -> Self {
        Self::from_flags(
            env_enabled("ACP_ENABLE_ADAPTIVE_POLICY_PROMOTION"),
            env_enabled("ACP_ADAPTIVE_POLICY_PROMOTION_ACTIVE"),
        )
    }

    pub fn from_flags(enabled: bool, active: bool) -> Self {
        Self { enabled, active }
    }

    pub fn evaluate(
        self,
        promotion: &ContextualPolicyPromotion,
    ) -> ContextualPolicyPromotionVerdict {
        let mut blocked_reasons = validate_promotion(promotion);
        if !self.enabled || !self.active {
            blocked_reasons.push("adaptive_policy_promotion_gates_disabled".to_string());
        }
        if !promotion.confirm_adaptive_policy_promotion {
            blocked_reasons.push("confirm_adaptive_policy_promotion is required".to_string());
        }
        if promotion.sample_count < MIN_PROMOTION_SAMPLES {
            blocked_reasons.push("minimum_sample_count_not_met".to_string());
        }
        if promotion.confidence < MIN_PROMOTION_CONFIDENCE {
            blocked_reasons.push("minimum_confidence_not_met".to_string());
        }
        if promotion.mean_quality_delta < 0.0 {
            blocked_reasons.push("quality_regression_detected".to_string());
        }
        if promotion.mean_cost_reduction < 0.0 {
            blocked_reasons.push("cost_regression_detected".to_string());
        }
        if promotion.failure_rate_delta > 0.02 {
            blocked_reasons.push("failure_rate_regression_detected".to_string());
        }
        if high_risk(&promotion.risk_level) {
            blocked_reasons.push("high_risk_context_excluded".to_string());
        }
        blocked_reasons.sort();
        blocked_reasons.dedup();

        let policy = if blocked_reasons.is_empty() {
            Some(PromotedAdaptivePolicy::new(promotion))
        } else {
            None
        };
        ContextualPolicyPromotionVerdict {
            schema_version: CONTEXTUAL_POLICY_PROMOTION_SCHEMA_VERSION.to_string(),
            eligible: blocked_reasons.is_empty(),
            blocked_reasons,
            policy,
        }
    }
}

impl PromotedAdaptivePolicy {
    pub fn new(promotion: &ContextualPolicyPromotion) -> Self {
        let policy_key = contextual_policy_key(&promotion.task_class, promotion.objective);
        let mut evidence_run_ids = promotion.evidence_run_ids.clone();
        evidence_run_ids.sort();
        evidence_run_ids.dedup();
        let hash_input = json!({
            "schema_version": CONTEXTUAL_POLICY_SCHEMA_VERSION,
            "policy_key": policy_key,
            "task_class": promotion.task_class,
            "objective": promotion.objective,
            "candidate_id": promotion.candidate_id,
            "baseline_candidate_id": promotion.baseline_candidate_id,
            "sample_count": promotion.sample_count,
            "confidence": promotion.confidence,
            "mean_quality_delta": promotion.mean_quality_delta,
            "mean_cost_reduction": promotion.mean_cost_reduction,
            "failure_rate_delta": promotion.failure_rate_delta,
            "evidence_run_ids": evidence_run_ids,
            "shadow_first": true,
            "live_execution_authority": false,
            "requires_explicit_adaptive_plan": true,
        });
        let policy_hash = stable_hash(&hash_input);
        Self {
            schema_version: CONTEXTUAL_POLICY_SCHEMA_VERSION.to_string(),
            policy_key,
            task_class: promotion.task_class.clone(),
            objective: promotion.objective,
            candidate_id: promotion.candidate_id.clone(),
            baseline_candidate_id: promotion.baseline_candidate_id.clone(),
            sample_count: promotion.sample_count,
            confidence: promotion.confidence,
            mean_quality_delta: promotion.mean_quality_delta,
            mean_cost_reduction: promotion.mean_cost_reduction,
            failure_rate_delta: promotion.failure_rate_delta,
            evidence_run_ids,
            policy_hash,
            shadow_first: true,
            live_execution_authority: false,
            requires_explicit_adaptive_plan: true,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.schema_version == CONTEXTUAL_POLICY_SCHEMA_VERSION
            && self.policy_key == contextual_policy_key(&self.task_class, self.objective)
            && self.shadow_first
            && !self.live_execution_authority
            && self.requires_explicit_adaptive_plan
            && valid_id(&self.candidate_id)
            && valid_id(&self.baseline_candidate_id)
            && !contains_sensitive_patterns(&serde_json::to_string(self).unwrap_or_default())
            && self.policy_hash
                == PromotedAdaptivePolicy::new(&ContextualPolicyPromotion {
                    schema_version: CONTEXTUAL_POLICY_PROMOTION_SCHEMA_VERSION.to_string(),
                    task_class: self.task_class.clone(),
                    objective: self.objective,
                    candidate_id: self.candidate_id.clone(),
                    baseline_candidate_id: self.baseline_candidate_id.clone(),
                    sample_count: self.sample_count,
                    confidence: self.confidence,
                    mean_quality_delta: self.mean_quality_delta,
                    mean_cost_reduction: self.mean_cost_reduction,
                    failure_rate_delta: self.failure_rate_delta,
                    evidence_run_ids: self.evidence_run_ids.clone(),
                    risk_level: "low".to_string(),
                    confirm_adaptive_policy_promotion: true,
                })
                .policy_hash
    }
}

pub struct ContextualBanditEngine;

impl ContextualBanditEngine {
    pub fn decide(
        request: &ContextualPolicyRequest,
        evaluation: &TaskClassEvaluation,
        observations: &[ContextualBanditObservation],
        exploration_gate: &AdaptiveExplorationGate,
    ) -> Result<ContextualPolicyDecision, ContextualPolicyError> {
        let mut violations = validate_request(request);
        if evaluation.task_class != request.task_class {
            violations.push("evaluation_task_class_mismatch".to_string());
        }
        if observations.len() > MAX_OBSERVATIONS {
            violations.push("observation_limit_exceeded".to_string());
        }
        let candidate_count = evaluation.candidates.len();
        if candidate_count == 0 || candidate_count > MAX_CANDIDATES {
            violations.push("invalid_candidate_count".to_string());
        }
        if !violations.is_empty() {
            return Err(ContextualPolicyError::validation(violations));
        }

        let candidate_ids = evaluation
            .candidates
            .iter()
            .map(|candidate| candidate.candidate_id.as_str())
            .collect::<BTreeSet<_>>();
        if candidate_ids
            .iter()
            .any(|candidate_id| !valid_id(candidate_id))
        {
            return Err(ContextualPolicyError::validation(vec![
                "invalid_candidate_identity".to_string(),
            ]));
        }

        let mut accepted = Vec::new();
        let mut seen_observations = BTreeSet::new();
        let mut seen_run_candidate = BTreeSet::new();
        for observation in observations {
            let observation_violations = validate_observation(observation, request);
            if !observation_violations.is_empty() {
                return Err(ContextualPolicyError::validation(observation_violations));
            }
            if !candidate_ids.contains(observation.candidate_id.as_str()) {
                return Err(ContextualPolicyError::validation(vec![
                    "unknown_observation_candidate".to_string(),
                ]));
            }
            if !seen_observations.insert(observation.observation_id.clone()) {
                return Err(ContextualPolicyError::validation(vec![
                    "duplicate_observation_id".to_string(),
                ]));
            }
            if !seen_run_candidate
                .insert((observation.run_id.clone(), observation.candidate_id.clone()))
            {
                return Err(ContextualPolicyError::validation(vec![
                    "duplicate_run_candidate_observation".to_string(),
                ]));
            }
            accepted.push(observation.clone());
        }

        let scorecards = score_candidates(request.objective, &evaluation.candidates, &accepted);
        let Some(best) = scorecards.first() else {
            return Err(ContextualPolicyError::validation(vec![
                "no_candidate_scorecard".to_string(),
            ]));
        };
        let exploration_rate = exploration_gate.rate_for(&request.risk_level);
        let explore = exploration_rate > 0.0
            && scorecards.len() > 1
            && deterministic_bucket(&request.request_id, request.exploration_seed)
                < exploration_rate;
        let selected_candidate_id = if explore {
            scorecards
                .iter()
                .skip(1)
                .max_by(|left, right| {
                    left.decayed_sample_weight
                        .partial_cmp(&right.decayed_sample_weight)
                        .unwrap_or(Ordering::Equal)
                        .reverse()
                        .then_with(|| left.candidate_id.cmp(&right.candidate_id).reverse())
                })
                .map(|scorecard| scorecard.candidate_id.clone())
                .unwrap_or_else(|| best.candidate_id.clone())
        } else {
            best.candidate_id.clone()
        };
        Ok(ContextualPolicyDecision {
            schema_version: CONTEXTUAL_POLICY_DECISION_SCHEMA_VERSION.to_string(),
            task_class: request.task_class.clone(),
            objective: request.objective,
            selected_candidate_id,
            baseline_candidate_id: evaluation
                .recommendations
                .iter()
                .find(|recommendation| recommendation.objective == request.objective)
                .map(|recommendation| recommendation.candidate_id.clone()),
            exploration_assigned: explore,
            exploration_rate,
            scorecards,
            shadow_only: true,
            live_execution_authority: false,
            requires_explicit_adaptive_plan: true,
        })
    }
}

fn score_candidates(
    objective: ObjectiveProfile,
    candidates: &[CandidateAggregate],
    observations: &[ContextualBanditObservation],
) -> Vec<ContextualCandidateScore> {
    let weights = objective_weights(objective);
    let cost_range = metric_range(candidates, |candidate| candidate.average_cost_usd);
    let latency_range = metric_range(candidates, |candidate| candidate.average_latency_ms);
    let mut latest_sequence = observations
        .iter()
        .map(|observation| observation.sequence)
        .max()
        .unwrap_or_default();
    latest_sequence = latest_sequence.max(1);
    let mut scores = Vec::new();
    for candidate in candidates {
        let matching = observations
            .iter()
            .filter(|observation| observation.candidate_id == candidate.candidate_id)
            .collect::<Vec<_>>();
        let mut score = if matching.is_empty() {
            score_from_aggregate(candidate, objective, cost_range, latency_range)
        } else {
            score_from_observations(&matching, latest_sequence, objective)
        };
        let reliability = (score.success_score + score.tool_success_score) / 2.0;
        score.utility_score = score.quality_score * weights.quality
            + reliability * weights.success
            + score.cost_efficiency_score * weights.cost_efficiency
            + score.latency_efficiency_score * weights.latency_efficiency;
        scores.push(score);
    }
    scores.sort_by(|left, right| {
        right
            .utility_score
            .partial_cmp(&left.utility_score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.candidate_id.cmp(&right.candidate_id))
    });
    scores
}

fn score_from_aggregate(
    candidate: &CandidateAggregate,
    objective: ObjectiveProfile,
    cost_range: (f64, f64),
    latency_range: (f64, f64),
) -> ContextualCandidateScore {
    let cost_efficiency = inverse_normalized(candidate.average_cost_usd, cost_range);
    let latency_efficiency = inverse_normalized(candidate.average_latency_ms, latency_range);
    ContextualCandidateScore {
        candidate_id: candidate.candidate_id.clone(),
        sample_count: candidate.sample_count,
        decayed_sample_weight: candidate.sample_count as f64,
        utility_score: 0.0,
        quality_score: candidate.average_quality_score,
        success_score: candidate.success_rate,
        tool_success_score: candidate.average_tool_success_score,
        cost_efficiency_score: if objective == ObjectiveProfile::Quality {
            cost_efficiency.min(0.95)
        } else {
            cost_efficiency
        },
        latency_efficiency_score: if objective == ObjectiveProfile::Quality {
            latency_efficiency.min(0.95)
        } else {
            latency_efficiency
        },
    }
}

fn score_from_observations(
    observations: &[&ContextualBanditObservation],
    latest_sequence: u64,
    objective: ObjectiveProfile,
) -> ContextualCandidateScore {
    let mut total_weight = 0.0;
    let mut quality = 0.0;
    let mut success = 0.0;
    let mut tool_success = 0.0;
    let mut cost = 0.0;
    let mut latency = 0.0;
    for observation in observations {
        let age = latest_sequence.saturating_sub(observation.sequence) as f64;
        let weight = SEQUENCE_DECAY_BASE.powf(age);
        let quality_score = observation
            .human_score
            .map(|human| (human + observation.quality_score) / 2.0)
            .unwrap_or(observation.quality_score);
        total_weight += weight;
        quality += quality_score * weight;
        success += f64::from(observation.success) * weight;
        tool_success += observation.tool_success_score * weight;
        cost += observation.cost_efficiency_score * weight;
        latency += observation.latency_efficiency_score * weight;
    }
    let denominator = total_weight.max(f64::EPSILON);
    ContextualCandidateScore {
        candidate_id: observations[0].candidate_id.clone(),
        sample_count: observations.len(),
        decayed_sample_weight: total_weight,
        utility_score: 0.0,
        quality_score: quality / denominator,
        success_score: success / denominator,
        tool_success_score: tool_success / denominator,
        cost_efficiency_score: if objective == ObjectiveProfile::Quality {
            (cost / denominator).min(0.95)
        } else {
            cost / denominator
        },
        latency_efficiency_score: if objective == ObjectiveProfile::Quality {
            (latency / denominator).min(0.95)
        } else {
            latency / denominator
        },
    }
}

fn metric_range(
    candidates: &[CandidateAggregate],
    metric: impl Fn(&CandidateAggregate) -> f64,
) -> (f64, f64) {
    candidates.iter().fold(
        (f64::INFINITY, f64::NEG_INFINITY),
        |(minimum, maximum), candidate| {
            let value = metric(candidate);
            (minimum.min(value), maximum.max(value))
        },
    )
}

fn inverse_normalized(value: f64, (minimum, maximum): (f64, f64)) -> f64 {
    if (maximum - minimum).abs() < f64::EPSILON {
        1.0
    } else {
        ((maximum - value) / (maximum - minimum)).clamp(0.0, 1.0)
    }
}

fn validate_request(request: &ContextualPolicyRequest) -> Vec<String> {
    let mut violations = Vec::new();
    if request.schema_version != CONTEXTUAL_POLICY_SCHEMA_VERSION {
        violations.push("invalid_schema_version".to_string());
    }
    if !valid_id(&request.request_id) || !valid_id(&request.task_class) {
        violations.push("invalid_request_identity".to_string());
    }
    if !valid_risk(&request.risk_level) {
        violations.push("invalid_risk_level".to_string());
    }
    if serde_json::to_string(request)
        .ok()
        .is_some_and(|value| contains_sensitive_patterns(&value))
    {
        violations.push("sensitive_pattern_detected".to_string());
    }
    violations
}

fn validate_observation(
    observation: &ContextualBanditObservation,
    request: &ContextualPolicyRequest,
) -> Vec<String> {
    let mut violations = Vec::new();
    if observation.schema_version != CONTEXTUAL_POLICY_SCHEMA_VERSION {
        violations.push("invalid_observation_schema_version".to_string());
    }
    if observation.task_class != request.task_class || observation.objective != request.objective {
        violations.push("observation_context_mismatch".to_string());
    }
    if [
        &observation.observation_id,
        &observation.run_id,
        &observation.task_class,
        &observation.candidate_id,
    ]
    .iter()
    .any(|value| !valid_id(value))
    {
        violations.push("invalid_observation_identity".to_string());
    }
    if observation.sequence == 0 {
        violations.push("invalid_observation_sequence".to_string());
    }
    if ![
        observation.quality_score,
        observation.tool_success_score,
        observation.cost_efficiency_score,
        observation.latency_efficiency_score,
    ]
    .iter()
    .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
    {
        violations.push("invalid_observation_score".to_string());
    }
    if observation
        .human_score
        .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
    {
        violations.push("invalid_human_score".to_string());
    }
    if serde_json::to_string(observation)
        .ok()
        .is_some_and(|value| contains_sensitive_patterns(&value))
    {
        violations.push("sensitive_pattern_detected".to_string());
    }
    violations
}

fn validate_promotion(promotion: &ContextualPolicyPromotion) -> Vec<String> {
    let mut violations = Vec::new();
    if promotion.schema_version != CONTEXTUAL_POLICY_PROMOTION_SCHEMA_VERSION {
        violations.push("invalid_schema_version".to_string());
    }
    if !valid_id(&promotion.task_class)
        || !valid_id(&promotion.candidate_id)
        || !valid_id(&promotion.baseline_candidate_id)
    {
        violations.push("invalid_promotion_identity".to_string());
    }
    if !valid_risk(&promotion.risk_level) {
        violations.push("invalid_risk_level".to_string());
    }
    if !promotion.confidence.is_finite() || !(0.0..=1.0).contains(&promotion.confidence) {
        violations.push("invalid_confidence".to_string());
    }
    if ![
        promotion.mean_quality_delta,
        promotion.mean_cost_reduction,
        promotion.failure_rate_delta,
    ]
    .iter()
    .all(|value| value.is_finite())
    {
        violations.push("invalid_delta".to_string());
    }
    if promotion.evidence_run_ids.is_empty() || promotion.evidence_run_ids.len() > MAX_EVIDENCE_IDS
    {
        violations.push("invalid_evidence_count".to_string());
    }
    let mut evidence = BTreeSet::new();
    if promotion
        .evidence_run_ids
        .iter()
        .any(|id| !valid_id(id) || !evidence.insert(id))
    {
        violations.push("invalid_evidence_id".to_string());
    }
    if serde_json::to_string(promotion)
        .ok()
        .is_some_and(|value| contains_sensitive_patterns(&value))
    {
        violations.push("sensitive_pattern_detected".to_string());
    }
    violations
}

fn deterministic_bucket(request_id: &str, seed: u64) -> f64 {
    let hash = stable_hash(&json!({"request_id": request_id, "seed": seed}));
    let prefix = u64::from_str_radix(&hash[..16], 16).unwrap_or_default();
    prefix as f64 / u64::MAX as f64
}

pub fn contextual_policy_key(task_class: &str, objective: ObjectiveProfile) -> String {
    let objective = match objective {
        ObjectiveProfile::Efficient => "efficient",
        ObjectiveProfile::Quality => "quality",
    };
    format!("{task_class}:{objective}")
}

fn env_enabled(key: &str) -> bool {
    std::env::var(key)
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn valid_risk(value: &str) -> bool {
    matches!(value, "low" | "medium" | "high" | "critical")
}

fn high_risk(value: &str) -> bool {
    matches!(value, "high" | "critical")
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.:/@".contains(character))
        && !contains_sensitive_patterns(value)
}

#[allow(dead_code)]
fn _canonical_map(values: impl IntoIterator<Item = (String, Value)>) -> BTreeMap<String, Value> {
    values.into_iter().collect()
}
