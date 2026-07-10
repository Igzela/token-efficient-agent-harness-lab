use serde_json::{json, Map, Value};

use crate::orchestration::schemas::AgentState;
use crate::provider::redaction::{contains_sensitive_patterns, redact_sensitive_patterns};

const MEMORY_DIGEST_KEY: &str = "memory_digest";
const MAX_MEMORY_SUMMARY_BYTES: usize = 1024;
const MAX_MEMORY_SOURCE_REFS: usize = 16;
const MAX_MEMORY_SOURCE_REF_BYTES: usize = 128;

pub fn load_memory_digest_from_agent_state(state: &AgentState) -> Option<Value> {
    state
        .metadata
        .get(MEMORY_DIGEST_KEY)
        .and_then(|value| normalize_memory_digest_for_agent(value, &state.run_id, &state.agent_id))
        .or_else(|| scratchpad_digest(state))
}

pub fn normalize_memory_digest_for_agent(
    value: &Value,
    run_id: &str,
    agent_id: &str,
) -> Option<Value> {
    let mut digest = normalize_memory_digest(value)?;
    let expected_source_ref = format!("agent_state:{run_id}:{agent_id}:scratchpad_summary");
    if let Some(source_refs) = digest.get_mut("source_refs").and_then(Value::as_array_mut) {
        source_refs.retain(|source_ref| {
            source_ref
                .as_str()
                .is_some_and(|source_ref| source_ref == expected_source_ref)
        });
    }
    let has_source_refs = digest
        .get("source_refs")
        .and_then(Value::as_array)
        .is_some_and(|refs| !refs.is_empty());
    let has_summary = digest
        .get("summary")
        .and_then(Value::as_str)
        .is_some_and(|summary| !summary.is_empty());
    (has_source_refs || has_summary).then_some(digest)
}

pub fn normalize_memory_digest(value: &Value) -> Option<Value> {
    let obj = value.as_object()?;

    let source_refs = obj
        .get("source_refs")
        .and_then(Value::as_array)
        .map(|refs| {
            refs.iter()
                .filter_map(Value::as_str)
                .filter(|source_ref| is_safe_source_ref(source_ref))
                .take(MAX_MEMORY_SOURCE_REFS)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut normalized = Map::new();
    normalized.insert("source_refs".to_string(), json!(source_refs));
    normalized.insert("expiry_policy".to_string(), json!("on_prune"));
    normalized.insert(
        "conflict_resolution".to_string(),
        json!("latest_summary_wins"),
    );

    let mut has_summary = false;
    if let Some(summary) = obj.get("summary").and_then(Value::as_str) {
        let summary = sanitize_memory_text(summary);
        if !summary.is_empty() {
            normalized.insert("summary".to_string(), json!(summary));
            has_summary = true;
        }
    }
    if let Some(updated_at) = obj.get("updated_at").and_then(Value::as_str) {
        if is_safe_timestamp(updated_at) {
            normalized.insert("updated_at".to_string(), json!(cap_text(updated_at, 64)));
        }
    }
    if let Some(count) = obj.get("mailbox_pending_count").and_then(Value::as_i64) {
        normalized.insert("mailbox_pending_count".to_string(), json!(count.max(0)));
    }

    if normalized
        .get("source_refs")
        .and_then(Value::as_array)
        .map_or(true, Vec::is_empty)
        && !has_summary
    {
        return None;
    }

    Some(Value::Object(normalized))
}

pub fn consolidate_memory_digest(
    state: &AgentState,
    action_result_summary: Option<&str>,
    mailbox_pending_count: i64,
) -> Option<Value> {
    let prior = load_memory_digest_from_agent_state(state);
    let summary = action_result_summary
        .or(state.scratchpad_summary.as_deref())
        .map(sanitize_memory_text)
        .filter(|text| !text.is_empty())
        .or_else(|| {
            prior
                .as_ref()
                .and_then(|digest| digest.get("summary"))
                .and_then(Value::as_str)
                .map(str::to_string)
        });

    if summary.is_none() && prior.is_none() {
        return None;
    }

    let mut source_refs = prior
        .as_ref()
        .and_then(|digest| digest.get("source_refs"))
        .and_then(Value::as_array)
        .map(|refs| {
            refs.iter()
                .filter_map(Value::as_str)
                .filter(|source_ref| is_safe_source_ref(source_ref))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let scratchpad_ref = scratchpad_source_ref(state);
    if !source_refs
        .iter()
        .any(|source_ref| source_ref == &scratchpad_ref)
    {
        source_refs.push(scratchpad_ref);
    }
    source_refs.truncate(MAX_MEMORY_SOURCE_REFS);

    let mut digest = Map::new();
    digest.insert("source_refs".to_string(), json!(source_refs));
    digest.insert("expiry_policy".to_string(), json!("on_prune"));
    digest.insert(
        "conflict_resolution".to_string(),
        json!("latest_summary_wins"),
    );
    if let Some(summary) = summary {
        digest.insert("summary".to_string(), json!(summary));
    }
    digest.insert(
        "mailbox_pending_count".to_string(),
        json!(mailbox_pending_count.max(0)),
    );
    digest.insert(
        "updated_at".to_string(),
        json!(cap_text(&state.updated_at, 64)),
    );

    Some(Value::Object(digest))
}

pub fn memory_digest_to_metadata_patch(digest: &Value) -> Value {
    json!({ MEMORY_DIGEST_KEY: digest })
}

pub fn build_memory_context_for_node(state: &AgentState, budget: usize) -> Option<Value> {
    let digest = load_memory_digest_from_agent_state(state)?;
    let max_tokens = budget.max(1);
    let summary = digest.get("summary").and_then(Value::as_str).unwrap_or("");
    let estimated_tokens = estimate_tokens(summary);
    let included_tokens = estimated_tokens.min(max_tokens);
    let truncated = estimated_tokens > included_tokens;

    let mut bounded_digest = digest.clone();
    if truncated {
        if let Some(obj) = bounded_digest.as_object_mut() {
            obj.insert(
                "summary".to_string(),
                json!(truncate_to_tokens(summary, included_tokens)),
            );
        }
    }

    Some(json!({
        "schema_version": "agent_memory_context.v1",
        "source": "agent_state.metadata.memory_digest",
        "injection_surface": "node_metadata_only",
        "max_context_tokens": max_tokens,
        "estimated_tokens": estimated_tokens,
        "included_tokens": included_tokens,
        "truncated": truncated,
        "memory_digest": bounded_digest,
        "context_layers": {
            "memory_digest": bounded_digest,
            "freshness": "current",
            "cache_policy": "no_cache",
            "pack_prune_policy": "preserve_invariants"
        }
    }))
}

pub fn estimate_memory_state_bytes(digest: Option<&Value>, context: Option<&Value>) -> i64 {
    let persisted_digest = digest.or_else(|| context.and_then(|value| value.get("memory_digest")));
    let source = persisted_digest.or(context);
    source
        .and_then(|value| serde_json::to_vec(value).ok())
        .map(|bytes| bytes.len())
        .unwrap_or(0) as i64
}

fn scratchpad_digest(state: &AgentState) -> Option<Value> {
    let summary = sanitize_memory_text(state.scratchpad_summary.as_deref()?);
    if summary.is_empty() {
        return None;
    }
    Some(json!({
        "source_refs": [scratchpad_source_ref(state)],
        "expiry_policy": "on_prune",
        "conflict_resolution": "latest_summary_wins",
        "summary": summary,
        "updated_at": cap_text(&state.updated_at, 64),
    }))
}

fn scratchpad_source_ref(state: &AgentState) -> String {
    format!(
        "agent_state:{}:{}:scratchpad_summary",
        state.run_id, state.agent_id
    )
}

fn sanitize_memory_text(text: &str) -> String {
    cap_text(
        &redact_private_paths(&redact_loose_secret_shapes(&redact_sensitive_patterns(
            text,
        ))),
        MAX_MEMORY_SUMMARY_BYTES,
    )
}

fn redact_private_paths(text: &str) -> String {
    let path_re = regex::Regex::new(
        r"(?x)
        (?P<unix>/(?:home|Users|root|tmp|var|etc|workspace)/[^\s,;]+)
        |
        (?P<windows>[A-Za-z]:\\[^\s,;]+)
        ",
    )
    .expect("valid path redaction regex");
    path_re.replace_all(text, "[redacted-path]").to_string()
}

fn redact_loose_secret_shapes(text: &str) -> String {
    let secret_re =
        regex::Regex::new(r"(?i)\bsk-[A-Za-z0-9_\-]{8,}\b").expect("valid secret regex");
    secret_re.replace_all(text, "***").to_string()
}

fn cap_text(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut split = max_bytes;
    while split > 0 && !text.is_char_boundary(split) {
        split -= 1;
    }
    format!("{} [truncated]", &text[..split])
}

fn is_safe_source_ref(source_ref: &str) -> bool {
    !source_ref.is_empty()
        && source_ref.len() <= MAX_MEMORY_SOURCE_REF_BYTES
        && !source_ref.contains('/')
        && !source_ref.contains('\\')
        && !contains_sensitive_patterns(source_ref)
        && source_ref
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | ':' | '.'))
}

fn is_safe_timestamp(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, '-' | ':' | 'T' | 'Z' | '.' | '+'))
}

fn estimate_tokens(text: &str) -> usize {
    (text.chars().count() / 4).max(1)
}

fn truncate_to_tokens(text: &str, max_tokens: usize) -> String {
    text.chars().take(max_tokens.saturating_mul(4)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    fn state_with(scratchpad_summary: Option<&str>, memory_digest: Option<Value>) -> AgentState {
        let mut metadata = HashMap::new();
        if let Some(digest) = memory_digest {
            metadata.insert("memory_digest".to_string(), digest);
        }
        AgentState {
            schema_version: "agent_state.v1".to_string(),
            agent_id: "agent-1".to_string(),
            run_id: "run-1".to_string(),
            role: "implementer".to_string(),
            capability_profile: vec!["code".to_string()],
            objective: Some("raw objective must not be copied".to_string()),
            status: "busy".to_string(),
            scratchpad_summary: scratchpad_summary.map(str::to_string),
            redaction_filter: None,
            metadata,
            created_at: "2026-07-08T00:00:00Z".to_string(),
            updated_at: "2026-07-08T00:01:00Z".to_string(),
        }
    }

    #[test]
    fn normalizes_digest_with_bounds_redaction_and_safe_source_refs() {
        let digest = normalize_memory_digest(&json!({
            "source_refs": [
                "agent_state:run-1:agent-1:scratchpad_summary",
                "/home/igzela/private/repo/src/lib.rs"
            ],
            "expiry_policy": "forever",
            "conflict_resolution": "append_raw",
            "updated_at": "token: sk-proj-should-not-leak",
            "summary": format!("{} sk-proj-secret-token", "safe progress ".repeat(800))
        }))
        .expect("digest should normalize");

        assert_eq!(digest["expiry_policy"], "on_prune");
        assert_eq!(digest["conflict_resolution"], "latest_summary_wins");
        assert_eq!(
            digest["source_refs"],
            json!(["agent_state:run-1:agent-1:scratchpad_summary"])
        );
        let text = digest["summary"].as_str().unwrap();
        assert!(!text.contains("sk-proj-secret-token"));
        assert!(!text.contains("/home/igzela"));
        assert!(text.len() <= 1024 + " [truncated]".len());
        assert!(digest.get("updated_at").is_none());
    }

    #[test]
    fn load_digest_falls_back_to_scratchpad_when_metadata_is_invalid() {
        let state = state_with(
            Some("Working on memory policy with token sk-test-secret"),
            Some(json!("not-a-digest")),
        );

        let digest = load_memory_digest_from_agent_state(&state).expect("fallback digest");
        assert_eq!(
            digest["source_refs"],
            json!(["agent_state:run-1:agent-1:scratchpad_summary"])
        );
        assert!(!digest["summary"]
            .as_str()
            .unwrap()
            .contains("sk-test-secret"));
    }

    #[test]
    fn load_digest_keeps_only_current_run_non_secret_source_refs() {
        let state = state_with(
            None,
            Some(json!({
                "source_refs": [
                    "agent_state:run-1:agent-1:scratchpad_summary",
                    "agent_state:other-run:agent-1:scratchpad_summary",
                    "sk-abcdefghijklmnopqrstuvwxyz"
                ],
                "summary": "bounded summary"
            })),
        );

        let digest = load_memory_digest_from_agent_state(&state).expect("memory digest");
        assert_eq!(
            digest["source_refs"],
            json!(["agent_state:run-1:agent-1:scratchpad_summary"])
        );
    }

    #[test]
    fn normalizes_harness_key_workspace_path_and_exact_agent_source_ref() {
        let harness_key = format!("harness_{}", "b".repeat(64));
        let state = state_with(
            None,
            Some(json!({
                "source_refs": [
                    "agent_state:run-1:agent-1:scratchpad_summary",
                    "agent_state:run-1:agent-2:scratchpad_summary",
                    "agent_state:run-1:agent-1:forged_suffix"
                ],
                "summary": format!("key={harness_key} file=/workspace/private/src/lib.rs")
            })),
        );

        let digest = load_memory_digest_from_agent_state(&state).expect("memory digest");
        assert_eq!(
            digest["source_refs"],
            json!(["agent_state:run-1:agent-1:scratchpad_summary"])
        );
        assert_eq!(digest["summary"], "key=*** file=[redacted-path]");
    }

    #[test]
    fn consolidate_prefers_action_summary_and_keeps_metadata_only() {
        let state = state_with(
            Some("old summary"),
            Some(json!({
                "source_refs": ["agent_state:run-1:agent-1:scratchpad_summary"],
                "expiry_policy": "on_prune",
                "conflict_resolution": "latest_summary_wins",
                "summary": "old summary"
            })),
        );

        let digest = consolidate_memory_digest(
            &state,
            Some("new bounded summary with /home/igzela/private.txt"),
            3,
        )
        .expect("consolidated digest");

        assert_eq!(
            digest["summary"],
            "new bounded summary with [redacted-path]"
        );
        assert_eq!(digest["mailbox_pending_count"], 3);
        assert_eq!(digest["updated_at"], "2026-07-08T00:01:00Z");
    }

    #[test]
    fn metadata_patch_stores_digest_under_memory_digest_key() {
        let digest = json!({
            "source_refs": [],
            "expiry_policy": "on_prune",
            "conflict_resolution": "latest_summary_wins"
        });

        let patch = memory_digest_to_metadata_patch(&digest);
        assert_eq!(patch, json!({"memory_digest": digest}));
    }

    #[test]
    fn memory_context_is_bounded_by_budget() {
        let state = state_with(
            None,
            Some(json!({
                "source_refs": ["agent_state:run-1:agent-1:scratchpad_summary"],
                "expiry_policy": "on_prune",
                "conflict_resolution": "latest_summary_wins",
                "summary": "0123456789abcdef0123456789abcdef"
            })),
        );

        let context = build_memory_context_for_node(&state, 4).expect("memory context");
        assert_eq!(context["injection_surface"], "node_metadata_only");
        assert!(context["included_tokens"].as_i64().unwrap() <= 4);
        assert_eq!(context["truncated"], true);
        assert!(estimate_memory_state_bytes(context.get("memory_digest"), Some(&context)) > 0);
    }

    #[test]
    fn state_read_bytes_count_persisted_digest_once() {
        let state = state_with(
            None,
            Some(json!({
                "source_refs": ["agent_state:run-1:agent-1:scratchpad_summary"],
                "summary": "bounded summary"
            })),
        );
        let digest = load_memory_digest_from_agent_state(&state).unwrap();
        let context = build_memory_context_for_node(&state, 100).unwrap();
        let expected = serde_json::to_vec(&digest).unwrap().len() as i64;

        assert_eq!(
            estimate_memory_state_bytes(Some(&digest), Some(&context)),
            expected
        );
    }
}
