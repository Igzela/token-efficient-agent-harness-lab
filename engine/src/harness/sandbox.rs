use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sandbox {
    pub sandbox_id: String,
    pub task_id: String,
    pub status: String,
    pub claimed_files: Vec<String>,
    pub created_at: String,
    pub released_at: Option<String>,
}

impl Default for Sandbox {
    fn default() -> Self {
        Self {
            sandbox_id: String::new(),
            task_id: String::new(),
            status: "created".to_string(),
            claimed_files: Vec::new(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            released_at: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileClaim {
    pub claim_id: String,
    pub sandbox_id: String,
    pub file_path: String,
    pub claimed_at: String,
    pub released: bool,
}

impl Default for FileClaim {
    fn default() -> Self {
        Self {
            claim_id: String::new(),
            sandbox_id: String::new(),
            file_path: String::new(),
            claimed_at: "2026-01-01T00:00:00Z".to_string(),
            released: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConflictReport {
    pub has_conflict: bool,
    pub conflicting_sandbox_id: Option<String>,
    pub conflicting_file: Option<String>,
    pub message: String,
}

impl Default for ConflictReport {
    fn default() -> Self {
        Self {
            has_conflict: false,
            conflicting_sandbox_id: None,
            conflicting_file: None,
            message: "ok".to_string(),
        }
    }
}

pub struct SandboxManager {
    sandboxes: HashMap<String, Sandbox>,
    file_owners: HashMap<String, String>,
    claims: HashMap<String, FileClaim>,
    claim_counter: u64,
}

impl Default for SandboxManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SandboxManager {
    pub fn new() -> Self {
        Self {
            sandboxes: HashMap::new(),
            file_owners: HashMap::new(),
            claims: HashMap::new(),
            claim_counter: 0,
        }
    }

    pub fn create_sandbox(
        &mut self,
        task_id: &str,
        files: &[String],
        timestamp: &str,
    ) -> Result<Sandbox, String> {
        let sandbox_id = format!("sbx_{}_{}", task_id, self.sandboxes.len());
        let sandbox = Sandbox {
            sandbox_id: sandbox_id.clone(),
            task_id: task_id.to_string(),
            status: "created".to_string(),
            claimed_files: Vec::new(),
            created_at: timestamp.to_string(),
            released_at: None,
        };
        self.sandboxes.insert(sandbox_id.clone(), sandbox);
        if !files.is_empty() {
            let report = self.claim_files(&sandbox_id, files, timestamp)?;
            if report.has_conflict {
                return Err(format!("initial claim failed: {}", report.message));
            }
        }
        Ok(self.sandboxes.get(&sandbox_id).unwrap().clone())
    }

    pub fn claim_files(
        &mut self,
        sandbox_id: &str,
        files: &[String],
        timestamp: &str,
    ) -> Result<ConflictReport, String> {
        let sandbox = match self.sandboxes.get(sandbox_id) {
            Some(s) => s.clone(),
            None => {
                return Ok(ConflictReport {
                    has_conflict: true,
                    conflicting_sandbox_id: None,
                    conflicting_file: None,
                    message: format!("unknown sandbox: {}", sandbox_id),
                });
            }
        };
        if sandbox.status != "created" && sandbox.status != "active" {
            return Ok(ConflictReport {
                has_conflict: true,
                conflicting_sandbox_id: None,
                conflicting_file: None,
                message: format!("sandbox {} is {}", sandbox_id, sandbox.status),
            });
        }
        for file_path in files {
            if let Some(owner) = self.file_owners.get(file_path) {
                if owner != sandbox_id {
                    return Ok(ConflictReport {
                        has_conflict: true,
                        conflicting_sandbox_id: Some(owner.clone()),
                        conflicting_file: Some(file_path.clone()),
                        message: format!("file {} claimed by {}", file_path, owner),
                    });
                }
            }
        }
        for file_path in files {
            if !self.file_owners.contains_key(file_path) {
                self.file_owners
                    .insert(file_path.clone(), sandbox_id.to_string());
                self.claim_counter += 1;
                let claim_id = format!("claim_{}", self.claim_counter);
                self.claims.insert(
                    claim_id.clone(),
                    FileClaim {
                        claim_id,
                        sandbox_id: sandbox_id.to_string(),
                        file_path: file_path.clone(),
                        claimed_at: timestamp.to_string(),
                        released: false,
                    },
                );
            }
        }
        let mut existing: Vec<String> = sandbox.claimed_files.clone();
        for f in files {
            if !existing.contains(f) {
                existing.push(f.clone());
            }
        }
        existing.sort();
        let updated = Sandbox {
            sandbox_id: sandbox.sandbox_id.clone(),
            task_id: sandbox.task_id.clone(),
            status: "active".to_string(),
            claimed_files: existing,
            created_at: sandbox.created_at.clone(),
            released_at: sandbox.released_at.clone(),
        };
        self.sandboxes.insert(sandbox_id.to_string(), updated);
        Ok(ConflictReport {
            has_conflict: false,
            conflicting_sandbox_id: None,
            conflicting_file: None,
            message: "ok".to_string(),
        })
    }

    pub fn release_sandbox(
        &mut self,
        sandbox_id: &str,
        timestamp: &str,
    ) -> Result<Sandbox, String> {
        let sandbox = match self.sandboxes.get(sandbox_id) {
            Some(s) => s.clone(),
            None => return Err(format!("unknown sandbox: {}", sandbox_id)),
        };
        for file_path in &sandbox.claimed_files {
            if self.file_owners.get(file_path).map(|s| s.as_str()) == Some(sandbox_id) {
                self.file_owners.remove(file_path);
            }
        }
        let claim_ids: Vec<String> = self
            .claims
            .values()
            .filter(|c| c.sandbox_id == sandbox_id && !c.released)
            .map(|c| c.claim_id.clone())
            .collect();
        for cid in claim_ids {
            if let Some(claim) = self.claims.get(&cid) {
                self.claims.insert(
                    cid.clone(),
                    FileClaim {
                        claim_id: claim.claim_id.clone(),
                        sandbox_id: claim.sandbox_id.clone(),
                        file_path: claim.file_path.clone(),
                        claimed_at: claim.claimed_at.clone(),
                        released: true,
                    },
                );
            }
        }
        let released = Sandbox {
            sandbox_id: sandbox.sandbox_id.clone(),
            task_id: sandbox.task_id.clone(),
            status: "released".to_string(),
            claimed_files: sandbox.claimed_files.clone(),
            created_at: sandbox.created_at.clone(),
            released_at: Some(timestamp.to_string()),
        };
        self.sandboxes
            .insert(sandbox_id.to_string(), released.clone());
        Ok(released)
    }

    pub fn get_sandbox(&self, sandbox_id: &str) -> Option<&Sandbox> {
        self.sandboxes.get(sandbox_id)
    }

    pub fn list_active(&self) -> Vec<&Sandbox> {
        self.sandboxes
            .values()
            .filter(|s| s.status == "created" || s.status == "active")
            .collect()
    }

    pub fn list_all(&self) -> Vec<&Sandbox> {
        self.sandboxes.values().collect()
    }

    pub fn is_file_claimed(&self, file_path: &str) -> Option<&str> {
        self.file_owners.get(file_path).map(|s| s.as_str())
    }

    pub fn get_claims(&self, sandbox_id: &str) -> Vec<&FileClaim> {
        self.claims
            .values()
            .filter(|c| c.sandbox_id == sandbox_id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_sandbox_no_files() {
        let mut mgr = SandboxManager::new();
        let sb = mgr
            .create_sandbox("task1", &[], "2026-01-01T00:00:00Z")
            .unwrap();
        assert_eq!(sb.task_id, "task1");
        assert_eq!(sb.status, "created");
        assert!(sb.claimed_files.is_empty());
        assert!(sb.released_at.is_none());
    }

    #[test]
    fn test_create_sandbox_with_files() {
        let mut mgr = SandboxManager::new();
        let files = vec!["a.py".to_string(), "b.py".to_string()];
        let sb = mgr
            .create_sandbox("task1", &files, "2026-01-01T00:00:00Z")
            .unwrap();
        assert_eq!(sb.status, "active");
        assert_eq!(sb.claimed_files, vec!["a.py", "b.py"]);
    }

    #[test]
    fn test_file_conflict_on_create() {
        let mut mgr = SandboxManager::new();
        let files = vec!["shared.py".to_string()];
        mgr.create_sandbox("task1", &files, "2026-01-01T00:00:00Z")
            .unwrap();
        let result = mgr.create_sandbox("task2", &files, "2026-01-01T00:00:00Z");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("initial claim failed"));
    }

    #[test]
    fn test_release_sandbox_frees_files() {
        let mut mgr = SandboxManager::new();
        let files = vec!["x.rs".to_string()];
        let sb = mgr
            .create_sandbox("t1", &files, "2026-01-01T00:00:00Z")
            .unwrap();
        let sbx_id = sb.sandbox_id.clone();
        let released = mgr
            .release_sandbox(&sbx_id, "2026-01-01T01:00:00Z")
            .unwrap();
        assert_eq!(released.status, "released");
        assert!(released.released_at.is_some());
        assert!(mgr.is_file_claimed("x.rs").is_none());
    }

    #[test]
    fn test_claim_files_on_active_sandbox() {
        let mut mgr = SandboxManager::new();
        let sb = mgr
            .create_sandbox("t1", &[], "2026-01-01T00:00:00Z")
            .unwrap();
        let sbx_id = sb.sandbox_id.clone();
        let report = mgr
            .claim_files(&sbx_id, &["new.py".to_string()], "2026-01-01T00:01:00Z")
            .unwrap();
        assert!(!report.has_conflict);
        assert_eq!(report.message, "ok");
        assert_eq!(mgr.is_file_claimed("new.py"), Some(sbx_id.as_str()));
    }

    #[test]
    fn test_claim_files_on_released_sandbox_returns_conflict() {
        let mut mgr = SandboxManager::new();
        let sb = mgr
            .create_sandbox("t1", &[], "2026-01-01T00:00:00Z")
            .unwrap();
        let sbx_id = sb.sandbox_id.clone();
        mgr.release_sandbox(&sbx_id, "2026-01-01T01:00:00Z")
            .unwrap();
        let report = mgr
            .claim_files(&sbx_id, &["f.py".to_string()], "2026-01-01T02:00:00Z")
            .unwrap();
        assert!(report.has_conflict);
        assert!(report.message.contains("released"));
    }

    #[test]
    fn test_list_active_and_list_all() {
        let mut mgr = SandboxManager::new();
        mgr.create_sandbox("t1", &[], "2026-01-01T00:00:00Z")
            .unwrap();
        let sb2 = mgr
            .create_sandbox("t2", &[], "2026-01-01T00:00:00Z")
            .unwrap();
        mgr.release_sandbox(&sb2.sandbox_id, "2026-01-01T01:00:00Z")
            .unwrap();
        assert_eq!(mgr.list_active().len(), 1);
        assert_eq!(mgr.list_all().len(), 2);
    }

    #[test]
    fn test_get_claims_for_sandbox() {
        let mut mgr = SandboxManager::new();
        let files = vec!["a.py".to_string(), "b.py".to_string()];
        let sb = mgr
            .create_sandbox("t1", &files, "2026-01-01T00:00:00Z")
            .unwrap();
        let claims = mgr.get_claims(&sb.sandbox_id);
        assert_eq!(claims.len(), 2);
        for c in &claims {
            assert!(!c.released);
        }
    }

    #[test]
    fn test_release_unknown_sandbox_errors() {
        let mut mgr = SandboxManager::new();
        let result = mgr.release_sandbox("nonexistent", "2026-01-01T00:00:00Z");
        assert!(result.is_err());
    }

    #[test]
    fn test_sandbox_default_values() {
        let sb = Sandbox::default();
        assert_eq!(sb.status, "created");
        assert!(sb.claimed_files.is_empty());
        assert!(sb.released_at.is_none());
    }
}
