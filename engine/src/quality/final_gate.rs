use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FinalGateDecision {
    pub result: String,
    pub next_project_status: String,
    pub reasons: Vec<String>,
    pub evidence_refs: Vec<String>,
}

impl Default for FinalGateDecision {
    fn default() -> Self {
        Self {
            result: "fail".to_string(),
            next_project_status: "review".to_string(),
            reasons: Vec::new(),
            evidence_refs: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn failed_review(reasons: Vec<String>, evidence_refs: Vec<String>) -> FinalGateDecision {
    FinalGateDecision {
        result: "fail".to_string(),
        next_project_status: "review".to_string(),
        reasons,
        evidence_refs,
    }
}

fn collect_evidence_refs(handoff_pack: &Value, run_log_present: bool) -> Vec<String> {
    let mut refs: Vec<String> = Vec::new();
    if let Some(evidence) = handoff_pack.get("evidence_refs").and_then(Value::as_array) {
        for ref_val in evidence {
            if let Some(obj) = ref_val.as_object() {
                if let Some(path) = obj.get("path").and_then(Value::as_str) {
                    if !path.is_empty() {
                        refs.push(path.to_string());
                    }
                }
            } else if let Some(s) = ref_val.as_str() {
                refs.push(s.to_string());
            }
        }
    }
    if run_log_present {
        refs.push("run_log.md".to_string());
    }
    refs
}

fn pending_approval_reason(handoff_pack: &Value) -> Option<String> {
    for request in walk_approval_requests(handoff_pack) {
        let has_approval_id = request.get("approval_id").map_or(false, |v| !v.is_null());
        let decision = request.get("decision").and_then(Value::as_str);
        if has_approval_id && decision == Some("pending") {
            let approval_id = request
                .get("approval_id")
                .and_then(Value::as_str)
                .unwrap_or("<unknown>");
            return Some(format!(
                "approval_request {} is pending; Final Gate did not execute approval",
                approval_id
            ));
        }
    }
    None
}

fn walk_approval_requests(value: &Value) -> Vec<&Value> {
    let mut results = Vec::new();
    walk_approval_requests_inner(value, &mut results);
    results
}

fn walk_approval_requests_inner<'a>(value: &'a Value, results: &mut Vec<&'a Value>) {
    match value {
        Value::Object(map) => {
            if map.contains_key("approval_id") && map.contains_key("decision") {
                results.push(value);
            }
            for child in map.values() {
                walk_approval_requests_inner(child, results);
            }
        }
        Value::Array(arr) => {
            for child in arr {
                walk_approval_requests_inner(child, results);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Validation helpers (minimal, deterministic)
// ---------------------------------------------------------------------------

fn validate_completion_record(completion: &Value) -> (bool, Vec<String>, Vec<String>) {
    let mut errors: Vec<String> = Vec::new();
    let warnings: Vec<String> = Vec::new();

    if completion.is_null() || !completion.is_object() {
        errors.push("completion is not a JSON object".to_string());
        return (false, errors, warnings);
    }

    for field_name in &["status", "exit_code", "artifact_refs"] {
        if completion.get(field_name).is_none() {
            errors.push(format!("missing required field: {}", field_name));
        }
    }

    let valid_statuses = ["completed", "failed"];
    let status = completion.get("status").and_then(Value::as_str);
    if !status.map_or(false, |s| valid_statuses.contains(&s)) {
        errors.push("status must be completed or failed".to_string());
    }

    (errors.is_empty(), errors, warnings)
}

fn validate_handoff_pack(handoff_pack: &Value) -> (bool, Vec<String>, Vec<String>) {
    let mut errors: Vec<String> = Vec::new();
    let warnings: Vec<String> = Vec::new();

    if handoff_pack.is_null() || !handoff_pack.is_object() {
        errors.push("handoff_pack is not a JSON object".to_string());
        return (false, errors, warnings);
    }

    for field_name in &["structured_fields", "summary", "evidence_refs"] {
        if handoff_pack.get(field_name).is_none() {
            errors.push(format!("missing required field: {}", field_name));
        }
    }

    (errors.is_empty(), errors, warnings)
}

// ---------------------------------------------------------------------------
// FinalGateRunner
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct FinalGateRunner;

impl FinalGateRunner {
    pub fn new() -> Self {
        Self
    }

    pub fn evaluate(
        &self,
        completion: &Value,
        handoff_pack: &Value,
        run_log_present: bool,
        current_item_status: &str,
    ) -> FinalGateDecision {
        let evidence_refs = collect_evidence_refs(handoff_pack, run_log_present);

        if current_item_status != "review" {
            return FinalGateDecision {
                result: "fail".to_string(),
                next_project_status: "review".to_string(),
                reasons: vec![format!(
                    "project item must be review before Final Gate, got {}",
                    current_item_status
                )],
                evidence_refs,
            };
        }

        let (completion_ok, completion_errors, completion_warnings) =
            validate_completion_record(completion);
        if !completion_ok {
            return failed_review(
                completion_errors
                    .iter()
                    .map(|e| format!("completion.json: {}", e))
                    .collect(),
                evidence_refs,
            );
        }

        let (handoff_ok, handoff_errors, handoff_warnings) = validate_handoff_pack(handoff_pack);
        if !handoff_ok {
            return failed_review(
                handoff_errors
                    .iter()
                    .map(|e| format!("handoff_pack.json: {}", e))
                    .collect(),
                evidence_refs,
            );
        }

        if let Some(block_reason) = pending_approval_reason(handoff_pack) {
            return FinalGateDecision {
                result: "fail".to_string(),
                next_project_status: "review".to_string(),
                reasons: vec![block_reason],
                evidence_refs,
            };
        }

        let status = completion.get("status").and_then(Value::as_str);
        let exit_code = completion.get("exit_code").and_then(Value::as_i64);
        if status != Some("completed") || exit_code != Some(0) {
            return FinalGateDecision {
                result: "fail".to_string(),
                next_project_status: "failed".to_string(),
                reasons: vec![
                    "task completion did not report completed with exit_code 0".to_string()
                ],
                evidence_refs,
            };
        }

        let mut warnings: Vec<String> = Vec::new();
        if !run_log_present {
            warnings.push(
                "run_log.md not present; treating task record as pass_with_notes".to_string(),
            );
        }
        warnings.extend(completion_warnings);
        warnings.extend(handoff_warnings);

        if !warnings.is_empty() {
            return FinalGateDecision {
                result: "pass_with_notes".to_string(),
                next_project_status: "review".to_string(),
                reasons: warnings,
                evidence_refs,
            };
        }

        FinalGateDecision {
            result: "pass".to_string(),
            next_project_status: "done".to_string(),
            reasons: vec!["completion and handoff evidence passed Final Gate".to_string()],
            evidence_refs,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn runner() -> FinalGateRunner {
        FinalGateRunner::new()
    }

    fn good_completion() -> Value {
        json!({"status": "completed", "exit_code": 0, "artifact_refs": []})
    }

    fn good_handoff() -> Value {
        json!({
            "structured_fields": {"k": "v"},
            "summary": "done",
            "evidence_refs": [{"path": "evidence.md"}]
        })
    }

    #[test]
    fn test_pass_full() {
        let decision = runner().evaluate(&good_completion(), &good_handoff(), true, "review");
        assert_eq!(decision.result, "pass");
        assert_eq!(decision.next_project_status, "done");
    }

    #[test]
    fn test_fail_not_review_status() {
        let decision = runner().evaluate(&good_completion(), &good_handoff(), true, "ready");
        assert_eq!(decision.result, "fail");
        assert_eq!(decision.next_project_status, "review");
        assert!(decision.reasons[0].contains("must be review"));
    }

    #[test]
    fn test_fail_missing_completion_fields() {
        let completion = json!({"status": "completed"});
        let decision = runner().evaluate(&completion, &good_handoff(), true, "review");
        assert_eq!(decision.result, "fail");
        assert!(decision
            .reasons
            .iter()
            .any(|r| r.contains("completion.json")));
    }

    #[test]
    fn test_fail_pending_approval() {
        let handoff = json!({
            "structured_fields": {"k": "v"},
            "summary": "done",
            "evidence_refs": [{"path": "e.md"}],
            "approval_requests": [
                {"approval_id": "A1", "decision": "pending"}
            ]
        });
        let decision = runner().evaluate(&good_completion(), &handoff, true, "review");
        assert_eq!(decision.result, "fail");
        assert!(decision.reasons[0].contains("A1"));
        assert!(decision.reasons[0].contains("pending"));
    }

    #[test]
    fn test_fail_non_zero_exit() {
        let completion = json!({"status": "completed", "exit_code": 1, "artifact_refs": []});
        let decision = runner().evaluate(&completion, &good_handoff(), true, "review");
        assert_eq!(decision.result, "fail");
        assert_eq!(decision.next_project_status, "failed");
    }

    #[test]
    fn test_pass_with_notes_missing_run_log() {
        let decision = runner().evaluate(&good_completion(), &good_handoff(), false, "review");
        assert_eq!(decision.result, "pass_with_notes");
        assert_eq!(decision.next_project_status, "review");
        assert!(decision.reasons.iter().any(|r| r.contains("run_log")));
    }

    #[test]
    fn test_evidence_refs_collected() {
        let handoff = json!({
            "structured_fields": {},
            "summary": "ok",
            "evidence_refs": [
                {"path": "a.md"},
                {"path": "b.md"}
            ]
        });
        let decision = runner().evaluate(&good_completion(), &handoff, true, "review");
        assert!(decision.evidence_refs.contains(&"a.md".to_string()));
        assert!(decision.evidence_refs.contains(&"b.md".to_string()));
        assert!(decision.evidence_refs.contains(&"run_log.md".to_string()));
    }

    #[test]
    fn test_walk_approval_requests_nested() {
        let value = json!({
            "level1": {
                "level2": [
                    {"approval_id": "X1", "decision": "approved"},
                    {"approval_id": "X2", "decision": "pending"}
                ]
            }
        });
        let requests = walk_approval_requests(&value);
        assert_eq!(requests.len(), 2);
    }

    #[test]
    fn test_decision_default() {
        let d = FinalGateDecision::default();
        assert_eq!(d.result, "fail");
        assert_eq!(d.next_project_status, "review");
        assert!(d.reasons.is_empty());
    }

    #[test]
    fn test_decision_serializes_roundtrip() {
        let d = FinalGateDecision {
            result: "pass".to_string(),
            next_project_status: "done".to_string(),
            reasons: vec!["ok".to_string()],
            evidence_refs: vec!["e.md".to_string()],
        };
        let json_str = serde_json::to_string(&d).unwrap();
        let deserialized: FinalGateDecision = serde_json::from_str(&json_str).unwrap();
        assert_eq!(d, deserialized);
    }
}
