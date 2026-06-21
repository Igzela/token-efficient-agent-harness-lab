use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use super::adaptive_fusion::{objective_weights, ObjectiveProfile};
use super::run_trace_recorder::RunTrace;
use crate::provider::redaction::contains_sensitive_patterns;

pub const OFFLINE_EVALUATION_SCHEMA_VERSION: &str = "offline_fusion_evaluation.v1";
const MAX_OBSERVATIONS: usize = 10_000;
const MAX_CANDIDATES_PER_TASK_CLASS: usize = 512;
const MAX_OBSERVATION_COST_USD: f64 = 1_000_000.0;
const MAX_ID_BYTES: usize = 160;
const MIN_JUDGE_CALIBRATION_SAMPLES: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateKind {
    Endpoint,
    Portfolio,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JudgeEvidence {
    pub judge_endpoint_id: String,
    pub judge_score: f64,
    pub reference_score: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfflineReplayObservation {
    pub schema_version: String,
    pub observation_id: String,
    pub run_id: String,
    pub task_class: String,
    pub candidate_id: String,
    pub candidate_kind: CandidateKind,
    pub member_endpoint_ids: Vec<String>,
    pub success: bool,
    pub quality_score: f64,
    pub tool_success_score: f64,
    pub cost_usd: f64,
    pub latency_ms: u64,
    pub judge_evidence: Option<JudgeEvidence>,
}

impl OfflineReplayObservation {
    #[allow(clippy::too_many_arguments)]
    pub fn from_run_trace(
        trace: &RunTrace,
        candidate_id: &str,
        candidate_kind: CandidateKind,
        member_endpoint_ids: Vec<String>,
        tool_success_score: f64,
        judge_evidence: Option<JudgeEvidence>,
    ) -> Result<Self, OfflineEvaluationError> {
        let Some(quality_score) = trace
            .evaluation
            .get("quality_score")
            .and_then(serde_json::Value::as_f64)
        else {
            return Err(OfflineEvaluationError::validation(vec![
                "missing_trace_quality_score".to_string(),
            ]));
        };
        let latency_ms = trace.latency_ms.unwrap_or_default();
        let Ok(latency_ms) = u64::try_from(latency_ms) else {
            return Err(OfflineEvaluationError::validation(vec![
                "invalid_latency_ms".to_string(),
            ]));
        };
        let mut observation = Self {
            schema_version: OFFLINE_EVALUATION_SCHEMA_VERSION.to_string(),
            observation_id: format!("replay-{}-{candidate_id}", trace.dispatch_id),
            run_id: trace.dispatch_id.clone(),
            task_class: trace.task_class.clone(),
            candidate_id: candidate_id.to_string(),
            candidate_kind,
            member_endpoint_ids,
            success: trace.success,
            quality_score,
            tool_success_score,
            cost_usd: trace.estimated_cost_usd.unwrap_or(trace.total_cost),
            latency_ms,
            judge_evidence,
        };
        normalize_observation(&mut observation);
        let violations = validate_observation(&observation);
        if violations.is_empty() {
            Ok(observation)
        } else {
            Err(OfflineEvaluationError::validation(violations))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateAggregate {
    pub candidate_id: String,
    pub candidate_kind: CandidateKind,
    pub member_endpoint_ids: Vec<String>,
    pub sample_count: usize,
    pub evidence_run_ids: Vec<String>,
    pub success_rate: f64,
    pub average_quality_score: f64,
    pub average_tool_success_score: f64,
    pub average_cost_usd: f64,
    pub average_latency_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShadowCandidateRecommendation {
    pub objective: ObjectiveProfile,
    pub candidate_id: String,
    pub utility_score: f64,
    pub evidence_run_ids: Vec<String>,
    pub shadow_only: bool,
    pub influence_selected_tier: bool,
    pub influence_executor_type: bool,
    pub influence_retry_path: bool,
    pub influence_routing_policy: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskClassEvaluation {
    pub task_class: String,
    pub candidates: Vec<CandidateAggregate>,
    pub pareto_candidate_ids: Vec<String>,
    pub recommendations: Vec<ShadowCandidateRecommendation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JudgeCalibration {
    pub judge_endpoint_id: String,
    pub sample_count: usize,
    pub mean_signed_bias: f64,
    pub mean_absolute_error: f64,
    pub recommended_score_offset: f64,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfflineEvaluationReport {
    pub schema_version: String,
    pub task_classes: Vec<TaskClassEvaluation>,
    pub judge_calibrations: Vec<JudgeCalibration>,
    pub accepted_observation_count: usize,
    pub rejected_observation_count: usize,
    pub rejection_reasons: BTreeMap<String, usize>,
    pub shadow_only: bool,
    pub influence_selected_tier: bool,
    pub influence_executor_type: bool,
    pub influence_retry_path: bool,
    pub influence_routing_policy: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfflineEvaluationError {
    pub code: String,
    pub violations: Vec<String>,
}

impl OfflineEvaluationError {
    fn validation(mut violations: Vec<String>) -> Self {
        violations.sort();
        violations.dedup();
        Self {
            code: "observation_validation_failed".to_string(),
            violations,
        }
    }

    fn observation_limit() -> Self {
        Self {
            code: "observation_limit_exceeded".to_string(),
            violations: vec!["observation_limit_exceeded".to_string()],
        }
    }
}

impl fmt::Display for OfflineEvaluationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.code, self.violations)
    }
}

impl std::error::Error for OfflineEvaluationError {}

pub struct OfflineEvaluationEngine;

impl OfflineEvaluationEngine {
    pub fn evaluate(
        observations: &[OfflineReplayObservation],
    ) -> Result<OfflineEvaluationReport, OfflineEvaluationError> {
        if observations.len() > MAX_OBSERVATIONS {
            return Err(OfflineEvaluationError::observation_limit());
        }

        let mut ordered = observations.to_vec();
        for observation in &mut ordered {
            normalize_observation(observation);
        }
        let mut ordered = ordered
            .into_iter()
            .map(|observation| {
                let canonical = canonical_observation(&observation).unwrap_or_default();
                (observation, canonical)
            })
            .collect::<Vec<_>>();
        ordered.sort_by(|(left, left_canonical), (right, right_canonical)| {
            left.observation_id
                .cmp(&right.observation_id)
                .then_with(|| left_canonical.cmp(right_canonical))
        });

        let mut seen_observation_ids = BTreeSet::new();
        let mut candidate_definitions =
            BTreeMap::<(String, String), (CandidateKind, Vec<String>)>::new();
        let mut task_candidate_counts = BTreeMap::<String, usize>::new();
        let mut accepted = Vec::new();
        let mut rejection_reasons = BTreeMap::<String, usize>::new();
        let mut rejected_observation_count = 0;
        for (observation, _) in ordered {
            let violations = validate_observation(&observation);
            if !violations.is_empty() {
                record_rejections(&mut rejection_reasons, &violations);
                rejected_observation_count += 1;
                continue;
            }
            if !seen_observation_ids.insert(observation.observation_id.clone()) {
                record_rejection(&mut rejection_reasons, "duplicate_observation_id");
                rejected_observation_count += 1;
                continue;
            }
            let definition_key = (
                observation.task_class.clone(),
                observation.candidate_id.clone(),
            );
            let definition = (
                observation.candidate_kind,
                observation.member_endpoint_ids.clone(),
            );
            if !candidate_definitions.contains_key(&definition_key)
                && task_candidate_counts
                    .get(&observation.task_class)
                    .copied()
                    .unwrap_or_default()
                    >= MAX_CANDIDATES_PER_TASK_CLASS
            {
                record_rejection(&mut rejection_reasons, "task_candidate_limit_exceeded");
                rejected_observation_count += 1;
                continue;
            }
            if candidate_definitions
                .get(&definition_key)
                .is_some_and(|existing| existing != &definition)
            {
                record_rejection(&mut rejection_reasons, "inconsistent_candidate_definition");
                rejected_observation_count += 1;
                continue;
            }
            if candidate_definitions
                .insert(definition_key, definition)
                .is_none()
            {
                *task_candidate_counts
                    .entry(observation.task_class.clone())
                    .or_default() += 1;
            }
            accepted.push(observation);
        }

        let task_classes = aggregate_task_classes(&accepted);
        let judge_calibrations = calibrate_judges(&accepted);
        Ok(OfflineEvaluationReport {
            schema_version: OFFLINE_EVALUATION_SCHEMA_VERSION.to_string(),
            task_classes,
            judge_calibrations,
            accepted_observation_count: accepted.len(),
            rejected_observation_count,
            rejection_reasons,
            shadow_only: true,
            influence_selected_tier: false,
            influence_executor_type: false,
            influence_retry_path: false,
            influence_routing_policy: false,
        })
    }
}

#[derive(Debug)]
struct CandidateAccumulator {
    candidate_kind: CandidateKind,
    member_endpoint_ids: Vec<String>,
    evidence_run_ids: BTreeSet<String>,
    sample_count: usize,
    success_count: usize,
    quality_total: f64,
    tool_success_total: f64,
    cost_total: f64,
    latency_total: u128,
}

impl CandidateAccumulator {
    fn new(observation: &OfflineReplayObservation) -> Self {
        Self {
            candidate_kind: observation.candidate_kind,
            member_endpoint_ids: observation.member_endpoint_ids.clone(),
            evidence_run_ids: BTreeSet::new(),
            sample_count: 0,
            success_count: 0,
            quality_total: 0.0,
            tool_success_total: 0.0,
            cost_total: 0.0,
            latency_total: 0,
        }
    }

    fn add(&mut self, observation: &OfflineReplayObservation) {
        self.evidence_run_ids.insert(observation.run_id.clone());
        self.sample_count += 1;
        self.success_count += usize::from(observation.success);
        self.quality_total += observation.quality_score;
        self.tool_success_total += observation.tool_success_score;
        self.cost_total += observation.cost_usd;
        self.latency_total += u128::from(observation.latency_ms);
    }

    fn aggregate(self, candidate_id: String) -> CandidateAggregate {
        let sample_count = self.sample_count;
        let denominator = sample_count as f64;
        CandidateAggregate {
            candidate_id,
            candidate_kind: self.candidate_kind,
            member_endpoint_ids: self.member_endpoint_ids,
            sample_count,
            evidence_run_ids: self.evidence_run_ids.into_iter().collect(),
            success_rate: self.success_count as f64 / denominator,
            average_quality_score: self.quality_total / denominator,
            average_tool_success_score: self.tool_success_total / denominator,
            average_cost_usd: self.cost_total / denominator,
            average_latency_ms: self.latency_total as f64 / denominator,
        }
    }
}

fn aggregate_task_classes(observations: &[OfflineReplayObservation]) -> Vec<TaskClassEvaluation> {
    let mut groups = BTreeMap::<String, BTreeMap<String, CandidateAccumulator>>::new();
    for observation in observations {
        let candidates = groups.entry(observation.task_class.clone()).or_default();
        candidates
            .entry(observation.candidate_id.clone())
            .or_insert_with(|| CandidateAccumulator::new(observation))
            .add(observation);
    }

    groups
        .into_iter()
        .map(|(task_class, candidates)| {
            let candidates = candidates
                .into_iter()
                .map(|(candidate_id, accumulator)| accumulator.aggregate(candidate_id))
                .collect::<Vec<_>>();
            let pareto_candidate_ids = pareto_frontier(&candidates);
            let frontier = candidates
                .iter()
                .filter(|candidate| pareto_candidate_ids.contains(&candidate.candidate_id))
                .collect::<Vec<_>>();
            let recommendations = [ObjectiveProfile::Efficient, ObjectiveProfile::Quality]
                .into_iter()
                .filter_map(|objective| recommend(objective, &frontier))
                .collect();
            TaskClassEvaluation {
                task_class,
                candidates,
                pareto_candidate_ids,
                recommendations,
            }
        })
        .collect()
}

fn pareto_frontier(candidates: &[CandidateAggregate]) -> Vec<String> {
    candidates
        .iter()
        .filter(|candidate| {
            !candidates.iter().any(|other| {
                other.candidate_id != candidate.candidate_id && dominates(other, candidate)
            })
        })
        .map(|candidate| candidate.candidate_id.clone())
        .collect()
}

fn dominates(left: &CandidateAggregate, right: &CandidateAggregate) -> bool {
    let no_worse = left.average_quality_score >= right.average_quality_score
        && left.success_rate >= right.success_rate
        && left.average_tool_success_score >= right.average_tool_success_score
        && left.average_cost_usd <= right.average_cost_usd
        && left.average_latency_ms <= right.average_latency_ms;
    let strictly_better = left.average_quality_score > right.average_quality_score
        || left.success_rate > right.success_rate
        || left.average_tool_success_score > right.average_tool_success_score
        || left.average_cost_usd < right.average_cost_usd
        || left.average_latency_ms < right.average_latency_ms;
    no_worse && strictly_better
}

fn recommend(
    objective: ObjectiveProfile,
    candidates: &[&CandidateAggregate],
) -> Option<ShadowCandidateRecommendation> {
    let cost_range = metric_range(candidates, |candidate| candidate.average_cost_usd);
    let latency_range = metric_range(candidates, |candidate| candidate.average_latency_ms);
    let weights = objective_weights(objective);
    let mut ranked = candidates
        .iter()
        .map(|candidate| {
            let reliability = (candidate.success_rate + candidate.average_tool_success_score) / 2.0;
            let cost_efficiency = inverse_normalized(candidate.average_cost_usd, cost_range);
            let latency_efficiency =
                inverse_normalized(candidate.average_latency_ms, latency_range);
            let utility = candidate.average_quality_score * weights.quality
                + reliability * weights.success
                + cost_efficiency * weights.cost_efficiency
                + latency_efficiency * weights.latency_efficiency;
            (*candidate, utility)
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|(left, left_score), (right, right_score)| {
        right_score
            .partial_cmp(left_score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.candidate_id.cmp(&right.candidate_id))
    });
    let (candidate, utility_score) = ranked.first().copied()?;
    Some(ShadowCandidateRecommendation {
        objective,
        candidate_id: candidate.candidate_id.clone(),
        utility_score,
        evidence_run_ids: candidate.evidence_run_ids.clone(),
        shadow_only: true,
        influence_selected_tier: false,
        influence_executor_type: false,
        influence_retry_path: false,
        influence_routing_policy: false,
    })
}

fn metric_range(
    candidates: &[&CandidateAggregate],
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
        (maximum - value) / (maximum - minimum)
    }
}

fn calibrate_judges(observations: &[OfflineReplayObservation]) -> Vec<JudgeCalibration> {
    let mut samples = BTreeMap::<String, Vec<f64>>::new();
    for evidence in observations
        .iter()
        .filter_map(|observation| observation.judge_evidence.as_ref())
    {
        samples
            .entry(evidence.judge_endpoint_id.clone())
            .or_default()
            .push(evidence.judge_score - evidence.reference_score);
    }
    samples
        .into_iter()
        .map(|(judge_endpoint_id, biases)| {
            let sample_count = biases.len();
            let denominator = sample_count as f64;
            let mean_signed_bias = biases.iter().sum::<f64>() / denominator;
            let mean_absolute_error =
                biases.iter().map(|bias| bias.abs()).sum::<f64>() / denominator;
            JudgeCalibration {
                judge_endpoint_id,
                sample_count,
                mean_signed_bias,
                mean_absolute_error,
                recommended_score_offset: -mean_signed_bias,
                status: if sample_count >= MIN_JUDGE_CALIBRATION_SAMPLES {
                    "calibrated"
                } else {
                    "insufficient_data"
                }
                .to_string(),
            }
        })
        .collect()
}

fn normalize_observation(observation: &mut OfflineReplayObservation) {
    observation.member_endpoint_ids.sort();
    observation.member_endpoint_ids.dedup();
}

fn validate_observation(observation: &OfflineReplayObservation) -> Vec<String> {
    let mut violations = Vec::new();
    if observation.schema_version != OFFLINE_EVALUATION_SCHEMA_VERSION {
        violations.push("invalid_schema_version".to_string());
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
    let valid_members = observation
        .member_endpoint_ids
        .iter()
        .all(|member| valid_id(member));
    let valid_member_count = match observation.candidate_kind {
        CandidateKind::Endpoint => {
            observation.member_endpoint_ids.len() == 1
                && observation.member_endpoint_ids[0] == observation.candidate_id
        }
        CandidateKind::Portfolio => observation.member_endpoint_ids.len() >= 2,
    };
    if !valid_members || !valid_member_count {
        violations.push("invalid_candidate_members".to_string());
    }
    if !normalized(observation.quality_score) {
        violations.push("invalid_quality_score".to_string());
    }
    if !normalized(observation.tool_success_score) {
        violations.push("invalid_tool_success_score".to_string());
    }
    if !observation.cost_usd.is_finite()
        || !(0.0..=MAX_OBSERVATION_COST_USD).contains(&observation.cost_usd)
    {
        violations.push("invalid_cost_usd".to_string());
    }
    if let Some(evidence) = &observation.judge_evidence {
        if !valid_id(&evidence.judge_endpoint_id)
            || !normalized(evidence.judge_score)
            || !normalized(evidence.reference_score)
        {
            violations.push("invalid_judge_evidence".to_string());
        }
    }
    if canonical_observation(observation).is_some_and(|value| contains_sensitive_patterns(&value)) {
        violations.push("sensitive_pattern_detected".to_string());
    }
    violations
}

fn canonical_observation(observation: &OfflineReplayObservation) -> Option<String> {
    serde_json::to_string(observation).ok()
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.:/@".contains(character))
}

fn normalized(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

fn record_rejections(reasons: &mut BTreeMap<String, usize>, violations: &[String]) {
    for violation in violations {
        record_rejection(reasons, violation);
    }
}

fn record_rejection(reasons: &mut BTreeMap<String, usize>, reason: &str) {
    *reasons.entry(reason.to_string()).or_default() += 1;
}
