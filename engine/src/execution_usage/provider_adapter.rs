//! Normalize Rust-owned provider/proxy response usage into `ExecutionUsageEventV1`.

use super::codex_adapter::UsageBindingContext;
use super::{
    stable_usage_event_id, CostSource, EventCompleteness, EvidenceSourceKind,
    ExecutionUsageEventV1, ExecutorKind, EXECUTION_USAGE_EVENT_SCHEMA,
};
use crate::provider::ProviderResponse;

pub const PROVIDER_RESPONSE_SOURCE_SCHEMA: &str = "provider_response_usage.v1";

/// Bind a provider response's usage to a managed execution.
///
/// Token counts are owner-reported by the Rust provider boundary. Monetary cost
/// on `ProviderResponse.estimated_cost` is always `Estimated` unless a future
/// verified provider-reported cost field is added.
pub fn provider_response_to_usage_event(
    response: &ProviderResponse,
    binding: &UsageBindingContext,
    timestamp: &str,
) -> Result<ExecutionUsageEventV1, String> {
    let input = response
        .input_tokens
        .filter(|n| *n >= 0)
        .map(|n| n as u64)
        .ok_or_else(|| "provider response missing input_tokens".to_string())?;
    let output = response
        .output_tokens
        .filter(|n| *n >= 0)
        .map(|n| n as u64)
        .ok_or_else(|| "provider response missing output_tokens".to_string())?;
    // Same body → owner → ordinal → ambiguous policy as protocol path.
    // Never fall back to managed_execution_id as an exact request identity.
    let (request_id, completeness) =
        resolve_request_identity(response.provider_request_id.as_deref(), binding);
    let token_sig = format!("i{input}:c0:w0:o{output}:r0");
    let root = binding
        .managed_execution_id
        .clone()
        .unwrap_or_else(|| "unbound".into());
    let dedupe_request = request_id
        .clone()
        .unwrap_or_else(|| format!("ambiguous:{token_sig}:{timestamp}"));
    let event_id = stable_usage_event_id(
        EvidenceSourceKind::ProviderResponse,
        &root,
        &dedupe_request,
        &token_sig,
        timestamp,
    );
    let (locally_estimated_cost, cost_source) = match response.estimated_cost {
        Some(c) if c.is_finite() && c >= 0.0 => (Some(c), CostSource::Estimated),
        _ => (None, CostSource::Unavailable),
    };
    Ok(ExecutionUsageEventV1 {
        schema_version: EXECUTION_USAGE_EVENT_SCHEMA.to_string(),
        event_id: event_id.clone(),
        product_task_id: binding.product_task_id.clone(),
        workflow_node_id: binding.workflow_node_id.clone(),
        managed_execution_id: binding.managed_execution_id.clone(),
        executor_kind: ExecutorKind::ProviderProxy,
        evidence_source_kind: EvidenceSourceKind::ProviderResponse,
        provider_id: Some(response.provider_id.clone()),
        requested_model: binding.requested_model.clone(),
        resolved_model: Some(response.model.clone()),
        executable_path_fingerprint: binding.executable_path_fingerprint.clone(),
        executable_version: binding.executable_version.clone(),
        executable_sha256: binding.executable_sha256.clone(),
        root_session_id: binding.managed_execution_id.clone(),
        parent_session_id: None,
        request_or_message_id: request_id,
        input_tokens: input,
        cached_input_tokens: 0,
        cache_creation_tokens: 0,
        output_tokens: output,
        reasoning_output_tokens: 0,
        cumulative_task_tokens: None,
        provider_reported_cost: None,
        locally_estimated_cost,
        cost_source,
        pricing_table_version: None,
        timestamp: timestamp.to_string(),
        event_completeness: completeness,
        source_schema_version: PROVIDER_RESPONSE_SOURCE_SCHEMA.to_string(),
        stable_dedupe_identity: event_id,
        provenance_refs: vec![format!("provider:{}", response.provider_id)],
    })
}

/// Extract usage fields from common provider JSON bodies without storing content.
pub fn usage_from_openai_compatible_body(body: &serde_json::Value) -> Option<(u64, u64)> {
    let parsed = super::protocol_usage::from_openai_compatible_auto(body)?;
    Some((parsed.input_tokens, parsed.output_tokens))
}

pub fn usage_from_anthropic_body(body: &serde_json::Value) -> Option<(u64, u64, u64, u64)> {
    let parsed = super::protocol_usage::from_anthropic_response(body)?;
    Some((
        parsed.input_tokens,
        parsed.output_tokens,
        parsed.cache_read_tokens,
        parsed.cache_creation_tokens,
    ))
}

/// Resolve request identity without collapsing distinct calls onto managed_execution_id.
///
/// Prefer body id, then owner-supplied exact id, then owner ordinal (partial).
/// If none, return Ambiguous with no request_or_message_id (dedupe uses token+timestamp).
fn resolve_request_identity(
    body_id: Option<&str>,
    binding: &UsageBindingContext,
) -> (Option<String>, EventCompleteness) {
    if let Some(id) = body_id.filter(|s| !s.is_empty()) {
        return (Some(id.to_string()), EventCompleteness::Complete);
    }
    if let Some(id) = binding
        .request_or_message_id
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        return (Some(id.to_string()), EventCompleteness::Complete);
    }
    if let Some(ord) = binding.request_ordinal {
        return (Some(format!("ordinal:{ord}")), EventCompleteness::Partial);
    }
    (None, EventCompleteness::Ambiguous)
}

/// Full protocol usage (+ optional local estimate) for gateway/provider evidence.
///
/// Local pricing never upgrades `cost_source` to provider-reported.
/// Token fields on the event are **canonical disjoint buckets**.
pub fn protocol_body_to_usage_event(
    body: &serde_json::Value,
    path: &str,
    binding: &UsageBindingContext,
    timestamp: &str,
    apply_local_estimate: bool,
) -> Result<ExecutionUsageEventV1, String> {
    use super::endpoint_identity::classify_provider_path;
    use super::pricing_estimate::{estimate_cost_usd, LOCAL_PRICING_TABLE_VERSION};
    use super::protocol_usage::usage_from_body;

    let kind = classify_provider_path(path);
    let parsed = usage_from_body(kind, body)
        .ok_or_else(|| format!("no usage extractable for path {path}"))?;
    if !parsed.has_billable_tokens() {
        return Err("usage present but all token counters are zero".into());
    }
    let (buckets, token_completeness) = match parsed.try_to_canonical_buckets() {
        Ok(b) => (b, EventCompleteness::Complete),
        Err(super::protocol_usage::CanonicalizationAnomaly::Partial { buckets, reasons }) => {
            if reasons
                .iter()
                .any(|r| r.contains("exceeds") || r.contains("smaller"))
            {
                return Err(format!(
                    "provider token counters contradictory (partial): {}",
                    reasons.join("; ")
                ));
            }
            (buckets, EventCompleteness::Partial)
        }
        Err(super::protocol_usage::CanonicalizationAnomaly::Ambiguous { reasons, .. }) => {
            return Err(format!(
                "provider token counters ambiguous: {}",
                reasons.join("; ")
            ));
        }
        Err(super::protocol_usage::CanonicalizationAnomaly::Rejected { reasons, .. }) => {
            return Err(format!(
                "provider token counters rejected: {}",
                reasons.join("; ")
            ));
        }
    };
    let (request_id, mut completeness) =
        resolve_request_identity(parsed.message_or_response_id.as_deref(), binding);
    if token_completeness != EventCompleteness::Complete
        && completeness == EventCompleteness::Complete
    {
        completeness = token_completeness;
    }
    // Provider identity: only exact owner-supplied id; never invent placeholders.
    let provider_id = binding
        .provider_id
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if provider_id.is_none() && completeness == EventCompleteness::Complete {
        completeness = EventCompleteness::Partial;
    }
    let token_sig = buckets.token_signature();
    let root = binding
        .managed_execution_id
        .clone()
        .unwrap_or_else(|| "unbound".into());
    // Dedupe key: never use managed_execution_id alone as request identity.
    let dedupe_request = request_id
        .clone()
        .unwrap_or_else(|| format!("ambiguous:{token_sig}:{timestamp}"));
    let event_id = stable_usage_event_id(
        EvidenceSourceKind::ProviderResponse,
        &root,
        &dedupe_request,
        &token_sig,
        timestamp,
    );
    let resolved = parsed
        .model
        .clone()
        .or_else(|| binding.requested_model.clone());
    // Estimates use provider-raw ProtocolTokenUsage (with semantics), not double-counted buckets.
    let (locally_estimated_cost, cost_source, pricing_table_version) = if apply_local_estimate {
        if let Some(model) = resolved.as_deref() {
            if let Some(est) = estimate_cost_usd(&parsed, model) {
                (
                    Some(est.total_cost_usd),
                    CostSource::Estimated,
                    Some(LOCAL_PRICING_TABLE_VERSION.to_string()),
                )
            } else {
                (None, CostSource::Unavailable, None)
            }
        } else {
            (None, CostSource::Unavailable, None)
        }
    } else {
        (None, CostSource::Unavailable, None)
    };
    Ok(ExecutionUsageEventV1 {
        schema_version: EXECUTION_USAGE_EVENT_SCHEMA.to_string(),
        event_id: event_id.clone(),
        product_task_id: binding.product_task_id.clone(),
        workflow_node_id: binding.workflow_node_id.clone(),
        managed_execution_id: binding.managed_execution_id.clone(),
        executor_kind: ExecutorKind::ProviderProxy,
        evidence_source_kind: EvidenceSourceKind::ProviderResponse,
        provider_id,
        requested_model: binding.requested_model.clone(),
        resolved_model: resolved,
        executable_path_fingerprint: binding.executable_path_fingerprint.clone(),
        executable_version: binding.executable_version.clone(),
        executable_sha256: binding.executable_sha256.clone(),
        root_session_id: binding.managed_execution_id.clone(),
        parent_session_id: None,
        request_or_message_id: request_id,
        input_tokens: buckets.fresh_input,
        cached_input_tokens: buckets.cached_input,
        cache_creation_tokens: buckets.cache_creation,
        output_tokens: buckets.output_non_reasoning,
        reasoning_output_tokens: buckets.reasoning_output,
        cumulative_task_tokens: None,
        provider_reported_cost: None,
        locally_estimated_cost,
        cost_source,
        pricing_table_version,
        timestamp: timestamp.to_string(),
        event_completeness: completeness,
        source_schema_version: PROVIDER_RESPONSE_SOURCE_SCHEMA.to_string(),
        stable_dedupe_identity: event_id,
        provenance_refs: vec![
            format!("path:{path}"),
            format!("endpoint:{}", kind.as_str()),
            format!("canonical_billable:{}", buckets.billable_token_total()),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ProviderResponse;

    #[test]
    fn maps_provider_response_and_marks_cost_estimated() {
        let response = ProviderResponse {
            schema_version: "provider_response.v1".into(),
            provider_id: "openai".into(),
            model: "gpt-test".into(),
            output: "redacted".into(),
            input_tokens: Some(11),
            output_tokens: Some(3),
            estimated_cost: Some(0.002),
            provider_request_id: Some("req_1".into()),
        };
        let event = provider_response_to_usage_event(
            &response,
            &UsageBindingContext {
                managed_execution_id: Some("exec".into()),
                product_task_id: Some("pt".into()),
                ..UsageBindingContext::default()
            },
            "ts",
        )
        .unwrap();
        assert_eq!(event.input_tokens, 11);
        assert_eq!(event.output_tokens, 3);
        assert_eq!(event.cost_source, CostSource::Estimated);
        assert_eq!(event.provider_reported_cost, None);
        assert_eq!(event.locally_estimated_cost, Some(0.002));
    }

    #[test]
    fn missing_tokens_fail_closed() {
        let response = ProviderResponse {
            schema_version: "provider_response.v1".into(),
            provider_id: "openai".into(),
            model: "gpt-test".into(),
            output: "x".into(),
            input_tokens: None,
            output_tokens: Some(1),
            estimated_cost: None,
            provider_request_id: Some("r".into()),
        };
        assert!(provider_response_to_usage_event(
            &response,
            &UsageBindingContext {
                managed_execution_id: Some("e".into()),
                ..UsageBindingContext::default()
            },
            "t"
        )
        .is_err());
    }

    #[test]
    fn parses_openai_and_anthropic_bodies() {
        let oai = serde_json::json!({"usage":{"prompt_tokens":5,"completion_tokens":2}});
        assert_eq!(usage_from_openai_compatible_body(&oai), Some((5, 2)));
        let anth = serde_json::json!({
            "usage":{
                "input_tokens":9,
                "output_tokens":4,
                "cache_read_input_tokens":3,
                "cache_creation_input_tokens":1
            }
        });
        assert_eq!(usage_from_anthropic_body(&anth), Some((9, 4, 3, 1)));
    }

    #[test]
    fn protocol_body_estimate_never_provider_reported() {
        let body = serde_json::json!({
            "id": "chatcmpl_x",
            "model": "gpt-test-model",
            "usage": {"prompt_tokens": 1000, "completion_tokens": 10}
        });
        let event = protocol_body_to_usage_event(
            &body,
            "/v1/chat/completions",
            &UsageBindingContext {
                managed_execution_id: Some("exec-1".into()),
                provider_id: Some("openai".into()),
                requested_model: Some("gpt-test-model".into()),
                ..UsageBindingContext::default()
            },
            "ts",
            true,
        )
        .unwrap();
        assert_eq!(event.cost_source, CostSource::Estimated);
        assert!(event.locally_estimated_cost.is_some());
        assert!(event.provider_reported_cost.is_none());
        assert_eq!(event.provider_id.as_deref(), Some("openai"));
        assert_eq!(event.cached_input_tokens, 0);
        assert_eq!(event.input_tokens, 1000);
        assert_eq!(event.billable_token_total(), 1010);
        // No content fields survived.
        assert!(!format!("{event:?}").contains("messages"));
    }

    #[test]
    fn protocol_body_without_estimate_is_cost_unavailable() {
        let body = serde_json::json!({
            "id": "chatcmpl_y",
            "model": "unknown-xyz",
            "usage": {"prompt_tokens": 3, "completion_tokens": 1}
        });
        let event = protocol_body_to_usage_event(
            &body,
            "/v1/chat/completions",
            &UsageBindingContext {
                managed_execution_id: Some("exec-2".into()),
                provider_id: Some("openai".into()),
                ..UsageBindingContext::default()
            },
            "ts",
            true,
        )
        .unwrap();
        assert_eq!(event.cost_source, CostSource::Unavailable);
        assert!(event.locally_estimated_cost.is_none());
    }

    #[test]
    fn never_invents_bound_provider_placeholder() {
        let body = serde_json::json!({
            "id": "chatcmpl_z",
            "model": "gpt-test-model",
            "usage": {"prompt_tokens": 1, "completion_tokens": 1}
        });
        let event = protocol_body_to_usage_event(
            &body,
            "/v1/chat/completions",
            &UsageBindingContext {
                managed_execution_id: Some("exec".into()),
                requested_model: Some("gpt-test-model".into()),
                ..UsageBindingContext::default()
            },
            "ts",
            false,
        )
        .unwrap();
        assert!(event.provider_id.is_none());
        assert_eq!(event.event_completeness, EventCompleteness::Partial);
        assert_ne!(event.provider_id.as_deref(), Some("bound_provider"));
    }

    #[test]
    fn does_not_collapse_request_id_to_managed_execution() {
        let body = serde_json::json!({
            "model": "gpt-test-model",
            "usage": {"prompt_tokens": 5, "completion_tokens": 1}
            // no id
        });
        let event = protocol_body_to_usage_event(
            &body,
            "/v1/chat/completions",
            &UsageBindingContext {
                managed_execution_id: Some("exec-shared".into()),
                provider_id: Some("openai".into()),
                request_ordinal: Some(2),
                ..UsageBindingContext::default()
            },
            "ts",
            false,
        )
        .unwrap();
        assert_eq!(event.request_or_message_id.as_deref(), Some("ordinal:2"));
        assert_ne!(event.request_or_message_id.as_deref(), Some("exec-shared"));
        assert_eq!(event.event_completeness, EventCompleteness::Partial);

        let ambiguous = protocol_body_to_usage_event(
            &body,
            "/v1/chat/completions",
            &UsageBindingContext {
                managed_execution_id: Some("exec-shared".into()),
                provider_id: Some("openai".into()),
                ..UsageBindingContext::default()
            },
            "ts-a",
            false,
        )
        .unwrap();
        assert!(ambiguous.request_or_message_id.is_none());
        assert_eq!(ambiguous.event_completeness, EventCompleteness::Ambiguous);
        // Distinct timestamps must not share a request_or_message_id of managed_execution_id
        assert_ne!(
            ambiguous.request_or_message_id.as_deref(),
            Some("exec-shared")
        );
    }

    #[test]
    fn openai_with_cache_canonical_billable_matches_disjoint_sum() {
        let body = serde_json::json!({
            "id": "resp_1",
            "model": "gpt-test-model",
            "usage": {
                "input_tokens": 1000,
                "output_tokens": 50,
                "input_tokens_details": {"cached_tokens": 200, "cache_write_tokens": 100}
            }
        });
        let event = protocol_body_to_usage_event(
            &body,
            "/v1/responses",
            &UsageBindingContext {
                managed_execution_id: Some("e".into()),
                provider_id: Some("openai".into()),
                ..UsageBindingContext::default()
            },
            "ts",
            true,
        )
        .unwrap();
        assert_eq!(event.input_tokens, 700);
        assert_eq!(event.cached_input_tokens, 200);
        assert_eq!(event.cache_creation_tokens, 100);
        assert_eq!(event.output_tokens, 50);
        assert_eq!(event.billable_token_total(), 1050);
        assert_eq!(event.event_completeness, EventCompleteness::Complete);
    }

    #[test]
    fn provider_response_never_uses_managed_execution_as_request_id() {
        let response = ProviderResponse {
            schema_version: "provider_response.v1".into(),
            provider_id: "openai".into(),
            model: "gpt-test".into(),
            output: "x".into(),
            input_tokens: Some(2),
            output_tokens: Some(1),
            estimated_cost: None,
            provider_request_id: None,
        };
        let event = provider_response_to_usage_event(
            &response,
            &UsageBindingContext {
                managed_execution_id: Some("exec-shared".into()),
                request_ordinal: Some(7),
                ..UsageBindingContext::default()
            },
            "ts",
        )
        .unwrap();
        assert_eq!(event.request_or_message_id.as_deref(), Some("ordinal:7"));
        assert_ne!(event.request_or_message_id.as_deref(), Some("exec-shared"));
        assert_eq!(event.event_completeness, EventCompleteness::Partial);

        let ambiguous = provider_response_to_usage_event(
            &response,
            &UsageBindingContext {
                managed_execution_id: Some("exec-shared".into()),
                ..UsageBindingContext::default()
            },
            "ts2",
        )
        .unwrap();
        assert!(ambiguous.request_or_message_id.is_none());
        assert_eq!(ambiguous.event_completeness, EventCompleteness::Ambiguous);
    }

    #[test]
    fn protocol_rejects_malformed_cache_exceeding_input() {
        let body = serde_json::json!({
            "id": "resp_bad",
            "model": "gpt-test-model",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 1,
                "input_tokens_details": {"cached_tokens": 20, "cache_write_tokens": 5}
            }
        });
        let err = protocol_body_to_usage_event(
            &body,
            "/v1/responses",
            &UsageBindingContext {
                managed_execution_id: Some("e".into()),
                provider_id: Some("openai".into()),
                ..UsageBindingContext::default()
            },
            "ts",
            false,
        )
        .unwrap_err();
        assert!(
            err.contains("rejected") || err.contains("contradict"),
            "{err}"
        );
    }
}
