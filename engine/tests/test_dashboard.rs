use engine::ecosystem::dashboard::*;

fn make_experiment(id: &str, metric: &str, va: f64, vb: f64, winner: &str) -> ExperimentResult {
    ExperimentResult {
        schema_version: DASHBOARD_SCHEMA_VERSION.to_string(),
        experiment_id: id.to_string(),
        model_a: "gpt-4".to_string(),
        model_b: "claude-3".to_string(),
        task_group: "code/review".to_string(),
        metric_name: metric.to_string(),
        value_a: va,
        value_b: vb,
        winner: winner.to_string(),
        sample_count: 100,
        created_at: 1000.0,
    }
}

#[test]
fn test_record_and_get() {
    let mut dashboard = DispatchDashboard::new();
    assert!(dashboard.record_experiment(&make_experiment("e1", "quality", 0.8, 0.9, "b")));
    let e = dashboard.get_experiment("e1").unwrap();
    assert_eq!(e.winner, "b");
}

#[test]
fn test_record_duplicate() {
    let mut dashboard = DispatchDashboard::new();
    assert!(dashboard.record_experiment(&make_experiment("e1", "quality", 0.8, 0.9, "b")));
    assert!(!dashboard.record_experiment(&make_experiment("e1", "quality", 0.8, 0.9, "b")));
}

#[test]
fn test_validate_valid() {
    let dashboard = DispatchDashboard::new();
    let errors = dashboard.validate_experiment(&make_experiment("e1", "quality", 0.8, 0.9, "b"));
    assert!(errors.is_empty());
}

#[test]
fn test_validate_invalid_winner() {
    let dashboard = DispatchDashboard::new();
    let mut exp = make_experiment("e1", "quality", 0.8, 0.9, "b");
    exp.winner = "invalid".to_string();
    let errors = dashboard.validate_experiment(&exp);
    assert!(errors.iter().any(|e| e.contains("winner")));
}

#[test]
fn test_validate_nan_values() {
    let dashboard = DispatchDashboard::new();
    let mut exp = make_experiment("e1", "quality", f64::NAN, 0.9, "b");
    exp.winner = "b".to_string();
    let errors = dashboard.validate_experiment(&exp);
    assert!(errors.iter().any(|e| e.contains("finite")));
}

#[test]
fn test_validate_negative_sample_count() {
    let dashboard = DispatchDashboard::new();
    let mut exp = make_experiment("e1", "quality", 0.8, 0.9, "b");
    exp.sample_count = -1;
    let errors = dashboard.validate_experiment(&exp);
    assert!(errors.iter().any(|e| e.contains("sample_count")));
}

#[test]
fn test_experiments_by_model() {
    let mut dashboard = DispatchDashboard::new();
    dashboard.record_experiment(&make_experiment("e1", "quality", 0.8, 0.9, "b"));
    let by_model = dashboard.experiments_by_model("gpt-4");
    assert_eq!(by_model.len(), 1);
    assert_eq!(dashboard.experiments_by_model("nonexistent").len(), 0);
}

#[test]
fn test_experiments_by_task_group() {
    let mut dashboard = DispatchDashboard::new();
    dashboard.record_experiment(&make_experiment("e1", "quality", 0.8, 0.9, "b"));
    assert_eq!(dashboard.experiments_by_task_group("code/review").len(), 1);
    assert_eq!(dashboard.experiments_by_task_group("nonexistent").len(), 0);
}

#[test]
fn test_compute_summary_empty() {
    let dashboard = DispatchDashboard::new();
    let summary = dashboard.compute_summary(100, 50);
    assert_eq!(summary.total_dispatches, 100);
    assert_eq!(summary.active_experiments, 0);
    assert_eq!(summary.cost_savings_pct, 0.0);
}

#[test]
fn test_compute_summary_with_experiments() {
    let mut dashboard = DispatchDashboard::new();
    dashboard.record_experiment(&make_experiment("e1", "cost", 0.10, 0.05, "b"));
    dashboard.record_experiment(&make_experiment("e2", "quality", 0.8, 0.9, "b"));
    let summary = dashboard.compute_summary(100, 50);
    assert_eq!(summary.active_experiments, 2);
    // cost: (0.05 - 0.10) / 0.10 * 100 = -50%, averaged over 2 = -25%
    assert!((summary.cost_savings_pct - (-25.0)).abs() < 0.01);
}

#[test]
fn test_constants() {
    assert!(VALID_WINNERS.contains(&"a"));
    assert!(VALID_WINNERS.contains(&"b"));
    assert!(VALID_WINNERS.contains(&"tie"));
}
