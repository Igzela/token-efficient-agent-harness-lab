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
        EvaluationResult {
            schema_version: "evaluation_result.v1".to_string(),
            evaluation_id: runtime.id("eval-"),
            dispatch_id: result.dispatch_id.clone(),
            decision_id: result.decision_id.clone(),
            execution_result_id: result.result_id.clone(),
            status: if requires_review {
                "needs_human_review"
            } else {
                "pass"
            }
            .to_string(),
            checks,
            quality_score: None,
            requires_retry: false,
            retry_reason: None,
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
