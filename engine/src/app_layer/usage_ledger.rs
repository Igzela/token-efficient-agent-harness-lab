use std::collections::HashMap;

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const SCHEMA_VERSION: &str = "usage_ledger.v1";

pub const REQUIRED_FIELDS: &[&str] = &[
    "schema_version",
    "run_id",
    "case_id",
    "input_tokens",
    "output_tokens",
    "cached_tokens",
    "request_count",
    "tool_call_count",
    "retry_count",
    "wall_clock_ms",
    "estimated_cost",
    "pass",
    "cost_of_pass_group",
    "model_profile_id",
    "context_pack_id",
];

static COST_OF_PASS_GROUP_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-zA-Z0-9_\-]+/[a-zA-Z0-9_\-]+/[a-zA-Z0-9_\-]+/[a-zA-Z0-9_\-]+$").unwrap()
});

// ---------------------------------------------------------------------------
// Data structs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UsageLedgerRow {
    pub run_id: String,
    pub case_id: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_tokens: i64,
    pub request_count: i64,
    pub tool_call_count: i64,
    pub retry_count: i64,
    pub wall_clock_ms: i64,
    pub estimated_cost: f64,
    #[serde(rename = "pass")]
    pub pass_: bool,
    pub cost_of_pass_group: String,
    pub model_profile_id: String,
    pub context_pack_id: String,
    pub schema_version: String,
}

impl Default for UsageLedgerRow {
    fn default() -> Self {
        Self {
            run_id: String::new(),
            case_id: String::new(),
            input_tokens: 0,
            output_tokens: 0,
            cached_tokens: 0,
            request_count: 0,
            tool_call_count: 0,
            retry_count: 0,
            wall_clock_ms: 0,
            estimated_cost: 0.0,
            pass_: false,
            cost_of_pass_group: String::new(),
            model_profile_id: String::new(),
            context_pack_id: String::new(),
            schema_version: SCHEMA_VERSION.to_string(),
        }
    }
}

impl UsageLedgerRow {
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CostOfPassAggregate {
    pub cost_of_pass_group: String,
    pub total_estimated_cost: f64,
    pub success_count: i64,
    pub failure_count: i64,
    pub total_count: i64,
    pub cost_of_pass: Option<f64>,
}

impl Default for CostOfPassAggregate {
    fn default() -> Self {
        Self {
            cost_of_pass_group: String::new(),
            total_estimated_cost: 0.0,
            success_count: 0,
            failure_count: 0,
            total_count: 0,
            cost_of_pass: None,
        }
    }
}

impl CostOfPassAggregate {
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ComparisonResult {
    pub group_a: String,
    pub group_b: String,
    pub aggregate_a: CostOfPassAggregate,
    pub aggregate_b: CostOfPassAggregate,
    pub valid: bool,
    pub reason: String,
    pub cost_delta: Option<f64>,
    pub relative_change_pct: Option<f64>,
}

impl Default for ComparisonResult {
    fn default() -> Self {
        Self {
            group_a: String::new(),
            group_b: String::new(),
            aggregate_a: CostOfPassAggregate::default(),
            aggregate_b: CostOfPassAggregate::default(),
            valid: false,
            reason: String::new(),
            cost_delta: None,
            relative_change_pct: None,
        }
    }
}

impl ComparisonResult {
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap()
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

pub fn validate_usage_ledger_row(data: &serde_json::Value) -> Vec<String> {
    let mut violations = Vec::new();

    let object = match data.as_object() {
        Some(o) => o,
        None => {
            violations.push("usage_ledger_row must be a JSON object".to_string());
            return violations;
        }
    };

    for field in REQUIRED_FIELDS {
        if !object.contains_key(*field) {
            violations.push(format!("missing required field: {}", field));
        }
    }
    if !violations.is_empty() {
        return violations;
    }

    let sv = object["schema_version"].as_str().unwrap_or("");
    if sv != SCHEMA_VERSION {
        violations.push(format!(
            "schema_version must be {}, got {:?}",
            SCHEMA_VERSION, sv
        ));
    }

    if !object["pass"].is_boolean() {
        violations.push(format!(
            "pass must be a bool, got {}",
            json_type_name(&object["pass"])
        ));
    }

    let int_fields = [
        "input_tokens",
        "output_tokens",
        "cached_tokens",
        "request_count",
        "tool_call_count",
        "retry_count",
        "wall_clock_ms",
    ];
    for field in &int_fields {
        match object[*field].as_i64() {
            Some(v) if v >= 0 => {}
            _ => violations.push(format!(
                "{} must be a non-negative integer, got {}",
                field, object[*field]
            )),
        }
    }

    match object["estimated_cost"].as_f64() {
        Some(v) if v >= 0.0 => {}
        _ => violations.push(format!(
            "estimated_cost must be a non-negative number, got {}",
            object["estimated_cost"]
        )),
    }

    if let (Some(cached), Some(input)) = (
        object["cached_tokens"].as_i64(),
        object["input_tokens"].as_i64(),
    ) {
        if cached > input {
            violations.push(format!(
                "cached_tokens ({}) must not exceed input_tokens ({})",
                cached, input
            ));
        }
    }

    let group = object["cost_of_pass_group"].as_str().unwrap_or("");
    if !COST_OF_PASS_GROUP_PATTERN.is_match(group) {
        violations.push(format!(
            "cost_of_pass_group {:?} does not match format <eval_suite>/<task_family>/<variant_family>/<success_criterion>",
            group
        ));
    }

    for field in &["model_profile_id", "context_pack_id"] {
        let val = &object[*field];
        if !val.is_null() && !val.is_string() {
            violations.push(format!("{} must be a string, null, or empty string", field));
        }
    }

    violations
}

fn json_type_name(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

// ---------------------------------------------------------------------------
// Group format helpers
// ---------------------------------------------------------------------------

pub fn is_valid_cost_of_pass_group(group: &str) -> bool {
    COST_OF_PASS_GROUP_PATTERN.is_match(group)
}

pub fn parse_cost_of_pass_group(group: &str) -> Result<(String, String, String, String), String> {
    if !COST_OF_PASS_GROUP_PATTERN.is_match(group) {
        return Err(format!(
            "cost_of_pass_group {:?} does not match <eval_suite>/<task_family>/<variant_family>/<success_criterion>",
            group
        ));
    }
    let parts: Vec<&str> = group.split('/').collect();
    Ok((
        parts[0].to_string(),
        parts[1].to_string(),
        parts[2].to_string(),
        parts[3].to_string(),
    ))
}

// ---------------------------------------------------------------------------
// Aggregation
// ---------------------------------------------------------------------------

pub fn aggregate_cost_of_pass(rows: &[serde_json::Value]) -> CostOfPassAggregate {
    if rows.is_empty() {
        return CostOfPassAggregate::default();
    }

    let group = rows[0]
        .get("cost_of_pass_group")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();

    let mut total_cost = 0.0_f64;
    let mut success: i64 = 0;
    let mut failure: i64 = 0;

    for row in rows {
        total_cost += row
            .get("estimated_cost")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        if row.get("pass").and_then(serde_json::Value::as_bool) == Some(true) {
            success += 1;
        } else {
            failure += 1;
        }
    }

    let total = success + failure;
    let cop = if success > 0 {
        Some(total_cost / success as f64)
    } else {
        None
    };

    CostOfPassAggregate {
        cost_of_pass_group: group,
        total_estimated_cost: total_cost,
        success_count: success,
        failure_count: failure,
        total_count: total,
        cost_of_pass: cop,
    }
}

pub fn group_usage_rows(rows: &[serde_json::Value]) -> HashMap<String, Vec<serde_json::Value>> {
    let mut groups: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
    for row in rows {
        let group = row
            .get("cost_of_pass_group")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();
        groups.entry(group).or_default().push(row.clone());
    }
    groups
}

pub fn compare_cost_groups(
    before_rows: &[serde_json::Value],
    after_rows: &[serde_json::Value],
) -> ComparisonResult {
    let before_agg = aggregate_cost_of_pass(before_rows);
    let after_agg = aggregate_cost_of_pass(after_rows);

    if before_agg.cost_of_pass_group != after_agg.cost_of_pass_group {
        return ComparisonResult {
            group_a: before_agg.cost_of_pass_group.clone(),
            group_b: after_agg.cost_of_pass_group.clone(),
            aggregate_a: before_agg,
            aggregate_b: after_agg,
            valid: false,
            reason: "cannot compare different cost_of_pass_groups directly".to_string(),
            cost_delta: None,
            relative_change_pct: None,
        };
    }

    if before_agg.cost_of_pass.is_none() || after_agg.cost_of_pass.is_none() {
        return ComparisonResult {
            group_a: before_agg.cost_of_pass_group.clone(),
            group_b: after_agg.cost_of_pass_group.clone(),
            aggregate_a: before_agg,
            aggregate_b: after_agg,
            valid: false,
            reason: "cost_of_pass undefined for one or both groups (success_count=0)".to_string(),
            cost_delta: None,
            relative_change_pct: None,
        };
    }

    let before_cop = before_agg.cost_of_pass.unwrap();
    let after_cop = after_agg.cost_of_pass.unwrap();
    let delta = after_cop - before_cop;
    let pct = if before_cop != 0.0 {
        Some(delta / before_cop * 100.0)
    } else {
        None
    };

    ComparisonResult {
        group_a: before_agg.cost_of_pass_group.clone(),
        group_b: after_agg.cost_of_pass_group.clone(),
        aggregate_a: before_agg,
        aggregate_b: after_agg,
        valid: true,
        reason: "same group, both have defined cost_of_pass".to_string(),
        cost_delta: Some(delta),
        relative_change_pct: pct,
    }
}

pub fn detect_invalid_cost_comparison(
    group_a_rows: &[serde_json::Value],
    group_b_rows: &[serde_json::Value],
) -> (bool, String) {
    let agg_a = aggregate_cost_of_pass(group_a_rows);
    let agg_b = aggregate_cost_of_pass(group_b_rows);

    if agg_a.cost_of_pass_group != agg_b.cost_of_pass_group {
        return (
            true,
            format!(
                "groups {:?} and {:?} are different; direct cost comparison is invalid",
                agg_a.cost_of_pass_group, agg_b.cost_of_pass_group
            ),
        );
    }

    if agg_a.cost_of_pass.is_none() || agg_b.cost_of_pass.is_none() {
        return (
            true,
            "one or both groups have undefined cost_of_pass (success_count=0)".to_string(),
        );
    }

    (false, "comparison is valid".to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const GROUP: &str = "eval_suite/task_family/variant_family/success_criterion";

    fn valid_row(pass: bool, cost: f64, group: &str) -> serde_json::Value {
        json!({
            "schema_version": SCHEMA_VERSION,
            "run_id": "run-1",
            "case_id": "case-1",
            "input_tokens": 100,
            "output_tokens": 50,
            "cached_tokens": 10,
            "request_count": 1,
            "tool_call_count": 0,
            "retry_count": 0,
            "wall_clock_ms": 500,
            "estimated_cost": cost,
            "pass": pass,
            "cost_of_pass_group": group,
            "model_profile_id": "mimo-v2",
            "context_pack_id": "cp-001"
        })
    }

    #[test]
    fn test_valid_row_passes() {
        let violations = validate_usage_ledger_row(&valid_row(true, 0.05, GROUP));
        assert!(
            violations.is_empty(),
            "expected no violations, got {:?}",
            violations
        );
    }

    #[test]
    fn test_missing_required_field() {
        let mut data = valid_row(true, 0.01, GROUP);
        data.as_object_mut().unwrap().remove("run_id");
        let violations = validate_usage_ledger_row(&data);
        assert!(violations.iter().any(|v| v.contains("run_id")));
    }

    #[test]
    fn test_negative_input_tokens() {
        let mut data = valid_row(true, 0.01, GROUP);
        data["input_tokens"] = json!(-5);
        let violations = validate_usage_ledger_row(&data);
        assert!(violations
            .iter()
            .any(|v| v.contains("input_tokens") && v.contains("non-negative")));
    }

    #[test]
    fn test_cached_exceeds_input() {
        let mut data = valid_row(true, 0.01, GROUP);
        data["cached_tokens"] = json!(200);
        data["input_tokens"] = json!(100);
        let violations = validate_usage_ledger_row(&data);
        assert!(violations
            .iter()
            .any(|v| v.contains("cached_tokens") && v.contains("must not exceed")));
    }

    #[test]
    fn test_bad_cost_of_pass_group_format() {
        let mut data = valid_row(true, 0.01, GROUP);
        data["cost_of_pass_group"] = json!("bad_format");
        let violations = validate_usage_ledger_row(&data);
        assert!(violations.iter().any(|v| v.contains("cost_of_pass_group")));
    }

    #[test]
    fn test_is_valid_cost_of_pass_group() {
        assert!(is_valid_cost_of_pass_group(GROUP));
        assert!(!is_valid_cost_of_pass_group("bad"));
        assert!(!is_valid_cost_of_pass_group("a/b/c"));
        assert!(!is_valid_cost_of_pass_group("a/b/c/d/e"));
    }

    #[test]
    fn test_parse_cost_of_pass_group() {
        let (a, b, c, d) = parse_cost_of_pass_group(GROUP).unwrap();
        assert_eq!(a, "eval_suite");
        assert_eq!(b, "task_family");
        assert_eq!(c, "variant_family");
        assert_eq!(d, "success_criterion");

        assert!(parse_cost_of_pass_group("bad").is_err());
    }

    #[test]
    fn test_aggregate_cost_of_pass_basic() {
        let rows = vec![
            valid_row(true, 0.10, GROUP),
            valid_row(true, 0.20, GROUP),
            valid_row(false, 0.05, GROUP),
        ];
        let agg = aggregate_cost_of_pass(&rows);
        assert_eq!(agg.success_count, 2);
        assert_eq!(agg.failure_count, 1);
        assert_eq!(agg.total_count, 3);
        assert!((agg.total_estimated_cost - 0.35).abs() < 1e-10);
        assert!(agg.cost_of_pass.is_some());
        let cop = agg.cost_of_pass.unwrap();
        assert!((cop - 0.175).abs() < 1e-10);
    }

    #[test]
    fn test_aggregate_empty_rows() {
        let agg = aggregate_cost_of_pass(&[]);
        assert_eq!(agg.total_count, 0);
        assert!(agg.cost_of_pass.is_none());
    }

    #[test]
    fn test_aggregate_all_failures() {
        let rows = vec![valid_row(false, 0.10, GROUP), valid_row(false, 0.20, GROUP)];
        let agg = aggregate_cost_of_pass(&rows);
        assert_eq!(agg.success_count, 0);
        assert_eq!(agg.failure_count, 2);
        assert!(agg.cost_of_pass.is_none());
    }

    #[test]
    fn test_group_usage_rows() {
        let group_a = "suite/fam/variant/crit_a";
        let group_b = "suite/fam/variant/crit_b";
        let rows = vec![
            valid_row(true, 0.1, group_a),
            valid_row(true, 0.2, group_a),
            valid_row(false, 0.3, group_b),
        ];
        let groups = group_usage_rows(&rows);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[group_a].len(), 2);
        assert_eq!(groups[group_b].len(), 1);
    }

    #[test]
    fn test_compare_cost_groups_valid() {
        let before = vec![valid_row(true, 0.10, GROUP), valid_row(true, 0.20, GROUP)];
        let after = vec![valid_row(true, 0.05, GROUP), valid_row(true, 0.10, GROUP)];
        let result = compare_cost_groups(&before, &after);
        assert!(result.valid);
        assert!(result.cost_delta.is_some());
        let delta = result.cost_delta.unwrap();
        assert!(delta < 0.0);
    }

    #[test]
    fn test_compare_cost_groups_different_groups() {
        let group_a = "suite/fam/v1/crit";
        let group_b = "suite/fam/v2/crit";
        let before = vec![valid_row(true, 0.1, group_a)];
        let after = vec![valid_row(true, 0.1, group_b)];
        let result = compare_cost_groups(&before, &after);
        assert!(!result.valid);
        assert!(result.reason.contains("different cost_of_pass_groups"));
    }

    #[test]
    fn test_compare_cost_groups_no_success() {
        let rows = vec![valid_row(false, 0.1, GROUP)];
        let result = compare_cost_groups(&rows, &rows);
        assert!(!result.valid);
        assert!(result.reason.contains("success_count=0"));
    }

    #[test]
    fn test_detect_invalid_cost_comparison() {
        let group_a = "s/f/v/c1";
        let group_b = "s/f/v/c2";
        let rows_a = vec![valid_row(true, 0.1, group_a)];
        let rows_b = vec![valid_row(true, 0.2, group_b)];
        let (invalid, reason) = detect_invalid_cost_comparison(&rows_a, &rows_b);
        assert!(invalid);
        assert!(reason.contains("different"));

        let rows_same = vec![valid_row(true, 0.1, GROUP)];
        let (invalid2, reason2) = detect_invalid_cost_comparison(&rows_same, &rows_same);
        assert!(!invalid2);
        assert_eq!(reason2, "comparison is valid");
    }

    #[test]
    fn test_usage_ledger_row_to_value() {
        let row_json = valid_row(true, 0.5, GROUP);
        let violations = validate_usage_ledger_row(&row_json);
        assert!(violations.is_empty());
    }
}
