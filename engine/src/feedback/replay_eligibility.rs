//! Trace-grounded, non-authoritative replay evidence.
//!
//! This module deliberately treats `RunTrace` and its persisted sections as the
//! source of truth.  It does not accept caller booleans for completeness or
//! coverage, caller candidate definitions, or caller calibration claims.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use super::outcome_attributor::OutcomeAttributor;
use super::run_trace_recorder::{RunTrace, RUN_TRACE_SCHEMA_VERSION};
use crate::provider::redaction::contains_sensitive_patterns;

pub const POLICY_REPLAY_CONTRACT_SCHEMA_VERSION: &str = "policy_replay_contract.v2";
pub const TRACE_REPLAY_EVIDENCE_SCHEMA_VERSION: &str = "trace_replay_evidence.v1";
pub const MIN_CANDIDATE_OBSERVATIONS: usize = 30;
pub const MIN_JUDGE_CALIBRATION_SAMPLES: usize = 3;

const MAX_TRACES: usize = 10_000;
const MAX_EVIDENCE_REFERENCES: usize = 4;
const MAX_ID_BYTES: usize = 160;
const MAX_TRACE_AGE_SECONDS: u64 = 30 * 24 * 60 * 60;
const MAX_COST_USD: f64 = 1_000_000.0;
const MAX_LATENCY_MS: u64 = 86_400_000;
const MAX_TOTAL_TOKENS: u64 = 10_000_000;
const MAX_RETRY_COUNT: u64 = 1_024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceDisposition {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CostEvidenceKind {
    Measured,
    Posted,
    Estimated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JudgeReferenceEvidence {
    pub judge_endpoint_id: String,
    pub judge_score: f64,
    pub reference_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayEvidenceReference {
    pub source_kind: String,
    pub source_id: String,
    pub schema_version: String,
    pub content_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NormalizedReplayObservation {
    pub schema_version: String,
    pub observation_id: String,
    pub trace_id: String,
    pub dispatch_id: String,
    pub observed_at: Option<String>,
    pub task_class: Option<String>,
    pub task_domain: Option<String>,
    pub task_intent: Option<String>,
    pub objective: Option<String>,
    pub candidate_id: Option<String>,
    pub candidate_version: Option<String>,
    pub candidate_definition_sha256: Option<String>,
    pub member_endpoint_ids: Vec<String>,
    pub routing_policy: Option<String>,
    pub policy_version: Option<String>,
    pub policy_hash: Option<String>,
    pub measurement_schema_version: Option<String>,
    pub complexity_score: Option<f64>,
    pub complexity_bucket: Option<String>,
    pub execution_status: Option<String>,
    pub terminal_status: Option<String>,
    pub evaluation_status: Option<String>,
    pub success: Option<bool>,
    pub latency_ms: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cost_usd: Option<f64>,
    pub cost_kind: Option<CostEvidenceKind>,
    pub retry_count: Option<u64>,
    pub quality_score: Option<f64>,
    pub tool_success_score: Option<f64>,
    pub quality_score_source: Option<String>,
    pub judge_reference: Option<JudgeReferenceEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayObservationEvidence {
    pub schema_version: String,
    pub observation: NormalizedReplayObservation,
    pub disposition: EvidenceDisposition,
    pub reason_codes: Vec<String>,
    pub evidence_references: Vec<ReplayEvidenceReference>,
    pub content_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ReplayEvidenceScope {
    pub max_cost_usd: Option<f64>,
    pub max_latency_ms: Option<u64>,
    pub max_total_tokens: Option<u64>,
    pub max_retry_count: Option<u64>,
    pub allowed_complexity_buckets: Option<BTreeSet<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayTraceInput {
    pub trace: RunTrace,
    /// A source-owner hash is verified against the canonical trace content. It
    /// is never used to establish candidate, coverage, or calibration facts.
    pub source_content_sha256: String,
}

impl ReplayTraceInput {
    pub fn from_trace(trace: RunTrace) -> Result<Self, ReplayEvidenceError> {
        Ok(Self {
            source_content_sha256: trace_content_sha256(&trace)?,
            trace,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayEligibilityRequest {
    pub schema_version: String,
    pub generated_at: String,
    pub maximum_trace_age_seconds: u64,
    #[serde(default)]
    pub scope: ReplayEvidenceScope,
    pub traces: Vec<ReplayTraceInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayCoverage {
    pub submitted_observation_count: usize,
    pub accepted_observation_count: usize,
    pub rejected_observation_count: usize,
    pub accepted_ratio: f64,
    pub candidate_counts: BTreeMap<String, usize>,
    pub candidate_complexity_bucket_counts: BTreeMap<String, BTreeMap<String, usize>>,
    pub paired_complexity_bucket_counts: BTreeMap<String, usize>,
    pub judge_pair_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayCandidateBinding {
    pub candidate_id: String,
    pub candidate_version: String,
    pub candidate_definition_sha256: String,
    pub member_endpoint_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayMetricEnvelope {
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayEnvelope {
    pub cost_usd: ReplayMetricEnvelope,
    pub latency_ms: ReplayMetricEnvelope,
    pub total_tokens: ReplayMetricEnvelope,
    pub retry_count: ReplayMetricEnvelope,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayCohort {
    pub task_class: String,
    pub task_domain: String,
    pub task_intent: String,
    pub objective: String,
    pub measurement_schema_version: String,
    pub routing_policy: Option<String>,
    pub policy_version: Option<String>,
    pub policy_hash: Option<String>,
    pub candidate_bindings: BTreeMap<String, ReplayCandidateBinding>,
    pub complexity_buckets: Vec<String>,
    pub envelope: ReplayEnvelope,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JudgeCalibrationEvidence {
    pub judge_endpoint_id: String,
    pub sample_count: usize,
    pub mean_signed_bias: f64,
    pub mean_absolute_error: f64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayEligibilityResult {
    pub schema_version: String,
    pub evidence_schema_version: String,
    pub eligible: bool,
    pub reason_codes: Vec<String>,
    pub observations: Vec<ReplayObservationEvidence>,
    pub accepted_trace_ids: Vec<String>,
    pub rejected_trace_ids: Vec<String>,
    pub coverage: ReplayCoverage,
    pub cohort: Option<ReplayCohort>,
    pub judge_calibrations: Vec<JudgeCalibrationEvidence>,
    pub content_sha256: String,
    pub shadow_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayEvidenceError {
    pub code: String,
}

impl std::fmt::Display for ReplayEvidenceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.code)
    }
}

impl std::error::Error for ReplayEvidenceError {}

/// Evaluate only evidence derived from real persisted traces.
pub fn evaluate_replay_eligibility(request: &ReplayEligibilityRequest) -> ReplayEligibilityResult {
    let mut result_reasons = BTreeSet::new();
    let generated_at = parse_time(&request.generated_at);

    if request.schema_version != POLICY_REPLAY_CONTRACT_SCHEMA_VERSION {
        result_reasons.insert("unsupported_contract_schema".to_string());
    }
    if generated_at.is_none() {
        result_reasons.insert("malformed_generated_at".to_string());
    }
    if request.maximum_trace_age_seconds == 0
        || request.maximum_trace_age_seconds > MAX_TRACE_AGE_SECONDS
    {
        result_reasons.insert("invalid_maximum_trace_age".to_string());
    }
    validate_scope(&request.scope, &mut result_reasons);
    if request.traces.len() > MAX_TRACES {
        result_reasons.insert("observation_limit_exceeded".to_string());
    }

    let mut inputs = request.traces.clone();
    inputs.sort_by(|left, right| {
        left.trace
            .trace_id
            .cmp(&right.trace.trace_id)
            .then_with(|| left.source_content_sha256.cmp(&right.source_content_sha256))
            .then_with(|| canonical_json(&left.trace).cmp(&canonical_json(&right.trace)))
    });
    inputs.truncate(MAX_TRACES);

    let mut observations = inputs
        .into_iter()
        .map(|input| build_observation(input, generated_at, request, &mut result_reasons))
        .collect::<Vec<_>>();

    let mut seen_trace_ids = BTreeSet::new();
    for evidence in &mut observations {
        if !seen_trace_ids.insert(evidence.observation.trace_id.clone()) {
            evidence
                .reason_codes
                .push("duplicate_trace_identity".to_string());
            finalize_observation(evidence);
        }
    }

    let mut candidate_bindings = BTreeMap::<String, ReplayCandidateBinding>::new();
    for evidence in &mut observations {
        if evidence.disposition != EvidenceDisposition::Accepted {
            continue;
        }
        let Some(binding) = candidate_binding(&evidence.observation) else {
            continue;
        };
        if candidate_bindings
            .get(&binding.candidate_id)
            .is_some_and(|existing| existing != &binding)
        {
            evidence
                .reason_codes
                .push("inconsistent_candidate_definition".to_string());
            finalize_observation(evidence);
            continue;
        }
        candidate_bindings.insert(binding.candidate_id.clone(), binding);
    }

    let accepted = observations
        .iter()
        .filter(|evidence| evidence.disposition == EvidenceDisposition::Accepted)
        .collect::<Vec<_>>();
    let coverage = build_coverage(&observations, &accepted);
    let cohort_keys = accepted
        .iter()
        .filter_map(|evidence| cohort_key(&evidence.observation))
        .collect::<BTreeSet<_>>();
    if cohort_keys.len() > 1 {
        result_reasons.insert("incompatible_cohort".to_string());
    }

    if accepted.is_empty() || coverage.accepted_ratio < 0.9 {
        result_reasons.insert("insufficient_trace_coverage".to_string());
    }
    if coverage.candidate_counts.len() < 2 {
        result_reasons.insert("insufficient_candidate_coverage".to_string());
    }
    if coverage
        .candidate_counts
        .values()
        .any(|count| *count < MIN_CANDIDATE_OBSERVATIONS)
    {
        result_reasons.insert("sparse_candidate_coverage".to_string());
    }
    if coverage
        .paired_complexity_bucket_counts
        .values()
        .any(|count| *count < coverage.candidate_counts.len())
    {
        result_reasons.insert("incomplete_paired_comparison_coverage".to_string());
    }

    let judge_calibrations = calibrate_judges(&accepted, &mut result_reasons);
    let cohort = if cohort_keys.len() == 1 && !accepted.is_empty() {
        Some(build_cohort(&accepted, candidate_bindings))
    } else {
        None
    };

    let accepted_trace_ids = observations
        .iter()
        .filter(|evidence| evidence.disposition == EvidenceDisposition::Accepted)
        .map(|evidence| evidence.observation.trace_id.clone())
        .collect::<Vec<_>>();
    let rejected_trace_ids = observations
        .iter()
        .filter(|evidence| evidence.disposition == EvidenceDisposition::Rejected)
        .map(|evidence| evidence.observation.trace_id.clone())
        .collect::<Vec<_>>();
    for evidence in &observations {
        result_reasons.extend(evidence.reason_codes.iter().cloned());
    }
    let reason_codes = result_reasons.into_iter().collect::<Vec<_>>();
    let mut result = ReplayEligibilityResult {
        schema_version: POLICY_REPLAY_CONTRACT_SCHEMA_VERSION.to_string(),
        evidence_schema_version: TRACE_REPLAY_EVIDENCE_SCHEMA_VERSION.to_string(),
        eligible: reason_codes.is_empty(),
        reason_codes,
        observations,
        accepted_trace_ids,
        rejected_trace_ids,
        coverage,
        cohort,
        judge_calibrations,
        content_sha256: String::new(),
        shadow_only: true,
    };
    result.content_sha256 = hash_result(&result);
    result
}

pub fn trace_content_sha256(trace: &RunTrace) -> Result<String, ReplayEvidenceError> {
    let canonical = canonical_json(trace).ok_or_else(|| ReplayEvidenceError {
        code: "trace_serialization_failed".to_string(),
    })?;
    Ok(sha256_hex(canonical.as_bytes()))
}

pub fn replay_observation_evidence_sha256(
    evidence: &ReplayObservationEvidence,
) -> Result<String, ReplayEvidenceError> {
    let mut copy = evidence.clone();
    copy.content_sha256.clear();
    hash_value(&copy).ok_or_else(|| ReplayEvidenceError {
        code: "replay_evidence_serialization_failed".to_string(),
    })
}

pub fn replay_eligibility_result_sha256(
    result: &ReplayEligibilityResult,
) -> Result<String, ReplayEvidenceError> {
    let mut copy = result.clone();
    copy.content_sha256.clear();
    hash_value(&copy).ok_or_else(|| ReplayEvidenceError {
        code: "replay_result_serialization_failed".to_string(),
    })
}

fn build_observation(
    input: ReplayTraceInput,
    generated_at: Option<DateTime<FixedOffset>>,
    request: &ReplayEligibilityRequest,
    result_reasons: &mut BTreeSet<String>,
) -> ReplayObservationEvidence {
    let trace = input.trace;
    let trace_hash = trace_content_sha256(&trace).ok();
    let mut reasons = BTreeSet::new();
    if trace.schema_version != RUN_TRACE_SCHEMA_VERSION {
        reasons.insert("incompatible_trace_schema".to_string());
    }
    if !valid_hash(&input.source_content_sha256) {
        reasons.insert("malformed_source_hash".to_string());
    }
    if trace_hash.as_deref() != Some(input.source_content_sha256.as_str()) {
        reasons.insert("tampered_evidence".to_string());
    }
    if trace_hash.is_none() {
        reasons.insert("malformed_evidence".to_string());
    }
    if canonical_json(&trace).is_some_and(|canonical| contains_sensitive_patterns(&canonical)) {
        reasons.insert("sensitive_pattern_detected".to_string());
    }

    let observation = normalize_trace(&trace, &mut reasons);
    if let (Some(now), Some(observed_at)) = (generated_at, observation.observed_at.as_deref()) {
        if let Some(observed_at) = parse_time(observed_at) {
            let age = now.signed_duration_since(observed_at);
            if age.num_seconds() < 0 {
                reasons.insert("future_trace".to_string());
            } else if u64::try_from(age.num_seconds()).unwrap_or(u64::MAX)
                > request.maximum_trace_age_seconds
            {
                reasons.insert("stale_trace".to_string());
            }
        }
    }
    if generated_at.is_some() && observation.observed_at.is_none() {
        reasons.insert("missing_observation_time".to_string());
    }
    apply_scope(&observation, &request.scope, &mut reasons);

    let mut evidence = ReplayObservationEvidence {
        schema_version: TRACE_REPLAY_EVIDENCE_SCHEMA_VERSION.to_string(),
        observation,
        disposition: if reasons.is_empty() {
            EvidenceDisposition::Accepted
        } else {
            EvidenceDisposition::Rejected
        },
        reason_codes: reasons.into_iter().collect(),
        evidence_references: build_references(&trace, trace_hash.as_deref()),
        content_sha256: String::new(),
    };
    if !evidence.reason_codes.is_empty() {
        result_reasons.extend(evidence.reason_codes.iter().cloned());
    }
    finalize_observation(&mut evidence);
    evidence
}

fn normalize_trace(
    trace: &RunTrace,
    reasons: &mut BTreeSet<String>,
) -> NormalizedReplayObservation {
    let candidate_id = first_str_values(
        &[&trace.decision, &trace.analysis],
        &[&["candidate_id"], &["selected_tier"]],
    )
    .filter(|value| !value.trim().is_empty() && *value != "unknown")
    .map(str::to_owned);
    let candidate_version = first_str_values(
        &[&trace.decision, &trace.analysis],
        &[&["candidate_version"], &["candidate", "version"]],
    )
    .map(str::to_owned);
    let candidate_definition_sha256 = first_str_values(
        &[&trace.decision, &trace.analysis],
        &[
            &["candidate_definition_sha256"],
            &["candidate_hash"],
            &["candidate", "definition_sha256"],
            &["candidate", "hash"],
        ],
    )
    .map(str::to_owned);
    let member_endpoint_ids = first_string_array(
        &[&trace.decision, &trace.analysis],
        &[
            &["member_endpoint_ids"],
            &["endpoint_ids"],
            &["panel_endpoint_ids"],
        ],
    )
    .into_iter()
    .chain(
        first_str_values(
            &[&trace.decision, &trace.analysis],
            &[&["endpoint_id"], &["selected_endpoint_id"]],
        )
        .into_iter()
        .map(str::to_owned),
    )
    .collect::<BTreeSet<_>>()
    .into_iter()
    .collect::<Vec<_>>();
    let routing_policy = trace.routing_policy.clone().or_else(|| {
        first_str_values(&[&trace.decision, &trace.analysis], &[&["routing_policy"]])
            .map(str::to_owned)
    });
    let policy_version = first_str_values(
        &[&trace.decision, &trace.analysis],
        &[&["policy_version"], &["routing_policy_version"]],
    )
    .map(str::to_owned);
    let policy_hash = first_str_values(
        &[&trace.decision, &trace.analysis],
        &[&["policy_hash"], &["routing_policy_hash"]],
    )
    .map(str::to_owned);
    let objective = first_str_values(
        &[&trace.analysis, &trace.decision, &trace.evaluation],
        &[&["objective"], &["objective_profile"]],
    )
    .map(str::to_owned);
    let measurement_schema_version = first_str_values(
        &[&trace.evaluation, &trace.decision],
        &[
            &["measurement_schema_version"],
            &["quality_measurement_schema_version"],
        ],
    )
    .map(str::to_owned);
    let quality_score = first_f64_values(&[&trace.evaluation], &[&["quality_score"]]);
    let tool_success_score = first_f64_values(&[&trace.evaluation], &[&["tool_success_score"]]);
    let quality_score_source = first_str_values(
        &[&trace.evaluation],
        &[&["quality_score_source"], &["quality_source"]],
    )
    .map(str::to_owned);
    let complexity_bucket = first_str_values(
        &[&trace.analysis, &trace.decision],
        &[&["complexity_bucket"]],
    )
    .map(str::to_owned)
    .or_else(|| {
        trace
            .complexity_score
            .and_then(complexity_bucket_from_score)
    });
    let execution_status = non_empty_lowercase(trace.execution_status.as_deref());
    let terminal_status = non_empty_lowercase(Some(&trace.final_status));
    let evaluation_status = non_empty_lowercase(Some(&trace.evaluation_status));
    let (cost_usd, cost_kind) = cost_evidence(trace);
    let retry_count = first_i64_values(&[&trace.execution, &trace.decision], &[&["retry_count"]])
        .and_then(|value| u64::try_from(value).ok());
    let judge_reference = judge_reference(trace, reasons);

    if !valid_id(trace.trace_id.as_str()) || !valid_id(trace.dispatch_id.as_str()) {
        reasons.insert("malformed_trace_identity".to_string());
    }
    if parse_time_opt(trace.created_at.as_deref()).is_none() {
        reasons.insert("malformed_observation_time".to_string());
    }
    if trace.task_class.trim().is_empty() || trace.task_class == "unknown" {
        reasons.insert("missing_task_class".to_string());
    }
    if trace.task_domain.as_deref().is_none_or(str::is_empty) {
        reasons.insert("missing_task_domain".to_string());
    }
    if trace.task_intent.as_deref().is_none_or(str::is_empty) {
        reasons.insert("missing_task_intent".to_string());
    }
    if objective.as_deref().is_none_or(str::is_empty) {
        reasons.insert("missing_objective".to_string());
    }
    if candidate_id.as_deref().is_none_or(str::is_empty) {
        reasons.insert("missing_candidate_identity".to_string());
    }
    if candidate_version.as_deref().is_none_or(str::is_empty) {
        reasons.insert("missing_candidate_version".to_string());
    }
    match candidate_definition_sha256.as_deref() {
        Some(value) if valid_hash(value) => {}
        Some(_) => {
            reasons.insert("malformed_candidate_definition".to_string());
        }
        None => {
            reasons.insert("missing_candidate_definition".to_string());
        }
    }
    if member_endpoint_ids.iter().any(|member| !valid_id(member)) {
        reasons.insert("malformed_endpoint_member_set".to_string());
    }
    if measurement_schema_version
        .as_deref()
        .is_none_or(str::is_empty)
    {
        reasons.insert("missing_measurement_schema".to_string());
    }
    if let Some(policy) = routing_policy.as_deref() {
        if policy.trim().is_empty() {
            reasons.insert("malformed_routing_policy".to_string());
        } else if policy_version.is_none() && policy_hash.is_none() {
            reasons.insert("missing_policy_version_binding".to_string());
        }
    }
    if complexity_bucket.is_none() {
        reasons.insert("missing_complexity_bucket".to_string());
    } else if complexity_bucket
        .as_deref()
        .is_some_and(|bucket| !["low", "medium", "high"].contains(&bucket))
    {
        reasons.insert("malformed_complexity_bucket".to_string());
    }
    if candidate_version
        .as_deref()
        .is_some_and(|version| !valid_id(version))
    {
        reasons.insert("malformed_candidate_version".to_string());
    }
    if policy_hash.as_deref().is_some_and(|hash| !valid_hash(hash)) {
        reasons.insert("malformed_policy_hash".to_string());
    }

    validate_terminal_status(
        execution_status.as_deref(),
        terminal_status.as_deref(),
        evaluation_status.as_deref(),
        trace.success,
        reasons,
    );
    validate_measurements(
        trace,
        quality_score,
        cost_usd,
        cost_kind.as_ref(),
        retry_count,
        tool_success_score,
        reasons,
    );
    validate_trace_consistency(trace, reasons);

    NormalizedReplayObservation {
        schema_version: TRACE_REPLAY_EVIDENCE_SCHEMA_VERSION.to_string(),
        observation_id: trace.trace_id.clone(),
        trace_id: trace.trace_id.clone(),
        dispatch_id: trace.dispatch_id.clone(),
        observed_at: trace.created_at.clone(),
        task_class: non_empty_string(Some(&trace.task_class)),
        task_domain: non_empty_string(trace.task_domain.as_ref()),
        task_intent: non_empty_string(trace.task_intent.as_ref()),
        objective,
        candidate_id,
        candidate_version,
        candidate_definition_sha256,
        member_endpoint_ids,
        routing_policy,
        policy_version,
        policy_hash,
        measurement_schema_version,
        complexity_score: trace.complexity_score.filter(|value| value.is_finite()),
        complexity_bucket,
        execution_status,
        terminal_status,
        evaluation_status,
        success: Some(trace.success),
        latency_ms: non_negative_u64(trace.latency_ms, "latency_ms", reasons),
        input_tokens: non_negative_u64(trace.input_tokens, "input_tokens", reasons),
        output_tokens: non_negative_u64(trace.output_tokens, "output_tokens", reasons),
        cost_usd,
        cost_kind,
        retry_count,
        quality_score,
        tool_success_score,
        quality_score_source,
        judge_reference,
    }
}

fn validate_terminal_status(
    execution_status: Option<&str>,
    terminal_status: Option<&str>,
    evaluation_status: Option<&str>,
    success: bool,
    reasons: &mut BTreeSet<String>,
) {
    let terminal = [
        "completed",
        "failed",
        "cancelled",
        "timeout",
        "timed_out",
        "not_executed",
    ];
    let execution_is_terminal = execution_status.is_some_and(|status| terminal.contains(&status));
    let final_is_terminal = terminal_status.is_some_and(|status| terminal.contains(&status));
    if !execution_is_terminal || !final_is_terminal {
        reasons.insert("non_terminal_execution_outcome".to_string());
    }
    let evaluation_is_known = evaluation_status.is_some_and(|status| {
        [
            "pass",
            "passed",
            "success",
            "succeeded",
            "fail",
            "failed",
            "error",
        ]
        .contains(&status)
    });
    if !evaluation_is_known {
        reasons.insert("missing_terminal_evaluation_outcome".to_string());
    }
    if success != terminal_status.is_some_and(|status| ["completed"].contains(&status)) {
        reasons.insert("contradictory_terminal_outcome".to_string());
    }
}

fn validate_measurements(
    trace: &RunTrace,
    quality_score: Option<f64>,
    cost_usd: Option<f64>,
    cost_kind: Option<&CostEvidenceKind>,
    retry_count: Option<u64>,
    tool_success_score: Option<f64>,
    reasons: &mut BTreeSet<String>,
) {
    if quality_score.is_none_or(|value| !normalized_score(value)) {
        reasons.insert("unmeasured_quality".to_string());
    }
    if tool_success_score.is_none_or(|value| !normalized_score(value)) {
        reasons.insert("unmeasured_tool_success".to_string());
        if tool_success_score.is_some() {
            reasons.insert("invalid_tool_success_measurement".to_string());
        }
    }
    if quality_score.is_some_and(|value| !normalized_score(value)) {
        reasons.insert("invalid_quality_measurement".to_string());
    }
    if cost_usd.is_some_and(|value| !value.is_finite() || !(0.0..=MAX_COST_USD).contains(&value)) {
        reasons.insert("invalid_cost_measurement".to_string());
    }
    if first_i64_values(&[&trace.execution, &trace.decision], &[&["retry_count"]])
        .is_some_and(|value| value < 0)
    {
        reasons.insert("invalid_retry_count".to_string());
    }
    if trace.latency_ms.is_none() {
        reasons.insert("missing_latency_measurement".to_string());
    }
    if trace.input_tokens.is_none() || trace.output_tokens.is_none() {
        reasons.insert("missing_token_measurement".to_string());
    }
    if retry_count.is_none() {
        reasons.insert("missing_retry_measurement".to_string());
    }
    match (cost_usd, cost_kind) {
        (None, _) => {
            reasons.insert("missing_cost_measurement".to_string());
            reasons.insert("unpriced_evidence".to_string());
        }
        (Some(_), Some(CostEvidenceKind::Estimated)) => {
            reasons.insert("unmeasured_cost".to_string());
            reasons.insert("unpriced_evidence".to_string());
        }
        (Some(_), Some(CostEvidenceKind::Measured | CostEvidenceKind::Posted)) => {}
        (Some(_), None) => {
            reasons.insert("malformed_cost_measurement".to_string());
        }
    };
    if first_bool_values(
        &[&trace.execution, &trace.evaluation, &trace.decision],
        &[&["pricing_complete"]],
    ) == Some(false)
    {
        reasons.insert("incomplete_pricing".to_string());
    }
}

fn validate_trace_consistency(trace: &RunTrace, reasons: &mut BTreeSet<String>) {
    if let Some(raw_status) = first_str_values(&[&trace.execution], &[&["status"]]) {
        if non_empty_lowercase(Some(raw_status))
            != non_empty_lowercase(trace.execution_status.as_deref())
        {
            reasons.insert("contradictory_execution_status".to_string());
        }
    }
    if let Some(raw_status) = first_str_values(&[&trace.evaluation], &[&["status"]]) {
        if non_empty_lowercase(Some(raw_status))
            != non_empty_lowercase(Some(&trace.evaluation_status))
        {
            reasons.insert("contradictory_evaluation_status".to_string());
        }
    }
    if let Some(raw_success) = first_bool_values(&[&trace.execution], &[&["success"]]) {
        if raw_success != trace.success {
            reasons.insert("contradictory_execution_success".to_string());
        }
    }
    for (raw, recorded, field) in [
        (
            first_i64_values(&[&trace.execution], &[&["latency_ms"]]),
            trace.latency_ms,
            "latency_ms",
        ),
        (
            first_i64_values(&[&trace.execution], &[&["input_tokens"]]),
            trace.input_tokens,
            "input_tokens",
        ),
        (
            first_i64_values(&[&trace.execution], &[&["output_tokens"]]),
            trace.output_tokens,
            "output_tokens",
        ),
        (
            first_i64_values(&[&trace.execution], &[&["retry_count"]]),
            Some(trace.retry_count),
            "retry_count",
        ),
    ] {
        if raw
            .zip(recorded)
            .is_some_and(|(raw, recorded)| raw != recorded)
        {
            reasons.insert(format!("contradictory_{field}_measurement"));
        }
    }
    if let Some(raw_tier) = first_str_values(&[&trace.decision], &[&["selected_tier"]]) {
        if raw_tier != trace.selected_tier {
            reasons.insert("contradictory_candidate_identity".to_string());
        }
    }
}

fn judge_reference(
    trace: &RunTrace,
    reasons: &mut BTreeSet<String>,
) -> Option<JudgeReferenceEvidence> {
    let judge_id = first_str_values(
        &[&trace.evaluation],
        &[
            &["judge_endpoint_id"],
            &["judge_id"],
            &["judge", "endpoint_id"],
        ],
    );
    let judge_score = first_f64_values(
        &[&trace.evaluation],
        &[&["judge_score"], &["judge", "score"]],
    );
    let reference_score = first_f64_values(
        &[&trace.evaluation],
        &[&["reference_score"], &["reference", "score"]],
    );
    if judge_id.is_none() && judge_score.is_none() && reference_score.is_none() {
        return None;
    }
    let Some(judge_endpoint_id) = judge_id else {
        reasons.insert("malformed_judge_reference_pair".to_string());
        return None;
    };
    let (Some(judge_score), Some(reference_score)) = (judge_score, reference_score) else {
        reasons.insert("malformed_judge_reference_pair".to_string());
        return None;
    };
    if !valid_id(judge_endpoint_id)
        || !normalized_score(judge_score)
        || !normalized_score(reference_score)
    {
        reasons.insert("malformed_judge_reference_pair".to_string());
        return None;
    }
    Some(JudgeReferenceEvidence {
        judge_endpoint_id: judge_endpoint_id.to_string(),
        judge_score,
        reference_score,
    })
}

fn cost_evidence(trace: &RunTrace) -> (Option<f64>, Option<CostEvidenceKind>) {
    let measured_paths = [
        &["actual_cost_usd"][..],
        &["measured_cost_usd"][..],
        &["cost_usd"][..],
        &["total_cost_usd"][..],
    ];
    if let Some(value) = first_f64_values(&[&trace.execution, &trace.evaluation], &measured_paths) {
        return (Some(value), Some(CostEvidenceKind::Measured));
    }
    if let Some(value) = first_f64_values(
        &[&trace.execution, &trace.evaluation],
        &[&["posted_cost_usd"]],
    ) {
        return (Some(value), Some(CostEvidenceKind::Posted));
    }
    if let Some(value) = first_f64_values(
        &[&trace.execution, &trace.evaluation],
        &[&["estimated_cost"], &["estimated_cost_usd"]],
    ) {
        return (Some(value), Some(CostEvidenceKind::Estimated));
    }
    (None, None)
}

fn candidate_binding(observation: &NormalizedReplayObservation) -> Option<ReplayCandidateBinding> {
    Some(ReplayCandidateBinding {
        candidate_id: observation.candidate_id.clone()?,
        candidate_version: observation.candidate_version.clone()?,
        candidate_definition_sha256: observation.candidate_definition_sha256.clone()?,
        member_endpoint_ids: observation.member_endpoint_ids.clone(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CohortKey {
    task_class: String,
    task_domain: String,
    task_intent: String,
    objective: String,
    measurement_schema_version: String,
    routing_policy: Option<String>,
    policy_version: Option<String>,
    policy_hash: Option<String>,
}

fn cohort_key(observation: &NormalizedReplayObservation) -> Option<CohortKey> {
    Some(CohortKey {
        task_class: observation.task_class.clone()?,
        task_domain: observation.task_domain.clone()?,
        task_intent: observation.task_intent.clone()?,
        objective: observation.objective.clone()?,
        measurement_schema_version: observation.measurement_schema_version.clone()?,
        routing_policy: observation.routing_policy.clone(),
        policy_version: observation.policy_version.clone(),
        policy_hash: observation.policy_hash.clone(),
    })
}

fn build_coverage(
    observations: &[ReplayObservationEvidence],
    accepted: &[&ReplayObservationEvidence],
) -> ReplayCoverage {
    let submitted = observations.len();
    let accepted_count = accepted.len();
    let mut candidate_counts = BTreeMap::new();
    let mut candidate_complexity_bucket_counts = BTreeMap::<String, BTreeMap<String, usize>>::new();
    let mut paired_complexity_bucket_counts = BTreeMap::new();
    let mut judge_pair_counts = BTreeMap::new();
    for evidence in accepted {
        let Some(candidate_id) = evidence.observation.candidate_id.as_ref() else {
            continue;
        };
        *candidate_counts.entry(candidate_id.clone()).or_default() += 1;
        if let Some(bucket) = evidence.observation.complexity_bucket.as_ref() {
            *candidate_complexity_bucket_counts
                .entry(candidate_id.clone())
                .or_default()
                .entry(bucket.clone())
                .or_default() += 1;
        }
        if let Some(judge) = evidence.observation.judge_reference.as_ref() {
            *judge_pair_counts
                .entry(judge.judge_endpoint_id.clone())
                .or_default() += 1;
        }
    }
    let buckets = candidate_complexity_bucket_counts
        .values()
        .flat_map(|counts| counts.keys().cloned())
        .collect::<BTreeSet<_>>();
    for bucket in buckets {
        let count = candidate_complexity_bucket_counts
            .values()
            .filter(|counts| counts.contains_key(&bucket))
            .count();
        paired_complexity_bucket_counts.insert(bucket, count);
    }
    ReplayCoverage {
        submitted_observation_count: submitted,
        accepted_observation_count: accepted_count,
        rejected_observation_count: submitted.saturating_sub(accepted_count),
        accepted_ratio: if submitted == 0 {
            0.0
        } else {
            accepted_count as f64 / submitted as f64
        },
        candidate_counts,
        candidate_complexity_bucket_counts,
        paired_complexity_bucket_counts,
        judge_pair_counts,
    }
}

fn calibrate_judges(
    accepted: &[&ReplayObservationEvidence],
    reasons: &mut BTreeSet<String>,
) -> Vec<JudgeCalibrationEvidence> {
    let mut samples = BTreeMap::<String, Vec<(f64, f64)>>::new();
    for evidence in accepted {
        if let Some(pair) = evidence.observation.judge_reference.as_ref() {
            samples
                .entry(pair.judge_endpoint_id.clone())
                .or_default()
                .push((pair.judge_score, pair.reference_score));
        }
    }
    samples
        .into_iter()
        .map(|(judge_endpoint_id, values)| {
            let sample_count = values.len();
            let denominator = sample_count as f64;
            let mean_signed_bias = values
                .iter()
                .map(|(judge, reference)| judge - reference)
                .sum::<f64>()
                / denominator;
            let mean_absolute_error = values
                .iter()
                .map(|(judge, reference)| (judge - reference).abs())
                .sum::<f64>()
                / denominator;
            let status = if sample_count >= MIN_JUDGE_CALIBRATION_SAMPLES {
                "calibrated"
            } else {
                reasons.insert("insufficient_judge_calibration".to_string());
                "insufficient_data"
            };
            JudgeCalibrationEvidence {
                judge_endpoint_id,
                sample_count,
                mean_signed_bias,
                mean_absolute_error,
                status: status.to_string(),
            }
        })
        .collect()
}

fn build_cohort(
    accepted: &[&ReplayObservationEvidence],
    candidate_bindings: BTreeMap<String, ReplayCandidateBinding>,
) -> ReplayCohort {
    let first = accepted
        .first()
        .and_then(|evidence| cohort_key(&evidence.observation))
        .expect("accepted replay evidence has a complete cohort key");
    let complexity_buckets = accepted
        .iter()
        .filter_map(|evidence| evidence.observation.complexity_bucket.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    ReplayCohort {
        task_class: first.task_class,
        task_domain: first.task_domain,
        task_intent: first.task_intent,
        objective: first.objective,
        measurement_schema_version: first.measurement_schema_version,
        routing_policy: first.routing_policy,
        policy_version: first.policy_version,
        policy_hash: first.policy_hash,
        candidate_bindings,
        complexity_buckets,
        envelope: build_envelope(accepted),
    }
}

fn build_envelope(accepted: &[&ReplayObservationEvidence]) -> ReplayEnvelope {
    let costs = accepted
        .iter()
        .filter_map(|evidence| evidence.observation.cost_usd)
        .collect::<Vec<_>>();
    let latencies = accepted
        .iter()
        .filter_map(|evidence| evidence.observation.latency_ms.map(|value| value as f64))
        .collect::<Vec<_>>();
    let tokens = accepted
        .iter()
        .filter_map(|evidence| {
            Some((evidence.observation.input_tokens? + evidence.observation.output_tokens?) as f64)
        })
        .collect::<Vec<_>>();
    let retries = accepted
        .iter()
        .filter_map(|evidence| evidence.observation.retry_count.map(|value| value as f64))
        .collect::<Vec<_>>();
    ReplayEnvelope {
        cost_usd: metric_envelope(&costs),
        latency_ms: metric_envelope(&latencies),
        total_tokens: metric_envelope(&tokens),
        retry_count: metric_envelope(&retries),
    }
}

fn metric_envelope(values: &[f64]) -> ReplayMetricEnvelope {
    ReplayMetricEnvelope {
        minimum: values.iter().copied().reduce(f64::min),
        maximum: values.iter().copied().reduce(f64::max),
    }
}

fn validate_scope(scope: &ReplayEvidenceScope, reasons: &mut BTreeSet<String>) {
    if scope
        .max_cost_usd
        .is_some_and(|value| !value.is_finite() || !(0.0..=MAX_COST_USD).contains(&value))
    {
        reasons.insert("invalid_cost_envelope".to_string());
    }
    if scope
        .max_latency_ms
        .is_some_and(|value| value > MAX_LATENCY_MS)
    {
        reasons.insert("invalid_latency_envelope".to_string());
    }
    if scope
        .max_total_tokens
        .is_some_and(|value| value > MAX_TOTAL_TOKENS)
    {
        reasons.insert("invalid_token_envelope".to_string());
    }
    if scope
        .max_retry_count
        .is_some_and(|value| value > MAX_RETRY_COUNT)
    {
        reasons.insert("invalid_retry_envelope".to_string());
    }
    if scope
        .allowed_complexity_buckets
        .as_ref()
        .is_some_and(|buckets| {
            buckets.is_empty()
                || buckets
                    .iter()
                    .any(|bucket| !["low", "medium", "high"].contains(&bucket.as_str()))
        })
    {
        reasons.insert("invalid_complexity_envelope".to_string());
    }
}

fn apply_scope(
    observation: &NormalizedReplayObservation,
    scope: &ReplayEvidenceScope,
    reasons: &mut BTreeSet<String>,
) {
    if observation
        .cost_usd
        .is_some_and(|value| value > scope.max_cost_usd.unwrap_or(MAX_COST_USD))
    {
        reasons.insert("out_of_distribution_cost".to_string());
    }
    if observation
        .latency_ms
        .is_some_and(|value| value > scope.max_latency_ms.unwrap_or(MAX_LATENCY_MS))
    {
        reasons.insert("out_of_distribution_latency".to_string());
    }
    if observation
        .input_tokens
        .zip(observation.output_tokens)
        .is_some_and(|(input, output)| {
            input.saturating_add(output) > scope.max_total_tokens.unwrap_or(MAX_TOTAL_TOKENS)
        })
    {
        reasons.insert("out_of_distribution_tokens".to_string());
    }
    if observation
        .retry_count
        .is_some_and(|value| value > scope.max_retry_count.unwrap_or(MAX_RETRY_COUNT))
    {
        reasons.insert("out_of_distribution_retries".to_string());
    }
    if scope
        .allowed_complexity_buckets
        .as_ref()
        .zip(observation.complexity_bucket.as_ref())
        .is_some_and(|(allowed, bucket)| !allowed.contains(bucket))
    {
        reasons.insert("out_of_distribution_complexity".to_string());
    }
}

fn build_references(trace: &RunTrace, trace_hash: Option<&str>) -> Vec<ReplayEvidenceReference> {
    let mut references = Vec::new();
    if let Some(trace_hash) = trace_hash {
        references.push(ReplayEvidenceReference {
            source_kind: "run_trace".to_string(),
            source_id: trace.trace_id.clone(),
            schema_version: trace.schema_version.clone(),
            content_sha256: trace_hash.to_string(),
        });
    }
    if let Some(evaluation_id) = first_str_values(&[&trace.evaluation], &[&["evaluation_id"]]) {
        if let Some(content_sha256) = hash_value(&trace.evaluation) {
            references.push(ReplayEvidenceReference {
                source_kind: "evaluation_result".to_string(),
                source_id: evaluation_id.to_string(),
                schema_version: first_str_values(&[&trace.evaluation], &[&["schema_version"]])
                    .unwrap_or("evaluation_result.unknown")
                    .to_string(),
                content_sha256,
            });
        }
    }
    let attribution = OutcomeAttributor::attribute(trace);
    if let Some(content_sha256) = hash_value(&attribution) {
        references.push(ReplayEvidenceReference {
            source_kind: "outcome_attribution".to_string(),
            source_id: trace.trace_id.clone(),
            schema_version: attribution.schema_version.clone(),
            content_sha256,
        });
    }
    references.truncate(MAX_EVIDENCE_REFERENCES);
    references
}

fn finalize_observation(evidence: &mut ReplayObservationEvidence) {
    evidence.reason_codes.sort();
    evidence.reason_codes.dedup();
    evidence.disposition = if evidence.reason_codes.is_empty() {
        EvidenceDisposition::Accepted
    } else {
        EvidenceDisposition::Rejected
    };
    evidence.content_sha256 =
        replay_observation_evidence_sha256(evidence).expect("replay evidence is serializable");
}

fn hash_result(result: &ReplayEligibilityResult) -> String {
    replay_eligibility_result_sha256(result).expect("replay eligibility result is serializable")
}

fn hash_value<T: Serialize>(value: &T) -> Option<String> {
    canonical_json(value).map(|canonical| sha256_hex(canonical.as_bytes()))
}

fn canonical_json<T: Serialize>(value: &T) -> Option<String> {
    let value = serde_json::to_value(value).ok()?;
    serde_json::to_string(&canonical_value(value)).ok()
}

fn canonical_value(value: Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut ordered = Map::new();
            let mut entries = object.into_iter().collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            for (key, value) in entries {
                ordered.insert(key, canonical_value(value));
            }
            Value::Object(ordered)
        }
        Value::Array(values) => Value::Array(values.into_iter().map(canonical_value).collect()),
        other => other,
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn value_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter()
        .try_fold(value, |current, key| current.get(*key))
}

fn first_str_values<'a>(values: &[&'a Value], paths: &[&[&str]]) -> Option<&'a str> {
    values.iter().find_map(|value| {
        paths
            .iter()
            .find_map(|path| value_at(value, path)?.as_str())
    })
}

fn first_f64_values(values: &[&Value], paths: &[&[&str]]) -> Option<f64> {
    values.iter().find_map(|value| {
        paths
            .iter()
            .find_map(|path| value_at(value, path)?.as_f64())
    })
}

fn first_i64_values(values: &[&Value], paths: &[&[&str]]) -> Option<i64> {
    values.iter().find_map(|value| {
        paths
            .iter()
            .find_map(|path| value_at(value, path)?.as_i64())
    })
}

fn first_bool_values(values: &[&Value], paths: &[&[&str]]) -> Option<bool> {
    values.iter().find_map(|value| {
        paths
            .iter()
            .find_map(|path| value_at(value, path)?.as_bool())
    })
}

fn first_string_array(values: &[&Value], paths: &[&[&str]]) -> Vec<String> {
    values
        .iter()
        .find_map(|value| {
            paths.iter().find_map(|path| {
                value_at(value, path)?.as_array().map(|array| {
                    array
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
            })
        })
        .unwrap_or_default()
}

fn non_empty_string(value: Option<&String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty()).cloned()
}

fn non_empty_lowercase(value: Option<&str>) -> Option<String> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_ascii_lowercase())
}

fn parse_time(value: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(value).ok()
}

fn parse_time_opt(value: Option<&str>) -> Option<DateTime<FixedOffset>> {
    value.and_then(parse_time)
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

fn normalized_score(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

fn complexity_bucket_from_score(score: f64) -> Option<String> {
    if !normalized_score(score) {
        None
    } else if score < 0.34 {
        Some("low".to_string())
    } else if score < 0.67 {
        Some("medium".to_string())
    } else {
        Some("high".to_string())
    }
}

fn non_negative_u64(value: Option<i64>, name: &str, reasons: &mut BTreeSet<String>) -> Option<u64> {
    match value {
        Some(value) if value >= 0 => Some(value as u64),
        Some(_) => {
            reasons.insert(format!("invalid_{name}"));
            None
        }
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::run_trace_recorder::RunTraceRecorder;
    use super::*;
    use serde_json::json;

    fn trace(candidate: &str, index: usize, judge: bool) -> RunTrace {
        let mut evaluation = json!({
            "schema_version": "evaluation_result.v1",
            "evaluation_id": format!("evaluation-{candidate}-{index}"),
            "status": "pass",
            "measurement_schema_version": "quality.v2",
            "quality_score": 0.8,
            "tool_success_score": 0.9,
            "quality_score_source": "reference_check",
        });
        if judge {
            evaluation["judge_endpoint_id"] = json!("judge-1");
            evaluation["judge_score"] = json!(0.8);
            evaluation["reference_score"] = json!(0.75);
        }
        RunTrace {
            schema_version: RUN_TRACE_SCHEMA_VERSION.to_string(),
            trace_id: format!("trace-{candidate}-{index}"),
            dispatch_id: format!("dispatch-{candidate}-{index}"),
            history_id: Some(index as i64),
            created_at: Some("2026-07-11T00:00:00Z".to_string()),
            task_class: "code".to_string(),
            task_domain: Some("software".to_string()),
            task_intent: Some("implement".to_string()),
            selected_tier: candidate.to_string(),
            selected_profile: None,
            routing_policy: Some("policy-map".to_string()),
            complexity_score: Some(0.4),
            constraints: vec![],
            human_review_flag: false,
            retry_policy: None,
            shadow_routes: vec![],
            executor_type: "stub".to_string(),
            execution_status: Some("completed".to_string()),
            latency_ms: Some(100 + index as i64),
            input_tokens: Some(100),
            output_tokens: Some(50),
            estimated_cost_usd: Some(0.01),
            reserved_cost: 0.02,
            total_cost: 0.01,
            retry_count: 0,
            evaluation_status: "pass".to_string(),
            final_status: "completed".to_string(),
            success: true,
            failure_domain: None,
            analysis: json!({
                "task_class": "code",
                "task_domain": "software",
                "task_intent": "implement",
                "objective": "quality",
                "complexity_bucket": "medium"
            }),
            decision: json!({
                "selected_tier": candidate,
                "candidate_id": candidate,
                "candidate_version": "v1",
                "candidate_hash": format!("{:064x}", if candidate == "candidate-a" { 1 } else { 2 }),
                "routing_policy": "policy-map",
                "policy_version": "policy-v1",
                "member_endpoint_ids": [candidate]
            }),
            execution: json!({
                "status": "completed",
                "latency_ms": 100 + index,
                "input_tokens": 100,
                "output_tokens": 50,
                "actual_cost_usd": 0.01,
                "retry_count": 0,
                "success": true
            }),
            evaluation,
        }
    }

    fn request(traces: Vec<RunTrace>) -> ReplayEligibilityRequest {
        ReplayEligibilityRequest {
            schema_version: POLICY_REPLAY_CONTRACT_SCHEMA_VERSION.to_string(),
            generated_at: "2026-07-11T00:01:00Z".to_string(),
            maximum_trace_age_seconds: 300,
            scope: ReplayEvidenceScope::default(),
            traces: traces
                .into_iter()
                .map(|trace| ReplayTraceInput::from_trace(trace).expect("test trace hash"))
                .collect(),
        }
    }

    #[test]
    fn normalized_contract_uses_trace_fields_and_is_order_independent() {
        let mut traces = (0..30)
            .map(|index| trace("candidate-a", index, index < 3))
            .collect::<Vec<_>>();
        traces.extend((0..30).map(|index| trace("candidate-b", index, index < 3)));
        let first = evaluate_replay_eligibility(&request(traces.clone()));
        traces.reverse();
        let second = evaluate_replay_eligibility(&request(traces));
        assert!(first.eligible, "{:?}", first.reason_codes);
        assert_eq!(first.content_sha256, second.content_sha256);
        assert!(first.observations.iter().all(|evidence| evidence
            .evidence_references
            .iter()
            .any(|reference| reference.source_kind == "run_trace")));
        assert_eq!(first.judge_calibrations[0].sample_count, 6);
        assert_eq!(first.judge_calibrations[0].status, "calibrated");
    }

    #[test]
    fn caller_hash_mismatch_is_tampered_and_cannot_be_repaired_by_booleans() {
        let trace = trace("candidate-a", 0, false);
        let mut input = ReplayTraceInput::from_trace(trace).expect("test trace hash");
        input.trace.evaluation["quality_score"] = json!(1.0);
        let result = evaluate_replay_eligibility(&request(vec![input.trace.clone()]));
        assert!(result
            .reason_codes
            .contains(&"insufficient_candidate_coverage".to_string()));

        let tampered_request = ReplayEligibilityRequest {
            schema_version: POLICY_REPLAY_CONTRACT_SCHEMA_VERSION.to_string(),
            generated_at: "2026-07-11T00:01:00Z".to_string(),
            maximum_trace_age_seconds: 300,
            scope: ReplayEvidenceScope::default(),
            traces: vec![input],
        };
        let tampered = evaluate_replay_eligibility(&tampered_request);
        assert!(tampered
            .reason_codes
            .contains(&"tampered_evidence".to_string()));
        assert!(!tampered
            .reason_codes
            .contains(&"unmeasured_quality".to_string()));
    }

    #[test]
    fn missing_and_estimated_measurements_fail_closed() {
        let mut trace = trace("candidate-a", 0, false);
        trace.execution["actual_cost_usd"] = Value::Null;
        trace.execution["estimated_cost"] = json!(0.01);
        trace.execution["retry_count"] = Value::Null;
        trace.evaluation["quality_score"] = Value::Null;
        let result = evaluate_replay_eligibility(&request(vec![trace]));
        for reason in [
            "unmeasured_quality",
            "unmeasured_cost",
            "unpriced_evidence",
            "missing_retry_measurement",
            "insufficient_trace_coverage",
        ] {
            assert!(
                result.reason_codes.contains(&reason.to_string()),
                "missing {reason}"
            );
        }
    }

    #[test]
    fn actual_paired_values_not_sample_count_calibrate_judge() {
        let mut traces = (0..30)
            .map(|index| trace("candidate-a", index, index < 1))
            .collect::<Vec<_>>();
        traces.extend((0..30).map(|index| trace("candidate-b", index, index < 1)));
        let result = evaluate_replay_eligibility(&request(traces));
        assert!(!result.eligible);
        assert!(result
            .reason_codes
            .contains(&"insufficient_judge_calibration".to_string()));
        assert_eq!(result.judge_calibrations[0].sample_count, 2);
    }

    #[test]
    fn scope_rejects_out_of_distribution_observations() {
        let mut trace = trace("candidate-a", 0, false);
        trace.latency_ms = Some(10_000);
        trace.execution["latency_ms"] = json!(10_000);
        let mut request = request(vec![trace]);
        request.scope.max_latency_ms = Some(1_000);
        let result = evaluate_replay_eligibility(&request);
        assert!(result
            .reason_codes
            .contains(&"out_of_distribution_latency".to_string()));
    }

    #[test]
    fn contradictory_persisted_sections_are_rejected() {
        let mut trace = trace("candidate-a", 0, false);
        trace.execution["success"] = json!(false);
        let result = evaluate_replay_eligibility(&request(vec![trace]));
        assert!(result
            .reason_codes
            .contains(&"contradictory_execution_success".to_string()));
    }

    #[test]
    fn real_run_trace_defaults_are_not_promoted_to_replay_facts() {
        let dispatch = json!({
            "dispatch_id": "dispatch-real",
            "created_at": "2026-07-11T00:00:00Z",
            "bundle": {
                "analysis": { "task_class": "code" },
                "decision": { "selected_tier": "balanced_worker" },
                "execution_result": {
                    "executor_type": "noop",
                    "status": "completed",
                    "success": true,
                    "latency_ms": 10,
                    "input_tokens": 10,
                    "output_tokens": 5,
                    "estimated_cost": 0.01
                },
                "evaluation_result": { "status": "pass", "quality_score": 0.8 },
                "record": { "final_status": "completed" }
            }
        });
        let trace = RunTraceRecorder::record_from_dispatch(&dispatch);
        let request = request(vec![trace]);
        let result = evaluate_replay_eligibility(&request);
        assert!(!result.eligible);
        for reason in [
            "missing_candidate_definition",
            "missing_candidate_version",
            "missing_measurement_schema",
            "missing_objective",
            "missing_retry_measurement",
            "unpriced_evidence",
            "unmeasured_tool_success",
        ] {
            assert!(
                result.reason_codes.contains(&reason.to_string()),
                "missing {reason}"
            );
        }
        assert!(result
            .observations
            .iter()
            .all(|evidence| evidence.observation.candidate_definition_sha256.is_none()));
    }
}
