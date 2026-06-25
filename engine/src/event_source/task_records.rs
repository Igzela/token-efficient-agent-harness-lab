use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const REQUIRED_TASK_RECORD_FILES: &[&str] = &[
    "task_spec.json",
    "completion.json",
    "handoff_pack.json",
    "events.jsonl",
];

// ---------------------------------------------------------------------------
// Data structs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TaskRecordBundle {
    pub task_dir: PathBuf,
    pub task_spec: Value,
    pub completion: Value,
    pub handoff_pack: Value,
    pub events_path: PathBuf,
    pub run_log_path: Option<PathBuf>,
    pub run_log_text: Option<String>,
}

impl Default for TaskRecordBundle {
    fn default() -> Self {
        Self {
            task_dir: PathBuf::new(),
            task_spec: Value::Null,
            completion: Value::Null,
            handoff_pack: Value::Null,
            events_path: PathBuf::new(),
            run_log_path: None,
            run_log_text: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TaskRecordValidationReport {
    pub task_dir: PathBuf,
    #[allow(dead_code)]
    pub ok: bool,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    pub bundle: Option<TaskRecordBundle>,
}

impl Default for TaskRecordValidationReport {
    fn default() -> Self {
        Self {
            task_dir: PathBuf::new(),
            ok: true,
            errors: Vec::new(),
            warnings: Vec::new(),
            bundle: None,
        }
    }
}

// ---------------------------------------------------------------------------
// TaskRecordStore — in-memory store
// ---------------------------------------------------------------------------

pub struct TaskRecordStore {
    pub root_path: PathBuf,
    pub bundles: Vec<TaskRecordBundle>,
}

impl Default for TaskRecordStore {
    fn default() -> Self {
        Self {
            root_path: PathBuf::new(),
            bundles: Vec::new(),
        }
    }
}

impl TaskRecordStore {
    pub fn new(root_path: PathBuf) -> Self {
        Self {
            root_path,
            bundles: Vec::new(),
        }
    }

    pub fn load_bundle(
        &mut self,
        task_dir: PathBuf,
        task_spec: Value,
        completion: Value,
        handoff_pack: Value,
        events_path: PathBuf,
        run_log_path: Option<PathBuf>,
        run_log_text: Option<String>,
    ) -> Result<&TaskRecordBundle, String> {
        let missing: Vec<&str> = REQUIRED_TASK_RECORD_FILES
            .iter()
            .copied()
            .filter(|name| {
                let p = task_dir.join(name);
                !p.exists()
            })
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "task record missing required file(s): {}",
                missing.join(", ")
            ));
        }

        let bundle = TaskRecordBundle {
            task_dir,
            task_spec,
            completion,
            handoff_pack,
            events_path,
            run_log_path,
            run_log_text,
        };
        self.bundles.push(bundle);
        Ok(self.bundles.last().unwrap())
    }

    pub fn validate_bundle(&self, bundle: &TaskRecordBundle) -> TaskRecordValidationReport {
        let mut errors: Vec<String> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();

        if bundle.task_dir.exists() {
            let missing: Vec<&str> = REQUIRED_TASK_RECORD_FILES
                .iter()
                .copied()
                .filter(|name| !bundle.task_dir.join(name).exists())
                .collect();
            for name in &missing {
                errors.push(format!("missing required file: {}", name));
            }
            if !errors.is_empty() {
                return TaskRecordValidationReport {
                    task_dir: bundle.task_dir.clone(),
                    ok: false,
                    errors,
                    warnings,
                    bundle: None,
                };
            }
        }

        let completion_result = validate_completion_record(&bundle.completion);
        for err in &completion_result.errors {
            errors.push(format!("completion.json: {}", err));
        }
        for warn in &completion_result.warnings {
            warnings.push(format!("completion.json: {}", warn));
        }

        let handoff_result = validate_handoff_pack(&bundle.handoff_pack);
        for err in &handoff_result.errors {
            errors.push(format!("handoff_pack.json: {}", err));
        }
        for warn in &handoff_result.warnings {
            warnings.push(format!("handoff_pack.json: {}", warn));
        }

        TaskRecordValidationReport {
            task_dir: bundle.task_dir.clone(),
            ok: errors.is_empty(),
            errors,
            warnings,
            bundle: Some(bundle.clone()),
        }
    }

    pub fn get_bundle(&self, index: usize) -> Option<&TaskRecordBundle> {
        self.bundles.get(index)
    }

    pub fn len(&self) -> usize {
        self.bundles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bundles.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Validation helpers (inline, mirrors validators.py logic)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
struct ValidationResult {
    ok: bool,
    errors: Vec<String>,
    warnings: Vec<String>,
}

fn validate_completion_record(record: &Value) -> ValidationResult {
    let mut errors = Vec::new();

    if record.get("_template") != Some(&Value::Bool(false)) {
        errors.push("_template must be false".to_string());
    }
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
        if !exit_code.is_number() {
            errors.push("exit_code must be an integer".to_string());
        }
    }
    if let Some(artifact_refs) = record.get("artifact_refs") {
        if !artifact_refs.is_array() {
            errors.push("artifact_refs must be a list".to_string());
        }
    }

    ValidationResult {
        ok: errors.is_empty(),
        errors,
        warnings: Vec::new(),
    }
}

fn validate_handoff_pack(pack: &Value) -> ValidationResult {
    let mut errors = Vec::new();

    if pack.get("_template") != Some(&Value::Bool(false)) {
        errors.push("_template must be false".to_string());
    }
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

    fn make_bundle() -> TaskRecordBundle {
        TaskRecordBundle {
            task_dir: PathBuf::from("/tmp/test_task"),
            task_spec: json!({"task_id": "t1", "description": "test"}),
            completion: json!({"_template": false, "status": "completed", "exit_code": 0, "artifact_refs": ["a.txt"]}),
            handoff_pack: json!({"_template": false, "structured_fields": {"key": "val"}, "summary": "done", "evidence_refs": ["e1"]}),
            events_path: PathBuf::from("/tmp/test_task/events.jsonl"),
            run_log_path: None,
            run_log_text: None,
        }
    }

    #[test]
    fn test_task_record_store_default() {
        let store = TaskRecordStore::default();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn test_task_record_bundle_default() {
        let bundle = TaskRecordBundle::default();
        assert_eq!(bundle.task_dir, PathBuf::new());
        assert_eq!(bundle.task_spec, Value::Null);
    }

    #[test]
    fn test_validate_bundle_completion_missing_template() {
        let store = TaskRecordStore::default();
        let mut bundle = make_bundle();
        bundle.completion = json!({"status": "completed", "exit_code": 0, "artifact_refs": []});
        let report = store.validate_bundle(&bundle);
        assert!(!report.ok);
        assert!(report.errors.iter().any(|e| e.contains("_template")));
    }

    #[test]
    fn test_validate_bundle_handoff_missing_fields() {
        let store = TaskRecordStore::default();
        let mut bundle = make_bundle();
        bundle.handoff_pack = json!({"_template": false});
        let report = store.validate_bundle(&bundle);
        assert!(!report.ok);
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("structured_fields")));
        assert!(report.errors.iter().any(|e| e.contains("summary")));
    }

    #[test]
    fn test_validate_bundle_valid() {
        let store = TaskRecordStore::default();
        let bundle = make_bundle();
        let report = store.validate_bundle(&bundle);
        assert!(report.ok);
        assert!(report.errors.is_empty());
    }

    #[test]
    fn test_validate_completion_invalid_status() {
        let store = TaskRecordStore::default();
        let mut bundle = make_bundle();
        bundle.completion =
            json!({"_template": false, "status": "pending", "exit_code": 0, "artifact_refs": []});
        let report = store.validate_bundle(&bundle);
        assert!(!report.ok);
        assert!(report.errors.iter().any(|e| e.contains("status must be")));
    }

    #[test]
    fn test_validate_handoff_pack_empty_evidence_refs() {
        let store = TaskRecordStore::default();
        let mut bundle = make_bundle();
        bundle.handoff_pack = json!({"_template": false, "structured_fields": {}, "summary": "ok", "evidence_refs": []});
        let report = store.validate_bundle(&bundle);
        assert!(!report.ok);
        assert!(report.errors.iter().any(|e| e.contains("evidence_refs")));
    }

    #[test]
    fn test_task_record_validation_report_default() {
        let report = TaskRecordValidationReport::default();
        assert!(report.ok);
        assert!(report.errors.is_empty());
        assert!(report.bundle.is_none());
    }
}
