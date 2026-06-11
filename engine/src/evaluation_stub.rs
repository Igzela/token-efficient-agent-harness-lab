use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::dispatch_decision::DispatchDecision;
use crate::executor_adapter::ExecutionResult;
use crate::runtime::FixtureRuntime;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EvaluationCheck {
    pub check_id: String,
    pub name: String,
    pub status: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EvaluationResult {
    pub schema_version: String,
    pub evaluation_id: String,
    pub dispatch_id: String,
    pub decision_id: String,
    pub execution_result_id: String,
    pub status: String,
    pub checks: Vec<EvaluationCheck>,
    pub quality_score: Option<f64>,
    pub requires_retry: bool,
    pub retry_reason: Option<String>,
    pub created_at: String,
}

impl EvaluationResult {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("EvaluationResult should serialize")
    }
}

pub trait Evaluator: Send + Sync {
    fn evaluate(
        &self,
        result: &ExecutionResult,
        decision: &DispatchDecision,
        runtime: &mut FixtureRuntime,
    ) -> EvaluationResult;
}

#[derive(Default)]
pub struct EvaluationStub;

impl EvaluationStub {
    pub fn evaluate(
        &self,
        result: &ExecutionResult,
        decision: &DispatchDecision,
        runtime: &mut FixtureRuntime,
    ) -> EvaluationResult {
        let requires_review = decision
            .execution_policy
            .get("requires_human_review")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut checks = vec![
            EvaluationCheck {
                check_id: runtime.id("chk-"),
                name: "schema_validity".to_string(),
                status: "pass".to_string(),
                reason: "required fields present".to_string(),
            },
            EvaluationCheck {
                check_id: runtime.id("chk-"),
                name: "boundary_compliance".to_string(),
                status: "pass".to_string(),
                reason: "executor_type=noop within boundaries".to_string(),
            },
            EvaluationCheck {
                check_id: runtime.id("chk-"),
                name: "output_present".to_string(),
                status: "warning".to_string(),
                reason: "noop executor produces no output (expected)".to_string(),
            },
            EvaluationCheck {
                check_id: runtime.id("chk-"),
                name: "error_free".to_string(),
                status: "pass".to_string(),
                reason: "no errors".to_string(),
            },
        ];
        checks.push(EvaluationCheck {
            check_id: runtime.id("chk-"),
            name: "human_review_required".to_string(),
            status: if requires_review { "warning" } else { "pass" }.to_string(),
            reason: if requires_review {
                "execution policy requires human review"
            } else {
                "no human review required"
            }
            .to_string(),
        });
        if let Some(ref output) = result.output {
            checks.push(EvaluationCheck {
                check_id: runtime.id("chk-"),
                name: "output_non_empty".to_string(),
                status: if output.is_empty() { "fail" } else { "pass" }.to_string(),
                reason: if output.is_empty() {
                    "LLM returned empty output"
                } else {
                    "output is non-empty"
                }
                .to_string(),
            });
            checks.push(EvaluationCheck {
                check_id: runtime.id("chk-"),
                name: "output_substantial".to_string(),
                status: if !output.is_empty() && output.len() < 10 {
                    "warning"
                } else {
                    "pass"
                }
                .to_string(),
                reason: if !output.is_empty() && output.len() < 10 {
                    "Output suspiciously short"
                } else {
                    "output length is acceptable"
                }
                .to_string(),
            });
        }
        if result.status == "failed" {
            checks.push(EvaluationCheck {
                check_id: runtime.id("chk-"),
                name: "no_error_in_output".to_string(),
                status: "fail".to_string(),
                reason: result
                    .error_message
                    .clone()
                    .unwrap_or_else(|| "execution failed".to_string()),
            });
        }
        let has_fail = checks.iter().any(|c| c.status == "fail");
        let status = if requires_review {
            "needs_human_review"
        } else if has_fail {
            "fail"
        } else {
            "pass"
        };
        let should_retry = has_fail && !requires_review;
        EvaluationResult {
            schema_version: "evaluation_result.v1".to_string(),
            evaluation_id: runtime.id("eval-"),
            dispatch_id: result.dispatch_id.clone(),
            decision_id: result.decision_id.clone(),
            execution_result_id: result.result_id.clone(),
            status: status.to_string(),
            checks,
            quality_score: None,
            requires_retry: should_retry,
            retry_reason: if should_retry {
                Some("evaluation check failed".to_string())
            } else {
                None
            },
            created_at: runtime.now(),
        }
    }
}

impl Evaluator for EvaluationStub {
    fn evaluate(
        &self,
        result: &ExecutionResult,
        decision: &DispatchDecision,
        runtime: &mut FixtureRuntime,
    ) -> EvaluationResult {
        EvaluationStub::evaluate(self, result, decision, runtime)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_human_review_prevents_retry() {
        let stub = EvaluationStub;
        let mut runtime = FixtureRuntime::new();
        let result = ExecutionResult {
            schema_version: "execution_result.v1".to_string(),
            result_id: runtime.id("exec-"),
            dispatch_id: "d1".to_string(),
            decision_id: "dec1".to_string(),
            executor_type: "noop".to_string(),
            status: "completed".to_string(),
            output: Some("ok".to_string()),
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
        };
        let decision = DispatchDecision {
            decision_id: "dec1".to_string(),
            execution_policy: serde_json::json!({"requires_human_review": true}),
            ..DispatchDecision::default()
        };
        let eval = stub.evaluate(&result, &decision, &mut runtime);
        assert_eq!(eval.status, "needs_human_review");
        assert!(
            !eval.requires_retry,
            "should not retry when human review is required"
        );
    }
}
