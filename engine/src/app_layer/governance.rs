use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Schema version
// ---------------------------------------------------------------------------

pub const GOVERNANCE_DECISION_VERSION: &str = "governance_decision.v1";

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

pub const DECISION_VALUES: &[&str] = &[
    "approve_activation",
    "reject_activation",
    "defer_activation",
    "require_more_evidence",
];

pub const GATE_RESULTS: &[&str] = &["pass", "fail"];

pub const USER_PROJECT_INDICATORS: &[&str] = &[
    "/src/",
    "/lib/",
    "/app/",
    "/home/",
    "/Users/",
    "/workspace/",
];

// ---------------------------------------------------------------------------
// Required fields
// ---------------------------------------------------------------------------

pub const GOVERNANCE_DECISION_REQUIRED: &[&str] = &[
    "schema_version",
    "decision_id",
    "candidate_id",
    "policy_id",
    "decision",
    "decision_basis",
    "gate_results",
    "blocked_reasons",
    "allowed_next_actions",
    "forbidden_next_actions",
    "decided_by",
    "decided_at",
];

pub const DECISION_BASIS_REQUIRED: &[&str] = &[
    "admitted_evidence_refs",
    "diagnostic_evidence_refs",
    "approval_ref",
    "rollback_plan_ref",
    "registry_entry_ref",
];

pub const GATE_RESULTS_REQUIRED: &[&str] = &[
    "evidence_gate",
    "approval_gate",
    "rollback_gate",
    "scope_gate",
    "unknown_error_gate",
];

// ---------------------------------------------------------------------------
// Data structs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GateResult {
    pub result: String,
    pub reason: String,
}

impl Default for GateResult {
    fn default() -> Self {
        Self {
            result: "fail".to_string(),
            reason: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GovernanceDecision {
    pub schema_version: String,
    pub decision_id: String,
    pub candidate_id: String,
    pub policy_id: String,
    pub decision: String,
    pub decision_basis: DecisionBasis,
    pub gate_results: GovernanceGateResults,
    pub blocked_reasons: Vec<String>,
    pub allowed_next_actions: Vec<String>,
    pub forbidden_next_actions: Vec<String>,
    pub decided_by: String,
    pub decided_at: String,
}

impl Default for GovernanceDecision {
    fn default() -> Self {
        Self {
            schema_version: GOVERNANCE_DECISION_VERSION.to_string(),
            decision_id: String::new(),
            candidate_id: String::new(),
            policy_id: String::new(),
            decision: String::new(),
            decision_basis: DecisionBasis::default(),
            gate_results: GovernanceGateResults::default(),
            blocked_reasons: Vec::new(),
            allowed_next_actions: Vec::new(),
            forbidden_next_actions: Vec::new(),
            decided_by: "governance_engine".to_string(),
            decided_at: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DecisionBasis {
    pub admitted_evidence_refs: Vec<Value>,
    pub diagnostic_evidence_refs: Vec<Value>,
    pub approval_ref: String,
    pub rollback_plan_ref: String,
    pub registry_entry_ref: String,
}

#[allow(clippy::derivable_impls)]
impl Default for DecisionBasis {
    fn default() -> Self {
        Self {
            admitted_evidence_refs: Vec::new(),
            diagnostic_evidence_refs: Vec::new(),
            approval_ref: String::new(),
            rollback_plan_ref: String::new(),
            registry_entry_ref: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GovernanceGateResults {
    pub evidence_gate: String,
    pub approval_gate: String,
    pub rollback_gate: String,
    pub scope_gate: String,
    pub unknown_error_gate: String,
}

impl Default for GovernanceGateResults {
    fn default() -> Self {
        Self {
            evidence_gate: "fail".to_string(),
            approval_gate: "fail".to_string(),
            rollback_gate: "fail".to_string(),
            scope_gate: "fail".to_string(),
            unknown_error_gate: "fail".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

pub fn validate_governance_decision(data: &Value) -> Vec<String> {
    let mut violations = Vec::new();

    for &f in GOVERNANCE_DECISION_REQUIRED {
        if data.get(f).is_none() {
            violations.push(format!("missing required field: {}", f));
        }
    }
    if !violations.is_empty() {
        return violations;
    }

    if data["schema_version"].as_str() != Some(GOVERNANCE_DECISION_VERSION) {
        violations.push(format!(
            "schema_version must be {}",
            GOVERNANCE_DECISION_VERSION
        ));
    }
    if let Some(decision) = data["decision"].as_str() {
        if !DECISION_VALUES.contains(&decision) {
            violations.push(format!(
                "decision {:?} not in {:?}",
                decision, DECISION_VALUES
            ));
        }
    }

    // Validate decision_basis
    match data.get("decision_basis") {
        Some(Value::Object(_)) => {
            for &f in DECISION_BASIS_REQUIRED {
                if data["decision_basis"].get(f).is_none() {
                    violations.push(format!("decision_basis missing required field: {}", f));
                }
            }
        }
        _ => violations.push("decision_basis must be a dict".to_string()),
    }

    // Validate gate_results
    match data.get("gate_results") {
        Some(Value::Object(map)) => {
            for &f in GATE_RESULTS_REQUIRED {
                match map.get(f) {
                    Some(val) => {
                        if let Some(s) = val.as_str() {
                            if !GATE_RESULTS.contains(&s) {
                                violations.push(format!(
                                    "gate_results.{} must be pass or fail, got {:?}",
                                    f, s
                                ));
                            }
                        } else {
                            violations.push(format!("gate_results.{} must be a string", f));
                        }
                    }
                    None => violations.push(format!("gate_results missing required field: {}", f)),
                }
            }
        }
        _ => violations.push("gate_results must be a dict".to_string()),
    }

    // Validate lists
    if data.get("blocked_reasons").is_some() && !data["blocked_reasons"].is_array() {
        violations.push("blocked_reasons must be a list".to_string());
    }
    if data.get("allowed_next_actions").is_some() && !data["allowed_next_actions"].is_array() {
        violations.push("allowed_next_actions must be a list".to_string());
    }
    if data.get("forbidden_next_actions").is_some() && !data["forbidden_next_actions"].is_array() {
        violations.push("forbidden_next_actions must be a list".to_string());
    }

    violations
}

// ---------------------------------------------------------------------------
// Gate evaluation helpers
// ---------------------------------------------------------------------------

pub fn evaluate_evidence_gate(evidence_pack: &Value) -> (String, String) {
    let admitted = evidence_pack
        .get("admitted_evidence_refs")
        .and_then(|v| v.as_array());
    match admitted {
        Some(arr) if !arr.is_empty() => (
            "pass".to_string(),
            format!("evidence_gate passed: {} admitted evidence refs", arr.len()),
        ),
        _ => (
            "fail".to_string(),
            "evidence_gate failed: no admitted_evidence_refs (diagnostic-only evidence cannot drive adoption)".to_string(),
        ),
    }
}

pub fn evaluate_approval_gate(approval_record: Option<&Value>) -> (String, String) {
    match approval_record {
        None => (
            "fail".to_string(),
            "approval_gate failed: no approval_record provided".to_string(),
        ),
        Some(record) => {
            let decision = record
                .get("decision")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if decision == "approved" {
                (
                    "pass".to_string(),
                    "approval_gate passed: decision is approved".to_string(),
                )
            } else {
                (
                    "fail".to_string(),
                    format!(
                        "approval_gate failed: decision is {:?} (must be approved)",
                        decision
                    ),
                )
            }
        }
    }
}

pub fn evaluate_rollback_gate(rollback_plan: Option<&Value>) -> (String, String) {
    match rollback_plan {
        None => (
            "fail".to_string(),
            "rollback_gate failed: no rollback_plan provided".to_string(),
        ),
        Some(plan) => {
            let status = plan.get("status").and_then(|v| v.as_str()).unwrap_or("");
            let steps = plan.get("rollback_steps").and_then(|v| v.as_array());
            if status != "approved" {
                return (
                    "fail".to_string(),
                    format!(
                        "rollback_gate failed: status is {:?} (must be approved)",
                        status
                    ),
                );
            }
            match steps {
                Some(arr) if !arr.is_empty() => (
                    "pass".to_string(),
                    "rollback_gate passed: status=approved with rollback_steps".to_string(),
                ),
                _ => (
                    "fail".to_string(),
                    "rollback_gate failed: rollback_steps is empty".to_string(),
                ),
            }
        }
    }
}

pub fn evaluate_scope_gate(
    _registry_entry: &Value,
    rollback_plan: Option<&Value>,
) -> (String, String) {
    match rollback_plan {
        None => (
            "fail".to_string(),
            "scope_gate failed: no rollback_plan to check".to_string(),
        ),
        Some(plan) => {
            let impacted = plan
                .get("impacted_refs")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            for ref_item in &impacted {
                if let Some(obj) = ref_item.as_object() {
                    if let Some(path) = obj.get("path_or_registry_key").and_then(|v| v.as_str()) {
                        for indicator in USER_PROJECT_INDICATORS {
                            if path.contains(indicator) {
                                return (
                                    "fail".to_string(),
                                    format!(
                                        "scope_gate failed: impacted_ref {:?} points to user project file",
                                        path
                                    ),
                                );
                            }
                        }
                    }
                }
            }
            (
                "pass".to_string(),
                "scope_gate passed: all impacted_refs are harness-level".to_string(),
            )
        }
    }
}

pub fn evaluate_unknown_error_gate(evidence_pack: &Value) -> (String, String) {
    let diagnostic = evidence_pack
        .get("diagnostic_evidence_refs")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let human_refs = evidence_pack
        .get("human_review_refs")
        .and_then(|v| v.as_array());

    let has_unknown = diagnostic.iter().any(|ref_item| {
        let s = serde_json::to_string(ref_item).unwrap_or_default();
        s.contains("unknown_error")
    });

    if has_unknown {
        match human_refs {
            Some(arr) if !arr.is_empty() => (
                "pass".to_string(),
                "unknown_error_gate passed: unknown_error evidence has human_review_refs"
                    .to_string(),
            ),
            _ => (
                "fail".to_string(),
                "unknown_error_gate failed: unknown_error evidence requires human review"
                    .to_string(),
            ),
        }
    } else {
        (
            "pass".to_string(),
            "unknown_error_gate passed: no unknown_error evidence".to_string(),
        )
    }
}

// ---------------------------------------------------------------------------
// Decision helpers
// ---------------------------------------------------------------------------

pub fn decide_policy_activation(
    candidate: &Value,
    evidence_pack: &Value,
    approval_record: Option<&Value>,
    rollback_plan: Option<&Value>,
    registry_entry: &Value,
) -> Value {
    let (ev_result, ev_reason) = evaluate_evidence_gate(evidence_pack);
    let (ap_result, ap_reason) = evaluate_approval_gate(approval_record);
    let (rb_result, rb_reason) = evaluate_rollback_gate(rollback_plan);
    let (sc_result, sc_reason) = evaluate_scope_gate(registry_entry, rollback_plan);
    let (ue_result, ue_reason) = evaluate_unknown_error_gate(evidence_pack);

    let all_pass = [&ev_result, &ap_result, &rb_result, &sc_result, &ue_result]
        .iter()
        .all(|r| r.as_str() == "pass");

    let mut blocked = Vec::new();
    if ev_result == "fail" {
        blocked.push(ev_reason.clone());
    }
    if ap_result == "fail" {
        blocked.push(ap_reason.clone());
    }
    if rb_result == "fail" {
        blocked.push(rb_reason.clone());
    }
    if sc_result == "fail" {
        blocked.push(sc_reason.clone());
    }
    if ue_result == "fail" {
        blocked.push(ue_reason.clone());
    }

    let (decision, allowed, forbidden) = if all_pass {
        ("approve_activation", vec!["activate_policy"], vec![])
    } else if blocked.iter().any(|r| r.contains("unknown_error")) {
        (
            "require_more_evidence",
            vec!["collect_human_review"],
            vec!["activate_policy"],
        )
    } else if ev_result == "fail" {
        (
            "require_more_evidence",
            vec!["collect_admitted_evidence"],
            vec!["activate_policy"],
        )
    } else {
        (
            "reject_activation",
            vec!["revise_candidate"],
            vec!["activate_policy"],
        )
    };

    let candidate_id = candidate
        .get("candidate_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let policy_id = candidate
        .get("policy_id")
        .and_then(|v| v.as_str())
        .or_else(|| registry_entry.get("policy_id").and_then(|v| v.as_str()))
        .unwrap_or("");

    let approval_ref = approval_record
        .and_then(|r| r.get("candidate_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let rollback_ref = rollback_plan
        .and_then(|r| r.get("rollback_plan_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let registry_ref = registry_entry
        .get("policy_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let admitted_refs = evidence_pack
        .get("admitted_evidence_refs")
        .cloned()
        .unwrap_or_else(|| Value::Array(vec![]));
    let diagnostic_refs = evidence_pack
        .get("diagnostic_evidence_refs")
        .cloned()
        .unwrap_or_else(|| Value::Array(vec![]));

    serde_json::json!({
        "schema_version": GOVERNANCE_DECISION_VERSION,
        "decision_id": format!("gov-{}", candidate_id),
        "candidate_id": candidate_id,
        "policy_id": policy_id,
        "decision": decision,
        "decision_basis": {
            "admitted_evidence_refs": admitted_refs,
            "diagnostic_evidence_refs": diagnostic_refs,
            "approval_ref": approval_ref,
            "rollback_plan_ref": rollback_ref,
            "registry_entry_ref": registry_ref,
        },
        "gate_results": {
            "evidence_gate": ev_result,
            "approval_gate": ap_result,
            "rollback_gate": rb_result,
            "scope_gate": sc_result,
            "unknown_error_gate": ue_result,
        },
        "blocked_reasons": blocked,
        "allowed_next_actions": allowed,
        "forbidden_next_actions": forbidden,
        "decided_by": "governance_engine",
        "decided_at": "",
    })
}

pub fn governance_allows_activation(decision: &Value) -> (bool, String) {
    if decision.get("decision").and_then(|v| v.as_str()) == Some("approve_activation") {
        (
            true,
            "governance decision is approve_activation".to_string(),
        )
    } else {
        let d = decision
            .get("decision")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        (false, format!("governance decision is {:?}", d))
    }
}

pub fn governance_blocks_activation(decision: &Value) -> (bool, String) {
    if decision.get("decision").and_then(|v| v.as_str()) != Some("approve_activation") {
        let blocked: Vec<String> = decision
            .get("blocked_reasons")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let msg = if blocked.is_empty() {
            decision
                .get("decision")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        } else {
            blocked.join("; ")
        };
        (true, format!("governance blocks activation: {}", msg))
    } else {
        (false, "governance does not block activation".to_string())
    }
}

pub fn explain_blocked_reasons(decision: &Value) -> Vec<String> {
    if decision.get("decision").and_then(|v| v.as_str()) == Some("approve_activation") {
        return Vec::new();
    }
    decision
        .get("blocked_reasons")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_decision() -> Value {
        json!({
            "schema_version": GOVERNANCE_DECISION_VERSION,
            "decision_id": "gov-c001",
            "candidate_id": "c001",
            "policy_id": "p001",
            "decision": "approve_activation",
            "decision_basis": {
                "admitted_evidence_refs": ["e1"],
                "diagnostic_evidence_refs": [],
                "approval_ref": "a001",
                "rollback_plan_ref": "rb001",
                "registry_entry_ref": "p001",
            },
            "gate_results": {
                "evidence_gate": "pass",
                "approval_gate": "pass",
                "rollback_gate": "pass",
                "scope_gate": "pass",
                "unknown_error_gate": "pass",
            },
            "blocked_reasons": [],
            "allowed_next_actions": ["activate_policy"],
            "forbidden_next_actions": [],
            "decided_by": "governance_engine",
            "decided_at": "",
        })
    }

    #[test]
    fn test_validate_valid_decision() {
        let v = valid_decision();
        let violations = validate_governance_decision(&v);
        assert!(violations.is_empty(), "violations: {:?}", violations);
    }

    #[test]
    fn test_validate_missing_field() {
        let mut v = valid_decision();
        v.as_object_mut().unwrap().remove("decision_id");
        let violations = validate_governance_decision(&v);
        assert!(violations.iter().any(|v| v.contains("decision_id")));
    }

    #[test]
    fn test_validate_wrong_schema_version() {
        let mut v = valid_decision();
        v["schema_version"] = json!("wrong.v1");
        let violations = validate_governance_decision(&v);
        assert!(violations.iter().any(|v| v.contains("schema_version")));
    }

    #[test]
    fn test_validate_invalid_decision_value() {
        let mut v = valid_decision();
        v["decision"] = json!("invalid_decision");
        let violations = validate_governance_decision(&v);
        assert!(violations.iter().any(|v| v.contains("decision")));
    }

    #[test]
    fn test_evaluate_evidence_gate_pass() {
        let ep = json!({"admitted_evidence_refs": ["e1", "e2"]});
        let (result, reason) = evaluate_evidence_gate(&ep);
        assert_eq!(result, "pass");
        assert!(reason.contains("2"));
    }

    #[test]
    fn test_evaluate_evidence_gate_fail() {
        let ep = json!({"admitted_evidence_refs": []});
        let (result, _) = evaluate_evidence_gate(&ep);
        assert_eq!(result, "fail");
    }

    #[test]
    fn test_evaluate_approval_gate_pass() {
        let ar = json!({"decision": "approved"});
        let (result, _) = evaluate_approval_gate(Some(&ar));
        assert_eq!(result, "pass");
    }

    #[test]
    fn test_evaluate_approval_gate_fail_no_record() {
        let (result, reason) = evaluate_approval_gate(None);
        assert_eq!(result, "fail");
        assert!(reason.contains("no approval_record"));
    }

    #[test]
    fn test_evaluate_rollback_gate_pass() {
        let rp = json!({"status": "approved", "rollback_steps": [{"step_id": "s1"}]});
        let (result, _) = evaluate_rollback_gate(Some(&rp));
        assert_eq!(result, "pass");
    }

    #[test]
    fn test_evaluate_rollback_gate_fail_empty_steps() {
        let rp = json!({"status": "approved", "rollback_steps": []});
        let (result, _) = evaluate_rollback_gate(Some(&rp));
        assert_eq!(result, "fail");
    }

    #[test]
    fn test_evaluate_scope_gate_user_project_path() {
        let re = json!({});
        let rp = json!({
            "impacted_refs": [{"path_or_registry_key": "/home/user/project/main.py"}]
        });
        let (result, reason) = evaluate_scope_gate(&re, Some(&rp));
        assert_eq!(result, "fail");
        assert!(reason.contains("user project file"));
    }

    #[test]
    fn test_evaluate_scope_gate_harness_level() {
        let re = json!({});
        let rp = json!({
            "impacted_refs": [{"path_or_registry_key": "harness://config/policy_x"}]
        });
        let (result, _) = evaluate_scope_gate(&re, Some(&rp));
        assert_eq!(result, "pass");
    }

    #[test]
    fn test_evaluate_unknown_error_gate_with_human_review() {
        let ep = json!({
            "diagnostic_evidence_refs": ["unknown_error_detected"],
            "human_review_refs": ["hr1"],
        });
        let (result, _) = evaluate_unknown_error_gate(&ep);
        assert_eq!(result, "pass");
    }

    #[test]
    fn test_evaluate_unknown_error_gate_without_human_review() {
        let ep = json!({
            "diagnostic_evidence_refs": ["unknown_error_detected"],
            "human_review_refs": [],
        });
        let (result, _) = evaluate_unknown_error_gate(&ep);
        assert_eq!(result, "fail");
    }

    #[test]
    fn test_decide_policy_activation_all_pass() {
        let candidate = json!({"candidate_id": "c1", "policy_id": "p1"});
        let evidence = json!({"admitted_evidence_refs": ["e1"], "diagnostic_evidence_refs": []});
        let approval = json!({"candidate_id": "c1", "decision": "approved"});
        let rollback =
            json!({"rollback_plan_id": "rb1", "status": "approved", "rollback_steps": ["s1"]});
        let registry = json!({"policy_id": "p1"});

        let decision = decide_policy_activation(
            &candidate,
            &evidence,
            Some(&approval),
            Some(&rollback),
            &registry,
        );
        assert_eq!(decision["decision"], "approve_activation");
        assert!(decision["blocked_reasons"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_decide_policy_activation_evidence_fail() {
        let candidate = json!({"candidate_id": "c1", "policy_id": "p1"});
        let evidence = json!({"admitted_evidence_refs": [], "diagnostic_evidence_refs": []});
        let approval = json!({"candidate_id": "c1", "decision": "approved"});
        let rollback =
            json!({"rollback_plan_id": "rb1", "status": "approved", "rollback_steps": ["s1"]});
        let registry = json!({"policy_id": "p1"});

        let decision = decide_policy_activation(
            &candidate,
            &evidence,
            Some(&approval),
            Some(&rollback),
            &registry,
        );
        assert_eq!(decision["decision"], "require_more_evidence");
    }

    #[test]
    fn test_governance_allows_activation() {
        let d = json!({"decision": "approve_activation"});
        let (ok, _) = governance_allows_activation(&d);
        assert!(ok);
    }

    #[test]
    fn test_governance_blocks_activation() {
        let d = json!({"decision": "reject_activation", "blocked_reasons": ["no evidence"]});
        let (blocked, reason) = governance_blocks_activation(&d);
        assert!(blocked);
        assert!(reason.contains("no evidence"));
    }

    #[test]
    fn test_explain_blocked_reasons_empty_on_approve() {
        let d = json!({"decision": "approve_activation", "blocked_reasons": []});
        let reasons = explain_blocked_reasons(&d);
        assert!(reasons.is_empty());
    }

    #[test]
    fn test_explain_blocked_reasons_returns_reasons() {
        let d = json!({"decision": "reject_activation", "blocked_reasons": ["r1", "r2"]});
        let reasons = explain_blocked_reasons(&d);
        assert_eq!(reasons.len(), 2);
    }
}
