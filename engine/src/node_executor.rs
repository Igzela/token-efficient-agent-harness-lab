use serde_json::{json, Value};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::thread;

use crate::agent_memory::{
    build_memory_context_for_node, consolidate_memory_digest, estimate_memory_state_bytes,
    load_memory_digest_from_agent_state, memory_digest_to_metadata_patch,
};
use crate::orchestration::schemas::{
    AgentState, ChildTaskProposal, DebatePosition, DebateRequest, DebateResolution, HandoffRequest,
    ReviewRequest, ReviewVerdict, MAX_DEBATE_PARTICIPANTS, MAX_DEBATE_ROUNDS,
    MAX_REVIEW_DEBATE_TEXT_BYTES, REVIEW_VERDICTS,
};
use crate::provider::redaction::{contains_sensitive_patterns, redact_sensitive_patterns};
use crate::storage::local_product_store::LocalProductStore;

/// Decision function for agent_step: given context, return the next action.
pub type AgentDecisionFn =
    Box<dyn Fn(&AgentStepContext) -> Result<AgentAction, String> + Send + Sync>;

const MAX_COMMAND_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_PROPOSAL_OBJECTIVE_BYTES: usize = 4096;
const MAX_NOTE_BYTES: usize = 4096;

/// Returns a sanitized action descriptor for audit events.
/// Never includes raw user/note/observation/scratchpad/proposal body.
fn sanitized_action_descriptor(action: &AgentAction) -> Value {
    match action {
        AgentAction::Wait => json!({"action_type": "wait"}),
        AgentAction::Complete => json!({"action_type": "complete"}),
        AgentAction::UpdateScratchpadSummary(text) => json!({
            "action_type": "update_scratchpad",
            "char_count": text.len(),
        }),
        AgentAction::ReadMailbox => json!({"action_type": "read_mailbox"}),
        AgentAction::AckMessage(id) => json!({
            "action_type": "ack_message",
            "message_id": id,
        }),
        AgentAction::EmitNote(text) => json!({
            "action_type": "emit_note",
            "char_count": text.len(),
        }),
        AgentAction::RecordObservation(text) => json!({
            "action_type": "record_observation",
            "char_count": text.len(),
        }),
        AgentAction::ProposeChildTask(p) => json!({
            "action_type": "propose_child_task",
            "correlation_id": p.correlation_id,
            "agent_id": p.agent_id,
            "objective_char_count": p.objective.len(),
        }),
        AgentAction::RequestHandoff(r) => json!({
            "action_type": "request_handoff",
            "correlation_id": r.correlation_id,
            "source_agent_id": r.source_agent_id,
            "target_agent_id": r.target_agent_id,
        }),
        AgentAction::AcceptHandoff(cid) => json!({
            "action_type": "accept_handoff",
            "correlation_id": cid,
        }),
        AgentAction::RejectHandoff(cid) => json!({
            "action_type": "reject_handoff",
            "correlation_id": cid,
        }),
        AgentAction::CancelProposal(cid) => json!({
            "action_type": "cancel_proposal",
            "correlation_id": cid,
        }),
        AgentAction::RequestReview(r) => json!({
            "action_type": "request_review",
            "correlation_id": r.correlation_id,
            "target_agent_id": r.target_agent_id,
            "blocking": r.blocking,
        }),
        AgentAction::SubmitReviewVerdict(v) => json!({
            "action_type": "submit_review_verdict",
            "correlation_id": v.correlation_id,
            "review_request_id": v.review_request_id,
            "verdict": v.verdict,
            "blocking": v.blocking,
        }),
        AgentAction::OpenDebate(d) => json!({
            "action_type": "open_debate",
            "correlation_id": d.correlation_id,
            "participant_count": d.participant_agent_ids.len(),
            "max_rounds": d.max_rounds,
        }),
        AgentAction::SubmitDebatePosition(p) => json!({
            "action_type": "submit_debate_position",
            "correlation_id": p.correlation_id,
            "debate_id": p.debate_id,
            "position_char_count": p.position.len(),
        }),
        AgentAction::ResolveDebate(r) => json!({
            "action_type": "resolve_debate",
            "correlation_id": r.correlation_id,
            "debate_id": r.debate_id,
            "has_winning_position": r.winning_position.is_some(),
        }),
        AgentAction::Unsupported(name) => json!({
            "action_type": "unsupported",
            "name": name,
        }),
    }
}

fn sanitize_text_field(text: &str) -> (String, &'static str) {
    if contains_sensitive_patterns(text) {
        let redacted = redact_sensitive_patterns(text);
        let capped = if redacted.len() > MAX_NOTE_BYTES {
            let mut split = MAX_NOTE_BYTES;
            while split > 0 && !redacted.is_char_boundary(split) {
                split -= 1;
            }
            format!(
                "{} [truncated {} bytes]",
                &redacted[..split],
                redacted.len() - split
            )
        } else {
            redacted
        };
        (capped, "redacted")
    } else if text.len() > MAX_NOTE_BYTES {
        let mut split = MAX_NOTE_BYTES;
        while split > 0 && !text.is_char_boundary(split) {
            split -= 1;
        }
        (
            format!(
                "{} [truncated {} bytes]",
                &text[..split],
                text.len() - split
            ),
            "capped",
        )
    } else {
        (text.to_string(), "none")
    }
}

/// Input for node-level execution within a workflow run.
#[derive(Debug, Clone)]
pub struct NodeExecutionInput {
    pub node_id: String,
    pub task_type: String,
    pub run_id: String,
    pub workflow_id: String,
    pub node_metadata: Value,
}

/// Output from node-level execution.
#[derive(Debug, Clone)]
pub struct NodeExecutionOutput {
    pub status: String,
    pub executor_type: String,
    pub output: Option<String>,
    pub error_domain: Option<String>,
    pub error_message: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub estimated_cost: Option<f64>,
    pub latency_ms: Option<i64>,
}

impl NodeExecutionOutput {
    pub fn to_value(&self) -> Value {
        let mut value = json!({
            "status": self.status,
            "executor_type": self.executor_type,
            "output": self.output,
            "error_domain": self.error_domain,
            "error_message": self.error_message,
            "input_tokens": self.input_tokens,
            "output_tokens": self.output_tokens,
            "estimated_cost": self.estimated_cost,
            "latency_ms": self.latency_ms,
        });
        if matches!(
            self.executor_type.as_str(),
            "provider" | "adaptive_provider" | "claude_code_cli" | "codex_cli"
        ) {
            if let Some(obj) = value.as_object_mut() {
                let (env_gate, auth_scope, cost_gate, kill_path) =
                    if self.executor_type == "adaptive_provider" {
                        (
                            "provider_plus_adaptive",
                            "dispatch_execute_with_configured_auth",
                            "global_plus_plan_plus_per_call",
                            "adaptive_kill_switch_or_provider_timeout",
                        )
                    } else {
                        (
                            "passed",
                            "explicit_local_runtime",
                            if self.estimated_cost.is_some() {
                                "evaluated"
                            } else {
                                "not_applicable"
                            },
                            "workflow_cancel_or_process_timeout",
                        )
                    };
                obj.insert(
                    "trace".to_string(),
                    json!({
                        "schema_version": "execution_trace.v2",
                        "executor_type": self.executor_type,
                        "env_gate": env_gate,
                        "auth_scope": auth_scope,
                        "output_policy": "redacted_and_capped",
                        "cost_gate": cost_gate,
                        "kill_path": kill_path,
                    }),
                );
            }
        }
        value
    }
}

/// Trait for executing individual workflow nodes.
pub trait NodeExecutor: Send + Sync {
    fn execute_node(&self, input: &NodeExecutionInput) -> NodeExecutionOutput;
    fn executor_type_name(&self) -> &str;
}

/// Noop executor that always succeeds immediately.
#[derive(Clone)]
pub struct NoopNodeExecutor;

impl NodeExecutor for NoopNodeExecutor {
    fn executor_type_name(&self) -> &str {
        "noop"
    }
    fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
        NodeExecutionOutput {
            status: "completed".to_string(),
            executor_type: "noop".to_string(),
            output: None,
            error_domain: None,
            error_message: None,
            input_tokens: None,
            output_tokens: None,
            estimated_cost: None,
            latency_ms: None,
        }
    }
}

/// Stub executor that simulates success with a fixed output.
pub struct StubNodeExecutor {
    pub output_template: String,
}

impl Default for StubNodeExecutor {
    fn default() -> Self {
        Self {
            output_template: "stub execution completed for {node_id}".to_string(),
        }
    }
}

impl NodeExecutor for StubNodeExecutor {
    fn executor_type_name(&self) -> &str {
        "stub"
    }
    fn execute_node(&self, input: &NodeExecutionInput) -> NodeExecutionOutput {
        let output = self
            .output_template
            .replace("{node_id}", &input.node_id)
            .replace("{task_type}", &input.task_type)
            .replace("{run_id}", &input.run_id);
        NodeExecutionOutput {
            status: "completed".to_string(),
            executor_type: "stub".to_string(),
            output: Some(output),
            error_domain: None,
            error_message: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            estimated_cost: Some(0.0),
            latency_ms: Some(0),
        }
    }
}

/// Failure executor that always fails (for testing retry logic).
#[derive(Clone)]
pub struct FailNodeExecutor {
    pub error_domain: String,
    pub error_message: String,
}

impl Default for FailNodeExecutor {
    fn default() -> Self {
        Self {
            error_domain: "test_failure".to_string(),
            error_message: "simulated failure".to_string(),
        }
    }
}

impl NodeExecutor for FailNodeExecutor {
    fn executor_type_name(&self) -> &str {
        "fail"
    }
    fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
        NodeExecutionOutput {
            status: "failed".to_string(),
            executor_type: "fail".to_string(),
            output: None,
            error_domain: Some(self.error_domain.clone()),
            error_message: Some(self.error_message.clone()),
            input_tokens: None,
            output_tokens: None,
            estimated_cost: None,
            latency_ms: None,
        }
    }
}

/// Executor that runs the local runner validation deterministically.
///
/// Runs the stateful-vs-stateless experiment with stub provider,
/// validates stateful < stateless tokens, and emits a bounded summary
/// (validation_status, token_totals, run_ids). No raw prompts, outputs,
/// transcripts, or scorecard steps are persisted in the node output.
///
/// After the node completes, the workflow tick path automatically records
/// a native_scorecard_artifact via the existing tick-level automatic
/// scorecard recording path (see workflow_runs.rs tick function).
#[derive(Clone)]
pub struct LocalRunnerValidationExecutor;

impl LocalRunnerValidationExecutor {
    fn run_validation(
        input: &NodeExecutionInput,
    ) -> (NodeExecutionOutput, Option<serde_json::Value>) {
        let iterations = input
            .node_metadata
            .get("iterations")
            .and_then(|v| v.as_u64())
            .unwrap_or(10)
            .clamp(2, 50) as usize;
        let max_calls = input
            .node_metadata
            .get("max_calls")
            .and_then(|v| v.as_u64())
            .unwrap_or((iterations * 2) as u64)
            .clamp(iterations as u64 * 2, 200) as usize;

        let config = match crate::local_runner_provider::build_config(
            crate::local_runner_provider::ProviderKind::Stub,
            iterations,
            max_calls,
            120000,
            30.0,
            0.25,
            1.0,
            0.94,
        ) {
            Ok(c) => c,
            Err(e) => {
                return (
                    NodeExecutionOutput {
                        status: "failed".to_string(),
                        executor_type: "local_runner_validation".to_string(),
                        output: None,
                        error_domain: Some("config_error".to_string()),
                        error_message: Some(format!("config error: {e}")),
                        input_tokens: None,
                        output_tokens: None,
                        estimated_cost: None,
                        latency_ms: None,
                    },
                    None,
                );
            }
        };

        let provider = match crate::local_runner_provider::build_provider(&config, None) {
            Ok(p) => p,
            Err(e) => {
                return (
                    NodeExecutionOutput {
                        status: "failed".to_string(),
                        executor_type: "local_runner_validation".to_string(),
                        output: None,
                        error_domain: Some("provider_error".to_string()),
                        error_message: Some(format!("provider error: {e}")),
                        input_tokens: None,
                        output_tokens: None,
                        estimated_cost: None,
                        latency_ms: None,
                    },
                    None,
                );
            }
        };

        let (stateless, stateful) = match crate::local_runner_provider::run_pair(&config, &provider)
        {
            Ok(pair) => pair,
            Err(e) => {
                return (
                    NodeExecutionOutput {
                        status: "failed".to_string(),
                        executor_type: "local_runner_validation".to_string(),
                        output: None,
                        error_domain: Some("run_error".to_string()),
                        error_message: Some(format!("run error: {e}")),
                        input_tokens: None,
                        output_tokens: None,
                        estimated_cost: None,
                        latency_ms: None,
                    },
                    None,
                );
            }
        };

        let sl = crate::local_scorecard_import::validate_scorecard_for_bounded_export(&stateless)
            .unwrap_or(false);
        let sf = crate::local_scorecard_import::validate_scorecard_for_bounded_export(&stateful)
            .unwrap_or(false);
        if !sl || !sf {
            return (
                NodeExecutionOutput {
                    status: "failed".to_string(),
                    executor_type: "local_runner_validation".to_string(),
                    output: None,
                    error_domain: Some("validation_error".to_string()),
                    error_message: Some("scorecard validation failed".to_string()),
                    input_tokens: None,
                    output_tokens: None,
                    estimated_cost: None,
                    latency_ms: None,
                },
                None,
            );
        }

        let stateless_tokens = stateless["input_token_total"].as_i64().unwrap_or(0)
            + stateless["output_token_total"].as_i64().unwrap_or(0);
        let stateful_tokens = stateful["input_token_total"].as_i64().unwrap_or(0)
            + stateful["output_token_total"].as_i64().unwrap_or(0);

        let summary = serde_json::json!({
            "validation_status": if stateful_tokens < stateless_tokens { "pass" } else { "fail" },
            "stateless_total_tokens": stateless_tokens,
            "stateful_total_tokens": stateful_tokens,
            "token_reduction_ratio": if stateless_tokens > 0 {
                ((stateless_tokens - stateful_tokens) as f64 / stateless_tokens as f64 * 10000.0).round() / 10000.0
            } else { 0.0 },
            "stateless_run_id": stateless["adapter_run_id"].as_str().unwrap_or(""),
            "stateful_run_id": stateful["adapter_run_id"].as_str().unwrap_or(""),
            "scenario_id": stateless["scenario_id"].as_str().unwrap_or(""),
        });

        let output = NodeExecutionOutput {
            status: "completed".to_string(),
            executor_type: "local_runner_validation".to_string(),
            output: Some(serde_json::to_string(&summary).unwrap_or_default()),
            error_domain: None,
            error_message: None,
            input_tokens: Some(stateless_tokens + stateful_tokens),
            output_tokens: Some(0),
            estimated_cost: Some(0.0),
            latency_ms: Some(iterations as i64 * 5),
        };

        (output, Some(summary))
    }
}

impl NodeExecutor for LocalRunnerValidationExecutor {
    fn executor_type_name(&self) -> &str {
        "local_runner_validation"
    }

    fn execute_node(&self, input: &NodeExecutionInput) -> NodeExecutionOutput {
        let (output, _summary) = Self::run_validation(input);
        output
    }
}

/// Command executor with timeout, allowlist, cwd, and env policy.
pub struct CommandNodeExecutor {
    pub timeout_ms: u64,
    pub allowed_commands: Vec<String>,
    pub allowed_binaries: Vec<String>,
    pub env_vars: Vec<(String, String)>,
}

impl Default for CommandNodeExecutor {
    fn default() -> Self {
        Self {
            timeout_ms: 30_000,
            allowed_commands: vec![
                "echo".to_string(),
                "cat".to_string(),
                "ls".to_string(),
                "head".to_string(),
                "tail".to_string(),
                "grep".to_string(),
                "wc".to_string(),
                "true".to_string(),
                "false".to_string(),
                "test".to_string(),
                "tee".to_string(),
                "sed".to_string(),
                "python3".to_string(),
                "python".to_string(),
            ],
            allowed_binaries: vec![
                "echo".to_string(),
                "cat".to_string(),
                "ls".to_string(),
                "head".to_string(),
                "tail".to_string(),
                "grep".to_string(),
                "wc".to_string(),
                "true".to_string(),
                "false".to_string(),
                "test".to_string(),
                "tee".to_string(),
                "sed".to_string(),
                "python3".to_string(),
                "python".to_string(),
            ],
            env_vars: Vec::new(),
        }
    }
}

impl CommandNodeExecutor {
    pub fn with_timeout(mut self, ms: u64) -> Self {
        self.timeout_ms = ms;
        self
    }

    pub fn with_allowed_commands(mut self, cmds: Vec<String>) -> Self {
        self.allowed_commands = cmds;
        self
    }

    fn is_command_allowed(&self, command: &str) -> bool {
        let first_token = command.split_whitespace().next().unwrap_or("");
        let binary = first_token.rsplit('/').next().unwrap_or(first_token);
        self.allowed_binaries.iter().any(|a| a == binary)
            || self.allowed_commands.iter().any(|a| a == binary)
    }

    fn has_shell_metacharacters(command: &str) -> bool {
        for ch in command.chars() {
            match ch {
                ';' | '|' | '>' | '<' | '&' | '$' | '`' | '\'' | '"' | '\\' => return true,
                c if c.is_control() && c != '\t' => return true,
                _ => {}
            }
        }
        false
    }

    fn parse_argv(command: &str) -> Vec<String> {
        command.split_whitespace().map(|s| s.to_string()).collect()
    }

    fn workspace_cwd(input: &NodeExecutionInput) -> Result<PathBuf, String> {
        let cwd = input
            .node_metadata
            .get("workspace_path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        if cwd == "." {
            return Ok(PathBuf::from("."));
        }

        let cwd_path = Path::new(cwd);
        ensure_clean_workspace_path(cwd_path, "workspace_path")?;
        let cwd_canonical =
            std::fs::canonicalize(cwd_path).map_err(|e| format!("workspace_path invalid: {e}"))?;

        if let Some(root) = input
            .node_metadata
            .get("workspace_root")
            .and_then(|v| v.as_str())
        {
            let root_path = Path::new(root);
            ensure_clean_workspace_path(root_path, "workspace_root")?;
            let root_canonical = std::fs::canonicalize(root_path)
                .map_err(|e| format!("workspace_root invalid: {e}"))?;
            if !cwd_canonical.starts_with(&root_canonical) {
                return Err("workspace_path escaped workspace_root".to_string());
            }
        }

        Ok(cwd_canonical)
    }
}

fn ensure_clean_workspace_path(path: &Path, field: &str) -> Result<(), String> {
    if !path.is_absolute() {
        return Err(format!("{field} must be absolute"));
    }
    for component in path.components() {
        if matches!(component, Component::ParentDir | Component::CurDir) {
            return Err(format!("{field} must not contain . or .. components"));
        }
    }
    Ok(())
}

fn truncate_command_output(mut output: String, original_len: usize) -> String {
    if original_len <= MAX_COMMAND_OUTPUT_BYTES {
        return output;
    }
    let mut split = MAX_COMMAND_OUTPUT_BYTES;
    split = split.min(output.len());
    while split > 0 && !output.is_char_boundary(split) {
        split -= 1;
    }
    output.truncate(split);
    output.push_str(&format!(
        "\n[truncated {} bytes]\n",
        original_len.saturating_sub(split)
    ));
    output
}

fn read_command_output(mut reader: impl Read) -> std::io::Result<(Vec<u8>, usize)> {
    let mut kept = Vec::new();
    let mut total = 0_usize;
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read);
        let remaining = MAX_COMMAND_OUTPUT_BYTES.saturating_sub(kept.len());
        kept.extend_from_slice(&buffer[..remaining.min(read)]);
    }
    Ok((kept, total))
}

impl NodeExecutor for CommandNodeExecutor {
    fn executor_type_name(&self) -> &str {
        "command"
    }
    fn execute_node(&self, input: &NodeExecutionInput) -> NodeExecutionOutput {
        let start = std::time::Instant::now();
        let command = input
            .node_metadata
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("echo noop");

        if Self::has_shell_metacharacters(command) {
            return NodeExecutionOutput {
                status: "failed".to_string(),
                executor_type: "command".to_string(),
                output: None,
                error_domain: Some("command_not_allowed".to_string()),
                error_message: Some(format!(
                    "shell metacharacters rejected: {}",
                    command.split_whitespace().next().unwrap_or("")
                )),
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(start.elapsed().as_millis() as i64),
            };
        }

        if !self.is_command_allowed(command) {
            return NodeExecutionOutput {
                status: "failed".to_string(),
                executor_type: "command".to_string(),
                output: None,
                error_domain: Some("command_not_allowed".to_string()),
                error_message: Some(format!(
                    "command not in allowlist: {}",
                    command.split_whitespace().next().unwrap_or("")
                )),
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(start.elapsed().as_millis() as i64),
            };
        }

        let argv = Self::parse_argv(command);
        if argv.is_empty() {
            return NodeExecutionOutput {
                status: "failed".to_string(),
                executor_type: "command".to_string(),
                output: None,
                error_domain: Some("command_not_allowed".to_string()),
                error_message: Some("empty command".to_string()),
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(start.elapsed().as_millis() as i64),
            };
        }

        let cwd = match Self::workspace_cwd(input) {
            Ok(path) => path,
            Err(e) => {
                return NodeExecutionOutput {
                    status: "failed".to_string(),
                    executor_type: "command".to_string(),
                    output: None,
                    error_domain: Some("workspace_escape".to_string()),
                    error_message: Some(e),
                    input_tokens: None,
                    output_tokens: None,
                    estimated_cost: None,
                    latency_ms: Some(start.elapsed().as_millis() as i64),
                };
            }
        };

        let mut cmd = std::process::Command::new(&argv[0]);
        if argv.len() > 1 {
            cmd.args(&argv[1..]);
        }
        cmd.current_dir(cwd);
        cmd.env_clear();
        cmd.env(
            "PATH",
            std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string()),
        );
        for (k, v) in &self.env_vars {
            cmd.env(k, v);
        }

        let mut child = match cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                return NodeExecutionOutput {
                    status: "failed".to_string(),
                    executor_type: "command".to_string(),
                    output: None,
                    error_domain: Some("command_spawn_error".to_string()),
                    error_message: Some(e.to_string()),
                    input_tokens: None,
                    output_tokens: None,
                    estimated_cost: None,
                    latency_ms: Some(start.elapsed().as_millis() as i64),
                };
            }
        };
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let stdout_reader = thread::spawn(move || read_command_output(stdout));
        let stderr_reader = thread::spawn(move || read_command_output(stderr));

        let deadline = std::time::Duration::from_millis(self.timeout_ms);
        let wait_start = std::time::Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    if wait_start.elapsed() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        let _ = stdout_reader.join();
                        let _ = stderr_reader.join();
                        return NodeExecutionOutput {
                            status: "failed".to_string(),
                            executor_type: "command".to_string(),
                            output: None,
                            error_domain: Some("command_timeout".to_string()),
                            error_message: Some(format!("timeout after {}ms", self.timeout_ms)),
                            input_tokens: None,
                            output_tokens: None,
                            estimated_cost: None,
                            latency_ms: Some(start.elapsed().as_millis() as i64),
                        };
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(e) => {
                    return NodeExecutionOutput {
                        status: "failed".to_string(),
                        executor_type: "command".to_string(),
                        output: None,
                        error_domain: Some("command_wait_error".to_string()),
                        error_message: Some(e.to_string()),
                        input_tokens: None,
                        output_tokens: None,
                        estimated_cost: None,
                        latency_ms: Some(start.elapsed().as_millis() as i64),
                    };
                }
            }
        };
        let (stdout_bytes, stdout_len) = match stdout_reader.join() {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => {
                return command_output_error(start, error.to_string());
            }
            Err(_) => return command_output_error(start, "stdout reader failed".to_string()),
        };
        let (stderr_bytes, stderr_len) = match stderr_reader.join() {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => {
                return command_output_error(start, error.to_string());
            }
            Err(_) => return command_output_error(start, "stderr reader failed".to_string()),
        };

        let elapsed_ms = start.elapsed().as_millis() as i64;
        let stdout = String::from_utf8_lossy(&stdout_bytes).to_string();
        let stderr = String::from_utf8_lossy(&stderr_bytes).to_string();
        let exit_code = status.code().unwrap_or(-1);
        let combined = if stderr.is_empty() {
            stdout.clone()
        } else {
            format!("{stdout}\n[stderr]\n{stderr}")
        };
        let delimiter_len = if stderr_len > 0 {
            "\n[stderr]\n".len()
        } else {
            0
        };
        let combined = truncate_command_output(
            combined,
            stdout_len
                .saturating_add(stderr_len)
                .saturating_add(delimiter_len),
        );

        if status.success() {
            NodeExecutionOutput {
                status: "completed".to_string(),
                executor_type: "command".to_string(),
                output: Some(combined),
                error_domain: None,
                error_message: None,
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(elapsed_ms),
            }
        } else {
            NodeExecutionOutput {
                status: "failed".to_string(),
                executor_type: "command".to_string(),
                output: Some(combined),
                error_domain: Some("command_exit_nonzero".to_string()),
                error_message: Some(format!("exit code {exit_code}: {}", stderr.trim())),
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(elapsed_ms),
            }
        }
    }
}

fn command_output_error(start: std::time::Instant, message: String) -> NodeExecutionOutput {
    NodeExecutionOutput {
        status: "failed".to_string(),
        executor_type: "command".to_string(),
        output: None,
        error_domain: Some("command_output_error".to_string()),
        error_message: Some(message),
        input_tokens: None,
        output_tokens: None,
        estimated_cost: None,
        latency_ms: Some(start.elapsed().as_millis() as i64),
    }
}

// ── Agent Step Executor (AR-2) ──────────────────────────────────────────

const ACP_ENABLE_AGENT_RUNTIME: &str = "ACP_ENABLE_AGENT_RUNTIME";

/// AR-2 + AR-3 agent actions.
/// AR-3 adds: propose_child_task, request_handoff, accept_handoff,
/// reject_handoff, cancel_proposal.
/// AR-5 adds: request_review, submit_review_verdict, open_debate,
/// submit_debate_position, resolve_debate.
/// Does NOT include: provider/CLI calls, target-output, merge, deploy,
/// or release.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentAction {
    Wait,
    Complete,
    UpdateScratchpadSummary(String),
    ReadMailbox,
    AckMessage(String),
    EmitNote(String),
    RecordObservation(String),
    Unsupported(String),
    // AR-3: Bounded planning, child task proposals, and handoff
    ProposeChildTask(ChildTaskProposal),
    RequestHandoff(HandoffRequest),
    AcceptHandoff(String),
    RejectHandoff(String),
    CancelProposal(String),
    // AR-5: Bounded review/debate primitives
    RequestReview(ReviewRequest),
    SubmitReviewVerdict(ReviewVerdict),
    OpenDebate(DebateRequest),
    SubmitDebatePosition(DebatePosition),
    ResolveDebate(DebateResolution),
}

/// Context assembled during the observe phase for the decision source.
#[derive(Debug, Clone)]
pub struct AgentStepContext {
    pub agent_id: String,
    pub run_id: String,
    pub node_id: String,
    pub workflow_id: String,
    pub agent_state: Option<AgentState>,
    pub mailbox_pending_count: i64,
    pub memory_digest: Option<Value>,
    pub memory_context: Option<Value>,
    pub memory_state_read_bytes: i64,
    pub node_metadata: Value,
}

/// Bounded one-step agent executor: observe → decide → act → persist.
///
/// AR-2 actions: Wait, Complete, UpdateScratchpadSummary, ReadMailbox,
/// AckMessage, EmitNote, RecordObservation.
/// AR-3 actions (behind the same env gate + kill switch):
/// ProposeChildTask, RequestHandoff, AcceptHandoff, RejectHandoff, CancelProposal.
///
/// Does not implement concurrent multi-agent scheduling, review/debate,
/// provider/CLI calls, hidden mailboxes, automatic merge/deploy/release,
/// or direct target-repo main writes.
/// Provider-backed decision source is default-off; tests use a deterministic stub.
pub struct AgentStepExecutor {
    pub store: Arc<LocalProductStore>,
    pub decision_source: AgentDecisionFn,
}

fn agent_step_fail(message: &str, start: &std::time::Instant) -> NodeExecutionOutput {
    NodeExecutionOutput {
        status: "failed".to_string(),
        executor_type: "agent_step".to_string(),
        output: None,
        error_domain: Some("agent_step_error".to_string()),
        error_message: Some(message.to_string()),
        input_tokens: None,
        output_tokens: None,
        estimated_cost: None,
        latency_ms: Some(start.elapsed().as_millis() as i64),
    }
}

fn action_result_with_state_metrics(
    mut result: Value,
    state_read_bytes: i64,
    state_write_bytes: i64,
) -> String {
    if let Some(obj) = result.as_object_mut() {
        obj.insert(
            "state_read_bytes".to_string(),
            json!(state_read_bytes.max(0)),
        );
        obj.insert(
            "state_write_bytes".to_string(),
            json!(state_write_bytes.max(0)),
        );
    }
    result.to_string()
}

impl AgentStepExecutor {
    pub fn new(store: Arc<LocalProductStore>, decision_source: AgentDecisionFn) -> Self {
        Self {
            store,
            decision_source,
        }
    }

    /// Append a best-effort audit event for agent-step lifecycle.
    ///
    /// These events are diagnostic — a failed audit write must not block or
    /// change the execution outcome. The agent already persisted its action
    /// result (or failure) through `execute_action` before this runs.
    /// Sanitized descriptors are used; no raw action body, note, observation,
    /// scratchpad, or proposal text appears in the audit payload.
    fn append_agent_step_audit_best_effort(
        &self,
        action: &str,
        agent_id: &str,
        run_id: &str,
        details: &Value,
    ) {
        let _ = self.store.append_audit(
            "agent_step",
            action,
            &format!("agent_state/{agent_id}/{run_id}"),
            details,
        );
    }

    fn execute_action(
        &self,
        agent_id: &str,
        run_id: &str,
        _input_node_id: &str,
        agent_state: &AgentState,
        mailbox_pending_count: i64,
        memory_state_read_bytes: i64,
        action: &AgentAction,
    ) -> Result<String, String> {
        match action {
            AgentAction::Wait => Ok(action_result_with_state_metrics(
                json!({"action":"wait"}),
                memory_state_read_bytes,
                0,
            )),
            AgentAction::Complete => {
                self.store
                    .update_agent_state(agent_id, run_id, Some("completed"), None, None, None)
                    .map_err(|e| format!("failed to update agent state: {e}"))?;
                Ok(action_result_with_state_metrics(
                    json!({"action":"complete"}),
                    memory_state_read_bytes,
                    0,
                ))
            }
            AgentAction::UpdateScratchpadSummary(text) => {
                let digest = consolidate_memory_digest(
                    agent_state,
                    Some(text.as_str()),
                    mailbox_pending_count,
                )
                .ok_or_else(|| "failed to consolidate memory digest".to_string())?;
                let safe_summary = digest
                    .get("summary")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let metadata_patch = memory_digest_to_metadata_patch(&digest);
                self.store
                    .update_agent_state(
                        agent_id,
                        run_id,
                        None,
                        Some(&safe_summary),
                        None,
                        Some(&metadata_patch),
                    )
                    .map_err(|e| format!("failed to update scratchpad: {e}"))?;
                Ok(action_result_with_state_metrics(
                    json!({"action":"update_scratchpad"}),
                    memory_state_read_bytes,
                    estimate_memory_state_bytes(Some(&digest), None),
                ))
            }
            AgentAction::ReadMailbox => {
                let msgs = self
                    .store
                    .list_mailbox(Some(agent_id), Some(run_id), None, Some("pending"), 10, 0)
                    .map_err(|e| format!("failed to list mailbox: {e}"))?;
                let summaries: Vec<Value> = msgs
                    .iter()
                    .map(|m| {
                        json!({
                            "message_id": m.message_id,
                            "from": m.from_agent_id,
                            "type": m.message_type,
                            "summary": m.body_summary,
                        })
                    })
                    .collect();
                Ok(action_result_with_state_metrics(
                    json!({"action":"read_mailbox","mailbox_count": summaries.len(),"messages": summaries}),
                    memory_state_read_bytes,
                    0,
                ))
            }
            AgentAction::AckMessage(message_id) => {
                let acked = self
                    .store
                    .ack_message_for_agent(message_id, agent_id, run_id)
                    .map_err(|e| format!("failed to ack message: {e}"))?;
                if acked.is_some() {
                    Ok(action_result_with_state_metrics(
                        json!({"action":"ack_message","message_id": message_id}),
                        memory_state_read_bytes,
                        0,
                    ))
                } else {
                    Err(format!("message {message_id} not found or already acked"))
                }
            }
            AgentAction::EmitNote(text) => {
                let (_safe_note, redact_status) = sanitize_text_field(text);
                let _ = self.store.append_audit(
                    "agent_step",
                    "agent_step.note",
                    &format!("agent_state/{agent_id}/{run_id}"),
                    &json!({"redaction_status": redact_status, "char_count": text.len()}),
                );
                Ok(action_result_with_state_metrics(
                    json!({"action":"emit_note","redaction_status": redact_status}),
                    memory_state_read_bytes,
                    0,
                ))
            }
            AgentAction::RecordObservation(text) => {
                let (_safe_obs, redact_status) = sanitize_text_field(text);
                let _ = self.store.append_audit(
                    "agent_step",
                    "agent_step.observation",
                    &format!("agent_state/{agent_id}/{run_id}"),
                    &json!({"redaction_status": redact_status, "char_count": text.len()}),
                );
                Ok(action_result_with_state_metrics(
                    json!({"action":"record_observation","redaction_status": redact_status}),
                    memory_state_read_bytes,
                    0,
                ))
            }
            // ── AR-3: Bounded planning, child task proposals, and handoff ──
            AgentAction::ProposeChildTask(proposal) => {
                if proposal.objective.len() > MAX_PROPOSAL_OBJECTIVE_BYTES {
                    return Err(format!(
                        "proposal objective exceeds {} byte cap",
                        MAX_PROPOSAL_OBJECTIVE_BYTES
                    ));
                }
                let proposal_id = format!(
                    "prop-{}-{}",
                    proposal.correlation_id,
                    chrono::Utc::now().timestamp_millis()
                );
                self.store
                    .create_proposal(
                        &proposal_id,
                        &proposal.correlation_id,
                        &proposal.run_id,
                        &proposal.parent_node_id,
                        &proposal.agent_id,
                        "child_task",
                        &proposal.objective,
                        &proposal.context_summary,
                        None,
                        proposal.proposed_node_id.as_deref(),
                        proposal.proposed_edge_id.as_deref(),
                    )
                    .map_err(|e| format!("failed to create child task proposal: {e}"))?;
                let _ = self.store.append_audit(
                    "agent_step",
                    "agent_step.propose_child_task",
                    &format!("agent_state/{agent_id}/{run_id}"),
                    &json!({"proposal_id": proposal_id, "correlation_id": proposal.correlation_id,
                            "agent_id": agent_id, "run_id": run_id}),
                );
                Ok(
                    json!({"action":"propose_child_task","proposal_id": proposal_id,
                          "correlation_id": proposal.correlation_id})
                    .to_string(),
                )
            }
            AgentAction::RequestHandoff(request) => {
                if request.objective.len() > MAX_PROPOSAL_OBJECTIVE_BYTES {
                    return Err(format!(
                        "handoff objective exceeds {} byte cap",
                        MAX_PROPOSAL_OBJECTIVE_BYTES
                    ));
                }
                if request.target_agent_id == *agent_id {
                    return Err(
                        "handoff target agent must be different from source agent".to_string()
                    );
                }
                let proposal_id = format!(
                    "handoff-{}-{}",
                    request.correlation_id,
                    chrono::Utc::now().timestamp_millis()
                );
                self.store
                    .create_proposal(
                        &proposal_id,
                        &request.correlation_id,
                        &request.run_id,
                        &request.node_id,
                        &request.source_agent_id,
                        "handoff",
                        &request.objective,
                        &request.context_summary,
                        Some(&request.target_agent_id),
                        None,
                        None,
                    )
                    .map_err(|e| format!("failed to create handoff proposal: {e}"))?;
                let msg_id = format!(
                    "msg-{}-{}",
                    request.correlation_id,
                    chrono::Utc::now().timestamp_millis()
                );
                self.store
                    .send_message(
                        &msg_id,
                        &request.source_agent_id,
                        &request.target_agent_id,
                        "handoff_request",
                        Some(&format!("Objective: {}", request.objective)),
                        Some(&request.correlation_id),
                        Some(&request.run_id),
                        Some(&request.node_id),
                        None,
                        &json!({"proposal_id": proposal_id}),
                    )
                    .map_err(|e| format!("failed to send handoff message: {e}"))?;
                let _ = self.store.append_audit(
                    "agent_step",
                    "agent_step.request_handoff",
                    &format!("agent_state/{agent_id}/{run_id}"),
                    &json!({"proposal_id": proposal_id, "correlation_id": request.correlation_id,
                            "source_agent_id": request.source_agent_id,
                            "target_agent_id": request.target_agent_id}),
                );
                Ok(
                    json!({"action":"request_handoff","proposal_id": proposal_id,
                          "correlation_id": request.correlation_id,
                          "target_agent_id": request.target_agent_id})
                    .to_string(),
                )
            }
            AgentAction::AcceptHandoff(correlation_id) => {
                let proposal = self
                    .store
                    .find_pending_handoff_for_target(correlation_id, agent_id, run_id)
                    .map_err(|e| format!("failed to find proposal: {e}"))?;
                match proposal {
                    Some(p) => {
                        let pid = p["proposal_id"].as_str().unwrap_or("");
                        if p["status"].as_str() != Some("pending") {
                            return Err(format!("handoff proposal {pid} is not pending"));
                        }
                        let updated = self
                            .store
                            .update_proposal_status(pid, "accepted")
                            .map_err(|e| format!("failed to accept handoff: {e}"))?;
                        if !updated {
                            return Err(format!("handoff proposal {pid} could not be accepted (not found or not pending)"));
                        }
                        let msg_id = format!(
                            "ack-{}-{}",
                            correlation_id,
                            chrono::Utc::now().timestamp_millis()
                        );
                        let from = p["agent_id"].as_str().unwrap_or("");
                        let run = p["run_id"].as_str().unwrap_or("");
                        let node = p["parent_node_id"].as_str().unwrap_or("");
                        // Reply to the source agent
                        let _ = self.store.send_message(
                            &msg_id,
                            agent_id,
                            from,
                            "handoff_accepted",
                            Some("Handoff accepted"),
                            Some(correlation_id),
                            Some(run),
                            Some(node),
                            None,
                            &json!({"proposal_id": pid}),
                        );
                        let _ = self.store.append_audit(
                            "agent_step",
                            "agent_step.accept_handoff",
                            &format!("agent_state/{agent_id}/{run_id}"),
                            &json!({"proposal_id": pid, "correlation_id": correlation_id}),
                        );
                        Ok(json!({"action":"accept_handoff","proposal_id": pid,
                                  "correlation_id": correlation_id})
                        .to_string())
                    }
                    None => Err(format!(
                        "no pending handoff proposal found for correlation_id {correlation_id}"
                    )),
                }
            }
            AgentAction::RejectHandoff(correlation_id) => {
                let proposal = self
                    .store
                    .find_pending_handoff_for_target(correlation_id, agent_id, run_id)
                    .map_err(|e| format!("failed to find proposal: {e}"))?;
                match proposal {
                    Some(p) => {
                        let pid = p["proposal_id"].as_str().unwrap_or("");
                        if p["status"].as_str() != Some("pending") {
                            return Err(format!("handoff proposal {pid} is not pending"));
                        }
                        let updated = self
                            .store
                            .update_proposal_status(pid, "rejected")
                            .map_err(|e| format!("failed to reject handoff: {e}"))?;
                        if !updated {
                            return Err(format!("handoff proposal {pid} could not be rejected (not found or not pending)"));
                        }
                        let msg_id = format!(
                            "rej-{}-{}",
                            correlation_id,
                            chrono::Utc::now().timestamp_millis()
                        );
                        let from = p["agent_id"].as_str().unwrap_or("");
                        let run = p["run_id"].as_str().unwrap_or("");
                        let node = p["parent_node_id"].as_str().unwrap_or("");
                        let _ = self.store.send_message(
                            &msg_id,
                            agent_id,
                            from,
                            "handoff_rejected",
                            Some("Handoff rejected"),
                            Some(correlation_id),
                            Some(run),
                            Some(node),
                            None,
                            &json!({"proposal_id": pid}),
                        );
                        let _ = self.store.append_audit(
                            "agent_step",
                            "agent_step.reject_handoff",
                            &format!("agent_state/{agent_id}/{run_id}"),
                            &json!({"proposal_id": pid, "correlation_id": correlation_id}),
                        );
                        Ok(json!({"action":"reject_handoff","proposal_id": pid,
                                  "correlation_id": correlation_id})
                        .to_string())
                    }
                    None => Err(format!(
                        "no pending handoff proposal found for correlation_id {correlation_id}"
                    )),
                }
            }
            AgentAction::CancelProposal(correlation_id) => {
                let proposal = self
                    .store
                    .find_proposal_by_correlation(correlation_id, agent_id)
                    .map_err(|e| format!("failed to find proposal: {e}"))?;
                match proposal {
                    Some(p) => {
                        let pid = p["proposal_id"].as_str().unwrap_or("");
                        if p["status"].as_str() != Some("pending") {
                            return Err(format!("proposal {pid} is not pending"));
                        }
                        let owner = p["agent_id"].as_str().unwrap_or("");
                        if owner != agent_id {
                            return Err(format!(
                                "only the proposal owner ({owner}) can cancel, not {agent_id}"
                            ));
                        }
                        let updated = self
                            .store
                            .update_proposal_status(pid, "cancelled")
                            .map_err(|e| format!("failed to cancel proposal: {e}"))?;
                        if !updated {
                            return Err(format!(
                                "proposal {pid} could not be cancelled (not found or not pending)"
                            ));
                        }
                        let _ = self.store.append_audit(
                            "agent_step",
                            "agent_step.cancel_proposal",
                            &format!("agent_state/{agent_id}/{run_id}"),
                            &json!({"proposal_id": pid, "correlation_id": correlation_id}),
                        );
                        Ok(json!({"action":"cancel_proposal","proposal_id": pid,
                                  "correlation_id": correlation_id})
                        .to_string())
                    }
                    None => Err(format!(
                        "no pending proposal found for correlation_id {correlation_id}"
                    )),
                }
            }
            AgentAction::Unsupported(name) => Err(format!("unsupported action: {name}")),
            // ── AR-5: Bounded review/debate primitives ──
            AgentAction::RequestReview(request) => {
                if request.run_id != run_id {
                    return Err(format!(
                        "review request run_id '{}' does not match current run '{}'",
                        request.run_id, run_id
                    ));
                }
                if request.target_agent_id == *agent_id {
                    return Err(
                        "review target agent must be different from requesting agent".to_string(),
                    );
                }
                if request.subject_summary.len() > MAX_REVIEW_DEBATE_TEXT_BYTES {
                    return Err(format!(
                        "subject_summary exceeds {} byte cap",
                        MAX_REVIEW_DEBATE_TEXT_BYTES
                    ));
                }
                if request.rationale_summary.len() > MAX_REVIEW_DEBATE_TEXT_BYTES {
                    return Err(format!(
                        "rationale_summary exceeds {} byte cap",
                        MAX_REVIEW_DEBATE_TEXT_BYTES
                    ));
                }
                let proposal_id = format!(
                    "review-{}-{}",
                    request.correlation_id,
                    chrono::Utc::now().timestamp_millis()
                );
                self.store
                    .create_proposal(
                        &proposal_id,
                        &request.correlation_id,
                        &request.run_id,
                        &request.node_id,
                        agent_id,
                        "review_request",
                        &request.subject_summary,
                        &request.rationale_summary,
                        Some(&request.target_agent_id),
                        None,
                        None,
                    )
                    .map_err(|e| format!("failed to create review request: {e}"))?;
                let msg_id = format!(
                    "review-msg-{}-{}",
                    request.correlation_id,
                    chrono::Utc::now().timestamp_millis()
                );
                self.store
                    .send_message(
                        &msg_id,
                        agent_id,
                        &request.target_agent_id,
                        "review_request",
                        Some(&format!("Review requested: {}", request.subject_summary)),
                        Some(&request.correlation_id),
                        Some(&request.run_id),
                        Some(&request.node_id),
                        None,
                        &json!({"proposal_id": proposal_id, "blocking": request.blocking}),
                    )
                    .map_err(|e| format!("failed to send review request message: {e}"))?;
                let _ = self.store.append_audit(
                    "agent_step",
                    "review.requested",
                    &format!("agent_state/{agent_id}/{run_id}"),
                    &json!({
                        "run_id": run_id,
                        "agent_id": agent_id,
                        "target_agent_id": request.target_agent_id,
                        "proposal_id": proposal_id,
                        "correlation_id": request.correlation_id,
                        "blocking": request.blocking,
                    }),
                );
                Ok(json!({"action":"request_review","proposal_id": proposal_id,
                          "correlation_id": request.correlation_id,
                          "target_agent_id": request.target_agent_id})
                .to_string())
            }
            AgentAction::SubmitReviewVerdict(verdict) => {
                if verdict.run_id != run_id {
                    return Err(format!(
                        "review verdict run_id '{}' does not match current run '{}'",
                        verdict.run_id, run_id
                    ));
                }
                if !REVIEW_VERDICTS.contains(&verdict.verdict.as_str()) {
                    return Err(format!(
                        "invalid verdict '{}', expected one of {}",
                        verdict.verdict,
                        REVIEW_VERDICTS.join(", ")
                    ));
                }
                if verdict.rationale_summary.len() > MAX_REVIEW_DEBATE_TEXT_BYTES {
                    return Err(format!(
                        "rationale_summary exceeds {} byte cap",
                        MAX_REVIEW_DEBATE_TEXT_BYTES
                    ));
                }
                let proposal = self
                    .store
                    .get_proposal(&verdict.review_request_id)
                    .map_err(|e| format!("failed to find review request: {e}"))?;
                match proposal {
                    Some(p) => {
                        let pid = p["proposal_id"].as_str().unwrap_or("");
                        let ptype = p["proposal_type"].as_str().unwrap_or("");
                        let pstatus = p["status"].as_str().unwrap_or("");
                        let prun = p["run_id"].as_str().unwrap_or("");
                        let ptarget = p["target_agent_id"].as_str().unwrap_or("");
                        if ptype != "review_request" {
                            return Err(format!("proposal {pid} is not a review_request"));
                        }
                        if prun != run_id {
                            return Err(format!(
                                "review request {pid} belongs to run '{prun}', not '{run_id}'"
                            ));
                        }
                        if ptarget != agent_id {
                            return Err(format!(
                                "agent {agent_id} is not the target reviewer for proposal {pid}"
                            ));
                        }
                        if pstatus != "pending" {
                            return Err(format!(
                                "review request {pid} is not pending (status: {pstatus})"
                            ));
                        }
                        let new_status = if verdict.verdict == "accepted" {
                            "accepted"
                        } else {
                            "rejected"
                        };
                        let updated = self
                            .store
                            .update_proposal_status(pid, new_status)
                            .map_err(|e| format!("failed to update review status: {e}"))?;
                        if !updated {
                            return Err(format!(
                                "review request {pid} status update failed — proposal may no longer be pending"
                            ));
                        }
                        let verdict_proposal_id = format!(
                            "rv-{}-{}",
                            verdict.correlation_id,
                            chrono::Utc::now().timestamp_millis()
                        );
                        self.store
                            .create_proposal(
                                &verdict_proposal_id,
                                &verdict.correlation_id,
                                &verdict.run_id,
                                &verdict.node_id,
                                agent_id,
                                "review_verdict",
                                &verdict.verdict,
                                &verdict.rationale_summary,
                                None,
                                None,
                                None,
                            )
                            .map_err(|e| format!("failed to create review verdict: {e}"))?;
                        let _ = self.store.append_audit(
                            "agent_step",
                            "review.verdict_submitted",
                            &format!("agent_state/{agent_id}/{run_id}"),
                            &json!({
                                "run_id": run_id,
                                "agent_id": agent_id,
                                "proposal_id": pid,
                                "verdict_proposal_id": verdict_proposal_id,
                                "correlation_id": verdict.correlation_id,
                                "verdict": verdict.verdict,
                                "blocking": verdict.blocking,
                            }),
                        );
                        Ok(json!({"action":"submit_review_verdict","proposal_id": pid,
                                  "verdict_proposal_id": verdict_proposal_id,
                                  "verdict": verdict.verdict})
                        .to_string())
                    }
                    None => Err(format!(
                        "review request proposal {} not found",
                        verdict.review_request_id
                    )),
                }
            }
            AgentAction::OpenDebate(debate) => {
                if debate.run_id != run_id {
                    return Err(format!(
                        "debate request run_id '{}' does not match current run '{}'",
                        debate.run_id, run_id
                    ));
                }
                if debate.participant_agent_ids.is_empty() {
                    return Err("debate must have at least one participant".to_string());
                }
                if debate.participant_agent_ids.len() > MAX_DEBATE_PARTICIPANTS {
                    return Err(format!(
                        "debate participant count {} exceeds max {}",
                        debate.participant_agent_ids.len(),
                        MAX_DEBATE_PARTICIPANTS
                    ));
                }
                let max_rounds = debate.max_rounds.min(MAX_DEBATE_ROUNDS);
                if debate.subject_summary.len() > MAX_REVIEW_DEBATE_TEXT_BYTES {
                    return Err(format!(
                        "subject_summary exceeds {} byte cap",
                        MAX_REVIEW_DEBATE_TEXT_BYTES
                    ));
                }
                let proposal_id = format!(
                    "debate-{}-{}",
                    debate.correlation_id,
                    chrono::Utc::now().timestamp_millis()
                );
                let debate_meta = json!({
                    "max_rounds": max_rounds,
                    "current_round": 0,
                    "participant_agent_ids": debate.participant_agent_ids,
                });
                self.store
                    .create_proposal(
                        &proposal_id,
                        &debate.correlation_id,
                        &debate.run_id,
                        &debate.node_id,
                        agent_id,
                        "debate_request",
                        &debate.subject_summary,
                        &debate_meta.to_string(),
                        None,
                        None,
                        None,
                    )
                    .map_err(|e| format!("failed to create debate request: {e}"))?;
                for pid in &debate.participant_agent_ids {
                    let msg_id = format!(
                        "debate-msg-{}-{}-{}",
                        debate.correlation_id,
                        pid,
                        chrono::Utc::now().timestamp_millis()
                    );
                    self.store
                        .send_message(
                            &msg_id,
                            agent_id,
                            pid,
                            "debate_request",
                            Some(&format!("Debate opened: {}", debate.subject_summary)),
                            Some(&debate.correlation_id),
                            Some(&debate.run_id),
                            Some(&debate.node_id),
                            None,
                            &json!({"proposal_id": proposal_id, "max_rounds": max_rounds}),
                        )
                        .map_err(|e| format!("failed to notify participant {pid}: {e}"))?;
                }
                let _ = self.store.append_audit(
                    "agent_step",
                    "debate.opened",
                    &format!("agent_state/{agent_id}/{run_id}"),
                    &json!({
                        "run_id": run_id,
                        "agent_id": agent_id,
                        "proposal_id": proposal_id,
                        "correlation_id": debate.correlation_id,
                        "participant_count": debate.participant_agent_ids.len(),
                        "max_rounds": max_rounds,
                    }),
                );
                Ok(json!({"action":"open_debate","proposal_id": proposal_id,
                          "correlation_id": debate.correlation_id,
                          "max_rounds": max_rounds})
                .to_string())
            }
            AgentAction::SubmitDebatePosition(position) => {
                if position.run_id != run_id {
                    return Err(format!(
                        "debate position run_id '{}' does not match current run '{}'",
                        position.run_id, run_id
                    ));
                }
                if position.position.len() > MAX_REVIEW_DEBATE_TEXT_BYTES {
                    return Err(format!(
                        "position exceeds {} byte cap",
                        MAX_REVIEW_DEBATE_TEXT_BYTES
                    ));
                }
                if position.rationale_summary.len() > MAX_REVIEW_DEBATE_TEXT_BYTES {
                    return Err(format!(
                        "rationale_summary exceeds {} byte cap",
                        MAX_REVIEW_DEBATE_TEXT_BYTES
                    ));
                }
                let debate_proposal = self
                    .store
                    .get_proposal(&position.debate_id)
                    .map_err(|e| format!("failed to find debate: {e}"))?;
                match debate_proposal {
                    Some(dp) => {
                        let dpid = dp["proposal_id"].as_str().unwrap_or("");
                        let dptype = dp["proposal_type"].as_str().unwrap_or("");
                        let dpstatus = dp["status"].as_str().unwrap_or("");
                        let dprun = dp["run_id"].as_str().unwrap_or("");
                        if dptype != "debate_request" {
                            return Err(format!("proposal {dpid} is not a debate_request"));
                        }
                        if dprun != run_id {
                            return Err(format!(
                                "debate {dpid} belongs to run '{dprun}', not '{run_id}'"
                            ));
                        }
                        if dpstatus != "pending" {
                            return Err(format!(
                                "debate {dpid} is not pending (status: {dpstatus})"
                            ));
                        }
                        let meta_str = dp["context_summary"].as_str().unwrap_or("{}");
                        let meta: serde_json::Value =
                            serde_json::from_str(meta_str).unwrap_or(json!({}));
                        let participants: Vec<String> = meta["participant_agent_ids"]
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default();
                        if !participants.contains(&agent_id.to_string()) {
                            return Err(format!(
                                "agent {agent_id} is not a participant in debate {dpid}"
                            ));
                        }
                        let max_rounds = meta["max_rounds"].as_u64().unwrap_or(1) as usize;
                        let current_round = meta["current_round"].as_u64().unwrap_or(0) as usize;
                        if current_round >= max_rounds {
                            return Err(format!(
                                "debate {dpid} has reached max rounds ({max_rounds})"
                            ));
                        }
                        let new_meta = json!({
                            "max_rounds": max_rounds,
                            "current_round": current_round + 1,
                            "participant_agent_ids": participants,
                        });
                        let round_updated = self
                            .store
                            .update_debate_round_if_pending(
                                dpid,
                                run_id,
                                current_round,
                                &new_meta.to_string(),
                            )
                            .map_err(|e| format!("failed to update debate round: {e}"))?;
                        if !round_updated {
                            return Err(format!(
                                "debate {dpid} round update failed — stale, terminal, or already at max"
                            ));
                        }
                        let pos_proposal_id = format!(
                            "dp-{}-{}-{}",
                            position.correlation_id,
                            agent_id,
                            chrono::Utc::now().timestamp_millis()
                        );
                        self.store
                            .create_proposal(
                                &pos_proposal_id,
                                &position.correlation_id,
                                &position.run_id,
                                &position.node_id,
                                agent_id,
                                "debate_position",
                                &position.position,
                                &position.rationale_summary,
                                Some(&position.debate_id),
                                None,
                                None,
                            )
                            .map_err(|e| format!("failed to create debate position: {e}"))?;
                        let _ = self.store.append_audit(
                            "agent_step",
                            "debate.position_submitted",
                            &format!("agent_state/{agent_id}/{run_id}"),
                            &json!({
                                "run_id": run_id,
                                "agent_id": agent_id,
                                "proposal_id": dpid,
                                "position_proposal_id": pos_proposal_id,
                                "correlation_id": position.correlation_id,
                                "current_round": current_round + 1,
                                "max_rounds": max_rounds,
                            }),
                        );
                        Ok(json!({"action":"submit_debate_position","debate_id": dpid,
                                  "position_proposal_id": pos_proposal_id,
                                  "current_round": current_round + 1})
                        .to_string())
                    }
                    None => Err(format!("debate proposal {} not found", position.debate_id)),
                }
            }
            AgentAction::ResolveDebate(resolution) => {
                if resolution.run_id != run_id {
                    return Err(format!(
                        "debate resolution run_id '{}' does not match current run '{}'",
                        resolution.run_id, run_id
                    ));
                }
                if resolution.resolution.len() > MAX_REVIEW_DEBATE_TEXT_BYTES {
                    return Err(format!(
                        "resolution exceeds {} byte cap",
                        MAX_REVIEW_DEBATE_TEXT_BYTES
                    ));
                }
                let debate_proposal = self
                    .store
                    .get_proposal(&resolution.debate_id)
                    .map_err(|e| format!("failed to find debate: {e}"))?;
                match debate_proposal {
                    Some(dp) => {
                        let dpid = dp["proposal_id"].as_str().unwrap_or("");
                        let dptype = dp["proposal_type"].as_str().unwrap_or("");
                        let dpstatus = dp["status"].as_str().unwrap_or("");
                        let dprun = dp["run_id"].as_str().unwrap_or("");
                        let dpagent = dp["agent_id"].as_str().unwrap_or("");
                        if dptype != "debate_request" {
                            return Err(format!("proposal {dpid} is not a debate_request"));
                        }
                        if dprun != run_id {
                            return Err(format!(
                                "debate {dpid} belongs to run '{dprun}', not '{run_id}'"
                            ));
                        }
                        if dpagent != agent_id {
                            return Err(format!(
                                "only the debate opener ({dpagent}) can resolve debate {dpid}"
                            ));
                        }
                        if dpstatus != "pending" {
                            return Err(format!(
                                "debate {dpid} is not pending (status: {dpstatus})"
                            ));
                        }
                        let updated = self
                            .store
                            .update_proposal_status(dpid, "accepted")
                            .map_err(|e| format!("failed to resolve debate: {e}"))?;
                        if !updated {
                            return Err(format!(
                                "debate {dpid} status update failed — debate may no longer be pending"
                            ));
                        }
                        let resolution_proposal_id = format!(
                            "dr-{}-{}",
                            resolution.correlation_id,
                            chrono::Utc::now().timestamp_millis()
                        );
                        self.store
                            .create_proposal(
                                &resolution_proposal_id,
                                &resolution.correlation_id,
                                &resolution.run_id,
                                &resolution.node_id,
                                agent_id,
                                "debate_resolution",
                                &resolution.resolution,
                                &resolution.winning_position.clone().unwrap_or_default(),
                                None,
                                None,
                                None,
                            )
                            .map_err(|e| format!("failed to create debate resolution: {e}"))?;
                        let _ = self.store.append_audit(
                            "agent_step",
                            "debate.resolved",
                            &format!("agent_state/{agent_id}/{run_id}"),
                            &json!({
                                "run_id": run_id,
                                "agent_id": agent_id,
                                "proposal_id": dpid,
                                "resolution_proposal_id": resolution_proposal_id,
                                "correlation_id": resolution.correlation_id,
                                "has_winning_position": resolution.winning_position.is_some(),
                            }),
                        );
                        Ok(json!({"action":"resolve_debate","debate_id": dpid,
                                  "resolution_proposal_id": resolution_proposal_id})
                        .to_string())
                    }
                    None => Err(format!(
                        "debate proposal {} not found",
                        resolution.debate_id
                    )),
                }
            }
        }
    }
}

impl NodeExecutor for AgentStepExecutor {
    fn executor_type_name(&self) -> &str {
        "agent_step"
    }

    fn execute_node(&self, input: &NodeExecutionInput) -> NodeExecutionOutput {
        let start = std::time::Instant::now();

        if std::env::var(ACP_ENABLE_AGENT_RUNTIME).as_deref() != Ok("1") {
            return agent_step_fail("ACP_ENABLE_AGENT_RUNTIME is not set to 1", &start);
        }

        if std::env::var("ACP_AGENT_RUNTIME_KILL_SWITCH").as_deref() == Ok("1") {
            return agent_step_fail("agent runtime kill switch is active", &start);
        }

        let agent_id = match input.node_metadata.get("agent_id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => return agent_step_fail("missing agent_id in node_metadata", &start),
        };

        let agent_state = match self.store.get_agent_state(&agent_id, &input.run_id) {
            Ok(Some(s)) => s,
            Ok(None) => {
                return agent_step_fail(
                    &format!("AgentState not found for {agent_id}/{}", input.run_id),
                    &start,
                )
            }
            Err(e) => return agent_step_fail(&format!("failed to load AgentState: {e}"), &start),
        };

        let mailbox_count =
            match self
                .store
                .count_mailbox(Some(&agent_id), Some(&input.run_id), Some("pending"))
            {
                Ok(c) => c,
                Err(e) => return agent_step_fail(&format!("failed to count mailbox: {e}"), &start),
            };

        let memory_digest = load_memory_digest_from_agent_state(&agent_state);
        let memory_context = build_memory_context_for_node(&agent_state, 1200);
        let memory_state_read_bytes =
            estimate_memory_state_bytes(memory_digest.as_ref(), memory_context.as_ref());

        let context = AgentStepContext {
            agent_id: agent_id.clone(),
            run_id: input.run_id.clone(),
            node_id: input.node_id.clone(),
            workflow_id: input.workflow_id.clone(),
            agent_state: Some(agent_state.clone()),
            mailbox_pending_count: mailbox_count,
            memory_digest,
            memory_context,
            memory_state_read_bytes,
            node_metadata: input.node_metadata.clone(),
        };

        self.append_agent_step_audit_best_effort(
            "agent_step.start",
            &agent_id,
            &input.run_id,
            &json!({"agent_id": agent_id, "run_id": input.run_id}),
        );

        let action = match (self.decision_source)(&context) {
            Ok(a) => a,
            Err(e) => {
                self.append_agent_step_audit_best_effort(
                    "agent_step.decision_failed",
                    &agent_id,
                    &input.run_id,
                    &json!({"error": "decision_failed"}),
                );
                return agent_step_fail(&format!("decision failed: {e}"), &start);
            }
        };

        let descriptor = sanitized_action_descriptor(&action);
        self.append_agent_step_audit_best_effort(
            "agent_step.decision",
            &agent_id,
            &input.run_id,
            &descriptor,
        );

        match self.execute_action(
            &agent_id,
            &input.run_id,
            &input.node_id,
            &agent_state,
            mailbox_count,
            memory_state_read_bytes,
            &action,
        ) {
            Ok(result) => {
                self.append_agent_step_audit_best_effort(
                    "agent_step.completed",
                    &agent_id,
                    &input.run_id,
                    &json!({"action_type": "completed"}),
                );
                NodeExecutionOutput {
                    status: "completed".to_string(),
                    executor_type: "agent_step".to_string(),
                    output: Some(result),
                    error_domain: None,
                    error_message: None,
                    input_tokens: None,
                    output_tokens: None,
                    estimated_cost: None,
                    latency_ms: Some(start.elapsed().as_millis() as i64),
                }
            }
            Err(e) => {
                self.append_agent_step_audit_best_effort(
                    "agent_step.failed",
                    &agent_id,
                    &input.run_id,
                    &json!({"action_type": "failed", "error": "agent_step_error"}),
                );
                agent_step_fail(&e, &start)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_executor_succeeds() {
        let executor = NoopNodeExecutor;
        let input = NodeExecutionInput {
            node_id: "node-0001".to_string(),
            task_type: "test".to_string(),
            run_id: "run-0001".to_string(),
            workflow_id: "wf-0001".to_string(),
            node_metadata: json!({}),
        };
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "completed");
        assert_eq!(output.executor_type, "noop");
    }

    #[test]
    fn test_stub_executor_produces_output() {
        let executor = StubNodeExecutor::default();
        let input = NodeExecutionInput {
            node_id: "node-0002".to_string(),
            task_type: "analyze".to_string(),
            run_id: "run-0002".to_string(),
            workflow_id: "wf-0002".to_string(),
            node_metadata: json!({}),
        };
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "completed");
        assert_eq!(output.executor_type, "stub");
        assert!(output.output.unwrap().contains("node-0002"));
    }

    #[test]
    fn test_fail_executor_fails() {
        let executor = FailNodeExecutor::default();
        let input = NodeExecutionInput {
            node_id: "node-0003".to_string(),
            task_type: "test".to_string(),
            run_id: "run-0003".to_string(),
            workflow_id: "wf-0003".to_string(),
            node_metadata: json!({}),
        };
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "failed");
        assert_eq!(output.error_domain.unwrap(), "test_failure");
    }

    #[test]
    fn test_node_execution_output_serializes() {
        let output = NodeExecutionOutput {
            status: "completed".to_string(),
            executor_type: "noop".to_string(),
            output: Some("done".to_string()),
            error_domain: None,
            error_message: None,
            input_tokens: Some(10),
            output_tokens: Some(5),
            estimated_cost: Some(0.001),
            latency_ms: Some(100),
        };
        let value = output.to_value();
        assert_eq!(value["status"], "completed");
        assert_eq!(value["input_tokens"], 10);
    }

    #[test]
    fn test_command_echo_ok() {
        let executor = CommandNodeExecutor::default();
        let input = NodeExecutionInput {
            node_id: "node-cmd-001".to_string(),
            task_type: "command".to_string(),
            run_id: "run-001".to_string(),
            workflow_id: "wf-001".to_string(),
            node_metadata: json!({"command": "echo ok"}),
        };
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "completed");
        assert_eq!(output.executor_type, "command");
        assert!(output.output.unwrap().contains("ok"));
    }

    #[test]
    fn test_command_rejects_shell_injection() {
        let executor = CommandNodeExecutor::default();
        let input = NodeExecutionInput {
            node_id: "node-cmd-002".to_string(),
            task_type: "command".to_string(),
            run_id: "run-002".to_string(),
            workflow_id: "wf-002".to_string(),
            node_metadata: json!({"command": "echo ok; rm -rf x"}),
        };
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "failed");
        assert_eq!(output.error_domain.unwrap(), "command_not_allowed");
    }

    #[test]
    fn test_command_timeout_kills() {
        let executor = CommandNodeExecutor {
            timeout_ms: 200,
            allowed_commands: vec!["sleep".to_string()],
            allowed_binaries: vec!["sleep".to_string()],
            env_vars: Vec::new(),
        };
        let input = NodeExecutionInput {
            node_id: "node-cmd-003".to_string(),
            task_type: "command".to_string(),
            run_id: "run-003".to_string(),
            workflow_id: "wf-003".to_string(),
            node_metadata: json!({"command": "sleep 30"}),
        };
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "failed");
        assert_eq!(output.error_domain.unwrap(), "command_timeout");
    }

    #[test]
    fn test_command_nonzero_exit() {
        let executor = CommandNodeExecutor::default();
        let input = NodeExecutionInput {
            node_id: "node-cmd-004".to_string(),
            task_type: "command".to_string(),
            run_id: "run-004".to_string(),
            workflow_id: "wf-004".to_string(),
            node_metadata: json!({"command": "false"}),
        };
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "failed");
        assert_eq!(output.error_domain.unwrap(), "command_exit_nonzero");
    }

    #[test]
    fn test_command_rejects_unclean_workspace_path() {
        let executor = CommandNodeExecutor::default();
        let input = NodeExecutionInput {
            node_id: "node-cmd-005".to_string(),
            task_type: "command".to_string(),
            run_id: "run-005".to_string(),
            workflow_id: "wf-005".to_string(),
            node_metadata: json!({"command": "echo ok", "workspace_path": "/tmp/workspace/../escape"}),
        };
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "failed");
        assert_eq!(output.error_domain.unwrap(), "workspace_escape");
    }

    #[test]
    fn test_command_output_is_truncated() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("large.txt");
        std::fs::write(&file, "x".repeat(80_000)).unwrap();
        let executor = CommandNodeExecutor::default();
        let input = NodeExecutionInput {
            node_id: "node-cmd-006".to_string(),
            task_type: "command".to_string(),
            run_id: "run-006".to_string(),
            workflow_id: "wf-006".to_string(),
            node_metadata: json!({"command": format!("cat {}", file.to_string_lossy())}),
        };
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "completed");
        let rendered = output.output.unwrap();
        assert!(rendered.len() < 70_000);
        assert!(rendered.contains("[truncated"));
    }

    // ── AR-2 agent step tests ────────────────────────────────────────────

    // Serializes env-var access. All tests that set/remove ACP_ENABLE_AGENT_RUNTIME
    // or ACP_AGENT_RUNTIME_KILL_SWITCH must hold this lock.
    static AGENT_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn ar2_store() -> LocalProductStore {
        LocalProductStore::new(":memory:").expect("failed to create in-memory store")
    }

    fn stub_decision(action: AgentAction) -> AgentDecisionFn {
        Box::new(move |_| Ok(action.clone()))
    }

    fn agent_step_input(agent_id: &str, run_id: &str) -> NodeExecutionInput {
        NodeExecutionInput {
            node_id: "agent-node-001".to_string(),
            task_type: "agent_step".to_string(),
            run_id: run_id.to_string(),
            workflow_id: "wf-ar2-001".to_string(),
            node_metadata: json!({"agent_id": agent_id}),
        }
    }

    fn create_test_agent(store: &LocalProductStore, agent_id: &str, run_id: &str) {
        store
            .create_agent_state(
                agent_id,
                run_id,
                "implementer",
                &["code".to_string()],
                Some("test objective"),
                "idle",
                &json!({}),
            )
            .expect("create agent state");
    }

    #[test]
    fn test_agent_step_env_gates() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-env", "run-ar2-env");
        let executor = AgentStepExecutor::new(store, stub_decision(AgentAction::Complete));
        let input = agent_step_input("agent-env", "run-ar2-env");

        std::env::remove_var("ACP_ENABLE_AGENT_RUNTIME");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let output = executor.execute_node(&input);
        assert_eq!(output.status, "failed");
        assert!(output
            .error_message
            .as_ref()
            .unwrap()
            .contains("ACP_ENABLE_AGENT_RUNTIME"));

        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "completed");

        std::env::set_var("ACP_AGENT_RUNTIME_KILL_SWITCH", "1");
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "failed");

        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
    }

    #[test]
    fn test_agent_step_missing_state_fails_closed() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let executor = AgentStepExecutor::new(store, stub_decision(AgentAction::Complete));
        let input = agent_step_input("agent-nonexistent", "run-missing");

        let output = executor.execute_node(&input);
        assert_eq!(output.status, "failed");
        assert!(output.error_message.unwrap().contains("AgentState"));
    }

    #[test]
    fn test_agent_step_missing_agent_id_fails_closed() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let executor = AgentStepExecutor::new(store, stub_decision(AgentAction::Complete));
        let input = NodeExecutionInput {
            node_id: "agent-node-no-id".to_string(),
            task_type: "agent_step".to_string(),
            run_id: "run-ar2-noid".to_string(),
            workflow_id: "wf-ar2-noid".to_string(),
            node_metadata: json!({}),
        };

        let output = executor.execute_node(&input);
        assert_eq!(output.status, "failed");
        assert!(output.error_message.unwrap().contains("agent_id"));
    }

    #[test]
    fn test_agent_step_success_complete() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-c", "run-ar2-c");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let executor = AgentStepExecutor::new(store.clone(), stub_decision(AgentAction::Complete));
        let input = agent_step_input("agent-c", "run-ar2-c");

        let output = executor.execute_node(&input);
        assert_eq!(output.status, "completed");
        assert_eq!(output.executor_type, "agent_step");
        assert!(output.output.unwrap().contains("complete"));

        let state = store
            .get_agent_state("agent-c", "run-ar2-c")
            .expect("get state")
            .unwrap();
        assert_eq!(state.status, "completed");
    }

    #[test]
    fn test_agent_step_unsupported_action_fails_closed() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-unsup", "run-ar2-unsup");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let executor = AgentStepExecutor::new(
            store,
            stub_decision(AgentAction::Unsupported("bad_action".to_string())),
        );
        let input = agent_step_input("agent-unsup", "run-ar2-unsup");

        let output = executor.execute_node(&input);
        assert_eq!(output.status, "failed");
        assert!(output.error_message.unwrap().contains("unsupported action"));
    }

    #[test]
    fn test_agent_step_scratchpad_update() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-scr", "run-ar2-scr");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let executor = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::UpdateScratchpadSummary(
                "progress: 50%".to_string(),
            )),
        );
        let input = agent_step_input("agent-scr", "run-ar2-scr");

        let output = executor.execute_node(&input);
        assert_eq!(output.status, "completed");

        let state = store
            .get_agent_state("agent-scr", "run-ar2-scr")
            .expect("get state")
            .unwrap();
        assert_eq!(state.scratchpad_summary, Some("progress: 50%".to_string()));
    }

    #[test]
    fn test_agent_step_observe_attaches_bounded_memory_context() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-mem", "run-ar2-mem");
        store
            .update_agent_state(
                "agent-mem",
                "run-ar2-mem",
                None,
                None,
                None,
                Some(&json!({
                    "memory_digest": {
                        "source_refs": [
                            "agent_state:run-mem:agent-mem:scratchpad_summary",
                            "/home/igzela/private/repo.rs"
                        ],
                        "expiry_policy": "forever",
                        "conflict_resolution": "append_raw",
                        "summary": "bounded progress with sk-proj-secret-token"
                    }
                })),
            )
            .expect("seed memory metadata");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let decision: AgentDecisionFn = Box::new(|context| {
            let digest = context.memory_digest.as_ref().expect("memory digest");
            assert_eq!(
                digest["source_refs"],
                json!(["agent_state:run-mem:agent-mem:scratchpad_summary"])
            );
            assert!(!digest.to_string().contains("/home/igzela"));
            assert!(!digest.to_string().contains("sk-proj-secret-token"));

            let memory_context = context.memory_context.as_ref().expect("memory context");
            assert_eq!(memory_context["injection_surface"], "node_metadata_only");
            assert!(memory_context["included_tokens"].as_i64().unwrap() > 0);
            assert!(context.memory_state_read_bytes > 0);
            Ok(AgentAction::Wait)
        });
        let executor = AgentStepExecutor::new(store, decision);
        let input = agent_step_input("agent-mem", "run-ar2-mem");

        let output = executor.execute_node(&input);
        assert_eq!(output.status, "completed");
    }

    #[test]
    fn test_agent_step_scratchpad_update_synchronizes_memory_digest() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-sync", "run-ar2-sync");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let executor = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::UpdateScratchpadSummary(
                "progress from /home/igzela/private.txt using sk-test-secret".to_string(),
            )),
        );
        let input = agent_step_input("agent-sync", "run-ar2-sync");

        let output = executor.execute_node(&input);
        assert_eq!(output.status, "completed");
        let result: Value = serde_json::from_str(&output.output.unwrap()).unwrap();
        assert!(result["state_write_bytes"].as_i64().unwrap() > 0);

        let state = store
            .get_agent_state("agent-sync", "run-ar2-sync")
            .expect("get state")
            .unwrap();
        assert_eq!(
            state.scratchpad_summary,
            Some("progress from [redacted-path] using ***".to_string())
        );
        let digest = state
            .metadata
            .get("memory_digest")
            .expect("memory digest should persist");
        assert_eq!(digest["summary"], "progress from [redacted-path] using ***");
        assert_eq!(
            digest["source_refs"],
            json!(["agent_state:run-ar2-sync:agent-sync:scratchpad_summary"])
        );
    }

    #[test]
    fn test_agent_step_mailbox_read_and_ack() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-mail", "run-ar2-mail");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        store
            .send_message(
                "msg-ar2-001",
                "agent-other",
                "agent-mail",
                "task_assign",
                Some("build feature Y"),
                None,
                Some("run-ar2-mail"),
                Some("node-001"),
                None,
                &json!({}),
            )
            .expect("send message");

        let executor =
            AgentStepExecutor::new(store.clone(), stub_decision(AgentAction::ReadMailbox));
        let input = agent_step_input("agent-mail", "run-ar2-mail");
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "completed");
        let out = output.output.unwrap();
        assert!(out.contains("msg-ar2-001"));

        let executor = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::AckMessage("msg-ar2-001".to_string())),
        );
        let input = agent_step_input("agent-mail", "run-ar2-mail");
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "completed");

        let msg = store
            .read_message("msg-ar2-001")
            .expect("read message")
            .unwrap();
        assert_eq!(msg.status, "acked");
    }

    #[test]
    fn test_agent_step_audit_events_emitted() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-audit", "run-ar2-audit");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let executor = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::EmitNote("test note".to_string())),
        );
        let input = agent_step_input("agent-audit", "run-ar2-audit");

        let output = executor.execute_node(&input);
        assert_eq!(output.status, "completed");

        let events = store.audit_events(100).expect("audit events");
        let step_events: Vec<_> = events
            .iter()
            .filter(|e| {
                e.get("action")
                    .and_then(|a| a.as_str())
                    .map(|a| a.starts_with("agent_step."))
                    .unwrap_or(false)
            })
            .collect();
        assert!(
            step_events.len() >= 3,
            "expected >=3 agent_step audit events, got {}",
            step_events.len()
        );
    }

    #[test]
    fn test_agent_step_wait_action() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-wait", "run-ar2-wait");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let executor = AgentStepExecutor::new(store, stub_decision(AgentAction::Wait));
        let input = agent_step_input("agent-wait", "run-ar2-wait");

        let output = executor.execute_node(&input);
        assert_eq!(output.status, "completed");
        assert!(output.output.unwrap().contains("wait"));
    }

    #[test]
    fn test_agent_step_no_provider_or_cli_called() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        // This test verifies that agent_step never calls provider/CLI paths.
        // The executor uses only store methods and the decision stub; no
        // provider adapter or CLI subprocess is involved in the agent_step path.
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-noprov", "run-ar2-noprov");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        // All allowed actions must complete without provider/CLI calls
        for action in [
            AgentAction::Wait,
            AgentAction::Complete,
            AgentAction::UpdateScratchpadSummary("test".to_string()),
            AgentAction::ReadMailbox,
            AgentAction::EmitNote("note".to_string()),
            AgentAction::RecordObservation("obs".to_string()),
        ] {
            let store = Arc::new(ar2_store());
            create_test_agent(&store, "agent-np", "run-ar2-np");
            let executor = AgentStepExecutor::new(store, stub_decision(action.clone()));
            let input = agent_step_input("agent-np", "run-ar2-np");
            let output = executor.execute_node(&input);
            assert_eq!(
                output.status, "completed",
                "action {action:?} should not need provider/CLI"
            );
        }
    }

    // ── AR-3 tests ───────────────────────────────────────────────────────────

    fn stub_child_task_proposal(agent_id: &str, run_id: &str) -> ChildTaskProposal {
        ChildTaskProposal {
            schema_version: "child_task_proposal.v1".to_string(),
            correlation_id: "corr-ar3-001".to_string(),
            objective: "implement feature X".to_string(),
            context_summary: "context for feature X".to_string(),
            proposed_node_id: Some("child-node-001".to_string()),
            proposed_edge_id: Some("edge-001".to_string()),
            parent_node_id: "agent-node-001".to_string(),
            run_id: run_id.to_string(),
            agent_id: agent_id.to_string(),
        }
    }

    fn stub_handoff_request(agent_id: &str, run_id: &str) -> HandoffRequest {
        HandoffRequest {
            schema_version: "handoff_request.v1".to_string(),
            correlation_id: "corr-handoff-001".to_string(),
            objective: "review my work".to_string(),
            context_summary: "context for review".to_string(),
            target_agent_id: "target-agent".to_string(),
            source_agent_id: agent_id.to_string(),
            run_id: run_id.to_string(),
            node_id: "agent-node-001".to_string(),
        }
    }

    #[test]
    fn test_agent_step_propose_child_task() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-ar3a", "run-ar3a");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let proposal = stub_child_task_proposal("agent-ar3a", "run-ar3a");
        let executor = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::ProposeChildTask(proposal)),
        );
        let input = agent_step_input("agent-ar3a", "run-ar3a");

        let output = executor.execute_node(&input);
        assert_eq!(output.status, "completed");
        let result = output.output.unwrap();
        assert!(result.contains("propose_child_task"));
        assert!(result.contains("corr-ar3-001"));

        let proposals = store
            .list_proposals_by_run("run-ar3a", 100, 0)
            .expect("list");
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0]["status"], "pending");
        assert_eq!(proposals[0]["correlation_id"], "corr-ar3-001");
        assert_eq!(proposals[0]["proposal_type"], "child_task");
        assert_eq!(proposals[0]["agent_id"], "agent-ar3a");
    }

    #[test]
    fn test_agent_step_proposal_links_ids() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-ar3b", "run-ar3b");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let proposal = ChildTaskProposal {
            schema_version: "child_task_proposal.v1".to_string(),
            correlation_id: "corr-unique-999".to_string(),
            objective: "test".to_string(),
            context_summary: "test context".to_string(),
            proposed_node_id: Some("child-node-999".to_string()),
            proposed_edge_id: Some("edge-999".to_string()),
            parent_node_id: "parent-node-999".to_string(),
            run_id: "run-ar3b".to_string(),
            agent_id: "agent-ar3b".to_string(),
        };
        let executor = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::ProposeChildTask(proposal)),
        );
        let input = agent_step_input("agent-ar3b", "run-ar3b");

        let output = executor.execute_node(&input);
        assert_eq!(output.status, "completed");

        let proposals = store
            .list_proposals_by_run("run-ar3b", 100, 0)
            .expect("list");
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0]["correlation_id"], "corr-unique-999");
        assert_eq!(proposals[0]["run_id"], "run-ar3b");
        assert_eq!(proposals[0]["agent_id"], "agent-ar3b");
        assert_eq!(proposals[0]["parent_node_id"], "parent-node-999");
    }

    #[test]
    fn test_agent_step_invalid_proposal_fails_closed() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-ar3c", "run-ar3c");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let big_objective = "x".repeat(5000);
        let proposal = ChildTaskProposal {
            schema_version: "child_task_proposal.v1".to_string(),
            correlation_id: "corr-big".to_string(),
            objective: big_objective,
            context_summary: "small".to_string(),
            proposed_node_id: None,
            proposed_edge_id: None,
            parent_node_id: "pn-001".to_string(),
            run_id: "run-ar3c".to_string(),
            agent_id: "agent-ar3c".to_string(),
        };
        let executor = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::ProposeChildTask(proposal)),
        );
        let input = agent_step_input("agent-ar3c", "run-ar3c");
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "failed");
        assert!(output.error_message.unwrap().contains("byte cap"));

        let executor2 = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::Unsupported("bad_ar3".to_string())),
        );
        let input2 = agent_step_input("agent-ar3c", "run-ar3c");
        let output2 = executor2.execute_node(&input2);
        assert_eq!(output2.status, "failed");
    }

    #[test]
    fn test_agent_step_handoff_request() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-ar3d", "run-ar3d");
        create_test_agent(&store, "target-agent", "run-ar3d");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let request = stub_handoff_request("agent-ar3d", "run-ar3d");
        let executor = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::RequestHandoff(request)),
        );
        let input = agent_step_input("agent-ar3d", "run-ar3d");

        let output = executor.execute_node(&input);
        assert_eq!(output.status, "completed");
        let result = output.output.unwrap();
        assert!(result.contains("request_handoff"));

        let proposals = store
            .list_proposals_by_run("run-ar3d", 100, 0)
            .expect("list");
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0]["proposal_type"], "handoff");
        assert_eq!(proposals[0]["status"], "pending");

        let msgs = store
            .list_mailbox(Some("target-agent"), Some("run-ar3d"), None, None, 100, 0)
            .expect("list mailbox");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].message_type, "handoff_request");
    }

    #[test]
    fn test_agent_step_accept_reject_handoff() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-src", "run-ar3e");
        create_test_agent(&store, "agent-dst", "run-ar3e");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let request = HandoffRequest {
            schema_version: "handoff_request.v1".to_string(),
            correlation_id: "corr-handoff-accept".to_string(),
            objective: "please review".to_string(),
            context_summary: "review context".to_string(),
            target_agent_id: "agent-dst".to_string(),
            source_agent_id: "agent-src".to_string(),
            run_id: "run-ar3e".to_string(),
            node_id: "agent-node-001".to_string(),
        };
        let req_exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::RequestHandoff(request)),
        );
        let req_out = req_exec.execute_node(&agent_step_input("agent-src", "run-ar3e"));
        assert_eq!(req_out.status, "completed");

        let accept_exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::AcceptHandoff(
                "corr-handoff-accept".to_string(),
            )),
        );
        let accept_out = accept_exec.execute_node(&agent_step_input("agent-dst", "run-ar3e"));
        assert_eq!(accept_out.status, "completed");
        assert!(accept_out.output.unwrap().contains("accept_handoff"));

        let proposals = store
            .list_proposals_by_run("run-ar3e", 100, 0)
            .expect("list");
        let handoff_proposals: Vec<_> = proposals
            .iter()
            .filter(|p| p["proposal_type"] == "handoff")
            .collect();
        assert_eq!(handoff_proposals.len(), 1);
        assert_eq!(handoff_proposals[0]["status"], "accepted");

        let request2 = HandoffRequest {
            schema_version: "handoff_request.v1".to_string(),
            correlation_id: "corr-handoff-reject".to_string(),
            objective: "please review 2".to_string(),
            context_summary: "review context 2".to_string(),
            target_agent_id: "agent-dst".to_string(),
            source_agent_id: "agent-src".to_string(),
            run_id: "run-ar3e".to_string(),
            node_id: "agent-node-001".to_string(),
        };
        let req_exec2 = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::RequestHandoff(request2)),
        );
        let _ = req_exec2.execute_node(&agent_step_input("agent-src", "run-ar3e"));

        let reject_exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::RejectHandoff(
                "corr-handoff-reject".to_string(),
            )),
        );
        let reject_out = reject_exec.execute_node(&agent_step_input("agent-dst", "run-ar3e"));
        assert_eq!(reject_out.status, "completed");
        assert!(reject_out.output.unwrap().contains("reject_handoff"));

        let proposals2 = store
            .list_proposals_by_run("run-ar3e", 100, 0)
            .expect("list");
        let rejected: Vec<_> = proposals2
            .iter()
            .filter(|p| p["correlation_id"] == "corr-handoff-reject")
            .collect();
        assert_eq!(rejected.len(), 1);
        assert_eq!(rejected[0]["status"], "rejected");
    }

    #[test]
    fn test_agent_step_kill_switch_blocks_ar3() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-ar3k", "run-ar3k");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::set_var("ACP_AGENT_RUNTIME_KILL_SWITCH", "1");

        let proposal = stub_child_task_proposal("agent-ar3k", "run-ar3k");
        let executor = AgentStepExecutor::new(
            store,
            stub_decision(AgentAction::ProposeChildTask(proposal)),
        );
        let input = agent_step_input("agent-ar3k", "run-ar3k");
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "failed");
        assert!(output.error_message.unwrap().contains("kill switch"));

        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
    }

    #[test]
    fn test_agent_step_disabled_runtime_blocks_ar3() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-ar3d", "run-ar3d");
        std::env::remove_var("ACP_ENABLE_AGENT_RUNTIME");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let proposal = stub_child_task_proposal("agent-ar3d", "run-ar3d");
        let executor = AgentStepExecutor::new(
            store,
            stub_decision(AgentAction::ProposeChildTask(proposal)),
        );
        let input = agent_step_input("agent-ar3d", "run-ar3d");
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "failed");
        assert!(output
            .error_message
            .unwrap()
            .contains("ACP_ENABLE_AGENT_RUNTIME"));

        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");
    }

    #[test]
    fn test_agent_step_ar3_redaction_and_size_caps() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-ar3r", "run-ar3r");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let secret_objective = "my password is AKIA1234ABCDEF".to_string();
        let proposal = ChildTaskProposal {
            schema_version: "child_task_proposal.v1".to_string(),
            correlation_id: "corr-secret".to_string(),
            objective: secret_objective,
            context_summary: "normal context".to_string(),
            proposed_node_id: None,
            proposed_edge_id: None,
            parent_node_id: "pn-001".to_string(),
            run_id: "run-ar3r".to_string(),
            agent_id: "agent-ar3r".to_string(),
        };
        let executor = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::ProposeChildTask(proposal)),
        );
        let input = agent_step_input("agent-ar3r", "run-ar3r");
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "completed");

        let proposals = store
            .list_proposals_by_run("run-ar3r", 100, 0)
            .expect("list");
        assert_eq!(proposals.len(), 1);
        assert!(!proposals[0]["objective"].as_str().unwrap().is_empty());
    }

    #[test]
    fn test_agent_step_ar3_audit_events_emitted() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-ar3a", "run-ar3a");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let proposal = stub_child_task_proposal("agent-ar3a", "run-ar3a");
        let executor = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::ProposeChildTask(proposal)),
        );
        let input = agent_step_input("agent-ar3a", "run-ar3a");
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "completed");

        let events = store.audit_events(100).expect("audit events");
        let ar3_events: Vec<_> = events
            .iter()
            .filter(|e| {
                e.get("action")
                    .and_then(|a| a.as_str())
                    .map(|a| a.starts_with("agent_step.") || a.contains("agent_proposal"))
                    .unwrap_or(false)
            })
            .collect();
        assert!(
            ar3_events.len() >= 4,
            "expected >=4 AR-3 audit events, got {}",
            ar3_events.len()
        );
    }

    #[test]
    fn test_agent_step_ar3_no_provider_or_cli_called() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-ar3np", "run-ar3np");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let proposal = stub_child_task_proposal("agent-ar3np", "run-ar3np");
        let actions: Vec<AgentAction> = vec![AgentAction::ProposeChildTask(proposal.clone())];
        for action in actions {
            let s = Arc::new(ar2_store());
            create_test_agent(&s, "ag-ar3np", "run-ar3np");
            let exec = AgentStepExecutor::new(s, stub_decision(action.clone()));
            let inp = agent_step_input("ag-ar3np", "run-ar3np");
            let out = exec.execute_node(&inp);
            assert_eq!(
                out.status, "completed",
                "action {action:?} should not need provider/CLI"
            );
        }
    }

    #[test]
    fn test_agent_step_cancel_proposal() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-ar3c", "run-ar3c");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let proposal = stub_child_task_proposal("agent-ar3c", "run-ar3c");
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::ProposeChildTask(proposal)),
        );
        let out = exec.execute_node(&agent_step_input("agent-ar3c", "run-ar3c"));
        assert_eq!(out.status, "completed");

        let cancel_exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::CancelProposal("corr-ar3-001".to_string())),
        );
        let cancel_out = cancel_exec.execute_node(&agent_step_input("agent-ar3c", "run-ar3c"));
        assert_eq!(cancel_out.status, "completed");
        assert!(cancel_out.output.unwrap().contains("cancel_proposal"));

        let proposals = store
            .list_proposals_by_run("run-ar3c", 100, 0)
            .expect("list");
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0]["status"], "cancelled");
    }

    #[test]
    fn test_agent_step_self_handoff_fails_closed() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-ar3s", "run-ar3s");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let request = HandoffRequest {
            schema_version: "handoff_request.v1".to_string(),
            correlation_id: "corr-self".to_string(),
            objective: "self handoff".to_string(),
            context_summary: "context".to_string(),
            target_agent_id: "agent-ar3s".to_string(),
            source_agent_id: "agent-ar3s".to_string(),
            run_id: "run-ar3s".to_string(),
            node_id: "node-001".to_string(),
        };
        let exec =
            AgentStepExecutor::new(store, stub_decision(AgentAction::RequestHandoff(request)));
        let out = exec.execute_node(&agent_step_input("agent-ar3s", "run-ar3s"));
        assert_eq!(out.status, "failed");
        assert!(out
            .error_message
            .unwrap()
            .contains("target agent must be different"));
    }

    #[test]
    fn test_agent_step_accept_nonexistent_handoff_fails_closed() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-ar3x", "run-ar3x");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let exec = AgentStepExecutor::new(
            store,
            stub_decision(AgentAction::AcceptHandoff(
                "nonexistent-correlation".to_string(),
            )),
        );
        let out = exec.execute_node(&agent_step_input("agent-ar3x", "run-ar3x"));
        assert_eq!(out.status, "failed");
    }

    #[test]
    fn test_agent_step_wrong_target_cannot_accept_handoff() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "src-ar3wt", "run-ar3wt");
        create_test_agent(&store, "dst-ar3wt", "run-ar3wt");
        create_test_agent(&store, "wrong-ar3wt", "run-ar3wt");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let request = HandoffRequest {
            schema_version: "handoff_request.v1".to_string(),
            correlation_id: "corr-wt".to_string(),
            objective: "review".to_string(),
            context_summary: "ctx".to_string(),
            target_agent_id: "dst-ar3wt".to_string(),
            source_agent_id: "src-ar3wt".to_string(),
            run_id: "run-ar3wt".to_string(),
            node_id: "node-001".to_string(),
        };
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::RequestHandoff(request)),
        );
        let out = exec.execute_node(&agent_step_input("src-ar3wt", "run-ar3wt"));
        assert_eq!(out.status, "completed");

        // wrong agent (not target) tries to accept
        let wrong = AgentStepExecutor::new(
            store,
            stub_decision(AgentAction::AcceptHandoff("corr-wt".to_string())),
        );
        let out2 = wrong.execute_node(&agent_step_input("wrong-ar3wt", "run-ar3wt"));
        assert_eq!(out2.status, "failed");
    }

    #[test]
    fn test_agent_step_source_cannot_accept_own_handoff() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "src-sa", "run-sa");
        create_test_agent(&store, "dst-sa", "run-sa");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let request = HandoffRequest {
            schema_version: "handoff_request.v1".to_string(),
            correlation_id: "corr-sa".to_string(),
            objective: "review".to_string(),
            context_summary: "ctx".to_string(),
            target_agent_id: "dst-sa".to_string(),
            source_agent_id: "src-sa".to_string(),
            run_id: "run-sa".to_string(),
            node_id: "node-001".to_string(),
        };
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::RequestHandoff(request)),
        );
        let out = exec.execute_node(&agent_step_input("src-sa", "run-sa"));
        assert_eq!(out.status, "completed");

        // source agent tries to accept its own outgoing handoff
        let src = AgentStepExecutor::new(
            store,
            stub_decision(AgentAction::AcceptHandoff("corr-sa".to_string())),
        );
        let out2 = src.execute_node(&agent_step_input("src-sa", "run-sa"));
        assert_eq!(out2.status, "failed");
    }

    #[test]
    fn test_agent_step_target_cannot_cancel_owners_proposal() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "owner-ar3tc", "run-ar3tc");
        create_test_agent(&store, "target-ar3tc", "run-ar3tc");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let request = HandoffRequest {
            schema_version: "handoff_request.v1".to_string(),
            correlation_id: "corr-tc".to_string(),
            objective: "review".to_string(),
            context_summary: "ctx".to_string(),
            target_agent_id: "target-ar3tc".to_string(),
            source_agent_id: "owner-ar3tc".to_string(),
            run_id: "run-ar3tc".to_string(),
            node_id: "node-001".to_string(),
        };
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::RequestHandoff(request)),
        );
        let out = exec.execute_node(&agent_step_input("owner-ar3tc", "run-ar3tc"));
        assert_eq!(out.status, "completed");

        // target tries to cancel the owner's proposal
        let target = AgentStepExecutor::new(
            store,
            stub_decision(AgentAction::CancelProposal("corr-tc".to_string())),
        );
        let out2 = target.execute_node(&agent_step_input("target-ar3tc", "run-ar3tc"));
        assert_eq!(out2.status, "failed");
    }

    #[test]
    fn test_agent_step_owner_can_cancel_own_child_task() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "oc-ar3", "run-oc");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let proposal = stub_child_task_proposal("oc-ar3", "run-oc");
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::ProposeChildTask(proposal)),
        );
        let out = exec.execute_node(&agent_step_input("oc-ar3", "run-oc"));
        assert_eq!(out.status, "completed");

        let cancel = AgentStepExecutor::new(
            store,
            stub_decision(AgentAction::CancelProposal("corr-ar3-001".to_string())),
        );
        let out2 = cancel.execute_node(&agent_step_input("oc-ar3", "run-oc"));
        assert_eq!(out2.status, "completed");
    }

    #[test]
    fn test_cross_agent_ack_rejected() {
        let store = Arc::new(ar2_store());
        let src = store
            .send_message(
                "msg-cross-ack",
                "agent-a",
                "agent-b",
                "task_assign",
                Some("secret info"),
                None,
                Some("run-cross"),
                None,
                None,
                &json!({}),
            )
            .expect("send");
        assert_eq!(src.to_agent_id, "agent-b");

        // agent-a (not the target) should not be able to ack
        let result = store.ack_message_for_agent("msg-cross-ack", "agent-a", "run-cross");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not the target"));

        // agent-b (the target) should be able to ack
        let ok = store
            .ack_message_for_agent("msg-cross-ack", "agent-b", "run-cross")
            .expect("ack");
        assert!(ok.is_some());
    }

    #[test]
    fn test_create_proposal_rejects_invalid_type() {
        let store = Arc::new(ar2_store());
        let err = store.create_proposal(
            "prop-badtype",
            "corr-badtype",
            "run-bt",
            "pn-001",
            "agent-bt",
            "invalid_type",
            "objective",
            "context",
            None,
            None,
            None,
        );
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("invalid proposal_type"));
    }

    #[test]
    fn test_update_proposal_status_rejects_invalid_status() {
        let store = Arc::new(ar2_store());
        store
            .create_proposal(
                "prop-badstatus",
                "corr-badstatus",
                "run-bs",
                "pn-001",
                "agent-bs",
                "child_task",
                "objective",
                "context",
                None,
                None,
                None,
            )
            .expect("create");
        let err = store.update_proposal_status("prop-badstatus", "invalid_status");
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("invalid status"));
    }

    #[test]
    fn test_ack_message_wrong_run_rejected() {
        let store = Arc::new(ar2_store());
        store
            .send_message(
                "msg-wr-run",
                "agent-a",
                "agent-b",
                "task_assign",
                Some("hello"),
                None,
                Some("run-42"),
                None,
                None,
                &json!({}),
            )
            .expect("send");

        // same agent, wrong run
        let err = store.ack_message_for_agent("msg-wr-run", "agent-b", "run-99");
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("run_id"));
    }

    #[test]
    fn test_ack_message_missing_run_rejected() {
        let store = Arc::new(ar2_store());
        store
            .send_message(
                "msg-nr",
                "agent-a",
                "agent-b",
                "task_assign",
                Some("hello"),
                None,
                None, // no run_id
                None,
                None,
                &json!({}),
            )
            .expect("send");

        // correct agent but no run_id on message
        let err = store.ack_message_for_agent("msg-nr", "agent-b", "run-1");
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("no run_id"));
    }

    #[test]
    fn test_ack_message_correct_agent_and_run() {
        let store = Arc::new(ar2_store());
        store
            .send_message(
                "msg-correct",
                "agent-a",
                "agent-b",
                "task_assign",
                Some("hello"),
                None,
                Some("run-1"),
                None,
                None,
                &json!({}),
            )
            .expect("send");

        let ok = store
            .ack_message_for_agent("msg-correct", "agent-b", "run-1")
            .expect("ack");
        assert!(ok.is_some());
    }

    // ── AR-5 tests ──────────────────────────────────────────────────────────

    fn stub_review_request(_requester: &str, target: &str, run_id: &str) -> ReviewRequest {
        ReviewRequest {
            schema_version: "review_request.v1".to_string(),
            correlation_id: "corr-review-001".to_string(),
            subject_summary: "review my implementation".to_string(),
            rationale_summary: "needs a second pair of eyes".to_string(),
            target_agent_id: target.to_string(),
            run_id: run_id.to_string(),
            node_id: "agent-node-001".to_string(),
            blocking: true,
        }
    }

    fn stub_debate_request(_opener: &str, run_id: &str) -> DebateRequest {
        DebateRequest {
            schema_version: "debate_request.v1".to_string(),
            correlation_id: "corr-debate-001".to_string(),
            subject_summary: "architecture approach A vs B".to_string(),
            participant_agent_ids: vec!["p1".to_string(), "p2".to_string()],
            max_rounds: 3,
            run_id: run_id.to_string(),
            node_id: "agent-node-001".to_string(),
        }
    }

    #[test]
    fn test_ar5_valid_review_request_creation() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-req", "run-ar5-req");
        create_test_agent(&store, "agent-tgt", "run-ar5-req");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let request = stub_review_request("agent-req", "agent-tgt", "run-ar5-req");
        let executor = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::RequestReview(request)),
        );
        let output = executor.execute_node(&agent_step_input("agent-req", "run-ar5-req"));
        assert_eq!(output.status, "completed");
        let result = output.output.unwrap();
        assert!(result.contains("request_review"));

        let proposals = store
            .list_proposals_by_run("run-ar5-req", 100, 0)
            .expect("list");
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0]["proposal_type"], "review_request");
        assert_eq!(proposals[0]["status"], "pending");
        assert_eq!(proposals[0]["target_agent_id"], "agent-tgt");
    }

    #[test]
    fn test_ar5_non_target_agent_cannot_submit_review_verdict() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-req", "run-ar5-v");
        create_test_agent(&store, "agent-tgt", "run-ar5-v");
        create_test_agent(&store, "agent-wrong", "run-ar5-v");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let request = stub_review_request("agent-req", "agent-tgt", "run-ar5-v");
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::RequestReview(request)),
        );
        let out = exec.execute_node(&agent_step_input("agent-req", "run-ar5-v"));
        assert_eq!(out.status, "completed");

        let proposals = store
            .list_proposals_by_run("run-ar5-v", 100, 0)
            .expect("list");
        let review_pid = proposals[0]["proposal_id"].as_str().unwrap().to_string();

        let verdict = ReviewVerdict {
            schema_version: "review_verdict.v1".to_string(),
            correlation_id: "corr-review-001".to_string(),
            review_request_id: review_pid,
            verdict: "accepted".to_string(),
            rationale_summary: "looks good".to_string(),
            run_id: "run-ar5-v".to_string(),
            node_id: "agent-node-001".to_string(),
            blocking: true,
        };
        let wrong_exec = AgentStepExecutor::new(
            store,
            stub_decision(AgentAction::SubmitReviewVerdict(verdict)),
        );
        let out2 = wrong_exec.execute_node(&agent_step_input("agent-wrong", "run-ar5-v"));
        assert_eq!(out2.status, "failed");
        assert!(out2
            .error_message
            .unwrap()
            .contains("not the target reviewer"));
    }

    #[test]
    fn test_ar5_wrong_run_id_cannot_submit_verdict() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-req", "run-ar5-wr");
        create_test_agent(&store, "agent-tgt", "run-ar5-wr");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let request = stub_review_request("agent-req", "agent-tgt", "run-ar5-wr");
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::RequestReview(request)),
        );
        let out = exec.execute_node(&agent_step_input("agent-req", "run-ar5-wr"));
        assert_eq!(out.status, "completed");

        let proposals = store
            .list_proposals_by_run("run-ar5-wr", 100, 0)
            .expect("list");
        let review_pid = proposals[0]["proposal_id"].as_str().unwrap().to_string();

        let verdict = ReviewVerdict {
            schema_version: "review_verdict.v1".to_string(),
            correlation_id: "corr-review-001".to_string(),
            review_request_id: review_pid,
            verdict: "accepted".to_string(),
            rationale_summary: "looks good".to_string(),
            run_id: "run-ar5-wrong-run".to_string(),
            node_id: "agent-node-001".to_string(),
            blocking: true,
        };
        let exec2 = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::SubmitReviewVerdict(verdict)),
        );
        let out2 = exec2.execute_node(&agent_step_input("agent-tgt", "run-ar5-wrong-run"));
        assert_eq!(out2.status, "failed");
        // Verify no verdict proposal was created — only the original review_request
        let proposals_after = store
            .list_proposals_by_run("run-ar5-wr", 100, 0)
            .expect("list");
        let verdict_count = proposals_after
            .iter()
            .filter(|p| p["proposal_type"] == "review_verdict")
            .count();
        assert_eq!(
            verdict_count, 0,
            "no verdict proposal should be created on run_id mismatch"
        );
    }

    #[test]
    fn test_ar5_requester_can_cancel_pending_review() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-req", "run-ar5-cancel");
        create_test_agent(&store, "agent-tgt", "run-ar5-cancel");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let request = stub_review_request("agent-req", "agent-tgt", "run-ar5-cancel");
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::RequestReview(request)),
        );
        let out = exec.execute_node(&agent_step_input("agent-req", "run-ar5-cancel"));
        assert_eq!(out.status, "completed");

        let proposals = store
            .list_proposals_by_run("run-ar5-cancel", 100, 0)
            .expect("list");
        let corr = proposals[0]["correlation_id"].as_str().unwrap().to_string();

        let cancel_exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::CancelProposal(corr)),
        );
        let cancel_out = cancel_exec.execute_node(&agent_step_input("agent-req", "run-ar5-cancel"));
        assert_eq!(cancel_out.status, "completed");
    }

    #[test]
    fn test_ar5_non_owner_cannot_cancel_review() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-req", "run-ar5-no-cancel");
        create_test_agent(&store, "agent-tgt", "run-ar5-no-cancel");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let request = stub_review_request("agent-req", "agent-tgt", "run-ar5-no-cancel");
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::RequestReview(request)),
        );
        let out = exec.execute_node(&agent_step_input("agent-req", "run-ar5-no-cancel"));
        assert_eq!(out.status, "completed");

        let proposals = store
            .list_proposals_by_run("run-ar5-no-cancel", 100, 0)
            .expect("list");
        let corr = proposals[0]["correlation_id"].as_str().unwrap().to_string();

        let cancel_exec =
            AgentStepExecutor::new(store, stub_decision(AgentAction::CancelProposal(corr)));
        let cancel_out =
            cancel_exec.execute_node(&agent_step_input("agent-tgt", "run-ar5-no-cancel"));
        assert_eq!(cancel_out.status, "failed");
    }

    #[test]
    fn test_ar5_terminal_review_cannot_be_modified() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-req", "run-ar5-term");
        create_test_agent(&store, "agent-tgt", "run-ar5-term");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let request = stub_review_request("agent-req", "agent-tgt", "run-ar5-term");
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::RequestReview(request)),
        );
        let out = exec.execute_node(&agent_step_input("agent-req", "run-ar5-term"));
        assert_eq!(out.status, "completed");

        let proposals = store
            .list_proposals_by_run("run-ar5-term", 100, 0)
            .expect("list");
        let review_pid = proposals[0]["proposal_id"].as_str().unwrap().to_string();

        let verdict = ReviewVerdict {
            schema_version: "review_verdict.v1".to_string(),
            correlation_id: "corr-review-001".to_string(),
            review_request_id: review_pid.clone(),
            verdict: "accepted".to_string(),
            rationale_summary: "looks good".to_string(),
            run_id: "run-ar5-term".to_string(),
            node_id: "agent-node-001".to_string(),
            blocking: true,
        };
        let verdict_exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::SubmitReviewVerdict(verdict)),
        );
        let vout = verdict_exec.execute_node(&agent_step_input("agent-tgt", "run-ar5-term"));
        assert_eq!(vout.status, "completed");

        let verdict2 = ReviewVerdict {
            schema_version: "review_verdict.v1".to_string(),
            correlation_id: "corr-review-002".to_string(),
            review_request_id: review_pid,
            verdict: "rejected".to_string(),
            rationale_summary: "changed mind".to_string(),
            run_id: "run-ar5-term".to_string(),
            node_id: "agent-node-001".to_string(),
            blocking: true,
        };
        let dup_exec = AgentStepExecutor::new(
            store,
            stub_decision(AgentAction::SubmitReviewVerdict(verdict2)),
        );
        let dout = dup_exec.execute_node(&agent_step_input("agent-tgt", "run-ar5-term"));
        assert_eq!(dout.status, "failed");
        assert!(dout.error_message.unwrap().contains("not pending"));
    }

    #[test]
    fn test_ar5_debate_opens_with_max_rounds() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-opener", "run-ar5-deb");
        create_test_agent(&store, "p1", "run-ar5-deb");
        create_test_agent(&store, "p2", "run-ar5-deb");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let debate = stub_debate_request("agent-opener", "run-ar5-deb");
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::OpenDebate(debate)),
        );
        let out = exec.execute_node(&agent_step_input("agent-opener", "run-ar5-deb"));
        assert_eq!(out.status, "completed");
        let result = out.output.unwrap();
        assert!(result.contains("open_debate"));
        assert!(result.contains("\"max_rounds\":3"));

        let proposals = store
            .list_proposals_by_run("run-ar5-deb", 100, 0)
            .expect("list");
        let debate_proposals: Vec<_> = proposals
            .iter()
            .filter(|p| p["proposal_type"] == "debate_request")
            .collect();
        assert_eq!(debate_proposals.len(), 1);
        assert_eq!(debate_proposals[0]["status"], "pending");

        let msgs_p1 = store
            .list_mailbox(Some("p1"), Some("run-ar5-deb"), None, None, 100, 0)
            .expect("list mailbox p1");
        assert_eq!(msgs_p1.len(), 1);
        assert_eq!(msgs_p1[0].message_type, "debate_request");

        let msgs_p2 = store
            .list_mailbox(Some("p2"), Some("run-ar5-deb"), None, None, 100, 0)
            .expect("list mailbox p2");
        assert_eq!(msgs_p2.len(), 1);
    }

    #[test]
    fn test_ar5_non_participant_cannot_submit_debate_position() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-opener", "run-ar5-np");
        create_test_agent(&store, "p1", "run-ar5-np");
        create_test_agent(&store, "outsider", "run-ar5-np");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let debate = stub_debate_request("agent-opener", "run-ar5-np");
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::OpenDebate(debate)),
        );
        let out = exec.execute_node(&agent_step_input("agent-opener", "run-ar5-np"));
        assert_eq!(out.status, "completed");

        let proposals = store
            .list_proposals_by_run("run-ar5-np", 100, 0)
            .expect("list");
        let debate_pid = proposals[0]["proposal_id"].as_str().unwrap().to_string();

        let position = DebatePosition {
            schema_version: "debate_position.v1".to_string(),
            correlation_id: "corr-debate-001".to_string(),
            debate_id: debate_pid,
            position: "approach A is better".to_string(),
            rationale_summary: "because reasons".to_string(),
            run_id: "run-ar5-np".to_string(),
            node_id: "agent-node-001".to_string(),
        };
        let outsider_exec = AgentStepExecutor::new(
            store,
            stub_decision(AgentAction::SubmitDebatePosition(position)),
        );
        let out2 = outsider_exec.execute_node(&agent_step_input("outsider", "run-ar5-np"));
        assert_eq!(out2.status, "failed");
        assert!(out2.error_message.unwrap().contains("not a participant"));
    }

    #[test]
    fn test_ar5_max_rounds_prevents_further_positions() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-opener", "run-ar5-mr");
        create_test_agent(&store, "p1", "run-ar5-mr");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let debate = DebateRequest {
            schema_version: "debate_request.v1".to_string(),
            correlation_id: "corr-debate-mr".to_string(),
            subject_summary: "topic".to_string(),
            participant_agent_ids: vec!["p1".to_string()],
            max_rounds: 1,
            run_id: "run-ar5-mr".to_string(),
            node_id: "agent-node-001".to_string(),
        };
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::OpenDebate(debate)),
        );
        let out = exec.execute_node(&agent_step_input("agent-opener", "run-ar5-mr"));
        assert_eq!(out.status, "completed");

        let proposals = store
            .list_proposals_by_run("run-ar5-mr", 100, 0)
            .expect("list");
        let debate_pid = proposals[0]["proposal_id"].as_str().unwrap().to_string();

        let position = DebatePosition {
            schema_version: "debate_position.v1".to_string(),
            correlation_id: "corr-debate-mr".to_string(),
            debate_id: debate_pid.clone(),
            position: "round 1 position".to_string(),
            rationale_summary: "because".to_string(),
            run_id: "run-ar5-mr".to_string(),
            node_id: "agent-node-001".to_string(),
        };
        let pos_exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::SubmitDebatePosition(position)),
        );
        let pout = pos_exec.execute_node(&agent_step_input("p1", "run-ar5-mr"));
        assert_eq!(pout.status, "completed");

        let position2 = DebatePosition {
            schema_version: "debate_position.v1".to_string(),
            correlation_id: "corr-debate-mr".to_string(),
            debate_id: debate_pid,
            position: "round 2 position".to_string(),
            rationale_summary: "because".to_string(),
            run_id: "run-ar5-mr".to_string(),
            node_id: "agent-node-001".to_string(),
        };
        let pos_exec2 = AgentStepExecutor::new(
            store,
            stub_decision(AgentAction::SubmitDebatePosition(position2)),
        );
        let pout2 = pos_exec2.execute_node(&agent_step_input("p1", "run-ar5-mr"));
        assert_eq!(pout2.status, "failed");
        assert!(pout2.error_message.unwrap().contains("reached max rounds"));
    }

    #[test]
    fn test_ar5_debate_resolution_is_terminal() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-opener", "run-ar5-res");
        create_test_agent(&store, "p1", "run-ar5-res");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let debate = stub_debate_request("agent-opener", "run-ar5-res");
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::OpenDebate(debate)),
        );
        let out = exec.execute_node(&agent_step_input("agent-opener", "run-ar5-res"));
        assert_eq!(out.status, "completed");

        let proposals = store
            .list_proposals_by_run("run-ar5-res", 100, 0)
            .expect("list");
        let debate_pid = proposals[0]["proposal_id"].as_str().unwrap().to_string();

        let resolution = DebateResolution {
            schema_version: "debate_resolution.v1".to_string(),
            correlation_id: "corr-debate-res".to_string(),
            debate_id: debate_pid.clone(),
            resolution: "approach A wins".to_string(),
            winning_position: Some("approach A".to_string()),
            unresolved_risks: None,
            run_id: "run-ar5-res".to_string(),
            node_id: "agent-node-001".to_string(),
        };
        let res_exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::ResolveDebate(resolution)),
        );
        let rout = res_exec.execute_node(&agent_step_input("agent-opener", "run-ar5-res"));
        assert_eq!(rout.status, "completed");

        let position = DebatePosition {
            schema_version: "debate_position.v1".to_string(),
            correlation_id: "corr-debate-res".to_string(),
            debate_id: debate_pid,
            position: "too late".to_string(),
            rationale_summary: "because".to_string(),
            run_id: "run-ar5-res".to_string(),
            node_id: "agent-node-001".to_string(),
        };
        let late_exec = AgentStepExecutor::new(
            store,
            stub_decision(AgentAction::SubmitDebatePosition(position)),
        );
        let lout = late_exec.execute_node(&agent_step_input("p1", "run-ar5-res"));
        assert_eq!(lout.status, "failed");
        assert!(lout.error_message.unwrap().contains("not pending"));
    }

    #[test]
    fn test_ar5_audit_events_are_metadata_only() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-req", "run-ar5-audit");
        create_test_agent(&store, "agent-tgt", "run-ar5-audit");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let request = stub_review_request("agent-req", "agent-tgt", "run-ar5-audit");
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::RequestReview(request)),
        );
        let out = exec.execute_node(&agent_step_input("agent-req", "run-ar5-audit"));
        assert_eq!(out.status, "completed");

        let events = store.audit_events(100).expect("audit events");
        let ar5_events: Vec<_> = events
            .iter()
            .filter(|e| {
                e.get("action")
                    .and_then(|a| a.as_str())
                    .map(|a| a.starts_with("review.") || a.starts_with("debate."))
                    .unwrap_or(false)
            })
            .collect();
        assert!(
            !ar5_events.is_empty(),
            "expected at least one review/debate audit event"
        );
        for event in &ar5_events {
            let details = event.get("details").unwrap();
            let details_str = details.to_string().to_lowercase();
            assert!(
                !details_str.contains("password"),
                "audit event should not contain raw secrets"
            );
            assert!(
                !details_str.contains("rationale_summary"),
                "audit event should not contain rationale text"
            );
        }
    }

    #[test]
    fn test_ar5_self_review_fails_closed() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-self", "run-ar5-sr");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let request = ReviewRequest {
            schema_version: "review_request.v1".to_string(),
            correlation_id: "corr-self-review".to_string(),
            subject_summary: "review myself".to_string(),
            rationale_summary: "why not".to_string(),
            target_agent_id: "agent-self".to_string(),
            run_id: "run-ar5-sr".to_string(),
            node_id: "agent-node-001".to_string(),
            blocking: true,
        };
        let exec =
            AgentStepExecutor::new(store, stub_decision(AgentAction::RequestReview(request)));
        let out = exec.execute_node(&agent_step_input("agent-self", "run-ar5-sr"));
        assert_eq!(out.status, "failed");
        assert!(out.error_message.unwrap().contains("must be different"));
    }

    #[test]
    fn test_ar5_debate_non_opener_cannot_resolve() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-opener", "run-ar5-nor");
        create_test_agent(&store, "p1", "run-ar5-nor");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let debate = stub_debate_request("agent-opener", "run-ar5-nor");
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::OpenDebate(debate)),
        );
        let out = exec.execute_node(&agent_step_input("agent-opener", "run-ar5-nor"));
        assert_eq!(out.status, "completed");

        let proposals = store
            .list_proposals_by_run("run-ar5-nor", 100, 0)
            .expect("list");
        let debate_pid = proposals[0]["proposal_id"].as_str().unwrap().to_string();

        let resolution = DebateResolution {
            schema_version: "debate_resolution.v1".to_string(),
            correlation_id: "corr-debate-nor".to_string(),
            debate_id: debate_pid,
            resolution: "I decide".to_string(),
            winning_position: None,
            unresolved_risks: None,
            run_id: "run-ar5-nor".to_string(),
            node_id: "agent-node-001".to_string(),
        };
        let p1_exec =
            AgentStepExecutor::new(store, stub_decision(AgentAction::ResolveDebate(resolution)));
        let out2 = p1_exec.execute_node(&agent_step_input("p1", "run-ar5-nor"));
        assert_eq!(out2.status, "failed");
        assert!(out2
            .error_message
            .unwrap()
            .contains("only the debate opener"));
    }

    #[test]
    fn test_ar5_review_requester_cancel_by_correlation() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-req", "run-ar5-rc");
        create_test_agent(&store, "agent-tgt", "run-ar5-rc");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let request = ReviewRequest {
            schema_version: "review_request.v1".to_string(),
            correlation_id: "corr-cancel-test".to_string(),
            subject_summary: "review me".to_string(),
            rationale_summary: "please".to_string(),
            target_agent_id: "agent-tgt".to_string(),
            run_id: "run-ar5-rc".to_string(),
            node_id: "agent-node-001".to_string(),
            blocking: false,
        };
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::RequestReview(request)),
        );
        let out = exec.execute_node(&agent_step_input("agent-req", "run-ar5-rc"));
        assert_eq!(out.status, "completed");

        let cancel_exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::CancelProposal("corr-cancel-test".to_string())),
        );
        let cout = cancel_exec.execute_node(&agent_step_input("agent-req", "run-ar5-rc"));
        assert_eq!(cout.status, "completed");

        let proposals = store
            .list_proposals_by_run("run-ar5-rc", 100, 0)
            .expect("list");
        assert_eq!(proposals[0]["status"], "cancelled");
    }

    // ── Blocker 1: run_id fail-closed tests ────────────────────────────────

    #[test]
    fn test_ar5_wrong_run_id_request_review() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-req", "run-ar5-rr-wr");
        create_test_agent(&store, "agent-tgt", "run-ar5-rr-wr");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let mut request = stub_review_request("agent-req", "agent-tgt", "run-ar5-rr-wr");
        request.run_id = "run-ar5-wrong".to_string();
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::RequestReview(request)),
        );
        let out = exec.execute_node(&agent_step_input("agent-req", "run-ar5-rr-wr"));
        assert_eq!(out.status, "failed");
        assert!(out
            .error_message
            .unwrap()
            .contains("does not match current run"));

        let proposals = store
            .list_proposals_by_run("run-ar5-rr-wr", 100, 0)
            .expect("list");
        assert!(
            proposals.is_empty(),
            "no proposals should be created on run_id mismatch"
        );
    }

    #[test]
    fn test_ar5_wrong_run_id_open_debate() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-opener", "run-ar5-od-wr");
        create_test_agent(&store, "p1", "run-ar5-od-wr");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let mut debate = stub_debate_request("agent-opener", "run-ar5-od-wr");
        debate.run_id = "run-ar5-wrong".to_string();
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::OpenDebate(debate)),
        );
        let out = exec.execute_node(&agent_step_input("agent-opener", "run-ar5-od-wr"));
        assert_eq!(out.status, "failed");
        assert!(out
            .error_message
            .unwrap()
            .contains("does not match current run"));

        let proposals = store
            .list_proposals_by_run("run-ar5-od-wr", 100, 0)
            .expect("list");
        assert!(
            proposals.is_empty(),
            "no proposals should be created on run_id mismatch"
        );
    }

    #[test]
    fn test_ar5_wrong_run_id_submit_debate_position() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-opener", "run-ar5-dp-wr");
        create_test_agent(&store, "p1", "run-ar5-dp-wr");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let debate = stub_debate_request("agent-opener", "run-ar5-dp-wr");
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::OpenDebate(debate)),
        );
        let out = exec.execute_node(&agent_step_input("agent-opener", "run-ar5-dp-wr"));
        assert_eq!(out.status, "completed");

        let proposals = store
            .list_proposals_by_run("run-ar5-dp-wr", 100, 0)
            .expect("list");
        let debate_pid = proposals[0]["proposal_id"].as_str().unwrap().to_string();

        let position = DebatePosition {
            schema_version: "debate_position.v1".to_string(),
            correlation_id: "corr-debate-001".to_string(),
            debate_id: debate_pid,
            position: "approach A".to_string(),
            rationale_summary: "reasons".to_string(),
            run_id: "run-ar5-wrong".to_string(),
            node_id: "agent-node-001".to_string(),
        };
        let pos_exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::SubmitDebatePosition(position)),
        );
        let pout = pos_exec.execute_node(&agent_step_input("p1", "run-ar5-dp-wr"));
        assert_eq!(pout.status, "failed");
        assert!(pout
            .error_message
            .unwrap()
            .contains("does not match current run"));

        // Only the debate_request should exist, no debate_position
        let proposals_after = store
            .list_proposals_by_run("run-ar5-dp-wr", 100, 0)
            .expect("list");
        let pos_count = proposals_after
            .iter()
            .filter(|p| p["proposal_type"] == "debate_position")
            .count();
        assert_eq!(
            pos_count, 0,
            "no debate_position should be created on run_id mismatch"
        );
    }

    #[test]
    fn test_ar5_wrong_run_id_resolve_debate() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-opener", "run-ar5-dr-wr");
        create_test_agent(&store, "p1", "run-ar5-dr-wr");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let debate = stub_debate_request("agent-opener", "run-ar5-dr-wr");
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::OpenDebate(debate)),
        );
        let out = exec.execute_node(&agent_step_input("agent-opener", "run-ar5-dr-wr"));
        assert_eq!(out.status, "completed");

        let proposals = store
            .list_proposals_by_run("run-ar5-dr-wr", 100, 0)
            .expect("list");
        let debate_pid = proposals[0]["proposal_id"].as_str().unwrap().to_string();

        let resolution = DebateResolution {
            schema_version: "debate_resolution.v1".to_string(),
            correlation_id: "corr-debate-dr".to_string(),
            debate_id: debate_pid,
            resolution: "A wins".to_string(),
            winning_position: Some("A".to_string()),
            unresolved_risks: None,
            run_id: "run-ar5-wrong".to_string(),
            node_id: "agent-node-001".to_string(),
        };
        let res_exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::ResolveDebate(resolution)),
        );
        let rout = res_exec.execute_node(&agent_step_input("agent-opener", "run-ar5-dr-wr"));
        assert_eq!(rout.status, "failed");
        assert!(rout
            .error_message
            .unwrap()
            .contains("does not match current run"));

        let proposals_after = store
            .list_proposals_by_run("run-ar5-dr-wr", 100, 0)
            .expect("list");
        let res_count = proposals_after
            .iter()
            .filter(|p| p["proposal_type"] == "debate_resolution")
            .count();
        assert_eq!(
            res_count, 0,
            "no debate_resolution should be created on run_id mismatch"
        );
    }

    // ── Blocker 2: status update bool tests ────────────────────────────────

    #[test]
    fn test_ar5_terminal_review_cannot_create_second_verdict() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-req", "run-ar5-tv2");
        create_test_agent(&store, "agent-tgt", "run-ar5-tv2");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let request = stub_review_request("agent-req", "agent-tgt", "run-ar5-tv2");
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::RequestReview(request)),
        );
        let out = exec.execute_node(&agent_step_input("agent-req", "run-ar5-tv2"));
        assert_eq!(out.status, "completed");

        let proposals = store
            .list_proposals_by_run("run-ar5-tv2", 100, 0)
            .expect("list");
        let review_pid = proposals[0]["proposal_id"].as_str().unwrap().to_string();

        // First verdict — succeeds
        let verdict1 = ReviewVerdict {
            schema_version: "review_verdict.v1".to_string(),
            correlation_id: "corr-v1".to_string(),
            review_request_id: review_pid.clone(),
            verdict: "accepted".to_string(),
            rationale_summary: "looks good".to_string(),
            run_id: "run-ar5-tv2".to_string(),
            node_id: "agent-node-001".to_string(),
            blocking: true,
        };
        let exec1 = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::SubmitReviewVerdict(verdict1)),
        );
        let vout1 = exec1.execute_node(&agent_step_input("agent-tgt", "run-ar5-tv2"));
        assert_eq!(vout1.status, "completed");

        // Second verdict — must fail, review is no longer pending
        let verdict2 = ReviewVerdict {
            schema_version: "review_verdict.v1".to_string(),
            correlation_id: "corr-v2".to_string(),
            review_request_id: review_pid,
            verdict: "rejected".to_string(),
            rationale_summary: "changed mind".to_string(),
            run_id: "run-ar5-tv2".to_string(),
            node_id: "agent-node-001".to_string(),
            blocking: true,
        };
        let exec2 = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::SubmitReviewVerdict(verdict2)),
        );
        let vout2 = exec2.execute_node(&agent_step_input("agent-tgt", "run-ar5-tv2"));
        assert_eq!(vout2.status, "failed");
        assert!(vout2.error_message.unwrap().contains("not pending"));

        // Verify only ONE review_verdict proposal exists
        let proposals_after = store
            .list_proposals_by_run("run-ar5-tv2", 100, 0)
            .expect("list");
        let verdict_count = proposals_after
            .iter()
            .filter(|p| p["proposal_type"] == "review_verdict")
            .count();
        assert_eq!(verdict_count, 1, "only one verdict proposal should exist");
    }

    #[test]
    fn test_ar5_resolved_debate_cannot_create_second_resolution() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-opener", "run-ar5-rd2");
        create_test_agent(&store, "p1", "run-ar5-rd2");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let debate = stub_debate_request("agent-opener", "run-ar5-rd2");
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::OpenDebate(debate)),
        );
        let out = exec.execute_node(&agent_step_input("agent-opener", "run-ar5-rd2"));
        assert_eq!(out.status, "completed");

        let proposals = store
            .list_proposals_by_run("run-ar5-rd2", 100, 0)
            .expect("list");
        let debate_pid = proposals[0]["proposal_id"].as_str().unwrap().to_string();

        // First resolution — succeeds
        let res1 = DebateResolution {
            schema_version: "debate_resolution.v1".to_string(),
            correlation_id: "corr-r1".to_string(),
            debate_id: debate_pid.clone(),
            resolution: "A wins".to_string(),
            winning_position: Some("A".to_string()),
            unresolved_risks: None,
            run_id: "run-ar5-rd2".to_string(),
            node_id: "agent-node-001".to_string(),
        };
        let exec1 = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::ResolveDebate(res1)),
        );
        let rout1 = exec1.execute_node(&agent_step_input("agent-opener", "run-ar5-rd2"));
        assert_eq!(rout1.status, "completed");

        // Second resolution — must fail
        let res2 = DebateResolution {
            schema_version: "debate_resolution.v1".to_string(),
            correlation_id: "corr-r2".to_string(),
            debate_id: debate_pid,
            resolution: "B wins actually".to_string(),
            winning_position: Some("B".to_string()),
            unresolved_risks: None,
            run_id: "run-ar5-rd2".to_string(),
            node_id: "agent-node-001".to_string(),
        };
        let exec2 = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::ResolveDebate(res2)),
        );
        let rout2 = exec2.execute_node(&agent_step_input("agent-opener", "run-ar5-rd2"));
        assert_eq!(rout2.status, "failed");
        assert!(rout2.error_message.unwrap().contains("not pending"));

        // Verify only ONE debate_resolution proposal exists
        let proposals_after = store
            .list_proposals_by_run("run-ar5-rd2", 100, 0)
            .expect("list");
        let res_count = proposals_after
            .iter()
            .filter(|p| p["proposal_type"] == "debate_resolution")
            .count();
        assert_eq!(res_count, 1, "only one resolution proposal should exist");
    }

    // ── Blocker 3: debate round fail-closed tests ──────────────────────────

    #[test]
    fn test_ar5_terminal_debate_cannot_accept_position() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-opener", "run-ar5-tdp");
        create_test_agent(&store, "p1", "run-ar5-tdp");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let debate = stub_debate_request("agent-opener", "run-ar5-tdp");
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::OpenDebate(debate)),
        );
        let out = exec.execute_node(&agent_step_input("agent-opener", "run-ar5-tdp"));
        assert_eq!(out.status, "completed");

        let proposals = store
            .list_proposals_by_run("run-ar5-tdp", 100, 0)
            .expect("list");
        let debate_pid = proposals[0]["proposal_id"].as_str().unwrap().to_string();

        // Resolve the debate — makes it terminal
        let resolution = DebateResolution {
            schema_version: "debate_resolution.v1".to_string(),
            correlation_id: "corr-tdp".to_string(),
            debate_id: debate_pid.clone(),
            resolution: "A wins".to_string(),
            winning_position: Some("A".to_string()),
            unresolved_risks: None,
            run_id: "run-ar5-tdp".to_string(),
            node_id: "agent-node-001".to_string(),
        };
        let res_exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::ResolveDebate(resolution)),
        );
        let rout = res_exec.execute_node(&agent_step_input("agent-opener", "run-ar5-tdp"));
        assert_eq!(rout.status, "completed");

        // Try to submit a position — must fail, debate is terminal
        let position = DebatePosition {
            schema_version: "debate_position.v1".to_string(),
            correlation_id: "corr-tdp".to_string(),
            debate_id: debate_pid,
            position: "too late".to_string(),
            rationale_summary: "because".to_string(),
            run_id: "run-ar5-tdp".to_string(),
            node_id: "agent-node-001".to_string(),
        };
        let pos_exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::SubmitDebatePosition(position)),
        );
        let pout = pos_exec.execute_node(&agent_step_input("p1", "run-ar5-tdp"));
        assert_eq!(pout.status, "failed");
        assert!(pout.error_message.unwrap().contains("not pending"));

        // Verify no debate_position proposals exist
        let proposals_after = store
            .list_proposals_by_run("run-ar5-tdp", 100, 0)
            .expect("list");
        let pos_count = proposals_after
            .iter()
            .filter(|p| p["proposal_type"] == "debate_position")
            .count();
        assert_eq!(
            pos_count, 0,
            "no position should be created on terminal debate"
        );
    }

    // ── Blocker 4: send failure propagation test ───────────────────────────

    #[test]
    fn test_ar5_open_debate_send_failure_is_propagated() {
        let _lock = AGENT_ENV_LOCK.lock().unwrap();
        let store = Arc::new(ar2_store());
        create_test_agent(&store, "agent-opener", "run-ar5-sf");
        create_test_agent(&store, "p1", "run-ar5-sf");
        create_test_agent(&store, "p2", "run-ar5-sf");
        std::env::set_var("ACP_ENABLE_AGENT_RUNTIME", "1");
        std::env::remove_var("ACP_AGENT_RUNTIME_KILL_SWITCH");

        let debate = stub_debate_request("agent-opener", "run-ar5-sf");
        let exec = AgentStepExecutor::new(
            store.clone(),
            stub_decision(AgentAction::OpenDebate(debate)),
        );
        let out = exec.execute_node(&agent_step_input("agent-opener", "run-ar5-sf"));
        assert_eq!(out.status, "completed");

        // Verify messages were sent to both participants
        let msgs_p1 = store
            .list_mailbox(Some("p1"), Some("run-ar5-sf"), None, None, 100, 0)
            .expect("mailbox p1");
        assert_eq!(msgs_p1.len(), 1);
        assert_eq!(msgs_p1[0].message_type, "debate_request");

        let msgs_p2 = store
            .list_mailbox(Some("p2"), Some("run-ar5-sf"), None, None, 100, 0)
            .expect("mailbox p2");
        assert_eq!(msgs_p2.len(), 1);
        assert_eq!(msgs_p2[0].message_type, "debate_request");
    }

    // ── CAS-style round update tests ───────────────────────────────────────

    /// CAS guard at the store level: the second call to
    /// update_debate_round_if_pending on the same proposal must fail because
    /// the first call already changed context_summary, so the WHERE clause
    /// (context_summary = old) no longer matches.
    #[test]
    fn test_ar5_cas_round_update_rejects_double_advance() {
        let store = ar2_store();
        let debate_pid = "debate-cas-test";
        let initial_meta = json!({
            "max_rounds": 5,
            "current_round": 0,
            "participant_agent_ids": ["p1"],
        });
        store
            .create_proposal(
                debate_pid,
                "corr-cas",
                "run-cas",
                "node-1",
                "opener",
                "debate_request",
                "topic",
                &initial_meta.to_string(),
                None,
                None,
                None,
            )
            .expect("create debate");

        let next_meta = json!({
            "max_rounds": 5,
            "current_round": 1,
            "participant_agent_ids": ["p1"],
        });

        // First CAS succeeds: context matches, round 0 == expected 0
        let ok = store
            .update_debate_round_if_pending(debate_pid, "run-cas", 0, &next_meta.to_string())
            .expect("first CAS");
        assert!(ok, "first CAS should succeed");

        // Second CAS with same expected_round=0 must fail: context_summary
        // already changed (now current_round=1), so WHERE clause matches 0
        // rows — this is the true CAS guard against concurrent writers.
        let ok2 = store
            .update_debate_round_if_pending(debate_pid, "run-cas", 0, &next_meta.to_string())
            .expect("second CAS");
        assert!(
            !ok2,
            "second CAS must fail — context_summary changed since first read"
        );
    }

    /// CAS guard rejects mismatched expected_current_round.
    #[test]
    fn test_ar5_cas_round_update_rejects_wrong_expected_round() {
        let store = ar2_store();
        let debate_pid = "debate-cas-wrong";
        let initial_meta = json!({
            "max_rounds": 5,
            "current_round": 0,
            "participant_agent_ids": ["p1"],
        });
        store
            .create_proposal(
                debate_pid,
                "corr-cas-w",
                "run-cas-w",
                "node-1",
                "opener",
                "debate_request",
                "topic",
                &initial_meta.to_string(),
                None,
                None,
                None,
            )
            .expect("create debate");

        let next_meta = json!({
            "max_rounds": 5,
            "current_round": 1,
            "participant_agent_ids": ["p1"],
        });

        // expected_current_round=5 doesn't match actual round=0
        let ok = store
            .update_debate_round_if_pending(debate_pid, "run-cas-w", 5, &next_meta.to_string())
            .expect("CAS with wrong round");
        assert!(
            !ok,
            "CAS must reject when expected round doesn't match actual"
        );

        // Verify the original context is unchanged
        let p = store.get_proposal(debate_pid).expect("get").unwrap();
        let ctx: serde_json::Value =
            serde_json::from_str(p["context_summary"].as_str().unwrap()).unwrap();
        assert_eq!(ctx["current_round"], 0, "context must be unchanged");
    }
}
