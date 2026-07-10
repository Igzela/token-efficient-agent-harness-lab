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
                let eid: &str = &event.event_id;
                let did: &str = &event.dispatch_id;
                let pid: &str = &event.provider_id;
                let etype: &str = &event.event_type;
                let itc: Option<i32> = event.input_token_count.map(|v| v as i32);
                let otc: Option<i32> = event.output_token_count.map(|v| v as i32);
                let cost = event.cost;
                let cur = event.currency.as_deref();
                let lat: Option<i32> = event.latency_ms.map(|v| v as i32);
                let edom = event.error_domain.as_deref();
                let rs: &str = &event.redaction_status;
                let cat: &str = &event.created_at;
                let params: Vec<&(dyn postgres::types::ToSql + Sync)> = vec![
                    &eid, &did, &pid, &etype, &itc, &otc, &cost, &cur, &lat, &edom, &rs, &cat,
                ];
                client
                    .execute(
                        "INSERT INTO provider_audit_events
                     (event_id, dispatch_id, provider_id, event_type,
                      input_token_count, output_token_count, cost, currency,
                      latency_ms, error_domain, redaction_status, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                     ON CONFLICT DO NOTHING",
                        &params,
                    )
                    .map_err(|e| e.to_string())?;
                Ok(())
            }),
        }
    }

    pub fn provider_audit_events(&self, limit: i64) -> Result<Vec<Value>, String> {
        self.provider_audit_events_with_offset(limit, 0)
    }

    pub fn daily_provider_audit_cost_usd(&self, date_prefix: &str) -> Result<f64, String> {
        if date_prefix.len() != 10
            || !date_prefix.chars().enumerate().all(|(index, ch)| {
                matches!(index, 4 | 7) && ch == '-'
                    || !matches!(index, 4 | 7) && ch.is_ascii_digit()
            })
        {
            return Err("date prefix must use YYYY-MM-DD".to_string());
        }
        let pattern = format!("{date_prefix}%");
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT COALESCE(SUM(cost), 0.0)
                     FROM provider_audit_events
                     WHERE event_type = 'response_received'
                       AND cost IS NOT NULL
                       AND created_at LIKE ?1",
                    params![pattern],
                    |row| row.get::<_, f64>(0),
                )
                .map_err(|e| e.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_one(
                        "SELECT COALESCE(SUM(cost), 0.0)::DOUBLE PRECISION
                         FROM provider_audit_events
                         WHERE event_type = 'response_received'
                           AND cost IS NOT NULL
                           AND created_at LIKE $1",
                        &[&pattern],
                    )
                    .map(|row| row.get::<_, f64>(0))
                    .map_err(|e| e.to_string())
            }),
        }
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
                let p1: &str = dispatch_id;
                let rows = client
                    .query(
                        "SELECT event_id, dispatch_id, provider_id, event_type,
                                input_token_count, output_token_count, cost, currency,
                                latency_ms, error_domain, redaction_status, created_at
                         FROM provider_audit_events
                         WHERE dispatch_id = $1
                         ORDER BY created_at DESC",
                        &[&p1],
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
        let itc: Option<i32> = row.get(4);
        let otc: Option<i32> = row.get(5);
        let lat: Option<i32> = row.get(8);
        result.push(json!({
            "event_id": row.get::<_, String>(0),
            "dispatch_id": row.get::<_, String>(1),
            "provider_id": row.get::<_, String>(2),
            "event_type": row.get::<_, String>(3),
            "input_token_count": itc.map(|v| v as i64),
            "output_token_count": otc.map(|v| v as i64),
            "cost": row.get::<_, Option<f64>>(6),
            "currency": row.get::<_, Option<String>>(7),
            "latency_ms": lat.map(|v| v as i64),
            "error_domain": row.get::<_, Option<String>>(9),
            "redaction_status": row.get::<_, String>(10),
            "created_at": row.get::<_, String>(11),
        }));
    }
    Ok(result)
}
