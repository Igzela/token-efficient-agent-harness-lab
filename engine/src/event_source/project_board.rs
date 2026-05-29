use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const PROJECT_STATUSES: &[&str] = &[
    "todo", "ready", "running", "blocked", "review", "done", "failed",
];

static LEGAL_TRANSITIONS: LazyLock<HashMap<&'static str, Vec<&'static str>>> =
    LazyLock::new(|| {
        let mut m = HashMap::new();
        m.insert("todo", vec!["ready"]);
        m.insert("ready", vec!["running"]);
        m.insert("running", vec!["blocked", "review"]);
        m.insert("blocked", vec!["running", "failed"]);
        m.insert("review", vec!["done", "review", "failed"]);
        m.insert("done", vec![]);
        m.insert("failed", vec![]);
        m
    });

// ---------------------------------------------------------------------------
// Data structs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProjectBoardItem {
    pub item_id: String,
    pub status: String,
    #[serde(default)]
    pub allowed_files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_reason: Option<String>,
}

impl Default for ProjectBoardItem {
    fn default() -> Self {
        Self {
            item_id: String::new(),
            status: "todo".to_string(),
            allowed_files: Vec::new(),
            blocked_reason: None,
            last_reason: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TransitionResult {
    pub item: ProjectBoardItem,
    pub previous_status: String,
    pub new_status: String,
    pub reason: String,
}

impl Default for TransitionResult {
    fn default() -> Self {
        Self {
            item: ProjectBoardItem::default(),
            previous_status: String::new(),
            new_status: String::new(),
            reason: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FinalGateResult {
    pub item: ProjectBoardItem,
    pub decision: String,
    pub reason: String,
}

impl Default for FinalGateResult {
    fn default() -> Self {
        Self {
            item: ProjectBoardItem::default(),
            decision: String::new(),
            reason: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AllowedFilesCheck {
    pub ok: bool,
    #[serde(default)]
    pub missing_files: Vec<String>,
}

impl Default for AllowedFilesCheck {
    fn default() -> Self {
        Self {
            ok: true,
            missing_files: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Validation helper
// ---------------------------------------------------------------------------

fn validate_status(status: &str) -> Result<(), String> {
    if PROJECT_STATUSES.contains(&status) {
        Ok(())
    } else {
        Err(format!("unknown project board status: {}", status))
    }
}

// ---------------------------------------------------------------------------
// Transition logic
// ---------------------------------------------------------------------------

/// Validate and apply one project board status transition.
pub fn transition_item(
    item: &ProjectBoardItem,
    new_status: &str,
    reason: &str,
    blocked_reason: Option<&str>,
) -> Result<TransitionResult, String> {
    validate_status(&item.status)?;
    validate_status(new_status)?;

    let allowed = LEGAL_TRANSITIONS
        .get(item.status.as_str())
        .ok_or_else(|| format!("no transitions defined for status: {}", item.status))?;

    if !allowed.contains(&new_status) {
        return Err(format!(
            "illegal project board transition: {} -> {}",
            item.status, new_status
        ));
    }

    let updated = ProjectBoardItem {
        item_id: item.item_id.clone(),
        status: new_status.to_string(),
        allowed_files: item.allowed_files.clone(),
        blocked_reason: if new_status == "blocked" {
            blocked_reason.map(|s| s.to_string())
        } else {
            None
        },
        last_reason: Some(reason.to_string()),
    };

    Ok(TransitionResult {
        item: updated,
        previous_status: item.status.clone(),
        new_status: new_status.to_string(),
        reason: reason.to_string(),
    })
}

/// Map task completion to review, preserving task completed != item done.
pub fn complete_task_to_review(
    item: &ProjectBoardItem,
    reason: &str,
) -> Result<TransitionResult, String> {
    if item.status != "running" {
        return Err("task completion can only move a running item to review".to_string());
    }
    transition_item(item, "review", reason, None)
}

/// Apply Final Gate decision from review state.
pub fn final_gate(
    item: &ProjectBoardItem,
    decision: &str,
    reason: &str,
) -> Result<FinalGateResult, String> {
    if item.status != "review" {
        return Err("Final Gate can only run for items in review".to_string());
    }
    let result = match decision {
        "pass" => transition_item(item, "done", reason, None)?,
        "pass_with_notes" => transition_item(item, "review", reason, None)?,
        "fail" => transition_item(item, "failed", reason, None)?,
        _ => {
            return Err("Final Gate decision must be pass, pass_with_notes, or fail".to_string());
        }
    };
    Ok(FinalGateResult {
        item: result.item,
        decision: decision.to_string(),
        reason: reason.to_string(),
    })
}

/// Check whether a task's allowed_files covers all planned writes.
pub fn check_allowed_files(
    allowed_files: &[String],
    required_files: &[String],
) -> AllowedFilesCheck {
    let allowed: HashSet<&String> = allowed_files.iter().collect();
    let missing: Vec<String> = required_files
        .iter()
        .filter(|f| !allowed.contains(f))
        .cloned()
        .collect();
    AllowedFilesCheck {
        ok: missing.is_empty(),
        missing_files: missing,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_item(status: &str) -> ProjectBoardItem {
        ProjectBoardItem {
            item_id: "item-1".to_string(),
            status: status.to_string(),
            allowed_files: Vec::new(),
            blocked_reason: None,
            last_reason: None,
        }
    }

    #[test]
    fn todo_to_ready_transition() {
        let item = make_item("todo");
        let result = transition_item(&item, "ready", "sprint start", None).unwrap();
        assert_eq!(result.previous_status, "todo");
        assert_eq!(result.new_status, "ready");
        assert_eq!(result.item.status, "ready");
    }

    #[test]
    fn illegal_transition_rejected() {
        let item = make_item("todo");
        let result = transition_item(&item, "done", "skip ahead", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("illegal"));
    }

    #[test]
    fn complete_task_to_review_from_running() {
        let item = make_item("running");
        let result = complete_task_to_review(&item, "task done").unwrap();
        assert_eq!(result.item.status, "review");
        assert_eq!(result.new_status, "review");
    }

    #[test]
    fn complete_task_to_review_from_non_running_errors() {
        let item = make_item("ready");
        let result = complete_task_to_review(&item, "task done");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("running"));
    }

    #[test]
    fn final_gate_pass_moves_to_done() {
        let item = make_item("review");
        let result = final_gate(&item, "pass", "looks good").unwrap();
        assert_eq!(result.item.status, "done");
        assert_eq!(result.decision, "pass");
    }

    #[test]
    fn final_gate_fail_moves_to_failed() {
        let item = make_item("review");
        let result = final_gate(&item, "fail", "not ready").unwrap();
        assert_eq!(result.item.status, "failed");
    }

    #[test]
    fn final_gate_pass_with_notes_stays_in_review() {
        let item = make_item("review");
        let result = final_gate(&item, "pass_with_notes", "minor fixes").unwrap();
        assert_eq!(result.item.status, "review");
    }

    #[test]
    fn final_gate_invalid_decision() {
        let item = make_item("review");
        let result = final_gate(&item, "maybe", "uncertain");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("pass, pass_with_notes, or fail"));
    }

    #[test]
    fn final_gate_from_non_review_errors() {
        let item = make_item("running");
        let result = final_gate(&item, "pass", "done");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("review"));
    }

    #[test]
    fn check_allowed_files_all_present() {
        let allowed = vec!["a.rs".to_string(), "b.rs".to_string()];
        let required = vec!["a.rs".to_string()];
        let result = check_allowed_files(&allowed, &required);
        assert!(result.ok);
        assert!(result.missing_files.is_empty());
    }

    #[test]
    fn check_allowed_files_missing() {
        let allowed = vec!["a.rs".to_string()];
        let required = vec!["a.rs".to_string(), "c.rs".to_string()];
        let result = check_allowed_files(&allowed, &required);
        assert!(!result.ok);
        assert_eq!(result.missing_files, vec!["c.rs".to_string()]);
    }

    #[test]
    fn blocked_transition_preserves_reason() {
        let item = make_item("running");
        let result = transition_item(
            &item,
            "blocked",
            "waiting on dependency",
            Some("dep not ready"),
        )
        .unwrap();
        assert_eq!(result.item.status, "blocked");
        assert_eq!(
            result.item.blocked_reason,
            Some("dep not ready".to_string())
        );
    }

    #[test]
    fn blocked_to_running_clears_blocked_reason() {
        let item = ProjectBoardItem {
            item_id: "item-1".to_string(),
            status: "blocked".to_string(),
            allowed_files: Vec::new(),
            blocked_reason: Some("dep not ready".to_string()),
            last_reason: None,
        };
        let result = transition_item(&item, "running", "dep resolved", None).unwrap();
        assert_eq!(result.item.status, "running");
        assert!(result.item.blocked_reason.is_none());
    }

    #[test]
    fn unknown_status_rejected() {
        let item = make_item("unknown_status");
        let result = transition_item(&item, "ready", "try", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown project board status"));
    }

    #[test]
    fn done_is_terminal() {
        let item = make_item("done");
        let result = transition_item(&item, "ready", "reopen", None);
        assert!(result.is_err());
    }

    #[test]
    fn failed_is_terminal() {
        let item = make_item("failed");
        let result = transition_item(&item, "running", "retry", None);
        assert!(result.is_err());
    }

    #[test]
    fn full_lifecycle() {
        let item = make_item("todo");
        let r1 = transition_item(&item, "ready", "planned", None).unwrap();
        let r2 = transition_item(&r1.item, "running", "started", None).unwrap();
        let r3 = complete_task_to_review(&r2.item, "completed").unwrap();
        let r4 = final_gate(&r3.item, "pass", "approved").unwrap();
        assert_eq!(r4.item.status, "done");
    }
}
