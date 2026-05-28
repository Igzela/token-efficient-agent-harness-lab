use engine::ecosystem::benchmark::*;

fn make_task(id: &str) -> BenchmarkTask {
    BenchmarkTask {
        schema_version: BENCHMARK_SCHEMA_VERSION.to_string(),
        task_id: id.to_string(),
        prompt: "Test prompt".to_string(),
        expected_quality: 0.8,
        task_group: "code/review".to_string(),
        max_tokens: 1000,
    }
}

fn make_result(task_id: &str, model: &str, quality: f64, passed: bool) -> BenchmarkResult {
    BenchmarkResult {
        schema_version: BENCHMARK_SCHEMA_VERSION.to_string(),
        task_id: task_id.to_string(),
        model_name: model.to_string(),
        provider: "openai".to_string(),
        output: "test output".to_string(),
        quality_score: quality,
        tokens_used: 100,
        latency_ms: 500.0,
        cost_usd: 0.01,
        passed,
    }
}

#[test]
fn test_add_and_list_tasks() {
    let mut suite = BenchmarkSuite::new();
    assert!(suite.add_task(&make_task("t1")));
    assert!(suite.add_task(&make_task("t2")));
    assert_eq!(suite.list_tasks().len(), 2);
}

#[test]
fn test_add_duplicate_task() {
    let mut suite = BenchmarkSuite::new();
    assert!(suite.add_task(&make_task("t1")));
    assert!(!suite.add_task(&make_task("t1")));
}

#[test]
fn test_remove_task() {
    let mut suite = BenchmarkSuite::new();
    suite.add_task(&make_task("t1"));
    assert!(suite.remove_task("t1"));
    assert!(suite.list_tasks().is_empty());
    assert!(!suite.remove_task("nonexistent"));
}

#[test]
fn test_record_result() {
    let mut suite = BenchmarkSuite::new();
    suite.add_task(&make_task("t1"));
    assert!(suite.record_result(&make_result("t1", "gpt-4", 0.9, true)));
    assert_eq!(suite.results_for_task("t1").len(), 1);
}

#[test]
fn test_record_result_unknown_task() {
    let mut suite = BenchmarkSuite::new();
    assert!(!suite.record_result(&make_result("nonexistent", "gpt-4", 0.9, true)));
}

#[test]
fn test_results_for_model() {
    let mut suite = BenchmarkSuite::new();
    suite.add_task(&make_task("t1"));
    suite.add_task(&make_task("t2"));
    suite.record_result(&make_result("t1", "gpt-4", 0.9, true));
    suite.record_result(&make_result("t2", "claude-3", 0.85, true));
    assert_eq!(suite.results_for_model("gpt-4").len(), 1);
    assert_eq!(suite.results_for_model("claude-3").len(), 1);
}

#[test]
fn test_compare_models() {
    let mut suite = BenchmarkSuite::new();
    suite.add_task(&make_task("t1"));
    suite.record_result(&make_result("t1", "gpt-4", 0.9, true));
    suite.record_result(&make_result("t1", "claude-3", 0.85, true));
    let comparison = suite.compare_models("gpt-4", "claude-3");
    assert_eq!(comparison["model_a"], "gpt-4");
    assert_eq!(comparison["model_b"], "claude-3");
    assert_eq!(comparison["model_a_stats"]["task_count"], 1);
    assert_eq!(comparison["model_b_stats"]["task_count"], 1);
}

#[test]
fn test_leaderboard() {
    let mut suite = BenchmarkSuite::new();
    suite.add_task(&make_task("t1"));
    suite.add_task(&make_task("t2"));
    suite.record_result(&make_result("t1", "gpt-4", 0.9, true));
    suite.record_result(&make_result("t2", "gpt-4", 0.8, true));
    suite.record_result(&make_result("t1", "claude-3", 0.85, true));
    let lb = suite.leaderboard();
    assert_eq!(lb.len(), 2);
    // Sorted by avg_quality descending
    assert_eq!(lb[0]["model"], "gpt-4"); // avg 0.85
    assert_eq!(lb[1]["model"], "claude-3"); // avg 0.85
}

#[test]
fn test_validate_task_valid() {
    let suite = BenchmarkSuite::new();
    let errors = suite.validate_task(&make_task("t1"));
    assert!(errors.is_empty());
}

#[test]
fn test_validate_task_empty_id() {
    let suite = BenchmarkSuite::new();
    let mut task = make_task("t1");
    task.task_id = String::new();
    let errors = suite.validate_task(&task);
    assert!(errors.iter().any(|e| e.contains("task_id")));
}

#[test]
fn test_validate_task_quality_out_of_range() {
    let suite = BenchmarkSuite::new();
    let mut task = make_task("t1");
    task.expected_quality = 1.5;
    let errors = suite.validate_task(&task);
    assert!(errors.iter().any(|e| e.contains("expected_quality")));
}

#[test]
fn test_validate_result_valid() {
    let suite = BenchmarkSuite::new();
    let errors = suite.validate_result(&make_result("t1", "gpt-4", 0.9, true));
    assert!(errors.is_empty());
}

#[test]
fn test_validate_result_negative_latency() {
    let suite = BenchmarkSuite::new();
    let mut result = make_result("t1", "gpt-4", 0.9, true);
    result.latency_ms = -1.0;
    let errors = suite.validate_result(&result);
    assert!(errors.iter().any(|e| e.contains("latency")));
}

#[test]
fn test_to_dict() {
    let task = make_task("t1");
    let d = task.to_dict();
    assert_eq!(d["task_id"], "t1");
    assert_eq!(d["expected_quality"], 0.8);
}
