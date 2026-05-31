use std::process::{Command, Stdio};

use crate::cli::spawn_with_timeout;
use crate::dispatch_decision::DispatchDecision;
use crate::executor_adapter::{ExecutionResult, Executor};
use crate::runtime::FixtureRuntime;

pub struct ClaudeCodeCliExecutor {
    bin_path: String,
    timeout_ms: u64,
}

impl ClaudeCodeCliExecutor {
    pub fn new(bin_path: String, timeout_ms: u64) -> Self {
        Self {
            bin_path,
            timeout_ms,
        }
    }
}

impl Executor for ClaudeCodeCliExecutor {
    fn execute(
        &self,
        decision: &DispatchDecision,
        raw_request: &str,
        dispatch_id: &str,
        runtime: &mut FixtureRuntime,
    ) -> ExecutionResult {
        let start = std::time::Instant::now();

        let model = decision
            .analysis_snapshot
            .get("selected_model")
            .and_then(|v| v.as_str())
            .unwrap_or("default");

        let mut cmd = Command::new(&self.bin_path);
        cmd.arg("-p")
            .arg(raw_request)
            .arg("--output-format")
            .arg("json")
            .arg("--model")
            .arg(model)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = spawn_with_timeout(&mut cmd, self.timeout_ms);

        let elapsed = start.elapsed().as_millis() as i64;

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
                    return ExecutionResult {
                        schema_version: "execution_result.v1".to_string(),
                        result_id: runtime.id("exec-"),
                        dispatch_id: dispatch_id.to_string(),
                        decision_id: decision.decision_id.clone(),
                        executor_type: "claude_code_cli".to_string(),
                        status: "failed".to_string(),
                        output: if stdout.is_empty() {
                            None
                        } else {
                            Some(stdout)
                        },
                        prompt_pack: None,
                        input_tokens: None,
                        output_tokens: None,
                        estimated_cost: None,
                        latency_ms: Some(elapsed),
                        error_domain: Some("cli_execution_error".to_string()),
                        error_message: Some(msg),
                        provider_request_id: None,
                        attempt_number: Some(1),
                        finish_reason: Some("cli_error".to_string()),
                        usage_source: Some("claude_code_cli".to_string()),
                        created_at: runtime.now(),
                    };
                }

                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                parse_claude_output(&stdout, decision, dispatch_id, elapsed, runtime)
            }
            Err(timeout_elapsed) => {
                let (domain, msg, reason) = if timeout_elapsed == 0 {
                    (
                        "cli_not_found",
                        "failed to spawn claude".to_string(),
                        "spawn_error",
                    )
                } else {
                    (
                        "cli_timeout",
                        format!(
                            "claude timed out after {}ms (limit {}ms)",
                            timeout_elapsed, self.timeout_ms
                        ),
                        "timeout",
                    )
                };
                ExecutionResult {
                    schema_version: "execution_result.v1".to_string(),
                    result_id: runtime.id("exec-"),
                    dispatch_id: dispatch_id.to_string(),
                    decision_id: decision.decision_id.clone(),
                    executor_type: "claude_code_cli".to_string(),
                    status: "failed".to_string(),
                    output: None,
                    prompt_pack: None,
                    input_tokens: None,
                    output_tokens: None,
                    estimated_cost: None,
                    latency_ms: Some(elapsed),
                    error_domain: Some(domain.to_string()),
                    error_message: Some(msg),
                    provider_request_id: None,
                    attempt_number: Some(1),
                    finish_reason: Some(reason.to_string()),
                    usage_source: Some("claude_code_cli".to_string()),
                    created_at: runtime.now(),
                }
            }
        }
    }
}

fn parse_claude_output(
    raw: &str,
    decision: &DispatchDecision,
    dispatch_id: &str,
    latency_ms: i64,
    runtime: &mut FixtureRuntime,
) -> ExecutionResult {
    let parsed: serde_json::Value = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(err) => {
            return ExecutionResult {
                schema_version: "execution_result.v1".to_string(),
                result_id: runtime.id("exec-"),
                dispatch_id: dispatch_id.to_string(),
                decision_id: decision.decision_id.clone(),
                executor_type: "claude_code_cli".to_string(),
                status: "failed".to_string(),
                output: if raw.is_empty() {
                    None
                } else {
                    Some(raw.to_string())
                },
                prompt_pack: None,
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(latency_ms),
                error_domain: Some("cli_output_parse_error".to_string()),
                error_message: Some(format!("failed to parse claude JSON output: {err}")),
                provider_request_id: None,
                attempt_number: Some(1),
                finish_reason: Some("parse_error".to_string()),
                usage_source: Some("claude_code_cli".to_string()),
                created_at: runtime.now(),
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

    let estimated_cost = compute_cli_cost(
        "claude_code_cli",
        input_tokens.unwrap_or(0),
        output_tokens.unwrap_or(0),
    );

    ExecutionResult {
        schema_version: "execution_result.v1".to_string(),
        result_id: runtime.id("exec-"),
        dispatch_id: dispatch_id.to_string(),
        decision_id: decision.decision_id.clone(),
        executor_type: "claude_code_cli".to_string(),
        status: "cli_completed".to_string(),
        output: Some(output_text),
        prompt_pack: None,
        input_tokens,
        output_tokens,
        estimated_cost: Some(estimated_cost),
        latency_ms: Some(latency_ms),
        error_domain: None,
        error_message: None,
        provider_request_id: parsed.get("id").and_then(|v| v.as_str()).map(String::from),
        attempt_number: Some(1),
        finish_reason: Some("cli_success".to_string()),
        usage_source: Some("claude_code_cli".to_string()),
        created_at: runtime.now(),
    }
}

pub fn compute_cli_cost(tier: &str, input_tokens: i64, output_tokens: i64) -> f64 {
    let (input_rate, output_rate) = match tier {
        "claude_code_cli" => (0.015, 0.075),
        "codex_cli" => (0.003, 0.015),
        _ => (0.003, 0.015),
    };
    (input_tokens as f64 / 1000.0 * input_rate) + (output_tokens as f64 / 1000.0 * output_rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_cli_cost_claude() {
        let cost = compute_cli_cost("claude_code_cli", 1000, 500);
        assert!((cost - 0.0525).abs() < 0.0001);
    }

    #[test]
    fn test_compute_cli_cost_codex() {
        let cost = compute_cli_cost("codex_cli", 1000, 500);
        assert!((cost - 0.0105).abs() < 0.0001);
    }

    #[test]
    fn test_compute_cli_cost_zero_tokens() {
        let cost = compute_cli_cost("claude_code_cli", 0, 0);
        assert_eq!(cost, 0.0);
    }

    fn decision() -> DispatchDecision {
        DispatchDecision {
            decision_id: "dec-test".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn malformed_claude_json_is_a_failed_execution() {
        let mut runtime = FixtureRuntime::new();
        let result = parse_claude_output("not-json", &decision(), "disp-test", 3, &mut runtime);

        assert_eq!(result.executor_type, "claude_code_cli");
        assert_eq!(result.status, "failed");
        assert_eq!(
            result.error_domain.as_deref(),
            Some("cli_output_parse_error")
        );
        assert_eq!(result.finish_reason.as_deref(), Some("parse_error"));
        assert_eq!(result.output.as_deref(), Some("not-json"));
        assert_eq!(result.estimated_cost, None);
    }
}
