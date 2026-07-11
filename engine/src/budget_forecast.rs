use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, FixedOffset, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::budget_manager::{
    BudgetConfidence, BudgetConfidenceLevel, BudgetEvidenceCoverage, BudgetEvidenceOutcome,
    BudgetEvidenceReference, BudgetEvidenceScope, BudgetEvidenceWindow, BudgetForecastEstimate,
    BudgetForecastEvidence, BudgetObservedUsage, BUDGET_FORECAST_EVIDENCE_SCHEMA_VERSION,
};
use crate::provider::ProviderAuditEvent;

const MAX_FORECAST_OBSERVATIONS: usize = 10_000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BudgetUsageObservation {
    pub evidence_type: String,
    pub evidence_id: String,
    pub content_sha256: Option<String>,
    pub occurred_at: String,
    pub run_id: Option<String>,
    pub workspace_id: Option<String>,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub cost_usd: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BudgetForecastRequest {
    pub forecast_id: String,
    pub scope: BudgetEvidenceScope,
    pub start_inclusive: String,
    pub end_exclusive: String,
    pub generated_at: String,
    pub horizon_seconds: u64,
    pub remaining_tokens: Option<i64>,
    pub remaining_cost_usd: Option<f64>,
    pub required_dimensions: Vec<String>,
    pub min_samples: u32,
    pub max_freshness_seconds: u64,
    pub max_duplicate_events: u32,
}

pub fn observation_from_provider_audit(event: &ProviderAuditEvent) -> BudgetUsageObservation {
    let total_tokens = match (event.input_token_count, event.output_token_count) {
        (Some(input), Some(output)) => input.checked_add(output),
        _ => None,
    };
    BudgetUsageObservation {
        evidence_type: "provider_audit_event".to_string(),
        evidence_id: event.event_id.clone(),
        content_sha256: None,
        occurred_at: event.created_at.clone(),
        run_id: None,
        workspace_id: None,
        provider_id: Some(event.provider_id.clone()),
        model_id: None,
        input_tokens: event.input_token_count,
        output_tokens: event.output_token_count,
        total_tokens,
        cost_usd: match event.currency.as_deref() {
            Some("USD") => event.cost,
            _ => None,
        },
    }
}

pub fn build_budget_forecast(
    request: &BudgetForecastRequest,
    observations: &[BudgetUsageObservation],
) -> Result<BudgetForecastEvidence, String> {
    validate_request(request)?;
    if observations.len() > MAX_FORECAST_OBSERVATIONS {
        return Err("forecast observation count exceeds the bounded maximum".to_string());
    }

    let start = parse_timestamp("start_inclusive", &request.start_inclusive)?;
    let end = parse_timestamp("end_exclusive", &request.end_exclusive)?;
    let generated = parse_timestamp("generated_at", &request.generated_at)?;
    let freshness_seconds = (generated - end).num_seconds() as u64;

    let mut selected = Vec::new();
    for observation in observations {
        let occurred = match parse_timestamp("observation.occurred_at", &observation.occurred_at) {
            Ok(value) => value,
            Err(_) => {
                return make_evidence(
                    request,
                    BudgetEvidenceOutcome::InvalidEvidence,
                    0,
                    freshness_seconds,
                    0,
                    vec!["invalid_evidence.timestamp".to_string()],
                    vec![],
                    false,
                    empty_observed(),
                    None,
                );
            }
        };
        if occurred >= start && occurred < end && scope_matches(&request.scope, observation) {
            selected.push(observation.clone());
        }
    }
    selected.sort_by(|left, right| observation_key(left).cmp(&observation_key(right)));

    let mut deduplicated = BTreeMap::new();
    let mut duplicate_events = 0_u32;
    for observation in selected {
        let key = (
            observation.evidence_type.clone(),
            observation.evidence_id.clone(),
        );
        match deduplicated.get(&key) {
            None => {
                deduplicated.insert(key, observation);
            }
            Some(existing) if existing == &observation => {
                duplicate_events = duplicate_events.saturating_add(1);
            }
            Some(_) => {
                return make_evidence(
                    request,
                    BudgetEvidenceOutcome::InvalidEvidence,
                    deduplicated.len() as u32,
                    freshness_seconds,
                    duplicate_events.saturating_add(1),
                    vec!["invalid_evidence.conflicting_duplicate".to_string()],
                    evidence_references(deduplicated.values()),
                    false,
                    empty_observed(),
                    None,
                );
            }
        }
    }
    let selected = deduplicated.into_values().collect::<Vec<_>>();
    let sample_count = selected.len() as u32;

    if let Some(reason) = invalid_observation_reason(&selected) {
        return make_evidence(
            request,
            BudgetEvidenceOutcome::InvalidEvidence,
            sample_count,
            freshness_seconds,
            duplicate_events,
            vec![reason],
            evidence_references(selected.iter()),
            false,
            empty_observed(),
            None,
        );
    }

    let observed_dimensions = observed_dimensions(&selected);
    let mut missing_fields = request
        .required_dimensions
        .iter()
        .filter(|dimension| !observed_dimensions.contains(*dimension))
        .cloned()
        .collect::<Vec<_>>();
    let mixed_dimensions =
        mixed_dimensions(&request.scope, &request.required_dimensions, &selected);
    missing_fields.extend(
        mixed_dimensions
            .iter()
            .map(|dimension| format!("{dimension}.mixed")),
    );
    missing_fields.sort();
    missing_fields.dedup();

    let pricing_complete = !selected.is_empty()
        && selected
            .iter()
            .all(|observation| observation.cost_usd.is_some());
    let observed = aggregate_observed(&selected)?;
    let references = evidence_references(selected.iter());

    let mut insufficiency_reasons = Vec::new();
    if sample_count < request.min_samples {
        insufficiency_reasons.push("insufficient_evidence.sparse".to_string());
    }
    if freshness_seconds > request.max_freshness_seconds {
        insufficiency_reasons.push("insufficient_evidence.stale".to_string());
    }
    if duplicate_events > request.max_duplicate_events {
        insufficiency_reasons.push("insufficient_evidence.duplicate_limit".to_string());
    }
    if !missing_fields.is_empty() {
        insufficiency_reasons.push("insufficient_evidence.missing_dimensions".to_string());
    }
    if !mixed_dimensions.is_empty() {
        insufficiency_reasons.push("insufficient_evidence.mixed_dimensions".to_string());
    }
    if !pricing_complete {
        insufficiency_reasons.push("insufficient_evidence.incomplete_pricing".to_string());
    }
    insufficiency_reasons.sort();
    insufficiency_reasons.dedup();

    if !insufficiency_reasons.is_empty() {
        return make_evidence_with_coverage(
            request,
            BudgetEvidenceOutcome::InsufficientEvidence,
            sample_count,
            freshness_seconds,
            duplicate_events,
            insufficiency_reasons,
            references,
            pricing_complete,
            observed,
            None,
            observed_dimensions,
            missing_fields,
        );
    }

    let estimate = estimate(request, &observed, start, end, generated)?;
    make_evidence_with_coverage(
        request,
        BudgetEvidenceOutcome::Supported,
        sample_count,
        freshness_seconds,
        duplicate_events,
        vec!["forecast.deterministic".to_string()],
        references,
        pricing_complete,
        observed,
        Some(estimate),
        observed_dimensions,
        missing_fields,
    )
}

fn validate_request(request: &BudgetForecastRequest) -> Result<(), String> {
    let start = parse_timestamp("start_inclusive", &request.start_inclusive)?;
    let end = parse_timestamp("end_exclusive", &request.end_exclusive)?;
    let generated = parse_timestamp("generated_at", &request.generated_at)?;
    if start >= end {
        return Err("forecast request window must have start before end".to_string());
    }
    if end > generated {
        return Err("forecast request window cannot end after generation time".to_string());
    }
    if request.horizon_seconds == 0 {
        return Err("forecast horizon_seconds must be positive".to_string());
    }
    if request.min_samples < 3 {
        return Err("forecast min_samples must be at least 3".to_string());
    }
    if request.remaining_tokens.is_some_and(|value| value < 0) {
        return Err("remaining_tokens must be non-negative".to_string());
    }
    if request
        .remaining_cost_usd
        .is_some_and(|value| !value.is_finite() || value < 0.0)
    {
        return Err("remaining_cost_usd must be finite and non-negative".to_string());
    }
    if request
        .required_dimensions
        .windows(2)
        .any(|pair| pair[0] >= pair[1])
    {
        return Err("required_dimensions must be sorted and unique".to_string());
    }
    Ok(())
}

fn scope_matches(scope: &BudgetEvidenceScope, observation: &BudgetUsageObservation) -> bool {
    optional_matches(scope.run_id.as_deref(), observation.run_id.as_deref())
        && optional_matches(
            scope.workspace_id.as_deref(),
            observation.workspace_id.as_deref(),
        )
        && optional_matches(
            scope.provider_id.as_deref(),
            observation.provider_id.as_deref(),
        )
        && optional_matches(scope.model_id.as_deref(), observation.model_id.as_deref())
}

fn optional_matches(expected: Option<&str>, actual: Option<&str>) -> bool {
    expected.is_none_or(|expected| actual == Some(expected))
}

fn observation_key(observation: &BudgetUsageObservation) -> (&str, &str, &str) {
    (
        observation.occurred_at.as_str(),
        observation.evidence_type.as_str(),
        observation.evidence_id.as_str(),
    )
}

fn invalid_observation_reason(observations: &[BudgetUsageObservation]) -> Option<String> {
    for observation in observations {
        for value in [
            observation.input_tokens,
            observation.output_tokens,
            observation.total_tokens,
        ] {
            if value.is_some_and(|value| value < 0) {
                return Some("invalid_evidence.negative_tokens".to_string());
            }
        }
        if observation
            .cost_usd
            .is_some_and(|value| !value.is_finite() || value < 0.0)
        {
            return Some("invalid_evidence.invalid_cost".to_string());
        }
        if let (Some(input), Some(output), Some(total)) = (
            observation.input_tokens,
            observation.output_tokens,
            observation.total_tokens,
        ) {
            if input.checked_add(output) != Some(total) {
                return Some("invalid_evidence.contradictory_tokens".to_string());
            }
        }
    }
    None
}

fn observed_dimensions(observations: &[BudgetUsageObservation]) -> Vec<String> {
    let mut dimensions = Vec::new();
    if !observations.is_empty() && observations.iter().all(|item| item.run_id.is_some()) {
        dimensions.push("run_id".to_string());
    }
    if !observations.is_empty() && observations.iter().all(|item| item.workspace_id.is_some()) {
        dimensions.push("workspace_id".to_string());
    }
    if !observations.is_empty() && observations.iter().all(|item| item.provider_id.is_some()) {
        dimensions.push("provider_id".to_string());
    }
    if !observations.is_empty() && observations.iter().all(|item| item.model_id.is_some()) {
        dimensions.push("model_id".to_string());
    }
    dimensions.sort();
    dimensions
}

fn mixed_dimensions(
    scope: &BudgetEvidenceScope,
    required_dimensions: &[String],
    observations: &[BudgetUsageObservation],
) -> Vec<String> {
    let mut mixed = Vec::new();
    if required_dimensions
        .iter()
        .any(|dimension| dimension == "run_id")
        && scope.run_id.is_none()
        && distinct_count(
            observations
                .iter()
                .filter_map(|item| item.run_id.as_deref()),
        ) > 1
    {
        mixed.push("run_id".to_string());
    }
    if required_dimensions
        .iter()
        .any(|dimension| dimension == "workspace_id")
        && scope.workspace_id.is_none()
        && distinct_count(
            observations
                .iter()
                .filter_map(|item| item.workspace_id.as_deref()),
        ) > 1
    {
        mixed.push("workspace_id".to_string());
    }
    if required_dimensions
        .iter()
        .any(|dimension| dimension == "provider_id")
        && scope.provider_id.is_none()
        && distinct_count(
            observations
                .iter()
                .filter_map(|item| item.provider_id.as_deref()),
        ) > 1
    {
        mixed.push("provider_id".to_string());
    }
    if required_dimensions
        .iter()
        .any(|dimension| dimension == "model_id")
        && scope.model_id.is_none()
        && distinct_count(
            observations
                .iter()
                .filter_map(|item| item.model_id.as_deref()),
        ) > 1
    {
        mixed.push("model_id".to_string());
    }
    mixed.sort();
    mixed
}

fn distinct_count<'a>(values: impl Iterator<Item = &'a str>) -> usize {
    values.collect::<BTreeSet<_>>().len()
}

fn empty_observed() -> BudgetObservedUsage {
    BudgetObservedUsage {
        input_tokens: None,
        output_tokens: None,
        total_tokens: None,
        cost_usd: None,
        latency_ms: None,
        retry_count: None,
        context_bytes: None,
    }
}

fn aggregate_observed(
    observations: &[BudgetUsageObservation],
) -> Result<BudgetObservedUsage, String> {
    let mut input_tokens = 0_i64;
    let mut output_tokens = 0_i64;
    let mut total_tokens = 0_i64;
    let mut cost_usd = 0.0_f64;
    let mut input_complete = !observations.is_empty();
    let mut output_complete = !observations.is_empty();
    let mut total_complete = !observations.is_empty();
    let mut cost_complete = !observations.is_empty();
    for observation in observations {
        if let Some(value) = observation.input_tokens {
            input_tokens = input_tokens
                .checked_add(value)
                .ok_or_else(|| "input token aggregation overflow".to_string())?;
        } else {
            input_complete = false;
        }
        if let Some(value) = observation.output_tokens {
            output_tokens = output_tokens
                .checked_add(value)
                .ok_or_else(|| "output token aggregation overflow".to_string())?;
        } else {
            output_complete = false;
        }
        if let Some(value) = observation.total_tokens {
            total_tokens = total_tokens
                .checked_add(value)
                .ok_or_else(|| "total token aggregation overflow".to_string())?;
        } else {
            total_complete = false;
        }
        if let Some(value) = observation.cost_usd {
            cost_usd += value;
        } else {
            cost_complete = false;
        }
    }
    Ok(BudgetObservedUsage {
        input_tokens: input_complete.then_some(input_tokens),
        output_tokens: output_complete.then_some(output_tokens),
        total_tokens: total_complete.then_some(total_tokens),
        cost_usd: cost_complete.then_some(cost_usd),
        latency_ms: None,
        retry_count: None,
        context_bytes: None,
    })
}

fn estimate(
    request: &BudgetForecastRequest,
    observed: &BudgetObservedUsage,
    start: DateTime<FixedOffset>,
    end: DateTime<FixedOffset>,
    generated: DateTime<FixedOffset>,
) -> Result<BudgetForecastEstimate, String> {
    let window_seconds = (end - start).num_seconds() as f64;
    let horizon = request.horizon_seconds as f64;
    let expected_total_tokens = observed
        .total_tokens
        .map(|value| value as f64 * horizon / window_seconds);
    let expected_cost_usd = observed
        .cost_usd
        .map(|value| value * horizon / window_seconds);

    let token_exhaustion = match (request.remaining_tokens, observed.total_tokens) {
        (Some(remaining), Some(observed_tokens)) if observed_tokens > 0 => {
            Some(remaining as f64 / (observed_tokens as f64 / window_seconds))
        }
        _ => None,
    };
    let cost_exhaustion = match (request.remaining_cost_usd, observed.cost_usd) {
        (Some(remaining), Some(observed_cost)) if observed_cost > 0.0 => {
            Some(remaining / (observed_cost / window_seconds))
        }
        _ => None,
    };
    let exhaustion_seconds = [token_exhaustion, cost_exhaustion]
        .into_iter()
        .flatten()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .min_by(f64::total_cmp);
    let exhaustion_at = exhaustion_seconds.map(|seconds| {
        let seconds = seconds.ceil().min(i64::MAX as f64) as i64;
        (generated.with_timezone(&Utc) + chrono::Duration::seconds(seconds))
            .to_rfc3339_opts(SecondsFormat::Secs, true)
    });

    Ok(BudgetForecastEstimate {
        expected_total_tokens,
        expected_cost_usd,
        exhaustion_at,
    })
}

fn make_evidence(
    request: &BudgetForecastRequest,
    outcome: BudgetEvidenceOutcome,
    sample_count: u32,
    freshness_seconds: u64,
    duplicate_events: u32,
    reason_codes: Vec<String>,
    evidence_references: Vec<BudgetEvidenceReference>,
    pricing_complete: bool,
    observed: BudgetObservedUsage,
    estimate: Option<BudgetForecastEstimate>,
) -> Result<BudgetForecastEvidence, String> {
    make_evidence_with_coverage(
        request,
        outcome,
        sample_count,
        freshness_seconds,
        duplicate_events,
        reason_codes,
        evidence_references,
        pricing_complete,
        observed,
        estimate,
        vec![],
        request.required_dimensions.clone(),
    )
}

#[allow(clippy::too_many_arguments)]
fn make_evidence_with_coverage(
    request: &BudgetForecastRequest,
    outcome: BudgetEvidenceOutcome,
    sample_count: u32,
    freshness_seconds: u64,
    duplicate_events: u32,
    mut reason_codes: Vec<String>,
    evidence_references: Vec<BudgetEvidenceReference>,
    pricing_complete: bool,
    observed: BudgetObservedUsage,
    estimate: Option<BudgetForecastEstimate>,
    mut observed_dimensions: Vec<String>,
    mut missing_fields: Vec<String>,
) -> Result<BudgetForecastEvidence, String> {
    reason_codes.sort();
    reason_codes.dedup();
    observed_dimensions.sort();
    observed_dimensions.dedup();
    missing_fields.sort();
    missing_fields.dedup();
    let confidence = confidence(&outcome, sample_count, duplicate_events);
    let mut evidence = BudgetForecastEvidence {
        schema_version: BUDGET_FORECAST_EVIDENCE_SCHEMA_VERSION.to_string(),
        forecast_id: request.forecast_id.clone(),
        scope: request.scope.clone(),
        outcome,
        window: BudgetEvidenceWindow {
            start_inclusive: request.start_inclusive.clone(),
            end_exclusive: request.end_exclusive.clone(),
            generated_at: request.generated_at.clone(),
            freshness_seconds,
            sample_count,
        },
        coverage: BudgetEvidenceCoverage {
            required_dimensions: request.required_dimensions.clone(),
            observed_dimensions,
            pricing_complete,
            duplicate_events,
            missing_fields,
        },
        confidence,
        reason_codes,
        evidence_references,
        observed,
        estimate,
        assumptions: vec![
            "linear_rate_over_explicit_horizon".to_string(),
            "posted_evidence_only".to_string(),
        ],
        evidence_sha256: String::new(),
    };
    evidence.seal()?;
    evidence.validate()?;
    Ok(evidence)
}

fn confidence(
    outcome: &BudgetEvidenceOutcome,
    sample_count: u32,
    duplicate_events: u32,
) -> BudgetConfidence {
    match outcome {
        BudgetEvidenceOutcome::Supported if sample_count >= 10 && duplicate_events == 0 => {
            BudgetConfidence {
                level: BudgetConfidenceLevel::High,
                score: 0.9,
                reason_codes: vec!["confidence.coverage_high".to_string()],
            }
        }
        BudgetEvidenceOutcome::Supported => BudgetConfidence {
            level: BudgetConfidenceLevel::Medium,
            score: 0.7,
            reason_codes: vec!["confidence.coverage_medium".to_string()],
        },
        BudgetEvidenceOutcome::InsufficientEvidence => BudgetConfidence {
            level: BudgetConfidenceLevel::Low,
            score: 0.2,
            reason_codes: vec!["confidence.insufficient_evidence".to_string()],
        },
        BudgetEvidenceOutcome::InvalidEvidence => BudgetConfidence {
            level: BudgetConfidenceLevel::Low,
            score: 0.0,
            reason_codes: vec!["confidence.invalid_evidence".to_string()],
        },
    }
}

fn evidence_references<'a>(
    observations: impl Iterator<Item = &'a BudgetUsageObservation>,
) -> Vec<BudgetEvidenceReference> {
    let mut references = observations
        .map(|observation| BudgetEvidenceReference {
            evidence_type: observation.evidence_type.clone(),
            evidence_id: observation.evidence_id.clone(),
            content_sha256: observation.content_sha256.clone(),
        })
        .collect::<Vec<_>>();
    references.sort_by(|left, right| {
        (left.evidence_type.as_str(), left.evidence_id.as_str())
            .cmp(&(right.evidence_type.as_str(), right.evidence_id.as_str()))
    });
    references
}

fn parse_timestamp(name: &str, value: &str) -> Result<DateTime<FixedOffset>, String> {
    DateTime::parse_from_rfc3339(value).map_err(|_| format!("{name} must be RFC3339"))
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;

    fn request() -> BudgetForecastRequest {
        BudgetForecastRequest {
            forecast_id: "forecast-provider-a".to_string(),
            scope: BudgetEvidenceScope {
                provider_id: Some("provider-a".to_string()),
                ..BudgetEvidenceScope::default()
            },
            start_inclusive: "2026-07-10T00:00:00Z".to_string(),
            end_exclusive: "2026-07-10T01:00:00Z".to_string(),
            generated_at: "2026-07-10T01:05:00Z".to_string(),
            horizon_seconds: 3600,
            remaining_tokens: Some(600),
            remaining_cost_usd: Some(6.0),
            required_dimensions: vec!["provider_id".to_string()],
            min_samples: 3,
            max_freshness_seconds: 600,
            max_duplicate_events: 1,
        }
    }

    fn observation(
        id: &str,
        minute: u32,
        tokens: i64,
        cost: Option<f64>,
    ) -> BudgetUsageObservation {
        BudgetUsageObservation {
            evidence_type: "provider_audit_event".to_string(),
            evidence_id: id.to_string(),
            content_sha256: Some(format!("{:0>64}", minute)),
            occurred_at: format!("2026-07-10T00:{minute:02}:00Z"),
            run_id: None,
            workspace_id: None,
            provider_id: Some("provider-a".to_string()),
            model_id: Some("model-a".to_string()),
            input_tokens: Some(tokens / 2),
            output_tokens: Some(tokens - tokens / 2),
            total_tokens: Some(tokens),
            cost_usd: cost,
        }
    }

    fn supported_observations() -> Vec<BudgetUsageObservation> {
        vec![
            observation("event-1", 10, 100, Some(1.0)),
            observation("event-2", 20, 200, Some(2.0)),
            observation("event-3", 30, 300, Some(3.0)),
        ]
    }

    #[test]
    fn deterministic_forecast_separates_observed_and_estimated_values() {
        let first = build_budget_forecast(&request(), &supported_observations()).unwrap();
        let mut reversed = supported_observations();
        reversed.reverse();
        let second = build_budget_forecast(&request(), &reversed).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.outcome, BudgetEvidenceOutcome::Supported);
        assert_eq!(first.observed.total_tokens, Some(600));
        assert_eq!(
            first.estimate.as_ref().unwrap().expected_total_tokens,
            Some(600.0)
        );
        assert_eq!(
            first.estimate.as_ref().unwrap().expected_cost_usd,
            Some(6.0)
        );
        assert_eq!(
            first.estimate.as_ref().unwrap().exhaustion_at.as_deref(),
            Some("2026-07-10T02:05:00Z")
        );
    }

    #[test]
    fn sparse_and_unpriced_evidence_refuse_forecasts() {
        let sparse = build_budget_forecast(&request(), &supported_observations()[..2]).unwrap();
        assert_eq!(sparse.outcome, BudgetEvidenceOutcome::InsufficientEvidence);
        assert!(sparse.estimate.is_none());
        assert!(sparse
            .reason_codes
            .contains(&"insufficient_evidence.sparse".to_string()));

        let mut unpriced = supported_observations();
        unpriced[1].cost_usd = None;
        let unpriced = build_budget_forecast(&request(), &unpriced).unwrap();
        assert_eq!(
            unpriced.outcome,
            BudgetEvidenceOutcome::InsufficientEvidence
        );
        assert!(unpriced
            .reason_codes
            .contains(&"insufficient_evidence.incomplete_pricing".to_string()));
    }

    #[test]
    fn zero_usage_is_supported_without_fabricated_exhaustion() {
        let observations = vec![
            observation("zero-1", 10, 0, Some(0.0)),
            observation("zero-2", 20, 0, Some(0.0)),
            observation("zero-3", 30, 0, Some(0.0)),
        ];
        let forecast = build_budget_forecast(&request(), &observations).unwrap();
        assert_eq!(forecast.outcome, BudgetEvidenceOutcome::Supported);
        assert_eq!(
            forecast.estimate.as_ref().unwrap().expected_total_tokens,
            Some(0.0)
        );
        assert!(forecast.estimate.as_ref().unwrap().exhaustion_at.is_none());
    }

    #[test]
    fn bursty_evidence_uses_the_explicit_window_rate() {
        let observations = vec![
            observation("burst-1", 1, 0, Some(0.0)),
            observation("burst-2", 2, 0, Some(0.0)),
            observation("burst-3", 59, 600, Some(6.0)),
        ];
        let forecast = build_budget_forecast(&request(), &observations).unwrap();
        assert_eq!(
            forecast.estimate.as_ref().unwrap().expected_total_tokens,
            Some(600.0)
        );
        assert_eq!(forecast.reason_codes, vec!["forecast.deterministic"]);
    }

    #[test]
    fn mixed_model_or_provider_evidence_is_explicitly_insufficient() {
        let mut request = request();
        request.scope = BudgetEvidenceScope {
            workspace_id: Some("workspace-a".to_string()),
            ..BudgetEvidenceScope::default()
        };
        request.required_dimensions = vec!["model_id".to_string(), "workspace_id".to_string()];
        let mut observations = supported_observations();
        for item in &mut observations {
            item.workspace_id = Some("workspace-a".to_string());
        }
        observations[2].model_id = Some("model-b".to_string());
        let forecast = build_budget_forecast(&request, &observations).unwrap();
        assert_eq!(
            forecast.outcome,
            BudgetEvidenceOutcome::InsufficientEvidence
        );
        assert!(forecast
            .reason_codes
            .contains(&"insufficient_evidence.mixed_dimensions".to_string()));
    }

    #[test]
    fn evidence_window_is_start_inclusive_and_end_exclusive() {
        let mut observations = supported_observations();
        observations.push(BudgetUsageObservation {
            occurred_at: "2026-07-10T01:00:00Z".to_string(),
            evidence_id: "excluded-end".to_string(),
            ..observation("placeholder", 40, 999, Some(9.99))
        });
        let forecast = build_budget_forecast(&request(), &observations).unwrap();
        assert_eq!(forecast.window.sample_count, 3);
        assert_eq!(forecast.observed.total_tokens, Some(600));
    }

    #[test]
    fn duplicate_and_out_of_order_evidence_is_deterministic() {
        let mut observations = supported_observations();
        observations.push(observations[1].clone());
        observations.swap(0, 2);
        let forecast = build_budget_forecast(&request(), &observations).unwrap();
        assert_eq!(forecast.outcome, BudgetEvidenceOutcome::Supported);
        assert_eq!(forecast.coverage.duplicate_events, 1);

        let mut conflicting = observations;
        conflicting.last_mut().unwrap().total_tokens = Some(999);
        let invalid = build_budget_forecast(&request(), &conflicting).unwrap();
        assert_eq!(invalid.outcome, BudgetEvidenceOutcome::InvalidEvidence);
        assert_eq!(
            invalid.reason_codes,
            vec!["invalid_evidence.conflicting_duplicate"]
        );
    }

    #[test]
    fn concurrent_reads_return_identical_forecasts() {
        let request = request();
        let observations = supported_observations();
        let handles = (0..8)
            .map(|_| {
                let request = request.clone();
                let observations = observations.clone();
                thread::spawn(move || build_budget_forecast(&request, &observations).unwrap())
            })
            .collect::<Vec<_>>();
        let forecasts = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();
        assert!(forecasts.windows(2).all(|pair| pair[0] == pair[1]));
    }

    #[test]
    fn provider_audit_adapter_does_not_invent_missing_dimensions_or_pricing() {
        let event = ProviderAuditEvent {
            schema_version: "provider_audit_event.v1".to_string(),
            event_id: "paudit-1".to_string(),
            dispatch_id: "dispatch-1".to_string(),
            provider_id: "provider-a".to_string(),
            event_type: "provider.completed".to_string(),
            input_token_count: Some(40),
            output_token_count: Some(60),
            cost: Some(0.25),
            currency: Some("EUR".to_string()),
            latency_ms: Some(20),
            error_domain: None,
            redaction_status: "redacted".to_string(),
            created_at: "2026-07-10T00:10:00Z".to_string(),
        };
        let observation = observation_from_provider_audit(&event);
        assert_eq!(observation.provider_id.as_deref(), Some("provider-a"));
        assert_eq!(observation.total_tokens, Some(100));
        assert!(observation.run_id.is_none());
        assert!(observation.model_id.is_none());
        assert!(observation.cost_usd.is_none());
    }
}
