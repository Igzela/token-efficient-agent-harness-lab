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
