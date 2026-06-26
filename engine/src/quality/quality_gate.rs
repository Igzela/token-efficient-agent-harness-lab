use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::quality::final_gate::FinalGateDecision;
use crate::quality::scoring::TaskScore;

// ---------------------------------------------------------------------------
// Local type stubs for artifact_gate / trajectory (placeholder modules)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ArtifactCheck {
    pub name: String,
    pub passed: bool,
    pub message: String,
}

impl Default for ArtifactCheck {
    fn default() -> Self {
        Self {
            name: String::new(),
            passed: false,
            message: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ArtifactGateResult {
    pub ok: bool,
    pub checks: Vec<ArtifactCheck>,
    pub missing_artifacts: Vec<String>,
    pub schema_violations: Vec<String>,
    pub forbidden_violations: Vec<String>,
}

impl Default for ArtifactGateResult {
    fn default() -> Self {
        Self {
            ok: true,
            checks: Vec::new(),
            missing_artifacts: Vec::new(),
            schema_violations: Vec::new(),
            forbidden_violations: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TrajectoryAnomaly {
    pub anomaly_type: String,
    pub item_id: Option<String>,
    pub event_ids: Vec<String>,
    pub message: String,
    pub severity: String,
}

impl Default for TrajectoryAnomaly {
    fn default() -> Self {
        Self {
            anomaly_type: String::new(),
            item_id: None,
            event_ids: Vec::new(),
            message: String::new(),
            severity: "info".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TrajectoryReport {
    pub ok: bool,
    pub anomalies: Vec<TrajectoryAnomaly>,
    pub retry_count: u64,
    pub loop_detected: bool,
    pub missing_handoff_count: u64,
}

impl Default for TrajectoryReport {
    fn default() -> Self {
        Self {
            ok: true,
            anomalies: Vec::new(),
            retry_count: 0,
            loop_detected: false,
            missing_handoff_count: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// QualityGateDecision
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct QualityGateDecision {
    pub result: String,
    pub reasons: Vec<String>,
    pub next_project_status: String,
    pub score: Option<TaskScore>,
    pub artifact_result: Option<ArtifactGateResult>,
    pub trajectory_result: Option<TrajectoryReport>,
}

impl Default for QualityGateDecision {
    fn default() -> Self {
        Self {
            result: "fail_terminal".to_string(),
            reasons: Vec::new(),
            next_project_status: "failed".to_string(),
            score: None,
            artifact_result: None,
            trajectory_result: None,
        }
    }
}

// ---------------------------------------------------------------------------
// QualityGateManager
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct QualityGateManager;

impl QualityGateManager {
    pub fn new() -> Self {
        Self
    }

    pub fn evaluate(
        &self,
        handoff_pack: &Value,
        completion: &Value,
        final_gate: &FinalGateDecision,
        artifact_result: &ArtifactGateResult,
        trajectory_report: Option<&TrajectoryReport>,
        task_score: Option<&TaskScore>,
    ) -> QualityGateDecision {
        let mut reasons: Vec<String> = Vec::new();

        // Pending approval requires human review
        if has_pending_approval(handoff_pack) {
            reasons.push("approval_request is pending; requires human review".to_string());
            return QualityGateDecision {
                result: "requires_human_review".to_string(),
                reasons,
                next_project_status: "blocked".to_string(),
                score: task_score.cloned(),
                artifact_result: Some(artifact_result.clone()),
                trajectory_result: trajectory_report.cloned(),
            };
        }

        // Check trajectory for error-level anomalies
        if let Some(tr) = trajectory_report {
            if !tr.ok {
                let error_anomalies: Vec<_> = tr
                    .anomalies
                    .iter()
                    .filter(|a| a.severity == "error")
                    .collect();
                if !error_anomalies.is_empty() {
                    reasons.push(format!(
                        "trajectory anomalies detected: {} error(s)",
                        error_anomalies.len()
                    ));
                    for a in error_anomalies.iter().take(3) {
                        reasons.push(format!("  {}: {}", a.anomaly_type, a.message));
                    }
                }
            }
        }

        // Final Gate pass path
        if final_gate.result == "pass" || final_gate.result == "pass_with_notes" {
            let score_value = task_score.map(|s| s.weighted_score).unwrap_or(1.0);

            if !artifact_result.ok {
                let blocking = artifact_result.checks.iter().filter(|c| !c.passed).count();
                reasons.push(format!("artifact gate failed: {} check(s)", blocking));
            }

            if score_value >= 0.75 && artifact_result.ok && reasons.is_empty() {
                reasons.extend(final_gate.reasons.iter().cloned());
                return QualityGateDecision {
                    result: "pass".to_string(),
                    reasons,
                    next_project_status: "done".to_string(),
                    score: task_score.cloned(),
                    artifact_result: Some(artifact_result.clone()),
                    trajectory_result: trajectory_report.cloned(),
                };
            }

            if score_value >= 0.60 {
                reasons.extend(final_gate.reasons.iter().cloned());
                if !artifact_result.ok {
                    reasons.push("artifact gate has warnings but score is acceptable".to_string());
                }
                return QualityGateDecision {
                    result: "pass_with_notes".to_string(),
                    reasons,
                    next_project_status: "done".to_string(),
                    score: task_score.cloned(),
                    artifact_result: Some(artifact_result.clone()),
                    trajectory_result: trajectory_report.cloned(),
                };
            }

            reasons.push(format!("score {:.2} below 0.60 threshold", score_value));
            return QualityGateDecision {
                result: "fail_terminal".to_string(),
                reasons,
                next_project_status: "failed".to_string(),
                score: task_score.cloned(),
                artifact_result: Some(artifact_result.clone()),
                trajectory_result: trajectory_report.cloned(),
            };
        }

        // Final Gate fail path
        let retry_count = completion
            .get("retry_count")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let score_value = task_score.map(|s| s.weighted_score).unwrap_or(0.0);

        if score_value >= 0.40 && retry_count < 3 {
            reasons.extend(final_gate.reasons.iter().cloned());
            reasons.push(format!(
                "score {:.2} >= 0.40 and retry_count={} < 3",
                score_value, retry_count
            ));
            return QualityGateDecision {
                result: "fail_retryable".to_string(),
                reasons,
                next_project_status: "ready".to_string(),
                score: task_score.cloned(),
                artifact_result: Some(artifact_result.clone()),
                trajectory_result: trajectory_report.cloned(),
            };
        }

        reasons.extend(final_gate.reasons.iter().cloned());
        if retry_count >= 3 {
            reasons.push(format!("retry_count={} >= 3", retry_count));
        }
        if score_value < 0.40 {
            reasons.push(format!("score {:.2} < 0.40", score_value));
        }
        QualityGateDecision {
            result: "fail_terminal".to_string(),
            reasons,
            next_project_status: "failed".to_string(),
            score: task_score.cloned(),
            artifact_result: Some(artifact_result.clone()),
            trajectory_result: trajectory_report.cloned(),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn has_pending_approval(value: &Value) -> bool {
    walk_for_pending_approval(value)
}

fn walk_for_pending_approval(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            if map.get("approval_id").map_or(false, |v| !v.is_null())
                && map.get("decision").and_then(Value::as_str) == Some("pending")
            {
                return true;
            }
            map.values().any(walk_for_pending_approval)
        }
        Value::Array(arr) => arr.iter().any(walk_for_pending_approval),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn manager() -> QualityGateManager {
        QualityGateManager::new()
    }

    fn pass_final_gate() -> FinalGateDecision {
        FinalGateDecision {
            result: "pass".to_string(),
            next_project_status: "done".to_string(),
            reasons: vec!["ok".to_string()],
            evidence_refs: Vec::new(),
        }
    }

    fn fail_final_gate() -> FinalGateDecision {
        FinalGateDecision {
            result: "fail".to_string(),
            next_project_status: "review".to_string(),
            reasons: vec!["failed".to_string()],
            evidence_refs: Vec::new(),
        }
    }

    fn ok_artifact_result() -> ArtifactGateResult {
        ArtifactGateResult {
            ok: true,
            checks: vec![ArtifactCheck {
                name: "test".to_string(),
                passed: true,
                message: "ok".to_string(),
            }],
            ..Default::default()
        }
    }

    #[allow(dead_code)]
    fn failed_artifact_result() -> ArtifactGateResult {
        ArtifactGateResult {
            ok: false,
            checks: vec![ArtifactCheck {
                name: "test".to_string(),
                passed: false,
                message: "fail".to_string(),
            }],
            ..Default::default()
        }
    }

    #[test]
    fn test_pass_high_score() {
        let score = TaskScore {
            weighted_score: 0.85,
            ..Default::default()
        };
        let decision = manager().evaluate(
            &json!({}),
            &json!({}),
            &pass_final_gate(),
            &ok_artifact_result(),
            None,
            Some(&score),
        );
        assert_eq!(decision.result, "pass");
        assert_eq!(decision.next_project_status, "done");
    }

    #[test]
    fn test_pass_with_notes_medium_score() {
        let score = TaskScore {
            weighted_score: 0.65,
            ..Default::default()
        };
        let decision = manager().evaluate(
            &json!({}),
            &json!({}),
            &pass_final_gate(),
            &ok_artifact_result(),
            None,
            Some(&score),
        );
        assert_eq!(decision.result, "pass_with_notes");
        assert_eq!(decision.next_project_status, "done");
    }

    #[test]
    fn test_fail_terminal_low_score_pass_gate() {
        let score = TaskScore {
            weighted_score: 0.30,
            ..Default::default()
        };
        let decision = manager().evaluate(
            &json!({}),
            &json!({}),
            &pass_final_gate(),
            &ok_artifact_result(),
            None,
            Some(&score),
        );
        assert_eq!(decision.result, "fail_terminal");
        assert_eq!(decision.next_project_status, "failed");
        assert!(decision.reasons.iter().any(|r| r.contains("below 0.60")));
    }

    #[test]
    fn test_fail_retryable() {
        let score = TaskScore {
            weighted_score: 0.50,
            ..Default::default()
        };
        let decision = manager().evaluate(
            &json!({}),
            &json!({"retry_count": 1}),
            &fail_final_gate(),
            &ok_artifact_result(),
            None,
            Some(&score),
        );
        assert_eq!(decision.result, "fail_retryable");
        assert_eq!(decision.next_project_status, "ready");
    }

    #[test]
    fn test_fail_terminal_high_retry_count() {
        let score = TaskScore {
            weighted_score: 0.50,
            ..Default::default()
        };
        let decision = manager().evaluate(
            &json!({}),
            &json!({"retry_count": 3}),
            &fail_final_gate(),
            &ok_artifact_result(),
            None,
            Some(&score),
        );
        assert_eq!(decision.result, "fail_terminal");
        assert!(decision
            .reasons
            .iter()
            .any(|r| r.contains("retry_count=3 >= 3")));
    }

    #[test]
    fn test_requires_human_review_pending_approval() {
        let handoff = json!({
            "approval_id": "A1",
            "decision": "pending"
        });
        let decision = manager().evaluate(
            &handoff,
            &json!({}),
            &pass_final_gate(),
            &ok_artifact_result(),
            None,
            None,
        );
        assert_eq!(decision.result, "requires_human_review");
        assert_eq!(decision.next_project_status, "blocked");
    }

    #[test]
    fn test_trajectory_error_anomalies() {
        let report = TrajectoryReport {
            ok: false,
            anomalies: vec![TrajectoryAnomaly {
                anomaly_type: "repeated_failure".to_string(),
                item_id: Some("T1".to_string()),
                event_ids: vec!["e1".to_string()],
                message: "item T1 failed 3 times".to_string(),
                severity: "error".to_string(),
            }],
            ..Default::default()
        };
        let score = TaskScore {
            weighted_score: 0.85,
            ..Default::default()
        };
        let decision = manager().evaluate(
            &json!({}),
            &json!({}),
            &pass_final_gate(),
            &ok_artifact_result(),
            Some(&report),
            Some(&score),
        );
        assert!(decision
            .reasons
            .iter()
            .any(|r| r.contains("trajectory anomalies")));
    }

    #[test]
    fn test_walk_pending_approval_nested() {
        let value = json!({
            "outer": {
                "inner": [
                    {"approval_id": "A1", "decision": "approved"},
                    {"approval_id": "A2", "decision": "pending"}
                ]
            }
        });
        assert!(has_pending_approval(&value));
    }

    #[test]
    fn test_walk_no_pending_approval() {
        let value = json!({
            "approval_id": "A1",
            "decision": "approved"
        });
        assert!(!has_pending_approval(&value));
    }

    #[test]
    fn test_decision_default() {
        let d = QualityGateDecision::default();
        assert_eq!(d.result, "fail_terminal");
        assert_eq!(d.next_project_status, "failed");
        assert!(d.score.is_none());
    }

    #[test]
    fn test_decision_serializes_roundtrip() {
        let d = QualityGateDecision {
            result: "pass".to_string(),
            reasons: vec!["ok".to_string()],
            next_project_status: "done".to_string(),
            score: None,
            artifact_result: None,
            trajectory_result: None,
        };
        let json_str = serde_json::to_string(&d).unwrap();
        let deserialized: QualityGateDecision = serde_json::from_str(&json_str).unwrap();
        assert_eq!(d, deserialized);
    }
}
