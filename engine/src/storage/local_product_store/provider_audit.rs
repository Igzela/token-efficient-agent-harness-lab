use rusqlite::params;
use serde_json::{json, Value};

use super::{collect_values, DatabaseConnection, LocalProductStore};

impl LocalProductStore {
    pub fn record_provider_audit_event(
        &self,
        event: &crate::provider::ProviderAuditEvent,
    ) -> Result<(), String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute(
                    "INSERT OR IGNORE INTO provider_audit_events
                     (event_id, dispatch_id, provider_id, event_type,
                      input_token_count, output_token_count, cost, currency,
                      latency_ms, error_domain, redaction_status, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        event.event_id,
                        event.dispatch_id,
                        event.provider_id,
                        event.event_type,
                        event.input_token_count,
                        event.output_token_count,
                        event.cost,
                        event.currency,
                        event.latency_ms,
                        event.error_domain,
                        event.redaction_status,
                        event.created_at,
                    ],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .execute(
                        "INSERT INTO provider_audit_events
                     (event_id, dispatch_id, provider_id, event_type,
                      input_token_count, output_token_count, cost, currency,
                      latency_ms, error_domain, redaction_status, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                     ON CONFLICT DO NOTHING",
                        &[
                            &event.event_id,
                            &event.dispatch_id,
                            &event.provider_id,
                            &event.event_type,
                            &event.input_token_count,
                            &event.output_token_count,
                            &event.cost,
                            &event.currency,
                            &event.latency_ms,
                            &event.error_domain,
                            &event.redaction_status,
                            &event.created_at,
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(())
            }),
        }
    }

    pub fn provider_audit_events(&self, limit: i64) -> Result<Vec<Value>, String> {
        self.provider_audit_events_with_offset(limit, 0)
    }

    pub fn provider_audit_events_with_offset(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT event_id, dispatch_id, provider_id, event_type,
                                input_token_count, output_token_count, cost, currency,
                                latency_ms, error_domain, redaction_status, created_at
                         FROM provider_audit_events
                         ORDER BY created_at DESC
                         LIMIT ?1 OFFSET ?2",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![limit, offset], |row| {
                        Ok(json!({
                            "event_id": row.get::<_, String>(0)?,
                            "dispatch_id": row.get::<_, String>(1)?,
                            "provider_id": row.get::<_, String>(2)?,
                            "event_type": row.get::<_, String>(3)?,
                            "input_token_count": row.get::<_, Option<i64>>(4)?,
                            "output_token_count": row.get::<_, Option<i64>>(5)?,
                            "cost": row.get::<_, Option<f64>>(6)?,
                            "currency": row.get::<_, Option<String>>(7)?,
                            "latency_ms": row.get::<_, Option<i64>>(8)?,
                            "error_domain": row.get::<_, Option<String>>(9)?,
                            "redaction_status": row.get::<_, String>(10)?,
                            "created_at": row.get::<_, String>(11)?,
                        }))
                    })
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT event_id, dispatch_id, provider_id, event_type,
                                input_token_count, output_token_count, cost, currency,
                                latency_ms, error_domain, redaction_status, created_at
                         FROM provider_audit_events
                         ORDER BY created_at DESC
                         LIMIT $1 OFFSET $2",
                        &[&limit, &offset],
                    )
                    .map_err(|e| e.to_string())?;
                pg_provider_audit_rows(rows)
            }),
        }
    }

    pub fn provider_audit_events_for_dispatch(
        &self,
        dispatch_id: &str,
    ) -> Result<Vec<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT event_id, dispatch_id, provider_id, event_type,
                                input_token_count, output_token_count, cost, currency,
                                latency_ms, error_domain, redaction_status, created_at
                         FROM provider_audit_events
                         WHERE dispatch_id = ?1
                         ORDER BY created_at DESC",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![dispatch_id], |row| {
                        Ok(json!({
                            "event_id": row.get::<_, String>(0)?,
                            "dispatch_id": row.get::<_, String>(1)?,
                            "provider_id": row.get::<_, String>(2)?,
                            "event_type": row.get::<_, String>(3)?,
                            "input_token_count": row.get::<_, Option<i64>>(4)?,
                            "output_token_count": row.get::<_, Option<i64>>(5)?,
                            "cost": row.get::<_, Option<f64>>(6)?,
                            "currency": row.get::<_, Option<String>>(7)?,
                            "latency_ms": row.get::<_, Option<i64>>(8)?,
                            "error_domain": row.get::<_, Option<String>>(9)?,
                            "redaction_status": row.get::<_, String>(10)?,
                            "created_at": row.get::<_, String>(11)?,
                        }))
                    })
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT event_id, dispatch_id, provider_id, event_type,
                                input_token_count, output_token_count, cost, currency,
                                latency_ms, error_domain, redaction_status, created_at
                         FROM provider_audit_events
                         WHERE dispatch_id = $1
                         ORDER BY created_at DESC",
                        &[&dispatch_id],
                    )
                    .map_err(|e| e.to_string())?;
                pg_provider_audit_rows(rows)
            }),
        }
    }
}

#[cfg(feature = "pg")]
fn pg_provider_audit_rows(rows: Vec<postgres::Row>) -> Result<Vec<Value>, String> {
    let mut result = Vec::new();
    for row in &rows {
        result.push(json!({
            "event_id": row.get::<_, String>(0),
            "dispatch_id": row.get::<_, String>(1),
            "provider_id": row.get::<_, String>(2),
            "event_type": row.get::<_, String>(3),
            "input_token_count": row.get::<_, Option<i64>>(4),
            "output_token_count": row.get::<_, Option<i64>>(5),
            "cost": row.get::<_, Option<f64>>(6),
            "currency": row.get::<_, Option<String>>(7),
            "latency_ms": row.get::<_, Option<i64>>(8),
            "error_domain": row.get::<_, Option<String>>(9),
            "redaction_status": row.get::<_, String>(10),
            "created_at": row.get::<_, String>(11),
        }));
    }
    Ok(result)
}
