use rusqlite::{params, Row};
use serde_json::{json, Value};

use super::{collect_values, DatabaseConnection, LocalProductStore};

impl LocalProductStore {
    pub fn audit_events(&self, limit: i64) -> Result<Vec<Value>, String> {
        self.audit_events_with_offset(limit, 0)
    }

    pub fn audit_events_with_offset(&self, limit: i64, offset: i64) -> Result<Vec<Value>, String> {
        self.search_audit_events(limit, offset, None)
    }

    pub fn search_audit_events(
        &self,
        limit: i64,
        offset: i64,
        search: Option<&str>,
    ) -> Result<Vec<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                if let Some(raw_search) = search {
                    let trimmed = raw_search.trim().to_lowercase();
                    if !trimmed.is_empty() {
                        let needle = format!("%{trimmed}%");
                        let mut stmt = conn
                            .prepare(
                                "SELECT audit_id, created_at, actor, action, resource, details_json
                                 FROM audit_log
                                 WHERE lower(actor) LIKE ?1
                                    OR lower(action) LIKE ?1
                                    OR lower(resource) LIKE ?1
                                    OR lower(details_json) LIKE ?1
                                 ORDER BY audit_id DESC
                                 LIMIT ?2 OFFSET ?3",
                            )
                            .map_err(|e| e.to_string())?;
                        let rows = stmt
                            .query_map(params![needle, limit, offset], audit_row_json)
                            .map_err(|e| e.to_string())?;
                        return collect_values(rows);
                    }
                }

                let mut stmt = conn
                    .prepare(
                        "SELECT audit_id, created_at, actor, action, resource, details_json
                         FROM audit_log
                         ORDER BY audit_id DESC
                         LIMIT ?1 OFFSET ?2",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![limit, offset], audit_row_json)
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                if let Some(raw_search) = search {
                    let trimmed = raw_search.trim().to_lowercase();
                    if !trimmed.is_empty() {
                        let needle = format!("%{trimmed}%");
                        let rows = client
                            .query(
                                "SELECT audit_id, created_at, actor, action, resource, details_json
                                 FROM audit_log
                                 WHERE lower(actor) LIKE $1
                                    OR lower(action) LIKE $1
                                    OR lower(resource) LIKE $1
                                    OR lower(details_json) LIKE $1
                                 ORDER BY audit_id DESC
                                 LIMIT $2 OFFSET $3",
                                &[&needle, &limit, &offset],
                            )
                            .map_err(|e| e.to_string())?;
                        return pg_audit_rows(rows);
                    }
                }

                let rows = client
                    .query(
                        "SELECT audit_id, created_at, actor, action, resource, details_json
                         FROM audit_log
                         ORDER BY audit_id DESC
                         LIMIT $1 OFFSET $2",
                        &[&limit, &offset],
                    )
                    .map_err(|e| e.to_string())?;
                pg_audit_rows(rows)
            }),
        }
    }

    pub fn append_audit(
        &self,
        actor: &str,
        action: &str,
        resource: &str,
        details: &Value,
    ) -> Result<Value, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let now = self.now();
                let audit_id =
                    super::append_audit_locked(conn, &now, actor, action, resource, details)?;
                Ok(json!({
                    "audit_id": audit_id,
                    "created_at": now,
                    "actor": actor,
                    "action": action,
                    "resource": resource,
                    "details": details,
                }))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let now = self.now();
                let details_json = details.to_string();
                let row = client
                    .query_one(
                        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                         VALUES ($1, $2, $3, $4, $5)
                         RETURNING audit_id",
                        &[&now, &actor, &action, &resource, &details_json],
                    )
                    .map_err(|e| e.to_string())?;
                let audit_id: i64 = row.get(0);
                Ok(json!({
                    "audit_id": audit_id,
                    "created_at": now,
                    "actor": actor,
                    "action": action,
                    "resource": resource,
                    "details": details,
                }))
            }),
        }
    }
}

fn audit_row_json(row: &Row<'_>) -> rusqlite::Result<Value> {
    let details_text: String = row.get(5)?;
    let details: Value = serde_json::from_str(&details_text).unwrap_or(Value::Null);
    Ok(json!({
        "audit_id": row.get::<_, i64>(0)?,
        "created_at": row.get::<_, String>(1)?,
        "actor": row.get::<_, String>(2)?,
        "action": row.get::<_, String>(3)?,
        "resource": row.get::<_, String>(4)?,
        "details": details,
    }))
}

#[cfg(feature = "pg")]
fn pg_audit_rows(rows: Vec<postgres::Row>) -> Result<Vec<Value>, String> {
    let mut result = Vec::new();
    for row in &rows {
        let details_text: String = row.get(5);
        let details: Value = serde_json::from_str(&details_text).unwrap_or(Value::Null);
        result.push(json!({
            "audit_id": row.get::<_, i64>(0),
            "created_at": row.get::<_, String>(1),
            "actor": row.get::<_, String>(2),
            "action": row.get::<_, String>(3),
            "resource": row.get::<_, String>(4),
            "details": details,
        }));
    }
    Ok(result)
}
