use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::adaptive_fusion::{objective_weights, ObjectiveProfile};
use super::replay_eligibility::{
    canonical_json, evaluate_replay_eligibility, replay_eligibility_result_sha256,
    replay_observation_evidence_sha256, replay_reason_category, CostEvidenceKind,
    EvidenceDisposition, JudgeCalibrationEvidence, ReplayEligibilityRequest,
    ReplayEligibilityResult, ReplayObservationEvidence, ReplayReasonCategory,
    POLICY_REPLAY_CONTRACT_SCHEMA_VERSION, TRACE_REPLAY_EVIDENCE_SCHEMA_VERSION,
};
use super::run_trace_recorder::RunTrace;
use crate::provider::redaction::contains_sensitive_patterns;

pub const OFFLINE_EVALUATION_SCHEMA_VERSION: &str = "offline_fusion_evaluation.v1";
pub const OFFLINE_REPLAY_SCHEMA_VERSION: &str = "offline_policy_replay.v2";
pub const LEGACY_OFFLINE_REPLAY_SCHEMA_VERSION: &str = "offline_policy_replay.v1";
const MAX_OBSERVATIONS: usize = 10_000;
const MAX_CANDIDATES_PER_TASK_CLASS: usize = 512;
const MAX_OBSERVATION_COST_USD: f64 = 1_000_000.0;
const MAX_ID_BYTES: usize = 160;
const MAX_MEMBER_ENDPOINTS: usize = 64;
const MAX_OBSERVATION_CANONICAL_BYTES: usize = 1024 * 1024;
const MAX_REPLAY_REQUEST_CANONICAL_BYTES: usize = 16 * 1024 * 1024;
const MAX_REPLAY_RESULT_CANONICAL_BYTES: usize = 16 * 1024 * 1024;
const MIN_JUDGE_CALIBRATION_SAMPLES: usize = 3;
const MAX_POLICY_CANDIDATES: usize = 32;
const MAX_POLICY_SELECTIONS_PER_POLICY: usize = 512;
const MAX_REPORT_TASK_CLASSES: usize = 512;
const MAX_REPORT_FACTS: usize = MAX_OBSERVATIONS;
const MAX_REPORT_OUTCOMES: usize = MAX_OBSERVATIONS;
const MAX_REPORT_COMPARISONS: usize = MAX_OBSERVATIONS;
const MAX_REPORT_REASON_CODES: usize = 256;
const MAX_REPORT_JUDGES: usize = 128;
const MAX_REPORT_JUDGE_PAIRS_PER_JUDGE: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflinePolicySelection {
    pub candidate_id: String,
    pub candidate_version: String,
    pub candidate_definition_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfflinePolicyDefinition {
    pub schema_version: String,
    pub policy_id: String,
    pub policy_version: String,
    pub policy_hash: String,
    pub selections: BTreeMap<String, OfflinePolicySelection>,
}

impl OfflinePolicyDefinition {
    pub fn new(
        policy_id: impl Into<String>,
        policy_version: impl Into<String>,
        selections: BTreeMap<String, OfflinePolicySelection>,
    ) -> Result<Self, OfflineEvaluationError> {
        let mut definition = Self {
            schema_version: OFFLINE_REPLAY_SCHEMA_VERSION.to_string(),
            policy_id: policy_id.into(),
            policy_version: policy_version.into(),
            policy_hash: String::new(),
            selections,
        };
        definition.policy_hash = definition.content_sha256()?;
        Ok(definition)
    }

    pub fn content_sha256(&self) -> Result<String, OfflineEvaluationError> {
        let mut unsigned = self.clone();
        unsigned.policy_hash.clear();
        canonical_sha256(&unsigned).ok_or_else(|| {
            OfflineEvaluationError::validation(vec!["policy_serialization_failed".to_string()])
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfflineReplayRequest {
    pub schema_version: String,
    /// Raw trace-owner input. The eligibility result is always derived inside
    /// `replay_policies`; callers cannot establish replay authority by
    /// supplying `eligible`, accepted observations, coverage, or calibration.
    pub eligibility: ReplayEligibilityRequest,
    pub current_policy: OfflinePolicyDefinition,
    pub candidate_policies: Vec<OfflinePolicyDefinition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfflineReplayStatus {
    Sufficient,
    InsufficientEvidence,
    IncompatibleCohort,
    StaleEvidence,
    TamperedEvidence,
    UncalibratedEvidence,
    OutOfDistribution,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfflineObservedFacts {
    pub task_class: String,
    pub candidate_id: String,
    pub candidate_version: String,
    pub candidate_definition_sha256: String,
    pub member_endpoint_ids: Vec<String>,
    pub trace_ids: Vec<String>,
    pub evidence_content_sha256: Vec<String>,
    pub sample_count: usize,
    pub success_rate: f64,
    pub average_quality_score: f64,
    pub average_tool_success_score: f64,
    pub average_cost_usd: f64,
    pub average_latency_ms: f64,
    pub average_total_tokens: f64,
    pub average_retry_count: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfflineCounterfactualEstimate {
    pub policy_id: String,
    pub policy_version: String,
    pub policy_hash: String,
    pub task_class: String,
    pub selection: OfflinePolicySelection,
    pub source_candidate_id: String,
    pub source_candidate_version: String,
    pub source_candidate_definition_sha256: String,
    pub source_trace_ids: Vec<String>,
    pub source_evidence_content_sha256: Vec<String>,
    pub sample_count: usize,
    pub estimated_success_rate: f64,
    pub estimated_quality_score: f64,
    pub estimated_tool_success_score: f64,
    pub estimated_cost_usd: f64,
    pub estimated_latency_ms: f64,
    pub estimated_total_tokens: f64,
    pub estimated_retry_count: f64,
    pub estimation_method: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfflinePolicyComparison {
    pub policy_id: String,
    pub policy_version: String,
    pub policy_hash: String,
    pub task_class: String,
    pub current_observed_candidate_id: String,
    pub candidate_selection: OfflinePolicySelection,
    pub current_observed: OfflineObservedFacts,
    pub counterfactual: OfflineCounterfactualEstimate,
    pub success_rate_delta: f64,
    pub quality_score_delta: f64,
    pub tool_success_score_delta: f64,
    pub cost_usd_delta: f64,
    pub latency_ms_delta: f64,
    pub total_tokens_delta: f64,
    pub retry_count_delta: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfflineReplayOutcome {
    pub status: OfflineReplayStatus,
    pub policy_id: Option<String>,
    pub task_class: Option<String>,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfflineReplayReport {
    pub schema_version: String,
    pub status: OfflineReplayStatus,
    pub reason_codes: Vec<String>,
    pub current_policy: OfflinePolicyDefinition,
    pub candidate_policies: Vec<OfflinePolicyDefinition>,
    pub observed_facts: Vec<OfflineObservedFacts>,
    pub counterfactual_estimates: Vec<OfflineCounterfactualEstimate>,
    pub comparisons: Vec<OfflinePolicyComparison>,
    pub outcomes: Vec<OfflineReplayOutcome>,
    pub eligibility_content_sha256: String,
    /// Calibration derived from paired judge/reference values in the trace-backed eligibility result.
    pub replay_judge_calibrations: Vec<JudgeCalibrationEvidence>,
    pub source_trace_ids: Vec<String>,
    pub source_evidence_content_sha256: Vec<String>,
    pub shadow_only: bool,
    pub influence_selected_tier: bool,
    pub influence_executor_type: bool,
    pub influence_retry_path: bool,
    pub influence_routing_policy: bool,
    pub content_sha256: String,
}

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
    /// Convert only an accepted trace-derived replay observation into the
    /// legacy aggregation shape. Caller-supplied candidate definitions and
    /// coverage claims do not enter this adapter.
    pub fn from_replay_evidence(
        evidence: &ReplayObservationEvidence,
    ) -> Result<Self, OfflineEvaluationError> {
        if evidence.disposition != EvidenceDisposition::Accepted {
            return Err(OfflineEvaluationError::validation(vec![
                "replay_evidence_not_accepted".to_string(),
            ]));
        }
        let observation = &evidence.observation;
        let (
            Some(candidate_id),
            Some(task_class),
            Some(quality_score),
            Some(tool_success_score),
            Some(cost_usd),
            Some(latency_ms),
            Some(success),
        ) = (
            observation.candidate_id.clone(),
            observation.task_class.clone(),
            observation.quality_score,
            observation.tool_success_score,
            observation.cost_usd,
            observation.latency_ms,
            observation.success,
        )
        else {
            return Err(OfflineEvaluationError::validation(vec![
                "incomplete_trace_replay_measurements".to_string(),
            ]));
        };
        if !matches!(
            observation.cost_kind,
            Some(CostEvidenceKind::Measured | CostEvidenceKind::Posted)
        ) {
            return Err(OfflineEvaluationError::validation(vec![
                "unmeasured_or_unpriced_trace_cost".to_string(),
            ]));
        }
        let candidate_kind = if observation.member_endpoint_ids.len() <= 1 {
            CandidateKind::Endpoint
        } else {
            CandidateKind::Portfolio
        };
        let judge_evidence = observation
            .judge_reference
            .as_ref()
            .map(|pair| JudgeEvidence {
                judge_endpoint_id: pair.judge_endpoint_id.clone(),
                judge_score: pair.judge_score,
                reference_score: pair.reference_score,
            });
        Ok(Self {
            schema_version: OFFLINE_EVALUATION_SCHEMA_VERSION.to_string(),
            observation_id: observation.observation_id.clone(),
            run_id: observation.dispatch_id.clone(),
            task_class,
            candidate_id,
            candidate_kind,
            member_endpoint_ids: observation.member_endpoint_ids.clone(),
            success,
            quality_score,
            tool_success_score,
            cost_usd,
            latency_ms,
            judge_evidence,
        })
    }

    /// Compatibility adapter for older callers. This remains intentionally
    /// caller-asserted and must not be used to establish replay eligibility;
    /// new replay paths must call `from_replay_evidence`.
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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub schema_version: String,
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
    pub fn evaluate_trace_evidence(
        evidence: &[ReplayObservationEvidence],
    ) -> Result<OfflineEvaluationReport, OfflineEvaluationError> {
        let observations = evidence
            .iter()
            .map(OfflineReplayObservation::from_replay_evidence)
            .collect::<Result<Vec<_>, _>>()?;
        Self::evaluate(&observations)
    }

    pub fn replay_policies(
        request: &OfflineReplayRequest,
    ) -> Result<OfflineReplayReport, OfflineEvaluationError> {
        let violations = validate_replay_request(request);
        if !violations.is_empty() {
            return Err(OfflineEvaluationError::validation(violations));
        }

        let eligibility = evaluate_replay_eligibility(&request.eligibility);
        let eligibility_hash = replay_eligibility_result_sha256(&eligibility)
            .map_err(|error| OfflineEvaluationError::validation(vec![error.code]))?;
        if eligibility_hash != eligibility.content_sha256 {
            return Ok(blocked_replay_report(
                request,
                &eligibility,
                OfflineReplayStatus::TamperedEvidence,
                vec!["tampered_replay_result".to_string()],
                eligibility_hash,
            ));
        }
        if eligibility.schema_version != POLICY_REPLAY_CONTRACT_SCHEMA_VERSION
            || eligibility.evidence_schema_version != TRACE_REPLAY_EVIDENCE_SCHEMA_VERSION
        {
            return Ok(blocked_replay_report(
                request,
                &eligibility,
                OfflineReplayStatus::TamperedEvidence,
                vec!["invalid_replay_evidence_schema".to_string()],
                eligibility_hash,
            ));
        }

        let accepted = eligibility
            .observations
            .iter()
            .filter(|evidence| evidence.disposition == EvidenceDisposition::Accepted)
            .collect::<Vec<_>>();
        if accepted.iter().any(|evidence| {
            replay_observation_evidence_sha256(evidence)
                .map(|hash| hash != evidence.content_sha256)
                .unwrap_or(true)
        }) {
            return Ok(blocked_replay_report(
                request,
                &eligibility,
                OfflineReplayStatus::TamperedEvidence,
                vec!["tampered_replay_observation".to_string()],
                eligibility_hash,
            ));
        }

        if !eligibility.eligible {
            return Ok(blocked_replay_report(
                request,
                &eligibility,
                status_for_reasons(&eligibility.reason_codes),
                eligibility.reason_codes.clone(),
                eligibility_hash,
            ));
        }

        let Some(cohort) = eligibility.cohort.as_ref() else {
            return Ok(blocked_replay_report(
                request,
                &eligibility,
                OfflineReplayStatus::InsufficientEvidence,
                vec!["missing_replay_cohort".to_string()],
                eligibility_hash,
            ));
        };
        if accepted.iter().any(|evidence| {
            let observation = &evidence.observation;
            let Some(candidate_id) = observation.candidate_id.as_ref() else {
                return true;
            };
            let Some(binding) = cohort.candidate_bindings.get(candidate_id) else {
                return true;
            };
            binding.candidate_version
                != observation.candidate_version.as_deref().unwrap_or_default()
                || binding.candidate_definition_sha256
                    != observation
                        .candidate_definition_sha256
                        .as_deref()
                        .unwrap_or_default()
                || binding.member_endpoint_ids != observation.member_endpoint_ids
        }) {
            return Ok(blocked_replay_report(
                request,
                &eligibility,
                OfflineReplayStatus::IncompatibleCohort,
                vec!["candidate_binding_mismatch".to_string()],
                eligibility_hash,
            ));
        }
        if cohort.policy_version.as_deref() != Some(request.current_policy.policy_version.as_str())
            || cohort.policy_hash.as_deref() != Some(request.current_policy.policy_hash.as_str())
        {
            return Ok(blocked_replay_report(
                request,
                &eligibility,
                OfflineReplayStatus::IncompatibleCohort,
                vec!["current_policy_binding_mismatch".to_string()],
                eligibility_hash,
            ));
        }

        let observed_facts = aggregate_replay_facts(&accepted)?;
        let Some(current_selection) = request.current_policy.selections.get(&cohort.task_class)
        else {
            return Ok(blocked_replay_report(
                request,
                &eligibility,
                OfflineReplayStatus::OutOfDistribution,
                vec!["missing_current_policy_selection".to_string()],
                eligibility_hash,
            ));
        };
        let Some(current_observed) = find_observed_fact(&observed_facts, current_selection) else {
            return Ok(blocked_replay_report(
                request,
                &eligibility,
                OfflineReplayStatus::OutOfDistribution,
                vec!["current_policy_candidate_not_observed".to_string()],
                eligibility_hash,
            ));
        };

        let candidate_policies = ordered_policies(&request.candidate_policies);
        let mut counterfactual_estimates = Vec::new();
        let mut comparisons = Vec::new();
        let mut outcomes = Vec::new();
        for policy in &candidate_policies {
            let Some(selection) = policy.selections.get(&cohort.task_class) else {
                outcomes.push(OfflineReplayOutcome {
                    status: OfflineReplayStatus::OutOfDistribution,
                    policy_id: Some(policy.policy_id.clone()),
                    task_class: Some(cohort.task_class.clone()),
                    reason_codes: vec!["missing_candidate_policy_selection".to_string()],
                });
                continue;
            };
            let Some(source) = find_observed_fact(&observed_facts, selection) else {
                outcomes.push(OfflineReplayOutcome {
                    status: OfflineReplayStatus::OutOfDistribution,
                    policy_id: Some(policy.policy_id.clone()),
                    task_class: Some(cohort.task_class.clone()),
                    reason_codes: vec![
                        "candidate_policy_candidate_not_observed_in_comparable_cohort".to_string(),
                    ],
                });
                continue;
            };
            let estimate = make_counterfactual_estimate(policy, selection, source);
            comparisons.push(make_policy_comparison(
                policy,
                selection,
                current_observed,
                &estimate,
            ));
            counterfactual_estimates.push(estimate);
        }

        let mut report = OfflineReplayReport {
            schema_version: OFFLINE_REPLAY_SCHEMA_VERSION.to_string(),
            status: if outcomes.is_empty() {
                OfflineReplayStatus::Sufficient
            } else {
                overall_status(&outcomes)
            },
            reason_codes: outcomes
                .iter()
                .flat_map(|outcome| outcome.reason_codes.iter().cloned())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            current_policy: request.current_policy.clone(),
            candidate_policies,
            observed_facts,
            counterfactual_estimates,
            comparisons,
            outcomes,
            eligibility_content_sha256: eligibility.content_sha256.clone(),
            replay_judge_calibrations: eligibility.judge_calibrations.clone(),
            source_trace_ids: accepted
                .iter()
                .map(|evidence| evidence.observation.trace_id.clone())
                .collect(),
            source_evidence_content_sha256: accepted
                .iter()
                .map(|evidence| evidence.content_sha256.clone())
                .collect(),
            shadow_only: true,
            influence_selected_tier: false,
            influence_executor_type: false,
            influence_retry_path: false,
            influence_routing_policy: false,
            content_sha256: String::new(),
        };
        finalize_replay_report(&mut report);
        Ok(report)
    }

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
                let canonical = canonical_observation(&observation).ok_or_else(|| {
                    OfflineEvaluationError::validation(vec![
                        "observation_serialization_failed".to_string()
                    ])
                })?;
                Ok((observation, canonical))
            })
            .collect::<Result<Vec<_>, OfflineEvaluationError>>()?;
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

fn validate_replay_request(request: &OfflineReplayRequest) -> Vec<String> {
    let mut violations = Vec::new();
    match canonical_json(request) {
        Some(canonical) if canonical.len() <= MAX_REPLAY_REQUEST_CANONICAL_BYTES => {}
        Some(_) => violations.push("replay_request_size_limit_exceeded".to_string()),
        None => violations.push("replay_request_serialization_failed".to_string()),
    }
    if request.schema_version != OFFLINE_REPLAY_SCHEMA_VERSION {
        violations.push("invalid_offline_replay_schema".to_string());
    }
    validate_policy_definition(&request.current_policy, "current_policy", &mut violations);
    let mut policy_ids = BTreeSet::new();
    policy_ids.insert(request.current_policy.policy_id.clone());
    for policy in &request.candidate_policies {
        validate_policy_definition(policy, "candidate_policy", &mut violations);
        if !policy_ids.insert(policy.policy_id.clone()) {
            violations.push("duplicate_policy_identity".to_string());
        }
    }
    if request.candidate_policies.len() > MAX_POLICY_CANDIDATES {
        violations.push("policy_candidate_limit_exceeded".to_string());
    }
    violations
}

pub(crate) fn validate_offline_replay_report_bounds(
    report: &OfflineReplayReport,
) -> Result<(), String> {
    if report.reason_codes.len() > MAX_REPORT_REASON_CODES
        || report.candidate_policies.len() > MAX_POLICY_CANDIDATES
        || report.observed_facts.len() > MAX_REPORT_FACTS
        || report.counterfactual_estimates.len() > MAX_REPORT_FACTS
        || report.comparisons.len() > MAX_REPORT_COMPARISONS
        || report.outcomes.len() > MAX_REPORT_OUTCOMES
        || report.replay_judge_calibrations.len() > MAX_REPORT_JUDGES
        || report.source_trace_ids.len() > MAX_REPORT_FACTS
        || report.source_evidence_content_sha256.len() > MAX_REPORT_FACTS
        || report.source_trace_ids.iter().any(|value| !valid_id(value))
    {
        return Err("offline_replay_report_cardinality_limit_exceeded".to_string());
    }
    let task_classes = report
        .observed_facts
        .iter()
        .map(|facts| facts.task_class.as_str())
        .collect::<BTreeSet<_>>();
    if task_classes.len() > MAX_REPORT_TASK_CLASSES {
        return Err("offline_replay_report_task_class_bound_exceeded".to_string());
    }
    for policy in std::iter::once(&report.current_policy).chain(report.candidate_policies.iter()) {
        if policy.selections.len() > MAX_POLICY_SELECTIONS_PER_POLICY
            || !valid_id(&policy.policy_id)
            || !valid_id(&policy.policy_version)
        {
            return Err("offline_replay_report_policy_bound_exceeded".to_string());
        }
        for (task_class, selection) in &policy.selections {
            if !valid_id(task_class)
                || !valid_id(&selection.candidate_id)
                || !valid_id(&selection.candidate_version)
            {
                return Err("offline_replay_report_policy_bound_exceeded".to_string());
            }
        }
    }
    for facts in &report.observed_facts {
        if facts.member_endpoint_ids.len() > MAX_MEMBER_ENDPOINTS
            || facts.trace_ids.len() > MAX_REPORT_FACTS
            || facts.evidence_content_sha256.len() > MAX_REPORT_FACTS
            || !valid_id(&facts.task_class)
            || !valid_id(&facts.candidate_id)
            || !valid_id(&facts.candidate_version)
            || facts
                .member_endpoint_ids
                .iter()
                .any(|value| !valid_id(value))
            || facts.trace_ids.iter().any(|value| !valid_id(value))
        {
            return Err("offline_replay_report_fact_bound_exceeded".to_string());
        }
    }
    for estimate in &report.counterfactual_estimates {
        if estimate.source_trace_ids.len() > MAX_REPORT_FACTS
            || estimate.source_evidence_content_sha256.len() > MAX_REPORT_FACTS
            || !valid_id(&estimate.policy_id)
            || !valid_id(&estimate.policy_version)
            || !valid_id(&estimate.task_class)
            || !valid_id(&estimate.source_candidate_id)
            || !valid_id(&estimate.source_candidate_version)
            || !valid_id(&estimate.selection.candidate_id)
            || !valid_id(&estimate.selection.candidate_version)
            || estimate
                .source_trace_ids
                .iter()
                .any(|value| !valid_id(value))
        {
            return Err("offline_replay_report_estimate_bound_exceeded".to_string());
        }
    }
    for outcome in &report.outcomes {
        if outcome.reason_codes.len() > MAX_REPORT_REASON_CODES
            || outcome
                .policy_id
                .as_deref()
                .is_some_and(|value| !valid_id(value))
            || outcome
                .task_class
                .as_deref()
                .is_some_and(|value| !valid_id(value))
        {
            return Err("offline_replay_report_outcome_bound_exceeded".to_string());
        }
    }
    for calibration in &report.replay_judge_calibrations {
        if !valid_id(&calibration.judge_endpoint_id)
            || calibration.sample_count > MAX_REPORT_JUDGE_PAIRS_PER_JUDGE
        {
            return Err("offline_replay_report_calibration_bound_exceeded".to_string());
        }
    }
    Ok(())
}

fn validate_policy_definition(
    policy: &OfflinePolicyDefinition,
    label: &str,
    violations: &mut Vec<String>,
) {
    if policy.schema_version != OFFLINE_REPLAY_SCHEMA_VERSION {
        violations.push(format!("invalid_{label}_schema"));
    }
    if !valid_id(&policy.policy_id) || !valid_id(&policy.policy_version) {
        violations.push(format!("invalid_{label}_identity"));
    }
    if !valid_hash(&policy.policy_hash) {
        violations.push(format!("invalid_{label}_hash"));
    } else if policy.content_sha256().ok().as_deref() != Some(policy.policy_hash.as_str()) {
        violations.push(format!("tampered_{label}"));
    }
    if policy.selections.is_empty() {
        violations.push(format!("missing_{label}_selection"));
    }
    if policy.selections.len() > MAX_POLICY_SELECTIONS_PER_POLICY {
        violations.push(format!("{label}_selection_limit_exceeded"));
    }
    for (task_class, selection) in &policy.selections {
        if !valid_id(task_class)
            || !valid_id(&selection.candidate_id)
            || !valid_id(&selection.candidate_version)
            || !valid_hash(&selection.candidate_definition_sha256)
        {
            violations.push(format!("invalid_{label}_candidate_binding"));
        }
    }
}

fn status_for_reasons(reasons: &[String]) -> OfflineReplayStatus {
    let category = reasons
        .iter()
        .map(|reason| match replay_reason_category(reason) {
            ReplayReasonCategory::Tampered => 6,
            ReplayReasonCategory::Stale => 5,
            ReplayReasonCategory::Calibration => 4,
            ReplayReasonCategory::Cohort => 3,
            ReplayReasonCategory::OutOfDistribution => 2,
            ReplayReasonCategory::Insufficient => 1,
        })
        .max()
        .unwrap_or(1);
    match category {
        6 => OfflineReplayStatus::TamperedEvidence,
        5 => OfflineReplayStatus::StaleEvidence,
        4 => OfflineReplayStatus::UncalibratedEvidence,
        3 => OfflineReplayStatus::IncompatibleCohort,
        2 => OfflineReplayStatus::OutOfDistribution,
        _ => OfflineReplayStatus::InsufficientEvidence,
    }
}

fn blocked_replay_report(
    request: &OfflineReplayRequest,
    eligibility: &ReplayEligibilityResult,
    status: OfflineReplayStatus,
    reason_codes: Vec<String>,
    eligibility_hash: String,
) -> OfflineReplayReport {
    let sorted_reasons = sorted_unique(reason_codes);
    let mut report = OfflineReplayReport {
        schema_version: OFFLINE_REPLAY_SCHEMA_VERSION.to_string(),
        status,
        reason_codes: sorted_reasons.clone(),
        current_policy: request.current_policy.clone(),
        candidate_policies: ordered_policies(&request.candidate_policies),
        observed_facts: Vec::new(),
        counterfactual_estimates: Vec::new(),
        comparisons: Vec::new(),
        outcomes: vec![OfflineReplayOutcome {
            status,
            policy_id: None,
            task_class: None,
            reason_codes: sorted_reasons,
        }],
        eligibility_content_sha256: eligibility.content_sha256.clone(),
        replay_judge_calibrations: eligibility.judge_calibrations.clone(),
        source_trace_ids: eligibility
            .observations
            .iter()
            .map(|evidence| evidence.observation.trace_id.clone())
            .collect(),
        source_evidence_content_sha256: eligibility
            .observations
            .iter()
            .filter(|evidence| valid_hash(&evidence.content_sha256))
            .map(|evidence| evidence.content_sha256.clone())
            .collect(),
        shadow_only: true,
        influence_selected_tier: false,
        influence_executor_type: false,
        influence_retry_path: false,
        influence_routing_policy: false,
        content_sha256: String::new(),
    };
    report.eligibility_content_sha256 = if eligibility_hash.is_empty() {
        report.eligibility_content_sha256.clone()
    } else {
        eligibility_hash
    };
    finalize_replay_report(&mut report);
    report
}

fn aggregate_replay_facts(
    accepted: &[&ReplayObservationEvidence],
) -> Result<Vec<OfflineObservedFacts>, OfflineEvaluationError> {
    let mut groups = BTreeMap::<(String, String, String, String), ReplayFactAccumulator>::new();
    for evidence in accepted {
        let observation = &evidence.observation;
        let Some((task_class, candidate_id, candidate_version, candidate_definition_sha256)) =
            observation
                .task_class
                .clone()
                .zip(observation.candidate_id.clone())
                .zip(observation.candidate_version.clone())
                .zip(observation.candidate_definition_sha256.clone())
                .map(
                    |(
                        ((task_class, candidate_id), candidate_version),
                        candidate_definition_sha256,
                    )| {
                        (
                            task_class,
                            candidate_id,
                            candidate_version,
                            candidate_definition_sha256,
                        )
                    },
                )
        else {
            continue;
        };
        let key = (
            task_class,
            candidate_id,
            candidate_version,
            candidate_definition_sha256,
        );
        groups
            .entry(key)
            .or_insert_with(|| ReplayFactAccumulator::new(observation))
            .add(observation, &evidence.content_sha256);
    }
    if groups
        .values()
        .any(|accumulator| accumulator.token_overflowed)
    {
        return Err(OfflineEvaluationError::validation(vec![
            "token_addition_overflow".to_string(),
        ]));
    }
    Ok(groups
        .into_iter()
        .map(|(key, accumulator)| accumulator.finish(key))
        .collect())
}

#[derive(Debug, Default)]
struct ReplayFactAccumulator {
    member_endpoint_ids: Vec<String>,
    trace_ids: BTreeSet<String>,
    evidence_content_sha256: BTreeSet<String>,
    sample_count: usize,
    success_count: usize,
    quality_total: f64,
    tool_success_total: f64,
    cost_total: f64,
    latency_total: f64,
    total_tokens_total: f64,
    retry_total: f64,
    token_overflowed: bool,
}

impl ReplayFactAccumulator {
    fn new(observation: &super::replay_eligibility::NormalizedReplayObservation) -> Self {
        Self {
            member_endpoint_ids: observation.member_endpoint_ids.clone(),
            ..Self::default()
        }
    }

    fn add(
        &mut self,
        observation: &super::replay_eligibility::NormalizedReplayObservation,
        evidence_content_sha256: &str,
    ) {
        self.trace_ids.insert(observation.trace_id.clone());
        self.evidence_content_sha256
            .insert(evidence_content_sha256.to_string());
        self.sample_count += 1;
        self.success_count += usize::from(observation.success.unwrap_or(false));
        self.quality_total += observation.quality_score.unwrap_or_default();
        self.tool_success_total += observation.tool_success_score.unwrap_or_default();
        self.cost_total += observation.cost_usd.unwrap_or_default();
        self.latency_total += observation.latency_ms.unwrap_or_default() as f64;
        match observation
            .input_tokens
            .unwrap_or_default()
            .checked_add(observation.output_tokens.unwrap_or_default())
        {
            Some(total) => self.total_tokens_total += total as f64,
            None => self.token_overflowed = true,
        }
        self.retry_total += observation.retry_count.unwrap_or_default() as f64;
    }

    fn finish(
        self,
        (task_class, candidate_id, candidate_version, candidate_definition_sha256): (
            String,
            String,
            String,
            String,
        ),
    ) -> OfflineObservedFacts {
        let denominator = self.sample_count as f64;
        OfflineObservedFacts {
            task_class,
            candidate_id,
            candidate_version,
            candidate_definition_sha256,
            member_endpoint_ids: self.member_endpoint_ids,
            trace_ids: self.trace_ids.into_iter().collect(),
            evidence_content_sha256: self.evidence_content_sha256.into_iter().collect(),
            sample_count: self.sample_count,
            success_rate: self.success_count as f64 / denominator,
            average_quality_score: self.quality_total / denominator,
            average_tool_success_score: self.tool_success_total / denominator,
            average_cost_usd: self.cost_total / denominator,
            average_latency_ms: self.latency_total / denominator,
            average_total_tokens: self.total_tokens_total / denominator,
            average_retry_count: self.retry_total / denominator,
        }
    }
}

fn find_observed_fact<'a>(
    facts: &'a [OfflineObservedFacts],
    selection: &OfflinePolicySelection,
) -> Option<&'a OfflineObservedFacts> {
    facts.iter().find(|fact| {
        fact.candidate_id == selection.candidate_id
            && fact.candidate_version == selection.candidate_version
            && fact.candidate_definition_sha256 == selection.candidate_definition_sha256
    })
}

fn make_counterfactual_estimate(
    policy: &OfflinePolicyDefinition,
    selection: &OfflinePolicySelection,
    source: &OfflineObservedFacts,
) -> OfflineCounterfactualEstimate {
    OfflineCounterfactualEstimate {
        policy_id: policy.policy_id.clone(),
        policy_version: policy.policy_version.clone(),
        policy_hash: policy.policy_hash.clone(),
        task_class: source.task_class.clone(),
        selection: selection.clone(),
        source_candidate_id: source.candidate_id.clone(),
        source_candidate_version: source.candidate_version.clone(),
        source_candidate_definition_sha256: source.candidate_definition_sha256.clone(),
        source_trace_ids: source.trace_ids.clone(),
        source_evidence_content_sha256: source.evidence_content_sha256.clone(),
        sample_count: source.sample_count,
        estimated_success_rate: source.success_rate,
        estimated_quality_score: source.average_quality_score,
        estimated_tool_success_score: source.average_tool_success_score,
        estimated_cost_usd: source.average_cost_usd,
        estimated_latency_ms: source.average_latency_ms,
        estimated_total_tokens: source.average_total_tokens,
        estimated_retry_count: source.average_retry_count,
        estimation_method: "observed_comparable_candidate_cohort".to_string(),
    }
}

fn make_policy_comparison(
    policy: &OfflinePolicyDefinition,
    selection: &OfflinePolicySelection,
    current_observed: &OfflineObservedFacts,
    counterfactual: &OfflineCounterfactualEstimate,
) -> OfflinePolicyComparison {
    OfflinePolicyComparison {
        policy_id: policy.policy_id.clone(),
        policy_version: policy.policy_version.clone(),
        policy_hash: policy.policy_hash.clone(),
        task_class: current_observed.task_class.clone(),
        current_observed_candidate_id: current_observed.candidate_id.clone(),
        candidate_selection: selection.clone(),
        current_observed: current_observed.clone(),
        counterfactual: counterfactual.clone(),
        success_rate_delta: counterfactual.estimated_success_rate - current_observed.success_rate,
        quality_score_delta: counterfactual.estimated_quality_score
            - current_observed.average_quality_score,
        tool_success_score_delta: counterfactual.estimated_tool_success_score
            - current_observed.average_tool_success_score,
        cost_usd_delta: counterfactual.estimated_cost_usd - current_observed.average_cost_usd,
        latency_ms_delta: counterfactual.estimated_latency_ms - current_observed.average_latency_ms,
        total_tokens_delta: counterfactual.estimated_total_tokens
            - current_observed.average_total_tokens,
        retry_count_delta: counterfactual.estimated_retry_count
            - current_observed.average_retry_count,
    }
}

fn ordered_policies(policies: &[OfflinePolicyDefinition]) -> Vec<OfflinePolicyDefinition> {
    let mut ordered = policies.to_vec();
    ordered.sort_by(|left, right| left.policy_id.cmp(&right.policy_id));
    ordered
}

fn overall_status(outcomes: &[OfflineReplayOutcome]) -> OfflineReplayStatus {
    outcomes
        .iter()
        .map(|outcome| outcome.status)
        .max_by_key(|status| status_rank(*status))
        .unwrap_or(OfflineReplayStatus::Sufficient)
}

fn status_rank(status: OfflineReplayStatus) -> u8 {
    match status {
        OfflineReplayStatus::Sufficient => 0,
        OfflineReplayStatus::InsufficientEvidence => 1,
        OfflineReplayStatus::OutOfDistribution => 2,
        OfflineReplayStatus::UncalibratedEvidence => 3,
        OfflineReplayStatus::IncompatibleCohort => 4,
        OfflineReplayStatus::StaleEvidence => 5,
        OfflineReplayStatus::TamperedEvidence => 6,
    }
}

fn sorted_unique(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}

fn finalize_replay_report(report: &mut OfflineReplayReport) {
    report.reason_codes = sorted_unique(std::mem::take(&mut report.reason_codes));
    report.source_trace_ids = sorted_unique(std::mem::take(&mut report.source_trace_ids));
    report.source_evidence_content_sha256 =
        sorted_unique(std::mem::take(&mut report.source_evidence_content_sha256));
    report.candidate_policies = ordered_policies(&report.candidate_policies);
    let bounds_valid = match validate_offline_replay_report_bounds(report) {
        Ok(()) => true,
        Err(reason) => {
            report.reason_codes.push(reason);
            false
        }
    };
    report.outcomes.sort_by(|left, right| {
        left.policy_id
            .cmp(&right.policy_id)
            .then_with(|| left.task_class.cmp(&right.task_class))
            .then_with(|| status_rank(left.status).cmp(&status_rank(right.status)))
    });
    report.content_sha256.clear();
    match canonical_json(report) {
        Some(canonical) if canonical.len() <= MAX_REPLAY_RESULT_CANONICAL_BYTES => {
            if bounds_valid {
                report.content_sha256 = hex::encode(Sha256::digest(canonical.as_bytes()));
            }
        }
        Some(_) => report
            .reason_codes
            .push("result_size_limit_exceeded".to_string()),
        None => report
            .reason_codes
            .push("serialization_failure".to_string()),
    }
    report.reason_codes.sort();
    report.reason_codes.dedup();
}

fn canonical_sha256<T: Serialize>(value: &T) -> Option<String> {
    Some(hex::encode(Sha256::digest(
        canonical_json(value)?.as_bytes(),
    )))
}

pub fn offline_replay_report_sha256(
    report: &OfflineReplayReport,
) -> Result<String, OfflineEvaluationError> {
    let mut unsigned = report.clone();
    unsigned.content_sha256.clear();
    canonical_sha256(&unsigned).ok_or_else(|| {
        OfflineEvaluationError::validation(vec![
            "offline_replay_report_serialization_failed".to_string()
        ])
    })
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
                schema_version: super::replay_eligibility::JUDGE_CALIBRATION_SCHEMA_VERSION
                    .to_string(),
                judge_endpoint_id,
                sample_count,
                mean_signed_bias,
                mean_absolute_error,
                recommended_score_offset: -mean_signed_bias,
                status: if sample_count < MIN_JUDGE_CALIBRATION_SAMPLES {
                    "insufficient_data"
                } else if mean_signed_bias.abs()
                    <= super::replay_eligibility::MAX_JUDGE_ABSOLUTE_BIAS + f64::EPSILON
                    && mean_absolute_error
                        <= super::replay_eligibility::MAX_JUDGE_MEAN_ABSOLUTE_ERROR + f64::EPSILON
                {
                    "within_tolerance"
                } else {
                    "outside_tolerance"
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
    if observation.member_endpoint_ids.len() > MAX_MEMBER_ENDPOINTS {
        violations.push("member_endpoint_limit_exceeded".to_string());
    }
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
    match canonical_observation(observation) {
        Some(value) if value.len() > MAX_OBSERVATION_CANONICAL_BYTES => {
            violations.push("observation_size_limit_exceeded".to_string())
        }
        Some(value) if contains_sensitive_patterns(&value) => {
            violations.push("sensitive_pattern_detected".to_string())
        }
        Some(_) => {}
        None => violations.push("observation_serialization_failed".to_string()),
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

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|character| character.is_ascii_hexdigit())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn oversized_policy() -> OfflinePolicyDefinition {
        OfflinePolicyDefinition {
            schema_version: OFFLINE_REPLAY_SCHEMA_VERSION.to_string(),
            policy_id: "policy".to_string(),
            policy_version: "v1".to_string(),
            policy_hash: String::new(),
            selections: BTreeMap::from([(
                "task".to_string(),
                OfflinePolicySelection {
                    candidate_id: "candidate".to_string(),
                    candidate_version: "v1".to_string(),
                    candidate_definition_sha256: "a".repeat(64),
                },
            )]),
        }
    }

    #[test]
    fn replay_request_envelope_is_bounded_before_owner_evaluation() {
        let mut policy = oversized_policy();
        policy.policy_id = "x".repeat(MAX_REPLAY_REQUEST_CANONICAL_BYTES + 1);
        let request = OfflineReplayRequest {
            schema_version: OFFLINE_REPLAY_SCHEMA_VERSION.to_string(),
            eligibility: ReplayEligibilityRequest {
                schema_version: POLICY_REPLAY_CONTRACT_SCHEMA_VERSION.to_string(),
                generated_at: "2026-07-12T00:00:00Z".to_string(),
                maximum_trace_age_seconds: 300,
                scope: Default::default(),
                traces: Vec::new(),
            },
            current_policy: policy,
            candidate_policies: Vec::new(),
        };

        assert!(validate_replay_request(&request)
            .iter()
            .any(|violation| violation == "replay_request_size_limit_exceeded"));

        let mut too_many_selections = oversized_policy();
        too_many_selections.selections = (0..=MAX_POLICY_SELECTIONS_PER_POLICY)
            .map(|index| {
                (
                    format!("task-{index}"),
                    OfflinePolicySelection {
                        candidate_id: "candidate".to_string(),
                        candidate_version: "v1".to_string(),
                        candidate_definition_sha256: "a".repeat(64),
                    },
                )
            })
            .collect();
        let request = OfflineReplayRequest {
            current_policy: too_many_selections,
            ..request
        };
        assert!(validate_replay_request(&request)
            .iter()
            .any(|violation| violation == "current_policy_selection_limit_exceeded"));
    }

    #[test]
    fn replay_result_size_failure_is_explicit_and_non_authorizing() {
        let mut policy = oversized_policy();
        policy.policy_id = "x".repeat(MAX_REPLAY_RESULT_CANONICAL_BYTES + 1);
        let mut report = OfflineReplayReport {
            schema_version: OFFLINE_REPLAY_SCHEMA_VERSION.to_string(),
            status: OfflineReplayStatus::Sufficient,
            reason_codes: Vec::new(),
            current_policy: policy,
            candidate_policies: Vec::new(),
            observed_facts: Vec::new(),
            counterfactual_estimates: Vec::new(),
            comparisons: Vec::new(),
            outcomes: Vec::new(),
            eligibility_content_sha256: String::new(),
            replay_judge_calibrations: Vec::new(),
            source_trace_ids: Vec::new(),
            source_evidence_content_sha256: Vec::new(),
            shadow_only: true,
            influence_selected_tier: false,
            influence_executor_type: false,
            influence_retry_path: false,
            influence_routing_policy: false,
            content_sha256: String::new(),
        };

        finalize_replay_report(&mut report);

        assert!(report.content_sha256.is_empty());
        assert!(report
            .reason_codes
            .contains(&"result_size_limit_exceeded".to_string()));
    }

    #[test]
    fn replay_report_cardinality_is_bounded_before_downstream_use() {
        let mut report = OfflineReplayReport {
            schema_version: OFFLINE_REPLAY_SCHEMA_VERSION.to_string(),
            status: OfflineReplayStatus::InsufficientEvidence,
            reason_codes: Vec::new(),
            current_policy: oversized_policy(),
            candidate_policies: Vec::new(),
            observed_facts: Vec::new(),
            counterfactual_estimates: Vec::new(),
            comparisons: Vec::new(),
            outcomes: Vec::new(),
            eligibility_content_sha256: String::new(),
            replay_judge_calibrations: Vec::new(),
            source_trace_ids: (0..=MAX_REPORT_FACTS)
                .map(|index| format!("trace-{index}"))
                .collect(),
            source_evidence_content_sha256: Vec::new(),
            shadow_only: true,
            influence_selected_tier: false,
            influence_executor_type: false,
            influence_retry_path: false,
            influence_routing_policy: false,
            content_sha256: String::new(),
        };

        assert_eq!(
            validate_offline_replay_report_bounds(&report),
            Err("offline_replay_report_cardinality_limit_exceeded".to_string())
        );
        finalize_replay_report(&mut report);
        assert!(report
            .reason_codes
            .contains(&"offline_replay_report_cardinality_limit_exceeded".to_string()));
        assert!(report.content_sha256.is_empty());
    }
}
