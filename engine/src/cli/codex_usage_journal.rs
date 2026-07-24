//! Parent/Rust-owned Codex budget usage journal (not a second ProductTask budget owner).
//!
//! The journal lives **outside** every path mounted into the child sandbox so the
//! mediated Codex process cannot read or rewrite reservation state.
//!
//! Durability contract:
//! - reserve/in-flight is persisted **before** any upstream forward;
//! - committed usage / terminal state is persisted **before** the gateway returns
//!   a provider response body to the child;
//! - journal write/sync failure halts the gateway (no further admits);
//! - restart of an in-flight or outcome-unknown attempt never returns budget.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const CODEX_USAGE_JOURNAL_SCHEMA: &str = "codex_usage_journal.v2";
/// Parent-only journal root name under the process temp dir (never sandbox-mounted).
pub const PARENT_JOURNAL_ROOT_NAME: &str = "acp-codex-parent-journal";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalRequestState {
    /// No in-flight request; counters are durable.
    Idle,
    /// A request was reserved and may have been forwarded; budget is charged.
    InFlight,
    /// Forward completed with measured usage committed.
    Committed,
    /// Forward or usage classification failed; budget remains charged.
    OutcomeUnknown,
    /// Journal persistence failed; gateway must refuse further admits.
    PersistenceFailed,
}

impl JournalRequestState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::InFlight => "in_flight",
            Self::Committed => "committed",
            Self::OutcomeUnknown => "outcome_unknown",
            Self::PersistenceFailed => "persistence_failed",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "idle" => Ok(Self::Idle),
            "in_flight" => Ok(Self::InFlight),
            "committed" => Ok(Self::Committed),
            "outcome_unknown" => Ok(Self::OutcomeUnknown),
            "persistence_failed" => Ok(Self::PersistenceFailed),
            other => Err(format!("unknown journal request state: {other}")),
        }
    }

    /// States that permanently charge or block budget on resume.
    pub fn charges_or_blocks_budget(self) -> bool {
        matches!(
            self,
            Self::InFlight | Self::Committed | Self::OutcomeUnknown | Self::PersistenceFailed
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodexUsageJournalEntry {
    pub schema_version: String,
    /// Collision-resistant attempt identity (only this attempt may resume the journal).
    pub attempt_id: String,
    pub task_id: String,
    pub provider_kind: String,
    pub provider_host: String,
    pub model: String,
    pub binary_sha256: String,
    pub provider_requests: u64,
    /// Declared retry axis counter (see notes: Codex does not label retries on HTTP).
    pub observed_retry_posts: u64,
    pub cumulative_input_tokens: u64,
    pub cumulative_output_tokens: u64,
    /// Conservative tokens reserved for the current in-flight request (if any).
    pub reserved_input_tokens: u64,
    pub reserved_output_tokens: u64,
    pub state: JournalRequestState,
    pub last_request_id: Option<String>,
    pub integrity_sha256: String,
}

impl CodexUsageJournalEntry {
    pub fn new_idle(
        attempt_id: &str,
        task_id: &str,
        provider_kind: &str,
        provider_host: &str,
        model: &str,
        binary_sha256: &str,
    ) -> Self {
        let mut entry = Self {
            schema_version: CODEX_USAGE_JOURNAL_SCHEMA.to_string(),
            attempt_id: attempt_id.to_string(),
            task_id: task_id.to_string(),
            provider_kind: provider_kind.to_string(),
            provider_host: provider_host.to_string(),
            model: model.to_string(),
            binary_sha256: binary_sha256.to_ascii_lowercase(),
            provider_requests: 0,
            observed_retry_posts: 0,
            cumulative_input_tokens: 0,
            cumulative_output_tokens: 0,
            reserved_input_tokens: 0,
            reserved_output_tokens: 0,
            state: JournalRequestState::Idle,
            last_request_id: None,
            integrity_sha256: String::new(),
        };
        entry.recompute_integrity();
        entry
    }

    fn canonical_payload(&self) -> Value {
        json!({
            "schema_version": self.schema_version,
            "attempt_id": self.attempt_id,
            "task_id": self.task_id,
            "provider_kind": self.provider_kind,
            "provider_host": self.provider_host,
            "model": self.model,
            "binary_sha256": self.binary_sha256,
            "provider_requests": self.provider_requests,
            "observed_retry_posts": self.observed_retry_posts,
            "cumulative_input_tokens": self.cumulative_input_tokens,
            "cumulative_output_tokens": self.cumulative_output_tokens,
            "reserved_input_tokens": self.reserved_input_tokens,
            "reserved_output_tokens": self.reserved_output_tokens,
            "state": self.state.as_str(),
            "last_request_id": self.last_request_id,
            // Never persist prompts, outputs, credentials, or private paths.
        })
    }

    pub fn recompute_integrity(&mut self) {
        let body = serde_json::to_vec(&self.canonical_payload()).unwrap_or_default();
        self.integrity_sha256 = hex::encode(Sha256::digest(&body));
    }

    pub fn to_json(&self) -> Value {
        let mut value = self.canonical_payload();
        if let Some(object) = value.as_object_mut() {
            object.insert("integrity_sha256".to_string(), json!(self.integrity_sha256));
        }
        value
    }

    pub fn from_json(value: &Value) -> Result<Self, String> {
        if value.get("schema_version").and_then(Value::as_str) != Some(CODEX_USAGE_JOURNAL_SCHEMA) {
            return Err("usage journal schema version is unsupported".to_string());
        }
        let state = JournalRequestState::parse(
            value
                .get("state")
                .and_then(Value::as_str)
                .ok_or_else(|| "usage journal missing state".to_string())?,
        )?;
        let mut entry = Self {
            schema_version: CODEX_USAGE_JOURNAL_SCHEMA.to_string(),
            attempt_id: required_str(value, "attempt_id")?,
            task_id: required_str(value, "task_id")?,
            provider_kind: required_str(value, "provider_kind")?,
            provider_host: required_str(value, "provider_host")?,
            model: required_str(value, "model")?,
            binary_sha256: required_str(value, "binary_sha256")?,
            provider_requests: required_u64(value, "provider_requests")?,
            observed_retry_posts: required_u64(value, "observed_retry_posts")?,
            cumulative_input_tokens: required_u64(value, "cumulative_input_tokens")?,
            cumulative_output_tokens: required_u64(value, "cumulative_output_tokens")?,
            reserved_input_tokens: required_u64(value, "reserved_input_tokens")?,
            reserved_output_tokens: required_u64(value, "reserved_output_tokens")?,
            state,
            last_request_id: value
                .get("last_request_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            integrity_sha256: required_str(value, "integrity_sha256")?,
        };
        let expected = entry.integrity_sha256.clone();
        entry.recompute_integrity();
        if entry.integrity_sha256 != expected {
            return Err("usage journal integrity_sha256 mismatch (tamper detected)".to_string());
        }
        Ok(entry)
    }
}

fn required_str(value: &Value, key: &str) -> Result<String, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("usage journal missing {key}"))
}

fn required_u64(value: &Value, key: &str) -> Result<u64, String> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("usage journal missing {key}"))
}

/// Absolute parent-owned journal path for one attempt (never under sandbox mounts).
pub fn parent_owned_journal_path(attempt_id: &str) -> PathBuf {
    let attempt_id = attempt_id.trim();
    // Refuse path separators so the file stays under the parent journal root.
    let safe: String = attempt_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    std::env::temp_dir()
        .join(PARENT_JOURNAL_ROOT_NAME)
        .join(format!("{safe}.json"))
}

/// Durable write with fsync on the temp file before rename.
pub fn durable_write_json(path: &Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create usage journal dir: {error}"))?;
    }
    let body = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to encode usage journal: {error}"))?;
    let tmp = path.with_extension("tmp");
    {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)
            .map_err(|error| format!("failed to open usage journal tmp: {error}"))?;
        file.write_all(&body)
            .map_err(|error| format!("failed to write usage journal tmp: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("failed to sync usage journal tmp: {error}"))?;
    }
    fs::rename(&tmp, path).map_err(|error| format!("failed to commit usage journal: {error}"))?;
    // Best-effort directory sync so the rename itself is durable.
    if let Some(parent) = path.parent() {
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
    }
    Ok(())
}

pub fn load_journal(path: &Path) -> Result<CodexUsageJournalEntry, String> {
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("failed to read usage journal: {error}"))?;
    let value: Value = serde_json::from_str(&raw)
        .map_err(|error| format!("usage journal is not valid JSON: {error}"))?;
    CodexUsageJournalEntry::from_json(&value)
}

/// In-memory + durable journal owner used by the gateway.
#[derive(Debug)]
pub struct CodexUsageJournal {
    path: PathBuf,
    entry: CodexUsageJournalEntry,
    halted: bool,
}

impl CodexUsageJournal {
    pub fn create_new(
        path: PathBuf,
        attempt_id: &str,
        task_id: &str,
        provider_kind: &str,
        provider_host: &str,
        model: &str,
        binary_sha256: &str,
    ) -> Result<Self, String> {
        if path.exists() {
            return Err(
                "usage journal path already exists; refusing to overwrite a different attempt"
                    .to_string(),
            );
        }
        let entry = CodexUsageJournalEntry::new_idle(
            attempt_id,
            task_id,
            provider_kind,
            provider_host,
            model,
            binary_sha256,
        );
        durable_write_json(&path, &entry.to_json())?;
        Ok(Self {
            path,
            entry,
            halted: false,
        })
    }

    /// Resume only when attempt identity matches exactly.
    pub fn resume_exact_attempt(
        path: PathBuf,
        attempt_id: &str,
        task_id: &str,
        provider_kind: &str,
        provider_host: &str,
        model: &str,
        binary_sha256: &str,
    ) -> Result<Self, String> {
        let mut entry = load_journal(&path)?;
        if entry.attempt_id != attempt_id {
            return Err("usage journal attempt_id does not match authority".to_string());
        }
        if entry.task_id != task_id {
            return Err("usage journal task_id does not match authority".to_string());
        }
        if entry.provider_kind != provider_kind || entry.provider_host != provider_host {
            return Err("usage journal provider identity does not match authority".to_string());
        }
        if entry.model != model || entry.binary_sha256 != binary_sha256.to_ascii_lowercase() {
            return Err("usage journal model/binary identity does not match authority".to_string());
        }
        // In-flight or outcome-unknown: permanently charge reserved worst-case and
        // never return that budget to the free residual.
        if matches!(
            entry.state,
            JournalRequestState::InFlight | JournalRequestState::OutcomeUnknown
        ) {
            entry.cumulative_input_tokens = entry
                .cumulative_input_tokens
                .saturating_add(entry.reserved_input_tokens);
            entry.cumulative_output_tokens = entry
                .cumulative_output_tokens
                .saturating_add(entry.reserved_output_tokens);
            entry.reserved_input_tokens = 0;
            entry.reserved_output_tokens = 0;
            entry.state = JournalRequestState::OutcomeUnknown;
            entry.recompute_integrity();
            durable_write_json(&path, &entry.to_json())?;
        }
        let halted = entry.state == JournalRequestState::PersistenceFailed;
        Ok(Self {
            path,
            entry,
            halted,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn entry(&self) -> &CodexUsageJournalEntry {
        &self.entry
    }

    pub fn is_halted(&self) -> bool {
        self.halted || self.entry.state == JournalRequestState::PersistenceFailed
    }

    pub fn admits_new_request(&self) -> Result<(), String> {
        if self.is_halted() {
            return Err("usage journal is halted after persistence failure".to_string());
        }
        if matches!(
            self.entry.state,
            JournalRequestState::InFlight | JournalRequestState::OutcomeUnknown
        ) {
            return Err(
                "usage journal has unresolved in-flight or outcome-unknown request; no new request admitted"
                    .to_string(),
            );
        }
        Ok(())
    }

    fn persist(&mut self) -> Result<(), String> {
        self.entry.recompute_integrity();
        match durable_write_json(&self.path, &self.entry.to_json()) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.halted = true;
                self.entry.state = JournalRequestState::PersistenceFailed;
                // Best-effort write of the halted state; ignore secondary failure.
                self.entry.recompute_integrity();
                let _ = durable_write_json(&self.path, &self.entry.to_json());
                Err(error)
            }
        }
    }

    /// Pre-forward reservation: increments request count and persists in_flight.
    pub fn reserve_before_forward(
        &mut self,
        reserved_input: u64,
        reserved_output: u64,
    ) -> Result<(), String> {
        self.admits_new_request()?;
        self.entry.provider_requests = self.entry.provider_requests.saturating_add(1);
        // Without Codex labeling retries on the wire, every POST after the first
        // is counted on the observed_retry_posts axis for evidence only.
        if self.entry.provider_requests > 1 {
            self.entry.observed_retry_posts = self.entry.observed_retry_posts.saturating_add(1);
        }
        self.entry.reserved_input_tokens = reserved_input;
        self.entry.reserved_output_tokens = reserved_output;
        self.entry.state = JournalRequestState::InFlight;
        self.persist()
    }

    pub fn commit_after_forward(
        &mut self,
        input_tokens: u64,
        output_tokens: u64,
        last_request_id: Option<String>,
    ) -> Result<(), String> {
        if self.is_halted() {
            return Err("usage journal is halted".to_string());
        }
        if self.entry.state != JournalRequestState::InFlight {
            return Err("commit requires in_flight journal state".to_string());
        }
        self.entry.cumulative_input_tokens = self
            .entry
            .cumulative_input_tokens
            .saturating_add(input_tokens);
        self.entry.cumulative_output_tokens = self
            .entry
            .cumulative_output_tokens
            .saturating_add(output_tokens);
        self.entry.reserved_input_tokens = 0;
        self.entry.reserved_output_tokens = 0;
        self.entry.last_request_id = last_request_id;
        self.entry.state = JournalRequestState::Committed;
        self.persist()?;
        // Return to idle for the next admit while preserving committed counters.
        self.entry.state = JournalRequestState::Idle;
        self.persist()
    }

    pub fn mark_outcome_unknown(&mut self, detail: &str) -> Result<(), String> {
        let _ = detail;
        if self.is_halted() {
            return Err("usage journal is halted".to_string());
        }
        // Charge reserved worst-case so the attempt never regains that budget.
        self.entry.cumulative_input_tokens = self
            .entry
            .cumulative_input_tokens
            .saturating_add(self.entry.reserved_input_tokens);
        self.entry.cumulative_output_tokens = self
            .entry
            .cumulative_output_tokens
            .saturating_add(self.entry.reserved_output_tokens);
        self.entry.reserved_input_tokens = 0;
        self.entry.reserved_output_tokens = 0;
        self.entry.state = JournalRequestState::OutcomeUnknown;
        self.persist()
    }

    pub fn halt(&mut self, reason: &str) -> Result<(), String> {
        let _ = reason;
        self.halted = true;
        self.entry.state = JournalRequestState::PersistenceFailed;
        self.persist()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(label: &str) -> PathBuf {
        std::env::temp_dir()
            .join(PARENT_JOURNAL_ROOT_NAME)
            .join(format!(
                "test-{}-{}-{}.json",
                label,
                std::process::id(),
                uuid::Uuid::new_v4()
            ))
    }

    #[test]
    fn durable_round_trip_and_integrity() {
        let path = temp_path("round");
        let _ = fs::remove_file(&path);
        let mut journal = CodexUsageJournal::create_new(
            path.clone(),
            "attempt-1",
            "task-1",
            "openai_compatible",
            "api.openai.com",
            "gpt-test",
            "abc",
        )
        .unwrap();
        journal.reserve_before_forward(10, 5).unwrap();
        journal
            .commit_after_forward(8, 4, Some("resp_1".into()))
            .unwrap();
        let loaded = load_journal(&path).unwrap();
        assert_eq!(loaded.provider_requests, 1);
        assert_eq!(loaded.cumulative_input_tokens, 8);
        assert_eq!(loaded.cumulative_output_tokens, 4);
        assert_eq!(loaded.state, JournalRequestState::Idle);
        let raw = fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("sk-real"));
        assert!(!raw.contains("\"prompt\""));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn tamper_is_detected() {
        let path = temp_path("tamper");
        let _ = fs::remove_file(&path);
        let journal = CodexUsageJournal::create_new(
            path.clone(),
            "attempt-2",
            "task-1",
            "openai_compatible",
            "api.openai.com",
            "gpt-test",
            "abc",
        )
        .unwrap();
        drop(journal);
        let mut raw: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        raw.as_object_mut()
            .unwrap()
            .insert("provider_requests".into(), json!(99));
        fs::write(&path, serde_json::to_vec_pretty(&raw).unwrap()).unwrap();
        let err = load_journal(&path).unwrap_err();
        assert!(err.contains("integrity") || err.contains("tamper"), "{err}");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn in_flight_restart_charges_reservation() {
        let path = temp_path("inflight");
        let _ = fs::remove_file(&path);
        let mut journal = CodexUsageJournal::create_new(
            path.clone(),
            "attempt-3",
            "task-1",
            "openai_compatible",
            "api.openai.com",
            "gpt-test",
            "abc",
        )
        .unwrap();
        journal.reserve_before_forward(20, 10).unwrap();
        drop(journal);
        let resumed = CodexUsageJournal::resume_exact_attempt(
            path.clone(),
            "attempt-3",
            "task-1",
            "openai_compatible",
            "api.openai.com",
            "gpt-test",
            "abc",
        )
        .unwrap();
        assert_eq!(resumed.entry().state, JournalRequestState::OutcomeUnknown);
        assert_eq!(resumed.entry().provider_requests, 1);
        assert_eq!(resumed.entry().cumulative_input_tokens, 20);
        assert_eq!(resumed.entry().cumulative_output_tokens, 10);
        assert!(resumed.admits_new_request().is_err());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn different_attempt_cannot_reuse_journal() {
        let path = temp_path("reuse");
        let _ = fs::remove_file(&path);
        let _ = CodexUsageJournal::create_new(
            path.clone(),
            "attempt-A",
            "task-1",
            "openai_compatible",
            "api.openai.com",
            "gpt-test",
            "abc",
        )
        .unwrap();
        let err = CodexUsageJournal::resume_exact_attempt(
            path.clone(),
            "attempt-B",
            "task-1",
            "openai_compatible",
            "api.openai.com",
            "gpt-test",
            "abc",
        )
        .unwrap_err();
        assert!(err.contains("attempt_id"), "{err}");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn parent_owned_path_is_under_parent_journal_root() {
        let path = parent_owned_journal_path("codex-attempt-xyz");
        assert!(path.to_string_lossy().contains(PARENT_JOURNAL_ROOT_NAME));
        assert!(path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with(".json"));
    }
}
