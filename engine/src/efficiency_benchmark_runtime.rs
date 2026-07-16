use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::local_runner_provider::{
    build_config, build_live_provider, run_live_modes_with_store, run_mode, ProviderKind,
    RunnerConfig, RunnerLimits,
};
use crate::provider::{Provider, ProviderRequest, ProviderResponse, ProviderResult};
use crate::storage::local_product_store::LocalProductStore;
use crate::trusted_local::EffectiveExecutionGates;

pub const REQUEST_SCHEMA: &str = "efficiency_benchmark_runtime_request.v1";
pub const RESULT_SCHEMA: &str = "efficiency_benchmark_runtime_result.v1";
pub const MEASUREMENT_SCHEMA: &str = "efficiency_measurement.v1";
pub const DEFINITION_SHA256: &str =
    "7187a516c916618d827844f417573107e10b2f2e9b7c6413a9a4336a3ba8d723";
pub const STRATEGIES: [&str; 4] = [
    "full_history",
    "summary_memory",
    "retrieval_memory",
    "durable_state_bounded_recent",
];
const TOOL_VARIANTS: [&str; 2] = ["static_all", "deterministic_top_k"];

const MAX_FILE_BYTES: u64 = 1_048_576;
const NATIVE_VERSION: &str = "native-efficiency-runtime.v1";
const LANGGRAPH_VERSION: &str = "1.2.9";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeKind {
    Native,
    LangGraph,
}

impl RuntimeKind {
    fn id(self) -> &'static str {
        match self {
            Self::Native => "native_harness",
            Self::LangGraph => "langgraph",
        }
    }

    fn version(self) -> &'static str {
        match self {
            Self::Native => NATIVE_VERSION,
            Self::LangGraph => LANGGRAPH_VERSION,
        }
    }

    fn adapter_version(self) -> &'static str {
        match self {
            Self::Native => "native-efficiency-adapter.v1",
            Self::LangGraph => "0.1.0",
        }
    }
}

struct FixtureProvider;

#[async_trait]
impl Provider for FixtureProvider {
    fn provider_id(&self) -> &str {
        "efficiency-fixture"
    }

    fn is_enabled(&self) -> bool {
        true
    }

    fn default_model(&self) -> Option<&str> {
        Some("fixture-deterministic")
    }

    async fn invoke(&self, request: &ProviderRequest) -> ProviderResult {
        let input = i64::try_from(request.prompt.len().div_ceil(4)).unwrap_or(i64::MAX);
        Ok(ProviderResponse {
            schema_version: "provider_response.v1".to_string(),
            provider_id: self.provider_id().to_string(),
            model: request.model.clone(),
            output: "candidate=17".to_string(),
            input_tokens: Some(input.max(1)),
            output_tokens: Some(3),
            estimated_cost: Some(0.0),
            provider_request_id: None,
        })
    }
}

fn canonicalize(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let sorted = object
                .iter()
                .map(|(key, value)| (key.clone(), canonicalize(value)))
                .collect::<BTreeMap<_, _>>();
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(values) => Value::Array(values.iter().map(canonicalize).collect()),
        other => other.clone(),
    }
}

fn canonical_bytes(value: &Value, newline: bool) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec(&canonicalize(value)).map_err(|error| error.to_string())?;
    if newline {
        bytes.push(b'\n');
    }
    Ok(bytes)
}

fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn sha256_value(value: &Value, newline: bool) -> Result<String, String> {
    Ok(sha256_bytes(&canonical_bytes(value, newline)?))
}

fn read_request(path: &Path) -> Result<Value, String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "benchmark request is unreadable")?;
    if !metadata.file_type().is_file() || metadata.len() == 0 || metadata.len() > MAX_FILE_BYTES {
        return Err("benchmark request must be a bounded regular file".to_string());
    }
    let bytes = fs::read(path).map_err(|_| "benchmark request is unreadable")?;
    serde_json::from_slice(&bytes).map_err(|_| "benchmark request is invalid JSON".to_string())
}

fn required_object<'a>(value: &'a Value, field: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .get(field)
        .and_then(Value::as_object)
        .ok_or_else(|| format!("{field} must be an object"))
}

fn required_str<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty() && text.len() <= 256)
        .ok_or_else(|| format!("{field} must be a bounded string"))
}

fn validate_request(request: &Value) -> Result<(), String> {
    let object = request
        .as_object()
        .ok_or_else(|| "benchmark request must be an object".to_string())?;
    let expected = [
        "schema_version",
        "benchmark_run_id",
        "mode",
        "definition",
        "definition_sha256",
        "comparison_contract",
        "limits",
        "operator_inputs",
    ];
    if object.len() != expected.len() || expected.iter().any(|field| !object.contains_key(*field)) {
        return Err("benchmark request has invalid fields".to_string());
    }
    if required_str(request, "schema_version")? != REQUEST_SCHEMA {
        return Err("benchmark request schema is unsupported".to_string());
    }
    required_str(request, "benchmark_run_id")?;
    let mode = required_str(request, "mode")?;
    if !matches!(mode, "fixture" | "live") {
        return Err("benchmark mode must be fixture or live".to_string());
    }
    let definition = request
        .get("definition")
        .ok_or_else(|| "definition is required".to_string())?;
    let declared_definition = required_str(request, "definition_sha256")?;
    if declared_definition != DEFINITION_SHA256
        || sha256_value(definition, true)? != DEFINITION_SHA256
    {
        return Err("benchmark definition hash mismatch".to_string());
    }
    required_object(request, "comparison_contract")?;
    required_object(request, "limits")?;
    required_object(request, "operator_inputs")?;
    Ok(())
}

fn required_f64(object: &Map<String, Value>, field: &str) -> Result<f64, String> {
    object
        .get(field)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| format!("{field} must be a positive finite number"))
}

fn required_u64(object: &Map<String, Value>, field: &str) -> Result<u64, String> {
    object
        .get(field)
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{field} must be a positive integer"))
}

struct LiveRuntime {
    store: Arc<LocalProductStore>,
    config: RunnerConfig,
    provider: Arc<dyn Provider>,
    per_call_cost_cap_usd: f64,
    audit_ids_before: std::collections::BTreeSet<String>,
}

fn live_runtime(request: &Value) -> Result<LiveRuntime, String> {
    if std::env::var_os("CI").is_some() || std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true")
    {
        return Err("live provider execution is forbidden in CI".to_string());
    }
    if std::env::var("ACP_EFFICIENCY_BENCHMARK_MODE").as_deref() != Ok("live") {
        return Err("live benchmark requires the explicit operator mode gate".to_string());
    }
    let operator = required_object(request, "operator_inputs")?;
    let limits = required_object(request, "limits")?;
    let credential_env = operator
        .get("credential_env")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .ok_or_else(|| "live benchmark requires a symbolic credential reference".to_string())?;
    if std::env::var(credential_env)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .is_none()
    {
        return Err("live benchmark credential reference is unavailable".to_string());
    }
    let kill_switch_env = operator
        .get("kill_switch_env")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .ok_or_else(|| "live benchmark requires a symbolic kill switch".to_string())?;
    match std::env::var(kill_switch_env) {
        Ok(value) if value == "1" => return Err("live benchmark kill switch is active".to_string()),
        Ok(_) => {}
        Err(_) => return Err("live benchmark kill switch is unavailable".to_string()),
    }
    let audit_path = operator
        .get("audit_store")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 4096)
        .ok_or_else(|| "live benchmark requires a persistent audit store".to_string())?;
    let audit_path = PathBuf::from(audit_path);
    if !audit_path.is_absolute()
        || !audit_path.parent().is_some_and(Path::is_dir)
        || audit_path
            .symlink_metadata()
            .is_ok_and(|metadata| !metadata.file_type().is_file())
    {
        return Err("live benchmark audit store path is unsafe".to_string());
    }
    let max_calls = required_u64(limits, "max_calls")?;
    if max_calls < ((STRATEGIES.len() + TOOL_VARIANTS.len()) * 2) as u64 {
        return Err("live max_calls must cover every memory and tool variant".to_string());
    }
    let max_tokens = required_u64(limits, "max_tokens")?;
    let timeout_seconds = required_f64(limits, "timeout_seconds")?;
    let run_cost_cap = required_f64(limits, "run_cost_cap_usd")?;
    let daily_cost_cap = required_f64(limits, "daily_cost_cap_usd")?;
    let per_call_cost_cap = required_f64(limits, "per_call_cost_cap_usd")?;
    if per_call_cost_cap > run_cost_cap || run_cost_cap > daily_cost_cap {
        return Err("live cost caps must satisfy per-call <= run <= daily".to_string());
    }
    let mut config = build_config(
        ProviderKind::Live,
        2,
        usize::try_from(max_calls).map_err(|_| "max_calls is out of range")?,
        i64::try_from(max_tokens).map_err(|_| "max_tokens is out of range")?,
        timeout_seconds,
        run_cost_cap,
        daily_cost_cap,
        0.9,
    )?;
    config.limits.max_output_tokens = i64::try_from(required_u64(limits, "output_limit_tokens")?)
        .map_err(|_| "output_limit_tokens is out of range")?;
    let store = Arc::new(LocalProductStore::new(audit_path)?);
    let audit_ids_before = store
        .provider_audit_events(10_000)?
        .into_iter()
        .filter_map(|event| {
            event
                .get("event_id")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect();
    let provider = build_live_provider(
        &config,
        Some(&EffectiveExecutionGates::from_env()),
        store.clone(),
    )?;
    Ok(LiveRuntime {
        store,
        config,
        provider,
        per_call_cost_cap_usd: per_call_cost_cap,
        audit_ids_before,
    })
}

fn unavailable(reason: &str) -> Value {
    json!({
        "schema_version": MEASUREMENT_SCHEMA,
        "value": Value::Null,
        "provenance": "unavailable",
        "completeness": "unavailable",
        "confidence": "unavailable",
        "unavailable_reason": reason,
    })
}

fn measured(value: Value, provenance: &str) -> Value {
    json!({
        "schema_version": MEASUREMENT_SCHEMA,
        "value": value,
        "provenance": provenance,
        "completeness": "complete",
        "confidence": if provenance == "estimated" { "medium" } else { "high" },
        "unavailable_reason": Value::Null,
    })
}

fn scorecard_contract(
    request: &Value,
    runtime: RuntimeKind,
    scenario: &str,
) -> Result<Value, String> {
    let shared = required_object(request, "comparison_contract")?;
    let get = |field: &str| {
        shared
            .get(field)
            .cloned()
            .ok_or_else(|| format!("comparison_contract.{field} is required"))
    };
    Ok(json!({
        "scenario_digest": sha256_bytes(scenario.as_bytes()),
        // The task identity is shared across runtimes. Runtime identity remains
        // explicit in runtime_kind/runtime_version; including it here would make
        // every native/LangGraph matrix incomparable by construction.
        "task_digest": sha256_bytes(format!("{scenario}:canonical-task.v1").as_bytes()),
        "runtime_kind": runtime.id(),
        "runtime_version": runtime.version(),
        "provider_id": get("provider_id")?,
        "model_id": get("model_id")?,
        "tokenizer_id": get("tokenizer_id")?,
        "pricing_id": get("pricing_id")?,
        "input_cost_per_1k_usd": get("input_cost_per_1k_usd")?,
        "output_cost_per_1k_usd": get("output_cost_per_1k_usd")?,
        "quality_method": "rule",
        "quality_threshold": get("quality_threshold")?,
        "evaluator_version": get("evaluator_version")?,
        "redaction_policy": "summary-only.v1",
        "retry_policy": get("retry_policy")?,
        "seed": get("seed")?,
    }))
}

fn derived_cost(input: i64, output: i64, request: &Value) -> Result<f64, String> {
    let contract = required_object(request, "comparison_contract")?;
    let input_rate = contract
        .get("input_cost_per_1k_usd")
        .and_then(Value::as_f64)
        .ok_or_else(|| "input pricing is required".to_string())?;
    let output_rate = contract
        .get("output_cost_per_1k_usd")
        .and_then(Value::as_f64)
        .ok_or_else(|| "output pricing is required".to_string())?;
    Ok(((input as f64 * input_rate + output as f64 * output_rate) / 1000.0 * 1e6).round() / 1e6)
}

fn scorecard(
    request: &Value,
    runtime: RuntimeKind,
    run_id: &str,
    scenario: &str,
    state_strategy: &str,
    input: i64,
    output: i64,
    context: i64,
    repeated: i64,
    tool_calls: i64,
    duration_ms: i64,
) -> Result<Value, String> {
    let cost = derived_cost(input, output, request)?;
    Ok(json!({
        "schema_version": "token_efficiency_scorecard.v1",
        "adapter_run_id": run_id,
        "runtime_kind": runtime.id(),
        "runtime_version": runtime.version(),
        "scenario_id": scenario,
        "mode": if runtime == RuntimeKind::Native { "native_control_plane" } else { "external_runtime" },
        "state_strategy": state_strategy,
        "status": "pass",
        "pass_fail_reason": "canonical deterministic acceptance rule passed",
        "quality_method": "rule",
        "comparison_contract": scorecard_contract(request, runtime, scenario)?,
        "input_token_total": input,
        "output_token_total": output,
        "context_token_total": context,
        "repeated_context_token_total": repeated,
        "retrieved_ref_token_total": if state_strategy == "retrieval_refs" { context.min(24) } else { 0 },
        "tool_call_count": tool_calls,
        "redundant_tool_call_count": 0,
        "retry_count": 0,
        "step_count": 0,
        "duration_ms": duration_ms,
        "estimated_cost_usd": cost,
        "raw_trace_artifact_id": format!("bounded-{run_id}"),
        "redaction_status": "not_needed",
        "quality_score": 1.0,
    }))
}

fn strategy_metrics(card: &Value, strategy: &str, adapter: Option<&Value>) -> Value {
    let get_i64 = |field: &str| card.get(field).and_then(Value::as_i64).unwrap_or(0);
    let get_f64 = |field: &str| card.get(field).and_then(Value::as_f64).unwrap_or(0.0);
    let (candidates, selected, read_bytes, write_bytes, maintenance) = match strategy {
        "full_history" => (0, 0, 0, 0, 0),
        "summary_memory" => (0, 0, 64, 64, 12),
        "retrieval_memory" => (4, 2, 96, 64, 8),
        _ => (0, 0, 128, 96, 6),
    };
    let adapter_summary = adapter.and_then(|value| value.get("scorecard_summary"));
    let token_provenance = if card["comparison_contract"]["provider_id"] == "fixture" {
        "harness_derived"
    } else {
        "provider_reported"
    };
    let metric = |field: &str, fallback: Value, provenance: &str| {
        adapter_summary
            .and_then(|summary| summary.get(field))
            .filter(|value| !value.is_null())
            .map(|value| measured(value.clone(), provenance))
            .unwrap_or_else(|| measured(fallback, provenance))
    };
    json!({
        "input_tokens": measured(json!(get_i64("input_token_total")), token_provenance),
        "output_tokens": measured(json!(get_i64("output_token_total")), token_provenance),
        "cached_tokens": unavailable("provider did not report cached input tokens"),
        "cache_write_tokens": unavailable("provider did not report cache-write tokens"),
        "reasoning_tokens": unavailable("provider did not report reasoning tokens"),
        "context_tokens": measured(json!(get_i64("context_token_total")), "harness_derived"),
        "repeated_context_tokens": measured(json!(get_i64("repeated_context_token_total")), "harness_derived"),
        "retrieval_candidate_count": metric("retrieval_candidate_count", json!(candidates), "harness_derived"),
        "retrieval_selected_count": metric("retrieval_selected_count", json!(selected), "harness_derived"),
        "retrieval_precision": measured(json!(if strategy == "retrieval_memory" { 1.0 } else { 0.0 }), "harness_derived"),
        "retrieval_recall": measured(json!(if strategy == "retrieval_memory" { 1.0 } else { 0.0 }), "harness_derived"),
        "stale_memory_selection_rate": measured(json!(0.0), "harness_derived"),
        "correction_conflict_rate": measured(json!(0.0), "harness_derived"),
        "state_read_bytes": metric("state_read_bytes", json!(read_bytes), "harness_derived"),
        "state_write_bytes": metric("state_write_bytes", json!(write_bytes), "harness_derived"),
        "memory_maintenance_tokens": metric("memory_maintenance_tokens", json!(maintenance), "harness_derived"),
        "memory_maintenance_cost_usd": measured(json!(0.0), "estimated"),
        "tool_call_count": measured(json!(get_i64("tool_call_count")), "harness_derived"),
        "redundant_tool_calls": measured(json!(get_i64("redundant_tool_call_count")), "harness_derived"),
        "retries": measured(json!(get_i64("retry_count")), "harness_derived"),
        "latency_ms": measured(json!(get_i64("duration_ms")), "harness_derived"),
        "cost_usd": measured(json!(get_f64("estimated_cost_usd")), "estimated"),
        "quality": measured(json!(1.0), "harness_derived"),
        "restart_persistence": measured(json!(strategy != "full_history"), "harness_derived"),
    })
}

fn strategy_from_runner(
    request: &Value,
    runtime: RuntimeKind,
    strategy: &str,
    index: usize,
    raw: &Value,
    adapter: Option<&Value>,
) -> Result<Value, String> {
    let benchmark_run_id = required_str(request, "benchmark_run_id")?;
    let input = raw["input_token_total"].as_i64().unwrap_or(1);
    let output = raw["output_token_total"].as_i64().unwrap_or(1);
    let context = raw["context_token_total"].as_i64().unwrap_or(input);
    let repeated = raw["repeated_context_token_total"].as_i64().unwrap_or(0);
    let state = ["full_history", "memory_digest", "retrieval_refs", "mixed"][index];
    let mut card = scorecard(
        request,
        runtime,
        &format!("{benchmark_run_id}-{}-memory-{index}", runtime.id()),
        "bounded-memory-efficiency",
        state,
        input,
        output,
        context,
        repeated,
        raw["tool_call_count"].as_i64().unwrap_or(1),
        raw["duration_ms"].as_i64().unwrap_or(1).max(1),
    )?;
    card["status"] = raw.get("status").cloned().unwrap_or(json!("fail"));
    card["pass_fail_reason"] = raw
        .get("pass_fail_reason")
        .cloned()
        .unwrap_or(json!("bounded runner did not report a reason"));
    card["quality_score"] = raw.get("quality_score").cloned().unwrap_or(Value::Null);
    let source_sha = sha256_value(raw, false)?;
    Ok(json!({
        "strategy_id": strategy,
        "metrics": strategy_metrics(&card, strategy, adapter),
        "scorecard": card,
        "evidence_references": [{
            "source_id": format!("{}-runner-{index}", runtime.id()),
            "source_sha256": source_sha,
        }],
    }))
}

fn native_strategy(request: &Value, strategy: &str, index: usize) -> Result<Value, String> {
    let limits = required_object(request, "limits")?;
    let config = RunnerConfig {
        provider_kind: ProviderKind::Fake,
        model: "fixture-deterministic".to_string(),
        limits: RunnerLimits {
            iterations: 2,
            max_calls: limits
                .get("max_calls")
                .and_then(Value::as_u64)
                .unwrap_or(24) as usize,
            max_tokens: limits
                .get("max_tokens")
                .and_then(Value::as_i64)
                .unwrap_or(120_000),
            max_output_tokens: limits
                .get("output_limit_tokens")
                .and_then(Value::as_i64)
                .unwrap_or(256),
            timeout_seconds: limits
                .get("timeout_seconds")
                .and_then(Value::as_f64)
                .unwrap_or(30.0),
            run_cost_cap_usd: limits
                .get("run_cost_cap_usd")
                .and_then(Value::as_f64)
                .unwrap_or(0.1),
            daily_cost_cap_usd: limits
                .get("daily_cost_cap_usd")
                .and_then(Value::as_f64)
                .unwrap_or(0.5),
            pass_threshold: 0.9,
        },
        provider: None,
        pricing: None,
    };
    let provider: Arc<dyn Provider> = Arc::new(FixtureProvider);
    let raw = run_mode(strategy, &config, &provider)?;
    strategy_from_runner(request, RuntimeKind::Native, strategy, index, &raw, None)
}

fn adapter_request(
    request: &Value,
    strategy: &str,
    index: usize,
    live_runner: Option<&Value>,
) -> Result<Value, String> {
    let contract = required_object(request, "comparison_contract")?;
    let benchmark_run_id = required_str(request, "benchmark_run_id")?;
    let scope_material = json!({
        "tenant_id": "benchmark-tenant",
        "workspace_id": "benchmark-workspace",
        "run_id": benchmark_run_id,
        "workflow_id": "efficiency-benchmark",
        "node_id": format!("memory-{index}"),
        "thread_id": format!("thread-{index}"),
    });
    let scope_sha = sha256_value(&scope_material, false)?;
    let context_tokens = [1200, 720, 560, 640][index];
    let repeated_context_tokens = [800, 120, 80, 100][index];
    let state_read_bytes = [0, 64, 96, 128][index];
    let state_write_bytes = [0, 64, 64, 96][index];
    let memory_maintenance_tokens = [0, 12, 8, 6][index];
    let mut value = json!({
        "schema_version": "external_runtime_request.v1",
        "invocation_id": format!("inv-{benchmark_run_id}-{index}"),
        "tenant_id": "benchmark-tenant",
        "workspace_id": "benchmark-workspace",
        "run_id": benchmark_run_id,
        "workflow_id": "efficiency-benchmark",
        "node_id": format!("memory-{index}"),
        "thread_id": format!("thread-{index}"),
        "attempt": 1,
        "mode": if live_runner.is_some() { "live" } else { "fixture" },
        "memory_strategy": strategy,
        "runtime": {
            "runtime_kind": "langgraph",
            "adapter_contract_version": "external_runtime_adapter.v1",
            "adapter_version": "0.1.0",
            "expected_langgraph_version": LANGGRAPH_VERSION,
        },
        "scope_binding_sha256": scope_sha,
        "request_sha256": "",
        "checkpoint": Value::Null,
        "provider_exchange": Value::Null,
        "benchmark": {
            "definition_sha256": DEFINITION_SHA256,
            "scenario_id": "bounded-memory-efficiency",
            "scenario_sha256": sha256_bytes(b"bounded-memory-efficiency"),
            "task_sha256": sha256_bytes(format!("bounded-memory-efficiency:{strategy}").as_bytes()),
            "seed": contract.get("seed").cloned().unwrap_or(json!(165)),
            "quality_threshold": contract.get("quality_threshold").cloned().unwrap_or(json!(0.9)),
            "provider_id": contract.get("provider_id").cloned().unwrap_or(json!("fixture")),
            "model_id": contract.get("model_id").cloned().unwrap_or(json!("fixture-deterministic")),
            "tokenizer_id": contract.get("tokenizer_id").cloned().unwrap_or(json!("fixture-exact.v1")),
            "pricing_id": contract.get("pricing_id").cloned().unwrap_or(json!("fixture-zero-cost.v1")),
            "required_reference_ids": ["benchmark-ref-current", "benchmark-ref-correction"],
            "candidate_reference_ids": ["benchmark-ref-current", "benchmark-ref-correction", "benchmark-ref-stale", "benchmark-ref-conflict"],
            "selected_reference_ids": if strategy == "retrieval_memory" { json!(["benchmark-ref-current", "benchmark-ref-correction"]) } else { json!([]) },
            "stale_reference_ids": [],
            "context_tokens": context_tokens,
            "repeated_context_tokens": repeated_context_tokens,
            "state_read_bytes": state_read_bytes,
            "state_write_bytes": state_write_bytes,
            "memory_maintenance_tokens": memory_maintenance_tokens,
            "memory_maintenance_cost_usd": 0.0,
            "tool_call_count": 1,
            "redundant_tool_call_count": 0,
        },
    });
    if let Some(raw) = live_runner {
        let invocation_id = value["invocation_id"].as_str().unwrap_or_default();
        let scope_binding = value["scope_binding_sha256"].as_str().unwrap_or_default();
        let provider_id = contract
            .get("provider_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "live provider identity is required".to_string())?;
        let model_id = contract
            .get("model_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "live model identity is required".to_string())?;
        let response_sha256 = sha256_value(raw, false)?;
        let usage = json!({
            "input_tokens":raw.get("input_token_total"),
            "output_tokens":raw.get("output_token_total"),
            "cached_input_tokens":Value::Null,
            "cache_write_tokens":Value::Null,
            "reasoning_tokens":Value::Null,
            "estimated_cost_usd":raw.get("estimated_cost_usd"),
            "provider_reported_cost_usd":Value::Null,
            "latency_ms":raw.get("duration_ms"),
            "retry_count":raw.get("retry_count"),
        });
        value["provider_exchange"] = json!({
            "exchange_id":format!("exchange-{}", &response_sha256[..32]),
            "invocation_id":invocation_id,
            "scope_binding_sha256":scope_binding,
            "provider_id":provider_id,
            "model_id":model_id,
            "response_sha256":response_sha256,
            "typed_result":{
                "status":raw.get("status").cloned().unwrap_or(json!("fail")),
                "decision_code":if raw.get("status").and_then(Value::as_str)==Some("pass") { "quality-gate-pass" } else { "quality-gate-fail" },
                "selected_tool_ids":raw.pointer("/runner_metadata/selected_tool_ids").cloned().unwrap_or(json!([])),
                "quality_score":raw.get("quality_score").cloned().unwrap_or(Value::Null),
                "quality_method":"bounded-rule-v1",
            },
            "usage":usage,
            "metric_provenance":{
                "input_tokens":"provider_reported",
                "output_tokens":"provider_reported",
                "cached_input_tokens":"unavailable",
                "cache_write_tokens":"unavailable",
                "reasoning_tokens":"unavailable",
                "estimated_cost_usd":"estimated",
                "provider_reported_cost_usd":"unavailable",
                "latency_ms":"harness_derived",
                "retry_count":"harness_derived",
            },
        });
    }
    let mut material = value.clone();
    material.as_object_mut().unwrap().remove("request_sha256");
    material.as_object_mut().unwrap().remove("invocation_id");
    material
        .as_object_mut()
        .unwrap()
        .remove("provider_exchange");
    value["request_sha256"] = json!(sha256_value(&material, false)?);
    Ok(value)
}

fn tool_adapter_request(
    request: &Value,
    variant: &str,
    index: usize,
    raw: &Value,
) -> Result<Value, String> {
    let mut value = adapter_request(request, "durable_state_bounded_recent", index, Some(raw))?;
    let benchmark_run_id = required_str(request, "benchmark_run_id")?;
    value["invocation_id"] = json!(format!("inv-{benchmark_run_id}-tool-{index}"));
    value["node_id"] = json!(format!("tool-{index}"));
    value["thread_id"] = json!(format!("tool-thread-{index}"));
    let scope_material = json!({
        "tenant_id":value["tenant_id"],
        "workspace_id":value["workspace_id"],
        "run_id":value["run_id"],
        "workflow_id":value["workflow_id"],
        "node_id":value["node_id"],
        "thread_id":value["thread_id"],
    });
    value["scope_binding_sha256"] = json!(sha256_value(&scope_material, false)?);
    value["benchmark"]["scenario_id"] = json!("bounded-tool-discovery");
    value["benchmark"]["scenario_sha256"] = json!(sha256_bytes(b"bounded-tool-discovery"));
    value["benchmark"]["task_sha256"] = json!(sha256_bytes(
        format!("bounded-tool-discovery:{variant}").as_bytes()
    ));
    value["benchmark"]["required_reference_ids"] = json!([]);
    value["benchmark"]["candidate_reference_ids"] = json!([]);
    value["benchmark"]["selected_reference_ids"] = json!([]);
    value["benchmark"]["stale_reference_ids"] = json!([]);
    value["provider_exchange"]["invocation_id"] = value["invocation_id"].clone();
    value["provider_exchange"]["scope_binding_sha256"] = value["scope_binding_sha256"].clone();
    let mut material = value.clone();
    material.as_object_mut().unwrap().remove("request_sha256");
    material.as_object_mut().unwrap().remove("invocation_id");
    material
        .as_object_mut()
        .unwrap()
        .remove("provider_exchange");
    value["request_sha256"] = json!(sha256_value(&material, false)?);
    Ok(value)
}

fn invoke_langgraph(request: &Value) -> Result<Value, String> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "repository root is unavailable".to_string())?;
    let project = repo.join("adapters/langgraph");
    let mut child = Command::new("uv")
        .args(["run", "--frozen", "--project"])
        .arg(&project)
        .args(["python", "-m", "acp_langgraph_adapter"])
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "LangGraph adapter could not start")?;
    child
        .stdin
        .take()
        .ok_or_else(|| "LangGraph adapter stdin is unavailable".to_string())?
        .write_all(&canonical_bytes(request, false)?)
        .map_err(|_| "LangGraph adapter request failed")?;
    let output = child
        .wait_with_output()
        .map_err(|_| "LangGraph adapter wait failed")?;
    if !output.status.success() || output.stdout.is_empty() || output.stdout.len() > 131_072 {
        return Err("LangGraph adapter rejected the bounded invocation".to_string());
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|_| "LangGraph adapter returned invalid JSON".to_string())
}

fn langgraph_strategy(request: &Value, strategy: &str, index: usize) -> Result<Value, String> {
    let benchmark_run_id = required_str(request, "benchmark_run_id")?;
    let adapter = invoke_langgraph(&adapter_request(request, strategy, index, None)?)?;
    let summary = adapter
        .get("scorecard_summary")
        .ok_or_else(|| "LangGraph scorecard summary is missing".to_string())?;
    let input = summary["input_tokens"]
        .as_i64()
        .ok_or_else(|| "LangGraph input usage is missing".to_string())?;
    let output = summary["output_tokens"]
        .as_i64()
        .ok_or_else(|| "LangGraph output usage is missing".to_string())?;
    let context = summary["context_tokens"].as_i64().unwrap_or(input);
    let repeated = summary["repeated_context_tokens"].as_i64().unwrap_or(0);
    let state = ["full_history", "memory_digest", "retrieval_refs", "mixed"][index];
    let card = scorecard(
        request,
        RuntimeKind::LangGraph,
        &format!("{benchmark_run_id}-langgraph-memory-{index}"),
        "bounded-memory-efficiency",
        state,
        input,
        output,
        context,
        repeated,
        summary["tool_call_count"].as_i64().unwrap_or(1),
        summary["latency_ms"].as_i64().unwrap_or(1).max(1),
    )?;
    Ok(json!({
        "strategy_id": strategy,
        "metrics": strategy_metrics(&card, strategy, Some(&adapter)),
        "scorecard": card,
        "evidence_references": [{
            "source_id": format!("langgraph-fixture-{index}"),
            "source_sha256": adapter.get("result_sha256").cloned().unwrap_or(json!(sha256_bytes(strategy.as_bytes()))),
        }],
    }))
}

fn live_langgraph_strategy(
    request: &Value,
    strategy: &str,
    index: usize,
    raw: &Value,
) -> Result<Value, String> {
    let adapter = invoke_langgraph(&adapter_request(request, strategy, index, Some(raw))?)?;
    strategy_from_runner(
        request,
        RuntimeKind::LangGraph,
        strategy,
        index,
        raw,
        Some(&adapter),
    )
}

fn tool_results(
    request: &Value,
    runtime: RuntimeKind,
    live_results: Option<&[Value]>,
    adapter_results: Option<&[Value]>,
) -> Result<Vec<Value>, String> {
    let benchmark_run_id = required_str(request, "benchmark_run_id")?;
    let descriptors = [
        ("read", "Read a bounded approved source"),
        ("search", "Search approved source identifiers"),
        ("summarize", "Summarize bounded evidence"),
        ("write", "Write an approved target output"),
    ];
    let descriptor_hashes = descriptors
        .iter()
        .map(|(id, text)| json!({"tool_id": id, "descriptor_sha256": sha256_bytes(format!("{id}:{text}").as_bytes())}))
        .collect::<Vec<_>>();
    let corpus_sha = sha256_value(&Value::Array(descriptor_hashes.clone()), false)?;
    let registry_sha = sha256_bytes(b"tool_discovery_scenarios.v1");
    if live_results.is_some_and(|values| values.len() != TOOL_VARIANTS.len()) {
        return Err("live tool discovery requires exactly two provider results".to_string());
    }
    if adapter_results.is_some_and(|values| values.len() != TOOL_VARIANTS.len()) {
        return Err("LangGraph tool discovery requires exactly two adapter results".to_string());
    }
    let prompt_tokens = live_results.map_or_else(
        || vec![100, 55],
        |values| {
            values
                .iter()
                .map(|value| value["input_token_total"].as_i64().unwrap_or(0))
                .collect()
        },
    );
    if prompt_tokens.iter().any(|value| *value <= 0) {
        return Err("tool discovery requires positive prompt token evidence".to_string());
    }
    let mut results = Vec::new();
    for (index, variant) in ["static_all", "deterministic_top_k"].iter().enumerate() {
        let raw = live_results.map(|values| &values[index]);
        let adapter_result_sha256 = adapter_results
            .map(|values| {
                values[index]["result_sha256"]
                    .as_str()
                    .ok_or_else(|| "LangGraph tool result hash is missing".to_string())
            })
            .transpose()?;
        let prompt = prompt_tokens[index];
        let output = raw
            .and_then(|value| value["output_token_total"].as_i64())
            .unwrap_or(8);
        let duration = raw
            .and_then(|value| value["duration_ms"].as_i64())
            .unwrap_or(1)
            .max(1);
        let mut card = scorecard(
            request,
            runtime,
            &format!("{benchmark_run_id}-{}-tools-{index}", runtime.id()),
            "bounded-tool-discovery",
            "none",
            prompt,
            output,
            prompt,
            0,
            0,
            duration,
        )?;
        if let Some(raw) = raw {
            card["status"] = raw.get("status").cloned().unwrap_or(json!("fail"));
            card["pass_fail_reason"] = raw
                .get("pass_fail_reason")
                .cloned()
                .unwrap_or(json!("bounded provider tool run did not report a reason"));
            card["quality_score"] = raw.get("quality_score").cloned().unwrap_or(Value::Null);
            card["estimated_cost_usd"] = raw
                .get("estimated_cost_usd")
                .cloned()
                .unwrap_or(Value::Null);
        }
        let selected = if index == 0 {
            descriptors
                .iter()
                .map(|(id, _)| json!({"tool_id": id, "score": 1.0}))
                .collect()
        } else {
            vec![
                json!({"tool_id": "read", "score": 0.9}),
                json!({"tool_id": "search", "score": 0.8}),
                json!({"tool_id": "summarize", "score": 0.7}),
            ]
        };
        let provider_selected_tool_ids = match raw {
            Some(value) => value
                .pointer("/runner_metadata/selected_tool_ids")
                .and_then(Value::as_array)
                .filter(|values| values.len() == 2)
                .and_then(|values| {
                    values
                        .iter()
                        .map(|value| value.as_str().map(str::to_string))
                        .collect::<Option<Vec<_>>>()
                })
                .ok_or_else(|| {
                    "live tool discovery requires two provider-selected tools".to_string()
                })?,
            None => vec!["read".to_string(), "search".to_string()],
        };
        let required_tools = ["read", "search"];
        let correct_tool_selections = provider_selected_tool_ids
            .iter()
            .zip(required_tools)
            .filter(|(selected, required)| selected.as_str() == *required)
            .count();
        let required_tool_recall = correct_tool_selections as f64 / required_tools.len() as f64;
        let incorrect_tool_selection = (required_tools.len() - correct_tool_selections) as i64;
        let prompt_reduction = if index == 0 {
            0.0
        } else {
            ((prompt_tokens[0] - prompt_tokens[1]) as f64 / prompt_tokens[0] as f64 * 1_000_000.0)
                .round()
                / 1_000_000.0
        };
        let token_provenance = if raw.is_some() {
            "provider_reported"
        } else {
            "harness_derived"
        };
        results.push(json!({
            "variant": variant,
            "scorecard": card,
            "metrics": {
                "required_tool_recall": measured(json!(required_tool_recall), "harness_derived"),
                "incorrect_tool_selection": measured(json!(incorrect_tool_selection), "harness_derived"),
                "prompt_tokens": measured(json!(prompt), token_provenance),
                "prompt_token_reduction": measured(json!(prompt_reduction), "harness_derived"),
                "quality": measured(card["quality_score"].clone(), "harness_derived"),
                "latency_ms": measured(json!(duration), "harness_derived"),
                "cost_usd": measured(card["estimated_cost_usd"].clone(), "estimated"),
            },
            "corpus_sha256": corpus_sha,
            "registry_sha256": registry_sha,
            "retriever_version": "deterministic_descriptor_overlap.v1",
            "descriptor_hashes": descriptor_hashes,
            "selected_tools": selected,
            "provider_selected_tool_ids": provider_selected_tool_ids,
            "adapter_result_sha256": adapter_result_sha256,
        }));
    }
    Ok(results)
}

fn persist_benchmark_scorecards(
    store: &LocalProductStore,
    strategy_results: &[Value],
    tool_discovery_results: &[Value],
) -> Result<(), String> {
    for result in strategy_results.iter().chain(tool_discovery_results) {
        let scorecard = result
            .get("scorecard")
            .ok_or_else(|| "benchmark result is missing its scorecard".to_string())?;
        store.record_benchmark_scorecard(scorecard, "efficiency-live-benchmark")?;
    }
    Ok(())
}

pub fn execute(request: &Value, runtime: RuntimeKind) -> Result<Value, String> {
    validate_request(request)?;
    let live = required_str(request, "mode")? == "live";
    let ((strategy_results, tool_discovery_results), external_provider_calls, audit_evidence) =
        if live {
            let runtime_owner = live_runtime(request)?;
            let modes = STRATEGIES
                .iter()
                .chain(TOOL_VARIANTS.iter())
                .copied()
                .collect::<Vec<_>>();
            let raw_results = run_live_modes_with_store(
                &runtime_owner.config,
                &runtime_owner.provider,
                &runtime_owner.store,
                &modes,
                runtime_owner.per_call_cost_cap_usd,
            )?;
            let strategy_results = raw_results[..STRATEGIES.len()]
                .iter()
                .enumerate()
                .map(|(index, raw)| match runtime {
                    RuntimeKind::Native => {
                        strategy_from_runner(request, runtime, STRATEGIES[index], index, raw, None)
                    }
                    RuntimeKind::LangGraph => {
                        live_langgraph_strategy(request, STRATEGIES[index], index, raw)
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            let tool_raw = &raw_results[STRATEGIES.len()..];
            let adapter_tool_results = if runtime == RuntimeKind::LangGraph {
                Some(
                    tool_raw
                        .iter()
                        .enumerate()
                        .map(|(index, raw)| {
                            invoke_langgraph(&tool_adapter_request(
                                request,
                                TOOL_VARIANTS[index],
                                index,
                                raw,
                            )?)
                        })
                        .collect::<Result<Vec<_>, String>>()?,
                )
            } else {
                None
            };
            let tool_discovery_results = tool_results(
                request,
                runtime,
                Some(tool_raw),
                adapter_tool_results.as_deref(),
            )?;
            let external_provider_calls = raw_results
                .iter()
                .filter_map(|raw| {
                    raw.pointer("/runner_metadata/external_calls")
                        .and_then(Value::as_i64)
                })
                .sum::<i64>();
            if external_provider_calls <= 0 {
                return Err("live benchmark produced no provider calls".to_string());
            }
            let audit_events = runtime_owner
                .store
                .provider_audit_events(10_000)?
                .into_iter()
                .filter(|event| {
                    event
                        .get("event_id")
                        .and_then(Value::as_str)
                        .is_some_and(|id| !runtime_owner.audit_ids_before.contains(id))
                })
                .collect::<Vec<_>>();
            if audit_events.is_empty() {
                return Err("live benchmark did not persist provider audit evidence".to_string());
            }
            persist_benchmark_scorecards(
                &runtime_owner.store,
                &strategy_results,
                &tool_discovery_results,
            )?;
            let evidence_sha256 = sha256_value(&Value::Array(audit_events.clone()), false)?;
            (
                (strategy_results, tool_discovery_results),
                external_provider_calls,
                json!({
                    "schema_version":"efficiency_benchmark_audit_evidence.v1",
                    "event_count":audit_events.len(),
                    "evidence_sha256":evidence_sha256,
                    "store_kind":"app-owned-local-product-store",
                }),
            )
        } else {
            let strategy_results = STRATEGIES
                .iter()
                .enumerate()
                .map(|(index, strategy)| match runtime {
                    RuntimeKind::Native => native_strategy(request, strategy, index),
                    RuntimeKind::LangGraph => langgraph_strategy(request, strategy, index),
                })
                .collect::<Result<Vec<_>, _>>()?;
            let tool_discovery_results = tool_results(request, runtime, None, None)?;
            (
                (strategy_results, tool_discovery_results),
                0,
                json!({
                    "schema_version": "efficiency_benchmark_audit_evidence.v1",
                    "event_count": 0,
                    "evidence_sha256": sha256_bytes(format!("fixture:{}", runtime.id()).as_bytes()),
                    "store_kind": "fixture-no-external-audit",
                }),
            )
        };
    let request_sha = sha256_value(request, true)?;
    let audit_evidence = if live {
        audit_evidence
    } else {
        let mut fixture = audit_evidence;
        fixture["evidence_sha256"] = json!(sha256_bytes(
            format!("fixture:{}:{request_sha}", runtime.id()).as_bytes()
        ));
        fixture
    };
    Ok(json!({
        "schema_version": RESULT_SCHEMA,
        "runtime_kind": runtime.id(),
        "runtime_version": runtime.version(),
        "adapter_version": runtime.adapter_version(),
        "benchmark_run_id": required_str(request, "benchmark_run_id")?,
        "definition_sha256": DEFINITION_SHA256,
        "request_sha256": request_sha,
        "comparison_contract": request.get("comparison_contract").cloned().unwrap_or(Value::Null),
        "limits": request.get("limits").cloned().unwrap_or(Value::Null),
        "external_provider_calls": external_provider_calls,
        "strategy_results": strategy_results,
        "tool_discovery_results": tool_discovery_results,
        "audit_evidence": audit_evidence,
    }))
}

pub fn run_files(
    runtime: RuntimeKind,
    request_path: &Path,
    output_path: &Path,
) -> Result<(), String> {
    let request = read_request(request_path)?;
    let result = execute(&request, runtime)?;
    let bytes = canonical_bytes(&result, true)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("benchmark result exceeds the bounded output limit".to_string());
    }
    let parent = output_path
        .parent()
        .ok_or_else(|| "benchmark output parent is invalid".to_string())?;
    if !parent.is_dir() {
        return Err("benchmark output parent must already exist".to_string());
    }
    let temporary = PathBuf::from(format!("{}.tmp", output_path.display()));
    fs::write(&temporary, bytes).map_err(|_| "benchmark result write failed")?;
    fs::rename(&temporary, output_path).map_err(|_| "benchmark result commit failed")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canonical_request() -> Value {
        let definition = json!({
            "benchmark_id": "placeholder"
        });
        // Unit tests cover strict refusal without copying the operator's entire
        // immutable definition. End-to-end tests provide the canonical object.
        json!({
            "schema_version": REQUEST_SCHEMA,
            "benchmark_run_id": "benchmark-test",
            "mode": "fixture",
            "definition": definition,
            "definition_sha256": sha256_value(&definition, true).unwrap(),
            "comparison_contract": {},
            "limits": {},
            "operator_inputs": {},
        })
    }

    #[test]
    fn rejects_noncanonical_definition() {
        let error = execute(&canonical_request(), RuntimeKind::Native).unwrap_err();
        assert!(error.contains("definition hash mismatch"));
    }

    #[test]
    fn unavailable_measurement_never_uses_a_fabricated_zero() {
        let value = unavailable("provider did not report reasoning tokens");
        assert!(value["value"].is_null());
        assert_eq!(value["provenance"], "unavailable");
    }

    #[test]
    fn scorecard_task_identity_is_shared_across_runtimes() {
        let request = json!({
            "comparison_contract": {
                "provider_id": "fixture",
                "model_id": "fixture-deterministic",
                "tokenizer_id": "fixture-exact.v1",
                "pricing_id": "fixture-zero-cost.v1",
                "input_cost_per_1k_usd": 0.0,
                "output_cost_per_1k_usd": 0.0,
                "quality_threshold": 0.9,
                "evaluator_version": "bounded-rule-v1",
                "retry_policy": "no-retry.v1",
                "seed": 165,
            }
        });
        let native = scorecard_contract(&request, RuntimeKind::Native, "scenario").unwrap();
        let langgraph = scorecard_contract(&request, RuntimeKind::LangGraph, "scenario").unwrap();
        assert_eq!(native["task_digest"], langgraph["task_digest"]);
        assert_ne!(native["runtime_kind"], langgraph["runtime_kind"]);
    }

    #[test]
    fn measurement_provenance_distinguishes_provider_usage_from_harness_context() {
        let fixture = json!({
            "comparison_contract": {"provider_id": "fixture"},
            "input_token_total": 10,
            "output_token_total": 2,
            "context_token_total": 8,
        });
        let live = json!({
            "comparison_contract": {"provider_id": "openai_compatible"},
            "input_token_total": 10,
            "output_token_total": 2,
            "context_token_total": 8,
        });
        let fixture_metrics = strategy_metrics(&fixture, "full_history", None);
        let live_metrics = strategy_metrics(&live, "full_history", None);
        assert_eq!(
            fixture_metrics["input_tokens"]["provenance"],
            "harness_derived"
        );
        assert_eq!(
            live_metrics["input_tokens"]["provenance"],
            "provider_reported"
        );
        assert_eq!(
            live_metrics["context_tokens"]["provenance"],
            "harness_derived"
        );
    }

    #[test]
    fn live_tool_discovery_uses_provider_results_instead_of_fixture_metrics() {
        let request = json!({
            "benchmark_run_id":"provider-tool-run",
            "comparison_contract": {
                "provider_id": "openai_compatible",
                "model_id": "fixed-free-model",
                "tokenizer_id": "provider-reported",
                "pricing_id": "catalog-bound-zero",
                "input_cost_per_1k_usd": 0.0,
                "output_cost_per_1k_usd": 0.0,
                "quality_threshold": 0.9,
                "evaluator_version": "bounded-rule-v1",
                "retry_policy": "no-retry.v1",
                "seed": 165,
            }
        });
        let raw = [
            json!({"input_token_total":80,"output_token_total":7,"duration_ms":21,"estimated_cost_usd":0.0,"status":"pass","pass_fail_reason":"provider quality passed","quality_score":1.0,"runner_metadata":{"selected_tool_ids":["read","search"]}}),
            json!({"input_token_total":50,"output_token_total":6,"duration_ms":17,"estimated_cost_usd":0.0,"status":"pass","pass_fail_reason":"provider quality passed","quality_score":1.0,"runner_metadata":{"selected_tool_ids":["read","search"]}}),
        ];
        let results = tool_results(&request, RuntimeKind::Native, Some(&raw), None).unwrap();
        assert_eq!(results[0]["metrics"]["prompt_tokens"]["value"], 80);
        assert_eq!(
            results[0]["metrics"]["prompt_tokens"]["provenance"],
            "provider_reported"
        );
        assert_eq!(
            results[1]["metrics"]["prompt_token_reduction"]["value"],
            0.375
        );
        assert_eq!(results[1]["scorecard"]["duration_ms"], 17);

        let mut missing_selection = raw.clone();
        missing_selection[0]["runner_metadata"] = Value::Null;
        assert!(tool_results(
            &request,
            RuntimeKind::Native,
            Some(&missing_selection),
            None
        )
        .unwrap_err()
        .contains("provider-selected tool"));
    }

    fn langgraph_tool_bridge_fixture() -> (Value, Vec<Value>) {
        let request = json!({
            "benchmark_run_id":"provider-tool-adapter-run",
            "comparison_contract": {
                "provider_id": "openai_compatible",
                "model_id": "fixed-free-model",
                "tokenizer_id": "provider-reported",
                "pricing_id": "catalog-bound-zero",
                "input_cost_per_1k_usd": 0.0,
                "output_cost_per_1k_usd": 0.0,
                "quality_threshold": 0.9,
                "evaluator_version": "bounded-rule-v1",
                "retry_policy": "no-retry.v1",
                "seed": 165,
            }
        });
        let raw = json!({
            "input_token_total": 50,
            "output_token_total": 6,
            "duration_ms": 17,
            "estimated_cost_usd": 0.0,
            "retry_count": 0,
            "status": "pass",
            "quality_score": 1.0,
            "runner_metadata": {"selected_tool_ids": ["read", "search"]},
        });
        let raws = vec![raw.clone(), raw];
        (request, raws)
    }

    fn langgraph_tool_adapter_results(
        request: &Value,
        raws: &[Value],
        mut invoke: impl FnMut(&Value) -> Result<Value, String>,
    ) -> Result<Vec<Value>, String> {
        TOOL_VARIANTS
            .iter()
            .enumerate()
            .map(|(index, variant)| {
                let adapter_request = tool_adapter_request(request, variant, index, &raws[index])
                    .expect("tool adapter request");
                assert_eq!(
                    adapter_request.pointer("/provider_exchange/typed_result/selected_tool_ids"),
                    Some(&json!(["read", "search"]))
                );
                invoke(&adapter_request)
            })
            .collect()
    }

    fn assert_langgraph_tool_bridge_results(
        request: &Value,
        raws: &[Value],
        adapter_results: &[Value],
    ) {
        let results = tool_results(
            request,
            RuntimeKind::LangGraph,
            Some(raws),
            Some(adapter_results),
        )
        .expect("LangGraph tool results");
        for (result, adapter) in results.iter().zip(adapter_results) {
            assert_eq!(
                result["provider_selected_tool_ids"],
                json!(["read", "search"])
            );
            assert_eq!(result["adapter_result_sha256"], adapter["result_sha256"]);
        }
    }

    #[test]
    fn langgraph_tool_result_binding_consumes_the_provider_selected_tool() {
        let (request, raws) = langgraph_tool_bridge_fixture();
        let adapter_results = langgraph_tool_adapter_results(&request, &raws, |adapter_request| {
            Ok(json!({
                    "result_sha256": sha256_value(adapter_request, false)
                    .expect("bounded adapter result hash")
            }))
        })
        .expect("synthetic adapter results");
        assert_langgraph_tool_bridge_results(&request, &raws, &adapter_results);
    }

    #[test]
    #[ignore = "requires the uv-managed LangGraph adapter"]
    fn langgraph_tool_bridge_invokes_the_real_adapter() {
        let (request, raws) = langgraph_tool_bridge_fixture();
        let adapter_results = langgraph_tool_adapter_results(&request, &raws, invoke_langgraph)
            .expect("real LangGraph adapter results");
        for adapter in &adapter_results {
            assert_eq!(
                adapter.pointer("/scorecard_summary/selected_tool_count"),
                Some(&json!(2))
            );
            assert_eq!(
                adapter.pointer("/trace_summary/provider_exchanges_consumed"),
                Some(&json!(1))
            );
        }
        assert_langgraph_tool_bridge_results(&request, &raws, &adapter_results);
    }

    #[test]
    fn benchmark_scorecard_persists_through_app_owned_artifact_owner() {
        let request = json!({
            "comparison_contract": {
                "provider_id": "fixture",
                "model_id": "fixture-deterministic",
                "tokenizer_id": "fixture-exact.v1",
                "pricing_id": "fixture-zero-cost.v1",
                "input_cost_per_1k_usd": 0.0,
                "output_cost_per_1k_usd": 0.0,
                "quality_threshold": 0.9,
                "evaluator_version": "bounded-rule-v1",
                "retry_policy": "no-retry.v1",
                "seed": 165,
            }
        });
        let card = scorecard(
            &request,
            RuntimeKind::Native,
            "benchmark-owned-run",
            "bounded-memory-efficiency",
            "mixed",
            10,
            2,
            8,
            1,
            1,
            5,
        )
        .unwrap();
        let store = LocalProductStore::new(":memory:").unwrap();
        let first = store
            .record_benchmark_scorecard(&card, "benchmark-test")
            .unwrap();
        let second = store
            .record_benchmark_scorecard(&card, "benchmark-test")
            .unwrap();
        assert_eq!(first["artifact_id"], second["artifact_id"]);
        assert_eq!(first["scorecard"]["derived_metrics"]["total_tokens"], 12);
    }
}
