use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::dispatch_decision::DispatchDecision;
use crate::evaluation_stub::EvaluationResult;
use crate::executor_adapter::ExecutionResult;
use crate::runtime::FixtureRuntime;
use crate::task_analyzer::TaskAnalysis;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DispatchRecord {
    pub schema_version: String,
    pub dispatch_id: String,
    pub request_snapshot: String,
    pub task_analysis_id: String,
    pub decision_id: String,
    pub execution_result_id: Option<String>,
    pub evaluation_result_id: Option<String>,
    pub usage_ledger_row_id: Option<String>,
    pub budget_reservation_id: Option<String>,
    pub final_status: String,
    pub created_at: String,
    pub updated_at: String,
}

impl DispatchRecord {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("DispatchRecord should serialize")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DispatchBundle {
    pub record: Value,
    pub analysis: Value,
    pub decision: Value,
    pub execution_result: Value,
    pub evaluation_result: Value,
}

impl DispatchBundle {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("DispatchBundle should serialize")
    }
}

#[derive(Default)]
pub struct DispatchLedger;

impl DispatchLedger {
    pub fn new() -> Self {
        Self
    }

    pub fn create_record(
        &self,
        dispatch_id: &str,
        request_snapshot: &str,
        task_analysis_id: &str,
        decision_id: &str,
        budget_reservation_id: Option<String>,
        runtime: &FixtureRuntime,
    ) -> DispatchRecord {
        DispatchRecord {
            schema_version: "dispatch_record.v1".to_string(),
            dispatch_id: dispatch_id.to_string(),
            request_snapshot: request_snapshot.to_string(),
            task_analysis_id: task_analysis_id.to_string(),
            decision_id: decision_id.to_string(),
            execution_result_id: None,
            evaluation_result_id: None,
            usage_ledger_row_id: None,
            budget_reservation_id,
            final_status: "dispatched".to_string(),
            created_at: runtime.now(),
            updated_at: runtime.now(),
        }
    }

    pub fn update_record(
        &self,
        record: DispatchRecord,
        final_status: &str,
        execution_result_id: Option<String>,
        evaluation_result_id: Option<String>,
        usage_ledger_row_id: Option<String>,
        runtime: &FixtureRuntime,
    ) -> DispatchRecord {
        DispatchRecord {
            final_status: final_status.to_string(),
            execution_result_id: execution_result_id.or(record.execution_result_id),
            evaluation_result_id: evaluation_result_id.or(record.evaluation_result_id),
            usage_ledger_row_id: usage_ledger_row_id.or(record.usage_ledger_row_id),
            updated_at: runtime.now(),
            ..record
        }
    }

    pub fn store_bundle(
        &self,
        record: DispatchRecord,
        analysis: TaskAnalysis,
        decision: DispatchDecision,
        execution_result: ExecutionResult,
        evaluation_result: EvaluationResult,
    ) -> DispatchBundle {
        DispatchBundle {
            record: record.to_value(),
            analysis: analysis.to_value(),
            decision: decision.to_value(),
            execution_result: execution_result.to_value(),
            evaluation_result: evaluation_result.to_value(),
        }
    }
}
