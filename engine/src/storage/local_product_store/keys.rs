use rusqlite::params;
use serde_json::{json, Value};

use super::{append_audit_locked, collect_values, LocalProductStore};

impl LocalProductStore {
    pub fn record_api_key_metadata(
        &self,
        key_id: &str,
        user_id: &str,
        role: &str,
        scopes: &[String],
        actor: &str,
    ) -> Result<Value, String> {
        let scopes_json = serde_json::to_string(scopes).map_err(|e| e.to_string())?;
        self.with_conn(|conn| {
            let now = self.now();
            conn.execute(
                "INSERT INTO api_key_metadata
                 (key_id, user_id, role, scopes_json, created_at, created_by, revoked_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)
                 ON CONFLICT(key_id) DO UPDATE SET
                    user_id = excluded.user_id,
                    role = excluded.role,
                    scopes_json = excluded.scopes_json,
                    created_by = excluded.created_by",
                params![key_id, user_id, role, scopes_json, now, actor],
            )
            .map_err(|e| e.to_string())?;
            append_audit_locked(
                conn,
                &now,
                actor,
                "api_key.record_metadata",
                key_id,
                &json!({"key_id": key_id, "user_id": user_id, "role": role}),
            )?;
            Ok(json!({
                "key_id": key_id,
                "user_id": user_id,
                "role": role,
                "scopes": scopes,
                "created_at": now,
                "created_by": actor,
            }))
        })
    }

    pub fn get_api_key_metadata(&self, key_id: &str) -> Result<Option<Value>, String> {
        self.with_conn(|conn| {
            let result = conn.query_row(
                "SELECT key_id, user_id, role, scopes_json, created_at, created_by,
                        revoked_at, last_used_at, expires_at
                 FROM api_key_metadata WHERE key_id = ?1",
                params![key_id],
                |row| {
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
                },
            );
            match result {
                Ok(value) => Ok(Some(value)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.to_string()),
            }
        })
    }

    pub fn list_api_key_metadata(&self, limit: i64) -> Result<Vec<Value>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT key_id, user_id, role, scopes_json, created_at, created_by,
                            revoked_at, last_used_at, expires_at
                     FROM api_key_metadata
                     ORDER BY created_at DESC
                     LIMIT ?1",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![limit], |row| {
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
            collect_values(rows)
        })
    }

    pub fn revoke_api_key_metadata(&self, key_id: &str, actor: &str) -> Result<bool, String> {
        self.with_conn(|conn| {
            let now = self.now();
            let rows = conn
                .execute(
                    "UPDATE api_key_metadata SET revoked_at = ?1 WHERE key_id = ?2 AND revoked_at IS NULL",
                    params![now, key_id],
                )
                .map_err(|e| e.to_string())?;
            if rows > 0 {
                append_audit_locked(
                    conn,
                    &now,
                    actor,
                    "team.key.revoked",
                    key_id,
                    &json!({"key_id": key_id}),
                )?;
            }
            Ok(rows > 0)
        })
    }

    pub fn delete_api_key_metadata(&self, key_id: &str, actor: &str) -> Result<bool, String> {
        self.with_conn(|conn| {
            let rows = conn
                .execute(
                    "DELETE FROM api_key_metadata WHERE key_id = ?1",
                    params![key_id],
                )
                .map_err(|e| e.to_string())?;
            if rows > 0 {
                append_audit_locked(
                    conn,
                    &self.now(),
                    actor,
                    "team.key.deleted",
                    key_id,
                    &json!({"key_id": key_id}),
                )?;
            }
            Ok(rows > 0)
        })
    }

    pub fn update_api_key_scopes(
        &self,
        key_id: &str,
        scopes: &[String],
        actor: &str,
    ) -> Result<bool, String> {
        let scopes_json = serde_json::to_string(scopes).map_err(|e| e.to_string())?;
        self.with_conn(|conn| {
            let rows = conn
                .execute(
                    "UPDATE api_key_metadata SET scopes_json = ?1 WHERE key_id = ?2",
                    params![scopes_json, key_id],
                )
                .map_err(|e| e.to_string())?;
            if rows > 0 {
                append_audit_locked(
                    conn,
                    &self.now(),
                    actor,
                    "team.key.scopes_updated",
                    key_id,
                    &json!({"key_id": key_id, "scopes": scopes}),
                )?;
            }
            Ok(rows > 0)
        })
    }

    pub fn touch_api_key_last_used(&self, key_id: &str) -> Result<(), String> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE api_key_metadata SET last_used_at = ?1 WHERE key_id = ?2",
                params![self.now(), key_id],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })
    }
}
