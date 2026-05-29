use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const VALID_TRANSITIONS: &[(&str, &[&str])] = &[
    ("draft", &["produced"]),
    ("produced", &["verified", "rejected"]),
    ("verified", &["promoted"]),
    ("promoted", &["archived"]),
    ("archived", &[]),
    ("rejected", &[]),
];

fn is_valid_transition(from: &str, to: &str) -> bool {
    VALID_TRANSITIONS
        .iter()
        .find(|(s, _)| *s == from)
        .map(|(_, targets)| targets.contains(&to))
        .unwrap_or(false)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub artifact_id: String,
    pub task_id: String,
    pub artifact_type: String,
    pub path: String,
    pub sha256: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Default for ArtifactRecord {
    fn default() -> Self {
        Self {
            artifact_id: String::new(),
            task_id: String::new(),
            artifact_type: String::new(),
            path: String::new(),
            sha256: String::new(),
            status: "draft".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            metadata: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactTransition {
    pub artifact_id: String,
    pub from_status: String,
    pub to_status: String,
    pub timestamp: String,
    pub reason: String,
}

impl Default for ArtifactTransition {
    fn default() -> Self {
        Self {
            artifact_id: String::new(),
            from_status: String::new(),
            to_status: String::new(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            reason: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DependencyUnlock {
    pub artifact_id: String,
    pub dependency_id: String,
    pub unlocked: bool,
    pub reason: String,
}

impl Default for DependencyUnlock {
    fn default() -> Self {
        Self {
            artifact_id: String::new(),
            dependency_id: String::new(),
            unlocked: false,
            reason: String::new(),
        }
    }
}

pub struct ArtifactLifecycleManager {
    records: HashMap<String, ArtifactRecord>,
    transitions: Vec<ArtifactTransition>,
}

impl Default for ArtifactLifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ArtifactLifecycleManager {
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
            transitions: Vec::new(),
        }
    }

    pub fn produce_artifact(
        &mut self,
        artifact_id: &str,
        task_id: &str,
        artifact_type: &str,
        path: &str,
        sha256: &str,
        timestamp: &str,
        metadata: Option<&HashMap<String, serde_json::Value>>,
    ) -> Result<ArtifactRecord, String> {
        if let Some(existing) = self.records.get(artifact_id) {
            if existing.task_id == task_id
                && existing.artifact_type == artifact_type
                && existing.path == path
                && existing.sha256 == sha256
            {
                return Ok(existing.clone());
            }
            return Err(format!(
                "artifact {} already exists with different content",
                artifact_id
            ));
        }
        let record = ArtifactRecord {
            artifact_id: artifact_id.to_string(),
            task_id: task_id.to_string(),
            artifact_type: artifact_type.to_string(),
            path: path.to_string(),
            sha256: sha256.to_string(),
            status: "produced".to_string(),
            created_at: timestamp.to_string(),
            updated_at: timestamp.to_string(),
            metadata: metadata.cloned().unwrap_or_default(),
        };
        self.records.insert(artifact_id.to_string(), record.clone());
        self.transitions.push(ArtifactTransition {
            artifact_id: artifact_id.to_string(),
            from_status: "draft".to_string(),
            to_status: "produced".to_string(),
            timestamp: timestamp.to_string(),
            reason: "artifact produced".to_string(),
        });
        Ok(record)
    }

    pub fn verify_artifact(
        &mut self,
        artifact_id: &str,
        timestamp: &str,
        reason: &str,
    ) -> Result<ArtifactRecord, String> {
        self.transition(artifact_id, "verified", timestamp, reason)
    }

    pub fn reject_artifact(
        &mut self,
        artifact_id: &str,
        timestamp: &str,
        reason: &str,
    ) -> Result<ArtifactRecord, String> {
        self.transition(artifact_id, "rejected", timestamp, reason)
    }

    pub fn promote_artifact(
        &mut self,
        artifact_id: &str,
        timestamp: &str,
        reason: &str,
    ) -> Result<ArtifactRecord, String> {
        self.transition(artifact_id, "promoted", timestamp, reason)
    }

    pub fn archive_artifact(
        &mut self,
        artifact_id: &str,
        timestamp: &str,
        reason: &str,
    ) -> Result<ArtifactRecord, String> {
        self.transition(artifact_id, "archived", timestamp, reason)
    }

    pub fn dependency_unlock(
        &self,
        artifact_id: &str,
        dependency_id: &str,
    ) -> Result<DependencyUnlock, String> {
        let record = self.require(artifact_id)?;
        let unlocked = record.status == "verified" || record.status == "promoted";
        let reason = if unlocked {
            format!("artifact {} is {}", artifact_id, record.status)
        } else {
            format!("artifact {} is not verified or promoted", artifact_id)
        };
        Ok(DependencyUnlock {
            artifact_id: artifact_id.to_string(),
            dependency_id: dependency_id.to_string(),
            unlocked,
            reason,
        })
    }

    pub fn get_artifact(&self, artifact_id: &str) -> Option<&ArtifactRecord> {
        self.records.get(artifact_id)
    }

    pub fn list_artifacts(&self) -> Vec<&ArtifactRecord> {
        let mut keys: Vec<&String> = self.records.keys().collect();
        keys.sort();
        keys.iter().map(|k| self.records.get(*k).unwrap()).collect()
    }

    pub fn list_transitions(&self, artifact_id: Option<&str>) -> Vec<&ArtifactTransition> {
        match artifact_id {
            Some(aid) => self
                .transitions
                .iter()
                .filter(|t| t.artifact_id == aid)
                .collect(),
            None => self.transitions.iter().collect(),
        }
    }

    fn transition(
        &mut self,
        artifact_id: &str,
        to_status: &str,
        timestamp: &str,
        reason: &str,
    ) -> Result<ArtifactRecord, String> {
        let current = self.require(artifact_id)?.clone();
        if !is_valid_transition(&current.status, to_status) {
            return Err(format!(
                "invalid artifact transition {} -> {}",
                current.status, to_status
            ));
        }
        let from_status = current.status.clone();
        let updated = ArtifactRecord {
            artifact_id: current.artifact_id.clone(),
            task_id: current.task_id.clone(),
            artifact_type: current.artifact_type.clone(),
            path: current.path.clone(),
            sha256: current.sha256.clone(),
            status: to_status.to_string(),
            created_at: current.created_at.clone(),
            updated_at: timestamp.to_string(),
            metadata: current.metadata.clone(),
        };
        self.records
            .insert(artifact_id.to_string(), updated.clone());
        self.transitions.push(ArtifactTransition {
            artifact_id: artifact_id.to_string(),
            from_status,
            to_status: to_status.to_string(),
            timestamp: timestamp.to_string(),
            reason: reason.to_string(),
        });
        Ok(updated)
    }

    fn require(&self, artifact_id: &str) -> Result<&ArtifactRecord, String> {
        self.records
            .get(artifact_id)
            .ok_or_else(|| format!("unknown artifact: {}", artifact_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TS: &str = "2026-01-01T00:00:00Z";

    fn produce_default(mgr: &mut ArtifactLifecycleManager) -> ArtifactRecord {
        mgr.produce_artifact("art1", "task1", "file", "/a.txt", "abc123", TS, None)
            .unwrap()
    }

    #[test]
    fn test_produce_artifact() {
        let mut mgr = ArtifactLifecycleManager::new();
        let rec = produce_default(&mut mgr);
        assert_eq!(rec.status, "produced");
        assert_eq!(rec.artifact_id, "art1");
    }

    #[test]
    fn test_produce_idempotent_same_content() {
        let mut mgr = ArtifactLifecycleManager::new();
        let r1 = produce_default(&mut mgr);
        let r2 = produce_default(&mut mgr);
        assert_eq!(r1, r2);
    }

    #[test]
    fn test_produce_different_content_errors() {
        let mut mgr = ArtifactLifecycleManager::new();
        produce_default(&mut mgr);
        let result = mgr.produce_artifact("art1", "task1", "file", "/b.txt", "def456", TS, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already exists"));
    }

    #[test]
    fn test_full_lifecycle() {
        let mut mgr = ArtifactLifecycleManager::new();
        produce_default(&mut mgr);
        let verified = mgr.verify_artifact("art1", TS, "ok").unwrap();
        assert_eq!(verified.status, "verified");
        let promoted = mgr.promote_artifact("art1", TS, "good").unwrap();
        assert_eq!(promoted.status, "promoted");
        let archived = mgr.archive_artifact("art1", TS, "done").unwrap();
        assert_eq!(archived.status, "archived");
    }

    #[test]
    fn test_reject_from_produced() {
        let mut mgr = ArtifactLifecycleManager::new();
        produce_default(&mut mgr);
        let rejected = mgr.reject_artifact("art1", TS, "bad").unwrap();
        assert_eq!(rejected.status, "rejected");
    }

    #[test]
    fn test_invalid_transition_verify_from_rejected() {
        let mut mgr = ArtifactLifecycleManager::new();
        produce_default(&mut mgr);
        mgr.reject_artifact("art1", TS, "bad").unwrap();
        let result = mgr.verify_artifact("art1", TS, "retry");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid artifact transition"));
    }

    #[test]
    fn test_invalid_transition_promote_from_produced() {
        let mut mgr = ArtifactLifecycleManager::new();
        produce_default(&mut mgr);
        let result = mgr.promote_artifact("art1", TS, "skip");
        assert!(result.is_err());
    }

    #[test]
    fn test_dependency_unlock_verified() {
        let mut mgr = ArtifactLifecycleManager::new();
        produce_default(&mut mgr);
        mgr.verify_artifact("art1", TS, "ok").unwrap();
        let du = mgr.dependency_unlock("art1", "dep1").unwrap();
        assert!(du.unlocked);
        assert!(du.reason.contains("verified"));
    }

    #[test]
    fn test_dependency_unlock_not_verified() {
        let mut mgr = ArtifactLifecycleManager::new();
        produce_default(&mut mgr);
        let du = mgr.dependency_unlock("art1", "dep1").unwrap();
        assert!(!du.unlocked);
        assert!(du.reason.contains("not verified or promoted"));
    }

    #[test]
    fn test_list_transitions_filtered() {
        let mut mgr = ArtifactLifecycleManager::new();
        produce_default(&mut mgr);
        mgr.verify_artifact("art1", TS, "ok").unwrap();
        let all = mgr.list_transitions(None);
        let filtered = mgr.list_transitions(Some("art1"));
        assert_eq!(all.len(), 2);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_unknown_artifact_errors() {
        let mgr = ArtifactLifecycleManager::new();
        let result = mgr.dependency_unlock("missing", "dep1");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown artifact"));
    }

    #[test]
    fn test_artifact_default_values() {
        let rec = ArtifactRecord::default();
        assert_eq!(rec.status, "draft");
        assert!(rec.metadata.is_empty());
    }
}
