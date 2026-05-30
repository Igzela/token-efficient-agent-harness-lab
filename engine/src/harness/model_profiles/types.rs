use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::constants::{MODEL_PROFILE_SCHEMA_VERSION, SHADOW_ROUTING_SCHEMA_VERSION};

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
