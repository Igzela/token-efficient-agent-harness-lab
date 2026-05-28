use engine::dispatch_decision::{BudgetReservation, DispatchDecision};
use engine::dispatch_ledger::DispatchLedger;
use engine::evaluation_stub::EvaluationResult;
use engine::executor_adapter::ExecutionResult;
use engine::runtime::FixtureRuntime;
use engine::task_analyzer::{RuleBasedTaskAnalyzer, TaskAnalysis};

fn make_analysis() -> TaskAnalysis {
    RuleBasedTaskAnalyzer::new().analyze("Review auth.py for security issues", "test_fixture")
}

fn make_decision() -> DispatchDecision {
    DispatchDecision {
        decision_id: "dec-001".to_string(),
        analysis_id: "a-001".to_string(),
        analysis_snapshot: serde_json::json!({}),
        selected_tier: "balanced_worker".to_string(),
        fallback_tier: "cheap_executor".to_string(),
        routing_reason: "test".to_string(),
        quality_requirement: "standard".to_string(),
        expected_quality_band: "medium".to_string(),
        confidence: 0.8,
        confidence_label: "high".to_string(),
        budget_reservation: BudgetReservation::default(),
        execution_policy: serde_json::json!({}),
        decision_status: "decided".to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        ..Default::default()
    }
}

fn make_execution_result() -> ExecutionResult {
    ExecutionResult {
        schema_version: "execution_result.v1".to_string(),
        result_id: "exec-001".to_string(),
        dispatch_id: "disp-001".to_string(),
        decision_id: "dec-001".to_string(),
        executor_type: "noop".to_string(),
        status: "completed".to_string(),
        output: Some("test output".to_string()),
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
        created_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

fn make_evaluation_result() -> EvaluationResult {
    EvaluationResult {
        schema_version: "evaluation_result.v1".to_string(),
        evaluation_id: "eval-001".to_string(),
        dispatch_id: "disp-001".to_string(),
        decision_id: "dec-001".to_string(),
        execution_result_id: "exec-001".to_string(),
        status: "pass".to_string(),
        checks: vec![],
        quality_score: None,
        requires_retry: false,
        retry_reason: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

// DispatchRecord tests
#[test]
fn test_create_record() {
    let ledger = DispatchLedger::new();
    let rt = FixtureRuntime::new();
    let r = ledger.create_record("disp-001", "test request", "a-001", "dec-001", None, &rt);
    assert_eq!(r.dispatch_id, "disp-001");
    assert_eq!(r.final_status, "dispatched");
}

#[test]
fn test_create_with_budget_reservation() {
    let ledger = DispatchLedger::new();
    let rt = FixtureRuntime::new();
    let r = ledger.create_record(
        "disp-001",
        "req",
        "a-001",
        "dec-001",
        Some("bres-001".to_string()),
        &rt,
    );
    assert_eq!(r.budget_reservation_id, Some("bres-001".to_string()));
}

#[test]
fn test_update_record() {
    let ledger = DispatchLedger::new();
    let rt = FixtureRuntime::new();
    let r = ledger.create_record("disp-001", "req", "a-001", "dec-001", None, &rt);
    let updated = ledger.update_record(
        r,
        "completed",
        Some("exec-001".to_string()),
        None,
        None,
        &rt,
    );
    assert_eq!(updated.final_status, "completed");
    assert_eq!(updated.execution_result_id, Some("exec-001".to_string()));
}

#[test]
fn test_update_with_usage_ledger_row() {
    let ledger = DispatchLedger::new();
    let rt = FixtureRuntime::new();
    let r = ledger.create_record("disp-001", "req", "a-001", "dec-001", None, &rt);
    let updated =
        ledger.update_record(r, "completed", None, None, Some("ulr-001".to_string()), &rt);
    assert_eq!(updated.usage_ledger_row_id, Some("ulr-001".to_string()));
}

#[test]
fn test_record_to_value() {
    let ledger = DispatchLedger::new();
    let rt = FixtureRuntime::new();
    let r = ledger.create_record("disp-001", "req", "a-001", "dec-001", None, &rt);
    let v = r.to_value();
    assert!(v.get("dispatch_id").is_some());
    assert!(v.get("final_status").is_some());
    assert!(v.get("schema_version").is_some());
}

// DispatchBundle tests
#[test]
fn test_store_bundle() {
    let ledger = DispatchLedger::new();
    let rt = FixtureRuntime::new();
    let record = ledger.create_record("disp-001", "req", "a-001", "dec-001", None, &rt);
    let bundle = ledger.store_bundle(
        record,
        make_analysis(),
        make_decision(),
        make_execution_result(),
        make_evaluation_result(),
    );
    let v = bundle.to_value();
    assert!(v.get("record").is_some());
    assert!(v.get("analysis").is_some());
    assert!(v.get("decision").is_some());
    assert!(v.get("execution_result").is_some());
    assert!(v.get("evaluation_result").is_some());
}

#[test]
fn test_bundle_record_has_dispatch_id() {
    let ledger = DispatchLedger::new();
    let rt = FixtureRuntime::new();
    let record = ledger.create_record("disp-001", "req", "a-001", "dec-001", None, &rt);
    let bundle = ledger.store_bundle(
        record,
        make_analysis(),
        make_decision(),
        make_execution_result(),
        make_evaluation_result(),
    );
    assert_eq!(bundle.record["dispatch_id"], "disp-001");
}

#[test]
fn test_update_preserves_existing_ids() {
    let ledger = DispatchLedger::new();
    let rt = FixtureRuntime::new();
    let r = ledger.create_record(
        "disp-001",
        "req",
        "a-001",
        "dec-001",
        Some("bres-001".to_string()),
        &rt,
    );
    let updated = ledger.update_record(
        r,
        "completed",
        Some("exec-001".to_string()),
        Some("eval-001".to_string()),
        None,
        &rt,
    );
    assert_eq!(updated.budget_reservation_id, Some("bres-001".to_string()));
    assert_eq!(updated.execution_result_id, Some("exec-001".to_string()));
    assert_eq!(updated.evaluation_result_id, Some("eval-001".to_string()));
}
