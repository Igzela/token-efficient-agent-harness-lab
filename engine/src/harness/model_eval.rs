use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ModelEvalCase {
    pub case_id: String,
    pub expected_outcome: String,
    pub actual_outcome: String,
    pub passed: bool,
    pub score_delta: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ModelEvalReport {
    pub suite_id: String,
    pub cases: Vec<ModelEvalCase>,
    pub pass_count: usize,
    pub fail_count: usize,
    pub recommendation: String,
}

pub struct ControlledModelEvalHarness;

impl Default for ControlledModelEvalHarness {
    fn default() -> Self {
        Self
    }
}

impl ControlledModelEvalHarness {
    pub fn new() -> Self {
        Self
    }

    pub fn evaluate(&self, suite_id: &str, cases: Vec<ModelEvalCase>) -> ModelEvalReport {
        let pass_count = cases.iter().filter(|c| c.passed).count();
        let fail_count = cases.len() - pass_count;
        let recommendation = if pass_count > fail_count {
            "stub_is_sufficient"
        } else if fail_count > pass_count {
            "real_is_better"
        } else {
            "needs_more_data"
        };
        ModelEvalReport {
            suite_id: suite_id.to_string(),
            cases,
            pass_count,
            fail_count,
            recommendation: recommendation.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_case(id: &str, passed: bool) -> ModelEvalCase {
        ModelEvalCase {
            case_id: id.into(),
            expected_outcome: "ok".into(),
            actual_outcome: if passed { "ok" } else { "fail" }.into(),
            passed,
            score_delta: None,
        }
    }

    #[test]
    fn evaluate_all_pass() {
        let r = ControlledModelEvalHarness::new()
            .evaluate("s1", vec![make_case("c1", true), make_case("c2", true)]);
        assert_eq!(r.pass_count, 2);
        assert_eq!(r.recommendation, "stub_is_sufficient");
    }

    #[test]
    fn evaluate_all_fail() {
        let r = ControlledModelEvalHarness::new().evaluate("s1", vec![make_case("c1", false)]);
        assert_eq!(r.fail_count, 1);
        assert_eq!(r.recommendation, "real_is_better");
    }

    #[test]
    fn evaluate_mixed() {
        let r = ControlledModelEvalHarness::new()
            .evaluate("s1", vec![make_case("c1", true), make_case("c2", false)]);
        assert_eq!(r.recommendation, "needs_more_data");
    }

    #[test]
    fn report_serializes() {
        let v = serde_json::to_value(&ControlledModelEvalHarness::new().evaluate("s1", vec![]))
            .unwrap();
        assert_eq!(v["suite_id"], "s1");
    }

    #[test]
    fn case_serializes() {
        let v = serde_json::to_value(&make_case("c1", true)).unwrap();
        assert_eq!(v["passed"], true);
    }
}
