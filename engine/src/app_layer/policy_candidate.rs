use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Schema versions
// ---------------------------------------------------------------------------

pub const CANDIDATE_MANIFEST_VERSION: &str = "policy_candidate.v1";
pub const EVIDENCE_PACK_VERSION: &str = "candidate_evidence.v1";
pub const APPROVAL_RECORD_VERSION: &str = "approval_record.v1";
pub const ROLLBACK_PLAN_VERSION: &str = "rollback_plan.v1";
pub const POLICY_REGISTRY_VERSION: &str = "policy_registry.v1";

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

pub const CANDIDATE_TYPES: &[&str] = &[
    "context_pack",
    "tool_contract",
    "routing_rule",
    "skill_package",
    "eval_gate",
    "error_taxonomy",
    "model_profile",
];

pub const EVIDENCE_RECOMMENDATION: &[&str] = &["accept", "reject", "revise", "needs_more_evidence"];

pub const APPROVAL_DECISIONS: &[&str] = &["approved", "rejected", "deferred"];

pub const ROLLBACK_SCOPES: &[&str] = &[
    "docs_only",
    "config",
    "schema",
    "profile",
    "skill",
    "eval_gate",
    "runtime_guard",
];

pub const ROLLBACK_TRIGGER_CONDITIONS: &[&str] = &[
    "regression_detected",
    "cost_threshold_exceeded",
    "quality_gate_failure",
    "human_rejection",
    "unknown_error_increase",
];

pub const IMPACTED_REF_TYPES: &[&str] = &[
    "file",
    "registry_entry",
    "config_key",
    "schema_id",
    "skill_id",
    "model_profile_id",
];

pub const ROLLBACK_ACTIONS: &[&str] = &[
    "revert_file",
    "restore_registry_entry",
    "disable_policy",
    "restore_schema",
    "retire_skill",
    "restore_profile",
];

pub const REGISTRY_STATUSES: &[&str] =
    &["proposed", "approved", "active", "rolled_back", "retired"];

pub const ROLLBACK_STATUSES: &[&str] = &["proposed", "approved", "executed", "failed", "obsolete"];

const USER_PROJECT_INDICATORS: &[&str] = &[
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

pub const MANIFEST_REQUIRED: &[&str] = &[
    "schema_version",
    "candidate_id",
    "candidate_type",
    "title",
    "rationale",
    "source_refs",
    "proposed_change_summary",
    "affected_components",
    "expected_benefit",
    "expected_risk",
    "required_evidence",
    "evaluation_plan",
    "rollback_plan_ref",
    "approval_required",
    "created_at",
];

pub const EVIDENCE_PACK_REQUIRED: &[&str] = &[
    "schema_version",
    "candidate_id",
    "admitted_evidence_refs",
    "diagnostic_evidence_refs",
    "fixture_results",
    "quality_deltas",
    "cost_deltas",
    "failure_cluster_refs",
    "human_review_refs",
    "recommendation",
];

pub const APPROVAL_REQUIRED: &[&str] = &[
    "schema_version",
    "candidate_id",
    "approver",
    "decision",
    "rationale",
    "required_followups",
    "deployment_scope",
    "rollback_required",
    "approved_at",
];

pub const ROLLBACK_PLAN_REQUIRED: &[&str] = &[
    "schema_version",
    "rollback_plan_id",
    "candidate_id",
    "policy_id",
    "rollback_scope",
    "trigger_conditions",
    "impacted_refs",
    "rollback_steps",
    "validation_steps",
    "rollback_owner",
    "max_rollback_time",
    "fallback_policy",
    "status",
    "created_at",
];

pub const REGISTRY_REQUIRED: &[&str] = &[
    "schema_version",
    "policy_id",
    "candidate_id",
    "policy_type",
    "status",
    "active_scope",
    "version",
    "evidence_pack_ref",
    "approval_ref",
    "rollback_plan_ref",
    "activated_at",
    "retired_at",
];

// ---------------------------------------------------------------------------
// Data structs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PolicyRegistryEntry {
    pub schema_version: String,
    pub policy_id: String,
    pub candidate_id: String,
    pub policy_type: String,
    pub status: String,
    pub active_scope: String,
    pub version: String,
    pub evidence_pack_ref: String,
    pub approval_ref: String,
    pub rollback_plan_ref: String,
    pub activated_at: String,
    pub retired_at: String,
}

impl Default for PolicyRegistryEntry {
    fn default() -> Self {
        Self {
            schema_version: POLICY_REGISTRY_VERSION.to_string(),
            policy_id: String::new(),
            candidate_id: String::new(),
            policy_type: String::new(),
            status: "proposed".to_string(),
            active_scope: String::new(),
            version: "1.0".to_string(),
            evidence_pack_ref: String::new(),
            approval_ref: String::new(),
            rollback_plan_ref: String::new(),
            activated_at: String::new(),
            retired_at: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

pub fn validate_policy_candidate_manifest(data: &Value) -> Vec<String> {
    let mut violations = Vec::new();

    for &f in MANIFEST_REQUIRED {
        if data.get(f).is_none() {
            violations.push(format!("missing required field: {}", f));
        }
    }
    if !violations.is_empty() {
        return violations;
    }

    if data["schema_version"].as_str() != Some(CANDIDATE_MANIFEST_VERSION) {
        violations.push(format!(
            "schema_version must be {}",
            CANDIDATE_MANIFEST_VERSION
        ));
    }
    if let Some(ct) = data["candidate_type"].as_str() {
        if !CANDIDATE_TYPES.contains(&ct) {
            violations.push(format!(
                "candidate_type {:?} not in {:?}",
                ct, CANDIDATE_TYPES
            ));
        }
    }
    if data["approval_required"] != Value::Bool(true) {
        violations.push(format!(
            "approval_required must be true, got {:?}",
            data["approval_required"]
        ));
    }
    if !data["source_refs"].is_array() {
        violations.push("source_refs must be a list".to_string());
    }

    violations
}

pub fn validate_candidate_evidence_pack(data: &Value) -> Vec<String> {
    let mut violations = Vec::new();

    for &f in EVIDENCE_PACK_REQUIRED {
        if data.get(f).is_none() {
            violations.push(format!("missing required field: {}", f));
        }
    }
    if !violations.is_empty() {
        return violations;
    }

    if data["schema_version"].as_str() != Some(EVIDENCE_PACK_VERSION) {
        violations.push(format!("schema_version must be {}", EVIDENCE_PACK_VERSION));
    }
    if let Some(rec) = data["recommendation"].as_str() {
        if !EVIDENCE_RECOMMENDATION.contains(&rec) {
            violations.push(format!(
                "recommendation {:?} not in {:?}",
                rec, EVIDENCE_RECOMMENDATION
            ));
        }
    }
    if !data["admitted_evidence_refs"].is_array() {
        violations.push("admitted_evidence_refs must be a list".to_string());
    }
    if !data["diagnostic_evidence_refs"].is_array() {
        violations.push("diagnostic_evidence_refs must be a list".to_string());
    }

    violations
}

pub fn validate_approval_record(data: &Value) -> Vec<String> {
    let mut violations = Vec::new();

    for &f in APPROVAL_REQUIRED {
        if data.get(f).is_none() {
            violations.push(format!("missing required field: {}", f));
        }
    }
    if !violations.is_empty() {
        return violations;
    }

    if data["schema_version"].as_str() != Some(APPROVAL_RECORD_VERSION) {
        violations.push(format!(
            "schema_version must be {}",
            APPROVAL_RECORD_VERSION
        ));
    }
    if let Some(d) = data["decision"].as_str() {
        if !APPROVAL_DECISIONS.contains(&d) {
            violations.push(format!("decision {:?} not in {:?}", d, APPROVAL_DECISIONS));
        }
    }
    if data["rollback_required"] != Value::Bool(true) {
        violations.push(format!(
            "rollback_required must be true, got {:?}",
            data["rollback_required"]
        ));
    }

    violations
}

pub fn validate_rollback_plan(data: &Value) -> Vec<String> {
    let mut violations = Vec::new();

    for &f in ROLLBACK_PLAN_REQUIRED {
        if data.get(f).is_none() {
            violations.push(format!("missing required field: {}", f));
        }
    }
    if !violations.is_empty() {
        return violations;
    }

    if data["schema_version"].as_str() != Some(ROLLBACK_PLAN_VERSION) {
        violations.push(format!("schema_version must be {}", ROLLBACK_PLAN_VERSION));
    }
    if let Some(rs) = data["rollback_scope"].as_str() {
        if !ROLLBACK_SCOPES.contains(&rs) {
            violations.push(format!(
                "rollback_scope {:?} not in {:?}",
                rs, ROLLBACK_SCOPES
            ));
        }
    }
    if let Some(st) = data["status"].as_str() {
        if !ROLLBACK_STATUSES.contains(&st) {
            violations.push(format!("status {:?} not in {:?}", st, ROLLBACK_STATUSES));
        }
    }
    if !data["trigger_conditions"].is_array() {
        violations.push("trigger_conditions must be a list".to_string());
    }
    if !data["rollback_steps"].is_array() {
        violations.push("rollback_steps must be a list".to_string());
    } else {
        let steps = data["rollback_steps"].as_array().unwrap();
        if steps.is_empty() {
            violations.push("rollback_steps must not be empty".to_string());
        }
    }
    if !data["validation_steps"].is_array() {
        violations.push("validation_steps must be a list".to_string());
    }

    // Check rollback_steps have required fields
    if let Some(steps) = data.get("rollback_steps").and_then(|v| v.as_array()) {
        for (i, step) in steps.iter().enumerate() {
            match step.as_object() {
                None => violations.push(format!("rollback_steps[{}] must be a dict", i)),
                Some(obj) => {
                    if !obj.contains_key("step_id") {
                        violations.push(format!("rollback_steps[{}] missing step_id", i));
                    }
                    match obj.get("action").and_then(|v| v.as_str()) {
                        None => {
                            violations.push(format!("rollback_steps[{}] missing action", i));
                        }
                        Some(action) => {
                            if !ROLLBACK_ACTIONS.contains(&action) {
                                violations.push(format!(
                                    "rollback_steps[{}].action {:?} not in {:?}",
                                    i, action, ROLLBACK_ACTIONS
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    // Check impacted_refs for user project paths
    if let Some(refs) = data.get("impacted_refs").and_then(|v| v.as_array()) {
        for (i, ref_item) in refs.iter().enumerate() {
            if let Some(obj) = ref_item.as_object() {
                if let Some(path) = obj.get("path_or_registry_key").and_then(|v| v.as_str()) {
                    if is_user_project_path(path) {
                        violations.push(format!(
                            "impacted_refs[{}] points to user project path {:?}; rollback targets must be harness-level only",
                            i, path
                        ));
                    }
                }
            }
        }
    }

    violations
}

pub fn validate_policy_registry_entry(data: &Value) -> Vec<String> {
    let mut violations = Vec::new();

    for &f in REGISTRY_REQUIRED {
        if data.get(f).is_none() {
            violations.push(format!("missing required field: {}", f));
        }
    }
    if !violations.is_empty() {
        return violations;
    }

    if data["schema_version"].as_str() != Some(POLICY_REGISTRY_VERSION) {
        violations.push(format!(
            "schema_version must be {}",
            POLICY_REGISTRY_VERSION
        ));
    }
    if let Some(st) = data["status"].as_str() {
        if !REGISTRY_STATUSES.contains(&st) {
            violations.push(format!("status {:?} not in {:?}", st, REGISTRY_STATUSES));
        }
    }

    // Active policy must have approval_ref and rollback_plan_ref
    if data["status"].as_str() == Some("active") {
        if data
            .get("approval_ref")
            .and_then(|v| v.as_str())
            .map(|s| s.is_empty())
            .unwrap_or(true)
        {
            violations.push("active policy must have approval_ref".to_string());
        }
        if data
            .get("rollback_plan_ref")
            .and_then(|v| v.as_str())
            .map(|s| s.is_empty())
            .unwrap_or(true)
        {
            violations.push("active policy must have rollback_plan_ref".to_string());
        }
    }

    violations
}

fn is_user_project_path(path: &str) -> bool {
    USER_PROJECT_INDICATORS
        .iter()
        .any(|indicator| path.contains(indicator))
}

// ---------------------------------------------------------------------------
// Lifecycle helpers
// ---------------------------------------------------------------------------

pub fn candidate_has_required_evidence(_manifest: &Value, evidence: &Value) -> (bool, String) {
    let admitted = evidence
        .get("admitted_evidence_refs")
        .and_then(|v| v.as_array());
    match admitted {
        Some(arr) if !arr.is_empty() => (true, "candidate has admitted evidence".to_string()),
        _ => (
            false,
            "no admitted_evidence_refs; adoption requires at least one admitted evidence"
                .to_string(),
        ),
    }
}

pub fn evidence_pack_is_adoptable(evidence: &Value) -> (bool, String) {
    let admitted = evidence
        .get("admitted_evidence_refs")
        .and_then(|v| v.as_array());

    match admitted {
        Some(arr) if arr.is_empty() => {
            if evidence.get("recommendation").and_then(|v| v.as_str()) == Some("accept") {
                return (
                    false,
                    "diagnostic-only evidence pack cannot recommend accept".to_string(),
                );
            }
            (false, "no admitted evidence for adoption".to_string())
        }
        None => {
            if evidence.get("recommendation").and_then(|v| v.as_str()) == Some("accept") {
                return (
                    false,
                    "diagnostic-only evidence pack cannot recommend accept".to_string(),
                );
            }
            (false, "no admitted evidence for adoption".to_string())
        }
        Some(_) => {
            if evidence.get("recommendation").and_then(|v| v.as_str()) != Some("accept") {
                let rec = evidence
                    .get("recommendation")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                return (false, format!("recommendation is {:?}, not accept", rec));
            }
            (true, "evidence pack is adoptable".to_string())
        }
    }
}

pub fn approval_allows_activation(approval: &Value) -> (bool, String) {
    if approval.get("decision").and_then(|v| v.as_str()) != Some("approved") {
        let d = approval
            .get("decision")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        (false, format!("decision is {:?}, not approved", d))
    } else {
        (true, "approval allows activation".to_string())
    }
}

pub fn rollback_plan_is_ready(plan: &Value) -> (bool, String) {
    let steps = plan.get("rollback_steps").and_then(|v| v.as_array());
    let status = plan.get("status").and_then(|v| v.as_str()).unwrap_or("");

    if steps.is_none_or(|a| a.is_empty()) {
        return (false, "rollback_plan has no rollback_steps".to_string());
    }
    if status == "failed" || status == "obsolete" {
        return (false, format!("rollback_plan status is {:?}", status));
    }
    (true, "rollback plan is ready".to_string())
}

pub fn can_activate_policy(
    registry: &Value,
    approval: Option<&Value>,
    rollback: Option<&Value>,
    evidence: Option<&Value>,
) -> (bool, String) {
    if registry.get("status").and_then(|v| v.as_str()) == Some("active") {
        return (true, "already active".to_string());
    }

    if registry
        .get("approval_ref")
        .and_then(|v| v.as_str())
        .map(|s| s.is_empty())
        .unwrap_or(true)
    {
        if let Some(ap) = approval {
            if ap.get("decision").and_then(|v| v.as_str()) != Some("approved") {
                return (false, "approval is not approved".to_string());
            }
        }
    }

    if registry
        .get("rollback_plan_ref")
        .and_then(|v| v.as_str())
        .map(|s| s.is_empty())
        .unwrap_or(true)
    {
        if let Some(rb) = rollback {
            let (ok, reason) = rollback_plan_is_ready(rb);
            if !ok {
                return (false, format!("rollback_plan not ready: {}", reason));
            }
        }
    }

    if let Some(ev) = evidence {
        let (ok, reason) = evidence_pack_is_adoptable(ev);
        if !ok {
            return (false, format!("evidence not adoptable: {}", reason));
        }
    }

    (true, "policy can be activated".to_string())
}

pub fn should_reject_diagnostic_only_candidate(evidence: &Value) -> (bool, String) {
    let admitted = evidence
        .get("admitted_evidence_refs")
        .and_then(|v| v.as_array());
    if let Some(arr) = admitted {
        if !arr.is_empty() {
            return (false, "candidate has admitted evidence".to_string());
        }
    }
    let diagnostic = evidence
        .get("diagnostic_evidence_refs")
        .and_then(|v| v.as_array());
    if let Some(arr) = diagnostic {
        if !arr.is_empty() {
            return (
                true,
                "candidate only has diagnostic evidence; cannot be adopted".to_string(),
            );
        }
    }
    (false, "no evidence at all".to_string())
}

#[allow(clippy::too_many_arguments)]
pub fn create_policy_registry_entry(
    policy_id: &str,
    candidate_id: &str,
    policy_type: &str,
    active_scope: &str,
    version: &str,
    evidence_pack_ref: &str,
    approval_ref: &str,
    rollback_plan_ref: &str,
) -> Value {
    serde_json::json!({
        "schema_version": POLICY_REGISTRY_VERSION,
        "policy_id": policy_id,
        "candidate_id": candidate_id,
        "policy_type": policy_type,
        "status": "proposed",
        "active_scope": active_scope,
        "version": version,
        "evidence_pack_ref": evidence_pack_ref,
        "approval_ref": approval_ref,
        "rollback_plan_ref": rollback_plan_ref,
        "activated_at": "",
        "retired_at": "",
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_manifest() -> Value {
        json!({
            "schema_version": CANDIDATE_MANIFEST_VERSION,
            "candidate_id": "c001",
            "candidate_type": "routing_rule",
            "title": "New routing rule",
            "rationale": "improve dispatch",
            "source_refs": ["s1"],
            "proposed_change_summary": "add adaptive routing",
            "affected_components": ["routing"],
            "expected_benefit": "better model selection",
            "expected_risk": "low",
            "required_evidence": ["fixture_pass"],
            "evaluation_plan": "run benchmarks",
            "rollback_plan_ref": "rb001",
            "approval_required": true,
            "created_at": "2026-05-29",
        })
    }

    fn valid_evidence_pack() -> Value {
        json!({
            "schema_version": EVIDENCE_PACK_VERSION,
            "candidate_id": "c001",
            "admitted_evidence_refs": ["e1"],
            "diagnostic_evidence_refs": [],
            "fixture_results": [],
            "quality_deltas": [],
            "cost_deltas": [],
            "failure_cluster_refs": [],
            "human_review_refs": [],
            "recommendation": "accept",
        })
    }

    fn valid_approval_record() -> Value {
        json!({
            "schema_version": APPROVAL_RECORD_VERSION,
            "candidate_id": "c001",
            "approver": "admin",
            "decision": "approved",
            "rationale": "looks good",
            "required_followups": [],
            "deployment_scope": "harness",
            "rollback_required": true,
            "approved_at": "2026-05-29",
        })
    }

    fn valid_rollback_plan() -> Value {
        json!({
            "schema_version": ROLLBACK_PLAN_VERSION,
            "rollback_plan_id": "rb001",
            "candidate_id": "c001",
            "policy_id": "p001",
            "rollback_scope": "config",
            "trigger_conditions": ["regression_detected"],
            "impacted_refs": [],
            "rollback_steps": [{"step_id": "s1", "action": "revert_file"}],
            "validation_steps": [],
            "rollback_owner": "admin",
            "max_rollback_time": "1h",
            "fallback_policy": "none",
            "status": "approved",
            "created_at": "2026-05-29",
        })
    }

    fn valid_registry_entry() -> Value {
        json!({
            "schema_version": POLICY_REGISTRY_VERSION,
            "policy_id": "p001",
            "candidate_id": "c001",
            "policy_type": "routing_rule",
            "status": "proposed",
            "active_scope": "harness",
            "version": "1.0",
            "evidence_pack_ref": "ep001",
            "approval_ref": "a001",
            "rollback_plan_ref": "rb001",
            "activated_at": "",
            "retired_at": "",
        })
    }

    #[test]
    fn test_validate_valid_manifest() {
        let v = valid_manifest();
        let violations = validate_policy_candidate_manifest(&v);
        assert!(violations.is_empty(), "violations: {:?}", violations);
    }

    #[test]
    fn test_validate_manifest_bad_candidate_type() {
        let mut v = valid_manifest();
        v["candidate_type"] = json!("invalid_type");
        let violations = validate_policy_candidate_manifest(&v);
        assert!(violations.iter().any(|v| v.contains("candidate_type")));
    }

    #[test]
    fn test_validate_manifest_approval_required_false() {
        let mut v = valid_manifest();
        v["approval_required"] = json!(false);
        let violations = validate_policy_candidate_manifest(&v);
        assert!(violations.iter().any(|v| v.contains("approval_required")));
    }

    #[test]
    fn test_validate_valid_evidence_pack() {
        let v = valid_evidence_pack();
        let violations = validate_candidate_evidence_pack(&v);
        assert!(violations.is_empty(), "violations: {:?}", violations);
    }

    #[test]
    fn test_validate_evidence_pack_bad_recommendation() {
        let mut v = valid_evidence_pack();
        v["recommendation"] = json!("invalid_rec");
        let violations = validate_candidate_evidence_pack(&v);
        assert!(violations.iter().any(|v| v.contains("recommendation")));
    }

    #[test]
    fn test_validate_valid_approval_record() {
        let v = valid_approval_record();
        let violations = validate_approval_record(&v);
        assert!(violations.is_empty(), "violations: {:?}", violations);
    }

    #[test]
    fn test_validate_approval_rollback_required_false() {
        let mut v = valid_approval_record();
        v["rollback_required"] = json!(false);
        let violations = validate_approval_record(&v);
        assert!(violations.iter().any(|v| v.contains("rollback_required")));
    }

    #[test]
    fn test_validate_valid_rollback_plan() {
        let v = valid_rollback_plan();
        let violations = validate_rollback_plan(&v);
        assert!(violations.is_empty(), "violations: {:?}", violations);
    }

    #[test]
    fn test_validate_rollback_plan_user_project_path() {
        let mut v = valid_rollback_plan();
        v["impacted_refs"] = json!([{"path_or_registry_key": "/home/user/project/main.py"}]);
        let violations = validate_rollback_plan(&v);
        assert!(violations.iter().any(|v| v.contains("user project path")));
    }

    #[test]
    fn test_validate_rollback_plan_bad_action() {
        let mut v = valid_rollback_plan();
        v["rollback_steps"] = json!([{"step_id": "s1", "action": "invalid_action"}]);
        let violations = validate_rollback_plan(&v);
        assert!(violations.iter().any(|v| v.contains("not in")));
    }

    #[test]
    fn test_validate_valid_registry_entry() {
        let v = valid_registry_entry();
        let violations = validate_policy_registry_entry(&v);
        assert!(violations.is_empty(), "violations: {:?}", violations);
    }

    #[test]
    fn test_validate_registry_active_needs_approval() {
        let mut v = valid_registry_entry();
        v["status"] = json!("active");
        v["approval_ref"] = json!("");
        let violations = validate_policy_registry_entry(&v);
        assert!(violations.iter().any(|v| v.contains("approval_ref")));
    }

    #[test]
    fn test_candidate_has_required_evidence_pass() {
        let manifest = json!({});
        let evidence = json!({"admitted_evidence_refs": ["e1"]});
        let (ok, _) = candidate_has_required_evidence(&manifest, &evidence);
        assert!(ok);
    }

    #[test]
    fn test_candidate_has_required_evidence_fail() {
        let manifest = json!({});
        let evidence = json!({"admitted_evidence_refs": []});
        let (ok, _) = candidate_has_required_evidence(&manifest, &evidence);
        assert!(!ok);
    }

    #[test]
    fn test_evidence_pack_is_adoptable_pass() {
        let evidence = json!({"admitted_evidence_refs": ["e1"], "recommendation": "accept"});
        let (ok, _) = evidence_pack_is_adoptable(&evidence);
        assert!(ok);
    }

    #[test]
    fn test_evidence_pack_is_adoptable_wrong_recommendation() {
        let evidence = json!({"admitted_evidence_refs": ["e1"], "recommendation": "reject"});
        let (ok, _) = evidence_pack_is_adoptable(&evidence);
        assert!(!ok);
    }

    #[test]
    fn test_approval_allows_activation_pass() {
        let approval = json!({"decision": "approved"});
        let (ok, _) = approval_allows_activation(&approval);
        assert!(ok);
    }

    #[test]
    fn test_approval_allows_activation_fail() {
        let approval = json!({"decision": "rejected"});
        let (ok, _) = approval_allows_activation(&approval);
        assert!(!ok);
    }

    #[test]
    fn test_rollback_plan_is_ready_pass() {
        let plan = json!({"rollback_steps": ["s1"], "status": "approved"});
        let (ok, _) = rollback_plan_is_ready(&plan);
        assert!(ok);
    }

    #[test]
    fn test_rollback_plan_is_ready_failed_status() {
        let plan = json!({"rollback_steps": ["s1"], "status": "failed"});
        let (ok, _) = rollback_plan_is_ready(&plan);
        assert!(!ok);
    }

    #[test]
    fn test_can_activate_policy_already_active() {
        let registry = json!({"status": "active"});
        let (ok, _) = can_activate_policy(&registry, None, None, None);
        assert!(ok);
    }

    #[test]
    fn test_should_reject_diagnostic_only() {
        let evidence = json!({
            "admitted_evidence_refs": [],
            "diagnostic_evidence_refs": ["d1"],
        });
        let (reject, _) = should_reject_diagnostic_only_candidate(&evidence);
        assert!(reject);
    }

    #[test]
    fn test_create_policy_registry_entry() {
        let entry =
            create_policy_registry_entry("p1", "c1", "routing_rule", "harness", "1.0", "", "", "");
        assert_eq!(entry["schema_version"], POLICY_REGISTRY_VERSION);
        assert_eq!(entry["status"], "proposed");
        assert_eq!(entry["policy_id"], "p1");
    }
}
