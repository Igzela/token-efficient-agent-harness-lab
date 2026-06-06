use std::collections::HashMap;

use crate::dispatch_decision::DispatchDecision;
use crate::executor_adapter::{ExecutionResult, Executor, NoopExecutor};
use crate::runtime::FixtureRuntime;

pub struct MultiExecutor {
    executors: HashMap<String, Box<dyn Executor>>,
    default_executor: Box<dyn Executor>,
}

impl MultiExecutor {
    pub fn new(executors: HashMap<String, Box<dyn Executor>>) -> Self {
        Self {
            executors,
            default_executor: Box::new(NoopExecutor),
        }
    }

    pub fn with_default(mut self, executor: Box<dyn Executor>) -> Self {
        self.default_executor = executor;
        self
    }

    pub fn executor_type_for_tier(&self, tier: &str) -> String {
        if self.executors.contains_key(tier) {
            tier.to_string()
        } else {
            "noop".to_string()
        }
    }

    pub fn has_executor_for_tier(&self, tier: &str) -> bool {
        self.executors.contains_key(tier)
    }
}

impl Executor for MultiExecutor {
    fn execute(
        &self,
        decision: &DispatchDecision,
        raw_request: &str,
        dispatch_id: &str,
        runtime: &mut FixtureRuntime,
    ) -> ExecutionResult {
        let tier = &decision.selected_tier;

        if let Some(executor) = self.executors.get(tier) {
            return executor.execute(decision, raw_request, dispatch_id, runtime);
        }

        self.default_executor
            .execute(decision, raw_request, dispatch_id, runtime)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatch_decision::DispatchDecision;

    fn make_decision(tier: &str) -> DispatchDecision {
        DispatchDecision {
            selected_tier: tier.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_multi_executor_routes_to_correct_executor() {
        let mut executors: HashMap<String, Box<dyn Executor>> = HashMap::new();
        executors.insert(
            "claude_code_cli".to_string(),
            Box::new(CliTestExecutor {
                executor_type: "claude_code_cli".to_string(),
            }),
        );
        let multi = MultiExecutor::new(executors);
        let mut runtime = FixtureRuntime::new();
        let decision = make_decision("claude_code_cli");

        let result = multi.execute(&decision, "test", "disp-1", &mut runtime);
        assert_eq!(result.executor_type, "claude_code_cli");
        assert_eq!(result.status, "cli_completed");
    }

    #[test]
    fn test_multi_executor_falls_back_to_default() {
        let multi = MultiExecutor::new(HashMap::new());
        let mut runtime = FixtureRuntime::new();
        let decision = make_decision("unknown_tier");

        let result = multi.execute(&decision, "test", "disp-1", &mut runtime);
        assert_eq!(result.executor_type, "noop");
        assert_eq!(result.status, "not_executed");
    }

    #[test]
    fn test_has_executor_for_tier() {
        let mut executors: HashMap<String, Box<dyn Executor>> = HashMap::new();
        executors.insert(
            "claude_code_cli".to_string(),
            Box::new(CliTestExecutor {
                executor_type: "claude_code_cli".to_string(),
            }),
        );
        let multi = MultiExecutor::new(executors);
        assert!(multi.has_executor_for_tier("claude_code_cli"));
        assert!(!multi.has_executor_for_tier("codex_cli"));
    }

    struct CliTestExecutor {
        executor_type: String,
    }

    impl Executor for CliTestExecutor {
        fn execute(
            &self,
            decision: &DispatchDecision,
            _raw_request: &str,
            dispatch_id: &str,
            runtime: &mut FixtureRuntime,
        ) -> ExecutionResult {
            ExecutionResult {
                schema_version: "execution_result.v1".to_string(),
                result_id: runtime.id("exec-"),
                dispatch_id: dispatch_id.to_string(),
                decision_id: decision.decision_id.clone(),
                executor_type: self.executor_type.clone(),
                status: "cli_completed".to_string(),
                output: Some("test output".to_string()),
                prompt_pack: None,
                input_tokens: Some(100),
                output_tokens: Some(50),
                estimated_cost: Some(0.01),
                latency_ms: Some(100),
                error_domain: None,
                error_message: None,
                provider_request_id: None,
                attempt_number: Some(1),
                finish_reason: Some("cli_success".to_string()),
                usage_source: Some(self.executor_type.clone()),
                created_at: runtime.now(),
            }
        }
    }
}
