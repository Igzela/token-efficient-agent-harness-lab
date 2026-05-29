use std::collections::HashSet;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Schema versions
// ---------------------------------------------------------------------------

pub const MODEL_PROFILE_SCHEMA_VERSION: &str = "model_harness_profile.v1";
pub const SHADOW_ROUTING_SCHEMA_VERSION: &str = "shadow_routing_recommendation.v1";

// ---------------------------------------------------------------------------
// Enum sets
// ---------------------------------------------------------------------------

pub const TIERS: &[&str] = &[
    "cheap_executor",
    "balanced_worker",
    "strong_planner",
    "verifier",
    "advisor",
];

pub const TOOL_STRICTNESS: &[&str] = &["strict", "tolerant", "unsupported"];

pub const JSON_TOLERANCE: &[&str] = &["strict_json", "tolerant_json", "text_only"];

pub const REASONING_EFFORT: &[&str] = &["low", "medium", "high"];

pub const PARALLEL_TOOL_PREFERENCE: &[&str] = &["none", "allowed", "preferred", "forbidden"];

pub const CACHE_STRATEGY: &[&str] = &["no_cache", "read_cache", "write_cache", "read_write_cache"];

pub const FALLBACK_POLICY: &[&str] = &[
    "no_fallback",
    "same_tier_only",
    "lower_cost_allowed",
    "higher_quality_allowed",
    "human_required",
];

pub const ENFORCEMENT_SCOPES: &[&str] = &[
    "prompt_assembly",
    "gateway_validation",
    "context_broker",
    "all",
];

pub const RECOMMENDATION_VALUES: &[&str] = &[
    "keep_baseline",
    "try_candidate",
    "reject_candidate",
    "needs_more_evidence",
];

pub const RISK_LEVELS: &[&str] = &["low", "medium", "high", "critical"];

pub const CREDENTIAL_KEYWORDS: &[&str] = &[
    "api_key",
    "secret",
    "token",
    "password",
    "credential",
    "private_key",
    "access_key",
    "auth_token",
];

// ---------------------------------------------------------------------------
// Required fields
// ---------------------------------------------------------------------------

pub const PROFILE_REQUIRED: &[&str] = &[
    "schema_version",
    "profile_id",
    "provider",
    "model_id",
    "tier",
    "tool_strictness",
    "json_tolerance",
    "reasoning_effort",
    "output_format_expectation",
    "parallel_tool_preference",
    "escaping_quirks",
    "cache_strategy",
    "fallback_policy",
    "context_window",
    "cost_metadata",
    "allowed_tools",
    "forbidden_previous_tools",
];

pub const SHADOW_ROUTING_REQUIRED: &[&str] = &[
    "recommendation_id",
    "task_family",
    "variant_family",
    "success_criterion",
    "candidate_profile_id",
    "baseline_profile_id",
    "rationale",
    "evidence_refs",
    "expected_quality_delta",
    "expected_cost_delta",
    "risk_level",
    "recommendation",
    "admission_scope",
    "active_routing_allowed",
];

// ---------------------------------------------------------------------------
// Data structs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CostMetadata {
    pub input_cost_per_1k: f64,
    pub output_cost_per_1k: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_cost_per_1k: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write_cost_per_1k: Option<f64>,
}

impl Default for CostMetadata {
    fn default() -> Self {
        Self {
            input_cost_per_1k: 0.0,
            output_cost_per_1k: 0.0,
            cache_read_cost_per_1k: None,
            cache_write_cost_per_1k: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ForbiddenPreviousTool {
    pub tool_id: String,
    #[serde(default)]
    pub tool_type: String,
    #[serde(default)]
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement_tool_id: Option<String>,
    #[serde(default = "default_enforcement_scope")]
    pub enforcement_scope: String,
}

fn default_enforcement_scope() -> String {
    "all".to_string()
}

impl Default for ForbiddenPreviousTool {
    fn default() -> Self {
        Self {
            tool_id: String::new(),
            tool_type: String::new(),
            reason: String::new(),
            replacement_tool_id: None,
            enforcement_scope: "all".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ModelHarnessProfile {
    pub schema_version: String,
    pub profile_id: String,
    pub provider: String,
    pub model_id: String,
    pub tier: String,
    pub tool_strictness: String,
    pub json_tolerance: String,
    pub reasoning_effort: String,
    pub output_format_expectation: String,
    pub parallel_tool_preference: String,
    pub escaping_quirks: String,
    pub cache_strategy: String,
    pub fallback_policy: String,
    pub context_window: i64,
    pub cost_metadata: CostMetadata,
    pub allowed_tools: Vec<Value>,
    pub forbidden_previous_tools: Vec<Value>,
}

impl Default for ModelHarnessProfile {
    fn default() -> Self {
        Self {
            schema_version: MODEL_PROFILE_SCHEMA_VERSION.to_string(),
            profile_id: String::new(),
            provider: String::new(),
            model_id: String::new(),
            tier: "cheap_executor".to_string(),
            tool_strictness: "tolerant".to_string(),
            json_tolerance: "tolerant_json".to_string(),
            reasoning_effort: "medium".to_string(),
            output_format_expectation: String::new(),
            parallel_tool_preference: "allowed".to_string(),
            escaping_quirks: String::new(),
            cache_strategy: "no_cache".to_string(),
            fallback_policy: "no_fallback".to_string(),
            context_window: 4096,
            cost_metadata: CostMetadata::default(),
            allowed_tools: Vec::new(),
            forbidden_previous_tools: Vec::new(),
        }
    }
}

impl ModelHarnessProfile {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("ModelHarnessProfile should serialize to JSON")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ShadowRoutingRecommendation {
    pub schema_version: String,
    pub recommendation_id: String,
    pub task_family: String,
    pub variant_family: String,
    pub success_criterion: String,
    pub candidate_profile_id: String,
    pub baseline_profile_id: String,
    pub rationale: String,
    pub evidence_refs: Vec<String>,
    pub expected_quality_delta: f64,
    pub expected_cost_delta: f64,
    pub risk_level: String,
    pub recommendation: String,
    pub admission_scope: String,
    pub active_routing_allowed: bool,
}

impl Default for ShadowRoutingRecommendation {
    fn default() -> Self {
        Self {
            schema_version: SHADOW_ROUTING_SCHEMA_VERSION.to_string(),
            recommendation_id: String::new(),
            task_family: String::new(),
            variant_family: String::new(),
            success_criterion: String::new(),
            candidate_profile_id: String::new(),
            baseline_profile_id: String::new(),
            rationale: String::new(),
            evidence_refs: Vec::new(),
            expected_quality_delta: 0.0,
            expected_cost_delta: 0.0,
            risk_level: "low".to_string(),
            recommendation: "keep_baseline".to_string(),
            admission_scope: "diagnostic".to_string(),
            active_routing_allowed: false,
        }
    }
}

impl ShadowRoutingRecommendation {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("ShadowRoutingRecommendation should serialize to JSON")
    }
}

// ---------------------------------------------------------------------------
// Validation
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

fn validate_cost_metadata(meta: Option<&Value>) -> Vec<String> {
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

fn validate_forbidden_previous_tools(items: Option<&Value>) -> Vec<String> {
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

fn extract_tool_ids(items: Option<&Value>) -> HashSet<String> {
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

fn check_tool_conflict(allowed: Option<&Value>, forbidden: Option<&Value>) -> Vec<String> {
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

fn detect_credentials(data: &Value) -> Vec<String> {
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

// ---------------------------------------------------------------------------
// Shadow routing helpers
// ---------------------------------------------------------------------------

/// Check if a recommendation is shadow-only (diagnostic, not active).
pub fn is_shadow_only(recommendation: &Value) -> bool {
    recommendation
        .get("admission_scope")
        .and_then(|v| v.as_str())
        == Some("diagnostic")
        && recommendation
            .get("active_routing_allowed")
            .and_then(|v| v.as_bool())
            == Some(false)
}

/// Check if a shadow recommendation can be compared with a usage_ledger group.
pub fn can_compare_with_usage_ledger(
    recommendation: &Value,
    usage_ledger_group: &str,
) -> (bool, String) {
    let task = recommendation
        .get("task_family")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let variant = recommendation
        .get("variant_family")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let criterion = recommendation
        .get("success_criterion")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let rec_group = format!("{}/{}/{}", task, variant, criterion);

    let ledger_tail = if let Some(idx) = usage_ledger_group.find('/') {
        &usage_ledger_group[idx + 1..]
    } else {
        usage_ledger_group
    };

    if rec_group == ledger_tail {
        (
            true,
            "recommendation matches usage_ledger group tail".to_string(),
        )
    } else {
        (
            false,
            format!(
                "recommendation {:?} does not match usage_ledger group {:?}",
                rec_group, usage_ledger_group
            ),
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_profile_json() -> Value {
        json!({
            "schema_version": "model_harness_profile.v1",
            "profile_id": "test-profile-1",
            "provider": "stub",
            "model_id": "stub-planner",
            "tier": "strong_planner",
            "tool_strictness": "strict",
            "json_tolerance": "strict_json",
            "reasoning_effort": "high",
            "output_format_expectation": "json",
            "parallel_tool_preference": "allowed",
            "escaping_quirks": "none",
            "cache_strategy": "read_write_cache",
            "fallback_policy": "same_tier_only",
            "context_window": 200000,
            "cost_metadata": {
                "input_cost_per_1k": 0.015,
                "output_cost_per_1k": 0.06
            },
            "allowed_tools": ["tool_a", "tool_b"],
            "forbidden_previous_tools": [
                {"tool_id": "tool_c", "reason": "conflicts with tool_a"}
            ]
        })
    }

    fn valid_shadow_json() -> Value {
        json!({
            "schema_version": "shadow_routing_recommendation.v1",
            "recommendation_id": "rec-1",
            "task_family": "code/generate",
            "variant_family": "standard",
            "success_criterion": "pass_tests",
            "candidate_profile_id": "cand-1",
            "baseline_profile_id": "base-1",
            "rationale": "Testing candidate model for cost reduction",
            "evidence_refs": ["ref-1", "ref-2"],
            "expected_quality_delta": -0.01,
            "expected_cost_delta": -0.30,
            "risk_level": "low",
            "recommendation": "try_candidate",
            "admission_scope": "diagnostic",
            "active_routing_allowed": false
        })
    }

    #[test]
    fn valid_profile_passes() {
        let data = valid_profile_json();
        let violations = validate_model_harness_profile(&data);
        assert!(
            violations.is_empty(),
            "expected no violations, got: {:?}",
            violations
        );
    }

    #[test]
    fn missing_required_field_detected() {
        let mut data = valid_profile_json();
        data.as_object_mut().unwrap().remove("profile_id");
        let violations = validate_model_harness_profile(&data);
        assert!(violations.iter().any(|v| v.contains("profile_id")));
    }

    #[test]
    fn invalid_tier_detected() {
        let mut data = valid_profile_json();
        data["tier"] = json!("nonexistent_tier");
        let violations = validate_model_harness_profile(&data);
        assert!(violations.iter().any(|v| v.contains("tier")));
    }

    #[test]
    fn negative_cost_detected() {
        let mut data = valid_profile_json();
        data["cost_metadata"]["input_cost_per_1k"] = json!(-0.01);
        let violations = validate_model_harness_profile(&data);
        assert!(violations.iter().any(|v| v.contains("non-negative")));
    }

    #[test]
    fn credential_in_profile_detected() {
        let mut data = valid_profile_json();
        data["provider"] = json!("uses api_key from env");
        let violations = validate_model_harness_profile(&data);
        assert!(violations.iter().any(|v| v.contains("credential")));
    }

    #[test]
    fn tool_conflict_detected() {
        let mut data = valid_profile_json();
        data["allowed_tools"] = json!(["tool_a", "tool_b"]);
        data["forbidden_previous_tools"] =
            json!([{"tool_id": "tool_a", "reason": "reuse conflict"}]);
        let violations = validate_model_harness_profile(&data);
        assert!(violations.iter().any(|v| v.contains("conflict")));
    }

    #[test]
    fn valid_shadow_passes() {
        let data = valid_shadow_json();
        let violations = validate_shadow_routing_recommendation(&data);
        assert!(
            violations.is_empty(),
            "expected no violations, got: {:?}",
            violations
        );
    }

    #[test]
    fn shadow_non_diagnostic_scope_detected() {
        let mut data = valid_shadow_json();
        data["admission_scope"] = json!("active");
        let violations = validate_shadow_routing_recommendation(&data);
        assert!(violations.iter().any(|v| v.contains("diagnostic")));
    }

    #[test]
    fn shadow_active_routing_not_false_detected() {
        let mut data = valid_shadow_json();
        data["active_routing_allowed"] = json!(true);
        let violations = validate_shadow_routing_recommendation(&data);
        assert!(violations
            .iter()
            .any(|v| v.contains("active_routing_allowed")));
    }

    #[test]
    fn is_shadow_only_works() {
        let data = valid_shadow_json();
        assert!(is_shadow_only(&data));

        let mut data2 = data.clone();
        data2["active_routing_allowed"] = json!(true);
        assert!(!is_shadow_only(&data2));
    }

    #[test]
    fn can_compare_with_usage_ledger_match() {
        let data = valid_shadow_json();
        let (ok, msg) =
            can_compare_with_usage_ledger(&data, "eval_suite/code/generate/standard/pass_tests");
        assert!(ok);
        assert!(msg.contains("matches"));
    }

    #[test]
    fn can_compare_with_usage_ledger_mismatch() {
        let data = valid_shadow_json();
        let (ok, msg) = can_compare_with_usage_ledger(&data, "eval_suite/other/family/criterion");
        assert!(!ok);
        assert!(msg.contains("does not match"));
    }

    #[test]
    fn empty_rationale_detected() {
        let mut data = valid_shadow_json();
        data["rationale"] = json!("");
        let violations = validate_shadow_routing_recommendation(&data);
        assert!(violations.iter().any(|v| v.contains("rationale")));
    }

    #[test]
    fn profile_struct_roundtrip() {
        let data = valid_profile_json();
        let profile: ModelHarnessProfile = serde_json::from_value(data.clone()).unwrap();
        let back = profile.to_value();
        assert_eq!(back["profile_id"], json!("test-profile-1"));
        assert_eq!(back["tier"], json!("strong_planner"));
    }

    #[test]
    fn shadow_struct_roundtrip() {
        let data = valid_shadow_json();
        let rec: ShadowRoutingRecommendation = serde_json::from_value(data.clone()).unwrap();
        let back = rec.to_value();
        assert_eq!(back["recommendation_id"], json!("rec-1"));
        assert_eq!(back["admission_scope"], json!("diagnostic"));
    }

    #[test]
    fn cost_metadata_default() {
        let cm = CostMetadata::default();
        assert_eq!(cm.input_cost_per_1k, 0.0);
        assert_eq!(cm.output_cost_per_1k, 0.0);
        assert!(cm.cache_read_cost_per_1k.is_none());
    }

    #[test]
    fn forbidden_previous_tool_default() {
        let fpt = ForbiddenPreviousTool::default();
        assert_eq!(fpt.enforcement_scope, "all");
        assert!(fpt.replacement_tool_id.is_none());
    }

    #[test]
    fn context_window_zero_detected() {
        let mut data = valid_profile_json();
        data["context_window"] = json!(0);
        let violations = validate_model_harness_profile(&data);
        assert!(violations.iter().any(|v| v.contains("positive integer")));
    }
}
