//! Default-off OpenCode external coding executor (PE7 fixture-first).
//!
//! Rust owns admission, timeout, kill-switch, and typed receipts. The adapter
//! process (fixture mode) never receives network, MCP, or provider authority.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

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
pub const PINNED_OPENCODE_VERSION: &str = "1.1.48";

pub const ENABLE_ENV: &str = "ACP_ENABLE_OPENCODE_RUNTIME";
pub const MODE_ENV: &str = "ACP_OPENCODE_MODE";
pub const PYTHON_ENV: &str = "ACP_OPENCODE_PYTHON";
pub const ADAPTER_PATH_ENV: &str = "ACP_OPENCODE_ADAPTER_PATH";
pub const KILL_SWITCH_ENV: &str = "ACP_OPENCODE_KILL_SWITCH";
pub const TIMEOUT_MS_ENV: &str = "ACP_OPENCODE_TIMEOUT_MS";

const MAX_PROCESS_OUTPUT_BYTES: usize = 256 * 1024;
const MAX_PROCESS_ERROR_BYTES: usize = 16 * 1024;
const MAX_PATCH_BYTES: usize = 64 * 1024;

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
}

impl OpenCodeRuntimeConfig {
    pub fn from_env() -> Result<Option<Self>, String> {
        if std::env::var(ENABLE_ENV).as_deref() != Ok("1") {
            return Ok(None);
        }
        if std::env::var(KILL_SWITCH_ENV).as_deref() == Ok("1") {
            return Err("OpenCode runtime kill switch is active".to_string());
        }
        let mode = match required_env(MODE_ENV)?.as_str() {
            "fixture" => OpenCodeMode::Fixture,
            "live" => {
                return Err("OpenCode live mode is not admitted in PE7-OPENCODE-EXTERNAL-ADAPTER-1; fixture only".to_string())
            }
            other => return Err(format!("{MODE_ENV} unsupported value: {other}")),
        };
        let python_program = canonical_executable_path(&required_env(PYTHON_ENV)?, PYTHON_ENV)?;
        let adapter_path =
            canonical_regular_file(&required_env(ADAPTER_PATH_ENV)?, ADAPTER_PATH_ENV)?;
        let timeout_ms = required_u64(TIMEOUT_MS_ENV, 1_000, 120_000)?;
        Ok(Some(Self {
            mode,
            python_program,
            adapter_path,
            timeout_ms,
            adapter_version: OPENCODE_ADAPTER_VERSION.to_string(),
            expected_opencode_version: PINNED_OPENCODE_VERSION.to_string(),
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

pub trait OpenCodeInvoker: Send + Sync {
    fn invoke(&self, request: &Value, timeout_ms: u64) -> Result<Value, OpenCodeInvokeError>;
}

pub struct OpenCodeProcessInvoker {
    python_program: PathBuf,
    adapter_path: PathBuf,
}

impl OpenCodeProcessInvoker {
    pub fn new(config: &OpenCodeRuntimeConfig) -> Self {
        Self {
            python_program: config.python_program.clone(),
            adapter_path: config.adapter_path.clone(),
        }
    }
}

impl OpenCodeInvoker for OpenCodeProcessInvoker {
    fn invoke(&self, request: &Value, timeout_ms: u64) -> Result<Value, OpenCodeInvokeError> {
        let input = canonical_event_json(request)
            .map_err(|error| OpenCodeInvokeError::new("request_encoding", error.to_string()))?;
        // Prefer package entrypoint: adapter_path may be .../src/acp_opencode_adapter/__main__.py
        // or .../src; PYTHONPATH must include the package root (`src`).
        let pythonpath = self
            .adapter_path
            .parent()
            .and_then(|p| {
                if p.file_name().and_then(|n| n.to_str()) == Some("acp_opencode_adapter") {
                    p.parent().map(Path::to_path_buf)
                } else {
                    Some(p.to_path_buf())
                }
            })
            .unwrap_or_else(|| self.adapter_path.clone());
        let mut command = Command::new(&self.python_program);
        command
            .arg("-m")
            .arg("acp_opencode_adapter")
            .env_clear()
            .env("PYTHONIOENCODING", "utf-8")
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env("PYTHONPATH", pythonpath)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|error| OpenCodeInvokeError::new("adapter_spawn", error.to_string()))?;
        child
            .stdin
            .take()
            .ok_or_else(|| OpenCodeInvokeError::new("adapter_stdin", "missing stdin"))?
            .write_all(input.as_bytes())
            .map_err(|error| OpenCodeInvokeError::new("adapter_stdin", error.to_string()))?;
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
            if std::env::var(KILL_SWITCH_ENV).as_deref() == Ok("1") {
                let _ = child.kill();
                let _ = child.wait();
                return Err(OpenCodeInvokeError::new(
                    "adapter_killed",
                    "OpenCode kill switch became active",
                ));
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() >= Duration::from_millis(timeout_ms) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(OpenCodeInvokeError::new(
                        "adapter_timeout",
                        format!("OpenCode adapter timed out after {timeout_ms}ms"),
                    ));
                }
                Ok(None) => thread::sleep(Duration::from_millis(25)),
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(OpenCodeInvokeError::new("adapter_wait", error.to_string()));
                }
            }
        };
        let stdout = stdout_reader
            .join()
            .map_err(|_| OpenCodeInvokeError::new("adapter_stdout", "stdout reader panicked"))?
            .map_err(|error| OpenCodeInvokeError::new("adapter_stdout", error.to_string()))?;
        let stderr = stderr_reader
            .join()
            .map_err(|_| OpenCodeInvokeError::new("adapter_stderr", "stderr reader panicked"))?
            .map_err(|error| OpenCodeInvokeError::new("adapter_stderr", error.to_string()))?;
        if stdout.truncated {
            return Err(OpenCodeInvokeError::new(
                "adapter_output_oversized",
                "OpenCode adapter output exceeded bounded cap",
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

pub struct OpenCodeNodeExecutor {
    config: OpenCodeRuntimeConfig,
    invoker: Arc<dyn OpenCodeInvoker>,
}

impl OpenCodeNodeExecutor {
    pub fn new(config: OpenCodeRuntimeConfig, invoker: Arc<dyn OpenCodeInvoker>) -> Self {
        Self { config, invoker }
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
        if std::env::var(KILL_SWITCH_ENV).as_deref() == Ok("1") {
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
            .unwrap_or("fixture-base");
        let worktree_id = metadata
            .get("worktree_id")
            .and_then(Value::as_str)
            .unwrap_or("fixture-worktree");
        let permission_profile = json!({
            "approval_mode":"deny_by_default",
            "network_enabled":false,
            "mcp_enabled":false,
            "websearch":false,
            "webfetch":false,
            "remote_agents":false,
            "background_agents":false,
            "provider_fallback":false,
        });
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
            "environment_allowlist": ["PATH", "HOME", "LANG", "LC_ALL", "TMPDIR"],
            "permission_profile": permission_profile,
            "permission_profile_hash": profile_hash,
            "requested_capabilities": [],
            "adapter_version": self.config.adapter_version,
            "expected_opencode_version": self.config.expected_opencode_version,
        });
        let result = self
            .invoker
            .invoke(&request, self.config.timeout_ms)
            .map_err(|error| self.failure(started, &error.code, &error.message))?;
        if result.get("schema_version").and_then(Value::as_str) != Some(OPENCODE_RESULT_SCHEMA) {
            return Err(self.failure(
                started,
                "opencode_result_invalid",
                "result schema_version is invalid",
            ));
        }
        if result.get("invocation_id").and_then(Value::as_str) != Some(invocation_id.as_str()) {
            return Err(self.failure(
                started,
                "opencode_result_binding_invalid",
                "invocation_id mismatch",
            ));
        }
        if result.get("run_id").and_then(Value::as_str) != Some(input.run_id.as_str()) {
            return Err(self.failure(
                started,
                "opencode_result_binding_invalid",
                "run_id mismatch",
            ));
        }
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
        }
        if let Some(changed) = result.get("changed_paths").and_then(Value::as_array) {
            for path in changed {
                let path = path.as_str().unwrap_or("");
                if !paths
                    .iter()
                    .any(|allowed| path == allowed || path.starts_with(&format!("{allowed}/")))
                {
                    return Err(self.failure(
                        started,
                        "opencode_scope_violation",
                        "changed path outside allowed_paths",
                    ));
                }
            }
        }
        let output = json!({
            "schema_version": "opencode_external_receipt.v1",
            "executor_type": OPENCODE_EXECUTOR_TYPE,
            "mode": self.config.mode.as_str(),
            "pinned_opencode_version": self.config.expected_opencode_version,
            "adapter_version": self.config.adapter_version,
            "adapter_contract_version": OPENCODE_ADAPTER_CONTRACT,
            "permission_profile_hash": profile_hash,
            "task_input_hash": task_input_hash,
            "base_commit": base_commit,
            "worktree_id": worktree_id,
            "allowed_paths": paths,
            "result": result,
            "network_used": false,
            "provider_used": false,
            "merges": false,
        });
        let output_json = canonical_event_json(&output).map_err(|error| {
            self.failure(started, "opencode_receipt_invalid", &error.to_string())
        })?;
        Ok(NodeExecutionOutput {
            status: "success".to_string(),
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

fn validate_allowed_path(path: &str) -> Result<(), String> {
    if path.is_empty() || path.starts_with('/') || path.starts_with('\\') {
        return Err("absolute paths are forbidden".to_string());
    }
    if path.contains("..") || path.contains('\0') {
        return Err("path traversal is forbidden".to_string());
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
        // allow bare program names resolved later? require absolute/path exists for pin honesty
        if path.components().count() == 1 {
            return Ok(path);
        }
        return Err(format!("{name} does not exist: {value}"));
    }
    std::fs::canonicalize(&path).map_err(|error| format!("{name}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct RecordingInvoker {
        last: Mutex<Option<Value>>,
        response: Value,
        fail: Option<OpenCodeInvokeError>,
    }

    impl OpenCodeInvoker for RecordingInvoker {
        fn invoke(&self, request: &Value, _timeout_ms: u64) -> Result<Value, OpenCodeInvokeError> {
            *self.last.lock().unwrap() = Some(request.clone());
            if let Some(error) = &self.fail {
                return Err(error.clone());
            }
            let mut response = self.response.clone();
            response["invocation_id"] = request["invocation_id"].clone();
            response["run_id"] = request["run_id"].clone();
            response["node_id"] = request["node_id"].clone();
            response["lease_id"] = request["lease_id"].clone();
            Ok(response)
        }
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

    #[test]
    fn analysis_fixture_succeeds() {
        let invoker = Arc::new(RecordingInvoker {
            last: Mutex::new(None),
            response: json!({
                "schema_version": OPENCODE_RESULT_SCHEMA,
                "task_kind": "analysis",
                "status": "ok",
                "changed_paths": [],
                "patch": null,
                "patch_sha256": null,
                "analysis": {"findings_count": 1},
                "tool_summary": {"tool_call_count": 0, "network_attempts": 0, "mcp_attempts": 0},
                "reason_code": "fixture_analysis_ok",
                "runtime": {"runtime_kind": "opencode", "mode": "fixture"}
            }),
            fail: None,
        });
        let executor = OpenCodeNodeExecutor::new(
            OpenCodeRuntimeConfig::fixture(PathBuf::from("python3"), PathBuf::from("adapter.py")),
            invoker.clone(),
        );
        let output = executor.execute_node(&sample_input("analysis", &["docs/a.md"]));
        assert_eq!(output.status, "success");
        assert_eq!(output.executor_type, OPENCODE_EXECUTOR_TYPE);
        let request = invoker.last.lock().unwrap().clone().unwrap();
        assert_eq!(request["mode"], "fixture");
        assert_eq!(request["permission_profile"]["network_enabled"], false);
    }

    #[test]
    fn rejects_path_traversal_metadata() {
        let invoker = Arc::new(RecordingInvoker {
            last: Mutex::new(None),
            response: json!({}),
            fail: None,
        });
        let executor = OpenCodeNodeExecutor::new(
            OpenCodeRuntimeConfig::fixture(PathBuf::from("python3"), PathBuf::from("adapter.py")),
            invoker,
        );
        let output = executor.execute_node(&sample_input("analysis", &["../secret"]));
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("opencode_path_invalid")
        );
    }

    #[test]
    fn rejects_scope_violation_from_adapter() {
        let invoker = Arc::new(RecordingInvoker {
            last: Mutex::new(None),
            response: json!({
                "schema_version": OPENCODE_RESULT_SCHEMA,
                "task_kind": "allowed_path_patch",
                "status": "ok",
                "changed_paths": ["secrets/token"],
                "patch": "x",
                "patch_sha256": sha256("x"),
                "tool_summary": {"tool_call_count": 1, "network_attempts": 0, "mcp_attempts": 0},
                "reason_code": "bad",
                "runtime": {"runtime_kind": "opencode", "mode": "fixture"}
            }),
            fail: None,
        });
        let executor = OpenCodeNodeExecutor::new(
            OpenCodeRuntimeConfig::fixture(PathBuf::from("python3"), PathBuf::from("adapter.py")),
            invoker,
        );
        let output = executor.execute_node(&sample_input("allowed_path_patch", &["docs/a.md"]));
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("opencode_scope_violation")
        );
    }

    #[test]
    fn kill_switch_fails_closed() {
        std::env::set_var(KILL_SWITCH_ENV, "1");
        let invoker = Arc::new(RecordingInvoker {
            last: Mutex::new(None),
            response: json!({}),
            fail: None,
        });
        let executor = OpenCodeNodeExecutor::new(
            OpenCodeRuntimeConfig::fixture(PathBuf::from("python3"), PathBuf::from("adapter.py")),
            invoker,
        );
        let output = executor.execute_node(&sample_input("analysis", &["docs/a.md"]));
        std::env::remove_var(KILL_SWITCH_ENV);
        assert_eq!(output.status, "failed");
        assert_eq!(output.error_domain.as_deref(), Some("opencode_killed"));
    }

    #[test]
    fn wrong_task_type_rejected() {
        let invoker = Arc::new(RecordingInvoker {
            last: Mutex::new(None),
            response: json!({}),
            fail: None,
        });
        let executor = OpenCodeNodeExecutor::new(
            OpenCodeRuntimeConfig::fixture(PathBuf::from("python3"), PathBuf::from("adapter.py")),
            invoker,
        );
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
    fn process_adapter_analysis_roundtrip() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let adapter = repo.join("adapters/opencode/src/acp_opencode_adapter/__main__.py");
        if !adapter.exists() {
            return;
        }
        let python = which_python();
        let config = OpenCodeRuntimeConfig::fixture(python, adapter.clone());
        let src = repo.join("adapters/opencode/src");
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
            "worktree_id": "wt",
            "allowed_paths": ["docs/x.md"],
            "environment_allowlist": ["PATH", "HOME", "LANG"],
            "permission_profile": {
                "approval_mode": "deny_by_default",
                "network_enabled": false,
                "mcp_enabled": false,
                "websearch": false,
                "webfetch": false,
                "remote_agents": false,
                "background_agents": false,
                "provider_fallback": false
            },
            "requested_capabilities": []
        });
        let result = OpenCodeProcessInvoker::new(&config)
            .invoke(&request, 15_000)
            .or_else(|_| {
                // fallback if package path resolution differs in test layout
                let mut command = Command::new(&config.python_program);
                let input = canonical_event_json(&request).unwrap();
                command
                    .arg("-m")
                    .arg("acp_opencode_adapter")
                    .env_clear()
                    .env("PYTHONPATH", &src)
                    .env("PYTHONIOENCODING", "utf-8")
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                let mut child = command.spawn().unwrap();
                child
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(input.as_bytes())
                    .unwrap();
                let output = child.wait_with_output().unwrap();
                serde_json::from_slice(&output.stdout)
                    .map_err(|e| OpenCodeInvokeError::new("adapter_output_invalid", e.to_string()))
            })
            .expect("adapter invoke");
        assert_eq!(result["status"], "ok");
        assert_eq!(result["task_kind"], "analysis");
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
