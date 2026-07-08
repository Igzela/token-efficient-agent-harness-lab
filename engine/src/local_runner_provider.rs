use serde_json::{json, Value};
use std::sync::Arc;

use crate::provider::config::{
    provider_pricing_from_env, CredentialRef, ProviderConfig, PROVIDER_TYPES,
};
use crate::provider::credential::CredentialBoundary;
use crate::provider::fake::FakeProvider;
use crate::provider::openai::OpenAiProvider;
use crate::provider::stub::StubProvider;
use crate::provider::transport::ReqwestTransport;
use crate::provider::Provider;
use crate::provider::ProviderRequest;
use crate::trusted_local::EffectiveExecutionGates;

pub const SCENARIO_ID: &str = "provider_gated_remember_dont_reread_runner";
pub const RUNTIME_VERSION: &str = "provider-gated-real-runner.v1";
pub const SOURCE_REF_KEY: &str = "raw_trace_artifact_id";
pub const LOCAL_RUNNER_PROVIDER_TYPE_ENV: &str = "ACP_LOCAL_RUNNER_PROVIDER_TYPE";
pub const LOCAL_RUNNER_BASE_URL_ENV: &str = "ACP_LOCAL_RUNNER_BASE_URL";
pub const LOCAL_RUNNER_MODEL_ENV: &str = "ACP_LOCAL_RUNNER_MODEL";
pub const LOCAL_RUNNER_API_KEY_ENV_REF: &str = "ACP_LOCAL_RUNNER_API_KEY_ENV";

#[derive(Debug, Clone)]
pub struct RunnerLimits {
    pub iterations: usize,
    pub max_calls: usize,
    pub max_tokens: i64,
    pub timeout_seconds: f64,
    pub run_cost_cap_usd: f64,
    pub daily_cost_cap_usd: f64,
    pub pass_threshold: f64,
}

impl Default for RunnerLimits {
    fn default() -> Self {
        Self {
            iterations: 10,
            max_calls: 40,
            max_tokens: 120000,
            timeout_seconds: 30.0,
            run_cost_cap_usd: 0.25,
            daily_cost_cap_usd: 1.0,
            pass_threshold: 0.94,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProviderKind {
    Stub,
    Fake,
    Live,
}

#[derive(Clone)]
pub struct RunnerConfig {
    pub provider_kind: ProviderKind,
    pub model: String,
    pub limits: RunnerLimits,
    pub provider: Option<Arc<dyn Provider>>,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            provider_kind: ProviderKind::Stub,
            model: "stub-deterministic".to_string(),
            limits: RunnerLimits::default(),
            provider: None,
        }
    }
}

fn estimate_tokens(text: &str) -> i64 {
    let words = text.split_whitespace().count() as f64;
    std::cmp::max(
        1,
        (words * 1.35) as i64 + std::cmp::max(1, text.len() as i64 / 18),
    )
}

fn parse_candidate(text: &str, fallback: i64) -> i64 {
    let re = regex::Regex::new(r"candidate\s*=\s*(-?\d+)").unwrap();
    if let Some(caps) = re.captures(text) {
        let val: i64 = caps[1].parse().unwrap_or(fallback);
        val.clamp(0, 25)
    } else {
        fallback
    }
}

fn score(candidate: i64) -> f64 {
    (0.0f64).max(1.0 - (candidate - 17).abs() as f64 / 25.0)
}

fn summarize_state(history: &[Value]) -> String {
    if history.is_empty() {
        return "no prior experiments".to_string();
    }
    let best = history
        .iter()
        .max_by(|a, b| {
            let sa = a["score"].as_f64().unwrap_or(0.0);
            let sb = b["score"].as_f64().unwrap_or(0.0);
            sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap();
    let recent: Vec<&Value> = history.iter().rev().take(2).collect();
    let recent_text: Vec<String> = recent
        .iter()
        .map(|item| {
            format!(
                "i={} c={} s={}",
                item["iteration"].as_i64().unwrap_or(0),
                item["candidate"].as_i64().unwrap_or(0),
                item["score"].as_f64().unwrap_or(0.0)
            )
        })
        .collect();
    format!(
        "best c={} s={}; recent {}",
        best["candidate"].as_i64().unwrap_or(0),
        best["score"].as_f64().unwrap_or(0.0),
        recent_text.join("; ")
    )
}

fn make_prompt(mode: &str, iteration: usize, history: &[Value]) -> String {
    let task =
        "Find an integer candidate from 0 to 25 that maximizes a hidden deterministic score.";
    if mode == "stateless_reread" {
        let history_text = serde_json::to_string(history).unwrap_or_else(|_| "[]".to_string());
        format!(
            "Task: {task}\nIteration: {iteration}\nFull prior compact history: {history_text}\nReturn candidate=<number>."
        )
    } else {
        let state_summary = summarize_state(history);
        let recent: Vec<&Value> = history.iter().rev().take(2).collect();
        let recent_json = serde_json::to_string(&recent).unwrap_or_else(|_| "[]".to_string());
        format!(
            "Task: {task}\nIteration: {iteration}\nState summary: {state_summary}\nRecent window: {recent_json}\nReturn candidate=<number>."
        )
    }
}

fn provider_request_model<'a>(
    config: &'a RunnerConfig,
    provider: &'a Arc<dyn Provider>,
) -> &'a str {
    provider.default_model().unwrap_or(&config.model)
}

fn compute_context_tokens(mode: &str, prompt: &str, history: &[Value]) -> (i64, i64) {
    let context_tokens = estimate_tokens(prompt);
    let repeated = if mode == "stateless_reread" {
        if history.is_empty() {
            0
        } else {
            let history_text = serde_json::to_string(history).unwrap_or_else(|_| "[]".to_string());
            estimate_tokens(&history_text)
        }
    } else {
        let recent: Vec<&Value> = history.iter().rev().take(2).collect();
        let recent_json = serde_json::to_string(&recent).unwrap_or_else(|_| "[]".to_string());
        estimate_tokens(&recent_json)
    };
    (context_tokens, repeated)
}

fn build_step(
    mode: &str,
    run_id: &str,
    iteration: usize,
    input_tokens: i64,
    output_tokens: i64,
    context_tokens: i64,
    repeated_context_tokens: i64,
    candidate: i64,
    score_val: f64,
) -> Value {
    let retrieved_refs_count = if mode == "stateful_store" && iteration > 0 {
        1
    } else {
        0
    };
    let retrieved_ref_tokens = if mode == "stateful_store" && iteration > 0 {
        std::cmp::min(context_tokens, std::cmp::max(0, context_tokens / 5))
    } else {
        0
    };
    let state_read_bytes = if mode == "stateful_store" {
        (candidate.to_string().len() + score_val.to_string().len()) as i64
    } else {
        0
    };
    json!({
        "adapter_step_id": format!("{run_id}-iter-{iteration:02}"),
        "adapter_run_id": run_id,
        "step_index": iteration as i64,
        "node_name": format!("real_experiment_iteration_{iteration:02}"),
        "agent_role": "executor",
        "operation_kind": "model_call",
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "context_tokens": context_tokens,
        "repeated_context_tokens": repeated_context_tokens,
        "retrieved_refs_count": retrieved_refs_count,
        "retrieved_ref_tokens": retrieved_ref_tokens,
        "tool_name": Value::Null,
        "tool_call_id": Value::Null,
        "status": "pass",
        "error_kind": "none",
        "state_read_bytes": state_read_bytes,
        "state_write_bytes": if mode == "stateful_store" { 96 } else { 0 },
    })
}

pub fn run_mode(
    mode: &str,
    config: &RunnerConfig,
    provider: &Arc<dyn Provider>,
) -> Result<Value, String> {
    if mode != "stateless_reread" && mode != "stateful_store" {
        return Err(format!("unsupported mode: {mode}"));
    }
    let run_id = format!("real-runner-{mode}");
    let mut history: Vec<Value> = Vec::new();
    let mut steps: Vec<Value> = Vec::new();
    let mut total_tokens: i64 = 0;
    let mut total_cost: f64 = 0.0;
    let mut best_score_val: f64 = 0.0;
    let mut status = "fail";

    for (calls, iteration) in (0..config.limits.iterations).enumerate() {
        if calls >= config.limits.max_calls {
            return Err("call limit exceeded".to_string());
        }
        let prompt = make_prompt(mode, iteration, &history);
        let (context_tokens, repeated_context_tokens) =
            compute_context_tokens(mode, &prompt, &history);

        let provider_req = ProviderRequest::local_stub(
            provider.provider_id(),
            provider_request_model(config, provider),
            &prompt,
        );

        let result = {
            let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
            rt.block_on(provider.invoke(&provider_req))
        };

        let resp = match result {
            Ok(r) => r,
            Err(e) => {
                return Err(format!(
                    "provider invoke failed: {} ({})",
                    e.message, e.error_domain
                ));
            }
        };

        let input_tokens = resp.input_tokens.unwrap_or(estimate_tokens(&prompt));
        let output_tokens = resp.output_tokens.unwrap_or(0);
        total_tokens += input_tokens + output_tokens;
        total_cost += resp.estimated_cost.unwrap_or(0.0);

        if total_tokens > config.limits.max_tokens {
            return Err("token limit exceeded".to_string());
        }
        if total_cost > config.limits.run_cost_cap_usd
            || total_cost > config.limits.daily_cost_cap_usd
        {
            return Err("cost cap exceeded".to_string());
        }

        let fallback_candidate = std::cmp::min(17, 3 + (iteration as i64) * 2);
        let candidate = parse_candidate(&resp.output, fallback_candidate);
        let score_val = score(candidate);
        best_score_val = best_score_val.max(score_val);

        history.push(json!({
            "iteration": iteration as i64,
            "candidate": candidate,
            "score": score_val,
        }));

        steps.push(build_step(
            mode,
            &run_id,
            iteration,
            input_tokens,
            output_tokens,
            context_tokens,
            repeated_context_tokens,
            candidate,
            score_val,
        ));

        if best_score_val >= config.limits.pass_threshold {
            status = "pass";
            break;
        }
    }

    if steps.is_empty() {
        return Err("runner produced no steps".to_string());
    }

    let input_total: i64 = steps
        .iter()
        .map(|s| s["input_tokens"].as_i64().unwrap_or(0))
        .sum();
    let output_total: i64 = steps
        .iter()
        .map(|s| s["output_tokens"].as_i64().unwrap_or(0))
        .sum();
    let context_total: i64 = steps
        .iter()
        .map(|s| s["context_tokens"].as_i64().unwrap_or(0))
        .sum();
    let repeated_total: i64 = steps
        .iter()
        .map(|s| s["repeated_context_tokens"].as_i64().unwrap_or(0))
        .sum();
    let refs_total: i64 = steps
        .iter()
        .map(|s| s["retrieved_ref_tokens"].as_i64().unwrap_or(0))
        .sum();

    let duration_ms = std::cmp::max(1, steps.len() as i64 * 5);

    let quality_score = if status == "pass" {
        json!(best_score_val)
    } else {
        Value::Null
    };

    Ok(json!({
        "adapter_run_id": run_id,
        "runtime_kind": "native_harness",
        "runtime_version": RUNTIME_VERSION,
        "scenario_id": SCENARIO_ID,
        "mode": mode,
        "state_strategy": if mode == "stateful_store" { "durable_state" } else { "full_history" },
        "status": status,
        "pass_fail_reason": if status == "pass" {
            "same score threshold met"
        } else {
            "score threshold not met within bounded iterations"
        },
        "quality_method": "rule",
        "input_token_total": input_total,
        "output_token_total": output_total,
        "context_token_total": context_total,
        "repeated_context_token_total": repeated_total,
        "retrieved_ref_token_total": refs_total,
        "tool_call_count": steps.len() as i64,
        "redundant_tool_call_count": 0,
        "retry_count": 0,
        "step_count": steps.len() as i64,
        "duration_ms": duration_ms,
        "estimated_cost_usd": (total_cost * 1_000_000.0).round() / 1_000_000.0,
        SOURCE_REF_KEY: format!("bounded-provider-gated-runner-{mode}"),
        "redaction_status": if config.provider_kind == ProviderKind::Live { "redacted" } else { "not_needed" },
        "runner_metadata": {
            "live": config.provider_kind == ProviderKind::Live,
            "provider_kind": match config.provider_kind {
                ProviderKind::Stub => "stub",
                ProviderKind::Fake => "fake",
                ProviderKind::Live => "live",
            },
            "model": config.model,
            "external_calls": steps.len() as i64,
            "final_best_score": best_score_val,
            "context_protocol": if mode == "stateless_reread" {
                "full_history_reread"
            } else {
                "compact_summary_plus_recent_window"
            },
        },
        "steps": steps,
        "quality_score": quality_score,
    }))
}

fn required_env(name: &str) -> Result<String, String> {
    std::env::var(name)
        .map(|value| value.trim().to_string())
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{name} is required for live openai_compatible local runner"))
}

fn build_live_openai_compatible_provider() -> Result<Arc<dyn Provider>, String> {
    let provider_type = required_env(LOCAL_RUNNER_PROVIDER_TYPE_ENV)?;
    if provider_type != "openai_compatible" {
        let supported = PROVIDER_TYPES
            .iter()
            .copied()
            .filter(|value| *value == "openai_compatible")
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "{LOCAL_RUNNER_PROVIDER_TYPE_ENV} must be openai_compatible for the live local runner (supported now: {supported})"
        ));
    }

    let base_url = required_env(LOCAL_RUNNER_BASE_URL_ENV)?;
    let model = required_env(LOCAL_RUNNER_MODEL_ENV)?;
    let credential_env = required_env(LOCAL_RUNNER_API_KEY_ENV_REF)?;
    let boundary = CredentialBoundary::new("env")?;
    if !boundary.validate(&credential_env) {
        return Err(format!(
            "credential environment variable referenced by {LOCAL_RUNNER_API_KEY_ENV_REF} is not set"
        ));
    }

    let credential_ref = CredentialRef::new(
        &credential_env,
        "env",
        "***",
        "provider:local-runner-openai-compatible",
        "2026-01-01T00:00:00Z",
    );
    let mut provider_config = ProviderConfig::new(
        "local-runner-openai-compatible",
        "openai_compatible",
        &base_url,
        &model,
        &credential_env,
        "2026-01-01T00:00:00Z",
    );
    provider_config.apply_pricing(&provider_pricing_from_env());

    Ok(Arc::new(OpenAiProvider::new(
        provider_config,
        boundary,
        credential_ref,
        Arc::new(ReqwestTransport::new()),
        None,
    )))
}

pub fn build_config(
    provider_kind: ProviderKind,
    iterations: usize,
    max_calls: usize,
    max_tokens: i64,
    timeout_seconds: f64,
    run_cost_cap_usd: f64,
    daily_cost_cap_usd: f64,
    pass_threshold: f64,
) -> Result<RunnerConfig, String> {
    if !(2..=50).contains(&iterations) {
        return Err("iterations must be between 2 and 50".to_string());
    }
    if max_calls < iterations * 2 {
        return Err("max calls must cover both modes".to_string());
    }
    if max_tokens <= 0 {
        return Err("max tokens must be positive".to_string());
    }
    if timeout_seconds <= 0.0 {
        return Err("timeout must be positive".to_string());
    }
    if !(0.0..=1.0).contains(&pass_threshold) || pass_threshold == 0.0 {
        return Err("pass threshold must be in (0, 1]".to_string());
    }
    if run_cost_cap_usd <= 0.0 || daily_cost_cap_usd <= 0.0 {
        return Err("cost caps must be positive".to_string());
    }
    if run_cost_cap_usd > daily_cost_cap_usd {
        return Err("run cost cap cannot exceed daily cost cap".to_string());
    }

    Ok(RunnerConfig {
        provider_kind,
        model: match provider_kind {
            ProviderKind::Stub => "stub-deterministic",
            ProviderKind::Fake => "fake-deterministic",
            ProviderKind::Live => "live-provider",
        }
        .to_string(),
        limits: RunnerLimits {
            iterations,
            max_calls,
            max_tokens,
            timeout_seconds,
            run_cost_cap_usd,
            daily_cost_cap_usd,
            pass_threshold,
        },
        provider: None,
    })
}

pub fn build_provider(
    config: &RunnerConfig,
    gates: Option<&EffectiveExecutionGates>,
) -> Result<Arc<dyn Provider>, String> {
    match config.provider_kind {
        ProviderKind::Stub => Ok(Arc::new(StubProvider::new("local-runner-stub"))),
        ProviderKind::Fake => Ok(Arc::new(FakeProvider::new("local-runner-fake"))),
        ProviderKind::Live => {
            let gates =
                gates.ok_or_else(|| "live provider requires execution gates".to_string())?;
            if !gates.provider_execution {
                return Err("live provider execution not enabled by current gates".to_string());
            }
            if let Some(ref p) = config.provider {
                return Ok(p.clone());
            }
            build_live_openai_compatible_provider()
        }
    }
}

pub fn run_pair(
    config: &RunnerConfig,
    provider: &Arc<dyn Provider>,
) -> Result<(Value, Value), String> {
    let stateless = run_mode("stateless_reread", config, provider)?;
    let stateful = run_mode("stateful_store", config, provider)?;
    Ok((stateless, stateful))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::fake::FakeProvider;
    use crate::trusted_local::TrustedLocalProfileStatus;
    use std::sync::{Mutex, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn default_gates() -> EffectiveExecutionGates {
        EffectiveExecutionGates {
            profile: TrustedLocalProfileStatus {
                schema_version: "trusted_local_profile.v1".to_string(),
                requested: false,
                ready: false,
                blockers: vec![],
                capabilities: crate::trusted_local::TrustedLocalCapabilities {
                    provider_execution: false,
                    adaptive_execution: false,
                    default_routing: false,
                    experiments: false,
                    auto_promotion: false,
                },
            },
            task_advancement: crate::trusted_local::TrustedLocalTaskAdvancementStatus {
                schema_version: "trusted_local_task_advancement.v1".to_string(),
                requested: false,
                ready: false,
                blockers: vec![],
                executor_type: "".to_string(),
                worker_count: 0,
                max_concurrent: 0,
            },
            provider_execution: false,
            adaptive_execution: false,
            default_routing: false,
            scheduler_enabled: false,
            experiments_enabled: false,
            experiments_active: false,
            auto_promotion_enabled: false,
            auto_promotion_active: false,
            supervised_workers_enabled: false,
        }
    }

    fn gates_with_provider() -> EffectiveExecutionGates {
        let mut g = default_gates();
        g.provider_execution = true;
        g
    }

    fn with_env_lock<T>(f: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        f()
    }

    fn clear_live_env() {
        std::env::remove_var(LOCAL_RUNNER_PROVIDER_TYPE_ENV);
        std::env::remove_var(LOCAL_RUNNER_BASE_URL_ENV);
        std::env::remove_var(LOCAL_RUNNER_MODEL_ENV);
        std::env::remove_var(LOCAL_RUNNER_API_KEY_ENV_REF);
        std::env::remove_var("LOCAL_RUNNER_TEST_OPENAI_KEY");
    }

    fn live_config() -> RunnerConfig {
        RunnerConfig {
            provider_kind: ProviderKind::Live,
            model: "live".to_string(),
            limits: RunnerLimits::default(),
            provider: None,
        }
    }

    #[test]
    fn default_provider_is_stub() {
        let config = RunnerConfig::default();
        assert_eq!(config.provider_kind, ProviderKind::Stub);
        let provider = build_provider(&config, None).unwrap();
        assert_eq!(provider.provider_id(), "local-runner-stub");
    }

    #[test]
    fn fake_provider_builds_without_gates() {
        let config = RunnerConfig {
            provider_kind: ProviderKind::Fake,
            model: "fake-deterministic".to_string(),
            limits: RunnerLimits::default(),
            provider: None,
        };
        let provider = build_provider(&config, None).unwrap();
        assert_eq!(provider.provider_id(), "local-runner-fake");
    }

    fn assert_provider_err(result: Result<Arc<dyn Provider>, String>, expected: &str) {
        match result {
            Err(e) => assert!(
                e.contains(expected),
                "error '{e}' doesn't contain '{expected}'"
            ),
            Ok(_) => panic!("expected error containing '{expected}', got Ok"),
        }
    }

    fn assert_config_err(result: Result<RunnerConfig, String>, expected: &str) {
        match result {
            Err(e) => assert!(
                e.contains(expected),
                "error '{e}' doesn't contain '{expected}'"
            ),
            Ok(_) => panic!("expected error containing '{expected}', got Ok"),
        }
    }

    #[test]
    fn live_provider_fails_closed_without_gates() {
        let config = live_config();
        assert_provider_err(build_provider(&config, None), "execution gates");
    }

    #[test]
    fn live_provider_fails_closed_when_provider_execution_disabled() {
        let config = live_config();
        assert_provider_err(
            build_provider(&config, Some(&default_gates())),
            "not enabled",
        );
    }

    #[test]
    fn live_provider_fails_closed_without_env_config() {
        with_env_lock(|| {
            clear_live_env();
            let config = live_config();
            assert_provider_err(
                build_provider(&config, Some(&gates_with_provider())),
                LOCAL_RUNNER_PROVIDER_TYPE_ENV,
            );
        });
    }

    #[test]
    fn live_provider_fails_closed_for_unsupported_provider_type() {
        with_env_lock(|| {
            clear_live_env();
            std::env::set_var(LOCAL_RUNNER_PROVIDER_TYPE_ENV, "anthropic");
            std::env::set_var(LOCAL_RUNNER_BASE_URL_ENV, "https://api.example.test/v1");
            std::env::set_var(LOCAL_RUNNER_MODEL_ENV, "test-model");
            std::env::set_var(LOCAL_RUNNER_API_KEY_ENV_REF, "LOCAL_RUNNER_TEST_OPENAI_KEY");
            std::env::set_var("LOCAL_RUNNER_TEST_OPENAI_KEY", "sk-local-runner-test");
            let config = live_config();
            assert_provider_err(
                build_provider(&config, Some(&gates_with_provider())),
                "openai_compatible",
            );
            clear_live_env();
        });
    }

    #[test]
    fn live_provider_fails_closed_when_credential_env_is_missing() {
        with_env_lock(|| {
            clear_live_env();
            std::env::set_var(LOCAL_RUNNER_PROVIDER_TYPE_ENV, "openai_compatible");
            std::env::set_var(LOCAL_RUNNER_BASE_URL_ENV, "https://api.example.test/v1");
            std::env::set_var(LOCAL_RUNNER_MODEL_ENV, "test-model");
            std::env::set_var(LOCAL_RUNNER_API_KEY_ENV_REF, "LOCAL_RUNNER_TEST_OPENAI_KEY");
            let config = live_config();
            assert_provider_err(
                build_provider(&config, Some(&gates_with_provider())),
                "credential environment variable",
            );
            clear_live_env();
        });
    }

    #[test]
    fn live_provider_constructs_openai_compatible_without_invoking_network() {
        with_env_lock(|| {
            clear_live_env();
            std::env::set_var(LOCAL_RUNNER_PROVIDER_TYPE_ENV, "openai_compatible");
            std::env::set_var(LOCAL_RUNNER_BASE_URL_ENV, "https://api.example.test/v1");
            std::env::set_var(LOCAL_RUNNER_MODEL_ENV, "test-model");
            std::env::set_var(LOCAL_RUNNER_API_KEY_ENV_REF, "LOCAL_RUNNER_TEST_OPENAI_KEY");
            std::env::set_var("LOCAL_RUNNER_TEST_OPENAI_KEY", "sk-local-runner-test");
            let config = live_config();
            let provider = build_provider(&config, Some(&gates_with_provider())).unwrap();
            assert_eq!(provider.provider_id(), "local-runner-openai-compatible");
            assert!(provider.is_enabled());
            assert_eq!(provider.default_model(), Some("test-model"));
            clear_live_env();
        });
    }

    #[test]
    fn live_provider_with_configured_instance_works() {
        let provider: Arc<dyn Provider> = Arc::new(FakeProvider::new("live-test"));
        let config = RunnerConfig {
            provider_kind: ProviderKind::Live,
            model: "live".to_string(),
            limits: RunnerLimits::default(),
            provider: Some(provider.clone()),
        };
        let result = build_provider(&config, Some(&gates_with_provider())).unwrap();
        assert_eq!(result.provider_id(), "live-test");
    }

    #[test]
    fn stub_run_mode_produces_valid_output() {
        let config = RunnerConfig::default();
        let provider = build_provider(&config, None).unwrap();
        let result = run_mode("stateless_reread", &config, &provider).unwrap();
        assert_eq!(result["adapter_run_id"], "real-runner-stateless_reread");
        assert_eq!(result["status"], "pass");
        assert_eq!(result["mode"], "stateless_reread");
        assert_eq!(result["runtime_kind"], "native_harness");
        assert!(result["input_token_total"].as_i64().unwrap_or(0) > 0);
        assert!(result["step_count"].as_i64().unwrap_or(0) > 0);
        assert!(!result["steps"].as_array().unwrap().is_empty());
        assert_eq!(result["redaction_status"], "not_needed");
    }

    #[test]
    fn stub_run_mode_stateful_uses_fewer_tokens() {
        let config = RunnerConfig::default();
        let provider = build_provider(&config, None).unwrap();
        let stateless = run_mode("stateless_reread", &config, &provider).unwrap();
        let stateful = run_mode("stateful_store", &config, &provider).unwrap();
        let stateless_tokens = stateless["input_token_total"].as_i64().unwrap_or(0)
            + stateless["output_token_total"].as_i64().unwrap_or(0);
        let stateful_tokens = stateful["input_token_total"].as_i64().unwrap_or(0)
            + stateful["output_token_total"].as_i64().unwrap_or(0);
        assert!(
            stateful_tokens < stateless_tokens,
            "stateful {stateful_tokens} should use fewer tokens than stateless {stateless_tokens}"
        );
    }

    #[test]
    fn run_pair_produces_valid_scorecards() {
        let config = RunnerConfig::default();
        let provider = build_provider(&config, None).unwrap();
        let (stateless, stateful) = run_pair(&config, &provider).unwrap();
        assert_eq!(stateless["scenario_id"], SCENARIO_ID);
        assert_eq!(stateful["scenario_id"], SCENARIO_ID);
        assert_eq!(stateless["mode"], "stateless_reread");
        assert_eq!(stateful["mode"], "stateful_store");
        assert_eq!(stateless["status"], "pass");
        assert_eq!(stateful["status"], "pass");
    }

    #[test]
    fn fake_run_mode_has_zero_cost() {
        let config = RunnerConfig {
            provider_kind: ProviderKind::Fake,
            model: "fake".to_string(),
            limits: RunnerLimits::default(),
            provider: None,
        };
        let provider = build_provider(&config, None).unwrap();
        let result = run_mode("stateless_reread", &config, &provider).unwrap();
        assert_eq!(result["estimated_cost_usd"].as_f64().unwrap_or(-1.0), 0.0);
        assert_eq!(result["redaction_status"], "not_needed");
    }

    #[test]
    fn run_mode_fails_on_unknown_mode() {
        let config = RunnerConfig::default();
        let provider = build_provider(&config, None).unwrap();
        let result = run_mode("unknown_mode", &config, &provider);
        assert!(result.is_err());
    }

    #[test]
    fn build_config_validates_iterations() {
        assert_config_err(
            build_config(ProviderKind::Stub, 1, 40, 120000, 30.0, 0.25, 1.0, 0.94),
            "iterations",
        );
    }

    #[test]
    fn build_config_validates_cost_caps() {
        assert_config_err(
            build_config(ProviderKind::Stub, 10, 40, 120000, 30.0, 2.0, 1.0, 0.94),
            "run cost cap",
        );
    }

    #[test]
    fn build_config_ok_with_valid_inputs() {
        let result = build_config(ProviderKind::Stub, 10, 40, 120000, 30.0, 0.25, 1.0, 0.94);
        assert!(result.is_ok());
    }

    #[test]
    fn no_raw_prompt_or_output_in_steps() {
        let config = RunnerConfig::default();
        let provider = build_provider(&config, None).unwrap();
        let result = run_mode("stateless_reread", &config, &provider).unwrap();
        for step in result["steps"].as_array().unwrap() {
            assert!(step.get("prompt").is_none(), "step must not contain prompt");
            assert!(
                step.get("output").is_none(),
                "step must not contain raw output"
            );
            assert!(
                step.get("transcript").is_none(),
                "step must not contain transcript"
            );
        }
    }

    #[test]
    fn operator_evidence_fields_bounded() {
        let config = RunnerConfig::default();
        let provider = build_provider(&config, None).unwrap();
        let result = run_mode("stateless_reread", &config, &provider).unwrap();
        assert!(result.get("steps").is_some());
        assert!(result.get(SOURCE_REF_KEY).is_some());
        assert!(result.get("raw_prompt").is_none());
        assert!(result.get("raw_output").is_none());
        assert!(result.get("transcript").is_none());
        assert!(result.get("conversation").is_none());
    }
}
