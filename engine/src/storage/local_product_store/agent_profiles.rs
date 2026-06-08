use rusqlite::params;

use super::{DatabaseConnection, LocalProductStore};
use crate::workflow::agent_profiles::{
    AgentProfile, AgentProfileId, AgentProfileRole, WorkspaceScope,
};

impl LocalProductStore {
    pub fn upsert_agent_profile(
        &self,
        profile_id: &str,
        role: &str,
        tools: &[String],
        model_hint: Option<&str>,
        context_budget_tokens: Option<u64>,
        workspace_scope: &str,
        executor_preference: Option<&str>,
        max_retries: u32,
    ) -> Result<(), String> {
        let tools_json = serde_json::to_string(tools).map_err(|e| e.to_string())?;
        let now = self.now();
        let budget_i64 = context_budget_tokens.map(|v| v as i64);
        let retries_i64 = max_retries as i64;
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute(
                    "INSERT INTO agent_profiles
                     (profile_id, role, tools_json, model_hint, context_budget_tokens,
                      workspace_scope, executor_preference, max_retries, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                     ON CONFLICT(profile_id) DO UPDATE SET
                      role = excluded.role,
                      tools_json = excluded.tools_json,
                      model_hint = excluded.model_hint,
                      context_budget_tokens = excluded.context_budget_tokens,
                      workspace_scope = excluded.workspace_scope,
                      executor_preference = excluded.executor_preference,
                      max_retries = excluded.max_retries,
                      updated_at = excluded.updated_at",
                    params![
                        profile_id,
                        role,
                        tools_json,
                        model_hint,
                        budget_i64,
                        workspace_scope,
                        executor_preference,
                        retries_i64,
                        now,
                        now,
                    ],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .execute(
                        "INSERT INTO agent_profiles
                         (profile_id, role, tools_json, model_hint, context_budget_tokens,
                          workspace_scope, executor_preference, max_retries, created_at, updated_at)
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                         ON CONFLICT(profile_id) DO UPDATE SET
                          role = EXCLUDED.role,
                          tools_json = EXCLUDED.tools_json,
                          model_hint = EXCLUDED.model_hint,
                          context_budget_tokens = EXCLUDED.context_budget_tokens,
                          workspace_scope = EXCLUDED.workspace_scope,
                          executor_preference = EXCLUDED.executor_preference,
                          max_retries = EXCLUDED.max_retries,
                          updated_at = EXCLUDED.updated_at",
                        &[
                            &profile_id,
                            &role,
                            &tools_json,
                            &model_hint,
                            &budget_i64,
                            &workspace_scope,
                            &executor_preference,
                            &retries_i64,
                            &now,
                            &now,
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(())
            }),
        }
    }

    pub fn get_agent_profile(&self, profile_id: &str) -> Result<Option<AgentProfile>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT profile_id, role, tools_json, model_hint, context_budget_tokens,
                                workspace_scope, executor_preference, max_retries
                         FROM agent_profiles WHERE profile_id = ?1 LIMIT 1",
                    )
                    .map_err(|e| e.to_string())?;
                let mut rows = stmt
                    .query_map(params![profile_id], profile_row)
                    .map_err(|e| e.to_string())?;
                match rows.next() {
                    Some(row) => Ok(Some(row.map_err(|e| e.to_string())?)),
                    None => Ok(None),
                }
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT profile_id, role, tools_json, model_hint, context_budget_tokens,
                                workspace_scope, executor_preference, max_retries
                         FROM agent_profiles WHERE profile_id = $1 LIMIT 1",
                        &[&profile_id],
                    )
                    .map_err(|e| e.to_string())?;
                match rows.into_iter().next() {
                    Some(row) => Ok(Some(pg_profile_row(&row))),
                    None => Ok(None),
                }
            }),
        }
    }

    pub fn list_agent_profiles(&self) -> Result<Vec<AgentProfile>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT profile_id, role, tools_json, model_hint, context_budget_tokens,
                                workspace_scope, executor_preference, max_retries
                         FROM agent_profiles ORDER BY profile_id",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt.query_map([], profile_row).map_err(|e| e.to_string())?;
                let mut profiles = Vec::new();
                for row in rows {
                    profiles.push(row.map_err(|e| e.to_string())?);
                }
                Ok(profiles)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT profile_id, role, tools_json, model_hint, context_budget_tokens,
                                workspace_scope, executor_preference, max_retries
                         FROM agent_profiles ORDER BY profile_id",
                        &[],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(rows.iter().map(pg_profile_row).collect())
            }),
        }
    }

    pub fn delete_agent_profile(&self, profile_id: &str) -> Result<bool, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let affected = conn
                    .execute(
                        "DELETE FROM agent_profiles WHERE profile_id = ?1",
                        params![profile_id],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(affected > 0)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let affected = client
                    .execute(
                        "DELETE FROM agent_profiles WHERE profile_id = $1",
                        &[&profile_id],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(affected > 0)
            }),
        }
    }

    pub fn get_profile_for_role(&self, role: &str) -> Result<Option<AgentProfile>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT profile_id, role, tools_json, model_hint, context_budget_tokens,
                                workspace_scope, executor_preference, max_retries
                         FROM agent_profiles WHERE role = ?1 LIMIT 1",
                    )
                    .map_err(|e| e.to_string())?;
                let mut rows = stmt
                    .query_map(params![role], profile_row)
                    .map_err(|e| e.to_string())?;
                match rows.next() {
                    Some(row) => Ok(Some(row.map_err(|e| e.to_string())?)),
                    None => Ok(None),
                }
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT profile_id, role, tools_json, model_hint, context_budget_tokens,
                                workspace_scope, executor_preference, max_retries
                         FROM agent_profiles WHERE role = $1 LIMIT 1",
                        &[&role],
                    )
                    .map_err(|e| e.to_string())?;
                match rows.into_iter().next() {
                    Some(row) => Ok(Some(pg_profile_row(&row))),
                    None => Ok(None),
                }
            }),
        }
    }

    /// Seed default profiles into the store if none exist yet.
    pub fn seed_default_agent_profiles(&self) -> Result<(), String> {
        let existing = self.list_agent_profiles()?;
        if !existing.is_empty() {
            return Ok(());
        }
        let registry = crate::workflow::agent_profiles::AgentProfileRegistry::new();
        for profile in registry.list_all() {
            self.upsert_agent_profile(
                profile.profile_id.as_str(),
                profile.role.as_str(),
                &profile.tools,
                profile.model_hint.as_deref(),
                profile.context_budget_tokens,
                profile.workspace_scope.as_str(),
                profile.executor_preference.as_deref(),
                profile.max_retries,
            )?;
        }
        Ok(())
    }
}

fn profile_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentProfile> {
    let profile_id: String = row.get(0)?;
    let role_str: String = row.get(1)?;
    let tools_json: String = row.get(2)?;
    let model_hint: Option<String> = row.get(3)?;
    let context_budget: Option<i64> = row.get(4)?;
    let scope_str: String = row.get(5)?;
    let executor_preference: Option<String> = row.get(6)?;
    let max_retries: i64 = row.get(7)?;

    let tools: Vec<String> = serde_json::from_str(&tools_json).unwrap_or_default();
    let role = AgentProfileRole::parse_str(&role_str).unwrap_or(AgentProfileRole::Implementer);
    let workspace_scope = WorkspaceScope::parse_str(&scope_str).unwrap_or(WorkspaceScope::Task);

    Ok(AgentProfile {
        profile_id: AgentProfileId(profile_id),
        role,
        tools,
        model_hint,
        context_budget_tokens: context_budget.map(|v| v as u64),
        workspace_scope,
        executor_preference,
        max_retries: max_retries as u32,
    })
}

#[cfg(feature = "pg")]
fn pg_profile_row(row: &postgres::Row) -> AgentProfile {
    let profile_id: String = row.get(0);
    let role_str: String = row.get(1);
    let tools_json: String = row.get(2);
    let model_hint: Option<String> = row.get(3);
    let context_budget: Option<i64> = row.get(4);
    let scope_str: String = row.get(5);
    let executor_preference: Option<String> = row.get(6);
    let max_retries: i64 = row.get(7);

    let tools: Vec<String> = serde_json::from_str(&tools_json).unwrap_or_default();
    let role = AgentProfileRole::parse_str(&role_str).unwrap_or(AgentProfileRole::Implementer);
    let workspace_scope = WorkspaceScope::parse_str(&scope_str).unwrap_or(WorkspaceScope::Task);

    AgentProfile {
        profile_id: AgentProfileId(profile_id),
        role,
        tools,
        model_hint,
        context_budget_tokens: context_budget.map(|v| v as u64),
        workspace_scope,
        executor_preference,
        max_retries: max_retries as u32,
    }
}
