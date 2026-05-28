use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::dispatch_decision::DispatchDecision;
use crate::runtime::FixtureRuntime;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ExecutionResult {
    pub schema_version: String,
    pub result_id: String,
    pub dispatch_id: String,
    pub decision_id: String,
    pub executor_type: String,
    pub status: String,
    pub output: Option<String>,
    pub prompt_pack: Option<Value>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub estimated_cost: Option<f64>,
    pub latency_ms: Option<i64>,
    pub error_domain: Option<String>,
    pub error_message: Option<String>,
    pub provider_request_id: Option<String>,
    pub attempt_number: Option<i64>,
    pub finish_reason: Option<String>,
    pub usage_source: Option<String>,
    pub created_at: String,
}

impl ExecutionResult {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("ExecutionResult should serialize")
    }
}

pub trait Executor: Send + Sync {
    fn execute(
        &self,
        decision: &DispatchDecision,
        raw_request: &str,
        dispatch_id: &str,
        runtime: &mut FixtureRuntime,
    ) -> ExecutionResult;
}

#[derive(Default)]
pub struct NoopExecutor;

impl Executor for NoopExecutor {
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
            executor_type: "noop".to_string(),
            status: "not_executed".to_string(),
            output: None,
            prompt_pack: None,
            input_tokens: None,
            output_tokens: None,
            estimated_cost: None,
            latency_ms: None,
            error_domain: None,
            error_message: None,
            provider_request_id: None,
            attempt_number: None,
            finish_reason: None,
            usage_source: None,
            created_at: runtime.now(),
        }
    }
}
