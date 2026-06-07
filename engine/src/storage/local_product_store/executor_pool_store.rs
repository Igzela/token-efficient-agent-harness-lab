use rusqlite::params;
use serde_json::json;

use super::LocalProductStore;
use crate::executor_pool::{
    CostProfile, ExecutorCapabilities, ExecutorMetrics, ExecutorPoolEntry, ExecutorStatus,
};

impl LocalProductStore {
    pub fn save_executor_pool_snapshot(&self, entries: &[ExecutorPoolEntry]) -> Result<(), String> {
        let now = self.now();
        self.with_conn(|conn| {
            for entry in entries {
                let caps_json = serde_json::to_string(&json!({
                    "supported_task_types": entry.capabilities.supported_task_types,
                    "supported_task_domains": entry.capabilities.supported_task_domains,
                    "requires_auth": entry.capabilities.requires_auth,
                    "requires_cli": entry.capabilities.requires_cli,
                    "max_timeout_ms": entry.capabilities.max_timeout_ms,
                }))
                .map_err(|e| e.to_string())?;

                let status_json = serde_json::to_string(&json!({
                    "available": entry.status.available,
                    "active_count": entry.status.active_count,
                    "concurrency_limit": entry.status.concurrency_limit,
                    "cooldown_until": entry.status.cooldown_until,
                    "failure_score": entry.status.failure_score,
                }))
                .map_err(|e| e.to_string())?;

                let cost_json = serde_json::to_string(&json!({
                    "cost_per_execution_usd": entry.cost_profile.cost_per_execution_usd,
                    "daily_cost_usd": entry.cost_profile.daily_cost_usd,
                    "daily_cost_limit_usd": entry.cost_profile.daily_cost_limit_usd,
                }))
                .map_err(|e| e.to_string())?;

                let metrics_json = serde_json::to_string(&json!({
                    "total_executions": entry.metrics.total_executions,
                    "successful_executions": entry.metrics.successful_executions,
                    "failed_executions": entry.metrics.failed_executions,
                    "avg_latency_ms": entry.metrics.avg_latency_ms,
                    "total_latency_ms": entry.metrics.total_latency_ms,
                    "last_executed_at": entry.metrics.last_executed_at,
                }))
                .map_err(|e| e.to_string())?;

                conn.execute(
                    "INSERT OR REPLACE INTO executor_pool
                     (executor_type, capabilities_json, status_json, cost_profile_json, metrics_json, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![entry.executor_type, caps_json, status_json, cost_json, metrics_json, now],
                )
                .map_err(|e| e.to_string())?;
            }
            Ok(())
        })
    }

    pub fn load_executor_pool_snapshot(&self) -> Result<Vec<ExecutorPoolEntry>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT executor_type, capabilities_json, status_json, cost_profile_json, metrics_json
                     FROM executor_pool",
                )
                .map_err(|e| e.to_string())?;

            let rows = stmt
                .query_map([], |row| {
                    let executor_type: String = row.get(0)?;
                    let caps_json: String = row.get(1)?;
                    let status_json: String = row.get(2)?;
                    let cost_json: String = row.get(3)?;
                    let metrics_json: String = row.get(4)?;
                    Ok((
                        executor_type,
                        caps_json,
                        status_json,
                        cost_json,
                        metrics_json,
                    ))
                })
                .map_err(|e| e.to_string())?;

            let mut entries = Vec::new();
            for row in rows {
                let (executor_type, caps_json, status_json, cost_json, metrics_json) =
                    row.map_err(|e| e.to_string())?;

                let caps_val: serde_json::Value =
                    serde_json::from_str(&caps_json).map_err(|e| e.to_string())?;
                let status_val: serde_json::Value =
                    serde_json::from_str(&status_json).map_err(|e| e.to_string())?;
                let cost_val: serde_json::Value =
                    serde_json::from_str(&cost_json).map_err(|e| e.to_string())?;
                let metrics_val: serde_json::Value =
                    serde_json::from_str(&metrics_json).map_err(|e| e.to_string())?;

                let capabilities = ExecutorCapabilities {
                    supported_task_types: caps_val["supported_task_types"]
                        .as_array()
                        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                        .unwrap_or_default(),
                    supported_task_domains: caps_val["supported_task_domains"]
                        .as_array()
                        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                        .unwrap_or_default(),
                    requires_auth: caps_val["requires_auth"].as_bool().unwrap_or(false),
                    requires_cli: caps_val["requires_cli"].as_bool().unwrap_or(false),
                    max_timeout_ms: caps_val["max_timeout_ms"].as_u64().unwrap_or(300_000),
                };

                let status = ExecutorStatus {
                    available: status_val["available"].as_bool().unwrap_or(true),
                    active_count: status_val["active_count"].as_u64().unwrap_or(0),
                    concurrency_limit: status_val["concurrency_limit"].as_u64().unwrap_or(10),
                    cooldown_until: status_val["cooldown_until"]
                        .as_str()
                        .map(String::from),
                    failure_score: status_val["failure_score"].as_f64().unwrap_or(0.0),
                };

                let cost_profile = CostProfile {
                    cost_per_execution_usd: cost_val["cost_per_execution_usd"].as_f64(),
                    daily_cost_usd: cost_val["daily_cost_usd"].as_f64(),
                    daily_cost_limit_usd: cost_val["daily_cost_limit_usd"].as_f64(),
                };

                let metrics = ExecutorMetrics {
                    total_executions: metrics_val["total_executions"].as_u64().unwrap_or(0),
                    successful_executions: metrics_val["successful_executions"].as_u64().unwrap_or(0),
                    failed_executions: metrics_val["failed_executions"].as_u64().unwrap_or(0),
                    avg_latency_ms: metrics_val["avg_latency_ms"].as_f64().unwrap_or(0.0),
                    total_latency_ms: metrics_val["total_latency_ms"].as_u64().unwrap_or(0),
                    last_executed_at: metrics_val["last_executed_at"]
                        .as_str()
                        .map(String::from),
                };

                entries.push(ExecutorPoolEntry {
                    executor_type,
                    capabilities,
                    status,
                    cost_profile,
                    metrics,
                });
            }

            Ok(entries)
        })
    }
}
