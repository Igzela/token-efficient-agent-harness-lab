use std::collections::BTreeSet;

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::dispatch_decision::BudgetReservation;
use crate::runtime::FixtureRuntime;
use crate::task_analyzer::TaskAnalysis;

pub const BUDGET_FORECAST_EVIDENCE_SCHEMA_VERSION: &str = "budget_forecast_evidence.v1";
pub const BUDGET_ANOMALY_FINDING_SCHEMA_VERSION: &str = "budget_anomaly_finding.v1";

const MAX_CONTRACT_BYTES: usize = 64 * 1024;
const MAX_WINDOW_SECONDS: i64 = 30 * 24 * 60 * 60;
const MAX_SAMPLE_COUNT: u32 = 10_000;
const MAX_REASON_CODES: usize = 32;
const MAX_EVIDENCE_REFERENCES: usize = 64;
const MAX_IDENTIFIER_BYTES: usize = 160;
const ALLOWED_DIMENSIONS: [&str; 4] = ["model_id", "provider_id", "run_id", "workspace_id"];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BudgetEvidenceOutcome {
    Supported,
    InsufficientEvidence,
    InvalidEvidence,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BudgetConfidenceLevel {
    Low,
    Medium,
    High,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BudgetEvidenceScope {
    pub run_id: Option<String>,
    pub workspace_id: Option<String>,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BudgetEvidenceWindow {
    pub start_inclusive: String,
    pub end_exclusive: String,
    pub generated_at: String,
    pub freshness_seconds: u64,
    pub sample_count: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BudgetEvidenceCoverage {
    pub required_dimensions: Vec<String>,
    pub observed_dimensions: Vec<String>,
    pub pricing_complete: bool,
    pub duplicate_events: u32,
    pub missing_fields: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BudgetConfidence {
    pub level: BudgetConfidenceLevel,
    pub score: f64,
    pub reason_codes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BudgetEvidenceReference {
    pub evidence_type: String,
    pub evidence_id: String,
    pub content_sha256: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BudgetObservedUsage {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub cost_usd: Option<f64>,
    pub latency_ms: Option<i64>,
    pub retry_count: Option<i64>,
    pub context_bytes: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BudgetForecastEstimate {
    pub expected_total_tokens: Option<f64>,
    pub expected_cost_usd: Option<f64>,
    pub exhaustion_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BudgetForecastEvidence {
    pub schema_version: String,
    pub forecast_id: String,
    pub scope: BudgetEvidenceScope,
    pub outcome: BudgetEvidenceOutcome,
    pub window: BudgetEvidenceWindow,
    pub coverage: BudgetEvidenceCoverage,
    pub confidence: BudgetConfidence,
    pub reason_codes: Vec<String>,
    pub evidence_references: Vec<BudgetEvidenceReference>,
    pub observed: BudgetObservedUsage,
    pub estimate: Option<BudgetForecastEstimate>,
    pub assumptions: Vec<String>,
    pub evidence_sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BudgetAnomalyKind {
    CostSpike,
    TokenSpike,
    RetrySpike,
    LatencySpike,
    ContextGrowth,
    ModelMixShift,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BudgetAnomalySeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BudgetAnomalyMeasurement {
    pub metric: String,
    pub observed: f64,
    pub baseline: f64,
    pub threshold: f64,
    pub normalized_delta: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BudgetAnomalyFinding {
    pub schema_version: String,
    pub finding_id: String,
    pub scope: BudgetEvidenceScope,
    pub outcome: BudgetEvidenceOutcome,
    pub window: BudgetEvidenceWindow,
    pub coverage: BudgetEvidenceCoverage,
    pub confidence: BudgetConfidence,
    pub reason_codes: Vec<String>,
    pub evidence_references: Vec<BudgetEvidenceReference>,
    pub anomaly_kind: Option<BudgetAnomalyKind>,
    pub severity: Option<BudgetAnomalySeverity>,
    pub measurement: Option<BudgetAnomalyMeasurement>,
    pub evidence_sha256: String,
}

impl BudgetForecastEvidence {
    pub fn seal(&mut self) -> Result<(), String> {
        self.evidence_sha256 = canonical_hash(self)?;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), String> {
        validate_common(
            &self.schema_version,
            BUDGET_FORECAST_EVIDENCE_SCHEMA_VERSION,
            &self.forecast_id,
            &self.scope,
            &self.outcome,
            &self.window,
            &self.coverage,
            &self.confidence,
            &self.reason_codes,
            &self.evidence_references,
            &self.evidence_sha256,
            self,
        )?;
        validate_observed(&self.observed)?;
        validate_strings("assumptions", &self.assumptions, MAX_REASON_CODES)?;
        match self.outcome {
            BudgetEvidenceOutcome::Supported => {
                if !self.coverage.pricing_complete {
                    return Err("supported forecast requires complete pricing".to_string());
                }
                let estimate = self
                    .estimate
                    .as_ref()
                    .ok_or_else(|| "supported forecast requires an estimate".to_string())?;
                validate_forecast_estimate(estimate)?;
            }
            BudgetEvidenceOutcome::InsufficientEvidence | BudgetEvidenceOutcome::InvalidEvidence => {
                if self.estimate.is_some() {
                    return Err("unsupported forecast outcome must not contain estimates".to_string());
                }
            }
        }
        Ok(())
    }
}

impl BudgetAnomalyFinding {
    pub fn seal(&mut self) -> Result<(), String> {
        self.evidence_sha256 = canonical_hash(self)?;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), String> {
        validate_common(
            &self.schema_version,
            BUDGET_ANOMALY_FINDING_SCHEMA_VERSION,
            &self.finding_id,
            &self.scope,
            &self.outcome,
            &self.window,
            &self.coverage,
            &self.confidence,
            &self.reason_codes,
            &self.evidence_references,
            &self.evidence_sha256,
            self,
        )?;
        match self.outcome {
            BudgetEvidenceOutcome::Supported => {
                let kind = self
                    .anomaly_kind
                    .as_ref()
                    .ok_or_else(|| "supported anomaly requires a kind".to_string())?;
                if self.severity.is_none() || self.measurement.is_none() {
                    return Err("supported anomaly requires severity and measurement".to_string());
                }
                if matches!(kind, BudgetAnomalyKind::CostSpike)
                    && !self.coverage.pricing_complete
                {
                    return Err("cost anomaly requires complete pricing".to_string());
                }
                validate_measurement(self.measurement.as_ref().expect("checked above"))?;
            }
            BudgetEvidenceOutcome::InsufficientEvidence | BudgetEvidenceOutcome::InvalidEvidence => {
                if self.anomaly_kind.is_some()
                    || self.severity.is_some()
                    || self.measurement.is_some()
                {
                    return Err(
                        "unsupported anomaly outcome must not contain a finding".to_string(),
                    );
                }
            }
        }
        Ok(())
    }
}

fn validate_common<T: Serialize>(
    schema_version: &str,
    expected_schema: &str,
    artifact_id: &str,
    scope: &BudgetEvidenceScope,
    outcome: &BudgetEvidenceOutcome,
    window: &BudgetEvidenceWindow,
    coverage: &BudgetEvidenceCoverage,
    confidence: &BudgetConfidence,
    reason_codes: &[String],
    evidence_references: &[BudgetEvidenceReference],
    evidence_sha256: &str,
    value: &T,
) -> Result<(), String> {
    if schema_version != expected_schema {
        return Err(format!("unsupported schema version: {schema_version}"));
    }
    validate_identifier("artifact id", artifact_id)?;
    validate_scope(scope)?;
    validate_window(window)?;
    validate_coverage(coverage)?;
    validate_confidence(confidence)?;
    validate_reason_codes("reason_codes", reason_codes)?;
    validate_evidence_references(evidence_references)?;
    if matches!(outcome, BudgetEvidenceOutcome::Supported) {
        if window.sample_count < 3 {
            return Err("supported evidence requires at least 3 samples".to_string());
        }
        if !coverage.missing_fields.is_empty() {
            return Err("supported evidence cannot have missing fields".to_string());
        }
        let observed = coverage
            .observed_dimensions
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        if coverage
            .required_dimensions
            .iter()
            .any(|dimension| !observed.contains(dimension.as_str()))
        {
            return Err("supported evidence is missing required dimensions".to_string());
        }
        if evidence_references.is_empty() {
            return Err("supported evidence requires evidence references".to_string());
        }
    } else if reason_codes.is_empty() {
        return Err("unsupported evidence outcome requires reason codes".to_string());
    }
    let encoded = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    if encoded.len() > MAX_CONTRACT_BYTES {
        return Err("budget evidence exceeds the bounded contract size".to_string());
    }
    validate_hash(evidence_sha256)?;
    if canonical_hash(value)? != evidence_sha256 {
        return Err("budget evidence hash mismatch".to_string());
    }
    Ok(())
}

fn validate_scope(scope: &BudgetEvidenceScope) -> Result<(), String> {
    let values = [
        ("run_id", scope.run_id.as_deref()),
        ("workspace_id", scope.workspace_id.as_deref()),
        ("provider_id", scope.provider_id.as_deref()),
        ("model_id", scope.model_id.as_deref()),
    ];
    if values.iter().all(|(_, value)| value.is_none()) {
        return Err("budget evidence requires at least one scope dimension".to_string());
    }
    for (name, value) in values {
        if let Some(value) = value {
            validate_identifier(name, value)?;
        }
    }
    Ok(())
}

fn validate_window(window: &BudgetEvidenceWindow) -> Result<(), String> {
    let start = parse_timestamp("start_inclusive", &window.start_inclusive)?;
    let end = parse_timestamp("end_exclusive", &window.end_exclusive)?;
    let generated = parse_timestamp("generated_at", &window.generated_at)?;
    if start >= end {
        return Err("evidence window must have start before end".to_string());
    }
    if end > generated {
        return Err("evidence window cannot end after generation time".to_string());
    }
    if (end - start).num_seconds() > MAX_WINDOW_SECONDS {
        return Err("evidence window exceeds 30 days".to_string());
    }
    let expected_freshness = (generated - end).num_seconds() as u64;
    if window.freshness_seconds != expected_freshness {
        return Err("freshness_seconds does not match the evidence window".to_string());
    }
    if window.sample_count > MAX_SAMPLE_COUNT {
        return Err("sample_count exceeds the bounded maximum".to_string());
    }
    Ok(())
}

fn validate_coverage(coverage: &BudgetEvidenceCoverage) -> Result<(), String> {
    validate_sorted_unique(
        "required_dimensions",
        &coverage.required_dimensions,
        ALLOWED_DIMENSIONS.len(),
    )?;
    validate_sorted_unique(
        "observed_dimensions",
        &coverage.observed_dimensions,
        ALLOWED_DIMENSIONS.len(),
    )?;
    for dimension in coverage
        .required_dimensions
        .iter()
        .chain(coverage.observed_dimensions.iter())
    {
        if !ALLOWED_DIMENSIONS.contains(&dimension.as_str()) {
            return Err(format!("unsupported evidence dimension: {dimension}"));
        }
    }
    validate_reason_codes("missing_fields", &coverage.missing_fields)?;
    if coverage.duplicate_events > MAX_SAMPLE_COUNT {
        return Err("duplicate event count exceeds the bounded maximum".to_string());
    }
    Ok(())
}

fn validate_confidence(confidence: &BudgetConfidence) -> Result<(), String> {
    if !confidence.score.is_finite() || !(0.0..=1.0).contains(&confidence.score) {
        return Err("confidence score must be finite and between 0 and 1".to_string());
    }
    let valid_level = match confidence.level {
        BudgetConfidenceLevel::Low => confidence.score < 0.5,
        BudgetConfidenceLevel::Medium => (0.5..0.8).contains(&confidence.score),
        BudgetConfidenceLevel::High => confidence.score >= 0.8,
    };
    if !valid_level {
        return Err("confidence level does not match confidence score".to_string());
    }
    validate_reason_codes("confidence.reason_codes", &confidence.reason_codes)
}

fn validate_observed(observed: &BudgetObservedUsage) -> Result<(), String> {
    for (name, value) in [
        ("input_tokens", observed.input_tokens),
        ("output_tokens", observed.output_tokens),
        ("total_tokens", observed.total_tokens),
        ("latency_ms", observed.latency_ms),
        ("retry_count", observed.retry_count),
        ("context_bytes", observed.context_bytes),
    ] {
        if value.is_some_and(|value| value < 0) {
            return Err(format!("{name} must not be negative"));
        }
    }
    if observed
        .cost_usd
        .is_some_and(|value| !value.is_finite() || value < 0.0)
    {
        return Err("cost_usd must be finite and non-negative".to_string());
    }
    if let (Some(input), Some(output), Some(total)) = (
        observed.input_tokens,
        observed.output_tokens,
        observed.total_tokens,
    ) {
        if input + output != total {
            return Err("observed token totals are contradictory".to_string());
        }
    }
    Ok(())
}

fn validate_forecast_estimate(estimate: &BudgetForecastEstimate) -> Result<(), String> {
    for (name, value) in [
        ("expected_total_tokens", estimate.expected_total_tokens),
        ("expected_cost_usd", estimate.expected_cost_usd),
    ] {
        if value.is_some_and(|value| !value.is_finite() || value < 0.0) {
            return Err(format!("{name} must be finite and non-negative"));
        }
    }
    if estimate.expected_total_tokens.is_none() && estimate.expected_cost_usd.is_none() {
        return Err("supported forecast requires at least one estimate".to_string());
    }
    if let Some(exhaustion_at) = &estimate.exhaustion_at {
        parse_timestamp("exhaustion_at", exhaustion_at)?;
    }
    Ok(())
}

fn validate_measurement(measurement: &BudgetAnomalyMeasurement) -> Result<(), String> {
    validate_identifier("anomaly metric", &measurement.metric)?;
    for (name, value) in [
        ("observed", measurement.observed),
        ("baseline", measurement.baseline),
        ("threshold", measurement.threshold),
        ("normalized_delta", measurement.normalized_delta),
    ] {
        if !value.is_finite() {
            return Err(format!("anomaly {name} must be finite"));
        }
    }
    if measurement.observed < 0.0 || measurement.baseline < 0.0 || measurement.threshold < 0.0 {
        return Err("anomaly values and threshold must be non-negative".to_string());
    }
    Ok(())
}

fn validate_evidence_references(references: &[BudgetEvidenceReference]) -> Result<(), String> {
    if references.len() > MAX_EVIDENCE_REFERENCES {
        return Err("too many evidence references".to_string());
    }
    let keys = references
        .iter()
        .map(|reference| format!("{}\0{}", reference.evidence_type, reference.evidence_id))
        .collect::<Vec<_>>();
    validate_sorted_unique("evidence_references", &keys, MAX_EVIDENCE_REFERENCES)?;
    for reference in references {
        validate_identifier("evidence type", &reference.evidence_type)?;
        validate_identifier("evidence id", &reference.evidence_id)?;
        if let Some(hash) = &reference.content_sha256 {
            validate_hash(hash)?;
        }
    }
    Ok(())
}

fn validate_reason_codes(name: &str, values: &[String]) -> Result<(), String> {
    validate_sorted_unique(name, values, MAX_REASON_CODES)?;
    for value in values {
        if value.is_empty()
            || value.len() > 96
            || !value
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '.' | '-'))
        {
            return Err(format!("{name} contains an invalid reason code"));
        }
    }
    Ok(())
}

fn validate_strings(name: &str, values: &[String], maximum: usize) -> Result<(), String> {
    validate_sorted_unique(name, values, maximum)?;
    if values
        .iter()
        .any(|value| value.is_empty() || value.len() > 256 || value.chars().any(char::is_control))
    {
        return Err(format!("{name} contains an invalid bounded string"));
    }
    Ok(())
}

fn validate_sorted_unique(name: &str, values: &[String], maximum: usize) -> Result<(), String> {
    if values.len() > maximum {
        return Err(format!("{name} exceeds the bounded maximum"));
    }
    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(format!("{name} must be sorted and unique"));
    }
    Ok(())
}

fn validate_identifier(name: &str, value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > MAX_IDENTIFIER_BYTES
        || !value.chars().all(|ch| {
            ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':' | '/' | '@')
        })
    {
        return Err(format!("invalid {name}"));
    }
    Ok(())
}

fn parse_timestamp(name: &str, value: &str) -> Result<DateTime<FixedOffset>, String> {
    DateTime::parse_from_rfc3339(value).map_err(|_| format!("{name} must be RFC3339"))
}

fn validate_hash(value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .chars()
            .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character))
    {
        return Err("evidence hash must be lowercase SHA-256".to_string());
    }
    Ok(())
}

fn canonical_hash<T: Serialize>(value: &T) -> Result<String, String> {
    let mut json = serde_json::to_value(value).map_err(|error| error.to_string())?;
    let object = json
        .as_object_mut()
        .ok_or_else(|| "budget evidence must serialize as an object".to_string())?;
    object.insert(
        "evidence_sha256".to_string(),
        serde_json::Value::String(String::new()),
    );
    let bytes = serde_json::to_vec(&json).map_err(|error| error.to_string())?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub struct BudgetManager {
    default_currency: String,
}

impl Default for BudgetManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BudgetManager {
    pub fn new() -> Self {
        Self {
            default_currency: "token".to_string(),
        }
    }

    pub fn create_reservation(
        &self,
        decision_id: &str,
        analysis: &TaskAnalysis,
        tier: &str,
        runtime: &mut FixtureRuntime,
    ) -> BudgetReservation {
        let input_tokens = analysis.context_budget_estimate;
        let output_tokens = analysis.execution_budget_estimate;
        let total_tokens = input_tokens + output_tokens;
        BudgetReservation {
            reservation_id: runtime.id("res-"),
            decision_id: decision_id.to_string(),
            currency: self.default_currency.clone(),
            pre_budget: total_tokens,
            reserved_input_tokens: input_tokens,
            reserved_output_tokens: output_tokens,
            reserved_total_tokens: total_tokens,
            reserved_cost: round6(self.estimate_cost(tier, input_tokens, output_tokens)),
            status: "reserved".to_string(),
            created_at: runtime.now(),
            updated_at: runtime.now(),
            ..BudgetReservation::default()
        }
    }

    pub fn check_violation(
        &self,
        reservation: &BudgetReservation,
        actual_tokens: i64,
    ) -> (bool, Option<String>) {
        if actual_tokens > reservation.reserved_total_tokens {
            let delta = actual_tokens - reservation.reserved_total_tokens;
            return (true, Some(format!("budget exceeded by {delta} tokens")));
        }
        (false, None)
    }

    pub fn estimate_cost(&self, tier: &str, input_tokens: i64, output_tokens: i64) -> f64 {
        let input_rate = match tier {
            "cheap_executor" => 0.0005,
            "balanced_worker" => 0.003,
            "codex_cli" => 0.003,
            "strong_planner" => 0.015,
            "claude_code_cli" => 0.015,
            "verifier" => 0.003,
            "advisor" => 0.015,
            _ => 0.003,
        };
        let output_rate = match tier {
            "cheap_executor" => 0.0015,
            "balanced_worker" => 0.015,
            "codex_cli" => 0.015,
            "strong_planner" => 0.075,
            "claude_code_cli" => 0.075,
            "verifier" => 0.015,
            "advisor" => 0.075,
            _ => 0.015,
        };
        (input_tokens as f64 / 1000.0 * input_rate) + (output_tokens as f64 / 1000.0 * output_rate)
    }
}

fn round6(value: f64) -> f64 {
    let scaled = value * 1_000_000.0;
    let floor = scaled.floor();
    if scaled - floor == 0.5 {
        floor / 1_000_000.0
    } else {
        scaled.round() / 1_000_000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(sample_count: u32) -> BudgetEvidenceWindow {
        BudgetEvidenceWindow {
            start_inclusive: "2026-07-10T00:00:00Z".to_string(),
            end_exclusive: "2026-07-10T01:00:00Z".to_string(),
            generated_at: "2026-07-10T01:05:00Z".to_string(),
            freshness_seconds: 300,
            sample_count,
        }
    }

    fn scope() -> BudgetEvidenceScope {
        BudgetEvidenceScope {
            run_id: Some("run-1".to_string()),
            workspace_id: Some("workspace-1".to_string()),
            provider_id: Some("provider-1".to_string()),
            model_id: Some("model-1".to_string()),
        }
    }

    fn coverage(pricing_complete: bool) -> BudgetEvidenceCoverage {
        BudgetEvidenceCoverage {
            required_dimensions: vec![
                "model_id".to_string(),
                "provider_id".to_string(),
                "run_id".to_string(),
                "workspace_id".to_string(),
            ],
            observed_dimensions: vec![
                "model_id".to_string(),
                "provider_id".to_string(),
                "run_id".to_string(),
                "workspace_id".to_string(),
            ],
            pricing_complete,
            duplicate_events: 0,
            missing_fields: vec![],
        }
    }

    fn confidence() -> BudgetConfidence {
        BudgetConfidence {
            level: BudgetConfidenceLevel::High,
            score: 0.9,
            reason_codes: vec!["coverage.complete".to_string()],
        }
    }

    fn reference() -> BudgetEvidenceReference {
        BudgetEvidenceReference {
            evidence_type: "provider_audit_event".to_string(),
            evidence_id: "paudit-1".to_string(),
            content_sha256: Some("a".repeat(64)),
        }
    }

    fn observed() -> BudgetObservedUsage {
        BudgetObservedUsage {
            input_tokens: Some(100),
            output_tokens: Some(50),
            total_tokens: Some(150),
            cost_usd: Some(0.01),
            latency_ms: Some(200),
            retry_count: Some(0),
            context_bytes: Some(4096),
        }
    }

    fn supported_forecast() -> BudgetForecastEvidence {
        let mut evidence = BudgetForecastEvidence {
            schema_version: BUDGET_FORECAST_EVIDENCE_SCHEMA_VERSION.to_string(),
            forecast_id: "forecast-1".to_string(),
            scope: scope(),
            outcome: BudgetEvidenceOutcome::Supported,
            window: window(5),
            coverage: coverage(true),
            confidence: confidence(),
            reason_codes: vec!["forecast.deterministic".to_string()],
            evidence_references: vec![reference()],
            observed: observed(),
            estimate: Some(BudgetForecastEstimate {
                expected_total_tokens: Some(300.0),
                expected_cost_usd: Some(0.02),
                exhaustion_at: Some("2026-07-10T02:00:00Z".to_string()),
            }),
            assumptions: vec!["constant observed burn rate".to_string()],
            evidence_sha256: String::new(),
        };
        evidence.seal().unwrap();
        evidence
    }

    #[test]
    fn forecast_contract_round_trips_and_hashes_deterministically() {
        let evidence = supported_forecast();
        evidence.validate().unwrap();
        let encoded = serde_json::to_string(&evidence).unwrap();
        let decoded: BudgetForecastEvidence = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, evidence);
        assert_eq!(canonical_hash(&decoded).unwrap(), evidence.evidence_sha256);
        assert_eq!(decoded.schema_version, "budget_forecast_evidence.v1");
    }

    #[test]
    fn tampered_forecast_is_rejected() {
        let mut evidence = supported_forecast();
        evidence.estimate.as_mut().unwrap().expected_cost_usd = Some(99.0);
        assert_eq!(evidence.validate().unwrap_err(), "budget evidence hash mismatch");
    }

    #[test]
    fn sparse_evidence_is_explicitly_insufficient_and_contains_no_estimate() {
        let mut evidence = supported_forecast();
        evidence.outcome = BudgetEvidenceOutcome::InsufficientEvidence;
        evidence.window = window(1);
        evidence.coverage.observed_dimensions = vec!["provider_id".to_string()];
        evidence.coverage.missing_fields = vec!["model_id".to_string()];
        evidence.coverage.pricing_complete = false;
        evidence.confidence = BudgetConfidence {
            level: BudgetConfidenceLevel::Low,
            score: 0.2,
            reason_codes: vec!["coverage.sparse".to_string()],
        };
        evidence.reason_codes = vec!["insufficient_evidence.sparse".to_string()];
        evidence.estimate = None;
        evidence.seal().unwrap();
        evidence.validate().unwrap();
    }

    #[test]
    fn supported_forecast_rejects_sparse_or_unpriced_evidence() {
        let mut sparse = supported_forecast();
        sparse.window.sample_count = 2;
        sparse.seal().unwrap();
        assert_eq!(
            sparse.validate().unwrap_err(),
            "supported evidence requires at least 3 samples"
        );

        let mut unpriced = supported_forecast();
        unpriced.coverage.pricing_complete = false;
        unpriced.seal().unwrap();
        assert_eq!(
            unpriced.validate().unwrap_err(),
            "supported forecast requires complete pricing"
        );
    }

    #[test]
    fn required_unknown_model_dimension_fails_closed() {
        let mut evidence = supported_forecast();
        evidence.scope.model_id = None;
        evidence.coverage.observed_dimensions = vec![
            "provider_id".to_string(),
            "run_id".to_string(),
            "workspace_id".to_string(),
        ];
        evidence.coverage.missing_fields = vec!["model_id".to_string()];
        evidence.seal().unwrap();
        assert_eq!(
            evidence.validate().unwrap_err(),
            "supported evidence cannot have missing fields"
        );
    }

    #[test]
    fn clock_and_window_boundaries_are_validated_without_wall_clock_reads() {
        let mut inverted = supported_forecast();
        inverted.window.start_inclusive = inverted.window.end_exclusive.clone();
        inverted.seal().unwrap();
        assert_eq!(
            inverted.validate().unwrap_err(),
            "evidence window must have start before end"
        );

        let mut future = supported_forecast();
        future.window.end_exclusive = "2026-07-10T01:06:00Z".to_string();
        future.window.freshness_seconds = 0;
        future.seal().unwrap();
        assert_eq!(
            future.validate().unwrap_err(),
            "evidence window cannot end after generation time"
        );
    }

    #[test]
    fn anomaly_contract_requires_explainable_supported_measurement() {
        let mut finding = BudgetAnomalyFinding {
            schema_version: BUDGET_ANOMALY_FINDING_SCHEMA_VERSION.to_string(),
            finding_id: "finding-1".to_string(),
            scope: scope(),
            outcome: BudgetEvidenceOutcome::Supported,
            window: window(8),
            coverage: coverage(true),
            confidence: confidence(),
            reason_codes: vec!["anomaly.token_spike".to_string()],
            evidence_references: vec![reference()],
            anomaly_kind: Some(BudgetAnomalyKind::TokenSpike),
            severity: Some(BudgetAnomalySeverity::Warning),
            measurement: Some(BudgetAnomalyMeasurement {
                metric: "total_tokens".to_string(),
                observed: 300.0,
                baseline: 100.0,
                threshold: 2.0,
                normalized_delta: 2.0,
            }),
            evidence_sha256: String::new(),
        };
        finding.seal().unwrap();
        finding.validate().unwrap();

        finding.anomaly_kind = Some(BudgetAnomalyKind::CostSpike);
        finding.coverage.pricing_complete = false;
        finding.seal().unwrap();
        assert_eq!(
            finding.validate().unwrap_err(),
            "cost anomaly requires complete pricing"
        );
    }

    #[test]
    fn malformed_and_noncanonical_fields_are_rejected() {
        let mut evidence = supported_forecast();
        evidence.reason_codes = vec![
            "forecast.second".to_string(),
            "forecast.first".to_string(),
        ];
        evidence.seal().unwrap();
        assert_eq!(
            evidence.validate().unwrap_err(),
            "reason_codes must be sorted and unique"
        );

        let mut oversized = supported_forecast();
        oversized.assumptions = vec!["x".repeat(257)];
        oversized.seal().unwrap();
        assert_eq!(
            oversized.validate().unwrap_err(),
            "assumptions contains an invalid bounded string"
        );
    }
}
