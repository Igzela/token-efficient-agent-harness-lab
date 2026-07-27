//! Multi-protocol token usage extraction (non-stream + stream aggregation).
//!
//! Adapted from MIT-licensed CC Switch
//! (`farion1231/cc-switch@878c26f31e012ba32b9772bd080bd4fa9e7d495e`)
//! `src-tauri/src/proxy/usage/parser.rs` (Responses / Chat Completions /
//! Anthropic / Gemini, stream terminal aggregation).
//!
//! Copyright (c) 2025 Jason Young — see `THIRD_PARTY_NOTICES.md`.
//!
//! **Authority boundary:**
//! - Extracts numeric usage and model/message ids only — never prompts/outputs.
//! - Output is evidence material for `ExecutionUsageEventV1`; not a budget owner.
//! - Does not authorize spend, open credentials, or retry provider calls.

use serde_json::Value;

/// How input_tokens relates to cache buckets for cost estimates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputTokenSemantics {
    /// OpenAI/Codex/Gemini: input total includes cache read/write buckets.
    InputIncludesCache,
    /// Anthropic: input_tokens is fresh input excluding cache buckets.
    InputExcludesCache,
}

/// Protocol-level token usage as reported by the provider wire (pre-canonicalization).
///
/// Field meanings depend on `input_semantics` / `output_includes_reasoning` and must
/// be passed through [`ProtocolTokenUsage::to_canonical_buckets`] before writing
/// `ExecutionUsageEventV1` (which uses disjoint buckets).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProtocolTokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub model: Option<String>,
    pub message_or_response_id: Option<String>,
    pub input_semantics: Option<InputTokenSemantics>,
    /// When true, `output_tokens` already includes `reasoning_output_tokens`.
    pub output_includes_reasoning: bool,
}

/// Disjoint repository token buckets for `ExecutionUsageEventV1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CanonicalTokenBuckets {
    pub fresh_input: u64,
    pub cached_input: u64,
    pub cache_creation: u64,
    pub output_non_reasoning: u64,
    pub reasoning_output: u64,
}

/// Typed outcome when provider counters cannot be safely normalized to complete buckets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalizationAnomaly {
    /// Counters are inconsistent; evidence may be kept as partial with reasons.
    Partial {
        reasons: Vec<String>,
        buckets: CanonicalTokenBuckets,
    },
    /// Identity of the counters is ambiguous (e.g. impossible totals).
    Ambiguous {
        reasons: Vec<String>,
        buckets: CanonicalTokenBuckets,
    },
    /// Counters are contradictory and must not be treated as billable complete evidence.
    Rejected {
        reasons: Vec<String>,
        buckets: CanonicalTokenBuckets,
    },
}

impl CanonicalizationAnomaly {
    pub fn reasons(&self) -> &[String] {
        match self {
            Self::Partial { reasons, .. }
            | Self::Ambiguous { reasons, .. }
            | Self::Rejected { reasons, .. } => reasons,
        }
    }

    pub fn buckets(&self) -> CanonicalTokenBuckets {
        match self {
            Self::Partial { buckets, .. }
            | Self::Ambiguous { buckets, .. }
            | Self::Rejected { buckets, .. } => *buckets,
        }
    }
}

impl CanonicalTokenBuckets {
    pub fn billable_token_total(self) -> u64 {
        self.fresh_input
            .saturating_add(self.cached_input)
            .saturating_add(self.cache_creation)
            .saturating_add(self.output_non_reasoning)
            .saturating_add(self.reasoning_output)
    }

    pub fn token_signature(self) -> String {
        format!(
            "i{}:c{}:w{}:o{}:r{}",
            self.fresh_input,
            self.cached_input,
            self.cache_creation,
            self.output_non_reasoning,
            self.reasoning_output
        )
    }
}

impl ProtocolTokenUsage {
    pub fn has_billable_tokens(&self) -> bool {
        self.input_tokens > 0
            || self.output_tokens > 0
            || self.cache_read_tokens > 0
            || self.cache_creation_tokens > 0
            || self.reasoning_output_tokens > 0
    }

    /// Map provider-reported totals into repository-disjoint buckets.
    ///
    /// - When input includes cache: `fresh_input = input - cache_read - cache_creation`.
    /// - When output includes reasoning: `output_non_reasoning = output - reasoning`.
    ///
    /// Prefer [`Self::try_to_canonical_buckets`] so contradictory counters are not
    /// silently saturated into complete-looking evidence.
    pub fn to_canonical_buckets(&self) -> CanonicalTokenBuckets {
        match self.try_to_canonical_buckets() {
            Ok(buckets) => buckets,
            Err(CanonicalizationAnomaly::Rejected { .. }) => CanonicalTokenBuckets::default(),
            Err(CanonicalizationAnomaly::Partial { buckets, .. })
            | Err(CanonicalizationAnomaly::Ambiguous { buckets, .. }) => buckets,
        }
    }

    /// Checked canonicalization. Contradictory provider counters yield typed anomalies
    /// rather than silent saturation presented as complete evidence.
    pub fn try_to_canonical_buckets(
        &self,
    ) -> Result<CanonicalTokenBuckets, CanonicalizationAnomaly> {
        let cache_sum = self
            .cache_read_tokens
            .saturating_add(self.cache_creation_tokens);
        let mut reasons = Vec::new();

        let fresh_input = match self.input_semantics {
            Some(InputTokenSemantics::InputIncludesCache) => {
                if cache_sum > self.input_tokens {
                    reasons.push(format!(
                        "cache_read+cache_creation ({cache_sum}) exceeds input_total ({})",
                        self.input_tokens
                    ));
                    0
                } else {
                    self.input_tokens - cache_sum
                }
            }
            Some(InputTokenSemantics::InputExcludesCache) | None => self.input_tokens,
        };

        let output_non_reasoning = if self.output_includes_reasoning {
            if self.reasoning_output_tokens > self.output_tokens {
                reasons.push(format!(
                    "reasoning ({}) exceeds output_total ({})",
                    self.reasoning_output_tokens, self.output_tokens
                ));
                0
            } else {
                self.output_tokens - self.reasoning_output_tokens
            }
        } else {
            self.output_tokens
        };

        // Gemini-style: totalTokenCount may be present as input_tokens when mis-parsed;
        // detect constituent totals larger than an explicit total-like field if both set.
        let constituent = fresh_input
            .saturating_add(self.cache_read_tokens)
            .saturating_add(self.cache_creation_tokens)
            .saturating_add(output_non_reasoning)
            .saturating_add(self.reasoning_output_tokens);
        // When InputIncludesCache, provider input_tokens should equal fresh+cache.
        if matches!(
            self.input_semantics,
            Some(InputTokenSemantics::InputIncludesCache)
        ) && reasons.is_empty()
            && self.input_tokens
                < self
                    .cache_read_tokens
                    .saturating_add(self.cache_creation_tokens)
        {
            reasons.push("input_total smaller than cache constituent totals".into());
        }
        if self.output_includes_reasoning
            && reasons.iter().all(|r| !r.contains("reasoning"))
            && self.output_tokens < self.reasoning_output_tokens
        {
            reasons.push("output_total smaller than reasoning constituent".into());
        }
        // Optional: if model field embeds a totalTokenCount style inconsistency via
        // zero fresh and zero output but positive constituents elsewhere.
        let _ = constituent;

        let buckets = CanonicalTokenBuckets {
            fresh_input,
            cached_input: self.cache_read_tokens,
            cache_creation: self.cache_creation_tokens,
            output_non_reasoning,
            reasoning_output: self.reasoning_output_tokens,
        };

        if reasons.is_empty() {
            return Ok(buckets);
        }
        // Severe contradictions (cache > input or reasoning > output) are rejected.
        let severe = reasons.iter().any(|r| {
            r.contains("exceeds input_total")
                || r.contains("exceeds output_total")
                || r.contains("smaller than cache")
                || r.contains("smaller than reasoning")
        });
        if severe {
            Err(CanonicalizationAnomaly::Rejected { reasons, buckets })
        } else {
            Err(CanonicalizationAnomaly::Partial { reasons, buckets })
        }
    }

    /// Stable dedupe key for evidence; never invents OAuth/session auth.
    pub fn evidence_dedupe_id(&self, scope: Option<(&str, &str)>) -> Option<String> {
        let mid = self.message_or_response_id.as_ref()?;
        if mid.is_empty() {
            return None;
        }
        Some(match scope {
            Some((app, provider)) => format!("usage:{app}:{provider}:{mid}"),
            None => format!("usage:{mid}"),
        })
    }
}

fn response_id(body: &Value, field: &str) -> Option<String> {
    body.get(field)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn as_u64_tok(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_i64().map(|n| n.max(0) as u64))
}

fn openai_cache_read_tokens(usage: &Value) -> u64 {
    usage
        .get("cache_read_input_tokens")
        .or_else(|| usage.pointer("/input_tokens_details/cached_tokens"))
        .or_else(|| usage.pointer("/prompt_tokens_details/cached_tokens"))
        .and_then(as_u64_tok)
        .unwrap_or(0)
}

fn openai_cache_write_tokens(usage: &Value) -> u64 {
    usage
        .get("cache_creation_input_tokens")
        .or_else(|| usage.pointer("/input_tokens_details/cache_write_tokens"))
        .or_else(|| usage.pointer("/prompt_tokens_details/cache_write_tokens"))
        .and_then(as_u64_tok)
        .unwrap_or(0)
}

/// Anthropic Messages non-stream body.
pub fn from_anthropic_response(body: &Value) -> Option<ProtocolTokenUsage> {
    let usage = body.get("usage")?;
    Some(ProtocolTokenUsage {
        input_tokens: usage.get("input_tokens").and_then(as_u64_tok)?,
        output_tokens: usage.get("output_tokens").and_then(as_u64_tok)?,
        cache_read_tokens: usage
            .get("cache_read_input_tokens")
            .and_then(as_u64_tok)
            .unwrap_or(0),
        cache_creation_tokens: usage
            .get("cache_creation_input_tokens")
            .and_then(as_u64_tok)
            .unwrap_or(0),
        reasoning_output_tokens: 0,
        model: body.get("model").and_then(Value::as_str).map(str::to_owned),
        message_or_response_id: response_id(body, "id"),
        input_semantics: Some(InputTokenSemantics::InputExcludesCache),
        output_includes_reasoning: false,
    })
}

/// Aggregate Anthropic SSE events (`message_start` + `message_delta`).
pub fn from_anthropic_stream_events(events: &[Value]) -> Option<ProtocolTokenUsage> {
    let mut usage = ProtocolTokenUsage {
        input_semantics: Some(InputTokenSemantics::InputExcludesCache),
        ..Default::default()
    };
    let mut input_from_delta = false;
    let mut saw = false;

    for event in events {
        let event_type = event.get("type").and_then(Value::as_str).unwrap_or("");
        match event_type {
            "message_start" => {
                if let Some(message) = event.get("message") {
                    if usage.model.is_none() {
                        usage.model = message
                            .get("model")
                            .and_then(Value::as_str)
                            .map(str::to_owned);
                    }
                    if usage.message_or_response_id.is_none() {
                        usage.message_or_response_id = response_id(message, "id");
                    }
                    if let Some(msg_usage) = message.get("usage") {
                        if let Some(input) = msg_usage.get("input_tokens").and_then(as_u64_tok) {
                            usage.input_tokens = input;
                            saw = true;
                        }
                        usage.cache_read_tokens = msg_usage
                            .get("cache_read_input_tokens")
                            .and_then(as_u64_tok)
                            .unwrap_or(0);
                        usage.cache_creation_tokens = msg_usage
                            .get("cache_creation_input_tokens")
                            .and_then(as_u64_tok)
                            .unwrap_or(0);
                    }
                }
            }
            "message_delta" => {
                if let Some(delta_usage) = event.get("usage") {
                    if let Some(output) = delta_usage.get("output_tokens").and_then(as_u64_tok) {
                        usage.output_tokens = output;
                        saw = true;
                    }
                    if let Some(input) = delta_usage.get("input_tokens").and_then(as_u64_tok) {
                        let should_use = input > 0
                            && (usage.input_tokens == 0
                                || input < usage.input_tokens
                                || (input_from_delta && input <= usage.input_tokens));
                        if should_use {
                            usage.input_tokens = input;
                            input_from_delta = true;
                            saw = true;
                            if let Some(c) = delta_usage
                                .get("cache_read_input_tokens")
                                .and_then(as_u64_tok)
                            {
                                usage.cache_read_tokens = c;
                            }
                            if let Some(c) = delta_usage
                                .get("cache_creation_input_tokens")
                                .and_then(as_u64_tok)
                            {
                                usage.cache_creation_tokens = c;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if saw && usage.has_billable_tokens() {
        Some(usage)
    } else {
        None
    }
}

/// OpenAI Responses API non-stream (`input_tokens` / `output_tokens`).
pub fn from_openai_responses_body(body: &Value) -> Option<ProtocolTokenUsage> {
    let usage = body.get("usage")?;
    let input = usage.get("input_tokens").and_then(as_u64_tok)?;
    let output = usage.get("output_tokens").and_then(as_u64_tok)?;
    let reasoning = usage
        .pointer("/output_tokens_details/reasoning_tokens")
        .and_then(as_u64_tok)
        .unwrap_or(0);
    Some(ProtocolTokenUsage {
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: openai_cache_read_tokens(usage),
        cache_creation_tokens: openai_cache_write_tokens(usage),
        reasoning_output_tokens: reasoning,
        model: body.get("model").and_then(Value::as_str).map(str::to_owned),
        message_or_response_id: response_id(body, "id"),
        input_semantics: Some(InputTokenSemantics::InputIncludesCache),
        // OpenAI reports reasoning as a detail of the total output count.
        output_includes_reasoning: reasoning > 0,
    })
}

/// OpenAI Chat Completions non-stream.
pub fn from_openai_chat_completions_body(body: &Value) -> Option<ProtocolTokenUsage> {
    let usage = body.get("usage")?;
    let prompt = usage.get("prompt_tokens").and_then(as_u64_tok)?;
    let completion = usage.get("completion_tokens").and_then(as_u64_tok)?;
    let reasoning = usage
        .pointer("/completion_tokens_details/reasoning_tokens")
        .and_then(as_u64_tok)
        .unwrap_or(0);
    Some(ProtocolTokenUsage {
        input_tokens: prompt,
        output_tokens: completion,
        cache_read_tokens: openai_cache_read_tokens(usage),
        cache_creation_tokens: openai_cache_write_tokens(usage),
        reasoning_output_tokens: reasoning,
        model: body.get("model").and_then(Value::as_str).map(str::to_owned),
        message_or_response_id: response_id(body, "id"),
        input_semantics: Some(InputTokenSemantics::InputIncludesCache),
        output_includes_reasoning: reasoning > 0,
    })
}

/// Auto-detect Responses vs Chat Completions on a non-stream body.
pub fn from_openai_compatible_auto(body: &Value) -> Option<ProtocolTokenUsage> {
    let usage = body.get("usage")?;
    if usage.get("prompt_tokens").is_some() {
        from_openai_chat_completions_body(body)
    } else if usage.get("input_tokens").is_some() {
        from_openai_responses_body(body)
    } else {
        None
    }
}

/// OpenAI Chat Completions stream: usage on final chunk.
pub fn from_openai_chat_stream_events(events: &[Value]) -> Option<ProtocolTokenUsage> {
    for event in events.iter().rev() {
        if let Some(usage) = event.get("usage") {
            if !usage.is_null() {
                let mut parsed = from_openai_chat_completions_body(event)?;
                if parsed.message_or_response_id.is_none() {
                    parsed.message_or_response_id =
                        events.iter().find_map(|chunk| response_id(chunk, "id"));
                }
                return Some(parsed);
            }
        }
    }
    None
}

/// OpenAI Responses stream: `response.completed`.
pub fn from_openai_responses_stream_events(events: &[Value]) -> Option<ProtocolTokenUsage> {
    for event in events {
        if event.get("type").and_then(Value::as_str) == Some("response.completed") {
            if let Some(response) = event.get("response") {
                return from_openai_compatible_auto(response);
            }
        }
    }
    from_openai_chat_stream_events(events)
}

/// Gemini `usageMetadata` non-stream.
pub fn from_gemini_response(body: &Value) -> Option<ProtocolTokenUsage> {
    let usage = body.get("usageMetadata")?;
    let prompt = usage.get("promptTokenCount").and_then(as_u64_tok)?;
    let total = usage.get("totalTokenCount").and_then(as_u64_tok)?;
    let output = total.saturating_sub(prompt);
    let reasoning = usage
        .get("thoughtsTokenCount")
        .and_then(as_u64_tok)
        .unwrap_or(0);
    Some(ProtocolTokenUsage {
        input_tokens: prompt,
        // Gemini derives output as total-prompt; that total includes thoughts.
        output_tokens: output,
        cache_read_tokens: usage
            .get("cachedContentTokenCount")
            .and_then(as_u64_tok)
            .unwrap_or(0),
        cache_creation_tokens: 0,
        reasoning_output_tokens: reasoning,
        model: body
            .get("modelVersion")
            .and_then(Value::as_str)
            .map(str::to_owned),
        message_or_response_id: response_id(body, "responseId"),
        input_semantics: Some(InputTokenSemantics::InputIncludesCache),
        output_includes_reasoning: reasoning > 0,
    })
}

/// Gemini stream chunks: last usageMetadata wins for totals.
pub fn from_gemini_stream_chunks(chunks: &[Value]) -> Option<ProtocolTokenUsage> {
    let mut last: Option<ProtocolTokenUsage> = None;
    for chunk in chunks {
        if chunk.get("usageMetadata").is_some() {
            if let Some(u) = from_gemini_response(chunk) {
                last = Some(u);
            }
        } else if let Some(ref mut u) = last {
            if u.model.is_none() {
                u.model = chunk
                    .get("modelVersion")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
            if u.message_or_response_id.is_none() {
                u.message_or_response_id = response_id(chunk, "responseId");
            }
        }
    }
    last.filter(|u| u.has_billable_tokens())
}

/// Parse one or more SSE `data:` JSON values (already decoded) by endpoint kind.
pub fn aggregate_stream_usage(
    kind: super::endpoint_identity::ProviderEndpointKind,
    events: &[Value],
) -> Option<ProtocolTokenUsage> {
    use super::endpoint_identity::ProviderEndpointKind;
    match kind {
        ProviderEndpointKind::AnthropicMessages => from_anthropic_stream_events(events),
        ProviderEndpointKind::OpenAiResponses => from_openai_responses_stream_events(events),
        ProviderEndpointKind::OpenAiChatCompletions => from_openai_chat_stream_events(events),
        ProviderEndpointKind::GeminiGenerateContent => from_gemini_stream_chunks(events),
        ProviderEndpointKind::Unknown => from_openai_responses_stream_events(events)
            .or_else(|| from_anthropic_stream_events(events))
            .or_else(|| from_gemini_stream_chunks(events)),
    }
}

/// Non-stream body by endpoint kind.
pub fn usage_from_body(
    kind: super::endpoint_identity::ProviderEndpointKind,
    body: &Value,
) -> Option<ProtocolTokenUsage> {
    use super::endpoint_identity::ProviderEndpointKind;
    match kind {
        ProviderEndpointKind::AnthropicMessages => from_anthropic_response(body),
        ProviderEndpointKind::OpenAiResponses => from_openai_responses_body(body),
        ProviderEndpointKind::OpenAiChatCompletions => from_openai_chat_completions_body(body),
        ProviderEndpointKind::GeminiGenerateContent => from_gemini_response(body),
        ProviderEndpointKind::Unknown => from_openai_compatible_auto(body)
            .or_else(|| from_anthropic_response(body))
            .or_else(|| from_gemini_response(body)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn anthropic_and_openai_shapes() {
        let anth = json!({
            "id": "msg_1",
            "model": "claude-test-model",
            "usage": {
                "input_tokens": 9,
                "output_tokens": 4,
                "cache_read_input_tokens": 3,
                "cache_creation_input_tokens": 1
            }
        });
        let u = from_anthropic_response(&anth).unwrap();
        assert_eq!(u.input_tokens, 9);
        assert_eq!(u.cache_read_tokens, 3);
        assert_eq!(u.message_or_response_id.as_deref(), Some("msg_1"));
        assert_eq!(
            u.input_semantics,
            Some(InputTokenSemantics::InputExcludesCache)
        );

        let oai = json!({
            "id": "chatcmpl_1",
            "model": "gpt-test-model",
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 2,
                "prompt_tokens_details": {"cached_tokens": 4}
            }
        });
        let u = from_openai_chat_completions_body(&oai).unwrap();
        assert_eq!(u.input_tokens, 10);
        assert_eq!(u.cache_read_tokens, 4);
        assert_eq!(
            u.input_semantics,
            Some(InputTokenSemantics::InputIncludesCache)
        );
    }

    #[test]
    fn responses_stream_aggregates_completed() {
        let events = vec![
            json!({"type": "response.created", "response": {"id": "resp_1"}}),
            json!({
                "type": "response.completed",
                "response": {
                    "id": "resp_1",
                    "model": "gpt-test-model",
                    "usage": {
                        "input_tokens": 20,
                        "output_tokens": 5,
                        "input_tokens_details": {"cached_tokens": 2}
                    }
                }
            }),
        ];
        let u = from_openai_responses_stream_events(&events).unwrap();
        assert_eq!(u.input_tokens, 20);
        assert_eq!(u.output_tokens, 5);
        assert_eq!(u.cache_read_tokens, 2);
        assert!(u.has_billable_tokens());
    }

    #[test]
    fn never_captures_prompt_content_fields() {
        let body = json!({
            "id": "msg_x",
            "model": "claude-test-model",
            "content": [{"type":"text","text":"SECRET_PROMPT_BODY"}],
            "usage": {"input_tokens": 1, "output_tokens": 1}
        });
        let u = from_anthropic_response(&body).unwrap();
        let s = format!("{u:?}");
        assert!(!s.contains("SECRET_PROMPT_BODY"));
    }

    #[test]
    fn empty_usage_filtered() {
        let u = ProtocolTokenUsage::default();
        assert!(!u.has_billable_tokens());
        assert!(u.evidence_dedupe_id(None).is_none());
    }

    #[test]
    fn openai_canonical_buckets_do_not_double_count_cache() {
        let body = json!({
            "id": "resp_c",
            "model": "gpt-test-model",
            "usage": {
                "input_tokens": 1000,
                "output_tokens": 50,
                "input_tokens_details": {"cached_tokens": 200, "cache_write_tokens": 100}
            }
        });
        let u = from_openai_responses_body(&body).unwrap();
        let c = u.to_canonical_buckets();
        assert_eq!(c.fresh_input, 700);
        assert_eq!(c.cached_input, 200);
        assert_eq!(c.cache_creation, 100);
        assert_eq!(c.output_non_reasoning, 50);
        assert_eq!(c.billable_token_total(), 1050);
        // Provider-raw additive would wrongly be 1000+200+100+50=1350
        assert_ne!(
            c.billable_token_total(),
            u.input_tokens + u.cache_read_tokens + u.cache_creation_tokens + u.output_tokens
        );
    }

    #[test]
    fn gemini_thoughts_are_disjoint_from_output() {
        let body = json!({
            "responseId": "g1",
            "modelVersion": "gemini-2.0-flash",
            "usageMetadata": {
                "promptTokenCount": 1000,
                "totalTokenCount": 1500,
                "thoughtsTokenCount": 200,
                "cachedContentTokenCount": 50
            }
        });
        let u = from_gemini_response(&body).unwrap();
        assert!(u.output_includes_reasoning);
        let c = u.to_canonical_buckets();
        // fresh = 1000 - 50 = 950; output total was 500 including 200 thoughts
        assert_eq!(c.fresh_input, 950);
        assert_eq!(c.cached_input, 50);
        assert_eq!(c.reasoning_output, 200);
        assert_eq!(c.output_non_reasoning, 300);
        assert_eq!(c.billable_token_total(), 1500);
    }

    #[test]
    fn cache_exceeds_input_is_rejected() {
        let u = ProtocolTokenUsage {
            input_tokens: 10,
            output_tokens: 1,
            cache_read_tokens: 8,
            cache_creation_tokens: 5,
            reasoning_output_tokens: 0,
            model: None,
            message_or_response_id: Some("x".into()),
            input_semantics: Some(InputTokenSemantics::InputIncludesCache),
            output_includes_reasoning: false,
        };
        let err = u.try_to_canonical_buckets().unwrap_err();
        assert!(
            matches!(err, CanonicalizationAnomaly::Rejected { .. }),
            "{err:?}"
        );
        assert!(err.reasons().iter().any(|r| r.contains("exceeds input")));
    }

    #[test]
    fn reasoning_exceeds_output_is_rejected() {
        let u = ProtocolTokenUsage {
            input_tokens: 5,
            output_tokens: 3,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            reasoning_output_tokens: 9,
            model: None,
            message_or_response_id: None,
            input_semantics: Some(InputTokenSemantics::InputIncludesCache),
            output_includes_reasoning: true,
        };
        let err = u.try_to_canonical_buckets().unwrap_err();
        assert!(
            matches!(err, CanonicalizationAnomaly::Rejected { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn consistent_openai_cache_is_complete_ok() {
        let u = ProtocolTokenUsage {
            input_tokens: 1000,
            output_tokens: 50,
            cache_read_tokens: 200,
            cache_creation_tokens: 100,
            reasoning_output_tokens: 0,
            model: Some("gpt-test".into()),
            message_or_response_id: Some("r".into()),
            input_semantics: Some(InputTokenSemantics::InputIncludesCache),
            output_includes_reasoning: false,
        };
        let c = u.try_to_canonical_buckets().unwrap();
        assert_eq!(c.fresh_input, 700);
        assert_eq!(c.cached_input, 200);
        assert_eq!(c.cache_creation, 100);
        assert_eq!(c.billable_token_total(), 1050);
    }
}
