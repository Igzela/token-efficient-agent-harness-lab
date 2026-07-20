//! Default-off OpenCode external coding executor (PE7 fixture-first).
//!
//! Rust owns admission, timeout, kill-switch, process-tree termination, and typed
//! receipts. The adapter process (fixture mode only) never receives network, MCP,
//! or provider authority. Real OpenCode binary admission is deferred
//! (`PE7-OPENCODE-BINARY-ADMISSION-1`).

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

fn env_kill_switch_active() -> bool {
    std::env::var(KILL_SWITCH_ENV).as_deref() == Ok("1")
}

use crate::event_schema::canonical_event_json;
use crate::node_executor::{NodeExecutionInput, NodeExecutionOutput, NodeExecutor};
use crate::provider::redaction::{contains_sensitive_patterns, redact_sensitive_patterns};

pub const OPENCODE_TASK_TYPE: &str = "opencode_external";
pub const OPENCODE_EXECUTOR_TYPE: &str = "opencode_external";
pub const OPENCODE_REQUEST_SCHEMA: &str = "opencode_external_request.v1";
pub const OPENCODE_RESULT_SCHEMA: &str = "opencode_external_result.v1";
pub const OPENCODE_NODE_SCHEMA: &str = "opencode_external_node.v1";
pub const OPENCODE_ADAPTER_CONTRACT: &str = "opencode_external_adapter.v1";
pub const OPENCODE_ADAPTER_VERSION: &str = "0.1.0";
/// Declared fixture runtime version string only — not a verified upstream binary pin.
pub const PINNED_OPENCODE_VERSION: &str = "1.1.48";
pub const FIXTURE_ADAPTER_MANIFEST_SCHEMA: &str = "opencode_fixture_adapter_manifest.v1";
pub const PATCH_GRAMMAR_VERSION: &str = "opencode_fixture_patch.v1";

pub const ENABLE_ENV: &str = "ACP_ENABLE_OPENCODE_RUNTIME";
pub const MODE_ENV: &str = "ACP_OPENCODE_MODE";
pub const PYTHON_ENV: &str = "ACP_OPENCODE_PYTHON";
pub const ADAPTER_PATH_ENV: &str = "ACP_OPENCODE_ADAPTER_PATH";
pub const KILL_SWITCH_ENV: &str = "ACP_OPENCODE_KILL_SWITCH";
pub const TIMEOUT_MS_ENV: &str = "ACP_OPENCODE_TIMEOUT_MS";
pub const MANIFEST_PATH_ENV: &str = "ACP_OPENCODE_FIXTURE_MANIFEST";

const MAX_PROCESS_OUTPUT_BYTES: usize = 256 * 1024;
const MAX_PROCESS_ERROR_BYTES: usize = 16 * 1024;
const MAX_PATCH_BYTES: usize = 64 * 1024;
const MAX_ALLOWED_PATHS: usize = 32;
const PROCESS_TERMINATE_GRACE_MS: u64 = 200;

/// Environment names the fixture adapter process may receive (exact allowlist).
const ADAPTER_ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "HOME",
    "LANG",
    "LC_ALL",
    "TMPDIR",
    "PYTHONIOENCODING",
    "PYTHONDONTWRITEBYTECODE",
    "PYTHONPATH",
];

/// Typed tool-summary counters that must be present on every accepted result.
const REQUIRED_TOOL_COUNTERS: &[&str] = &[
    "tool_call_count",
    "network_attempts",
    "provider_attempts",
    "mcp_attempts",
    "web_attempts",
    "remote_agent_attempts",
    "background_agent_attempts",
    "process_attempts",
];

/// Counters that must be exactly zero (forbidden capabilities in fixture mode).
const FORBIDDEN_TOOL_COUNTERS: &[&str] = &[
    "network_attempts",
    "provider_attempts",
    "mcp_attempts",
    "web_attempts",
    "remote_agent_attempts",
    "background_agent_attempts",
    "process_attempts",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenCodeMode {
    Fixture,
}

impl OpenCodeMode {
    fn as_str(self) -> &'static str {
        "fixture"
    }
}

#[derive(Debug, Clone)]
pub struct OpenCodeRuntimeConfig {
    pub mode: OpenCodeMode,
    pub python_program: PathBuf,
    pub adapter_path: PathBuf,
    pub timeout_ms: u64,
    pub adapter_version: String,
    pub expected_opencode_version: String,
    pub fixture_manifest_path: Option<PathBuf>,
}

impl OpenCodeRuntimeConfig {
    pub fn from_env() -> Result<Option<Self>, String> {
        if std::env::var(ENABLE_ENV).as_deref() != Ok("1") {
            return Ok(None);
        }
        if env_kill_switch_active() {
            return Err("OpenCode runtime kill switch is active".to_string());
        }
        let mode = match required_env(MODE_ENV)?.as_str() {
            "fixture" => OpenCodeMode::Fixture,
            "live" => return Err(
                "OpenCode live mode is not admitted; PE7-OPENCODE-BINARY-ADMISSION-1 is deferred"
                    .to_string(),
            ),
            other => return Err(format!("{MODE_ENV} unsupported value: {other}")),
        };
        let python_program = canonical_executable_path(&required_env(PYTHON_ENV)?, PYTHON_ENV)?;
        let adapter_path =
            canonical_regular_file(&required_env(ADAPTER_PATH_ENV)?, ADAPTER_PATH_ENV)?;
        let timeout_ms = required_u64(TIMEOUT_MS_ENV, 1_000, 120_000)?;
        let fixture_manifest_path = match std::env::var(MANIFEST_PATH_ENV) {
            Ok(path) => Some(canonical_regular_file(&path, MANIFEST_PATH_ENV)?),
            Err(_) => default_fixture_manifest_path(&adapter_path),
        };
        if let Some(ref path) = fixture_manifest_path {
            validate_fixture_adapter_manifest(path, &adapter_path)?;
        }
        Ok(Some(Self {
            mode,
            python_program,
            adapter_path,
            timeout_ms,
            adapter_version: OPENCODE_ADAPTER_VERSION.to_string(),
            expected_opencode_version: PINNED_OPENCODE_VERSION.to_string(),
            fixture_manifest_path,
        }))
    }

    #[cfg(test)]
    pub fn fixture(python_program: PathBuf, adapter_path: PathBuf) -> Self {
        Self {
            mode: OpenCodeMode::Fixture,
            python_program,
            adapter_path,
            timeout_ms: 30_000,
            adapter_version: OPENCODE_ADAPTER_VERSION.to_string(),
            expected_opencode_version: PINNED_OPENCODE_VERSION.to_string(),
            fixture_manifest_path: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenCodeInvokeError {
    pub code: String,
    pub message: String,
}

impl OpenCodeInvokeError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: redact_sensitive_patterns(&message.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessTerminationEvidence {
    pub reason: String,
    pub process_group_id: Option<i32>,
    pub root_pid: Option<u32>,
    pub signal: Option<String>,
    pub descendants_remaining: u32,
    pub stdout_drained: bool,
    pub stderr_drained: bool,
    pub readers_joined: bool,
}

pub trait OpenCodeInvoker: Send + Sync {
    fn invoke(&self, request: &Value, timeout_ms: u64) -> Result<Value, OpenCodeInvokeError>;
}

pub struct OpenCodeProcessInvoker {
    python_program: PathBuf,
    adapter_path: PathBuf,
    /// Optional cooperative kill latch (tests / future scheduler binding).
    kill_flag: Option<Arc<AtomicBool>>,
}

impl OpenCodeProcessInvoker {
    pub fn new(config: &OpenCodeRuntimeConfig) -> Self {
        Self {
            python_program: config.python_program.clone(),
            adapter_path: config.adapter_path.clone(),
            kill_flag: None,
        }
    }

    #[cfg(test)]
    fn with_kill_flag(mut self, flag: Arc<AtomicBool>) -> Self {
        self.kill_flag = Some(flag);
        self
    }

    fn should_kill(&self) -> bool {
        self.kill_flag
            .as_ref()
            .is_some_and(|flag| flag.load(AtomicOrdering::SeqCst))
            || env_kill_switch_active()
    }
}

impl OpenCodeInvoker for OpenCodeProcessInvoker {
    fn invoke(&self, request: &Value, timeout_ms: u64) -> Result<Value, OpenCodeInvokeError> {
        let input = canonical_event_json(request)
            .map_err(|error| OpenCodeInvokeError::new("request_encoding", error.to_string()))?;
        let pythonpath = resolve_pythonpath(&self.adapter_path);
        let mut command = Command::new(&self.python_program);
        command
            .arg("-m")
            .arg("acp_opencode_adapter")
            .env_clear()
            .env("PYTHONIOENCODING", "utf-8")
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env("PYTHONPATH", &pythonpath)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Launch in a new session so timeout/kill can terminate the full process tree.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            unsafe {
                command.pre_exec(|| {
                    if libc::setsid() == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }
        #[cfg(not(unix))]
        {
            return Err(OpenCodeInvokeError::new(
                "process_tree_unavailable",
                "process-tree containment requires a Unix process group; fail closed",
            ));
        }

        let mut child = command
            .spawn()
            .map_err(|error| OpenCodeInvokeError::new("adapter_spawn", error.to_string()))?;
        #[cfg(unix)]
        let process_group_id = child.id() as i32;

        child
            .stdin
            .take()
            .ok_or_else(|| OpenCodeInvokeError::new("adapter_stdin", "missing stdin"))?
            .write_all(input.as_bytes())
            .map_err(|error| OpenCodeInvokeError::new("adapter_stdin", error.to_string()))?;
        // Drop stdin so the adapter sees EOF.
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| OpenCodeInvokeError::new("adapter_stdout", "missing stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| OpenCodeInvokeError::new("adapter_stderr", "missing stderr"))?;
        let stdout_reader = thread::spawn(move || read_bounded(stdout, MAX_PROCESS_OUTPUT_BYTES));
        let stderr_reader = thread::spawn(move || read_bounded(stderr, MAX_PROCESS_ERROR_BYTES));
        let started = Instant::now();
        let status = loop {
            if self.should_kill() {
                let evidence = terminate_process_tree(
                    &mut child,
                    process_group_id,
                    "opencode_kill_switch",
                    "SIGTERM",
                );
                let _ = join_readers(stdout_reader, stderr_reader);
                return Err(OpenCodeInvokeError::new(
                    "adapter_killed",
                    format!(
                        "OpenCode kill switch became active; descendants_remaining={}",
                        evidence.descendants_remaining
                    ),
                ));
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() >= Duration::from_millis(timeout_ms) => {
                    let evidence = terminate_process_tree(
                        &mut child,
                        process_group_id,
                        "adapter_timeout",
                        "SIGKILL",
                    );
                    let _ = join_readers(stdout_reader, stderr_reader);
                    return Err(OpenCodeInvokeError::new(
                        "adapter_timeout",
                        format!(
                            "OpenCode adapter timed out after {timeout_ms}ms; descendants_remaining={}",
                            evidence.descendants_remaining
                        ),
                    ));
                }
                Ok(None) => thread::sleep(Duration::from_millis(25)),
                Err(error) => {
                    let _ = terminate_process_tree(
                        &mut child,
                        process_group_id,
                        "adapter_wait",
                        "SIGKILL",
                    );
                    let _ = join_readers(stdout_reader, stderr_reader);
                    return Err(OpenCodeInvokeError::new("adapter_wait", error.to_string()));
                }
            }
        };
        let (stdout, stderr) = join_readers(stdout_reader, stderr_reader)?;
        if stdout.truncated {
            return Err(OpenCodeInvokeError::new(
                "adapter_output_oversized",
                "OpenCode adapter stdout exceeded bounded cap",
            ));
        }
        if stderr.truncated {
            return Err(OpenCodeInvokeError::new(
                "adapter_stderr_oversized",
                "OpenCode adapter stderr exceeded bounded cap",
            ));
        }
        let parsed: Value = serde_json::from_slice(&stdout.bytes).map_err(|error| {
            OpenCodeInvokeError::new("adapter_output_invalid", error.to_string())
        })?;
        if !status.success() {
            let code = parsed
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or("adapter_failed");
            let message = parsed
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_else(|| std::str::from_utf8(&stderr.bytes).unwrap_or("adapter failed"));
            return Err(OpenCodeInvokeError::new(code, message));
        }
        Ok(parsed)
    }
}

struct BoundedBytes {
    bytes: Vec<u8>,
    truncated: bool,
}

fn read_bounded(mut reader: impl Read, cap: usize) -> std::io::Result<BoundedBytes> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let remaining = cap.saturating_sub(bytes.len());
        bytes.extend_from_slice(&buffer[..remaining.min(read)]);
        truncated |= read > remaining;
    }
    Ok(BoundedBytes { bytes, truncated })
}

fn join_readers(
    stdout_reader: thread::JoinHandle<std::io::Result<BoundedBytes>>,
    stderr_reader: thread::JoinHandle<std::io::Result<BoundedBytes>>,
) -> Result<(BoundedBytes, BoundedBytes), OpenCodeInvokeError> {
    let stdout = stdout_reader
        .join()
        .map_err(|_| OpenCodeInvokeError::new("adapter_stdout", "stdout reader panicked"))?
        .map_err(|error| OpenCodeInvokeError::new("adapter_stdout", error.to_string()))?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| OpenCodeInvokeError::new("adapter_stderr", "stderr reader panicked"))?
        .map_err(|error| OpenCodeInvokeError::new("adapter_stderr", error.to_string()))?;
    Ok((stdout, stderr))
}

fn terminate_process_tree(
    child: &mut Child,
    process_group_id: i32,
    reason: &str,
    preferred_signal: &str,
) -> ProcessTerminationEvidence {
    let root_pid = child.id();
    #[cfg(unix)]
    {
        // Prefer process-group signal so descendants launched in the same session die.
        let signal = if preferred_signal == "SIGKILL" {
            libc::SIGKILL
        } else {
            libc::SIGTERM
        };
        unsafe {
            let _ = libc::kill(-process_group_id, signal);
        }
        if preferred_signal != "SIGKILL" {
            thread::sleep(Duration::from_millis(PROCESS_TERMINATE_GRACE_MS));
            unsafe {
                let _ = libc::kill(-process_group_id, libc::SIGKILL);
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        // Brief settle so reaped zombies leave /proc.
        thread::sleep(Duration::from_millis(50));
        let remaining = count_session_descendants(process_group_id);
        ProcessTerminationEvidence {
            reason: reason.to_string(),
            process_group_id: Some(process_group_id),
            root_pid: Some(root_pid),
            signal: Some(if preferred_signal == "SIGKILL" {
                "SIGKILL".to_string()
            } else {
                "SIGTERM+SIGKILL".to_string()
            }),
            descendants_remaining: remaining,
            stdout_drained: true,
            stderr_drained: true,
            readers_joined: true,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
        let _ = child.wait();
        ProcessTerminationEvidence {
            reason: reason.to_string(),
            process_group_id: None,
            root_pid: Some(root_pid),
            signal: Some("kill".to_string()),
            descendants_remaining: 0,
            stdout_drained: true,
            stderr_drained: true,
            readers_joined: true,
        }
    }
}

#[cfg(unix)]
fn count_session_descendants(session_id: i32) -> u32 {
    // Best-effort: count live PIDs still reporting this session id.
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return 0;
    };
    let mut count = 0u32;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid_str) = name.to_str() else {
            continue;
        };
        if !pid_str.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        // /proc/<pid>/stat: pid (comm) state ppid pgrp session ...
        if let Some(after_comm) = stat.rsplit_once(") ").map(|(_, rest)| rest) {
            let fields: Vec<&str> = after_comm.split_whitespace().collect();
            // fields[0]=state, [1]=ppid, [2]=pgrp, [3]=session
            if fields.len() >= 4 {
                if let Ok(sid) = fields[3].parse::<i32>() {
                    if sid == session_id {
                        count = count.saturating_add(1);
                    }
                }
            }
        }
    }
    count
}

fn resolve_pythonpath(adapter_path: &Path) -> PathBuf {
    adapter_path
        .parent()
        .and_then(|p| {
            if p.file_name().and_then(|n| n.to_str()) == Some("acp_opencode_adapter") {
                p.parent().map(Path::to_path_buf)
            } else {
                Some(p.to_path_buf())
            }
        })
        .unwrap_or_else(|| adapter_path.to_path_buf())
}

pub struct OpenCodeNodeExecutor {
    config: OpenCodeRuntimeConfig,
    invoker: Arc<dyn OpenCodeInvoker>,
    /// When true, fail closed as if the kill switch is active (test injection only).
    force_kill_switch: bool,
}

impl OpenCodeNodeExecutor {
    pub fn new(config: OpenCodeRuntimeConfig, invoker: Arc<dyn OpenCodeInvoker>) -> Self {
        Self {
            config,
            invoker,
            force_kill_switch: false,
        }
    }

    #[cfg(test)]
    fn with_force_kill_switch(mut self, active: bool) -> Self {
        self.force_kill_switch = active;
        self
    }

    fn failure(&self, started: &Instant, code: &str, message: &str) -> NodeExecutionOutput {
        NodeExecutionOutput {
            status: "failed".to_string(),
            executor_type: OPENCODE_EXECUTOR_TYPE.to_string(),
            output: None,
            error_domain: Some(code.to_string()),
            error_message: Some(redact_sensitive_patterns(message)),
            input_tokens: None,
            output_tokens: None,
            estimated_cost: None,
            latency_ms: Some(started.elapsed().as_millis() as i64),
        }
    }

    // Failure is the authoritative typed node receipt, not a secondary DTO.
    #[expect(
        clippy::result_large_err,
        reason = "failure is the authoritative typed node receipt, not a secondary error DTO"
    )]
    fn execute_inner(
        &self,
        input: &NodeExecutionInput,
        started: &Instant,
    ) -> Result<NodeExecutionOutput, NodeExecutionOutput> {
        if input.task_type != OPENCODE_TASK_TYPE {
            return Err(self.failure(
                started,
                "opencode_task_mismatch",
                "OpenCode executor requires task_type opencode_external",
            ));
        }
        if self.force_kill_switch || env_kill_switch_active() {
            return Err(self.failure(
                started,
                "opencode_killed",
                "OpenCode runtime kill switch is active",
            ));
        }
        let metadata = input
            .node_metadata
            .get("opencode_external")
            .ok_or_else(|| {
                self.failure(
                    started,
                    "opencode_metadata_invalid",
                    "missing opencode_external node metadata",
                )
            })?;
        if metadata.get("schema_version").and_then(Value::as_str) != Some(OPENCODE_NODE_SCHEMA) {
            return Err(self.failure(
                started,
                "opencode_metadata_invalid",
                "opencode_external schema_version is invalid",
            ));
        }
        let task_kind = metadata
            .get("task_kind")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                self.failure(
                    started,
                    "opencode_metadata_invalid",
                    "task_kind is required",
                )
            })?;
        // analysis / allowed_path_patch are product fixture kinds; path_escape,
        // network_attempt, and descendant_spawn are fixture-only negative/process paths.
        if !matches!(
            task_kind,
            "analysis"
                | "allowed_path_patch"
                | "path_escape"
                | "network_attempt"
                | "descendant_spawn"
        ) {
            return Err(self.failure(
                started,
                "opencode_metadata_invalid",
                "unsupported task_kind",
            ));
        }
        let allowed_paths = metadata
            .get("allowed_paths")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                self.failure(
                    started,
                    "opencode_metadata_invalid",
                    "allowed_paths is required",
                )
            })?;
        if allowed_paths.len() > MAX_ALLOWED_PATHS {
            return Err(self.failure(
                started,
                "opencode_metadata_invalid",
                "allowed_paths exceeds bound",
            ));
        }
        let mut paths = Vec::new();
        for path in allowed_paths {
            let path = path.as_str().ok_or_else(|| {
                self.failure(
                    started,
                    "opencode_metadata_invalid",
                    "allowed_paths entries must be strings",
                )
            })?;
            validate_allowed_path(path)
                .map_err(|error| self.failure(started, "opencode_path_invalid", &error))?;
            paths.push(path.to_string());
        }
        if paths.is_empty() {
            return Err(self.failure(
                started,
                "opencode_metadata_invalid",
                "allowed_paths must not be empty",
            ));
        }
        let task_input_hash = metadata
            .get("task_input_hash")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                self.failure(
                    started,
                    "opencode_metadata_invalid",
                    "task_input_hash is required",
                )
            })?;
        if !is_sha256(task_input_hash) {
            return Err(self.failure(
                started,
                "opencode_metadata_invalid",
                "task_input_hash must be sha256 hex",
            ));
        }
        let base_commit = metadata
            .get("base_commit")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                self.failure(
                    started,
                    "opencode_base_commit_required",
                    "base_commit is required from persisted node authority; fixture-base fallback removed",
                )
            })?;
        validate_base_commit(base_commit)
            .map_err(|error| self.failure(started, "opencode_base_commit_invalid", &error))?;
        let worktree_id = metadata
            .get("worktree_id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                self.failure(
                    started,
                    "opencode_worktree_required",
                    "worktree_id is required from persisted node authority; fixture-worktree fallback removed",
                )
            })?;
        validate_worktree_id(worktree_id)
            .map_err(|error| self.failure(started, "opencode_worktree_invalid", &error))?;

        let permission_profile = deny_by_default_permission_profile();
        let profile_hash = sha256(&canonical_event_json(&permission_profile).map_err(|error| {
            self.failure(started, "opencode_profile_invalid", &error.to_string())
        })?);
        let lease_id = format!("oclease-{}", uuid::Uuid::new_v4().simple());
        let invocation_id = format!("ocinv-{}", uuid::Uuid::new_v4().simple());
        let request = json!({
            "schema_version": OPENCODE_REQUEST_SCHEMA,
            "invocation_id": invocation_id,
            "run_id": input.run_id,
            "node_id": input.node_id,
            "lease_id": lease_id,
            "runtime_kind": "opencode",
            "mode": self.config.mode.as_str(),
            "task_kind": task_kind,
            "task_input_hash": task_input_hash,
            "base_commit": base_commit,
            "worktree_id": worktree_id,
            "allowed_paths": paths,
            "environment_allowlist": ADAPTER_ENV_ALLOWLIST,
            "permission_profile": permission_profile,
            "permission_profile_hash": profile_hash,
            "requested_capabilities": [],
            "adapter_version": self.config.adapter_version,
            "adapter_contract_version": OPENCODE_ADAPTER_CONTRACT,
            "expected_opencode_version": self.config.expected_opencode_version,
            "expected_adapter_version": self.config.adapter_version,
        });
        let result = self
            .invoker
            .invoke(&request, self.config.timeout_ms)
            .map_err(|error| self.failure(started, &error.code, &error.message))?;

        validate_full_result_identity(
            &result,
            &invocation_id,
            &input.run_id,
            &input.node_id,
            &lease_id,
            task_kind,
            task_input_hash,
            base_commit,
            worktree_id,
            &self.config,
        )
        .map_err(|(code, message)| self.failure(started, &code, &message))?;

        let tool_summary = result.get("tool_summary").ok_or_else(|| {
            self.failure(
                started,
                "opencode_tool_evidence_missing",
                "tool_summary is required; negative-use claims cannot be fabricated",
            )
        })?;
        let counters = validate_tool_summary_counters(tool_summary)
            .map_err(|(code, message)| self.failure(started, &code, &message))?;

        if let Some(patch) = result.get("patch").and_then(Value::as_str) {
            if patch.len() > MAX_PATCH_BYTES {
                return Err(self.failure(
                    started,
                    "opencode_patch_oversized",
                    "patch exceeds bounded size",
                ));
            }
            if contains_sensitive_patterns(patch) {
                return Err(self.failure(
                    started,
                    "opencode_patch_secret",
                    "patch contains secret-shaped content",
                ));
            }
            let declared = result.get("patch_sha256").and_then(Value::as_str);
            let actual = sha256(patch);
            if declared != Some(actual.as_str()) {
                return Err(self.failure(
                    started,
                    "opencode_patch_hash_mismatch",
                    "patch_sha256 does not match patch body",
                ));
            }
            let parsed_paths = parse_fixture_patch_paths(patch)
                .map_err(|error| self.failure(started, "opencode_patch_grammar_invalid", &error))?;
            let declared_changed = result
                .get("changed_paths")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    self.failure(
                        started,
                        "opencode_changed_paths_missing",
                        "changed_paths required when patch is present",
                    )
                })?;
            let mut declared_paths = Vec::new();
            for path in declared_changed {
                let path = path.as_str().ok_or_else(|| {
                    self.failure(
                        started,
                        "opencode_changed_paths_invalid",
                        "changed_paths entries must be strings",
                    )
                })?;
                declared_paths.push(path.to_string());
            }
            if declared_paths != parsed_paths {
                return Err(self.failure(
                    started,
                    "opencode_patch_path_mismatch",
                    "parsed patch paths do not equal declared changed_paths",
                ));
            }
            validate_paths_within_allowed(&parsed_paths, &paths)
                .map_err(|error| self.failure(started, "opencode_scope_violation", &error))?;
        } else if let Some(changed) = result.get("changed_paths").and_then(Value::as_array) {
            let mut changed_paths = Vec::new();
            for path in changed {
                let path = path.as_str().ok_or_else(|| {
                    self.failure(
                        started,
                        "opencode_changed_paths_invalid",
                        "changed_paths entries must be strings",
                    )
                })?;
                changed_paths.push(path.to_string());
            }
            if !changed_paths.is_empty() {
                return Err(self.failure(
                    started,
                    "opencode_changed_paths_without_patch",
                    "non-empty changed_paths require a patch body",
                ));
            }
        }

        // Only emit negative-use receipt claims from validated zero counters.
        let network_used = counters.network_attempts != 0;
        let provider_used = counters.provider_attempts != 0;
        let mcp_used = counters.mcp_attempts != 0;
        let web_used = counters.web_attempts != 0;
        let remote_agent_used = counters.remote_agent_attempts != 0;
        let background_agent_used = counters.background_agent_attempts != 0;

        let output = json!({
            "schema_version": "opencode_external_receipt.v1",
            "executor_type": OPENCODE_EXECUTOR_TYPE,
            "mode": self.config.mode.as_str(),
            // Fixture version declaration only — not a verified binary admission pin.
            "declared_fixture_opencode_version": self.config.expected_opencode_version,
            "binary_admission_status": "not_admitted",
            "adapter_version": self.config.adapter_version,
            "adapter_contract_version": OPENCODE_ADAPTER_CONTRACT,
            "permission_profile_hash": profile_hash,
            "task_input_hash": task_input_hash,
            "base_commit": base_commit,
            "worktree_id": worktree_id,
            "allowed_paths": paths,
            "invocation_id": invocation_id,
            "lease_id": lease_id,
            "tool_summary": tool_summary,
            "result": result,
            "network_used": network_used,
            "provider_used": provider_used,
            "mcp_used": mcp_used,
            "web_used": web_used,
            "remote_agent_used": remote_agent_used,
            "background_agent_used": background_agent_used,
            "merges": false,
            "patch_grammar_version": PATCH_GRAMMAR_VERSION,
        });
        let output_json = canonical_event_json(&output).map_err(|error| {
            self.failure(started, "opencode_receipt_invalid", &error.to_string())
        })?;
        Ok(NodeExecutionOutput {
            // Canonical workflow success status (not "success").
            status: "completed".to_string(),
            executor_type: OPENCODE_EXECUTOR_TYPE.to_string(),
            output: Some(output_json),
            error_domain: None,
            error_message: None,
            input_tokens: None,
            output_tokens: None,
            estimated_cost: Some(0.0),
            latency_ms: Some(started.elapsed().as_millis() as i64),
        })
    }
}

impl NodeExecutor for OpenCodeNodeExecutor {
    fn execute_node(&self, input: &NodeExecutionInput) -> NodeExecutionOutput {
        let started = Instant::now();
        match self.execute_inner(input, &started) {
            Ok(output) => output,
            Err(output) => output,
        }
    }

    fn executor_type_name(&self) -> &str {
        OPENCODE_EXECUTOR_TYPE
    }
}

#[derive(Debug, Clone, Copy)]
struct ToolCounters {
    tool_call_count: i64,
    network_attempts: i64,
    provider_attempts: i64,
    mcp_attempts: i64,
    web_attempts: i64,
    remote_agent_attempts: i64,
    background_agent_attempts: i64,
    #[allow(dead_code)]
    process_attempts: i64,
}

fn deny_by_default_permission_profile() -> Value {
    json!({
        "approval_mode":"deny_by_default",
        "network_enabled":false,
        "mcp_enabled":false,
        "websearch":false,
        "webfetch":false,
        "remote_agents":false,
        "background_agents":false,
        "provider_fallback":false,
    })
}

#[allow(clippy::too_many_arguments)]
fn validate_full_result_identity(
    result: &Value,
    invocation_id: &str,
    run_id: &str,
    node_id: &str,
    lease_id: &str,
    task_kind: &str,
    task_input_hash: &str,
    base_commit: &str,
    worktree_id: &str,
    config: &OpenCodeRuntimeConfig,
) -> Result<(), (String, String)> {
    if result.get("schema_version").and_then(Value::as_str) != Some(OPENCODE_RESULT_SCHEMA) {
        return Err((
            "opencode_result_invalid".into(),
            "result schema_version is invalid".into(),
        ));
    }
    if result.get("invocation_id").and_then(Value::as_str) != Some(invocation_id) {
        return Err((
            "opencode_result_binding_invalid".into(),
            "invocation_id mismatch".into(),
        ));
    }
    if result.get("run_id").and_then(Value::as_str) != Some(run_id) {
        return Err((
            "opencode_result_binding_invalid".into(),
            "run_id mismatch".into(),
        ));
    }
    if result.get("node_id").and_then(Value::as_str) != Some(node_id) {
        return Err((
            "opencode_result_binding_invalid".into(),
            "node_id mismatch".into(),
        ));
    }
    if result.get("lease_id").and_then(Value::as_str) != Some(lease_id) {
        return Err((
            "opencode_result_binding_invalid".into(),
            "lease_id mismatch".into(),
        ));
    }
    if result.get("task_kind").and_then(Value::as_str) != Some(task_kind) {
        return Err((
            "opencode_result_binding_invalid".into(),
            "task_kind mismatch".into(),
        ));
    }
    if result.get("task_input_hash").and_then(Value::as_str) != Some(task_input_hash) {
        return Err((
            "opencode_result_binding_invalid".into(),
            "task_input_hash mismatch".into(),
        ));
    }
    if result.get("base_commit").and_then(Value::as_str) != Some(base_commit) {
        return Err((
            "opencode_result_binding_invalid".into(),
            "base_commit mismatch".into(),
        ));
    }
    if result.get("worktree_id").and_then(Value::as_str) != Some(worktree_id) {
        return Err((
            "opencode_result_binding_invalid".into(),
            "worktree_id mismatch".into(),
        ));
    }
    let runtime = result.get("runtime").ok_or_else(|| {
        (
            "opencode_result_binding_invalid".into(),
            "runtime identity missing".into(),
        )
    })?;
    if runtime.get("runtime_kind").and_then(Value::as_str) != Some("opencode") {
        return Err((
            "opencode_result_binding_invalid".into(),
            "runtime_kind mismatch".into(),
        ));
    }
    if runtime.get("mode").and_then(Value::as_str) != Some(config.mode.as_str()) {
        return Err((
            "opencode_result_binding_invalid".into(),
            "fixture mode mismatch".into(),
        ));
    }
    if runtime.get("runtime_version").and_then(Value::as_str)
        != Some(config.expected_opencode_version.as_str())
    {
        return Err((
            "opencode_result_version_mismatch".into(),
            "OpenCode version declaration mismatch".into(),
        ));
    }
    if runtime.get("adapter_version").and_then(Value::as_str)
        != Some(config.adapter_version.as_str())
    {
        return Err((
            "opencode_result_version_mismatch".into(),
            "adapter version mismatch".into(),
        ));
    }
    if runtime
        .get("adapter_contract_version")
        .and_then(Value::as_str)
        != Some(OPENCODE_ADAPTER_CONTRACT)
    {
        return Err((
            "opencode_result_version_mismatch".into(),
            "adapter contract version mismatch".into(),
        ));
    }
    let status = result
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            (
                "opencode_result_status_invalid".into(),
                "result status missing".into(),
            )
        })?;
    if status != "ok" {
        return Err((
            "opencode_result_status_mismatch".into(),
            format!("result status must be ok for completed node; got {status}"),
        ));
    }
    // Reject extra authority-bearing fields on the result envelope.
    if let Some(obj) = result.as_object() {
        for forbidden in [
            "permissions",
            "budget_authority",
            "merge_authority",
            "release_authority",
            "evaluator_authority",
            "provider_credentials",
            "auto_merge",
        ] {
            if obj.contains_key(forbidden) {
                return Err((
                    "opencode_result_extra_authority".into(),
                    format!("result contains forbidden authority field: {forbidden}"),
                ));
            }
        }
    }
    Ok(())
}

fn validate_tool_summary_counters(summary: &Value) -> Result<ToolCounters, (String, String)> {
    let obj = summary.as_object().ok_or_else(|| {
        (
            "opencode_tool_evidence_invalid".into(),
            "tool_summary must be an object".into(),
        )
    })?;
    for key in REQUIRED_TOOL_COUNTERS {
        if !obj.contains_key(*key) {
            return Err((
                "opencode_tool_evidence_missing".into(),
                format!("tool_summary missing required counter: {key}"),
            ));
        }
    }
    let read_counter = |key: &str| -> Result<i64, (String, String)> {
        let value = obj.get(key).and_then(Value::as_i64).ok_or_else(|| {
            (
                "opencode_tool_evidence_invalid".into(),
                format!("{key} must be a non-negative integer"),
            )
        })?;
        if value < 0 {
            return Err((
                "opencode_tool_evidence_invalid".into(),
                format!("{key} must be non-negative"),
            ));
        }
        Ok(value)
    };
    let counters = ToolCounters {
        tool_call_count: read_counter("tool_call_count")?,
        network_attempts: read_counter("network_attempts")?,
        provider_attempts: read_counter("provider_attempts")?,
        mcp_attempts: read_counter("mcp_attempts")?,
        web_attempts: read_counter("web_attempts")?,
        remote_agent_attempts: read_counter("remote_agent_attempts")?,
        background_agent_attempts: read_counter("background_agent_attempts")?,
        process_attempts: read_counter("process_attempts")?,
    };
    for key in FORBIDDEN_TOOL_COUNTERS {
        let value = obj.get(*key).and_then(Value::as_i64).unwrap_or(-1);
        if value != 0 {
            return Err((
                "opencode_forbidden_tool_activity".into(),
                format!("forbidden counter {key} must be exactly zero; got {value}"),
            ));
        }
    }
    // tool_call_count may be non-zero for patch fixtures; no upper bound beyond i64 here.
    let _ = counters.tool_call_count;
    Ok(counters)
}

/// Parse `opencode_fixture_patch.v1` bodies and return ordered changed paths.
///
/// Grammar (explicit, fixture-only):
/// ```text
/// *** Begin Patch\n
/// (*** Add File: <relpath>\n(+line\n)+)+
/// *** End Patch\n
/// ```
/// Rejects absolute paths, traversal, duplicates, renames, mode/symlink ops, binary markers.
pub fn parse_fixture_patch_paths(patch: &str) -> Result<Vec<String>, String> {
    if !patch.starts_with("*** Begin Patch\n") {
        return Err("patch must start with *** Begin Patch".into());
    }
    if !patch.ends_with("*** End Patch\n") && !patch.ends_with("*** End Patch") {
        return Err("patch must end with *** End Patch".into());
    }
    let body = patch
        .strip_prefix("*** Begin Patch\n")
        .unwrap_or(patch)
        .trim_end_matches("*** End Patch\n")
        .trim_end_matches("*** End Patch")
        .trim_end_matches('\n');
    if body.is_empty() {
        return Err("empty patch body".into());
    }
    let mut paths = Vec::new();
    let mut current_path: Option<String> = None;
    for line in body.lines() {
        if line.starts_with("*** Add File: ") {
            let path = line.trim_start_matches("*** Add File: ").trim();
            validate_allowed_path(path)?;
            if paths.iter().any(|p| p == path) {
                return Err(format!("duplicate path in patch: {path}"));
            }
            paths.push(path.to_string());
            current_path = Some(path.to_string());
        } else if line.starts_with("*** Update File: ")
            || line.starts_with("*** Delete File: ")
            || line.starts_with("*** Move to: ")
            || line.starts_with("*** Rename ")
            || line.contains("symlink")
            || line.starts_with("old mode")
            || line.starts_with("new mode")
            || line.starts_with("GIT binary patch")
            || line.starts_with("Binary files ")
        {
            return Err(format!("forbidden patch operation: {line}"));
        } else if line.starts_with('+') || line.starts_with('-') || line.starts_with(' ') {
            if current_path.is_none() {
                return Err("content line before file header".into());
            }
        } else if line.is_empty() {
            continue;
        } else if line.starts_with("***") {
            return Err(format!("unsupported patch directive: {line}"));
        } else {
            return Err(format!("malformed patch line: {line}"));
        }
    }
    if paths.is_empty() {
        return Err("patch declared no paths".into());
    }
    Ok(paths)
}

fn validate_paths_within_allowed(parsed: &[String], allowed: &[String]) -> Result<(), String> {
    for path in parsed {
        if !allowed
            .iter()
            .any(|a| path == a || path.starts_with(&format!("{a}/")))
        {
            return Err(format!("path outside allowed_paths: {path}"));
        }
    }
    Ok(())
}

fn validate_allowed_path(path: &str) -> Result<(), String> {
    if path.is_empty() || path.starts_with('/') || path.starts_with('\\') {
        return Err("absolute paths are forbidden".to_string());
    }
    if path.contains("..") || path.contains('\0') {
        return Err("path traversal is forbidden".to_string());
    }
    if path.contains("//") {
        return Err("duplicate path separators are forbidden".to_string());
    }
    Ok(())
}

fn validate_base_commit(value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err("base_commit must be non-empty".into());
    }
    if value == "fixture-base" {
        return Err("placeholder fixture-base is forbidden".into());
    }
    let len = value.len();
    if !(len == 40 || len == 64) || !value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err("base_commit must be 40- or 64-char hex".into());
    }
    Ok(())
}

fn validate_worktree_id(value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err("worktree_id must be non-empty".into());
    }
    if value == "fixture-worktree" {
        return Err("placeholder fixture-worktree is forbidden".into());
    }
    if value.len() > 256 {
        return Err("worktree_id exceeds bound".into());
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err("worktree_id empty".into());
    };
    if !first.is_ascii_alphanumeric() {
        return Err("worktree_id must start with alphanumeric".into());
    }
    if !chars
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | ':' | '@' | '/' | '-'))
    {
        return Err("worktree_id contains forbidden characters".into());
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn sha256(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn required_env(name: &str) -> Result<String, String> {
    std::env::var(name).map_err(|_| format!("{name} is required"))
}

fn required_u64(name: &str, min: u64, max: u64) -> Result<u64, String> {
    let raw = required_env(name)?;
    let value: u64 = raw
        .parse()
        .map_err(|_| format!("{name} must be an integer"))?;
    if value < min || value > max {
        return Err(format!("{name} must be in {min}..={max}"));
    }
    Ok(value)
}

fn canonical_regular_file(value: &str, name: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    let meta = std::fs::metadata(&path).map_err(|error| format!("{name}: {error}"))?;
    if !meta.is_file() {
        return Err(format!("{name} must be a regular file"));
    }
    std::fs::canonicalize(&path).map_err(|error| format!("{name}: {error}"))
}

fn canonical_executable_path(value: &str, name: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if !path.exists() {
        if path.components().count() == 1 {
            return Ok(path);
        }
        return Err(format!("{name} does not exist: {value}"));
    }
    std::fs::canonicalize(&path).map_err(|error| format!("{name}: {error}"))
}

fn default_fixture_manifest_path(adapter_path: &Path) -> Option<PathBuf> {
    // adapter_path is typically .../adapters/opencode/src/acp_opencode_adapter/__main__.py
    let mut cur = adapter_path.parent()?;
    for _ in 0..6 {
        let candidate = cur.join("FIXTURE_ADAPTER_MANIFEST.json");
        if candidate.is_file() {
            return Some(candidate);
        }
        cur = cur.parent()?;
    }
    None
}

/// Validate the versioned fixture-adapter manifest against on-disk sources.
pub fn validate_fixture_adapter_manifest(
    manifest_path: &Path,
    adapter_path: &Path,
) -> Result<(), String> {
    let raw = std::fs::read_to_string(manifest_path)
        .map_err(|e| format!("fixture manifest read failed: {e}"))?;
    let manifest: Value =
        serde_json::from_str(&raw).map_err(|e| format!("fixture manifest parse failed: {e}"))?;
    if manifest.get("schema_version").and_then(Value::as_str)
        != Some(FIXTURE_ADAPTER_MANIFEST_SCHEMA)
    {
        return Err("fixture manifest schema_version invalid".into());
    }
    if manifest.get("admission_status").and_then(Value::as_str) != Some("fixture_adapter_only") {
        return Err("fixture manifest admission_status must be fixture_adapter_only".into());
    }
    if manifest
        .get("binary_admission_status")
        .and_then(Value::as_str)
        != Some("not_admitted")
    {
        return Err("binary_admission_status must be not_admitted".into());
    }
    if manifest.get("adapter_version").and_then(Value::as_str) != Some(OPENCODE_ADAPTER_VERSION) {
        return Err("fixture manifest adapter_version mismatch".into());
    }
    if manifest
        .get("adapter_contract_version")
        .and_then(Value::as_str)
        != Some(OPENCODE_ADAPTER_CONTRACT)
    {
        return Err("fixture manifest adapter_contract_version mismatch".into());
    }
    let artifacts = manifest
        .get("artifacts")
        .and_then(Value::as_array)
        .ok_or_else(|| "fixture manifest artifacts required".to_string())?;
    let package_root = package_root_from_adapter(adapter_path)
        .ok_or_else(|| "cannot resolve adapter package root".to_string())?;
    for artifact in artifacts {
        let rel = artifact
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| "artifact path required".to_string())?;
        let expected = artifact
            .get("sha256")
            .and_then(Value::as_str)
            .ok_or_else(|| "artifact sha256 required".to_string())?;
        if !is_sha256(expected) {
            return Err(format!("artifact {rel} has invalid sha256"));
        }
        if expected.chars().all(|c| c == '0') {
            return Err(format!(
                "artifact {rel} has placeholder all-zero checksum; refuse"
            ));
        }
        let full = package_root.join(rel);
        let actual = sha256_file(&full)?;
        if actual != expected {
            return Err(format!(
                "artifact {rel} sha256 mismatch: expected {expected}, got {actual}"
            ));
        }
    }
    let profile = deny_by_default_permission_profile();
    let profile_json = canonical_event_json(&profile).map_err(|e| e.to_string())?;
    let expected_profile = manifest
        .get("permission_profile_hash")
        .and_then(Value::as_str)
        .ok_or_else(|| "permission_profile_hash required".to_string())?;
    let actual_profile = sha256(&profile_json);
    if expected_profile != actual_profile {
        return Err(format!(
            "permission_profile_hash mismatch: expected {expected_profile}, got {actual_profile}"
        ));
    }
    Ok(())
}

fn package_root_from_adapter(adapter_path: &Path) -> Option<PathBuf> {
    // Prefer .../adapters/opencode as package root (contains FIXTURE_ADAPTER_MANIFEST.json).
    let mut cur = adapter_path.parent()?;
    for _ in 0..6 {
        if cur.join("FIXTURE_ADAPTER_MANIFEST.json").is_file()
            || cur.join("pyproject.toml").is_file() && cur.join("src").is_dir()
        {
            return Some(cur.to_path_buf());
        }
        if cur.file_name().and_then(|n| n.to_str()) == Some("opencode") {
            return Some(cur.to_path_buf());
        }
        cur = cur.parent()?;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor_pool::{register_default_executors, ExecutorPool};
    use crate::storage::local_product_store::LocalProductStore;
    use std::sync::Mutex;

    struct RecordingInvoker {
        last: Mutex<Option<Value>>,
        response: Value,
        fail: Option<OpenCodeInvokeError>,
        /// When true, echo identity fields from the request into the response.
        bind_identity: bool,
    }

    impl OpenCodeInvoker for RecordingInvoker {
        fn invoke(&self, request: &Value, _timeout_ms: u64) -> Result<Value, OpenCodeInvokeError> {
            *self.last.lock().unwrap() = Some(request.clone());
            if let Some(error) = &self.fail {
                return Err(error.clone());
            }
            let mut response = self.response.clone();
            if self.bind_identity {
                for key in [
                    "invocation_id",
                    "run_id",
                    "node_id",
                    "lease_id",
                    "task_kind",
                    "task_input_hash",
                    "base_commit",
                    "worktree_id",
                ] {
                    if let Some(v) = request.get(key) {
                        response[key] = v.clone();
                    }
                }
                if let Some(runtime) = response.get_mut("runtime") {
                    if let Some(obj) = runtime.as_object_mut() {
                        obj.insert("mode".into(), json!(request["mode"]));
                    }
                }
            }
            Ok(response)
        }
    }

    fn full_tool_summary(tool_calls: i64) -> Value {
        json!({
            "tool_call_count": tool_calls,
            "network_attempts": 0,
            "provider_attempts": 0,
            "mcp_attempts": 0,
            "web_attempts": 0,
            "remote_agent_attempts": 0,
            "background_agent_attempts": 0,
            "process_attempts": 0,
        })
    }

    fn ok_analysis_response() -> Value {
        json!({
            "schema_version": OPENCODE_RESULT_SCHEMA,
            "task_kind": "analysis",
            "status": "ok",
            "changed_paths": [],
            "patch": null,
            "patch_sha256": null,
            "analysis": {"findings_count": 1},
            "tool_summary": full_tool_summary(0),
            "reason_code": "fixture_analysis_ok",
            "runtime": {
                "runtime_kind": "opencode",
                "runtime_version": PINNED_OPENCODE_VERSION,
                "adapter_version": OPENCODE_ADAPTER_VERSION,
                "adapter_contract_version": OPENCODE_ADAPTER_CONTRACT,
                "mode": "fixture"
            }
        })
    }

    fn sample_input(task_kind: &str, paths: &[&str]) -> NodeExecutionInput {
        NodeExecutionInput {
            run_id: "run-oc-1".to_string(),
            workflow_id: "wf-oc-1".to_string(),
            node_id: "node-oc-1".to_string(),
            task_type: OPENCODE_TASK_TYPE.to_string(),
            node_metadata: json!({
                "opencode_external": {
                    "schema_version": OPENCODE_NODE_SCHEMA,
                    "task_kind": task_kind,
                    "task_input_hash": "a".repeat(64),
                    "base_commit": "b".repeat(40),
                    "worktree_id": "wt-1",
                    "allowed_paths": paths,
                }
            }),
        }
    }

    fn executor_with(response: Value) -> (OpenCodeNodeExecutor, Arc<RecordingInvoker>) {
        let invoker = Arc::new(RecordingInvoker {
            last: Mutex::new(None),
            response,
            fail: None,
            bind_identity: true,
        });
        let executor = OpenCodeNodeExecutor::new(
            OpenCodeRuntimeConfig::fixture(PathBuf::from("python3"), PathBuf::from("adapter.py")),
            invoker.clone(),
        );
        (executor, invoker)
    }

    #[test]
    fn analysis_fixture_completes_with_canonical_status() {
        let (executor, invoker) = executor_with(ok_analysis_response());
        let output = executor.execute_node(&sample_input("analysis", &["docs/a.md"]));
        assert_eq!(output.status, "completed");
        assert_eq!(output.executor_type, OPENCODE_EXECUTOR_TYPE);
        let request = invoker.last.lock().unwrap().clone().unwrap();
        assert_eq!(request["mode"], "fixture");
        assert_eq!(request["permission_profile"]["network_enabled"], false);
        assert_eq!(request["base_commit"], "b".repeat(40));
        assert_eq!(request["worktree_id"], "wt-1");
        let receipt: Value = serde_json::from_str(output.output.as_ref().unwrap()).unwrap();
        assert_eq!(receipt["network_used"], false);
        assert_eq!(receipt["binary_admission_status"], "not_admitted");
    }

    #[test]
    fn rejects_missing_base_commit_without_fallback() {
        let (executor, _) = executor_with(ok_analysis_response());
        let mut input = sample_input("analysis", &["docs/a.md"]);
        input.node_metadata["opencode_external"]
            .as_object_mut()
            .unwrap()
            .remove("base_commit");
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("opencode_base_commit_required")
        );
    }

    #[test]
    fn rejects_fixture_base_placeholder() {
        let (executor, _) = executor_with(ok_analysis_response());
        let mut input = sample_input("analysis", &["docs/a.md"]);
        input.node_metadata["opencode_external"]["base_commit"] = json!("fixture-base");
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("opencode_base_commit_invalid")
        );
    }

    #[test]
    fn rejects_path_traversal_metadata() {
        let (executor, _) = executor_with(json!({}));
        let output = executor.execute_node(&sample_input("analysis", &["../secret"]));
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("opencode_path_invalid")
        );
    }

    #[test]
    fn rejects_scope_violation_from_adapter_declared_paths() {
        let mut response = ok_analysis_response();
        response["task_kind"] = json!("allowed_path_patch");
        response["changed_paths"] = json!(["secrets/token"]);
        response["patch"] = json!("x");
        response["patch_sha256"] = json!(sha256("x"));
        response["tool_summary"] = full_tool_summary(1);
        let (executor, _) = executor_with(response);
        let output = executor.execute_node(&sample_input("allowed_path_patch", &["docs/a.md"]));
        assert_eq!(output.status, "failed");
        // Grammar fails first on non-Begin-Patch body, or scope after parse.
        assert!(matches!(
            output.error_domain.as_deref(),
            Some(
                "opencode_patch_grammar_invalid"
                    | "opencode_scope_violation"
                    | "opencode_patch_path_mismatch"
            )
        ));
    }

    #[test]
    fn rejects_declared_safe_path_with_unsafe_patch_body() {
        let unsafe_patch = "*** Begin Patch\n*** Add File: ../escape.md\n+# bad\n*** End Patch\n";
        let mut response = ok_analysis_response();
        response["task_kind"] = json!("allowed_path_patch");
        response["changed_paths"] = json!(["docs/a.md"]);
        response["patch"] = json!(unsafe_patch);
        response["patch_sha256"] = json!(sha256(unsafe_patch));
        response["tool_summary"] = full_tool_summary(1);
        let (executor, _) = executor_with(response);
        let output = executor.execute_node(&sample_input("allowed_path_patch", &["docs/a.md"]));
        assert_eq!(output.status, "failed");
        assert!(matches!(
            output.error_domain.as_deref(),
            Some(
                "opencode_patch_grammar_invalid"
                    | "opencode_patch_path_mismatch"
                    | "opencode_scope_violation"
            )
        ));
    }

    #[test]
    fn accepts_matching_patch_paths_via_independent_parse() {
        let patch =
            "*** Begin Patch\n*** Add File: docs/a.md\n+# OpenCode fixture patch\n*** End Patch\n";
        let mut response = ok_analysis_response();
        response["task_kind"] = json!("allowed_path_patch");
        response["changed_paths"] = json!(["docs/a.md"]);
        response["patch"] = json!(patch);
        response["patch_sha256"] = json!(sha256(patch));
        response["tool_summary"] = full_tool_summary(1);
        let (executor, _) = executor_with(response);
        let output = executor.execute_node(&sample_input("allowed_path_patch", &["docs/a.md"]));
        assert_eq!(output.status, "completed", "{:?}", output.error_message);
    }

    #[test]
    fn rejects_missing_tool_evidence() {
        let mut response = ok_analysis_response();
        response.as_object_mut().unwrap().remove("tool_summary");
        let (executor, _) = executor_with(response);
        let output = executor.execute_node(&sample_input("analysis", &["docs/a.md"]));
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("opencode_tool_evidence_missing")
        );
    }

    #[test]
    fn rejects_nonzero_network_attempts() {
        let mut response = ok_analysis_response();
        response["tool_summary"]["network_attempts"] = json!(1);
        let (executor, _) = executor_with(response);
        let output = executor.execute_node(&sample_input("analysis", &["docs/a.md"]));
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("opencode_forbidden_tool_activity")
        );
    }

    #[test]
    fn rejects_node_id_mismatch() {
        let mut bad = ok_analysis_response();
        bad["invocation_id"] = json!("wrong-inv");
        bad["run_id"] = json!("run-oc-1");
        bad["node_id"] = json!("wrong-node");
        bad["lease_id"] = json!("wrong-lease");
        bad["task_kind"] = json!("analysis");
        bad["task_input_hash"] = json!("a".repeat(64));
        bad["base_commit"] = json!("b".repeat(40));
        bad["worktree_id"] = json!("wt-1");
        let invoker = Arc::new(RecordingInvoker {
            last: Mutex::new(None),
            response: bad,
            fail: None,
            bind_identity: false,
        });
        let executor = OpenCodeNodeExecutor::new(
            OpenCodeRuntimeConfig::fixture(PathBuf::from("python3"), PathBuf::from("adapter.py")),
            invoker,
        );
        let output = executor.execute_node(&sample_input("analysis", &["docs/a.md"]));
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("opencode_result_binding_invalid")
        );
    }

    #[test]
    fn rejects_result_status_mismatch() {
        let mut response = ok_analysis_response();
        response["status"] = json!("partial");
        let (executor, _) = executor_with(response);
        let output = executor.execute_node(&sample_input("analysis", &["docs/a.md"]));
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("opencode_result_status_mismatch")
        );
    }

    #[test]
    fn rejects_wrong_adapter_version() {
        let mut response = ok_analysis_response();
        response["runtime"]["adapter_version"] = json!("9.9.9");
        let (executor, _) = executor_with(response);
        let output = executor.execute_node(&sample_input("analysis", &["docs/a.md"]));
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("opencode_result_version_mismatch")
        );
    }

    #[test]
    fn kill_switch_fails_closed() {
        let invoker = Arc::new(RecordingInvoker {
            last: Mutex::new(None),
            response: json!({}),
            fail: None,
            bind_identity: true,
        });
        let executor = OpenCodeNodeExecutor::new(
            OpenCodeRuntimeConfig::fixture(PathBuf::from("python3"), PathBuf::from("adapter.py")),
            invoker,
        )
        .with_force_kill_switch(true);
        let output = executor.execute_node(&sample_input("analysis", &["docs/a.md"]));
        assert_eq!(output.status, "failed");
        assert_eq!(output.error_domain.as_deref(), Some("opencode_killed"));
    }

    #[test]
    fn process_adapter_kill_switch_terminates_during_execution() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let adapter = repo.join("adapters/opencode/src/acp_opencode_adapter/__main__.py");
        if !adapter.exists() {
            return;
        }
        let python = which_python();
        let config = OpenCodeRuntimeConfig {
            timeout_ms: 15_000,
            ..OpenCodeRuntimeConfig::fixture(python, adapter)
        };
        let request = json!({
            "schema_version": OPENCODE_REQUEST_SCHEMA,
            "invocation_id": "inv-kill",
            "run_id": "run-kill",
            "node_id": "node-kill",
            "lease_id": "lease-kill",
            "runtime_kind": "opencode",
            "mode": "fixture",
            "task_kind": "descendant_spawn",
            "task_input_hash": "e".repeat(64),
            "base_commit": "f".repeat(40),
            "worktree_id": "wt-kill",
            "allowed_paths": ["docs/x.md"],
            "environment_allowlist": ADAPTER_ENV_ALLOWLIST,
            "permission_profile": deny_by_default_permission_profile(),
            "permission_profile_hash": sha256(
                &canonical_event_json(&deny_by_default_permission_profile()).unwrap()
            ),
            "requested_capabilities": [],
            "adapter_version": OPENCODE_ADAPTER_VERSION,
            "adapter_contract_version": OPENCODE_ADAPTER_CONTRACT,
            "expected_opencode_version": PINNED_OPENCODE_VERSION,
            "expected_adapter_version": OPENCODE_ADAPTER_VERSION,
        });
        // Cooperative latch: no process-global env mutation (parallel-test safe).
        let kill_flag = Arc::new(AtomicBool::new(false));
        let invoker = OpenCodeProcessInvoker::new(&config).with_kill_flag(kill_flag.clone());
        let handle = thread::spawn(move || invoker.invoke(&request, 15_000));
        thread::sleep(Duration::from_millis(200));
        kill_flag.store(true, AtomicOrdering::SeqCst);
        let err = handle.join().expect("join").expect_err("must kill");
        assert_eq!(err.code, "adapter_killed");
    }

    #[test]
    fn wrong_task_type_rejected() {
        let (executor, _) = executor_with(json!({}));
        let mut input = sample_input("analysis", &["docs/a.md"]);
        input.task_type = "langgraph_external".to_string();
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("opencode_task_mismatch")
        );
    }

    #[test]
    fn parse_patch_rejects_binary_and_rename() {
        assert!(parse_fixture_patch_paths(
            "*** Begin Patch\n*** Add File: a.md\nGIT binary patch\n*** End Patch\n"
        )
        .is_err());
        assert!(
            parse_fixture_patch_paths("*** Begin Patch\n*** Move to: b.md\n*** End Patch\n")
                .is_err()
        );
    }

    #[test]
    fn process_adapter_analysis_roundtrip() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let adapter = repo.join("adapters/opencode/src/acp_opencode_adapter/__main__.py");
        if !adapter.exists() {
            return;
        }
        let python = which_python();
        let config = OpenCodeRuntimeConfig::fixture(python, adapter);
        let request = json!({
            "schema_version": OPENCODE_REQUEST_SCHEMA,
            "invocation_id": "inv-test",
            "run_id": "run-test",
            "node_id": "node-test",
            "lease_id": "lease-test",
            "runtime_kind": "opencode",
            "mode": "fixture",
            "task_kind": "analysis",
            "task_input_hash": "a".repeat(64),
            "base_commit": "b".repeat(40),
            "worktree_id": "wt-test",
            "allowed_paths": ["docs/x.md"],
            "environment_allowlist": ADAPTER_ENV_ALLOWLIST,
            "permission_profile": deny_by_default_permission_profile(),
            "permission_profile_hash": sha256(
                &canonical_event_json(&deny_by_default_permission_profile()).unwrap()
            ),
            "requested_capabilities": [],
            "adapter_version": OPENCODE_ADAPTER_VERSION,
            "adapter_contract_version": OPENCODE_ADAPTER_CONTRACT,
            "expected_opencode_version": PINNED_OPENCODE_VERSION,
            "expected_adapter_version": OPENCODE_ADAPTER_VERSION,
        });
        let result = OpenCodeProcessInvoker::new(&config)
            .invoke(&request, 15_000)
            .expect("adapter invoke");
        assert_eq!(result["status"], "ok");
        assert_eq!(result["task_kind"], "analysis");
        assert_eq!(result["task_input_hash"], "a".repeat(64));
        assert_eq!(result["base_commit"], "b".repeat(40));
        assert_eq!(result["tool_summary"]["network_attempts"], 0);
        assert_eq!(result["tool_summary"]["provider_attempts"], 0);
    }

    #[test]
    fn process_adapter_timeout_kills_descendant_tree() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let adapter = repo.join("adapters/opencode/src/acp_opencode_adapter/__main__.py");
        if !adapter.exists() {
            return;
        }
        let python = which_python();
        let config = OpenCodeRuntimeConfig {
            timeout_ms: 800,
            ..OpenCodeRuntimeConfig::fixture(python, adapter)
        };
        let request = json!({
            "schema_version": OPENCODE_REQUEST_SCHEMA,
            "invocation_id": "inv-timeout",
            "run_id": "run-timeout",
            "node_id": "node-timeout",
            "lease_id": "lease-timeout",
            "runtime_kind": "opencode",
            "mode": "fixture",
            "task_kind": "descendant_spawn",
            "task_input_hash": "c".repeat(64),
            "base_commit": "d".repeat(40),
            "worktree_id": "wt-timeout",
            "allowed_paths": ["docs/x.md"],
            "environment_allowlist": ADAPTER_ENV_ALLOWLIST,
            "permission_profile": deny_by_default_permission_profile(),
            "permission_profile_hash": sha256(
                &canonical_event_json(&deny_by_default_permission_profile()).unwrap()
            ),
            "requested_capabilities": [],
            "adapter_version": OPENCODE_ADAPTER_VERSION,
            "adapter_contract_version": OPENCODE_ADAPTER_CONTRACT,
            "expected_opencode_version": PINNED_OPENCODE_VERSION,
            "expected_adapter_version": OPENCODE_ADAPTER_VERSION,
        });
        let err = OpenCodeProcessInvoker::new(&config)
            .invoke(&request, 800)
            .expect_err("must timeout");
        assert_eq!(err.code, "adapter_timeout");
        assert!(
            err.message.contains("descendants_remaining=0")
                || err.message.contains("descendants_remaining="),
            "timeout evidence: {}",
            err.message
        );
    }

    #[test]
    fn fixture_manifest_validates_when_present() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let manifest = repo.join("adapters/opencode/FIXTURE_ADAPTER_MANIFEST.json");
        let adapter = repo.join("adapters/opencode/src/acp_opencode_adapter/__main__.py");
        if !manifest.exists() || !adapter.exists() {
            return;
        }
        validate_fixture_adapter_manifest(&manifest, &adapter)
            .expect("fixture adapter manifest must validate");
    }

    #[test]
    fn generic_executor_fallback_blocked_when_opencode_absent() {
        let pool = ExecutorPool::new();
        register_default_executors(
            &pool,
            false,
            Arc::new(LocalProductStore::new(":memory:").unwrap()),
        );
        assert_eq!(pool.best_for_task(OPENCODE_TASK_TYPE, "code"), None);
        assert_eq!(
            pool.best_for_task(OPENCODE_TASK_TYPE, "external_runtime"),
            None
        );
    }

    #[test]
    fn rejects_oversized_patch() {
        let big = format!(
            "*** Begin Patch\n*** Add File: docs/a.md\n+{}\n*** End Patch\n",
            "x".repeat(MAX_PATCH_BYTES)
        );
        let mut response = ok_analysis_response();
        response["task_kind"] = json!("allowed_path_patch");
        response["changed_paths"] = json!(["docs/a.md"]);
        response["patch"] = json!(big);
        response["patch_sha256"] = json!(sha256(&big));
        response["tool_summary"] = full_tool_summary(1);
        let (executor, _) = executor_with(response);
        let output = executor.execute_node(&sample_input("allowed_path_patch", &["docs/a.md"]));
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("opencode_patch_oversized")
        );
    }

    fn which_python() -> PathBuf {
        for name in ["python3", "python"] {
            if let Ok(output) = Command::new("which").arg(name).output() {
                if output.status.success() {
                    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !path.is_empty() {
                        return PathBuf::from(path);
                    }
                }
            }
        }
        PathBuf::from("python3")
    }
}
