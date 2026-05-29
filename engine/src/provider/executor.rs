use std::sync::Arc;

use super::audit::ProviderAuditRecorder;
use super::{Provider, ProviderError, ProviderRequest};
use crate::dispatch_decision::DispatchDecision;
use crate::executor_adapter::{ExecutionResult, Executor};
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
                .unwrap_or("unknown")
                .to_string(),
            prompt: raw_request.to_string(),
            metadata: serde_json::json!({"dispatch_id": dispatch_id}),
        };

        let provider = self.provider.clone();
        let response = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(provider.invoke(&request))
        });

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
}
