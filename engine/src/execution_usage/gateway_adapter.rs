//! Map Rust-owned `CodexBudgetGateway` measured usage into `ExecutionUsageEventV1`.
//!
//! This is evidence only. It does not reserve, grant, restore, or reject ProductTask
//! budget. Gateway + parent journal remain the pre/cross-call enforcers.

use super::codex_adapter::UsageBindingContext;
use super::{
    stable_usage_event_id, CostSource, EventCompleteness, EvidenceSourceKind,
    ExecutionUsageEventV1, ExecutorKind, EXECUTION_USAGE_EVENT_SCHEMA,
};
use crate::cli::codex_budget_authority::{BudgetGatewayUsage, CodexBudgetAuthority};
use crate::cli::codex_session_usage::SessionUsageRollup;

pub const BUDGET_GATEWAY_SOURCE_SCHEMA: &str = "codex_budget_gateway_usage.v2";
pub const SESSION_ROLLUP_CORROBORATION_SCHEMA: &str = "codex_session_rollup_corroboration.v1";

/// Primary managed-execution measurement from the loopback budget gateway.
///
/// Cost is always unavailable unless a future verified monetary field exists on
/// the gateway (token×price is never provider-reported).
pub fn budget_gateway_usage_to_event(
    usage: &BudgetGatewayUsage,
    authority: &CodexBudgetAuthority,
    binding: &UsageBindingContext,
    timestamp: &str,
) -> ExecutionUsageEventV1 {
    let request_id = authority.execution_id.clone();
    let token_sig = format!(
        "i{}:c0:w0:o{}:r0",
        usage.cumulative_input_tokens, usage.cumulative_output_tokens
    );
    let event_id = stable_usage_event_id(
        EvidenceSourceKind::BudgetGateway,
        &authority.execution_id,
        &request_id,
        &token_sig,
        timestamp,
    );
    let completeness = if usage.journal_halted
        || usage
            .last_reject_class
            .as_deref()
            .is_some_and(|c| c == "outcome_unknown")
    {
        EventCompleteness::Ambiguous
    } else if usage.provider_requests == 0 {
        EventCompleteness::Partial
    } else {
        EventCompleteness::Complete
    };
    ExecutionUsageEventV1 {
        schema_version: EXECUTION_USAGE_EVENT_SCHEMA.to_string(),
        event_id: event_id.clone(),
        product_task_id: binding
            .product_task_id
            .clone()
            .or_else(|| Some(authority.task_id.clone())),
        workflow_node_id: binding
            .workflow_node_id
            .clone()
            .or_else(|| Some(authority.workflow_node_id.clone())),
        managed_execution_id: binding
            .managed_execution_id
            .clone()
            .or_else(|| Some(authority.execution_id.clone())),
        executor_kind: ExecutorKind::CodexCli,
        evidence_source_kind: EvidenceSourceKind::BudgetGateway,
        provider_id: Some(authority.provider.provider_kind.clone()),
        requested_model: binding
            .requested_model
            .clone()
            .or_else(|| Some(authority.model.clone())),
        resolved_model: Some(authority.model.clone()),
        executable_path_fingerprint: binding.executable_path_fingerprint.clone(),
        executable_version: binding
            .executable_version
            .clone()
            .or_else(|| Some(authority.executable.binary_version.clone())),
        executable_sha256: binding
            .executable_sha256
            .clone()
            .or_else(|| Some(authority.executable.binary_sha256.clone())),
        root_session_id: Some(authority.execution_id.clone()),
        parent_session_id: None,
        request_or_message_id: Some(request_id),
        input_tokens: usage.cumulative_input_tokens,
        cached_input_tokens: 0,
        cache_creation_tokens: 0,
        output_tokens: usage.cumulative_output_tokens,
        reasoning_output_tokens: 0,
        cumulative_task_tokens: Some(usage.cumulative_tokens),
        provider_reported_cost: None,
        locally_estimated_cost: None,
        cost_source: CostSource::Unavailable,
        pricing_table_version: None,
        timestamp: timestamp.to_string(),
        event_completeness: completeness,
        source_schema_version: BUDGET_GATEWAY_SOURCE_SCHEMA.to_string(),
        stable_dedupe_identity: event_id,
        provenance_refs: vec![
            format!("provider_host:{}", authority.provider.host),
            format!("provider_requests:{}", usage.provider_requests),
            format!("observed_retry_posts:{}", usage.observed_retry_posts),
            format!("attempt:{}", authority.execution_id),
        ],
    }
}

/// One corroborating cumulative event from Codex session JSONL rollup.
///
/// Does not grant or restore ProductTask budget. Used only for cross-source
/// reconcile against the gateway measurement.
pub fn session_rollup_to_corroboration_event(
    rollup: &SessionUsageRollup,
    authority: &CodexBudgetAuthority,
    binding: &UsageBindingContext,
    timestamp: &str,
) -> ExecutionUsageEventV1 {
    let request_id = authority.execution_id.clone();
    let token_sig = format!(
        "i{}:c0:w0:o{}:r0",
        rollup.cumulative_input_tokens, rollup.cumulative_output_tokens
    );
    let event_id = stable_usage_event_id(
        EvidenceSourceKind::CodexJsonlSession,
        &authority.execution_id,
        &request_id,
        &token_sig,
        timestamp,
    );
    ExecutionUsageEventV1 {
        schema_version: EXECUTION_USAGE_EVENT_SCHEMA.to_string(),
        event_id: event_id.clone(),
        product_task_id: binding
            .product_task_id
            .clone()
            .or_else(|| Some(authority.task_id.clone())),
        workflow_node_id: binding
            .workflow_node_id
            .clone()
            .or_else(|| Some(authority.workflow_node_id.clone())),
        managed_execution_id: Some(authority.execution_id.clone()),
        executor_kind: ExecutorKind::CodexCli,
        evidence_source_kind: EvidenceSourceKind::CodexJsonlSession,
        provider_id: Some(authority.provider.provider_kind.clone()),
        requested_model: Some(authority.model.clone()),
        resolved_model: Some(authority.model.clone()),
        executable_path_fingerprint: binding.executable_path_fingerprint.clone(),
        executable_version: Some(authority.executable.binary_version.clone()),
        executable_sha256: Some(authority.executable.binary_sha256.clone()),
        root_session_id: Some(rollup.root_thread_id.clone()),
        parent_session_id: None,
        request_or_message_id: Some(request_id),
        input_tokens: rollup.cumulative_input_tokens,
        cached_input_tokens: 0,
        cache_creation_tokens: 0,
        output_tokens: rollup.cumulative_output_tokens,
        reasoning_output_tokens: 0,
        cumulative_task_tokens: Some(
            rollup
                .cumulative_input_tokens
                .saturating_add(rollup.cumulative_output_tokens),
        ),
        provider_reported_cost: None,
        locally_estimated_cost: None,
        cost_source: CostSource::Unavailable,
        pricing_table_version: None,
        timestamp: timestamp.to_string(),
        event_completeness: EventCompleteness::Complete,
        source_schema_version: SESSION_ROLLUP_CORROBORATION_SCHEMA.to_string(),
        stable_dedupe_identity: event_id,
        provenance_refs: vec![
            "role:corroborating_session_rollup".into(),
            format!("events:{}", rollup.events.len()),
            "auth_independent:local_jsonl_only".into(),
        ],
    }
}

/// Build normalized evidence for one mediated attempt: gateway primary + optional
/// session corroboration. Session importers never touch ProductTask budget.
pub fn mediated_codex_usage_evidence_bundle(
    usage: &BudgetGatewayUsage,
    authority: &CodexBudgetAuthority,
    binding: &UsageBindingContext,
    session_rollup: Option<&SessionUsageRollup>,
    timestamp: &str,
) -> Vec<ExecutionUsageEventV1> {
    let mut events = vec![budget_gateway_usage_to_event(
        usage, authority, binding, timestamp,
    )];
    if let Some(rollup) = session_rollup {
        // Only attach corroboration when the rollup observed tokens; missing logs
        // must not invent evidence or weaken gateway enforcement.
        if rollup.cumulative_input_tokens > 0 || rollup.cumulative_output_tokens > 0 {
            events.push(session_rollup_to_corroboration_event(
                rollup, authority, binding, timestamp,
            ));
        }
    }
    events
}

/// Estimated cost helper: token×price is always `Estimated` with a pricing version,
/// never a provider billing receipt. Unknown pricing leaves cost unavailable.
pub fn apply_local_price_estimate(
    event: &mut ExecutionUsageEventV1,
    price_per_million_input: Option<f64>,
    price_per_million_output: Option<f64>,
    pricing_table_version: &str,
) {
    match (price_per_million_input, price_per_million_output) {
        (Some(pi), Some(po)) if pi.is_finite() && po.is_finite() && pi >= 0.0 && po >= 0.0 => {
            let input_billable = event
                .input_tokens
                .saturating_add(event.cached_input_tokens)
                .saturating_add(event.cache_creation_tokens);
            let cost = (input_billable as f64) * pi / 1_000_000.0
                + (event
                    .output_tokens
                    .saturating_add(event.reasoning_output_tokens) as f64)
                    * po
                    / 1_000_000.0;
            event.locally_estimated_cost = Some(cost);
            event.provider_reported_cost = None;
            event.cost_source = CostSource::Estimated;
            event.pricing_table_version = Some(pricing_table_version.to_string());
        }
        _ => {
            // Preserve token evidence; leave cost unresolved.
            event.locally_estimated_cost = None;
            event.provider_reported_cost = None;
            event.cost_source = CostSource::Unavailable;
            event.pricing_table_version = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::codex_budget_authority::{
        new_codex_attempt_id, CodexExecutableIdentity, CodexProviderIdentity,
        ADMITTED_CODEX_CLI_VERSION, CODEX_BUDGET_AUTHORITY_SCHEMA,
    };
    use crate::cli::codex_session_usage::SessionUsageRollup;
    use crate::execution_usage::reconcile::{admission_evidence_ok, reconcile_usage_events};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn sample_authority() -> CodexBudgetAuthority {
        let binary = std::env::temp_dir().join(format!("gw-ev-{}", uuid::Uuid::new_v4()));
        std::fs::write(&binary, b"x").unwrap();
        let sha = {
            use sha2::{Digest, Sha256};
            hex::encode(Sha256::digest(b"x"))
        };
        CodexBudgetAuthority {
            schema_version: CODEX_BUDGET_AUTHORITY_SCHEMA.to_string(),
            task_id: "ptask-1".into(),
            workflow_node_id: "node-1".into(),
            execution_id: new_codex_attempt_id(),
            executable: CodexExecutableIdentity {
                binary_path: binary,
                binary_version: ADMITTED_CODEX_CLI_VERSION.to_string(),
                binary_sha256: sha,
            },
            provider: CodexProviderIdentity::openai_compatible("https://api.openai.com/v1")
                .unwrap(),
            model: "gpt-test".into(),
            max_provider_requests: 4,
            max_retries: 1,
            max_input_tokens_per_request: 10_000,
            max_output_tokens_per_request: 128,
            max_cumulative_tokens: 10_000,
            max_cost_usd: None,
            timeout_ms: 30_000,
            worktree: std::env::temp_dir(),
            expires_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64
                + 60_000,
        }
    }

    #[test]
    fn gateway_event_maps_tokens_cost_unavailable_without_oauth() {
        let authority = sample_authority();
        let usage = BudgetGatewayUsage {
            provider_requests: 2,
            cumulative_input_tokens: 40,
            cumulative_output_tokens: 12,
            cumulative_tokens: 52,
            last_reject: None,
            last_reject_class: None,
            journal_halted: false,
            observed_retry_posts: 1,
        };
        let event = budget_gateway_usage_to_event(
            &usage,
            &authority,
            &UsageBindingContext {
                product_task_id: Some("ptask-1".into()),
                managed_execution_id: Some(authority.execution_id.clone()),
                ..UsageBindingContext::default()
            },
            "ts",
        );
        assert_eq!(
            event.evidence_source_kind,
            EvidenceSourceKind::BudgetGateway
        );
        assert_eq!(event.input_tokens, 40);
        assert_eq!(event.output_tokens, 12);
        assert_eq!(event.cost_source, CostSource::Unavailable);
        assert!(event.provider_reported_cost.is_none());
        assert!(event.provenance_refs.iter().any(|r| r.contains("attempt:")));
        // No OAuth-related fields required for parsing.
        assert!(!event.provenance_refs.iter().any(|r| r.contains("oauth")));
        let _ = std::fs::remove_file(&authority.executable.binary_path);
    }

    #[test]
    fn gateway_and_matching_session_rollup_dedupe_without_double_count() {
        let authority = sample_authority();
        let usage = BudgetGatewayUsage {
            provider_requests: 1,
            cumulative_input_tokens: 20,
            cumulative_output_tokens: 5,
            cumulative_tokens: 25,
            last_reject: None,
            last_reject_class: None,
            journal_halted: false,
            observed_retry_posts: 0,
        };
        let rollup = SessionUsageRollup {
            schema_version: "codex_session_usage_rollup.v1".into(),
            root_thread_id: "thread-root".into(),
            events: vec![],
            cumulative_total_tokens: 25,
            cumulative_input_tokens: 20,
            cumulative_output_tokens: 5,
            cumulative_reasoning_tokens: 0,
            resolved_model: Some("gpt-test".into()),
            deferred_child_threads: vec![],
            ambiguities: vec![],
        };
        let events = mediated_codex_usage_evidence_bundle(
            &usage,
            &authority,
            &UsageBindingContext {
                managed_execution_id: Some(authority.execution_id.clone()),
                product_task_id: Some(authority.task_id.clone()),
                ..UsageBindingContext::default()
            },
            Some(&rollup),
            "ts",
        );
        assert_eq!(events.len(), 2);
        let result = reconcile_usage_events(events);
        assert!(admission_evidence_ok(&result).is_ok(), "{result:?}");
        assert_eq!(result.canonical_events.len(), 1);
        assert_eq!(
            result.canonical_events[0].evidence_source_kind,
            EvidenceSourceKind::BudgetGateway
        );
        assert_eq!(result.suppressed_duplicates.len(), 1);
        // No double-count: single canonical total.
        assert_eq!(result.canonical_events[0].input_tokens, 20);
        assert_eq!(result.canonical_events[0].output_tokens, 5);
        let _ = std::fs::remove_file(&authority.executable.binary_path);
    }

    #[test]
    fn conflicting_gateway_and_session_fail_closed() {
        let authority = sample_authority();
        let usage = BudgetGatewayUsage {
            provider_requests: 1,
            cumulative_input_tokens: 20,
            cumulative_output_tokens: 5,
            cumulative_tokens: 25,
            last_reject: None,
            last_reject_class: None,
            journal_halted: false,
            observed_retry_posts: 0,
        };
        let rollup = SessionUsageRollup {
            schema_version: "codex_session_usage_rollup.v1".into(),
            root_thread_id: "thread-root".into(),
            events: vec![],
            cumulative_total_tokens: 104,
            cumulative_input_tokens: 99,
            cumulative_output_tokens: 5,
            cumulative_reasoning_tokens: 0,
            resolved_model: Some("gpt-test".into()),
            deferred_child_threads: vec![],
            ambiguities: vec![],
        };
        let events = mediated_codex_usage_evidence_bundle(
            &usage,
            &authority,
            &UsageBindingContext {
                managed_execution_id: Some(authority.execution_id.clone()),
                ..UsageBindingContext::default()
            },
            Some(&rollup),
            "ts",
        );
        let result = reconcile_usage_events(events);
        assert!(!result.conflicts.is_empty());
        assert!(admission_evidence_ok(&result).is_err());
        let _ = std::fs::remove_file(&authority.executable.binary_path);
    }

    #[test]
    fn missing_session_does_not_weaken_gateway_evidence() {
        let authority = sample_authority();
        let usage = BudgetGatewayUsage {
            provider_requests: 1,
            cumulative_input_tokens: 15,
            cumulative_output_tokens: 3,
            cumulative_tokens: 18,
            last_reject: None,
            last_reject_class: None,
            journal_halted: false,
            observed_retry_posts: 0,
        };
        let events = mediated_codex_usage_evidence_bundle(
            &usage,
            &authority,
            &UsageBindingContext::default(),
            None,
            "ts",
        );
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].input_tokens, 15);
        let result = reconcile_usage_events(events);
        assert!(admission_evidence_ok(&result).is_ok());
        let _ = std::fs::remove_file(&authority.executable.binary_path);
    }

    #[test]
    fn unknown_pricing_keeps_tokens_cost_unavailable() {
        let authority = sample_authority();
        let usage = BudgetGatewayUsage {
            provider_requests: 1,
            cumulative_input_tokens: 10,
            cumulative_output_tokens: 2,
            cumulative_tokens: 12,
            last_reject: None,
            last_reject_class: None,
            journal_halted: false,
            observed_retry_posts: 0,
        };
        let mut event = budget_gateway_usage_to_event(
            &usage,
            &authority,
            &UsageBindingContext::default(),
            "ts",
        );
        apply_local_price_estimate(&mut event, None, None, "prices-v0");
        assert_eq!(event.input_tokens, 10);
        assert_eq!(event.cost_source, CostSource::Unavailable);
        apply_local_price_estimate(&mut event, Some(1.0), Some(2.0), "prices-v1");
        assert_eq!(event.cost_source, CostSource::Estimated);
        assert!(event.locally_estimated_cost.is_some());
        assert!(event.provider_reported_cost.is_none());
        assert_eq!(event.pricing_table_version.as_deref(), Some("prices-v1"));
        let _ = std::fs::remove_file(&authority.executable.binary_path);
    }

    #[test]
    fn oauth_vs_api_key_shape_identical_for_local_usage_fields() {
        // Parsing does not depend on auth mode — only usage-bearing fields.
        let authority = sample_authority();
        let usage = BudgetGatewayUsage {
            provider_requests: 1,
            cumulative_input_tokens: 7,
            cumulative_output_tokens: 1,
            cumulative_tokens: 8,
            last_reject: None,
            last_reject_class: None,
            journal_halted: false,
            observed_retry_posts: 0,
        };
        let api_key_event = budget_gateway_usage_to_event(
            &usage,
            &authority,
            &UsageBindingContext::default(),
            "ts",
        );
        // Simulate "oauth path" local evidence by mapping the same counters via rollup.
        let rollup = SessionUsageRollup {
            schema_version: "codex_session_usage_rollup.v1".into(),
            root_thread_id: "sess".into(),
            events: vec![],
            cumulative_total_tokens: 8,
            cumulative_input_tokens: 7,
            cumulative_output_tokens: 1,
            cumulative_reasoning_tokens: 0,
            resolved_model: Some("gpt-test".into()),
            deferred_child_threads: vec![],
            ambiguities: vec![],
        };
        let oauth_local = session_rollup_to_corroboration_event(
            &rollup,
            &authority,
            &UsageBindingContext::default(),
            "ts",
        );
        assert_eq!(api_key_event.input_tokens, oauth_local.input_tokens);
        assert_eq!(api_key_event.output_tokens, oauth_local.output_tokens);
        assert_eq!(api_key_event.cost_source, oauth_local.cost_source);
        assert_eq!(api_key_event.schema_version, oauth_local.schema_version);
        let _ = std::fs::remove_file(&authority.executable.binary_path);
    }
}
