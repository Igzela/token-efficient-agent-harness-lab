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
use crate::storage::local_product_store::{
    validate_memory_strategy, ExternalRuntimeInvocationClaim, LocalProductStore,
};

pub const LANGGRAPH_TASK_TYPE: &str = "langgraph_external";
pub const LANGGRAPH_EXECUTOR_TYPE: &str = "langgraph_external";
pub const EXTERNAL_RUNTIME_NODE_SCHEMA_VERSION: &str = "external_runtime_node.v1";
pub const EXTERNAL_RUNTIME_REQUEST_SCHEMA_VERSION: &str = "external_runtime_request.v1";
pub const EXTERNAL_RUNTIME_RESULT_SCHEMA_VERSION: &str = "external_runtime_result.v1";
pub const EXTERNAL_RUNTIME_ADAPTER_CONTRACT_VERSION: &str = "external_runtime_adapter.v1";
pub const LANGGRAPH_ADAPTER_VERSION: &str = "0.1.0";
pub const PINNED_LANGGRAPH_VERSION: &str = "1.2.9";

pub const ENABLE_ENV: &str = "ACP_ENABLE_LANGGRAPH_RUNTIME";
pub const MODE_ENV: &str = "ACP_LANGGRAPH_MODE";
pub const PYTHON_ENV: &str = "ACP_LANGGRAPH_PYTHON";
pub const ADAPTER_PATH_ENV: &str = "ACP_LANGGRAPH_ADAPTER_PATH";
pub const KILL_SWITCH_ENV: &str = "ACP_LANGGRAPH_KILL_SWITCH";
pub const LIVE_CONFIRM_ENV: &str = "ACP_LANGGRAPH_LIVE_CONFIRM";
pub const PER_CALL_COST_CAP_ENV: &str = "ACP_LANGGRAPH_PER_CALL_COST_CAP_USD";
pub const RUN_COST_CAP_ENV: &str = "ACP_LANGGRAPH_RUN_COST_CAP_USD";
pub const DAILY_COST_CAP_ENV: &str = "ACP_LANGGRAPH_DAILY_COST_CAP_USD";
pub const TOKEN_CAP_ENV: &str = "ACP_LANGGRAPH_TOKEN_CAP";
pub const TIMEOUT_MS_ENV: &str = "ACP_LANGGRAPH_TIMEOUT_MS";
const LIVE_CONFIRMATION: &str = "I_UNDERSTAND_THIS_CALLS_A_PAID_PROVIDER";
const MAX_PROCESS_OUTPUT_BYTES: usize = 1_048_576;
const MAX_PROCESS_ERROR_BYTES: usize = 16 * 1024;
const MAX_BENCHMARK_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalRuntimeMode {
    Fixture,
    Live,
}

impl ExternalRuntimeMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Fixture => "fixture",
            Self::Live => "live",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExternalRuntimeConfig {
    pub mode: ExternalRuntimeMode,
    pub python_program: PathBuf,
    pub adapter_path: PathBuf,
    pub timeout_ms: u64,
    pub token_cap: i64,
    pub per_call_cost_cap_usd: f64,
    pub run_cost_cap_usd: f64,
    pub daily_cost_cap_usd: f64,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub adapter_version: String,
    pub expected_langgraph_version: String,
}

impl ExternalRuntimeConfig {
    pub fn from_env(
        provider_execution_enabled: bool,
        require_auth: bool,
        provider_available: bool,
    ) -> Result<Option<Self>, String> {
        if std::env::var(ENABLE_ENV).as_deref() != Ok("1") {
            return Ok(None);
        }
        if std::env::var(KILL_SWITCH_ENV).as_deref() == Ok("1") {
            return Err("LangGraph runtime kill switch is active".to_string());
        }
        if !require_auth {
            return Err("LangGraph runtime requires ACP_REQUIRE_AUTH=1".to_string());
        }
        let mode = match required_env(MODE_ENV)?.as_str() {
            "fixture" => ExternalRuntimeMode::Fixture,
            "live" => ExternalRuntimeMode::Live,
            _ => return Err(format!("{MODE_ENV} must be fixture or live")),
        };
        let python_program = canonical_executable_path(&required_env(PYTHON_ENV)?, PYTHON_ENV)?;
        let adapter_path =
            canonical_regular_file(&required_env(ADAPTER_PATH_ENV)?, ADAPTER_PATH_ENV)?;
        let timeout_ms = required_u64(TIMEOUT_MS_ENV, 1_000, 300_000)?;
        let token_cap = required_i64(TOKEN_CAP_ENV, 1, 200_000)?;
        let per_call_cost_cap_usd = required_positive_f64(PER_CALL_COST_CAP_ENV)?;
        let run_cost_cap_usd = required_positive_f64(RUN_COST_CAP_ENV)?;
        let daily_cost_cap_usd = required_positive_f64(DAILY_COST_CAP_ENV)?;
        if per_call_cost_cap_usd > run_cost_cap_usd || run_cost_cap_usd > daily_cost_cap_usd {
            return Err("LangGraph cost caps must satisfy per-call <= run <= daily".to_string());
        }
        if mode == ExternalRuntimeMode::Live {
            if std::env::var("CI").is_ok() {
                return Err("live LangGraph runtime is forbidden in CI".to_string());
            }
            if !provider_execution_enabled || !provider_available || !require_auth {
                return Err(
                    "live LangGraph runtime requires provider execution, configured provider, and auth"
                        .to_string(),
                );
            }
            if std::env::var(LIVE_CONFIRM_ENV).as_deref() != Ok(LIVE_CONFIRMATION) {
                return Err(format!(
                    "live LangGraph runtime requires exact {LIVE_CONFIRM_ENV} confirmation"
                ));
            }
        }
        Ok(Some(Self {
            mode,
            python_program,
            adapter_path,
            timeout_ms,
            token_cap,
            per_call_cost_cap_usd,
            run_cost_cap_usd,
            daily_cost_cap_usd,
            provider_id: None,
            model_id: None,
            adapter_version: LANGGRAPH_ADAPTER_VERSION.to_string(),
            expected_langgraph_version: PINNED_LANGGRAPH_VERSION.to_string(),
        }))
    }

    #[cfg(test)]
    pub fn fixture(python_program: PathBuf, adapter_path: PathBuf) -> Self {
        Self {
            mode: ExternalRuntimeMode::Fixture,
            python_program,
            adapter_path,
            timeout_ms: 30_000,
            token_cap: 20_000,
            per_call_cost_cap_usd: 0.01,
            run_cost_cap_usd: 0.05,
            daily_cost_cap_usd: 0.10,
            provider_id: None,
            model_id: None,
            adapter_version: LANGGRAPH_ADAPTER_VERSION.to_string(),
            expected_langgraph_version: PINNED_LANGGRAPH_VERSION.to_string(),
        }
    }

    pub fn bind_provider_identity(
        mut self,
        provider_id: impl Into<String>,
        model_id: impl Into<String>,
    ) -> Result<Self, String> {
        let provider_id = provider_id.into();
        let model_id = model_id.into();
        validate_wire_identifier("provider_id", &provider_id)?;
        validate_wire_identifier("model_id", &model_id)?;
        self.provider_id = Some(provider_id);
        self.model_id = Some(model_id);
        Ok(self)
    }
}

pub trait ExternalRuntimeInvoker: Send + Sync {
    fn invoke(&self, request: &Value, timeout_ms: u64) -> Result<Value, ExternalRuntimeError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalRuntimeError {
    pub code: String,
    pub message: String,
}

impl ExternalRuntimeError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: redact_sensitive_patterns(&message.into()),
        }
    }
}

pub struct LangGraphProcessInvoker {
    python_program: PathBuf,
    adapter_path: PathBuf,
}

impl LangGraphProcessInvoker {
    pub fn new(config: &ExternalRuntimeConfig) -> Self {
        Self {
            python_program: config.python_program.clone(),
            adapter_path: config.adapter_path.clone(),
        }
    }
}

impl ExternalRuntimeInvoker for LangGraphProcessInvoker {
    fn invoke(&self, request: &Value, timeout_ms: u64) -> Result<Value, ExternalRuntimeError> {
        let input = canonical_event_json(request)
            .map_err(|error| ExternalRuntimeError::new("request_encoding", error.to_string()))?;
        let mut command = Command::new(&self.python_program);
        command
            .arg(&self.adapter_path)
            .env_clear()
            .env("PYTHONIOENCODING", "utf-8")
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|error| ExternalRuntimeError::new("adapter_spawn", error.to_string()))?;
        child
            .stdin
            .take()
            .ok_or_else(|| ExternalRuntimeError::new("adapter_stdin", "adapter stdin missing"))?
            .write_all(input.as_bytes())
            .map_err(|error| ExternalRuntimeError::new("adapter_stdin", error.to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ExternalRuntimeError::new("adapter_stdout", "adapter stdout missing"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| ExternalRuntimeError::new("adapter_stderr", "adapter stderr missing"))?;
        let stdout_reader = thread::spawn(move || read_bounded(stdout, MAX_PROCESS_OUTPUT_BYTES));
        let stderr_reader = thread::spawn(move || read_bounded(stderr, MAX_PROCESS_ERROR_BYTES));
        let started = Instant::now();
        let status = loop {
            if std::env::var(KILL_SWITCH_ENV).as_deref() == Ok("1") {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ExternalRuntimeError::new(
                    "adapter_killed",
                    "LangGraph runtime kill switch became active",
                ));
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() >= Duration::from_millis(timeout_ms) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ExternalRuntimeError::new(
                        "adapter_timeout",
                        format!("LangGraph adapter timed out after {timeout_ms}ms"),
                    ));
                }
                Ok(None) => thread::sleep(Duration::from_millis(25)),
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ExternalRuntimeError::new("adapter_wait", error.to_string()));
                }
            }
        };
        let stdout = stdout_reader
            .join()
            .map_err(|_| ExternalRuntimeError::new("adapter_stdout", "stdout reader panicked"))?
            .map_err(|error| ExternalRuntimeError::new("adapter_stdout", error.to_string()))?;
        let stderr = stderr_reader
            .join()
            .map_err(|_| ExternalRuntimeError::new("adapter_stderr", "stderr reader panicked"))?
            .map_err(|error| ExternalRuntimeError::new("adapter_stderr", error.to_string()))?;
        if !status.success() {
            return Err(ExternalRuntimeError::new(
                "adapter_failed",
                String::from_utf8_lossy(&stderr.bytes).to_string(),
            ));
        }
        if stdout.truncated {
            return Err(ExternalRuntimeError::new(
                "adapter_output_oversized",
                "LangGraph adapter output exceeded bounded cap",
            ));
        }
        serde_json::from_slice(&stdout.bytes)
            .map_err(|error| ExternalRuntimeError::new("adapter_output_invalid", error.to_string()))
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

pub struct ExternalRuntimeNodeExecutor {
    store: Arc<LocalProductStore>,
    config: ExternalRuntimeConfig,
    invoker: Arc<dyn ExternalRuntimeInvoker>,
    provider_executor: Option<Arc<dyn NodeExecutor>>,
}

impl ExternalRuntimeNodeExecutor {
    pub fn new(
        store: Arc<LocalProductStore>,
        config: ExternalRuntimeConfig,
        invoker: Arc<dyn ExternalRuntimeInvoker>,
        provider_executor: Option<Arc<dyn NodeExecutor>>,
    ) -> Result<Self, String> {
        if config.mode == ExternalRuntimeMode::Live && provider_executor.is_none() {
            return Err("live LangGraph runtime requires a Rust provider executor".to_string());
        }
        Ok(Self {
            store,
            config,
            invoker,
            provider_executor,
        })
    }

    fn failure(&self, started: &Instant, code: &str, message: &str) -> NodeExecutionOutput {
        NodeExecutionOutput {
            status: "failed".to_string(),
            executor_type: LANGGRAPH_EXECUTOR_TYPE.to_string(),
            output: None,
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

    // Both variants intentionally carry the same complete typed executor
    // receipt so the caller cannot lose usage/error fields on failure.
    #[expect(
        clippy::result_large_err,
        reason = "failure is the authoritative typed node receipt, not a secondary error DTO"
    )]
    fn execute_inner(
        &self,
        input: &NodeExecutionInput,
        started: &Instant,
    ) -> Result<NodeExecutionOutput, NodeExecutionOutput> {
        if input.task_type != LANGGRAPH_TASK_TYPE {
            return Err(self.failure(
                started,
                "external_runtime_task_mismatch",
                "LangGraph executor requires task_type langgraph_external",
            ));
        }
        if std::env::var(KILL_SWITCH_ENV).as_deref() == Ok("1") {
            return Err(self.failure(
                started,
                "external_runtime_killed",
                "LangGraph runtime kill switch is active",
            ));
        }
        let metadata = input.node_metadata.get("external_runtime").ok_or_else(|| {
            self.failure(
                started,
                "external_runtime_metadata_invalid",
                "missing external_runtime node metadata",
            )
        })?;
        if metadata.get("schema_version").and_then(Value::as_str)
            != Some(EXTERNAL_RUNTIME_NODE_SCHEMA_VERSION)
        {
            return Err(self.failure(
                started,
                "external_runtime_metadata_invalid",
                "external_runtime metadata schema_version is invalid",
            ));
        }
        if metadata.get("runtime_kind").and_then(Value::as_str) != Some("langgraph") {
            return Err(self.failure(
                started,
                "external_runtime_metadata_invalid",
                "runtime_kind must be langgraph",
            ));
        }
        let mode = metadata
            .get("mode")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                self.failure(
                    started,
                    "external_runtime_metadata_invalid",
                    "mode is required",
                )
            })?;
        if mode != self.config.mode.as_str() {
            return Err(self.failure(
                started,
                "external_runtime_mode_mismatch",
                "node mode does not match configured runtime mode",
            ));
        }
        let strategy = metadata
            .get("memory_strategy")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                self.failure(
                    started,
                    "external_runtime_metadata_invalid",
                    "memory_strategy is required",
                )
            })?;
        validate_memory_strategy(strategy)
            .map_err(|error| self.failure(started, "external_runtime_metadata_invalid", &error))?;
        let thread_id = metadata
            .get("thread_id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                self.failure(
                    started,
                    "external_runtime_metadata_invalid",
                    "thread_id is required",
                )
            })?;
        let benchmark = metadata.get("benchmark").cloned().ok_or_else(|| {
            self.failure(
                started,
                "external_runtime_metadata_invalid",
                "benchmark definition is required",
            )
        })?;
        let benchmark_json = canonical_event_json(&benchmark).map_err(|error| {
            self.failure(
                started,
                "external_runtime_metadata_invalid",
                &error.to_string(),
            )
        })?;
        if benchmark_json.len() > MAX_BENCHMARK_BYTES {
            return Err(self.failure(
                started,
                "external_runtime_metadata_oversized",
                "benchmark definition exceeds bounded cap",
            ));
        }
        let benchmark_sha256 = sha256(&benchmark_json);
        let declared_benchmark_sha256 = metadata
            .get("benchmark_sha256")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                self.failure(
                    started,
                    "external_runtime_metadata_invalid",
                    "benchmark_sha256 is required",
                )
            })?;
        if declared_benchmark_sha256 != benchmark_sha256 {
            return Err(self.failure(
                started,
                "external_runtime_metadata_invalid",
                "benchmark_sha256 does not match canonical definition",
            ));
        }
        let scope = self
            .store
            .external_runtime_scope_for_node(&input.run_id, &input.node_id, thread_id)
            .map_err(|error| self.failure(started, "external_runtime_scope_invalid", &error))?;
        let scope_binding = json!({
            "tenant_id":scope.tenant_id,
            "workspace_id":scope.workspace_id,
            "run_id":scope.run_id,
            "workflow_id":input.workflow_id,
            "node_id":scope.node_id,
            "thread_id":scope.thread_id,
        });
        let scope_binding_sha256 =
            sha256(&canonical_event_json(&scope_binding).map_err(|error| {
                self.failure(
                    started,
                    "external_runtime_scope_invalid",
                    &error.to_string(),
                )
            })?);
        let attempt = input
            .node_metadata
            .get("execution_attempt")
            .and_then(Value::as_i64)
            .unwrap_or(1)
            .max(1);
        let checkpoint = self
            .store
            .external_runtime_checkpoint(&scope)
            .map_err(|error| self.failure(started, "external_runtime_checkpoint_read", &error))?;
        let runtime_identity = json!({
            "runtime_kind":"langgraph",
            "adapter_contract_version":EXTERNAL_RUNTIME_ADAPTER_CONTRACT_VERSION,
            "adapter_version":self.config.adapter_version,
            "expected_langgraph_version":self.config.expected_langgraph_version,
        });
        let checkpoint_request = adapter_checkpoint(checkpoint.as_ref());
        let request_identity = json!({
            "schema_version":EXTERNAL_RUNTIME_REQUEST_SCHEMA_VERSION,
            "tenant_id":scope.tenant_id,
            "workspace_id":scope.workspace_id,
            "run_id":scope.run_id,
            "workflow_id":input.workflow_id,
            "node_id":scope.node_id,
            "thread_id":scope.thread_id,
            "attempt":attempt,
            "mode":mode,
            "memory_strategy":strategy,
            "runtime":runtime_identity,
            "scope_binding_sha256":scope_binding_sha256,
            "checkpoint":checkpoint_request,
            "benchmark":benchmark,
        });
        let mut idempotency_identity = request_identity.clone();
        idempotency_identity
            .as_object_mut()
            .expect("request identity object")
            .remove("checkpoint");
        let idempotency_sha256 = sha256(&canonical_event_json(&idempotency_identity).map_err(
            |error| {
                self.failure(
                    started,
                    "external_runtime_idempotency_invalid",
                    &error.to_string(),
                )
            },
        )?);
        let lease_token = format!("lglease-{}", uuid::Uuid::new_v4().simple());
        let claim = self
            .store
            .claim_external_runtime_invocation(
                &scope,
                &idempotency_sha256,
                &lease_token,
                (self.config.timeout_ms as i64 / 1000).max(1),
                "langgraph_external",
            )
            .map_err(|error| self.failure(started, "external_runtime_claim_failed", &error))?;
        let (invocation_id, claimed_checkpoint, resumed) = match claim {
            ExternalRuntimeInvocationClaim::Completed { result_summary, .. } => {
                return Ok(cached_output(result_summary, started));
            }
            ExternalRuntimeInvocationClaim::Busy { .. } => {
                return Err(self.failure(
                    started,
                    "external_runtime_busy",
                    "an exact external runtime invocation is already active",
                ))
            }
            ExternalRuntimeInvocationClaim::Blocked { failure_code, .. } => return Err(self
                .failure(
                started,
                &failure_code,
                "external runtime invocation is blocked and requires explicit operator recovery",
            )),
            ExternalRuntimeInvocationClaim::Claimed {
                invocation_id,
                checkpoint,
                resumed,
                ..
            } => (invocation_id, checkpoint, resumed),
        };
        let claimed_checkpoint_request = adapter_checkpoint(claimed_checkpoint.as_ref());
        let mut bound_request_identity = request_identity;
        bound_request_identity["checkpoint"] = claimed_checkpoint_request.clone();
        let request_sha256 = sha256(&canonical_event_json(&bound_request_identity).map_err(
            |error| {
                self.failure(
                    started,
                    "external_runtime_request_invalid",
                    &error.to_string(),
                )
            },
        )?);
        let mut provider_usage = Value::Null;
        let provider_exchange = if self.config.mode == ExternalRuntimeMode::Live {
            let provider_id = self
                .config
                .provider_id
                .as_deref()
                .expect("validated live provider identity");
            let model_id = self
                .config
                .model_id
                .as_deref()
                .expect("validated live model identity");
            if benchmark.get("provider_id").and_then(Value::as_str) != Some(provider_id)
                || benchmark.get("model_id").and_then(Value::as_str) != Some(model_id)
            {
                let _ = self.store.fail_external_runtime_invocation(
                    &scope,
                    &invocation_id,
                    &lease_token,
                    "external_runtime_provider_identity_mismatch",
                    false,
                    "langgraph_external",
                );
                return Err(self.failure(
                    started,
                    "external_runtime_provider_identity_mismatch",
                    "benchmark provider/model identity does not match configured provider",
                ));
            }
            let executor = self
                .provider_executor
                .as_ref()
                .expect("validated live provider executor");
            let prompt = canonical_provider_prompt(&benchmark, strategy, self.config.token_cap);
            let mut provider_input = input.clone();
            if let Some(object) = provider_input.node_metadata.as_object_mut() {
                object.insert("prompt".to_string(), json!(prompt));
                object.insert(
                    "reserved_cost_usd".to_string(),
                    json!(self.config.per_call_cost_cap_usd),
                );
            }
            let result = executor.execute_node(&provider_input);
            if result.status != "completed" {
                let code = result
                    .error_domain
                    .as_deref()
                    .unwrap_or("external_runtime_provider_failed");
                let _ = self.store.fail_external_runtime_invocation(
                    &scope,
                    &invocation_id,
                    &lease_token,
                    code,
                    false,
                    "langgraph_external",
                );
                return Err(self.failure(
                    started,
                    code,
                    result
                        .error_message
                        .as_deref()
                        .unwrap_or("provider execution failed"),
                ));
            }
            let response = result.output.as_deref().ok_or_else(|| {
                let _ = self.store.fail_external_runtime_invocation(
                    &scope,
                    &invocation_id,
                    &lease_token,
                    "provider_outcome_unknown",
                    true,
                    "langgraph_external",
                );
                self.failure(
                    started,
                    "provider_outcome_unknown",
                    "provider returned no bounded typed result",
                )
            })?;
            let typed_result = parse_provider_typed_result(response).map_err(|error| {
                let _ = self.store.fail_external_runtime_invocation(
                    &scope,
                    &invocation_id,
                    &lease_token,
                    "provider_outcome_unknown",
                    true,
                    "langgraph_external",
                );
                self.failure(started, "provider_outcome_unknown", &error)
            })?;
            let response_sha256 = sha256(response);
            provider_usage = json!({
                "input_tokens":result.input_tokens,
                "output_tokens":result.output_tokens,
                "cached_input_tokens":Value::Null,
                "cache_write_tokens":Value::Null,
                "reasoning_tokens":Value::Null,
                "estimated_cost_usd":result.estimated_cost,
                "provider_reported_cost_usd":Value::Null,
                "latency_ms":result.latency_ms,
                "retry_count":Value::Null,
            });
            let metric_provenance = json!({
                "input_tokens":provenance_for(result.input_tokens,"provider_reported"),
                "output_tokens":provenance_for(result.output_tokens,"provider_reported"),
                "cached_input_tokens":"unavailable",
                "cache_write_tokens":"unavailable",
                "reasoning_tokens":"unavailable",
                "estimated_cost_usd":provenance_for(result.estimated_cost,"harness_derived"),
                "provider_reported_cost_usd":"unavailable",
                "latency_ms":provenance_for(result.latency_ms,"harness_derived"),
                "retry_count":"unavailable",
            });
            json!({
                "exchange_id":format!("lgx-{}",&sha256(&format!("{invocation_id}\0{response_sha256}"))[..32]),
                "invocation_id":invocation_id,
                "scope_binding_sha256":scope_binding_sha256,
                "provider_id":provider_id,
                "model_id":model_id,
                "response_sha256":response_sha256,
                "typed_result":typed_result,
                "usage":provider_usage,
                "metric_provenance":metric_provenance,
            })
        } else {
            Value::Null
        };
        let request = json!({
            "schema_version":EXTERNAL_RUNTIME_REQUEST_SCHEMA_VERSION,
            "invocation_id":invocation_id,
            "tenant_id":scope.tenant_id,
            "workspace_id":scope.workspace_id,
            "run_id":scope.run_id,
            "workflow_id":input.workflow_id,
            "node_id":scope.node_id,
            "thread_id":scope.thread_id,
            "attempt":attempt,
            "mode":mode,
            "memory_strategy":strategy,
            "runtime":runtime_identity,
            "scope_binding_sha256":scope_binding_sha256,
            "request_sha256":request_sha256,
            "checkpoint":claimed_checkpoint_request,
            "provider_exchange":provider_exchange,
            "benchmark":benchmark,
        });
        let result = match self.invoker.invoke(&request, self.config.timeout_ms) {
            Ok(result) => result,
            Err(error) => {
                let blocked = self.config.mode == ExternalRuntimeMode::Live;
                let code = if blocked {
                    "provider_outcome_unknown"
                } else {
                    error.code.as_str()
                };
                let _ = self.store.fail_external_runtime_invocation(
                    &scope,
                    &invocation_id,
                    &lease_token,
                    code,
                    blocked,
                    "langgraph_external",
                );
                return Err(self.failure(started, code, &error.message));
            }
        };
        validate_result_binding(&result, &request, &self.config).map_err(|error| {
            let blocked = self.config.mode == ExternalRuntimeMode::Live;
            let code = if blocked {
                "provider_outcome_unknown"
            } else {
                "external_runtime_result_invalid"
            };
            let _ = self.store.fail_external_runtime_invocation(
                &scope,
                &invocation_id,
                &lease_token,
                code,
                blocked,
                "langgraph_external",
            );
            self.failure(started, code, &error)
        })?;
        let scorecard = result
            .get("scorecard_summary")
            .expect("validated scorecard_summary");
        let artifact = match self.store.record_external_runtime_scorecard(
            scorecard,
            &invocation_id,
            "langgraph_external",
        ) {
            Ok(artifact) => artifact,
            Err(error) => {
                let blocked = self.config.mode == ExternalRuntimeMode::Live;
                let code = if blocked {
                    "provider_outcome_unknown"
                } else {
                    "external_runtime_scorecard_invalid"
                };
                let _ = self.store.fail_external_runtime_invocation(
                    &scope,
                    &invocation_id,
                    &lease_token,
                    code,
                    blocked,
                    "langgraph_external",
                );
                return Err(self.failure(started, code, &error));
            }
        };
        let artifact_id = artifact
            .get("artifact_id")
            .and_then(Value::as_str)
            .expect("stored external runtime artifact id");
        let checkpoint_next = result
            .get("checkpoint_next")
            .expect("validated checkpoint_next");
        let persisted_checkpoint_id = checkpoint_next
            .get("checkpoint_id")
            .and_then(Value::as_str)
            .expect("validated checkpoint id");
        let result_summary = json!({
            "schema_version":EXTERNAL_RUNTIME_RESULT_SCHEMA_VERSION,
            "invocation_id":invocation_id,
            "checkpoint_id":persisted_checkpoint_id,
            "artifact_id":artifact_id,
            "memory_strategy":strategy,
            "runtime_kind":"langgraph",
            "runtime_version":self.config.expected_langgraph_version,
            "adapter_version":self.config.adapter_version,
            "resumed":resumed,
            "provider_usage":provider_usage,
            "input_tokens":scorecard.get("input_tokens"),
            "output_tokens":scorecard.get("output_tokens"),
            "estimated_cost_usd":scorecard.get("estimated_cost_usd"),
            "trace_summary":result.get("trace_summary"),
            "raw_content_persisted":false,
        });
        if let Err(error) = self.store.complete_external_runtime_invocation(
            &scope,
            &invocation_id,
            &lease_token,
            &self.config.adapter_version,
            &self.config.expected_langgraph_version,
            strategy,
            checkpoint_next,
            &result_summary,
            artifact_id,
            "langgraph_external",
        ) {
            let blocked = self.config.mode == ExternalRuntimeMode::Live;
            let code = if blocked {
                "provider_outcome_unknown"
            } else {
                "external_runtime_completion_failed"
            };
            let _ = self.store.fail_external_runtime_invocation(
                &scope,
                &invocation_id,
                &lease_token,
                code,
                blocked,
                "langgraph_external",
            );
            return Err(self.failure(started, code, &error));
        }
        Ok(output_from_summary(result_summary, scorecard, started))
    }
}

impl NodeExecutor for ExternalRuntimeNodeExecutor {
    fn executor_type_name(&self) -> &str {
        LANGGRAPH_EXECUTOR_TYPE
    }

    fn execute_node(&self, input: &NodeExecutionInput) -> NodeExecutionOutput {
        let started = Instant::now();
        match self.execute_inner(input, &started) {
            Ok(output) | Err(output) => output,
        }
    }
}

fn adapter_checkpoint(checkpoint: Option<&Value>) -> Value {
    checkpoint
        .and_then(|value| value.get("checkpoint_summary"))
        .cloned()
        .unwrap_or(Value::Null)
}

fn validate_result_binding(
    result: &Value,
    request: &Value,
    config: &ExternalRuntimeConfig,
) -> Result<(), String> {
    if result.get("schema_version").and_then(Value::as_str)
        != Some(EXTERNAL_RUNTIME_RESULT_SCHEMA_VERSION)
    {
        return Err("adapter result schema_version is invalid".into());
    }
    for key in [
        "invocation_id",
        "tenant_id",
        "workspace_id",
        "run_id",
        "workflow_id",
        "node_id",
        "thread_id",
        "scope_binding_sha256",
        "request_sha256",
        "memory_strategy",
    ] {
        if result.get(key) != request.get(key) {
            return Err(format!("adapter result {key} binding changed"));
        }
    }
    if result.get("invocation_count").and_then(Value::as_i64) != Some(1) {
        return Err("adapter must report exactly one graph invocation".into());
    }
    for key in ["checkpoint_next", "trace_summary", "scorecard_summary"] {
        if !result.get(key).is_some_and(Value::is_object) {
            return Err(format!("adapter result {key} must be an object"));
        }
    }
    if !result
        .pointer("/checkpoint_next/state_summary")
        .is_some_and(Value::is_object)
    {
        return Err("adapter checkpoint state_summary must be an object".into());
    }
    let checkpoint_id = result
        .pointer("/checkpoint_next/checkpoint_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "adapter checkpoint id is required".to_string())?;
    validate_wire_identifier("checkpoint_id", checkpoint_id)?;
    if result
        .pointer("/checkpoint_next/version")
        .and_then(Value::as_i64)
        .filter(|version| *version > 0)
        .is_none()
    {
        return Err("adapter checkpoint version must be positive".into());
    }
    let state_sha256 = result
        .pointer("/checkpoint_next/state_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| "adapter checkpoint state hash is required".to_string())?;
    validate_sha256_value("checkpoint state_sha256", state_sha256)?;
    let state_summary = result
        .pointer("/checkpoint_next/state_summary")
        .expect("validated checkpoint state summary");
    if sha256(&canonical_event_json(state_summary).map_err(|error| error.to_string())?)
        != state_sha256
    {
        return Err("adapter checkpoint state hash changed".into());
    }
    if result
        .pointer("/runtime/runtime_kind")
        .and_then(Value::as_str)
        != Some("langgraph")
        || result
            .pointer("/runtime/adapter_version")
            .and_then(Value::as_str)
            != Some(config.adapter_version.as_str())
        || result
            .pointer("/runtime/runtime_version")
            .and_then(Value::as_str)
            != Some(config.expected_langgraph_version.as_str())
    {
        return Err("adapter runtime identity changed".into());
    }
    let encoded = canonical_event_json(result).map_err(|error| error.to_string())?;
    if encoded.len() > MAX_PROCESS_OUTPUT_BYTES {
        return Err("adapter result exceeds bounded cap".into());
    }
    Ok(())
}

fn output_from_summary(
    summary: Value,
    scorecard: &Value,
    started: &Instant,
) -> NodeExecutionOutput {
    NodeExecutionOutput {
        status: "completed".into(),
        executor_type: LANGGRAPH_EXECUTOR_TYPE.into(),
        output: Some(summary.to_string()),
        error_domain: None,
        error_message: None,
        input_tokens: scorecard.get("input_tokens").and_then(Value::as_i64),
        output_tokens: scorecard.get("output_tokens").and_then(Value::as_i64),
        estimated_cost: scorecard.get("estimated_cost_usd").and_then(Value::as_f64),
        latency_ms: Some(started.elapsed().as_millis() as i64),
        process_outcome: None,
        resolved_model: None,
    }
}

fn cached_output(summary: Value, started: &Instant) -> NodeExecutionOutput {
    let usage = summary.get("provider_usage");
    NodeExecutionOutput {
        status: "completed".into(),
        executor_type: LANGGRAPH_EXECUTOR_TYPE.into(),
        output: Some(summary.to_string()),
        error_domain: None,
        error_message: None,
        input_tokens: summary
            .get("input_tokens")
            .and_then(Value::as_i64)
            .or_else(|| {
                usage
                    .and_then(|value| value.get("input_tokens"))
                    .and_then(Value::as_i64)
            }),
        output_tokens: summary
            .get("output_tokens")
            .and_then(Value::as_i64)
            .or_else(|| {
                usage
                    .and_then(|value| value.get("output_tokens"))
                    .and_then(Value::as_i64)
            }),
        estimated_cost: summary
            .get("estimated_cost_usd")
            .and_then(Value::as_f64)
            .or_else(|| {
                usage
                    .and_then(|value| value.get("estimated_cost_usd"))
                    .and_then(Value::as_f64)
            }),
        latency_ms: Some(started.elapsed().as_millis() as i64),
        process_outcome: None,
        resolved_model: None,
    }
}

fn canonical_provider_prompt(benchmark: &Value, strategy: &str, token_cap: i64) -> String {
    let required = benchmark
        .get("required_reference_ids")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let selected = benchmark
        .get("selected_reference_ids")
        .cloned()
        .unwrap_or_else(|| json!([]));
    format!(
        "Evaluate one bounded memory benchmark. strategy={strategy}; required_reference_ids={required}; selected_reference_ids={selected}. Return JSON only with exactly these fields: status ('pass' or 'fail'), decision_code (bounded identifier), selected_tool_ids (bounded identifier array), quality_score (0..1 or null), quality_method (bounded identifier). Do not include prompts, outputs, transcripts, paths, credentials, or extra fields. Output token cap={token_cap}."
    )
}

fn parse_provider_typed_result(response: &str) -> Result<Value, String> {
    if response.len() > MAX_PROCESS_OUTPUT_BYTES || contains_sensitive_patterns(response) {
        return Err("provider typed result is oversized or secret-shaped".into());
    }
    let value: Value = serde_json::from_str(response)
        .map_err(|_| "provider response is not one typed JSON object".to_string())?;
    let object = value
        .as_object()
        .ok_or_else(|| "provider typed result must be an object".to_string())?;
    let expected = [
        "decision_code",
        "quality_method",
        "quality_score",
        "selected_tool_ids",
        "status",
    ];
    let mut keys = object.keys().map(String::as_str).collect::<Vec<_>>();
    keys.sort_unstable();
    if keys != expected {
        return Err("provider typed result contains missing or unknown fields".into());
    }
    if !matches!(
        object.get("status").and_then(Value::as_str),
        Some("pass" | "fail")
    ) {
        return Err("provider typed result status is invalid".into());
    }
    for key in ["decision_code", "quality_method"] {
        validate_wire_identifier(
            key,
            object
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| format!("provider typed result {key} is required"))?,
        )?;
    }
    let selected = object
        .get("selected_tool_ids")
        .and_then(Value::as_array)
        .ok_or_else(|| "provider selected_tool_ids must be an array".to_string())?;
    if selected.len() > 64 {
        return Err("provider selected_tool_ids exceeds 64 entries".into());
    }
    let mut seen = std::collections::HashSet::new();
    for value in selected {
        let id = value
            .as_str()
            .ok_or_else(|| "provider selected_tool_ids must contain identifiers".to_string())?;
        validate_wire_identifier("selected_tool_id", id)?;
        if !seen.insert(id) {
            return Err("provider selected_tool_ids contains duplicates".into());
        }
    }
    if let Some(score) = object.get("quality_score").filter(|value| !value.is_null()) {
        let score = score
            .as_f64()
            .filter(|score| score.is_finite() && (0.0..=1.0).contains(score))
            .ok_or_else(|| "provider quality_score must be null or between 0 and 1".to_string())?;
        let _ = score;
    }
    Ok(value)
}

fn provenance_for<T>(value: Option<T>, available: &'static str) -> &'static str {
    if value.is_some() {
        available
    } else {
        "unavailable"
    }
}

fn validate_wire_identifier(field: &str, value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().enumerate().all(|(index, byte)| {
            (index > 0 || byte.is_ascii_alphanumeric())
                && (byte.is_ascii_alphanumeric()
                    || matches!(byte, b'-' | b'_' | b'.' | b':' | b'@'))
        })
    {
        return Err(format!("{field} must be a bounded wire identifier"));
    }
    Ok(())
}

fn validate_sha256_value(field: &str, value: &str) -> Result<(), String> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(format!("{field} must be a lowercase SHA-256"))
    }
}

fn sha256(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}
fn required_env(name: &str) -> Result<String, String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("{name} is required"))
}
fn required_positive_f64(name: &str) -> Result<f64, String> {
    required_env(name)?
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0)
        .ok_or_else(|| format!("{name} must be finite and positive"))
}
fn required_u64(name: &str, min: u64, max: u64) -> Result<u64, String> {
    required_env(name)?
        .parse::<u64>()
        .ok()
        .filter(|v| (*v >= min) && (*v <= max))
        .ok_or_else(|| format!("{name} must be between {min} and {max}"))
}
fn required_i64(name: &str, min: i64, max: i64) -> Result<i64, String> {
    required_env(name)?
        .parse::<i64>()
        .ok()
        .filter(|v| (*v >= min) && (*v <= max))
        .ok_or_else(|| format!("{name} must be between {min} and {max}"))
}
fn canonical_regular_file(value: &str, name: &str) -> Result<PathBuf, String> {
    let path = Path::new(value);
    if !path.is_absolute() {
        return Err(format!("{name} must be an absolute path"));
    }
    let path =
        std::fs::canonicalize(path).map_err(|error| format!("{name} is invalid: {error}"))?;
    if !path.is_file() {
        return Err(format!("{name} must identify a regular file"));
    }
    Ok(path)
}
fn canonical_executable_path(value: &str, name: &str) -> Result<PathBuf, String> {
    canonical_regular_file(value, name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Map;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::tempdir;

    struct FixtureInvoker {
        calls: AtomicUsize,
    }

    impl FixtureInvoker {
        fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
            }
        }
    }

    impl ExternalRuntimeInvoker for FixtureInvoker {
        fn invoke(&self, request: &Value, _timeout_ms: u64) -> Result<Value, ExternalRuntimeError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let mut request_material = request.as_object().cloned().expect("request object");
            request_material.remove("invocation_id");
            request_material.remove("request_sha256");
            request_material.remove("provider_exchange");
            let expected_request_sha = sha256(
                &canonical_event_json(&Value::Object(request_material)).expect("canonical request"),
            );
            assert_eq!(
                request.get("request_sha256").and_then(Value::as_str),
                Some(expected_request_sha.as_str())
            );
            assert!(request.get("provider_exchange").is_some_and(Value::is_null));

            let previous = request.get("checkpoint").filter(|value| !value.is_null());
            let version = previous
                .and_then(|value| value.get("version"))
                .and_then(Value::as_i64)
                .unwrap_or(0)
                + 1;
            let event_hash = sha256(&format!(
                "{}:{version}",
                request
                    .get("request_sha256")
                    .and_then(Value::as_str)
                    .unwrap()
            ));
            let state_summary = json!({
                "memory_digest":event_hash,
                "summary_digest":event_hash,
                "fact_ids":["benchmark-ref-current"],
                "selected_reference_ids":["benchmark-ref-current"],
                "recent_event_hashes":[event_hash],
                "turn_count":version,
                "conflict_count":0,
                "correction_count":0,
            });
            let state_sha256 =
                sha256(&canonical_event_json(&state_summary).expect("canonical checkpoint state"));
            let checkpoint_identity = json!({
                "scope_binding_sha256":request.get("scope_binding_sha256"),
                "node_id":request.get("node_id"),
                "thread_id":request.get("thread_id"),
                "version":version,
                "state_sha256":state_sha256,
            });
            let checkpoint_id = format!(
                "ckpt-{}",
                sha256(&canonical_event_json(&checkpoint_identity).unwrap())
            );
            let checkpoint_next = json!({
                "checkpoint_id":checkpoint_id,
                "version":version,
                "parent_checkpoint_id":previous.and_then(|value| value.get("checkpoint_id")).cloned(),
                "state_summary":state_summary,
                "state_sha256":state_sha256,
            });
            let benchmark = request.get("benchmark").unwrap();
            let scorecard = json!({
                "schema_version":"external_runtime_scorecard_summary.v1",
                "runtime_kind":"langgraph",
                "runtime_version":PINNED_LANGGRAPH_VERSION,
                "adapter_version":LANGGRAPH_ADAPTER_VERSION,
                "definition_sha256":benchmark.get("definition_sha256"),
                "scenario_id":benchmark.get("scenario_id"),
                "scenario_sha256":benchmark.get("scenario_sha256"),
                "task_sha256":benchmark.get("task_sha256"),
                "memory_strategy":request.get("memory_strategy"),
                "mode":"fixture",
                "provider_id":benchmark.get("provider_id"),
                "model_id":benchmark.get("model_id"),
                "tokenizer_id":benchmark.get("tokenizer_id"),
                "pricing_id":benchmark.get("pricing_id"),
                "seed":165,
                "status":"pass",
                "quality_score":1.0,
                "quality_method":"deterministic_fixture.v1",
                "quality_threshold":0.9,
                "input_tokens":640,
                "output_tokens":80,
                "cached_input_tokens":Value::Null,
                "cache_write_tokens":Value::Null,
                "reasoning_tokens":Value::Null,
                "estimated_cost_usd":0.00144,
                "provider_reported_cost_usd":Value::Null,
                "context_tokens":700,
                "repeated_context_tokens":100,
                "retrieval_candidate_count":2,
                "retrieval_selected_count":1,
                "retrieval_precision":1.0,
                "retrieval_recall":1.0,
                "stale_memory_selection_rate":0.0,
                "correction_count":0,
                "conflict_count":0,
                "state_read_bytes":128,
                "state_write_bytes":64,
                "memory_maintenance_tokens":20,
                "memory_maintenance_cost_usd":0.00002,
                "tool_call_count":2,
                "redundant_tool_call_count":0,
                "selected_tool_count":0,
                "retry_count":0,
                "latency_ms":1,
                "restart_resumed":previous.is_some(),
                "metric_provenance":{
                    "input_tokens":"harness_derived",
                    "output_tokens":"harness_derived",
                    "estimated_cost_usd":"harness_derived"
                },
                "measurement_completeness":1.0,
                "measurement_confidence":"high",
            });
            Ok(json!({
                "schema_version":EXTERNAL_RUNTIME_RESULT_SCHEMA_VERSION,
                "invocation_id":request.get("invocation_id"),
                "tenant_id":request.get("tenant_id"),
                "workspace_id":request.get("workspace_id"),
                "run_id":request.get("run_id"),
                "workflow_id":request.get("workflow_id"),
                "node_id":request.get("node_id"),
                "thread_id":request.get("thread_id"),
                "memory_strategy":request.get("memory_strategy"),
                "scope_binding_sha256":request.get("scope_binding_sha256"),
                "request_sha256":request.get("request_sha256"),
                "invocation_count":1,
                "runtime":{
                    "runtime_kind":"langgraph",
                    "runtime_version":PINNED_LANGGRAPH_VERSION,
                    "adapter_contract_version":EXTERNAL_RUNTIME_ADAPTER_CONTRACT_VERSION,
                    "adapter_version":LANGGRAPH_ADAPTER_VERSION,
                },
                "checkpoint_next":checkpoint_next,
                "trace_summary":{
                    "schema_version":"external_runtime_trace_summary.v1",
                    "summary_level":true,
                    "graph_invoke_count":1,
                },
                "scorecard_summary":scorecard,
            }))
        }
    }

    fn benchmark() -> Value {
        json!({
            "definition_sha256":"1".repeat(64),
            "scenario_id":"canonical-memory-benchmark",
            "scenario_sha256":"2".repeat(64),
            "task_sha256":"3".repeat(64),
            "seed":165,
            "quality_threshold":0.9,
            "provider_id":"offline-fixture",
            "model_id":"deterministic-provider-v1",
            "tokenizer_id":"fixture-tokenizer-v1",
            "pricing_id":"fixture-pricing-v1",
            "required_reference_ids":["benchmark-ref-current"],
            "candidate_reference_ids":["benchmark-ref-current","benchmark-ref-unused"],
            "selected_reference_ids":["benchmark-ref-current"],
            "stale_reference_ids":[],
            "context_tokens":700,
            "repeated_context_tokens":100,
            "state_read_bytes":128,
            "state_write_bytes":64,
            "memory_maintenance_tokens":20,
            "memory_maintenance_cost_usd":0.00002,
            "tool_call_count":2,
            "redundant_tool_call_count":0,
        })
    }

    fn create_run(store: &LocalProductStore) -> (String, String) {
        let benchmark = benchmark();
        let benchmark_sha256 = sha256(&canonical_event_json(&benchmark).unwrap());
        let plan = store
            .create_workflow_plan("bounded fixture", "test", "test", |ids, _| {
                Ok(json!({
                    "schema_version":"read_only_plan.v1",
                    "plan_id":ids.plan_id,
                    "status":"planned_read_only",
                    "workflow_id":ids.workflow_id,
                    "dispatch_id":ids.dispatch_id,
                    "analysis":{"analysis_id":"analysis-external","task_domain":"benchmark"},
                    "graph":{
                        "schema_version":"workflow_graph.v1",
                        "workflow_id":ids.workflow_id,
                        "dispatch_id":ids.dispatch_id,
                        "status":"decomposed",
                        "nodes":[{
                            "schema_version":"workflow_node.v1",
                            "node_id":"node-langgraph",
                            "workflow_id":ids.workflow_id,
                            "task_type":LANGGRAPH_TASK_TYPE,
                            "executor":LANGGRAPH_EXECUTOR_TYPE,
                            "status":"pending",
                            "input_refs":[],
                            "external_runtime":{
                                "schema_version":EXTERNAL_RUNTIME_NODE_SCHEMA_VERSION,
                                "runtime_kind":"langgraph",
                                "mode":"fixture",
                                "memory_strategy":"durable_state_bounded_recent",
                                "thread_id":"thread-1",
                                "benchmark":benchmark,
                                "benchmark_sha256":benchmark_sha256,
                            }
                        }],
                        "edges":[]
                    },
                    "boundaries":{
                        "execution_authority":"bounded_local",
                        "target_repository_writes":"disabled",
                        "runtime_workers":"managed_external_runtime"
                    }
                }))
            })
            .unwrap();
        let run = store
            .create_workflow_run_from_plan_scoped(
                plan.get("plan_id").and_then(Value::as_str).unwrap(),
                "test",
                "tenant-1",
                "workspace-1",
            )
            .unwrap();
        (
            run.get("run_id")
                .and_then(Value::as_str)
                .unwrap()
                .to_string(),
            run.get("workflow_id")
                .and_then(Value::as_str)
                .unwrap()
                .to_string(),
        )
    }

    fn input(
        store: &LocalProductStore,
        run_id: &str,
        workflow_id: &str,
        attempt: i64,
    ) -> NodeExecutionInput {
        let run = store.get_workflow_run(run_id).unwrap().unwrap();
        let mut metadata = run
            .get("nodes")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .find(|node| node.get("node_id").and_then(Value::as_str) == Some("node-langgraph"))
            .cloned()
            .unwrap();
        metadata
            .as_object_mut()
            .unwrap_or(&mut Map::new())
            .insert("execution_attempt".to_string(), json!(attempt));
        NodeExecutionInput {
            node_id: "node-langgraph".into(),
            task_type: LANGGRAPH_TASK_TYPE.into(),
            run_id: run_id.into(),
            workflow_id: workflow_id.into(),
            node_metadata: metadata,
        }
    }

    #[test]
    fn fixture_execution_is_idempotent_restart_safe_and_scope_bound() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("external-runtime.db");
        let store = Arc::new(LocalProductStore::new(&path).unwrap());
        let (run_id, workflow_id) = create_run(&store);
        let invoker = Arc::new(FixtureInvoker::new());
        let config =
            ExternalRuntimeConfig::fixture(PathBuf::from("/python"), PathBuf::from("/adapter"));
        let executor =
            ExternalRuntimeNodeExecutor::new(store.clone(), config.clone(), invoker.clone(), None)
                .unwrap();
        let first = executor.execute_node(&input(&store, &run_id, &workflow_id, 1));
        assert_eq!(first.status, "completed", "first output: {first:?}");
        let duplicate = executor.execute_node(&input(&store, &run_id, &workflow_id, 1));
        assert_eq!(duplicate.status, "completed");
        assert_eq!(invoker.calls.load(Ordering::SeqCst), 1);

        let scope = store
            .external_runtime_scope_for_node(&run_id, "node-langgraph", "thread-1")
            .unwrap();
        let checkpoint = store.external_runtime_checkpoint(&scope).unwrap().unwrap();
        assert_eq!(checkpoint.get("version").and_then(Value::as_i64), Some(1));
        let mut cross_scope = scope.clone();
        cross_scope.workspace_id = "workspace-other".into();
        assert!(store
            .external_runtime_checkpoint(&cross_scope)
            .unwrap()
            .is_none());
        drop(executor);
        drop(store);

        let reopened = Arc::new(LocalProductStore::new(&path).unwrap());
        let resumed_invoker = Arc::new(FixtureInvoker::new());
        let resumed = ExternalRuntimeNodeExecutor::new(
            reopened.clone(),
            config,
            resumed_invoker.clone(),
            None,
        )
        .unwrap()
        .execute_node(&input(&reopened, &run_id, &workflow_id, 2));
        assert_eq!(resumed.status, "completed");
        assert_eq!(resumed_invoker.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            reopened
                .external_runtime_checkpoint(&scope)
                .unwrap()
                .unwrap()
                .get("version")
                .and_then(Value::as_i64),
            Some(2)
        );
    }
}
