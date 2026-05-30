use rusqlite::params;
use serde_json::{json, Value};

use super::{collect_values, LocalProductStore};

impl LocalProductStore {
    pub fn audit_events(&self, limit: i64) -> Result<Vec<Value>, String> {
        self.audit_events_with_offset(limit, 0)
    }

    pub fn audit_events_with_offset(&self, limit: i64, offset: i64) -> Result<Vec<Value>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT audit_id, created_at, actor, action, resource, details_json
                     FROM audit_log
                     ORDER BY audit_id DESC
                     LIMIT ?1 OFFSET ?2",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![limit, offset], |row| {
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
                })
                .map_err(|e| e.to_string())?;
            collect_values(rows)
        })
    }

    pub fn append_audit(
        &self,
        actor: &str,
        action: &str,
        resource: &str,
        details: &Value,
    ) -> Result<Value, String> {
        self.with_conn(|conn| {
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
        })
    }
}
