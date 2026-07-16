use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::infrastructure::circuit_breaker::CircuitBreaker;
use crate::provider::circuit_breaker_provider::CircuitBreakerProvider;
use crate::provider::config::{
    provider_pricing_from_env, CredentialRef, ProviderConfig, PROVIDER_TYPES,
};
use crate::provider::credential::CredentialBoundary;
use crate::provider::fake::FakeProvider;
use crate::provider::openai::OpenAiProvider;
use crate::provider::stub::StubProvider;
use crate::provider::transport::ReqwestTransport;
use crate::provider::{
    Provider, ProviderAuditRecorder, ProviderError, ProviderRequest, ProviderResult,
};
use crate::storage::local_product_store::LocalProductStore;
use crate::trusted_local::EffectiveExecutionGates;

pub const SCENARIO_ID: &str = "provider_gated_remember_dont_reread_runner";
pub const RUNTIME_VERSION: &str = "provider-gated-real-runner.v1";
pub const SOURCE_REF_KEY: &str = "raw_trace_artifact_id";
pub const LOCAL_RUNNER_PROVIDER_TYPE_ENV: &str = "ACP_LOCAL_RUNNER_PROVIDER_TYPE";
pub const LOCAL_RUNNER_BASE_URL_ENV: &str = "ACP_LOCAL_RUNNER_BASE_URL";
pub const LOCAL_RUNNER_MODEL_ENV: &str = "ACP_LOCAL_RUNNER_MODEL";
pub const LOCAL_RUNNER_API_KEY_ENV_REF: &str = "ACP_LOCAL_RUNNER_API_KEY_ENV";
pub const LOCAL_RUNNER_KILL_SWITCH_ENV: &str = "ACP_LOCAL_RUNNER_KILL_SWITCH";

struct AuditedProvider {
    inner: Arc<dyn Provider>,
    audit: Arc<ProviderAuditRecorder>,
}

impl AuditedProvider {
    fn new(inner: Arc<dyn Provider>, store: Arc<LocalProductStore>) -> Self {
        Self {
            inner,
            audit: Arc::new(ProviderAuditRecorder::with_store(store)),
        }
    }

    fn audit_error(&self) -> ProviderError {
        ProviderError {
            schema_version: "provider_error.v1".to_string(),
            provider_id: self.inner.provider_id().to_string(),
            error_domain: "provider_audit".to_string(),
            message: "provider audit persistence failed".to_string(),
            retryable: false,
        }
    }
}

#[async_trait::async_trait]
impl Provider for AuditedProvider {
    fn provider_id(&self) -> &str {
        self.inner.provider_id()
    }

    fn is_enabled(&self) -> bool {
        self.inner.is_enabled()
    }

    fn default_model(&self) -> Option<&str> {
        self.inner.default_model()
    }

    async fn invoke(&self, request: &ProviderRequest) -> ProviderResult {
        let dispatch_id = request
            .metadata
            .get("dispatch_id")
            .and_then(Value::as_str)
            .unwrap_or("local-runner:unknown");
        let request_extra = json!({"redaction_status": "redacted"});
        self.audit
            .try_create_and_record(
                dispatch_id,
                self.inner.provider_id(),
                "request_sent",
                Some(&request_extra),
            )
            .map_err(|_| self.audit_error())?;

        let started = Instant::now();
        match self.inner.invoke(request).await {
            Ok(response) => {
                let extra = json!({
                    "input_token_count": response.input_tokens,
                    "output_token_count": response.output_tokens,
                    "cost": response.estimated_cost,
                    "currency": "USD",
                    "latency_ms": started.elapsed().as_millis() as i64,
                    "redaction_status": "redacted",
                });
                self.audit
                    .try_create_and_record(
                        dispatch_id,
                        self.inner.provider_id(),
                        "response_received",
                        Some(&extra),
                    )
                    .map_err(|_| self.audit_error())?;
                Ok(response)
            }
            Err(error) => {
                let extra = json!({
                    "error_domain": error.error_domain,
                    "latency_ms": started.elapsed().as_millis() as i64,
                    "redaction_status": "redacted",
                });
                self.audit
                    .try_create_and_record(
                        dispatch_id,
                        self.inner.provider_id(),
                        "error",
                        Some(&extra),
                    )
                    .map_err(|_| self.audit_error())?;
                Err(error)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunnerLimits {
    pub iterations: usize,
    pub max_calls: usize,
    pub max_tokens: i64,
    pub max_output_tokens: i64,
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
            max_output_tokens: 1024,
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
    pub pricing: Option<RunnerPricing>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunnerPricing {
    pub input_cost_per_1k: f64,
    pub output_cost_per_1k: f64,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            provider_kind: ProviderKind::Stub,
            model: "stub-deterministic".to_string(),
            limits: RunnerLimits::default(),
            provider: None,
            pricing: None,
        }
    }
}

fn runner_pricing_from_env() -> Option<RunnerPricing> {
    let pricing = provider_pricing_from_env();
    let (input_cost_per_1k, output_cost_per_1k) =
        pricing.input_cost_per_1k.zip(pricing.output_cost_per_1k)?;
    (input_cost_per_1k >= 0.0 && output_cost_per_1k >= 0.0).then_some(RunnerPricing {
        input_cost_per_1k,
        output_cost_per_1k,
    })
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
    if matches!(mode, "static_all" | "deterministic_top_k") {
        let descriptors = if mode == "static_all" {
            "read, search, summarize, write"
        } else {
            "read, search, summarize"
        };
        let (task, required_tool) = if iteration == 0 {
            ("inspect bounded evidence", "read")
        } else {
            ("locate an approved source identifier", "search")
        };
        format!(
            "Task: {task}. Required capability: {required_tool}.\nCanonical task index: {iteration}\nAvailable tool descriptors: {descriptors}\nInvoke exactly one available tool."
        )
    } else {
        let task =
            "Find an integer candidate from 0 to 25 that maximizes a hidden deterministic score.";
        if matches!(mode, "stateless_reread" | "full_history") {
            let history_text = serde_json::to_string(history).unwrap_or_else(|_| "[]".to_string());
            format!(
            "Task: {task}\nIteration: {iteration}\nFull prior compact history: {history_text}\nReturn candidate=<number>."
        )
        } else if mode == "retrieval_memory" {
            let best = summarize_state(history);
            format!(
                "Task: {task}\nIteration: {iteration}\nRetrieved bounded reference: {best}\nReturn candidate=<number>."
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
}

fn tool_descriptors(mode: &str) -> Option<Value> {
    let names: &[(&str, &str)] = match mode {
        "static_all" => &[
            ("read", "Read one approved bounded source"),
            ("search", "Search approved source identifiers"),
            ("summarize", "Summarize bounded approved evidence"),
            ("write", "Write one approved bounded output"),
        ],
        "deterministic_top_k" => &[
            ("read", "Read one approved bounded source"),
            ("search", "Search approved source identifiers"),
            ("summarize", "Summarize bounded approved evidence"),
        ],
        _ => return None,
    };
    Some(Value::Array(
        names
            .iter()
            .map(|(name, description)| {
                json!({
                    "type":"function",
                    "function":{
                        "name":name,
                        "description":description,
                        "parameters":{"type":"object","properties":{},"additionalProperties":false}
                    }
                })
            })
            .collect(),
    ))
}

fn selected_tool(output: &str) -> Option<String> {
    let captures = regex::Regex::new(r"(?:^|;)tool=(read|search|summarize|write)(?:;|$)")
        .expect("bounded tool regex")
        .captures(output)?;
    Some(captures[1].to_string())
}

fn provider_request_model<'a>(
    config: &'a RunnerConfig,
    provider: &'a Arc<dyn Provider>,
) -> &'a str {
    provider.default_model().unwrap_or(&config.model)
}

fn compute_context_tokens(mode: &str, prompt: &str, history: &[Value]) -> (i64, i64) {
    let context_tokens = estimate_tokens(prompt);
    let repeated = if matches!(mode, "stateless_reread" | "full_history") {
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
    let retrieves = matches!(mode, "stateful_store" | "retrieval_memory") && iteration > 0;
    let durable = matches!(
        mode,
        "stateful_store" | "summary_memory" | "retrieval_memory" | "durable_state_bounded_recent"
    );
    let retrieved_refs_count = if retrieves { 1 } else { 0 };
    let retrieved_ref_tokens = if retrieves {
        std::cmp::min(context_tokens, std::cmp::max(0, context_tokens / 5))
    } else {
        0
    };
    let state_read_bytes = if durable {
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
        "state_write_bytes": if durable { 96 } else { 0 },
    })
}

#[derive(Default)]
struct RunnerUsage {
    calls: usize,
    tokens: i64,
    run_cost_usd: f64,
    prior_daily_cost_usd: f64,
    per_call_cost_cap_usd: Option<f64>,
}

pub fn run_mode(
    mode: &str,
    config: &RunnerConfig,
    provider: &Arc<dyn Provider>,
) -> Result<Value, String> {
    if config.provider_kind == ProviderKind::Live {
        return Err(
            "live provider requires persistent daily cost evidence; use the guarded live runner"
                .to_string(),
        );
    }
    run_mode_with_usage(mode, config, provider, &mut RunnerUsage::default())
}

pub fn run_mode_with_daily_cost(
    mode: &str,
    config: &RunnerConfig,
    provider: &Arc<dyn Provider>,
    prior_daily_cost_usd: f64,
) -> Result<Value, String> {
    if config.provider_kind != ProviderKind::Live {
        return Err("daily cost evidence is only accepted for live provider runs".to_string());
    }
    if !prior_daily_cost_usd.is_finite() || prior_daily_cost_usd < 0.0 {
        return Err("prior daily cost must be finite and non-negative".to_string());
    }
    let mut usage = RunnerUsage {
        prior_daily_cost_usd,
        ..RunnerUsage::default()
    };
    run_mode_with_usage(mode, config, provider, &mut usage)
}

fn run_mode_with_usage(
    mode: &str,
    config: &RunnerConfig,
    provider: &Arc<dyn Provider>,
    usage: &mut RunnerUsage,
) -> Result<Value, String> {
    if config.limits.max_output_tokens <= 0 {
        return Err("output token limit must be positive".to_string());
    }
    if !matches!(
        mode,
        "stateless_reread"
            | "stateful_store"
            | "full_history"
            | "summary_memory"
            | "retrieval_memory"
            | "durable_state_bounded_recent"
            | "static_all"
            | "deterministic_top_k"
    ) {
        return Err(format!("unsupported mode: {mode}"));
    }
    let started = Instant::now();
    let run_id = format!("real-runner-{mode}");
    let request_model = provider_request_model(config, provider).to_string();
    let mut history: Vec<Value> = Vec::new();
    let mut steps: Vec<Value> = Vec::new();
    let mut mode_cost: f64 = 0.0;
    let mut best_score_val: f64 = 0.0;
    let mut status = "fail";
    let mut selected_tool_ids = Vec::new();
    let mut correct_tool_selections = 0_i64;

    for iteration in 0..config.limits.iterations {
        if config.provider_kind == ProviderKind::Live
            && std::env::var(LOCAL_RUNNER_KILL_SWITCH_ENV).as_deref() == Ok("1")
        {
            return Err("local runner kill switch is active".to_string());
        }
        if usage.calls >= config.limits.max_calls {
            return Err("call limit exceeded".to_string());
        }
        if usage.run_cost_usd >= config.limits.run_cost_cap_usd {
            return Err("run cost cap reached".to_string());
        }
        if usage.prior_daily_cost_usd + usage.run_cost_usd >= config.limits.daily_cost_cap_usd {
            return Err("daily cost cap reached".to_string());
        }
        let prompt = make_prompt(mode, iteration, &history);
        let (context_tokens, repeated_context_tokens) =
            compute_context_tokens(mode, &prompt, &history);

        // A UTF-8 byte cannot expand to more than one provider token. Use that
        // conservative upper bound, then cap the requested output by the
        // remaining run token budget.
        let reserved_input_tokens = std::cmp::max(1, prompt.len() as i64);
        let remaining_output_tokens = config
            .limits
            .max_tokens
            .saturating_sub(usage.tokens)
            .saturating_sub(reserved_input_tokens);
        if remaining_output_tokens <= 0 {
            return Err("token reservation would exceed run token limit".to_string());
        }
        let request_max_output_tokens =
            std::cmp::min(config.limits.max_output_tokens, remaining_output_tokens);

        if config.provider_kind == ProviderKind::Live {
            let pricing = config.pricing.ok_or_else(|| {
                "live runner requires positive pricing for cost reservation".to_string()
            })?;
            let reserved_cost = (reserved_input_tokens as f64 / 1_000.0)
                * pricing.input_cost_per_1k
                + (request_max_output_tokens as f64 / 1_000.0) * pricing.output_cost_per_1k;
            if usage
                .per_call_cost_cap_usd
                .is_some_and(|cap| reserved_cost > cap)
            {
                return Err("call cost reservation would exceed per-call cost cap".to_string());
            }
            if usage.run_cost_usd + reserved_cost > config.limits.run_cost_cap_usd {
                return Err("call cost reservation would exceed run cost cap".to_string());
            }
            if usage.prior_daily_cost_usd + usage.run_cost_usd + reserved_cost
                > config.limits.daily_cost_cap_usd
            {
                return Err("call cost reservation would exceed daily cost cap".to_string());
            }
        }

        let mut provider_req =
            ProviderRequest::local_stub(provider.provider_id(), &request_model, &prompt);
        provider_req.metadata = json!({
            "dispatch_id": format!("local-runner:{mode}:{iteration}"),
            "max_tokens": request_max_output_tokens,
        });
        if let Some(tools) = tool_descriptors(mode) {
            provider_req.metadata["tools"] = tools;
            provider_req.metadata["tool_choice"] = json!("required");
        }

        let timeout = Duration::try_from_secs_f64(config.limits.timeout_seconds)
            .map_err(|_| "timeout must be finite and positive".to_string())?;
        usage.calls += 1;
        let result = {
            let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
            rt.block_on(async {
                tokio::time::timeout(timeout, provider.invoke(&provider_req)).await
            })
            .map_err(|_| {
                format!(
                    "provider invoke timed out after {} seconds",
                    config.limits.timeout_seconds
                )
            })?
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

        let input_tokens = match resp.input_tokens {
            Some(value) => value,
            None if config.provider_kind == ProviderKind::Live => {
                return Err("live provider response is missing input token usage".to_string())
            }
            None => estimate_tokens(&prompt),
        };
        if input_tokens < 0 {
            return Err("provider returned negative input token usage".to_string());
        }
        let output_tokens = match resp.output_tokens {
            Some(value) => value,
            None if config.provider_kind == ProviderKind::Live => {
                return Err("live provider response is missing output token usage".to_string())
            }
            None => 0,
        };
        if output_tokens < 0 || output_tokens > request_max_output_tokens {
            return Err(
                "provider output token usage exceeded the per-call output limit".to_string(),
            );
        }
        usage.tokens += input_tokens + output_tokens;
        let call_cost = match resp.estimated_cost {
            Some(cost) if cost.is_finite() && cost >= 0.0 => cost,
            Some(_) => return Err("provider returned invalid estimated cost".to_string()),
            None if config.provider_kind == ProviderKind::Live => {
                return Err("live provider response is missing estimated cost".to_string())
            }
            None => 0.0,
        };
        usage.run_cost_usd += call_cost;
        mode_cost += call_cost;

        if usage
            .per_call_cost_cap_usd
            .is_some_and(|cap| call_cost > cap)
        {
            return Err("provider call cost exceeded per-call cost cap".to_string());
        }

        if usage.tokens > config.limits.max_tokens {
            return Err("token limit exceeded".to_string());
        }
        if usage.run_cost_usd > config.limits.run_cost_cap_usd {
            return Err("run cost cap exceeded".to_string());
        }
        if usage.prior_daily_cost_usd + usage.run_cost_usd > config.limits.daily_cost_cap_usd {
            return Err("daily cost cap exceeded".to_string());
        }

        let fallback_candidate = std::cmp::min(17, 3 + (iteration as i64) * 2);
        let tool_id = selected_tool(&resp.output);
        let candidate = parse_candidate(&resp.output, fallback_candidate);
        let tool_mode = matches!(mode, "static_all" | "deterministic_top_k");
        let score_val = if tool_mode {
            let required_tool = if iteration == 0 { "read" } else { "search" };
            if tool_id.as_deref() == Some(required_tool) {
                correct_tool_selections += 1;
                1.0
            } else {
                0.0
            }
        } else {
            score(candidate)
        };
        best_score_val = if tool_mode {
            correct_tool_selections as f64 / (iteration + 1) as f64
        } else {
            best_score_val.max(score_val)
        };
        if let Some(tool_id) = &tool_id {
            selected_tool_ids.push(tool_id.clone());
        }

        history.push(json!({
            "iteration": iteration as i64,
            "candidate": candidate,
            "score": score_val,
        }));

        let mut step = build_step(
            mode,
            &run_id,
            iteration,
            input_tokens,
            output_tokens,
            context_tokens,
            repeated_context_tokens,
            candidate,
            score_val,
        );
        if let Some(tool_id) = tool_id {
            step["selected_tool_id"] = json!(tool_id);
        }
        steps.push(step);

        if tool_mode {
            if iteration == 1 {
                if best_score_val >= config.limits.pass_threshold {
                    status = "pass";
                }
                break;
            }
        } else if best_score_val >= config.limits.pass_threshold {
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

    let duration_ms = std::cmp::max(1, started.elapsed().as_millis() as i64);

    let quality_score = if status == "pass" {
        json!(best_score_val)
    } else {
        Value::Null
    };
    let comparison_contract =
        local_runner_comparison_contract(config, provider.provider_id(), &request_model);

    Ok(json!({
        "adapter_run_id": run_id,
        "runtime_kind": "native_harness",
        "runtime_version": RUNTIME_VERSION,
        "scenario_id": SCENARIO_ID,
        "mode": mode,
        "state_strategy": match mode {
            "stateful_store" => "durable_state",
            "full_history" | "stateless_reread" => "full_history",
            "summary_memory" => "memory_digest",
            "retrieval_memory" => "retrieval_refs",
            "durable_state_bounded_recent" => "mixed",
            "static_all" | "deterministic_top_k" => "none",
            _ => unreachable!(),
        },
        "status": status,
        "pass_fail_reason": if status == "pass" {
            "same score threshold met"
        } else {
            "score threshold not met within bounded iterations"
        },
        "quality_method": "rule",
        "comparison_contract": comparison_contract,
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
        "estimated_cost_usd": (mode_cost * 1_000_000.0).round() / 1_000_000.0,
        SOURCE_REF_KEY: format!("bounded-provider-gated-runner-{mode}"),
        "redaction_status": if config.provider_kind == ProviderKind::Live { "redacted" } else { "not_needed" },
        "runner_metadata": {
            "live": config.provider_kind == ProviderKind::Live,
            "provider_kind": match config.provider_kind {
                ProviderKind::Stub => "stub",
                ProviderKind::Fake => "fake",
                ProviderKind::Live => "live",
            },
            "model": request_model,
            "external_calls": steps.len() as i64,
            "final_best_score": best_score_val,
            "selected_tool_ids": selected_tool_ids,
            "context_protocol": match mode {
                "stateless_reread" | "full_history" => "full_history_reread",
                "summary_memory" => "summary_memory",
                "retrieval_memory" => "bounded_retrieval",
                "stateful_store" | "durable_state_bounded_recent" => "durable_state_bounded_recent",
                "static_all" => "static_all_tool_descriptors",
                "deterministic_top_k" => "deterministic_top_k_tool_descriptors",
                _ => unreachable!(),
            },
        },
        "steps": steps,
        "quality_score": quality_score,
    }))
}

fn local_runner_comparison_contract(
    config: &RunnerConfig,
    provider_id: &str,
    model_id: &str,
) -> Value {
    let scenario_digest = hex::encode(Sha256::digest(SCENARIO_ID.as_bytes()));
    let task_basis = format!(
        "{SCENARIO_ID}:{provider_id}:{model_id}:iterations={}:threshold={}",
        config.limits.iterations, config.limits.pass_threshold
    );
    let task_digest = hex::encode(Sha256::digest(task_basis.as_bytes()));
    let (input_rate, output_rate) = config
        .pricing
        .map(|pricing| (pricing.input_cost_per_1k, pricing.output_cost_per_1k))
        .unwrap_or((0.0, 0.0));
    let provider_kind = match config.provider_kind {
        ProviderKind::Stub => "stub",
        ProviderKind::Fake => "fake",
        ProviderKind::Live => "live",
    };
    json!({
        "scenario_digest": scenario_digest,
        "task_digest": task_digest,
        "runtime_kind": "native_harness",
        "runtime_version": RUNTIME_VERSION,
        "provider_id": provider_id,
        "model_id": model_id,
        "tokenizer_id": if config.provider_kind == ProviderKind::Live {
            "provider-reported"
        } else {
            "deterministic-estimator.v1"
        },
        "pricing_id": format!("local-runner-{provider_kind}-pricing.v1"),
        "input_cost_per_1k_usd": input_rate,
        "output_cost_per_1k_usd": output_rate,
        "quality_method": "rule",
        "quality_threshold": config.limits.pass_threshold,
        "evaluator_version": "hidden-score-rule.v1",
        "redaction_policy": if config.provider_kind == ProviderKind::Live {
            "summary-redacted.v1"
        } else {
            "not-needed-generated-summary.v1"
        },
        "retry_policy": "provider-boundary.v1",
        "seed": 0,
    })
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
    let pricing = provider_pricing_from_env();
    let pricing_is_complete = pricing
        .input_cost_per_1k
        .zip(pricing.output_cost_per_1k)
        .is_some_and(|(input, output)| input >= 0.0 && output >= 0.0);
    if !pricing_is_complete {
        return Err(
            "live local runner requires complete non-negative input and output provider pricing"
                .to_string(),
        );
    }
    provider_config.apply_pricing(&pricing);

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
            max_output_tokens: 1024,
            timeout_seconds,
            run_cost_cap_usd,
            daily_cost_cap_usd,
            pass_threshold,
        },
        provider: None,
        pricing: if provider_kind == ProviderKind::Live {
            runner_pricing_from_env()
        } else {
            None
        },
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
            Err(
                "live provider requires a persistent audit store; use build_live_provider"
                    .to_string(),
            )
        }
    }
}

pub fn build_live_provider(
    config: &RunnerConfig,
    gates: Option<&EffectiveExecutionGates>,
    store: Arc<LocalProductStore>,
) -> Result<Arc<dyn Provider>, String> {
    if config.provider_kind != ProviderKind::Live {
        return Err("build_live_provider requires live provider kind".to_string());
    }
    let gates = gates.ok_or_else(|| "live provider requires execution gates".to_string())?;
    if !gates.provider_execution {
        return Err("live provider execution not enabled by current gates".to_string());
    }
    let base_provider = match &config.provider {
        Some(provider) => provider.clone(),
        None => build_live_openai_compatible_provider()?,
    };
    let circuit_breaker = Arc::new(CircuitBreaker::new(
        format!("provider:{}", base_provider.provider_id()),
        5,
        30_000,
    ));
    let guarded: Arc<dyn Provider> =
        Arc::new(CircuitBreakerProvider::new(base_provider, circuit_breaker));
    Ok(Arc::new(AuditedProvider::new(guarded, store)))
}

pub fn run_pair(
    config: &RunnerConfig,
    provider: &Arc<dyn Provider>,
) -> Result<(Value, Value), String> {
    if config.provider_kind == ProviderKind::Live {
        return Err(
            "live provider requires persistent daily cost evidence; use the guarded live runner"
                .to_string(),
        );
    }
    let mut usage = RunnerUsage::default();
    let stateless = run_mode_with_usage("stateless_reread", config, provider, &mut usage)?;
    let stateful = run_mode_with_usage("stateful_store", config, provider, &mut usage)?;
    Ok((stateless, stateful))
}

pub fn run_pair_with_daily_cost(
    config: &RunnerConfig,
    provider: &Arc<dyn Provider>,
    prior_daily_cost_usd: f64,
) -> Result<(Value, Value), String> {
    if config.provider_kind != ProviderKind::Live {
        return Err("daily cost evidence is only accepted for live provider runs".to_string());
    }
    if !prior_daily_cost_usd.is_finite() || prior_daily_cost_usd < 0.0 {
        return Err("prior daily cost must be finite and non-negative".to_string());
    }
    let mut usage = RunnerUsage {
        prior_daily_cost_usd,
        ..RunnerUsage::default()
    };
    let stateless = run_mode_with_usage("stateless_reread", config, provider, &mut usage)?;
    let stateful = run_mode_with_usage("stateful_store", config, provider, &mut usage)?;
    Ok((stateless, stateful))
}

pub fn run_live_pair_with_store(
    config: &RunnerConfig,
    provider: &Arc<dyn Provider>,
    store: &LocalProductStore,
) -> Result<(Value, Value), String> {
    let date_prefix = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let prior_daily_cost_usd = store.daily_provider_audit_cost_usd(&date_prefix)?;
    run_pair_with_daily_cost(config, provider, prior_daily_cost_usd)
}

/// Runs a bounded list of live memory strategies under one shared call, token,
/// run-cost, daily-cost, and per-call-cost budget. The provider remains the
/// existing audited/circuit-broken owner; this function adds no provider path.
pub fn run_live_modes_with_store(
    config: &RunnerConfig,
    provider: &Arc<dyn Provider>,
    store: &LocalProductStore,
    modes: &[&str],
    per_call_cost_cap_usd: f64,
) -> Result<Vec<Value>, String> {
    if config.provider_kind != ProviderKind::Live {
        return Err("bounded live modes require live provider kind".to_string());
    }
    if modes.is_empty() || modes.len() > 8 {
        return Err("bounded live modes require between one and eight strategies".to_string());
    }
    if !per_call_cost_cap_usd.is_finite()
        || per_call_cost_cap_usd <= 0.0
        || per_call_cost_cap_usd > config.limits.run_cost_cap_usd
    {
        return Err("per-call cost cap must be positive and no greater than run cap".to_string());
    }
    let date_prefix = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let prior_daily_cost_usd = store.daily_provider_audit_cost_usd(&date_prefix)?;
    let mut usage = RunnerUsage {
        prior_daily_cost_usd,
        per_call_cost_cap_usd: Some(per_call_cost_cap_usd),
        ..RunnerUsage::default()
    };
    modes
        .iter()
        .map(|mode| run_mode_with_usage(mode, config, provider, &mut usage))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::fake::FakeProvider;
    use crate::provider::{ProviderRequest, ProviderResponse, ProviderResult};
    use crate::trusted_local::TrustedLocalProfileStatus;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    struct ScriptedProvider {
        delay: Duration,
        estimated_cost: Option<f64>,
        model: String,
    }

    struct CountingProvider {
        calls: Arc<AtomicUsize>,
    }

    struct OutputLimitProvider {
        observed_max_tokens: Arc<std::sync::atomic::AtomicI64>,
        returned_output_tokens: i64,
    }

    struct MissingUsageProvider {
        missing_input: bool,
    }

    #[async_trait::async_trait]
    impl Provider for CountingProvider {
        fn provider_id(&self) -> &str {
            "counting-provider"
        }

        fn is_enabled(&self) -> bool {
            true
        }

        async fn invoke(&self, request: &ProviderRequest) -> ProviderResult {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ProviderResponse {
                schema_version: "provider_response.v1".to_string(),
                provider_id: self.provider_id().to_string(),
                model: request.model.clone(),
                output: "candidate=17".to_string(),
                input_tokens: Some(10),
                output_tokens: Some(2),
                estimated_cost: Some(0.01),
                provider_request_id: None,
            })
        }
    }

    #[async_trait::async_trait]
    impl Provider for OutputLimitProvider {
        fn provider_id(&self) -> &str {
            "output-limit-provider"
        }

        fn is_enabled(&self) -> bool {
            true
        }

        async fn invoke(&self, request: &ProviderRequest) -> ProviderResult {
            self.observed_max_tokens.store(
                request.metadata["max_tokens"].as_i64().unwrap_or_default(),
                Ordering::SeqCst,
            );
            Ok(ProviderResponse {
                schema_version: "provider_response.v1".to_string(),
                provider_id: self.provider_id().to_string(),
                model: request.model.clone(),
                output: "candidate=17".to_string(),
                input_tokens: Some(10),
                output_tokens: Some(self.returned_output_tokens),
                estimated_cost: Some(0.0),
                provider_request_id: None,
            })
        }
    }

    #[async_trait::async_trait]
    impl Provider for MissingUsageProvider {
        fn provider_id(&self) -> &str {
            "missing-usage-provider"
        }

        fn is_enabled(&self) -> bool {
            true
        }

        fn default_model(&self) -> Option<&str> {
            Some("actual-request-model")
        }

        async fn invoke(&self, request: &ProviderRequest) -> ProviderResult {
            Ok(ProviderResponse {
                schema_version: "provider_response.v1".to_string(),
                provider_id: self.provider_id().to_string(),
                model: request.model.clone(),
                output: "tool=search;candidate=17".to_string(),
                input_tokens: (!self.missing_input).then_some(10),
                output_tokens: self.missing_input.then_some(2),
                estimated_cost: Some(0.01),
                provider_request_id: None,
            })
        }
    }

    #[async_trait::async_trait]
    impl Provider for ScriptedProvider {
        fn provider_id(&self) -> &str {
            "scripted-provider"
        }

        fn is_enabled(&self) -> bool {
            true
        }

        fn default_model(&self) -> Option<&str> {
            Some(&self.model)
        }

        async fn invoke(&self, request: &ProviderRequest) -> ProviderResult {
            tokio::time::sleep(self.delay).await;
            let selected_tool = if request.prompt.contains("Required capability: read") {
                "read"
            } else {
                "search"
            };
            Ok(ProviderResponse {
                schema_version: "provider_response.v1".to_string(),
                provider_id: self.provider_id().to_string(),
                model: request.model.clone(),
                output: format!("tool={selected_tool};candidate=17"),
                input_tokens: Some(10),
                output_tokens: Some(2),
                estimated_cost: self.estimated_cost,
                provider_request_id: None,
            })
        }
    }

    fn scripted_runner(
        delay: Duration,
        estimated_cost: f64,
        timeout_seconds: f64,
        run_cost_cap_usd: f64,
    ) -> (RunnerConfig, Arc<dyn Provider>) {
        let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider {
            delay,
            estimated_cost: Some(estimated_cost),
            model: "actual-request-model".to_string(),
        });
        let config = RunnerConfig {
            provider_kind: ProviderKind::Live,
            model: "configured-model-alias".to_string(),
            limits: RunnerLimits {
                iterations: 2,
                max_calls: 4,
                max_tokens: 1_000,
                max_output_tokens: 1024,
                timeout_seconds,
                run_cost_cap_usd,
                daily_cost_cap_usd: 1.0,
                pass_threshold: 0.94,
            },
            provider: Some(provider.clone()),
            pricing: Some(RunnerPricing {
                input_cost_per_1k: 0.001,
                output_cost_per_1k: 0.001,
            }),
        };
        (config, provider)
    }

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
        std::env::remove_var(crate::provider::config::ACP_PROVIDER_INPUT_COST_PER_1K_USD);
        std::env::remove_var(crate::provider::config::ACP_PROVIDER_OUTPUT_COST_PER_1K_USD);
    }

    fn live_config() -> RunnerConfig {
        RunnerConfig {
            provider_kind: ProviderKind::Live,
            model: "live".to_string(),
            limits: RunnerLimits::default(),
            provider: None,
            pricing: None,
        }
    }

    fn audit_store() -> Arc<crate::storage::local_product_store::LocalProductStore> {
        Arc::new(crate::storage::local_product_store::LocalProductStore::new(":memory:").unwrap())
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
            pricing: None,
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
                build_live_provider(&config, Some(&gates_with_provider()), audit_store()),
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
            std::env::set_var(
                crate::provider::config::ACP_PROVIDER_INPUT_COST_PER_1K_USD,
                "0.01",
            );
            std::env::set_var(
                crate::provider::config::ACP_PROVIDER_OUTPUT_COST_PER_1K_USD,
                "0.02",
            );
            let config = live_config();
            assert_provider_err(
                build_live_provider(&config, Some(&gates_with_provider()), audit_store()),
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
                build_live_provider(&config, Some(&gates_with_provider()), audit_store()),
                "credential environment variable",
            );
            clear_live_env();
        });
    }

    #[test]
    fn live_provider_fails_closed_without_pricing() {
        with_env_lock(|| {
            clear_live_env();
            std::env::remove_var(crate::provider::config::ACP_PROVIDER_INPUT_COST_PER_1K_USD);
            std::env::remove_var(crate::provider::config::ACP_PROVIDER_OUTPUT_COST_PER_1K_USD);
            std::env::set_var(LOCAL_RUNNER_PROVIDER_TYPE_ENV, "openai_compatible");
            std::env::set_var(LOCAL_RUNNER_BASE_URL_ENV, "https://api.example.test/v1");
            std::env::set_var(LOCAL_RUNNER_MODEL_ENV, "test-model");
            std::env::set_var(LOCAL_RUNNER_API_KEY_ENV_REF, "LOCAL_RUNNER_TEST_OPENAI_KEY");
            std::env::set_var("LOCAL_RUNNER_TEST_OPENAI_KEY", "sk-local-runner-test");

            assert_provider_err(
                build_live_provider(&live_config(), Some(&gates_with_provider()), audit_store()),
                "pricing",
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
            std::env::set_var(
                crate::provider::config::ACP_PROVIDER_INPUT_COST_PER_1K_USD,
                "0.01",
            );
            std::env::set_var(
                crate::provider::config::ACP_PROVIDER_OUTPUT_COST_PER_1K_USD,
                "0.02",
            );
            let config = live_config();
            let provider =
                build_live_provider(&config, Some(&gates_with_provider()), audit_store()).unwrap();
            assert_eq!(provider.provider_id(), "local-runner-openai-compatible");
            assert!(provider.is_enabled());
            assert_eq!(provider.default_model(), Some("test-model"));
            clear_live_env();
        });
    }

    #[test]
    fn live_provider_accepts_explicit_zero_pricing_without_invoking_network() {
        with_env_lock(|| {
            clear_live_env();
            std::env::set_var(LOCAL_RUNNER_PROVIDER_TYPE_ENV, "openai_compatible");
            std::env::set_var(LOCAL_RUNNER_BASE_URL_ENV, "https://api.example.test/v1");
            std::env::set_var(LOCAL_RUNNER_MODEL_ENV, "free-model:free");
            std::env::set_var(LOCAL_RUNNER_API_KEY_ENV_REF, "LOCAL_RUNNER_TEST_OPENAI_KEY");
            std::env::set_var("LOCAL_RUNNER_TEST_OPENAI_KEY", "opaque-test-key");
            std::env::set_var(
                crate::provider::config::ACP_PROVIDER_INPUT_COST_PER_1K_USD,
                "0",
            );
            std::env::set_var(
                crate::provider::config::ACP_PROVIDER_OUTPUT_COST_PER_1K_USD,
                "0",
            );

            assert_eq!(
                runner_pricing_from_env(),
                Some(RunnerPricing {
                    input_cost_per_1k: 0.0,
                    output_cost_per_1k: 0.0,
                })
            );
            let provider =
                build_live_provider(&live_config(), Some(&gates_with_provider()), audit_store())
                    .unwrap();
            assert_eq!(provider.default_model(), Some("free-model:free"));
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
            pricing: None,
        };
        let result =
            build_live_provider(&config, Some(&gates_with_provider()), audit_store()).unwrap();
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
    fn run_mode_enforces_provider_timeout() {
        let (config, provider) = scripted_runner(Duration::from_millis(25), 0.0, 0.001, 0.25);

        let error =
            run_mode_with_daily_cost("stateless_reread", &config, &provider, 0.0).unwrap_err();

        assert!(error.contains("timed out"), "unexpected error: {error}");
    }

    #[test]
    fn run_mode_records_actual_model_and_measured_duration() {
        let (config, provider) = scripted_runner(Duration::from_millis(15), 0.0, 1.0, 0.25);

        let result = run_mode_with_daily_cost("stateless_reread", &config, &provider, 0.0).unwrap();

        assert_eq!(result["runner_metadata"]["model"], "actual-request-model");
        assert!(
            result["duration_ms"].as_i64().unwrap_or_default() >= 10,
            "duration should be measured from the provider call: {}",
            result["duration_ms"]
        );
    }

    #[test]
    fn live_request_enforces_the_configured_output_limit() {
        let observed = Arc::new(std::sync::atomic::AtomicI64::new(0));
        let provider: Arc<dyn Provider> = Arc::new(OutputLimitProvider {
            observed_max_tokens: Arc::clone(&observed),
            returned_output_tokens: 2,
        });
        let (mut config, _) = scripted_runner(Duration::ZERO, 0.0, 1.0, 0.25);
        config.limits.max_output_tokens = 256;
        run_mode_with_daily_cost("full_history", &config, &provider, 0.0).unwrap();
        assert_eq!(observed.load(Ordering::SeqCst), 256);
    }

    #[test]
    fn live_response_rejects_usage_above_the_configured_output_limit() {
        let provider: Arc<dyn Provider> = Arc::new(OutputLimitProvider {
            observed_max_tokens: Arc::new(std::sync::atomic::AtomicI64::new(0)),
            returned_output_tokens: 257,
        });
        let (mut config, _) = scripted_runner(Duration::ZERO, 0.0, 1.0, 0.25);
        config.limits.max_output_tokens = 256;
        let error = run_mode_with_daily_cost("full_history", &config, &provider, 0.0).unwrap_err();
        assert!(error.contains("output token usage exceeded"));
    }

    #[test]
    fn run_pair_shares_the_run_cost_cap_across_modes() {
        let (config, provider) = scripted_runner(Duration::ZERO, 0.15, 1.0, 0.25);

        let error = run_pair_with_daily_cost(&config, &provider, 0.0).unwrap_err();

        assert!(
            error.contains("cost cap exceeded"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn live_modes_share_limits_and_persist_audit_without_network() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            crate::storage::local_product_store::LocalProductStore::new(
                dir.path().join("benchmark-audit.db"),
            )
            .unwrap(),
        );
        let (mut config, _provider) = scripted_runner(Duration::ZERO, 0.001, 1.0, 0.25);
        config.limits.max_calls = 8;
        let provider =
            build_live_provider(&config, Some(&gates_with_provider()), store.clone()).unwrap();
        let results = run_live_modes_with_store(
            &config,
            &provider,
            &store,
            &[
                "full_history",
                "summary_memory",
                "retrieval_memory",
                "durable_state_bounded_recent",
                "static_all",
                "deterministic_top_k",
            ],
            0.05,
        )
        .unwrap();
        assert_eq!(results.len(), 6);
        assert_eq!(results[4]["mode"], "static_all");
        assert_eq!(results[5]["mode"], "deterministic_top_k");
        assert_eq!(
            results[4].pointer("/runner_metadata/selected_tool_ids"),
            Some(&json!(["read", "search"]))
        );
        assert_eq!(
            results[5].pointer("/runner_metadata/selected_tool_ids"),
            Some(&json!(["read", "search"]))
        );
        assert_eq!(store.provider_audit_events(100).unwrap().len(), 16);
    }

    #[test]
    fn live_modes_fail_closed_when_actual_call_cost_exceeds_per_call_cap() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            crate::storage::local_product_store::LocalProductStore::new(
                dir.path().join("benchmark-cap.db"),
            )
            .unwrap(),
        );
        let (config, _provider) = scripted_runner(Duration::ZERO, 0.15, 1.0, 0.25);
        let provider =
            build_live_provider(&config, Some(&gates_with_provider()), store.clone()).unwrap();
        let error = run_live_modes_with_store(&config, &provider, &store, &["full_history"], 0.10)
            .unwrap_err();
        assert!(
            error.contains("per-call cost cap"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn live_run_fails_closed_when_provider_cost_is_unknown() {
        let (mut config, _) = scripted_runner(Duration::ZERO, 0.0, 1.0, 0.25);
        let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider {
            delay: Duration::ZERO,
            estimated_cost: None,
            model: "actual-request-model".to_string(),
        });
        config.provider = Some(provider.clone());

        let error =
            run_mode_with_daily_cost("stateless_reread", &config, &provider, 0.0).unwrap_err();

        assert!(error.contains("cost"), "unexpected error: {error}");
    }

    #[test]
    fn live_run_fails_closed_when_provider_token_usage_is_incomplete() {
        for (missing_input, expected) in [(true, "input token"), (false, "output token")] {
            let (mut config, _) = scripted_runner(Duration::ZERO, 0.01, 1.0, 0.25);
            let provider: Arc<dyn Provider> = Arc::new(MissingUsageProvider { missing_input });
            config.provider = Some(provider.clone());

            let error =
                run_mode_with_daily_cost("full_history", &config, &provider, 0.0).unwrap_err();
            assert!(error.contains(expected), "unexpected error: {error}");
        }
    }

    #[test]
    fn live_run_requires_persistent_daily_cost_evidence() {
        let (config, provider) = scripted_runner(Duration::ZERO, 0.01, 1.0, 0.25);

        let error = run_pair(&config, &provider).unwrap_err();

        assert!(
            error.contains("daily cost evidence"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn prior_daily_cost_at_cap_blocks_before_provider_call() {
        let calls = Arc::new(AtomicUsize::new(0));
        let provider: Arc<dyn Provider> = Arc::new(CountingProvider {
            calls: calls.clone(),
        });
        let mut config = live_config();
        config.provider = Some(provider.clone());

        let error = run_pair_with_daily_cost(&config, &provider, config.limits.daily_cost_cap_usd)
            .unwrap_err();

        assert!(
            error.contains("daily cost cap"),
            "unexpected error: {error}"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn single_call_cost_reservation_blocks_before_provider_call() {
        let calls = Arc::new(AtomicUsize::new(0));
        let provider: Arc<dyn Provider> = Arc::new(CountingProvider {
            calls: calls.clone(),
        });
        let mut config = live_config();
        config.provider = Some(provider.clone());
        config.pricing = Some(RunnerPricing {
            input_cost_per_1k: 1.0,
            output_cost_per_1k: 1.0,
        });

        let error = run_pair_with_daily_cost(&config, &provider, 0.0).unwrap_err();

        assert!(
            error.contains("cost reservation"),
            "unexpected error: {error}"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn audited_provider_persists_bounded_request_and_response_events() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            crate::storage::local_product_store::LocalProductStore::new(
                dir.path().join("audit.db"),
            )
            .unwrap(),
        );
        let inner: Arc<dyn Provider> = Arc::new(ScriptedProvider {
            delay: Duration::ZERO,
            estimated_cost: Some(0.0125),
            model: "audited-model".to_string(),
        });
        let provider: Arc<dyn Provider> = Arc::new(AuditedProvider::new(inner, store.clone()));
        let mut request = ProviderRequest::local_stub(
            provider.provider_id(),
            "audited-model",
            "private prompt with sk-do-not-persist",
        );
        request.metadata = json!({"dispatch_id": "local-runner:test:0"});

        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(provider.invoke(&request)).unwrap();

        let events = store.provider_audit_events(10).unwrap();
        assert_eq!(events.len(), 2);
        assert!(events
            .iter()
            .any(|event| event["event_type"] == "request_sent"));
        let response = events
            .iter()
            .find(|event| event["event_type"] == "response_received")
            .unwrap();
        assert_eq!(response["cost"], 0.0125);
        assert_eq!(response["redaction_status"], "redacted");
        let serialized = serde_json::to_string(&events).unwrap();
        assert!(!serialized.contains("private prompt"));
        assert!(!serialized.contains("sk-do-not-persist"));
        assert!(!serialized.contains("candidate=17"));
    }

    #[test]
    fn fake_run_mode_has_zero_cost() {
        let config = RunnerConfig {
            provider_kind: ProviderKind::Fake,
            model: "fake".to_string(),
            limits: RunnerLimits::default(),
            provider: None,
            pricing: None,
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
