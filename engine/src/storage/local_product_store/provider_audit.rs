use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use super::{collect_values, DatabaseConnection, LocalProductStore};

type ProviderReservationBinding = (String, String, String, Option<f64>, Option<String>, String);

impl LocalProductStore {
    pub fn record_provider_audit_event(
        &self,
        event: &crate::provider::ProviderAuditEvent,
    ) -> Result<(), String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let changed = conn.execute(
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
                if changed == 0 {
                    let existing = conn.query_row(
                        "SELECT event_id,dispatch_id,provider_id,event_type,input_token_count,output_token_count,cost,currency,latency_ms,error_domain,redaction_status,created_at FROM provider_audit_events WHERE event_id=?1",
                        params![event.event_id],
                        provider_event_binding_sqlite,
                    ).map_err(|error|error.to_string())?;
                    if existing != provider_event_binding(event) {
                        return Err("provider audit event id binding conflict".to_string());
                    }
                }
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
                let changed = client
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
                if changed == 0 {
                    let row=client.query_one("SELECT event_id,dispatch_id,provider_id,event_type,input_token_count,output_token_count,cost,currency,latency_ms,error_domain,redaction_status,created_at FROM provider_audit_events WHERE event_id=$1",&[&event.event_id]).map_err(|error|error.to_string())?;
                    if provider_event_binding_pg(&row)!=provider_event_binding(event){return Err("provider audit event id binding conflict".to_string())}
                }
                Ok(())
            }),
        }
    }

    pub fn reserve_provider_audit_cost(
        &self,
        event: &crate::provider::ProviderAuditEvent,
        per_call_cap_usd: f64,
        daily_cap_usd: f64,
    ) -> Result<(), String> {
        let reserved_cost = event
            .cost
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| {
                "provider cost reservation must be finite and non-negative".to_string()
            })?;
        if !per_call_cap_usd.is_finite()
            || per_call_cap_usd <= 0.0
            || !daily_cap_usd.is_finite()
            || daily_cap_usd <= 0.0
        {
            return Err(
                "provider cost reservation requires positive per-call and daily caps".to_string(),
            );
        }
        if reserved_cost > per_call_cap_usd {
            return Err("agent decision provider per-call cost cap exceeded".to_string());
        }
        if event.event_type != "request_reserved" || event.created_at.len() < 10 {
            return Err("invalid provider cost reservation audit event".to_string());
        }
        let date_prefix = &event.created_at[..10];
        let pattern = format!("{date_prefix}%");

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(
                    conn,
                    rusqlite::TransactionBehavior::Immediate,
                )
                .map_err(|error| error.to_string())?;
                let existing: Option<ProviderReservationBinding> = tx
                    .query_row(
                        "SELECT dispatch_id, provider_id, event_type, cost, currency, created_at
                         FROM provider_audit_events WHERE event_id = ?1",
                        params![event.event_id],
                        |row| {
                            Ok((
                                row.get(0)?,
                                row.get(1)?,
                                row.get(2)?,
                                row.get(3)?,
                                row.get(4)?,
                                row.get(5)?,
                            ))
                        },
                    )
                    .optional()
                    .map_err(|error| error.to_string())?;
                if let Some(existing) = existing {
                    return validate_existing_reservation(event, &existing);
                }
                let dispatch_cost: f64 = tx
                    .query_row(
                        "SELECT COALESCE(SUM(COALESCE(estimated_cost_usd, reserved_cost)), 0.0)
                         FROM dispatch_history WHERE created_at LIKE ?1",
                        params![pattern],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                let reservation_cost: f64 = tx
                    .query_row(
                        "SELECT COALESCE(SUM(cost), 0.0) FROM provider_audit_events
                         WHERE event_type='request_reserved' AND created_at LIKE ?1",
                        params![pattern],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                let adaptive_raw: Option<String> = tx
                    .query_row(
                        "SELECT value_json FROM local_config WHERE key='adaptive_fusion_observations'",
                        [],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?;
                let adaptive_cost = adaptive_daily_cost(adaptive_raw.as_deref(), date_prefix)?;
                let today_total = dispatch_cost + reservation_cost + adaptive_cost;
                if today_total + reserved_cost > daily_cap_usd {
                    return Err("agent decision provider daily cost cap exceeded".to_string());
                }
                tx.execute(
                    "INSERT INTO provider_audit_events
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
                .map_err(|error| error.to_string())?;
                tx.commit().map_err(|error| error.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                tx.execute("SELECT pg_advisory_xact_lock(684214091)", &[])
                    .map_err(|error| error.to_string())?;
                let existing = tx
                    .query_opt(
                        "SELECT dispatch_id, provider_id, event_type,
                                cost::DOUBLE PRECISION, currency, created_at
                         FROM provider_audit_events WHERE event_id = $1",
                        &[&event.event_id],
                    )
                    .map_err(|error| error.to_string())?
                    .map(|row| {
                        (
                            row.get::<_, String>(0),
                            row.get::<_, String>(1),
                            row.get::<_, String>(2),
                            row.get::<_, Option<f64>>(3),
                            row.get::<_, Option<String>>(4),
                            row.get::<_, String>(5),
                        )
                    });
                if let Some(existing) = existing {
                    return validate_existing_reservation(event, &existing);
                }
                let dispatch_cost: f64 = tx
                    .query_one(
                        "SELECT COALESCE(SUM(COALESCE(estimated_cost_usd, reserved_cost)), 0.0)::DOUBLE PRECISION
                         FROM dispatch_history WHERE created_at LIKE $1",
                        &[&pattern],
                    )
                    .map_err(|error| error.to_string())?
                    .get(0);
                let reservation_cost: f64 = tx
                    .query_one(
                        "SELECT COALESCE(SUM(cost), 0.0)::DOUBLE PRECISION FROM provider_audit_events
                         WHERE event_type='request_reserved' AND created_at LIKE $1",
                        &[&pattern],
                    )
                    .map_err(|error| error.to_string())?
                    .get(0);
                let adaptive_raw = tx
                    .query_opt(
                        "SELECT value_json FROM local_config
                         WHERE key='adaptive_fusion_observations' FOR SHARE",
                        &[],
                    )
                    .map_err(|error| error.to_string())?
                    .map(|row| row.get::<_, String>(0));
                let adaptive_cost = adaptive_daily_cost(adaptive_raw.as_deref(), date_prefix)?;
                let today_total = dispatch_cost + reservation_cost + adaptive_cost;
                if today_total + reserved_cost > daily_cap_usd {
                    return Err("agent decision provider daily cost cap exceeded".to_string());
                }
                let input_tokens = event.input_token_count.map(|value| value as i32);
                let output_tokens = event.output_token_count.map(|value| value as i32);
                let latency_ms = event.latency_ms.map(|value| value as i32);
                tx.execute(
                    "INSERT INTO provider_audit_events
                     (event_id, dispatch_id, provider_id, event_type,
                      input_token_count, output_token_count, cost, currency,
                      latency_ms, error_domain, redaction_status, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
                    &[
                        &event.event_id,
                        &event.dispatch_id,
                        &event.provider_id,
                        &event.event_type,
                        &input_tokens,
                        &output_tokens,
                        &event.cost,
                        &event.currency,
                        &latency_ms,
                        &event.error_domain,
                        &event.redaction_status,
                        &event.created_at,
                    ],
                )
                .map_err(|error| error.to_string())?;
                tx.commit().map_err(|error| error.to_string())
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
                         ORDER BY created_at DESC, event_id DESC
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
                         ORDER BY created_at DESC, event_id DESC
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
                         ORDER BY created_at DESC, event_id DESC",
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
                         ORDER BY created_at DESC, event_id DESC",
                        &[&p1],
                    )
                    .map_err(|e| e.to_string())?;
                pg_provider_audit_rows(rows)
            }),
        }
    }
}

fn provider_event_binding(event: &crate::provider::ProviderAuditEvent) -> Value {
    json!({"event_id":event.event_id,"dispatch_id":event.dispatch_id,"provider_id":event.provider_id,"event_type":event.event_type,"input_token_count":event.input_token_count,"output_token_count":event.output_token_count,"cost":event.cost,"currency":event.currency,"latency_ms":event.latency_ms,"error_domain":event.error_domain,"redaction_status":event.redaction_status,"created_at":event.created_at})
}

fn provider_event_binding_sqlite(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    Ok(
        json!({"event_id":row.get::<_,String>(0)?,"dispatch_id":row.get::<_,String>(1)?,"provider_id":row.get::<_,String>(2)?,"event_type":row.get::<_,String>(3)?,"input_token_count":row.get::<_,Option<i64>>(4)?,"output_token_count":row.get::<_,Option<i64>>(5)?,"cost":row.get::<_,Option<f64>>(6)?,"currency":row.get::<_,Option<String>>(7)?,"latency_ms":row.get::<_,Option<i64>>(8)?,"error_domain":row.get::<_,Option<String>>(9)?,"redaction_status":row.get::<_,String>(10)?,"created_at":row.get::<_,String>(11)?}),
    )
}

#[cfg(feature = "pg")]
fn provider_event_binding_pg(row: &postgres::Row) -> Value {
    json!({"event_id":row.get::<_,String>(0),"dispatch_id":row.get::<_,String>(1),"provider_id":row.get::<_,String>(2),"event_type":row.get::<_,String>(3),"input_token_count":row.get::<_,Option<i32>>(4).map(i64::from),"output_token_count":row.get::<_,Option<i32>>(5).map(i64::from),"cost":row.get::<_,Option<f64>>(6),"currency":row.get::<_,Option<String>>(7),"latency_ms":row.get::<_,Option<i32>>(8).map(i64::from),"error_domain":row.get::<_,Option<String>>(9),"redaction_status":row.get::<_,String>(10),"created_at":row.get::<_,String>(11)})
}

fn validate_existing_reservation(
    event: &crate::provider::ProviderAuditEvent,
    existing: &ProviderReservationBinding,
) -> Result<(), String> {
    let same_cost = match (event.cost, existing.3) {
        (Some(expected), Some(actual)) => (expected - actual).abs() <= f64::EPSILON,
        (None, None) => true,
        _ => false,
    };
    if existing.0 == event.dispatch_id
        && existing.1 == event.provider_id
        && existing.2 == event.event_type
        && same_cost
        && existing.4 == event.currency
        && existing.5 == event.created_at
    {
        Ok(())
    } else {
        Err("provider cost reservation identity conflicts with persisted evidence".to_string())
    }
}

fn adaptive_daily_cost(raw: Option<&str>, date_prefix: &str) -> Result<f64, String> {
    let Some(raw) = raw else {
        return Ok(0.0);
    };
    let values: Value = serde_json::from_str(raw)
        .map_err(|error| format!("invalid adaptive observation cost evidence: {error}"))?;
    let observations = values
        .as_array()
        .ok_or_else(|| "adaptive observation cost evidence must be an array".to_string())?;
    observations.iter().try_fold(0.0, |total, observation| {
        let created_at = observation
            .get("created_at")
            .and_then(Value::as_str)
            .ok_or_else(|| "adaptive observation is missing created_at".to_string())?;
        let cost = observation
            .get("cost_usd")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| "adaptive observation has invalid cost_usd".to_string())?;
        Ok(if created_at.starts_with(date_prefix) {
            total + cost
        } else {
            total
        })
    })
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
