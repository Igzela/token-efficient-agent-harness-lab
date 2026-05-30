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
