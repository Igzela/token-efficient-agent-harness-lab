//! Rust-owned Codex session/rollout usage importer.
//!
//! Verified against installed Codex CLI **0.145.0** local rollout files under
//! `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` (and archived copies).
//!
//! Observed event shapes (provider-free inspection; no private content stored):
//!
//! ```text
//! session_meta.payload: session_id, id, forked_from_id?, parent_thread_id?,
//!                       cli_version, model_provider, source.subagent.thread_spawn?
//! turn_context.payload: turn_id, model, collaboration_mode.settings.model?
//! event_msg.payload where type=token_count:
//!   info.total_token_usage.{input,cached_input,cache_write_input,output,
//!                           reasoning_output,total}_tokens
//!   info.last_token_usage.{same fields}
//!   rate_limits.primary.{used_percent,window_minutes,resets_at}?  (account window)
//! ```
//!
//! This module provides **exact owner-reported usage evidence** after binding a
//! rollout to a managed execution. It does **not** by itself prove a hard
//! pre-dispatch or during-call token cap: real rollouts show additional
//! `response_item` provider rounds can be recorded after a `token_count` event
//! without an external budget interposition point.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

pub const CODEX_SESSION_USAGE_SCHEMA: &str = "codex_session_usage_event.v1";
pub const CODEX_SESSION_CURSOR_SCHEMA: &str = "codex_session_import_cursor.v1";
pub const CODEX_SESSION_ROLLUP_SCHEMA: &str = "codex_session_usage_rollup.v1";

/// Observed Codex CLI version for which this parser was verified.
pub const VERIFIED_CODEX_SESSION_LOG_VERSION: &str = "0.145.0";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TokenCounters {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_write_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub total_tokens: u64,
}

impl TokenCounters {
    pub fn from_json(value: &Value) -> Option<Self> {
        Some(Self {
            input_tokens: value.get("input_tokens")?.as_u64()?,
            cached_input_tokens: value
                .get("cached_input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            cache_write_input_tokens: value
                .get("cache_write_input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            output_tokens: value.get("output_tokens")?.as_u64()?,
            reasoning_output_tokens: value
                .get("reasoning_output_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            total_tokens: value.get("total_tokens")?.as_u64()?,
        })
    }

    /// Billable-ish input upper count used for cumulative task totals.
    /// Cached input is retained as a separate field; cumulative task tokens use
    /// Codex-reported `total_tokens` when present, else input+output+reasoning.
    pub fn effective_total(&self) -> u64 {
        if self.total_tokens > 0 {
            self.total_tokens
        } else {
            self.input_tokens
                .saturating_add(self.output_tokens)
                .saturating_add(self.reasoning_output_tokens)
        }
    }
}

/// Stable usage event derived from one `event_msg` / `token_count` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexSessionUsageEvent {
    pub schema_version: String,
    pub event_id: String,
    pub root_thread_id: String,
    pub parent_thread_id: Option<String>,
    pub source_thread_id: String,
    pub line_index: u64,
    pub timestamp: String,
    pub resolved_model: Option<String>,
    pub cli_version: Option<String>,
    pub total_token_usage: TokenCounters,
    pub last_token_usage: TokenCounters,
    /// `current_total - previous_total` on `total_token_usage.total_tokens`.
    pub cumulative_delta_total: u64,
    /// True when `last_token_usage.total_tokens == cumulative_delta_total`.
    pub last_matches_delta: bool,
    pub skipped_as_parent_replay: bool,
    pub ambiguous: bool,
    pub ambiguity_reason: Option<String>,
    pub source_path_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionImportCursor {
    pub schema_version: String,
    pub path_fingerprint: String,
    pub byte_offset: u64,
    pub next_line_index: u64,
    pub last_total_tokens: u64,
}

impl SessionImportCursor {
    pub fn new(path_fingerprint: String) -> Self {
        Self {
            schema_version: CODEX_SESSION_CURSOR_SCHEMA.to_string(),
            path_fingerprint,
            byte_offset: 0,
            next_line_index: 0,
            last_total_tokens: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionMeta {
    pub session_id: String,
    pub thread_id: String,
    pub parent_thread_id: Option<String>,
    pub forked_from_id: Option<String>,
    pub cli_version: Option<String>,
    pub is_subagent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionUsageRollup {
    pub schema_version: String,
    pub root_thread_id: String,
    pub events: Vec<CodexSessionUsageEvent>,
    pub cumulative_total_tokens: u64,
    pub cumulative_input_tokens: u64,
    pub cumulative_output_tokens: u64,
    pub cumulative_reasoning_tokens: u64,
    pub resolved_model: Option<String>,
    pub deferred_child_threads: Vec<String>,
    pub ambiguities: Vec<String>,
}

/// Fingerprint a path for stable evidence without storing private absolute paths.
pub fn path_fingerprint(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown");
    let bytes = std::fs::read(path).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(name.as_bytes());
    hasher.update(b"\0");
    hasher.update((bytes.len() as u64).to_le_bytes());
    if !bytes.is_empty() {
        // Content-address identity without retaining body text in evidence.
        hasher.update(Sha256::digest(&bytes));
    }
    hex::encode(hasher.finalize())
}

/// Cross-file parent/child replay signature (timestamp + counters, no line index).
pub fn replay_signature(timestamp: &str, total: &TokenCounters, last: &TokenCounters) -> String {
    format!(
        "{}:{}:{}:{}:{}:{}",
        timestamp,
        total.total_tokens,
        total.input_tokens,
        total.output_tokens,
        last.total_tokens,
        last.input_tokens
    )
}

pub fn stable_event_id(
    root_thread_id: &str,
    source_thread_id: &str,
    line_index: u64,
    timestamp: &str,
    total: &TokenCounters,
    last: &TokenCounters,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(root_thread_id.as_bytes());
    hasher.update(b"|");
    hasher.update(source_thread_id.as_bytes());
    hasher.update(b"|");
    hasher.update(line_index.to_string().as_bytes());
    hasher.update(b"|");
    hasher.update(timestamp.as_bytes());
    hasher.update(b"|");
    hasher.update(total.total_tokens.to_string().as_bytes());
    hasher.update(b"|");
    hasher.update(last.total_tokens.to_string().as_bytes());
    format!("csu-{}", &hex::encode(hasher.finalize())[..24])
}

/// Saturating cumulative delta. Non-monotonic counters yield delta 0 and mark
/// ambiguity for the caller rather than inventing negative usage.
pub fn cumulative_delta(previous_total: u64, current_total: u64) -> (u64, bool) {
    if current_total >= previous_total {
        (current_total - previous_total, false)
    } else {
        (0, true)
    }
}

fn optional_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub fn parse_session_meta(event: &Value) -> Option<SessionMeta> {
    if event.get("type").and_then(Value::as_str) != Some("session_meta") {
        return None;
    }
    let payload = event.get("payload")?;
    let session_id =
        optional_string(payload, "session_id").or_else(|| optional_string(payload, "id"))?;
    let thread_id = optional_string(payload, "id")
        .or_else(|| optional_string(payload, "session_id"))
        .unwrap_or_else(|| session_id.clone());
    let parent_thread_id = optional_string(payload, "parent_thread_id").or_else(|| {
        payload
            .pointer("/source/subagent/thread_spawn/parent_thread_id")
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    let forked_from_id = optional_string(payload, "forked_from_id");
    let is_subagent = payload.pointer("/source/subagent").is_some()
        || parent_thread_id.is_some()
        || forked_from_id.is_some();
    Some(SessionMeta {
        session_id,
        thread_id,
        parent_thread_id,
        forked_from_id,
        cli_version: optional_string(payload, "cli_version"),
        is_subagent,
    })
}

fn parse_turn_model(event: &Value) -> Option<String> {
    if event.get("type").and_then(Value::as_str) != Some("turn_context") {
        return None;
    }
    let payload = event.get("payload")?;
    optional_string(payload, "model").or_else(|| {
        payload
            .pointer("/collaboration_mode/settings/model")
            .and_then(Value::as_str)
            .map(str::to_string)
    })
}

fn parse_token_count_payload(event: &Value) -> Option<(TokenCounters, TokenCounters, String)> {
    if event.get("type").and_then(Value::as_str) != Some("event_msg") {
        return None;
    }
    let payload = event.get("payload")?;
    if payload.get("type").and_then(Value::as_str) != Some("token_count") {
        return None;
    }
    let info = payload.get("info")?;
    let total = TokenCounters::from_json(info.get("total_token_usage")?)?;
    let last = TokenCounters::from_json(info.get("last_token_usage")?)?;
    let timestamp = optional_string(event, "timestamp").unwrap_or_default();
    Some((total, last, timestamp))
}

/// Whether this event label indicates a new model/provider round after usage.
/// Used only for the request-ordering evidence fixture, not for success claims.
pub fn is_provider_round_label(label: &str) -> bool {
    matches!(
        label,
        "response_item:message"
            | "response_item:reasoning"
            | "response_item:function_call"
            | "response_item:custom_tool_call"
            | "response_item:tool_call"
    )
}

pub fn event_label(event: &Value) -> String {
    let top = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    if top == "event_msg" {
        let inner = event
            .get("payload")
            .and_then(|payload| payload.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        format!("event_msg:{inner}")
    } else if top == "response_item" {
        let inner = event
            .get("payload")
            .and_then(|payload| payload.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        format!("response_item:{inner}")
    } else {
        top.to_string()
    }
}

/// Request-ordering probe over an already-recorded label sequence.
///
/// Returns `true` only when every `event_msg:token_count` is never followed by a
/// later provider-round label in the same stream without an intervening external
/// gate marker. Real Codex rollouts fail this probe, proving JSONL observation
/// alone is not a cross-call hard authority boundary.
pub fn request_ordering_is_hard_gate(labels: &[String]) -> bool {
    let mut saw_token = false;
    for label in labels {
        if label == "event_msg:token_count" {
            saw_token = true;
            continue;
        }
        if saw_token && is_provider_round_label(label) {
            // A provider round after token_count means Codex proceeded without
            // an external budget interposition point in the log stream.
            return false;
        }
        if label == "external_budget_gate" {
            saw_token = false;
        }
    }
    true
}

/// Import usage events from one rollout file, resuming from `cursor`.
pub fn import_session_file(
    path: &Path,
    admitted_root_thread_id: &str,
    parent_event_signatures: &HashSet<String>,
    cursor: &mut SessionImportCursor,
    latest_model: &mut Option<String>,
) -> Result<Vec<CodexSessionUsageEvent>, String> {
    let fingerprint = path_fingerprint(path);
    if cursor.path_fingerprint != fingerprint && cursor.byte_offset != 0 {
        // File moved/replaced; restart only when content identity changes.
        *cursor = SessionImportCursor::new(fingerprint.clone());
    }
    cursor.path_fingerprint = fingerprint.clone();

    let mut file = File::open(path).map_err(|error| format!("session open failed: {error}"))?;
    file.seek(SeekFrom::Start(cursor.byte_offset))
        .map_err(|error| format!("session seek failed: {error}"))?;
    let mut reader = BufReader::new(file);
    let mut events = Vec::new();
    let mut meta: Option<SessionMeta> = None;
    let mut line_buf = String::new();
    let mut absolute_line = cursor.next_line_index;
    let mut previous_total = cursor.last_total_tokens;

    loop {
        line_buf.clear();
        let read = reader
            .read_line(&mut line_buf)
            .map_err(|error| format!("session read failed: {error}"))?;
        if read == 0 {
            break;
        }
        let consumed = read as u64;
        let line_index = absolute_line;
        absolute_line = absolute_line.saturating_add(1);
        cursor.byte_offset = cursor.byte_offset.saturating_add(consumed);
        cursor.next_line_index = absolute_line;

        let trimmed = line_buf.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(trimmed) {
            Ok(value) => value,
            Err(_) => {
                // Malformed line: fail closed for this file import.
                return Err(format!(
                    "malformed JSONL at line {line_index} of admitted session"
                ));
            }
        };

        if let Some(parsed_meta) = parse_session_meta(&value) {
            // Reject unrelated sessions.
            let root = parsed_meta
                .parent_thread_id
                .clone()
                .or_else(|| parsed_meta.forked_from_id.clone())
                .unwrap_or_else(|| parsed_meta.thread_id.clone());
            if parsed_meta.thread_id != admitted_root_thread_id
                && root != admitted_root_thread_id
                && parsed_meta.session_id != admitted_root_thread_id
            {
                return Err(
                    "session thread identity is not bound to the admitted managed root".to_string(),
                );
            }
            meta = Some(parsed_meta);
            continue;
        }
        if let Some(model) = parse_turn_model(&value) {
            *latest_model = Some(model);
            continue;
        }
        let Some((total, last, timestamp)) = parse_token_count_payload(&value) else {
            continue;
        };
        let meta = meta.clone().ok_or_else(|| {
            "token_count observed before session_meta; missing session metadata".to_string()
        })?;
        let source_thread_id = meta.thread_id.clone();
        let root_thread_id = meta
            .parent_thread_id
            .clone()
            .or_else(|| meta.forked_from_id.clone())
            .unwrap_or_else(|| source_thread_id.clone());
        if root_thread_id != admitted_root_thread_id && source_thread_id != admitted_root_thread_id
        {
            return Err("token event thread is not bound to admitted root".to_string());
        }

        let (delta, non_monotonic) = cumulative_delta(previous_total, total.total_tokens);
        let event_id = stable_event_id(
            &root_thread_id,
            &source_thread_id,
            line_index,
            &timestamp,
            &total,
            &last,
        );
        // Parent/child replay signatures deliberately omit line_index: forked
        // rollouts copy token_count payloads with the same timestamp/totals but
        // different line positions.
        let signature = replay_signature(&timestamp, &total, &last);
        let skipped_as_parent_replay = parent_event_signatures.contains(&signature)
            || parent_event_signatures.contains(&event_id);
        let last_matches_delta = last.total_tokens == delta;
        let mut ambiguous = non_monotonic || !last_matches_delta;
        let mut ambiguity_reason = None;
        if non_monotonic {
            ambiguity_reason = Some("non_monotonic_total_token_usage".to_string());
        } else if !last_matches_delta && delta > 0 {
            // Observed in real logs as rare; keep event but mark ambiguity.
            ambiguity_reason = Some("last_token_usage_differs_from_cumulative_delta".to_string());
        } else if delta == 0 && last.total_tokens > 0 {
            ambiguous = true;
            ambiguity_reason = Some("zero_delta_with_nonzero_last_token_usage".to_string());
        }

        // Always advance the cumulative watermark so a child file that copies a
        // parent prefix then continues with higher totals computes the correct
        // post-prefix delta. Only the counted delta is zeroed when skipped.
        previous_total = total.total_tokens;
        cursor.last_total_tokens = previous_total;

        events.push(CodexSessionUsageEvent {
            schema_version: CODEX_SESSION_USAGE_SCHEMA.to_string(),
            event_id,
            root_thread_id,
            parent_thread_id: meta.parent_thread_id.clone(),
            source_thread_id,
            line_index,
            timestamp,
            resolved_model: latest_model.clone(),
            cli_version: meta.cli_version.clone(),
            total_token_usage: total,
            last_token_usage: last,
            cumulative_delta_total: if skipped_as_parent_replay { 0 } else { delta },
            last_matches_delta,
            skipped_as_parent_replay,
            ambiguous,
            ambiguity_reason,
            source_path_fingerprint: fingerprint.clone(),
        });
    }
    Ok(events)
}

/// Discover rollout JSONL files under a CODEX_HOME-like root.
pub fn discover_rollout_files(codex_home: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let sessions = codex_home.join("sessions");
    let archived = codex_home.join("archived_sessions");
    for root in [sessions, archived] {
        if !root.exists() {
            continue;
        }
        visit_jsonl(&root, &mut out)?;
    }
    out.sort();
    Ok(out)
}

fn visit_jsonl(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|error| format!("failed to read session directory: {error}"))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("session dir entry error: {error}"))?;
        let path = entry.path();
        if path.is_dir() {
            visit_jsonl(&path, out)?;
        } else if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl"))
        {
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if name.starts_with("rollout-") {
                out.push(path);
            }
        }
    }
    Ok(())
}

/// Extract root thread id from the first session_meta in a file.
pub fn root_thread_id_from_file(path: &Path) -> Result<Option<SessionMeta>, String> {
    let file = File::open(path).map_err(|error| format!("session open failed: {error}"))?;
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|error| format!("session read failed: {error}"))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(trimmed)
            .map_err(|_| "malformed session_meta JSONL".to_string())?;
        if let Some(meta) = parse_session_meta(&value) {
            return Ok(Some(meta));
        }
    }
    Ok(None)
}

/// Import all admitted rollouts for one managed root thread under a CODEX_HOME.
pub fn import_managed_codex_home(
    codex_home: &Path,
    admitted_root_thread_id: &str,
) -> Result<SessionUsageRollup, String> {
    let files = discover_rollout_files(codex_home)?;
    let mut parent_signatures = HashSet::new();
    let mut child_files = Vec::new();
    let mut root_file = None;
    let mut metas = HashMap::new();

    for path in &files {
        if let Some(meta) = root_thread_id_from_file(path)? {
            metas.insert(path.clone(), meta.clone());
            if meta.thread_id == admitted_root_thread_id
                || meta.session_id == admitted_root_thread_id
            {
                root_file = Some(path.clone());
            } else if meta
                .parent_thread_id
                .as_deref()
                .is_some_and(|parent| parent == admitted_root_thread_id)
                || meta
                    .forked_from_id
                    .as_deref()
                    .is_some_and(|parent| parent == admitted_root_thread_id)
            {
                child_files.push(path.clone());
            }
        }
    }

    let Some(root_file) = root_file else {
        return Err("admitted root thread session file was not found".to_string());
    };

    let mut cursor = SessionImportCursor::new(path_fingerprint(&root_file));
    let mut model = None;
    let mut events = import_session_file(
        &root_file,
        admitted_root_thread_id,
        &parent_signatures,
        &mut cursor,
        &mut model,
    )?;
    for event in &events {
        parent_signatures.insert(event.event_id.clone());
        parent_signatures.insert(replay_signature(
            &event.timestamp,
            &event.total_token_usage,
            &event.last_token_usage,
        ));
    }

    let mut deferred = Vec::new();
    let mut ambiguities = Vec::new();
    for child in child_files {
        let meta = metas.get(&child).cloned();
        let Some(meta) = meta else {
            deferred.push(child.display().to_string());
            continue;
        };
        // If parent has no events yet, defer rather than guess.
        if events.is_empty() {
            deferred.push(meta.thread_id);
            continue;
        }
        let mut child_cursor = SessionImportCursor::new(path_fingerprint(&child));
        let mut child_model = model.clone();
        match import_session_file(
            &child,
            admitted_root_thread_id,
            &parent_signatures,
            &mut child_cursor,
            &mut child_model,
        ) {
            Ok(mut child_events) => {
                for event in &child_events {
                    if event.ambiguous {
                        if let Some(reason) = &event.ambiguity_reason {
                            ambiguities.push(format!("{}:{}", event.event_id, reason));
                        }
                    }
                }
                events.append(&mut child_events);
                if child_model.is_some() {
                    model = child_model;
                }
            }
            Err(error) => {
                ambiguities.push(format!("child_import_deferred:{}:{error}", meta.thread_id));
                deferred.push(meta.thread_id);
            }
        }
    }

    let mut rollup = SessionUsageRollup {
        schema_version: CODEX_SESSION_ROLLUP_SCHEMA.to_string(),
        root_thread_id: admitted_root_thread_id.to_string(),
        events: events.clone(),
        cumulative_total_tokens: 0,
        cumulative_input_tokens: 0,
        cumulative_output_tokens: 0,
        cumulative_reasoning_tokens: 0,
        resolved_model: model,
        deferred_child_threads: deferred,
        ambiguities,
    };
    for event in events {
        if event.skipped_as_parent_replay {
            continue;
        }
        rollup.cumulative_total_tokens = rollup
            .cumulative_total_tokens
            .saturating_add(event.cumulative_delta_total);
        // Prefer last_token_usage for per-call input/output counters. When
        // last_matches_delta is false the event is already marked ambiguous;
        // still use last_* fields rather than inventing counters.
        let call = &event.last_token_usage;
        rollup.cumulative_input_tokens = rollup
            .cumulative_input_tokens
            .saturating_add(call.input_tokens);
        rollup.cumulative_output_tokens = rollup
            .cumulative_output_tokens
            .saturating_add(call.output_tokens);
        rollup.cumulative_reasoning_tokens = rollup
            .cumulative_reasoning_tokens
            .saturating_add(call.reasoning_output_tokens);
    }
    Ok(rollup)
}

/// Bind a rollup into product evidence JSON (no private paths/prompts).
pub fn rollup_to_product_evidence(
    rollup: &SessionUsageRollup,
    product_task_id: &str,
    workflow_node_id: &str,
    execution_id: &str,
    binary_version: &str,
    binary_sha256: &str,
) -> Value {
    serde_json::json!({
        "schema_version": CODEX_SESSION_ROLLUP_SCHEMA,
        "verified_codex_log_version": VERIFIED_CODEX_SESSION_LOG_VERSION,
        "product_task_id": product_task_id,
        "workflow_node_id": workflow_node_id,
        "execution_id": execution_id,
        "binary_version": binary_version,
        "binary_sha256": binary_sha256,
        "root_thread_id": rollup.root_thread_id,
        "resolved_model": rollup.resolved_model,
        "cumulative_total_tokens": rollup.cumulative_total_tokens,
        "cumulative_input_tokens": rollup.cumulative_input_tokens,
        "cumulative_output_tokens": rollup.cumulative_output_tokens,
        "cumulative_reasoning_tokens": rollup.cumulative_reasoning_tokens,
        "event_count": rollup.events.len(),
        "event_ids": rollup.events.iter().map(|event| &event.event_id).collect::<Vec<_>>(),
        "deferred_child_threads": rollup.deferred_child_threads,
        "ambiguities": rollup.ambiguities,
        "authority_class": "owner_reported_session_usage",
        "hard_cross_call_gate": false,
        "single_request_output_cap": false,
    })
}

/// Official subscription-window preflight evidence (account-level only).
///
/// Isolated adapter: does not persist credentials, does not claim task budget
/// reservation, and fails closed when credentials or the endpoint are unavailable.
#[derive(Debug, Clone, PartialEq)]
pub struct CodexSubscriptionQuotaSnapshot {
    pub schema_version: String,
    pub plan_type: Option<String>,
    pub primary_used_percent: Option<f64>,
    pub primary_window_minutes: Option<u64>,
    pub primary_resets_at: Option<i64>,
    pub secondary_used_percent: Option<f64>,
    pub query_status: String,
}

pub const CODEX_SUBSCRIPTION_QUOTA_SCHEMA: &str = "codex_subscription_quota.v1";

/// Parse a redacted quota payload shape as observed on `token_count.rate_limits`.
/// This is **account-window utilization**, never task-token usage.
pub fn parse_subscription_quota_from_rate_limits(
    rate_limits: &Value,
) -> Result<CodexSubscriptionQuotaSnapshot, String> {
    if rate_limits.is_null() {
        return Err("rate_limits missing".to_string());
    }
    let primary = rate_limits.get("primary");
    let secondary = rate_limits.get("secondary");
    Ok(CodexSubscriptionQuotaSnapshot {
        schema_version: CODEX_SUBSCRIPTION_QUOTA_SCHEMA.to_string(),
        plan_type: optional_string(rate_limits, "plan_type"),
        primary_used_percent: primary
            .and_then(|value| value.get("used_percent"))
            .and_then(Value::as_f64),
        primary_window_minutes: primary
            .and_then(|value| value.get("window_minutes"))
            .and_then(Value::as_u64),
        primary_resets_at: primary
            .and_then(|value| value.get("resets_at"))
            .and_then(Value::as_i64),
        secondary_used_percent: secondary
            .and_then(|value| value.get("used_percent"))
            .and_then(Value::as_f64),
        query_status: "parsed_from_session_rate_limits".to_string(),
    })
}

/// Fail-closed official quota query placeholder.
///
/// Live ChatGPT OAuth retrieval is operator-bound and never runs in CI. This
/// function records the isolation boundary without performing a network call.
pub fn query_official_subscription_quota_preflight(
    auth_mode: &str,
) -> Result<CodexSubscriptionQuotaSnapshot, String> {
    if auth_mode != "chatgpt_oauth" {
        return Err(
            "only official ChatGPT OAuth auth mode is supported for quota preflight".to_string(),
        );
    }
    // Network disabled in CI / default product path. Operators may later enable
    // a versioned adapter once the endpoint contract is reviewed.
    Err(
        "codex_subscription_quota_query_disabled: no network preflight in provider-free path"
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn write_rollout(dir: &Path, name: &str, lines: &[Value]) -> PathBuf {
        let sessions = dir.join("sessions").join("2026").join("07").join("24");
        std::fs::create_dir_all(&sessions).unwrap();
        let path = sessions.join(name);
        let mut file = File::create(&path).unwrap();
        for line in lines {
            writeln!(file, "{}", serde_json::to_string(line).unwrap()).unwrap();
        }
        path
    }

    fn session_meta(id: &str, parent: Option<&str>) -> Value {
        let mut payload = json!({
            "session_id": id,
            "id": id,
            "cli_version": "0.145.0",
            "model_provider": "openai",
            "timestamp": "2026-07-24T00:00:00.000Z",
        });
        if let Some(parent) = parent {
            payload["parent_thread_id"] = json!(parent);
            payload["forked_from_id"] = json!(parent);
            payload["source"] = json!({
                "subagent": {
                    "thread_spawn": {
                        "parent_thread_id": parent,
                        "depth": 1
                    }
                }
            });
        }
        json!({
            "timestamp": "2026-07-24T00:00:00.000Z",
            "type": "session_meta",
            "payload": payload
        })
    }

    fn turn_context(model: &str) -> Value {
        json!({
            "timestamp": "2026-07-24T00:00:01.000Z",
            "type": "turn_context",
            "payload": {
                "turn_id": "turn-1",
                "model": model
            }
        })
    }

    fn token_count(ts: &str, total: u64, last: u64, input: u64, output: u64) -> Value {
        json!({
            "timestamp": ts,
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "info": {
                    "total_token_usage": {
                        "input_tokens": input,
                        "cached_input_tokens": 0,
                        "cache_write_input_tokens": 0,
                        "output_tokens": output,
                        "reasoning_output_tokens": 0,
                        "total_tokens": total
                    },
                    "last_token_usage": {
                        "input_tokens": input,
                        "cached_input_tokens": 0,
                        "cache_write_input_tokens": 0,
                        "output_tokens": output,
                        "reasoning_output_tokens": 0,
                        "total_tokens": last
                    },
                    "model_context_window": 200000
                },
                "rate_limits": {
                    "plan_type": "plus",
                    "primary": {
                        "used_percent": 12.5,
                        "window_minutes": 300,
                        "resets_at": 1_700_000_000
                    }
                }
            }
        })
    }

    #[test]
    fn cumulative_delta_and_last_token_usage() {
        assert_eq!(cumulative_delta(10, 25), (15, false));
        assert_eq!(cumulative_delta(25, 20), (0, true));
        let home = tempfile::tempdir().unwrap();
        write_rollout(
            home.path(),
            "rollout-root.jsonl",
            &[
                session_meta("root-1", None),
                turn_context("gpt-test"),
                token_count("t1", 100, 100, 80, 20),
                token_count("t2", 150, 50, 30, 20),
            ],
        );
        let rollup = import_managed_codex_home(home.path(), "root-1").unwrap();
        assert_eq!(rollup.events.len(), 2);
        assert_eq!(rollup.events[0].cumulative_delta_total, 100);
        assert!(rollup.events[0].last_matches_delta);
        assert_eq!(rollup.events[1].cumulative_delta_total, 50);
        assert!(rollup.events[1].last_matches_delta);
        assert_eq!(rollup.resolved_model.as_deref(), Some("gpt-test"));
        assert_eq!(rollup.cumulative_total_tokens, 150);
    }

    #[test]
    fn zero_delta_with_nonzero_last_is_ambiguous_not_double_counted() {
        let home = tempfile::tempdir().unwrap();
        write_rollout(
            home.path(),
            "rollout-root.jsonl",
            &[
                session_meta("root-1", None),
                token_count("t1", 100, 100, 80, 20),
                token_count("t2", 100, 40, 20, 20),
            ],
        );
        let rollup = import_managed_codex_home(home.path(), "root-1").unwrap();
        assert_eq!(rollup.events[1].cumulative_delta_total, 0);
        assert!(rollup.events[1].ambiguous);
        assert_eq!(rollup.cumulative_total_tokens, 100);
    }

    #[test]
    fn parent_child_replay_prefix_is_skipped() {
        let home = tempfile::tempdir().unwrap();
        write_rollout(
            home.path(),
            "rollout-root.jsonl",
            &[
                session_meta("root-1", None),
                token_count("t1", 100, 100, 80, 20),
            ],
        );
        // Child copies parent token event signature then adds new usage.
        write_rollout(
            home.path(),
            "rollout-child.jsonl",
            &[
                session_meta("child-1", Some("root-1")),
                token_count("t1", 100, 100, 80, 20),
                token_count("t3", 130, 30, 20, 10),
            ],
        );
        let rollup = import_managed_codex_home(home.path(), "root-1").unwrap();
        assert!(rollup
            .events
            .iter()
            .any(|event| event.skipped_as_parent_replay));
        // Root 100 + child new 30 only.
        assert_eq!(rollup.cumulative_total_tokens, 130);
    }

    #[test]
    fn unrelated_session_is_excluded() {
        let home = tempfile::tempdir().unwrap();
        write_rollout(
            home.path(),
            "rollout-root.jsonl",
            &[
                session_meta("root-1", None),
                token_count("t1", 10, 10, 5, 5),
            ],
        );
        write_rollout(
            home.path(),
            "rollout-other.jsonl",
            &[
                session_meta("other-9", None),
                token_count("t9", 999, 999, 500, 499),
            ],
        );
        let rollup = import_managed_codex_home(home.path(), "root-1").unwrap();
        assert_eq!(rollup.cumulative_total_tokens, 10);
        assert!(rollup
            .events
            .iter()
            .all(|event| event.root_thread_id == "root-1" || event.source_thread_id == "root-1"));
    }

    #[test]
    fn import_is_idempotent_with_cursor() {
        let home = tempfile::tempdir().unwrap();
        let path = write_rollout(
            home.path(),
            "rollout-root.jsonl",
            &[
                session_meta("root-1", None),
                token_count("t1", 40, 40, 20, 20),
            ],
        );
        let mut cursor = SessionImportCursor::new(path_fingerprint(&path));
        let mut model = None;
        let parents = HashSet::new();
        let first =
            import_session_file(&path, "root-1", &parents, &mut cursor, &mut model).unwrap();
        assert_eq!(first.len(), 1);
        let second =
            import_session_file(&path, "root-1", &parents, &mut cursor, &mut model).unwrap();
        assert!(second.is_empty());
    }

    #[test]
    fn malformed_token_event_fails_closed() {
        let home = tempfile::tempdir().unwrap();
        let path = write_rollout(
            home.path(),
            "rollout-root.jsonl",
            &[session_meta("root-1", None)],
        );
        // Append malformed line
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(file, "{{not-json").unwrap();
        let mut cursor = SessionImportCursor::new(path_fingerprint(&path));
        let mut model = None;
        let err = import_session_file(&path, "root-1", &HashSet::new(), &mut cursor, &mut model)
            .unwrap_err();
        assert!(err.contains("malformed"));
    }

    #[test]
    fn missing_session_meta_before_token_fails_closed() {
        let home = tempfile::tempdir().unwrap();
        let path = write_rollout(
            home.path(),
            "rollout-root.jsonl",
            &[token_count("t1", 10, 10, 5, 5)],
        );
        let mut cursor = SessionImportCursor::new(path_fingerprint(&path));
        let mut model = None;
        let err = import_session_file(&path, "root-1", &HashSet::new(), &mut cursor, &mut model)
            .unwrap_err();
        assert!(err.contains("session_meta") || err.contains("metadata"));
    }

    #[test]
    fn request_ordering_probe_fails_when_provider_round_follows_token_count() {
        // Sequence observed in real Codex 0.145.0 rollouts.
        let labels = vec![
            "response_item:message".into(),
            "response_item:custom_tool_call".into(),
            "event_msg:token_count".into(),
            "response_item:reasoning".into(),
            "response_item:message".into(),
        ];
        assert!(
            !request_ordering_is_hard_gate(&labels),
            "Codex continues provider rounds after token_count without an external gate"
        );
        let gated = vec![
            "event_msg:token_count".into(),
            "external_budget_gate".into(),
            "response_item:message".into(),
        ];
        assert!(request_ordering_is_hard_gate(&gated));
    }

    #[test]
    fn subscription_quota_is_account_level_not_task_budget() {
        let payload = json!({
            "plan_type": "plus",
            "primary": {"used_percent": 40.0, "window_minutes": 300, "resets_at": 123}
        });
        let snap = parse_subscription_quota_from_rate_limits(&payload).unwrap();
        assert_eq!(snap.primary_used_percent, Some(40.0));
        assert_eq!(snap.schema_version, CODEX_SUBSCRIPTION_QUOTA_SCHEMA);
        let err = query_official_subscription_quota_preflight("chatgpt_oauth").unwrap_err();
        assert!(err.contains("disabled") || err.contains("no network"));
        assert!(query_official_subscription_quota_preflight("api_key").is_err());
    }

    #[test]
    fn stable_event_ids_are_deterministic() {
        let total = TokenCounters {
            total_tokens: 10,
            input_tokens: 7,
            output_tokens: 3,
            ..TokenCounters::default()
        };
        let last = total.clone();
        let a = stable_event_id("root", "root", 3, "ts", &total, &last);
        let b = stable_event_id("root", "root", 3, "ts", &total, &last);
        assert_eq!(a, b);
        assert!(a.starts_with("csu-"));
    }
}
