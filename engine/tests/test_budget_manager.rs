use engine::budget_manager::BudgetManager;
use engine::runtime::FixtureRuntime;
use engine::task_analyzer::{RuleBasedTaskAnalyzer, TaskAnalysis};

fn make_analysis(request: &str) -> TaskAnalysis {
    RuleBasedTaskAnalyzer::new().analyze(request, "test_fixture")
}

#[test]
fn test_create_reservation() {
    let bm = BudgetManager::new();
    let analysis = make_analysis("Summarize the README");
    let mut rt = FixtureRuntime::new();
    let r = bm.create_reservation("dec-001", &analysis, "balanced_worker", &mut rt);
    assert_eq!(r.status, "reserved");
    assert_eq!(r.decision_id, "dec-001");
    assert!(r.reserved_total_tokens > 0);
}

#[test]
fn test_reservation_has_token_breakdown() {
    let bm = BudgetManager::new();
    let analysis = make_analysis("Summarize the README");
    let mut rt = FixtureRuntime::new();
    let r = bm.create_reservation("dec-001", &analysis, "balanced_worker", &mut rt);
    assert_eq!(
        r.reserved_total_tokens,
        r.reserved_input_tokens + r.reserved_output_tokens
    );
}

#[test]
fn test_check_violation_within_budget() {
    let bm = BudgetManager::new();
    let analysis = make_analysis("Summarize the README");
    let mut rt = FixtureRuntime::new();
    let r = bm.create_reservation("dec-001", &analysis, "balanced_worker", &mut rt);
    let (violated, _) = bm.check_violation(&r, r.reserved_total_tokens - 1);
    assert!(!violated);
}

#[test]
fn test_check_violation_exceeds_budget() {
    let bm = BudgetManager::new();
    let analysis = make_analysis("Summarize the README");
    let mut rt = FixtureRuntime::new();
    let r = bm.create_reservation("dec-001", &analysis, "balanced_worker", &mut rt);
    let (violated, reason) = bm.check_violation(&r, r.reserved_total_tokens + 100);
    assert!(violated);
    assert!(reason.unwrap().contains("exceeded"));
}

#[test]
fn test_estimate_cost_positive() {
    let bm = BudgetManager::new();
    let cost = bm.estimate_cost("balanced_worker", 1000, 500);
    assert!(cost > 0.0);
}

#[test]
fn test_estimate_cost_cheaper_tier() {
    let bm = BudgetManager::new();
    let cheap = bm.estimate_cost("cheap_executor", 1000, 500);
    let expensive = bm.estimate_cost("strong_planner", 1000, 500);
    assert!(cheap < expensive);
}

#[test]
fn test_default_currency() {
    let bm = BudgetManager::new();
    let analysis = make_analysis("Summarize the README");
    let mut rt = FixtureRuntime::new();
    let r = bm.create_reservation("dec-001", &analysis, "balanced_worker", &mut rt);
    assert_eq!(r.currency, "token");
}

#[test]
fn test_reservation_cost_rounded() {
    let bm = BudgetManager::new();
    let analysis = make_analysis("Summarize the README");
    let mut rt = FixtureRuntime::new();
    let r = bm.create_reservation("dec-001", &analysis, "balanced_worker", &mut rt);
    assert_eq!(
        r.reserved_cost,
        (r.reserved_cost * 1_000_000.0).round() / 1_000_000.0
    );
}
