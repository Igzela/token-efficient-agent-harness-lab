use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

pub const MANUAL_SESSION_SCHEMA_VERSION: &str = "manual_execution_session.v1";

pub const MANUAL_SESSION_STATUSES: &[&str] = &[
    "created",
    "prompt_generated",
    "human_executing",
    "result_submitted",
    "evaluated",
    "recorded",
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ManualExecutionSession {
    pub schema_version: String,
    pub session_id: String,
    pub dispatch_id: String,
    pub prompt_pack_id: String,
    pub status: String,
    pub submission_id: Option<String>,
    pub evaluation_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl ManualExecutionSession {
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}

pub struct ManualSessionStore {
    sessions: HashMap<String, ManualExecutionSession>,
}

impl Default for ManualSessionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ManualSessionStore {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    pub fn create(&mut self, dispatch_id: &str, prompt_pack_id: &str) -> ManualExecutionSession {
        let now = chrono::Utc::now().to_rfc3339();
        let session_id = format!(
            "msess-{}",
            &Uuid::new_v4().to_string().replace('-', "")[..12]
        );
        let session = ManualExecutionSession {
            schema_version: MANUAL_SESSION_SCHEMA_VERSION.to_string(),
            session_id: session_id.clone(),
            dispatch_id: dispatch_id.to_string(),
            prompt_pack_id: prompt_pack_id.to_string(),
            status: "created".to_string(),
            submission_id: None,
            evaluation_id: None,
            created_at: now.clone(),
            updated_at: now,
        };
        self.sessions.insert(session_id, session.clone());
        session
    }

    pub fn advance(
        &mut self,
        session: &ManualExecutionSession,
        new_status: &str,
        submission_id: Option<&str>,
        evaluation_id: Option<&str>,
    ) -> Result<ManualExecutionSession, String> {
        if !MANUAL_SESSION_STATUSES.contains(&new_status) {
            return Err(format!("Invalid status: {}", new_status));
        }
        let updated = ManualExecutionSession {
            schema_version: session.schema_version.clone(),
            session_id: session.session_id.clone(),
            dispatch_id: session.dispatch_id.clone(),
            prompt_pack_id: session.prompt_pack_id.clone(),
            status: new_status.to_string(),
            submission_id: submission_id
                .map(|s| s.to_string())
                .or_else(|| session.submission_id.clone()),
            evaluation_id: evaluation_id
                .map(|s| s.to_string())
                .or_else(|| session.evaluation_id.clone()),
            created_at: session.created_at.clone(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        self.sessions
            .insert(session.session_id.clone(), updated.clone());
        Ok(updated)
    }

    pub fn get(&self, session_id: &str) -> Option<&ManualExecutionSession> {
        self.sessions.get(session_id)
    }

    pub fn list_sessions(&self) -> Vec<&ManualExecutionSession> {
        self.sessions.values().collect()
    }

    pub fn get_by_dispatch(&self, dispatch_id: &str) -> Option<&ManualExecutionSession> {
        self.sessions
            .values()
            .find(|s| s.dispatch_id == dispatch_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_session() {
        let mut store = ManualSessionStore::new();
        let session = store.create("disp-001", "pp-001");
        assert_eq!(session.status, "created");
        assert_eq!(session.dispatch_id, "disp-001");
        assert!(session.session_id.starts_with("msess-"));
    }

    #[test]
    fn advance_session() {
        let mut store = ManualSessionStore::new();
        let session = store.create("disp-001", "pp-001");
        let updated = store
            .advance(&session, "prompt_generated", None, None)
            .unwrap();
        assert_eq!(updated.status, "prompt_generated");
    }

    #[test]
    fn advance_invalid_status() {
        let mut store = ManualSessionStore::new();
        let session = store.create("disp-001", "pp-001");
        let result = store.advance(&session, "bogus", None, None);
        assert!(result.is_err());
    }

    #[test]
    fn get_by_dispatch() {
        let mut store = ManualSessionStore::new();
        store.create("disp-001", "pp-001");
        store.create("disp-002", "pp-002");
        let found = store.get_by_dispatch("disp-002");
        assert!(found.is_some());
        assert_eq!(found.unwrap().prompt_pack_id, "pp-002");
    }

    #[test]
    fn list_sessions() {
        let mut store = ManualSessionStore::new();
        store.create("d1", "p1");
        store.create("d2", "p2");
        assert_eq!(store.list_sessions().len(), 2);
    }

    #[test]
    fn session_to_value_roundtrip() {
        let mut store = ManualSessionStore::new();
        let session = store.create("d1", "p1");
        let val = session.to_value();
        let decoded: ManualExecutionSession = serde_json::from_value(val).unwrap();
        assert_eq!(decoded, session);
    }
}
