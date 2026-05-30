use std::collections::HashMap;

use serde_json::Value;

use super::rules::*;

fn check_fields(data: &HashMap<String, Value>, required: &[&str], prefix: &str) -> Vec<String> {
    let mut violations = Vec::new();
    for f in required {
        if !data.contains_key(*f) {
            if prefix.is_empty() {
                violations.push(format!("missing required field: {f}"));
            } else {
                violations.push(format!("missing required field: {prefix}{f}"));
            }
        }
    }
    violations
}

fn validate_budget(budget: &Value, prefix: &str) -> Vec<String> {
    let mut v = Vec::new();
    let obj = match budget.as_object() {
        Some(o) => o,
        None => {
            v.push(format!("{prefix} must be a dict"));
            return v;
        }
    };
    if let Some(mct) = obj.get("max_context_tokens") {
        if !mct.is_i64() || mct.as_i64().unwrap_or(0) <= 0 {
            v.push(format!(
                "{prefix}.max_context_tokens must be a positive integer"
            ));
        }
    }
    if let Some(pct) = obj.get("preferred_context_tokens") {
        if !pct.is_i64() || pct.as_i64().unwrap_or(0) <= 0 {
            v.push(format!(
                "{prefix}.preferred_context_tokens must be a positive integer"
            ));
        }
    }
    v
}

fn validate_context_budget(budget: &Value) -> Vec<String> {
    let mut v = Vec::new();
    let obj = match budget.as_object() {
        Some(o) => o,
        None => {
            v.push("context_budget must be a dict".to_string());
            return v;
        }
    };
    if let Some(mct) = obj.get("max_context_tokens") {
        if !mct.is_i64() || mct.as_i64().unwrap_or(0) <= 0 {
            v.push("context_budget.max_context_tokens must be a positive integer".to_string());
        }
    }
    if let Some(pct) = obj.get("preferred_context_tokens") {
        if !pct.is_i64() || pct.as_i64().unwrap_or(0) <= 0 {
            v.push(
                "context_budget.preferred_context_tokens must be a positive integer".to_string(),
            );
        }
    }
    if let Some(rrt) = obj.get("reserved_response_tokens") {
        if !rrt.is_i64() || rrt.as_i64().unwrap_or(0) <= 0 {
            v.push(
                "context_budget.reserved_response_tokens must be a positive integer".to_string(),
            );
        }
    }
    v
}

fn validate_retrieval_policy(policy: &Value) -> Vec<String> {
    let mut v = Vec::new();
    let obj = match policy.as_object() {
        Some(o) => o,
        None => {
            v.push("retrieval_policy must be a dict".to_string());
            return v;
        }
    };
    if let Some(ar) = obj.get("allow_retrieval") {
        if !ar.is_boolean() {
            v.push("retrieval_policy.allow_retrieval must be a bool".to_string());
        }
    }
    if let Some(art) = obj.get("allowed_ref_types") {
        if !art.is_array() {
            v.push("retrieval_policy.allowed_ref_types must be a list".to_string());
        }
    }
    if let Some(fp) = obj.get("forbidden_paths") {
        if !fp.is_array() {
            v.push("retrieval_policy.forbidden_paths must be a list".to_string());
        }
    }
    v
}

fn validate_memory_digest(md: &Value) -> Vec<String> {
    let mut v = Vec::new();
    let obj = match md.as_object() {
        Some(o) => o,
        None => return v,
    };
    for f in MEMORY_DIGEST_REQUIRED {
        if !obj.contains_key(*f) {
            v.push(format!("memory_digest missing required field: {f}"));
        }
    }
    if let Some(sr) = obj.get("source_refs") {
        if !sr.is_array() {
            v.push("memory_digest.source_refs must be a list".to_string());
        }
    }
    v
}

// ---------------------------------------------------------------------------
// Public validators
// ---------------------------------------------------------------------------

pub fn validate_advisor_context_pack_v2(data: &HashMap<String, Value>) -> Vec<String> {
    let v = check_fields(data, ADVISOR_PACK_REQUIRED, "");
    if !v.is_empty() {
        return v;
    }

    let mut errors = Vec::new();

    if data.get("schema_version").and_then(Value::as_str) != Some(ADVISOR_CONTEXT_PACK_V2) {
        errors.push(format!("schema_version must be {ADVISOR_CONTEXT_PACK_V2}"));
    }
    if let Some(ct) = data.get("call_type").and_then(Value::as_str) {
        if !CALL_TYPES.contains(&ct) {
            errors.push(format!("call_type '{ct}' not in {CALL_TYPES:?}"));
        }
    }
    if let Some(af) = data.get("allowed_files") {
        if !af.is_array() {
            errors.push("allowed_files must be a list".to_string());
        }
    }
    if let Some(ff) = data.get("forbidden_files") {
        if !ff.is_array() {
            errors.push("forbidden_files must be a list".to_string());
        }
    }
    if let Some(ar) = data.get("artifact_refs") {
        if !ar.is_array() {
            errors.push("artifact_refs must be a list".to_string());
        }
    }
    if let Some(er) = data.get("evidence_refs") {
        if !er.is_array() {
            errors.push("evidence_refs must be a list".to_string());
        }
    }
    if let Some(budget) = data.get("budget") {
        errors.extend(validate_budget(budget, "budget"));
    }
    if let Some(rp) = data.get("retrieval_policy") {
        errors.extend(validate_retrieval_policy(rp));
    }
    errors
}

pub fn validate_model_context_pack_v2(data: &HashMap<String, Value>) -> Vec<String> {
    let v = check_fields(data, MODEL_PACK_REQUIRED, "");
    if !v.is_empty() {
        return v;
    }

    let mut errors = Vec::new();

    if data.get("schema_version").and_then(Value::as_str) != Some(MODEL_CONTEXT_PACK_V2) {
        errors.push(format!("schema_version must be {MODEL_CONTEXT_PACK_V2}"));
    }
    if let Some(role) = data.get("role").and_then(Value::as_str) {
        if !MODEL_ROLES.contains(&role) {
            errors.push(format!("role '{role}' not in {MODEL_ROLES:?}"));
        }
    }
    for field in &[
        "allowed_tools",
        "forbidden_tools",
        "allowed_files",
        "forbidden_files",
        "artifact_refs",
        "evidence_refs",
    ] {
        if let Some(val) = data.get(*field) {
            if !val.is_array() {
                errors.push(format!("{field} must be a list"));
            }
        }
    }
    if let Some(cb) = data.get("context_budget") {
        errors.extend(validate_context_budget(cb));
    }
    if let Some(rp) = data.get("retrieval_policy") {
        errors.extend(validate_retrieval_policy(rp));
    }
    errors
}

pub fn validate_context_retrieval_request(data: &HashMap<String, Value>) -> Vec<String> {
    let v = check_fields(data, RETRIEVAL_REQUEST_REQUIRED, "");
    if !v.is_empty() {
        return v;
    }

    let mut errors = Vec::new();

    if let Some(rt) = data.get("requester_type").and_then(Value::as_str) {
        if !REQUESTER_TYPES.contains(&rt) {
            errors.push(format!("requester_type '{rt}' not in {REQUESTER_TYPES:?}"));
        }
    }
    if let Some(reason) = data.get("reason").and_then(Value::as_str) {
        if reason.is_empty() {
            errors.push("reason must be a non-empty string".to_string());
        }
    } else {
        errors.push("reason must be a non-empty string".to_string());
    }
    if let Some(rr) = data.get("requested_refs") {
        if !rr.is_array() {
            errors.push("requested_refs must be a list".to_string());
        }
    }
    if let Some(prio) = data.get("priority").and_then(Value::as_str) {
        if !RETRIEVAL_PRIORITY.contains(&prio) {
            errors.push(format!("priority '{prio}' not in {RETRIEVAL_PRIORITY:?}"));
        }
    }
    if let Some(tb) = data.get("token_budget") {
        if !tb.is_i64() || tb.as_i64().unwrap_or(0) <= 0 {
            errors.push("token_budget must be a positive integer".to_string());
        }
    }
    if let Some(refs) = data.get("requested_refs").and_then(Value::as_array) {
        for r in refs {
            if let Some(scope) = r.get("requested_scope").and_then(Value::as_str) {
                if !CONTENT_MODES.contains(&scope) {
                    errors.push(format!(
                        "requested_scope '{scope}' not in {CONTENT_MODES:?}"
                    ));
                }
            }
        }
    }
    errors
}

pub fn validate_context_retrieval_result(data: &HashMap<String, Value>) -> Vec<String> {
    let v = check_fields(data, RETRIEVAL_RESULT_REQUIRED, "");
    if !v.is_empty() {
        return v;
    }

    let mut errors = Vec::new();

    if let Some(status) = data.get("status").and_then(Value::as_str) {
        if !RETRIEVAL_RESULT_STATUS.contains(&status) {
            errors.push(format!(
                "status '{status}' not in {RETRIEVAL_RESULT_STATUS:?}"
            ));
        }
    }
    if let Some(rr) = data.get("returned_refs") {
        if !rr.is_array() {
            errors.push("returned_refs must be a list".to_string());
        }
    }
    if let Some(tte) = data.get("total_token_estimate") {
        if !tte.is_i64() || tte.as_i64().unwrap_or(0) < 0 {
            errors.push("total_token_estimate must be a non-negative integer".to_string());
        }
    }
    if let Some(br) = data.get("budget_remaining") {
        if !br.is_i64() || br.as_i64().unwrap_or(0) < 0 {
            errors.push("budget_remaining must be a non-negative integer".to_string());
        }
    }
    if let Some(refs) = data.get("returned_refs").and_then(Value::as_array) {
        for r in refs {
            if let Some(cm) = r.get("content_mode").and_then(Value::as_str) {
                if !CONTENT_MODES.contains(&cm) {
                    errors.push(format!("content_mode '{cm}' not in {CONTENT_MODES:?}"));
                }
            }
            if r.get("token_estimate").is_none() {
                let ref_id = r.get("ref_id").and_then(Value::as_str).unwrap_or("unknown");
                errors.push(format!("returned_ref {ref_id} missing token_estimate"));
            }
        }
    }
    errors
}

pub fn validate_context_layers(data: &HashMap<String, Value>) -> Vec<String> {
    let v = check_fields(data, CONTEXT_LAYERS_REQUIRED, "");
    if !v.is_empty() {
        return v;
    }

    let mut errors = Vec::new();

    if let Some(md) = data.get("memory_digest") {
        if md.is_object() {
            errors.extend(validate_memory_digest(md));
        } else {
            errors.push("memory_digest must be a dict".to_string());
        }
    }

    if let Some(freshness) = data.get("freshness").and_then(Value::as_str) {
        if !FRESHNESS_VALUES.contains(&freshness) {
            errors.push(format!(
                "freshness '{freshness}' not in {FRESHNESS_VALUES:?}"
            ));
        }
    }

    if let Some(cp) = data.get("cache_policy").and_then(Value::as_str) {
        if !CACHE_POLICY_VALUES.contains(&cp) {
            errors.push(format!(
                "cache_policy '{cp}' not in {CACHE_POLICY_VALUES:?}"
            ));
        }
    }

    if let Some(ppp) = data.get("pack_prune_policy").and_then(Value::as_str) {
        if !PACK_PRUNE_POLICY_VALUES.contains(&ppp) {
            errors.push(format!(
                "pack_prune_policy '{ppp}' not in {PACK_PRUNE_POLICY_VALUES:?}"
            ));
        }
    }

    errors
}
