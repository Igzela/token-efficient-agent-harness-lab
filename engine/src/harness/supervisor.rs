use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WorkerHealth {
    pub worker_id: String,
    pub task_id: String,
    pub status: String,
    pub last_heartbeat: i64,
    pub started_at: i64,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ComponentHealth {
    pub component_id: String,
    pub status: String,
    pub message: String,
    pub checked_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SupervisorReport {
    pub checked_at: i64,
    pub healthy: bool,
    pub stuck_workers: Vec<WorkerHealth>,
    pub crashed_workers: Vec<WorkerHealth>,
    pub component_health: Vec<ComponentHealth>,
}

pub struct RuntimeSupervisor {
    heartbeat_timeout: i64,
}

impl Default for RuntimeSupervisor {
    fn default() -> Self {
        Self {
            heartbeat_timeout: 300,
        }
    }
}

impl RuntimeSupervisor {
    pub fn new(heartbeat_timeout: i64) -> Self {
        Self { heartbeat_timeout }
    }

    pub fn assess_workers(&self, workers: &[WorkerHealth], now: i64) -> SupervisorReport {
        let stuck: Vec<_> = workers
            .iter()
            .filter(|w| w.status == "running" && now - w.last_heartbeat > self.heartbeat_timeout)
            .cloned()
            .collect();
        let crashed: Vec<_> = workers
            .iter()
            .filter(|w| w.status == "crashed" || w.status == "failed")
            .cloned()
            .collect();

        let status = if !crashed.is_empty() {
            "failed"
        } else if !stuck.is_empty() {
            "degraded"
        } else {
            "healthy"
        };
        let message = if !crashed.is_empty() {
            format!("{} crashed worker(s)", crashed.len())
        } else if !stuck.is_empty() {
            format!("{} stuck worker(s)", stuck.len())
        } else {
            "all supplied workers healthy".to_string()
        };

        SupervisorReport {
            checked_at: now,
            healthy: stuck.is_empty() && crashed.is_empty(),
            stuck_workers: stuck,
            crashed_workers: crashed,
            component_health: vec![ComponentHealth {
                component_id: "runtime_supervisor".into(),
                status: status.into(),
                message,
                checked_at: now,
            }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worker(id: &str, status: &str, heartbeat: i64) -> WorkerHealth {
        WorkerHealth {
            worker_id: id.into(),
            task_id: format!("task-{}", id),
            status: status.into(),
            last_heartbeat: heartbeat,
            started_at: 0,
            error: None,
        }
    }

    #[test]
    fn all_healthy() {
        let w = vec![worker("w1", "running", 100)];
        let r = RuntimeSupervisor::new(300).assess_workers(&w, 200);
        assert!(r.healthy);
        assert!(r.stuck_workers.is_empty());
    }

    #[test]
    fn stuck_worker() {
        let w = vec![worker("w1", "running", 100)];
        let r = RuntimeSupervisor::new(300).assess_workers(&w, 500);
        assert!(!r.healthy);
        assert_eq!(r.stuck_workers.len(), 1);
    }

    #[test]
    fn crashed_worker() {
        let w = vec![worker("w1", "crashed", 100)];
        let r = RuntimeSupervisor::new(300).assess_workers(&w, 200);
        assert!(!r.healthy);
        assert_eq!(r.crashed_workers.len(), 1);
    }

    #[test]
    fn report_has_component_health() {
        let r = RuntimeSupervisor::new(300).assess_workers(&[], 0);
        assert_eq!(r.component_health.len(), 1);
        assert_eq!(r.component_health[0].status, "healthy");
    }

    #[test]
    fn report_serializes() {
        let v = serde_json::to_value(&RuntimeSupervisor::new(300).assess_workers(&[], 0)).unwrap();
        assert_eq!(v["healthy"], true);
    }
}
