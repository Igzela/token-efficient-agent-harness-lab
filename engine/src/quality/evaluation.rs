use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Score types (local — scoring.rs is a separate module)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskScore {
    pub task_id: String,
    pub weighted_score: f64,
    pub grade: String,
}

impl Default for TaskScore {
    fn default() -> Self {
        Self {
            task_id: String::new(),
            weighted_score: 0.0,
            grade: "F".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunScore {
    pub run_id: String,
    pub aggregate_score: f64,
    pub grade: String,
    pub item_count: usize,
    pub passed_count: usize,
    pub failed_count: usize,
}

impl Default for RunScore {
    fn default() -> Self {
        Self {
            run_id: String::new(),
            aggregate_score: 0.0,
            grade: "F".to_string(),
            item_count: 0,
            passed_count: 0,
            failed_count: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// EvalSpec
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalSpec {
    pub case_id: String,
    pub fixture_path: String,
    pub expected_outcome: String,
    pub task_dir: Option<String>,
    pub item_id: Option<String>,
    pub description: String,
}

impl Default for EvalSpec {
    fn default() -> Self {
        Self {
            case_id: String::new(),
            fixture_path: String::new(),
            expected_outcome: "pass".to_string(),
            task_dir: None,
            item_id: None,
            description: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// EvalCase
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalCase {
    pub case_id: String,
    pub fixture_path: String,
    pub expected_outcome: String,
    pub actual_outcome: String,
    pub passed: bool,
    pub score: Option<TaskScore>,
}

impl Default for EvalCase {
    fn default() -> Self {
        Self {
            case_id: String::new(),
            fixture_path: String::new(),
            expected_outcome: "pass".to_string(),
            actual_outcome: "fail".to_string(),
            passed: false,
            score: None,
        }
    }
}

// ---------------------------------------------------------------------------
// EvaluationReport
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluationReport {
    pub suite_id: String,
    pub cases: Vec<EvalCase>,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub score: Option<RunScore>,
}

impl Default for EvaluationReport {
    fn default() -> Self {
        Self {
            suite_id: String::new(),
            cases: Vec::new(),
            total: 0,
            passed: 0,
            failed: 0,
            score: None,
        }
    }
}

// ---------------------------------------------------------------------------
// EvaluationRunner
// ---------------------------------------------------------------------------

pub struct EvaluationRunner;

impl Default for EvaluationRunner {
    fn default() -> Self {
        Self
    }
}

impl EvaluationRunner {
    pub fn new() -> Self {
        Self
    }

    pub fn run_single(&self, spec: &EvalSpec) -> EvalCase {
        let actual = self.evaluate(spec);
        let passed = actual == spec.expected_outcome;
        EvalCase {
            case_id: spec.case_id.clone(),
            fixture_path: spec.fixture_path.clone(),
            expected_outcome: spec.expected_outcome.clone(),
            actual_outcome: actual,
            passed,
            score: None,
        }
    }

    pub fn run_suite(&self, suite_id: &str, specs: &[EvalSpec]) -> EvaluationReport {
        let results: Vec<EvalCase> = specs.iter().map(|s| self.run_single(s)).collect();
        let passed = results.iter().filter(|r| r.passed).count();
        let failed = results.len() - passed;

        EvaluationReport {
            suite_id: suite_id.to_string(),
            cases: results,
            total: specs.len(),
            passed,
            failed,
            score: None,
        }
    }

    fn evaluate(&self, spec: &EvalSpec) -> String {
        let path = std::path::Path::new(&spec.fixture_path);
        if !path.exists() {
            return "fail".to_string();
        }

        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "jsonl" {
                    return self.evaluate_jsonl(path);
                }
            }
            return "pass".to_string();
        }

        if path.is_dir() {
            return "pass".to_string();
        }

        "fail".to_string()
    }

    fn evaluate_jsonl(&self, path: &std::path::Path) -> String {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return "fail".to_string(),
        };

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if serde_json::from_str::<serde_json::Value>(trimmed).is_err() {
                return "fail".to_string();
            }
        }
        "pass".to_string()
    }

    pub fn compute_run_score(results: &[EvalCase]) -> RunScore {
        let total = results.len();
        let passed = results.iter().filter(|r| r.passed).count();
        let failed = total - passed;
        let aggregate = if total == 0 {
            0.0
        } else {
            passed as f64 / total as f64
        };
        RunScore {
            run_id: "<computed>".to_string(),
            aggregate_score: aggregate,
            grade: grade(aggregate),
            item_count: total,
            passed_count: passed,
            failed_count: failed,
        }
    }
}

fn grade(score: f64) -> String {
    if score >= 0.90 {
        "A"
    } else if score >= 0.75 {
        "B"
    } else if score >= 0.60 {
        "C"
    } else if score >= 0.40 {
        "D"
    } else {
        "F"
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_spec(case_id: &str, fixture_path: &str, expected: &str) -> EvalSpec {
        EvalSpec {
            case_id: case_id.to_string(),
            fixture_path: fixture_path.to_string(),
            expected_outcome: expected.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_eval_spec_default() {
        let spec = EvalSpec::default();
        assert_eq!(spec.case_id, "");
        assert_eq!(spec.expected_outcome, "pass");
        assert!(spec.task_dir.is_none());
        assert!(spec.item_id.is_none());
    }

    #[test]
    fn test_run_single_missing_fixture_fails() {
        let runner = EvaluationRunner::new();
        let spec = make_spec("c1", "/nonexistent/path.json", "pass");
        let result = runner.run_single(&spec);
        assert!(!result.passed);
        assert_eq!(result.actual_outcome, "fail");
    }

    #[test]
    fn test_run_single_valid_jsonl_passes() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("events.jsonl");
        fs::write(&path, "{\"a\":1}\n{\"b\":2}\n").unwrap();

        let runner = EvaluationRunner::new();
        let spec = make_spec("c1", path.to_str().unwrap(), "pass");
        let result = runner.run_single(&spec);
        assert!(result.passed);
        assert_eq!(result.actual_outcome, "pass");
    }

    #[test]
    fn test_run_single_invalid_jsonl_fails() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("bad.jsonl");
        fs::write(&path, "{\"ok\":true}\nNOT JSON\n").unwrap();

        let runner = EvaluationRunner::new();
        let spec = make_spec("c1", path.to_str().unwrap(), "pass");
        let result = runner.run_single(&spec);
        assert!(!result.passed);
        assert_eq!(result.actual_outcome, "fail");
    }

    #[test]
    fn test_run_single_directory_passes() {
        let tmp = TempDir::new().unwrap();
        let runner = EvaluationRunner::new();
        let spec = make_spec("c1", tmp.path().to_str().unwrap(), "pass");
        let result = runner.run_single(&spec);
        assert!(result.passed);
    }

    #[test]
    fn test_run_suite_counts() {
        let tmp = TempDir::new().unwrap();
        let ok_path = tmp.path().join("ok.jsonl");
        fs::write(&ok_path, "{\"x\":1}\n").unwrap();

        let runner = EvaluationRunner::new();
        let specs = vec![
            make_spec("c1", ok_path.to_str().unwrap(), "pass"),
            make_spec("c2", "/nonexistent/path", "pass"),
        ];
        let report = runner.run_suite("s1", &specs);
        assert_eq!(report.total, 2);
        assert_eq!(report.passed, 1);
        assert_eq!(report.failed, 1);
        assert_eq!(report.suite_id, "s1");
    }

    #[test]
    fn test_run_suite_empty() {
        let runner = EvaluationRunner::new();
        let report = runner.run_suite("empty", &[]);
        assert_eq!(report.total, 0);
        assert_eq!(report.passed, 0);
        assert_eq!(report.failed, 0);
    }

    #[test]
    fn test_eval_case_serializes_roundtrip() {
        let case = EvalCase {
            case_id: "c1".to_string(),
            fixture_path: "/tmp/test".to_string(),
            expected_outcome: "pass".to_string(),
            actual_outcome: "pass".to_string(),
            passed: true,
            score: None,
        };
        let json = serde_json::to_string(&case).unwrap();
        let back: EvalCase = serde_json::from_str(&json).unwrap();
        assert_eq!(case, back);
    }

    #[test]
    fn test_evaluation_report_serializes_roundtrip() {
        let report = EvaluationReport {
            suite_id: "s1".to_string(),
            cases: vec![],
            total: 0,
            passed: 0,
            failed: 0,
            score: None,
        };
        let json = serde_json::to_string(&report).unwrap();
        let back: EvaluationReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, back);
    }

    #[test]
    fn test_compute_run_score() {
        let cases = vec![
            EvalCase {
                passed: true,
                ..Default::default()
            },
            EvalCase {
                passed: false,
                ..Default::default()
            },
        ];
        let score = EvaluationRunner::compute_run_score(&cases);
        assert_eq!(score.item_count, 2);
        assert_eq!(score.passed_count, 1);
        assert_eq!(score.failed_count, 1);
        assert!((score.aggregate_score - 0.5).abs() < f64::EPSILON);
    }
}
