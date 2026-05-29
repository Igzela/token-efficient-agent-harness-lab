use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Schema constants
// ---------------------------------------------------------------------------

pub const SCHEMA_VERSION: &str = "user_style_mutation.v1";

pub const VARIANT_TYPES: &[&str] = &["formal_issue", "user_style_chat_request", "terse_ticket"];

pub const ADMISSION_OUTCOMES: &[&str] =
    &["admitted", "diagnostic", "needs_clarification", "rejected"];

pub const CONTAMINATION_RISKS: &[&str] = &["low", "medium", "high", "unknown"];

pub const ADMISSION_SCOPES: &[&str] = &["admitted", "diagnostic"];

pub const SOURCE_TYPES: &[&str] = &["synthetic", "copied_real_read_only", "mutated_user_style"];

pub const REQUIRED_MUTATION_FIELDS: &[&str] = &[
    "case_id",
    "base_fixture_id",
    "variant_type",
    "user_prompt",
    "expected_task_family",
    "expected_required_fields",
    "expected_missing_fields",
    "admission_expectation",
    "evidence_refs",
    "fixture_metadata",
];

pub const REQUIRED_METADATA_FIELDS: &[&str] = &[
    "fixture_id",
    "source_type",
    "freshness",
    "estimated_human_minutes",
    "difficulty",
    "contamination_risk",
    "admission_scope",
];

// ---------------------------------------------------------------------------
// Data structs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FixtureMetadata {
    pub fixture_id: String,
    pub source_type: String,
    pub freshness: String,
    pub estimated_human_minutes: f64,
    pub difficulty: String,
    pub contamination_risk: String,
    pub admission_scope: String,
}

impl Default for FixtureMetadata {
    fn default() -> Self {
        Self {
            fixture_id: String::new(),
            source_type: "mutated_user_style".to_string(),
            freshness: "2026-05-19".to_string(),
            estimated_human_minutes: 0.0,
            difficulty: "unknown".to_string(),
            contamination_risk: "low".to_string(),
            admission_scope: "admitted".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MutationCase {
    pub case_id: String,
    pub base_fixture_id: String,
    pub variant_type: String,
    pub user_prompt: String,
    pub expected_task_family: String,
    pub expected_required_fields: Vec<String>,
    pub expected_missing_fields: Vec<String>,
    pub admission_expectation: String,
    pub evidence_refs: Vec<String>,
    pub fixture_metadata: FixtureMetadata,
    pub schema_version: String,
}

impl Default for MutationCase {
    fn default() -> Self {
        Self {
            case_id: String::new(),
            base_fixture_id: String::new(),
            variant_type: String::new(),
            user_prompt: String::new(),
            expected_task_family: String::new(),
            expected_required_fields: Vec::new(),
            expected_missing_fields: Vec::new(),
            admission_expectation: String::new(),
            evidence_refs: Vec::new(),
            fixture_metadata: FixtureMetadata::default(),
            schema_version: SCHEMA_VERSION.to_string(),
        }
    }
}

impl MutationCase {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("MutationCase should serialize to JSON")
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

pub fn validate_fixture_metadata(data: &Value) -> Vec<String> {
    let mut violations = Vec::new();

    for &f in REQUIRED_METADATA_FIELDS {
        if data.get(f).is_none() {
            violations.push(format!("fixture_metadata missing required field: {}", f));
        }
    }
    if !violations.is_empty() {
        return violations;
    }

    if let Some(st) = data["source_type"].as_str() {
        if !SOURCE_TYPES.contains(&st) {
            violations.push(format!("source_type {:?} not in {:?}", st, SOURCE_TYPES));
        }
    }
    if let Some(cr) = data["contamination_risk"].as_str() {
        if !CONTAMINATION_RISKS.contains(&cr) {
            violations.push(format!(
                "contamination_risk {:?} not in {:?}",
                cr, CONTAMINATION_RISKS
            ));
        }
    }
    if let Some(as_val) = data["admission_scope"].as_str() {
        if !ADMISSION_SCOPES.contains(&as_val) {
            violations.push(format!(
                "admission_scope {:?} not in {:?}",
                as_val, ADMISSION_SCOPES
            ));
        }
    }
    let ehm = &data["estimated_human_minutes"];
    if !ehm.is_number() {
        violations.push("estimated_human_minutes must be numeric".to_string());
    }

    violations
}

pub fn validate_mutation_case(data: &Value) -> Vec<String> {
    let mut violations = Vec::new();

    for &f in REQUIRED_MUTATION_FIELDS {
        if data.get(f).is_none() {
            violations.push(format!("missing required field: {}", f));
        }
    }
    if !violations.is_empty() {
        return violations;
    }

    // schema_version
    if data["schema_version"].as_str() != Some(SCHEMA_VERSION) {
        violations.push(format!(
            "schema_version must be {}, got {:?}",
            SCHEMA_VERSION,
            data["schema_version"].as_str()
        ));
    }

    // variant_type
    if let Some(vt) = data["variant_type"].as_str() {
        if !VARIANT_TYPES.contains(&vt) {
            violations.push(format!("variant_type {:?} not in {:?}", vt, VARIANT_TYPES));
        }
    }

    // admission_expectation
    if let Some(ae) = data["admission_expectation"].as_str() {
        if !ADMISSION_OUTCOMES.contains(&ae) {
            violations.push(format!(
                "admission_expectation {:?} not in {:?}",
                ae, ADMISSION_OUTCOMES
            ));
        }
    }

    // evidence_refs must be a list
    if data.get("evidence_refs").is_some() && !data["evidence_refs"].is_array() {
        violations.push("evidence_refs must be a list".to_string());
    }

    // expected_required_fields must be a list
    if data.get("expected_required_fields").is_some()
        && !data["expected_required_fields"].is_array()
    {
        violations.push("expected_required_fields must be a list".to_string());
    }

    // expected_missing_fields must be a list
    if data.get("expected_missing_fields").is_some() && !data["expected_missing_fields"].is_array()
    {
        violations.push("expected_missing_fields must be a list".to_string());
    }

    // fixture_metadata sub-validation
    match data.get("fixture_metadata") {
        Some(Value::Object(_)) => {
            violations.extend(validate_fixture_metadata(&data["fixture_metadata"]));
        }
        _ => violations.push("fixture_metadata must be a dict".to_string()),
    }

    violations
}

#[allow(clippy::too_many_arguments)]
pub fn create_mutation_case(
    case_id: &str,
    base_fixture_id: &str,
    variant_type: &str,
    user_prompt: &str,
    expected_task_family: &str,
    expected_required_fields: Vec<String>,
    expected_missing_fields: Vec<String>,
    admission_expectation: &str,
    evidence_refs: Vec<String>,
    meta: Option<&Value>,
) -> MutationCase {
    let meta = meta.cloned().unwrap_or_else(|| json!({}));
    let get_str = |key: &str, default: &str| -> String {
        meta.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or(default)
            .to_string()
    };
    let get_f64 = |key: &str, default: f64| -> f64 {
        meta.get(key).and_then(|v| v.as_f64()).unwrap_or(default)
    };

    MutationCase {
        case_id: case_id.to_string(),
        base_fixture_id: base_fixture_id.to_string(),
        variant_type: variant_type.to_string(),
        user_prompt: user_prompt.to_string(),
        expected_task_family: expected_task_family.to_string(),
        expected_required_fields,
        expected_missing_fields,
        admission_expectation: admission_expectation.to_string(),
        evidence_refs,
        fixture_metadata: FixtureMetadata {
            fixture_id: get_str("fixture_id", case_id),
            source_type: get_str("source_type", "mutated_user_style"),
            freshness: get_str("freshness", "2026-05-19"),
            estimated_human_minutes: get_f64("estimated_human_minutes", 0.0),
            difficulty: get_str("difficulty", "unknown"),
            contamination_risk: get_str("contamination_risk", "low"),
            admission_scope: get_str("admission_scope", "admitted"),
        },
        schema_version: SCHEMA_VERSION.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Admission grouping
// ---------------------------------------------------------------------------

pub type FixtureEntry = (String, Value, Vec<String>);

pub fn group_by_admission(fixtures: &[FixtureEntry]) -> HashMap<String, Vec<Value>> {
    let mut groups: HashMap<String, Vec<Value>> = HashMap::new();
    for outcome in ADMISSION_OUTCOMES {
        groups.insert(outcome.to_string(), Vec::new());
    }
    for (_filename, data, violations) in fixtures {
        if violations.is_empty() {
            if let Some(outcome) = data.get("admission_expectation").and_then(|v| v.as_str()) {
                groups
                    .entry(outcome.to_string())
                    .or_default()
                    .push(data.clone());
            }
        }
    }
    groups
}

pub fn group_by_variant(fixtures: &[FixtureEntry]) -> HashMap<String, Vec<Value>> {
    let mut groups: HashMap<String, Vec<Value>> = HashMap::new();
    for vt in VARIANT_TYPES {
        groups.insert(vt.to_string(), Vec::new());
    }
    for (_filename, data, violations) in fixtures {
        if violations.is_empty() {
            if let Some(vt) = data.get("variant_type").and_then(|v| v.as_str()) {
                groups.entry(vt.to_string()).or_default().push(data.clone());
            }
        }
    }
    groups
}

pub fn group_by_base_fixture(fixtures: &[FixtureEntry]) -> HashMap<String, Vec<Value>> {
    let mut groups: HashMap<String, Vec<Value>> = HashMap::new();
    for (_filename, data, violations) in fixtures {
        if violations.is_empty() {
            if let Some(base) = data.get("base_fixture_id").and_then(|v| v.as_str()) {
                groups
                    .entry(base.to_string())
                    .or_default()
                    .push(data.clone());
            }
        }
    }
    groups
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_metadata() -> Value {
        json!({
            "fixture_id": "f001",
            "source_type": "mutated_user_style",
            "freshness": "2026-05-19",
            "estimated_human_minutes": 5.0,
            "difficulty": "medium",
            "contamination_risk": "low",
            "admission_scope": "admitted",
        })
    }

    fn valid_mutation_case() -> Value {
        json!({
            "schema_version": SCHEMA_VERSION,
            "case_id": "mc001",
            "base_fixture_id": "bf001",
            "variant_type": "formal_issue",
            "user_prompt": "Please fix the bug in module X",
            "expected_task_family": "bug_fix",
            "expected_required_fields": ["file_path"],
            "expected_missing_fields": [],
            "admission_expectation": "admitted",
            "evidence_refs": ["e1"],
            "fixture_metadata": valid_metadata(),
        })
    }

    #[test]
    fn test_validate_valid_metadata() {
        let v = valid_metadata();
        let violations = validate_fixture_metadata(&v);
        assert!(violations.is_empty(), "violations: {:?}", violations);
    }

    #[test]
    fn test_validate_metadata_bad_source_type() {
        let mut v = valid_metadata();
        v["source_type"] = json!("invalid_source");
        let violations = validate_fixture_metadata(&v);
        assert!(violations.iter().any(|v| v.contains("source_type")));
    }

    #[test]
    fn test_validate_metadata_bad_contamination_risk() {
        let mut v = valid_metadata();
        v["contamination_risk"] = json!("extreme");
        let violations = validate_fixture_metadata(&v);
        assert!(violations.iter().any(|v| v.contains("contamination_risk")));
    }

    #[test]
    fn test_validate_metadata_non_numeric_minutes() {
        let mut v = valid_metadata();
        v["estimated_human_minutes"] = json!("not_a_number");
        let violations = validate_fixture_metadata(&v);
        assert!(violations
            .iter()
            .any(|v| v.contains("estimated_human_minutes")));
    }

    #[test]
    fn test_validate_valid_mutation_case() {
        let v = valid_mutation_case();
        let violations = validate_mutation_case(&v);
        assert!(violations.is_empty(), "violations: {:?}", violations);
    }

    #[test]
    fn test_validate_mutation_case_bad_variant_type() {
        let mut v = valid_mutation_case();
        v["variant_type"] = json!("invalid_variant");
        let violations = validate_mutation_case(&v);
        assert!(violations.iter().any(|v| v.contains("variant_type")));
    }

    #[test]
    fn test_validate_mutation_case_bad_admission_expectation() {
        let mut v = valid_mutation_case();
        v["admission_expectation"] = json!("invalid_outcome");
        let violations = validate_mutation_case(&v);
        assert!(violations
            .iter()
            .any(|v| v.contains("admission_expectation")));
    }

    #[test]
    fn test_validate_mutation_case_missing_field() {
        let mut v = valid_mutation_case();
        v.as_object_mut().unwrap().remove("case_id");
        let violations = validate_mutation_case(&v);
        assert!(violations.iter().any(|v| v.contains("case_id")));
    }

    #[test]
    fn test_create_mutation_case_defaults() {
        let mc = create_mutation_case(
            "mc1",
            "bf1",
            "formal_issue",
            "fix bug",
            "bug_fix",
            vec![],
            vec![],
            "admitted",
            vec![],
            None,
        );
        assert_eq!(mc.case_id, "mc1");
        assert_eq!(mc.fixture_metadata.source_type, "mutated_user_style");
        assert_eq!(mc.fixture_metadata.contamination_risk, "low");
        assert_eq!(mc.schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn test_create_mutation_case_with_meta() {
        let meta = json!({
            "source_type": "synthetic",
            "contamination_risk": "high",
            "estimated_human_minutes": 10.0,
        });
        let mc = create_mutation_case(
            "mc2",
            "bf2",
            "terse_ticket",
            "fix it",
            "bug_fix",
            vec!["field_a".to_string()],
            vec!["field_b".to_string()],
            "diagnostic",
            vec!["e1".to_string()],
            Some(&meta),
        );
        assert_eq!(mc.fixture_metadata.source_type, "synthetic");
        assert_eq!(mc.fixture_metadata.contamination_risk, "high");
        assert_eq!(mc.fixture_metadata.estimated_human_minutes, 10.0);
    }

    #[test]
    fn test_group_by_admission() {
        let fixtures: Vec<FixtureEntry> = vec![
            (
                "f1.json".to_string(),
                json!({"admission_expectation": "admitted", "variant_type": "formal_issue", "base_fixture_id": "bf1"}),
                vec![],
            ),
            (
                "f2.json".to_string(),
                json!({"admission_expectation": "diagnostic", "variant_type": "terse_ticket", "base_fixture_id": "bf2"}),
                vec![],
            ),
            (
                "f3.json".to_string(),
                json!({"admission_expectation": "admitted", "variant_type": "formal_issue", "base_fixture_id": "bf1"}),
                vec!["error".to_string()],
            ),
        ];
        let groups = group_by_admission(&fixtures);
        assert_eq!(groups["admitted"].len(), 1);
        assert_eq!(groups["diagnostic"].len(), 1);
        assert_eq!(groups["rejected"].len(), 0);
    }

    #[test]
    fn test_group_by_variant() {
        let fixtures: Vec<FixtureEntry> = vec![
            (
                "f1.json".to_string(),
                json!({"admission_expectation": "admitted", "variant_type": "formal_issue", "base_fixture_id": "bf1"}),
                vec![],
            ),
            (
                "f2.json".to_string(),
                json!({"admission_expectation": "admitted", "variant_type": "formal_issue", "base_fixture_id": "bf2"}),
                vec![],
            ),
            (
                "f3.json".to_string(),
                json!({"admission_expectation": "admitted", "variant_type": "terse_ticket", "base_fixture_id": "bf3"}),
                vec![],
            ),
        ];
        let groups = group_by_variant(&fixtures);
        assert_eq!(groups["formal_issue"].len(), 2);
        assert_eq!(groups["terse_ticket"].len(), 1);
        assert_eq!(groups["user_style_chat_request"].len(), 0);
    }

    #[test]
    fn test_group_by_base_fixture() {
        let fixtures: Vec<FixtureEntry> = vec![
            (
                "f1.json".to_string(),
                json!({"admission_expectation": "admitted", "variant_type": "formal_issue", "base_fixture_id": "bf1"}),
                vec![],
            ),
            (
                "f2.json".to_string(),
                json!({"admission_expectation": "admitted", "variant_type": "terse_ticket", "base_fixture_id": "bf1"}),
                vec![],
            ),
            (
                "f3.json".to_string(),
                json!({"admission_expectation": "admitted", "variant_type": "formal_issue", "base_fixture_id": "bf2"}),
                vec![],
            ),
        ];
        let groups = group_by_base_fixture(&fixtures);
        assert_eq!(groups["bf1"].len(), 2);
        assert_eq!(groups["bf2"].len(), 1);
    }

    #[test]
    fn test_mutation_case_struct_default() {
        let mc = MutationCase::default();
        assert_eq!(mc.schema_version, SCHEMA_VERSION);
        assert!(mc.case_id.is_empty());
    }

    #[test]
    fn test_fixture_metadata_struct_default() {
        let fm = FixtureMetadata::default();
        assert_eq!(fm.source_type, "mutated_user_style");
        assert_eq!(fm.contamination_risk, "low");
        assert_eq!(fm.admission_scope, "admitted");
    }
}
