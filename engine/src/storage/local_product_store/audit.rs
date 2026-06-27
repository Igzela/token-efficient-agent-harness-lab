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

    pub fn search_audit_events_by_run(
        &self,
        run_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>, String> {
        // Broad SQL prefilter — gets candidate rows cheaply
        let needle = format!("%{run_id}%");
        let candidates = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT audit_id, created_at, actor, action, resource, details_json
                         FROM audit_log
                         WHERE resource LIKE ?1 OR details_json LIKE ?1
                         ORDER BY audit_id DESC
                         LIMIT ?2 OFFSET ?3",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![needle, limit, offset], audit_row_json)
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT audit_id, created_at, actor, action, resource, details_json
                         FROM audit_log
                         WHERE resource LIKE $1 OR details_json LIKE $1
                         ORDER BY audit_id DESC
                         LIMIT $2 OFFSET $3",
                        &[&needle, &limit, &offset],
                    )
                    .map_err(|e| e.to_string())?;
                pg_audit_rows(rows)
            })?,
        };

        // Exact attribution filter — eliminates substring collisions
        Ok(candidates
            .into_iter()
            .filter(|event| audit_event_matches_run(event, run_id))
            .collect())
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

/// Exact run attribution check. Returns true only if the audit event can be
/// confidently attributed to `run_id` via:
/// 1. `details.run_id == run_id` (exact JSON string match), OR
/// 2. `resource` contains `run_id` as a full path segment (split on `/`)
///
/// Substring collisions are rejected: `run-1` does not match `run-10`.
pub fn audit_event_matches_run(event: &Value, run_id: &str) -> bool {
    // Check details.run_id exact match
    if let Some(details_run_id) = event
        .get("details")
        .and_then(|d| d.get("run_id"))
        .and_then(|v| v.as_str())
    {
        if details_run_id == run_id {
            return true;
        }
    }

    // Check resource path segment match
    if let Some(resource) = event.get("resource").and_then(|v| v.as_str()) {
        if resource_path_contains_segment(resource, run_id) {
            return true;
        }
    }

    false
}

/// Returns true if `segment` appears as a full path segment in `resource`.
/// Splits on `/` and compares each segment for exact equality.
/// Example: "node/run-1/step-1" contains "run-1" but not "run-10".
fn resource_path_contains_segment(resource: &str, segment: &str) -> bool {
    resource.split('/').any(|s| s == segment)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_details_run_id() {
        let event = json!({
            "resource": "node/other",
            "details": {"run_id": "run-1"}
        });
        assert!(audit_event_matches_run(&event, "run-1"));
        assert!(!audit_event_matches_run(&event, "run-10"));
        assert!(!audit_event_matches_run(&event, "run-A"));
    }

    #[test]
    fn test_matches_resource_segment() {
        let event = json!({
            "resource": "agent_state/agent-1/run-1",
            "details": {}
        });
        assert!(audit_event_matches_run(&event, "run-1"));
        assert!(!audit_event_matches_run(&event, "run-10"));
    }

    #[test]
    fn test_resource_segment_not_substring() {
        let event = json!({
            "resource": "node/run-10/step-1",
            "details": {}
        });
        assert!(!audit_event_matches_run(&event, "run-1"));
        assert!(audit_event_matches_run(&event, "run-10"));
    }

    #[test]
    fn test_resource_segment_run_a_not_run_a_old() {
        let event = json!({
            "resource": "node/run-A-old/step-1",
            "details": {}
        });
        assert!(!audit_event_matches_run(&event, "run-A"));
        assert!(audit_event_matches_run(&event, "run-A-old"));
    }

    #[test]
    fn test_unrelated_details_text_rejected() {
        let event = json!({
            "resource": "node/other",
            "details": {"note": "this mentions run-1 in text"}
        });
        assert!(!audit_event_matches_run(&event, "run-1"));
    }

    #[test]
    fn test_details_run_id_takes_priority_over_resource() {
        let event = json!({
            "resource": "node/unrelated",
            "details": {"run_id": "run-42"}
        });
        assert!(audit_event_matches_run(&event, "run-42"));
        assert!(!audit_event_matches_run(&event, "run-99"));
    }

    #[test]
    fn test_no_details_no_resource_match() {
        let event = json!({
            "resource": "system/global",
            "details": {}
        });
        assert!(!audit_event_matches_run(&event, "run-1"));
    }

    #[test]
    fn test_resource_path_contains_segment() {
        assert!(resource_path_contains_segment(
            "agent_state/agent-1/run-1",
            "run-1"
        ));
        assert!(resource_path_contains_segment("node/run-1/step-1", "run-1"));
        assert!(!resource_path_contains_segment(
            "node/run-10/step-1",
            "run-1"
        ));
        assert!(!resource_path_contains_segment(
            "node/run-A-old/step-1",
            "run-A"
        ));
        assert!(resource_path_contains_segment("run-1", "run-1"));
        assert!(!resource_path_contains_segment("run-10", "run-1"));
    }
}
