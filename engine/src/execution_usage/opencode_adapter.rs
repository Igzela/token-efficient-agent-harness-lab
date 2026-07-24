//! OpenCode read-only SQLite usage adapter → `ExecutionUsageEventV1`.
//!
//! Verified against local OpenCode DB (`~/.local/share/opencode/opencode.db`):
//!
//! - `message(id, session_id, data JSON)`
//! - assistant `data`: `role`, `tokens.{input,output,reasoning,cache.read,cache.write,total}`,
//!   `modelID`, `providerID`, `cost`, `time.created/completed`, `finish`
//! - `session.parent_id`, session-level token rollups (not used as per-call authority)
//!
//! Opens the database read-only (`mode=ro`). Never writes. Defers messages that
//! lack `time.completed` instead of freezing incomplete totals.

use std::path::Path;

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use super::codex_adapter::UsageBindingContext;
use super::{
    stable_usage_event_id, CostSource, EventCompleteness, EvidenceSourceKind,
    ExecutionUsageEventV1, ExecutorKind, EXECUTION_USAGE_EVENT_SCHEMA,
};

pub const OPENCODE_SQLITE_SOURCE_SCHEMA: &str = "opencode_sqlite_message_tokens.v1";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OpenCodeImportResult {
    pub events: Vec<ExecutionUsageEventV1>,
    pub deferred_message_ids: Vec<String>,
    pub deferred_session_ids: Vec<String>,
}

fn open_readonly(db_path: &Path) -> Result<Connection, String> {
    if !db_path.is_file() {
        return Err("opencode database path is not a file".into());
    }
    // Read-only URI; WAL may still be present — readers observe a consistent
    // snapshot without modifying the OpenCode database.
    let uri = format!("file:{}?mode=ro", db_path.display());
    Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|e| format!("opencode open readonly failed: {e}"))
}

fn u64_json(value: &Value, key: &str) -> u64 {
    value
        .get(key)
        .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|n| n.max(0) as u64)))
        .unwrap_or(0)
}

fn parse_assistant_data(
    message_id: &str,
    session_id: &str,
    parent_session_id: Option<&str>,
    data: &Value,
    binding: &UsageBindingContext,
) -> Result<Option<ExecutionUsageEventV1>, String> {
    let role = data.get("role").and_then(Value::as_str).unwrap_or("");
    if role != "assistant" {
        return Ok(None);
    }
    let time = data.get("time").cloned().unwrap_or(Value::Null);
    let completed = time.get("completed").and_then(Value::as_i64);
    if completed.is_none() {
        // Incomplete: defer rather than freeze partial totals.
        return Ok(None);
    }
    let tokens = data
        .get("tokens")
        .cloned()
        .ok_or_else(|| "assistant message missing tokens".to_string())?;
    let cache = tokens.get("cache").cloned().unwrap_or(Value::Null);
    let input = u64_json(&tokens, "input");
    let output = u64_json(&tokens, "output");
    let reasoning = u64_json(&tokens, "reasoning");
    let cache_read = u64_json(&cache, "read");
    let cache_write = u64_json(&cache, "write");
    let model = data
        .get("modelID")
        .and_then(Value::as_str)
        .map(str::to_string);
    let provider = data
        .get("providerID")
        .and_then(Value::as_str)
        .map(str::to_string);
    let cost = data.get("cost").and_then(|v| {
        v.as_f64()
            .or_else(|| v.as_i64().map(|n| n as f64))
            .or_else(|| v.as_u64().map(|n| n as f64))
    });
    let ts = completed
        .map(|ms| ms.to_string())
        .unwrap_or_else(|| "0".into());
    let token_sig = format!("i{input}:c{cache_read}:w{cache_write}:o{output}:r{reasoning}");
    let event_id = stable_usage_event_id(
        EvidenceSourceKind::OpenCodeSqlite,
        session_id,
        message_id,
        &token_sig,
        &ts,
    );
    // OpenCode embeds `cost` on assistant messages; treat as executor-reported
    // when present (including explicit 0).
    let (provider_reported_cost, cost_source) = match cost {
        Some(c) if c.is_finite() => (Some(c), CostSource::ProviderOrExecutorReported),
        _ => (None, CostSource::Unavailable),
    };
    Ok(Some(ExecutionUsageEventV1 {
        schema_version: EXECUTION_USAGE_EVENT_SCHEMA.to_string(),
        event_id: event_id.clone(),
        product_task_id: binding.product_task_id.clone(),
        workflow_node_id: binding.workflow_node_id.clone(),
        managed_execution_id: binding.managed_execution_id.clone(),
        executor_kind: ExecutorKind::OpenCode,
        evidence_source_kind: EvidenceSourceKind::OpenCodeSqlite,
        provider_id: provider,
        requested_model: binding.requested_model.clone(),
        resolved_model: model,
        executable_path_fingerprint: binding.executable_path_fingerprint.clone(),
        executable_version: binding.executable_version.clone(),
        executable_sha256: binding.executable_sha256.clone(),
        root_session_id: Some(session_id.to_string()),
        parent_session_id: parent_session_id.map(str::to_string),
        request_or_message_id: Some(message_id.to_string()),
        input_tokens: input,
        cached_input_tokens: cache_read,
        cache_creation_tokens: cache_write,
        output_tokens: output,
        reasoning_output_tokens: reasoning,
        cumulative_task_tokens: Some(u64_json(&tokens, "total")),
        provider_reported_cost,
        locally_estimated_cost: None,
        cost_source,
        pricing_table_version: None,
        timestamp: ts,
        event_completeness: EventCompleteness::Complete,
        source_schema_version: OPENCODE_SQLITE_SOURCE_SCHEMA.to_string(),
        stable_dedupe_identity: format!("{session_id}+{message_id}"),
        provenance_refs: vec![
            format!("session:{session_id}"),
            format!("message:{message_id}"),
            format!(
                "finish:{}",
                data.get("finish").and_then(Value::as_str).unwrap_or("none")
            ),
        ],
    }))
}

/// Import completed assistant usage from an OpenCode SQLite database (read-only).
pub fn import_opencode_db(
    db_path: &Path,
    admitted_session_id: Option<&str>,
    binding: &UsageBindingContext,
) -> Result<OpenCodeImportResult, String> {
    let conn = open_readonly(db_path)?;
    // Optional parent map
    let mut parent_map = std::collections::HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT id, parent_id FROM session") {
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })
            .map_err(|e| e.to_string())?;
        for row in rows.flatten() {
            parent_map.insert(row.0, row.1);
        }
    }

    let mut stmt = conn
        .prepare("SELECT id, session_id, data FROM message")
        .map_err(|e| format!("opencode message query failed: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut result = OpenCodeImportResult::default();
    for row in rows {
        let (message_id, session_id, data_text) = row.map_err(|e| e.to_string())?;
        if let Some(admitted) = admitted_session_id {
            if session_id != admitted
                && parent_map.get(&session_id).and_then(|p| p.as_deref()) != Some(admitted)
            {
                continue;
            }
        }
        let data: Value = match serde_json::from_str(&data_text) {
            Ok(v) => v,
            Err(_) => {
                result.deferred_message_ids.push(message_id);
                continue;
            }
        };
        let parent = parent_map.get(&session_id).and_then(|p| p.as_deref());
        match parse_assistant_data(&message_id, &session_id, parent, &data, binding)? {
            Some(event) => result.events.push(event),
            None => {
                if data.get("role").and_then(Value::as_str) == Some("assistant") {
                    result.deferred_message_ids.push(message_id);
                    if !result.deferred_session_ids.contains(&session_id) {
                        result.deferred_session_ids.push(session_id);
                    }
                }
            }
        }
    }
    result.events.sort_by(|a, b| {
        a.timestamp
            .cmp(&b.timestamp)
            .then(a.event_id.cmp(&b.event_id))
    });
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn fixture_db(path: &Path) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE session (
              id TEXT PRIMARY KEY,
              parent_id TEXT
            );
            CREATE TABLE message (
              id TEXT PRIMARY KEY,
              session_id TEXT,
              data TEXT
            );
            "#,
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session (id, parent_id) VALUES (?1, NULL), (?2, ?1)",
            params!["ses_root", "ses_child"],
        )
        .unwrap();
        let complete = serde_json::json!({
            "role": "assistant",
            "modelID": "test-model",
            "providerID": "opencode",
            "cost": 0.12,
            "finish": "stop",
            "tokens": {
                "total": 30,
                "input": 20,
                "output": 5,
                "reasoning": 5,
                "cache": {"read": 2, "write": 1}
            },
            "time": {"created": 1, "completed": 2}
        });
        let incomplete = serde_json::json!({
            "role": "assistant",
            "modelID": "test-model",
            "providerID": "opencode",
            "tokens": {"input": 9, "output": 1, "reasoning": 0, "cache": {"read": 0, "write": 0}},
            "time": {"created": 3}
        });
        conn.execute(
            "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
            params!["msg_done", "ses_root", complete.to_string()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
            params!["msg_partial", "ses_root", incomplete.to_string()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
            params![
                "msg_user",
                "ses_root",
                serde_json::json!({"role":"user"}).to_string()
            ],
        )
        .unwrap();
    }

    #[test]
    fn imports_completed_only_and_defers_incomplete() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        fixture_db(&db);
        let result =
            import_opencode_db(&db, Some("ses_root"), &UsageBindingContext::default()).unwrap();
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].input_tokens, 20);
        assert_eq!(result.events[0].output_tokens, 5);
        assert_eq!(result.events[0].reasoning_output_tokens, 5);
        assert_eq!(result.events[0].cached_input_tokens, 2);
        assert_eq!(result.events[0].cache_creation_tokens, 1);
        assert_eq!(
            result.events[0].cost_source,
            CostSource::ProviderOrExecutorReported
        );
        assert_eq!(result.events[0].provider_reported_cost, Some(0.12));
        assert_eq!(result.deferred_message_ids, vec!["msg_partial".to_string()]);
    }

    #[test]
    fn open_is_readonly_mode() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        fixture_db(&db);
        let _ = import_opencode_db(&db, None, &UsageBindingContext::default()).unwrap();
        // Original file still readable and schema intact
        let conn = Connection::open(&db).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM message", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 3);
    }
}
