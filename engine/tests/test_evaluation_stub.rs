use engine::dispatch_decision::{BudgetReservation, DispatchDecision};
use engine::evaluation_stub::EvaluationStub;
use engine::executor_adapter::ExecutionResult;
use engine::runtime::FixtureRuntime;

fn make_execution_result() -> ExecutionResult {
    ExecutionResult {
        schema_version: "execution_result.v1".to_string(),
        result_id: "exec-001".to_string(),
        dispatch_id: "disp-001".to_string(),
        decision_id: "dec-001".to_string(),
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
        created_at: "2026-01-01T00:00:00Z".to_string(),
    }
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
        execution_policy: serde_json::json!({"executor_type": "noop", "execution_allowed": true, "requires_human_review": false, "max_retries": 0}),
        decision_status: "decided".to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        ..Default::default()
    }
}

#[test]
fn test_all_five_checks_present() {
    let evaluator = EvaluationStub;
    let mut rt = FixtureRuntime::new();
    let result = evaluator.evaluate(&make_execution_result(), &make_decision(), &mut rt);
    let check_names: Vec<&str> = result.checks.iter().map(|c| c.name.as_str()).collect();
    assert!(check_names.contains(&"schema_validity"));
    assert!(check_names.contains(&"boundary_compliance"));
    assert!(check_names.contains(&"output_present"));
    assert!(check_names.contains(&"error_free"));
    assert!(check_names.contains(&"human_review_required"));
}

#[test]
fn test_noop_output_present_is_warning() {
    let evaluator = EvaluationStub;
    let mut rt = FixtureRuntime::new();
    let result = evaluator.evaluate(&make_execution_result(), &make_decision(), &mut rt);
    let output_check = result
        .checks
        .iter()
        .find(|c| c.name == "output_present")
        .unwrap();
    assert_eq!(output_check.status, "warning");
}

#[test]
fn test_boundary_compliance_noop() {
    let evaluator = EvaluationStub;
    let mut rt = FixtureRuntime::new();
    let result = evaluator.evaluate(&make_execution_result(), &make_decision(), &mut rt);
    let bc = result
        .checks
        .iter()
        .find(|c| c.name == "boundary_compliance")
        .unwrap();
    assert_eq!(bc.status, "pass");
}

#[test]
fn test_error_free_no_error() {
    let evaluator = EvaluationStub;
    let mut rt = FixtureRuntime::new();
    let result = evaluator.evaluate(&make_execution_result(), &make_decision(), &mut rt);
    let ef = result
        .checks
        .iter()
        .find(|c| c.name == "error_free")
        .unwrap();
    assert_eq!(ef.status, "pass");
}

#[test]
fn test_human_review_required() {
    let evaluator = EvaluationStub;
    let mut rt = FixtureRuntime::new();
    let mut decision = make_decision();
    decision.execution_policy = serde_json::json!({"executor_type": "noop", "execution_allowed": true, "requires_human_review": true, "max_retries": 0});
    let result = evaluator.evaluate(&make_execution_result(), &decision, &mut rt);
    let hr = result
        .checks
        .iter()
        .find(|c| c.name == "human_review_required")
        .unwrap();
    assert_eq!(hr.status, "warning");
}

#[test]
fn test_overall_status_pass() {
    let evaluator = EvaluationStub;
    let mut rt = FixtureRuntime::new();
    let result = evaluator.evaluate(&make_execution_result(), &make_decision(), &mut rt);
    assert_eq!(result.status, "pass");
}

#[test]
fn test_overall_status_needs_human_review() {
    let evaluator = EvaluationStub;
    let mut rt = FixtureRuntime::new();
    let mut decision = make_decision();
    decision.execution_policy = serde_json::json!({"executor_type": "noop", "execution_allowed": true, "requires_human_review": true, "max_retries": 0});
    let result = evaluator.evaluate(&make_execution_result(), &decision, &mut rt);
    assert_eq!(result.status, "needs_human_review");
}

#[test]
fn test_to_value() {
    let evaluator = EvaluationStub;
    let mut rt = FixtureRuntime::new();
    let result = evaluator.evaluate(&make_execution_result(), &make_decision(), &mut rt);
    let v = result.to_value();
    assert!(v.get("evaluation_id").is_some());
    assert!(v.get("checks").is_some());
    assert!(v["checks"].is_array());
}

#[test]
fn test_result_links_to_decision() {
    let evaluator = EvaluationStub;
    let mut rt = FixtureRuntime::new();
    let result = evaluator.evaluate(&make_execution_result(), &make_decision(), &mut rt);
    assert_eq!(result.dispatch_id, "disp-001");
    assert_eq!(result.decision_id, "dec-001");
    assert_eq!(result.execution_result_id, "exec-001");
}
