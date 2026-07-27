//! Normalized managed-executor usage evidence.
//!
//! One Rust-owned contract for post-call usage observation across Codex JSONL,
//! Claude Code JSONL, OpenCode SQLite (read-only), and Rust provider/proxy
//! responses. Importers produce evidence only; `LocalProductStore` / ProductTask
//! budget remains the sole budget authority.
//!
//! Axes (classify separately per executor):
//! 1. exact post-call usage evidence
//! 2. enforceable pre-call / cross-call budget authority
//!
//! Accurate logs do **not** imply a native per-request hard token cap.

pub mod claude_adapter;
pub mod codex_adapter;
pub mod endpoint_identity;
pub mod gateway_adapter;
pub mod model_normalize;
pub mod opencode_adapter;
pub mod pricing_estimate;
pub mod protocol_usage;
pub mod provider_adapter;
pub mod reconcile;

// Re-export observation helpers used by gateway/CLI paths.
pub use endpoint_identity::{classify_provider_path, path_is_admitted, ProviderEndpointKind};
pub use model_normalize::{normalize_codex_model, normalize_for_pricing_lookup};
pub use pricing_estimate::{estimate_cost_usd, LOCAL_PRICING_TABLE_VERSION};
pub use protocol_usage::{
    aggregate_stream_usage, from_anthropic_response, from_openai_compatible_auto, usage_from_body,
    ProtocolTokenUsage,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const EXECUTION_USAGE_EVENT_SCHEMA: &str = "execution_usage_event.v1";
pub const EXECUTION_USAGE_RECONCILE_SCHEMA: &str = "execution_usage_reconcile.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorKind {
    CodexCli,
    ClaudeCodeCli,
    OpenCode,
    ProviderProxy,
    Other,
}

impl ExecutorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CodexCli => "codex_cli",
            Self::ClaudeCodeCli => "claude_code_cli",
            Self::OpenCode => "opencode",
            Self::ProviderProxy => "provider_proxy",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSourceKind {
    CodexJsonlSession,
    ClaudeJsonlSession,
    OpenCodeSqlite,
    ProviderResponse,
    BudgetGateway,
}

impl EvidenceSourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CodexJsonlSession => "codex_jsonl_session",
            Self::ClaudeJsonlSession => "claude_jsonl_session",
            Self::OpenCodeSqlite => "opencode_sqlite",
            Self::ProviderResponse => "provider_response",
            Self::BudgetGateway => "budget_gateway",
        }
    }

    /// Higher is preferred when two sources describe the same call and agree.
    pub fn precedence(self) -> u8 {
        match self {
            Self::BudgetGateway => 50,
            Self::ProviderResponse => 40,
            Self::CodexJsonlSession => 30,
            Self::ClaudeJsonlSession => 30,
            Self::OpenCodeSqlite => 30,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostSource {
    Unavailable,
    Estimated,
    ProviderOrExecutorReported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventCompleteness {
    Complete,
    Partial,
    Ambiguous,
    Conflicting,
}

/// Versioned normalized usage evidence event.
///
/// Never contains prompts, outputs, credentials, authorization headers, private
/// paths, or raw session bodies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionUsageEventV1 {
    pub schema_version: String,
    pub event_id: String,
    pub product_task_id: Option<String>,
    pub workflow_node_id: Option<String>,
    pub managed_execution_id: Option<String>,
    pub executor_kind: ExecutorKind,
    pub evidence_source_kind: EvidenceSourceKind,
    pub provider_id: Option<String>,
    pub requested_model: Option<String>,
    pub resolved_model: Option<String>,
    pub executable_path_fingerprint: Option<String>,
    pub executable_version: Option<String>,
    pub executable_sha256: Option<String>,
    pub root_session_id: Option<String>,
    pub parent_session_id: Option<String>,
    pub request_or_message_id: Option<String>,
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_creation_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub cumulative_task_tokens: Option<u64>,
    pub provider_reported_cost: Option<f64>,
    pub locally_estimated_cost: Option<f64>,
    pub cost_source: CostSource,
    pub pricing_table_version: Option<String>,
    pub timestamp: String,
    pub event_completeness: EventCompleteness,
    pub source_schema_version: String,
    pub stable_dedupe_identity: String,
    /// Bounded non-sensitive provenance tags (schema ids, line indexes, etc.).
    pub provenance_refs: Vec<String>,
}

impl ExecutionUsageEventV1 {
    pub fn billable_token_total(&self) -> u64 {
        self.input_tokens
            .saturating_add(self.cached_input_tokens)
            .saturating_add(self.cache_creation_tokens)
            .saturating_add(self.output_tokens)
            .saturating_add(self.reasoning_output_tokens)
    }

    pub fn token_signature(&self) -> String {
        format!(
            "i{}:c{}:w{}:o{}:r{}",
            self.input_tokens,
            self.cached_input_tokens,
            self.cache_creation_tokens,
            self.output_tokens,
            self.reasoning_output_tokens
        )
    }
}

/// Two-axis capability classification for an executor usage source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutorUsageCapability {
    pub executor_kind: ExecutorKind,
    pub exact_post_call_usage_evidence: bool,
    pub enforceable_pre_or_cross_call_budget: bool,
    pub cost_precision: String,
    pub source_provenance: String,
    pub remaining_admission_blocker: String,
}

pub fn codex_usage_capability() -> ExecutorUsageCapability {
    // Log-only JSONL evidence remains non-enforceable. Product-managed
    // API-key-mediated Codex (gateway + bwrap) is classified separately via
    // `cli::codex_mediation_admission::CodexMediatedCapabilityReport`.
    ExecutorUsageCapability {
        executor_kind: ExecutorKind::CodexCli,
        exact_post_call_usage_evidence: true,
        enforceable_pre_or_cross_call_budget: false,
        cost_precision: "unavailable_or_gateway_only".into(),
        source_provenance: "codex_jsonl_token_count_0.145.0".into(),
        remaining_admission_blocker:
            "JSONL alone is not a hard cross-call gate; product path requires loopback gateway + bwrap FS isolation (see codex_mediated_admission.v1)".into(),
    }
}

/// Capability when the product-managed mediated path is available (gateway + bwrap).
///
/// Usage-evidence closure does **not** clear residual admission blockers
/// (retry identity, loopback-only network, live credential authorization).
pub fn codex_mediated_usage_capability(bwrap_available: bool) -> ExecutorUsageCapability {
    if bwrap_available {
        ExecutorUsageCapability {
            executor_kind: ExecutorKind::CodexCli,
            exact_post_call_usage_evidence: true,
            enforceable_pre_or_cross_call_budget: true,
            cost_precision: "gateway_measured_tokens_cost_unavailable_or_estimated".into(),
            source_provenance: "codex_budget_gateway_plus_jsonl_corroboration".into(),
            remaining_admission_blocker:
                "mediation_hardened_partial: Codex internal retries not wire-labeled; loopback-only network isolation unproved; live credential+authorization required; usage evidence closed via execution_usage_event.v1"
                    .into(),
        }
    } else {
        ExecutorUsageCapability {
            executor_kind: ExecutorKind::CodexCli,
            exact_post_call_usage_evidence: true,
            enforceable_pre_or_cross_call_budget: false,
            cost_precision: "unavailable_or_gateway_only".into(),
            source_provenance: "codex_jsonl_token_count_0.145.0".into(),
            remaining_admission_blocker:
                "product-managed Codex mediation requires /usr/bin/bwrap filesystem isolation"
                    .into(),
        }
    }
}

pub fn claude_usage_capability() -> ExecutorUsageCapability {
    ExecutorUsageCapability {
        executor_kind: ExecutorKind::ClaudeCodeCli,
        exact_post_call_usage_evidence: true,
        enforceable_pre_or_cross_call_budget: false,
        cost_precision: "estimated_from_local_price_table_only".into(),
        source_provenance: "claude_code_jsonl_assistant_usage".into(),
        remaining_admission_blocker:
            "provider-independent worktree-only filesystem confinement unproved".into(),
    }
}

pub fn opencode_usage_capability() -> ExecutorUsageCapability {
    ExecutorUsageCapability {
        executor_kind: ExecutorKind::OpenCode,
        exact_post_call_usage_evidence: true,
        enforceable_pre_or_cross_call_budget: false,
        cost_precision: "executor_reported_when_present_else_estimated".into(),
        source_provenance: "opencode_sqlite_message_tokens_readonly".into(),
        remaining_admission_blocker:
            "real binary admission requires artifact/checksum/supply-chain/confinement".into(),
    }
}

/// Stable evidence event id (not a secret; no private paths).
pub fn stable_usage_event_id(
    source: EvidenceSourceKind,
    root_session: &str,
    request_or_message_id: &str,
    token_signature: &str,
    timestamp: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source.as_str().as_bytes());
    hasher.update(b"|");
    hasher.update(root_session.as_bytes());
    hasher.update(b"|");
    hasher.update(request_or_message_id.as_bytes());
    hasher.update(b"|");
    hasher.update(token_signature.as_bytes());
    hasher.update(b"|");
    hasher.update(timestamp.as_bytes());
    format!("eue-{}", &hex::encode(hasher.finalize())[..24])
}

pub fn path_content_fingerprint(path: &std::path::Path) -> String {
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("unknown");
    let bytes = std::fs::read(path).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(name.as_bytes());
    hasher.update(b"\0");
    hasher.update((bytes.len() as u64).to_le_bytes());
    if !bytes.is_empty() {
        hasher.update(Sha256::digest(&bytes));
    }
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_separate_evidence_from_enforcement() {
        assert!(codex_usage_capability().exact_post_call_usage_evidence);
        assert!(!codex_usage_capability().enforceable_pre_or_cross_call_budget);
        assert!(codex_mediated_usage_capability(true).enforceable_pre_or_cross_call_budget);
        assert!(!codex_mediated_usage_capability(false).enforceable_pre_or_cross_call_budget);
        assert!(claude_usage_capability().exact_post_call_usage_evidence);
        assert!(!claude_usage_capability().enforceable_pre_or_cross_call_budget);
        assert!(opencode_usage_capability().exact_post_call_usage_evidence);
        assert!(!opencode_usage_capability().enforceable_pre_or_cross_call_budget);
    }

    #[test]
    fn stable_ids_are_deterministic() {
        let a = stable_usage_event_id(
            EvidenceSourceKind::ClaudeJsonlSession,
            "sess",
            "msg",
            "i1:c0:w0:o2:r0",
            "ts",
        );
        let b = stable_usage_event_id(
            EvidenceSourceKind::ClaudeJsonlSession,
            "sess",
            "msg",
            "i1:c0:w0:o2:r0",
            "ts",
        );
        assert_eq!(a, b);
        assert!(a.starts_with("eue-"));
    }
}
