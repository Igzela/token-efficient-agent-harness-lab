use std::process::{Command, Stdio};

use serde_json::Value;

use crate::cli::spawn_with_timeout;
use crate::node_executor::{NodeExecutionInput, NodeExecutionOutput, NodeExecutor};

/// CLI-backed NodeExecutor that spawns real Claude Code or Codex CLI processes.
///
/// Gated behind `ACP_ENABLE_CLI_EXECUTION=1`. Reads `node_metadata` for:
/// - `prompt` or `command`: the task text to send to the CLI
/// - `executor`: `"claude_code_cli"` or `"codex_cli"` (default: config default)
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

    pub fn from_config(config: &super::config::CliConfig) -> Option<Self> {
        if !config.enabled {
            return None;
        }
        let has_any = config.claude_code_enabled || config.codex_enabled;
        if !has_any {
            return None;
        }
        Some(Self::new(
            config.claude_code_bin.clone(),
            config.codex_bin.clone(),
            config.timeout_ms,
        ))
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

    pub fn resolve_cwd(&self, input: &NodeExecutionInput) -> String {
        input
            .node_metadata
            .get("workspace_path")
            .and_then(|v| v.as_str())
            .unwrap_or(".")
            .to_string()
    }
}

impl NodeExecutor for CliNodeExecutor {
    fn executor_type_name(&self) -> &str { "cli" }
    fn execute_node(&self, input: &NodeExecutionInput) -> NodeExecutionOutput {
        let start = std::time::Instant::now();
        let executor_type = self.resolve_executor(input);
        let prompt = self.resolve_prompt(input);
        let model = self.resolve_model(input);
        let cwd = self.resolve_cwd(input);

        let (bin_path, effective_type) = match executor_type.as_str() {
            "claude_code_cli" => match &self.claude_bin {
                Some(bin) => (bin.clone(), "claude_code_cli"),
                None => {
                    return NodeExecutionOutput {
                        status: "failed".to_string(),
                        executor_type: "claude_code_cli".to_string(),
                        output: None,
                        error_domain: Some("cli_not_found".to_string()),
                        error_message: Some("claude binary not configured".to_string()),
                        input_tokens: None,
                        output_tokens: None,
                        estimated_cost: None,
                        latency_ms: Some(start.elapsed().as_millis() as i64),
                    };
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
                };
            }
        };

        let mut cmd = Command::new(&bin_path);
        match effective_type {
            "claude_code_cli" => {
                cmd.arg("-p")
                    .arg(&prompt)
                    .arg("--output-format")
                    .arg("json")
                    .arg("--allowedTools")
                    .arg("Edit,Write,Bash");
                if let Some(ref m) = model {
                    cmd.arg("--model").arg(m);
                }
            }
            "codex_cli" => {
                cmd.arg("exec").arg(&prompt);
            }
            _ => unreachable!(),
        }
        cmd.current_dir(&cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = spawn_with_timeout(&mut cmd, self.timeout_ms);
        let elapsed_ms = start.elapsed().as_millis() as i64;

        match output {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
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
                    };
                }

                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                parse_cli_output(&stdout, effective_type, elapsed_ms)
            }
            Err(timeout_elapsed) => {
                let (domain, msg) = if timeout_elapsed == 0 {
                    ("cli_not_found", format!("failed to spawn {effective_type}"))
                } else {
                    (
                        "cli_timeout",
                        format!(
                            "{effective_type} timed out after {timeout_elapsed}ms (limit {}ms)",
                            self.timeout_ms
                        ),
                    )
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
                }
            }
        }
    }
}

fn parse_cli_output(raw: &str, executor_type: &str, latency_ms: i64) -> NodeExecutionOutput {
    let parsed: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(err) => {
            return NodeExecutionOutput {
                status: "failed".to_string(),
                executor_type: executor_type.to_string(),
                output: if raw.is_empty() {
                    None
                } else {
                    Some(raw.to_string())
                },
                error_domain: Some("cli_output_parse_error".to_string()),
                error_message: Some(format!("failed to parse CLI JSON output: {err}")),
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(latency_ms),
            };
        }
    };

    let output_text = parsed
        .get("result")
        .or_else(|| parsed.get("output"))
        .or_else(|| parsed.get("content"))
        .and_then(|v| v.as_str())
        .unwrap_or(raw)
        .to_string();

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

    let estimated_cost = super::claude_code::compute_cli_cost(
        executor_type,
        input_tokens.unwrap_or(0),
        output_tokens.unwrap_or(0),
    );

    NodeExecutionOutput {
        status: "completed".to_string(),
        executor_type: executor_type.to_string(),
        output: Some(output_text),
        error_domain: None,
        error_message: None,
        input_tokens,
        output_tokens,
        estimated_cost: Some(estimated_cost),
        latency_ms: Some(latency_ms),
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

    #[test]
    fn test_cli_node_executor_no_binary_returns_failure() {
        let executor = CliNodeExecutor::new(None, None, 5000);
        let input = make_input(json!({"prompt": "hello"}));
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "failed");
        assert_eq!(output.error_domain.as_deref(), Some("cli_not_found"));
    }

    #[test]
    fn test_cli_node_executor_unknown_executor_type() {
        let executor = CliNodeExecutor::new(Some("/bin/claude".into()), None, 5000);
        let input = make_input(json!({"prompt": "hello", "executor": "unknown_type"}));
        let output = executor.execute_node(&input);
        assert_eq!(output.status, "failed");
        assert_eq!(output.error_domain.as_deref(), Some("unknown_cli_executor"));
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
    fn test_parse_cli_output_success() {
        let raw = r#"{"result":"hello world","usage":{"input_tokens":100,"output_tokens":50}}"#;
        let output = parse_cli_output(raw, "claude_code_cli", 100);
        assert_eq!(output.status, "completed");
        assert_eq!(output.output.as_deref(), Some("hello world"));
        assert_eq!(output.input_tokens, Some(100));
        assert_eq!(output.output_tokens, Some(50));
        assert!(output.estimated_cost.unwrap() > 0.0);
    }

    #[test]
    fn test_parse_cli_output_malformed_json() {
        let output = parse_cli_output("not-json", "codex_cli", 50);
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("cli_output_parse_error")
        );
    }

    #[test]
    fn test_parse_cli_output_fallback_to_raw() {
        let raw = r#"{"id":"req-123","usage":{}}"#;
        let output = parse_cli_output(raw, "claude_code_cli", 50);
        assert_eq!(output.status, "completed");
        assert_eq!(output.output.as_deref(), Some(raw));
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
            complexity_threshold: 0.7,
        };
        assert!(CliNodeExecutor::from_config(&config).is_none());
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
            complexity_threshold: 0.7,
        };
        assert!(CliNodeExecutor::from_config(&config).is_none());
    }

    #[test]
    fn test_from_config_with_claude() {
        let config = super::super::config::CliConfig {
            enabled: true,
            claude_code_bin: Some("/bin/claude".into()),
            claude_code_enabled: true,
            codex_bin: None,
            codex_enabled: false,
            timeout_ms: 5000,
            complexity_threshold: 0.7,
        };
        let executor = CliNodeExecutor::from_config(&config).unwrap();
        assert_eq!(executor.default_executor, "claude_code_cli");
    }
}
