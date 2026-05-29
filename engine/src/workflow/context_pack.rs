use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Schema versions
// ---------------------------------------------------------------------------

pub const ADVISOR_CONTEXT_PACK_V2: &str = "advisor_context_pack.v2";
pub const MODEL_CONTEXT_PACK_V2: &str = "model_context_pack.v2";
pub const CONTEXT_RETRIEVAL_REQUEST: &str = "context_retrieval_request.v1";
pub const CONTEXT_RETRIEVAL_RESULT: &str = "context_retrieval_result.v1";
pub const CONTEXT_LAYERS_VERSION: &str = "context_layers.v1";

// ---------------------------------------------------------------------------
// Constant sets
// ---------------------------------------------------------------------------

pub const CALL_TYPES: &[&str] = &["preflight", "correction", "arbitration", "risk_scan"];

pub const MODEL_ROLES: &[&str] = &[
    "planner",
    "executor",
    "debugger",
    "verifier",
    "advisor",
    "integrator",
];

pub const CONTENT_MODES: &[&str] = &["summary", "excerpt", "full"];

pub const RETRIEVAL_RESULT_STATUS: &[&str] = &[
    "fulfilled",
    "partial",
    "denied",
    "not_found",
    "budget_exceeded",
];

pub const REQUESTER_TYPES: &[&str] = &["advisor", "model", "verifier", "human", "evaluator"];

pub const REF_TYPES: &[&str] = &[
    "run_log",
    "completion",
    "handoff_pack",
    "artifact",
    "event",
    "digest",
    "source_excerpt",
];

pub const FRESHNESS_VALUES: &[&str] = &["current", "stale", "unknown"];

pub const CACHE_POLICY_VALUES: &[&str] = &[
    "no_cache",
    "read_cache_allowed",
    "write_cache_allowed",
    "read_write_cache_allowed",
];

pub const PACK_PRUNE_POLICY_VALUES: &[&str] = &[
    "preserve_invariants",
    "drop_recent_evidence_first",
    "drop_memory_digest_first",
    "deny_if_over_budget",
];

pub const RETRIEVAL_PRIORITY: &[&str] = &["low", "normal", "high"];

// ---------------------------------------------------------------------------
// Required fields per schema
// ---------------------------------------------------------------------------

pub const ADVISOR_PACK_REQUIRED: &[&str] = &[
    "schema_version",
    "pack_id",
    "task_id",
    "item_id",
    "call_type",
    "objective",
    "current_status",
    "allowed_files",
    "forbidden_files",
    "artifact_refs",
    "evidence_refs",
    "quality_signals",
    "budget",
    "retrieval_policy",
    "created_at",
];

pub const MODEL_PACK_REQUIRED: &[&str] = &[
    "schema_version",
    "pack_id",
    "task_id",
    "item_id",
    "model_tier",
    "model_harness_profile_id",
    "role",
    "task_summary",
    "allowed_tools",
    "forbidden_tools",
    "allowed_files",
    "forbidden_files",
    "artifact_refs",
    "evidence_refs",
    "context_budget",
    "retrieval_policy",
    "created_at",
];

pub const RETRIEVAL_REQUEST_REQUIRED: &[&str] = &[
    "request_id",
    "requester_id",
    "requester_type",
    "task_id",
    "reason",
    "requested_refs",
    "token_budget",
    "priority",
    "created_at",
];

pub const RETRIEVAL_RESULT_REQUIRED: &[&str] = &[
    "request_id",
    "result_id",
    "status",
    "returned_refs",
    "total_token_estimate",
    "budget_remaining",
    "created_at",
];

pub const CONTEXT_LAYERS_REQUIRED: &[&str] = &[
    "invariants",
    "task_pack",
    "dynamic_refs",
    "memory_digest",
    "recent_evidence",
];

pub const MEMORY_DIGEST_REQUIRED: &[&str] =
    &["source_refs", "expiry_policy", "conflict_resolution"];

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ContextBudget {
    pub max_context_tokens: i64,
    pub preferred_context_tokens: i64,
    pub max_response_tokens: Option<i64>,
    pub reserved_response_tokens: Option<i64>,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            max_context_tokens: 4000,
            preferred_context_tokens: 2000,
            max_response_tokens: None,
            reserved_response_tokens: None,
        }
    }
}

impl ContextBudget {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RetrievalPolicy {
    pub allow_retrieval: bool,
    pub allowed_ref_types: Option<Vec<String>>,
    pub forbidden_paths: Option<Vec<String>>,
    pub max_retrieval_calls: Option<i64>,
}

impl Default for RetrievalPolicy {
    fn default() -> Self {
        Self {
            allow_retrieval: true,
            allowed_ref_types: None,
            forbidden_paths: None,
            max_retrieval_calls: None,
        }
    }
}

impl RetrievalPolicy {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MemoryDigest {
    pub source_refs: Vec<String>,
    pub expiry_policy: String,
    pub conflict_resolution: String,
    pub summary: Option<String>,
}

impl Default for MemoryDigest {
    fn default() -> Self {
        Self {
            source_refs: Vec::new(),
            expiry_policy: String::new(),
            conflict_resolution: String::new(),
            summary: None,
        }
    }
}

impl MemoryDigest {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ContextLayers {
    pub invariants: HashMap<String, Value>,
    pub task_pack: HashMap<String, Value>,
    pub dynamic_refs: Vec<HashMap<String, Value>>,
    pub memory_digest: MemoryDigest,
    pub recent_evidence: Vec<HashMap<String, Value>>,
    pub freshness: String,
    pub cache_policy: String,
    pub pack_prune_policy: String,
}

impl Default for ContextLayers {
    fn default() -> Self {
        Self {
            invariants: HashMap::new(),
            task_pack: HashMap::new(),
            dynamic_refs: Vec::new(),
            memory_digest: MemoryDigest::default(),
            recent_evidence: Vec::new(),
            freshness: "current".to_string(),
            cache_policy: "no_cache".to_string(),
            pack_prune_policy: "preserve_invariants".to_string(),
        }
    }
}

impl ContextLayers {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Budget compliance and pruning
// ---------------------------------------------------------------------------

pub fn check_budget_compliance(
    pack_data: &HashMap<String, Value>,
    total_tokens_used: i64,
) -> (bool, String) {
    let budget = pack_data
        .get("context_budget")
        .or_else(|| pack_data.get("budget"));
    let max_tokens = budget
        .and_then(|b| b.get("max_context_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if max_tokens <= 0 {
        return (true, "no budget defined".to_string());
    }
    if total_tokens_used <= max_tokens {
        (
            true,
            format!("within budget ({total_tokens_used}/{max_tokens})"),
        )
    } else {
        (
            false,
            format!("over budget ({total_tokens_used}/{max_tokens})"),
        )
    }
}

pub fn apply_prune_policy(
    pack_data: &HashMap<String, Value>,
    current_tokens: i64,
    max_tokens: i64,
) -> Result<(HashMap<String, Value>, String), String> {
    let policy = pack_data
        .get("pack_prune_policy")
        .and_then(Value::as_str)
        .unwrap_or("deny_if_over_budget");

    if current_tokens <= max_tokens {
        return Ok((pack_data.clone(), "no_pruning_needed".to_string()));
    }

    if policy == "deny_if_over_budget" {
        return Err(format!(
            "pack over budget ({current_tokens}/{max_tokens}) and policy is deny_if_over_budget"
        ));
    }

    let mut pruned = pack_data.clone();
    let layers = pruned
        .get("context_layers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    if policy == "drop_recent_evidence_first" {
        if let Some(re) = layers.get("recent_evidence") {
            if re.as_array().map_or(false, |a| !a.is_empty()) {
                let mut new_layers = layers.clone();
                new_layers.insert("recent_evidence".to_string(), Value::Array(vec![]));
                pruned.insert("context_layers".to_string(), Value::Object(new_layers));
                return Ok((pruned, "dropped_recent_evidence".to_string()));
            }
        }
    } else if policy == "drop_memory_digest_first" {
        if let Some(md) = layers.get("memory_digest") {
            if md.is_object() {
                let mut new_layers = layers.clone();
                let mut empty_digest = serde_json::Map::new();
                empty_digest.insert("source_refs".to_string(), Value::Array(vec![]));
                empty_digest.insert(
                    "expiry_policy".to_string(),
                    Value::String("on_prune".to_string()),
                );
                empty_digest.insert(
                    "conflict_resolution".to_string(),
                    Value::String("drop".to_string()),
                );
                new_layers.insert("memory_digest".to_string(), Value::Object(empty_digest));
                pruned.insert("context_layers".to_string(), Value::Object(new_layers));
                return Ok((pruned, "dropped_memory_digest".to_string()));
            }
        }
    } else if policy == "preserve_invariants" {
        if let Some(re) = layers.get("recent_evidence") {
            if re.as_array().map_or(false, |a| !a.is_empty()) {
                let mut new_layers = layers.clone();
                new_layers.insert("recent_evidence".to_string(), Value::Array(vec![]));
                pruned.insert("context_layers".to_string(), Value::Object(new_layers));
                return Ok((pruned, "dropped_recent_evidence".to_string()));
            }
        }
        if let Some(md) = layers.get("memory_digest") {
            if md.is_object() {
                let mut new_layers = layers.clone();
                let mut empty_digest = serde_json::Map::new();
                empty_digest.insert("source_refs".to_string(), Value::Array(vec![]));
                empty_digest.insert(
                    "expiry_policy".to_string(),
                    Value::String("on_prune".to_string()),
                );
                empty_digest.insert(
                    "conflict_resolution".to_string(),
                    Value::String("drop".to_string()),
                );
                new_layers.insert("memory_digest".to_string(), Value::Object(empty_digest));
                pruned.insert("context_layers".to_string(), Value::Object(new_layers));
                return Ok((pruned, "dropped_memory_digest".to_string()));
            }
        }
    }

    Err(format!(
        "cannot prune pack under budget ({current_tokens}/{max_tokens}) with policy '{policy}'"
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_advisor_pack() -> HashMap<String, Value> {
        let mut d = HashMap::new();
        d.insert(
            "schema_version".to_string(),
            json!("advisor_context_pack.v2"),
        );
        d.insert("pack_id".to_string(), json!("p1"));
        d.insert("task_id".to_string(), json!("t1"));
        d.insert("item_id".to_string(), json!("i1"));
        d.insert("call_type".to_string(), json!("preflight"));
        d.insert("objective".to_string(), json!("test"));
        d.insert("current_status".to_string(), json!("active"));
        d.insert("allowed_files".to_string(), json!([]));
        d.insert("forbidden_files".to_string(), json!([]));
        d.insert("artifact_refs".to_string(), json!([]));
        d.insert("evidence_refs".to_string(), json!([]));
        d.insert("quality_signals".to_string(), json!([]));
        d.insert("budget".to_string(), json!({"max_context_tokens": 4000}));
        d.insert(
            "retrieval_policy".to_string(),
            json!({"allow_retrieval": true}),
        );
        d.insert("created_at".to_string(), json!("2026-01-01T00:00:00Z"));
        d
    }

    #[test]
    fn test_advisor_pack_valid() {
        let d = make_advisor_pack();
        assert!(validate_advisor_context_pack_v2(&d).is_empty());
    }

    #[test]
    fn test_advisor_pack_bad_call_type() {
        let mut d = make_advisor_pack();
        d.insert("call_type".to_string(), json!("bogus"));
        let errs = validate_advisor_context_pack_v2(&d);
        assert!(errs.iter().any(|e| e.contains("call_type")));
    }

    #[test]
    fn test_advisor_pack_missing_field() {
        let mut d = make_advisor_pack();
        d.remove("pack_id");
        let errs = validate_advisor_context_pack_v2(&d);
        assert!(errs[0].contains("missing required field: pack_id"));
    }

    #[test]
    fn test_model_pack_valid() {
        let mut d = HashMap::new();
        d.insert("schema_version".to_string(), json!("model_context_pack.v2"));
        d.insert("pack_id".to_string(), json!("p1"));
        d.insert("task_id".to_string(), json!("t1"));
        d.insert("item_id".to_string(), json!("i1"));
        d.insert("model_tier".to_string(), json!("balanced_worker"));
        d.insert("model_harness_profile_id".to_string(), json!("h1"));
        d.insert("role".to_string(), json!("planner"));
        d.insert("task_summary".to_string(), json!("test"));
        d.insert("allowed_tools".to_string(), json!([]));
        d.insert("forbidden_tools".to_string(), json!([]));
        d.insert("allowed_files".to_string(), json!([]));
        d.insert("forbidden_files".to_string(), json!([]));
        d.insert("artifact_refs".to_string(), json!([]));
        d.insert("evidence_refs".to_string(), json!([]));
        d.insert(
            "context_budget".to_string(),
            json!({"max_context_tokens": 4000}),
        );
        d.insert(
            "retrieval_policy".to_string(),
            json!({"allow_retrieval": true}),
        );
        d.insert("created_at".to_string(), json!("2026-01-01T00:00:00Z"));
        assert!(validate_model_context_pack_v2(&d).is_empty());
    }

    #[test]
    fn test_model_pack_bad_role() {
        let mut d = HashMap::new();
        d.insert("schema_version".to_string(), json!("model_context_pack.v2"));
        d.insert("pack_id".to_string(), json!("p1"));
        d.insert("task_id".to_string(), json!("t1"));
        d.insert("item_id".to_string(), json!("i1"));
        d.insert("model_tier".to_string(), json!("balanced_worker"));
        d.insert("model_harness_profile_id".to_string(), json!("h1"));
        d.insert("role".to_string(), json!("bogus"));
        d.insert("task_summary".to_string(), json!("test"));
        d.insert("allowed_tools".to_string(), json!([]));
        d.insert("forbidden_tools".to_string(), json!([]));
        d.insert("allowed_files".to_string(), json!([]));
        d.insert("forbidden_files".to_string(), json!([]));
        d.insert("artifact_refs".to_string(), json!([]));
        d.insert("evidence_refs".to_string(), json!([]));
        d.insert(
            "context_budget".to_string(),
            json!({"max_context_tokens": 4000}),
        );
        d.insert(
            "retrieval_policy".to_string(),
            json!({"allow_retrieval": true}),
        );
        d.insert("created_at".to_string(), json!("2026-01-01T00:00:00Z"));
        let errs = validate_model_context_pack_v2(&d);
        assert!(errs.iter().any(|e| e.contains("role")));
    }

    #[test]
    fn test_retrieval_request_valid() {
        let mut d = HashMap::new();
        d.insert("request_id".to_string(), json!("r1"));
        d.insert("requester_id".to_string(), json!("u1"));
        d.insert("requester_type".to_string(), json!("advisor"));
        d.insert("task_id".to_string(), json!("t1"));
        d.insert("reason".to_string(), json!("need context"));
        d.insert("requested_refs".to_string(), json!([]));
        d.insert("token_budget".to_string(), json!(1000));
        d.insert("priority".to_string(), json!("normal"));
        d.insert("created_at".to_string(), json!("2026-01-01T00:00:00Z"));
        assert!(validate_context_retrieval_request(&d).is_empty());
    }

    #[test]
    fn test_retrieval_request_empty_reason() {
        let mut d = HashMap::new();
        d.insert("request_id".to_string(), json!("r1"));
        d.insert("requester_id".to_string(), json!("u1"));
        d.insert("requester_type".to_string(), json!("advisor"));
        d.insert("task_id".to_string(), json!("t1"));
        d.insert("reason".to_string(), json!(""));
        d.insert("requested_refs".to_string(), json!([]));
        d.insert("token_budget".to_string(), json!(1000));
        d.insert("priority".to_string(), json!("normal"));
        d.insert("created_at".to_string(), json!("2026-01-01T00:00:00Z"));
        let errs = validate_context_retrieval_request(&d);
        assert!(errs.iter().any(|e| e.contains("non-empty")));
    }

    #[test]
    fn test_retrieval_result_valid() {
        let mut d = HashMap::new();
        d.insert("request_id".to_string(), json!("r1"));
        d.insert("result_id".to_string(), json!("res1"));
        d.insert("status".to_string(), json!("fulfilled"));
        d.insert(
            "returned_refs".to_string(),
            json!([{"ref_id": "a1", "content_mode": "summary", "token_estimate": 100}]),
        );
        d.insert("total_token_estimate".to_string(), json!(100));
        d.insert("budget_remaining".to_string(), json!(900));
        d.insert("created_at".to_string(), json!("2026-01-01T00:00:00Z"));
        assert!(validate_context_retrieval_result(&d).is_empty());
    }

    #[test]
    fn test_retrieval_result_missing_token_estimate() {
        let mut d = HashMap::new();
        d.insert("request_id".to_string(), json!("r1"));
        d.insert("result_id".to_string(), json!("res1"));
        d.insert("status".to_string(), json!("fulfilled"));
        d.insert(
            "returned_refs".to_string(),
            json!([{"ref_id": "a1", "content_mode": "summary"}]),
        );
        d.insert("total_token_estimate".to_string(), json!(0));
        d.insert("budget_remaining".to_string(), json!(1000));
        d.insert("created_at".to_string(), json!("2026-01-01T00:00:00Z"));
        let errs = validate_context_retrieval_result(&d);
        assert!(errs.iter().any(|e| e.contains("missing token_estimate")));
    }

    #[test]
    fn test_context_layers_valid() {
        let mut d = HashMap::new();
        d.insert("invariants".to_string(), json!({}));
        d.insert("task_pack".to_string(), json!({}));
        d.insert("dynamic_refs".to_string(), json!([]));
        d.insert(
            "memory_digest".to_string(),
            json!({"source_refs": [], "expiry_policy": "on_prune", "conflict_resolution": "drop"}),
        );
        d.insert("recent_evidence".to_string(), json!([]));
        assert!(validate_context_layers(&d).is_empty());
    }

    #[test]
    fn test_context_layers_missing_memory_digest_field() {
        let mut d = HashMap::new();
        d.insert("invariants".to_string(), json!({}));
        d.insert("task_pack".to_string(), json!({}));
        d.insert("dynamic_refs".to_string(), json!([]));
        d.insert("memory_digest".to_string(), json!({"source_refs": []}));
        d.insert("recent_evidence".to_string(), json!([]));
        let errs = validate_context_layers(&d);
        assert!(errs.iter().any(|e| e.contains("expiry_policy")));
    }

    #[test]
    fn test_check_budget_compliance_within() {
        let mut d = HashMap::new();
        d.insert(
            "context_budget".to_string(),
            json!({"max_context_tokens": 4000}),
        );
        let (ok, reason) = check_budget_compliance(&d, 2000);
        assert!(ok);
        assert!(reason.contains("within budget"));
    }

    #[test]
    fn test_check_budget_compliance_over() {
        let mut d = HashMap::new();
        d.insert(
            "context_budget".to_string(),
            json!({"max_context_tokens": 1000}),
        );
        let (ok, reason) = check_budget_compliance(&d, 2000);
        assert!(!ok);
        assert!(reason.contains("over budget"));
    }

    #[test]
    fn test_check_budget_compliance_no_budget() {
        let d = HashMap::new();
        let (ok, reason) = check_budget_compliance(&d, 9999);
        assert!(ok);
        assert!(reason.contains("no budget"));
    }

    #[test]
    fn test_apply_prune_policy_deny_if_over() {
        let mut d = HashMap::new();
        d.insert(
            "pack_prune_policy".to_string(),
            json!("deny_if_over_budget"),
        );
        let result = apply_prune_policy(&d, 2000, 1000);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("deny_if_over_budget"));
    }

    #[test]
    fn test_apply_prune_policy_no_pruning_needed() {
        let d = HashMap::new();
        let (_, action) = apply_prune_policy(&d, 500, 1000).unwrap();
        assert_eq!(action, "no_pruning_needed");
    }

    #[test]
    fn test_apply_prune_policy_drop_recent_evidence() {
        let mut d = HashMap::new();
        d.insert(
            "pack_prune_policy".to_string(),
            json!("drop_recent_evidence_first"),
        );
        let mut layers = serde_json::Map::new();
        layers.insert("recent_evidence".to_string(), json!([{"ref": "e1"}]));
        layers.insert("memory_digest".to_string(), json!({}));
        d.insert("context_layers".to_string(), Value::Object(layers));
        let (pruned, action) = apply_prune_policy(&d, 2000, 1000).unwrap();
        assert_eq!(action, "dropped_recent_evidence");
        let new_layers = pruned.get("context_layers").unwrap().as_object().unwrap();
        assert!(new_layers
            .get("recent_evidence")
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty());
    }
}
