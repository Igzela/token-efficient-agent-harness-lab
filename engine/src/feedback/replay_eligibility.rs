use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const POLICY_REPLAY_CONTRACT_SCHEMA_VERSION: &str = "policy_replay_contract.v1";
pub const MIN_CANDIDATE_OBSERVATIONS: usize = 30;
pub const MIN_JUDGE_CALIBRATION_SAMPLES: usize = 3;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReplayEligibilityInput {
    pub schema_version: String,
    pub trace_id: String,
    pub observed_at: String,
    pub task_class: String,
    pub objective: String,
    pub candidate_id: String,
    pub candidate_definition_sha256: String,
    pub measurement_schema_version: String,
    pub complexity_bucket: String,
    pub judge_id: Option<String>,
    pub has_reference_score: bool,
    pub has_complete_measurements: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReplayEligibilityRequest {
    pub schema_version: String,
    pub generated_at: String,
    pub maximum_trace_age_seconds: u64,
    pub inputs: Vec<ReplayEligibilityInput>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReplayEligibilityResult {
    pub schema_version: String,
    pub eligible: bool,
    pub reason_codes: Vec<String>,
    pub accepted_trace_ids: Vec<String>,
    pub content_sha256: String,
    pub shadow_only: bool,
}

pub fn evaluate_replay_eligibility(request: &ReplayEligibilityRequest) -> ReplayEligibilityResult {
    let mut reasons = BTreeSet::new();
    let generated_at = parse_time(&request.generated_at);
    if request.schema_version != POLICY_REPLAY_CONTRACT_SCHEMA_VERSION {
        reasons.insert("unsupported_contract_schema".to_string());
    }
    if request.maximum_trace_age_seconds == 0
        || request.maximum_trace_age_seconds > 30 * 24 * 60 * 60
    {
        reasons.insert("invalid_maximum_trace_age".to_string());
    }
    let mut seen = BTreeSet::new();
    let mut accepted = Vec::new();
    let mut candidates = BTreeMap::<String, usize>::new();
    let mut judge_samples = BTreeMap::<String, usize>::new();
    let mut definitions = BTreeMap::<String, String>::new();
    let mut cohorts = BTreeSet::new();
    for input in &request.inputs {
        if input.schema_version != "feedback_trace.v1" || !seen.insert(input.trace_id.clone()) {
            reasons.insert("duplicate_or_incompatible_trace".to_string());
            continue;
        }
        let observed = parse_time(&input.observed_at);
        if generated_at.zip(observed).is_none_or(|(now, then)| {
            then > now || (now - then).num_seconds() as u64 > request.maximum_trace_age_seconds
        }) {
            reasons.insert("stale_or_invalid_trace".to_string());
            continue;
        }
        if !input.has_complete_measurements {
            reasons.insert("uncovered_trace_measurement".to_string());
            continue;
        }
        let cohort = format!(
            "{}:{}:{}",
            input.task_class, input.objective, input.measurement_schema_version
        );
        cohorts.insert(cohort);
        if definitions
            .insert(
                input.candidate_id.clone(),
                input.candidate_definition_sha256.clone(),
            )
            .is_some_and(|existing| existing != input.candidate_definition_sha256)
        {
            reasons.insert("inconsistent_candidate_definition".to_string());
            continue;
        }
        *candidates.entry(input.candidate_id.clone()).or_default() += 1;
        if input.has_reference_score {
            if let Some(judge_id) = input.judge_id.as_ref() {
                *judge_samples.entry(judge_id.clone()).or_default() += 1;
            }
        }
        accepted.push(input.trace_id.clone());
    }
    if cohorts.len() != 1 {
        reasons.insert("incomparable_cohort".to_string());
    }
    if candidates.len() < 2
        || candidates
            .values()
            .any(|count| *count < MIN_CANDIDATE_OBSERVATIONS)
    {
        reasons.insert("sparse_candidate_coverage".to_string());
    }
    if !judge_samples.is_empty()
        && judge_samples
            .values()
            .any(|count| *count < MIN_JUDGE_CALIBRATION_SAMPLES)
    {
        reasons.insert("insufficient_judge_calibration".to_string());
    }
    if request.inputs.is_empty() || accepted.len() * 10 < request.inputs.len() * 9 {
        reasons.insert("insufficient_trace_coverage".to_string());
    }
    accepted.sort();
    let reason_codes = reasons.into_iter().collect::<Vec<_>>();
    let eligible = reason_codes.is_empty();
    let mut result = ReplayEligibilityResult {
        schema_version: POLICY_REPLAY_CONTRACT_SCHEMA_VERSION.to_string(),
        eligible,
        reason_codes,
        accepted_trace_ids: accepted,
        content_sha256: String::new(),
        shadow_only: true,
    };
    result.content_sha256 = hash_result(&result);
    result
}

fn parse_time(value: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(value).ok()
}
fn hash_result(result: &ReplayEligibilityResult) -> String {
    let mut copy = result.clone();
    copy.content_sha256.clear();
    hex::encode(Sha256::digest(
        serde_json::to_vec(&copy).unwrap_or_default(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(candidate: &str, index: usize) -> ReplayEligibilityInput {
        ReplayEligibilityInput {
            schema_version: "feedback_trace.v1".to_string(),
            trace_id: format!("{candidate}-{index}"),
            observed_at: "2026-07-11T00:00:00Z".to_string(),
            task_class: "code".to_string(),
            objective: "quality".to_string(),
            candidate_id: candidate.to_string(),
            candidate_definition_sha256: format!("{candidate}-definition"),
            measurement_schema_version: "quality.v1".to_string(),
            complexity_bucket: "medium".to_string(),
            judge_id: None,
            has_reference_score: false,
            has_complete_measurements: true,
        }
    }

    fn request(inputs: Vec<ReplayEligibilityInput>) -> ReplayEligibilityRequest {
        ReplayEligibilityRequest {
            schema_version: POLICY_REPLAY_CONTRACT_SCHEMA_VERSION.to_string(),
            generated_at: "2026-07-11T00:01:00Z".to_string(),
            maximum_trace_age_seconds: 300,
            inputs,
        }
    }

    #[test]
    fn eligible_cohort_is_deterministic_and_shadow_only() {
        let mut inputs = (0..30).map(|index| input("a", index)).collect::<Vec<_>>();
        inputs.extend((0..30).map(|index| input("b", index)));
        let first = evaluate_replay_eligibility(&request(inputs.clone()));
        inputs.reverse();
        let second = evaluate_replay_eligibility(&request(inputs));
        assert!(first.eligible);
        assert!(first.shadow_only);
        assert_eq!(first, second);
    }

    #[test]
    fn stale_sparse_and_duplicate_inputs_refuse_with_stable_codes() {
        let mut stale = input("a", 0);
        stale.observed_at = "2026-01-01T00:00:00Z".to_string();
        let duplicate = stale.clone();
        let result = evaluate_replay_eligibility(&request(vec![stale, duplicate]));
        assert!(!result.eligible);
        assert_eq!(
            result.reason_codes,
            vec![
                "duplicate_or_incompatible_trace",
                "incomparable_cohort",
                "insufficient_trace_coverage",
                "sparse_candidate_coverage",
                "stale_or_invalid_trace",
            ],
        );
    }
}
