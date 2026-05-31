use rusqlite::{params, Row};
use serde_json::{json, Value};

use super::{append_audit_locked, collect_values, str_at, LocalProductStore};

impl LocalProductStore {
    pub fn record_dispatch(
        &self,
        raw_request: &str,
        request_source: &str,
        bundle: &Value,
        actor: &str,
    ) -> Result<Value, String> {
        let dispatch_id = str_at(bundle, &["record", "dispatch_id"]).unwrap_or("unknown");
        let default_created_at = self.now();
        let created_at = str_at(bundle, &["record", "created_at"]).unwrap_or(&default_created_at);
        let final_status = str_at(bundle, &["record", "final_status"]).unwrap_or("unknown");
        let selected_tier = str_at(bundle, &["decision", "selected_tier"]).unwrap_or("unknown");
        let risk_level = str_at(bundle, &["analysis", "risk_level"]).unwrap_or("unknown");
        let reserved_cost = bundle
            .pointer("/decision/budget_reservation/reserved_cost")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let bundle_json = serde_json::to_string(bundle).map_err(|e| e.to_string())?;

        let input_tokens = bundle
            .pointer("/execution_result/input_tokens")
            .and_then(Value::as_i64);
        let output_tokens = bundle
            .pointer("/execution_result/output_tokens")
            .and_then(Value::as_i64);
        let estimated_cost_usd = bundle
            .pointer("/execution_result/estimated_cost")
            .and_then(Value::as_f64);
        let executor_type = bundle
            .pointer("/execution_result/executor_type")
            .and_then(Value::as_str)
            .unwrap_or("noop");
        let latency_ms = bundle
            .pointer("/execution_result/latency_ms")
            .and_then(Value::as_i64);

        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO dispatch_history
                 (dispatch_id, created_at, raw_request, request_source, final_status,
                  selected_tier, risk_level, reserved_cost, bundle_json,
                  input_tokens, output_tokens, estimated_cost_usd, executor_type, latency_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    dispatch_id,
                    created_at,
                    raw_request,
                    request_source,
                    final_status,
                    selected_tier,
                    risk_level,
                    reserved_cost,
                    bundle_json,
                    input_tokens,
                    output_tokens,
                    estimated_cost_usd,
                    executor_type,
                    latency_ms,
                ],
            )
            .map_err(|e| e.to_string())?;
            let history_id = conn.last_insert_rowid();
            append_audit_locked(
                conn,
                &self.now(),
                actor,
                "dispatch.record",
                dispatch_id,
                &json!({"history_id": history_id, "request_source": request_source}),
            )?;
            Ok(json!({
                "history_id": history_id,
                "dispatch_id": dispatch_id,
                "created_at": created_at,
                "raw_request": raw_request,
                "request_source": request_source,
                "final_status": final_status,
                "selected_tier": selected_tier,
                "risk_level": risk_level,
                "reserved_cost": reserved_cost,
                "input_tokens": input_tokens,
                "output_tokens": output_tokens,
                "estimated_cost_usd": estimated_cost_usd,
                "executor_type": executor_type,
                "latency_ms": latency_ms,
                "bundle": bundle,
            }))
        })
    }

    pub fn list_dispatches(&self, limit: i64) -> Result<Vec<Value>, String> {
        self.list_dispatches_with_offset(limit, 0)
    }

    pub fn list_dispatches_with_offset(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT history_id, dispatch_id, created_at, raw_request, request_source,
                            final_status, selected_tier, risk_level, reserved_cost, bundle_json,
                            input_tokens, output_tokens, estimated_cost_usd, executor_type, latency_ms
                     FROM dispatch_history
                     ORDER BY history_id DESC
                     LIMIT ?1 OFFSET ?2",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![limit, offset], dispatch_history_row)
                .map_err(|e| e.to_string())?;
            collect_values(rows)
        })
    }

    pub fn search_dispatches(
        &self,
        limit: i64,
        offset: i64,
        search: Option<&str>,
    ) -> Result<Vec<Value>, String> {
        let Some(search) = search.map(str::trim).filter(|value| !value.is_empty()) else {
            return self.list_dispatches_with_offset(limit, offset);
        };
        let pattern = format!("%{}%", escape_like(&search.to_lowercase()));

        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT history_id, dispatch_id, created_at, raw_request, request_source,
                            final_status, selected_tier, risk_level, reserved_cost, bundle_json,
                            input_tokens, output_tokens, estimated_cost_usd, executor_type, latency_ms
                     FROM dispatch_history
                     WHERE lower(dispatch_id) LIKE ?1 ESCAPE '\\'
                        OR lower(raw_request) LIKE ?1 ESCAPE '\\'
                        OR lower(request_source) LIKE ?1 ESCAPE '\\'
                        OR lower(final_status) LIKE ?1 ESCAPE '\\'
                        OR lower(selected_tier) LIKE ?1 ESCAPE '\\'
                        OR lower(risk_level) LIKE ?1 ESCAPE '\\'
                     ORDER BY history_id DESC
                     LIMIT ?2 OFFSET ?3",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![pattern, limit, offset], dispatch_history_row)
                .map_err(|e| e.to_string())?;
            collect_values(rows)
        })
    }

    pub fn get_dispatch(&self, dispatch_id: &str) -> Result<Option<Value>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT history_id, dispatch_id, created_at, raw_request, request_source,
                            final_status, selected_tier, risk_level, reserved_cost, bundle_json,
                            input_tokens, output_tokens, estimated_cost_usd, executor_type, latency_ms
                     FROM dispatch_history
                     WHERE dispatch_id = ?1
                     ORDER BY history_id DESC
                     LIMIT 1",
                )
                .map_err(|e| e.to_string())?;
            let mut rows = stmt
                .query_map(params![dispatch_id], dispatch_history_row)
                .map_err(|e| e.to_string())?;
            match rows.next() {
                Some(Ok(val)) => Ok(Some(val)),
                Some(Err(e)) => Err(e.to_string()),
                None => Ok(None),
            }
        })
    }
}

fn dispatch_history_row(row: &Row<'_>) -> rusqlite::Result<Value> {
    let bundle_text: String = row.get(9)?;
    let bundle: Value = serde_json::from_str(&bundle_text).unwrap_or(Value::Null);
    Ok(json!({
        "history_id": row.get::<_, i64>(0)?,
        "dispatch_id": row.get::<_, String>(1)?,
        "created_at": row.get::<_, String>(2)?,
        "raw_request": row.get::<_, String>(3)?,
        "request_source": row.get::<_, String>(4)?,
        "final_status": row.get::<_, String>(5)?,
        "selected_tier": row.get::<_, String>(6)?,
        "risk_level": row.get::<_, String>(7)?,
        "reserved_cost": row.get::<_, f64>(8)?,
        "bundle": bundle,
        "input_tokens": row.get::<_, Option<i64>>(10)?,
        "output_tokens": row.get::<_, Option<i64>>(11)?,
        "estimated_cost_usd": row.get::<_, Option<f64>>(12)?,
        "executor_type": row.get::<_, String>(13)?,
        "latency_ms": row.get::<_, Option<i64>>(14)?,
    }))
}

fn escape_like(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}
