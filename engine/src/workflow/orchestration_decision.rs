use serde_json::{json, Value};

pub const ORCHESTRATION_DECISION_SCHEMA_VERSION: &str = "orchestration_decision.v1";

#[derive(Debug, Clone, PartialEq)]
pub enum OrchestrationAction {
    ExecuteNode,
    RetryNode,
    GraphMutated,
    RequestApproval,
    RunCompleted,
    RunFailed,
    NoAction,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DecisionConfidence {
    High,
    Medium,
    Low,
}

impl DecisionConfidence {
    pub fn as_str(&self) -> &'static str {
        match self {
            DecisionConfidence::High => "high",
            DecisionConfidence::Medium => "medium",
            DecisionConfidence::Low => "low",
        }
    }

    pub fn from_score(score: f64) -> Self {
        if score >= 0.7 {
            DecisionConfidence::High
        } else if score >= 0.4 {
            DecisionConfidence::Medium
        } else {
            DecisionConfidence::Low
        }
    }
}

#[derive(Debug, Clone)]
pub struct OrchestrationDecision {
    pub decision_id: String,
    pub run_id: String,
    pub node_id: Option<String>,
    pub action: OrchestrationAction,
    pub action_reason: String,
    pub selected_executor: String,
    pub blocked_reason: Option<String>,
    pub confidence: DecisionConfidence,
    pub confidence_score: f64,
    pub input_signals: Value,
    pub created_at: String,
}

impl OrchestrationDecision {
    pub fn to_value(&self) -> Value {
        json!({
            "schema_version": ORCHESTRATION_DECISION_SCHEMA_VERSION,
            "decision_id": self.decision_id,
            "run_id": self.run_id,
            "node_id": self.node_id,
            "action": action_to_string(&self.action),
            "action_reason": self.action_reason,
            "selected_executor": self.selected_executor,
            "blocked_reason": self.blocked_reason,
            "confidence": self.confidence.as_str(),
            "confidence_score": self.confidence_score,
            "input_signals": self.input_signals,
            "created_at": self.created_at,
        })
    }
}

pub fn action_to_string(action: &OrchestrationAction) -> &'static str {
    match action {
        OrchestrationAction::ExecuteNode => "execute_node",
        OrchestrationAction::RetryNode => "retry_node",
        OrchestrationAction::GraphMutated => "graph_mutated",
        OrchestrationAction::RequestApproval => "request_approval",
        OrchestrationAction::RunCompleted => "run_completed",
        OrchestrationAction::RunFailed => "run_failed",
        OrchestrationAction::NoAction => "no_action",
    }
}

pub fn confidence_from_inputs(
    run_status: &str,
    node_status: Option<&str>,
    has_suggested_executor: bool,
    prior_success_rate: Option<f64>,
    blocked_reason: Option<&str>,
) -> (DecisionConfidence, f64) {
    let mut score = 0.8;

    if run_status == "running" && node_status == Some("pending") {
        score += 0.1;
    } else if run_status == "created" {
        score -= 0.2;
    }

    if has_suggested_executor {
        score += 0.1;
    }

    if let Some(rate) = prior_success_rate {
        score += rate * 0.1;
    }

    if blocked_reason.is_some() {
        score -= 0.5;
    }

    let score = score.clamp(0.0, 1.0);
    (DecisionConfidence::from_score(score), score)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decision_to_value() {
        let decision = OrchestrationDecision {
            decision_id: "dec-0001".to_string(),
            run_id: "run-0001".to_string(),
            node_id: Some("n1".to_string()),
            action: OrchestrationAction::ExecuteNode,
            action_reason: "pending node ready".to_string(),
            selected_executor: "noop".to_string(),
            blocked_reason: None,
            confidence: DecisionConfidence::High,
            confidence_score: 0.9,
            input_signals: json!({"run_status": "running"}),
            created_at: "2026-06-07T00:00:00Z".to_string(),
        };
        let value = decision.to_value();
        assert_eq!(
            value["schema_version"],
            ORCHESTRATION_DECISION_SCHEMA_VERSION
        );
        assert_eq!(value["decision_id"], "dec-0001");
        assert_eq!(value["action"], "execute_node");
        assert_eq!(value["confidence"], "high");
        assert_eq!(value["confidence_score"], 0.9);
    }

    #[test]
    fn test_confidence_from_inputs_high() {
        let (conf, score) =
            confidence_from_inputs("running", Some("pending"), true, Some(0.9), None);
        assert_eq!(conf, DecisionConfidence::High);
        assert!(score >= 0.7);
    }

    #[test]
    fn test_confidence_from_inputs_low_when_blocked() {
        let (conf, score) = confidence_from_inputs(
            "created",
            None,
            false,
            None,
            Some("max_ticks_per_run reached"),
        );
        assert_eq!(conf, DecisionConfidence::Low);
        assert!(score < 0.5);
    }

    #[test]
    fn test_confidence_from_inputs_medium() {
        let (conf, score) = confidence_from_inputs("created", Some("pending"), false, None, None);
        assert_eq!(conf, DecisionConfidence::Medium);
        assert!((0.3..0.7).contains(&score));
    }

    #[test]
    fn test_action_to_string() {
        assert_eq!(
            action_to_string(&OrchestrationAction::ExecuteNode),
            "execute_node"
        );
        assert_eq!(
            action_to_string(&OrchestrationAction::RetryNode),
            "retry_node"
        );
        assert_eq!(
            action_to_string(&OrchestrationAction::GraphMutated),
            "graph_mutated"
        );
        assert_eq!(
            action_to_string(&OrchestrationAction::RequestApproval),
            "request_approval"
        );
        assert_eq!(
            action_to_string(&OrchestrationAction::RunCompleted),
            "run_completed"
        );
        assert_eq!(
            action_to_string(&OrchestrationAction::RunFailed),
            "run_failed"
        );
        assert_eq!(
            action_to_string(&OrchestrationAction::NoAction),
            "no_action"
        );
    }

    #[test]
    fn test_decision_confidence_from_score() {
        assert_eq!(
            DecisionConfidence::from_score(1.0),
            DecisionConfidence::High
        );
        assert_eq!(
            DecisionConfidence::from_score(0.7),
            DecisionConfidence::High
        );
        assert_eq!(
            DecisionConfidence::from_score(0.5),
            DecisionConfidence::Medium
        );
        assert_eq!(DecisionConfidence::from_score(0.3), DecisionConfidence::Low);
        assert_eq!(DecisionConfidence::from_score(0.0), DecisionConfidence::Low);
    }
}
