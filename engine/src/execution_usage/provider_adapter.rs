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
    let request_id = response
        .provider_request_id
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| binding.managed_execution_id.clone())
        .ok_or_else(|| "provider response missing request/execution identity".to_string())?;
    let token_sig = format!("i{input}:c0:w0:o{output}:r0");
    let root = binding
        .managed_execution_id
        .clone()
        .unwrap_or_else(|| "unbound".into());
    let event_id = stable_usage_event_id(
        EvidenceSourceKind::ProviderResponse,
        &root,
        &request_id,
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
        request_or_message_id: Some(request_id),
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
        event_completeness: EventCompleteness::Complete,
        source_schema_version: PROVIDER_RESPONSE_SOURCE_SCHEMA.to_string(),
        stable_dedupe_identity: event_id,
        provenance_refs: vec![format!("provider:{}", response.provider_id)],
    })
}

/// Extract usage fields from common provider JSON bodies without storing content.
pub fn usage_from_openai_compatible_body(body: &serde_json::Value) -> Option<(u64, u64)> {
    let usage = body.get("usage")?;
    let input = usage
        .get("prompt_tokens")
        .or_else(|| usage.get("input_tokens"))
        .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|n| n.max(0) as u64)))?;
    let output = usage
        .get("completion_tokens")
        .or_else(|| usage.get("output_tokens"))
        .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|n| n.max(0) as u64)))?;
    Some((input, output))
}

pub fn usage_from_anthropic_body(body: &serde_json::Value) -> Option<(u64, u64, u64, u64)> {
    let usage = body.get("usage")?;
    let input = usage
        .get("input_tokens")
        .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|n| n.max(0) as u64)))?;
    let output = usage
        .get("output_tokens")
        .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|n| n.max(0) as u64)))?;
    let cache_read = usage
        .get("cache_read_input_tokens")
        .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|n| n.max(0) as u64)))
        .unwrap_or(0);
    let cache_create = usage
        .get("cache_creation_input_tokens")
        .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|n| n.max(0) as u64)))
        .unwrap_or(0);
    Some((input, output, cache_read, cache_create))
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
}
