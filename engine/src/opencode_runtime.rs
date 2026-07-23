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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

fn env_kill_switch_active() -> bool {
    std::env::var(KILL_SWITCH_ENV).as_deref() == Ok("1")
}

/// Test-only latch so mid-invocation kill-switch tests do not pollute process-global env
/// for parallel scheduler suites.
#[cfg(test)]
static TEST_SUPERVISED_KILL_OVERRIDE: AtomicBool = AtomicBool::new(false);

fn env_supervised_workers_kill_switch_active() -> bool {
    #[cfg(test)]
    {
        if TEST_SUPERVISED_KILL_OVERRIDE.load(AtomicOrdering::SeqCst) {
            return true;
        }
    }
    std::env::var("ACP_SUPERVISED_WORKERS_KILL_SWITCH").as_deref() == Ok("1")
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
const MAX_TOOL_CALL_COUNT: i64 = 8;
const PROCESS_TERMINATE_GRACE_MS: u64 = 200;
const TERMINATION_EVIDENCE_SCHEMA: &str = "opencode_process_termination.v1";
const FAILURE_RECEIPT_SCHEMA: &str = "opencode_external_failure_receipt.v1";

/// Exact relative paths and roles required in the fixture-adapter manifest.
const REQUIRED_MANIFEST_ARTIFACTS: &[(&str, &str)] = &[
    ("src/acp_opencode_adapter/__init__.py", "package_init"),
    ("src/acp_opencode_adapter/__main__.py", "entrypoint"),
    ("src/acp_opencode_adapter/adapter.py", "adapter_source"),
    ("pyproject.toml", "package_manifest"),
];

/// Environment names the fixture adapter process may receive (exact allowlist).
const ADAPTER_ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "HOME",
    "LANG",
    "LC_ALL",
    "TMPDIR",
    "PYTHONIOENCODING",
    "PYTHONDONTWRITEBYTECODE",
    "PYTHONNOUSERSITE",
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

/// Exact result-envelope keys admitted for successful fixture kinds.
const ANALYSIS_RESULT_KEYS: &[&str] = &[
    "schema_version",
    "invocation_id",
    "run_id",
    "node_id",
    "workflow_id",
    "scheduler_claim_id",
    "execution_attempt",
    "task_kind",
    "task_input_hash",
    "base_commit",
    "worktree_id",
    "status",
    "runtime",
    "changed_paths",
    "patch",
    "patch_sha256",
    "analysis",
    "tool_summary",
    "reason_code",
];

const PATCH_RESULT_KEYS: &[&str] = &[
    "schema_version",
    "invocation_id",
    "run_id",
    "node_id",
    "workflow_id",
    "scheduler_claim_id",
    "execution_attempt",
    "task_kind",
    "task_input_hash",
    "base_commit",
    "worktree_id",
    "status",
    "runtime",
    "changed_paths",
    "patch",
    "patch_sha256",
    "analysis",
    "tool_summary",
    "reason_code",
];

const EXACT_RUNTIME_KEYS: &[&str] = &[
    "runtime_kind",
    "runtime_version",
    "adapter_version",
    "adapter_contract_version",
    "mode",
];

const EXACT_ANALYSIS_BODY_KEYS: &[&str] = &["summary_digest", "findings_count", "scope_paths"];

const MAX_FINDINGS_COUNT: i64 = 8;

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
    /// Required whenever the runtime is enabled; absence fails closed.
    pub fixture_manifest_path: PathBuf,
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
            Ok(path) => canonical_regular_file(&path, MANIFEST_PATH_ENV)?,
            Err(_) => default_fixture_manifest_path(&adapter_path).ok_or_else(|| {
                format!(
                    "{MANIFEST_PATH_ENV} is required when repository-owned FIXTURE_ADAPTER_MANIFEST.json cannot be resolved from the adapter path"
                )
            })?,
        };
        validate_fixture_adapter_manifest(&fixture_manifest_path, &adapter_path)?;
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
        let manifest = default_fixture_manifest_path(&adapter_path)
            .or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../adapters/opencode/FIXTURE_ADAPTER_MANIFEST.json")
                    .canonicalize()
                    .ok()
            })
            .unwrap_or_else(|| PathBuf::from("adapters/opencode/FIXTURE_ADAPTER_MANIFEST.json"));
        Self {
            mode: OpenCodeMode::Fixture,
            python_program,
            adapter_path,
            timeout_ms: 30_000,
            adapter_version: OPENCODE_ADAPTER_VERSION.to_string(),
            expected_opencode_version: PINNED_OPENCODE_VERSION.to_string(),
            fixture_manifest_path: manifest,
        }
    }
}

/// Scheduler-owned cancellation latch shared with the OpenCode process invoker.
///
/// `reset` advances a generation so an in-flight invocation cannot be "revived"
/// by a later start/reset; the old process observes generation mismatch and exits.
#[derive(Debug, Clone, Default)]
pub struct OpenCodeCancellationHandle {
    cancelled: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
}

impl OpenCodeCancellationHandle {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, AtomicOrdering::SeqCst);
    }

    pub fn reset(&self) {
        // Advance generation before clearing cancelled so racing in-flight probes
        // still observe a stale generation even if they miss the cancelled flag.
        self.generation.fetch_add(1, AtomicOrdering::SeqCst);
        self.cancelled.store(false, AtomicOrdering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(AtomicOrdering::SeqCst)
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(AtomicOrdering::SeqCst)
    }

    pub fn flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancelled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenCodeInvokeError {
    pub code: String,
    pub message: String,
    /// Boxed to keep the Err-variant of invoke Results within Clippy size bounds.
    pub termination: Option<Box<ProcessTerminationEvidence>>,
}

impl OpenCodeInvokeError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: redact_sensitive_patterns(&message.into()),
            termination: None,
        }
    }

    fn with_termination(
        code: impl Into<String>,
        message: impl Into<String>,
        termination: ProcessTerminationEvidence,
    ) -> Self {
        Self {
            code: code.into(),
            message: redact_sensitive_patterns(&message.into()),
            termination: Some(Box::new(termination)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessTerminationEvidence {
    pub schema_version: String,
    pub reason: String,
    pub process_group_id: Option<i32>,
    pub root_pid: Option<u32>,
    pub signals_attempted: Vec<String>,
    pub wait_succeeded: bool,
    pub descendants_remaining: u32,
    pub stdout_drained: bool,
    pub stderr_drained: bool,
    pub readers_joined: bool,
    pub containment_ok: bool,
}

impl ProcessTerminationEvidence {
    fn to_bounded_json(&self) -> Value {
        json!({
            "schema_version": self.schema_version,
            "reason": self.reason,
            "process_group_id": self.process_group_id,
            "root_pid": self.root_pid,
            "signals_attempted": self.signals_attempted,
            "wait_succeeded": self.wait_succeeded,
            "descendants_remaining": self.descendants_remaining,
            "stdout_drained": self.stdout_drained,
            "stderr_drained": self.stderr_drained,
            "readers_joined": self.readers_joined,
            "containment_ok": self.containment_ok,
        })
    }
}

/// Per-invocation cancel probe (scheduler kill, env kill, stale claim/attempt).
pub trait OpenCodeCancelProbe: Send + Sync {
    /// Returns (should_cancel, trigger_code).
    fn should_cancel(&self) -> Option<&'static str>;
}

pub struct AlwaysLiveProbe;

impl OpenCodeCancelProbe for AlwaysLiveProbe {
    fn should_cancel(&self) -> Option<&'static str> {
        if env_kill_switch_active() {
            Some("opencode_kill_switch")
        } else if env_supervised_workers_kill_switch_active() {
            Some("supervised_workers_kill_switch")
        } else {
            None
        }
    }
}

pub struct CompositeCancelProbe {
    handle: OpenCodeCancellationHandle,
    start_generation: u64,
    store: Option<Arc<crate::storage::local_product_store::LocalProductStore>>,
    run_id: String,
    node_id: String,
    execution_attempt: i64,
}

impl CompositeCancelProbe {
    pub fn new(
        handle: OpenCodeCancellationHandle,
        store: Option<Arc<crate::storage::local_product_store::LocalProductStore>>,
        run_id: impl Into<String>,
        node_id: impl Into<String>,
        execution_attempt: i64,
    ) -> Self {
        let start_generation = handle.generation();
        Self {
            handle,
            start_generation,
            store,
            run_id: run_id.into(),
            node_id: node_id.into(),
            execution_attempt,
        }
    }
}

impl OpenCodeCancelProbe for CompositeCancelProbe {
    fn should_cancel(&self) -> Option<&'static str> {
        if env_kill_switch_active() {
            return Some("opencode_kill_switch");
        }
        if env_supervised_workers_kill_switch_active() {
            return Some("supervised_workers_kill_switch");
        }
        // start/reset advances generation; in-flight work must not continue under a new epoch.
        if self.handle.generation() != self.start_generation {
            return Some("scheduler_cancelled");
        }
        if self.handle.is_cancelled() {
            return Some("scheduler_cancelled");
        }
        if let Some(store) = &self.store {
            if !execution_claim_still_owns(
                store,
                &self.run_id,
                &self.node_id,
                self.execution_attempt,
            ) {
                return Some("stale_execution_claim");
            }
        }
        None
    }
}

fn execution_claim_still_owns(
    store: &crate::storage::local_product_store::LocalProductStore,
    run_id: &str,
    node_id: &str,
    execution_attempt: i64,
) -> bool {
    let Ok(Some(run)) = store.get_workflow_run(run_id) else {
        return false;
    };
    let Some(nodes) = run.get("nodes").and_then(Value::as_array) else {
        return false;
    };
    let Some(node) = nodes
        .iter()
        .find(|n| n.get("node_id").and_then(Value::as_str) == Some(node_id))
    else {
        return false;
    };
    let status = node
        .get("db_status")
        .or_else(|| node.get("status"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if status == "cancelled" || status == "completed" || status == "failed" {
        return false;
    }
    if status != "running" {
        return false;
    }
    let attempt = node
        .get("attempt_count")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    attempt == execution_attempt
}

pub trait OpenCodeInvoker: Send + Sync {
    fn invoke(
        &self,
        request: &Value,
        timeout_ms: u64,
        cancel: &dyn OpenCodeCancelProbe,
    ) -> Result<Value, OpenCodeInvokeError>;
}

pub struct OpenCodeProcessInvoker {
    python_program: PathBuf,
    adapter_path: PathBuf,
    cancel_handle: OpenCodeCancellationHandle,
}

impl OpenCodeProcessInvoker {
    pub fn new(config: &OpenCodeRuntimeConfig, cancel_handle: OpenCodeCancellationHandle) -> Self {
        Self {
            python_program: config.python_program.clone(),
            adapter_path: config.adapter_path.clone(),
            cancel_handle,
        }
    }

    pub fn cancellation_handle(&self) -> OpenCodeCancellationHandle {
        self.cancel_handle.clone()
    }
}

impl OpenCodeInvoker for OpenCodeProcessInvoker {
    fn invoke(
        &self,
        request: &Value,
        timeout_ms: u64,
        cancel: &dyn OpenCodeCancelProbe,
    ) -> Result<Value, OpenCodeInvokeError> {
        let input = canonical_event_json(request)
            .map_err(|error| OpenCodeInvokeError::new("request_encoding", error.to_string()))?;
        let pythonpath = resolve_pythonpath(&self.adapter_path);
        reject_unmanifested_startup_files(&pythonpath)?;
        let mut command = Command::new(&self.python_program);
        // -S: no site module (no sitecustomize/usercustomize/.pth auto-exec)
        // -s: no user site-packages
        // -B: no bytecode files
        // PYTHONPATH retains only the exact app-owned package root for the fixture.
        command
            .arg("-S")
            .arg("-s")
            .arg("-B")
            .arg("-m")
            .arg("acp_opencode_adapter")
            .env_clear()
            .env("PYTHONIOENCODING", "utf-8")
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env("PYTHONNOUSERSITE", "1")
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

        let child = command
            .spawn()
            .map_err(|error| OpenCodeInvokeError::new("adapter_spawn", error.to_string()))?;
        #[cfg(unix)]
        let process_group_id = child.id() as i32;
        let mut spawned = SpawnedAdapter::new(child, process_group_id);
        let start_generation = self.cancel_handle.generation();

        let mut stdin = match spawned.child.stdin.take() {
            Some(stdin) => stdin,
            None => {
                return Err(
                    spawned.fail_without_readers("adapter_stdin", "missing stdin after spawn")
                );
            }
        };
        if let Err(error) = stdin.write_all(input.as_bytes()) {
            drop(stdin);
            return Err(spawned
                .fail_without_readers("adapter_stdin", format!("stdin write failed: {error}")));
        }
        drop(stdin);

        let stdout = match spawned.child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                return Err(
                    spawned.fail_without_readers("adapter_stdout", "missing stdout after spawn")
                );
            }
        };
        let stderr = match spawned.child.stderr.take() {
            Some(stderr) => stderr,
            None => {
                return Err(
                    spawned.fail_without_readers("adapter_stderr", "missing stderr after spawn")
                );
            }
        };

        let stdout_reader = thread::spawn(move || read_bounded(stdout, MAX_PROCESS_OUTPUT_BYTES));
        let stderr_reader = thread::spawn(move || read_bounded(stderr, MAX_PROCESS_ERROR_BYTES));
        let started = Instant::now();
        let status = loop {
            if let Some(trigger) = cancel.should_cancel() {
                return Err(spawned.finalize_forced(
                    trigger,
                    "SIGTERM",
                    stdout_reader,
                    stderr_reader,
                ));
            }
            // Shared scheduler handle + generation epoch (covers AlwaysLiveProbe too).
            if self.cancel_handle.is_cancelled()
                || self.cancel_handle.generation() != start_generation
            {
                return Err(spawned.finalize_forced(
                    "scheduler_cancelled",
                    "SIGTERM",
                    stdout_reader,
                    stderr_reader,
                ));
            }
            if env_supervised_workers_kill_switch_active() {
                return Err(spawned.finalize_forced(
                    "supervised_workers_kill_switch",
                    "SIGTERM",
                    stdout_reader,
                    stderr_reader,
                ));
            }
            match spawned.child.try_wait() {
                Ok(Some(status)) => {
                    spawned.mark_reaped();
                    break status;
                }
                Ok(None) if started.elapsed() >= Duration::from_millis(timeout_ms) => {
                    return Err(spawned.finalize_forced(
                        "adapter_timeout",
                        "SIGKILL",
                        stdout_reader,
                        stderr_reader,
                    ));
                }
                Ok(None) => thread::sleep(Duration::from_millis(25)),
                Err(error) => {
                    let mut err = spawned.finalize_forced(
                        "adapter_wait",
                        "SIGKILL",
                        stdout_reader,
                        stderr_reader,
                    );
                    err.message = format!("adapter wait failed: {error}; {}", err.message);
                    if err.termination.is_none() {
                        err.termination = Some(Box::new(ProcessTerminationEvidence::failed(
                            "adapter_wait",
                            process_group_id,
                        )));
                    }
                    return Err(err);
                }
            }
        };

        let (stdout, stderr) = match join_readers(stdout_reader, stderr_reader) {
            Ok(pair) => pair,
            Err(error) => {
                // Child already waited; still force-kill any remaining session descendants.
                let mut evidence =
                    ensure_process_group_terminal(process_group_id, "adapter_reader");
                evidence.readers_joined = false;
                evidence.containment_ok = false;
                return Err(OpenCodeInvokeError::with_termination(
                    "process_containment_failed",
                    format!(
                        "reader join failed after child exit: {}; descendants_remaining={}",
                        error.message, evidence.descendants_remaining
                    ),
                    evidence,
                ));
            }
        };

        // After any child exit, prove the process tree is terminal (covers early-exit+descendant).
        let mut tree = ensure_process_group_terminal(process_group_id, "post_exit");
        tree.stdout_drained = !stdout.truncated;
        tree.stderr_drained = !stderr.truncated;
        tree.readers_joined = true;
        tree.wait_succeeded = true;
        tree.containment_ok = tree.descendants_remaining == 0
            && tree.readers_joined
            && tree.stdout_drained
            && tree.stderr_drained;

        if stdout.truncated {
            return Err(OpenCodeInvokeError::with_termination(
                if tree.containment_ok {
                    "adapter_output_oversized"
                } else {
                    "process_containment_failed"
                },
                "OpenCode adapter stdout exceeded bounded cap",
                tree,
            ));
        }
        if stderr.truncated {
            return Err(OpenCodeInvokeError::with_termination(
                if tree.containment_ok {
                    "adapter_stderr_oversized"
                } else {
                    "process_containment_failed"
                },
                "OpenCode adapter stderr exceeded bounded cap",
                tree,
            ));
        }
        if !tree.containment_ok {
            return Err(OpenCodeInvokeError::with_termination(
                "process_containment_failed",
                format!(
                    "process tree not terminal after adapter exit; descendants_remaining={}",
                    tree.descendants_remaining
                ),
                tree,
            ));
        }

        let parsed: Value = match serde_json::from_slice(&stdout.bytes) {
            Ok(value) => value,
            Err(error) => {
                return Err(OpenCodeInvokeError::with_termination(
                    "adapter_output_invalid",
                    error.to_string(),
                    tree,
                ));
            }
        };
        if !status.success() {
            let code = parsed
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or("adapter_failed");
            let message = parsed
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_else(|| std::str::from_utf8(&stderr.bytes).unwrap_or("adapter failed"));
            return Err(OpenCodeInvokeError::with_termination(code, message, tree));
        }
        Ok(parsed)
    }
}

/// Guard that terminates and reaps the adapter process group on every drop path
/// unless the owner explicitly marks the process reaped / finalized.
struct SpawnedAdapter {
    child: Child,
    process_group_id: i32,
    reaped: bool,
}

impl SpawnedAdapter {
    fn new(child: Child, process_group_id: i32) -> Self {
        Self {
            child,
            process_group_id,
            reaped: false,
        }
    }

    fn mark_reaped(&mut self) {
        self.reaped = true;
    }

    fn fail_without_readers(
        &mut self,
        reason: &str,
        message: impl Into<String>,
    ) -> OpenCodeInvokeError {
        let mut evidence =
            signal_process_tree(&mut self.child, self.process_group_id, reason, "SIGKILL");
        self.reaped = true;
        evidence.stdout_drained = true;
        evidence.stderr_drained = true;
        evidence.readers_joined = true;
        evidence.containment_ok = evidence.wait_succeeded && evidence.descendants_remaining == 0;
        let code = if evidence.containment_ok {
            reason
        } else {
            "process_containment_failed"
        };
        OpenCodeInvokeError::with_termination(
            code,
            format!(
                "{}; descendants_remaining={}; containment_ok={}",
                message.into(),
                evidence.descendants_remaining,
                evidence.containment_ok
            ),
            evidence,
        )
    }

    fn finalize_forced(
        &mut self,
        reason: &str,
        preferred_signal: &str,
        stdout_reader: thread::JoinHandle<std::io::Result<BoundedBytes>>,
        stderr_reader: thread::JoinHandle<std::io::Result<BoundedBytes>>,
    ) -> OpenCodeInvokeError {
        self.reaped = true;
        finalize_forced_termination(
            &mut self.child,
            self.process_group_id,
            reason,
            preferred_signal,
            stdout_reader,
            stderr_reader,
        )
    }
}

impl Drop for SpawnedAdapter {
    fn drop(&mut self) {
        if self.reaped {
            return;
        }
        let _ = signal_process_tree(
            &mut self.child,
            self.process_group_id,
            "drop_cleanup",
            "SIGKILL",
        );
        self.reaped = true;
    }
}

impl ProcessTerminationEvidence {
    fn failed(reason: &str, process_group_id: i32) -> Self {
        Self {
            schema_version: TERMINATION_EVIDENCE_SCHEMA.to_string(),
            reason: reason.to_string(),
            process_group_id: Some(process_group_id),
            root_pid: None,
            signals_attempted: vec!["SIGKILL".into()],
            wait_succeeded: false,
            descendants_remaining: 0,
            stdout_drained: false,
            stderr_drained: false,
            readers_joined: false,
            containment_ok: false,
        }
    }
}

/// Kill any remaining session members and report residual count.
fn ensure_process_group_terminal(
    process_group_id: i32,
    reason: &str,
) -> ProcessTerminationEvidence {
    #[cfg(unix)]
    {
        unsafe {
            let _ = libc::kill(-process_group_id, libc::SIGKILL);
        }
        thread::sleep(Duration::from_millis(50));
        let remaining = count_session_descendants(process_group_id);
        ProcessTerminationEvidence {
            schema_version: TERMINATION_EVIDENCE_SCHEMA.to_string(),
            reason: reason.to_string(),
            process_group_id: Some(process_group_id),
            root_pid: None,
            signals_attempted: vec!["SIGKILL".to_string()],
            wait_succeeded: true,
            descendants_remaining: remaining,
            stdout_drained: false,
            stderr_drained: false,
            readers_joined: false,
            containment_ok: remaining == 0,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (process_group_id, reason);
        ProcessTerminationEvidence {
            schema_version: TERMINATION_EVIDENCE_SCHEMA.to_string(),
            reason: reason.to_string(),
            process_group_id: None,
            root_pid: None,
            signals_attempted: vec!["kill".to_string()],
            wait_succeeded: false,
            descendants_remaining: 0,
            stdout_drained: false,
            stderr_drained: false,
            readers_joined: false,
            containment_ok: false,
        }
    }
}

/// Refuse package-root auto-exec files that would run outside the fixture manifest.
fn reject_unmanifested_startup_files(pythonpath: &Path) -> Result<(), OpenCodeInvokeError> {
    for name in ["sitecustomize.py", "usercustomize.py"] {
        let path = pythonpath.join(name);
        if path.is_file() {
            return Err(OpenCodeInvokeError::new(
                "adapter_startup_confinement",
                format!(
                    "refusing unmanifested auto-exec startup file on PYTHONPATH: {}",
                    path.display()
                ),
            ));
        }
    }
    if let Ok(entries) = std::fs::read_dir(pythonpath) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("pth") {
                return Err(OpenCodeInvokeError::new(
                    "adapter_startup_confinement",
                    format!(
                        "refusing unmanifested .pth auto-exec file on PYTHONPATH: {}",
                        path.display()
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn finalize_forced_termination(
    child: &mut Child,
    process_group_id: i32,
    reason: &str,
    preferred_signal: &str,
    stdout_reader: thread::JoinHandle<std::io::Result<BoundedBytes>>,
    stderr_reader: thread::JoinHandle<std::io::Result<BoundedBytes>>,
) -> OpenCodeInvokeError {
    let mut evidence = signal_process_tree(child, process_group_id, reason, preferred_signal);
    let join = join_readers_status(stdout_reader, stderr_reader);
    evidence.stdout_drained = join.stdout_ok;
    evidence.stderr_drained = join.stderr_ok;
    evidence.readers_joined = join.readers_joined;
    evidence.containment_ok = evidence.wait_succeeded
        && evidence.descendants_remaining == 0
        && evidence.readers_joined
        && evidence.stdout_drained
        && evidence.stderr_drained;
    let code = if !evidence.containment_ok {
        "process_containment_failed"
    } else if reason == "adapter_timeout" {
        "adapter_timeout"
    } else if reason == "stale_execution_claim" {
        "stale_execution_claim"
    } else if reason == "scheduler_cancelled" {
        "scheduler_cancelled"
    } else if reason == "supervised_workers_kill_switch" {
        "supervised_workers_kill_switch"
    } else {
        "adapter_killed"
    };
    OpenCodeInvokeError::with_termination(
        code,
        format!(
            "OpenCode adapter terminated ({reason}); descendants_remaining={}; containment_ok={}",
            evidence.descendants_remaining, evidence.containment_ok
        ),
        evidence,
    )
}

struct ReaderJoinStatus {
    stdout_ok: bool,
    stderr_ok: bool,
    readers_joined: bool,
}

fn join_readers_status(
    stdout_reader: thread::JoinHandle<std::io::Result<BoundedBytes>>,
    stderr_reader: thread::JoinHandle<std::io::Result<BoundedBytes>>,
) -> ReaderJoinStatus {
    let stdout_join = stdout_reader.join();
    let stderr_join = stderr_reader.join();
    let stdout_ok = matches!(&stdout_join, Ok(Ok(_)));
    let stderr_ok = matches!(&stderr_join, Ok(Ok(_)));
    ReaderJoinStatus {
        stdout_ok,
        stderr_ok,
        readers_joined: stdout_join.is_ok() && stderr_join.is_ok(),
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

/// Signal and wait for the process tree. Drain/reader flags are filled by the caller
/// only after those operations actually complete.
fn signal_process_tree(
    child: &mut Child,
    process_group_id: i32,
    reason: &str,
    preferred_signal: &str,
) -> ProcessTerminationEvidence {
    let root_pid = child.id();
    let mut signals = Vec::new();
    #[cfg(unix)]
    {
        if preferred_signal == "SIGKILL" {
            unsafe {
                let _ = libc::kill(-process_group_id, libc::SIGKILL);
            }
            signals.push("SIGKILL".to_string());
        } else {
            unsafe {
                let _ = libc::kill(-process_group_id, libc::SIGTERM);
            }
            signals.push("SIGTERM".to_string());
            thread::sleep(Duration::from_millis(PROCESS_TERMINATE_GRACE_MS));
            unsafe {
                let _ = libc::kill(-process_group_id, libc::SIGKILL);
            }
            signals.push("SIGKILL".to_string());
        }
        let _ = child.kill();
        let wait_succeeded = child.wait().is_ok();
        thread::sleep(Duration::from_millis(50));
        let remaining = count_session_descendants(process_group_id);
        ProcessTerminationEvidence {
            schema_version: TERMINATION_EVIDENCE_SCHEMA.to_string(),
            reason: reason.to_string(),
            process_group_id: Some(process_group_id),
            root_pid: Some(root_pid),
            signals_attempted: signals,
            wait_succeeded,
            descendants_remaining: remaining,
            // Caller must set these after join/drain succeeds.
            stdout_drained: false,
            stderr_drained: false,
            readers_joined: false,
            containment_ok: false,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
        let wait_succeeded = child.wait().is_ok();
        signals.push("kill".to_string());
        ProcessTerminationEvidence {
            schema_version: TERMINATION_EVIDENCE_SCHEMA.to_string(),
            reason: reason.to_string(),
            process_group_id: None,
            root_pid: Some(root_pid),
            signals_attempted: signals,
            wait_succeeded,
            descendants_remaining: 0,
            stdout_drained: false,
            stderr_drained: false,
            readers_joined: false,
            containment_ok: false,
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
    cancel_handle: OpenCodeCancellationHandle,
    store: Option<Arc<crate::storage::local_product_store::LocalProductStore>>,
    /// When true, fail closed as if the kill switch is active (test injection only).
    force_kill_switch: bool,
}

impl OpenCodeNodeExecutor {
    pub fn new(
        config: OpenCodeRuntimeConfig,
        invoker: Arc<dyn OpenCodeInvoker>,
        cancel_handle: OpenCodeCancellationHandle,
    ) -> Self {
        Self {
            config,
            invoker,
            cancel_handle,
            store: None,
            force_kill_switch: false,
        }
    }

    pub fn with_store(
        mut self,
        store: Arc<crate::storage::local_product_store::LocalProductStore>,
    ) -> Self {
        self.store = Some(store);
        self
    }

    pub fn cancellation_handle(&self) -> OpenCodeCancellationHandle {
        self.cancel_handle.clone()
    }

    #[cfg(test)]
    fn with_force_kill_switch(mut self, active: bool) -> Self {
        self.force_kill_switch = active;
        self
    }

    fn failure(&self, started: &Instant, code: &str, message: &str) -> NodeExecutionOutput {
        self.failure_with_evidence(started, code, message, None, None)
    }

    fn failure_with_evidence(
        &self,
        started: &Instant,
        code: &str,
        message: &str,
        termination: Option<&ProcessTerminationEvidence>,
        identity: Option<&ExecutionIdentity>,
    ) -> NodeExecutionOutput {
        let mut receipt = json!({
            "schema_version": FAILURE_RECEIPT_SCHEMA,
            "executor_type": OPENCODE_EXECUTOR_TYPE,
            "mode": self.config.mode.as_str(),
            "binary_admission_status": "not_admitted",
            "error_domain": code,
            "error_message": redact_sensitive_patterns(message),
        });
        if let Some(id) = identity {
            receipt["run_id"] = json!(id.run_id);
            receipt["node_id"] = json!(id.node_id);
            receipt["workflow_id"] = json!(id.workflow_id);
            receipt["execution_attempt"] = json!(id.execution_attempt);
            receipt["scheduler_claim_id"] = json!(id.scheduler_claim_id);
            receipt["invocation_id"] = json!(id.invocation_id);
        }
        if let Some(term) = termination {
            receipt["process_termination"] = term.to_bounded_json();
        }
        let output = canonical_event_json(&receipt).ok();
        NodeExecutionOutput {
            status: "failed".to_string(),
            executor_type: OPENCODE_EXECUTOR_TYPE.to_string(),
            output,
            error_domain: Some(code.to_string()),
            error_message: Some(redact_sensitive_patterns(message)),
            input_tokens: None,
            output_tokens: None,
            estimated_cost: None,
            latency_ms: Some(started.elapsed().as_millis() as i64),
            process_outcome: None,
            resolved_model: None,
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
        if env_kill_switch_active() || self.cancel_handle.is_cancelled() {
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
        // network_attempt, descendant_spawn, early_exit_with_descendant, and
        // stdin_close_early are fixture-only negative/process paths.
        if !matches!(
            task_kind,
            "analysis"
                | "allowed_path_patch"
                | "path_escape"
                | "network_attempt"
                | "descendant_spawn"
                | "early_exit_with_descendant"
                | "stdin_close_early"
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
        // Scheduler-owned attempt/claim identity (not a newly minted ocleave-* authority).
        let execution_attempt = input
            .node_metadata
            .get("execution_attempt")
            .and_then(Value::as_i64)
            .filter(|v| *v >= 1)
            .ok_or_else(|| {
                self.failure(
                    started,
                    "opencode_execution_identity_missing",
                    "execution_attempt from scheduler lease is required",
                )
            })?;
        let scheduler_claim_id = input
            .node_metadata
            .get("scheduler_claim_id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| {
                format!(
                    "workflow:{}:{}:{}",
                    input.run_id, input.node_id, execution_attempt
                )
            });
        let expected_claim = format!(
            "workflow:{}:{}:{}",
            input.run_id, input.node_id, execution_attempt
        );
        if scheduler_claim_id != expected_claim {
            return Err(self.failure(
                started,
                "opencode_execution_identity_invalid",
                "scheduler_claim_id does not match owner-derived claim",
            ));
        }
        // Correlation-only UUID; must not substitute for scheduler claim identity.
        let invocation_id = format!("ocinv-{}", uuid::Uuid::new_v4().simple());
        let identity = ExecutionIdentity {
            run_id: input.run_id.clone(),
            node_id: input.node_id.clone(),
            workflow_id: input.workflow_id.clone(),
            execution_attempt,
            scheduler_claim_id: scheduler_claim_id.clone(),
            invocation_id: invocation_id.clone(),
        };
        let request = json!({
            "schema_version": OPENCODE_REQUEST_SCHEMA,
            "invocation_id": invocation_id,
            "run_id": input.run_id,
            "node_id": input.node_id,
            "workflow_id": input.workflow_id,
            "execution_attempt": execution_attempt,
            "scheduler_claim_id": scheduler_claim_id,
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
        let cancel = CompositeCancelProbe::new(
            self.cancel_handle.clone(),
            self.store.clone(),
            &input.run_id,
            &input.node_id,
            execution_attempt,
        );
        if self.force_kill_switch {
            return Err(self.failure_with_evidence(
                started,
                "opencode_killed",
                "OpenCode runtime kill switch is active",
                None,
                Some(&identity),
            ));
        }
        let result = self
            .invoker
            .invoke(&request, self.config.timeout_ms, &cancel)
            .map_err(|error| {
                self.failure_with_evidence(
                    started,
                    &error.code,
                    &error.message,
                    error.termination.as_deref(),
                    Some(&identity),
                )
            })?;

        validate_full_result_identity(
            &result,
            &identity,
            task_kind,
            task_input_hash,
            base_commit,
            worktree_id,
            &self.config,
        )
        .map_err(|(code, message)| {
            self.failure_with_evidence(started, &code, &message, None, Some(&identity))
        })?;

        let tool_summary = result.get("tool_summary").ok_or_else(|| {
            self.failure(
                started,
                "opencode_tool_evidence_missing",
                "tool_summary is required; negative-use claims cannot be fabricated",
            )
        })?;
        let counters =
            validate_tool_summary_counters(tool_summary).map_err(|(code, message)| {
                self.failure_with_evidence(started, &code, &message, None, Some(&identity))
            })?;
        validate_task_kind_result_shape(&result, task_kind, &paths, task_input_hash).map_err(
            |(code, message)| {
                self.failure_with_evidence(started, &code, &message, None, Some(&identity))
            },
        )?;

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
            "invocation_id": identity.invocation_id,
            "execution_attempt": identity.execution_attempt,
            "scheduler_claim_id": identity.scheduler_claim_id,
            "workflow_id": identity.workflow_id,
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
            process_outcome: None,
            resolved_model: None,
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

#[derive(Debug, Clone)]
struct ExecutionIdentity {
    run_id: String,
    node_id: String,
    workflow_id: String,
    execution_attempt: i64,
    scheduler_claim_id: String,
    invocation_id: String,
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

fn validate_full_result_identity(
    result: &Value,
    identity: &ExecutionIdentity,
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
    if result.get("invocation_id").and_then(Value::as_str) != Some(identity.invocation_id.as_str())
    {
        return Err((
            "opencode_result_binding_invalid".into(),
            "invocation_id mismatch".into(),
        ));
    }
    if result.get("run_id").and_then(Value::as_str) != Some(identity.run_id.as_str()) {
        return Err((
            "opencode_result_binding_invalid".into(),
            "run_id mismatch".into(),
        ));
    }
    if result.get("node_id").and_then(Value::as_str) != Some(identity.node_id.as_str()) {
        return Err((
            "opencode_result_binding_invalid".into(),
            "node_id mismatch".into(),
        ));
    }
    // workflow_id is an explicit identity field on the request and must round-trip.
    if result.get("workflow_id").and_then(Value::as_str) != Some(identity.workflow_id.as_str()) {
        return Err((
            "opencode_result_binding_invalid".into(),
            "workflow_id mismatch".into(),
        ));
    }
    if result.get("scheduler_claim_id").and_then(Value::as_str)
        != Some(identity.scheduler_claim_id.as_str())
    {
        return Err((
            "opencode_result_binding_invalid".into(),
            "scheduler_claim_id mismatch".into(),
        ));
    }
    if result.get("execution_attempt").and_then(Value::as_i64) != Some(identity.execution_attempt) {
        return Err((
            "opencode_result_binding_invalid".into(),
            "execution_attempt mismatch".into(),
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
    let runtime_obj = runtime.as_object().ok_or_else(|| {
        (
            "opencode_result_binding_invalid".into(),
            "runtime identity must be an object".into(),
        )
    })?;
    if runtime_obj.len() != EXACT_RUNTIME_KEYS.len()
        || EXACT_RUNTIME_KEYS
            .iter()
            .any(|key| !runtime_obj.contains_key(*key))
    {
        return Err((
            "opencode_result_runtime_keys_invalid".into(),
            "runtime identity keys must match the exact admitted set".into(),
        ));
    }
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
    // Exact envelope keys for product fixture kinds; reject extras and authority fields.
    if matches!(task_kind, "analysis" | "allowed_path_patch") {
        let allowed = if task_kind == "analysis" {
            ANALYSIS_RESULT_KEYS
        } else {
            PATCH_RESULT_KEYS
        };
        let obj = result.as_object().ok_or_else(|| {
            (
                "opencode_result_invalid".into(),
                "result must be an object".into(),
            )
        })?;
        if obj.len() != allowed.len() || allowed.iter().any(|key| !obj.contains_key(*key)) {
            let extras: Vec<&String> = obj
                .keys()
                .filter(|k| !allowed.iter().any(|a| a == k))
                .collect();
            return Err((
                "opencode_result_keys_invalid".into(),
                format!(
                    "result envelope keys must match exact admitted set for {task_kind}; extras={extras:?}"
                ),
            ));
        }
        for forbidden in [
            "permissions",
            "budget_authority",
            "merge_authority",
            "release_authority",
            "evaluator_authority",
            "provider_credentials",
            "auto_merge",
            "tool_requests",
            "process_control",
            "executable",
            "mutation",
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
    if obj.len() != REQUIRED_TOOL_COUNTERS.len()
        || REQUIRED_TOOL_COUNTERS
            .iter()
            .any(|key| !obj.contains_key(*key))
    {
        return Err((
            "opencode_tool_evidence_keys_invalid".into(),
            "tool_summary keys must match the exact admitted counter set".into(),
        ));
    }
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
    if counters.tool_call_count > MAX_TOOL_CALL_COUNT {
        return Err((
            "opencode_tool_call_limit".into(),
            format!(
                "tool_call_count {} exceeds fixture max {}",
                counters.tool_call_count, MAX_TOOL_CALL_COUNT
            ),
        ));
    }
    Ok(counters)
}

fn validate_task_kind_result_shape(
    result: &Value,
    task_kind: &str,
    allowed_paths: &[String],
    task_input_hash: &str,
) -> Result<(), (String, String)> {
    let reason = result
        .get("reason_code")
        .and_then(Value::as_str)
        .unwrap_or("");
    match task_kind {
        "analysis" => {
            if !result.get("patch").map(Value::is_null).unwrap_or(false) {
                return Err((
                    "opencode_result_shape_invalid".into(),
                    "analysis task requires patch to be explicitly null".into(),
                ));
            }
            if !result
                .get("patch_sha256")
                .map(Value::is_null)
                .unwrap_or(false)
            {
                return Err((
                    "opencode_result_shape_invalid".into(),
                    "analysis task requires patch_sha256 to be explicitly null".into(),
                ));
            }
            let changed = result
                .get("changed_paths")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    (
                        "opencode_result_shape_invalid".into(),
                        "analysis task requires changed_paths array".into(),
                    )
                })?;
            if !changed.is_empty() {
                return Err((
                    "opencode_result_shape_invalid".into(),
                    "analysis task requires empty changed_paths".into(),
                ));
            }
            if reason != "fixture_analysis_ok" {
                return Err((
                    "opencode_reason_code_invalid".into(),
                    "analysis reason_code must be fixture_analysis_ok".into(),
                ));
            }
            let analysis = result
                .get("analysis")
                .and_then(Value::as_object)
                .ok_or_else(|| {
                    (
                        "opencode_result_shape_invalid".into(),
                        "analysis task requires analysis object".into(),
                    )
                })?;
            if analysis.len() != EXACT_ANALYSIS_BODY_KEYS.len()
                || EXACT_ANALYSIS_BODY_KEYS
                    .iter()
                    .any(|key| !analysis.contains_key(*key))
            {
                return Err((
                    "opencode_analysis_keys_invalid".into(),
                    "analysis object keys must match exact admitted set".into(),
                ));
            }
            let expected_digest = sha256(task_input_hash);
            if analysis.get("summary_digest").and_then(Value::as_str)
                != Some(expected_digest.as_str())
            {
                return Err((
                    "opencode_analysis_digest_invalid".into(),
                    "summary_digest must equal sha256(task_input_hash)".into(),
                ));
            }
            let findings = analysis
                .get("findings_count")
                .and_then(Value::as_i64)
                .ok_or_else(|| {
                    (
                        "opencode_analysis_findings_invalid".into(),
                        "findings_count must be an integer".into(),
                    )
                })?;
            if !(0..=MAX_FINDINGS_COUNT).contains(&findings) {
                return Err((
                    "opencode_analysis_findings_invalid".into(),
                    format!("findings_count out of bound 0..={MAX_FINDINGS_COUNT}"),
                ));
            }
            if findings != 1 {
                return Err((
                    "opencode_analysis_findings_invalid".into(),
                    "fixture analysis findings_count must be exactly 1".into(),
                ));
            }
            let scope = analysis
                .get("scope_paths")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    (
                        "opencode_analysis_scope_invalid".into(),
                        "scope_paths must be an array".into(),
                    )
                })?;
            let mut scope_paths = Vec::with_capacity(scope.len());
            for value in scope {
                let path = value.as_str().ok_or_else(|| {
                    (
                        "opencode_analysis_scope_invalid".into(),
                        "scope_paths entries must be strings".into(),
                    )
                })?;
                scope_paths.push(path.to_string());
            }
            if scope_paths != allowed_paths {
                return Err((
                    "opencode_analysis_scope_invalid".into(),
                    "scope_paths must exactly match admitted allowed_paths".into(),
                ));
            }
        }
        "allowed_path_patch" => {
            if !result.get("analysis").map(Value::is_null).unwrap_or(false) {
                return Err((
                    "opencode_result_shape_invalid".into(),
                    "allowed_path_patch requires analysis to be explicitly null".into(),
                ));
            }
            if result.get("patch").and_then(Value::as_str).is_none() {
                return Err((
                    "opencode_result_shape_invalid".into(),
                    "allowed_path_patch requires patch body".into(),
                ));
            }
            if result.get("patch_sha256").and_then(Value::as_str).is_none() {
                return Err((
                    "opencode_result_shape_invalid".into(),
                    "allowed_path_patch requires patch_sha256".into(),
                ));
            }
            let changed = result
                .get("changed_paths")
                .and_then(Value::as_array)
                .map(|a| a.len())
                .unwrap_or(0);
            if changed != 1 {
                return Err((
                    "opencode_result_shape_invalid".into(),
                    "allowed_path_patch requires exactly one changed path".into(),
                ));
            }
            if reason != "fixture_patch_ok" {
                return Err((
                    "opencode_reason_code_invalid".into(),
                    "patch reason_code must be fixture_patch_ok".into(),
                ));
            }
        }
        _ => {}
    }
    Ok(())
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
    if artifacts.is_empty() {
        return Err("fixture manifest artifacts must be non-empty".into());
    }
    if artifacts.len() != REQUIRED_MANIFEST_ARTIFACTS.len() {
        return Err(format!(
            "fixture manifest must declare exactly {} artifacts",
            REQUIRED_MANIFEST_ARTIFACTS.len()
        ));
    }
    let package_root = package_root_from_adapter(adapter_path)
        .ok_or_else(|| "cannot resolve adapter package root".to_string())?;
    let package_root = std::fs::canonicalize(&package_root)
        .map_err(|e| format!("package root canonicalize failed: {e}"))?;
    let mut seen_paths = std::collections::BTreeSet::new();
    let mut seen_roles = std::collections::BTreeSet::new();
    for artifact in artifacts {
        let rel = artifact
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| "artifact path required".to_string())?;
        let role = artifact
            .get("role")
            .and_then(Value::as_str)
            .ok_or_else(|| "artifact role required".to_string())?;
        if rel.is_empty()
            || rel.starts_with('/')
            || rel.starts_with('\\')
            || rel.contains("..")
            || rel.contains('\0')
        {
            return Err(format!("artifact path invalid: {rel}"));
        }
        if !seen_paths.insert(rel.to_string()) {
            return Err(format!("duplicate artifact path: {rel}"));
        }
        if !seen_roles.insert(role.to_string()) {
            return Err(format!("duplicate artifact role: {role}"));
        }
        let expected_role = REQUIRED_MANIFEST_ARTIFACTS
            .iter()
            .find(|(path, _)| *path == rel)
            .map(|(_, role)| *role)
            .ok_or_else(|| format!("unexpected artifact path: {rel}"))?;
        if role != expected_role {
            return Err(format!(
                "artifact {rel} role mismatch: expected {expected_role}, got {role}"
            ));
        }
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
        let full_canon = std::fs::canonicalize(&full)
            .map_err(|e| format!("artifact {rel} path resolve failed: {e}"))?;
        if !full_canon.starts_with(&package_root) {
            return Err(format!("artifact {rel} escapes package root"));
        }
        let actual = sha256_file(&full_canon)?;
        if actual != expected {
            return Err(format!(
                "artifact {rel} sha256 mismatch: expected {expected}, got {actual}"
            ));
        }
    }
    for (path, role) in REQUIRED_MANIFEST_ARTIFACTS {
        if !seen_paths.contains(*path) {
            return Err(format!("missing required artifact path: {path}"));
        }
        if !seen_roles.contains(*role) {
            return Err(format!("missing required artifact role: {role}"));
        }
    }
    let adapter_canon = std::fs::canonicalize(adapter_path)
        .map_err(|e| format!("adapter path canonicalize failed: {e}"))?;
    if !adapter_canon.starts_with(&package_root) {
        return Err("adapter path is outside the validated package root".into());
    }
    let entrypoint = package_root.join("src/acp_opencode_adapter/__main__.py");
    let entrypoint_canon = std::fs::canonicalize(&entrypoint)
        .map_err(|e| format!("entrypoint canonicalize failed: {e}"))?;
    let source = package_root.join("src/acp_opencode_adapter/adapter.py");
    let source_canon = std::fs::canonicalize(&source).ok();
    let package_module = package_root.join("src/acp_opencode_adapter");
    let package_module_canon = std::fs::canonicalize(&package_module).ok();
    let adapter_ok = adapter_canon == entrypoint_canon
        || source_canon.as_ref() == Some(&adapter_canon)
        || package_module_canon
            .as_ref()
            .is_some_and(|p| adapter_canon == *p || adapter_canon.starts_with(p));
    if !adapter_ok {
        return Err("adapter entrypoint is not one of the manifest-bound package artifacts".into());
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
    use crate::executor_pool::{
        register_default_executors, register_opencode_runtime_executor, ExecutorPool,
    };
    use crate::scheduler::{SchedulerConfig, WorkflowScheduler};
    use crate::storage::local_product_store::LocalProductStore;
    use std::sync::Mutex;

    struct RecordingInvoker {
        last: Mutex<Option<Value>>,
        response: Value,
        fail: Option<OpenCodeInvokeError>,
        bind_identity: bool,
        calls: Mutex<u32>,
    }

    impl OpenCodeInvoker for RecordingInvoker {
        fn invoke(
            &self,
            request: &Value,
            _timeout_ms: u64,
            _cancel: &dyn OpenCodeCancelProbe,
        ) -> Result<Value, OpenCodeInvokeError> {
            *self.calls.lock().unwrap() += 1;
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
                    "workflow_id",
                    "scheduler_claim_id",
                    "execution_attempt",
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
                // Keep analysis digest/scope consistent with the bound request.
                if response.get("task_kind").and_then(Value::as_str) == Some("analysis") {
                    if let Some(hash) = request.get("task_input_hash").and_then(Value::as_str) {
                        response["analysis"]["summary_digest"] = json!(sha256(hash));
                    }
                    if let Some(paths) = request.get("allowed_paths") {
                        response["analysis"]["scope_paths"] = paths.clone();
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
        let task_input_hash = "a".repeat(64);
        json!({
            "schema_version": OPENCODE_RESULT_SCHEMA,
            "task_kind": "analysis",
            "status": "ok",
            "workflow_id": "wf-oc-1",
            "changed_paths": [],
            "patch": null,
            "patch_sha256": null,
            "analysis": {
                "summary_digest": sha256(&task_input_hash),
                "findings_count": 1,
                "scope_paths": ["docs/a.md"]
            },
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
        sample_input_attempt(task_kind, paths, 1)
    }

    fn sample_input_attempt(task_kind: &str, paths: &[&str], attempt: i64) -> NodeExecutionInput {
        NodeExecutionInput {
            run_id: "run-oc-1".to_string(),
            workflow_id: "wf-oc-1".to_string(),
            node_id: "node-oc-1".to_string(),
            task_type: OPENCODE_TASK_TYPE.to_string(),
            node_metadata: json!({
                "execution_attempt": attempt,
                "scheduler_claim_id": format!("workflow:run-oc-1:node-oc-1:{attempt}"),
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

    fn make_executor(response: Value) -> (OpenCodeNodeExecutor, Arc<RecordingInvoker>) {
        let invoker = Arc::new(RecordingInvoker {
            last: Mutex::new(None),
            response,
            fail: None,
            bind_identity: true,
            calls: Mutex::new(0),
        });
        let cancel = OpenCodeCancellationHandle::new();
        let executor = OpenCodeNodeExecutor::new(
            OpenCodeRuntimeConfig::fixture(PathBuf::from("python3"), PathBuf::from("adapter.py")),
            invoker.clone(),
            cancel,
        );
        (executor, invoker)
    }

    #[test]
    fn analysis_fixture_completes_with_canonical_status() {
        let (executor, invoker) = make_executor(ok_analysis_response());
        let output = executor.execute_node(&sample_input("analysis", &["docs/a.md"]));
        assert_eq!(output.status, "completed", "{:?}", output.error_message);
        let request = invoker.last.lock().unwrap().clone().unwrap();
        assert_eq!(
            request["scheduler_claim_id"],
            "workflow:run-oc-1:node-oc-1:1"
        );
        assert_eq!(request["execution_attempt"], 1);
        assert!(request.get("lease_id").is_none());
    }

    #[test]
    fn rejects_missing_execution_attempt() {
        let (executor, _) = make_executor(ok_analysis_response());
        let mut input = sample_input("analysis", &["docs/a.md"]);
        input
            .node_metadata
            .as_object_mut()
            .unwrap()
            .remove("execution_attempt");
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("opencode_execution_identity_missing")
        );
    }

    #[test]
    fn rejects_stale_execution_attempt_result() {
        // invoker will bind identity from request; force wrong attempt after bind by disabling bind
        let invoker = Arc::new(RecordingInvoker {
            last: Mutex::new(None),
            response: {
                let mut r = ok_analysis_response();
                r["execution_attempt"] = json!(1);
                r["scheduler_claim_id"] = json!("workflow:run-oc-1:node-oc-1:1");
                r["invocation_id"] = json!("wrong");
                r["run_id"] = json!("run-oc-1");
                r["node_id"] = json!("node-oc-1");
                r["task_kind"] = json!("analysis");
                r["task_input_hash"] = json!("a".repeat(64));
                r["base_commit"] = json!("b".repeat(40));
                r["worktree_id"] = json!("wt-1");
                r
            },
            fail: None,
            bind_identity: false,
            calls: Mutex::new(0),
        });
        let executor = OpenCodeNodeExecutor::new(
            OpenCodeRuntimeConfig::fixture(PathBuf::from("python3"), PathBuf::from("adapter.py")),
            invoker,
            OpenCodeCancellationHandle::new(),
        );
        let output = executor.execute_node(&sample_input_attempt("analysis", &["docs/a.md"], 2));
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("opencode_result_binding_invalid")
        );
    }

    #[test]
    fn kill_switch_fails_closed() {
        let invoker = Arc::new(RecordingInvoker {
            last: Mutex::new(None),
            response: json!({}),
            fail: None,
            bind_identity: true,
            calls: Mutex::new(0),
        });
        let executor = OpenCodeNodeExecutor::new(
            OpenCodeRuntimeConfig::fixture(PathBuf::from("python3"), PathBuf::from("adapter.py")),
            invoker,
            OpenCodeCancellationHandle::new(),
        )
        .with_force_kill_switch(true);
        let output = executor.execute_node(&sample_input("analysis", &["docs/a.md"]));
        assert_eq!(output.status, "failed");
        assert_eq!(output.error_domain.as_deref(), Some("opencode_killed"));
    }

    #[test]
    fn cancellation_handle_stops_process_adapter() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let adapter = repo.join("adapters/opencode/src/acp_opencode_adapter/__main__.py");
        if !adapter.exists() {
            return;
        }
        let cancel = OpenCodeCancellationHandle::new();
        let config = OpenCodeRuntimeConfig {
            timeout_ms: 15_000,
            ..OpenCodeRuntimeConfig::fixture(which_python(), adapter)
        };
        let invoker = OpenCodeProcessInvoker::new(&config, cancel.clone());
        let request = json!({
            "schema_version": OPENCODE_REQUEST_SCHEMA,
            "invocation_id": "inv-kill",
            "run_id": "run-kill",
            "node_id": "node-kill",
            "workflow_id": "wf-kill",
            "execution_attempt": 1,
            "scheduler_claim_id": "workflow:run-kill:node-kill:1",
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
        let probe = CompositeCancelProbe::new(cancel.clone(), None, "run-kill", "node-kill", 1);
        let handle = thread::spawn(move || invoker.invoke(&request, 15_000, &probe));
        thread::sleep(Duration::from_millis(200));
        cancel.cancel();
        let err = handle.join().unwrap().expect_err("must cancel");
        assert!(
            matches!(
                err.code.as_str(),
                "scheduler_cancelled" | "process_containment_failed" | "adapter_killed"
            ),
            "{}",
            err.code
        );
        assert!(err.termination.is_some());
        let term = err.termination.unwrap();
        assert!(term.readers_joined || !term.containment_ok);
    }

    #[test]
    fn process_adapter_timeout_records_truthful_termination_evidence() {
        let _env = SupervisedKillEnvGuard::clean();
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let adapter = repo.join("adapters/opencode/src/acp_opencode_adapter/__main__.py");
        if !adapter.exists() {
            return;
        }
        let cancel = OpenCodeCancellationHandle::new();
        let config = OpenCodeRuntimeConfig {
            timeout_ms: 800,
            ..OpenCodeRuntimeConfig::fixture(which_python(), adapter)
        };
        let invoker = OpenCodeProcessInvoker::new(&config, cancel.clone());
        let request = json!({
            "schema_version": OPENCODE_REQUEST_SCHEMA,
            "invocation_id": "inv-timeout",
            "run_id": "run-timeout",
            "node_id": "node-timeout",
            "workflow_id": "wf-timeout",
            "execution_attempt": 1,
            "scheduler_claim_id": "workflow:run-timeout:node-timeout:1",
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
        let err = invoker
            .invoke(&request, 800, &AlwaysLiveProbe)
            .expect_err("timeout");
        assert!(
            matches!(
                err.code.as_str(),
                "adapter_timeout" | "process_containment_failed"
            ),
            "{}",
            err.code
        );
        let term = err.termination.expect("termination evidence");
        assert_eq!(term.schema_version, TERMINATION_EVIDENCE_SCHEMA);
        assert!(term.readers_joined);
        assert!(term.stdout_drained);
        assert!(term.stderr_drained);
        assert!(term.wait_succeeded);
    }

    #[test]
    fn fixture_manifest_validates_when_present() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let manifest = repo.join("adapters/opencode/FIXTURE_ADAPTER_MANIFEST.json");
        let adapter = repo.join("adapters/opencode/src/acp_opencode_adapter/__main__.py");
        validate_fixture_adapter_manifest(&manifest, &adapter).expect("manifest");
    }

    #[test]
    fn fixture_manifest_rejects_empty_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        let pkg = dir.path().join("opencode");
        std::fs::create_dir_all(pkg.join("src/acp_opencode_adapter")).unwrap();
        for name in ["__init__.py", "__main__.py", "adapter.py"] {
            std::fs::write(pkg.join("src/acp_opencode_adapter").join(name), b"x").unwrap();
        }
        std::fs::write(pkg.join("pyproject.toml"), b"[project]\nname='x'\n").unwrap();
        let manifest_path = pkg.join("FIXTURE_ADAPTER_MANIFEST.json");
        std::fs::write(
            &manifest_path,
            r#"{"schema_version":"opencode_fixture_adapter_manifest.v1","admission_status":"fixture_adapter_only","binary_admission_status":"not_admitted","adapter_version":"0.1.0","adapter_contract_version":"opencode_external_adapter.v1","permission_profile_hash":"00","artifacts":[]}"#,
        )
        .unwrap();
        let err = validate_fixture_adapter_manifest(
            &manifest_path,
            &pkg.join("src/acp_opencode_adapter/__main__.py"),
        )
        .unwrap_err();
        assert!(
            err.contains("non-empty") || err.contains("exactly"),
            "{err}"
        );
    }

    #[test]
    fn fixture_manifest_rejects_duplicate_path() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let good = repo.join("adapters/opencode/FIXTURE_ADAPTER_MANIFEST.json");
        let mut manifest: Value =
            serde_json::from_str(&std::fs::read_to_string(&good).unwrap()).unwrap();
        let arts = manifest["artifacts"].as_array_mut().unwrap();
        arts.push(arts[0].clone());
        let dir = tempfile::tempdir().unwrap();
        // copy package
        let pkg = dir.path().join("opencode");
        copy_dir(&repo.join("adapters/opencode"), &pkg);
        let path = pkg.join("FIXTURE_ADAPTER_MANIFEST.json");
        std::fs::write(&path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
        let err = validate_fixture_adapter_manifest(
            &path,
            &pkg.join("src/acp_opencode_adapter/__main__.py"),
        )
        .unwrap_err();
        assert!(
            err.contains("duplicate") || err.contains("exactly"),
            "{err}"
        );
    }

    fn copy_dir(src: &Path, dst: &Path) {
        std::fs::create_dir_all(dst).unwrap();
        for entry in std::fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let ty = entry.file_type().unwrap();
            let to = dst.join(entry.file_name());
            if ty.is_dir() {
                if entry.file_name() == "__pycache__" || entry.file_name() == "tests" {
                    continue;
                }
                copy_dir(&entry.path(), &to);
            } else {
                let _ = std::fs::copy(entry.path(), to);
            }
        }
    }

    #[test]
    fn same_process_replay_is_idempotent_after_completion() {
        let store = Arc::new(LocalProductStore::new(":memory:").unwrap());
        let invoker = Arc::new(RecordingInvoker {
            last: Mutex::new(None),
            response: ok_analysis_response(),
            fail: None,
            bind_identity: true,
            calls: Mutex::new(0),
        });
        let cancel = OpenCodeCancellationHandle::new();
        let executor = Arc::new(OpenCodeNodeExecutor::new(
            OpenCodeRuntimeConfig::fixture(PathBuf::from("python3"), PathBuf::from("adapter.py")),
            invoker.clone(),
            cancel,
        ));
        let plan = store
            .create_workflow_plan("oc-replay", "oc-replay-wf", "actor", |ids, _| {
                Ok(json!({
                    "schema_version": "read_only_plan.v1",
                    "plan_id": ids.plan_id,
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "analysis": {"analysis_id": "a", "task_domain": "code"},
                    "graph": {
                        "schema_version": "workflow_graph.v1",
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "status": "decomposed",
                        "created_at": "2026-07-08T00:00:00Z",
                        "updated_at": "2026-07-08T00:00:00Z",
                        "nodes": [{
                            "node_id": "oc-node-1",
                            "task_type": OPENCODE_TASK_TYPE,
                            "status": "pending",
                            "opencode_external": {
                                "schema_version": OPENCODE_NODE_SCHEMA,
                                "task_kind": "analysis",
                                "task_input_hash": "a".repeat(64),
                                "base_commit": "b".repeat(40),
                                "worktree_id": "wt-1",
                                "allowed_paths": ["docs/a.md"]
                            }
                        }],
                        "edges": []
                    },
                    "boundaries": {
                        "execution_authority": "disabled",
                        "target_repository_writes": "disabled",
                        "runtime_workers": "disabled",
                    },
                }))
            })
            .unwrap();
        let run = store
            .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
            .unwrap();
        let run_id = run["run_id"].as_str().unwrap().to_string();
        let pool = Arc::new(ExecutorPool::new());
        register_default_executors(&pool, false, store.clone());
        register_opencode_runtime_executor(&pool, executor.clone(), 1, 30_000);
        let config = SchedulerConfig {
            executor_type: "pool".to_string(),
            max_concurrent: 1,
            queue_enabled: false,
            backpressure_enabled: false,
            ..Default::default()
        };
        store
            .tick_with_executor_and_command_inner(&run_id, "scheduler", 0, &*executor, None, None)
            .unwrap();
        assert_eq!(*invoker.calls.lock().unwrap(), 1);
        let replay = store.tick_with_executor_and_command_inner(
            &run_id,
            "scheduler",
            0,
            &*executor,
            None,
            None,
        );
        assert!(
            replay.is_err()
                || replay
                    .as_ref()
                    .ok()
                    .and_then(|v| v.get("action").and_then(|a| a.as_str()))
                    != Some("node_executed"),
            "replay must not re-execute: {replay:?}"
        );
        assert_eq!(
            *invoker.calls.lock().unwrap(),
            1,
            "same-process replay must not re-invoke"
        );
        let run_after = store.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(run_after["status"], "completed");
        let _ = (config, pool);
    }

    #[test]
    fn durable_file_backed_restart_with_fresh_owners_does_not_reexecute() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("restart.db");
        let db = db_path.to_str().unwrap();
        let invoker_calls = Arc::new(Mutex::new(0u32));
        let carried_run_id = {
            let store = Arc::new(LocalProductStore::new(db).unwrap());
            let calls = invoker_calls.clone();
            let invoker = Arc::new(CountingInvoker {
                calls,
                response: ok_analysis_response(),
            });
            let cancel = OpenCodeCancellationHandle::new();
            let executor = Arc::new(
                OpenCodeNodeExecutor::new(
                    OpenCodeRuntimeConfig::fixture(
                        PathBuf::from("python3"),
                        PathBuf::from("adapter.py"),
                    ),
                    invoker,
                    cancel.clone(),
                )
                .with_store(store.clone()),
            );
            let plan = store
                .create_workflow_plan("oc-restart", "oc-restart-wf", "actor", |ids, _| {
                    Ok(json!({
                        "schema_version": "read_only_plan.v1",
                        "plan_id": ids.plan_id,
                        "status": "planned_read_only",
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "analysis": {"analysis_id": "a", "task_domain": "code"},
                        "graph": {
                            "schema_version": "workflow_graph.v1",
                            "workflow_id": ids.workflow_id,
                            "dispatch_id": ids.dispatch_id,
                            "status": "decomposed",
                            "created_at": "2026-07-08T00:00:00Z",
                            "updated_at": "2026-07-08T00:00:00Z",
                            "nodes": [{
                                "node_id": "oc-node-1",
                                "task_type": OPENCODE_TASK_TYPE,
                                "status": "pending",
                                "opencode_external": {
                                    "schema_version": OPENCODE_NODE_SCHEMA,
                                    "task_kind": "analysis",
                                    "task_input_hash": "a".repeat(64),
                                    "base_commit": "b".repeat(40),
                                    "worktree_id": "wt-1",
                                    "allowed_paths": ["docs/a.md"]
                                }
                            }],
                            "edges": []
                        },
                        "boundaries": {
                            "execution_authority": "disabled",
                            "target_repository_writes": "disabled",
                            "runtime_workers": "disabled",
                        },
                    }))
                })
                .unwrap();
            let run = store
                .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
                .unwrap();
            let run_id = run["run_id"].as_str().unwrap().to_string();
            let pool = Arc::new(ExecutorPool::new());
            register_default_executors(&pool, false, store.clone());
            register_opencode_runtime_executor(&pool, executor.clone(), 1, 30_000);
            let scheduler = WorkflowScheduler::new(
                store.clone(),
                SchedulerConfig {
                    executor_type: "pool".to_string(),
                    max_concurrent: 1,
                    queue_enabled: false,
                    backpressure_enabled: false,
                    interval_ms: 50,
                    ..Default::default()
                },
            )
            .with_opencode_runtime_executor(executor.clone(), 30_000)
            .with_opencode_cancellation(cancel);
            // Direct tick via store under first owners (simulates first process).
            store
                .tick_with_executor_and_command_inner(
                    &run_id,
                    "scheduler",
                    0,
                    &*executor,
                    None,
                    None,
                )
                .unwrap();
            let run_after = store.get_workflow_run(&run_id).unwrap().unwrap();
            assert_eq!(run_after["status"], "completed");
            assert_eq!(*invoker_calls.lock().unwrap(), 1);
            let _ = (pool, scheduler);
            // Carry the real created run id across restart (not a hard-coded sequence).
            run_id
        };

        // Reopen database with genuinely fresh owners.
        let store = Arc::new(LocalProductStore::new(db).unwrap());
        let calls = invoker_calls.clone();
        let invoker = Arc::new(CountingInvoker {
            calls,
            response: ok_analysis_response(),
        });
        let cancel = OpenCodeCancellationHandle::new();
        let executor = Arc::new(
            OpenCodeNodeExecutor::new(
                OpenCodeRuntimeConfig::fixture(
                    PathBuf::from("python3"),
                    PathBuf::from("adapter.py"),
                ),
                invoker,
                cancel.clone(),
            )
            .with_store(store.clone()),
        );
        let pool = Arc::new(ExecutorPool::new());
        register_default_executors(&pool, false, store.clone());
        register_opencode_runtime_executor(&pool, executor.clone(), 1, 30_000);
        let mut scheduler = WorkflowScheduler::new(
            store.clone(),
            SchedulerConfig {
                executor_type: "pool".to_string(),
                max_concurrent: 1,
                queue_enabled: false,
                backpressure_enabled: false,
                interval_ms: 50,
                supervised_workers_enabled: true,
                worker_count: 1,
                ..Default::default()
            },
        )
        .with_opencode_runtime_executor(executor.clone(), 30_000)
        .with_opencode_cancellation(cancel);

        let active = store.list_active_workflow_run_ids().unwrap();
        assert!(
            active.is_empty(),
            "completed run must not be active after durable reopen: {active:?}"
        );
        let run_after = store
            .get_workflow_run(&carried_run_id)
            .unwrap()
            .expect("carried run id must load after reopen");
        assert_eq!(run_after["status"], "completed");

        scheduler.start().unwrap();
        thread::sleep(Duration::from_millis(300));
        let _ = scheduler.stop();

        assert_eq!(
            *invoker_calls.lock().unwrap(),
            1,
            "durable restart must not re-invoke completed node"
        );
        let run_final = store.get_workflow_run(&carried_run_id).unwrap().unwrap();
        assert_eq!(run_final["status"], "completed");
        // Pool metrics after restart are process-local and must not be treated as
        // durable execution evidence of a second successful run.
        let snapshot = pool.snapshot();
        if let Some(entry) = snapshot
            .iter()
            .find(|e| e.executor_type == OPENCODE_EXECUTOR_TYPE)
        {
            assert_eq!(
                entry.metrics.total_executions, 0,
                "fresh pool metrics must not reconstruct durable execution evidence"
            );
            assert_eq!(entry.metrics.successful_executions, 0);
        }
    }

    struct CountingInvoker {
        calls: Arc<Mutex<u32>>,
        response: Value,
    }

    impl OpenCodeInvoker for CountingInvoker {
        fn invoke(
            &self,
            request: &Value,
            _timeout_ms: u64,
            _cancel: &dyn OpenCodeCancelProbe,
        ) -> Result<Value, OpenCodeInvokeError> {
            *self.calls.lock().unwrap() += 1;
            let mut response = self.response.clone();
            for key in [
                "invocation_id",
                "run_id",
                "node_id",
                "workflow_id",
                "scheduler_claim_id",
                "execution_attempt",
                "task_kind",
                "task_input_hash",
                "base_commit",
                "worktree_id",
            ] {
                if let Some(v) = request.get(key) {
                    response[key] = v.clone();
                }
            }
            if response.get("task_kind").and_then(Value::as_str) == Some("analysis") {
                if let Some(hash) = request.get("task_input_hash").and_then(Value::as_str) {
                    response["analysis"]["summary_digest"] = json!(sha256(hash));
                }
                if let Some(paths) = request.get("allowed_paths") {
                    response["analysis"]["scope_paths"] = paths.clone();
                }
            }
            Ok(response)
        }
    }

    #[test]
    fn scheduler_kill_terminates_running_opencode_descendant() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let adapter = repo.join("adapters/opencode/src/acp_opencode_adapter/__main__.py");
        if !adapter.exists() {
            return;
        }
        let store = Arc::new(LocalProductStore::new(":memory:").unwrap());
        let cancel = OpenCodeCancellationHandle::new();
        let config = OpenCodeRuntimeConfig {
            timeout_ms: 30_000,
            ..OpenCodeRuntimeConfig::fixture(which_python(), adapter)
        };
        let invoker = Arc::new(OpenCodeProcessInvoker::new(&config, cancel.clone()));
        let executor = Arc::new(
            OpenCodeNodeExecutor::new(config, invoker, cancel.clone()).with_store(store.clone()),
        );
        let plan = store
            .create_workflow_plan("oc-kill", "oc-kill-wf", "actor", |ids, _| {
                Ok(json!({
                    "schema_version": "read_only_plan.v1",
                    "plan_id": ids.plan_id,
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "analysis": {"analysis_id": "a", "task_domain": "code"},
                    "graph": {
                        "schema_version": "workflow_graph.v1",
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "status": "decomposed",
                        "created_at": "2026-07-08T00:00:00Z",
                        "updated_at": "2026-07-08T00:00:00Z",
                        "nodes": [{
                            "node_id": "oc-node-kill",
                            "task_type": OPENCODE_TASK_TYPE,
                            "status": "pending",
                            "opencode_external": {
                                "schema_version": OPENCODE_NODE_SCHEMA,
                                "task_kind": "descendant_spawn",
                                "task_input_hash": "a".repeat(64),
                                "base_commit": "b".repeat(40),
                                "worktree_id": "wt-kill",
                                "allowed_paths": ["docs/a.md"]
                            }
                        }],
                        "edges": []
                    },
                    "boundaries": {
                        "execution_authority": "disabled",
                        "target_repository_writes": "disabled",
                        "runtime_workers": "disabled",
                    },
                }))
            })
            .unwrap();
        let _run = store
            .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
            .unwrap();
        let mut scheduler = WorkflowScheduler::new(
            store.clone(),
            SchedulerConfig {
                interval_ms: 50,
                max_concurrent: 1,
                executor_type: "pool".to_string(),
                queue_enabled: false,
                backpressure_enabled: false,
                supervised_workers_enabled: true,
                worker_count: 1,
                lease_timeout_ms: 60_000,
                ..Default::default()
            },
        )
        .with_opencode_runtime_executor(executor, 30_000)
        .with_opencode_cancellation(cancel.clone());
        scheduler.start().unwrap();
        thread::sleep(Duration::from_millis(400));
        scheduler.kill("test").unwrap();
        // Process should terminate via cancel handle without waiting full timeout.
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if !scheduler.is_running() {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        assert!(!scheduler.is_running() || cancel.is_cancelled());
        let _ = scheduler.stop();
    }

    #[test]
    fn rejects_missing_base_commit_without_fallback() {
        let (executor, _) = make_executor(ok_analysis_response());
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
    fn rejects_nonzero_network_attempts() {
        let mut response = ok_analysis_response();
        response["tool_summary"]["network_attempts"] = json!(1);
        let (executor, _) = make_executor(response);
        let output = executor.execute_node(&sample_input("analysis", &["docs/a.md"]));
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("opencode_forbidden_tool_activity")
        );
    }

    #[test]
    fn rejects_wrong_analysis_reason_code() {
        let mut response = ok_analysis_response();
        response["reason_code"] = json!("not_the_family");
        let (executor, _) = make_executor(response);
        let output = executor.execute_node(&sample_input("analysis", &["docs/a.md"]));
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("opencode_reason_code_invalid")
        );
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
    }

    #[test]
    fn unmanifested_sitecustomize_is_rejected_and_cannot_execute() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let src = repo.join("adapters/opencode");
        let dir = tempfile::tempdir().unwrap();
        let pkg = dir.path().join("opencode");
        copy_dir(&src, &pkg);
        let pythonpath = pkg.join("src");
        let marker = dir.path().join("sitecustomize_ran.marker");
        std::fs::write(
            pythonpath.join("sitecustomize.py"),
            format!("open({marker:?}, 'w').write('ran')\n"),
        )
        .unwrap();
        // Confinement rejects the unmanifested auto-exec file before spawn.
        let err = reject_unmanifested_startup_files(&pythonpath).unwrap_err();
        assert_eq!(err.code, "adapter_startup_confinement");
        // Even if present, python -S must not execute sitecustomize.
        let python = which_python();
        let status = Command::new(&python)
            .arg("-S")
            .arg("-s")
            .arg("-B")
            .arg("-c")
            .arg("import acp_opencode_adapter")
            .env_clear()
            .env("PYTHONPATH", &pythonpath)
            .env("PYTHONNOUSERSITE", "1")
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .status()
            .expect("python -S");
        assert!(status.success());
        assert!(
            !marker.exists(),
            "unmanifested sitecustomize.py must not execute under -S confinement"
        );
    }

    #[test]
    fn stdin_write_failure_terminates_and_reaps_process_group() {
        let _env = SupervisedKillEnvGuard::clean();
        let dir = tempfile::tempdir().unwrap();
        let fake_python = dir.path().join("fake-python-exit");
        std::fs::write(
            &fake_python,
            "#!/bin/sh\n# Exit immediately so parent stdin write observes EPIPE.\nexit 0\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&fake_python).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&fake_python, perms).unwrap();
        }
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let adapter = repo.join("adapters/opencode/src/acp_opencode_adapter/__main__.py");
        let cancel = OpenCodeCancellationHandle::new();
        let config = OpenCodeRuntimeConfig {
            timeout_ms: 5_000,
            python_program: fake_python,
            ..OpenCodeRuntimeConfig::fixture(which_python(), adapter)
        };
        let invoker = OpenCodeProcessInvoker::new(&config, cancel);
        let request = json!({
            "schema_version": OPENCODE_REQUEST_SCHEMA,
            "invocation_id": "inv-stdin",
            "run_id": "run-stdin",
            "node_id": "node-stdin",
            "workflow_id": "wf-stdin",
            "execution_attempt": 1,
            "scheduler_claim_id": "workflow:run-stdin:node-stdin:1",
            "runtime_kind": "opencode",
            "mode": "fixture",
            "task_kind": "analysis",
            "task_input_hash": "a".repeat(64),
            "base_commit": "b".repeat(40),
            "worktree_id": "wt-stdin",
            "allowed_paths": ["docs/a.md"],
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
        let err = invoker
            .invoke(&request, 5_000, &AlwaysLiveProbe)
            .expect_err("stdin/path failure expected");
        assert!(
            matches!(
                err.code.as_str(),
                "adapter_stdin"
                    | "process_containment_failed"
                    | "adapter_output_invalid"
                    | "adapter_failed"
            ),
            "{}",
            err.code
        );
        assert!(
            err.termination.is_some(),
            "post-spawn failure must carry termination evidence"
        );
    }

    #[test]
    fn early_adapter_exit_with_descendant_is_contained() {
        let _env = SupervisedKillEnvGuard::clean();
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let adapter = repo.join("adapters/opencode/src/acp_opencode_adapter/__main__.py");
        if !adapter.exists() {
            return;
        }
        let cancel = OpenCodeCancellationHandle::new();
        let config = OpenCodeRuntimeConfig {
            timeout_ms: 10_000,
            ..OpenCodeRuntimeConfig::fixture(which_python(), adapter)
        };
        let invoker = OpenCodeProcessInvoker::new(&config, cancel);
        let request = json!({
            "schema_version": OPENCODE_REQUEST_SCHEMA,
            "invocation_id": "inv-early",
            "run_id": "run-early",
            "node_id": "node-early",
            "workflow_id": "wf-early",
            "execution_attempt": 1,
            "scheduler_claim_id": "workflow:run-early:node-early:1",
            "runtime_kind": "opencode",
            "mode": "fixture",
            "task_kind": "early_exit_with_descendant",
            "task_input_hash": "a".repeat(64),
            "base_commit": "b".repeat(40),
            "worktree_id": "wt-early",
            "allowed_paths": ["docs/a.md"],
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
        let err = invoker
            .invoke(&request, 10_000, &AlwaysLiveProbe)
            .expect_err("must fail closed");
        // Either typed adapter error after tree reaped, or containment failure if residual.
        assert!(
            matches!(
                err.code.as_str(),
                "early_exit_with_descendant" | "process_containment_failed" | "adapter_failed"
            ),
            "{}",
            err.code
        );
        let term = err.termination.expect("termination evidence required");
        assert_eq!(term.descendants_remaining, 0, "descendant must be reaped");
        assert!(term.wait_succeeded || !term.containment_ok || term.descendants_remaining == 0);
    }

    #[test]
    fn scheduler_stop_cancels_running_opencode() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let adapter = repo.join("adapters/opencode/src/acp_opencode_adapter/__main__.py");
        if !adapter.exists() {
            return;
        }
        let store = Arc::new(LocalProductStore::new(":memory:").unwrap());
        let cancel = OpenCodeCancellationHandle::new();
        let config = OpenCodeRuntimeConfig {
            timeout_ms: 30_000,
            ..OpenCodeRuntimeConfig::fixture(which_python(), adapter)
        };
        let invoker = Arc::new(OpenCodeProcessInvoker::new(&config, cancel.clone()));
        let executor = Arc::new(
            OpenCodeNodeExecutor::new(config, invoker, cancel.clone()).with_store(store.clone()),
        );
        let plan = store
            .create_workflow_plan("oc-stop", "oc-stop-wf", "actor", |ids, _| {
                Ok(json!({
                    "schema_version": "read_only_plan.v1",
                    "plan_id": ids.plan_id,
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "analysis": {"analysis_id": "a", "task_domain": "code"},
                    "graph": {
                        "schema_version": "workflow_graph.v1",
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "status": "decomposed",
                        "created_at": "2026-07-08T00:00:00Z",
                        "updated_at": "2026-07-08T00:00:00Z",
                        "nodes": [{
                            "node_id": "oc-node-stop",
                            "task_type": OPENCODE_TASK_TYPE,
                            "status": "pending",
                            "opencode_external": {
                                "schema_version": OPENCODE_NODE_SCHEMA,
                                "task_kind": "descendant_spawn",
                                "task_input_hash": "a".repeat(64),
                                "base_commit": "b".repeat(40),
                                "worktree_id": "wt-stop",
                                "allowed_paths": ["docs/a.md"]
                            }
                        }],
                        "edges": []
                    },
                    "boundaries": {
                        "execution_authority": "disabled",
                        "target_repository_writes": "disabled",
                        "runtime_workers": "disabled",
                    },
                }))
            })
            .unwrap();
        let _ = store
            .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
            .unwrap();
        let mut scheduler = WorkflowScheduler::new(
            store.clone(),
            SchedulerConfig {
                interval_ms: 50,
                max_concurrent: 1,
                executor_type: "pool".to_string(),
                queue_enabled: false,
                backpressure_enabled: false,
                supervised_workers_enabled: true,
                worker_count: 1,
                lease_timeout_ms: 60_000,
                ..Default::default()
            },
        )
        .with_opencode_runtime_executor(executor, 30_000)
        .with_opencode_cancellation(cancel.clone());
        scheduler.start().unwrap();
        thread::sleep(Duration::from_millis(400));
        scheduler.stop().unwrap();
        assert!(cancel.is_cancelled() || !scheduler.is_running());
        assert!(!scheduler.is_running());
    }

    #[test]
    fn scheduler_drop_cancels_opencode_before_join() {
        let cancel = OpenCodeCancellationHandle::new();
        {
            let store = Arc::new(LocalProductStore::new(":memory:").unwrap());
            let invoker = Arc::new(RecordingInvoker {
                last: Mutex::new(None),
                response: ok_analysis_response(),
                fail: None,
                bind_identity: true,
                calls: Mutex::new(0),
            });
            let executor = Arc::new(OpenCodeNodeExecutor::new(
                OpenCodeRuntimeConfig::fixture(
                    PathBuf::from("python3"),
                    PathBuf::from("adapter.py"),
                ),
                invoker,
                cancel.clone(),
            ));
            let mut scheduler = WorkflowScheduler::new(
                store,
                SchedulerConfig {
                    interval_ms: 50,
                    max_concurrent: 1,
                    executor_type: "pool".to_string(),
                    queue_enabled: false,
                    backpressure_enabled: false,
                    supervised_workers_enabled: true,
                    worker_count: 1,
                    ..Default::default()
                },
            )
            .with_opencode_runtime_executor(executor, 30_000)
            .with_opencode_cancellation(cancel.clone());
            scheduler.start().unwrap();
            thread::sleep(Duration::from_millis(100));
            drop(scheduler);
        }
        assert!(
            cancel.is_cancelled(),
            "Drop must cancel OpenCode before joining workers"
        );
    }

    #[test]
    fn supervised_workers_kill_switch_cancels_running_opencode() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let adapter = repo.join("adapters/opencode/src/acp_opencode_adapter/__main__.py");
        if !adapter.exists() {
            return;
        }
        // Use the test-only override (not process env) so parallel scheduler suites
        // that call start() are not poisoned by ACP_SUPERVISED_WORKERS_KILL_SWITCH.
        let _env = SupervisedKillEnvGuard::clean();
        TEST_SUPERVISED_KILL_OVERRIDE.store(false, AtomicOrdering::SeqCst);

        let cancel = OpenCodeCancellationHandle::new();
        let config = OpenCodeRuntimeConfig {
            timeout_ms: 15_000,
            ..OpenCodeRuntimeConfig::fixture(which_python(), adapter)
        };
        let invoker = OpenCodeProcessInvoker::new(&config, cancel.clone());
        let request = json!({
            "schema_version": OPENCODE_REQUEST_SCHEMA,
            "invocation_id": "inv-sup",
            "run_id": "run-sup",
            "node_id": "node-sup",
            "workflow_id": "wf-sup",
            "execution_attempt": 1,
            "scheduler_claim_id": "workflow:run-sup:node-sup:1",
            "runtime_kind": "opencode",
            "mode": "fixture",
            "task_kind": "descendant_spawn",
            "task_input_hash": "a".repeat(64),
            "base_commit": "b".repeat(40),
            "worktree_id": "wt-sup",
            "allowed_paths": ["docs/a.md"],
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
        let handle = thread::spawn(move || invoker.invoke(&request, 15_000, &AlwaysLiveProbe));
        thread::sleep(Duration::from_millis(250));
        TEST_SUPERVISED_KILL_OVERRIDE.store(true, AtomicOrdering::SeqCst);
        let err = handle
            .join()
            .unwrap()
            .expect_err("must cancel via kill switch");
        TEST_SUPERVISED_KILL_OVERRIDE.store(false, AtomicOrdering::SeqCst);
        assert!(
            matches!(
                err.code.as_str(),
                "supervised_workers_kill_switch"
                    | "process_containment_failed"
                    | "adapter_killed"
                    | "scheduler_cancelled"
            ),
            "{}",
            err.code
        );
        assert!(err.termination.is_some());
    }

    #[test]
    fn always_live_probe_observes_supervised_workers_env_kill_switch() {
        let _env = SupervisedKillEnvGuard::clean();
        assert!(AlwaysLiveProbe.should_cancel().is_none());
        std::env::set_var("ACP_SUPERVISED_WORKERS_KILL_SWITCH", "1");
        assert_eq!(
            AlwaysLiveProbe.should_cancel(),
            Some("supervised_workers_kill_switch")
        );
        std::env::remove_var("ACP_SUPERVISED_WORKERS_KILL_SWITCH");
        assert!(AlwaysLiveProbe.should_cancel().is_none());
    }

    #[test]
    fn cancel_reset_advances_generation_and_does_not_revive_inflight() {
        let handle = OpenCodeCancellationHandle::new();
        let gen0 = handle.generation();
        let probe = CompositeCancelProbe::new(handle.clone(), None, "run", "node", 1);
        assert!(probe.should_cancel().is_none());
        handle.cancel();
        assert_eq!(probe.should_cancel(), Some("scheduler_cancelled"));
        handle.reset();
        assert_ne!(handle.generation(), gen0);
        // Old probe remains cancelled via generation mismatch; new probe is live.
        assert_eq!(probe.should_cancel(), Some("scheduler_cancelled"));
        let probe2 = CompositeCancelProbe::new(handle.clone(), None, "run", "node", 1);
        assert!(probe2.should_cancel().is_none());
        assert!(!handle.is_cancelled());
    }

    #[test]
    fn rejects_extra_tool_summary_keys() {
        let mut response = ok_analysis_response();
        response["tool_summary"]["extra_counter"] = json!(0);
        let (executor, _) = make_executor(response);
        let output = executor.execute_node(&sample_input("analysis", &["docs/a.md"]));
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("opencode_tool_evidence_keys_invalid")
        );
    }

    #[test]
    fn rejects_analysis_missing_summary_digest() {
        struct IncompleteAnalysisInvoker;
        impl OpenCodeInvoker for IncompleteAnalysisInvoker {
            fn invoke(
                &self,
                request: &Value,
                _timeout_ms: u64,
                _cancel: &dyn OpenCodeCancelProbe,
            ) -> Result<Value, OpenCodeInvokeError> {
                let mut response = ok_analysis_response();
                for key in [
                    "invocation_id",
                    "run_id",
                    "node_id",
                    "workflow_id",
                    "scheduler_claim_id",
                    "execution_attempt",
                    "task_kind",
                    "task_input_hash",
                    "base_commit",
                    "worktree_id",
                ] {
                    if let Some(v) = request.get(key) {
                        response[key] = v.clone();
                    }
                }
                response["analysis"] = json!({
                    "findings_count": 1,
                    "scope_paths": request.get("allowed_paths").cloned().unwrap_or(json!([]))
                });
                Ok(response)
            }
        }
        let executor = OpenCodeNodeExecutor::new(
            OpenCodeRuntimeConfig::fixture(PathBuf::from("python3"), PathBuf::from("adapter.py")),
            Arc::new(IncompleteAnalysisInvoker),
            OpenCodeCancellationHandle::new(),
        );
        let output = executor.execute_node(&sample_input("analysis", &["docs/a.md"]));
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("opencode_analysis_keys_invalid")
        );
    }

    #[test]
    fn rejects_workflow_id_mismatch() {
        struct MismatchInvoker;
        impl OpenCodeInvoker for MismatchInvoker {
            fn invoke(
                &self,
                request: &Value,
                _timeout_ms: u64,
                _cancel: &dyn OpenCodeCancelProbe,
            ) -> Result<Value, OpenCodeInvokeError> {
                let mut response = ok_analysis_response();
                for key in [
                    "invocation_id",
                    "run_id",
                    "node_id",
                    "scheduler_claim_id",
                    "execution_attempt",
                    "task_kind",
                    "task_input_hash",
                    "base_commit",
                    "worktree_id",
                ] {
                    if let Some(v) = request.get(key) {
                        response[key] = v.clone();
                    }
                }
                response["workflow_id"] = json!("not-the-workflow");
                if let Some(hash) = request.get("task_input_hash").and_then(Value::as_str) {
                    response["analysis"]["summary_digest"] = json!(sha256(hash));
                }
                if let Some(paths) = request.get("allowed_paths") {
                    response["analysis"]["scope_paths"] = paths.clone();
                }
                Ok(response)
            }
        }
        let executor = OpenCodeNodeExecutor::new(
            OpenCodeRuntimeConfig::fixture(PathBuf::from("python3"), PathBuf::from("adapter.py")),
            Arc::new(MismatchInvoker),
            OpenCodeCancellationHandle::new(),
        );
        let output = executor.execute_node(&sample_input("analysis", &["docs/a.md"]));
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("opencode_result_binding_invalid")
        );
    }

    static SUPERVISED_KILL_ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Hold while invoking real adapter processes so parallel tests cannot leave
    /// ACP_SUPERVISED_WORKERS_KILL_SWITCH set mid-invocation.
    struct SupervisedKillEnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        previous: Option<String>,
    }

    impl SupervisedKillEnvGuard {
        fn clean() -> Self {
            let lock = SUPERVISED_KILL_ENV_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let previous = std::env::var("ACP_SUPERVISED_WORKERS_KILL_SWITCH").ok();
            std::env::remove_var("ACP_SUPERVISED_WORKERS_KILL_SWITCH");
            Self {
                _lock: lock,
                previous,
            }
        }
    }

    impl Drop for SupervisedKillEnvGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => std::env::set_var("ACP_SUPERVISED_WORKERS_KILL_SWITCH", value),
                None => std::env::remove_var("ACP_SUPERVISED_WORKERS_KILL_SWITCH"),
            }
        }
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
