use std::collections::HashMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use super::project_board::ProjectBoardItem;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const TASK_STATUSES: &[&str] = &[
    "QUEUED",
    "TRIAGED",
    "READY",
    "READY_READONLY",
    "READY_WRITE",
    "RUNNING",
    "WAITING_APPROVAL",
    "PAUSED_BUDGET",
    "WAITING_DEPENDENCY",
    "BLOCKED",
    "BLOCKED_UPSTREAM_FAILED",
    "BLOCKED_APPROVAL",
    "BLOCKED_PROVIDER",
    "COMPLETED",
    "FAILED",
    "CANCELLED_BY_DEPENDENCY",
];

static LEGAL_TASK_TRANSITIONS: LazyLock<HashMap<&'static str, Vec<&'static str>>> =
    LazyLock::new(|| {
        let mut m = HashMap::new();
        m.insert("QUEUED", vec!["TRIAGED", "READY", "RUNNING"]);
        m.insert(
            "TRIAGED",
            vec!["READY", "READY_READONLY", "READY_WRITE", "BLOCKED"],
        );
        m.insert("READY", vec!["RUNNING", "BLOCKED"]);
        m.insert(
            "READY_READONLY",
            vec!["RUNNING", "WAITING_DEPENDENCY", "READY_WRITE"],
        );
        m.insert("READY_WRITE", vec!["RUNNING", "BLOCKED"]);
        m.insert(
            "RUNNING",
            vec![
                "WAITING_APPROVAL",
                "PAUSED_BUDGET",
                "WAITING_DEPENDENCY",
                "BLOCKED",
                "BLOCKED_APPROVAL",
                "BLOCKED_PROVIDER",
                "COMPLETED",
                "FAILED",
            ],
        );
        m.insert(
            "WAITING_APPROVAL",
            vec!["RUNNING", "BLOCKED_APPROVAL", "FAILED"],
        );
        m.insert("PAUSED_BUDGET", vec!["RUNNING", "FAILED"]);
        m.insert(
            "WAITING_DEPENDENCY",
            vec![
                "READY",
                "BLOCKED_UPSTREAM_FAILED",
                "CANCELLED_BY_DEPENDENCY",
            ],
        );
        m.insert("BLOCKED", vec!["RUNNING", "FAILED"]);
        m.insert(
            "BLOCKED_UPSTREAM_FAILED",
            vec!["CANCELLED_BY_DEPENDENCY", "FAILED"],
        );
        m.insert("BLOCKED_APPROVAL", vec!["RUNNING", "FAILED"]);
        m.insert("BLOCKED_PROVIDER", vec!["RUNNING", "FAILED"]);
        m.insert("COMPLETED", vec![]);
        m.insert("FAILED", vec![]);
        m.insert("CANCELLED_BY_DEPENDENCY", vec![]);
        m
    });

static TASK_TO_PROJECT_BOARD: LazyLock<
    HashMap<&'static str, (&'static str, Option<&'static str>)>,
> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("QUEUED", ("ready", None));
    m.insert("TRIAGED", ("ready", None));
    m.insert("READY", ("ready", None));
    m.insert("READY_READONLY", ("ready", None));
    m.insert("READY_WRITE", ("ready", None));
    m.insert("RUNNING", ("running", None));
    m.insert("WAITING_APPROVAL", ("blocked", Some("approval")));
    m.insert("PAUSED_BUDGET", ("blocked", Some("budget")));
    m.insert("WAITING_DEPENDENCY", ("blocked", Some("dependency")));
    m.insert("BLOCKED", ("blocked", Some("generic")));
    m.insert(
        "BLOCKED_UPSTREAM_FAILED",
        ("blocked", Some("upstream_failed")),
    );
    m.insert("BLOCKED_APPROVAL", ("blocked", Some("approval")));
    m.insert("BLOCKED_PROVIDER", ("blocked", Some("provider")));
    m.insert("COMPLETED", ("review", None));
    m.insert("FAILED", ("failed", None));
    m.insert("CANCELLED_BY_DEPENDENCY", ("failed", None));
    m
});

// ---------------------------------------------------------------------------
// Data structs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TaskQueueEntry {
    pub task_id: String,
    pub item_id: String,
    pub status: String,
    pub handoff_id: String,
    pub scheduling_policy: String,
}

impl Default for TaskQueueEntry {
    fn default() -> Self {
        Self {
            task_id: String::new(),
            item_id: String::new(),
            status: "QUEUED".to_string(),
            handoff_id: String::new(),
            scheduling_policy: "sequential".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HandoffResult {
    pub task: TaskQueueEntry,
    pub accepted: bool,
}

impl Default for HandoffResult {
    fn default() -> Self {
        Self {
            task: TaskQueueEntry::default(),
            accepted: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TaskTransitionResult {
    pub task: TaskQueueEntry,
    pub previous_status: String,
    pub new_status: String,
    pub project_board_status: String,
    pub blocked_reason: Option<String>,
}

impl Default for TaskTransitionResult {
    fn default() -> Self {
        Self {
            task: TaskQueueEntry::default(),
            previous_status: String::new(),
            new_status: String::new(),
            project_board_status: String::new(),
            blocked_reason: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

fn validate_task_status(status: &str) -> Result<(), String> {
    if TASK_STATUSES.contains(&status) {
        Ok(())
    } else {
        Err(format!("unknown task queue status: {}", status))
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Accept a ready project item into the sequential task queue.
pub fn receive_handoff(
    item: &ProjectBoardItem,
    handoff_id: &str,
    scheduling_policy: &str,
    task_id: Option<&str>,
) -> Result<HandoffResult, String> {
    if item.status != "ready" {
        return Err("handoff only accepts project board items in ready status".to_string());
    }
    if scheduling_policy != "sequential" {
        return Err("only sequential scheduling is supported in Stage 1 Day 4".to_string());
    }
    if handoff_id.is_empty() {
        return Err("handoff_id is required".to_string());
    }

    let task = TaskQueueEntry {
        task_id: task_id
            .unwrap_or(&format!("task_for_{}", item.item_id))
            .to_string(),
        item_id: item.item_id.clone(),
        status: "QUEUED".to_string(),
        handoff_id: handoff_id.to_string(),
        scheduling_policy: scheduling_policy.to_string(),
    };
    Ok(HandoffResult {
        task,
        accepted: true,
    })
}

/// Validate and apply one task queue status transition.
pub fn transition_task(
    task: &TaskQueueEntry,
    new_status: &str,
) -> Result<TaskTransitionResult, String> {
    validate_task_status(&task.status)?;
    validate_task_status(new_status)?;

    let allowed = LEGAL_TASK_TRANSITIONS
        .get(task.status.as_str())
        .ok_or_else(|| format!("no transitions defined for status: {}", task.status))?;

    if !allowed.contains(&new_status) {
        return Err(format!(
            "illegal task queue transition: {} -> {}",
            task.status, new_status
        ));
    }

    let updated = TaskQueueEntry {
        task_id: task.task_id.clone(),
        item_id: task.item_id.clone(),
        status: new_status.to_string(),
        handoff_id: task.handoff_id.clone(),
        scheduling_policy: task.scheduling_policy.clone(),
    };

    let (board_status, blocked_reason) = map_task_status_to_project_board(new_status)?;

    Ok(TaskTransitionResult {
        task: updated,
        previous_status: task.status.clone(),
        new_status: new_status.to_string(),
        project_board_status: board_status.to_string(),
        blocked_reason: blocked_reason.map(String::from),
    })
}

/// Map Task Queue status to Project Board status and blocked reason.
pub fn map_task_status_to_project_board(
    task_status: &str,
) -> Result<(&'static str, Option<&'static str>), String> {
    validate_task_status(task_status)?;
    Ok(*TASK_TO_PROJECT_BOARD
        .get(task_status)
        .expect("validated but not in map"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(status: &str) -> TaskQueueEntry {
        TaskQueueEntry {
            task_id: "task-1".to_string(),
            item_id: "item-1".to_string(),
            status: status.to_string(),
            handoff_id: "hof-1".to_string(),
            scheduling_policy: "sequential".to_string(),
        }
    }

    fn make_ready_item() -> ProjectBoardItem {
        ProjectBoardItem {
            item_id: "item-1".to_string(),
            status: "ready".to_string(),
            allowed_files: Vec::new(),
            blocked_reason: None,
            last_reason: None,
        }
    }

    #[test]
    fn test_receive_handoff_success() {
        let item = make_ready_item();
        let result = receive_handoff(&item, "hof-1", "sequential", None).unwrap();
        assert!(result.accepted);
        assert_eq!(result.task.status, "QUEUED");
        assert_eq!(result.task.task_id, "task_for_item-1");
    }

    #[test]
    fn test_receive_handoff_custom_task_id() {
        let item = make_ready_item();
        let result = receive_handoff(&item, "hof-1", "sequential", Some("custom-task")).unwrap();
        assert_eq!(result.task.task_id, "custom-task");
    }

    #[test]
    fn test_receive_handoff_non_ready_rejected() {
        let mut item = make_ready_item();
        item.status = "running".to_string();
        let result = receive_handoff(&item, "hof-1", "sequential", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("ready"));
    }

    #[test]
    fn test_receive_handoff_empty_handoff_id() {
        let item = make_ready_item();
        let result = receive_handoff(&item, "", "sequential", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_transition_task_queued_to_ready() {
        let entry = make_entry("QUEUED");
        let result = transition_task(&entry, "READY").unwrap();
        assert_eq!(result.new_status, "READY");
        assert_eq!(result.previous_status, "QUEUED");
        assert_eq!(result.project_board_status, "ready");
    }

    #[test]
    fn test_transition_task_running_to_completed() {
        let entry = make_entry("RUNNING");
        let result = transition_task(&entry, "COMPLETED").unwrap();
        assert_eq!(result.project_board_status, "review");
    }

    #[test]
    fn test_transition_task_illegal() {
        let entry = make_entry("QUEUED");
        let result = transition_task(&entry, "COMPLETED");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("illegal"));
    }

    #[test]
    fn test_transition_task_unknown_status() {
        let entry = make_entry("UNKNOWN");
        let result = transition_task(&entry, "READY");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown"));
    }

    #[test]
    fn test_map_task_status_to_project_board() {
        let (status, reason) = map_task_status_to_project_board("WAITING_APPROVAL").unwrap();
        assert_eq!(status, "blocked");
        assert_eq!(reason, Some("approval"));
    }

    #[test]
    fn test_map_completed_to_review() {
        let (status, reason) = map_task_status_to_project_board("COMPLETED").unwrap();
        assert_eq!(status, "review");
        assert!(reason.is_none());
    }

    #[test]
    fn test_map_failed_to_failed() {
        let (status, _) = map_task_status_to_project_board("FAILED").unwrap();
        assert_eq!(status, "failed");
    }

    #[test]
    fn test_running_to_blocked_with_provider() {
        let entry = make_entry("RUNNING");
        let result = transition_task(&entry, "BLOCKED_PROVIDER").unwrap();
        assert_eq!(result.project_board_status, "blocked");
        assert_eq!(result.blocked_reason.as_deref(), Some("provider"));
    }
}
