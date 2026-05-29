use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Canonical failure codes (mirrors Python validators.py)
// ---------------------------------------------------------------------------

pub const CANONICAL_FAILURE_CODES: &[&str] = &[
    "F001_TIMEOUT",
    "F002_BUDGET_EXCEEDED",
    "F003_DEPENDENCY_FAILED",
    "F004_APPROVAL_REJECTED",
    "F005_PROVIDER_UNAVAILABLE",
    "F006_SCOPE_VIOLATION",
    "F007_TEST_FAILURE",
    "F008_FORMAT_ERROR",
    "F009_POLICY_VIOLATION",
    "F010_CANCELLED",
];

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ScoreComponent {
    pub name: String,
    pub weight: f64,
    pub raw_score: f64,
    pub weighted_score: f64,
    pub penalties: Vec<String>,
}

impl Default for ScoreComponent {
    fn default() -> Self {
        Self {
            name: String::new(),
            weight: 0.0,
            raw_score: 0.0,
            weighted_score: 0.0,
            penalties: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ArtifactScore {
    pub artifact_id: String,
    pub existence_ok: bool,
    pub schema_ok: bool,
    pub evidence_refs_ok: bool,
    pub score: f64,
    pub penalties: Vec<String>,
}

impl Default for ArtifactScore {
    fn default() -> Self {
        Self {
            artifact_id: String::new(),
            existence_ok: false,
            schema_ok: false,
            evidence_refs_ok: false,
            score: 0.0,
            penalties: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TaskScore {
    pub task_id: String,
    pub completion_score: f64,
    pub handoff_score: f64,
    pub artifact_score: f64,
    pub run_log_score: f64,
    pub failure_code_penalty: f64,
    pub weighted_score: f64,
    pub grade: String,
    pub penalties: Vec<String>,
}

impl Default for TaskScore {
    fn default() -> Self {
        Self {
            task_id: String::new(),
            completion_score: 0.0,
            handoff_score: 0.0,
            artifact_score: 0.0,
            run_log_score: 0.0,
            failure_code_penalty: 0.0,
            weighted_score: 0.0,
            grade: "F".to_string(),
            penalties: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RunScore {
    pub run_id: String,
    pub task_scores: Vec<TaskScore>,
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
            task_scores: Vec::new(),
            aggregate_score: 0.0,
            grade: "F".to_string(),
            item_count: 0,
            passed_count: 0,
            failed_count: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn clamp(value: f64, lo: f64, hi: f64) -> f64 {
    value.max(lo).min(hi)
}

pub fn grade(score: f64) -> &'static str {
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
}

// ---------------------------------------------------------------------------
// ScoringEngine
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct ScoringEngine;

impl ScoringEngine {
    pub fn new() -> Self {
        Self
    }

    /// Score a task from its JSON bundle data (task_spec, completion, handoff_pack).
    pub fn score_task(
        &self,
        task_spec: &Value,
        completion: &Value,
        handoff_pack: &Value,
        run_log_text: Option<&str>,
    ) -> TaskScore {
        let mut penalties: Vec<String> = Vec::new();

        let completion_score = self.score_completion_inner(completion, &mut penalties);
        let handoff_score = self.score_handoff_inner(handoff_pack, &mut penalties);
        let artifact_score_val =
            self.score_artifacts_inner(completion, handoff_pack, &mut penalties);
        let run_log_score = self.score_run_log_inner(run_log_text, &mut penalties);
        let failure_penalty = self.score_failure_code_inner(task_spec, completion, &mut penalties);

        let raw = 0.25 * completion_score
            + 0.20 * handoff_score
            + 0.25 * artifact_score_val
            + 0.10 * run_log_score;
        let weighted = (clamp(raw + failure_penalty, 0.0, 1.0) * 10000.0).round() / 10000.0;

        let task_id = task_spec
            .get("task_id")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>")
            .to_string();

        TaskScore {
            task_id,
            completion_score,
            handoff_score,
            artifact_score: artifact_score_val,
            run_log_score,
            failure_code_penalty: failure_penalty,
            weighted_score: weighted,
            grade: grade(weighted).to_string(),
            penalties,
        }
    }

    /// Score a single artifact reference.
    pub fn score_artifact(
        &self,
        artifact_ref: &Value,
        completion: &Value,
        handoff_pack: &Value,
        artifact_exists: bool,
    ) -> ArtifactScore {
        let mut penalties: Vec<String> = Vec::new();
        let artifact_path = artifact_ref
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("");
        let artifact_id = artifact_ref
            .get("artifact_id")
            .and_then(Value::as_str)
            .unwrap_or(artifact_path)
            .to_string();

        let existence_ok = !artifact_path.is_empty() && artifact_exists;
        if !existence_ok {
            penalties.push(format!("artifact not found: {}", artifact_path));
        }

        let has_completion =
            !completion.is_null() && completion.as_object().map_or(false, |m| !m.is_empty());
        let has_handoff =
            !handoff_pack.is_null() && handoff_pack.as_object().map_or(false, |m| !m.is_empty());
        let schema_ok = has_completion && has_handoff;
        if !schema_ok {
            penalties.push("completion or handoff_pack missing".to_string());
        }

        let evidence_refs_ok =
            if let Some(refs) = handoff_pack.get("evidence_refs").and_then(Value::as_array) {
                !refs.is_empty()
                    && refs.iter().all(|r| {
                        r.is_object()
                            && r.get("path")
                                .and_then(Value::as_str)
                                .map_or(false, |p| !p.is_empty())
                    })
            } else {
                false
            };
        if !evidence_refs_ok {
            penalties.push("evidence_refs invalid or empty".to_string());
        }

        let sub_scores = [
            if existence_ok { 0.40 } else { 0.0 },
            if schema_ok { 0.30 } else { 0.0 },
            if evidence_refs_ok { 0.30 } else { 0.0 },
        ];
        let score = clamp(sub_scores.iter().sum::<f64>(), 0.0, 1.0);

        ArtifactScore {
            artifact_id,
            existence_ok,
            schema_ok,
            evidence_refs_ok,
            score,
            penalties,
        }
    }

    /// Aggregate task scores into a run score.
    pub fn score_run(&self, task_scores: &[TaskScore]) -> RunScore {
        if task_scores.is_empty() {
            return RunScore {
                run_id: "<empty>".to_string(),
                task_scores: Vec::new(),
                aggregate_score: 0.0,
                grade: "F".to_string(),
                item_count: 0,
                passed_count: 0,
                failed_count: 0,
            };
        }

        let sum: f64 = task_scores.iter().map(|ts| ts.weighted_score).sum();
        let aggregate = clamp(sum / task_scores.len() as f64, 0.0, 1.0);
        let passed = task_scores
            .iter()
            .filter(|ts| ts.weighted_score >= 0.60)
            .count();
        let failed = task_scores.len() - passed;

        RunScore {
            run_id: "<run>".to_string(),
            task_scores: task_scores.to_vec(),
            aggregate_score: aggregate,
            grade: grade(aggregate).to_string(),
            item_count: task_scores.len(),
            passed_count: passed,
            failed_count: failed,
        }
    }

    // -- private scoring sub-methods --

    fn score_completion_inner(&self, completion: &Value, penalties: &mut Vec<String>) -> f64 {
        if completion.is_null() || completion.as_object().map_or(false, |m| m.is_empty()) {
            penalties.push("completion.json missing or empty".to_string());
            return 0.0;
        }

        let status = completion.get("status").and_then(Value::as_str);
        let exit_code = completion.get("exit_code").and_then(Value::as_i64);

        if status == Some("completed") && exit_code == Some(0) {
            return 1.0;
        }
        if status == Some("completed") {
            penalties.push(format!(
                "completion exit_code={}, expected 0",
                exit_code.unwrap_or(-1)
            ));
            return 0.3;
        }

        penalties.push(format!(
            "completion status={}, expected completed",
            status.unwrap_or("<missing>")
        ));
        0.0
    }

    fn score_handoff_inner(&self, handoff_pack: &Value, penalties: &mut Vec<String>) -> f64 {
        if handoff_pack.is_null() || handoff_pack.as_object().map_or(false, |m| m.is_empty()) {
            penalties.push("handoff_pack.json missing or empty".to_string());
            return 0.0;
        }

        let mut score = 1.0_f64;
        for field_name in &["structured_fields", "summary", "evidence_refs"] {
            let present = handoff_pack.get(*field_name).map_or(false, |v| {
                !v.is_null() && {
                    if let Some(arr) = v.as_array() {
                        !arr.is_empty()
                    } else if let Some(s) = v.as_str() {
                        !s.is_empty()
                    } else if let Some(obj) = v.as_object() {
                        !obj.is_empty()
                    } else {
                        true
                    }
                }
            });
            if !present {
                penalties.push(format!("handoff_pack.{} missing", field_name));
                score -= 0.34;
            }
        }
        clamp(score, 0.0, 1.0)
    }

    fn score_artifacts_inner(
        &self,
        completion: &Value,
        handoff_pack: &Value,
        penalties: &mut Vec<String>,
    ) -> f64 {
        let artifact_refs = completion.get("artifact_refs").and_then(Value::as_array);

        match artifact_refs {
            Some(refs) if !refs.is_empty() => {
                let scores: Vec<f64> = refs
                    .iter()
                    .map(|r| {
                        self.score_artifact(r, completion, handoff_pack, false)
                            .score
                    })
                    .collect();
                let avg = if scores.is_empty() {
                    0.0
                } else {
                    scores.iter().sum::<f64>() / scores.len() as f64
                };
                clamp(avg, 0.0, 1.0)
            }
            _ => {
                penalties.push("no artifact_refs in completion".to_string());
                0.0
            }
        }
    }

    fn score_run_log_inner(&self, run_log_text: Option<&str>, penalties: &mut Vec<String>) -> f64 {
        match run_log_text {
            None => {
                penalties.push("run_log.md missing".to_string());
                0.0
            }
            Some(text) if text.trim().len() < 20 => {
                penalties.push("run_log.md very short".to_string());
                0.5
            }
            _ => 1.0,
        }
    }

    fn score_failure_code_inner(
        &self,
        task_spec: &Value,
        completion: &Value,
        penalties: &mut Vec<String>,
    ) -> f64 {
        let failure_code = task_spec
            .get("failure_code")
            .and_then(Value::as_str)
            .or_else(|| completion.get("failure_code").and_then(Value::as_str));

        if let Some(code) = failure_code {
            if CANONICAL_FAILURE_CODES.contains(&code) {
                penalties.push(format!("canonical failure_code: {}", code));
                return -0.20;
            }
        }
        0.0
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn engine() -> ScoringEngine {
        ScoringEngine::new()
    }

    #[test]
    fn test_clamp_within_range() {
        assert_eq!(clamp(0.5, 0.0, 1.0), 0.5);
    }

    #[test]
    fn test_clamp_below() {
        assert_eq!(clamp(-0.5, 0.0, 1.0), 0.0);
    }

    #[test]
    fn test_clamp_above() {
        assert_eq!(clamp(1.5, 0.0, 1.0), 1.0);
    }

    #[test]
    fn test_grade_thresholds() {
        assert_eq!(grade(0.95), "A");
        assert_eq!(grade(0.90), "A");
        assert_eq!(grade(0.89), "B");
        assert_eq!(grade(0.75), "B");
        assert_eq!(grade(0.74), "C");
        assert_eq!(grade(0.60), "C");
        assert_eq!(grade(0.59), "D");
        assert_eq!(grade(0.40), "D");
        assert_eq!(grade(0.39), "F");
        assert_eq!(grade(0.0), "F");
    }

    #[test]
    fn test_score_task_perfect() {
        let task_spec = json!({"task_id": "T1"});
        let completion = json!({"status": "completed", "exit_code": 0, "artifact_refs": []});
        let handoff_pack = json!({
            "structured_fields": {"k": "v"},
            "summary": "done",
            "evidence_refs": [{"path": "evidence.md"}]
        });
        let score = engine().score_task(
            &task_spec,
            &completion,
            &handoff_pack,
            Some("A sufficiently long run log text here."),
        );
        assert_eq!(score.task_id, "T1");
        assert_eq!(score.completion_score, 1.0);
        assert_eq!(score.handoff_score, 1.0);
        assert_eq!(score.run_log_score, 1.0);
        assert_eq!(score.failure_code_penalty, 0.0);
        assert!(score.weighted_score > 0.0);
        assert!(
            score.penalties.is_empty() || score.penalties.iter().all(|p| p.contains("artifact"))
        );
    }

    #[test]
    fn test_score_task_missing_completion() {
        let task_spec = json!({"task_id": "T2"});
        let completion = json!({});
        let handoff_pack = json!({});
        let score = engine().score_task(&task_spec, &completion, &handoff_pack, None);
        assert_eq!(score.completion_score, 0.0);
        assert_eq!(score.handoff_score, 0.0);
        assert_eq!(score.run_log_score, 0.0);
        assert!(score.penalties.iter().any(|p| p.contains("completion")));
        assert!(score.penalties.iter().any(|p| p.contains("handoff_pack")));
        assert!(score.penalties.iter().any(|p| p.contains("run_log")));
    }

    #[test]
    fn test_score_task_with_canonical_failure() {
        let task_spec = json!({"task_id": "T3", "failure_code": "F001_TIMEOUT"});
        let completion = json!({"status": "completed", "exit_code": 0, "artifact_refs": []});
        let handoff_pack = json!({
            "structured_fields": {"k": "v"},
            "summary": "done",
            "evidence_refs": [{"path": "e.md"}]
        });
        let score = engine().score_task(
            &task_spec,
            &completion,
            &handoff_pack,
            Some("Long enough run log content here."),
        );
        assert_eq!(score.failure_code_penalty, -0.20);
        assert!(score.penalties.iter().any(|p| p.contains("F001_TIMEOUT")));
    }

    #[test]
    fn test_score_artifact_all_ok() {
        let artifact_ref = json!({"path": "output.txt", "artifact_id": "A1"});
        let completion = json!({"status": "completed"});
        let handoff_pack = json!({"evidence_refs": [{"path": "e.md"}]});
        let score = engine().score_artifact(&artifact_ref, &completion, &handoff_pack, true);
        assert_eq!(score.artifact_id, "A1");
        assert!(score.existence_ok);
        assert!(score.schema_ok);
        assert!(score.evidence_refs_ok);
        assert_eq!(score.score, 1.0);
        assert!(score.penalties.is_empty());
    }

    #[test]
    fn test_score_artifact_not_found() {
        let artifact_ref = json!({"path": "missing.txt"});
        let completion = json!({"status": "completed"});
        let handoff_pack = json!({"evidence_refs": [{"path": "e.md"}]});
        let score = engine().score_artifact(&artifact_ref, &completion, &handoff_pack, false);
        assert!(!score.existence_ok);
        assert_eq!(score.score, 0.60); // 0.0 + 0.30 + 0.30
        assert!(score.penalties.iter().any(|p| p.contains("not found")));
    }

    #[test]
    fn test_score_run_empty() {
        let run = engine().score_run(&[]);
        assert_eq!(run.run_id, "<empty>");
        assert_eq!(run.item_count, 0);
        assert_eq!(run.grade, "F");
        assert_eq!(run.aggregate_score, 0.0);
    }

    #[test]
    fn test_score_run_mixed() {
        let good = TaskScore {
            task_id: "T1".to_string(),
            weighted_score: 0.90,
            grade: "A".to_string(),
            ..Default::default()
        };
        let bad = TaskScore {
            task_id: "T2".to_string(),
            weighted_score: 0.30,
            grade: "F".to_string(),
            ..Default::default()
        };
        let run = engine().score_run(&[good, bad]);
        assert_eq!(run.item_count, 2);
        assert_eq!(run.passed_count, 1);
        assert_eq!(run.failed_count, 1);
        assert_eq!(run.aggregate_score, 0.60);
        assert_eq!(run.grade, "C");
    }

    #[test]
    fn test_score_run_log_short() {
        let task_spec = json!({"task_id": "T4"});
        let completion = json!({"status": "completed", "exit_code": 0, "artifact_refs": []});
        let handoff_pack = json!({
            "structured_fields": {"k": "v"},
            "summary": "done",
            "evidence_refs": [{"path": "e.md"}]
        });
        let score = engine().score_task(&task_spec, &completion, &handoff_pack, Some("short"));
        assert_eq!(score.run_log_score, 0.5);
        assert!(score.penalties.iter().any(|p| p.contains("very short")));
    }

    #[test]
    fn test_task_score_serializes_roundtrip() {
        let ts = TaskScore {
            task_id: "T1".to_string(),
            weighted_score: 0.85,
            grade: "B".to_string(),
            ..Default::default()
        };
        let json_str = serde_json::to_string(&ts).unwrap();
        let deserialized: TaskScore = serde_json::from_str(&json_str).unwrap();
        assert_eq!(ts, deserialized);
    }
}
