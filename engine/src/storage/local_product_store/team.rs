use rusqlite::params;
use serde_json::{json, Value};

use super::{append_audit_locked, collect_values, DatabaseConnection, LocalProductStore};

impl LocalProductStore {
    pub fn upsert_team_member(
        &self,
        user_id: &str,
        display_name: &str,
        role: &str,
    ) -> Result<Value, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let now = self.now();
                conn.execute(
                    "INSERT INTO team_members (user_id, display_name, role, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?4)
                     ON CONFLICT(user_id) DO UPDATE SET
                        display_name = excluded.display_name,
                        role = excluded.role,
                        updated_at = excluded.updated_at",
                    params![user_id, display_name, role, now],
                )
                .map_err(|e| e.to_string())?;
                Ok(json!({
                    "user_id": user_id,
                    "display_name": display_name,
                    "role": role,
                    "created_at": now,
                    "updated_at": now,
                }))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let now = self.now();
                client.execute(
                    "INSERT INTO team_members (user_id, display_name, role, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $4)
                     ON CONFLICT(user_id) DO UPDATE SET
                        display_name = excluded.display_name,
                        role = excluded.role,
                        updated_at = excluded.updated_at",
                    &[&user_id, &display_name, &role, &now],
                ).map_err(|e| e.to_string())?;
                Ok(json!({
                    "user_id": user_id,
                    "display_name": display_name,
                    "role": role,
                    "created_at": now,
                    "updated_at": now,
                }))
            }),
        }
    }

    pub fn update_team_member_role(
        &self,
        user_id: &str,
        role: &str,
        actor: &str,
    ) -> Result<bool, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let now = self.now();
                let rows = conn
                    .execute(
                        "UPDATE team_members SET role = ?1, updated_at = ?2 WHERE user_id = ?3",
                        params![role, now, user_id],
                    )
                    .map_err(|e| e.to_string())?;
                if rows > 0 {
                    append_audit_locked(
                        conn,
                        &now,
                        actor,
                        "team.member.role_updated",
                        user_id,
                        &json!({"user_id": user_id, "role": role}),
                    )?;
                }
                Ok(rows > 0)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let now = self.now();
                let rows = client
                    .execute(
                        "UPDATE team_members SET role = $1, updated_at = $2 WHERE user_id = $3",
                        &[&role, &now, &user_id],
                    )
                    .map_err(|e| e.to_string())?;
                if rows > 0 {
                    let details = json!({"user_id": user_id, "role": role}).to_string();
                    client.execute(
                        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                         VALUES ($1, $2, $3, $4, $5)",
                        &[&now, &actor, &"team.member.role_updated", &user_id, &details],
                    ).map_err(|e| e.to_string())?;
                }
                Ok(rows > 0)
            }),
        }
    }

    pub fn delete_team_member(&self, user_id: &str, actor: &str) -> Result<bool, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let rows = conn
                    .execute(
                        "DELETE FROM team_members WHERE user_id = ?1",
                        params![user_id],
                    )
                    .map_err(|e| e.to_string())?;
                if rows > 0 {
                    append_audit_locked(
                        conn,
                        &self.now(),
                        actor,
                        "team.member.deleted",
                        user_id,
                        &json!({"user_id": user_id}),
                    )?;
                }
                Ok(rows > 0)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let now = self.now();
                let rows = client
                    .execute("DELETE FROM team_members WHERE user_id = $1", &[&user_id])
                    .map_err(|e| e.to_string())?;
                if rows > 0 {
                    let details = json!({"user_id": user_id}).to_string();
                    client.execute(
                        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                         VALUES ($1, $2, $3, $4, $5)",
                        &[&now, &actor, &"team.member.deleted", &user_id, &details],
                    ).map_err(|e| e.to_string())?;
                }
                Ok(rows > 0)
            }),
        }
    }

    pub fn team_snapshot(&self) -> Result<Value, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let members = {
                    let mut stmt = conn
                        .prepare(
                            "SELECT user_id, display_name, role, created_at, updated_at
                             FROM team_members
                             ORDER BY user_id",
                        )
                        .map_err(|e| e.to_string())?;
                    let rows = stmt
                        .query_map([], |row| {
                            Ok(json!({
                                "user_id": row.get::<_, String>(0)?,
                                "display_name": row.get::<_, String>(1)?,
                                "role": row.get::<_, String>(2)?,
                                "created_at": row.get::<_, String>(3)?,
                                "updated_at": row.get::<_, String>(4)?,
                            }))
                        })
                        .map_err(|e| e.to_string())?;
                    collect_values(rows)?
                };

                let api_keys = {
                    let mut stmt = conn
                        .prepare(
                            "SELECT key_id, user_id, role, scopes_json, created_at, created_by,
                                    revoked_at, last_used_at, expires_at
                             FROM api_key_metadata
                             ORDER BY key_id",
                        )
                        .map_err(|e| e.to_string())?;
                    let rows = stmt
                        .query_map([], |row| {
                            let scopes_text: String = row.get(3)?;
                            let scopes: Vec<String> =
                                serde_json::from_str(&scopes_text).unwrap_or_default();
                            Ok(json!({
                                "key_id": row.get::<_, String>(0)?,
                                "user_id": row.get::<_, String>(1)?,
                                "role": row.get::<_, String>(2)?,
                                "scopes": scopes,
                                "created_at": row.get::<_, String>(4)?,
                                "created_by": row.get::<_, String>(5)?,
                                "revoked_at": row.get::<_, Option<String>>(6)?,
                                "last_used_at": row.get::<_, Option<String>>(7)?,
                                "expires_at": row.get::<_, Option<String>>(8)?,
                            }))
                        })
                        .map_err(|e| e.to_string())?;
                    collect_values(rows)?
                };

                Ok(json!({
                    "schema_version": "local_team.v1",
                    "members": members,
                    "api_keys": api_keys,
                }))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let members_rows = client
                    .query(
                        "SELECT user_id, display_name, role, created_at, updated_at
                         FROM team_members ORDER BY user_id",
                        &[],
                    )
                    .map_err(|e| e.to_string())?;
                let mut members = Vec::new();
                for row in &members_rows {
                    members.push(json!({
                        "user_id": row.get::<_, String>(0),
                        "display_name": row.get::<_, String>(1),
                        "role": row.get::<_, String>(2),
                        "created_at": row.get::<_, String>(3),
                        "updated_at": row.get::<_, String>(4),
                    }));
                }

                let key_rows = client
                    .query(
                        "SELECT key_id, user_id, role, scopes_json, created_at, created_by,
                                revoked_at, last_used_at, expires_at
                         FROM api_key_metadata ORDER BY key_id",
                        &[],
                    )
                    .map_err(|e| e.to_string())?;
                let mut api_keys = Vec::new();
                for row in &key_rows {
                    let scopes_text: String = row.get(3);
                    let scopes: Vec<String> =
                        serde_json::from_str(&scopes_text).unwrap_or_default();
                    api_keys.push(json!({
                        "key_id": row.get::<_, String>(0),
                        "user_id": row.get::<_, String>(1),
                        "role": row.get::<_, String>(2),
                        "scopes": scopes,
                        "created_at": row.get::<_, String>(4),
                        "created_by": row.get::<_, String>(5),
                        "revoked_at": row.get::<_, Option<String>>(6),
                        "last_used_at": row.get::<_, Option<String>>(7),
                        "expires_at": row.get::<_, Option<String>>(8),
                    }));
                }

                Ok(json!({
                    "schema_version": "local_team.v1",
                    "members": members,
                    "api_keys": api_keys,
                }))
            }),
        }
    }
}
