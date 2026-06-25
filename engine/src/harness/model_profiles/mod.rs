#![allow(dead_code, unused_imports)]

pub(crate) mod constants;
pub(crate) mod shadow;
pub(crate) mod types;
pub(crate) mod validation;

pub use constants::{
    CACHE_STRATEGY, CREDENTIAL_KEYWORDS, ENFORCEMENT_SCOPES, FALLBACK_POLICY, JSON_TOLERANCE,
    MODEL_PROFILE_SCHEMA_VERSION, PARALLEL_TOOL_PREFERENCE, PROFILE_REQUIRED, REASONING_EFFORT,
    RECOMMENDATION_VALUES, RISK_LEVELS, SHADOW_ROUTING_REQUIRED, SHADOW_ROUTING_SCHEMA_VERSION,
    TIERS, TOOL_STRICTNESS,
};
pub use shadow::{can_compare_with_usage_ledger, is_shadow_only};
pub use types::{
    CostMetadata, ForbiddenPreviousTool, ModelHarnessProfile, ShadowRoutingRecommendation,
};
pub use validation::{validate_model_harness_profile, validate_shadow_routing_recommendation};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_profile_json() -> serde_json::Value {
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

    fn valid_shadow_json() -> serde_json::Value {
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
