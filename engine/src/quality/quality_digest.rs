use serde::{Deserialize, Serialize};

use super::baseline::BaselineComparison;
use super::evaluation::RunScore;

// ---------------------------------------------------------------------------
// QualityDigestItem
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QualityDigestItem {
    pub item_id: String,
    pub status: String,
    pub quality_gate_result: String,
    pub score: f64,
    pub grade: String,
    pub anomalies: Vec<String>,
}

impl Default for QualityDigestItem {
    fn default() -> Self {
        Self {
            item_id: String::new(),
            status: String::new(),
            quality_gate_result: "not_evaluated".to_string(),
            score: 0.0,
            grade: "F".to_string(),
            anomalies: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// QualityDigest
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QualityDigest {
    pub batch_id: String,
    pub items: Vec<QualityDigestItem>,
    pub aggregate_score: f64,
    pub aggregate_grade: String,
    pub trajectory_ok: bool,
    pub baseline_delta: Option<f64>,
    pub summary: String,
    pub recommended_actions: Vec<String>,
}

impl Default for QualityDigest {
    fn default() -> Self {
        Self {
            batch_id: String::new(),
            items: Vec::new(),
            aggregate_score: 0.0,
            aggregate_grade: "F".to_string(),
            trajectory_ok: true,
            baseline_delta: None,
            summary: String::new(),
            recommended_actions: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// QualityDigestGenerator
// ---------------------------------------------------------------------------

pub struct QualityDigestGenerator;

impl Default for QualityDigestGenerator {
    fn default() -> Self {
        Self
    }
}

impl QualityDigestGenerator {
    pub fn new() -> Self {
        Self
    }

    pub fn generate(
        &self,
        batch_id: &str,
        items: Vec<QualityDigestItem>,
        run_score: &RunScore,
        trajectory_ok: bool,
        anomaly_count: usize,
        baseline: Option<&BaselineComparison>,
    ) -> QualityDigest {
        let actions =
            Self::recommend_actions_static(&items, trajectory_ok, anomaly_count, baseline);
        let baseline_delta = baseline.map(|b| b.score_delta);
        let summary = Self::build_summary_static(run_score, trajectory_ok, anomaly_count, baseline);

        QualityDigest {
            batch_id: batch_id.to_string(),
            items,
            aggregate_score: run_score.aggregate_score,
            aggregate_grade: run_score.grade.clone(),
            trajectory_ok,
            baseline_delta,
            summary,
            recommended_actions: actions,
        }
    }

    fn build_summary_static(
        run_score: &RunScore,
        trajectory_ok: bool,
        anomaly_count: usize,
        baseline: Option<&BaselineComparison>,
    ) -> String {
        let mut parts = vec![
            format!(
                "Run: {} items, score {:.2} ({})",
                run_score.item_count, run_score.aggregate_score, run_score.grade
            ),
            format!(
                "Passed: {}, Failed: {}",
                run_score.passed_count, run_score.failed_count
            ),
        ];

        if !trajectory_ok {
            parts.push(format!(
                "Trajectory: {} anomaly/anomalies detected",
                anomaly_count
            ));
        }

        if let Some(b) = baseline {
            let sign = if b.score_delta >= 0.0 { "+" } else { "" };
            parts.push(format!("Baseline delta: {}{:.2}", sign, b.score_delta));
            if b.regression_detected {
                parts.push(format!(
                    "REGRESSION: {} case(s) regressed",
                    b.regressed_cases.len()
                ));
            }
        }

        parts.join("; ")
    }

    fn recommend_actions_static(
        items: &[QualityDigestItem],
        trajectory_ok: bool,
        _anomaly_count: usize,
        baseline: Option<&BaselineComparison>,
    ) -> Vec<String> {
        let mut actions = Vec::new();

        for item in items {
            match item.quality_gate_result.as_str() {
                "fail_retryable" => {
                    actions.push(format!(
                        "Retry item {} (score {:.2})",
                        item.item_id, item.score
                    ));
                }
                "requires_human_review" => {
                    actions.push(format!("Human review required for item {}", item.item_id));
                }
                "fail_terminal" => {
                    actions.push(format!(
                        "Item {} failed terminally; investigate",
                        item.item_id
                    ));
                }
                _ => {}
            }
        }

        if let Some(b) = baseline {
            if b.regression_detected {
                actions.push(format!(
                    "Regression detected in: {}",
                    b.regressed_cases.join(", ")
                ));
            }
        }

        if !trajectory_ok {
            actions.push("Investigate trajectory anomalies before next run".to_string());
        }

        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_item(id: &str, gate_result: &str, score: f64) -> QualityDigestItem {
        QualityDigestItem {
            item_id: id.to_string(),
            status: "active".to_string(),
            quality_gate_result: gate_result.to_string(),
            score,
            grade: "B".to_string(),
            anomalies: Vec::new(),
        }
    }

    fn make_run_score() -> RunScore {
        RunScore {
            run_id: "r1".to_string(),
            aggregate_score: 0.85,
            grade: "B".to_string(),
            item_count: 3,
            passed_count: 2,
            failed_count: 1,
        }
    }

    #[test]
    fn test_quality_digest_item_default() {
        let item = QualityDigestItem::default();
        assert_eq!(item.score, 0.0);
        assert_eq!(item.grade, "F");
        assert_eq!(item.quality_gate_result, "not_evaluated");
    }

    #[test]
    fn test_generate_basic_digest() {
        let gen = QualityDigestGenerator::new();
        let items = vec![
            make_item("item1", "pass", 0.9),
            make_item("item2", "fail_retryable", 0.5),
        ];
        let score = make_run_score();
        let digest = gen.generate("batch1", items, &score, true, 0, None);

        assert_eq!(digest.batch_id, "batch1");
        assert_eq!(digest.items.len(), 2);
        assert!(digest.trajectory_ok);
        assert!(digest.baseline_delta.is_none());
        assert!(!digest.recommended_actions.is_empty());
    }

    #[test]
    fn test_recommend_actions_retryable() {
        let gen = QualityDigestGenerator::new();
        let items = vec![make_item("item1", "fail_retryable", 0.5)];
        let score = make_run_score();
        let digest = gen.generate("b1", items, &score, true, 0, None);

        assert!(digest
            .recommended_actions
            .iter()
            .any(|a| a.contains("Retry item item1")));
    }

    #[test]
    fn test_recommend_actions_terminal() {
        let gen = QualityDigestGenerator::new();
        let items = vec![make_item("item1", "fail_terminal", 0.1)];
        let score = make_run_score();
        let digest = gen.generate("b1", items, &score, true, 0, None);

        assert!(digest
            .recommended_actions
            .iter()
            .any(|a| a.contains("failed terminally")));
    }

    #[test]
    fn test_recommend_actions_human_review() {
        let gen = QualityDigestGenerator::new();
        let items = vec![make_item("item1", "requires_human_review", 0.5)];
        let score = make_run_score();
        let digest = gen.generate("b1", items, &score, true, 0, None);

        assert!(digest
            .recommended_actions
            .iter()
            .any(|a| a.contains("Human review required")));
    }

    #[test]
    fn test_summary_with_baseline_regression() {
        let gen = QualityDigestGenerator::new();
        let items = vec![make_item("item1", "pass", 0.9)];
        let score = make_run_score();
        let baseline = BaselineComparison {
            baseline_id: "b1".to_string(),
            current_run_score: score.clone(),
            score_delta: -0.15,
            regression_detected: true,
            improved_cases: vec![],
            regressed_cases: vec!["c1".to_string()],
        };
        let digest = gen.generate("b1", items, &score, true, 0, Some(&baseline));

        assert!(digest.summary.contains("REGRESSION"));
        assert!(digest.summary.contains("-0.15"));
        assert!(digest.baseline_delta.is_some());
    }

    #[test]
    fn test_serialization_roundtrip() {
        let digest = QualityDigest {
            batch_id: "b1".to_string(),
            items: vec![make_item("i1", "pass", 0.9)],
            aggregate_score: 0.9,
            aggregate_grade: "A".to_string(),
            trajectory_ok: true,
            baseline_delta: None,
            summary: "ok".to_string(),
            recommended_actions: vec![],
        };
        let json = serde_json::to_string(&digest).unwrap();
        let back: QualityDigest = serde_json::from_str(&json).unwrap();
        assert_eq!(digest, back);
    }
}
