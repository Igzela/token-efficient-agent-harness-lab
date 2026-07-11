use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

use crate::budget_manager::{
    BudgetAnomalyFinding, BudgetAnomalyKind, BudgetAnomalyMeasurement, BudgetAnomalySeverity,
    BudgetConfidence, BudgetConfidenceLevel, BudgetEvidenceCoverage, BudgetEvidenceOutcome,
    BudgetEvidenceReference, BudgetEvidenceScope, BudgetEvidenceWindow,
    BUDGET_ANOMALY_FINDING_SCHEMA_VERSION,
};

const MAX_ANOMALY_OBSERVATIONS: usize = 10_000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BudgetAnomalyObservation {
    pub evidence_type: String,
    pub evidence_id: String,
    pub content_sha256: Option<String>,
    pub occurred_at: String,
    pub run_id: Option<String>,
    pub workspace_id: Option<String>,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub total_tokens: Option<i64>,
    pub cost_usd: Option<f64>,
    pub retry_count: Option<i64>,
    pub latency_ms: Option<i64>,
    pub context_bytes: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BudgetAnomalyRequest {
    pub finding_id: String,
    pub scope: BudgetEvidenceScope,
    pub anomaly_kind: BudgetAnomalyKind,
    pub baseline_start_inclusive: String,
    pub current_start_inclusive: String,
    pub current_end_exclusive: String,
    pub generated_at: String,
    pub min_samples_per_window: u32,
    pub max_freshness_seconds: u64,
    pub max_duplicate_events: u32,
    pub required_dimensions: Vec<String>,
    pub relative_increase_threshold: f64,
    pub absolute_increase_threshold: f64,
    pub critical_increase_threshold: f64,
}

pub fn detect_budget_anomaly(
    request: &BudgetAnomalyRequest,
    observations: &[BudgetAnomalyObservation],
) -> Result<BudgetAnomalyFinding, String> {
    validate_request(request)?;
    if observations.len() > MAX_ANOMALY_OBSERVATIONS {
        return Err("anomaly observation count exceeds the bounded maximum".to_string());
    }

    let baseline_start = parse_timestamp(
        "baseline_start_inclusive",
        &request.baseline_start_inclusive,
    )?;
    let current_start =
        parse_timestamp("current_start_inclusive", &request.current_start_inclusive)?;
    let current_end = parse_timestamp("current_end_exclusive", &request.current_end_exclusive)?;
    let generated = parse_timestamp("generated_at", &request.generated_at)?;
    let freshness_seconds = (generated - current_end).num_seconds() as u64;

    let mut baseline = Vec::new();
    let mut current = Vec::new();
    for observation in observations {
        if !scope_matches(&request.scope, observation) {
            continue;
        }
        let occurred = match parse_timestamp("observation.occurred_at", &observation.occurred_at) {
            Ok(value) => value,
            Err(_) => {
                return make_finding(
                    request,
                    BudgetEvidenceOutcome::InvalidEvidence,
                    false,
                    1,
                    freshness_seconds,
                    0,
                    vec!["invalid_evidence.timestamp".to_string()],
                    evidence_references(std::iter::once(observation)),
                    observed_dimensions(std::slice::from_ref(observation)),
                    observation.cost_usd.is_some(),
                    None,
                );
            }
        };
        if occurred >= baseline_start && occurred < current_start {
            baseline.push(observation.clone());
        } else if occurred >= current_start && occurred < current_end {
            current.push(observation.clone());
        }
    }

    let baseline_sample_count = baseline.len() as u32;
    let baseline_references = evidence_references(baseline.iter());
    let baseline_observed_dimensions = observed_dimensions(&baseline);
    let baseline_pricing_complete = !baseline.is_empty()
        && baseline
            .iter()
            .all(|observation| observation.cost_usd.is_some());
    let (baseline, baseline_duplicates) = match deduplicate(baseline) {
        Ok(value) => value,
        Err(()) => {
            return make_finding(
                request,
                BudgetEvidenceOutcome::InvalidEvidence,
                false,
                baseline_sample_count,
                freshness_seconds,
                0,
                vec!["invalid_evidence.conflicting_duplicate".to_string()],
                baseline_references,
                baseline_observed_dimensions,
                baseline_pricing_complete,
                None,
            );
        }
    };

    let mut pre_dedup_combined = baseline
        .iter()
        .chain(current.iter())
        .cloned()
        .collect::<Vec<_>>();
    pre_dedup_combined.sort_by(|left, right| observation_key(left).cmp(&observation_key(right)));
    let current_sample_count = pre_dedup_combined.len() as u32;
    let current_references = evidence_references(pre_dedup_combined.iter());
    let current_observed_dimensions = observed_dimensions(&pre_dedup_combined);
    let current_pricing_complete = !pre_dedup_combined.is_empty()
        && pre_dedup_combined
            .iter()
            .all(|observation| observation.cost_usd.is_some());
    let (current, current_duplicates) = match deduplicate(current) {
        Ok(value) => value,
        Err(()) => {
            return make_finding(
                request,
                BudgetEvidenceOutcome::InvalidEvidence,
                false,
                current_sample_count,
                freshness_seconds,
                baseline_duplicates,
                vec!["invalid_evidence.conflicting_duplicate".to_string()],
                current_references,
                current_observed_dimensions,
                current_pricing_complete,
                None,
            );
        }
    };
    let duplicate_events = baseline_duplicates.saturating_add(current_duplicates);
    let mut combined = baseline
        .iter()
        .chain(current.iter())
        .cloned()
        .collect::<Vec<_>>();
    combined.sort_by(|left, right| observation_key(left).cmp(&observation_key(right)));

    let observed_dimensions = observed_dimensions(&combined);
    let pricing_complete = !combined.is_empty()
        && combined
            .iter()
            .all(|observation| observation.cost_usd.is_some());
    if let Some(reason) = invalid_observation_reason(&combined) {
        return make_finding(
            request,
            BudgetEvidenceOutcome::InvalidEvidence,
            false,
            combined.len() as u32,
            freshness_seconds,
            duplicate_events,
            vec![reason],
            evidence_references(combined.iter()),
            observed_dimensions,
            pricing_complete,
            None,
        );
    }

    let mut missing_fields =
        missing_required_dimensions(&request.required_dimensions, &observed_dimensions);
    let mixed = mixed_required_dimensions(&request.scope, &request.required_dimensions, &combined);
    missing_fields.extend(mixed.iter().map(|dimension| format!("{dimension}.mixed")));

    let metric_complete = metric_complete(&request.anomaly_kind, &baseline)
        && metric_complete(&request.anomaly_kind, &current);
    if !metric_complete {
        missing_fields.push(metric_name(&request.anomaly_kind).to_string());
    }
    if matches!(request.anomaly_kind, BudgetAnomalyKind::ModelMixShift)
        && combined
            .iter()
            .any(|observation| observation.model_id.is_none())
    {
        missing_fields.push("model_id".to_string());
    }
    missing_fields.sort();
    missing_fields.dedup();

    let mut insufficiency_reasons = Vec::new();
    if baseline.len() < request.min_samples_per_window as usize
        || current.len() < request.min_samples_per_window as usize
    {
        insufficiency_reasons.push("insufficient_evidence.sparse".to_string());
    }
    if freshness_seconds > request.max_freshness_seconds {
        insufficiency_reasons.push("insufficient_evidence.stale".to_string());
    }
    if duplicate_events > request.max_duplicate_events {
        insufficiency_reasons.push("insufficient_evidence.duplicate_limit".to_string());
    }
    if !mixed.is_empty() {
        insufficiency_reasons.push("insufficient_evidence.mixed_dimensions".to_string());
    }
    if !missing_fields.is_empty() {
        insufficiency_reasons.push("insufficient_evidence.missing_fields".to_string());
    }
    if matches!(request.anomaly_kind, BudgetAnomalyKind::CostSpike) && !pricing_complete {
        insufficiency_reasons.push("insufficient_evidence.incomplete_pricing".to_string());
    }
    insufficiency_reasons.sort();
    insufficiency_reasons.dedup();

    let references = evidence_references(combined.iter());
    if !insufficiency_reasons.is_empty() {
        return make_finding(
            request,
            BudgetEvidenceOutcome::InsufficientEvidence,
            false,
            combined.len() as u32,
            freshness_seconds,
            duplicate_events,
            insufficiency_reasons,
            references,
            observed_dimensions,
            pricing_complete,
            None,
        );
    }

    let (baseline_value, current_value) =
        metric_values(&request.anomaly_kind, &baseline, &current)?;
    let delta = current_value - baseline_value;
    let (normalized_delta, threshold) =
        if matches!(request.anomaly_kind, BudgetAnomalyKind::ModelMixShift) {
            (current_value, request.relative_increase_threshold)
        } else if baseline_value > 0.0 {
            (delta / baseline_value, request.relative_increase_threshold)
        } else {
            (delta, request.absolute_increase_threshold)
        };
    let detected = normalized_delta > threshold;
    let severity = detected.then(|| {
        if normalized_delta > request.critical_increase_threshold {
            BudgetAnomalySeverity::Critical
        } else {
            BudgetAnomalySeverity::Warning
        }
    });
    let measurement = detected.then(|| BudgetAnomalyMeasurement {
        metric: metric_name(&request.anomaly_kind).to_string(),
        observed: current_value,
        baseline: baseline_value,
        threshold,
        normalized_delta,
    });
    let reason_codes = if detected {
        vec![format!("anomaly.{}", metric_name(&request.anomaly_kind))]
    } else {
        vec!["anomaly.none".to_string()]
    };

    make_finding_with_details(
        request,
        BudgetEvidenceOutcome::Supported,
        detected,
        combined.len() as u32,
        freshness_seconds,
        duplicate_events,
        reason_codes,
        references,
        observed_dimensions,
        pricing_complete,
        missing_fields,
        severity,
        measurement,
    )
}

fn validate_request(request: &BudgetAnomalyRequest) -> Result<(), String> {
    let baseline_start = parse_timestamp(
        "baseline_start_inclusive",
        &request.baseline_start_inclusive,
    )?;
    let current_start =
        parse_timestamp("current_start_inclusive", &request.current_start_inclusive)?;
    let current_end = parse_timestamp("current_end_exclusive", &request.current_end_exclusive)?;
    let generated = parse_timestamp("generated_at", &request.generated_at)?;
    if baseline_start >= current_start || current_start >= current_end {
        return Err("anomaly windows must be ordered and non-empty".to_string());
    }
    if current_end > generated {
        return Err("anomaly current window cannot end after generation time".to_string());
    }
    if current_start - baseline_start != current_end - current_start {
        return Err("anomaly baseline and current windows must have equal duration".to_string());
    }
    if request.min_samples_per_window < 3 {
        return Err("anomaly min_samples_per_window must be at least 3".to_string());
    }
    for (name, value) in [
        (
            "relative_increase_threshold",
            request.relative_increase_threshold,
        ),
        (
            "absolute_increase_threshold",
            request.absolute_increase_threshold,
        ),
        (
            "critical_increase_threshold",
            request.critical_increase_threshold,
        ),
    ] {
        if !value.is_finite() || value < 0.0 {
            return Err(format!("{name} must be finite and non-negative"));
        }
    }
    if request.critical_increase_threshold < request.relative_increase_threshold {
        return Err("critical threshold must not be below the relative threshold".to_string());
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

fn deduplicate(
    mut observations: Vec<BudgetAnomalyObservation>,
) -> Result<(Vec<BudgetAnomalyObservation>, u32), ()> {
    observations.sort_by(|left, right| observation_key(left).cmp(&observation_key(right)));
    let mut deduplicated = BTreeMap::new();
    let mut duplicate_events = 0_u32;
    for observation in observations {
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
            Some(_) => return Err(()),
        }
    }
    Ok((deduplicated.into_values().collect(), duplicate_events))
}

fn scope_matches(scope: &BudgetEvidenceScope, observation: &BudgetAnomalyObservation) -> bool {
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

fn observation_key(observation: &BudgetAnomalyObservation) -> (&str, &str, &str) {
    (
        observation.occurred_at.as_str(),
        observation.evidence_type.as_str(),
        observation.evidence_id.as_str(),
    )
}

fn invalid_observation_reason(observations: &[BudgetAnomalyObservation]) -> Option<String> {
    for observation in observations {
        for value in [
            observation.total_tokens,
            observation.retry_count,
            observation.latency_ms,
            observation.context_bytes,
        ] {
            if value.is_some_and(|value| value < 0) {
                return Some("invalid_evidence.negative_metric".to_string());
            }
        }
        if observation
            .cost_usd
            .is_some_and(|value| !value.is_finite() || value < 0.0)
        {
            return Some("invalid_evidence.invalid_cost".to_string());
        }
    }
    None
}

fn metric_complete(kind: &BudgetAnomalyKind, observations: &[BudgetAnomalyObservation]) -> bool {
    !observations.is_empty()
        && observations.iter().all(|observation| match kind {
            BudgetAnomalyKind::CostSpike => observation.cost_usd.is_some(),
            BudgetAnomalyKind::TokenSpike => observation.total_tokens.is_some(),
            BudgetAnomalyKind::RetrySpike => observation.retry_count.is_some(),
            BudgetAnomalyKind::LatencySpike => observation.latency_ms.is_some(),
            BudgetAnomalyKind::ContextGrowth => observation.context_bytes.is_some(),
            BudgetAnomalyKind::ModelMixShift => observation.model_id.is_some(),
        })
}

fn metric_values(
    kind: &BudgetAnomalyKind,
    baseline: &[BudgetAnomalyObservation],
    current: &[BudgetAnomalyObservation],
) -> Result<(f64, f64), String> {
    if matches!(kind, BudgetAnomalyKind::ModelMixShift) {
        return Ok((0.0, maximum_model_share_shift(baseline, current)?));
    }
    let baseline_value = aggregate_metric(kind, baseline)?;
    let current_value = aggregate_metric(kind, current)?;
    Ok((baseline_value, current_value))
}

fn aggregate_metric(
    kind: &BudgetAnomalyKind,
    observations: &[BudgetAnomalyObservation],
) -> Result<f64, String> {
    let values = observations
        .iter()
        .map(|observation| match kind {
            BudgetAnomalyKind::CostSpike => observation.cost_usd,
            BudgetAnomalyKind::TokenSpike => observation.total_tokens.map(|value| value as f64),
            BudgetAnomalyKind::RetrySpike => observation.retry_count.map(|value| value as f64),
            BudgetAnomalyKind::LatencySpike => observation.latency_ms.map(|value| value as f64),
            BudgetAnomalyKind::ContextGrowth => observation.context_bytes.map(|value| value as f64),
            BudgetAnomalyKind::ModelMixShift => None,
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| "anomaly metric evidence is incomplete".to_string())?;
    if matches!(
        kind,
        BudgetAnomalyKind::CostSpike
            | BudgetAnomalyKind::TokenSpike
            | BudgetAnomalyKind::RetrySpike
    ) {
        Ok(values.iter().sum())
    } else {
        Ok(values.iter().sum::<f64>() / values.len() as f64)
    }
}

fn maximum_model_share_shift(
    baseline: &[BudgetAnomalyObservation],
    current: &[BudgetAnomalyObservation],
) -> Result<f64, String> {
    let baseline_counts = model_counts(baseline)?;
    let current_counts = model_counts(current)?;
    let models = baseline_counts
        .keys()
        .chain(current_counts.keys())
        .collect::<BTreeSet<_>>();
    let baseline_total = baseline.len() as f64;
    let current_total = current.len() as f64;
    Ok(models
        .into_iter()
        .map(|model| {
            let baseline_share = *baseline_counts.get(model).unwrap_or(&0) as f64 / baseline_total;
            let current_share = *current_counts.get(model).unwrap_or(&0) as f64 / current_total;
            (current_share - baseline_share).abs()
        })
        .fold(0.0, f64::max))
}

fn model_counts(
    observations: &[BudgetAnomalyObservation],
) -> Result<BTreeMap<String, usize>, String> {
    let mut counts = BTreeMap::new();
    for observation in observations {
        let model = observation
            .model_id
            .as_ref()
            .ok_or_else(|| "model mix evidence is incomplete".to_string())?;
        *counts.entry(model.clone()).or_insert(0) += 1;
    }
    Ok(counts)
}

fn metric_name(kind: &BudgetAnomalyKind) -> &'static str {
    match kind {
        BudgetAnomalyKind::CostSpike => "cost_spike",
        BudgetAnomalyKind::TokenSpike => "token_spike",
        BudgetAnomalyKind::RetrySpike => "retry_spike",
        BudgetAnomalyKind::LatencySpike => "latency_spike",
        BudgetAnomalyKind::ContextGrowth => "context_growth",
        BudgetAnomalyKind::ModelMixShift => "model_mix_shift",
    }
}

fn observed_dimensions(observations: &[BudgetAnomalyObservation]) -> Vec<String> {
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

fn missing_required_dimensions(
    required_dimensions: &[String],
    observed_dimensions: &[String],
) -> Vec<String> {
    let mut missing = required_dimensions
        .iter()
        .filter(|dimension| !observed_dimensions.contains(*dimension))
        .cloned()
        .collect::<Vec<_>>();
    missing.sort();
    missing.dedup();
    missing
}

fn mixed_required_dimensions(
    scope: &BudgetEvidenceScope,
    required_dimensions: &[String],
    observations: &[BudgetAnomalyObservation],
) -> Vec<String> {
    let mut mixed = Vec::new();
    if required_dimensions.iter().any(|value| value == "run_id")
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
        .any(|value| value == "workspace_id")
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
        .any(|value| value == "provider_id")
        && scope.provider_id.is_none()
        && distinct_count(
            observations
                .iter()
                .filter_map(|item| item.provider_id.as_deref()),
        ) > 1
    {
        mixed.push("provider_id".to_string());
    }
    if required_dimensions.iter().any(|value| value == "model_id")
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

fn evidence_references<'a>(
    observations: impl Iterator<Item = &'a BudgetAnomalyObservation>,
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

#[allow(clippy::too_many_arguments)]
fn make_finding(
    request: &BudgetAnomalyRequest,
    outcome: BudgetEvidenceOutcome,
    detected: bool,
    sample_count: u32,
    freshness_seconds: u64,
    duplicate_events: u32,
    reason_codes: Vec<String>,
    references: Vec<BudgetEvidenceReference>,
    observed_dimensions: Vec<String>,
    pricing_complete: bool,
    measurement: Option<BudgetAnomalyMeasurement>,
) -> Result<BudgetAnomalyFinding, String> {
    let missing_fields =
        missing_required_dimensions(&request.required_dimensions, &observed_dimensions);
    make_finding_with_details(
        request,
        outcome,
        detected,
        sample_count,
        freshness_seconds,
        duplicate_events,
        reason_codes,
        references,
        observed_dimensions,
        pricing_complete,
        missing_fields,
        None,
        measurement,
    )
}

#[allow(clippy::too_many_arguments)]
fn make_finding_with_details(
    request: &BudgetAnomalyRequest,
    outcome: BudgetEvidenceOutcome,
    detected: bool,
    sample_count: u32,
    freshness_seconds: u64,
    duplicate_events: u32,
    mut reason_codes: Vec<String>,
    references: Vec<BudgetEvidenceReference>,
    mut observed_dimensions: Vec<String>,
    pricing_complete: bool,
    mut missing_fields: Vec<String>,
    severity: Option<BudgetAnomalySeverity>,
    measurement: Option<BudgetAnomalyMeasurement>,
) -> Result<BudgetAnomalyFinding, String> {
    reason_codes.sort();
    reason_codes.dedup();
    observed_dimensions.sort();
    observed_dimensions.dedup();
    missing_fields.sort();
    missing_fields.dedup();
    let mut finding = BudgetAnomalyFinding {
        schema_version: BUDGET_ANOMALY_FINDING_SCHEMA_VERSION.to_string(),
        finding_id: request.finding_id.clone(),
        scope: request.scope.clone(),
        outcome: outcome.clone(),
        window: BudgetEvidenceWindow {
            start_inclusive: request.baseline_start_inclusive.clone(),
            end_exclusive: request.current_end_exclusive.clone(),
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
        confidence: confidence(&outcome, sample_count, duplicate_events),
        reason_codes,
        evidence_references: references,
        detected,
        anomaly_kind: detected.then(|| request.anomaly_kind.clone()),
        severity,
        measurement,
        evidence_sha256: String::new(),
    };
    finding.seal()?;
    finding.validate()?;
    Ok(finding)
}

fn confidence(
    outcome: &BudgetEvidenceOutcome,
    sample_count: u32,
    duplicate_events: u32,
) -> BudgetConfidence {
    match outcome {
        BudgetEvidenceOutcome::Supported if sample_count >= 20 && duplicate_events == 0 => {
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

fn parse_timestamp(name: &str, value: &str) -> Result<DateTime<FixedOffset>, String> {
    DateTime::parse_from_rfc3339(value).map_err(|_| format!("{name} must be RFC3339"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(kind: BudgetAnomalyKind) -> BudgetAnomalyRequest {
        BudgetAnomalyRequest {
            finding_id: format!("finding-{}", metric_name(&kind)),
            scope: BudgetEvidenceScope {
                provider_id: Some("provider-a".to_string()),
                ..BudgetEvidenceScope::default()
            },
            anomaly_kind: kind,
            baseline_start_inclusive: "2026-07-10T00:00:00Z".to_string(),
            current_start_inclusive: "2026-07-10T01:00:00Z".to_string(),
            current_end_exclusive: "2026-07-10T02:00:00Z".to_string(),
            generated_at: "2026-07-10T02:05:00Z".to_string(),
            min_samples_per_window: 3,
            max_freshness_seconds: 600,
            max_duplicate_events: 1,
            required_dimensions: vec!["provider_id".to_string()],
            relative_increase_threshold: 0.5,
            absolute_increase_threshold: 10.0,
            critical_increase_threshold: 1.0,
        }
    }

    fn observation(id: &str, hour: u32, minute: u32, value: i64) -> BudgetAnomalyObservation {
        BudgetAnomalyObservation {
            evidence_type: "provider_audit_event".to_string(),
            evidence_id: id.to_string(),
            content_sha256: Some(format!("{:0>64}", hour * 60 + minute)),
            occurred_at: format!("2026-07-10T{hour:02}:{minute:02}:00Z"),
            run_id: None,
            workspace_id: None,
            provider_id: Some("provider-a".to_string()),
            model_id: Some("model-a".to_string()),
            total_tokens: Some(value),
            cost_usd: Some(value as f64 / 100.0),
            retry_count: Some(value),
            latency_ms: Some(value),
            context_bytes: Some(value),
        }
    }

    fn paired(baseline: [i64; 3], current: [i64; 3]) -> Vec<BudgetAnomalyObservation> {
        baseline
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                observation(&format!("baseline-{index}"), 0, 10 + index as u32, value)
            })
            .chain(current.into_iter().enumerate().map(|(index, value)| {
                observation(&format!("current-{index}"), 1, 10 + index as u32, value)
            }))
            .collect()
    }

    #[test]
    fn normal_evidence_is_supported_without_a_finding() {
        let finding = detect_budget_anomaly(
            &request(BudgetAnomalyKind::TokenSpike),
            &paired([100, 100, 100], [110, 110, 110]),
        )
        .unwrap();
        assert_eq!(finding.outcome, BudgetEvidenceOutcome::Supported);
        assert!(!finding.detected);
        assert_eq!(finding.reason_codes, vec!["anomaly.none"]);
        assert!(finding.measurement.is_none());
    }

    #[test]
    fn explicit_rules_detect_each_numeric_anomaly_kind() {
        for kind in [
            BudgetAnomalyKind::CostSpike,
            BudgetAnomalyKind::TokenSpike,
            BudgetAnomalyKind::RetrySpike,
            BudgetAnomalyKind::LatencySpike,
            BudgetAnomalyKind::ContextGrowth,
        ] {
            let finding =
                detect_budget_anomaly(&request(kind.clone()), &paired([10, 10, 10], [30, 30, 30]))
                    .unwrap();
            assert!(finding.detected, "{kind:?}");
            assert_eq!(finding.anomaly_kind, Some(kind));
            assert_eq!(finding.severity, Some(BudgetAnomalySeverity::Critical));
        }
    }

    #[test]
    fn gradual_drift_is_explainable_and_warning_bounded() {
        let finding = detect_budget_anomaly(
            &request(BudgetAnomalyKind::LatencySpike),
            &paired([100, 100, 100], [160, 160, 160]),
        )
        .unwrap();
        assert!(finding.detected);
        assert_eq!(finding.severity, Some(BudgetAnomalySeverity::Warning));
        let measurement = finding.measurement.unwrap();
        assert_eq!(measurement.baseline, 100.0);
        assert_eq!(measurement.observed, 160.0);
        assert!((measurement.normalized_delta - 0.6).abs() < 1e-9);
    }

    #[test]
    fn threshold_equality_does_not_create_a_false_positive() {
        let finding = detect_budget_anomaly(
            &request(BudgetAnomalyKind::TokenSpike),
            &paired([100, 100, 100], [150, 150, 150]),
        )
        .unwrap();
        assert!(!finding.detected);
    }

    #[test]
    fn sparse_and_incomplete_pricing_are_explicitly_insufficient() {
        let sparse = detect_budget_anomaly(
            &request(BudgetAnomalyKind::TokenSpike),
            &paired([100, 100, 100], [300, 300, 300])[..5],
        )
        .unwrap();
        assert_eq!(sparse.outcome, BudgetEvidenceOutcome::InsufficientEvidence);
        assert!(!sparse.detected);
        assert!(sparse
            .coverage
            .observed_dimensions
            .contains(&"provider_id".to_string()));
        assert!(sparse.coverage.missing_fields.is_empty());
        assert_eq!(sparse.reason_codes, vec!["insufficient_evidence.sparse"]);

        let mut observations = paired([100, 100, 100], [300, 300, 300]);
        observations[4].cost_usd = None;
        let unpriced =
            detect_budget_anomaly(&request(BudgetAnomalyKind::CostSpike), &observations).unwrap();
        assert_eq!(
            unpriced.outcome,
            BudgetEvidenceOutcome::InsufficientEvidence
        );
        assert!(unpriced
            .reason_codes
            .contains(&"insufficient_evidence.incomplete_pricing".to_string()));
    }

    #[test]
    fn model_mix_shift_is_distribution_based_and_explainable() {
        let mut observations = paired([10, 10, 10], [10, 10, 10]);
        observations[4].model_id = Some("model-b".to_string());
        observations[5].model_id = Some("model-b".to_string());
        let mut request = request(BudgetAnomalyKind::ModelMixShift);
        request.relative_increase_threshold = 0.5;
        request.critical_increase_threshold = 0.9;
        let finding = detect_budget_anomaly(&request, &observations).unwrap();
        assert!(finding.detected);
        assert_eq!(finding.severity, Some(BudgetAnomalySeverity::Warning));
        assert!((finding.measurement.unwrap().observed - (2.0 / 3.0)).abs() < 1e-9);
    }

    #[test]
    fn ordering_and_identical_duplicates_do_not_change_recomputation() {
        let request = request(BudgetAnomalyKind::TokenSpike);
        let mut observations = paired([100, 100, 100], [300, 300, 300]);
        let expected = detect_budget_anomaly(&request, &observations).unwrap();
        observations.push(observations[2].clone());
        observations.reverse();
        let actual = detect_budget_anomaly(&request, &observations).unwrap();
        assert_eq!(actual.measurement, expected.measurement);
        assert_eq!(actual.reason_codes, expected.reason_codes);
        assert_eq!(actual.coverage.duplicate_events, 1);
    }

    #[test]
    fn mixed_required_workloads_are_insufficient() {
        let mut observations = paired([100, 100, 100], [300, 300, 300]);
        observations[5].provider_id = Some("provider-b".to_string());
        let mut request = request(BudgetAnomalyKind::TokenSpike);
        request.scope = BudgetEvidenceScope {
            workspace_id: Some("workspace-a".to_string()),
            ..BudgetEvidenceScope::default()
        };
        request.required_dimensions = vec!["provider_id".to_string(), "workspace_id".to_string()];
        for observation in &mut observations {
            observation.workspace_id = Some("workspace-a".to_string());
        }
        let finding = detect_budget_anomaly(&request, &observations).unwrap();
        assert_eq!(finding.outcome, BudgetEvidenceOutcome::InsufficientEvidence);
        assert!(finding
            .reason_codes
            .contains(&"insufficient_evidence.mixed_dimensions".to_string()));
    }

    #[test]
    fn missing_fields_only_reports_unobserved_required_dimensions() {
        let observations = paired([100, 100, 100], [300, 300, 300]);
        let mut request = request(BudgetAnomalyKind::TokenSpike);
        request.required_dimensions = vec!["provider_id".to_string(), "workspace_id".to_string()];
        let finding = detect_budget_anomaly(&request, &observations).unwrap();
        assert_eq!(finding.outcome, BudgetEvidenceOutcome::InsufficientEvidence);
        assert!(finding
            .coverage
            .observed_dimensions
            .contains(&"provider_id".to_string()));
        assert_eq!(finding.coverage.missing_fields, vec!["workspace_id"]);
    }

    #[test]
    fn invalid_metric_preserves_filtered_dimension_coverage() {
        let mut observations = paired([100, 100, 100], [300, 300, 300]);
        observations[0].total_tokens = Some(-1);
        let finding =
            detect_budget_anomaly(&request(BudgetAnomalyKind::TokenSpike), &observations).unwrap();
        assert_eq!(finding.outcome, BudgetEvidenceOutcome::InvalidEvidence);
        assert_eq!(
            finding.reason_codes,
            vec!["invalid_evidence.negative_metric"]
        );
        assert!(finding
            .coverage
            .observed_dimensions
            .contains(&"provider_id".to_string()));
        assert!(finding.coverage.missing_fields.is_empty());
        assert_eq!(finding.evidence_references.len(), observations.len());
    }

    #[test]
    fn conflicting_duplicates_fail_closed() {
        let request = request(BudgetAnomalyKind::TokenSpike);
        let mut observations = paired([100, 100, 100], [300, 300, 300]);
        let mut conflicting = observations[1].clone();
        conflicting.total_tokens = Some(999);
        observations.push(conflicting);
        let finding = detect_budget_anomaly(&request, &observations).unwrap();
        assert_eq!(finding.outcome, BudgetEvidenceOutcome::InvalidEvidence);
        assert_eq!(
            finding.reason_codes,
            vec!["invalid_evidence.conflicting_duplicate"]
        );
        assert!(!finding.detected);
        assert!(finding
            .coverage
            .observed_dimensions
            .contains(&"provider_id".to_string()));
        assert!(finding.coverage.missing_fields.is_empty());
    }
}
