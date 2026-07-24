//! Codex JSONL session adapter → `ExecutionUsageEventV1`.

use super::{
    stable_usage_event_id, CostSource, EventCompleteness, EvidenceSourceKind,
    ExecutionUsageEventV1, ExecutorKind, EXECUTION_USAGE_EVENT_SCHEMA,
};
use crate::cli::codex_session_usage::{
    import_managed_codex_home, CodexSessionUsageEvent, VERIFIED_CODEX_SESSION_LOG_VERSION,
};

/// Binding context supplied by the product/runtime owner (not invented by the importer).
#[derive(Debug, Clone, Default)]
pub struct UsageBindingContext {
    pub product_task_id: Option<String>,
    pub workflow_node_id: Option<String>,
    pub managed_execution_id: Option<String>,
    pub requested_model: Option<String>,
    pub executable_path_fingerprint: Option<String>,
    pub executable_version: Option<String>,
    pub executable_sha256: Option<String>,
}

pub fn codex_session_event_to_usage(
    event: &CodexSessionUsageEvent,
    binding: &UsageBindingContext,
) -> ExecutionUsageEventV1 {
    let last = &event.last_token_usage;
    let token_sig = format!(
        "i{}:c{}:w{}:o{}:r{}",
        last.input_tokens,
        last.cached_input_tokens,
        last.cache_write_input_tokens,
        last.output_tokens,
        last.reasoning_output_tokens
    );
    let request_id = format!("line:{}", event.line_index);
    let event_id = stable_usage_event_id(
        EvidenceSourceKind::CodexJsonlSession,
        &event.root_thread_id,
        &request_id,
        &token_sig,
        &event.timestamp,
    );
    let completeness = if event.skipped_as_parent_replay {
        EventCompleteness::Partial
    } else if event.ambiguous {
        EventCompleteness::Ambiguous
    } else {
        EventCompleteness::Complete
    };
    ExecutionUsageEventV1 {
        schema_version: EXECUTION_USAGE_EVENT_SCHEMA.to_string(),
        event_id: event_id.clone(),
        product_task_id: binding.product_task_id.clone(),
        workflow_node_id: binding.workflow_node_id.clone(),
        managed_execution_id: binding.managed_execution_id.clone(),
        executor_kind: ExecutorKind::CodexCli,
        evidence_source_kind: EvidenceSourceKind::CodexJsonlSession,
        provider_id: Some("openai".into()),
        requested_model: binding.requested_model.clone(),
        resolved_model: event.resolved_model.clone(),
        executable_path_fingerprint: binding.executable_path_fingerprint.clone(),
        executable_version: binding
            .executable_version
            .clone()
            .or_else(|| event.cli_version.clone()),
        executable_sha256: binding.executable_sha256.clone(),
        root_session_id: Some(event.root_thread_id.clone()),
        parent_session_id: event.parent_thread_id.clone(),
        request_or_message_id: Some(request_id),
        input_tokens: last.input_tokens,
        cached_input_tokens: last.cached_input_tokens,
        cache_creation_tokens: last.cache_write_input_tokens,
        output_tokens: last.output_tokens,
        reasoning_output_tokens: last.reasoning_output_tokens,
        cumulative_task_tokens: Some(event.total_token_usage.total_tokens),
        provider_reported_cost: None,
        locally_estimated_cost: None,
        cost_source: CostSource::Unavailable,
        pricing_table_version: None,
        timestamp: event.timestamp.clone(),
        event_completeness: completeness,
        source_schema_version: format!("codex_jsonl_{VERIFIED_CODEX_SESSION_LOG_VERSION}"),
        stable_dedupe_identity: event.event_id.clone(),
        provenance_refs: vec![
            format!("source_fp:{}", event.source_path_fingerprint),
            format!("line:{}", event.line_index),
            format!("delta:{}", event.cumulative_delta_total),
        ],
    }
}

pub fn import_codex_home_as_usage_events(
    codex_home: &std::path::Path,
    admitted_root_thread_id: &str,
    binding: &UsageBindingContext,
) -> Result<Vec<ExecutionUsageEventV1>, String> {
    let rollup = import_managed_codex_home(codex_home, admitted_root_thread_id)?;
    Ok(rollup
        .events
        .iter()
        .filter(|e| !e.skipped_as_parent_replay)
        .map(|e| codex_session_event_to_usage(e, binding))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::codex_session_usage::TokenCounters;

    #[test]
    fn maps_codex_last_usage_fields() {
        let event = CodexSessionUsageEvent {
            schema_version: "codex_session_usage_event.v1".into(),
            event_id: "csu-abc".into(),
            root_thread_id: "root".into(),
            parent_thread_id: None,
            source_thread_id: "root".into(),
            line_index: 3,
            timestamp: "t".into(),
            resolved_model: Some("gpt-test".into()),
            cli_version: Some("0.145.0".into()),
            total_token_usage: TokenCounters {
                total_tokens: 100,
                input_tokens: 80,
                output_tokens: 20,
                ..TokenCounters::default()
            },
            last_token_usage: TokenCounters {
                total_tokens: 30,
                input_tokens: 20,
                cached_input_tokens: 5,
                cache_write_input_tokens: 1,
                output_tokens: 4,
                reasoning_output_tokens: 2,
            },
            cumulative_delta_total: 30,
            last_matches_delta: true,
            skipped_as_parent_replay: false,
            ambiguous: false,
            ambiguity_reason: None,
            source_path_fingerprint: "fp".into(),
        };
        let mapped = codex_session_event_to_usage(
            &event,
            &UsageBindingContext {
                product_task_id: Some("pt".into()),
                managed_execution_id: Some("ex".into()),
                ..UsageBindingContext::default()
            },
        );
        assert_eq!(mapped.input_tokens, 20);
        assert_eq!(mapped.cached_input_tokens, 5);
        assert_eq!(mapped.cache_creation_tokens, 1);
        assert_eq!(mapped.output_tokens, 4);
        assert_eq!(mapped.reasoning_output_tokens, 2);
        assert_eq!(mapped.cost_source, CostSource::Unavailable);
        assert_eq!(mapped.executor_kind, ExecutorKind::CodexCli);
    }
}
