use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::project_board::check_allowed_files;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const CANONICAL_FAILURE_CODES: &[&str] = &[
    "F001_TIMEOUT",
    "F002_BUDGET_EXCEEDED",
    "F003_DEPENDENCY_FAILED",
    "F004_APPROVAL_REJECTED",
    "F005_PROVIDER_UNAVAILABLE",
    "F006_SCOPE_VIOLATION",
    "F007_TEST_FAILURE",
    "F008_FORMAT_ERROR",
    "F009_POLICY_VIOLATION",
    "F010_CANCELLED",
];

// ---------------------------------------------------------------------------
// Data struct
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ValidationResult {
    pub ok: bool,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl Default for ValidationResult {
    fn default() -> Self {
        Self {
            ok: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Validators
// ---------------------------------------------------------------------------

/// Validate an event against the event schema.
pub fn validate_events_schema(event: &Value) -> ValidationResult {
    if let Err(e) = crate::event_schema::validate_event(event) {
        return ValidationResult {
            ok: false,
            errors: vec![e.to_string()],
            warnings: Vec::new(),
        };
    }
    ValidationResult::default()
}

/// Validate a completion record.
pub fn validate_completion_record(record: &Value) -> ValidationResult {
    let mut errors = Vec::new();
    require_false_template(record, &mut errors);

    for field_name in &["status", "exit_code", "artifact_refs"] {
        if record.get(field_name).is_none() {
            errors.push(format!("missing required field: {}", field_name));
        }
    }
    match record.get("status").and_then(Value::as_str) {
        Some("completed") | Some("failed") => {}
        _ => errors.push("status must be completed or failed".to_string()),
    }
    if let Some(exit_code) = record.get("exit_code") {
        if !exit_code.is_i64() && !exit_code.is_u64() {
            errors.push("exit_code must be an integer".to_string());
        }
    }
    if let Some(artifact_refs) = record.get("artifact_refs") {
        if !artifact_refs.is_array() {
            errors.push("artifact_refs must be a list".to_string());
        }
    }
    to_result(errors)
}

/// Validate a handoff pack.
pub fn validate_handoff_pack(pack: &Value) -> ValidationResult {
    let mut errors = Vec::new();
    require_false_template(pack, &mut errors);

    for field_name in &["structured_fields", "summary", "evidence_refs"] {
        if pack.get(field_name).is_none() {
            errors.push(format!("missing required field: {}", field_name));
        }
    }
    match pack.get("structured_fields") {
        Some(v) if v.is_object() => {}
        _ => errors.push("structured_fields must be an object".to_string()),
    }
    match pack.get("summary").and_then(Value::as_str) {
        Some(s) if !s.is_empty() => {}
        _ => errors.push("summary must be non-empty".to_string()),
    }
    match pack.get("evidence_refs") {
        Some(Value::Array(arr)) if !arr.is_empty() => {}
        _ => errors.push("evidence_refs must be a non-empty list".to_string()),
    }
    to_result(errors)
}

/// Validate an approval request.
pub fn validate_approval_request(request: &Value) -> ValidationResult {
    let mut errors = Vec::new();
    let required = [
        "approval_id",
        "task_id",
        "risk_level",
        "requested_action",
        "summary",
        "reason",
        "affected_files",
        "options",
        "timeout_policy",
        "decision",
    ];
    for field_name in &required {
        if request.get(field_name).is_none() {
            errors.push(format!("missing required field: {}", field_name));
        }
    }
    if let Some(options) = request.get("options") {
        if !options.is_array() {
            errors.push("options must be a list".to_string());
        }
    }
    match request.get("decision").and_then(Value::as_str) {
        Some("pending") | Some("approved") | Some("rejected") | Some("deferred") => {}
        _ => errors.push("decision must be pending, approved, rejected, or deferred".to_string()),
    }
    to_result(errors)
}

/// Validate failure code against canonical set.
pub fn validate_failure_code(
    failure_code: &str,
    _failure_subcode: Option<&str>,
) -> ValidationResult {
    if CANONICAL_FAILURE_CODES.contains(&failure_code) {
        return ValidationResult::default();
    }
    ValidationResult {
        ok: false,
        errors: vec![format!("non-canonical failure_code: {}", failure_code)],
        warnings: Vec::new(),
    }
}

/// Validate allowed files completeness.
pub fn validate_allowed_files_completeness(
    allowed_files: &[String],
    required_files: &[String],
) -> ValidationResult {
    let check = check_allowed_files(allowed_files, required_files);
    if check.ok {
        return ValidationResult::default();
    }
    ValidationResult {
        ok: false,
        errors: check
            .missing_files
            .iter()
            .map(|f| format!("missing allowed file: {}", f))
            .collect(),
        warnings: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn require_false_template(record: &Value, errors: &mut Vec<String>) {
    if record.get("_template") != Some(&Value::Bool(false)) {
        errors.push("_template must be false".to_string());
    }
}

fn to_result(errors: Vec<String>) -> ValidationResult {
    ValidationResult {
        ok: errors.is_empty(),
        errors,
        warnings: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_valid_event() -> Value {
        json!({
            "event_id": "evt-1",
            "schema_version": "event.v1",
            "event_type": "test_event",
            "timestamp": "2026-01-01T00:00:00Z",
            "producer": {"component_id": "test", "component_type": "unit"},
            "correlation": {},
            "severity": "info",
            "payload": {},
            "idempotency_key": "idem-1",
            "parent_event_id": null
        })
    }

    #[test]
    fn test_validate_events_schema_valid() {
        let result = validate_events_schema(&make_valid_event());
        assert!(result.ok);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validate_events_schema_invalid() {
        let result = validate_events_schema(&json!({"foo": "bar"}));
        assert!(!result.ok);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_validate_completion_record_valid() {
        let record = json!({
            "_template": false,
            "status": "completed",
            "exit_code": 0,
            "artifact_refs": ["a.txt"]
        });
        let result = validate_completion_record(&record);
        assert!(result.ok);
    }

    #[test]
    fn test_validate_completion_record_missing_template() {
        let record = json!({
            "status": "completed",
            "exit_code": 0,
            "artifact_refs": []
        });
        let result = validate_completion_record(&record);
        assert!(!result.ok);
        assert!(result.errors.iter().any(|e| e.contains("_template")));
    }

    #[test]
    fn test_validate_completion_record_bad_status() {
        let record = json!({
            "_template": false,
            "status": "pending",
            "exit_code": 0,
            "artifact_refs": []
        });
        let result = validate_completion_record(&record);
        assert!(!result.ok);
        assert!(result.errors.iter().any(|e| e.contains("status")));
    }

    #[test]
    fn test_validate_handoff_pack_valid() {
        let pack = json!({
            "_template": false,
            "structured_fields": {"key": "val"},
            "summary": "completed the task",
            "evidence_refs": ["ref-1"]
        });
        let result = validate_handoff_pack(&pack);
        assert!(result.ok);
    }

    #[test]
    fn test_validate_handoff_pack_empty_summary() {
        let pack = json!({
            "_template": false,
            "structured_fields": {},
            "summary": "",
            "evidence_refs": ["ref-1"]
        });
        let result = validate_handoff_pack(&pack);
        assert!(!result.ok);
        assert!(result.errors.iter().any(|e| e.contains("summary")));
    }

    #[test]
    fn test_validate_approval_request_valid() {
        let request = json!({
            "approval_id": "a1",
            "task_id": "t1",
            "risk_level": "low",
            "requested_action": "deploy",
            "summary": "deploy v2",
            "reason": "new features",
            "affected_files": ["main.rs"],
            "options": ["approve", "reject"],
            "timeout_policy": "24h",
            "decision": "pending"
        });
        let result = validate_approval_request(&request);
        assert!(result.ok);
    }

    #[test]
    fn test_validate_approval_request_missing_fields() {
        let request = json!({"approval_id": "a1"});
        let result = validate_approval_request(&request);
        assert!(!result.ok);
        assert!(result.errors.len() >= 9);
    }

    #[test]
    fn test_validate_approval_request_bad_decision() {
        let request = json!({
            "approval_id": "a1",
            "task_id": "t1",
            "risk_level": "low",
            "requested_action": "deploy",
            "summary": "deploy v2",
            "reason": "new features",
            "affected_files": [],
            "options": [],
            "timeout_policy": "24h",
            "decision": "maybe"
        });
        let result = validate_approval_request(&request);
        assert!(!result.ok);
        assert!(result.errors.iter().any(|e| e.contains("decision")));
    }

    #[test]
    fn test_validate_failure_code_valid() {
        let result = validate_failure_code("F001_TIMEOUT", None);
        assert!(result.ok);
    }

    #[test]
    fn test_validate_failure_code_invalid() {
        let result = validate_failure_code("F999_MADEUP", None);
        assert!(!result.ok);
        assert!(result.errors[0].contains("non-canonical"));
    }

    #[test]
    fn test_validate_allowed_files_completeness_ok() {
        let allowed = vec!["a.rs".to_string(), "b.rs".to_string()];
        let required = vec!["a.rs".to_string()];
        let result = validate_allowed_files_completeness(&allowed, &required);
        assert!(result.ok);
    }

    #[test]
    fn test_validate_allowed_files_completeness_missing() {
        let allowed = vec!["a.rs".to_string()];
        let required = vec!["a.rs".to_string(), "c.rs".to_string()];
        let result = validate_allowed_files_completeness(&allowed, &required);
        assert!(!result.ok);
        assert!(result.errors.iter().any(|e| e.contains("c.rs")));
    }

    #[test]
    fn test_validation_result_default() {
        let result = ValidationResult::default();
        assert!(result.ok);
        assert!(result.errors.is_empty());
    }
}
