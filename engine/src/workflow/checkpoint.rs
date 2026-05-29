use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ArtifactRef {
    pub artifact_type: String,
    pub path: String,
    pub sha256: String,
}

impl Default for ArtifactRef {
    fn default() -> Self {
        Self {
            artifact_type: String::new(),
            path: String::new(),
            sha256: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CompensatingEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
    pub reason: String,
}

impl Default for CompensatingEvent {
    fn default() -> Self {
        Self {
            event_type: String::new(),
            payload: serde_json::Value::Object(serde_json::Map::new()),
            reason: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Checkpoint {
    pub checkpoint_id: String,
    pub task_id: String,
    pub node_id: String,
    pub dag_version: i64,
    pub status: String,
    pub current_step: String,
    pub completed_steps: Vec<String>,
    pub pending_steps: Vec<String>,
    pub input_hash: String,
    #[serde(default)]
    pub artifact_refs: Vec<ArtifactRef>,
    #[serde(default)]
    pub model_call_refs: Vec<String>,
    #[serde(default)]
    pub tool_call_refs: Vec<String>,
    #[serde(default = "default_true")]
    pub resumable: bool,
    #[serde(default = "default_resume_strategy")]
    pub resume_strategy: String,
    #[serde(default = "default_created_at")]
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_resume_strategy() -> String {
    "resume_in_same_sandbox".to_string()
}

fn default_created_at() -> String {
    "2026-01-01T00:00:00Z".to_string()
}

impl Default for Checkpoint {
    fn default() -> Self {
        Self {
            checkpoint_id: String::new(),
            task_id: String::new(),
            node_id: String::new(),
            dag_version: 0,
            status: "running".to_string(),
            current_step: String::new(),
            completed_steps: Vec::new(),
            pending_steps: Vec::new(),
            input_hash: String::new(),
            artifact_refs: Vec::new(),
            model_call_refs: Vec::new(),
            tool_call_refs: Vec::new(),
            resumable: true,
            resume_strategy: "resume_in_same_sandbox".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            reason: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RecoveryPlan {
    pub task_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_id: Option<String>,
    pub strategy: String,
    #[serde(default)]
    pub compensating_events: Vec<CompensatingEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resumed_from_step: Option<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl Default for RecoveryPlan {
    fn default() -> Self {
        Self {
            task_id: String::new(),
            checkpoint_id: None,
            strategy: "skip".to_string(),
            compensating_events: Vec::new(),
            resumed_from_step: None,
            warnings: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct IntegrityCheck {
    pub ok: bool,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl Default for IntegrityCheck {
    fn default() -> Self {
        Self {
            ok: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// CheckpointManager
// ---------------------------------------------------------------------------

pub struct CheckpointManager {
    pub store_dir: PathBuf,
}

impl CheckpointManager {
    pub fn new(store_dir: impl Into<PathBuf>) -> Self {
        Self {
            store_dir: store_dir.into(),
        }
    }

    pub fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<(), String> {
        fs::create_dir_all(&self.store_dir)
            .map_err(|e| format!("failed to create store dir: {}", e))?;
        let path = self.path_for(&checkpoint.checkpoint_id)?;
        let json_str = serde_json::to_string_pretty(checkpoint)
            .map_err(|e| format!("serialize error: {}", e))?;
        fs::write(&path, json_str + "\n").map_err(|e| format!("write error: {}", e))
    }

    pub fn load_checkpoint(&self, checkpoint_id: &str) -> Result<Option<Checkpoint>, String> {
        let path = self.path_for(checkpoint_id)?;
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read_to_string(&path).map_err(|e| format!("read error: {}", e))?;
        let checkpoint: Checkpoint =
            serde_json::from_str(&data).map_err(|e| format!("deserialize error: {}", e))?;
        Ok(Some(checkpoint))
    }

    pub fn list_checkpoints(&self, task_id: &str) -> Result<Vec<Checkpoint>, String> {
        if !self.store_dir.exists() {
            return Ok(Vec::new());
        }
        let mut checkpoints = Vec::new();
        let entries: Vec<_> = fs::read_dir(&self.store_dir)
            .map_err(|e| format!("read_dir error: {}", e))?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "json")
                    .unwrap_or(false)
            })
            .collect();

        let mut sorted_entries: Vec<_> = entries.into_iter().collect();
        sorted_entries.sort_by_key(|e| e.path());

        for entry in sorted_entries {
            let data =
                fs::read_to_string(entry.path()).map_err(|e| format!("read error: {}", e))?;
            let checkpoint: Checkpoint =
                serde_json::from_str(&data).map_err(|e| format!("deserialize error: {}", e))?;
            if checkpoint.task_id == task_id {
                checkpoints.push(checkpoint);
            }
        }

        checkpoints.sort_by(|a, b| {
            (&a.created_at, &a.checkpoint_id).cmp(&(&b.created_at, &b.checkpoint_id))
        });
        Ok(checkpoints)
    }

    pub fn latest_checkpoint(&self, task_id: &str) -> Result<Option<Checkpoint>, String> {
        let checkpoints = self.list_checkpoints(task_id)?;
        Ok(checkpoints.into_iter().last())
    }

    pub fn checkpoint_id_for(
        &self,
        task_id: &str,
        node_id: &str,
        dag_version: i64,
        current_step: &str,
        created_at: &str,
    ) -> String {
        let mut payload = serde_json::Map::new();
        payload.insert(
            "created_at".to_string(),
            serde_json::Value::String(created_at.to_string()),
        );
        payload.insert(
            "current_step".to_string(),
            serde_json::Value::String(current_step.to_string()),
        );
        payload.insert(
            "dag_version".to_string(),
            serde_json::Value::Number(serde_json::Number::from(dag_version)),
        );
        payload.insert(
            "node_id".to_string(),
            serde_json::Value::String(node_id.to_string()),
        );
        payload.insert(
            "task_id".to_string(),
            serde_json::Value::String(task_id.to_string()),
        );

        let json_str = serde_json::to_string(&payload).expect("Map should serialize");
        let mut hasher = Sha256::new();
        hasher.update(json_str.as_bytes());
        let digest = hex::encode(hasher.finalize());
        let short_digest = &digest[..16];
        format!("ckpt_{}_{}", task_id, short_digest)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_checkpoint(
        &self,
        task_id: &str,
        node_id: &str,
        dag_version: i64,
        status: &str,
        current_step: &str,
        completed_steps: &[String],
        pending_steps: &[String],
        input_hash: &str,
        created_at: &str,
        artifact_refs: Vec<ArtifactRef>,
        model_call_refs: Vec<String>,
        tool_call_refs: Vec<String>,
        resumable: bool,
        resume_strategy: &str,
        reason: Option<String>,
    ) -> Result<Checkpoint, String> {
        let checkpoint = Checkpoint {
            checkpoint_id: self.checkpoint_id_for(
                task_id,
                node_id,
                dag_version,
                current_step,
                created_at,
            ),
            task_id: task_id.to_string(),
            node_id: node_id.to_string(),
            dag_version,
            status: status.to_string(),
            current_step: current_step.to_string(),
            completed_steps: completed_steps.to_vec(),
            pending_steps: pending_steps.to_vec(),
            input_hash: input_hash.to_string(),
            artifact_refs,
            model_call_refs,
            tool_call_refs,
            resumable,
            resume_strategy: resume_strategy.to_string(),
            created_at: created_at.to_string(),
            reason,
        };
        self.save_checkpoint(&checkpoint)?;
        Ok(checkpoint)
    }

    pub fn create_recovery_plan(&self, task_id: &str) -> Result<RecoveryPlan, String> {
        let checkpoint = self.latest_checkpoint(task_id)?;
        let checkpoint = match checkpoint {
            None => {
                return Ok(RecoveryPlan {
                    task_id: task_id.to_string(),
                    checkpoint_id: None,
                    strategy: "skip".to_string(),
                    warnings: vec!["no checkpoint exists".to_string()],
                    ..RecoveryPlan::default()
                })
            }
            Some(c) => c,
        };

        if checkpoint.status == "running" && checkpoint.resumable {
            return Ok(RecoveryPlan {
                task_id: task_id.to_string(),
                checkpoint_id: Some(checkpoint.checkpoint_id),
                strategy: "resume".to_string(),
                resumed_from_step: Some(checkpoint.current_step),
                ..RecoveryPlan::default()
            });
        }

        if checkpoint.status == "running" {
            return Ok(RecoveryPlan {
                task_id: task_id.to_string(),
                checkpoint_id: Some(checkpoint.checkpoint_id),
                strategy: "restart".to_string(),
                warnings: vec!["checkpoint is not resumable".to_string()],
                ..RecoveryPlan::default()
            });
        }

        if checkpoint.status == "failed" {
            return Ok(RecoveryPlan {
                task_id: task_id.to_string(),
                checkpoint_id: Some(checkpoint.checkpoint_id.clone()),
                strategy: "compensate".to_string(),
                compensating_events: self.generate_compensating_events(&checkpoint),
                ..RecoveryPlan::default()
            });
        }

        Ok(RecoveryPlan {
            task_id: task_id.to_string(),
            checkpoint_id: Some(checkpoint.checkpoint_id),
            strategy: "skip".to_string(),
            warnings: vec![format!("checkpoint status is {}", checkpoint.status)],
            ..RecoveryPlan::default()
        })
    }

    pub fn generate_compensating_events(&self, checkpoint: &Checkpoint) -> Vec<CompensatingEvent> {
        vec![
            CompensatingEvent {
                event_type: "task_cancelled".to_string(),
                payload: serde_json::json!({
                    "checkpoint_id": checkpoint.checkpoint_id,
                    "reason": checkpoint.reason.as_deref().unwrap_or("recovery"),
                    "task_id": checkpoint.task_id,
                }),
                reason: "Cancel failed task by appending a compensating event".to_string(),
            },
            CompensatingEvent {
                event_type: "claim_released".to_string(),
                payload: serde_json::json!({
                    "task_id": checkpoint.task_id,
                }),
                reason: "Release file claims on recovery".to_string(),
            },
        ]
    }

    fn path_for(&self, checkpoint_id: &str) -> Result<PathBuf, String> {
        if checkpoint_id.contains("..")
            || checkpoint_id.contains('/')
            || checkpoint_id.contains('\\')
        {
            return Err(format!(
                "checkpoint_id contains path traversal: {:?}",
                checkpoint_id
            ));
        }
        let store_resolved = self
            .store_dir
            .canonicalize()
            .unwrap_or_else(|_| self.store_dir.clone());
        let candidate = store_resolved.join(format!("{}.json", checkpoint_id));
        let candidate_resolved = candidate
            .canonicalize()
            .unwrap_or_else(|_| candidate.clone());
        if !candidate_resolved.starts_with(&store_resolved) {
            return Err(format!(
                "checkpoint_id contains path traversal: {:?}",
                checkpoint_id
            ));
        }
        Ok(candidate)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_checkpoint(task_id: &str, checkpoint_id: &str, status: &str) -> Checkpoint {
        Checkpoint {
            checkpoint_id: checkpoint_id.to_string(),
            task_id: task_id.to_string(),
            node_id: "node-1".to_string(),
            dag_version: 1,
            status: status.to_string(),
            current_step: "step-2".to_string(),
            completed_steps: vec!["step-1".to_string()],
            pending_steps: vec!["step-2".to_string(), "step-3".to_string()],
            input_hash: "abc123".to_string(),
            created_at: "2026-05-29T00:00:00Z".to_string(),
            ..Checkpoint::default()
        }
    }

    #[test]
    fn test_save_and_load_checkpoint() {
        let tmp = TempDir::new().unwrap();
        let mgr = CheckpointManager::new(tmp.path());
        let cp = make_checkpoint("task-1", "ckpt-task-1-001", "running");
        mgr.save_checkpoint(&cp).unwrap();
        let loaded = mgr.load_checkpoint("ckpt-task-1-001").unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().task_id, "task-1");
    }

    #[test]
    fn test_load_nonexistent_checkpoint() {
        let tmp = TempDir::new().unwrap();
        let mgr = CheckpointManager::new(tmp.path());
        let loaded = mgr.load_checkpoint("ckpt-nonexistent").unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_list_checkpoints_filters_by_task() {
        let tmp = TempDir::new().unwrap();
        let mgr = CheckpointManager::new(tmp.path());
        mgr.save_checkpoint(&make_checkpoint("task-1", "ckpt-a", "running"))
            .unwrap();
        mgr.save_checkpoint(&make_checkpoint("task-1", "ckpt-b", "completed"))
            .unwrap();
        mgr.save_checkpoint(&make_checkpoint("task-2", "ckpt-c", "running"))
            .unwrap();

        let list = mgr.list_checkpoints("task-1").unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.iter().all(|c| c.task_id == "task-1"));
    }

    #[test]
    fn test_latest_checkpoint_returns_last() {
        let tmp = TempDir::new().unwrap();
        let mgr = CheckpointManager::new(tmp.path());

        let mut cp1 = make_checkpoint("task-1", "ckpt-a", "running");
        cp1.created_at = "2026-05-29T00:00:00Z".to_string();
        mgr.save_checkpoint(&cp1).unwrap();

        let mut cp2 = make_checkpoint("task-1", "ckpt-b", "running");
        cp2.created_at = "2026-05-29T01:00:00Z".to_string();
        mgr.save_checkpoint(&cp2).unwrap();

        let latest = mgr.latest_checkpoint("task-1").unwrap();
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().checkpoint_id, "ckpt-b");
    }

    #[test]
    fn test_latest_checkpoint_empty() {
        let tmp = TempDir::new().unwrap();
        let mgr = CheckpointManager::new(tmp.path());
        let latest = mgr.latest_checkpoint("task-1").unwrap();
        assert!(latest.is_none());
    }

    #[test]
    fn test_checkpoint_id_for_deterministic() {
        let tmp = TempDir::new().unwrap();
        let mgr = CheckpointManager::new(tmp.path());
        let id1 = mgr.checkpoint_id_for("t1", "n1", 1, "step-1", "2026-01-01T00:00:00Z");
        let id2 = mgr.checkpoint_id_for("t1", "n1", 1, "step-1", "2026-01-01T00:00:00Z");
        assert_eq!(id1, id2);
        assert!(id1.starts_with("ckpt_t1_"));
    }

    #[test]
    fn test_checkpoint_id_for_different_inputs_differ() {
        let tmp = TempDir::new().unwrap();
        let mgr = CheckpointManager::new(tmp.path());
        let id1 = mgr.checkpoint_id_for("t1", "n1", 1, "step-1", "2026-01-01T00:00:00Z");
        let id2 = mgr.checkpoint_id_for("t1", "n1", 1, "step-2", "2026-01-01T00:00:00Z");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_create_checkpoint_persists() {
        let tmp = TempDir::new().unwrap();
        let mgr = CheckpointManager::new(tmp.path());
        let cp = mgr
            .create_checkpoint(
                "task-1",
                "node-1",
                1,
                "running",
                "step-2",
                &["step-1".to_string()],
                &["step-2".to_string()],
                "input_hash",
                "2026-05-29T00:00:00Z",
                vec![],
                vec![],
                vec![],
                true,
                "resume_in_same_sandbox",
                None,
            )
            .unwrap();
        assert!(cp.checkpoint_id.starts_with("ckpt_task-1_"));
        let loaded = mgr.load_checkpoint(&cp.checkpoint_id).unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().status, "running");
    }

    #[test]
    fn test_recovery_plan_no_checkpoint() {
        let tmp = TempDir::new().unwrap();
        let mgr = CheckpointManager::new(tmp.path());
        let plan = mgr.create_recovery_plan("task-1").unwrap();
        assert_eq!(plan.strategy, "skip");
        assert!(plan.warnings.iter().any(|w| w.contains("no checkpoint")));
    }

    #[test]
    fn test_recovery_plan_resume() {
        let tmp = TempDir::new().unwrap();
        let mgr = CheckpointManager::new(tmp.path());
        let cp = make_checkpoint("task-1", "ckpt-1", "running");
        mgr.save_checkpoint(&cp).unwrap();
        let plan = mgr.create_recovery_plan("task-1").unwrap();
        assert_eq!(plan.strategy, "resume");
        assert_eq!(plan.resumed_from_step.as_deref(), Some("step-2"));
    }

    #[test]
    fn test_recovery_plan_restart_not_resumable() {
        let tmp = TempDir::new().unwrap();
        let mgr = CheckpointManager::new(tmp.path());
        let mut cp = make_checkpoint("task-1", "ckpt-1", "running");
        cp.resumable = false;
        mgr.save_checkpoint(&cp).unwrap();
        let plan = mgr.create_recovery_plan("task-1").unwrap();
        assert_eq!(plan.strategy, "restart");
        assert!(plan.warnings.iter().any(|w| w.contains("not resumable")));
    }

    #[test]
    fn test_recovery_plan_compensate_failed() {
        let tmp = TempDir::new().unwrap();
        let mgr = CheckpointManager::new(tmp.path());
        let cp = make_checkpoint("task-1", "ckpt-1", "failed");
        mgr.save_checkpoint(&cp).unwrap();
        let plan = mgr.create_recovery_plan("task-1").unwrap();
        assert_eq!(plan.strategy, "compensate");
        assert!(!plan.compensating_events.is_empty());
        assert!(plan
            .compensating_events
            .iter()
            .any(|e| e.event_type == "task_cancelled"));
        assert!(plan
            .compensating_events
            .iter()
            .any(|e| e.event_type == "claim_released"));
    }

    #[test]
    fn test_recovery_plan_skip_completed() {
        let tmp = TempDir::new().unwrap();
        let mgr = CheckpointManager::new(tmp.path());
        let cp = make_checkpoint("task-1", "ckpt-1", "completed");
        mgr.save_checkpoint(&cp).unwrap();
        let plan = mgr.create_recovery_plan("task-1").unwrap();
        assert_eq!(plan.strategy, "skip");
        assert!(plan.warnings.iter().any(|w| w.contains("completed")));
    }

    #[test]
    fn test_path_traversal_rejected() {
        let tmp = TempDir::new().unwrap();
        let mgr = CheckpointManager::new(tmp.path());
        let result = mgr.load_checkpoint("../etc/passwd");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("path traversal"));
    }

    #[test]
    fn test_checkpoint_default_values() {
        let cp = Checkpoint::default();
        assert!(cp.resumable);
        assert_eq!(cp.resume_strategy, "resume_in_same_sandbox");
        assert_eq!(cp.created_at, "2026-01-01T00:00:00Z");
        assert!(cp.reason.is_none());
        assert!(cp.artifact_refs.is_empty());
    }

    #[test]
    fn test_generate_compensating_events() {
        let tmp = TempDir::new().unwrap();
        let mgr = CheckpointManager::new(tmp.path());
        let mut cp = make_checkpoint("task-1", "ckpt-1", "failed");
        cp.reason = Some("test failure".to_string());
        let events = mgr.generate_compensating_events(&cp);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, "task_cancelled");
        assert_eq!(events[1].event_type, "claim_released");
        let reason_val = events[0].payload.get("reason").unwrap().as_str().unwrap();
        assert_eq!(reason_val, "test failure");
    }

    #[test]
    fn test_integrity_check_default() {
        let check = IntegrityCheck::default();
        assert!(check.ok);
        assert!(check.errors.is_empty());
        assert!(check.warnings.is_empty());
    }
}
