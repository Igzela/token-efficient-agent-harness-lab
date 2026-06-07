use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OrchestrationResult {
    pub run_id: String,
    pub task_count: usize,
    pub completed: usize,
    pub failed: usize,
    pub blocked: usize,
    pub status: String,
    pub items: Vec<serde_json::Value>,
}

pub struct Stage1Orchestrator;

impl Default for Stage1Orchestrator {
    fn default() -> Self {
        Self
    }
}

impl Stage1Orchestrator {
    pub fn new() -> Self {
        Self
    }

    pub fn orchestrate(&self, run_id: &str, tasks: &[serde_json::Value]) -> OrchestrationResult {
        let mut completed = 0;
        let mut failed = 0;
        let mut items = Vec::new();

        for task in tasks {
            let task_id = task
                .get("task_id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let status = task
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("pending");
            match status {
                "completed" => completed += 1,
                "failed" => failed += 1,
                _ => {}
            }
            items.push(json!({"task_id": task_id, "status": status}));
        }

        let blocked = tasks.len() - completed - failed;
        let overall = if failed > 0 {
            "partial_failure"
        } else if completed == tasks.len() {
            "completed"
        } else {
            "in_progress"
        };

        OrchestrationResult {
            run_id: run_id.to_string(),
            task_count: tasks.len(),
            completed,
            failed,
            blocked,
            status: overall.to_string(),
            items,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orchestrate_empty() {
        let r = Stage1Orchestrator::new().orchestrate("r1", &[]);
        assert_eq!(r.status, "completed");
    }

    #[test]
    fn orchestrate_all_done() {
        let tasks = vec![json!({"task_id": "t1", "status": "completed"})];
        let r = Stage1Orchestrator::new().orchestrate("r1", &tasks);
        assert_eq!(r.status, "completed");
        assert_eq!(r.completed, 1);
    }

    #[test]
    fn orchestrate_with_failure() {
        let tasks = vec![
            json!({"task_id": "t1", "status": "completed"}),
            json!({"task_id": "t2", "status": "failed"}),
        ];
        let r = Stage1Orchestrator::new().orchestrate("r1", &tasks);
        assert_eq!(r.status, "partial_failure");
        assert_eq!(r.failed, 1);
    }

    #[test]
    fn orchestrate_in_progress() {
        let tasks = vec![json!({"task_id": "t1", "status": "pending"})];
        let r = Stage1Orchestrator::new().orchestrate("r1", &tasks);
        assert_eq!(r.status, "in_progress");
    }

    #[test]
    fn result_serializes() {
        let v = serde_json::to_value(Stage1Orchestrator::new().orchestrate("r1", &[])).unwrap();
        assert_eq!(v["run_id"], "r1");
    }
}
