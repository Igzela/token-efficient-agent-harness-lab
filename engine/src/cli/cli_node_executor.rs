use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;

use super::codex_budget_authority::{
    authority_from_product_metadata, write_ephemeral_codex_home, CodexBudgetAuthority,
    CodexBudgetGateway, CodexExecutableIdentity,
};
use super::codex_session_usage::{
    discover_rollout_files, import_managed_codex_home, rollup_to_product_evidence,
    root_thread_id_from_file,
};
use super::config::{ClaudeCodeAdmission, CodexAdmission};
use crate::cli::{spawn_with_timeout, SpawnWithTimeoutError};
use crate::node_executor::{
    exit_status_signal, process_outcome_from_exit_status, NodeExecutionInput, NodeExecutionOutput,
    NodeExecutor, ProcessOutcome,
};
use crate::provider::redaction::redact_sensitive_patterns;

/// CLI-backed NodeExecutor for admitted managed coding CLI processes.
///
/// Gated behind `ACP_ENABLE_CLI_EXECUTION=1`. Reads `node_metadata` for:
/// - `prompt` or `command`: the task text to send to the CLI
/// - `executor`: `"codex_cli"` or the exactly admitted `"claude_code_cli"`
/// - `model`: optional Codex model override; Claude uses its admitted snapshot
/// - `workspace_path`: cwd for the subprocess
///
/// Product-managed Codex (`product_apply`) requires exact `CodexAdmission` and
/// routes every provider request through the app-owned loopback budget gateway.
pub struct CliNodeExecutor {
    pub claude_bin: Option<String>,
    pub codex_bin: Option<String>,
    pub timeout_ms: u64,
    pub default_executor: String,
    pub claude_admission: Option<ClaudeCodeAdmission>,
    pub codex_admission: Option<CodexAdmission>,
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
            claude_admission: None,
            codex_admission: None,
        }
    }

    fn admitted_claude(admission: ClaudeCodeAdmission, timeout_ms: u64) -> Self {
        Self {
            claude_bin: admission.binary_path.to_str().map(str::to_string),
            codex_bin: None,
            timeout_ms,
            default_executor: "claude_code_cli".to_string(),
            claude_admission: Some(admission),
            codex_admission: None,
        }
    }

    fn admitted_codex(
        admission: CodexAdmission,
        codex_bin: Option<String>,
        timeout_ms: u64,
    ) -> Self {
        Self {
            claude_bin: None,
            codex_bin: codex_bin.or_else(|| admission.binary_path.to_str().map(str::to_string)),
            timeout_ms,
            default_executor: "codex_cli".to_string(),
            claude_admission: None,
            codex_admission: Some(admission),
        }
    }

    pub fn from_config_for(config: &super::config::CliConfig, executor_type: &str) -> Option<Self> {
        if !config.enabled {
            return None;
        }
        match executor_type {
            "claude_code_cli" if config.claude_code_enabled => config
                .claude_code_admission
                .clone()
                .map(|admission| Self::admitted_claude(admission, config.timeout_ms)),
            "codex_cli" if config.codex_enabled => {
                if let Some(admission) = config.codex_admission.clone() {
                    Some(Self::admitted_codex(
                        admission,
                        config.codex_bin.clone(),
                        config.timeout_ms,
                    ))
                } else {
                    config
                        .codex_bin
                        .clone()
                        .map(|binary| Self::new(None, Some(binary), config.timeout_ms))
                }
            }
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
                    resolved_model: None,
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
                    resolved_model: None,
                };
            }
        };

        let (bin_path, effective_type) = match executor_type.as_str() {
            "claude_code_cli" => match &self.claude_admission {
                Some(admission) => {
                    if let Err(error) = validate_claude_execution_authority(input, admission) {
                        return failed_without_process(
                            "claude_code_cli",
                            "cli_execution_authority_invalid",
                            error,
                            start.elapsed().as_millis() as i64,
                        );
                    }
                    (
                        admission.binary_path.to_string_lossy().into_owned(),
                        "claude_code_cli",
                    )
                }
                None => {
                    return failed_without_process(
                        "claude_code_cli",
                        "cli_not_admitted",
                        "Claude Code has no exact runtime admission".to_string(),
                        start.elapsed().as_millis() as i64,
                    );
                }
            },
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
                        resolved_model: None,
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
                    resolved_model: None,
                };
            }
        };

        // Product-managed Codex must use the loopback budget gateway. Non-product
        // codex_cli paths retain the existing workspace-write adapter.
        if effective_type == "codex_cli" && is_product_apply(input) {
            return execute_product_codex_with_budget_gateway(
                self, input, &bin_path, &cwd, &prompt, start,
            );
        }

        let mut cmd = Command::new(&bin_path);
        match effective_type {
            "claude_code_cli" => {
                let admission = self.claude_admission.as_ref().expect("validated admission");
                let allowed_paths = input
                    .node_metadata
                    .get("allowed_paths")
                    .and_then(Value::as_array)
                    .expect("validated allowed paths");
                cmd.args(claude_invocation_args(admission, allowed_paths, &prompt));
            }
            "codex_cli" => {
                cmd.args(codex_invocation_args(&cwd, &prompt, None, false));
            }
            _ => unreachable!(),
        }
        let child_path = if effective_type == "claude_code_cli" {
            "/usr/bin:/bin".to_string()
        } else {
            std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string())
        };
        // Prompt/objective is already passed as a CLI argument for both admitted
        // Codex and Claude invocations. Keep stdin closed (EOF) so the child does
        // not block forever waiting for additional piped input.
        cmd.current_dir(&cwd)
            .env_clear()
            .env("PATH", child_path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let environment_keys = if effective_type == "claude_code_cli" {
            claude_env_allowlist()
        } else {
            cli_env_allowlist()
        };
        for key in environment_keys {
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
                        resolved_model: None,
                    };
                }

                let stdout = redact_sensitive_patterns(&String::from_utf8_lossy(&output.stdout));
                if effective_type == "claude_code_cli" {
                    parse_admitted_claude_output(
                        &stdout,
                        self.claude_admission.as_ref().expect("validated admission"),
                        elapsed_ms,
                        process_outcome,
                    )
                } else {
                    parse_cli_output(&stdout, effective_type, elapsed_ms, process_outcome)
                }
            }
            Err(error) => {
                let (domain, msg, process_outcome) = match error {
                    SpawnWithTimeoutError::SpawnFailed { kind, raw_os_error } => (
                        if kind == std::io::ErrorKind::NotFound {
                            "cli_not_found"
                        } else {
                            "cli_spawn_error"
                        },
                        format!(
                            "failed to spawn {effective_type}: kind={kind:?}, os_error={raw_os_error:?}"
                        ),
                        ProcessOutcome::failure(
                            "spawn_failed",
                            None,
                            "managed CLI OS process did not start",
                        ),
                    ),
                    SpawnWithTimeoutError::TimedOut {
                        elapsed_ms,
                        terminated_status,
                        termination,
                    } => (
                        "cli_timeout",
                        format!(
                            "{effective_type} timed out after {elapsed_ms}ms (limit {}ms); termination={}",
                            self.timeout_ms,
                            termination.summary(),
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
                        kind,
                        raw_os_error,
                        termination,
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
                                "{effective_type} wait/output collection failed after {elapsed_ms}ms; kind={kind:?}; os_error={raw_os_error:?}; termination={}",
                                termination.summary(),
                            ),
                            outcome,
                        )
                    }
                    SpawnWithTimeoutError::ReaderFailed {
                        stream,
                        kind,
                        raw_os_error,
                        termination,
                    } => (
                        match stream {
                            crate::cli::OutputStream::Stdout => "cli_stdout_reader_error",
                            crate::cli::OutputStream::Stderr => "cli_stderr_reader_error",
                            crate::cli::OutputStream::Combined => "cli_combined_reader_error",
                        },
                        format!(
                            "{effective_type} {stream:?} reader failed; kind={kind:?}; os_error={raw_os_error:?}; termination={}",
                            termination.summary(),
                        ),
                        ProcessOutcome::failure(
                            match stream {
                                crate::cli::OutputStream::Stdout => "stdout_reader_failed",
                                crate::cli::OutputStream::Stderr => "stderr_reader_failed",
                                crate::cli::OutputStream::Combined => "combined_reader_failed",
                            },
                            None,
                            "managed CLI output reader failed",
                        ),
                    ),
                    SpawnWithTimeoutError::OutputLimitExceeded {
                        details,
                        termination,
                    } => (
                        "cli_output_limit_exceeded",
                        format!(
                            "{effective_type} output limit exceeded; {}; termination={}",
                            details.summary(),
                            termination.summary(),
                        ),
                        ProcessOutcome::failure(
                            "output_limit_exceeded",
                            None,
                            "managed CLI output exceeded its bounded capture contract",
                        ),
                    ),
                    SpawnWithTimeoutError::ProcessTreeCleanupFailed {
                        elapsed_ms,
                        primary_reason,
                        termination,
                    } => (
                        "cli_process_tree_cleanup_error",
                        format!(
                            "{effective_type} process-tree cleanup failed after {elapsed_ms}ms; primary_reason={primary_reason}; termination={}",
                            termination.summary(),
                        ),
                        ProcessOutcome::failure(
                            "process_tree_cleanup_failed",
                            None,
                            "managed CLI process-tree cleanup was not proven",
                        ),
                    ),
                    SpawnWithTimeoutError::ProcessTreeContainmentUnsupported => (
                        "cli_process_tree_containment_unavailable",
                        format!(
                            "{effective_type} managed execution is unavailable because process-tree containment is unsupported"
                        ),
                        ProcessOutcome::failure(
                            "process_tree_containment_unavailable",
                            None,
                            "managed CLI process-tree containment is unsupported",
                        ),
                    ),
                    SpawnWithTimeoutError::InvalidOutputLimits { reason } => (
                        "cli_output_limits_invalid",
                        format!("{effective_type} managed output limits are invalid: {reason}"),
                        ProcessOutcome::failure(
                            "invalid_output_limits",
                            None,
                            "managed CLI output limits are invalid",
                        ),
                    ),
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
                    resolved_model: None,
                }
            }
        }
    }
}

fn failed_without_process(
    executor_type: &str,
    domain: &str,
    message: String,
    latency_ms: i64,
) -> NodeExecutionOutput {
    NodeExecutionOutput {
        status: "failed".to_string(),
        executor_type: executor_type.to_string(),
        output: None,
        error_domain: Some(domain.to_string()),
        error_message: Some(message),
        input_tokens: None,
        output_tokens: None,
        estimated_cost: None,
        latency_ms: Some(latency_ms),
        process_outcome: None,
        resolved_model: None,
    }
}

fn validate_claude_execution_authority(
    input: &NodeExecutionInput,
    admission: &ClaudeCodeAdmission,
) -> Result<(), String> {
    let current = ClaudeCodeAdmission::validate(
        &admission.binary_path,
        &admission.binary_version,
        &admission.binary_sha256,
        admission.model.as_deref(),
        admission.max_turns,
        admission.max_budget_usd,
    )?;
    if &current != admission {
        return Err("managed Claude runtime admission changed before execution".to_string());
    }
    if input.task_type != "claude_code_cli"
        || input
            .node_metadata
            .pointer("/managed_supervised_patch/operation")
            .and_then(Value::as_str)
            != Some("product_apply")
        || input
            .node_metadata
            .get("executor_class")
            .and_then(Value::as_str)
            != Some("managed_coding")
    {
        return Err("Claude Code is admitted only for managed product_apply nodes".to_string());
    }
    let identity = input
        .node_metadata
        .get("managed_executor_identity")
        .and_then(Value::as_object)
        .ok_or_else(|| "managed Claude executor identity is missing".to_string())?;
    if identity.get("schema_version").and_then(Value::as_str)
        != Some("managed_executor_identity.v1")
        || identity.get("executor_type").and_then(Value::as_str) != Some("claude_code_cli")
        || identity.get("binary_path").and_then(Value::as_str) != admission.binary_path.to_str()
        || identity.get("binary_version").and_then(Value::as_str)
            != Some(admission.binary_version.as_str())
        || identity.get("binary_sha256").and_then(Value::as_str)
            != Some(admission.binary_sha256.as_str())
        || identity.get("model_resolution").and_then(Value::as_str)
            != Some(admission.model_resolution())
        || !model_identity_matches(identity.get("model"), admission.model.as_deref())
        || identity.get("max_turns").and_then(Value::as_u64) != Some(admission.max_turns)
        || identity.get("max_attempt_tokens").and_then(Value::as_u64)
            != Some(admission.max_attempt_tokens)
        || identity.get("context_tokens").and_then(Value::as_u64) != Some(admission.context_tokens)
        || identity.get("max_output_tokens").and_then(Value::as_u64)
            != Some(admission.max_output_tokens)
        || identity.get("max_budget_usd").and_then(Value::as_f64) != Some(admission.max_budget_usd)
        || identity.get("input_usd_per_mtok").and_then(Value::as_f64)
            != Some(admission.input_usd_per_mtok)
        || identity
            .get("cache_write_5m_usd_per_mtok")
            .and_then(Value::as_f64)
            != Some(admission.cache_write_5m_usd_per_mtok)
        || identity
            .get("cache_write_1h_usd_per_mtok")
            .and_then(Value::as_f64)
            != Some(admission.cache_write_1h_usd_per_mtok)
        || identity
            .get("cache_read_usd_per_mtok")
            .and_then(Value::as_f64)
            != Some(admission.cache_read_usd_per_mtok)
        || identity.get("output_usd_per_mtok").and_then(Value::as_f64)
            != Some(admission.output_usd_per_mtok)
        || identity.get("pricing_source").and_then(Value::as_str)
            != Some(admission.pricing_source.as_str())
        || identity.get("pricing_verified_at").and_then(Value::as_str)
            != Some(admission.pricing_verified_at.as_str())
    {
        return Err("managed Claude executor identity changed".to_string());
    }
    let budget = input
        .node_metadata
        .get("product_budget")
        .and_then(Value::as_object)
        .ok_or_else(|| "managed Claude product budget is missing".to_string())?;
    if budget
        .get("total_tokens")
        .and_then(Value::as_u64)
        .is_none_or(|limit| limit < admission.max_attempt_tokens)
    {
        return Err(format!(
            "managed Claude token budget must be at least {}",
            admission.max_attempt_tokens
        ));
    }
    if budget.get("total_calls").and_then(Value::as_u64) != Some(1)
        || budget.get("max_retries").and_then(Value::as_u64) != Some(0)
    {
        return Err(
            "initial Claude admission requires total_calls=1 and max_retries=0".to_string(),
        );
    }
    Ok(())
}

fn model_identity_matches(identity_model: Option<&Value>, admitted_model: Option<&str>) -> bool {
    match admitted_model {
        Some(pin) => identity_model.and_then(Value::as_str) == Some(pin),
        None => identity_model.is_none_or(Value::is_null),
    }
}

fn claude_invocation_args(
    admission: &ClaudeCodeAdmission,
    allowed_paths: &[Value],
    prompt: &str,
) -> Vec<OsString> {
    let allow = allowed_paths
        .iter()
        .filter_map(Value::as_str)
        .flat_map(|path| {
            [
                format!("Edit(./{path})"),
                format!("Edit(./{path}/**)"),
                format!("Write(./{path})"),
                format!("Write(./{path}/**)"),
            ]
        })
        .collect::<Vec<_>>();
    let settings = serde_json::json!({
        "permissions": {
            "defaultMode": "dontAsk",
            "allow": allow,
            "deny": [
                "Bash",
                "WebFetch",
                "WebSearch",
                "Agent",
                "Task",
                "NotebookEdit",
                "Read(~/.claude/**)",
                "Read(~/.ssh/**)",
                "Read(~/.aws/**)",
                "Read(~/.config/**)",
                "Read(~/.git-credentials)",
                "Read(~/.netrc)",
                "Read(~/.bash_history)",
                "Read(~/.zsh_history)",
                "Read(**/.env)",
                "Read(**/.env.*)"
            ]
        },
        "sandbox": {
            "enabled": true,
            "failIfUnavailable": true,
            "allowUnsandboxedCommands": false,
            "autoAllowBashIfSandboxed": false
        }
    });
    let mut args = vec![
        "-p".into(),
        prompt.into(),
        "--output-format".into(),
        "json".into(),
        "--safe-mode".into(),
        "--no-chrome".into(),
        "--disable-slash-commands".into(),
        "--no-session-persistence".into(),
        "--setting-sources".into(),
        "".into(),
        "--prompt-suggestions".into(),
        "false".into(),
        "--permission-mode".into(),
        "dontAsk".into(),
        "--strict-mcp-config".into(),
        "--mcp-config".into(),
        "{\"mcpServers\":{}}".into(),
        "--tools".into(),
        "Read,Edit,Write".into(),
        "--settings".into(),
        settings.to_string().into(),
    ];
    if let Some(model) = &admission.model {
        args.push("--model".into());
        args.push(model.clone().into());
    }
    args.push("--max-turns".into());
    args.push(admission.max_turns.to_string().into());
    args.push("--max-budget-usd".into());
    args.push(format!("{:.2}", admission.max_budget_usd).into());
    args
}

fn parse_admitted_claude_output(
    raw: &str,
    admission: &ClaudeCodeAdmission,
    latency_ms: i64,
    process_outcome: ProcessOutcome,
) -> NodeExecutionOutput {
    let raw = redact_sensitive_patterns(raw);
    let parsed: Value = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(error) => {
            return NodeExecutionOutput {
                status: "failed".to_string(),
                executor_type: "claude_code_cli".to_string(),
                output: None,
                error_domain: Some("cli_output_parse_error".to_string()),
                error_message: Some(format!("failed to parse Claude Code JSON output: {error}")),
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(latency_ms),
                process_outcome: Some(process_outcome),
                resolved_model: None,
            };
        }
    };
    let usage = parsed.get("usage").and_then(Value::as_object);
    let input_tokens = usage
        .and_then(|usage| {
            let base = usage.get("input_tokens")?.as_u64()?;
            ["cache_creation_input_tokens", "cache_read_input_tokens"]
                .iter()
                .try_fold(base, |total, key| {
                    total.checked_add(usage.get(*key).and_then(Value::as_u64).unwrap_or(0))
                })
        })
        .filter(|value| *value <= admission.context_tokens * admission.max_turns)
        .and_then(|value| i64::try_from(value).ok());
    let output_tokens = usage
        .and_then(|usage| usage.get("output_tokens"))
        .and_then(Value::as_u64)
        .filter(|value| *value <= admission.max_output_tokens * admission.max_turns)
        .and_then(|value| i64::try_from(value).ok());
    let cost = parsed
        .get("total_cost_usd")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0 && *value <= admission.max_budget_usd);
    // The CLI must prove exactly one resolved model identity through its
    // owner-reported per-model usage. In pinned mode that identity must equal
    // the admitted snapshot; in subscription-default mode it is recorded as
    // the resolved identity. Missing or ambiguous model identity fails closed.
    let resolved = parsed
        .get("modelUsage")
        .and_then(Value::as_object)
        .filter(|models| models.len() == 1)
        .and_then(|models| models.iter().next());
    let resolved_model = resolved
        .map(|(model, _)| model.clone())
        .filter(|model| !model.trim().is_empty());
    let resolved_model_cost = resolved
        .and_then(|(_, usage)| usage.get("costUSD"))
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0);
    let model_matches_admission = match (&admission.model, &resolved_model) {
        (Some(pin), Some(resolved)) => resolved == pin,
        (None, Some(_)) => true,
        _ => false,
    };
    let exact_model_used = model_matches_admission
        && cost.zip(resolved_model_cost).is_some_and(|(total, model)| {
            (total - model).abs() <= 1e-9 && model <= admission.max_budget_usd
        });
    let successful_turns = parsed.get("subtype").and_then(Value::as_str) == Some("success")
        && parsed.get("is_error").and_then(Value::as_bool) == Some(false)
        && parsed
            .get("num_turns")
            .and_then(Value::as_u64)
            .is_some_and(|turns| turns > 0 && turns <= admission.max_turns);
    let result = parsed
        .get("result")
        .and_then(Value::as_str)
        .map(redact_sensitive_patterns);
    if input_tokens.is_none()
        || output_tokens.is_none()
        || cost.is_none()
        || !exact_model_used
        || !successful_turns
        || result.is_none()
    {
        return NodeExecutionOutput {
            status: "failed".to_string(),
            executor_type: "claude_code_cli".to_string(),
            output: None,
            error_domain: Some("cli_evidence_incomplete".to_string()),
            error_message: Some(
                "Claude Code response lacks a successful bounded turn or proven resolved model, usage, and cost evidence"
                    .to_string(),
            ),
            input_tokens,
            output_tokens,
            estimated_cost: cost,
            latency_ms: Some(latency_ms),
            process_outcome: Some(process_outcome),
            resolved_model,
        };
    }
    NodeExecutionOutput {
        status: "completed".to_string(),
        executor_type: "claude_code_cli".to_string(),
        output: result,
        error_domain: None,
        error_message: None,
        input_tokens,
        output_tokens,
        estimated_cost: cost,
        latency_ms: Some(latency_ms),
        process_outcome: Some(process_outcome),
        resolved_model,
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
        "The control plane has already authorized this bounded workspace-apply execution for product task {task_id}. Do not request or wait for another execution approval, and do not stop after proposing a plan. Implement the objective immediately inside the bound workspace, using only the allowed paths below. This authorization covers only those edits. It does not approve the artifact, confirm target output, authorize a branch push, or authorize pull-request creation. After the files change, verify the requested change and stop. Do not modify any other path.\n\nAllowed paths:\n- {allowed_paths}\n\nObjective:\n{objective}"
    ))
}

fn is_product_apply(input: &NodeExecutionInput) -> bool {
    input
        .node_metadata
        .get("managed_supervised_patch")
        .and_then(Value::as_object)
        .and_then(|binding| binding.get("operation"))
        .and_then(Value::as_str)
        == Some("product_apply")
}

fn codex_invocation_args(
    cwd: &Path,
    prompt: &str,
    model: Option<&str>,
    persist_sessions: bool,
) -> Vec<OsString> {
    // Product worktrees are app-owned git worktrees; skip the interactive trust
    // prompt while preserving the exact workspace-write sandbox and never-
    // approval policy. Prompt is a CLI arg and stdin is closed at spawn.
    //
    // Product-managed runs set persist_sessions=true so the Rust session-usage
    // importer can bind exact token_count events from CODEX_HOME rollouts.
    // Non-product paths keep --ephemeral.
    let mut args: Vec<OsString> = vec![
        "--ask-for-approval".into(),
        "never".into(),
        "-c".into(),
        "approval_policy=\"never\"".into(),
        "exec".into(),
        "--json".into(),
        "--sandbox".into(),
        "workspace-write".into(),
        "--cd".into(),
        cwd.as_os_str().to_os_string(),
    ];
    if !persist_sessions {
        args.push("--ephemeral".into());
    }
    args.push("--skip-git-repo-check".into());
    args.push("--ignore-user-config".into());
    if let Some(model) = model.filter(|value| !value.trim().is_empty()) {
        args.push("--model".into());
        args.push(model.into());
    }
    args.push(prompt.into());
    args
}

fn execute_product_codex_with_budget_gateway(
    executor: &CliNodeExecutor,
    input: &NodeExecutionInput,
    bin_path: &str,
    cwd: &Path,
    prompt: &str,
    start: std::time::Instant,
) -> NodeExecutionOutput {
    let admission = match executor.codex_admission.as_ref() {
        Some(admission) => admission,
        None => {
            return failed_without_process(
                "codex_cli",
                "cli_execution_authority_invalid",
                "product-managed Codex requires exact CodexAdmission and loopback budget mediation"
                    .to_string(),
                start.elapsed().as_millis() as i64,
            );
        }
    };
    let executable = CodexExecutableIdentity {
        binary_path: admission.binary_path.clone(),
        binary_version: admission.binary_version.clone(),
        binary_sha256: admission.binary_sha256.clone(),
    };
    if executable.binary_path.to_string_lossy() != bin_path
        && Path::new(bin_path).canonicalize().ok().as_ref() != Some(&executable.binary_path)
    {
        return failed_without_process(
            "codex_cli",
            "cli_execution_authority_invalid",
            "codex binary path does not match the admitted product identity".to_string(),
            start.elapsed().as_millis() as i64,
        );
    }

    let authority = match authority_from_product_metadata(
        executable,
        &input.node_metadata,
        &admission.model,
        executor.timeout_ms,
    ) {
        Ok(authority) => authority,
        Err(error) => {
            return failed_without_process(
                "codex_cli",
                "cli_execution_authority_invalid",
                error,
                start.elapsed().as_millis() as i64,
            );
        }
    };

    let upstream_key = std::env::var("ACP_CODEX_UPSTREAM_API_KEY")
        .or_else(|_| std::env::var("OPENAI_API_KEY"))
        .ok()
        .filter(|value| !value.trim().is_empty());
    let Some(upstream_key) = upstream_key else {
        return failed_without_process(
            "codex_cli",
            "cli_execution_authority_invalid",
            "product-managed Codex budget mediation requires ACP_CODEX_UPSTREAM_API_KEY (or OPENAI_API_KEY) held only by the parent gateway".to_string(),
            start.elapsed().as_millis() as i64,
        );
    };
    let upstream_base = std::env::var("ACP_CODEX_UPSTREAM_BASE_URL")
        .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());

    let gateway = match CodexBudgetGateway::start(authority, &upstream_base, &upstream_key) {
        Ok(gateway) => gateway,
        Err(error) => {
            return failed_without_process(
                "codex_cli",
                "cli_execution_authority_invalid",
                format!("failed to start codex budget gateway: {error}"),
                start.elapsed().as_millis() as i64,
            );
        }
    };
    // Drop the only copy of the upstream key from this stack frame.
    drop(upstream_key);

    let ephemeral_home = std::env::temp_dir().join(format!(
        "acp-codex-home-{}",
        gateway.authority().execution_id
    ));
    if let Err(error) = write_ephemeral_codex_home(
        &ephemeral_home,
        &gateway.authority().model,
        &gateway.base_url(),
    ) {
        let _ = gateway.shutdown();
        let _ = std::fs::remove_dir_all(&ephemeral_home);
        return failed_without_process(
            "codex_cli",
            "cli_execution_authority_invalid",
            error,
            start.elapsed().as_millis() as i64,
        );
    }

    let mut cmd = Command::new(bin_path);
    cmd.args(codex_invocation_args(
        cwd,
        prompt,
        Some(gateway.authority().model.as_str()),
        true, // persist sessions into controlled CODEX_HOME for exact usage import
    ));
    cmd.current_dir(cwd)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("CODEX_HOME", &ephemeral_home)
        .env("OPENAI_BASE_URL", gateway.base_url())
        .env("OPENAI_API_KEY", gateway.session_token())
        .env("HOME", &ephemeral_home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Locale-only allowlist; never forward real provider credentials or host HOME.
    for key in ["LANG", "LC_ALL", "LC_CTYPE", "TERM"] {
        if let Ok(value) = std::env::var(key) {
            cmd.env(key, value);
        }
    }

    let authority_snapshot = gateway.authority().clone();
    let output = spawn_with_timeout(&mut cmd, executor.timeout_ms);
    let elapsed_ms = start.elapsed().as_millis() as i64;
    let usage = gateway.shutdown();

    // Exact session-log usage evidence (owner-reported). Not a hard cross-call gate.
    let session_evidence =
        import_session_usage_evidence(&ephemeral_home, &authority_snapshot, admission);
    let _ = std::fs::remove_dir_all(&ephemeral_home);

    match output {
        Ok(output) => {
            let process_outcome = process_outcome_from_exit_status(&output.status);
            let stdout = redact_sensitive_patterns(&String::from_utf8_lossy(&output.stdout));
            let stderr = redact_sensitive_patterns(&String::from_utf8_lossy(&output.stderr));
            if !output.status.success() {
                let msg = if !stderr.is_empty() {
                    stderr
                } else {
                    format!("exit code: {}", output.status.code().unwrap_or(-1))
                };
                return NodeExecutionOutput {
                    status: "failed".to_string(),
                    executor_type: "codex_cli".to_string(),
                    output: if stdout.is_empty() {
                        None
                    } else {
                        Some(stdout)
                    },
                    error_domain: Some("cli_execution_error".to_string()),
                    error_message: Some(msg),
                    input_tokens: Some(usage.cumulative_input_tokens as i64),
                    output_tokens: Some(usage.cumulative_output_tokens as i64),
                    estimated_cost: None,
                    latency_ms: Some(elapsed_ms),
                    process_outcome: Some(process_outcome),
                    resolved_model: Some(admission.model.clone()),
                };
            }
            let mut parsed = parse_cli_output(&stdout, "codex_cli", elapsed_ms, process_outcome);
            // Prefer gateway-measured cumulative usage; corroborate with session logs.
            if usage.provider_requests > 0 {
                parsed.input_tokens = Some(usage.cumulative_input_tokens as i64);
                parsed.output_tokens = Some(usage.cumulative_output_tokens as i64);
            } else if let Some((_, rollup)) = session_evidence.as_ref() {
                parsed.input_tokens = Some(rollup.cumulative_input_tokens as i64);
                parsed.output_tokens = Some(rollup.cumulative_output_tokens as i64);
            } else if parsed.input_tokens.is_none() && parsed.output_tokens.is_none() {
                // Fail closed: product path requires owner-measured usage.
                parsed.status = "failed".to_string();
                parsed.error_domain = Some("cli_execution_authority_invalid".to_string());
                parsed.error_message = Some(
                    "product-managed Codex completed without gateway- or session-measured usage"
                        .to_string(),
                );
            }
            parsed.resolved_model = Some(admission.model.clone());
            let _ = session_evidence; // evidence is available for node metadata binding later
            parsed
        }
        Err(error) => {
            let domain = match error.reason_code() {
                "spawn_failed" => "cli_spawn_error",
                "timeout" => "cli_timeout",
                "wait_failed" => "cli_wait_error",
                "output_limit_exceeded" => "cli_output_limit_exceeded",
                other => other,
            };
            let process_outcome = ProcessOutcome::failure(
                error.reason_code(),
                None,
                "managed Codex process boundary failure",
            );
            NodeExecutionOutput {
                status: "failed".to_string(),
                executor_type: "codex_cli".to_string(),
                output: None,
                error_domain: Some(domain.to_string()),
                error_message: Some(format!("codex_cli process boundary failure: {error:?}")),
                input_tokens: Some(usage.cumulative_input_tokens as i64),
                output_tokens: Some(usage.cumulative_output_tokens as i64),
                estimated_cost: None,
                latency_ms: Some(elapsed_ms),
                process_outcome: Some(process_outcome),
                resolved_model: Some(admission.model.clone()),
            }
        }
    }
}

fn import_session_usage_evidence(
    codex_home: &Path,
    authority: &CodexBudgetAuthority,
    admission: &CodexAdmission,
) -> Option<(Value, super::codex_session_usage::SessionUsageRollup)> {
    let files = discover_rollout_files(codex_home).ok()?;
    let mut root_id = None;
    for path in files {
        if let Ok(Some(meta)) = root_thread_id_from_file(&path) {
            if meta.parent_thread_id.is_none() {
                root_id = Some(meta.thread_id);
                break;
            }
            root_id.get_or_insert(meta.thread_id);
        }
    }
    let root_id = root_id?;
    let rollup = import_managed_codex_home(codex_home, &root_id).ok()?;
    let evidence = rollup_to_product_evidence(
        &rollup,
        &authority.task_id,
        &authority.workflow_node_id,
        &authority.execution_id,
        &admission.binary_version,
        &admission.binary_sha256,
    );
    Some((evidence, rollup))
}

// Silence unused-import warnings when authority helpers are only used above.
#[allow(dead_code)]
fn _codex_authority_type_anchor(_: CodexBudgetAuthority) {}

fn cli_env_allowlist() -> Vec<String> {
    std::env::var("ACP_CLI_ENV_ALLOWLIST")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn claude_env_allowlist() -> Vec<String> {
    // Bounded process variables plus explicitly selected first-party Claude Code
    // credential/configuration variables. `ANTHROPIC_BASE_URL`,
    // `ANTHROPIC_AUTH_TOKEN`, and `ANTHROPIC_MODEL` are the first-party variables
    // an operator subscription import uses; they pass through only when the
    // operator sets them. Generic proxy variables and TLS overrides stay
    // discarded. Values are never persisted or logged by the harness.
    const ADMITTED: &[&str] = &[
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
        "LOGNAME",
        "SHELL",
        "TERM",
        "USER",
        "CLAUDE_CODE_OAUTH_TOKEN",
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_MODEL",
    ];
    cli_env_allowlist()
        .into_iter()
        .filter(|key| ADMITTED.contains(&key.as_str()))
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
                resolved_model: None,
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
        resolved_model: None,
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
                    resolved_model: None,
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
            resolved_model: None,
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
            resolved_model: None,
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
        resolved_model: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[cfg(unix)]
    fn admitted_fake_claude(workspace: &Path, body: &str) -> ClaudeCodeAdmission {
        use sha2::{Digest, Sha256};
        use std::os::unix::fs::PermissionsExt;

        let binary = workspace.join("claude-fixture-2.1.217");
        std::fs::write(
            &binary,
            format!(
                "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then printf '2.1.217 (Claude Code)\\n'; exit 0; fi\n{body}\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let digest = hex::encode(Sha256::digest(std::fs::read(&binary).unwrap()));
        ClaudeCodeAdmission::validate(
            &binary,
            "2.1.217",
            &digest,
            Some(super::super::config::ADMITTED_CLAUDE_CODE_MODEL),
            3,
            2.16,
        )
        .unwrap()
    }

    fn claude_metadata(workspace: &Path, admission: &ClaudeCodeAdmission) -> Value {
        json!({
            "executor": "claude_code_cli",
            "executor_class": "managed_coding",
            "workspace_path": workspace,
            "workspace_root": workspace,
            "product_task_id": "product-task-claude",
            "allowed_paths": ["docs/managed.md"],
            "managed_supervised_patch": {"operation": "product_apply"},
            "managed_executor_identity": {
                "schema_version": "managed_executor_identity.v1",
                "executor_type": "claude_code_cli",
                "binary_path": admission.binary_path,
                "binary_version": admission.binary_version,
                "binary_sha256": admission.binary_sha256,
                "model": admission.model,
                "model_resolution": admission.model_resolution(),
                "max_turns": admission.max_turns,
                "max_attempt_tokens": admission.max_attempt_tokens,
                "context_tokens": admission.context_tokens,
                "max_output_tokens": admission.max_output_tokens,
                "max_budget_usd": admission.max_budget_usd,
                "input_usd_per_mtok": admission.input_usd_per_mtok,
                "cache_write_5m_usd_per_mtok": admission.cache_write_5m_usd_per_mtok,
                "cache_write_1h_usd_per_mtok": admission.cache_write_1h_usd_per_mtok,
                "cache_read_usd_per_mtok": admission.cache_read_usd_per_mtok,
                "output_usd_per_mtok": admission.output_usd_per_mtok,
                "pricing_source": admission.pricing_source,
                "pricing_verified_at": admission.pricing_verified_at,
            },
            "product_budget": {
                "total_tokens": admission.max_attempt_tokens,
                "total_calls": 1,
                "max_retries": 0,
            },
            "prompt": "Create docs/managed.md"
        })
    }

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
        assert_eq!(output.error_domain.as_deref(), Some("cli_not_admitted"));
    }

    #[cfg(unix)]
    #[test]
    fn admitted_claude_uses_bounded_invocation_and_owner_reported_usage_cost() {
        let workspace = tempfile::tempdir().unwrap();
        let admission = admitted_fake_claude(
            workspace.path(),
            &format!(
                "printf '%s\\n' '{{\"subtype\":\"success\",\"is_error\":false,\"num_turns\":1,\"result\":\"done\",\"usage\":{{\"input_tokens\":10,\"cache_creation_input_tokens\":3,\"cache_read_input_tokens\":2,\"output_tokens\":4}},\"total_cost_usd\":0.01,\"modelUsage\":{{\"{}\":{{\"costUSD\":0.01}}}}}}'",
                super::super::config::ADMITTED_CLAUDE_CODE_MODEL
            ),
        );
        let args =
            claude_invocation_args(&admission, &[json!("docs/managed.md")], "bounded objective")
                .into_iter()
                .map(|value| value.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
        assert!(args.iter().any(|value| value == "--safe-mode"));
        assert!(args.iter().any(|value| value == "--no-chrome"));
        assert!(args.iter().any(|value| value == "--no-session-persistence"));
        assert!(args.iter().any(|value| value == "--disable-slash-commands"));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--setting-sources", ""]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--model", super::super::config::ADMITTED_CLAUDE_CODE_MODEL]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--mcp-config", "{\"mcpServers\":{}}"]));
        assert!(!args.iter().any(|value| value.contains("dangerously")));
        assert!(args
            .iter()
            .any(|value| value.contains("\"deny\":[\"Bash\"")));
        assert!(args
            .iter()
            .any(|value| value.contains("Read(~/.claude/**)")));
        assert!(args
            .iter()
            .any(|value| value.contains("Write(./docs/managed.md)")));

        let executor = CliNodeExecutor::admitted_claude(admission.clone(), 30_000);
        let output = executor.execute_node(&NodeExecutionInput {
            task_type: "claude_code_cli".to_string(),
            ..make_input(claude_metadata(workspace.path(), &admission))
        });
        assert_eq!(output.status, "completed", "{output:?}");
        assert_eq!(output.input_tokens, Some(15));
        assert_eq!(output.output_tokens, Some(4));
        assert_eq!(output.estimated_cost, Some(0.01));
        assert_eq!(
            output.resolved_model.as_deref(),
            Some(super::super::config::ADMITTED_CLAUDE_CODE_MODEL),
            "pin-mode must persist the exact admitted resolved_model"
        );
        let value = output.to_value();
        assert_eq!(
            value.get("resolved_model").and_then(Value::as_str),
            Some(super::super::config::ADMITTED_CLAUDE_CODE_MODEL)
        );
        assert_eq!(
            output
                .process_outcome
                .as_ref()
                .and_then(|value| value.exit_code),
            Some(0)
        );
    }

    #[cfg(unix)]
    #[test]
    fn admitted_claude_rejects_provider_error_even_with_complete_usage() {
        let workspace = tempfile::tempdir().unwrap();
        let admission = admitted_fake_claude(workspace.path(), "exit 0");
        let raw = format!(
            "{{\"subtype\":\"error_during_execution\",\"is_error\":true,\"num_turns\":1,\"result\":\"unavailable\",\"usage\":{{\"input_tokens\":10,\"output_tokens\":4}},\"total_cost_usd\":0.01,\"modelUsage\":{{\"{}\":{{\"costUSD\":0.01}}}}}}",
            super::super::config::ADMITTED_CLAUDE_CODE_MODEL
        );

        let output = parse_admitted_claude_output(&raw, &admission, 12, successful_process());

        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("cli_evidence_incomplete")
        );
        assert_eq!(output.input_tokens, Some(10));
        assert_eq!(output.output_tokens, Some(4));
        assert_eq!(output.estimated_cost, Some(0.01));
    }

    #[cfg(unix)]
    #[test]
    fn admitted_claude_rejects_unbounded_tokens_or_inconsistent_model_cost() {
        let workspace = tempfile::tempdir().unwrap();
        let admission = admitted_fake_claude(workspace.path(), "exit 0");
        let response = |output_tokens: u64, model_cost: f64| {
            format!(
                "{{\"subtype\":\"success\",\"is_error\":false,\"num_turns\":1,\"result\":\"done\",\"usage\":{{\"input_tokens\":10,\"output_tokens\":{output_tokens}}},\"total_cost_usd\":0.01,\"modelUsage\":{{\"{}\":{{\"costUSD\":{model_cost}}}}}}}",
                super::super::config::ADMITTED_CLAUDE_CODE_MODEL
            )
        };

        let unbounded = parse_admitted_claude_output(
            &response(admission.max_output_tokens * admission.max_turns + 1, 0.01),
            &admission,
            12,
            successful_process(),
        );
        let inconsistent =
            parse_admitted_claude_output(&response(4, 0.02), &admission, 12, successful_process());

        assert_eq!(unbounded.status, "failed");
        assert_eq!(inconsistent.status, "failed");
        assert_eq!(unbounded.output_tokens, None);
        assert_eq!(
            inconsistent.error_domain.as_deref(),
            Some("cli_evidence_incomplete")
        );
    }

    #[cfg(unix)]
    fn subscription_fake_claude(workspace: &Path, body: &str) -> ClaudeCodeAdmission {
        use sha2::{Digest, Sha256};
        use std::os::unix::fs::PermissionsExt;

        let binary = workspace.join("claude-fixture-subscription-2.1.217");
        std::fs::write(
            &binary,
            format!(
                "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then printf '2.1.217 (Claude Code)\\n'; exit 0; fi\n{body}\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let digest = hex::encode(Sha256::digest(std::fs::read(&binary).unwrap()));
        ClaudeCodeAdmission::validate(&binary, "2.1.217", &digest, None, 3, 2.16).unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn subscription_default_claude_omits_model_flag_and_records_resolved_identity() {
        let workspace = tempfile::tempdir().unwrap();
        let admission = subscription_fake_claude(
            workspace.path(),
            "printf '%s\\n' '{\"subtype\":\"success\",\"is_error\":false,\"num_turns\":1,\"result\":\"done\",\"usage\":{\"input_tokens\":10,\"output_tokens\":4},\"total_cost_usd\":0.0,\"modelUsage\":{\"subscription-claude-default\":{\"costUSD\":0.0}}}'",
        );
        let args =
            claude_invocation_args(&admission, &[json!("docs/managed.md")], "bounded objective")
                .into_iter()
                .map(|value| value.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
        assert!(!args.iter().any(|value| value == "--model"));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--max-budget-usd", "2.16"]));

        let executor = CliNodeExecutor::admitted_claude(admission.clone(), 30_000);
        let output = executor.execute_node(&NodeExecutionInput {
            task_type: "claude_code_cli".to_string(),
            ..make_input(claude_metadata(workspace.path(), &admission))
        });
        assert_eq!(output.status, "completed", "{output:?}");
        assert_eq!(
            output.resolved_model.as_deref(),
            Some("subscription-claude-default")
        );
        assert_eq!(output.input_tokens, Some(10));
        assert_eq!(output.output_tokens, Some(4));
        let value = output.to_value();
        assert_eq!(
            value.get("resolved_model").and_then(Value::as_str),
            Some("subscription-claude-default")
        );
    }

    #[cfg(unix)]
    #[test]
    fn subscription_default_claude_rejects_ambiguous_or_missing_model_identity() {
        let workspace = tempfile::tempdir().unwrap();
        let admission = subscription_fake_claude(workspace.path(), "exit 0");
        let ambiguous = "{\"subtype\":\"success\",\"is_error\":false,\"num_turns\":1,\"result\":\"done\",\"usage\":{\"input_tokens\":10,\"output_tokens\":4},\"total_cost_usd\":0.01,\"modelUsage\":{\"model-a\":{\"costUSD\":0.01},\"model-b\":{\"costUSD\":0.0}}}";
        let missing = "{\"subtype\":\"success\",\"is_error\":false,\"num_turns\":1,\"result\":\"done\",\"usage\":{\"input_tokens\":10,\"output_tokens\":4},\"total_cost_usd\":0.01}";

        for raw in [ambiguous, missing] {
            let output = parse_admitted_claude_output(raw, &admission, 12, successful_process());
            assert_eq!(output.status, "failed", "{raw}");
            assert_eq!(
                output.error_domain.as_deref(),
                Some("cli_evidence_incomplete"),
                "{raw}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn pinned_claude_rejects_a_different_resolved_model_identity() {
        let workspace = tempfile::tempdir().unwrap();
        let admission = admitted_fake_claude(workspace.path(), "exit 0");
        let raw = "{\"subtype\":\"success\",\"is_error\":false,\"num_turns\":1,\"result\":\"done\",\"usage\":{\"input_tokens\":10,\"output_tokens\":4},\"total_cost_usd\":0.01,\"modelUsage\":{\"some-other-model\":{\"costUSD\":0.01}}}";

        let output = parse_admitted_claude_output(raw, &admission, 12, successful_process());

        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("cli_evidence_incomplete")
        );
        assert_eq!(output.resolved_model.as_deref(), Some("some-other-model"));
    }

    #[cfg(unix)]
    #[test]
    fn claude_budget_refusal_occurs_before_provider_process() {
        let workspace = tempfile::tempdir().unwrap();
        let marker = workspace.path().join("provider-started");
        let admission =
            admitted_fake_claude(workspace.path(), &format!("touch '{}'", marker.display()));
        let mut metadata = claude_metadata(workspace.path(), &admission);
        metadata["product_budget"]["total_tokens"] = json!(50_000);
        let executor = CliNodeExecutor::admitted_claude(admission, 30_000);
        let output = executor.execute_node(&NodeExecutionInput {
            task_type: "claude_code_cli".to_string(),
            ..make_input(metadata)
        });
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("cli_execution_authority_invalid")
        );
        assert!(!marker.exists());
        assert!(output.process_outcome.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn changed_claude_binary_is_rejected_before_provider_process() {
        let workspace = tempfile::tempdir().unwrap();
        let marker = workspace.path().join("provider-started");
        let admission = admitted_fake_claude(workspace.path(), "exit 0");
        std::fs::write(
            &admission.binary_path,
            format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
        )
        .unwrap();
        let executor = CliNodeExecutor::admitted_claude(admission.clone(), 30_000);

        let output = executor.execute_node(&NodeExecutionInput {
            task_type: "claude_code_cli".to_string(),
            ..make_input(claude_metadata(workspace.path(), &admission))
        });

        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("cli_execution_authority_invalid")
        );
        assert!(!marker.exists());
        assert!(output.process_outcome.is_none());
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
            CliNodeExecutor::new(None, Some(binary.to_string_lossy().into_owned()), 30_000);
        let output = executor.execute_node(&make_input(json!({
            "executor": "codex_cli",
            "prompt": "bounded fixture",
            "workspace_path": workspace.path(),
        })));
        assert_eq!(output.status, "completed", "{output:?}");
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

    #[cfg(unix)]
    #[test]
    fn managed_cli_output_limit_fails_without_retaining_partial_output() {
        let workspace = tempfile::tempdir().unwrap();
        let binary = fake_codex(
            workspace.path(),
            "dd if=/dev/zero bs=1024 count=4097 status=none",
        );
        let executor =
            CliNodeExecutor::new(None, Some(binary.to_string_lossy().into_owned()), 30_000);
        let output = executor.execute_node(&make_input(json!({
            "executor": "codex_cli",
            "prompt": "bounded flood",
            "workspace_path": workspace.path(),
        })));
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("cli_output_limit_exceeded")
        );
        assert!(output.output.is_none(), "partial output must be discarded");
        let process_outcome = output.process_outcome.expect("process outcome");
        assert_eq!(process_outcome.state, "output_limit_exceeded");
        assert!(output
            .error_message
            .as_deref()
            .is_some_and(|message| message.contains("stream=stdout")));
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
        assert!(prompt.contains("do not stop after proposing a plan"));
        assert!(prompt.contains("Implement the objective immediately"));
        assert!(prompt.contains("does not approve the artifact"));
        assert!(prompt.contains("docs/managed.md"));
        assert!(prompt.ends_with("Create the requested file"));
    }

    #[test]
    fn codex_invocation_is_noninteractive_but_keeps_workspace_write_sandbox() {
        let args = codex_invocation_args(
            Path::new("/tmp/bound-workspace"),
            "bounded objective",
            None,
            false,
        );
        let args = args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(
            args,
            vec![
                "--ask-for-approval",
                "never",
                "-c",
                "approval_policy=\"never\"",
                "exec",
                "--json",
                "--sandbox",
                "workspace-write",
                "--cd",
                "/tmp/bound-workspace",
                "--ephemeral",
                "--skip-git-repo-check",
                "--ignore-user-config",
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
        let _guard = env_lock().lock().unwrap();
        std::env::remove_var("ACP_CLI_ENV_ALLOWLIST");
        assert!(cli_env_allowlist().is_empty());
    }

    #[test]
    fn claude_env_allowlist_excludes_host_paths_and_unadmitted_routing_overrides() {
        let _guard = env_lock().lock().unwrap();
        std::env::set_var(
            "ACP_CLI_ENV_ALLOWLIST",
            "HOME,TMP,TMPDIR,CLAUDE_CODE_OAUTH_TOKEN,ANTHROPIC_API_KEY,ANTHROPIC_BASE_URL,ANTHROPIC_AUTH_TOKEN,ANTHROPIC_MODEL,CLAUDE_CODE_USE_BEDROCK,HTTPS_PROXY,ANTHROPIC_TLS_INSECURE",
        );

        let admitted = claude_env_allowlist();
        std::env::remove_var("ACP_CLI_ENV_ALLOWLIST");

        assert_eq!(
            admitted,
            [
                "CLAUDE_CODE_OAUTH_TOKEN",
                "ANTHROPIC_API_KEY",
                "ANTHROPIC_BASE_URL",
                "ANTHROPIC_AUTH_TOKEN",
                "ANTHROPIC_MODEL"
            ]
        );
    }

    #[test]
    fn test_from_config_disabled() {
        let config = super::super::config::CliConfig {
            enabled: false,
            claude_code_bin: Some("/bin/claude".into()),
            claude_code_enabled: true,
            claude_code_admission: None,
            codex_bin: None,
            codex_enabled: false,
            codex_admission: None,
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
            claude_code_admission: None,
            codex_bin: None,
            codex_enabled: false,
            codex_admission: None,
            timeout_ms: 5000,
        };
        assert!(CliNodeExecutor::from_config_for(&config, "codex_cli").is_none());
    }

    #[test]
    fn from_config_rejects_claude_without_exact_admission_even_when_supplied() {
        let config = super::super::config::CliConfig {
            enabled: true,
            claude_code_bin: Some("/bin/claude".into()),
            claude_code_enabled: true,
            claude_code_admission: None,
            codex_bin: None,
            codex_enabled: false,
            codex_admission: None,
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
            claude_code_admission: None,
            codex_bin: Some("/bin/codex".into()),
            codex_enabled: true,
            codex_admission: None,
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
