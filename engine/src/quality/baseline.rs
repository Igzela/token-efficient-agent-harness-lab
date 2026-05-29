use serde::{Deserialize, Serialize};

use super::evaluation::{EvaluationReport, RunScore};

// ---------------------------------------------------------------------------
// BaselineRecord
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaselineRecord {
    pub baseline_id: String,
    pub timestamp: String,
    pub run_score: RunScore,
    pub evaluation_report: EvaluationReport,
    pub metadata: serde_json::Value,
}

impl Default for BaselineRecord {
    fn default() -> Self {
        Self {
            baseline_id: String::new(),
            timestamp: String::new(),
            run_score: RunScore::default(),
            evaluation_report: EvaluationReport::default(),
            metadata: serde_json::Value::Object(serde_json::Map::new()),
        }
    }
}

// ---------------------------------------------------------------------------
// BaselineComparison
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaselineComparison {
    pub baseline_id: String,
    pub current_run_score: RunScore,
    pub score_delta: f64,
    pub regression_detected: bool,
    pub improved_cases: Vec<String>,
    pub regressed_cases: Vec<String>,
}

impl Default for BaselineComparison {
    fn default() -> Self {
        Self {
            baseline_id: String::new(),
            current_run_score: RunScore::default(),
            score_delta: 0.0,
            regression_detected: false,
            improved_cases: Vec::new(),
            regressed_cases: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// BaselineManager
// ---------------------------------------------------------------------------

pub struct BaselineManager {
    pub baseline_dir: String,
}

impl Default for BaselineManager {
    fn default() -> Self {
        Self {
            baseline_dir: String::new(),
        }
    }
}

impl BaselineManager {
    pub fn new(baseline_dir: &str) -> Self {
        Self {
            baseline_dir: baseline_dir.to_string(),
        }
    }

    pub fn save_baseline(
        &self,
        report: &EvaluationReport,
        score: &RunScore,
        metadata: Option<serde_json::Value>,
    ) -> Result<BaselineRecord, String> {
        let dir = std::path::Path::new(&self.baseline_dir);
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;

        let now = chrono::Utc::now();
        let timestamp = now.format("%Y%m%dT%H%M%SZ").to_string();
        let baseline_id = format!("baseline_{}", timestamp);

        let record = BaselineRecord {
            baseline_id: baseline_id.clone(),
            timestamp,
            run_score: score.clone(),
            evaluation_report: report.clone(),
            metadata: metadata.unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new())),
        };

        let json = serde_json::to_string_pretty(&record).map_err(|e| e.to_string())?;
        let path = dir.join(format!("{}.json", baseline_id));
        std::fs::write(&path, json).map_err(|e| e.to_string())?;

        Ok(record)
    }

    pub fn load_latest_baseline(&self) -> Result<Option<BaselineRecord>, String> {
        let dir = std::path::Path::new(&self.baseline_dir);
        if !dir.exists() {
            return Ok(None);
        }

        let mut entries: Vec<std::path::PathBuf> = dir
            .read_dir()
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("baseline_") && n.ends_with(".json"))
                    .unwrap_or(false)
            })
            .collect();

        entries.sort();

        match entries.last() {
            Some(path) => {
                let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
                let record: BaselineRecord =
                    serde_json::from_str(&content).map_err(|e| e.to_string())?;
                Ok(Some(record))
            }
            None => Ok(None),
        }
    }

    pub fn compare(
        &self,
        current: &EvaluationReport,
        current_score: &RunScore,
    ) -> Result<Option<BaselineComparison>, String> {
        let baseline = match self.load_latest_baseline()? {
            Some(b) => b,
            None => return Ok(None),
        };

        let score_delta = current_score.aggregate_score - baseline.run_score.aggregate_score;

        let mut baseline_outcomes = std::collections::HashMap::new();
        for c in &baseline.evaluation_report.cases {
            baseline_outcomes.insert(&c.case_id, &c.actual_outcome);
        }

        let mut current_outcomes = std::collections::HashMap::new();
        for c in &current.cases {
            current_outcomes.insert(&c.case_id, &c.actual_outcome);
        }

        let mut all_ids: std::collections::HashSet<&String> = std::collections::HashSet::new();
        all_ids.extend(baseline_outcomes.keys());
        all_ids.extend(current_outcomes.keys());

        let mut improved = Vec::new();
        let mut regressed = Vec::new();

        for case_id in &all_ids {
            let base = baseline_outcomes.get(case_id);
            let curr = current_outcomes.get(case_id);
            if base == Some(&&"fail".to_string()) && curr == Some(&&"pass".to_string()) {
                improved.push(case_id.to_string());
            } else if base == Some(&&"pass".to_string()) && curr == Some(&&"fail".to_string()) {
                regressed.push(case_id.to_string());
            }
        }

        improved.sort();
        regressed.sort();

        Ok(Some(BaselineComparison {
            baseline_id: baseline.baseline_id,
            current_run_score: current_score.clone(),
            score_delta: (score_delta * 10000.0).round() / 10000.0,
            regression_detected: !regressed.is_empty(),
            improved_cases: improved,
            regressed_cases: regressed,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quality::evaluation::EvalCase;
    use tempfile::TempDir;

    fn make_run_score(aggregate: f64) -> RunScore {
        RunScore {
            run_id: "test-run".to_string(),
            aggregate_score: aggregate,
            grade: "A".to_string(),
            item_count: 1,
            passed_count: 1,
            failed_count: 0,
        }
    }

    fn make_report(cases: Vec<(&str, &str, bool)>) -> EvaluationReport {
        let eval_cases: Vec<EvalCase> = cases
            .into_iter()
            .map(|(id, outcome, passed)| EvalCase {
                case_id: id.to_string(),
                fixture_path: String::new(),
                expected_outcome: "pass".to_string(),
                actual_outcome: outcome.to_string(),
                passed,
                score: None,
            })
            .collect();
        let passed = eval_cases.iter().filter(|c| c.passed).count();
        EvaluationReport {
            suite_id: "test".to_string(),
            total: eval_cases.len(),
            passed,
            failed: eval_cases.len() - passed,
            cases: eval_cases,
            score: None,
        }
    }

    #[test]
    fn test_baseline_record_default() {
        let record = BaselineRecord::default();
        assert_eq!(record.baseline_id, "");
        assert_eq!(record.run_score.aggregate_score, 0.0);
    }

    #[test]
    fn test_save_and_load_baseline() {
        let tmp = TempDir::new().unwrap();
        let manager = BaselineManager::new(tmp.path().to_str().unwrap());
        let report = make_report(vec![("c1", "pass", true)]);
        let score = make_run_score(0.95);

        let saved = manager.save_baseline(&report, &score, None).unwrap();
        assert!(saved.baseline_id.starts_with("baseline_"));

        let loaded = manager.load_latest_baseline().unwrap().unwrap();
        assert_eq!(loaded.baseline_id, saved.baseline_id);
        assert!((loaded.run_score.aggregate_score - 0.95).abs() < f64::EPSILON);
    }

    #[test]
    fn test_load_latest_baseline_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let manager = BaselineManager::new(tmp.path().to_str().unwrap());
        assert!(manager.load_latest_baseline().unwrap().is_none());
    }

    #[test]
    fn test_compare_improvement() {
        let tmp = TempDir::new().unwrap();
        let manager = BaselineManager::new(tmp.path().to_str().unwrap());

        let old_report = make_report(vec![("c1", "fail", false)]);
        let old_score = make_run_score(0.5);
        manager
            .save_baseline(&old_report, &old_score, None)
            .unwrap();

        let new_report = make_report(vec![("c1", "pass", true)]);
        let new_score = make_run_score(0.95);
        let comparison = manager.compare(&new_report, &new_score).unwrap().unwrap();

        assert!(!comparison.regression_detected);
        assert!(comparison.improved_cases.contains(&"c1".to_string()));
        assert!(comparison.regressed_cases.is_empty());
    }

    #[test]
    fn test_compare_regression() {
        let tmp = TempDir::new().unwrap();
        let manager = BaselineManager::new(tmp.path().to_str().unwrap());

        let old_report = make_report(vec![("c1", "pass", true)]);
        let old_score = make_run_score(0.95);
        manager
            .save_baseline(&old_report, &old_score, None)
            .unwrap();

        let new_report = make_report(vec![("c1", "fail", false)]);
        let new_score = make_run_score(0.5);
        let comparison = manager.compare(&new_report, &new_score).unwrap().unwrap();

        assert!(comparison.regression_detected);
        assert!(comparison.regressed_cases.contains(&"c1".to_string()));
        assert!(comparison.improved_cases.is_empty());
    }

    #[test]
    fn test_compare_no_baseline() {
        let tmp = TempDir::new().unwrap();
        let manager = BaselineManager::new(tmp.path().to_str().unwrap());
        let report = make_report(vec![("c1", "pass", true)]);
        let score = make_run_score(0.95);
        assert!(manager.compare(&report, &score).unwrap().is_none());
    }

    #[test]
    fn test_serialization_roundtrip() {
        let record = BaselineRecord {
            baseline_id: "b1".to_string(),
            timestamp: "20260101T000000Z".to_string(),
            run_score: make_run_score(0.8),
            evaluation_report: make_report(vec![("c1", "pass", true)]),
            metadata: serde_json::json!({"env": "test"}),
        };
        let json = serde_json::to_string(&record).unwrap();
        let back: BaselineRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, back);
    }
}
