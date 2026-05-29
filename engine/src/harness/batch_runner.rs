use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RunResult {
    pub run_id: String,
    pub task_id: String,
    pub status: String,
    pub output: Option<String>,
    pub error: Option<String>,
    pub started_at: String,
    pub completed_at: String,
}

pub struct BatchRunner;

impl Default for BatchRunner {
    fn default() -> Self {
        Self
    }
}

impl BatchRunner {
    pub fn new() -> Self {
        Self
    }

    pub fn run_batch(&self, tasks: &[serde_json::Value]) -> Vec<RunResult> {
        let now = chrono::Utc::now().to_rfc3339();
        tasks
            .iter()
            .enumerate()
            .map(|(i, task)| {
                let default_id = format!("task-{}", i);
                let task_id = task
                    .get("task_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&default_id);
                RunResult {
                    run_id: format!("run-{}", i),
                    task_id: task_id.to_string(),
                    status: "completed".to_string(),
                    output: Some(format!("processed {}", task_id)),
                    error: None,
                    started_at: now.clone(),
                    completed_at: now.clone(),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_empty() {
        assert!(BatchRunner::new().run_batch(&[]).is_empty());
    }

    #[test]
    fn run_single() {
        let r = BatchRunner::new().run_batch(&[serde_json::json!({"task_id": "t1"})]);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].task_id, "t1");
    }

    #[test]
    fn run_multiple() {
        let r = BatchRunner::new().run_batch(&[serde_json::json!({}), serde_json::json!({})]);
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn result_has_status() {
        let r = BatchRunner::new().run_batch(&[serde_json::json!({})]);
        assert_eq!(r[0].status, "completed");
    }

    #[test]
    fn result_has_run_id() {
        let r = BatchRunner::new().run_batch(&[serde_json::json!({})]);
        assert!(r[0].run_id.starts_with("run-"));
    }
}
