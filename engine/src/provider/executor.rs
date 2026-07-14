use std::sync::Arc;

use super::audit::ProviderAuditRecorder;
use super::config::{provider_pricing_from_env, RetryPolicy};
use super::cost_gate::{check_cost_gates, CostGateConfig};
use super::redaction::redact_sensitive_patterns;
use super::retry::{compute_delay_ms, should_retry};
use super::{Provider, ProviderError, ProviderRequest};
use crate::dispatch_decision::DispatchDecision;
use crate::executor_adapter::{ExecutionResult, Executor};
use crate::node_executor::{NodeExecutionInput, NodeExecutionOutput, NodeExecutor};
use crate::runtime::FixtureRuntime;

pub struct ProviderExecutor {
    provider: Arc<dyn Provider>,
    audit_recorder: Option<Arc<ProviderAuditRecorder>>,
}

impl ProviderExecutor {
    pub fn new(provider: Arc<dyn Provider>) -> Self {
        Self {
            provider,
            audit_recorder: None,
        }
    }

    pub fn with_audit_recorder(mut self, recorder: Arc<ProviderAuditRecorder>) -> Self {
        self.audit_recorder = Some(recorder);
        self
    }
}

impl Executor for ProviderExecutor {
    fn execute(
        &self,
        decision: &DispatchDecision,
        raw_request: &str,
        dispatch_id: &str,
        runtime: &mut FixtureRuntime,
    ) -> ExecutionResult {
        let request = ProviderRequest {
            schema_version: "provider_request.v1".to_string(),
            provider_id: self.provider.provider_id().to_string(),
            model: decision
                .analysis_snapshot
                .get("selected_model")
                .and_then(|v| v.as_str())
                .or_else(|| self.provider.default_model())
                .unwrap_or("unknown")
                .to_string(),
            prompt: raw_request.to_string(),
            metadata: serde_json::json!({"dispatch_id": dispatch_id}),
        };

        let provider = self.provider.clone();
        let response = invoke_provider_blocking(provider, &request);

        match response {
            Ok(resp) => {
                if let Some(recorder) = &self.audit_recorder {
                    let extra = serde_json::json!({
                        "input_token_count": resp.input_tokens,
                        "output_token_count": resp.output_tokens,
                        "cost": resp.estimated_cost,
                    });
                    recorder.create_and_record(
                        dispatch_id,
                        self.provider.provider_id(),
                        "response_received",
                        Some(&extra),
                    );
                }
                ExecutionResult {
                    schema_version: "execution_result.v1".to_string(),
                    result_id: runtime.id("exec-"),
                    dispatch_id: dispatch_id.to_string(),
                    decision_id: decision.decision_id.clone(),
                    executor_type: "provider".to_string(),
                    status: "provider_completed".to_string(),
                    output: Some(resp.output),
                    prompt_pack: None,
                    input_tokens: resp.input_tokens,
                    output_tokens: resp.output_tokens,
                    estimated_cost: resp.estimated_cost,
                    latency_ms: None,
                    error_domain: None,
                    error_message: None,
                    provider_request_id: resp.provider_request_id,
                    attempt_number: None,
                    finish_reason: None,
                    usage_source: Some("provider_reported".to_string()),
                    created_at: runtime.now(),
                }
            }
            Err(ProviderError {
                error_domain,
                message,
                ..
            }) => {
                if let Some(recorder) = &self.audit_recorder {
                    let extra = serde_json::json!({
                        "error_domain": error_domain,
                    });
                    recorder.create_and_record(
                        dispatch_id,
                        self.provider.provider_id(),
                        "error",
                        Some(&extra),
                    );
                }
                ExecutionResult {
                    schema_version: "execution_result.v1".to_string(),
                    result_id: runtime.id("exec-"),
                    dispatch_id: dispatch_id.to_string(),
                    decision_id: decision.decision_id.clone(),
                    executor_type: "provider".to_string(),
                    status: "failed".to_string(),
                    output: None,
                    prompt_pack: None,
                    input_tokens: None,
                    output_tokens: None,
                    estimated_cost: None,
                    latency_ms: None,
                    error_domain: Some(error_domain),
                    error_message: Some(message),
                    provider_request_id: None,
                    attempt_number: None,
                    finish_reason: None,
                    usage_source: None,
                    created_at: runtime.now(),
                }
            }
        }
    }
}

pub struct ProviderNodeExecutor {
    provider: Arc<dyn Provider>,
    audit_recorder: Option<Arc<ProviderAuditRecorder>>,
    cost_gate_config: CostGateConfig,
    daily_cost_usd: f64,
    max_retries: i64,
}

impl ProviderNodeExecutor {
    pub fn new(provider: Arc<dyn Provider>) -> Self {
        Self {
            provider,
            audit_recorder: None,
            cost_gate_config: CostGateConfig::new(None, None),
            daily_cost_usd: 0.0,
            max_retries: 0,
        }
    }

    pub fn with_audit_recorder(mut self, recorder: Arc<ProviderAuditRecorder>) -> Self {
        self.audit_recorder = Some(recorder);
        self
    }

    pub fn with_cost_gate(mut self, config: CostGateConfig, daily_cost_usd: f64) -> Self {
        self.cost_gate_config = config;
        self.daily_cost_usd = daily_cost_usd;
        self
    }

    pub fn with_max_retries(mut self, max_retries: i64) -> Self {
        self.max_retries = max_retries.clamp(0, 10);
        self
    }

    fn prompt(input: &NodeExecutionInput) -> String {
        input
            .node_metadata
            .get("prompt")
            .or_else(|| input.node_metadata.get("command"))
            .and_then(|v| v.as_str())
            .unwrap_or("echo noop")
            .to_string()
    }

    fn model(&self, input: &NodeExecutionInput) -> String {
        input
            .node_metadata
            .get("model")
            .and_then(|v| v.as_str())
            .or_else(|| self.provider.default_model())
            .unwrap_or("default")
            .to_string()
    }

    fn dispatch_ref(input: &NodeExecutionInput) -> String {
        format!("workflow:{}:{}", input.run_id, input.node_id)
    }

    fn reserved_cost_usd(input: &NodeExecutionInput, prompt: &str) -> f64 {
        if let Some(v) = input
            .node_metadata
            .get("reserved_cost_usd")
            .and_then(|v| v.as_f64())
            .filter(|v| v.is_finite() && *v >= 0.0)
        {
            return v;
        }
        let pricing = provider_pricing_from_env();
        if !pricing.configured() {
            return 0.0;
        }
        let input_tokens = (prompt.len() as f64 / 4.0).ceil().max(1.0);
        let output_tokens = std::env::var("ACP_PROVIDER_OUTPUT_TOKEN_RESERVE")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(1024.0);
        (input_tokens / 1000.0 * pricing.input_cost_per_1k.unwrap_or(0.0))
            + (output_tokens / 1000.0 * pricing.output_cost_per_1k.unwrap_or(0.0))
    }

    fn audit(&self, dispatch_id: &str, event_type: &str, extra: serde_json::Value) {
        if let Some(recorder) = &self.audit_recorder {
            recorder.create_and_record(
                dispatch_id,
                self.provider.provider_id(),
                event_type,
                Some(&extra),
            );
        }
    }
}

impl NodeExecutor for ProviderNodeExecutor {
    fn executor_type_name(&self) -> &str {
        "provider"
    }

    fn execute_node(&self, input: &NodeExecutionInput) -> NodeExecutionOutput {
        let start = std::time::Instant::now();
        if !self.provider.is_enabled() {
            return NodeExecutionOutput {
                status: "failed".to_string(),
                executor_type: "provider".to_string(),
                output: None,
                error_domain: Some("provider_disabled".to_string()),
                error_message: Some("provider is not enabled".to_string()),
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(start.elapsed().as_millis() as i64),
            };
        }

        let prompt = Self::prompt(input);
        let model = self.model(input);
        let dispatch_ref = Self::dispatch_ref(input);
        let reserved_cost = Self::reserved_cost_usd(input, &prompt);

        if let Err(block) =
            check_cost_gates(&self.cost_gate_config, reserved_cost, self.daily_cost_usd)
        {
            let message = block.to_string();
            self.audit(
                &dispatch_ref,
                "error",
                serde_json::json!({"error_domain": "provider_cost_gate_blocked"}),
            );
            return NodeExecutionOutput {
                status: "failed".to_string(),
                executor_type: "provider".to_string(),
                output: None,
                error_domain: Some("provider_cost_gate_blocked".to_string()),
                error_message: Some(message),
                input_tokens: None,
                output_tokens: None,
                estimated_cost: Some(reserved_cost),
                latency_ms: Some(start.elapsed().as_millis() as i64),
            };
        }

        let request = ProviderRequest {
            schema_version: "provider_request.v1".to_string(),
            provider_id: self.provider.provider_id().to_string(),
            model,
            prompt,
            metadata: serde_json::json!({
                "run_id": input.run_id,
                "node_id": input.node_id,
                "workflow_id": input.workflow_id,
                "reserved_cost_usd": reserved_cost,
            }),
        };
        self.audit(
            &dispatch_ref,
            "request_sent",
            serde_json::json!({"cost": reserved_cost, "currency": "USD"}),
        );

        let mut policy = RetryPolicy::new("workflow-provider-node");
        policy.max_retries = self.max_retries;
        let mut attempt = 0;
        loop {
            match invoke_provider_blocking(self.provider.clone(), &request) {
                Ok(resp) => {
                    self.audit(
                        &dispatch_ref,
                        "response_received",
                        serde_json::json!({
                            "input_token_count": resp.input_tokens,
                            "output_token_count": resp.output_tokens,
                            "cost": resp.estimated_cost,
                            "currency": "USD",
                            "latency_ms": start.elapsed().as_millis() as i64,
                        }),
                    );
                    return NodeExecutionOutput {
                        status: "completed".to_string(),
                        executor_type: "provider".to_string(),
                        output: Some(redact_sensitive_patterns(&resp.output)),
                        error_domain: None,
                        error_message: None,
                        input_tokens: resp.input_tokens,
                        output_tokens: resp.output_tokens,
                        estimated_cost: resp.estimated_cost.or(Some(reserved_cost)),
                        latency_ms: Some(start.elapsed().as_millis() as i64),
                    };
                }
                Err(err) if should_retry(&err, &policy, attempt) => {
                    if policy.budget_check_per_retry
                        && check_cost_gates(
                            &self.cost_gate_config,
                            reserved_cost,
                            self.daily_cost_usd,
                        )
                        .is_err()
                    {
                        self.audit(
                            &dispatch_ref,
                            "error",
                            serde_json::json!({"error_domain": "budget_exhausted"}),
                        );
                        return NodeExecutionOutput {
                            status: "failed".to_string(),
                            executor_type: "provider".to_string(),
                            output: None,
                            error_domain: Some("budget_exhausted".to_string()),
                            error_message: Some("budget check failed before retry".to_string()),
                            input_tokens: None,
                            output_tokens: None,
                            estimated_cost: Some(reserved_cost),
                            latency_ms: Some(start.elapsed().as_millis() as i64),
                        };
                    }
                    self.audit(
                        &dispatch_ref,
                        "retry",
                        serde_json::json!({"error_domain": err.error_domain.clone()}),
                    );
                    std::thread::sleep(std::time::Duration::from_millis(compute_delay_ms(
                        &policy, attempt,
                    )
                        as u64));
                    attempt += 1;
                }
                Err(err) => {
                    self.audit(
                        &dispatch_ref,
                        "error",
                        serde_json::json!({"error_domain": err.error_domain.clone()}),
                    );
                    return NodeExecutionOutput {
                        status: "failed".to_string(),
                        executor_type: "provider".to_string(),
                        output: None,
                        error_domain: Some(err.error_domain),
                        error_message: Some(redact_sensitive_patterns(&err.message)),
                        input_tokens: None,
                        output_tokens: None,
                        estimated_cost: Some(reserved_cost),
                        latency_ms: Some(start.elapsed().as_millis() as i64),
                    };
                }
            }
        }
    }
}

pub(crate) fn invoke_provider_blocking(
    provider: Arc<dyn Provider>,
    request: &ProviderRequest,
) -> Result<super::ProviderResponse, ProviderError> {
    let request = request.clone();
    let provider_id = request.provider_id.clone();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("provider runtime");
        runtime.block_on(provider.invoke(&request))
    })
    .join()
    .unwrap_or_else(|_| {
        Err(ProviderError {
            schema_version: "provider_error.v1".to_string(),
            provider_id,
            error_domain: "provider_runtime".to_string(),
            message: "provider runtime thread panicked".to_string(),
            retryable: true,
        })
    })
}

pub fn make_not_executed_result(
    decision: &DispatchDecision,
    dispatch_id: &str,
    runtime: &mut FixtureRuntime,
    error_domain: &str,
    error_message: &str,
) -> ExecutionResult {
    ExecutionResult {
        schema_version: "execution_result.v1".to_string(),
        result_id: runtime.id("exec-"),
        dispatch_id: dispatch_id.to_string(),
        decision_id: decision.decision_id.clone(),
        executor_type: "provider".to_string(),
        status: "not_executed".to_string(),
        output: None,
        prompt_pack: None,
        input_tokens: None,
        output_tokens: None,
        estimated_cost: None,
        latency_ms: None,
        error_domain: Some(error_domain.to_string()),
        error_message: Some(error_message.to_string()),
        provider_request_id: None,
        attempt_number: None,
        finish_reason: None,
        usage_source: None,
        created_at: runtime.now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatch_decision::DispatchDecision;
    use crate::provider::stub::StubProvider;
    use crate::provider::{ProviderResponse, ProviderResult};

    struct ModelEchoProvider;

    struct SecretEchoProvider;

    #[async_trait::async_trait]
    impl Provider for ModelEchoProvider {
        fn provider_id(&self) -> &str {
            "model-echo"
        }

        fn is_enabled(&self) -> bool {
            true
        }

        fn default_model(&self) -> Option<&str> {
            Some("configured-model")
        }

        async fn invoke(&self, request: &ProviderRequest) -> ProviderResult {
            Ok(ProviderResponse {
                schema_version: "provider_response.v1".to_string(),
                provider_id: self.provider_id().to_string(),
                model: request.model.clone(),
                output: request.model.clone(),
                input_tokens: Some(1),
                output_tokens: Some(1),
                estimated_cost: Some(0.001),
                provider_request_id: None,
            })
        }
    }

    #[async_trait::async_trait]
    impl Provider for SecretEchoProvider {
        fn provider_id(&self) -> &str {
            "secret-echo"
        }

        fn is_enabled(&self) -> bool {
            true
        }

        async fn invoke(&self, request: &ProviderRequest) -> ProviderResult {
            Ok(ProviderResponse {
                schema_version: "provider_response.v1".to_string(),
                provider_id: self.provider_id().to_string(),
                model: request.model.clone(),
                output: "api_key=sk-abcdefghijklmnopqrstuvwxyz".to_string(),
                input_tokens: Some(10),
                output_tokens: Some(5),
                estimated_cost: Some(0.001),
                provider_request_id: None,
            })
        }
    }

    fn make_decision() -> DispatchDecision {
        DispatchDecision {
            decision_id: "dec-0001".to_string(),
            analysis_id: "ana-0001".to_string(),
            analysis_snapshot: serde_json::json!({"selected_model": "stub-model"}),
            selected_tier: "balanced_worker".to_string(),
            decision_status: "decided".to_string(),
            created_at: "2000-01-01T00:00:00+00:00".to_string(),
            ..DispatchDecision::default()
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn provider_executor_with_stub() {
        let provider = Arc::new(StubProvider::new("stub-test"));
        let executor = ProviderExecutor::new(provider);
        let decision = make_decision();
        let mut runtime = FixtureRuntime::new();

        let result = executor.execute(&decision, "hello world", "disp-0001", &mut runtime);

        assert_eq!(result.executor_type, "provider");
        assert_eq!(result.status, "provider_completed");
        assert!(result.output.is_some());
        let output = result.output.unwrap();
        assert!(output.starts_with("[stub:stub-test]"));
        assert!(result.input_tokens.is_some());
        assert!(result.output_tokens.is_some());
        assert!(result.estimated_cost.is_some());
        assert_eq!(result.usage_source, Some("provider_reported".to_string()));
        assert_eq!(result.dispatch_id, "disp-0001");
        assert_eq!(result.decision_id, "dec-0001");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn provider_executor_uses_provider_default_model_when_decision_has_none() {
        let provider = Arc::new(ModelEchoProvider);
        let executor = ProviderExecutor::new(provider);
        let mut decision = make_decision();
        decision.analysis_snapshot = serde_json::json!({});
        let mut runtime = FixtureRuntime::new();

        let result = executor.execute(&decision, "hello", "disp-model", &mut runtime);

        assert_eq!(result.status, "provider_completed");
        assert_eq!(result.output.as_deref(), Some("configured-model"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn provider_executor_disabled_provider() {
        let provider = Arc::new(crate::provider::DisabledProvider::new("disabled-test"));
        let executor = ProviderExecutor::new(provider);
        let decision = make_decision();
        let mut runtime = FixtureRuntime::new();

        let result = executor.execute(&decision, "hello", "disp-0002", &mut runtime);

        assert_eq!(result.executor_type, "provider");
        assert_eq!(result.status, "failed");
        assert_eq!(result.error_domain.as_deref(), Some("provider_disabled"));
        assert!(result.error_message.is_some());
    }

    #[test]
    fn make_not_executed_result_fields() {
        let decision = make_decision();
        let mut runtime = FixtureRuntime::new();
        let result = make_not_executed_result(
            &decision,
            "disp-0003",
            &mut runtime,
            "execution_not_authorized",
            "provider execution blocked by constraints",
        );
        assert_eq!(result.status, "not_executed");
        assert_eq!(result.executor_type, "provider");
        assert_eq!(
            result.error_domain.as_deref(),
            Some("execution_not_authorized")
        );
        assert!(result.output.is_none());
        assert!(result.input_tokens.is_none());
    }

    #[test]
    fn provider_node_executor_blocks_cost_gate_before_call() {
        let provider = Arc::new(StubProvider::new("stub-cost"));
        let executor = ProviderNodeExecutor::new(provider)
            .with_cost_gate(CostGateConfig::new(Some(0.01), None), 0.0);
        let input = NodeExecutionInput {
            node_id: "node-1".to_string(),
            task_type: "provider".to_string(),
            run_id: "run-1".to_string(),
            workflow_id: "wf-1".to_string(),
            node_metadata: serde_json::json!({
                "prompt": "hello",
                "reserved_cost_usd": 1.0
            }),
        };

        let output = executor.execute_node(&input);

        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("provider_cost_gate_blocked")
        );
    }

    #[test]
    fn provider_node_executor_redacts_secret_like_output_and_adds_trace() {
        let provider = Arc::new(SecretEchoProvider);
        let executor = ProviderNodeExecutor::new(provider);
        let input = NodeExecutionInput {
            node_id: "node-1".to_string(),
            task_type: "provider".to_string(),
            run_id: "run-1".to_string(),
            workflow_id: "wf-1".to_string(),
            node_metadata: serde_json::json!({"prompt": "hello"}),
        };

        let output = executor.execute_node(&input);
        let value = output.to_value();

        assert_eq!(output.status, "completed");
        assert!(!output
            .output
            .as_deref()
            .unwrap_or("")
            .contains("sk-abcdefghijklmnopqrstuvwxyz"));
        assert_eq!(value["trace"]["schema_version"], "execution_trace.v2");
        assert_eq!(value["trace"]["output_policy"], "redacted_and_capped");
    }
}
