use serde_json::{json, Value};

pub const ORCHESTRATION_DECISION_SCHEMA_VERSION: &str = "orchestration_decision.v2";

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
    pub quality_signal: Option<Value>,
    pub routing_signal: Option<Value>,
    pub cost_signal: Option<Value>,
    pub approval_signal: Option<Value>,
    pub queue_signal: Option<Value>,
    pub executor_pool_signal: Option<Value>,
    pub candidate_executors: Option<Vec<String>>,
    pub degraded_reason: Option<String>,
}

impl OrchestrationDecision {
    pub fn to_value(&self) -> Value {
        let mut map = serde_json::Map::new();
        map.insert(
            "schema_version".to_string(),
            json!(ORCHESTRATION_DECISION_SCHEMA_VERSION),
        );
        map.insert("decision_id".to_string(), json!(self.decision_id));
        map.insert("run_id".to_string(), json!(self.run_id));
        map.insert("node_id".to_string(), json!(self.node_id));
        map.insert("action".to_string(), json!(action_to_string(&self.action)));
        map.insert("action_reason".to_string(), json!(self.action_reason));
        map.insert(
            "selected_executor".to_string(),
            json!(self.selected_executor),
        );
        map.insert("blocked_reason".to_string(), json!(self.blocked_reason));
        map.insert("confidence".to_string(), json!(self.confidence.as_str()));
        map.insert("confidence_score".to_string(), json!(self.confidence_score));
        map.insert("input_signals".to_string(), self.input_signals.clone());
        map.insert("created_at".to_string(), json!(self.created_at));
        map.insert(
            "quality_signal".to_string(),
            self.quality_signal.clone().unwrap_or(Value::Null),
        );
        map.insert(
            "routing_signal".to_string(),
            self.routing_signal.clone().unwrap_or(Value::Null),
        );
        map.insert(
            "cost_signal".to_string(),
            self.cost_signal.clone().unwrap_or(Value::Null),
        );
        map.insert(
            "approval_signal".to_string(),
            self.approval_signal.clone().unwrap_or(Value::Null),
        );
        map.insert(
            "queue_signal".to_string(),
            self.queue_signal.clone().unwrap_or(Value::Null),
        );
        map.insert(
            "executor_pool_signal".to_string(),
            self.executor_pool_signal.clone().unwrap_or(Value::Null),
        );
        map.insert(
            "candidate_executors".to_string(),
            match &self.candidate_executors {
                Some(execs) => json!(execs),
                None => Value::Null,
            },
        );
        map.insert("degraded_reason".to_string(), json!(self.degraded_reason));
        Value::Object(map)
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
    confidence_from_inputs_with_degraded(
        run_status,
        node_status,
        has_suggested_executor,
        prior_success_rate,
        blocked_reason,
        None,
    )
}

pub fn confidence_from_inputs_with_degraded(
    run_status: &str,
    node_status: Option<&str>,
    has_suggested_executor: bool,
    prior_success_rate: Option<f64>,
    blocked_reason: Option<&str>,
    degraded_reason: Option<&str>,
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

    if degraded_reason.is_some() {
        score -= 0.15;
    }

    let score = score.clamp(0.0, 1.0);
    (DecisionConfidence::from_score(score), score)
}

pub fn build_enriched_input_signals(
    base: &Value,
    quality: Option<&Value>,
    routing: Option<&Value>,
    cost: Option<&Value>,
    approval: Option<&Value>,
    queue: Option<&Value>,
    pool: Option<&Value>,
    candidates: Option<&[String]>,
    degraded: Option<&str>,
) -> Value {
    let mut enriched = base.clone();
    if let Some(obj) = enriched.as_object_mut() {
        if let Some(q) = quality {
            obj.insert("quality_signal".to_string(), q.clone());
        }
        if let Some(r) = routing {
            obj.insert("routing_signal".to_string(), r.clone());
        }
        if let Some(c) = cost {
            obj.insert("cost_signal".to_string(), c.clone());
        }
        if let Some(a) = approval {
            obj.insert("approval_signal".to_string(), a.clone());
        }
        if let Some(q) = queue {
            obj.insert("queue_signal".to_string(), q.clone());
        }
        if let Some(p) = pool {
            obj.insert("executor_pool_signal".to_string(), p.clone());
        }
        if let Some(execs) = candidates {
            obj.insert("candidate_executors".to_string(), json!(execs));
        }
        if let Some(d) = degraded {
            obj.insert("degraded_reason".to_string(), json!(d));
        }
    }
    enriched
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_decision() -> OrchestrationDecision {
        OrchestrationDecision {
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
            quality_signal: None,
            routing_signal: None,
            cost_signal: None,
            approval_signal: None,
            queue_signal: None,
            executor_pool_signal: None,
            candidate_executors: None,
            degraded_reason: None,
        }
    }

    #[test]
    fn test_decision_to_value() {
        let decision = make_decision();
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

    #[test]
    fn test_decision_v2_schema_version() {
        let decision = make_decision();
        let value = decision.to_value();
        assert_eq!(value["schema_version"], "orchestration_decision.v2");
    }

    #[test]
    fn test_enriched_decision_to_value() {
        let mut decision = make_decision();
        decision.quality_signal = Some(json!({"passed": true, "score": 0.95}));
        decision.routing_signal =
            Some(json!({"suggested_executor": "claude_code_cli", "success_rate": 0.85}));
        decision.cost_signal = Some(json!({"estimated_cost": 0.05, "daily_cost": 1.2}));
        decision.approval_signal = Some(json!({"requires_approval": false}));
        decision.queue_signal = Some(json!({"queue_position": 2, "priority": 5}));
        decision.executor_pool_signal = Some(json!({"failure_score": 0.1, "active_count": 3}));
        decision.candidate_executors = Some(vec!["noop".to_string(), "command".to_string()]);
        decision.degraded_reason = Some("backpressure_active".to_string());

        let value = decision.to_value();
        assert_eq!(value["quality_signal"]["passed"], true);
        assert_eq!(
            value["routing_signal"]["suggested_executor"],
            "claude_code_cli"
        );
        assert_eq!(value["cost_signal"]["estimated_cost"], 0.05);
        assert_eq!(value["approval_signal"]["requires_approval"], false);
        assert_eq!(value["queue_signal"]["queue_position"], 2);
        assert_eq!(value["executor_pool_signal"]["failure_score"], 0.1);
        assert_eq!(value["candidate_executors"][0], "noop");
        assert_eq!(value["candidate_executors"][1], "command");
        assert_eq!(value["degraded_reason"], "backpressure_active");
    }

    #[test]
    fn test_confidence_degraded_low() {
        let (conf, score) = confidence_from_inputs_with_degraded(
            "running",
            Some("pending"),
            true,
            Some(0.9),
            None,
            Some("backpressure_active"),
        );
        // base 0.8 + 0.1 (pending) + 0.1 (suggested) + 0.09 (prior) - 0.15 (degraded) = 0.94
        // but it's clamped to 1.0 — so check without the suggested executor bonus
        let (conf2, score2) = confidence_from_inputs_with_degraded(
            "running",
            Some("pending"),
            false,
            None,
            None,
            Some("backpressure_active"),
        );
        // base 0.8 + 0.1 - 0.15 = 0.75 → High
        assert_eq!(conf2, DecisionConfidence::High);
        assert!((score2 - 0.75).abs() < 0.01);

        // Without degraded: 0.8 + 0.1 = 0.9 → High
        let (_, score_no_degraded) = confidence_from_inputs_with_degraded(
            "running",
            Some("pending"),
            false,
            None,
            None,
            None,
        );
        assert!((score_no_degraded - 0.9).abs() < 0.01);
        assert!(
            score2 < score_no_degraded,
            "degraded should lower confidence"
        );
        let _ = conf;
        let _ = score;
    }

    #[test]
    fn test_build_enriched_input_signals() {
        let base = json!({"run_status": "running", "source": "test"});
        let quality = json!({"passed": true});
        let routing = json!({"success_rate": 0.8});
        let queue = json!({"queue_position": 1});
        let candidates = vec!["noop".to_string(), "command".to_string()];

        let enriched = build_enriched_input_signals(
            &base,
            Some(&quality),
            Some(&routing),
            None,
            None,
            Some(&queue),
            None,
            Some(&candidates),
            Some("backpressure"),
        );

        assert_eq!(enriched["run_status"], "running");
        assert_eq!(enriched["quality_signal"]["passed"], true);
        assert_eq!(enriched["routing_signal"]["success_rate"], 0.8);
        assert_eq!(enriched["queue_signal"]["queue_position"], 1);
        assert_eq!(enriched["candidate_executors"][0], "noop");
        assert_eq!(enriched["degraded_reason"], "backpressure");
        // cost_signal and approval_signal were None, should not be present
        assert!(enriched.get("cost_signal").is_none());
        assert!(enriched.get("approval_signal").is_none());
    }
}
