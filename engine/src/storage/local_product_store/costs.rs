use rusqlite::params;
use serde_json::{json, Value};

use super::{collect_values, LocalProductStore};

impl LocalProductStore {
    pub fn cost_summary(&self) -> Result<Value, String> {
        self.with_conn(|conn| {
            let dispatch_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM dispatch_history", [], |row| {
                    row.get(0)
                })
                .map_err(|e| e.to_string())?;
            let total_reserved_cost: f64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(reserved_cost), 0.0) FROM dispatch_history",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            let total_estimated_cost_usd: f64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(estimated_cost_usd), 0.0) FROM dispatch_history",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            let total_input_tokens: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(input_tokens), 0) FROM dispatch_history",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            let total_output_tokens: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(output_tokens), 0) FROM dispatch_history",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            let estimated_cost_rows: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM dispatch_history WHERE estimated_cost_usd IS NOT NULL",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            let estimated_cost_available = estimated_cost_rows > 0;
            let cost_utilization = if total_reserved_cost > 0.0 {
                total_estimated_cost_usd / total_reserved_cost
            } else {
                0.0
            };
            let mut tier_stmt = conn
                .prepare(
                    "SELECT selected_tier, COUNT(*),
                            COALESCE(SUM(reserved_cost), 0.0),
                            COALESCE(SUM(estimated_cost_usd), 0.0),
                            COALESCE(SUM(input_tokens), 0),
                            COALESCE(SUM(output_tokens), 0)
                     FROM dispatch_history
                     GROUP BY selected_tier
                     ORDER BY selected_tier",
                )
                .map_err(|e| e.to_string())?;
            let tier_rows = tier_stmt
                .query_map([], |row| {
                    Ok(json!({
                        "selected_tier": row.get::<_, String>(0)?,
                        "dispatch_count": row.get::<_, i64>(1)?,
                        "reserved_cost": row.get::<_, f64>(2)?,
                        "estimated_cost_usd": row.get::<_, f64>(3)?,
                        "input_tokens": row.get::<_, i64>(4)?,
                        "output_tokens": row.get::<_, i64>(5)?,
                    }))
                })
                .map_err(|e| e.to_string())?;
            let mut daily_stmt = conn
                .prepare(
                    "SELECT substr(created_at, 1, 10) as dt, COUNT(*),
                            COALESCE(SUM(reserved_cost), 0.0),
                            COALESCE(SUM(estimated_cost_usd), 0.0)
                     FROM dispatch_history
                     GROUP BY dt
                     ORDER BY dt DESC
                     LIMIT 30",
                )
                .map_err(|e| e.to_string())?;
            let daily_rows = daily_stmt
                .query_map([], |row| {
                    Ok(json!({
                        "date": row.get::<_, String>(0)?,
                        "dispatch_count": row.get::<_, i64>(1)?,
                        "reserved_cost": row.get::<_, f64>(2)?,
                        "estimated_cost_usd": row.get::<_, f64>(3)?,
                    }))
                })
                .map_err(|e| e.to_string())?;
            Ok(json!({
                "schema_version": "local_cost_summary.v2",
                "currency": "USD",
                "dispatch_count": dispatch_count,
                "total_reserved_cost": total_reserved_cost,
                "total_estimated_cost_usd": total_estimated_cost_usd,
                "total_input_tokens": total_input_tokens,
                "total_output_tokens": total_output_tokens,
                "estimated_cost_available": estimated_cost_available,
                "cost_utilization": cost_utilization,
                "by_tier": collect_values(tier_rows)?,
                "daily": collect_values(daily_rows)?,
            }))
        })
    }

    pub fn dispatch_cost_details(&self, limit: i64) -> Result<Value, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT history_id, dispatch_id, created_at, selected_tier,
                            reserved_cost,
                            COALESCE(input_tokens, 0),
                            COALESCE(output_tokens, 0),
                            COALESCE(estimated_cost_usd, 0.0),
                            executor_type,
                            latency_ms
                     FROM dispatch_history
                     ORDER BY history_id DESC
                     LIMIT ?1",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![limit], |row| {
                    Ok(json!({
                        "history_id": row.get::<_, i64>(0)?,
                        "dispatch_id": row.get::<_, String>(1)?,
                        "created_at": row.get::<_, String>(2)?,
                        "selected_tier": row.get::<_, String>(3)?,
                        "reserved_cost": row.get::<_, f64>(4)?,
                        "input_tokens": row.get::<_, i64>(5)?,
                        "output_tokens": row.get::<_, i64>(6)?,
                        "estimated_cost_usd": row.get::<_, f64>(7)?,
                        "executor_type": row.get::<_, String>(8)?,
                        "latency_ms": row.get::<_, Option<i64>>(9)?,
                    }))
                })
                .map_err(|e| e.to_string())?;
            Ok(json!({
                "schema_version": "local_dispatch_cost_detail.v1",
                "dispatches": collect_values(rows)?,
            }))
        })
    }

    pub fn daily_estimated_cost_usd(&self, date_prefix: &str) -> Result<f64, String> {
        self.with_conn(|conn| {
            let like_pattern = format!("{}%", date_prefix);
            conn.query_row(
                "SELECT COALESCE(SUM(estimated_cost_usd), 0.0)
                 FROM dispatch_history
                 WHERE created_at LIKE ?1",
                params![like_pattern],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())
        })
    }
}
