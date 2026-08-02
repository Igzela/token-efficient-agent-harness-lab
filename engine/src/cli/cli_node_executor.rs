use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::codex_budget_authority::{
    write_ephemeral_codex_home, CodexBudgetAuthority, CodexBudgetGateway, CodexExecutableIdentity,
    CodexProviderIdentity, CODEX_BUDGET_AUTHORITY_SCHEMA,
};
use super::codex_managed_acceptance_preflight::run_lease_bound_managed_acceptance_preflight;
use super::codex_mediation_admission::{
    plan_mediated_codex_launch_for_gateway, reconcile_gateway_and_session_usage,
    CodexMediatedCapabilityReport, IsolationMode, UsageReconcileResult,
};
use super::codex_session_usage::{
    discover_rollout_files, import_managed_codex_home, rollup_to_product_evidence,
    root_thread_id_from_file,
};
use super::config::{ClaudeCodeAdmission, CodexAdmission};
use crate::cli::{spawn_with_timeout, SpawnWithTimeoutError};
use crate::execution_usage::codex_adapter::UsageBindingContext;
use crate::execution_usage::gateway_adapter::mediated_codex_usage_evidence_bundle;
use crate::execution_usage::reconcile::{admission_evidence_ok, reconcile_usage_events};
use crate::node_executor::{
    exit_status_signal, process_outcome_from_exit_status, NodeExecutionInput, NodeExecutionOutput,
    NodeExecutor, ProcessOutcome,
};
use crate::provider::redaction::redact_sensitive_patterns;
use crate::storage::local_product_store::{
    CostAuthority, LocalProductStore, ManagedCodexLaunchFacts, ManagedCodexSpawnLease,
};

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
    /// Present for every production constructor.  A missing store is fail
    /// closed for ProductTask Codex work rather than falling back to metadata.
    managed_acceptance_store: Option<Arc<LocalProductStore>>,
    /// Explicit provider-free seam for unit tests that exercise the legacy
    /// non-ProductTask Codex command path. Production callers cannot enable
    /// this crate-private flag and therefore require the store boundary.
    unmanaged_codex_test_seam: bool,
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
            managed_acceptance_store: None,
            unmanaged_codex_test_seam: false,
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
            managed_acceptance_store: None,
            unmanaged_codex_test_seam: false,
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
            managed_acceptance_store: None,
            unmanaged_codex_test_seam: false,
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

    /// Attach the sole persistence owner used by ProductTask Codex admission.
    /// A store-attached Codex invocation without a durable ProductTask owner is
    /// always rejected; ToolPolicy receipts are an additional prerequisite and
    /// never an execution or spend authority.
    pub fn with_managed_acceptance_store(mut self, store: Arc<LocalProductStore>) -> Self {
        self.managed_acceptance_store = Some(store);
        self
    }

    #[cfg(test)]
    fn with_unmanaged_codex_for_test(mut self) -> Self {
        self.unmanaged_codex_test_seam = true;
        self
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

        // Product-managed Codex must use the store-owned lease boundary.  Do
        // not trust a node metadata flag to decide this: the durable ProductTask
        // run owner wins, so stripping/changing metadata cannot fall through to
        // the generic direct Command::spawn path.
        if effective_type == "codex_cli" {
            if self.managed_acceptance_store.is_none() && !self.unmanaged_codex_test_seam {
                return failed_without_process(
                    "codex_cli",
                    "cli_execution_authority_invalid",
                    "Codex execution requires the store-owned ProductTask authority boundary"
                        .to_string(),
                    start.elapsed().as_millis() as i64,
                );
            }
            let store_owned_product_task = match self.managed_acceptance_store.as_deref() {
                Some(store) => match store.product_task_id_for_run(&input.run_id) {
                    Ok(task_id) => task_id,
                    Err(error) => {
                        return failed_without_process(
                            "codex_cli",
                            "cli_execution_authority_invalid",
                            format!("managed Codex ProductTask run-owner lookup failed: {error}"),
                            start.elapsed().as_millis() as i64,
                        );
                    }
                },
                None => None,
            };
            match (
                self.managed_acceptance_store.is_some(),
                store_owned_product_task,
            ) {
                (true, Some(_)) => {
                    return execute_product_codex_with_budget_gateway(
                        self, input, &bin_path, &cwd, &prompt, start,
                    );
                }
                (true, None) => {
                    return failed_without_process(
                        "codex_cli",
                        "cli_execution_authority_invalid",
                        "Codex run is not owned by a persisted ProductTask; ToolPolicy cannot substitute for managed spend, attempt admission, lease, gateway, or runtime attestation"
                            .to_string(),
                        start.elapsed().as_millis() as i64,
                    );
                }
                _ => {}
            }
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

#[allow(clippy::too_many_arguments)]
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
    let runtime_profile_sha256 = match admission.runtime_profile_sha256() {
        Ok(Some(value)) => value,
        Ok(None) => {
            return failed_without_process(
                "codex_cli",
                "cli_execution_authority_invalid",
                "new product-managed Codex execution requires a runtime profile".to_string(),
                start.elapsed().as_millis() as i64,
            );
        }
        Err(error) => {
            return failed_without_process(
                "codex_cli",
                "cli_execution_authority_invalid",
                format!("Codex runtime profile is invalid: {error}"),
                start.elapsed().as_millis() as i64,
            );
        }
    };
    let capability_probe_sha256 = match admission.capability_probe_sha256() {
        Some(value) => value.to_string(),
        None => {
            return failed_without_process(
                "codex_cli",
                "cli_execution_authority_invalid",
                "Codex runtime profile capability evidence is missing".to_string(),
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
    let store = match executor.managed_acceptance_store.as_deref() {
        Some(store) => store,
        None => {
            return failed_without_process(
                "codex_cli",
                "cli_execution_authority_invalid",
                "product-managed Codex requires the store-owned spawn authority".to_string(),
                start.elapsed().as_millis() as i64,
            );
        }
    };
    let facts = ManagedCodexLaunchFacts {
        run_id: input.run_id.clone(),
        workflow_id: input.workflow_id.clone(),
        node_id: input.node_id.clone(),
        workspace_path: cwd.to_path_buf(),
        executable_path: executable.binary_path,
        executable_version: executable.binary_version,
        executable_sha256: executable.binary_sha256,
        runtime_profile_sha256: Some(runtime_profile_sha256),
        capability_probe_sha256: Some(capability_probe_sha256),
        model: admission.model.clone(),
    };
    let lease = match store.admit_managed_codex_spawn(&facts) {
        Ok(lease) => lease,
        Err(error) => {
            return failed_without_process(
                "codex_cli",
                "cli_execution_authority_invalid",
                format!("managed Codex store admission rejected: {error}"),
                start.elapsed().as_millis() as i64,
            );
        }
    };
    let output = match authority_from_managed_codex_spawn_lease(&lease) {
        Ok(authority) => execute_product_codex_after_store_admission(
            bin_path, cwd, prompt, start, admission, store, &lease, authority,
        ),
        Err(error) => failed_without_process(
            "codex_cli",
            "cli_execution_authority_invalid",
            format!("managed Codex store-issued authority is invalid: {error}"),
            start.elapsed().as_millis() as i64,
        ),
    };
    if let Err(error) = terminalize_managed_codex_spawn_output(store, &lease, &output) {
        return failed_without_process(
            "codex_cli",
            "cli_execution_authority_invalid",
            format!("managed Codex attempt lease terminalization failed: {error}"),
            start.elapsed().as_millis() as i64,
        );
    }
    output
}

/// A gateway that fails before child spawn cannot have forwarded a provider
/// request. Remove its parent-owned journal along with the ephemeral child
/// home so a later one-use attempt cannot resume unrelated pre-child state.
fn cleanup_pre_child_managed_codex_gateway(
    gateway: CodexBudgetGateway,
    ephemeral_home: &Path,
    journal_path: &Path,
) -> bool {
    let _ = gateway.shutdown();
    cleanup_pre_child_managed_codex_artifacts(ephemeral_home, journal_path)
}

/// Gateway startup can persist the parent journal before its accept thread is
/// available. Keep cleanup ownership and failure reporting identical whether
/// startup returned a gateway or failed after that durable setup.
fn cleanup_pre_child_managed_codex_artifacts(ephemeral_home: &Path, journal_path: &Path) -> bool {
    let home_removed = remove_pre_child_path(|| std::fs::remove_dir_all(ephemeral_home));
    let journal_removed = remove_pre_child_path(|| std::fs::remove_file(journal_path));
    home_removed && journal_removed
}

fn remove_pre_child_path(remove: impl FnOnce() -> std::io::Result<()>) -> bool {
    match remove() {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(_) => false,
    }
}

fn failed_after_pre_child_managed_codex_cleanup(
    gateway: CodexBudgetGateway,
    ephemeral_home: &Path,
    journal_path: &Path,
    stage: &'static str,
    message: String,
    start: std::time::Instant,
) -> NodeExecutionOutput {
    let cleanup_complete =
        cleanup_pre_child_managed_codex_gateway(gateway, ephemeral_home, journal_path);
    failed_after_pre_child_managed_codex_cleanup_result(cleanup_complete, stage, message, start)
}

fn failed_after_pre_child_managed_codex_start_cleanup(
    ephemeral_home: &Path,
    journal_path: &Path,
    message: String,
    start: std::time::Instant,
) -> NodeExecutionOutput {
    let cleanup_complete = cleanup_pre_child_managed_codex_artifacts(ephemeral_home, journal_path);
    failed_after_pre_child_managed_codex_cleanup_result(
        cleanup_complete,
        "gateway_start",
        message,
        start,
    )
}

fn failed_after_pre_child_managed_codex_cleanup_result(
    cleanup_complete: bool,
    stage: &'static str,
    message: String,
    start: std::time::Instant,
) -> NodeExecutionOutput {
    let (error_domain, message) = if cleanup_complete {
        ("cli_execution_authority_invalid", message)
    } else {
        (
            "cli_execution_cleanup_incomplete",
            format!("managed Codex pre-child cleanup incomplete after {stage}"),
        )
    };
    failed_without_process(
        "codex_cli",
        error_domain,
        message,
        start.elapsed().as_millis() as i64,
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_product_codex_after_store_admission(
    bin_path: &str,
    cwd: &Path,
    prompt: &str,
    start: std::time::Instant,
    admission: &CodexAdmission,
    store: &LocalProductStore,
    lease: &ManagedCodexSpawnLease,
    authority: CodexBudgetAuthority,
) -> NodeExecutionOutput {
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
    // Provider identity is copied only from the consumed spend body.  The
    // parent credential can be supplied through process configuration, but its
    // presence never proves authority and cannot select an upstream endpoint.
    let upstream_base = authority.provider.base_url.clone();

    // Mediated product launch requires bubblewrap filesystem+PID isolation.
    // Full live Golden Path admission is still partial (retry/network blockers).
    let capability = CodexMediatedCapabilityReport::evaluate(
        if Path::new(super::codex_mediation_admission::BUBBLEWRAP_BIN).is_file() {
            IsolationMode::BubblewrapFilesystem
        } else {
            IsolationMode::Unavailable
        },
        Path::new(super::codex_mediation_admission::BUBBLEWRAP_BIN).is_file(),
    );
    if !capability.admission_class.allows_mediated_product_launch() {
        return failed_without_process(
            "codex_cli",
            "cli_execution_authority_invalid",
            capability
                .remaining_blocker
                .unwrap_or_else(|| "codex mediation admission is blocked".to_string()),
            start.elapsed().as_millis() as i64,
        );
    }

    let ephemeral_home =
        std::env::temp_dir().join(format!("acp-codex-home-{}", authority.execution_id));
    // Parent-owned journal path: NEVER under ephemeral_home (sandbox-mounted).
    let journal_path =
        super::codex_usage_journal::parent_owned_journal_path(&authority.execution_id);
    let gateway = match CodexBudgetGateway::start(
        lease.gateway_start_permit(),
        authority,
        &upstream_base,
        &upstream_key,
        journal_path.clone(),
    ) {
        Ok(gateway) => gateway,
        Err(error) => {
            return failed_after_pre_child_managed_codex_start_cleanup(
                &ephemeral_home,
                &journal_path,
                format!("failed to start codex budget gateway: {error}"),
                start,
            );
        }
    };
    // Drop the only copy of the upstream key from this stack frame.
    drop(upstream_key);

    if let Err(error) = write_ephemeral_codex_home(
        &ephemeral_home,
        &gateway.authority().model,
        &gateway.base_url(),
    ) {
        return failed_after_pre_child_managed_codex_cleanup(
            gateway,
            &ephemeral_home,
            &journal_path,
            "ephemeral_home_write",
            error,
            start,
        );
    }

    let codex_args = codex_invocation_args(
        cwd,
        prompt,
        Some(gateway.authority().model.as_str()),
        true, // persist sessions into controlled CODEX_HOME for exact usage import
    );
    let launch_plan = match plan_mediated_codex_launch_for_gateway(
        gateway.authority(),
        Path::new(bin_path),
        &ephemeral_home,
        &gateway,
        &codex_args,
    ) {
        Ok(plan) => plan,
        Err(error) => {
            return failed_after_pre_child_managed_codex_cleanup(
                gateway,
                &ephemeral_home,
                &journal_path,
                "launch_plan",
                format!("failed to plan mediated Codex launch: {error}"),
                start,
            );
        }
    };
    let runtime_attestation = match launch_plan.derive_runtime_attestation(&gateway) {
        Ok(attestation) => attestation,
        Err(error) => {
            return failed_after_pre_child_managed_codex_cleanup(
                gateway,
                &ephemeral_home,
                &journal_path,
                "runtime_attestation",
                format!("managed Codex runtime ownership attestation failed: {error}"),
                start,
            );
        }
    };
    let preflight =
        match run_lease_bound_managed_acceptance_preflight(store, lease, &runtime_attestation) {
            Ok(report) => report,
            Err(_) => {
                return failed_after_pre_child_managed_codex_cleanup(
                    gateway,
                    &ephemeral_home,
                    &journal_path,
                    "owner_derived_preflight",
                    "managed Codex owner-derived preflight could not establish current owners"
                        .to_string(),
                    start,
                );
            }
        };
    if !preflight.allows_child_spawn() {
        return failed_after_pre_child_managed_codex_cleanup(
            gateway,
            &ephemeral_home,
            &journal_path,
            "owner_derived_preflight_blocked",
            format!(
                "managed Codex owner-derived preflight blocked: {}",
                preflight.result().as_str()
            ),
            start,
        );
    }
    if let Err(error) = store.confirm_managed_codex_spawn_before_child(lease, &runtime_attestation)
    {
        return failed_after_pre_child_managed_codex_cleanup(
            gateway,
            &ephemeral_home,
            &journal_path,
            "final_store_confirmation",
            error,
            start,
        );
    }
    // The profile observation is sampled before store admission and must be
    // sampled again immediately before the child is created.  A compatible
    // version range never permits a replacement, a changed capability set, or
    // a profile mutation between those two points.
    if let Err(error) = admission.revalidate_before_spawn() {
        return failed_after_pre_child_managed_codex_cleanup(
            gateway,
            &ephemeral_home,
            &journal_path,
            "runtime_profile_revalidation",
            error,
            start,
        );
    }

    let mut cmd = launch_plan.to_command();
    let authority_snapshot = gateway.authority().clone();
    let output = spawn_with_timeout(&mut cmd, authority_snapshot.timeout_ms);
    let elapsed_ms = start.elapsed().as_millis() as i64;
    let usage = gateway.shutdown();
    // This is a typed gateway/journal outcome, not an inference from output
    // strings.  Once a forwarded request has an unknown effect, the attempt
    // terminal state must preserve that uncertainty and the consumed spend is
    // never reactivated.
    let gateway_outcome_unknown = usage.journal_halted
        || usage
            .last_reject_class
            .as_deref()
            .is_some_and(|class| class == "outcome_unknown");

    // Exact session-log usage evidence (corroborating only). Gateway is the cross-call gate.
    let session_evidence =
        import_session_usage_evidence(&ephemeral_home, &authority_snapshot, admission);
    let session_input = session_evidence
        .as_ref()
        .map(|(_, rollup)| rollup.cumulative_input_tokens);
    let session_output = session_evidence
        .as_ref()
        .map(|(_, rollup)| rollup.cumulative_output_tokens);
    let reconciled = reconcile_gateway_and_session_usage(&usage, session_input, session_output);
    // Map gateway + optional session rollup into execution_usage_event.v1 and
    // reconcile without granting ProductTask budget from session importers.
    let binding = UsageBindingContext {
        product_task_id: Some(authority_snapshot.task_id.clone()),
        workflow_node_id: Some(authority_snapshot.workflow_node_id.clone()),
        managed_execution_id: Some(authority_snapshot.execution_id.clone()),
        provider_id: Some(authority_snapshot.provider.provider_kind.clone()),
        requested_model: Some(authority_snapshot.model.clone()),
        executable_path_fingerprint: None,
        executable_version: Some(authority_snapshot.executable.binary_version.clone()),
        executable_sha256: Some(authority_snapshot.executable.binary_sha256.clone()),
        ..UsageBindingContext::default()
    };
    let usage_events = mediated_codex_usage_evidence_bundle(
        &usage,
        &authority_snapshot,
        &binding,
        session_evidence.as_ref().map(|(_, rollup)| rollup),
        &format!("{}", start.elapsed().as_millis()),
    );
    let usage_reconcile = reconcile_usage_events(usage_events);
    // Conflicts fail closed for admission evidence; session importers still do not
    // restore or weaken gateway-enforced budget counters already applied above.
    let evidence_conflict = admission_evidence_ok(&usage_reconcile).err();
    let _ = std::fs::remove_dir_all(&ephemeral_home);
    // Retain parent journal when halted, outcome-unknown, or any non-clean last
    // reject class so operators can inspect charged/blocked attempts.
    let retain_journal = usage.journal_halted
        || usage
            .last_reject_class
            .as_deref()
            .is_some_and(|c| c == "outcome_unknown")
        || matches!(
            reconciled,
            UsageReconcileResult::Conflict { .. } | UsageReconcileResult::Missing { .. }
        );
    if !retain_journal {
        let _ = std::fs::remove_file(&journal_path);
    }

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
                let (in_tok, out_tok) = reconciled_token_pair(&reconciled, &usage);
                return NodeExecutionOutput {
                    status: "failed".to_string(),
                    executor_type: "codex_cli".to_string(),
                    output: if stdout.is_empty() {
                        None
                    } else {
                        Some(stdout)
                    },
                    error_domain: Some(
                        if gateway_outcome_unknown {
                            "cli_outcome_unknown"
                        } else {
                            "cli_execution_error"
                        }
                        .to_string(),
                    ),
                    error_message: Some(msg),
                    input_tokens: in_tok,
                    output_tokens: out_tok,
                    estimated_cost: None,
                    latency_ms: Some(elapsed_ms),
                    process_outcome: Some(process_outcome),
                    resolved_model: Some(admission.model.clone()),
                };
            }
            let mut parsed = parse_cli_output(&stdout, "codex_cli", elapsed_ms, process_outcome);
            if let Some(detail) = evidence_conflict.as_ref() {
                parsed.status = "failed".to_string();
                parsed.error_domain = Some(
                    if gateway_outcome_unknown {
                        "cli_outcome_unknown"
                    } else {
                        "cli_execution_authority_invalid"
                    }
                    .to_string(),
                );
                parsed.error_message = Some(format!(
                    "product-managed Codex execution_usage_event.v1 reconcile failed closed: {detail}"
                ));
                parsed.input_tokens = Some(usage.cumulative_input_tokens as i64);
                parsed.output_tokens = Some(usage.cumulative_output_tokens as i64);
                parsed.resolved_model = Some(admission.model.clone());
                return parsed;
            }
            if gateway_outcome_unknown {
                parsed.status = "failed".to_string();
                parsed.error_domain = Some("cli_outcome_unknown".to_string());
                parsed.error_message =
                    Some("gateway journal recorded an outcome-unknown provider effect".to_string());
                parsed.input_tokens = Some(usage.cumulative_input_tokens as i64);
                parsed.output_tokens = Some(usage.cumulative_output_tokens as i64);
            }
            match &reconciled {
                UsageReconcileResult::PreferGateway {
                    input_tokens,
                    output_tokens,
                    ..
                } => {
                    parsed.input_tokens = Some(*input_tokens as i64);
                    parsed.output_tokens = Some(*output_tokens as i64);
                }
                // Product-mediated path requires gateway interposition. Session-only
                // usage without gateway POSTs is fail-closed (child-writable logs
                // must not admit success without the cross-call gate).
                UsageReconcileResult::PreferSessionOnly { reason, .. } => {
                    parsed.status = "failed".to_string();
                    parsed.error_domain = Some("cli_execution_authority_invalid".to_string());
                    parsed.error_message = Some(format!(
                        "product-managed Codex requires gateway-measured usage; session-only evidence is not admitted: {reason}"
                    ));
                    parsed.input_tokens = Some(usage.cumulative_input_tokens as i64);
                    parsed.output_tokens = Some(usage.cumulative_output_tokens as i64);
                }
                UsageReconcileResult::Conflict { detail, .. } => {
                    parsed.status = "failed".to_string();
                    parsed.error_domain = Some("cli_execution_authority_invalid".to_string());
                    parsed.error_message = Some(format!(
                        "product-managed Codex usage reconcile failed closed: {detail}"
                    ));
                    parsed.input_tokens = Some(usage.cumulative_input_tokens as i64);
                    parsed.output_tokens = Some(usage.cumulative_output_tokens as i64);
                }
                UsageReconcileResult::Missing { detail } => {
                    parsed.status = "failed".to_string();
                    parsed.error_domain = Some("cli_execution_authority_invalid".to_string());
                    parsed.error_message = Some(format!(
                        "product-managed Codex completed without measured usage: {detail}"
                    ));
                    parsed.input_tokens = Some(usage.cumulative_input_tokens as i64);
                    parsed.output_tokens = Some(usage.cumulative_output_tokens as i64);
                }
            }
            parsed.resolved_model = Some(admission.model.clone());
            // Redact again after parse: never retain prompt/credential-shaped durable text.
            if let Some(text) = parsed.output.as_mut() {
                *text = redact_sensitive_patterns(text);
            }
            if let Some(text) = parsed.error_message.as_mut() {
                *text = redact_sensitive_patterns(text);
            }
            let _ = session_evidence;
            let _ = capability;
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
            let (in_tok, out_tok) = reconciled_token_pair(&reconciled, &usage);
            NodeExecutionOutput {
                status: "failed".to_string(),
                executor_type: "codex_cli".to_string(),
                output: None,
                error_domain: Some(domain.to_string()),
                error_message: Some(format!("codex_cli process boundary failure: {error:?}")),
                input_tokens: in_tok,
                output_tokens: out_tok,
                estimated_cost: None,
                latency_ms: Some(elapsed_ms),
                process_outcome: Some(process_outcome),
                resolved_model: Some(admission.model.clone()),
            }
        }
    }
}

/// Construct gateway authority exclusively from the store-issued lease.  This
/// deliberately has no metadata/environment parameters: those sources cannot
/// select a model, provider, budget, target, or execution identity.
fn authority_from_managed_codex_spawn_lease(
    lease: &ManagedCodexSpawnLease,
) -> Result<CodexBudgetAuthority, String> {
    let spend = lease.spend_body();
    let required_string = |field: &str| -> Result<String, String> {
        spend
            .get(field)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .ok_or_else(|| format!("managed Codex spend {field} is missing"))
    };
    let required_u64 = |field: &str| -> Result<u64, String> {
        spend
            .get(field)
            .and_then(Value::as_u64)
            .filter(|value| *value > 0)
            .ok_or_else(|| format!("managed Codex spend {field} must be a positive integer"))
    };
    let provider_kind = required_string("provider_kind")?;
    let provider_base_url = required_string("provider_base_url")?;
    let provider_host = required_string("provider_host")?;
    let endpoint_paths = spend
        .get("admitted_endpoint_paths")
        .and_then(Value::as_array)
        .filter(|paths| !paths.is_empty())
        .ok_or_else(|| "managed Codex spend admitted_endpoint_paths is missing".to_string())?
        .iter()
        .map(|path| {
            path.as_str()
                .filter(|path| path.starts_with('/') && !path.contains("//"))
                .map(str::to_string)
                .ok_or_else(|| "managed Codex spend endpoint path is invalid".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut provider = CodexProviderIdentity::openai_compatible(&provider_base_url)?;
    if provider_kind != provider.provider_kind || provider_host != provider.host {
        return Err("managed Codex spend provider identity is inconsistent".to_string());
    }
    provider.admitted_endpoint_paths = endpoint_paths;

    let cost_authority = CostAuthority::from_json(
        spend
            .get("cost_authority")
            .ok_or_else(|| "managed Codex spend cost authority is missing".to_string())?,
    )?;
    let max_cost_usd = match cost_authority {
        CostAuthority::ProviderReported { max_cost, .. }
        | CostAuthority::LocalEstimate { max_cost, .. } => Some(max_cost),
        CostAuthority::CostUnavailable => None,
    };
    let expires_at = required_string("expires_at")?;
    let expires_unix_ms = chrono::DateTime::parse_from_rfc3339(&expires_at)
        .map_err(|_| "managed Codex spend expiry is invalid".to_string())?
        .timestamp_millis()
        .try_into()
        .map_err(|_| "managed Codex spend expiry predates Unix epoch".to_string())?;

    CodexBudgetAuthority {
        schema_version: CODEX_BUDGET_AUTHORITY_SCHEMA.to_string(),
        task_id: lease.product_task_id().to_string(),
        workflow_node_id: lease.facts().node_id.clone(),
        execution_id: lease.execution_id().to_string(),
        executable: CodexExecutableIdentity {
            binary_path: lease.facts().executable_path.clone(),
            binary_version: lease.facts().executable_version.clone(),
            binary_sha256: lease.facts().executable_sha256.clone(),
        },
        provider,
        model: lease.facts().model.clone(),
        max_provider_requests: required_u64("max_provider_requests")?,
        max_retries: spend
            .get("max_retries")
            .and_then(Value::as_u64)
            .ok_or_else(|| "managed Codex spend max_retries is missing".to_string())?,
        max_input_tokens_per_request: required_u64("max_input_tokens")?,
        max_output_tokens_per_request: required_u64("max_output_tokens")?,
        max_cumulative_tokens: required_u64("max_total_tokens")?,
        max_cost_usd,
        timeout_ms: required_u64("max_wall_time_ms")?,
        worktree: lease.facts().workspace_path.clone(),
        expires_unix_ms,
    }
    .validate_new()
}

/// Every admitted attempt has exactly one durable terminal receipt.  The
/// receipt intentionally contains only identity, structured process outcome,
/// bounded counters, and hashes of redacted error text—never prompts, output,
/// credentials, transcript data, or workspace paths.
fn terminalize_managed_codex_spawn_output(
    store: &LocalProductStore,
    lease: &ManagedCodexSpawnLease,
    output: &NodeExecutionOutput,
) -> Result<(), String> {
    let (status, terminal_class) = managed_codex_terminal_state(output);
    let error_domain = output.error_domain.as_deref().unwrap_or("none");
    let process_outcome = output.process_outcome.as_ref().map(|outcome| {
        json!({
            "schema_version": outcome.schema_version,
            "state": outcome.state,
            "exit_code": outcome.exit_code,
            "signal": outcome.signal,
            "unavailable_reason_sha256": outcome
                .unavailable_reason
                .as_deref()
                .map(|value| hex::encode(Sha256::digest(value.as_bytes()))),
        })
    });
    let receipt = json!({
        "schema_version": "managed_codex_attempt_terminal.v1",
        "product_task_id": lease.product_task_id(),
        "product_task_version": lease.product_task_version(),
        "tenant_id": lease.tenant_id(),
        "principal_id": lease.principal_id(),
        "principal_kind": lease.principal_kind().as_str(),
        "spend_authorization_id": lease.spend_authorization_id(),
        "attempt_id": lease.attempt_id(),
        "execution_id": lease.execution_id(),
        "status": status,
        "terminal_class": terminal_class,
        "error_domain": error_domain,
        "error_message_sha256": output
            .error_message
            .as_deref()
            .map(|value| hex::encode(Sha256::digest(value.as_bytes()))),
        "process_outcome": process_outcome,
        "input_tokens": output.input_tokens,
        "output_tokens": output.output_tokens,
        "content_excluded": true,
    });
    store.terminalize_managed_codex_spawn(lease, status, terminal_class, &receipt)?;
    Ok(())
}

/// Map every gateway/process completion into one durable attempt terminal
/// state. The caller performs the store-owned current-lease check and writes
/// the receipt exactly once after applying this classification.
fn managed_codex_terminal_state(output: &NodeExecutionOutput) -> (&'static str, &'static str) {
    let error_domain = output.error_domain.as_deref().unwrap_or("none");
    let process_state = output
        .process_outcome
        .as_ref()
        .map(|outcome| outcome.state.as_str())
        .unwrap_or("unavailable");
    if output.status == "completed" {
        ("succeeded", "process_succeeded")
    } else if error_domain.contains("cancel") || process_state == "cancelled" {
        ("cancelled", "cancelled")
    } else if error_domain.contains("outcome_unknown") || process_state == "outcome_unknown" {
        ("outcome_unknown", "outcome_unknown")
    } else if error_domain.contains("budget") || error_domain.contains("output_limit") {
        ("failed", "budget_exhausted")
    } else if error_domain.contains("timeout") || process_state == "timed_out" {
        ("failed", "timeout")
    } else if error_domain.contains("spawn") {
        ("failed", "spawn_failed")
    } else if error_domain.contains("gateway") {
        ("failed", "gateway_failed")
    } else if error_domain.contains("cleanup_incomplete") {
        ("failed", "pre_child_cleanup_incomplete")
    } else {
        ("failed", "execution_failed")
    }
}

fn reconciled_token_pair(
    reconciled: &UsageReconcileResult,
    usage: &super::codex_budget_authority::BudgetGatewayUsage,
) -> (Option<i64>, Option<i64>) {
    match reconciled {
        UsageReconcileResult::PreferGateway {
            input_tokens,
            output_tokens,
            ..
        }
        | UsageReconcileResult::PreferSessionOnly {
            input_tokens,
            output_tokens,
            ..
        } => (Some(*input_tokens as i64), Some(*output_tokens as i64)),
        UsageReconcileResult::Conflict { .. } | UsageReconcileResult::Missing { .. } => (
            Some(usage.cumulative_input_tokens as i64),
            Some(usage.cumulative_output_tokens as i64),
        ),
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
    use crate::cli::codex_partial_mediation_authority_decision::OPERATOR_RISK_ACCEPTANCE_PHRASE;
    use crate::product_golden_path::{
        validate_intake, ProductExecutorPolicy, ProductTaskBudget, ProductTaskIntakeRequest,
        ProductVerificationCommand, PRODUCT_TASK_GATE,
    };
    use crate::storage::local_product_store::{
        CostAuthority, RiskAcknowledgementRequest, SpendAuthorizationRequest,
        ALL_MANAGED_ACCEPTANCE_SCOPES,
    };
    use serde_json::json;
    use std::sync::Arc;

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::cli::config::cli_env_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[cfg(unix)]
    struct ProductTaskOutputEnvGuard {
        product_task_gate: Option<std::ffi::OsString>,
        target_output_enabled: Option<std::ffi::OsString>,
        target_output_kill_switch: Option<std::ffi::OsString>,
        cli_execution_enabled: Option<std::ffi::OsString>,
        codex_bin: Option<std::ffi::OsString>,
        codex_sha256: Option<std::ffi::OsString>,
        codex_version_policy: Option<std::ffi::OsString>,
        codex_model: Option<std::ffi::OsString>,
        codex_runtime_profile_id: Option<std::ffi::OsString>,
        codex_required_capabilities: Option<std::ffi::OsString>,
        upstream_api_key: Option<std::ffi::OsString>,
        fallback_upstream_api_key: Option<std::ffi::OsString>,
    }

    #[cfg(unix)]
    impl ProductTaskOutputEnvGuard {
        fn enable() -> Self {
            let guard = Self {
                product_task_gate: std::env::var_os(PRODUCT_TASK_GATE),
                target_output_enabled: std::env::var_os("ACP_ENABLE_TARGET_REPO_OUTPUT"),
                target_output_kill_switch: std::env::var_os("ACP_TARGET_REPO_OUTPUT_KILL_SWITCH"),
                cli_execution_enabled: std::env::var_os("ACP_ENABLE_CLI_EXECUTION"),
                codex_bin: std::env::var_os("ACP_CODEX_BIN"),
                codex_sha256: std::env::var_os("ACP_CODEX_SHA256"),
                codex_version_policy: std::env::var_os("ACP_CODEX_VERSION_POLICY"),
                codex_model: std::env::var_os("ACP_CODEX_MODEL"),
                codex_runtime_profile_id: std::env::var_os("ACP_CODEX_RUNTIME_PROFILE_ID"),
                codex_required_capabilities: std::env::var_os("ACP_CODEX_REQUIRED_CAPABILITIES"),
                upstream_api_key: std::env::var_os("ACP_CODEX_UPSTREAM_API_KEY"),
                fallback_upstream_api_key: std::env::var_os("OPENAI_API_KEY"),
            };
            std::env::set_var(PRODUCT_TASK_GATE, "1");
            std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
            std::env::set_var("ACP_TARGET_REPO_OUTPUT_KILL_SWITCH", "0");
            std::env::set_var("ACP_ENABLE_CLI_EXECUTION", "1");
            std::env::set_var(
                "ACP_CODEX_UPSTREAM_API_KEY",
                "provider-free-test-parent-key",
            );
            std::env::set_var("OPENAI_API_KEY", "provider-free-test-fallback-parent-key");
            guard
        }
    }

    #[cfg(unix)]
    impl Drop for ProductTaskOutputEnvGuard {
        fn drop(&mut self) {
            restore_test_env(PRODUCT_TASK_GATE, self.product_task_gate.take());
            restore_test_env(
                "ACP_ENABLE_TARGET_REPO_OUTPUT",
                self.target_output_enabled.take(),
            );
            restore_test_env(
                "ACP_TARGET_REPO_OUTPUT_KILL_SWITCH",
                self.target_output_kill_switch.take(),
            );
            restore_test_env(
                "ACP_ENABLE_CLI_EXECUTION",
                self.cli_execution_enabled.take(),
            );
            restore_test_env("ACP_CODEX_BIN", self.codex_bin.take());
            restore_test_env("ACP_CODEX_SHA256", self.codex_sha256.take());
            restore_test_env("ACP_CODEX_VERSION_POLICY", self.codex_version_policy.take());
            restore_test_env("ACP_CODEX_MODEL", self.codex_model.take());
            restore_test_env(
                "ACP_CODEX_RUNTIME_PROFILE_ID",
                self.codex_runtime_profile_id.take(),
            );
            restore_test_env(
                "ACP_CODEX_REQUIRED_CAPABILITIES",
                self.codex_required_capabilities.take(),
            );
            restore_test_env("ACP_CODEX_UPSTREAM_API_KEY", self.upstream_api_key.take());
            restore_test_env("OPENAI_API_KEY", self.fallback_upstream_api_key.take());
        }
    }

    #[cfg(unix)]
    fn restore_test_env(name: &str, value: Option<std::ffi::OsString>) {
        if let Some(value) = value {
            std::env::set_var(name, value);
        } else {
            std::env::remove_var(name);
        }
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

    #[cfg(unix)]
    fn init_product_repo(root: &Path) -> String {
        std::fs::create_dir_all(root).unwrap();
        std::fs::write(root.join("README.md"), "fixture\n").unwrap();
        for args in [
            &["init", "-b", "main"][..],
            &["config", "user.email", "executor-test@example.invalid"][..],
            &["config", "user.name", "Executor Test"][..],
            &["add", "README.md"][..],
            &["commit", "-m", "fixture"][..],
            &[
                "remote",
                "add",
                "origin",
                "https://example.invalid/cli-product.git",
            ][..],
        ] {
            let output = Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .unwrap();
            assert!(output.status.success(), "git {args:?}: {output:?}");
        }
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(root)
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }

    #[cfg(unix)]
    fn admitted_fake_codex_with_child_marker(
        root: &Path,
        marker: &Path,
        version_probe_credential_marker: &Path,
    ) -> CodexAdmission {
        use std::os::unix::fs::PermissionsExt;

        let binary = root.join("codex-managed-spawn-fixture");
        std::fs::write(
            &binary,
            format!(
                "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  if [ -n \"${{ACP_CODEX_UPSTREAM_API_KEY:-}}\" ] || [ -n \"${{OPENAI_API_KEY:-}}\" ]; then\n    printf parent-credential-visible > {}\n    exit 91\n  fi\n  printf 'codex-cli 0.145.0\\n'\n  exit 0\nfi\nif [ \"$1\" = \"exec\" ] && [ \"$2\" = \"--help\" ]; then\n  printf '%s\\n' '--json --sandbox workspace-write --ask-for-approval --model'\n  exit 0\nfi\nprintf child-spawned > {}\nexit 0\n",
                version_probe_credential_marker.to_string_lossy(),
                marker.to_string_lossy(),
            ),
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let digest = hex::encode(Sha256::digest(std::fs::read(&binary).unwrap()));
        std::env::set_var("ACP_CODEX_BIN", &binary);
        std::env::set_var("ACP_CODEX_SHA256", digest);
        std::env::set_var("ACP_CODEX_VERSION_POLICY", "=0.145.0");
        std::env::set_var("ACP_CODEX_MODEL", "gpt-5.6-luna");
        std::env::set_var("ACP_CODEX_RUNTIME_PROFILE_ID", "test-codex-runtime-profile");
        std::env::set_var("ACP_CODEX_REQUIRED_CAPABILITIES", "--json");
        crate::cli::config::CliConfig::from_env()
            .codex_admission
            .expect("fixture Codex runtime profile must admit")
    }

    #[cfg(unix)]
    fn managed_codex_decision_body(
        decision_id: &str,
        task_id: &str,
        workflow_id: &str,
        node_id: &str,
        attempt_id: &str,
        target_main_sha: &str,
        admission: &CodexAdmission,
    ) -> Value {
        json!({
            "decision_id": decision_id,
            "schema_version": "codex_partial_mediation_authority_decision.v2",
            "status": "draft_pending_operator",
            "invalidation_state": "none",
            "acknowledgement": {"required_phrase": OPERATOR_RISK_ACCEPTANCE_PHRASE},
            "trial_envelope": {
                "max_retries": 0,
                "max_provider_requests": 1,
                "draft_pr_only": true,
                "max_input_tokens": 8_000,
                "max_output_tokens": 4_000,
                "max_total_tokens": 12_000,
                "max_wall_time_ms": 300_000,
                "exact_codex_version": admission.binary_version,
                "exact_codex_sha_required": true,
                "runtime_profile_sha256": admission.runtime_profile_sha256().unwrap(),
                "capability_probe_sha256": admission.capability_probe_sha256(),
                "provider_kind": "openai_compatible",
                "provider_host": "api.openai.com",
                "provider_base_url": "https://api.openai.com/v1",
                "admitted_endpoint_paths": ["/v1/responses"],
                "model": admission.model,
                "product_task_id": task_id,
                "workflow_id": workflow_id,
                "workflow_node_id": node_id,
                "execution_id": format!("codex-attempt-{attempt_id}"),
                "attempt_id": attempt_id,
                "target_repo": "managed-codex-target",
                "target_main_sha": target_main_sha,
                "exact_codex_path": admission.binary_path,
                "exact_codex_sha256": admission.binary_sha256,
                "cancellation_identity": "cancel-managed-codex",
                "rollback_identity": "rollback-managed-codex",
                "output_branch_prefix": "acp/",
                "cost_authority": {
                    "kind": "cost_unavailable",
                    "monetary_ceiling_enforced": false,
                    "note": "rely on request/token/time caps; no monetary ceiling claimed",
                },
                "auto_merge_disabled": true,
            },
        })
    }

    #[cfg(unix)]
    struct ManagedCodexExecutionFixture {
        store: Arc<LocalProductStore>,
        workspace_path: PathBuf,
        run_id: String,
        workflow_id: String,
        node_id: String,
        admission: CodexAdmission,
        marker: PathBuf,
        version_probe_credential_marker: PathBuf,
        attempt_id: String,
        spend_authorization_id: String,
    }

    #[cfg(unix)]
    fn prepare_managed_codex_execution_fixture(root: &Path) -> ManagedCodexExecutionFixture {
        let repo = root.join("target");
        let revision = init_product_repo(&repo);
        let store = Arc::new(LocalProductStore::new(root.join("store.db")).unwrap());
        let marker = root.join("codex-child-spawned");
        let version_probe_credential_marker =
            root.join("codex-version-probe-observed-parent-credential");
        let admission =
            admitted_fake_codex_with_child_marker(root, &marker, &version_probe_credential_marker);

        let intake = ProductTaskIntakeRequest {
            objective: "managed Codex executor authority fixture".to_string(),
            target_id: "managed-codex-target".to_string(),
            target_repo_path: repo.to_string_lossy().into_owned(),
            source_kind: None,
            source_revision: revision.clone(),
            source_tree_hash: None,
            allowed_paths: vec!["docs/managed.md".to_string()],
            verification_commands: vec![ProductVerificationCommand {
                command: "test -f README.md".to_string(),
                timeout_ms: 5_000,
            }],
            output_intent: "artifact_only".to_string(),
            executor_policy: ProductExecutorPolicy {
                allowed_executors: vec!["codex_cli".to_string()],
                prefer: Some("codex_cli".to_string()),
            },
            budget: Some(ProductTaskBudget {
                total_tokens: Some(12_000),
                total_calls: Some(1),
                total_elapsed_ms: Some(300_000),
                max_retries: Some(0),
                max_repairs: Some(0),
                max_concurrency: Some(1),
                stage_budgets: None,
            }),
            risk_class: "low".to_string(),
            approval_required: true,
            confirm_execution: Some(true),
            confirm_output: Some(true),
            idempotency_key: "managed-codex-executor-authority".to_string(),
            expected_version: None,
            tenant_id: Some("managed-codex-tenant".to_string()),
            workspace_id: Some("managed-codex-workspace".to_string()),
            workspace_mode: Some("git_worktree".to_string()),
        };
        let validated =
            validate_intake(&intake, "managed-codex-tenant", "managed-codex-workspace").unwrap();
        let task = store.admit_product_task(&validated, "operator").unwrap();
        let task_id = task["task_id"].as_str().unwrap().to_string();
        let compiled = store
            .compile_and_schedule_product_task(&task_id, "operator", &["codex_cli".to_string()])
            .unwrap();
        let run_id = compiled["task"]["run_id"].as_str().unwrap().to_string();
        let workspace_path = store
            .get_product_task(&task_id)
            .unwrap()
            .unwrap()
            .pointer("/workspace_binding/workspace_canonical_path")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .expect("compiled ProductTask must retain its canonical workspace owner");
        let run = store.get_workflow_run(&run_id).unwrap().unwrap();
        let node = run["nodes"][0].clone();
        let workflow_id = run["workflow_id"].as_str().unwrap().to_string();
        let node_id = node["node_id"].as_str().unwrap().to_string();

        let key_id = "operator-key-managed-codex";
        let scopes = ALL_MANAGED_ACCEPTANCE_SCOPES
            .iter()
            .map(|scope| (*scope).to_string())
            .collect::<Vec<_>>();
        store
            .record_api_key_metadata_for_tenant(
                "managed-codex-tenant",
                key_id,
                "operator-user-managed-codex",
                "operator",
                &scopes,
                "test",
            )
            .unwrap();
        let principal = store
            .authenticate_managed_acceptance_principal("managed-codex-tenant", key_id, None)
            .unwrap();
        let attempt_id = format!("managed-codex-attempt-{}", uuid::Uuid::new_v4());
        let decision_id = "managed-codex-decision";
        let residual = "ab".repeat(32);
        let expires_at = (chrono::Utc::now() + chrono::Duration::hours(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let decision = store
            .upsert_managed_acceptance_decision(
                "managed-codex-tenant",
                &managed_codex_decision_body(
                    decision_id,
                    &task_id,
                    &workflow_id,
                    &node_id,
                    &attempt_id,
                    &revision,
                    &admission,
                ),
                &residual,
                "draft_pending_operator",
                None,
                Some(&expires_at),
            )
            .unwrap();
        let risk = store
            .accept_managed_acceptance_decision(
                &principal,
                &RiskAcknowledgementRequest {
                    decision_id: decision_id.to_string(),
                    expected_decision_body_sha256: decision["decision_body_sha256"]
                        .as_str()
                        .unwrap()
                        .to_string(),
                    expected_residual_finding_sha256: residual,
                    submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.to_string(),
                    explicit_go: true,
                },
            )
            .unwrap();
        let spend = store
            .issue_managed_acceptance_spend_authorization(
                &principal,
                &SpendAuthorizationRequest {
                    risk_authorization_id: risk["authorization_id"].as_str().unwrap().to_string(),
                    product_task_id: task_id,
                    workflow_id: Some(workflow_id.clone()),
                    workflow_node_id: Some(node_id.clone()),
                    execution_id: format!("codex-attempt-{attempt_id}"),
                    attempt_id: attempt_id.clone(),
                    binary_path: admission.binary_path.to_string_lossy().into_owned(),
                    binary_version: admission.binary_version.clone(),
                    binary_sha256: admission.binary_sha256.clone(),
                    runtime_profile_sha256: admission.runtime_profile_sha256().unwrap(),
                    capability_probe_sha256: admission
                        .capability_probe_sha256()
                        .map(str::to_string),
                    provider_kind: "openai_compatible".to_string(),
                    provider_host: "api.openai.com".to_string(),
                    provider_base_url: "https://api.openai.com/v1".to_string(),
                    admitted_endpoint_paths: vec!["/v1/responses".to_string()],
                    model: admission.model.clone(),
                    target_repo: "managed-codex-target".to_string(),
                    target_main_sha: revision,
                    output_branch_prefix: "acp/".to_string(),
                    draft_pr_only: true,
                    max_provider_requests: 1,
                    max_retries: 0,
                    max_input_tokens: 8_000,
                    max_output_tokens: 4_000,
                    max_total_tokens: 12_000,
                    max_wall_time_ms: 300_000,
                    cost_authority: CostAuthority::CostUnavailable,
                    cancellation_identity: "cancel-managed-codex".to_string(),
                    rollback_identity: "rollback-managed-codex".to_string(),
                },
            )
            .unwrap();
        let spend_authorization_id = spend["spend_authorization_id"]
            .as_str()
            .unwrap()
            .to_string();
        store
            .bind_managed_codex_spend_to_product_node(&principal, &spend_authorization_id)
            .unwrap();

        ManagedCodexExecutionFixture {
            store,
            workspace_path,
            run_id,
            workflow_id,
            node_id,
            admission,
            marker,
            version_probe_credential_marker,
            attempt_id,
            spend_authorization_id,
        }
    }

    #[cfg(unix)]
    #[test]
    fn product_scheduler_executor_uses_store_lease_before_gateway_or_child_spawn() {
        let _guard = env_lock();
        let _env = ProductTaskOutputEnvGuard::enable();
        let root = tempfile::tempdir().unwrap();
        let fixture = prepare_managed_codex_execution_fixture(root.path());
        let executor = CliNodeExecutor::admitted_codex(
            fixture.admission.clone(),
            Some(fixture.admission.binary_path.to_string_lossy().into_owned()),
            300_000,
        )
        .with_managed_acceptance_store(Arc::clone(&fixture.store));
        let tick = fixture
            .store
            .tick_with_executor(&fixture.run_id, "scheduler", 1, &executor)
            .expect("scheduler must reach the real CliNodeExecutor authority seam");
        assert_eq!(tick["result"]["executor_type"], "codex_cli");
        assert!(
            !fixture.marker.exists(),
            "child Codex must not spawn without a proved runtime lease path"
        );
        assert!(
            !fixture.version_probe_credential_marker.exists(),
            "every managed Codex version probe must clear parent-only credentials"
        );

        let attempt = fixture
            .store
            .get_managed_acceptance_attempt(&fixture.attempt_id)
            .unwrap()
            .expect("store admission must create one attempt lease");
        assert_eq!(attempt["status"], "failed");
        assert!(attempt["terminal_class"].as_str().is_some());
        assert_eq!(attempt["receipt_json"]["content_excluded"], true);
        assert!(attempt["receipt_json"].get("lease_token").is_none());
        let consumed = fixture
            .store
            .get_managed_acceptance_spend_authorization(&fixture.spend_authorization_id)
            .unwrap()
            .unwrap();
        assert_eq!(consumed["status"], "consumed");
        assert_eq!(consumed["consumed_by_attempt_id"], fixture.attempt_id);
    }

    #[cfg(unix)]
    #[test]
    fn runtime_owner_preflight_blocks_before_managed_child_spawn() {
        let _guard = env_lock();
        let _env = ProductTaskOutputEnvGuard::enable();
        let root = tempfile::tempdir().unwrap();
        let fixture = prepare_managed_codex_execution_fixture(root.path());
        let facts = ManagedCodexLaunchFacts {
            run_id: fixture.run_id.clone(),
            workflow_id: fixture.workflow_id.clone(),
            node_id: fixture.node_id.clone(),
            workspace_path: fixture.workspace_path.clone(),
            executable_path: fixture.admission.binary_path.clone(),
            executable_version: fixture.admission.binary_version.clone(),
            executable_sha256: fixture.admission.binary_sha256.clone(),
            runtime_profile_sha256: fixture.admission.runtime_profile_sha256().unwrap(),
            capability_probe_sha256: fixture
                .admission
                .capability_probe_sha256()
                .map(str::to_string),
            model: fixture.admission.model.clone(),
        };
        let lease = fixture.store.admit_managed_codex_spawn(&facts).unwrap();
        let authority = authority_from_managed_codex_spawn_lease(&lease).unwrap();
        let binary = fixture.admission.binary_path.to_string_lossy().into_owned();
        let journal_path =
            crate::cli::codex_usage_journal::parent_owned_journal_path(lease.execution_id());
        assert!(!journal_path.exists());

        let output = execute_product_codex_after_store_admission(
            &binary,
            &fixture.workspace_path,
            "provider-free runtime preflight fixture",
            std::time::Instant::now(),
            &fixture.admission,
            fixture.store.as_ref(),
            &lease,
            authority,
        );

        assert_eq!(
            output.error_domain.as_deref(),
            Some("cli_execution_authority_invalid")
        );
        assert!(
            output
                .error_message
                .as_deref()
                .is_some_and(|message| message
                    .starts_with("managed Codex owner-derived preflight blocked: blocked_")),
            "runtime preflight must expose only a bounded blocker class: {output:?}"
        );
        assert!(!format!("{output:?}").contains("provider-free-test-parent-key"));
        assert!(!format!("{output:?}").contains("provider-free-test-fallback-parent-key"));
        assert!(output.input_tokens.is_none());
        assert!(output.output_tokens.is_none());
        assert!(
            !fixture.marker.exists(),
            "runtime preflight must reject before the child can spawn"
        );
        assert!(
            !fixture.version_probe_credential_marker.exists(),
            "runtime preflight must not expose parent-only credentials to a version probe"
        );
        assert!(
            !journal_path.exists(),
            "a pre-child preflight failure must clean its parent-owned journal"
        );

        terminalize_managed_codex_spawn_output(fixture.store.as_ref(), &lease, &output).unwrap();
        let attempt = fixture
            .store
            .get_managed_acceptance_attempt(&fixture.attempt_id)
            .unwrap()
            .unwrap();
        assert_eq!(attempt["status"], "failed");
        assert_eq!(attempt["receipt_json"]["content_excluded"], true);
        let spend = fixture
            .store
            .get_managed_acceptance_spend_authorization(&fixture.spend_authorization_id)
            .unwrap()
            .unwrap();
        assert_eq!(spend["status"], "consumed");
    }

    #[cfg(unix)]
    #[test]
    fn final_store_confirmation_accepts_only_the_exact_consumed_lease_and_rechecks_the_gate() {
        let _guard = env_lock();
        let _env = ProductTaskOutputEnvGuard::enable();
        let root = tempfile::tempdir().unwrap();
        let fixture = prepare_managed_codex_execution_fixture(root.path());
        let facts = ManagedCodexLaunchFacts {
            run_id: fixture.run_id.clone(),
            workflow_id: fixture.workflow_id.clone(),
            node_id: fixture.node_id.clone(),
            workspace_path: fixture.workspace_path.clone(),
            executable_path: fixture.admission.binary_path.clone(),
            executable_version: fixture.admission.binary_version.clone(),
            executable_sha256: fixture.admission.binary_sha256.clone(),
            runtime_profile_sha256: fixture.admission.runtime_profile_sha256().unwrap(),
            capability_probe_sha256: fixture
                .admission
                .capability_probe_sha256()
                .map(str::to_string),
            model: fixture.admission.model.clone(),
        };
        let lease = fixture.store.admit_managed_codex_spawn(&facts).unwrap();
        let runtime =
            crate::cli::codex_mediation_admission::ManagedCodexRuntimeAttestation::fully_attested_for_test();

        fixture
            .store
            .confirm_managed_codex_spawn_before_child(&lease, &runtime)
            .expect("the exact consumed lease must be revalidated without requiring active spend");

        // This flip occurs after lease admission. The final store confirmation,
        // immediately before a child could be spawned, must refuse it.
        std::env::set_var(PRODUCT_TASK_GATE, "0");
        let error = fixture
            .store
            .confirm_managed_codex_spawn_before_child(&lease, &runtime)
            .unwrap_err();
        assert_eq!(error, "product golden path execution gate is disabled");
        assert!(
            !fixture.marker.exists(),
            "the direct confirmation seam never starts a child process"
        );
    }

    #[cfg(unix)]
    #[test]
    fn pre_child_cleanup_incomplete_is_bounded_and_terminalized() {
        struct EphemeralHomeFileGuard(PathBuf);

        impl Drop for EphemeralHomeFileGuard {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }

        let _guard = env_lock();
        let _env = ProductTaskOutputEnvGuard::enable();
        let root = tempfile::tempdir().unwrap();
        let fixture = prepare_managed_codex_execution_fixture(root.path());
        let facts = ManagedCodexLaunchFacts {
            run_id: fixture.run_id.clone(),
            workflow_id: fixture.workflow_id.clone(),
            node_id: fixture.node_id.clone(),
            workspace_path: fixture.workspace_path.clone(),
            executable_path: fixture.admission.binary_path.clone(),
            executable_version: fixture.admission.binary_version.clone(),
            executable_sha256: fixture.admission.binary_sha256.clone(),
            runtime_profile_sha256: fixture.admission.runtime_profile_sha256().unwrap(),
            capability_probe_sha256: fixture
                .admission
                .capability_probe_sha256()
                .map(str::to_string),
            model: fixture.admission.model.clone(),
        };
        let lease = fixture.store.admit_managed_codex_spawn(&facts).unwrap();
        let authority = authority_from_managed_codex_spawn_lease(&lease).unwrap();
        let ephemeral_home =
            std::env::temp_dir().join(format!("acp-codex-home-{}", lease.execution_id()));
        assert!(!ephemeral_home.exists());
        std::fs::write(&ephemeral_home, "not a directory").unwrap();
        let _ephemeral_home_guard = EphemeralHomeFileGuard(ephemeral_home);
        let journal_path =
            crate::cli::codex_usage_journal::parent_owned_journal_path(lease.execution_id());

        let output = execute_product_codex_after_store_admission(
            &fixture.admission.binary_path.to_string_lossy(),
            &fixture.workspace_path,
            "provider-free cleanup failure fixture",
            std::time::Instant::now(),
            &fixture.admission,
            fixture.store.as_ref(),
            &lease,
            authority,
        );

        assert_eq!(
            output.error_message.as_deref(),
            Some("managed Codex pre-child cleanup incomplete after ephemeral_home_write")
        );
        assert_eq!(
            output.error_domain.as_deref(),
            Some("cli_execution_cleanup_incomplete")
        );
        assert!(!journal_path.exists());
        assert!(!fixture.marker.exists());
        assert!(!fixture.version_probe_credential_marker.exists());

        terminalize_managed_codex_spawn_output(fixture.store.as_ref(), &lease, &output).unwrap();
        let attempt = fixture
            .store
            .get_managed_acceptance_attempt(&fixture.attempt_id)
            .unwrap()
            .unwrap();
        assert_eq!(attempt["status"], "failed");
        assert_eq!(attempt["terminal_class"], "pre_child_cleanup_incomplete");
        assert_eq!(
            attempt["receipt_json"]["error_domain"],
            "cli_execution_cleanup_incomplete"
        );
        assert_eq!(attempt["receipt_json"]["content_excluded"], true);
    }

    #[cfg(unix)]
    #[test]
    fn gateway_start_cleanup_incomplete_is_bounded_and_terminalized() {
        struct JournalDirectoryGuard(PathBuf);

        impl Drop for JournalDirectoryGuard {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        let _guard = env_lock();
        let _env = ProductTaskOutputEnvGuard::enable();
        let root = tempfile::tempdir().unwrap();
        let fixture = prepare_managed_codex_execution_fixture(root.path());
        let facts = ManagedCodexLaunchFacts {
            run_id: fixture.run_id.clone(),
            workflow_id: fixture.workflow_id.clone(),
            node_id: fixture.node_id.clone(),
            workspace_path: fixture.workspace_path.clone(),
            executable_path: fixture.admission.binary_path.clone(),
            executable_version: fixture.admission.binary_version.clone(),
            executable_sha256: fixture.admission.binary_sha256.clone(),
            runtime_profile_sha256: fixture.admission.runtime_profile_sha256().unwrap(),
            capability_probe_sha256: fixture
                .admission
                .capability_probe_sha256()
                .map(str::to_string),
            model: fixture.admission.model.clone(),
        };
        let lease = fixture.store.admit_managed_codex_spawn(&facts).unwrap();
        let authority = authority_from_managed_codex_spawn_lease(&lease).unwrap();
        let journal_path =
            crate::cli::codex_usage_journal::parent_owned_journal_path(lease.execution_id());
        assert!(!journal_path.exists());
        std::fs::create_dir_all(&journal_path).unwrap();
        let _journal_guard = JournalDirectoryGuard(journal_path.clone());

        let output = execute_product_codex_after_store_admission(
            &fixture.admission.binary_path.to_string_lossy(),
            &fixture.workspace_path,
            "provider-free gateway start cleanup fixture",
            std::time::Instant::now(),
            &fixture.admission,
            fixture.store.as_ref(),
            &lease,
            authority,
        );

        assert_eq!(
            output.error_message.as_deref(),
            Some("managed Codex pre-child cleanup incomplete after gateway_start")
        );
        assert_eq!(
            output.error_domain.as_deref(),
            Some("cli_execution_cleanup_incomplete")
        );
        assert!(!fixture.marker.exists());
        assert!(!fixture.version_probe_credential_marker.exists());

        terminalize_managed_codex_spawn_output(fixture.store.as_ref(), &lease, &output).unwrap();
        let attempt = fixture
            .store
            .get_managed_acceptance_attempt(&fixture.attempt_id)
            .unwrap()
            .unwrap();
        assert_eq!(attempt["status"], "failed");
        assert_eq!(attempt["terminal_class"], "pre_child_cleanup_incomplete");
        assert_eq!(
            attempt["receipt_json"]["error_domain"],
            "cli_execution_cleanup_incomplete"
        );
        assert_eq!(attempt["receipt_json"]["content_excluded"], true);
    }

    #[cfg(unix)]
    #[test]
    fn product_run_owner_cannot_be_downgraded_by_stripped_node_metadata() {
        let _guard = env_lock();
        let _env = ProductTaskOutputEnvGuard::enable();

        {
            let root = tempfile::tempdir().unwrap();
            let repo = root.path().join("target");
            let revision = init_product_repo(&repo);
            let database_path = root.path().join("store.db");
            let store = Arc::new(LocalProductStore::new(&database_path).unwrap());
            let marker = root.path().join("generic-codex-child-spawned");
            let version_probe_marker = root.path().join("generic-codex-version-probe");
            let admission =
                admitted_fake_codex_with_child_marker(root.path(), &marker, &version_probe_marker);
            let binary = admission.binary_path.clone();
            let intake = ProductTaskIntakeRequest {
                objective: "ProductTask run-owner routing regression".to_string(),
                target_id: "managed-codex-target".to_string(),
                target_repo_path: repo.to_string_lossy().into_owned(),
                source_kind: None,
                source_revision: revision,
                source_tree_hash: None,
                allowed_paths: vec!["docs/managed.md".to_string()],
                verification_commands: vec![ProductVerificationCommand {
                    command: "test -f README.md".to_string(),
                    timeout_ms: 5_000,
                }],
                output_intent: "artifact_only".to_string(),
                executor_policy: ProductExecutorPolicy {
                    allowed_executors: vec!["codex_cli".to_string()],
                    prefer: Some("codex_cli".to_string()),
                },
                budget: Some(ProductTaskBudget {
                    total_tokens: Some(12_000),
                    total_calls: Some(1),
                    total_elapsed_ms: Some(300_000),
                    max_retries: Some(0),
                    max_repairs: Some(0),
                    max_concurrency: Some(1),
                    stage_budgets: None,
                }),
                risk_class: "low".to_string(),
                approval_required: true,
                confirm_execution: Some(true),
                confirm_output: Some(true),
                idempotency_key: "managed-codex-run-owner-routing".to_string(),
                expected_version: None,
                tenant_id: Some("managed-codex-tenant".to_string()),
                workspace_id: Some("managed-codex-workspace".to_string()),
                workspace_mode: Some("git_worktree".to_string()),
            };
            let validated =
                validate_intake(&intake, "managed-codex-tenant", "managed-codex-workspace")
                    .unwrap();
            let task = store.admit_product_task(&validated, "operator").unwrap();
            let task_id = task["task_id"].as_str().unwrap();
            let compiled = store
                .compile_and_schedule_product_task(task_id, "operator", &["codex_cli".to_string()])
                .unwrap();
            let run_id = compiled["task"]["run_id"].as_str().unwrap().to_string();
            let node_id = store.get_workflow_run(&run_id).unwrap().unwrap()["nodes"][0]["node_id"]
                .as_str()
                .unwrap()
                .to_string();

            // Simulate an untrusted scheduler metadata mutation.  The durable
            // ProductTask row still owns this run, so executor routing must not
            // fall through to direct `Command::spawn`.
            let connection = rusqlite::Connection::open(&database_path).unwrap();
            let encoded: String = connection
                .query_row(
                    "SELECT node_json FROM workflow_run_nodes WHERE run_id=?1 AND node_id=?2",
                    [&run_id, &node_id],
                    |row| row.get(0),
                )
                .unwrap();
            let mut node: Value = serde_json::from_str(&encoded).unwrap();
            node.as_object_mut()
                .unwrap()
                .remove("managed_supervised_patch");
            node.as_object_mut().unwrap().remove("product_task_id");
            connection
                .execute(
                    "UPDATE workflow_run_nodes SET node_json=?1 WHERE run_id=?2 AND node_id=?3",
                    rusqlite::params![node.to_string(), run_id, node_id],
                )
                .unwrap();

            let executor =
                CliNodeExecutor::new(None, Some(binary.to_string_lossy().into_owned()), 5_000)
                    .with_managed_acceptance_store(Arc::clone(&store));
            let tick = store
                .tick_with_executor(&run_id, "scheduler", 1, &executor)
                .expect("scheduler must reach the real run-owner executor seam");
            assert_eq!(
                tick.pointer("/result/error_domain").and_then(Value::as_str),
                Some("cli_execution_authority_invalid"),
                "durable run ownership must route to the store boundary"
            );
            assert!(
                !marker.exists(),
                "stripped node metadata must never permit generic Codex child spawn"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn codex_without_persisted_product_task_owner_fails_closed() {
        let root = tempfile::tempdir().unwrap();
        let store = Arc::new(LocalProductStore::new(root.path().join("store.db")).unwrap());
        let marker = root.path().join("unowned-codex-child-spawned");
        let binary = fake_codex(
            root.path(),
            &format!("printf child-spawned > {}", marker.to_string_lossy()),
        );
        let executor =
            CliNodeExecutor::new(None, Some(binary.to_string_lossy().into_owned()), 5_000)
                .with_managed_acceptance_store(store);
        let output = executor.execute_node(&make_input(json!({
            "executor": "codex_cli",
            "prompt": "unowned codex",
            "workspace_path": root.path(),
        })));
        assert_eq!(
            output.error_domain.as_deref(),
            Some("cli_execution_authority_invalid")
        );
        assert!(
            !marker.exists(),
            "unowned Codex must never reach Command::spawn"
        );
    }

    #[test]
    fn managed_codex_terminal_state_covers_every_process_outcome_class() {
        let cases = [
            ("completed", None, ("succeeded", "process_succeeded")),
            ("failed", Some("cli_cancelled"), ("cancelled", "cancelled")),
            (
                "failed",
                Some("cli_outcome_unknown"),
                ("outcome_unknown", "outcome_unknown"),
            ),
            (
                "failed",
                Some("cli_output_limit_exceeded"),
                ("failed", "budget_exhausted"),
            ),
            ("failed", Some("cli_timeout"), ("failed", "timeout")),
            (
                "failed",
                Some("cli_spawn_error"),
                ("failed", "spawn_failed"),
            ),
            (
                "failed",
                Some("cli_gateway_error"),
                ("failed", "gateway_failed"),
            ),
            (
                "failed",
                Some("cli_execution_authority_invalid"),
                ("failed", "execution_failed"),
            ),
        ];
        for (status, error_domain, expected) in cases {
            let output = NodeExecutionOutput {
                status: status.to_string(),
                executor_type: "codex_cli".to_string(),
                output: None,
                error_domain: error_domain.map(str::to_string),
                error_message: Some("redacted fixture failure".to_string()),
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(1),
                process_outcome: None,
                resolved_model: None,
            };
            assert_eq!(
                managed_codex_terminal_state(&output),
                expected,
                "{error_domain:?}"
            );
        }
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
            CliNodeExecutor::new(None, Some(binary.to_string_lossy().into_owned()), 30_000)
                .with_unmanaged_codex_for_test();
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
            CliNodeExecutor::new(None, Some(binary.to_string_lossy().into_owned()), 5_000)
                .with_unmanaged_codex_for_test();
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
        let executor = CliNodeExecutor::new(None, Some(binary.to_string_lossy().into_owned()), 50)
            .with_unmanaged_codex_for_test();
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
            CliNodeExecutor::new(None, Some(binary.to_string_lossy().into_owned()), 30_000)
                .with_unmanaged_codex_for_test();
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
        )
        .with_unmanaged_codex_for_test();
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
        let _guard = env_lock();
        std::env::remove_var("ACP_CLI_ENV_ALLOWLIST");
        assert!(cli_env_allowlist().is_empty());
    }

    #[test]
    fn claude_env_allowlist_excludes_host_paths_and_unadmitted_routing_overrides() {
        let _guard = env_lock();
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
