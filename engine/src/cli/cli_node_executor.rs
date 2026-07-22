use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;

use crate::cli::{spawn_with_timeout, SpawnWithTimeoutError};
use crate::node_executor::{
    exit_status_signal, process_outcome_from_exit_status, NodeExecutionInput, NodeExecutionOutput,
    NodeExecutor, ProcessOutcome,
};
use crate::provider::redaction::redact_sensitive_patterns;

/// CLI-backed NodeExecutor for the managed Codex CLI process.
///
/// Gated behind `ACP_ENABLE_CLI_EXECUTION=1`. Reads `node_metadata` for:
/// - `prompt` or `command`: the task text to send to the CLI
/// - `executor`: `"codex_cli"`
/// - `model`: optional model override
/// - `workspace_path`: cwd for the subprocess
pub struct CliNodeExecutor {
    pub claude_bin: Option<String>,
    pub codex_bin: Option<String>,
    pub timeout_ms: u64,
    pub default_executor: String,
}

impl CliNodeExecutor {
    pub fn new(claude_bin: Option<String>, codex_bin: Option<String>, timeout_ms: u64) -> Self {
        let default_executor = if claude_bin.is_some() {
            "claude_code_cli".to_string()
        } else if codex_bin.is_some() {
            "codex_cli".to_string()
        } else {
            "claude_code_cli".to_string()
        };
        Self {
            claude_bin,
            codex_bin,
            timeout_ms,
            default_executor,
        }
    }

    pub fn from_config_for(config: &super::config::CliConfig, executor_type: &str) -> Option<Self> {
        if !config.enabled {
            return None;
        }
        match executor_type {
            "codex_cli" if config.codex_enabled => config
                .codex_bin
                .clone()
                .map(|binary| Self::new(None, Some(binary), config.timeout_ms)),
            _ => None,
        }
    }

    pub fn resolve_executor(&self, input: &NodeExecutionInput) -> String {
        input
            .node_metadata
            .get("executor")
            .and_then(|v| v.as_str())
            .unwrap_or(&self.default_executor)
            .to_string()
    }

    pub fn resolve_prompt(&self, input: &NodeExecutionInput) -> String {
        input
            .node_metadata
            .get("prompt")
            .or_else(|| input.node_metadata.get("command"))
            .and_then(|v| v.as_str())
            .unwrap_or("echo noop")
            .to_string()
    }

    pub fn resolve_model(&self, input: &NodeExecutionInput) -> Option<String> {
        input
            .node_metadata
            .get("model")
            .and_then(|v| v.as_str())
            .map(String::from)
    }

    pub fn resolve_cwd(&self, input: &NodeExecutionInput) -> Result<PathBuf, String> {
        let raw = input
            .node_metadata
            .get("workspace_path")
            .and_then(|v| v.as_str())
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "CLI execution requires a bound workspace_path".to_string())?;
        let path = Path::new(raw);
        if !path.is_absolute()
            || path
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err("workspace_path must be an absolute clean path".to_string());
        }
        let canonical = std::fs::canonicalize(path)
            .map_err(|error| format!("workspace_path is unavailable: {error}"))?;
        if !canonical.is_dir() {
            return Err("workspace_path must be a directory".to_string());
        }
        if let Some(root) = input
            .node_metadata
            .get("workspace_root")
            .and_then(Value::as_str)
        {
            let root = std::fs::canonicalize(root)
                .map_err(|error| format!("workspace_root is unavailable: {error}"))?;
            if !canonical.starts_with(&root) {
                return Err("workspace_path escapes workspace_root".to_string());
            }
        }
        Ok(canonical)
    }
}

impl NodeExecutor for CliNodeExecutor {
    fn executor_type_name(&self) -> &str {
        "cli"
    }
    fn execute_node(&self, input: &NodeExecutionInput) -> NodeExecutionOutput {
        let start = std::time::Instant::now();
        let executor_type = self.resolve_executor(input);
        if executor_type == "claude_code_cli" {
            return NodeExecutionOutput {
                status: "failed".to_string(),
                executor_type,
                output: None,
                error_domain: Some("cli_executor_unsupported".to_string()),
                error_message: Some(
                    "claude_code_cli is unavailable because nested tool calls cannot be mediated by the app-owned workspace and tool-policy boundary"
                        .to_string(),
                ),
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(start.elapsed().as_millis() as i64),
            process_outcome: None,
            };
        }
        let prompt = match product_execution_prompt(input, &self.resolve_prompt(input)) {
            Ok(prompt) => prompt,
            Err(error) => {
                return NodeExecutionOutput {
                    status: "failed".to_string(),
                    executor_type,
                    output: None,
                    error_domain: Some("cli_execution_authority_invalid".to_string()),
                    error_message: Some(error),
                    input_tokens: None,
                    output_tokens: None,
                    estimated_cost: None,
                    latency_ms: Some(start.elapsed().as_millis() as i64),
                    process_outcome: None,
                };
            }
        };
        let cwd = match self.resolve_cwd(input) {
            Ok(cwd) => cwd,
            Err(error) => {
                return NodeExecutionOutput {
                    status: "failed".to_string(),
                    executor_type: self.resolve_executor(input),
                    output: None,
                    error_domain: Some("cli_workspace_required".to_string()),
                    error_message: Some(error),
                    input_tokens: None,
                    output_tokens: None,
                    estimated_cost: None,
                    latency_ms: Some(start.elapsed().as_millis() as i64),
                    process_outcome: None,
                };
            }
        };

        let (bin_path, effective_type) = match executor_type.as_str() {
            "codex_cli" => match &self.codex_bin {
                Some(bin) => (bin.clone(), "codex_cli"),
                None => {
                    return NodeExecutionOutput {
                        status: "failed".to_string(),
                        executor_type: "codex_cli".to_string(),
                        output: None,
                        error_domain: Some("cli_not_found".to_string()),
                        error_message: Some("codex binary not configured".to_string()),
                        input_tokens: None,
                        output_tokens: None,
                        estimated_cost: None,
                        latency_ms: Some(start.elapsed().as_millis() as i64),
                        process_outcome: None,
                    };
                }
            },
            other => {
                return NodeExecutionOutput {
                    status: "failed".to_string(),
                    executor_type: other.to_string(),
                    output: None,
                    error_domain: Some("unknown_cli_executor".to_string()),
                    error_message: Some(format!("unknown CLI executor type: {other}")),
                    input_tokens: None,
                    output_tokens: None,
                    estimated_cost: None,
                    latency_ms: Some(start.elapsed().as_millis() as i64),
                    process_outcome: None,
                };
            }
        };

        let mut cmd = Command::new(&bin_path);
        match effective_type {
            "codex_cli" => {
                cmd.args(codex_invocation_args(&cwd, &prompt));
            }
            _ => unreachable!(),
        }
        cmd.current_dir(&cwd)
            .env_clear()
            .env(
                "PATH",
                std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string()),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for key in cli_env_allowlist() {
            if let Ok(value) = std::env::var(&key) {
                cmd.env(key, value);
            }
        }

        let output = spawn_with_timeout(&mut cmd, self.timeout_ms);
        let elapsed_ms = start.elapsed().as_millis() as i64;

        match output {
            Ok(output) => {
                let process_outcome = process_outcome_from_exit_status(&output.status);
                if !output.status.success() {
                    let stderr =
                        redact_sensitive_patterns(&String::from_utf8_lossy(&output.stderr));
                    let stdout =
                        redact_sensitive_patterns(&String::from_utf8_lossy(&output.stdout));
                    let msg = if !stderr.is_empty() {
                        stderr
                    } else {
                        format!("exit code: {}", output.status.code().unwrap_or(-1))
                    };
                    return NodeExecutionOutput {
                        status: "failed".to_string(),
                        executor_type: effective_type.to_string(),
                        output: if stdout.is_empty() {
                            None
                        } else {
                            Some(stdout)
                        },
                        error_domain: Some("cli_execution_error".to_string()),
                        error_message: Some(msg),
                        input_tokens: None,
                        output_tokens: None,
                        estimated_cost: None,
                        latency_ms: Some(elapsed_ms),
                        process_outcome: Some(process_outcome),
                    };
                }

                let stdout = redact_sensitive_patterns(&String::from_utf8_lossy(&output.stdout));
                parse_cli_output(&stdout, effective_type, elapsed_ms, process_outcome)
            }
            Err(error) => {
                let (domain, msg, process_outcome) = match error {
                    SpawnWithTimeoutError::SpawnFailed => (
                        "cli_not_found",
                        format!("failed to spawn {effective_type}"),
                        ProcessOutcome::failure(
                            "spawn_failed",
                            None,
                            "managed CLI OS process did not start",
                        ),
                    ),
                    SpawnWithTimeoutError::TimedOut {
                        elapsed_ms,
                        terminated_status,
                    } => (
                        "cli_timeout",
                        format!(
                            "{effective_type} timed out after {elapsed_ms}ms (limit {}ms)",
                            self.timeout_ms
                        ),
                        ProcessOutcome::failure(
                            "timed_out",
                            terminated_status.as_ref().and_then(exit_status_signal),
                            "timeout has no successful OS exit code",
                        ),
                    ),
                    SpawnWithTimeoutError::WaitFailed {
                        elapsed_ms,
                        observed_status,
                    } => {
                        let mut outcome = observed_status
                            .as_ref()
                            .map(process_outcome_from_exit_status)
                            .unwrap_or_else(|| {
                                ProcessOutcome::failure(
                                    "wait_failed",
                                    None,
                                    "managed CLI wait failed before an exit code was available",
                                )
                            });
                        outcome.state = "wait_failed".to_string();
                        outcome.unavailable_reason = Some(
                            "managed CLI wait/output collection failed after process spawn"
                                .to_string(),
                        );
                        (
                            "cli_wait_error",
                            format!(
                                "{effective_type} wait/output collection failed after {elapsed_ms}ms"
                            ),
                            outcome,
                        )
                    }
                };
                NodeExecutionOutput {
                    status: "failed".to_string(),
                    executor_type: effective_type.to_string(),
                    output: None,
                    error_domain: Some(domain.to_string()),
                    error_message: Some(msg),
                    input_tokens: None,
                    output_tokens: None,
                    estimated_cost: None,
                    latency_ms: Some(elapsed_ms),
                    process_outcome: Some(process_outcome),
                }
            }
        }
    }
}

fn product_execution_prompt(input: &NodeExecutionInput, objective: &str) -> Result<String, String> {
    let managed = input.node_metadata.get("managed_supervised_patch");
    let is_product_apply = managed
        .and_then(Value::as_object)
        .and_then(|binding| binding.get("operation"))
        .and_then(Value::as_str)
        == Some("product_apply");
    if !is_product_apply {
        return Ok(objective.to_string());
    }

    let task_id = input
        .node_metadata
        .get("product_task_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "product apply task identity is missing".to_string())?;
    let allowed_paths = input
        .node_metadata
        .get("allowed_paths")
        .and_then(Value::as_array)
        .filter(|paths| {
            !paths.is_empty()
                && paths
                    .iter()
                    .all(|path| path.as_str().is_some_and(|value| !value.trim().is_empty()))
        })
        .ok_or_else(|| "product apply allowed-path authority is missing".to_string())?;
    let allowed_paths = allowed_paths
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join("\n- ");

    Ok(format!(
        "The control plane has already authorized this bounded workspace-apply execution for product task {task_id}. Do not request or wait for another execution approval. This authorization covers only edits inside the bound workspace and only the allowed paths below. It does not approve the artifact, confirm target output, authorize a branch push, or authorize pull-request creation. Complete the objective now, verify the requested change, and do not modify any other path.\n\nAllowed paths:\n- {allowed_paths}\n\nObjective:\n{objective}"
    ))
}

fn codex_invocation_args(cwd: &Path, prompt: &str) -> Vec<OsString> {
    vec![
        "--ask-for-approval".into(),
        "never".into(),
        "exec".into(),
        "--json".into(),
        "--sandbox".into(),
        "workspace-write".into(),
        "--cd".into(),
        cwd.as_os_str().to_os_string(),
        "--ephemeral".into(),
        prompt.into(),
    ]
}

fn cli_env_allowlist() -> Vec<String> {
    std::env::var("ACP_CLI_ENV_ALLOWLIST")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn parse_cli_output(
    raw: &str,
    executor_type: &str,
    latency_ms: i64,
    process_outcome: ProcessOutcome,
) -> NodeExecutionOutput {
    let raw = redact_sensitive_patterns(raw);
    if executor_type == "codex_cli" {
        return parse_codex_jsonl(&raw, latency_ms, process_outcome);
    }
    let parsed: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(err) => {
            return NodeExecutionOutput {
                status: "failed".to_string(),
                executor_type: executor_type.to_string(),
                output: if raw.is_empty() { None } else { Some(raw) },
                error_domain: Some("cli_output_parse_error".to_string()),
                error_message: Some(format!("failed to parse CLI JSON output: {err}")),
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(latency_ms),
                process_outcome: Some(process_outcome),
            };
        }
    };

    let output_text = parsed
        .get("result")
        .or_else(|| parsed.get("output"))
        .or_else(|| parsed.get("content"))
        .and_then(|v| v.as_str())
        .unwrap_or(&raw)
        .to_string();
    let output_text = redact_sensitive_patterns(&output_text);

    let input_tokens = parsed
        .get("usage")
        .and_then(|u| u.get("input_tokens").or_else(|| u.get("prompt_tokens")))
        .and_then(|v| v.as_i64());

    let output_tokens = parsed
        .get("usage")
        .and_then(|u| {
            u.get("output_tokens")
                .or_else(|| u.get("completion_tokens"))
        })
        .and_then(|v| v.as_i64());

    NodeExecutionOutput {
        status: "completed".to_string(),
        executor_type: executor_type.to_string(),
        output: Some(output_text),
        error_domain: None,
        error_message: None,
        input_tokens,
        output_tokens,
        // The installed CLI adapter has no exact app-bound model, pricing source,
        // or pricing effective date. Preserve unavailable cost instead of
        // fabricating zero or a hard-coded harness estimate.
        estimated_cost: None,
        latency_ms: Some(latency_ms),
        process_outcome: Some(process_outcome),
    }
}

fn parse_codex_jsonl(
    raw: &str,
    latency_ms: i64,
    process_outcome: ProcessOutcome,
) -> NodeExecutionOutput {
    let mut output = None;
    let mut input_tokens = None;
    let mut output_tokens = None;
    let mut completed = false;
    let mut failure = None;

    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        let event: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(error) => {
                return NodeExecutionOutput {
                    status: "failed".to_string(),
                    executor_type: "codex_cli".to_string(),
                    output: Some(raw.to_string()),
                    error_domain: Some("cli_output_parse_error".to_string()),
                    error_message: Some(format!("failed to parse Codex JSONL output: {error}")),
                    input_tokens: None,
                    output_tokens: None,
                    estimated_cost: None,
                    latency_ms: Some(latency_ms),
                    process_outcome: Some(process_outcome),
                };
            }
        };
        match event.get("type").and_then(Value::as_str) {
            Some("item.completed")
                if event
                    .get("item")
                    .and_then(|item| item.get("type"))
                    .and_then(Value::as_str)
                    == Some("agent_message") =>
            {
                output = event
                    .get("item")
                    .and_then(|item| item.get("text"))
                    .and_then(Value::as_str)
                    .map(redact_sensitive_patterns);
            }
            Some("turn.completed") => {
                completed = true;
                let usage = event.get("usage");
                input_tokens = usage
                    .and_then(|value| value.get("input_tokens"))
                    .and_then(Value::as_i64);
                output_tokens = usage
                    .and_then(|value| value.get("output_tokens"))
                    .and_then(Value::as_i64);
            }
            Some("turn.failed") => {
                failure = event
                    .get("error")
                    .and_then(|value| value.get("message"))
                    .and_then(Value::as_str)
                    .map(redact_sensitive_patterns)
                    .or_else(|| Some("Codex turn failed".to_string()));
            }
            Some("error") => {
                failure = event
                    .get("message")
                    .and_then(Value::as_str)
                    .map(redact_sensitive_patterns)
                    .or_else(|| Some("Codex execution error".to_string()));
            }
            _ => {}
        }
    }

    if let Some(message) = failure {
        return NodeExecutionOutput {
            status: "failed".to_string(),
            executor_type: "codex_cli".to_string(),
            output,
            error_domain: Some("cli_execution_error".to_string()),
            error_message: Some(message),
            input_tokens,
            output_tokens,
            estimated_cost: None,
            latency_ms: Some(latency_ms),
            process_outcome: Some(process_outcome),
        };
    }
    if !completed {
        return NodeExecutionOutput {
            status: "failed".to_string(),
            executor_type: "codex_cli".to_string(),
            output,
            error_domain: Some("cli_output_parse_error".to_string()),
            error_message: Some("Codex JSONL output did not include turn.completed".to_string()),
            input_tokens,
            output_tokens,
            estimated_cost: None,
            latency_ms: Some(latency_ms),
            process_outcome: Some(process_outcome),
        };
    }

    NodeExecutionOutput {
        status: "completed".to_string(),
        executor_type: "codex_cli".to_string(),
        output,
        error_domain: None,
        error_message: None,
        input_tokens,
        output_tokens,
        estimated_cost: None,
        latency_ms: Some(latency_ms),
        process_outcome: Some(process_outcome),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_input(metadata: Value) -> NodeExecutionInput {
        NodeExecutionInput {
            node_id: "node-test".to_string(),
            task_type: "cli_task".to_string(),
            run_id: "run-test".to_string(),
            workflow_id: "wf-test".to_string(),
            node_metadata: metadata,
        }
    }

    fn successful_process() -> ProcessOutcome {
        ProcessOutcome::exited(0)
    }

    #[cfg(unix)]
    fn fake_codex(workspace: &Path, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let binary = workspace.join("codex-fixture");
        std::fs::write(&binary, format!("#!/bin/sh\n{body}\n")).unwrap();
        let mut permissions = std::fs::metadata(&binary).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&binary, permissions).unwrap();
        binary
    }

    #[test]
    fn claude_cli_is_rejected_before_binary_or_process_access() {
        let workspace = tempfile::tempdir().unwrap();
        let executor = CliNodeExecutor::new(None, None, 5000);
        let input = make_input(json!({
            "prompt": "hello",
            "workspace_path": workspace.path(),
        }));
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("cli_executor_unsupported")
        );
    }

    #[test]
    fn test_cli_node_executor_unknown_executor_type() {
        let workspace = tempfile::tempdir().unwrap();
        let executor = CliNodeExecutor::new(None, Some("/bin/codex".into()), 5000);
        let input = make_input(json!({
            "prompt": "hello",
            "executor": "unknown_type",
            "workspace_path": workspace.path(),
        }));
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "failed");
        assert_eq!(output.error_domain.as_deref(), Some("unknown_cli_executor"));
    }

    #[cfg(unix)]
    #[test]
    fn managed_cli_preserves_successful_process_exit() {
        let workspace = tempfile::tempdir().unwrap();
        let binary = fake_codex(
            workspace.path(),
            "printf '%s\\n' '{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":3,\"output_tokens\":1}}'",
        );
        let executor =
            CliNodeExecutor::new(None, Some(binary.to_string_lossy().into_owned()), 5_000);
        let output = executor.execute_node(&make_input(json!({
            "executor": "codex_cli",
            "prompt": "bounded fixture",
            "workspace_path": workspace.path(),
        })));
        assert_eq!(output.status, "completed");
        assert_eq!(output.process_outcome.as_ref().unwrap().state, "exited");
        assert_eq!(output.process_outcome.as_ref().unwrap().exit_code, Some(0));
    }

    #[cfg(unix)]
    #[test]
    fn managed_cli_preserves_nonzero_process_exit_other_than_one() {
        let workspace = tempfile::tempdir().unwrap();
        let binary = fake_codex(workspace.path(), "printf 'bounded failure' >&2\nexit 7");
        let executor =
            CliNodeExecutor::new(None, Some(binary.to_string_lossy().into_owned()), 5_000);
        let output = executor.execute_node(&make_input(json!({
            "executor": "codex_cli",
            "prompt": "bounded fixture",
            "workspace_path": workspace.path(),
        })));
        assert_eq!(output.status, "failed");
        assert_eq!(output.process_outcome.as_ref().unwrap().state, "exited");
        assert_eq!(output.process_outcome.as_ref().unwrap().exit_code, Some(7));
    }

    #[cfg(unix)]
    #[test]
    fn managed_cli_timeout_has_no_fabricated_exit_code() {
        let workspace = tempfile::tempdir().unwrap();
        let binary = fake_codex(workspace.path(), "sleep 5");
        let executor = CliNodeExecutor::new(None, Some(binary.to_string_lossy().into_owned()), 50);
        let output = executor.execute_node(&make_input(json!({
            "executor": "codex_cli",
            "prompt": "bounded fixture",
            "workspace_path": workspace.path(),
        })));
        assert_eq!(output.status, "failed");
        assert_eq!(output.error_domain.as_deref(), Some("cli_timeout"));
        assert_eq!(output.process_outcome.as_ref().unwrap().state, "timed_out");
        assert_eq!(output.process_outcome.as_ref().unwrap().exit_code, None);
    }

    #[test]
    fn managed_cli_spawn_failure_is_explicit() {
        let workspace = tempfile::tempdir().unwrap();
        let executor = CliNodeExecutor::new(
            None,
            Some(
                workspace
                    .path()
                    .join("missing-codex")
                    .to_string_lossy()
                    .into_owned(),
            ),
            5_000,
        );
        let output = executor.execute_node(&make_input(json!({
            "executor": "codex_cli",
            "prompt": "bounded fixture",
            "workspace_path": workspace.path(),
        })));
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.process_outcome.as_ref().unwrap().state,
            "spawn_failed"
        );
        assert_eq!(output.process_outcome.as_ref().unwrap().exit_code, None);
    }

    #[test]
    fn cli_executor_never_defaults_to_control_plane_working_directory() {
        let executor = CliNodeExecutor::new(None, Some("/bin/codex".into()), 5000);
        let output = executor.execute_node(&make_input(json!({"prompt": "hello"})));

        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("cli_workspace_required")
        );
    }

    #[test]
    fn test_cli_node_executor_resolves_prompt_from_command() {
        let executor = CliNodeExecutor::new(None, None, 5000);
        let input = make_input(json!({"command": "echo test"}));
        assert_eq!(executor.resolve_prompt(&input), "echo test");
    }

    #[test]
    fn test_cli_node_executor_resolves_prompt_from_prompt() {
        let executor = CliNodeExecutor::new(None, None, 5000);
        let input = make_input(json!({"prompt": "do something"}));
        assert_eq!(executor.resolve_prompt(&input), "do something");
    }

    #[test]
    fn test_cli_node_executor_default_executor_from_claude() {
        let executor = CliNodeExecutor::new(Some("/bin/claude".into()), None, 5000);
        let input = make_input(json!({}));
        assert_eq!(executor.resolve_executor(&input), "claude_code_cli");
    }

    #[test]
    fn test_cli_node_executor_default_executor_from_codex() {
        let executor = CliNodeExecutor::new(None, Some("/bin/codex".into()), 5000);
        let input = make_input(json!({}));
        assert_eq!(executor.resolve_executor(&input), "codex_cli");
    }

    #[test]
    fn product_apply_prompt_records_execution_authority_without_output_authority() {
        let input = make_input(json!({
            "product_task_id": "product-task-1",
            "allowed_paths": ["docs/managed.md"],
            "managed_supervised_patch": {
                "operation": "product_apply"
            }
        }));

        let prompt = product_execution_prompt(&input, "Create the requested file").unwrap();
        assert!(prompt.contains("already authorized"));
        assert!(prompt.contains("Do not request or wait for another execution approval"));
        assert!(prompt.contains("does not approve the artifact"));
        assert!(prompt.contains("docs/managed.md"));
        assert!(prompt.ends_with("Create the requested file"));
    }

    #[test]
    fn codex_invocation_is_noninteractive_but_keeps_workspace_write_sandbox() {
        let args = codex_invocation_args(Path::new("/tmp/bound-workspace"), "bounded objective");
        let args = args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(
            args,
            vec![
                "--ask-for-approval",
                "never",
                "exec",
                "--json",
                "--sandbox",
                "workspace-write",
                "--cd",
                "/tmp/bound-workspace",
                "--ephemeral",
                "bounded objective",
            ]
        );
        assert!(!args.iter().any(|value| value == "danger-full-access"));
        assert!(!args
            .iter()
            .any(|value| value == "--dangerously-bypass-approvals-and-sandbox"));
    }

    #[test]
    fn test_parse_codex_jsonl_returns_last_agent_message() {
        let output = parse_cli_output(
            "{\"type\":\"thread.started\",\"thread_id\":\"thread-1\"}\n\
             {\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"READY\"}}\n\
             {\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":10,\"output_tokens\":2}}\n",
            "codex_cli",
            25,
            successful_process(),
        );

        assert_eq!(output.status, "completed");
        assert_eq!(output.output.as_deref(), Some("READY"));
        assert_eq!(output.input_tokens, Some(10));
        assert_eq!(output.output_tokens, Some(2));
    }

    #[test]
    fn test_parse_codex_jsonl_surfaces_turn_failure() {
        let output = parse_cli_output(
            "{\"type\":\"turn.failed\",\"error\":{\"message\":\"usage limit\"}}\n",
            "codex_cli",
            25,
            successful_process(),
        );

        assert_eq!(output.status, "failed");
        assert_eq!(output.error_domain.as_deref(), Some("cli_execution_error"));
        assert_eq!(output.error_message.as_deref(), Some("usage limit"));
    }

    #[test]
    fn test_parse_cli_output_success() {
        let raw = r#"{"result":"hello world","usage":{"input_tokens":100,"output_tokens":50}}"#;
        let output = parse_cli_output(raw, "claude_code_cli", 100, successful_process());
        assert_eq!(output.status, "completed");
        assert_eq!(output.output.as_deref(), Some("hello world"));
        assert_eq!(output.input_tokens, Some(100));
        assert_eq!(output.output_tokens, Some(50));
        assert_eq!(output.estimated_cost, None);
    }

    #[test]
    fn test_parse_cli_output_malformed_json() {
        let output = parse_cli_output("not-json", "codex_cli", 50, successful_process());
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("cli_output_parse_error")
        );
    }

    #[test]
    fn test_parse_cli_output_fallback_to_raw() {
        let raw = r#"{"id":"req-123","usage":{}}"#;
        let output = parse_cli_output(raw, "claude_code_cli", 50, successful_process());
        assert_eq!(output.status, "completed");
        assert_eq!(output.output.as_deref(), Some(raw));
    }

    #[test]
    fn test_parse_cli_output_redacts_secret_like_output() {
        let raw = r#"{"result":"api_key=sk-abcdefghijklmnopqrstuvwxyz","usage":{}}"#;
        let output = parse_cli_output(raw, "claude_code_cli", 50, successful_process());
        assert_eq!(output.status, "completed");
        assert!(!output
            .output
            .as_deref()
            .unwrap_or("")
            .contains("sk-abcdefghijklmnopqrstuvwxyz"));
    }

    #[test]
    fn test_cli_env_allowlist_defaults_empty() {
        std::env::remove_var("ACP_CLI_ENV_ALLOWLIST");
        assert!(cli_env_allowlist().is_empty());
    }

    #[test]
    fn test_from_config_disabled() {
        let config = super::super::config::CliConfig {
            enabled: false,
            claude_code_bin: Some("/bin/claude".into()),
            claude_code_enabled: true,
            codex_bin: None,
            codex_enabled: false,
            timeout_ms: 5000,
        };
        assert!(CliNodeExecutor::from_config_for(&config, "claude_code_cli").is_none());
    }

    #[test]
    fn test_from_config_no_binaries() {
        let config = super::super::config::CliConfig {
            enabled: true,
            claude_code_bin: None,
            claude_code_enabled: false,
            codex_bin: None,
            codex_enabled: false,
            timeout_ms: 5000,
        };
        assert!(CliNodeExecutor::from_config_for(&config, "codex_cli").is_none());
    }

    #[test]
    fn from_config_rejects_unsandboxed_claude_even_when_supplied() {
        let config = super::super::config::CliConfig {
            enabled: true,
            claude_code_bin: Some("/bin/claude".into()),
            claude_code_enabled: true,
            codex_bin: None,
            codex_enabled: false,
            timeout_ms: 5000,
        };
        assert!(CliNodeExecutor::from_config_for(&config, "claude_code_cli").is_none());
    }

    #[test]
    fn from_config_for_registers_only_exact_sandboxed_codex_identity() {
        let config = super::super::config::CliConfig {
            enabled: true,
            claude_code_bin: Some("/bin/claude".into()),
            claude_code_enabled: true,
            codex_bin: Some("/bin/codex".into()),
            codex_enabled: true,
            timeout_ms: 5000,
        };

        assert!(CliNodeExecutor::from_config_for(&config, "claude_code_cli").is_none());

        let codex = CliNodeExecutor::from_config_for(&config, "codex_cli").unwrap();
        assert_eq!(codex.default_executor, "codex_cli");
        assert_eq!(codex.codex_bin.as_deref(), Some("/bin/codex"));
        assert!(codex.claude_bin.is_none());
    }

    #[test]
    fn missing_exact_cli_pricing_never_fabricates_zero_cost() {
        let output = parse_cli_output(
            "{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":100,\"output_tokens\":50}}\n",
            "codex_cli",
            1,
            successful_process(),
        );
        assert_eq!(output.input_tokens, Some(100));
        assert_eq!(output.output_tokens, Some(50));
        assert_eq!(output.estimated_cost, None);
    }
}
