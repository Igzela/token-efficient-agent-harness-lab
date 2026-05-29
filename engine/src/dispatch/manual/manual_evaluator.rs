use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MANUAL_EVAL_SCHEMA_VERSION: &str = "manual_eval.v1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ManualEvalCheck {
    pub check_id: String,
    pub check_type: String,
    pub passed: bool,
    pub message: String,
    pub evidence: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ManualEvalResult {
    pub schema_version: String,
    pub eval_id: String,
    pub submission_id: String,
    pub dispatch_id: String,
    pub checks: Vec<ManualEvalCheck>,
    pub overall_passed: bool,
    pub score: f64,
    pub evaluator: String,
    pub evaluated_at: String,
}

pub struct ManualEvaluator;

impl Default for ManualEvaluator {
    fn default() -> Self {
        Self
    }
}

impl ManualEvaluator {
    pub fn new() -> Self {
        Self
    }

    pub fn evaluate(
        &self,
        submission_id: &str,
        dispatch_id: &str,
        _raw_output: &str,
        checks: Vec<ManualEvalCheck>,
        evaluator: &str,
    ) -> ManualEvalResult {
        let overall_passed = checks.iter().all(|c| c.passed);
        let passed_count = checks.iter().filter(|c| c.passed).count();
        let score = if checks.is_empty() {
            0.0
        } else {
            passed_count as f64 / checks.len() as f64
        };
        ManualEvalResult {
            schema_version: MANUAL_EVAL_SCHEMA_VERSION.to_string(),
            eval_id: format!(
                "eval-{}",
                &Uuid::new_v4().to_string().replace('-', "")[..12]
            ),
            submission_id: submission_id.to_string(),
            dispatch_id: dispatch_id.to_string(),
            checks,
            overall_passed,
            score,
            evaluator: evaluator.to_string(),
            evaluated_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluate_all_pass() {
        let eval = ManualEvaluator::new();
        let checks = vec![ManualEvalCheck {
            check_id: "c1".into(),
            check_type: "format".into(),
            passed: true,
            message: "ok".into(),
            evidence: None,
        }];
        let r = eval.evaluate("s1", "d1", "output", checks, "human");
        assert!(r.overall_passed);
        assert!((r.score - 1.0).abs() < 0.001);
    }

    #[test]
    fn evaluate_partial_fail() {
        let eval = ManualEvaluator::new();
        let checks = vec![
            ManualEvalCheck {
                check_id: "c1".into(),
                check_type: "f".into(),
                passed: true,
                message: "ok".into(),
                evidence: None,
            },
            ManualEvalCheck {
                check_id: "c2".into(),
                check_type: "f".into(),
                passed: false,
                message: "bad".into(),
                evidence: None,
            },
        ];
        let r = eval.evaluate("s1", "d1", "out", checks, "h");
        assert!(!r.overall_passed);
        assert!((r.score - 0.5).abs() < 0.001);
    }

    #[test]
    fn evaluate_empty_checks() {
        let r = ManualEvaluator::new().evaluate("s1", "d1", "out", vec![], "h");
        assert!(r.overall_passed);
        assert_eq!(r.score, 0.0);
    }

    #[test]
    fn eval_result_serializes() {
        let r = ManualEvaluator::new().evaluate("s1", "d1", "out", vec![], "h");
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["schema_version"], MANUAL_EVAL_SCHEMA_VERSION);
    }

    #[test]
    fn eval_id_unique() {
        let r1 = ManualEvaluator::new().evaluate("s1", "d1", "o", vec![], "h");
        let r2 = ManualEvaluator::new().evaluate("s2", "d2", "o", vec![], "h");
        assert_ne!(r1.eval_id, r2.eval_id);
    }
}
