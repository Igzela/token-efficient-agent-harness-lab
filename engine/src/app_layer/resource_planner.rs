use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlanningTask {
    pub task_id: String,
    pub task_type: String,
    pub description: String,
    pub repo_id: Option<String>,
    pub risk_level: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlannedStep {
    pub step_id: String,
    pub action: String,
    pub role: String,
    pub reason: String,
    pub context_mode: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ResourcePlan {
    pub schema_version: String,
    pub plan_id: String,
    pub task: PlanningTask,
    pub steps: Vec<PlannedStep>,
    pub approval_gates: Vec<String>,
    pub estimated_tokens: i64,
    pub created_at: String,
}

pub struct DeterministicResourcePlanner;

impl Default for DeterministicResourcePlanner {
    fn default() -> Self { Self }
}

impl DeterministicResourcePlanner {
    pub fn new() -> Self { Self }

    pub fn plan(&self, task: &PlanningTask) -> ResourcePlan {
        let mut steps = Vec::new();
        let mut gates = Vec::new();

        steps.push(PlannedStep {
            step_id: "step-1".to_string(),
            action: "analyze".to_string(),
            role: "analyzer".to_string(),
            reason: "understand task requirements".to_string(),
            context_mode: "full".to_string(),
        });

        if task.risk_level.as_deref() == Some("high") {
            gates.push("human_approval".to_string());
            steps.push(PlannedStep {
                step_id: "step-2".to_string(),
                action: "review".to_string(),
                role: "reviewer".to_string(),
                reason: "high risk requires review".to_string(),
                context_mode: "summary".to_string(),
            });
        }

        steps.push(PlannedStep {
            step_id: format!("step-{}", steps.len() + 1),
            action: "execute".to_string(),
            role: "executor".to_string(),
            reason: "perform the task".to_string(),
            context_mode: "full".to_string(),
        });

        ResourcePlan {
            schema_version: "resource_plan.v1".to_string(),
            plan_id: format!("plan-{}", task.task_id),
            task: task.clone(),
            steps,
            approval_gates: gates,
            estimated_tokens: 4000,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task() -> PlanningTask {
        PlanningTask {
            task_id: "t1".to_string(),
            task_type: "feature".to_string(),
            description: "implement feature X".to_string(),
            repo_id: Some("repo-1".to_string()),
            risk_level: Some("low".to_string()),
        }
    }

    #[test]
    fn plan_low_risk() {
        let plan = DeterministicResourcePlanner::new().plan(&make_task());
        assert_eq!(plan.steps.len(), 2);
        assert!(plan.approval_gates.is_empty());
    }

    #[test]
    fn plan_high_risk_adds_gate() {
        let mut task = make_task();
        task.risk_level = Some("high".to_string());
        let plan = DeterministicResourcePlanner::new().plan(&task);
        assert_eq!(plan.steps.len(), 3);
        assert_eq!(plan.approval_gates, vec!["human_approval"]);
    }

    #[test]
    fn plan_has_schema_version() {
        let plan = DeterministicResourcePlanner::new().plan(&make_task());
        assert_eq!(plan.schema_version, "resource_plan.v1");
    }

    #[test]
    fn plan_serializes() {
        let plan = DeterministicResourcePlanner::new().plan(&make_task());
        let v = serde_json::to_value(&plan).unwrap();
        assert_eq!(v["plan_id"], format!("plan-{}", make_task().task_id));
    }

    #[test]
    fn plan_estimates_tokens() {
        let plan = DeterministicResourcePlanner::new().plan(&make_task());
        assert!(plan.estimated_tokens > 0);
    }
}
