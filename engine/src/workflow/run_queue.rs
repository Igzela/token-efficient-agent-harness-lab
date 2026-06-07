// ---------------------------------------------------------------------------
// QueuedRun
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct QueuedRun {
    pub run_id: String,
    pub priority: i32,
    pub created_at: String,
    pub deadline_at: Option<String>,
    pub sla_ms: Option<i64>,
    pub tenant_id: Option<String>,
    pub pause_reason: Option<String>,
    pub queue_position: Option<i32>,
}

// ---------------------------------------------------------------------------
// QueueConfig
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct QueueConfig {
    pub max_concurrent: usize,
    pub max_queued: usize,
    pub priority_ceiling: i32,
    pub priority_floor: i32,
    pub deadline_overdue_threshold_ms: i64,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 4,
            max_queued: 100,
            priority_ceiling: 10,
            priority_floor: 1,
            deadline_overdue_threshold_ms: 60_000,
        }
    }
}

// ---------------------------------------------------------------------------
// QueueStatus
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct QueueStatus {
    pub total_queued: usize,
    pub total_running: usize,
    pub total_paused: usize,
    pub total_completed: u64,
    pub total_failed: u64,
    pub avg_priority: f64,
    pub overdue_count: usize,
    pub capacity_utilization: f64,
    pub queue_depth_ratio: f64,
    pub backpressure_active: bool,
    pub tenant_counts: Vec<TenantQueueInfo>,
}

// ---------------------------------------------------------------------------
// TenantQueueInfo
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct TenantQueueInfo {
    pub tenant_id: String,
    pub run_count: usize,
    pub avg_priority: f64,
}

// ---------------------------------------------------------------------------
// AdmissionResult
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct AdmissionResult {
    pub allowed: bool,
    pub reason: String,
    pub suggested_action: Option<String>,
}

// ---------------------------------------------------------------------------
// DeadlineAction
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct DeadlineAction {
    pub run_id: String,
    pub action: String,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// BackpressureSignal
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct BackpressureSignal {
    pub active: bool,
    pub degrade_mode: Option<String>,
    pub pause_targets: Vec<String>,
    pub priority_ceiling_temp: Option<i32>,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// RunQueue
// ---------------------------------------------------------------------------

pub struct RunQueue {
    config: QueueConfig,
}

impl RunQueue {
    pub fn new(config: QueueConfig) -> Self {
        Self { config }
    }

    pub fn sort_runs(&self, mut runs: Vec<QueuedRun>) -> Vec<QueuedRun> {
        runs.sort_by(|a, b| {
            let a_paused = a.pause_reason.is_some();
            let b_paused = b.pause_reason.is_some();
            match (a_paused, b_paused) {
                (false, true) => return std::cmp::Ordering::Less,
                (true, false) => return std::cmp::Ordering::Greater,
                _ => {}
            }

            let priority_cmp = a.priority.cmp(&b.priority);
            if priority_cmp != std::cmp::Ordering::Equal {
                return priority_cmp;
            }

            let deadline_cmp = compare_deadline(a.deadline_at.as_deref(), b.deadline_at.as_deref());
            if deadline_cmp != std::cmp::Ordering::Equal {
                return deadline_cmp;
            }

            a.created_at.cmp(&b.created_at)
        });
        runs
    }

    pub fn admission_check(&self, queue_status: &QueueStatus) -> AdmissionResult {
        if queue_status.total_queued >= self.config.max_queued {
            return AdmissionResult {
                allowed: false,
                reason: format!(
                    "queue at capacity: {} / {}",
                    queue_status.total_queued, self.config.max_queued
                ),
                suggested_action: Some("wait".to_string()),
            };
        }

        if queue_status.capacity_utilization > 0.9 {
            return AdmissionResult {
                allowed: true,
                reason: format!(
                    "utilization high: {:.1}%",
                    queue_status.capacity_utilization * 100.0
                ),
                suggested_action: Some("throttle".to_string()),
            };
        }

        AdmissionResult {
            allowed: true,
            reason: "within limits".to_string(),
            suggested_action: None,
        }
    }

    pub fn check_deadlines(&self, runs: &[QueuedRun]) -> Vec<DeadlineAction> {
        runs.iter()
            .filter_map(|run| {
                let deadline = run.deadline_at.as_ref()?;
                let deadline_ts = parse_timestamp_ms(deadline)?;
                let now_ms = parse_timestamp_ms(&run.created_at)?;

                // Use SLA if available, otherwise use the overdue threshold
                let effective_deadline = if let Some(sla) = run.sla_ms {
                    now_ms + sla
                } else {
                    deadline_ts
                };

                let overdue_ms = now_ms - effective_deadline;

                if overdue_ms > 0 {
                    Some(DeadlineAction {
                        run_id: run.run_id.clone(),
                        action: "boost".to_string(),
                        reason: format!("overdue by {}ms", overdue_ms),
                    })
                } else if overdue_ms > -self.config.deadline_overdue_threshold_ms {
                    Some(DeadlineAction {
                        run_id: run.run_id.clone(),
                        action: "warn".to_string(),
                        reason: format!("approaching deadline: {}ms remaining", -overdue_ms),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn compute_backpressure(&self, queue_status: &QueueStatus) -> BackpressureSignal {
        let utilization_high = queue_status.capacity_utilization > 0.8;
        let queue_depth_high = queue_status.queue_depth_ratio > 0.8;

        if utilization_high || queue_depth_high {
            let mut reason_parts = Vec::new();
            if utilization_high {
                reason_parts.push(format!(
                    "utilization {:.1}%",
                    queue_status.capacity_utilization * 100.0
                ));
            }
            if queue_depth_high {
                reason_parts.push(format!(
                    "queue depth {:.1}%",
                    queue_status.queue_depth_ratio * 100.0
                ));
            }

            BackpressureSignal {
                active: true,
                degrade_mode: Some("throttle".to_string()),
                pause_targets: Vec::new(),
                priority_ceiling_temp: Some(self.config.priority_ceiling.min(5)),
                reason: reason_parts.join(", "),
            }
        } else {
            BackpressureSignal {
                active: false,
                degrade_mode: None,
                pause_targets: Vec::new(),
                priority_ceiling_temp: None,
                reason: "within limits".to_string(),
            }
        }
    }

    pub fn validate_priority(&self, priority: i32) -> Result<i32, String> {
        if priority < self.config.priority_floor || priority > self.config.priority_ceiling {
            Err(format!(
                "priority {} outside range [{}, {}]",
                priority, self.config.priority_floor, self.config.priority_ceiling
            ))
        } else {
            Ok(priority)
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn compare_deadline(a: Option<&str>, b: Option<&str>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => a.cmp(b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

fn parse_timestamp_ms(ts: &str) -> Option<i64> {
    // Simple ISO-8601-ish parser for "YYYY-MM-DDTHH:MM:SSZ" or epoch ms
    if let Ok(epoch) = ts.parse::<i64>() {
        return Some(epoch);
    }

    // Try parsing as ISO-8601
    let ts = ts.trim_end_matches('Z');
    let parts: Vec<&str> = ts.split('T').collect();
    if parts.len() != 2 {
        return None;
    }

    let date_parts: Vec<&str> = parts[0].split('-').collect();
    if date_parts.len() != 3 {
        return None;
    }

    let year: i64 = date_parts[0].parse().ok()?;
    let month: i64 = date_parts[1].parse().ok()?;
    let day: i64 = date_parts[2].parse().ok()?;

    let time_parts: Vec<&str> = parts[1].split(':').collect();
    if time_parts.len() < 3 {
        return None;
    }

    let hour: i64 = time_parts[0].parse().ok()?;
    let minute: i64 = time_parts[1].parse().ok()?;
    let second: i64 = time_parts[2].split('.').next()?.parse().ok()?;

    // Approximate epoch ms (ignoring leap seconds, timezone)
    let days_since_epoch = (year - 1970) * 365 + (month - 1) * 30 + (day - 1); // Simplified
    let total_seconds = days_since_epoch * 86400 + hour * 3600 + minute * 60 + second;
    Some(total_seconds * 1000)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_run(id: &str, priority: i32, created_at: &str) -> QueuedRun {
        QueuedRun {
            run_id: id.to_string(),
            priority,
            created_at: created_at.to_string(),
            deadline_at: None,
            sla_ms: None,
            tenant_id: None,
            pause_reason: None,
            queue_position: None,
        }
    }

    fn make_run_with_deadline(
        id: &str,
        priority: i32,
        created_at: &str,
        deadline_at: &str,
    ) -> QueuedRun {
        QueuedRun {
            run_id: id.to_string(),
            priority,
            created_at: created_at.to_string(),
            deadline_at: Some(deadline_at.to_string()),
            sla_ms: None,
            tenant_id: None,
            pause_reason: None,
            queue_position: None,
        }
    }

    fn make_run_paused(id: &str, priority: i32, created_at: &str, reason: &str) -> QueuedRun {
        QueuedRun {
            run_id: id.to_string(),
            priority,
            created_at: created_at.to_string(),
            deadline_at: None,
            sla_ms: None,
            tenant_id: None,
            pause_reason: Some(reason.to_string()),
            queue_position: None,
        }
    }

    #[test]
    fn test_sort_by_priority() {
        let queue = RunQueue::new(QueueConfig::default());
        let runs = vec![
            make_run("low", 5, "2026-06-07T10:00:00Z"),
            make_run("high", 1, "2026-06-07T10:00:01Z"),
            make_run("mid", 3, "2026-06-07T10:00:02Z"),
        ];
        let sorted = queue.sort_runs(runs);
        assert_eq!(sorted[0].run_id, "high");
        assert_eq!(sorted[1].run_id, "mid");
        assert_eq!(sorted[2].run_id, "low");
    }

    #[test]
    fn test_sort_paused_runs_last() {
        let queue = RunQueue::new(QueueConfig::default());
        let runs = vec![
            make_run_paused("paused", 1, "2026-06-07T10:00:00Z", "waiting"),
            make_run("active", 5, "2026-06-07T10:00:01Z"),
        ];
        let sorted = queue.sort_runs(runs);
        assert_eq!(sorted[0].run_id, "active");
        assert_eq!(sorted[1].run_id, "paused");
    }

    #[test]
    fn test_sort_by_deadline_when_same_priority() {
        let queue = RunQueue::new(QueueConfig::default());
        let runs = vec![
            make_run_with_deadline("later", 1, "2026-06-07T10:00:00Z", "2026-06-07T12:00:00Z"),
            make_run_with_deadline("sooner", 1, "2026-06-07T10:00:01Z", "2026-06-07T11:00:00Z"),
        ];
        let sorted = queue.sort_runs(runs);
        assert_eq!(sorted[0].run_id, "sooner");
        assert_eq!(sorted[1].run_id, "later");
    }

    #[test]
    fn test_sort_by_created_at_when_same_priority_and_deadline() {
        let queue = RunQueue::new(QueueConfig::default());
        let runs = vec![
            make_run("second", 1, "2026-06-07T10:00:01Z"),
            make_run("first", 1, "2026-06-07T10:00:00Z"),
        ];
        let sorted = queue.sort_runs(runs);
        assert_eq!(sorted[0].run_id, "first");
        assert_eq!(sorted[1].run_id, "second");
    }

    #[test]
    fn test_admission_check_allowed() {
        let queue = RunQueue::new(QueueConfig::default());
        let status = QueueStatus {
            total_queued: 50,
            total_running: 10,
            total_paused: 0,
            total_completed: 100,
            total_failed: 5,
            avg_priority: 3.0,
            overdue_count: 0,
            capacity_utilization: 0.6,
            queue_depth_ratio: 0.5,
            backpressure_active: false,
            tenant_counts: Vec::new(),
        };
        let result = queue.admission_check(&status);
        assert!(result.allowed);
        assert!(result.suggested_action.is_none());
    }

    #[test]
    fn test_admission_check_denied_at_capacity() {
        let queue = RunQueue::new(QueueConfig::default());
        let status = QueueStatus {
            total_queued: 100,
            total_running: 10,
            total_paused: 0,
            total_completed: 100,
            total_failed: 5,
            avg_priority: 3.0,
            overdue_count: 0,
            capacity_utilization: 1.0,
            queue_depth_ratio: 1.0,
            backpressure_active: false,
            tenant_counts: Vec::new(),
        };
        let result = queue.admission_check(&status);
        assert!(!result.allowed);
        assert_eq!(result.suggested_action, Some("wait".to_string()));
    }

    #[test]
    fn test_admission_check_warns_when_high_utilization() {
        let queue = RunQueue::new(QueueConfig::default());
        let status = QueueStatus {
            total_queued: 50,
            total_running: 10,
            total_paused: 0,
            total_completed: 100,
            total_failed: 5,
            avg_priority: 3.0,
            overdue_count: 0,
            capacity_utilization: 0.95,
            queue_depth_ratio: 0.5,
            backpressure_active: false,
            tenant_counts: Vec::new(),
        };
        let result = queue.admission_check(&status);
        assert!(result.allowed);
        assert_eq!(result.suggested_action, Some("throttle".to_string()));
    }

    #[test]
    fn test_compute_backpressure_active_when_utilization_high() {
        let queue = RunQueue::new(QueueConfig::default());
        let status = QueueStatus {
            total_queued: 80,
            total_running: 10,
            total_paused: 0,
            total_completed: 100,
            total_failed: 5,
            avg_priority: 3.0,
            overdue_count: 0,
            capacity_utilization: 0.85,
            queue_depth_ratio: 0.5,
            backpressure_active: false,
            tenant_counts: Vec::new(),
        };
        let signal = queue.compute_backpressure(&status);
        assert!(signal.active);
        assert_eq!(signal.degrade_mode, Some("throttle".to_string()));
    }

    #[test]
    fn test_compute_backpressure_active_when_queue_depth_high() {
        let queue = RunQueue::new(QueueConfig::default());
        let status = QueueStatus {
            total_queued: 80,
            total_running: 10,
            total_paused: 0,
            total_completed: 100,
            total_failed: 5,
            avg_priority: 3.0,
            overdue_count: 0,
            capacity_utilization: 0.5,
            queue_depth_ratio: 0.85,
            backpressure_active: false,
            tenant_counts: Vec::new(),
        };
        let signal = queue.compute_backpressure(&status);
        assert!(signal.active);
        assert!(signal.reason.contains("queue depth"));
    }

    #[test]
    fn test_compute_backpressure_inactive() {
        let queue = RunQueue::new(QueueConfig::default());
        let status = QueueStatus {
            total_queued: 30,
            total_running: 10,
            total_paused: 0,
            total_completed: 100,
            total_failed: 5,
            avg_priority: 3.0,
            overdue_count: 0,
            capacity_utilization: 0.4,
            queue_depth_ratio: 0.3,
            backpressure_active: false,
            tenant_counts: Vec::new(),
        };
        let signal = queue.compute_backpressure(&status);
        assert!(!signal.active);
        assert!(signal.degrade_mode.is_none());
    }

    #[test]
    fn test_validate_priority_valid() {
        let queue = RunQueue::new(QueueConfig::default());
        assert_eq!(queue.validate_priority(5), Ok(5));
        assert_eq!(queue.validate_priority(1), Ok(1));
        assert_eq!(queue.validate_priority(10), Ok(10));
    }

    #[test]
    fn test_validate_priority_out_of_range() {
        let queue = RunQueue::new(QueueConfig::default());
        assert!(queue.validate_priority(0).is_err());
        assert!(queue.validate_priority(11).is_err());
        assert!(queue.validate_priority(-1).is_err());
    }

    #[test]
    fn test_queue_config_default() {
        let config = QueueConfig::default();
        assert_eq!(config.max_queued, 100);
        assert_eq!(config.priority_ceiling, 10);
        assert_eq!(config.priority_floor, 1);
        assert_eq!(config.deadline_overdue_threshold_ms, 60_000);
    }

    #[test]
    fn test_check_deadlines_empty() {
        let queue = RunQueue::new(QueueConfig::default());
        let actions = queue.check_deadlines(&[]);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_check_deadlines_no_deadline() {
        let queue = RunQueue::new(QueueConfig::default());
        let runs = vec![make_run("r1", 1, "2026-06-07T10:00:00Z")];
        let actions = queue.check_deadlines(&runs);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_backpressure_signal_priority_ceiling_temp() {
        let queue = RunQueue::new(QueueConfig::default());
        let status = QueueStatus {
            total_queued: 80,
            total_running: 10,
            total_paused: 0,
            total_completed: 100,
            total_failed: 5,
            avg_priority: 3.0,
            overdue_count: 0,
            capacity_utilization: 0.9,
            queue_depth_ratio: 0.9,
            backpressure_active: false,
            tenant_counts: Vec::new(),
        };
        let signal = queue.compute_backpressure(&status);
        assert!(signal.active);
        assert_eq!(signal.priority_ceiling_temp, Some(5));
    }
}
