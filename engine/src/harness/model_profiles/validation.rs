use std::collections::HashSet;

use regex::Regex;
use serde_json::Value;

use super::constants::*;

// ---------------------------------------------------------------------------
// Public validators
// ---------------------------------------------------------------------------

/// Validate a model_harness_profile value. Returns list of violations.
pub fn validate_model_harness_profile(data: &Value) -> Vec<String> {
    let mut violations = Vec::new();

    // 1. Required fields
    for f in PROFILE_REQUIRED {
        if data.get(f).is_none() {
            violations.push(format!("missing required field: {}", f));
        }
    }
    if !violations.is_empty() {
        return violations;
    }

    // 2. schema_version
    if data["schema_version"] != MODEL_PROFILE_SCHEMA_VERSION {
        violations.push(format!(
            "schema_version must be {}",
            MODEL_PROFILE_SCHEMA_VERSION
        ));
    }

    // 3. Enums
    let tier = data["tier"].as_str().unwrap_or("");
    if !TIERS.contains(&tier) {
        violations.push(format!("tier {:?} not in {:?}", tier, TIERS));
    }
    let tool_strictness = data["tool_strictness"].as_str().unwrap_or("");
    if !TOOL_STRICTNESS.contains(&tool_strictness) {
        violations.push(format!(
            "tool_strictness {:?} not in {:?}",
            tool_strictness, TOOL_STRICTNESS
        ));
    }
    let json_tolerance = data["json_tolerance"].as_str().unwrap_or("");
    if !JSON_TOLERANCE.contains(&json_tolerance) {
        violations.push(format!(
            "json_tolerance {:?} not in {:?}",
            json_tolerance, JSON_TOLERANCE
        ));
    }
    let reasoning_effort = data["reasoning_effort"].as_str().unwrap_or("");
    if !REASONING_EFFORT.contains(&reasoning_effort) {
        violations.push(format!(
            "reasoning_effort {:?} not in {:?}",
            reasoning_effort, REASONING_EFFORT
        ));
    }
    let parallel = data["parallel_tool_preference"].as_str().unwrap_or("");
    if !PARALLEL_TOOL_PREFERENCE.contains(&parallel) {
        violations.push(format!(
            "parallel_tool_preference {:?} not in {:?}",
            parallel, PARALLEL_TOOL_PREFERENCE
        ));
    }
    let cache = data["cache_strategy"].as_str().unwrap_or("");
    if !CACHE_STRATEGY.contains(&cache) {
        violations.push(format!(
            "cache_strategy {:?} not in {:?}",
            cache, CACHE_STRATEGY
        ));
    }
    let fallback = data["fallback_policy"].as_str().unwrap_or("");
    if !FALLBACK_POLICY.contains(&fallback) {
        violations.push(format!(
            "fallback_policy {:?} not in {:?}",
            fallback, FALLBACK_POLICY
        ));
    }

    // 4. context_window must be positive integer
    match data.get("context_window") {
        Some(v) => match v.as_i64() {
            Some(n) if n > 0 => {}
            _ => violations.push(format!(
                "context_window must be a positive integer, got {:?}",
                v
            )),
        },
        None => violations.push("context_window must be a positive integer".to_string()),
    }

    // 5. cost_metadata validation
    let v_cost = validate_cost_metadata(data.get("cost_metadata"));
    violations.extend(v_cost);

    // 6. allowed_tools must be an array
    if let Some(v) = data.get("allowed_tools") {
        if !v.is_array() {
            violations.push("allowed_tools must be a list".to_string());
        }
    }

    // 7. forbidden_previous_tools validation
    let v_fpt = validate_forbidden_previous_tools(data.get("forbidden_previous_tools"));
    violations.extend(v_fpt);

    // 8. allowed_tools and forbidden_previous_tools conflict
    let v_conflict = check_tool_conflict(
        data.get("allowed_tools"),
        data.get("forbidden_previous_tools"),
    );
    violations.extend(v_conflict);

    // 9. Credential detection
    let v_cred = detect_credentials(data);
    violations.extend(v_cred);

    violations
}

/// Validate a shadow_routing_recommendation value. Returns list of violations.
pub fn validate_shadow_routing_recommendation(data: &Value) -> Vec<String> {
    let mut violations = Vec::new();

    // 1. Required fields
    for f in SHADOW_ROUTING_REQUIRED {
        if data.get(f).is_none() {
            violations.push(format!("missing required field: {}", f));
        }
    }
    if !violations.is_empty() {
        return violations;
    }

    // 2. schema_version
    if data.get("schema_version").and_then(|v| v.as_str()) != Some(SHADOW_ROUTING_SCHEMA_VERSION) {
        violations.push(format!(
            "schema_version must be {}",
            SHADOW_ROUTING_SCHEMA_VERSION
        ));
    }

    // 3. recommendation enum
    let rec = data["recommendation"].as_str().unwrap_or("");
    if !RECOMMENDATION_VALUES.contains(&rec) {
        violations.push(format!(
            "recommendation {:?} not in {:?}",
            rec, RECOMMENDATION_VALUES
        ));
    }

    // 4. risk_level enum
    let risk = data["risk_level"].as_str().unwrap_or("");
    if !RISK_LEVELS.contains(&risk) {
        violations.push(format!("risk_level {:?} not in {:?}", risk, RISK_LEVELS));
    }

    // 5. admission_scope must be diagnostic
    let scope = data["admission_scope"].as_str().unwrap_or("");
    if scope != "diagnostic" {
        violations.push(format!(
            "admission_scope must be 'diagnostic', got {:?}",
            scope
        ));
    }

    // 6. active_routing_allowed must be false
    match data.get("active_routing_allowed") {
        Some(Value::Bool(false)) => {}
        Some(other) => violations.push(format!(
            "active_routing_allowed must be false, got {:?}",
            other
        )),
        None => violations.push("active_routing_allowed must be false".to_string()),
    }

    // 7. evidence_refs must be an array
    if let Some(v) = data.get("evidence_refs") {
        if !v.is_array() {
            violations.push("evidence_refs must be a list".to_string());
        }
    }

    // 8. rationale must be non-empty string
    match data.get("rationale").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => {}
        _ => violations.push("rationale must be a non-empty string".to_string()),
    }

    violations
}

// ---------------------------------------------------------------------------
// Internal validation helpers
// ---------------------------------------------------------------------------

pub(super) fn validate_cost_metadata(meta: Option<&Value>) -> Vec<String> {
    let mut violations = Vec::new();
    let meta = match meta {
        Some(v) if v.is_object() => v,
        Some(_) => {
            violations.push("cost_metadata must be a dict".to_string());
            return violations;
        }
        None => return violations,
    };
    for field in &["input_cost_per_1k", "output_cost_per_1k"] {
        if let Some(val) = meta.get(*field) {
            match val.as_f64() {
                Some(n) if n >= 0.0 => {}
                _ => violations.push(format!(
                    "cost_metadata.{} must be non-negative, got {:?}",
                    field, val
                )),
            }
        }
    }
    violations
}

pub(super) fn validate_forbidden_previous_tools(items: Option<&Value>) -> Vec<String> {
    let mut violations = Vec::new();
    let items = match items {
        Some(v) if v.is_array() => v.as_array().unwrap(),
        Some(_) => {
            violations.push("forbidden_previous_tools must be a list".to_string());
            return violations;
        }
        None => return violations,
    };
    for (i, item) in items.iter().enumerate() {
        if !item.is_object() {
            violations.push(format!("forbidden_previous_tools[{}] must be a dict", i));
            continue;
        }
        if item.get("tool_id").is_none() {
            violations.push(format!("forbidden_previous_tools[{}] missing tool_id", i));
        }
        match item.get("reason").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => {}
            _ => violations.push(format!("forbidden_previous_tools[{}] missing reason", i)),
        }
        if let Some(scope) = item.get("enforcement_scope").and_then(|v| v.as_str()) {
            if !ENFORCEMENT_SCOPES.contains(&scope) {
                violations.push(format!(
                    "forbidden_previous_tools[{}].enforcement_scope {:?} not in {:?}",
                    i, scope, ENFORCEMENT_SCOPES
                ));
            }
        }
    }
    violations
}

pub(super) fn extract_tool_ids(items: Option<&Value>) -> HashSet<String> {
    let mut ids = HashSet::new();
    if let Some(arr) = items.and_then(|v| v.as_array()) {
        for item in arr {
            match item {
                Value::Object(m) => {
                    if let Some(id) = m.get("tool_id").and_then(|v| v.as_str()) {
                        ids.insert(id.to_string());
                    }
                }
                Value::String(s) => {
                    ids.insert(s.clone());
                }
                _ => {}
            }
        }
    }
    ids
}

pub(super) fn check_tool_conflict(
    allowed: Option<&Value>,
    forbidden: Option<&Value>,
) -> Vec<String> {
    let mut violations = Vec::new();
    let allowed_ids = extract_tool_ids(allowed);
    let forbidden_ids = extract_tool_ids(forbidden);
    let conflict: Vec<_> = allowed_ids.intersection(&forbidden_ids).collect();
    if !conflict.is_empty() {
        let conflict_strs: Vec<_> = conflict.iter().map(|s| s.as_str()).collect();
        violations.push(format!(
            "tool_id conflict between allowed_tools and forbidden_previous_tools: {:?}",
            conflict_strs
        ));
    }
    violations
}

pub(super) fn detect_credentials(data: &Value) -> Vec<String> {
    let mut violations = Vec::new();
    let data_str = data.to_string().to_lowercase();
    static CREDENTIAL_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        let pattern = CREDENTIAL_KEYWORDS.join("|");
        Regex::new(&pattern).expect("invalid credential keyword regex")
    });
    if CREDENTIAL_RE.is_match(&data_str) {
        for kw in CREDENTIAL_KEYWORDS {
            if data_str.contains(kw) {
                violations.push(format!(
                    "provider credential detected in profile: '{}' found; \
                     profiles describe capabilities, not credentials",
                    kw
                ));
                break;
            }
        }
    }
    violations
}
