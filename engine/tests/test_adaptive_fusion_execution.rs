use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use engine::feedback::{
    AdaptiveExplorationGate, CandidateAggregate, CandidateKind, ContextualPolicyPromotion,
    ContextualPolicyPromotionGate, ObjectiveProfile, CONTEXTUAL_POLICY_PROMOTION_SCHEMA_VERSION,
    CONTEXTUAL_POLICY_SCHEMA_VERSION,
};
use engine::node_executor::{NodeExecutionInput, NodeExecutor};
use engine::provider::adaptive_execution::{
    parse_adaptive_provider_endpoints_json, AdaptiveEndpointInvocation, AdaptiveExecutionExecutor,
    AdaptiveExecutionGate, AdaptiveExecutionKillSwitch, AdaptiveExecutionLimits,
    AdaptiveExecutionPlan, AdaptiveExecutionRequest, AdaptiveProviderNodeExecutor,
};
use engine::provider::{
    CostGateConfig, Provider, ProviderAuditRecorder, ProviderError, ProviderRequest,
    ProviderResponse, ProviderResult,
};
use serde_json::json;

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct ScriptedProvider {
    provider_id: String,
    responses: Mutex<VecDeque<ProviderResult>>,
    calls: AtomicUsize,
    prompts: Mutex<Vec<String>>,
    call_order: Arc<Mutex<Vec<String>>>,
    delay_ms: u64,
    enabled: bool,
    kill_on_call: Option<AdaptiveExecutionKillSwitch>,
    concurrency: Option<Arc<ConcurrencyTracker>>,
}

#[derive(Default)]
struct ConcurrencyTracker {
    active: AtomicUsize,
    max_active: AtomicUsize,
}

impl ConcurrencyTracker {
    fn enter(&self) {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(active, Ordering::SeqCst);
    }

    fn leave(&self) {
        self.active.fetch_sub(1, Ordering::SeqCst);
    }

    fn max_active(&self) -> usize {
        self.max_active.load(Ordering::SeqCst)
    }
}

impl ScriptedProvider {
    fn new(
        provider_id: &str,
        responses: Vec<ProviderResult>,
        call_order: Arc<Mutex<Vec<String>>>,
    ) -> Self {
        Self {
            provider_id: provider_id.to_string(),
            responses: Mutex::new(responses.into()),
            calls: AtomicUsize::new(0),
            prompts: Mutex::new(Vec::new()),
            call_order,
            delay_ms: 0,
            enabled: true,
            kill_on_call: None,
            concurrency: None,
        }
    }

    fn with_delay(mut self, delay_ms: u64) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    fn with_kill_on_call(mut self, kill_switch: AdaptiveExecutionKillSwitch) -> Self {
        self.kill_on_call = Some(kill_switch);
        self
    }

    fn with_concurrency_tracker(mut self, concurrency: Arc<ConcurrencyTracker>) -> Self {
        self.concurrency = Some(concurrency);
        self
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn prompts(&self) -> Vec<String> {
        self.prompts.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl Provider for ScriptedProvider {
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn default_model(&self) -> Option<&str> {
        Some("test-model")
    }

    async fn invoke(&self, request: &ProviderRequest) -> ProviderResult {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.prompts.lock().unwrap().push(request.prompt.clone());
        self.call_order
            .lock()
            .unwrap()
            .push(self.provider_id.clone());
        if let Some(kill_switch) = &self.kill_on_call {
            kill_switch.kill();
        }
        if let Some(concurrency) = &self.concurrency {
            concurrency.enter();
        }
        if self.delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
        }
        if let Some(concurrency) = &self.concurrency {
            concurrency.leave();
        }
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Err(provider_error(&self.provider_id, "script_exhausted")))
    }
}

fn response(provider_id: &str, output: &str, cost: f64) -> ProviderResult {
    response_with_usage(provider_id, output, cost, 10, 5)
}

fn response_with_usage(
    provider_id: &str,
    output: &str,
    cost: f64,
    input_tokens: i64,
    output_tokens: i64,
) -> ProviderResult {
    Ok(ProviderResponse {
        schema_version: "provider_response.v1".to_string(),
        provider_id: provider_id.to_string(),
        model: "test-model".to_string(),
        output: output.to_string(),
        input_tokens: Some(input_tokens),
        output_tokens: Some(output_tokens),
        estimated_cost: Some(cost),
        provider_request_id: Some(format!("request-{provider_id}")),
    })
}

fn provider_error(provider_id: &str, domain: &str) -> ProviderError {
    ProviderError {
        schema_version: "provider_error.v1".to_string(),
        provider_id: provider_id.to_string(),
        error_domain: domain.to_string(),
        message: format!("{domain} from {provider_id}"),
        retryable: false,
    }
}

fn endpoint(endpoint_id: &str, reserved_cost_usd: f64) -> AdaptiveEndpointInvocation {
    AdaptiveEndpointInvocation::new(endpoint_id, "test-model", reserved_cost_usd)
}

fn limits(max_calls: usize, max_cost_usd: f64, max_elapsed_ms: u64) -> AdaptiveExecutionLimits {
    AdaptiveExecutionLimits::new(max_calls, max_cost_usd, max_elapsed_ms, 1)
}

fn request(
    plan: AdaptiveExecutionPlan,
    limits: AdaptiveExecutionLimits,
) -> AdaptiveExecutionRequest {
    AdaptiveExecutionRequest::new("dispatch-af3", "solve the task", plan, limits)
}

fn enabled_gate() -> AdaptiveExecutionGate {
    AdaptiveExecutionGate::from_flags(true, true, true)
}

fn contextual_policy() -> engine::feedback::PromotedAdaptivePolicy {
    ContextualPolicyPromotionGate::from_flags(true, true)
        .evaluate(&ContextualPolicyPromotion {
            schema_version: CONTEXTUAL_POLICY_PROMOTION_SCHEMA_VERSION.to_string(),
            task_class: "coding".to_string(),
            objective: ObjectiveProfile::Quality,
            candidate_id: "strong".to_string(),
            baseline_candidate_id: "cheap".to_string(),
            sample_count: 30,
            confidence: 0.9,
            mean_quality_delta: 0.1,
            mean_cost_reduction: 0.01,
            failure_rate_delta: 0.0,
            evidence_run_ids: (0..30).map(|index| format!("run-{index}")).collect(),
            risk_level: "low".to_string(),
            confirm_adaptive_policy_promotion: true,
        })
        .policy
        .unwrap()
}

fn contextual_candidate(id: &str, quality: f64, cost: f64) -> CandidateAggregate {
    CandidateAggregate {
        candidate_id: id.to_string(),
        candidate_kind: CandidateKind::Endpoint,
        member_endpoint_ids: vec![id.to_string()],
        sample_count: 30,
        evidence_run_ids: (0..30).map(|index| format!("run-{id}-{index}")).collect(),
        success_rate: quality,
        average_quality_score: quality,
        average_tool_success_score: quality,
        average_cost_usd: cost,
        average_latency_ms: 1000.0,
    }
}

fn executor(
    providers: Vec<(&str, Arc<dyn Provider>)>,
    audit: Arc<ProviderAuditRecorder>,
    kill_switch: AdaptiveExecutionKillSwitch,
) -> AdaptiveExecutionExecutor {
    AdaptiveExecutionExecutor::new(
        providers
            .into_iter()
            .map(|(endpoint_id, provider)| (endpoint_id.to_string(), provider))
            .collect::<BTreeMap<_, _>>(),
        audit,
        kill_switch,
    )
}

#[test]
fn adaptive_provider_endpoint_config_parses_multiple_provider_types() {
    let configs = parse_adaptive_provider_endpoints_json(
        &json!([
            {
                "endpoint_id": "local-fast",
                "provider_type": "stub",
                "model": "stub-fast",
                "timeout_ms": 1000
            },
            {
                "endpoint_id": "openai-quality",
                "provider_type": "openai_compatible",
                "base_url": "https://api.example.com/v1",
                "model": "model-quality",
                "credential_env": "OPENAI_QUALITY_KEY",
                "timeout_ms": 30000,
                "input_cost_per_1k_usd": 0.01,
                "output_cost_per_1k_usd": 0.03
            },
            {
                "endpoint_id": "anthropic-judge",
                "provider_type": "anthropic",
                "base_url": "http://127.0.0.1:8081",
                "model": "judge-model",
                "credential_env": "ANTHROPIC_JUDGE_KEY",
                "timeout_ms": 20000
            }
        ])
        .to_string(),
    )
    .unwrap();

    assert_eq!(configs.len(), 3);
    assert_eq!(configs[0].endpoint_id, "anthropic-judge");
    assert_eq!(configs[1].endpoint_id, "local-fast");
    assert_eq!(configs[2].endpoint_id, "openai-quality");
}

#[test]
fn adaptive_provider_endpoint_config_rejects_unsafe_or_unbounded_values() {
    let cases = [
        (
            json!([
                {"endpoint_id": "same", "provider_type": "stub", "model": "m"},
                {"endpoint_id": "same", "provider_type": "stub", "model": "m"}
            ])
            .to_string(),
            "duplicate_endpoint_id",
        ),
        (
            serde_json::to_string(
                &(0..9)
                    .map(|index| {
                        json!({
                            "endpoint_id": format!("endpoint-{index}"),
                            "provider_type": "stub",
                            "model": "m"
                        })
                    })
                    .collect::<Vec<_>>(),
            )
            .unwrap(),
            "endpoint_limit_exceeded",
        ),
        (
            json!([{
                "endpoint_id": "secret",
                "provider_type": "openai_compatible",
                "base_url": "https://api.example.com",
                "model": "m",
                "credential_env": "sk-abcdefghijklmnopqrstuvwxyz"
            }])
            .to_string(),
            "sensitive_pattern_detected",
        ),
        (
            json!([{
                "endpoint_id": "bad-env",
                "provider_type": "openai_compatible",
                "base_url": "https://api.example.com",
                "model": "m",
                "credential_env": "mixedCaseKey"
            }])
            .to_string(),
            "invalid_credential_env",
        ),
        (
            json!([{
                "endpoint_id": "remote-http",
                "provider_type": "openai_compatible",
                "base_url": "http://api.example.com",
                "model": "m",
                "credential_env": "REMOTE_KEY"
            }])
            .to_string(),
            "invalid_base_url",
        ),
        (
            json!([{
                "endpoint_id": "partial-pricing",
                "provider_type": "openai_compatible",
                "base_url": "https://api.example.com",
                "model": "m",
                "credential_env": "REMOTE_KEY",
                "input_cost_per_1k_usd": 0.01
            }])
            .to_string(),
            "invalid_pricing",
        ),
        (
            json!([{
                "endpoint_id": "bad-timeout",
                "provider_type": "stub",
                "model": "m",
                "timeout_ms": 500000
            }])
            .to_string(),
            "invalid_timeout_ms",
        ),
    ];

    for (raw, expected_code) in cases {
        let error = parse_adaptive_provider_endpoints_json(&raw).unwrap_err();
        assert_eq!(error.code, expected_code);
        assert!(!error.to_string().contains("sk-abcdefghijklmnopqrstuvwxyz"));
    }
}

#[test]
fn environment_gate_requires_both_execution_flags_and_auth() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    std::env::remove_var("ACP_ENABLE_PROVIDER_EXECUTION");
    std::env::remove_var("ACP_ENABLE_ADAPTIVE_FUSION_EXECUTION");
    assert!(!AdaptiveExecutionGate::from_env(true).is_enabled());

    std::env::set_var("ACP_ENABLE_PROVIDER_EXECUTION", "1");
    assert!(!AdaptiveExecutionGate::from_env(true).is_enabled());

    std::env::set_var("ACP_ENABLE_ADAPTIVE_FUSION_EXECUTION", "true");
    assert!(!AdaptiveExecutionGate::from_env(false).is_enabled());
    assert!(AdaptiveExecutionGate::from_env(true).is_enabled());

    std::env::remove_var("ACP_ENABLE_PROVIDER_EXECUTION");
    std::env::remove_var("ACP_ENABLE_ADAPTIVE_FUSION_EXECUTION");
}

#[tokio::test]
async fn all_provider_adaptive_and_auth_gates_are_required_before_any_call() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let provider = Arc::new(ScriptedProvider::new(
        "primary",
        vec![response("primary", "ok", 0.01)],
        order,
    ));
    let execution = executor(
        vec![("primary", provider.clone())],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::from_flags(false),
    );
    let request = request(
        AdaptiveExecutionPlan::Single {
            endpoint: endpoint("primary", 0.02),
        },
        limits(1, 0.1, 1_000),
    );

    for gate in [
        AdaptiveExecutionGate::from_flags(false, true, true),
        AdaptiveExecutionGate::from_flags(true, false, true),
        AdaptiveExecutionGate::from_flags(true, true, false),
    ] {
        let error = execution.execute(&request, &gate).await.unwrap_err();
        assert_eq!(error.code.as_ref(), "adaptive_execution_disabled");
    }

    assert_eq!(provider.calls(), 0);
}

#[tokio::test]
async fn single_execution_records_bounded_audit_and_result_evidence() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let provider = Arc::new(ScriptedProvider::new(
        "primary",
        vec![response("primary", "answer", 0.01)],
        order,
    ));
    let audit = Arc::new(ProviderAuditRecorder::new());
    let execution = executor(
        vec![("primary", provider.clone())],
        audit.clone(),
        AdaptiveExecutionKillSwitch::from_flags(false),
    );

    let result = execution
        .execute(
            &request(
                AdaptiveExecutionPlan::Single {
                    endpoint: endpoint("primary", 0.02),
                },
                limits(1, 0.1, 1_000),
            ),
            &enabled_gate(),
        )
        .await
        .unwrap();

    assert_eq!(result.output.as_deref(), Some("answer"));
    assert_eq!(result.selected_endpoint_id.as_deref(), Some("primary"));
    assert_eq!(result.calls.len(), 1);
    assert_eq!(result.total_provider_cost_usd, 0.01);
    assert_eq!(result.total_input_token_count, 10);
    assert_eq!(result.total_output_token_count, 5);
    assert_eq!(provider.calls(), 1);
    let events = audit.list_events("dispatch-af3");
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].event_type, "adaptive_single_request");
    assert_eq!(events[1].event_type, "adaptive_single_response");
    assert_eq!(events[2].event_type, "adaptive_execution_completed");
}

#[tokio::test]
async fn ordered_fallback_stops_after_first_success_and_preserves_order() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let first = Arc::new(ScriptedProvider::new(
        "first",
        vec![Err(provider_error("first", "provider_capacity"))],
        order.clone(),
    ));
    let second = Arc::new(ScriptedProvider::new(
        "second",
        vec![response("second", "fallback answer", 0.02)],
        order.clone(),
    ));
    let third = Arc::new(ScriptedProvider::new(
        "third",
        vec![response("third", "unused", 0.02)],
        order.clone(),
    ));
    let execution = executor(
        vec![
            ("first", first.clone()),
            ("second", second.clone()),
            ("third", third.clone()),
        ],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::from_flags(false),
    );

    let result = execution
        .execute(
            &request(
                AdaptiveExecutionPlan::OrderedFallback {
                    endpoints: vec![
                        endpoint("first", 0.02),
                        endpoint("second", 0.03),
                        endpoint("third", 0.03),
                    ],
                },
                limits(3, 0.1, 1_000),
            ),
            &enabled_gate(),
        )
        .await
        .unwrap();

    assert_eq!(result.output.as_deref(), Some("fallback answer"));
    assert_eq!(result.selected_endpoint_id.as_deref(), Some("second"));
    assert_eq!(*order.lock().unwrap(), vec!["first", "second"]);
    assert_eq!(first.calls(), 1);
    assert_eq!(second.calls(), 1);
    assert_eq!(third.calls(), 0);
}

#[tokio::test]
async fn fusion_executes_panel_then_judge_then_synthesizer() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let concurrency = Arc::new(ConcurrencyTracker::default());
    let panel_a = Arc::new(
        ScriptedProvider::new(
            "panel-a",
            vec![response("panel-a", "analysis a", 0.01)],
            order.clone(),
        )
        .with_delay(50)
        .with_concurrency_tracker(concurrency.clone()),
    );
    let panel_b = Arc::new(
        ScriptedProvider::new(
            "panel-b",
            vec![response("panel-b", "analysis b", 0.01)],
            order.clone(),
        )
        .with_delay(50)
        .with_concurrency_tracker(concurrency.clone()),
    );
    let panel_c = Arc::new(
        ScriptedProvider::new(
            "panel-c",
            vec![response("panel-c", "analysis c", 0.01)],
            order.clone(),
        )
        .with_delay(50)
        .with_concurrency_tracker(concurrency.clone()),
    );
    let judge = Arc::new(ScriptedProvider::new(
        "judge",
        vec![response("judge", "prefer b", 0.02)],
        order.clone(),
    ));
    let synthesizer = Arc::new(ScriptedProvider::new(
        "synth",
        vec![response("synth", "final answer", 0.02)],
        order.clone(),
    ));
    let execution = executor(
        vec![
            ("panel-a", panel_a),
            ("panel-b", panel_b),
            ("panel-c", panel_c),
            ("judge", judge.clone()),
            ("synth", synthesizer.clone()),
        ],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::from_flags(false),
    );

    let result = execution
        .execute(
            &request(
                AdaptiveExecutionPlan::Fusion {
                    panel: vec![
                        endpoint("panel-a", 0.02),
                        endpoint("panel-b", 0.02),
                        endpoint("panel-c", 0.02),
                    ],
                    judge: endpoint("judge", 0.03),
                    synthesizer: endpoint("synth", 0.03),
                },
                AdaptiveExecutionLimits::new(5, 0.2, 1_000, 3),
            ),
            &enabled_gate(),
        )
        .await
        .unwrap();

    assert_eq!(result.output.as_deref(), Some("final answer"));
    assert!(concurrency.max_active() >= 2);
    let order = order.lock().unwrap().clone();
    assert_eq!(&order[3..], ["judge", "synth"]);
    assert_eq!(
        result
            .calls
            .iter()
            .map(|call| call.endpoint_id.as_str())
            .collect::<Vec<_>>(),
        vec!["panel-a", "panel-b", "panel-c", "judge", "synth"]
    );
    let judge_prompt = judge.prompts().pop().unwrap();
    assert!(judge_prompt.contains("analysis a"));
    assert!(judge_prompt.contains("analysis b"));
    assert!(judge_prompt.contains("analysis c"));
    let synth_prompt = synthesizer.prompts().pop().unwrap();
    assert!(synth_prompt.contains("prefer b"));
}

#[tokio::test]
async fn fusion_continues_after_recoverable_panel_failure_when_quorum_is_met() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let panel_a = Arc::new(ScriptedProvider::new(
        "panel-a",
        vec![response("panel-a", "analysis a", 0.01)],
        order.clone(),
    ));
    let panel_b = Arc::new(ScriptedProvider::new(
        "panel-b",
        vec![Err(provider_error("panel-b", "provider_capacity"))],
        order.clone(),
    ));
    let panel_c = Arc::new(ScriptedProvider::new(
        "panel-c",
        vec![response("panel-c", "analysis c", 0.01)],
        order.clone(),
    ));
    let judge = Arc::new(ScriptedProvider::new(
        "judge",
        vec![response("judge", "prefer c", 0.02)],
        order.clone(),
    ));
    let synthesizer = Arc::new(ScriptedProvider::new(
        "synth",
        vec![response("synth", "final answer", 0.02)],
        order,
    ));
    let audit = Arc::new(ProviderAuditRecorder::new());
    let execution = executor(
        vec![
            ("panel-a", panel_a),
            ("panel-b", panel_b),
            ("panel-c", panel_c),
            ("judge", judge.clone()),
            ("synth", synthesizer),
        ],
        audit.clone(),
        AdaptiveExecutionKillSwitch::from_flags(false),
    );

    let result = execution
        .execute(
            &request(
                AdaptiveExecutionPlan::Fusion {
                    panel: vec![
                        endpoint("panel-a", 0.02),
                        endpoint("panel-b", 0.02),
                        endpoint("panel-c", 0.02),
                    ],
                    judge: endpoint("judge", 0.03),
                    synthesizer: endpoint("synth", 0.03),
                },
                AdaptiveExecutionLimits::new(5, 0.2, 1_000, 3).with_min_successful_panel_calls(2),
            ),
            &enabled_gate(),
        )
        .await
        .unwrap();

    assert_eq!(result.output.as_deref(), Some("final answer"));
    assert_eq!(
        result
            .calls
            .iter()
            .map(|call| (call.endpoint_id.as_str(), call.status.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("panel-a", "completed"),
            ("panel-b", "failed"),
            ("panel-c", "completed"),
            ("judge", "completed"),
            ("synth", "completed"),
        ]
    );
    let judge_prompt = judge.prompts().pop().unwrap();
    assert!(judge_prompt.contains("analysis a"));
    assert!(!judge_prompt.contains("panel-b"));
    assert!(judge_prompt.contains("analysis c"));
    assert!(audit
        .list_events("dispatch-af3")
        .iter()
        .any(|event| event.event_type == "adaptive_panel_partial_failure"));
}

#[tokio::test]
async fn fusion_blocks_judge_when_panel_success_quorum_is_not_met() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let successful = Arc::new(ScriptedProvider::new(
        "successful",
        vec![response("successful", "usable answer", 0.01)],
        order.clone(),
    ));
    let failed_a = Arc::new(ScriptedProvider::new(
        "failed-a",
        vec![Err(provider_error("failed-a", "provider_capacity"))],
        order.clone(),
    ));
    let failed_b = Arc::new(ScriptedProvider::new(
        "failed-b",
        vec![Err(provider_error("failed-b", "provider_capacity"))],
        order.clone(),
    ));
    let judge = Arc::new(ScriptedProvider::new(
        "judge",
        vec![response("judge", "unused", 0.01)],
        order.clone(),
    ));
    let synthesizer = Arc::new(ScriptedProvider::new(
        "synth",
        vec![response("synth", "unused", 0.01)],
        order,
    ));
    let execution = executor(
        vec![
            ("successful", successful),
            ("failed-a", failed_a),
            ("failed-b", failed_b),
            ("judge", judge.clone()),
            ("synth", synthesizer.clone()),
        ],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::from_flags(false),
    );

    let error = execution
        .execute(
            &request(
                AdaptiveExecutionPlan::Fusion {
                    panel: vec![
                        endpoint("successful", 0.02),
                        endpoint("failed-a", 0.02),
                        endpoint("failed-b", 0.02),
                    ],
                    judge: endpoint("judge", 0.02),
                    synthesizer: endpoint("synth", 0.02),
                },
                AdaptiveExecutionLimits::new(5, 0.2, 1_000, 2).with_min_successful_panel_calls(2),
            ),
            &enabled_gate(),
        )
        .await
        .unwrap_err();

    assert_eq!(error.code.as_ref(), "adaptive_panel_quorum_not_met");
    assert_eq!(judge.calls(), 0);
    assert_eq!(synthesizer.calls(), 0);
    assert_eq!(
        error
            .calls
            .iter()
            .map(|call| call.endpoint_id.as_str())
            .collect::<Vec<_>>(),
        vec!["successful", "failed-a", "failed-b"]
    );
}

#[tokio::test]
async fn fusion_default_quorum_stops_when_remaining_panel_cannot_recover() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let failed = Arc::new(ScriptedProvider::new(
        "failed",
        vec![Err(provider_error("failed", "provider_capacity"))],
        order.clone(),
    ));
    let unused = Arc::new(ScriptedProvider::new(
        "unused",
        vec![response("unused", "unused", 0.01)],
        order,
    ));
    let execution = executor(
        vec![("failed", failed.clone()), ("unused", unused.clone())],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::from_flags(false),
    );

    let error = execution
        .execute(
            &request(
                AdaptiveExecutionPlan::Fusion {
                    panel: vec![endpoint("failed", 0.02), endpoint("unused", 0.02)],
                    judge: endpoint("failed", 0.02),
                    synthesizer: endpoint("unused", 0.02),
                },
                AdaptiveExecutionLimits::new(4, 0.1, 1_000, 1),
            ),
            &enabled_gate(),
        )
        .await
        .unwrap_err();

    assert_eq!(error.code.as_ref(), "adaptive_panel_quorum_not_met");
    assert_eq!(failed.calls(), 1);
    assert_eq!(unused.calls(), 0);
}

#[tokio::test]
async fn fusion_kill_during_parallel_wave_prevents_next_wave_and_final_stages() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let kill_switch = AdaptiveExecutionKillSwitch::from_flags(false);
    let killing = Arc::new(
        ScriptedProvider::new(
            "killing",
            vec![response("killing", "unused", 0.01)],
            order.clone(),
        )
        .with_kill_on_call(kill_switch.clone()),
    );
    let in_flight = Arc::new(
        ScriptedProvider::new(
            "in-flight",
            vec![response("in-flight", "unused", 0.01)],
            order.clone(),
        )
        .with_delay(25),
    );
    let next_wave = Arc::new(ScriptedProvider::new(
        "next-wave",
        vec![response("next-wave", "unused", 0.01)],
        order.clone(),
    ));
    let judge = Arc::new(ScriptedProvider::new(
        "judge",
        vec![response("judge", "unused", 0.01)],
        order.clone(),
    ));
    let synthesizer = Arc::new(ScriptedProvider::new(
        "synth",
        vec![response("synth", "unused", 0.01)],
        order,
    ));
    let execution = executor(
        vec![
            ("killing", killing),
            ("in-flight", in_flight),
            ("next-wave", next_wave.clone()),
            ("judge", judge.clone()),
            ("synth", synthesizer.clone()),
        ],
        Arc::new(ProviderAuditRecorder::new()),
        kill_switch,
    );

    let error = execution
        .execute(
            &request(
                AdaptiveExecutionPlan::Fusion {
                    panel: vec![
                        endpoint("killing", 0.02),
                        endpoint("in-flight", 0.02),
                        endpoint("next-wave", 0.02),
                    ],
                    judge: endpoint("judge", 0.02),
                    synthesizer: endpoint("synth", 0.02),
                },
                AdaptiveExecutionLimits::new(5, 0.2, 1_000, 2),
            ),
            &enabled_gate(),
        )
        .await
        .unwrap_err();

    assert_eq!(error.code.as_ref(), "adaptive_execution_killed");
    assert_eq!(next_wave.calls(), 0);
    assert_eq!(judge.calls(), 0);
    assert_eq!(synthesizer.calls(), 0);
}

#[tokio::test]
async fn fusion_parallel_timeout_blocks_judge_and_synthesizer() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let slow = Arc::new(
        ScriptedProvider::new("slow", vec![response("slow", "late", 0.01)], order.clone())
            .with_delay(500),
    );
    let fast = Arc::new(ScriptedProvider::new(
        "fast",
        vec![response("fast", "quick", 0.01)],
        order.clone(),
    ));
    let judge = Arc::new(ScriptedProvider::new(
        "judge",
        vec![response("judge", "unused", 0.01)],
        order.clone(),
    ));
    let synthesizer = Arc::new(ScriptedProvider::new(
        "synth",
        vec![response("synth", "unused", 0.01)],
        order,
    ));
    let execution = executor(
        vec![
            ("slow", slow.clone()),
            ("fast", fast.clone()),
            ("judge", judge.clone()),
            ("synth", synthesizer.clone()),
        ],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::from_flags(false),
    );

    let error = execution
        .execute(
            &request(
                AdaptiveExecutionPlan::Fusion {
                    panel: vec![endpoint("slow", 0.02), endpoint("fast", 0.02)],
                    judge: endpoint("judge", 0.02),
                    synthesizer: endpoint("synth", 0.02),
                },
                AdaptiveExecutionLimits::new(4, 0.1, 200, 2),
            ),
            &enabled_gate(),
        )
        .await
        .unwrap_err();

    assert_eq!(error.code.as_ref(), "adaptive_execution_timeout");
    assert_eq!(slow.calls(), 1);
    assert_eq!(fast.calls(), 1);
    assert_eq!(judge.calls(), 0);
    assert_eq!(synthesizer.calls(), 0);
}

#[tokio::test]
async fn fusion_parallel_identity_mismatch_blocks_final_stages() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let mut mismatched = response("wrong-provider", "bad identity", 0.01).unwrap();
    mismatched.model = "test-model".to_string();
    let bad = Arc::new(ScriptedProvider::new(
        "bad",
        vec![Ok(mismatched)],
        order.clone(),
    ));
    let good = Arc::new(ScriptedProvider::new(
        "good",
        vec![response("good", "valid", 0.01)],
        order.clone(),
    ));
    let judge = Arc::new(ScriptedProvider::new(
        "judge",
        vec![response("judge", "unused", 0.01)],
        order.clone(),
    ));
    let synthesizer = Arc::new(ScriptedProvider::new(
        "synth",
        vec![response("synth", "unused", 0.01)],
        order,
    ));
    let execution = executor(
        vec![
            ("bad", bad),
            ("good", good),
            ("judge", judge.clone()),
            ("synth", synthesizer.clone()),
        ],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::from_flags(false),
    );

    let error = execution
        .execute(
            &request(
                AdaptiveExecutionPlan::Fusion {
                    panel: vec![endpoint("bad", 0.02), endpoint("good", 0.02)],
                    judge: endpoint("judge", 0.02),
                    synthesizer: endpoint("synth", 0.02),
                },
                AdaptiveExecutionLimits::new(4, 0.1, 1_000, 2),
            ),
            &enabled_gate(),
        )
        .await
        .unwrap_err();

    assert_eq!(error.code.as_ref(), "adaptive_provider_identity_mismatch");
    assert_eq!(judge.calls(), 0);
    assert_eq!(synthesizer.calls(), 0);
}

#[tokio::test]
async fn fusion_parallel_token_overrun_blocks_final_stages() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let overrun = Arc::new(ScriptedProvider::new(
        "overrun",
        vec![response_with_usage(
            "overrun",
            "too many tokens",
            0.01,
            5_000,
            5_000,
        )],
        order.clone(),
    ));
    let good = Arc::new(ScriptedProvider::new(
        "good",
        vec![response("good", "valid", 0.01)],
        order.clone(),
    ));
    let judge = Arc::new(ScriptedProvider::new(
        "judge",
        vec![response("judge", "unused", 0.01)],
        order.clone(),
    ));
    let synthesizer = Arc::new(ScriptedProvider::new(
        "synth",
        vec![response("synth", "unused", 0.01)],
        order,
    ));
    let execution = executor(
        vec![
            ("overrun", overrun),
            ("good", good),
            ("judge", judge.clone()),
            ("synth", synthesizer.clone()),
        ],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::from_flags(false),
    );

    let error = execution
        .execute(
            &request(
                AdaptiveExecutionPlan::Fusion {
                    panel: vec![endpoint("overrun", 0.02), endpoint("good", 0.02)],
                    judge: endpoint("judge", 0.02),
                    synthesizer: endpoint("synth", 0.02),
                },
                AdaptiveExecutionLimits::new(4, 0.1, 1_000, 2).with_max_total_tokens(4_096),
            ),
            &enabled_gate(),
        )
        .await
        .unwrap_err();

    assert_eq!(
        error.code.as_ref(),
        "adaptive_provider_token_over_reservation"
    );
    assert_eq!(judge.calls(), 0);
    assert_eq!(synthesizer.calls(), 0);
}

#[tokio::test]
async fn validation_rejects_call_cost_concurrency_and_missing_endpoint_before_calls() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let provider = Arc::new(ScriptedProvider::new(
        "primary",
        vec![response("primary", "ok", 0.01)],
        order,
    ));
    let execution = executor(
        vec![("primary", provider.clone())],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::from_flags(false),
    );

    let cases = [
        request(
            AdaptiveExecutionPlan::OrderedFallback {
                endpoints: vec![endpoint("primary", 0.01), endpoint("missing", 0.01)],
            },
            limits(1, 1.0, 1_000),
        ),
        request(
            AdaptiveExecutionPlan::Single {
                endpoint: endpoint("primary", 0.2),
            },
            limits(1, 0.1, 1_000),
        ),
        request(
            AdaptiveExecutionPlan::Single {
                endpoint: endpoint("missing", 0.01),
            },
            limits(1, 1.0, 1_000),
        ),
        AdaptiveExecutionRequest::new(
            "dispatch-af3",
            "solve the task",
            AdaptiveExecutionPlan::Single {
                endpoint: endpoint("primary", 0.01),
            },
            AdaptiveExecutionLimits::new(1, 1.0, 1_000, 2),
        ),
        AdaptiveExecutionRequest::new(
            "dispatch-af3",
            "solve the task",
            AdaptiveExecutionPlan::Fusion {
                panel: vec![endpoint("primary", 0.01), endpoint("missing", 0.01)],
                judge: endpoint("primary", 0.01),
                synthesizer: endpoint("primary", 0.01),
            },
            AdaptiveExecutionLimits::new(4, 1.0, 1_000, 4),
        ),
        AdaptiveExecutionRequest::new(
            "dispatch-af3",
            "solve the task",
            AdaptiveExecutionPlan::Fusion {
                panel: vec![endpoint("primary", 0.01), endpoint("missing", 0.01)],
                judge: endpoint("primary", 0.01),
                synthesizer: endpoint("primary", 0.01),
            },
            AdaptiveExecutionLimits::new(4, 1.0, 1_000, 2).with_min_successful_panel_calls(3),
        ),
    ];

    let expected = [
        "adaptive_call_limit_exceeded",
        "adaptive_cost_limit_exceeded",
        "adaptive_endpoint_not_found",
        "adaptive_concurrency_not_supported",
        "adaptive_concurrency_limit_invalid",
        "adaptive_panel_quorum_invalid",
    ];
    for (request, expected_code) in cases.iter().zip(expected) {
        let error = execution
            .execute(request, &enabled_gate())
            .await
            .unwrap_err();
        assert_eq!(error.code.as_ref(), expected_code);
    }

    assert_eq!(provider.calls(), 0);
}

#[tokio::test]
async fn endpoint_model_binding_rejects_plan_model_override_before_call() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let provider = Arc::new(ScriptedProvider::new(
        "primary",
        vec![response("primary", "unused", 0.01)],
        order,
    ));
    let execution = executor(
        vec![("primary", provider.clone())],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::from_flags(false),
    );
    let mut invocation = endpoint("primary", 0.02);
    invocation.model = "different-model".to_string();

    let error = execution
        .execute(
            &request(
                AdaptiveExecutionPlan::Single {
                    endpoint: invocation,
                },
                limits(1, 0.1, 1_000),
            ),
            &enabled_gate(),
        )
        .await
        .unwrap_err();

    assert_eq!(error.code.as_ref(), "adaptive_endpoint_model_mismatch");
    assert_eq!(provider.calls(), 0);
}

#[tokio::test]
async fn token_ceiling_blocks_call_when_prompt_and_output_reservation_do_not_fit() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let provider = Arc::new(ScriptedProvider::new(
        "primary",
        vec![response("primary", "unused", 0.01)],
        order,
    ));
    let execution = executor(
        vec![("primary", provider.clone())],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::from_flags(false),
    );

    let error = execution
        .execute(
            &request(
                AdaptiveExecutionPlan::Single {
                    endpoint: endpoint("primary", 0.02),
                },
                limits(1, 0.1, 1_000).with_max_total_tokens(1),
            ),
            &enabled_gate(),
        )
        .await
        .unwrap_err();

    assert_eq!(error.code.as_ref(), "adaptive_token_limit_exceeded");
    assert_eq!(provider.calls(), 0);
}

#[tokio::test]
async fn provider_token_over_reservation_is_recorded_and_stops_execution() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let provider = Arc::new(ScriptedProvider::new(
        "primary",
        vec![response("primary", "too many tokens", 0.01)],
        order,
    ));
    let execution = executor(
        vec![("primary", provider.clone())],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::from_flags(false),
    );

    let error = execution
        .execute(
            &request(
                AdaptiveExecutionPlan::Single {
                    endpoint: endpoint("primary", 0.02),
                },
                limits(1, 0.1, 1_000).with_max_total_tokens(5),
            ),
            &enabled_gate(),
        )
        .await
        .unwrap_err();

    assert_eq!(
        error.code.as_ref(),
        "adaptive_provider_token_over_reservation"
    );
    assert_eq!(error.total_input_token_count, 10);
    assert_eq!(error.total_output_token_count, 5);
    assert_eq!(provider.calls(), 1);
}

#[tokio::test]
async fn provider_response_identity_mismatch_is_rejected_with_usage_evidence() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let provider = Arc::new(ScriptedProvider::new(
        "primary",
        vec![Ok(ProviderResponse {
            schema_version: "provider_response.v1".to_string(),
            provider_id: "different-provider".to_string(),
            model: "different-model".to_string(),
            output: "untrusted".to_string(),
            input_tokens: Some(10),
            output_tokens: Some(5),
            estimated_cost: Some(0.01),
            provider_request_id: None,
        })],
        order,
    ));
    let execution = executor(
        vec![("primary", provider.clone())],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::from_flags(false),
    );

    let error = execution
        .execute(
            &request(
                AdaptiveExecutionPlan::Single {
                    endpoint: endpoint("primary", 0.02),
                },
                limits(1, 0.1, 1_000),
            ),
            &enabled_gate(),
        )
        .await
        .unwrap_err();

    assert_eq!(error.code.as_ref(), "adaptive_provider_identity_mismatch");
    assert_eq!(error.total_provider_cost_usd, 0.01);
    assert_eq!(error.total_input_token_count, 10);
    assert_eq!(error.total_output_token_count, 5);
    assert_eq!(provider.calls(), 1);
}

#[tokio::test]
async fn ordered_fallback_skips_disabled_endpoint_and_uses_next_endpoint() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let disabled = Arc::new(
        ScriptedProvider::new(
            "disabled",
            vec![response("disabled", "unused", 0.01)],
            order.clone(),
        )
        .disabled(),
    );
    let second = Arc::new(ScriptedProvider::new(
        "second",
        vec![response("second", "available", 0.01)],
        order,
    ));
    let execution = executor(
        vec![("disabled", disabled.clone()), ("second", second.clone())],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::from_flags(false),
    );

    let result = execution
        .execute(
            &request(
                AdaptiveExecutionPlan::OrderedFallback {
                    endpoints: vec![endpoint("disabled", 0.02), endpoint("second", 0.02)],
                },
                limits(2, 0.1, 1_000),
            ),
            &enabled_gate(),
        )
        .await
        .unwrap();

    assert_eq!(result.selected_endpoint_id.as_deref(), Some("second"));
    assert_eq!(result.calls.len(), 2);
    assert_eq!(result.calls[0].status, "disabled");
    assert_eq!(disabled.calls(), 0);
    assert_eq!(second.calls(), 1);
}

#[tokio::test]
async fn kill_switch_blocks_execution_and_stops_between_calls() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let first = Arc::new(ScriptedProvider::new(
        "first",
        vec![Err(provider_error("first", "provider_capacity"))],
        order.clone(),
    ));
    let second = Arc::new(ScriptedProvider::new(
        "second",
        vec![response("second", "unused", 0.01)],
        order,
    ));
    let kill_switch = AdaptiveExecutionKillSwitch::from_flags(false);
    kill_switch.kill();
    let execution = executor(
        vec![("first", first.clone()), ("second", second.clone())],
        Arc::new(ProviderAuditRecorder::new()),
        kill_switch,
    );

    let error = execution
        .execute(
            &request(
                AdaptiveExecutionPlan::OrderedFallback {
                    endpoints: vec![endpoint("first", 0.02), endpoint("second", 0.02)],
                },
                limits(2, 0.1, 1_000),
            ),
            &enabled_gate(),
        )
        .await
        .unwrap_err();

    assert_eq!(error.code.as_ref(), "adaptive_execution_killed");
    assert_eq!(first.calls(), 0);
    assert_eq!(second.calls(), 0);
}

#[tokio::test]
async fn total_timeout_cancels_current_call_and_prevents_fallback() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let slow = Arc::new(
        ScriptedProvider::new("slow", vec![response("slow", "late", 0.01)], order.clone())
            .with_delay(500),
    );
    let fallback = Arc::new(ScriptedProvider::new(
        "fallback",
        vec![response("fallback", "unused", 0.01)],
        order,
    ));
    let execution = executor(
        vec![("slow", slow.clone()), ("fallback", fallback.clone())],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::from_flags(false),
    );

    let error = execution
        .execute(
            &request(
                AdaptiveExecutionPlan::OrderedFallback {
                    endpoints: vec![endpoint("slow", 0.02), endpoint("fallback", 0.02)],
                },
                limits(2, 0.1, 100),
            ),
            &enabled_gate(),
        )
        .await
        .unwrap_err();

    assert_eq!(error.code.as_ref(), "adaptive_execution_timeout");
    assert_eq!(slow.calls(), 1);
    assert_eq!(fallback.calls(), 0);
}

#[tokio::test]
async fn provider_cost_over_reservation_stops_later_calls() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let first = Arc::new(ScriptedProvider::new(
        "first",
        vec![response("first", "unexpectedly expensive", 0.08)],
        order.clone(),
    ));
    let second = Arc::new(ScriptedProvider::new(
        "second",
        vec![response("second", "unused", 0.01)],
        order,
    ));
    let execution = executor(
        vec![("first", first.clone()), ("second", second.clone())],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::from_flags(false),
    );

    let error = execution
        .execute(
            &request(
                AdaptiveExecutionPlan::Fusion {
                    panel: vec![endpoint("first", 0.02), endpoint("second", 0.02)],
                    judge: endpoint("first", 0.02),
                    synthesizer: endpoint("second", 0.02),
                },
                limits(4, 0.1, 1_000),
            ),
            &enabled_gate(),
        )
        .await
        .unwrap_err();

    assert_eq!(
        error.code.as_ref(),
        "adaptive_provider_cost_over_reservation"
    );
    assert_eq!(error.total_provider_cost_usd, 0.08);
    assert_eq!(first.calls(), 1);
    assert_eq!(second.calls(), 0);
    assert_eq!(
        error.calls.iter().map(|call| call.role).collect::<Vec<_>>(),
        vec![engine::provider::adaptive_execution::AdaptiveCallRole::Panel]
    );
}

#[tokio::test]
async fn outputs_are_redacted_and_capped_before_reuse_or_return() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let secret_output = format!("token sk-abcdefghijklmnopqrstuvwxyz {}", "x".repeat(80_000));
    let provider = Arc::new(ScriptedProvider::new(
        "primary",
        vec![response("primary", &secret_output, 0.01)],
        order,
    ));
    let execution = executor(
        vec![("primary", provider)],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::from_flags(false),
    );

    let result = execution
        .execute(
            &request(
                AdaptiveExecutionPlan::Single {
                    endpoint: endpoint("primary", 0.02),
                },
                limits(1, 0.1, 1_000),
            ),
            &enabled_gate(),
        )
        .await
        .unwrap();
    let output = result.output.unwrap();

    assert!(!output.contains("sk-abcdefghijklmnopqrstuvwxyz"));
    assert!(output.contains("***"));
    assert!(output.len() <= 65_536);
    assert!(result.output_truncated);
}

#[tokio::test]
async fn secret_shaped_endpoint_or_model_is_rejected_before_audit_or_call() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let provider = Arc::new(ScriptedProvider::new(
        "primary",
        vec![response("primary", "ok", 0.01)],
        order,
    ));
    let audit = Arc::new(ProviderAuditRecorder::new());
    let execution = executor(
        vec![("primary", provider.clone())],
        audit.clone(),
        AdaptiveExecutionKillSwitch::from_flags(false),
    );
    let mut secret_endpoint = endpoint("primary", 0.02);
    secret_endpoint.endpoint_id = "sk-abcdefghijklmnopqrstuvwxyz".to_string();
    let mut secret_model = endpoint("primary", 0.02);
    secret_model.model = "token=abcdefghijklmnop".to_string();

    for endpoint in [secret_endpoint, secret_model] {
        let error = execution
            .execute(
                &request(
                    AdaptiveExecutionPlan::Single { endpoint },
                    limits(1, 0.1, 1_000),
                ),
                &enabled_gate(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code.as_ref(), "adaptive_endpoint_invalid");
    }

    assert_eq!(provider.calls(), 0);
    let serialized = serde_json::to_string(&audit.list_all()).unwrap();
    assert_eq!(audit.count(), 2);
    assert!(!serialized.contains("sk-abcdefghijklmnopqrstuvwxyz"));
    assert!(!serialized.contains("abcdefghijklmnop"));
}

#[tokio::test]
async fn kill_switch_is_checked_between_fallback_calls() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let kill_switch = AdaptiveExecutionKillSwitch::from_flags(false);
    let first = Arc::new(
        ScriptedProvider::new(
            "first",
            vec![Err(provider_error("first", "provider_capacity"))],
            order.clone(),
        )
        .with_kill_on_call(kill_switch.clone()),
    );
    let second = Arc::new(ScriptedProvider::new(
        "second",
        vec![response("second", "unused", 0.01)],
        order,
    ));
    let execution = executor(
        vec![("first", first.clone()), ("second", second.clone())],
        Arc::new(ProviderAuditRecorder::new()),
        kill_switch,
    );

    let error = execution
        .execute(
            &request(
                AdaptiveExecutionPlan::OrderedFallback {
                    endpoints: vec![endpoint("first", 0.02), endpoint("second", 0.02)],
                },
                limits(2, 0.1, 1_000),
            ),
            &enabled_gate(),
        )
        .await
        .unwrap_err();

    assert_eq!(error.code.as_ref(), "adaptive_execution_killed");
    assert_eq!(error.calls.len(), 1);
    assert_eq!(first.calls(), 1);
    assert_eq!(second.calls(), 0);
}

#[tokio::test]
async fn fallback_exhaustion_returns_composite_error_with_all_call_evidence() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let first = Arc::new(ScriptedProvider::new(
        "first",
        vec![Err(provider_error("first", "provider_capacity"))],
        order.clone(),
    ));
    let second = Arc::new(ScriptedProvider::new(
        "second",
        vec![Err(provider_error("second", "provider_auth"))],
        order,
    ));
    let execution = executor(
        vec![("first", first), ("second", second)],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::from_flags(false),
    );

    let error = execution
        .execute(
            &request(
                AdaptiveExecutionPlan::OrderedFallback {
                    endpoints: vec![endpoint("first", 0.02), endpoint("second", 0.02)],
                },
                limits(2, 0.1, 1_000),
            ),
            &enabled_gate(),
        )
        .await
        .unwrap_err();

    assert_eq!(error.code.as_ref(), "adaptive_fallback_exhausted");
    assert_eq!(error.calls.len(), 2);
    assert_eq!(
        error.calls[0].error_domain.as_deref(),
        Some("provider_capacity")
    );
    assert_eq!(
        error.calls[1].error_domain.as_deref(),
        Some("provider_auth")
    );
}

#[tokio::test]
async fn disabled_provider_is_rejected_without_invocation() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let provider = Arc::new(
        ScriptedProvider::new("primary", vec![response("primary", "unused", 0.01)], order)
            .disabled(),
    );
    let execution = executor(
        vec![("primary", provider.clone())],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::from_flags(false),
    );

    let error = execution
        .execute(
            &request(
                AdaptiveExecutionPlan::Single {
                    endpoint: endpoint("primary", 0.02),
                },
                limits(1, 0.1, 1_000),
            ),
            &enabled_gate(),
        )
        .await
        .unwrap_err();

    assert_eq!(error.code.as_ref(), "adaptive_provider_disabled");
    assert_eq!(provider.calls(), 0);
}

#[tokio::test]
async fn audit_records_never_include_raw_prompt_or_provider_output() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let provider = Arc::new(ScriptedProvider::new(
        "primary",
        vec![response("primary", "private provider output", 0.01)],
        order,
    ));
    let audit = Arc::new(ProviderAuditRecorder::new());
    let execution = executor(
        vec![("primary", provider)],
        audit.clone(),
        AdaptiveExecutionKillSwitch::from_flags(false),
    );
    let request = AdaptiveExecutionRequest::new(
        "dispatch-af3",
        "private task prompt",
        AdaptiveExecutionPlan::Single {
            endpoint: endpoint("primary", 0.02),
        },
        limits(1, 0.1, 1_000),
    );

    execution.execute(&request, &enabled_gate()).await.unwrap();
    let serialized = serde_json::to_string(&audit.list_all()).unwrap();

    assert!(!serialized.contains("private task prompt"));
    assert!(!serialized.contains("private provider output"));
}

#[test]
fn adaptive_node_executor_reads_explicit_plan_and_returns_workflow_output() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let provider = Arc::new(ScriptedProvider::new(
        "primary",
        vec![response("primary", "node answer", 0.01)],
        order,
    ));
    let execution = Arc::new(executor(
        vec![("primary", provider.clone())],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::from_flags(false),
    ));
    let node_executor = AdaptiveProviderNodeExecutor::new(execution, enabled_gate());
    let input = NodeExecutionInput {
        node_id: "node-1".to_string(),
        task_type: "generate".to_string(),
        run_id: "run-1".to_string(),
        workflow_id: "workflow-1".to_string(),
        node_metadata: json!({
            "prompt": "solve from node metadata",
            "adaptive_execution": {
                "observation_context": {
                    "request_id": "request-1",
                    "task_class": "coding",
                    "objective": "quality",
                    "risk_level": "low",
                    "candidate_id": "candidate-primary",
                    "policy_hash": null
                },
                "plan": {
                    "mode": "single",
                    "endpoint": {
                        "endpoint_id": "primary",
                        "model": "test-model",
                        "reserved_cost_usd": 0.02
                    }
                },
                "limits": {
                    "max_calls": 1,
                    "max_cost_usd": 0.1,
                    "max_elapsed_ms": 1000,
                    "max_concurrency": 1
                }
            }
        }),
    };

    let output = node_executor.execute_node(&input);

    assert_eq!(node_executor.executor_type_name(), "adaptive_provider");
    assert_eq!(output.status, "completed");
    assert_eq!(output.executor_type, "adaptive_provider");
    assert_eq!(output.output.as_deref(), Some("node answer"));
    assert_eq!(output.estimated_cost, Some(0.01));
    assert_eq!(provider.calls(), 1);
    let observation = node_executor.take_observation().unwrap();
    assert_eq!(observation.run_id, "run-1");
    assert_eq!(observation.candidate_id, "candidate-primary");
    assert_eq!(observation.candidate_kind, "single");
    assert!(observation.success);
    let serialized = serde_json::to_string(&observation).unwrap();
    assert!(!serialized.contains("solve from node metadata"));
    assert!(!serialized.contains("node answer"));
}

#[test]
fn adaptive_node_executor_rejects_missing_plan_without_provider_call() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let provider = Arc::new(ScriptedProvider::new(
        "primary",
        vec![response("primary", "unused", 0.01)],
        order,
    ));
    let execution = Arc::new(executor(
        vec![("primary", provider.clone())],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::from_flags(false),
    ));
    let node_executor = AdaptiveProviderNodeExecutor::new(execution, enabled_gate());
    let input = NodeExecutionInput {
        node_id: "node-1".to_string(),
        task_type: "generate".to_string(),
        run_id: "run-1".to_string(),
        workflow_id: "workflow-1".to_string(),
        node_metadata: json!({"prompt": "solve"}),
    };

    let output = node_executor.execute_node(&input);

    assert_eq!(output.status, "failed");
    assert_eq!(
        output.error_domain.as_deref(),
        Some("adaptive_plan_missing")
    );
    assert_eq!(provider.calls(), 0);
}

#[test]
fn adaptive_node_executor_applies_existing_global_cost_gate() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let provider = Arc::new(ScriptedProvider::new(
        "primary",
        vec![response("primary", "unused", 0.01)],
        order,
    ));
    let audit = Arc::new(ProviderAuditRecorder::new());
    let execution = Arc::new(executor(
        vec![("primary", provider.clone())],
        audit.clone(),
        AdaptiveExecutionKillSwitch::from_flags(false),
    ));
    let node_executor = AdaptiveProviderNodeExecutor::new(execution, enabled_gate())
        .with_cost_gate(CostGateConfig::new(Some(0.01), None), 0.0);
    let input = NodeExecutionInput {
        node_id: "node-1".to_string(),
        task_type: "generate".to_string(),
        run_id: "run-1".to_string(),
        workflow_id: "workflow-1".to_string(),
        node_metadata: json!({
            "prompt": "solve",
            "adaptive_execution": {
                "plan": {
                    "mode": "single",
                    "endpoint": {
                        "endpoint_id": "primary",
                        "model": "test-model",
                        "reserved_cost_usd": 0.02
                    }
                },
                "limits": {
                    "max_calls": 1,
                    "max_cost_usd": 0.1,
                    "max_elapsed_ms": 1000,
                    "max_concurrency": 1
                }
            }
        }),
    };

    let output = node_executor.execute_node(&input);

    assert_eq!(output.status, "failed");
    assert_eq!(
        output.error_domain.as_deref(),
        Some("adaptive_global_cost_gate_blocked")
    );
    assert_eq!(provider.calls(), 0);
    let events = audit.list_events("workflow:run-1:node-1");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "adaptive_execution_blocked");
    assert_eq!(
        events[0].error_domain.as_deref(),
        Some("adaptive_global_cost_gate_blocked")
    );
}

#[test]
fn adaptive_environment_kill_switch_blocks_node_execution() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    std::env::set_var("ACP_ADAPTIVE_FUSION_KILL_SWITCH", "1");
    let order = Arc::new(Mutex::new(Vec::new()));
    let provider = Arc::new(ScriptedProvider::new(
        "primary",
        vec![response("primary", "unused", 0.01)],
        order,
    ));
    let execution = Arc::new(executor(
        vec![("primary", provider.clone())],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::new(),
    ));
    let node_executor = AdaptiveProviderNodeExecutor::new(execution, enabled_gate());
    let input = NodeExecutionInput {
        node_id: "node-1".to_string(),
        task_type: "generate".to_string(),
        run_id: "run-1".to_string(),
        workflow_id: "workflow-1".to_string(),
        node_metadata: json!({
            "prompt": "solve",
            "adaptive_execution": {
                "plan": {
                    "mode": "single",
                    "endpoint": {
                        "endpoint_id": "primary",
                        "model": "test-model",
                        "reserved_cost_usd": 0.02
                    }
                },
                "limits": {
                    "max_calls": 1,
                    "max_cost_usd": 0.1,
                    "max_elapsed_ms": 1000,
                    "max_concurrency": 1
                }
            }
        }),
    };

    let output = node_executor.execute_node(&input);
    std::env::remove_var("ACP_ADAPTIVE_FUSION_KILL_SWITCH");

    assert_eq!(output.status, "failed");
    assert_eq!(
        output.error_domain.as_deref(),
        Some("adaptive_execution_killed")
    );
    assert_eq!(provider.calls(), 0);
}

#[test]
fn adaptive_node_failure_uses_reserved_cost_when_provider_cost_is_unknown() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let provider = Arc::new(ScriptedProvider::new(
        "primary",
        vec![Err(provider_error("primary", "provider_capacity"))],
        order,
    ));
    let execution = Arc::new(executor(
        vec![("primary", provider)],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::from_flags(false),
    ));
    let node_executor = AdaptiveProviderNodeExecutor::new(execution, enabled_gate());
    let input = NodeExecutionInput {
        node_id: "node-1".to_string(),
        task_type: "generate".to_string(),
        run_id: "run-1".to_string(),
        workflow_id: "workflow-1".to_string(),
        node_metadata: json!({
            "prompt": "solve",
            "adaptive_execution": {
                "plan": {
                    "mode": "single",
                    "endpoint": {
                        "endpoint_id": "primary",
                        "model": "test-model",
                        "reserved_cost_usd": 0.04
                    }
                },
                "limits": {
                    "max_calls": 1,
                    "max_cost_usd": 0.1,
                    "max_elapsed_ms": 1000,
                    "max_concurrency": 1
                }
            }
        }),
    };

    let output = node_executor.execute_node(&input);

    assert_eq!(output.status, "failed");
    assert_eq!(output.estimated_cost, Some(0.04));
}

#[test]
fn adaptive_policy_node_requires_promoted_policy_before_provider_call() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let provider = Arc::new(ScriptedProvider::new(
        "strong",
        vec![response("strong", "unused", 0.01)],
        order,
    ));
    let execution = Arc::new(executor(
        vec![("strong", provider.clone())],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::from_flags(false),
    ));
    let node_executor = AdaptiveProviderNodeExecutor::new(execution, enabled_gate());
    let input = NodeExecutionInput {
        node_id: "node-1".to_string(),
        task_type: "generate".to_string(),
        run_id: "run-1".to_string(),
        workflow_id: "workflow-1".to_string(),
        node_metadata: json!({
            "prompt": "solve",
            "adaptive_policy_execution": {
                "request": {
                    "schema_version": CONTEXTUAL_POLICY_SCHEMA_VERSION,
                    "request_id": "request-1",
                    "task_class": "coding",
                    "objective": "quality",
                    "risk_level": "low",
                    "exploration_seed": 0
                },
                "evaluation": {
                    "task_class": "coding",
                    "candidates": [contextual_candidate("strong", 0.9, 0.08)],
                    "pareto_candidate_ids": ["strong"],
                    "recommendations": []
                },
                "observations": [],
                "candidate_plans": {
                    "strong": {
                        "plan": {
                            "mode": "single",
                            "endpoint": {
                                "endpoint_id": "strong",
                                "model": "test-model",
                                "reserved_cost_usd": 0.02
                            }
                        },
                        "limits": {
                            "max_calls": 1,
                            "max_cost_usd": 0.1,
                            "max_elapsed_ms": 1000,
                            "max_concurrency": 1
                        }
                    }
                }
            }
        }),
    };

    let output = node_executor.execute_node(&input);

    assert_eq!(output.status, "failed");
    assert_eq!(
        output.error_domain.as_deref(),
        Some("adaptive_policy_not_promoted")
    );
    assert_eq!(provider.calls(), 0);
}

#[test]
fn adaptive_policy_node_selects_promoted_candidate_with_explicit_bounded_plan() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let cheap = Arc::new(ScriptedProvider::new(
        "cheap",
        vec![response("cheap", "cheap answer", 0.001)],
        order.clone(),
    ));
    let strong = Arc::new(ScriptedProvider::new(
        "strong",
        vec![response("strong", "strong answer", 0.01)],
        order.clone(),
    ));
    let execution = Arc::new(executor(
        vec![("cheap", cheap.clone()), ("strong", strong.clone())],
        Arc::new(ProviderAuditRecorder::new()),
        AdaptiveExecutionKillSwitch::from_flags(false),
    ));
    let node_executor = AdaptiveProviderNodeExecutor::new(execution, enabled_gate())
        .with_contextual_policies(
            vec![contextual_policy()],
            AdaptiveExplorationGate::from_flags(false, false, false, 0.05),
        );
    let input = NodeExecutionInput {
        node_id: "node-1".to_string(),
        task_type: "generate".to_string(),
        run_id: "run-1".to_string(),
        workflow_id: "workflow-1".to_string(),
        node_metadata: json!({
            "prompt": "solve",
            "adaptive_policy_execution": {
                "request": {
                    "schema_version": CONTEXTUAL_POLICY_SCHEMA_VERSION,
                    "request_id": "request-1",
                    "task_class": "coding",
                    "objective": "quality",
                    "risk_level": "low",
                    "exploration_seed": 0
                },
                "evaluation": {
                    "task_class": "coding",
                    "candidates": [
                        contextual_candidate("cheap", 0.76, 0.01),
                        contextual_candidate("strong", 0.92, 0.08)
                    ],
                    "pareto_candidate_ids": ["cheap", "strong"],
                    "recommendations": []
                },
                "observations": [],
                "candidate_plans": {
                    "cheap": {
                        "plan": {
                            "mode": "single",
                            "endpoint": {
                                "endpoint_id": "cheap",
                                "model": "test-model",
                                "reserved_cost_usd": 0.01
                            }
                        },
                        "limits": {
                            "max_calls": 1,
                            "max_cost_usd": 0.1,
                            "max_elapsed_ms": 1000,
                            "max_concurrency": 1
                        }
                    },
                    "strong": {
                        "plan": {
                            "mode": "single",
                            "endpoint": {
                                "endpoint_id": "strong",
                                "model": "test-model",
                                "reserved_cost_usd": 0.02
                            }
                        },
                        "limits": {
                            "max_calls": 1,
                            "max_cost_usd": 0.1,
                            "max_elapsed_ms": 1000,
                            "max_concurrency": 1
                        }
                    }
                }
            }
        }),
    };

    let output = node_executor.execute_node(&input);

    assert_eq!(output.status, "completed");
    assert_eq!(output.output.as_deref(), Some("strong answer"));
    assert_eq!(cheap.calls(), 0);
    assert_eq!(strong.calls(), 1);
    assert_eq!(order.lock().unwrap().as_slice(), ["strong"]);
}
