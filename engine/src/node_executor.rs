use serde_json::{json, Value};

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
        json!({
            "status": self.status,
            "executor_type": self.executor_type,
            "output": self.output,
            "error_domain": self.error_domain,
            "error_message": self.error_message,
            "input_tokens": self.input_tokens,
            "output_tokens": self.output_tokens,
            "estimated_cost": self.estimated_cost,
            "latency_ms": self.latency_ms,
        })
    }
}

/// Trait for executing individual workflow nodes.
pub trait NodeExecutor: Send + Sync {
    fn execute_node(&self, input: &NodeExecutionInput) -> NodeExecutionOutput;
}

/// Noop executor that always succeeds immediately.
pub struct NoopNodeExecutor;

impl NodeExecutor for NoopNodeExecutor {
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
        let binary = first_token
            .rsplit('/')
            .next()
            .unwrap_or(first_token);
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
        command
            .split_whitespace()
            .map(|s| s.to_string())
            .collect()
    }
}

impl NodeExecutor for CommandNodeExecutor {
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

        let cwd = input
            .node_metadata
            .get("workspace_path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        let mut cmd = std::process::Command::new(&argv[0]);
        if argv.len() > 1 {
            cmd.args(&argv[1..]);
        }
        cmd.current_dir(cwd);
        for (k, v) in &self.env_vars {
            cmd.env(k, v);
        }

        let child = match cmd.stdout(std::process::Stdio::piped())
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

        let deadline = std::time::Duration::from_millis(self.timeout_ms);
        let wait_start = std::time::Instant::now();
        let mut child = child;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if wait_start.elapsed() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
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
        }

        let output = match child.wait_with_output() {
            Ok(o) => o,
            Err(e) => {
                return NodeExecutionOutput {
                    status: "failed".to_string(),
                    executor_type: "command".to_string(),
                    output: None,
                    error_domain: Some("command_output_error".to_string()),
                    error_message: Some(e.to_string()),
                    input_tokens: None,
                    output_tokens: None,
                    estimated_cost: None,
                    latency_ms: Some(start.elapsed().as_millis() as i64),
                };
            }
        };

        let elapsed_ms = start.elapsed().as_millis() as i64;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);
        let combined = if stderr.is_empty() {
            stdout.clone()
        } else {
            format!("{stdout}\n[stderr]\n{stderr}")
        };

        if output.status.success() {
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
}
