//! Claude Code JSONL session adapter → `ExecutionUsageEventV1`.
//!
//! Verified shape (provider-free, Claude Code 2.1.217 transcripts):
//!
//! ```text
//! type=assistant
//! message.id, message.model, message.stop_reason
//! message.usage.{input_tokens, output_tokens,
//!   cache_read_input_tokens, cache_creation_input_tokens,
//!   cache_creation.ephemeral_5m_input_tokens,
//!   cache_creation.ephemeral_1h_input_tokens}
//! sessionId, agentId?, isSidechain?, uuid, timestamp
//! ```
//!
//! Subagent files under `.../subagents/agent-*.jsonl` carry `agentId` and
//! `isSidechain=true`. Message-ID dedupe prefers completed (`end_turn`) records,
//! else the largest valid cumulative output snapshot.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::codex_adapter::UsageBindingContext;
use super::{
    path_content_fingerprint, stable_usage_event_id, CostSource, EventCompleteness,
    EvidenceSourceKind, ExecutionUsageEventV1, ExecutorKind, EXECUTION_USAGE_EVENT_SCHEMA,
};

pub const CLAUDE_JSONL_SOURCE_SCHEMA: &str = "claude_code_jsonl_assistant_usage.v1";
pub const CLAUDE_CURSOR_SCHEMA: &str = "claude_session_import_cursor.v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeImportCursor {
    pub schema_version: String,
    pub path_fingerprint: String,
    pub byte_offset: u64,
    pub next_line_index: u64,
}

impl ClaudeImportCursor {
    pub fn new(path_fingerprint: String) -> Self {
        Self {
            schema_version: CLAUDE_CURSOR_SCHEMA.to_string(),
            path_fingerprint,
            byte_offset: 0,
            next_line_index: 0,
        }
    }
}

#[derive(Debug, Clone)]
struct RawAssistantUsage {
    message_id: String,
    session_id: String,
    agent_id: Option<String>,
    is_sidechain: bool,
    model: Option<String>,
    stop_reason: Option<String>,
    timestamp: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_input_tokens: u64,
    cache_creation_input_tokens: u64,
    line_index: u64,
    path_fingerprint: String,
}

fn u64_field(value: &Value, key: &str) -> u64 {
    value
        .get(key)
        .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|n| n.max(0) as u64)))
        .unwrap_or(0)
}

fn parse_assistant_line(
    value: &Value,
    line_index: u64,
    path_fingerprint: &str,
) -> Option<RawAssistantUsage> {
    if value.get("type").and_then(Value::as_str) != Some("assistant") {
        return None;
    }
    let message = value.get("message")?;
    let usage = message.get("usage")?;
    let message_id = message
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())?
        .to_string();
    let session_id = value
        .get("sessionId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())?
        .to_string();
    let mut cache_creation = u64_field(usage, "cache_creation_input_tokens");
    if let Some(cc) = usage.get("cache_creation") {
        cache_creation = cache_creation
            .saturating_add(u64_field(cc, "ephemeral_5m_input_tokens"))
            .saturating_add(u64_field(cc, "ephemeral_1h_input_tokens"));
    }
    Some(RawAssistantUsage {
        message_id,
        session_id,
        agent_id: value
            .get("agentId")
            .and_then(Value::as_str)
            .map(str::to_string),
        is_sidechain: value
            .get("isSidechain")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        model: message
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_string),
        stop_reason: message
            .get("stop_reason")
            .and_then(Value::as_str)
            .map(str::to_string),
        timestamp: value
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        input_tokens: u64_field(usage, "input_tokens"),
        output_tokens: u64_field(usage, "output_tokens"),
        cache_read_input_tokens: u64_field(usage, "cache_read_input_tokens"),
        cache_creation_input_tokens: cache_creation,
        line_index,
        path_fingerprint: path_fingerprint.to_string(),
    })
}

fn is_completed(stop_reason: &Option<String>) -> bool {
    matches!(
        stop_reason.as_deref(),
        Some("end_turn") | Some("stop_sequence") | Some("max_tokens")
    )
}

/// Prefer completed message records; otherwise keep largest cumulative output.
fn prefer_message(existing: &RawAssistantUsage, candidate: &RawAssistantUsage) -> bool {
    let e_done = is_completed(&existing.stop_reason);
    let c_done = is_completed(&candidate.stop_reason);
    if c_done && !e_done {
        return true;
    }
    if e_done && !c_done {
        return false;
    }
    candidate.output_tokens > existing.output_tokens
        || (candidate.output_tokens == existing.output_tokens
            && candidate
                .input_tokens
                .saturating_add(candidate.cache_read_input_tokens)
                > existing
                    .input_tokens
                    .saturating_add(existing.cache_read_input_tokens))
}

fn raw_to_event(raw: &RawAssistantUsage, binding: &UsageBindingContext) -> ExecutionUsageEventV1 {
    let token_sig = format!(
        "i{}:c{}:w{}:o{}:r0",
        raw.input_tokens,
        raw.cache_read_input_tokens,
        raw.cache_creation_input_tokens,
        raw.output_tokens
    );
    let event_id = stable_usage_event_id(
        EvidenceSourceKind::ClaudeJsonlSession,
        &raw.session_id,
        &raw.message_id,
        &token_sig,
        &raw.timestamp,
    );
    let completeness = if is_completed(&raw.stop_reason) {
        EventCompleteness::Complete
    } else {
        // Still billable (e.g. tool_use mid-turn); mark partial, do not drop.
        EventCompleteness::Partial
    };
    let parent = if raw.is_sidechain || raw.agent_id.is_some() {
        // Sidechain/subagent: sessionId is still the root session in observed files.
        None
    } else {
        None
    };
    ExecutionUsageEventV1 {
        schema_version: EXECUTION_USAGE_EVENT_SCHEMA.to_string(),
        event_id: event_id.clone(),
        product_task_id: binding.product_task_id.clone(),
        workflow_node_id: binding.workflow_node_id.clone(),
        managed_execution_id: binding.managed_execution_id.clone(),
        executor_kind: ExecutorKind::ClaudeCodeCli,
        evidence_source_kind: EvidenceSourceKind::ClaudeJsonlSession,
        provider_id: None, // may be third-party; do not invent anthropic
        requested_model: binding.requested_model.clone(),
        resolved_model: raw.model.clone(),
        executable_path_fingerprint: binding.executable_path_fingerprint.clone(),
        executable_version: binding.executable_version.clone(),
        executable_sha256: binding.executable_sha256.clone(),
        root_session_id: Some(raw.session_id.clone()),
        parent_session_id: parent.or_else(|| raw.agent_id.clone()),
        request_or_message_id: Some(raw.message_id.clone()),
        input_tokens: raw.input_tokens,
        cached_input_tokens: raw.cache_read_input_tokens,
        cache_creation_tokens: raw.cache_creation_input_tokens,
        output_tokens: raw.output_tokens,
        reasoning_output_tokens: 0,
        cumulative_task_tokens: None,
        provider_reported_cost: None,
        locally_estimated_cost: None,
        cost_source: CostSource::Unavailable, // local price table applied by owner if any
        pricing_table_version: None,
        timestamp: raw.timestamp.clone(),
        event_completeness: completeness,
        source_schema_version: CLAUDE_JSONL_SOURCE_SCHEMA.to_string(),
        stable_dedupe_identity: event_id,
        provenance_refs: vec![
            format!("source_fp:{}", raw.path_fingerprint),
            format!("line:{}", raw.line_index),
            format!("stop:{}", raw.stop_reason.as_deref().unwrap_or("none")),
        ],
    }
}

/// Incremental import from one Claude JSONL file with message-id dedupe.
pub fn import_claude_jsonl(
    path: &Path,
    admitted_session_id: Option<&str>,
    cursor: &mut ClaudeImportCursor,
    binding: &UsageBindingContext,
) -> Result<Vec<ExecutionUsageEventV1>, String> {
    let fingerprint = path_content_fingerprint(path);
    if cursor.path_fingerprint != fingerprint && cursor.byte_offset != 0 {
        *cursor = ClaudeImportCursor::new(fingerprint.clone());
    }
    cursor.path_fingerprint = fingerprint.clone();

    let mut file = File::open(path).map_err(|e| format!("claude session open failed: {e}"))?;
    file.seek(SeekFrom::Start(cursor.byte_offset))
        .map_err(|e| format!("claude session seek failed: {e}"))?;
    let mut reader = BufReader::new(file);
    let mut by_message: HashMap<String, RawAssistantUsage> = HashMap::new();
    let mut line_buf = String::new();
    let mut absolute_line = cursor.next_line_index;

    loop {
        line_buf.clear();
        let n = reader
            .read_line(&mut line_buf)
            .map_err(|e| format!("claude session read failed: {e}"))?;
        if n == 0 {
            break;
        }
        cursor.byte_offset = cursor.byte_offset.saturating_add(n as u64);
        let line_index = absolute_line;
        absolute_line = absolute_line.saturating_add(1);
        cursor.next_line_index = absolute_line;
        let trimmed = line_buf.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(trimmed)
            .map_err(|_| format!("malformed Claude JSONL at line {line_index}"))?;
        let Some(raw) = parse_assistant_line(&value, line_index, &fingerprint) else {
            continue;
        };
        if let Some(admitted) = admitted_session_id {
            if raw.session_id != admitted {
                return Err("claude session identity is not bound to admitted session".into());
            }
        }
        by_message
            .entry(raw.message_id.clone())
            .and_modify(|existing| {
                if prefer_message(existing, &raw) {
                    *existing = raw.clone();
                }
            })
            .or_insert(raw);
    }

    let mut events: Vec<_> = by_message
        .into_values()
        .map(|raw| raw_to_event(&raw, binding))
        .collect();
    events.sort_by(|a, b| {
        a.timestamp
            .cmp(&b.timestamp)
            .then(a.event_id.cmp(&b.event_id))
    });
    Ok(events)
}

/// Discover main + subagent JSONL under a Claude project directory.
pub fn discover_claude_session_files(project_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    visit(project_dir, &mut out)?;
    out.sort();
    Ok(out)
}

fn visit(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            visit(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_jsonl(path: &Path, lines: &[Value]) {
        let mut f = File::create(path).unwrap();
        for line in lines {
            writeln!(f, "{}", serde_json::to_string(line).unwrap()).unwrap();
        }
    }

    fn assistant(
        session: &str,
        msg_id: &str,
        model: &str,
        stop: Option<&str>,
        input: u64,
        output: u64,
        cache_read: u64,
        cache_create: u64,
        agent: Option<&str>,
    ) -> Value {
        let mut v = serde_json::json!({
            "type": "assistant",
            "uuid": "u1",
            "timestamp": "2026-07-24T00:00:00.000Z",
            "sessionId": session,
            "isSidechain": agent.is_some(),
            "message": {
                "id": msg_id,
                "model": model,
                "role": "assistant",
                "type": "message",
                "stop_reason": stop,
                "usage": {
                    "input_tokens": input,
                    "output_tokens": output,
                    "cache_read_input_tokens": cache_read,
                    "cache_creation_input_tokens": cache_create,
                    "cache_creation": {
                        "ephemeral_5m_input_tokens": 0,
                        "ephemeral_1h_input_tokens": 0
                    }
                }
            }
        });
        if let Some(agent) = agent {
            v["agentId"] = serde_json::json!(agent);
        }
        v
    }

    #[test]
    fn prefers_completed_over_partial_same_message() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sess.jsonl");
        write_jsonl(
            &path,
            &[
                assistant("s1", "m1", "claude-x", Some("tool_use"), 10, 5, 0, 0, None),
                assistant("s1", "m1", "claude-x", Some("end_turn"), 12, 8, 2, 1, None),
            ],
        );
        let mut cursor = ClaudeImportCursor::new(path_content_fingerprint(&path));
        let events = import_claude_jsonl(
            &path,
            Some("s1"),
            &mut cursor,
            &UsageBindingContext::default(),
        )
        .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].output_tokens, 8);
        assert_eq!(events[0].cached_input_tokens, 2);
        assert_eq!(events[0].cache_creation_tokens, 1);
        assert_eq!(events[0].event_completeness, EventCompleteness::Complete);
    }

    #[test]
    fn keeps_partial_tool_use_as_billable_partial() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sess.jsonl");
        write_jsonl(
            &path,
            &[assistant(
                "s1",
                "m2",
                "claude-x",
                Some("tool_use"),
                100,
                20,
                0,
                0,
                None,
            )],
        );
        let mut cursor = ClaudeImportCursor::new(path_content_fingerprint(&path));
        let events = import_claude_jsonl(
            &path,
            Some("s1"),
            &mut cursor,
            &UsageBindingContext::default(),
        )
        .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_completeness, EventCompleteness::Partial);
        assert_eq!(events[0].cost_source, CostSource::Unavailable);
    }

    #[test]
    fn incremental_cursor_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sess.jsonl");
        write_jsonl(
            &path,
            &[assistant(
                "s1",
                "m1",
                "claude-x",
                Some("end_turn"),
                1,
                1,
                0,
                0,
                None,
            )],
        );
        let mut cursor = ClaudeImportCursor::new(path_content_fingerprint(&path));
        let first = import_claude_jsonl(
            &path,
            Some("s1"),
            &mut cursor,
            &UsageBindingContext::default(),
        )
        .unwrap();
        assert_eq!(first.len(), 1);
        let second = import_claude_jsonl(
            &path,
            Some("s1"),
            &mut cursor,
            &UsageBindingContext::default(),
        )
        .unwrap();
        assert!(second.is_empty());
    }

    #[test]
    fn rejects_unrelated_session() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sess.jsonl");
        write_jsonl(
            &path,
            &[assistant(
                "other",
                "m1",
                "claude-x",
                Some("end_turn"),
                1,
                1,
                0,
                0,
                None,
            )],
        );
        let mut cursor = ClaudeImportCursor::new(path_content_fingerprint(&path));
        let err = import_claude_jsonl(
            &path,
            Some("s1"),
            &mut cursor,
            &UsageBindingContext::default(),
        )
        .unwrap_err();
        assert!(err.contains("not bound"));
    }
}
