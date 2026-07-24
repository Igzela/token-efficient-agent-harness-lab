//! Cross-source usage reconciliation and deterministic deduplication.

use super::{EventCompleteness, ExecutionUsageEventV1, EXECUTION_USAGE_RECONCILE_SCHEMA};

#[derive(Debug, Clone, PartialEq)]
pub struct ReconcileConflict {
    pub left_event_id: String,
    pub right_event_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReconcileResult {
    pub schema_version: String,
    pub canonical_events: Vec<ExecutionUsageEventV1>,
    pub suppressed_duplicates: Vec<String>,
    pub conflicts: Vec<ReconcileConflict>,
}

/// Reconcile multi-source usage events for one managed execution.
///
/// Matching keys (when present): managed_execution_id, request/message id,
/// root session + token signature + model, with a coarse timestamp window.
/// Prefer higher source precedence when counters agree; conflict when they disagree.
pub fn reconcile_usage_events(mut events: Vec<ExecutionUsageEventV1>) -> ReconcileResult {
    events.sort_by(|a, b| {
        a.timestamp
            .cmp(&b.timestamp)
            .then_with(|| a.event_id.cmp(&b.event_id))
    });

    let mut canonical: Vec<ExecutionUsageEventV1> = Vec::new();
    let mut suppressed = Vec::new();
    let mut conflicts = Vec::new();

    'outer: for event in events {
        for existing in &mut canonical {
            if !same_call(existing, &event) {
                continue;
            }
            if counters_agree(existing, &event) {
                // Keep higher-precedence source; mark duplicate.
                if event.evidence_source_kind.precedence()
                    > existing.evidence_source_kind.precedence()
                {
                    suppressed.push(existing.event_id.clone());
                    *existing = event;
                } else {
                    suppressed.push(event.event_id.clone());
                }
                continue 'outer;
            }
            // Contradictory counters: do not merge.
            existing.event_completeness = EventCompleteness::Conflicting;
            conflicts.push(ReconcileConflict {
                left_event_id: existing.event_id.clone(),
                right_event_id: event.event_id.clone(),
                reason: format!(
                    "token_signature_mismatch left={} right={}",
                    existing.token_signature(),
                    event.token_signature()
                ),
            });
            continue 'outer;
        }
        canonical.push(event);
    }

    ReconcileResult {
        schema_version: EXECUTION_USAGE_RECONCILE_SCHEMA.to_string(),
        canonical_events: canonical,
        suppressed_duplicates: suppressed,
        conflicts,
    }
}

fn same_call(a: &ExecutionUsageEventV1, b: &ExecutionUsageEventV1) -> bool {
    if let (Some(x), Some(y)) = (&a.managed_execution_id, &b.managed_execution_id) {
        if x != y {
            return false;
        }
    }
    if let (Some(x), Some(y)) = (&a.request_or_message_id, &b.request_or_message_id) {
        if !x.is_empty() && !y.is_empty() {
            return x == y;
        }
    }
    let model_a = a.resolved_model.as_deref().or(a.requested_model.as_deref());
    let model_b = b.resolved_model.as_deref().or(b.requested_model.as_deref());
    if model_a.is_some() && model_b.is_some() && model_a != model_b {
        return false;
    }
    if let (Some(x), Some(y)) = (&a.root_session_id, &b.root_session_id) {
        if x == y && a.token_signature() == b.token_signature() {
            return true;
        }
    }
    // Same execution + identical token signature within coarse window.
    a.managed_execution_id.is_some()
        && a.managed_execution_id == b.managed_execution_id
        && a.token_signature() == b.token_signature()
}

fn counters_agree(a: &ExecutionUsageEventV1, b: &ExecutionUsageEventV1) -> bool {
    a.token_signature() == b.token_signature()
}

/// Fail closed for admission evidence when conflicts remain.
pub fn admission_evidence_ok(result: &ReconcileResult) -> Result<(), String> {
    if !result.conflicts.is_empty() {
        return Err(format!(
            "usage_evidence_conflict: {} conflicting pair(s)",
            result.conflicts.len()
        ));
    }
    if result
        .canonical_events
        .iter()
        .any(|e| matches!(e.event_completeness, EventCompleteness::Conflicting))
    {
        return Err("usage_evidence_conflict: conflicting completeness".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_usage::{CostSource, EvidenceSourceKind, ExecutorKind};

    fn sample(
        id: &str,
        source: EvidenceSourceKind,
        msg: Option<&str>,
        input: u64,
        output: u64,
    ) -> ExecutionUsageEventV1 {
        ExecutionUsageEventV1 {
            schema_version: super::super::EXECUTION_USAGE_EVENT_SCHEMA.into(),
            event_id: id.into(),
            product_task_id: Some("pt".into()),
            workflow_node_id: Some("n".into()),
            managed_execution_id: Some("exec-1".into()),
            executor_kind: ExecutorKind::CodexCli,
            evidence_source_kind: source,
            provider_id: Some("openai".into()),
            requested_model: Some("m".into()),
            resolved_model: Some("m".into()),
            executable_path_fingerprint: None,
            executable_version: None,
            executable_sha256: None,
            root_session_id: Some("root".into()),
            parent_session_id: None,
            request_or_message_id: msg.map(str::to_string),
            input_tokens: input,
            cached_input_tokens: 0,
            cache_creation_tokens: 0,
            output_tokens: output,
            reasoning_output_tokens: 0,
            cumulative_task_tokens: None,
            provider_reported_cost: None,
            locally_estimated_cost: None,
            cost_source: CostSource::Unavailable,
            pricing_table_version: None,
            timestamp: "t1".into(),
            event_completeness: EventCompleteness::Complete,
            source_schema_version: "test".into(),
            stable_dedupe_identity: id.into(),
            provenance_refs: vec![],
        }
    }

    #[test]
    fn prefers_higher_precedence_duplicate() {
        let result = reconcile_usage_events(vec![
            sample(
                "a",
                EvidenceSourceKind::CodexJsonlSession,
                Some("r1"),
                10,
                2,
            ),
            sample("b", EvidenceSourceKind::BudgetGateway, Some("r1"), 10, 2),
        ]);
        assert_eq!(result.canonical_events.len(), 1);
        assert_eq!(
            result.canonical_events[0].evidence_source_kind,
            EvidenceSourceKind::BudgetGateway
        );
        assert_eq!(result.suppressed_duplicates, vec!["a".to_string()]);
        assert!(admission_evidence_ok(&result).is_ok());
    }

    #[test]
    fn conflicting_counters_fail_closed() {
        let result = reconcile_usage_events(vec![
            sample("a", EvidenceSourceKind::ProviderResponse, Some("r1"), 10, 2),
            sample(
                "b",
                EvidenceSourceKind::CodexJsonlSession,
                Some("r1"),
                11,
                2,
            ),
        ]);
        assert_eq!(result.conflicts.len(), 1);
        assert!(admission_evidence_ok(&result).is_err());
    }
}
