use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use super::{append_audit_locked, collect_values, DatabaseConnection, LocalProductStore};

impl LocalProductStore {
    pub fn record_api_key_metadata(
        &self,
        key_id: &str,
        user_id: &str,
        role: &str,
        scopes: &[String],
        actor: &str,
    ) -> Result<Value, String> {
        self.record_api_key_metadata_with_tenant(key_id, user_id, role, None, scopes, None, actor)
    }

    pub fn record_api_key_metadata_for_tenant(
        &self,
        tenant_id: &str,
        key_id: &str,
        user_id: &str,
        role: &str,
        scopes: &[String],
        actor: &str,
    ) -> Result<Value, String> {
        self.record_api_key_metadata_with_tenant(
            key_id,
            user_id,
            role,
            Some(tenant_id),
            scopes,
            None,
            actor,
        )
    }

    pub fn record_api_key_metadata_with_expiry(
        &self,
        key_id: &str,
        user_id: &str,
        role: &str,
        scopes: &[String],
        expires_at: Option<f64>,
        actor: &str,
    ) -> Result<Value, String> {
        self.record_api_key_metadata_with_tenant(
            key_id, user_id, role, None, scopes, expires_at, actor,
        )
    }

    pub fn record_api_key_metadata_with_expiry_for_tenant(
        &self,
        tenant_id: &str,
        key_id: &str,
        user_id: &str,
        role: &str,
        scopes: &[String],
        expires_at: Option<f64>,
        actor: &str,
    ) -> Result<Value, String> {
        self.record_api_key_metadata_with_tenant(
            key_id,
            user_id,
            role,
            Some(tenant_id),
            scopes,
            expires_at,
            actor,
        )
    }

    fn record_api_key_metadata_with_tenant(
        &self,
        key_id: &str,
        user_id: &str,
        role: &str,
        tenant_id: Option<&str>,
        scopes: &[String],
        expires_at: Option<f64>,
        actor: &str,
    ) -> Result<Value, String> {
        if tenant_id.is_some_and(|tenant| tenant.trim().is_empty()) {
            return Err("tenant_id must not be empty".into());
        }
        let scopes_json = serde_json::to_string(scopes).map_err(|e| e.to_string())?;
        let expires_at_text = expires_at.map(|value| value.to_string());
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_sqlite_transaction(|conn| {
                let now = self.now();
                conn.execute(
                    "INSERT INTO api_key_metadata
                     (key_id, user_id, role, tenant_id, scopes_json, created_at, created_by, revoked_at, expires_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8)
                     ON CONFLICT(key_id) DO UPDATE SET
                        user_id = excluded.user_id,
                        role = excluded.role,
                        tenant_id = excluded.tenant_id,
                        scopes_json = excluded.scopes_json,
                        created_by = excluded.created_by,
                        expires_at = excluded.expires_at",
                    params![
                        key_id,
                        user_id,
                        role,
                        tenant_id,
                        scopes_json,
                        now,
                        actor,
                        expires_at_text
                    ],
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
                    "tenant_id": tenant_id,
                    "scopes": scopes,
                    "created_at": now,
                    "created_by": actor,
                    "expires_at": expires_at,
                }))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => {
                self.with_pg_conn(|client| {
                    let mut transaction = client.transaction().map_err(|e| e.to_string())?;
                    let now = self.now();
                    transaction
                        .execute(
                            "INSERT INTO api_key_metadata
                     (key_id, user_id, role, tenant_id, scopes_json, created_at, created_by, revoked_at, expires_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, NULL, $8)
                     ON CONFLICT(key_id) DO UPDATE SET
                        user_id = excluded.user_id,
                        role = excluded.role,
                        tenant_id = excluded.tenant_id,
                        scopes_json = excluded.scopes_json,
                        created_by = excluded.created_by,
                        expires_at = excluded.expires_at",
                            &[
                                &key_id,
                                &user_id,
                                &role,
                                &tenant_id,
                                &scopes_json,
                                &now,
                                &actor,
                                &expires_at_text,
                            ],
                        )
                        .map_err(|e| e.to_string())?;
                    let details =
                        json!({"key_id": key_id, "user_id": user_id, "role": role}).to_string();
                    transaction.execute(
                    "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                     VALUES ($1, $2, $3, $4, $5)",
                    &[&now, &actor, &"api_key.record_metadata", &key_id, &details],
                ).map_err(|e| e.to_string())?;
                    transaction.commit().map_err(|e| e.to_string())?;
                    Ok(json!({
                        "key_id": key_id,
                        "user_id": user_id,
                        "role": role,
                        "tenant_id": tenant_id,
                        "scopes": scopes,
                        "created_at": now,
                        "created_by": actor,
                        "expires_at": expires_at,
                    }))
                })
            }
        }
    }

    pub fn get_api_key_metadata(&self, key_id: &str) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let result = conn.query_row(
                    "SELECT key_id, user_id, role, tenant_id, scopes_json, created_at, created_by,
                            revoked_at, last_used_at, expires_at
                     FROM api_key_metadata WHERE key_id = ?1",
                    params![key_id],
                    |row| {
                        let scopes_text: String = row.get(4)?;
                        let scopes: Vec<String> =
                            serde_json::from_str(&scopes_text).unwrap_or_default();
                        let expires_at = row
                            .get::<_, Option<String>>(9)?
                            .and_then(|value| value.parse::<f64>().ok());
                        Ok(json!({
                            "key_id": row.get::<_, String>(0)?,
                            "user_id": row.get::<_, String>(1)?,
                            "role": row.get::<_, String>(2)?,
                            "tenant_id": row.get::<_, Option<String>>(3)?,
                            "scopes": scopes,
                            "created_at": row.get::<_, String>(5)?,
                            "created_by": row.get::<_, String>(6)?,
                            "revoked_at": row.get::<_, Option<String>>(7)?,
                            "last_used_at": row.get::<_, Option<String>>(8)?,
                            "expires_at": expires_at,
                        }))
                    },
                );
                match result {
                    Ok(value) => Ok(Some(value)),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                    Err(e) => Err(e.to_string()),
                }
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT key_id, user_id, role, tenant_id, scopes_json, created_at, created_by,
                                revoked_at, last_used_at, expires_at
                         FROM api_key_metadata WHERE key_id = $1",
                        &[&key_id],
                    )
                    .map_err(|e| e.to_string())?;
                if rows.is_empty() {
                    return Ok(None);
                }
                let row = &rows[0];
                let scopes_text: String = row.get(4);
                let scopes: Vec<String> = serde_json::from_str(&scopes_text).unwrap_or_default();
                let expires_at = row
                    .get::<_, Option<String>>(9)
                    .and_then(|value| value.parse::<f64>().ok());
                Ok(Some(json!({
                    "key_id": row.get::<_, String>(0),
                    "user_id": row.get::<_, String>(1),
                    "role": row.get::<_, String>(2),
                    "tenant_id": row.get::<_, Option<String>>(3),
                    "scopes": scopes,
                    "created_at": row.get::<_, String>(5),
                    "created_by": row.get::<_, String>(6),
                    "revoked_at": row.get::<_, Option<String>>(7),
                    "last_used_at": row.get::<_, Option<String>>(8),
                    "expires_at": expires_at,
                })))
            }),
        }
    }

    pub fn list_api_key_metadata(&self, limit: i64) -> Result<Vec<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT key_id, user_id, role, tenant_id, scopes_json, created_at, created_by,
                                revoked_at, last_used_at, expires_at
                         FROM api_key_metadata
                         ORDER BY created_at DESC
                         LIMIT ?1",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![limit], |row| {
                        let scopes_text: String = row.get(4)?;
                        let scopes: Vec<String> =
                            serde_json::from_str(&scopes_text).unwrap_or_default();
                        let expires_at = row
                            .get::<_, Option<String>>(9)?
                            .and_then(|value| value.parse::<f64>().ok());
                        Ok(json!({
                            "key_id": row.get::<_, String>(0)?,
                            "user_id": row.get::<_, String>(1)?,
                            "role": row.get::<_, String>(2)?,
                            "tenant_id": row.get::<_, Option<String>>(3)?,
                            "scopes": scopes,
                            "created_at": row.get::<_, String>(5)?,
                            "created_by": row.get::<_, String>(6)?,
                            "revoked_at": row.get::<_, Option<String>>(7)?,
                            "last_used_at": row.get::<_, Option<String>>(8)?,
                            "expires_at": expires_at,
                        }))
                    })
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT key_id, user_id, role, tenant_id, scopes_json, created_at, created_by,
                                revoked_at, last_used_at, expires_at
                         FROM api_key_metadata
                         ORDER BY created_at DESC
                         LIMIT $1",
                        &[&limit],
                    )
                    .map_err(|e| e.to_string())?;
                let mut result = Vec::new();
                for row in &rows {
                    let scopes_text: String = row.get(4);
                    let scopes: Vec<String> =
                        serde_json::from_str(&scopes_text).unwrap_or_default();
                    let expires_at = row
                        .get::<_, Option<String>>(9)
                        .and_then(|value| value.parse::<f64>().ok());
                    result.push(json!({
                        "key_id": row.get::<_, String>(0),
                        "user_id": row.get::<_, String>(1),
                        "role": row.get::<_, String>(2),
                        "tenant_id": row.get::<_, Option<String>>(3),
                        "scopes": scopes,
                        "created_at": row.get::<_, String>(5),
                        "created_by": row.get::<_, String>(6),
                        "revoked_at": row.get::<_, Option<String>>(7),
                        "last_used_at": row.get::<_, Option<String>>(8),
                        "expires_at": expires_at,
                    }));
                }
                Ok(result)
            }),
        }
    }

    pub fn revoke_api_key_metadata(&self, key_id: &str, actor: &str) -> Result<bool, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_sqlite_transaction(|conn| {
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
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut transaction = client.transaction().map_err(|e| e.to_string())?;
                let now = self.now();
                let rows = transaction
                    .execute(
                        "UPDATE api_key_metadata SET revoked_at = $1 WHERE key_id = $2 AND revoked_at IS NULL",
                        &[&now, &key_id],
                    )
                    .map_err(|e| e.to_string())?;
                if rows > 0 {
                    let details = json!({"key_id": key_id}).to_string();
                    transaction.execute(
                        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                         VALUES ($1, $2, $3, $4, $5)",
                        &[&now, &actor, &"team.key.revoked", &key_id, &details],
                    ).map_err(|e| e.to_string())?;
                }
                transaction.commit().map_err(|e| e.to_string())?;
                Ok(rows > 0)
            }),
        }
    }

    /// Atomically replace one live API key with its rotated successor and
    /// append both audit records in the same store-owned transaction.
    pub fn rotate_api_key_metadata(
        &self,
        old_key_id: &str,
        new_key_id: &str,
        user_id: &str,
        role: &str,
        scopes: &[String],
        expires_at: Option<f64>,
        actor: &str,
    ) -> Result<bool, String> {
        let scopes_json = serde_json::to_string(scopes).map_err(|e| e.to_string())?;
        let expires_at_text = expires_at.map(|value| value.to_string());
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_sqlite_transaction(|conn| {
                let now = self.now();
                let old_tenant_id: Option<String> = conn
                    .query_row(
                        "SELECT tenant_id FROM api_key_metadata WHERE key_id=?1",
                        params![old_key_id],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?;
                let revoked = conn
                    .execute(
                        "UPDATE api_key_metadata SET revoked_at = ?1
                         WHERE key_id = ?2 AND revoked_at IS NULL",
                        params![now, old_key_id],
                    )
                    .map_err(|e| e.to_string())?;
                if revoked == 0 {
                    return Ok(false);
                }
                conn.execute(
                    "INSERT INTO api_key_metadata
                     (key_id, user_id, role, tenant_id, scopes_json, created_at, created_by, revoked_at, expires_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8)",
                    params![
                        new_key_id,
                        user_id,
                        role,
                        old_tenant_id,
                        scopes_json,
                        now,
                        actor,
                        expires_at_text
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    conn,
                    &now,
                    actor,
                    "team.key.revoked",
                    old_key_id,
                    &json!({"key_id": old_key_id, "rotated_to": new_key_id}),
                )?;
                append_audit_locked(
                    conn,
                    &now,
                    actor,
                    "api_key.record_metadata",
                    new_key_id,
                    &json!({"key_id": new_key_id, "user_id": user_id, "role": role}),
                )?;
                Ok(true)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut transaction = client.transaction().map_err(|e| e.to_string())?;
                let now = self.now();
                let old_tenant_id: Option<String> = transaction
                    .query_opt(
                        "SELECT tenant_id FROM api_key_metadata WHERE key_id=$1",
                        &[&old_key_id],
                    )
                    .map_err(|e| e.to_string())?
                    .map(|row| row.get(0));
                let revoked = transaction
                    .execute(
                        "UPDATE api_key_metadata SET revoked_at = $1
                         WHERE key_id = $2 AND revoked_at IS NULL",
                        &[&now, &old_key_id],
                    )
                    .map_err(|e| e.to_string())?;
                if revoked == 0 {
                    transaction.rollback().map_err(|e| e.to_string())?;
                    return Ok(false);
                }
                transaction
                    .execute(
                        "INSERT INTO api_key_metadata
                         (key_id, user_id, role, tenant_id, scopes_json, created_at, created_by, revoked_at, expires_at)
                         VALUES ($1, $2, $3, $4, $5, $6, $7, NULL, $8)",
                        &[
                            &new_key_id,
                            &user_id,
                            &role,
                            &old_tenant_id,
                            &scopes_json,
                            &now,
                            &actor,
                            &expires_at_text,
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                let revoked_details = json!({"key_id": old_key_id, "rotated_to": new_key_id}).to_string();
                transaction
                    .execute(
                        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                         VALUES ($1, $2, $3, $4, $5)",
                        &[
                            &now,
                            &actor,
                            &"team.key.revoked",
                            &old_key_id,
                            &revoked_details,
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                let created_details =
                    json!({"key_id": new_key_id, "user_id": user_id, "role": role}).to_string();
                transaction
                    .execute(
                        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                         VALUES ($1, $2, $3, $4, $5)",
                        &[
                            &now,
                            &actor,
                            &"api_key.record_metadata",
                            &new_key_id,
                            &created_details,
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                transaction.commit().map_err(|e| e.to_string())?;
                Ok(true)
            }),
        }
    }

    pub fn delete_api_key_metadata(&self, key_id: &str, actor: &str) -> Result<bool, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_sqlite_transaction(|conn| {
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
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut transaction = client.transaction().map_err(|e| e.to_string())?;
                let now = self.now();
                let rows = transaction
                    .execute("DELETE FROM api_key_metadata WHERE key_id = $1", &[&key_id])
                    .map_err(|e| e.to_string())?;
                if rows > 0 {
                    let details = json!({"key_id": key_id}).to_string();
                    transaction.execute(
                        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                         VALUES ($1, $2, $3, $4, $5)",
                        &[&now, &actor, &"team.key.deleted", &key_id, &details],
                    ).map_err(|e| e.to_string())?;
                }
                transaction.commit().map_err(|e| e.to_string())?;
                Ok(rows > 0)
            }),
        }
    }

    pub fn update_api_key_scopes(
        &self,
        key_id: &str,
        scopes: &[String],
        actor: &str,
    ) -> Result<bool, String> {
        let scopes_json = serde_json::to_string(scopes).map_err(|e| e.to_string())?;
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_sqlite_transaction(|conn| {
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
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut transaction = client.transaction().map_err(|e| e.to_string())?;
                let now = self.now();
                let rows = transaction
                    .execute(
                        "UPDATE api_key_metadata SET scopes_json = $1 WHERE key_id = $2",
                        &[&scopes_json, &key_id],
                    )
                    .map_err(|e| e.to_string())?;
                if rows > 0 {
                    let details = json!({"key_id": key_id, "scopes": scopes}).to_string();
                    transaction.execute(
                        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                         VALUES ($1, $2, $3, $4, $5)",
                        &[&now, &actor, &"team.key.scopes_updated", &key_id, &details],
                    ).map_err(|e| e.to_string())?;
                }
                transaction.commit().map_err(|e| e.to_string())?;
                Ok(rows > 0)
            }),
        }
    }

    pub fn touch_api_key_last_used(&self, key_id: &str) -> Result<(), String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute(
                    "UPDATE api_key_metadata SET last_used_at = ?1 WHERE key_id = ?2",
                    params![self.now(), key_id],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .execute(
                        "UPDATE api_key_metadata SET last_used_at = $1 WHERE key_id = $2",
                        &[&self.now(), &key_id],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(())
            }),
        }
    }
}
