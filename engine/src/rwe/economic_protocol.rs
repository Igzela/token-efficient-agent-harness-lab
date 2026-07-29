//! Provider-free RWE economic protocol and VDE artifact contracts.
//!
//! These immutable, hash-bound documents project evidence from existing RWE
//! and Product Golden Path owners. They do not authorize execution, spend,
//! reviewer acceptance, output, adoption, merge, release, or deployment.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

pub const RWE_ECONOMIC_PROTOCOL_SCHEMA: &str = "rwe_economic_protocol.v1";
pub const TASK_VALUE_PROFILE_SCHEMA: &str = "task_value_profile.v1";
pub const IMPLEMENTATION_COST_RECEIPT_SCHEMA: &str = "implementation_cost_receipt.v1";
pub const VERIFIED_DELIVERY_OBSERVATION_SCHEMA: &str = "verified_delivery_observation.v1";
pub const VERIFIED_DELIVERY_COMPARISON_SCHEMA: &str = "verified_delivery_comparison.v1";

const VDE_ARTIFACT_SCHEMAS: &[&str] = &[
    TASK_VALUE_PROFILE_SCHEMA,
    IMPLEMENTATION_COST_RECEIPT_SCHEMA,
    VERIFIED_DELIVERY_OBSERVATION_SCHEMA,
    VERIFIED_DELIVERY_COMPARISON_SCHEMA,
];

const SENSITIVE_KEYS: &[&str] = &[
    "api_key",
    "authorization_header",
    "credential",
    "credentials",
    "prompt",
    "raw_output",
    "raw_prompt",
    "raw_response",
    "raw_transcript",
    "secret",
    "transcript",
];

const REQUIRED_COST_FIELDS: &[&str] = &[
    "provider_requests",
    "input_tokens",
    "output_tokens",
    "latency_ms",
    "monetary_cost",
    "agent_sessions",
    "review_cycles",
    "repair_iterations",
    "ci_runs",
    "ci_compute_minutes",
    "human_preparation_minutes",
    "review_minutes",
    "material_rework_minutes",
    "recovery_minutes",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvidenceSufficiency {
    InsufficientRepetitions,
    PointEstimateOnly,
    IntervalAvailable,
    ComparisonEligible,
}

impl EvidenceSufficiency {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InsufficientRepetitions => "INSUFFICIENT_REPETITIONS",
            Self::PointEstimateOnly => "POINT_ESTIMATE_ONLY",
            Self::IntervalAvailable => "INTERVAL_AVAILABLE",
            Self::ComparisonEligible => "COMPARISON_ELIGIBLE",
        }
    }
}

/// Immutable artifact-first evidence document.
#[derive(Debug, Clone, PartialEq)]
pub struct FrozenEvidenceDocument {
    pub schema_version: String,
    pub body_sha256: String,
    pub body: Value,
}

impl FrozenEvidenceDocument {
    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "schema_version": self.schema_version,
            "body_sha256": self.body_sha256,
            "body": self.body,
            "execution_authority": false,
            "spend_authority": false,
            "adoption_authority": false,
        })
    }
}

/// Classify evidence without converting missing evidence into a zero.
pub fn classify_evidence_sufficiency(
    completed_repetitions: u64,
    minimum_repetitions: u64,
    interval_available: bool,
    cost_complete: bool,
    identical_protocol: bool,
) -> EvidenceSufficiency {
    if minimum_repetitions == 0 || completed_repetitions < minimum_repetitions {
        EvidenceSufficiency::InsufficientRepetitions
    } else if !interval_available {
        EvidenceSufficiency::PointEstimateOnly
    } else if !cost_complete || !identical_protocol {
        EvidenceSufficiency::IntervalAvailable
    } else {
        EvidenceSufficiency::ComparisonEligible
    }
}

/// Validate and freeze a real-workload economic measurement protocol.
///
/// Fixture and placeholder repositories are rejected. Freezing this document
/// remains provider-free and never creates a live execution authorization.
pub fn freeze_rwe_economic_protocol(body: Value) -> Result<FrozenEvidenceDocument, String> {
    require_schema(&body, RWE_ECONOMIC_PROTOCOL_SCHEMA)?;
    reject_sensitive_keys(&body, "$")?;
    require_bool(&body, "frozen_before_results", true)?;
    require_bool(&body, "live_execution_authorized", false)?;
    require_bool(&body, "fixture_only", false)?;
    required_nonempty_str(&body, "protocol_id")?;
    required_sha256(&body, "authority_corpus_sha256")?;

    let tasks = required_array(&body, "tasks")?;
    if tasks.is_empty() {
        return Err("tasks must not be empty".into());
    }
    let mut task_ids = HashSet::new();
    for (index, task) in tasks.iter().enumerate() {
        validate_protocol_task(task, index, &mut task_ids)?;
    }

    validate_reviewer_policy(required_object(&body, "reviewer_policy")?)?;
    if required_positive_u64(&body, "minimum_repetitions_per_task")? < 2 {
        return Err("minimum_repetitions_per_task must be at least 2".into());
    }
    let budget_point_ids = validate_budget_points(required_array(&body, "budget_points")?)?;
    validate_task_budget_refs(tasks, &budget_point_ids)?;
    require_unique_strings(&body, "stop_rules", false)?;
    require_unique_u64s(&body, "seeds")?;
    validate_non_inferiority(required_object(&body, "non_inferiority_margins")?)?;
    validate_cost_completeness(required_object(&body, "cost_completeness")?)?;
    validate_statistical_method(required_object(&body, "statistical_method")?)?;
    freeze(body)
}

/// Validate and freeze one of the four artifact-first VDE evidence contracts.
pub fn freeze_vde_artifact(
    body: Value,
    protocol: &FrozenEvidenceDocument,
) -> Result<FrozenEvidenceDocument, String> {
    if protocol.schema_version != RWE_ECONOMIC_PROTOCOL_SCHEMA {
        return Err("protocol must be rwe_economic_protocol.v1".into());
    }
    reject_sensitive_keys(&body, "$")?;
    let schema = required_nonempty_str(&body, "schema_version")?;
    if !VDE_ARTIFACT_SCHEMAS.contains(&schema) {
        return Err(format!("unsupported VDE artifact schema {schema}"));
    }
    if body.get("protocol_sha256").and_then(Value::as_str) != Some(protocol.body_sha256.as_str()) {
        return Err("protocol_sha256 does not match frozen protocol".into());
    }
    require_bool(&body, "fixture_only", false)?;
    require_bool(&body, "adoption_authorized", false)?;
    match schema {
        TASK_VALUE_PROFILE_SCHEMA => validate_task_value_profile(&body, protocol)?,
        IMPLEMENTATION_COST_RECEIPT_SCHEMA => validate_implementation_cost_receipt(&body)?,
        VERIFIED_DELIVERY_OBSERVATION_SCHEMA => {
            validate_verified_delivery_observation(&body, protocol)?
        }
        VERIFIED_DELIVERY_COMPARISON_SCHEMA => {
            validate_verified_delivery_comparison(&body, protocol)?
        }
        _ => unreachable!(),
    }
    freeze(body)
}

fn validate_protocol_task(
    task: &Value,
    index: usize,
    task_ids: &mut HashSet<String>,
) -> Result<(), String> {
    let prefix = format!("tasks[{index}]");
    let task_id = required_nonempty_str(task, "task_id").map_err(|e| format!("{prefix}: {e}"))?;
    if !task_ids.insert(task_id.to_string()) {
        return Err(format!("duplicate task_id {task_id}"));
    }
    let repository =
        required_nonempty_str(task, "source_repository").map_err(|e| format!("{prefix}: {e}"))?;
    if repository.starts_with("fixture://")
        || repository.contains("example.com")
        || repository.ends_with("/owner/repository")
        || !repository.starts_with("https://")
    {
        return Err(format!(
            "{prefix}: source_repository must be a non-fixture https repository"
        ));
    }
    required_git_sha(task, "source_commit").map_err(|e| format!("{prefix}: {e}"))?;
    required_sha256(task, "source_tree_hash").map_err(|e| format!("{prefix}: {e}"))?;
    required_sha256(task, "task_definition_sha256").map_err(|e| format!("{prefix}: {e}"))?;
    require_unique_strings(task, "allowed_mutable_paths", false)
        .map_err(|e| format!("{prefix}: {e}"))?;
    require_unique_strings(task, "verification_commands", false)
        .map_err(|e| format!("{prefix}: {e}"))?;
    required_positive_u64(task, "patch_max_files").map_err(|e| format!("{prefix}: {e}"))?;
    required_positive_u64(task, "patch_max_lines").map_err(|e| format!("{prefix}: {e}"))?;
    required_positive_u64(task, "timeout_ms").map_err(|e| format!("{prefix}: {e}"))?;
    required_nonempty_str(task, "cancel_behavior").map_err(|e| format!("{prefix}: {e}"))?;
    required_nonempty_str(task, "executor_identity").map_err(|e| format!("{prefix}: {e}"))?;
    required_nonempty_str(task, "model_identity").map_err(|e| format!("{prefix}: {e}"))?;
    required_nonempty_str(task, "expected_outcome_class").map_err(|e| format!("{prefix}: {e}"))?;
    require_bool(task, "draft_pr_only", true).map_err(|e| format!("{prefix}: {e}"))?;
    require_bool(task, "auto_merge_disabled", true).map_err(|e| format!("{prefix}: {e}"))?;
    require_unique_strings(task, "budget_point_ids", false)
        .map_err(|e| format!("{prefix}: {e}"))?;
    require_unique_strings(task, "cleanup_rules", false).map_err(|e| format!("{prefix}: {e}"))?;

    validate_value_profile(
        required_object(task, "value_profile").map_err(|e| format!("{prefix}: {e}"))?,
    )
    .map_err(|e| format!("{prefix}: {e}"))?;
    let rubric =
        required_object(task, "acceptance_rubric").map_err(|e| format!("{prefix}: {e}"))?;
    for layer in [
        "machine_verification",
        "artifact_integrity",
        "reviewer_acceptance",
        "output_confirmation",
        "terminal_evidence",
    ] {
        required_nonempty_str_from_map(rubric, layer)
            .map_err(|e| format!("{prefix}.acceptance_rubric: {e}"))?;
    }
    Ok(())
}

fn validate_value_profile(profile: &Map<String, Value>) -> Result<(), String> {
    let basis = required_nonempty_str_from_map(profile, "primary_value_basis")?;
    if ![
        "monetary_outcome",
        "verified_delivery_points",
        "human_time_equivalent",
        "operator_defined_units",
    ]
    .contains(&basis)
    {
        return Err(format!("unsupported primary_value_basis {basis}"));
    }
    let source = required_nonempty_str_from_map(profile, "value_source")?;
    let source_lower = source.to_ascii_lowercase();
    if source_lower.contains("fixture") || source_lower.contains("placeholder") {
        return Err("value_source must not be fixture or placeholder".into());
    }
    required_sha256_from_map(profile, "value_source_sha256")?;
    let confidence = required_nonempty_str_from_map(profile, "value_confidence")?;
    if !["measured", "contractual", "estimated"].contains(&confidence) {
        return Err(format!("unsupported value_confidence {confidence}"));
    }
    required_nonempty_str_from_map(profile, "unit")?;
    Ok(())
}

fn validate_reviewer_policy(policy: &Map<String, Value>) -> Result<(), String> {
    required_nonempty_str_from_map(policy, "reviewer_identity_class")?;
    required_sha256_from_map(policy, "rubric_sha256")?;
    required_positive_u64_from_map(policy, "minimum_reviewers")?;
    required_bool_from_map(policy, "blinded")?;
    required_nonempty_str_from_map(policy, "permitted_repair")?;
    required_nonempty_str_from_map(policy, "disagreement_resolution")?;
    required_bool_from_map(policy, "measure_review_time")?;
    Ok(())
}

fn validate_budget_points(points: &[Value]) -> Result<HashSet<String>, String> {
    if points.is_empty() {
        return Err("budget_points must not be empty".into());
    }
    let mut ids = HashSet::new();
    for (index, point) in points.iter().enumerate() {
        let id = required_nonempty_str(point, "budget_point_id")
            .map_err(|e| format!("budget_points[{index}]: {e}"))?;
        if !ids.insert(id.to_string()) {
            return Err(format!("duplicate budget_point_id {id}"));
        }
        for field in [
            "max_provider_requests",
            "max_total_tokens",
            "max_wall_time_ms",
        ] {
            required_positive_u64(point, field)
                .map_err(|e| format!("budget_points[{index}]: {e}"))?;
        }
    }
    Ok(ids)
}

fn validate_task_budget_refs(
    tasks: &[Value],
    budget_point_ids: &HashSet<String>,
) -> Result<(), String> {
    for (index, task) in tasks.iter().enumerate() {
        for point_id in required_array(task, "budget_point_ids")? {
            let point_id = point_id
                .as_str()
                .ok_or_else(|| format!("tasks[{index}].budget_point_ids must contain strings"))?;
            if !budget_point_ids.contains(point_id) {
                return Err(format!(
                    "tasks[{index}].budget_point_ids references unknown {point_id}"
                ));
            }
        }
    }
    Ok(())
}

fn validate_non_inferiority(margins: &Map<String, Value>) -> Result<(), String> {
    for field in [
        "accepted_delivery_rate_max_regression",
        "machine_verification_rate_max_regression",
        "reviewer_acceptance_rate_max_regression",
        "recovery_failure_rate_max_increase",
    ] {
        let value = margins
            .get(field)
            .and_then(Value::as_f64)
            .ok_or_else(|| format!("{field} must be numeric"))?;
        if !(0.0..=1.0).contains(&value) {
            return Err(format!("{field} must be between 0 and 1"));
        }
    }
    Ok(())
}

fn validate_cost_completeness(cost: &Map<String, Value>) -> Result<(), String> {
    require_unique_strings_from_map(cost, "required_fields", false)?;
    let required = cost
        .get("required_fields")
        .and_then(Value::as_array)
        .ok_or("cost_completeness.required_fields required array")?;
    let values: HashSet<_> = required.iter().filter_map(Value::as_str).collect();
    for field in REQUIRED_COST_FIELDS {
        if !values.contains(field) {
            return Err(format!("cost_completeness.required_fields missing {field}"));
        }
    }
    required_bool_from_map(cost, "failed_attempt_costs_required")?;
    required_bool_from_map(cost, "unavailable_must_remain_unavailable")?;
    Ok(())
}

fn validate_statistical_method(method: &Map<String, Value>) -> Result<(), String> {
    required_nonempty_str_from_map(method, "method_id")?;
    required_sha256_from_map(method, "method_sha256")?;
    required_nonempty_str_from_map(method, "interval_method")?;
    required_nonempty_str_from_map(method, "stopping_analysis")?;
    let confidence = method
        .get("confidence_level")
        .and_then(Value::as_f64)
        .ok_or("statistical_method.confidence_level must be numeric")?;
    if !(0.0..1.0).contains(&confidence) {
        return Err("statistical_method.confidence_level must be between 0 and 1".into());
    }
    Ok(())
}

fn validate_task_value_profile(
    body: &Value,
    protocol: &FrozenEvidenceDocument,
) -> Result<(), String> {
    let task_id = required_nonempty_str(body, "task_id")?;
    let task = protocol_task(protocol, task_id)?;
    if body.get("value_profile") != task.get("value_profile") {
        return Err("value_profile does not exactly match frozen protocol task".into());
    }
    Ok(())
}

fn validate_implementation_cost_receipt(body: &Value) -> Result<(), String> {
    required_nonempty_str(body, "board_id")?;
    required_git_sha(body, "exact_head_sha")?;
    let realized = required_object(body, "realized_lifecycle_cost")?;
    for field in [
        "agent_sessions",
        "review_cycles",
        "repair_iterations",
        "ci_runs",
        "files_changed",
        "schema_migrations",
        "compatibility_adapters_added",
        "authority_boundaries_touched",
    ] {
        required_u64_from_map(realized, field)?;
    }
    required_nonempty_str_from_map(realized, "rollback_complexity")?;
    require_unique_strings_from_map(realized, "known_maintenance_surface", true)?;
    require_unique_strings(body, "cost_or_measurement_unavailable_fields", true)?;
    if body.get("forecast_lifecycle_cost").is_none() {
        return Err("forecast_lifecycle_cost required (use null when unavailable)".into());
    }
    required_u64(body, "observed_reuse_count")?;
    if body.get("expected_reuse_scenario").is_none() {
        return Err("expected_reuse_scenario required (use null when unavailable)".into());
    }
    Ok(())
}

fn validate_verified_delivery_observation(
    body: &Value,
    protocol: &FrozenEvidenceDocument,
) -> Result<(), String> {
    let task_id = required_nonempty_str(body, "task_id")?;
    protocol_task(protocol, task_id)?;
    required_nonempty_str(body, "attempt_id")?;
    required_nonempty_str(body, "failure_class")?;
    let layers = required_object(body, "layered_success")?;
    for layer in [
        "execution_completed",
        "machine_verification_passed",
        "artifact_integrity_passed",
        "reviewer_accepted",
        "output_confirmed",
        "terminal_evidence_complete",
    ] {
        required_bool_from_map(layers, layer)?;
    }
    for field in [
        "usage_evidence_ref",
        "implementation_cost_receipt_ref",
        "review_evidence_ref",
        "recovery_evidence_ref",
    ] {
        required_nonempty_str(body, field)?;
    }
    if required_nonempty_str(body, "evidence_sufficiency")?
        != EvidenceSufficiency::InsufficientRepetitions.as_str()
    {
        return Err(
            "a single verified_delivery_observation must be INSUFFICIENT_REPETITIONS".into(),
        );
    }
    Ok(())
}

fn validate_verified_delivery_comparison(
    body: &Value,
    protocol: &FrozenEvidenceDocument,
) -> Result<(), String> {
    let identical_protocol = required_bool_value(body, "identical_protocol")?;
    let hard_gates_passed = required_bool_value(body, "hard_gate_non_inferiority_passed")?;
    require_bool(body, "cross_value_basis_aggregation", false)?;
    require_bool(body, "automatic_go_authorized", false)?;
    required_sha256(body, "pre_observation_set_sha256")?;
    required_sha256(body, "post_observation_set_sha256")?;
    let completed = required_positive_u64(body, "completed_repetitions_per_task")?;
    let minimum = protocol
        .body
        .get("minimum_repetitions_per_task")
        .and_then(Value::as_u64)
        .ok_or("frozen protocol missing minimum_repetitions_per_task")?;
    let interval = required_bool_value(body, "interval_available")?;
    let cost_complete = required_bool_value(body, "cost_complete")?;
    let expected = classify_evidence_sufficiency(
        completed,
        minimum,
        interval,
        cost_complete,
        identical_protocol && hard_gates_passed,
    );
    let declared = required_nonempty_str(body, "evidence_sufficiency")?;
    if declared != expected.as_str() {
        return Err(format!(
            "evidence_sufficiency {declared} does not match derived {}",
            expected.as_str()
        ));
    }
    if expected == EvidenceSufficiency::ComparisonEligible
        && required_array(body, "lifecycle_cost_pareto_frontier")?.is_empty()
    {
        return Err("lifecycle_cost_pareto_frontier must not be empty".into());
    }
    Ok(())
}

fn protocol_task<'a>(
    protocol: &'a FrozenEvidenceDocument,
    task_id: &str,
) -> Result<&'a Value, String> {
    protocol
        .body
        .get("tasks")
        .and_then(Value::as_array)
        .and_then(|tasks| {
            tasks
                .iter()
                .find(|task| task.get("task_id").and_then(Value::as_str) == Some(task_id))
        })
        .ok_or_else(|| format!("task_id {task_id} is not in frozen protocol"))
}

fn freeze(body: Value) -> Result<FrozenEvidenceDocument, String> {
    let schema_version = required_nonempty_str(&body, "schema_version")?.to_string();
    let canonical = sort_value(&body);
    let body_sha256 = hex::encode(Sha256::digest(canonical.to_string().as_bytes()));
    Ok(FrozenEvidenceDocument {
        schema_version,
        body_sha256,
        body: canonical,
    })
}

fn sort_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            let mut out = Map::new();
            for key in keys {
                out.insert(key.clone(), sort_value(&map[&key]));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(sort_value).collect()),
        other => other.clone(),
    }
}

fn reject_sensitive_keys(value: &Value, path: &str) -> Result<(), String> {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if SENSITIVE_KEYS.contains(&key.to_ascii_lowercase().as_str()) {
                    return Err(format!("sensitive field forbidden at {path}.{key}"));
                }
                reject_sensitive_keys(child, &format!("{path}.{key}"))?;
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                reject_sensitive_keys(child, &format!("{path}[{index}]"))?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn require_schema(value: &Value, expected: &str) -> Result<(), String> {
    if value.get("schema_version").and_then(Value::as_str) != Some(expected) {
        return Err(format!("schema_version must be {expected}"));
    }
    Ok(())
}

fn required_object<'a>(value: &'a Value, key: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .get(key)
        .and_then(Value::as_object)
        .ok_or_else(|| format!("{key} required object"))
}

fn required_array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{key} required array"))
}

fn required_nonempty_str<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| format!("{key} required non-empty string"))
}

fn required_nonempty_str_from_map<'a>(
    value: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| format!("{key} required non-empty string"))
}

fn required_u64(value: &Value, key: &str) -> Result<u64, String> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{key} required non-negative integer"))
}

fn required_u64_from_map(value: &Map<String, Value>, key: &str) -> Result<u64, String> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{key} required non-negative integer"))
}

fn required_positive_u64(value: &Value, key: &str) -> Result<u64, String> {
    let number = required_u64(value, key)?;
    if number == 0 {
        return Err(format!("{key} must be positive"));
    }
    Ok(number)
}

fn required_positive_u64_from_map(value: &Map<String, Value>, key: &str) -> Result<u64, String> {
    let number = required_u64_from_map(value, key)?;
    if number == 0 {
        return Err(format!("{key} must be positive"));
    }
    Ok(number)
}

fn require_bool(value: &Value, key: &str, expected: bool) -> Result<(), String> {
    let actual = required_bool_value(value, key)?;
    if actual != expected {
        return Err(format!("{key} must be {expected}"));
    }
    Ok(())
}

fn required_bool_value(value: &Value, key: &str) -> Result<bool, String> {
    value
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("{key} required boolean"))
}

fn required_bool_from_map(value: &Map<String, Value>, key: &str) -> Result<bool, String> {
    value
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("{key} required boolean"))
}

fn required_sha256(value: &Value, key: &str) -> Result<(), String> {
    validate_hex_len(required_nonempty_str(value, key)?, 64, key)
}

fn required_sha256_from_map(value: &Map<String, Value>, key: &str) -> Result<(), String> {
    validate_hex_len(required_nonempty_str_from_map(value, key)?, 64, key)
}

fn required_git_sha(value: &Value, key: &str) -> Result<(), String> {
    let text = required_nonempty_str(value, key)?;
    if ![40, 64].contains(&text.len()) || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("{key} must be a 40- or 64-character git SHA"));
    }
    Ok(())
}

fn validate_hex_len(text: &str, len: usize, key: &str) -> Result<(), String> {
    if text.len() != len || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("{key} must be {len} hexadecimal characters"));
    }
    Ok(())
}

fn require_unique_strings(value: &Value, key: &str, allow_empty: bool) -> Result<(), String> {
    let map = value
        .as_object()
        .ok_or_else(|| format!("{key} parent must be object"))?;
    require_unique_strings_from_map(map, key, allow_empty)
}

fn require_unique_strings_from_map(
    value: &Map<String, Value>,
    key: &str,
    allow_empty: bool,
) -> Result<(), String> {
    let items = value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{key} required array"))?;
    if !allow_empty && items.is_empty() {
        return Err(format!("{key} must not be empty"));
    }
    let mut unique = HashSet::new();
    for item in items {
        let text = item
            .as_str()
            .filter(|text| !text.trim().is_empty())
            .ok_or_else(|| format!("{key} must contain only non-empty strings"))?;
        if !unique.insert(text) {
            return Err(format!("{key} contains duplicate {text}"));
        }
    }
    Ok(())
}

fn require_unique_u64s(value: &Value, key: &str) -> Result<(), String> {
    let items = required_array(value, key)?;
    if items.is_empty() {
        return Err(format!("{key} must not be empty"));
    }
    let mut unique = HashSet::new();
    for item in items {
        let number = item
            .as_u64()
            .ok_or_else(|| format!("{key} must contain only non-negative integers"))?;
        if !unique.insert(number) {
            return Err(format!("{key} contains duplicate {number}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sha() -> &'static str {
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    }

    fn protocol_body() -> Value {
        json!({
            "schema_version": RWE_ECONOMIC_PROTOCOL_SCHEMA,
            "protocol_id": "rwe-economic-protocol-test-v1",
            "authority_corpus_sha256": sha(),
            "frozen_before_results": true,
            "live_execution_authorized": false,
            "fixture_only": false,
            "tasks": [{
                "task_id": "task-1",
                "source_repository": "https://github.com/Igzela/token-efficient-agent-harness-lab",
                "source_commit": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "source_tree_hash": sha(),
                "task_definition_sha256": sha(),
                "allowed_mutable_paths": ["src/lib.rs"],
                "verification_commands": ["cargo test"],
                "patch_max_files": 2,
                "patch_max_lines": 50,
                "timeout_ms": 120000,
                "cancel_behavior": "fail_closed_charged",
                "executor_identity": "codex-cli-exact",
                "model_identity": "model-exact",
                "expected_outcome_class": "success_or_controlled_failure",
                "draft_pr_only": true,
                "auto_merge_disabled": true,
                "budget_point_ids": ["small"],
                "cleanup_rules": ["remove_worktree"],
                "value_profile": {
                    "primary_value_basis": "verified_delivery_points",
                    "value_source": "operator-approved rubric",
                    "value_source_sha256": sha(),
                    "value_confidence": "contractual",
                    "unit": "verified_points"
                },
                "acceptance_rubric": {
                    "machine_verification": "all admitted commands pass",
                    "artifact_integrity": "artifact hash matches",
                    "reviewer_acceptance": "independent reviewer accepts",
                    "output_confirmation": "draft output receipt matches",
                    "terminal_evidence": "terminal bundle complete"
                }
            }],
            "reviewer_policy": {
                "reviewer_identity_class": "independent_repository_reviewer",
                "rubric_sha256": sha(),
                "minimum_reviewers": 1,
                "blinded": true,
                "permitted_repair": "one bounded repair cycle",
                "disagreement_resolution": "record disagreement and fail closed",
                "measure_review_time": true
            },
            "minimum_repetitions_per_task": 3,
            "budget_points": [{
                "budget_point_id": "small",
                "max_provider_requests": 1,
                "max_total_tokens": 12000,
                "max_wall_time_ms": 180000
            }],
            "stop_rules": ["stop_on_authority_failure", "stop_on_cost_incompleteness"],
            "non_inferiority_margins": {
                "accepted_delivery_rate_max_regression": 0.0,
                "machine_verification_rate_max_regression": 0.0,
                "reviewer_acceptance_rate_max_regression": 0.0,
                "recovery_failure_rate_max_increase": 0.0
            },
            "cost_completeness": {
                "required_fields": REQUIRED_COST_FIELDS,
                "failed_attempt_costs_required": true,
                "unavailable_must_remain_unavailable": true
            },
            "seeds": [7, 11, 19],
            "statistical_method": {
                "method_id": "paired-bootstrap-v1",
                "method_sha256": sha(),
                "interval_method": "paired bootstrap confidence interval",
                "confidence_level": 0.95,
                "stopping_analysis": "fixed repetitions; no optional stopping"
            }
        })
    }

    #[test]
    fn freezes_protocol_deterministically_without_authority() {
        let first = freeze_rwe_economic_protocol(protocol_body()).unwrap();
        let second = freeze_rwe_economic_protocol(protocol_body()).unwrap();
        assert_eq!(first.body_sha256, second.body_sha256);
        assert_eq!(first.body_sha256.len(), 64);
        assert_eq!(first.to_json()["execution_authority"], false);
        assert_eq!(first.to_json()["spend_authority"], false);
        assert_eq!(first.to_json()["adoption_authority"], false);
    }

    #[test]
    fn rejects_fixture_placeholder_and_sensitive_protocols() {
        let mut fixture = protocol_body();
        fixture["tasks"][0]["source_repository"] = json!("fixture://rwe-source-repo");
        assert!(freeze_rwe_economic_protocol(fixture)
            .unwrap_err()
            .contains("non-fixture https repository"));

        let mut placeholder = protocol_body();
        placeholder["tasks"][0]["source_repository"] =
            json!("https://example.com/owner/repository");
        assert!(freeze_rwe_economic_protocol(placeholder)
            .unwrap_err()
            .contains("non-fixture https repository"));

        let mut sensitive = protocol_body();
        sensitive["tasks"][0]["prompt"] = json!("raw task prompt");
        assert!(freeze_rwe_economic_protocol(sensitive)
            .unwrap_err()
            .contains("sensitive field forbidden"));
    }

    #[test]
    fn derives_evidence_sufficiency_fail_closed() {
        assert_eq!(
            classify_evidence_sufficiency(1, 3, false, false, false),
            EvidenceSufficiency::InsufficientRepetitions
        );
        assert_eq!(
            classify_evidence_sufficiency(3, 3, false, true, true),
            EvidenceSufficiency::PointEstimateOnly
        );
        assert_eq!(
            classify_evidence_sufficiency(3, 3, true, false, true),
            EvidenceSufficiency::IntervalAvailable
        );
        assert_eq!(
            classify_evidence_sufficiency(3, 3, true, true, true),
            EvidenceSufficiency::ComparisonEligible
        );
    }

    #[test]
    fn observation_remains_insufficient_and_hash_bound() {
        let protocol = freeze_rwe_economic_protocol(protocol_body()).unwrap();
        let body = json!({
            "schema_version": VERIFIED_DELIVERY_OBSERVATION_SCHEMA,
            "protocol_sha256": protocol.body_sha256,
            "fixture_only": false,
            "adoption_authorized": false,
            "task_id": "task-1",
            "attempt_id": "attempt-1",
            "failure_class": "none",
            "layered_success": {
                "execution_completed": true,
                "machine_verification_passed": true,
                "artifact_integrity_passed": true,
                "reviewer_accepted": true,
                "output_confirmed": true,
                "terminal_evidence_complete": true
            },
            "usage_evidence_ref": "usage:1",
            "implementation_cost_receipt_ref": "cost:1",
            "review_evidence_ref": "review:1",
            "recovery_evidence_ref": "recovery:1",
            "evidence_sufficiency": "INSUFFICIENT_REPETITIONS"
        });
        let artifact = freeze_vde_artifact(body, &protocol).unwrap();
        assert_eq!(
            artifact.schema_version,
            VERIFIED_DELIVERY_OBSERVATION_SCHEMA
        );
        assert_eq!(artifact.body_sha256.len(), 64);
    }

    #[test]
    fn task_value_profile_must_match_frozen_protocol() {
        let protocol = freeze_rwe_economic_protocol(protocol_body()).unwrap();
        let value_profile = protocol.body["tasks"][0]["value_profile"].clone();
        let mut body = json!({
            "schema_version": TASK_VALUE_PROFILE_SCHEMA,
            "protocol_sha256": protocol.body_sha256,
            "fixture_only": false,
            "adoption_authorized": false,
            "task_id": "task-1",
            "value_profile": value_profile
        });
        freeze_vde_artifact(body.clone(), &protocol).unwrap();
        body["value_profile"]["unit"] = json!("caller-rewritten-unit");
        assert!(freeze_vde_artifact(body, &protocol)
            .unwrap_err()
            .contains("does not exactly match"));
    }

    #[test]
    fn comparison_requires_derived_eligibility_and_pareto_evidence() {
        let protocol = freeze_rwe_economic_protocol(protocol_body()).unwrap();
        let mut body = json!({
            "schema_version": VERIFIED_DELIVERY_COMPARISON_SCHEMA,
            "protocol_sha256": protocol.body_sha256,
            "fixture_only": false,
            "adoption_authorized": false,
            "identical_protocol": true,
            "hard_gate_non_inferiority_passed": true,
            "cross_value_basis_aggregation": false,
            "automatic_go_authorized": false,
            "pre_observation_set_sha256": sha(),
            "post_observation_set_sha256": sha(),
            "completed_repetitions_per_task": 3,
            "interval_available": true,
            "cost_complete": true,
            "evidence_sufficiency": "COMPARISON_ELIGIBLE",
            "lifecycle_cost_pareto_frontier": [{"point_id": "p1"}]
        });
        freeze_vde_artifact(body.clone(), &protocol).unwrap();
        body["cost_complete"] = json!(false);
        assert!(freeze_vde_artifact(body, &protocol)
            .unwrap_err()
            .contains("does not match derived"));

        let mut hard_gate_failure = json!({
            "schema_version": VERIFIED_DELIVERY_COMPARISON_SCHEMA,
            "protocol_sha256": protocol.body_sha256,
            "fixture_only": false,
            "adoption_authorized": false,
            "identical_protocol": true,
            "hard_gate_non_inferiority_passed": false,
            "cross_value_basis_aggregation": false,
            "automatic_go_authorized": false,
            "pre_observation_set_sha256": sha(),
            "post_observation_set_sha256": sha(),
            "completed_repetitions_per_task": 3,
            "interval_available": true,
            "cost_complete": true,
            "evidence_sufficiency": "COMPARISON_ELIGIBLE",
            "lifecycle_cost_pareto_frontier": []
        });
        assert!(freeze_vde_artifact(hard_gate_failure.clone(), &protocol)
            .unwrap_err()
            .contains("does not match derived"));
        hard_gate_failure["evidence_sufficiency"] = json!("INTERVAL_AVAILABLE");
        freeze_vde_artifact(hard_gate_failure, &protocol).unwrap();

        let mut cross_basis = json!({
            "schema_version": VERIFIED_DELIVERY_COMPARISON_SCHEMA,
            "protocol_sha256": protocol.body_sha256,
            "fixture_only": false,
            "adoption_authorized": false,
            "identical_protocol": true,
            "hard_gate_non_inferiority_passed": true,
            "cross_value_basis_aggregation": true,
            "automatic_go_authorized": false,
            "pre_observation_set_sha256": sha(),
            "post_observation_set_sha256": sha(),
            "completed_repetitions_per_task": 3,
            "interval_available": true,
            "cost_complete": true,
            "evidence_sufficiency": "COMPARISON_ELIGIBLE",
            "lifecycle_cost_pareto_frontier": [{"point_id": "p1"}]
        });
        assert!(freeze_vde_artifact(cross_basis.clone(), &protocol)
            .unwrap_err()
            .contains("cross_value_basis_aggregation must be false"));
        cross_basis["cross_value_basis_aggregation"] = json!(false);
        freeze_vde_artifact(cross_basis, &protocol).unwrap();
    }

    #[test]
    fn cost_receipt_separates_realized_forecast_and_unknowns() {
        let protocol = freeze_rwe_economic_protocol(protocol_body()).unwrap();
        let body = json!({
            "schema_version": IMPLEMENTATION_COST_RECEIPT_SCHEMA,
            "protocol_sha256": protocol.body_sha256,
            "fixture_only": false,
            "adoption_authorized": false,
            "board_id": "board-1",
            "exact_head_sha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "realized_lifecycle_cost": {
                "agent_sessions": 1,
                "review_cycles": 1,
                "repair_iterations": 0,
                "ci_runs": 1,
                "files_changed": 3,
                "schema_migrations": 0,
                "compatibility_adapters_added": 0,
                "authority_boundaries_touched": 0,
                "rollback_complexity": "revert one commit",
                "known_maintenance_surface": ["engine/src/rwe/economic_protocol.rs"]
            },
            "forecast_lifecycle_cost": null,
            "observed_reuse_count": 0,
            "expected_reuse_scenario": null,
            "cost_or_measurement_unavailable_fields": ["ci_compute_minutes"]
        });
        freeze_vde_artifact(body, &protocol).unwrap();
    }
}
