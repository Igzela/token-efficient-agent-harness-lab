use rusqlite::params;

use super::LocalProductStore;

// ---------------------------------------------------------------------------
// FeedbackRecord
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct FeedbackRecord {
    pub feedback_id: String,
    pub run_id: String,
    pub node_id: Option<String>,
    pub executor_type: String,
    pub task_group: String,
    pub task_domain: String,
    pub task_intent: String,
    pub success: bool,
    pub latency_ms: i64,
    pub retry_count: i64,
    pub quality_score: f64,
    pub cost: f64,
    pub error_domain: Option<String>,
    pub created_at: String,
}

impl FeedbackRecord {
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::json!({
            "feedback_id": self.feedback_id,
            "run_id": self.run_id,
            "node_id": self.node_id,
            "executor_type": self.executor_type,
            "task_group": self.task_group,
            "task_domain": self.task_domain,
            "task_intent": self.task_intent,
            "success": self.success,
            "latency_ms": self.latency_ms,
            "retry_count": self.retry_count,
            "quality_score": self.quality_score,
            "cost": self.cost,
            "error_domain": self.error_domain,
            "created_at": self.created_at,
        })
    }
}

// ---------------------------------------------------------------------------
// FeedbackStoreStats
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct FeedbackStoreStats {
    pub total_records: i64,
    pub success_rate: f64,
    pub avg_latency_ms: f64,
    pub avg_quality: f64,
    pub avg_cost: f64,
    pub by_executor_type: serde_json::Value,
}

// ---------------------------------------------------------------------------
// LocalProductStore methods
// ---------------------------------------------------------------------------

impl LocalProductStore {
    pub fn insert_scheduler_feedback(
        &self,
        run_id: &str,
        node_id: Option<&str>,
        executor_type: &str,
        task_group: &str,
        success: bool,
        latency_ms: i64,
        retry_count: i64,
        quality_score: f64,
        cost: f64,
        error_domain: Option<&str>,
    ) -> Result<FeedbackRecord, String> {
        let (task_domain, task_intent) = crate::routing::schemas::parse_task_group(task_group);
        let created_at = self.now();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let feedback_id = format!("feedback-{}-{}", run_id, nanos);

        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO scheduler_feedback
                 (feedback_id, run_id, node_id, executor_type, task_group, task_domain, task_intent,
                  success, latency_ms, retry_count, quality_score, cost, error_domain, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    feedback_id,
                    run_id,
                    node_id,
                    executor_type,
                    task_group,
                    task_domain,
                    task_intent,
                    if success { 1 } else { 0 },
                    latency_ms,
                    retry_count,
                    quality_score,
                    cost,
                    error_domain,
                    created_at,
                ],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })?;

        Ok(FeedbackRecord {
            feedback_id,
            run_id: run_id.to_string(),
            node_id: node_id.map(str::to_string),
            executor_type: executor_type.to_string(),
            task_group: task_group.to_string(),
            task_domain,
            task_intent,
            success,
            latency_ms,
            retry_count,
            quality_score,
            cost,
            error_domain: error_domain.map(str::to_string),
            created_at,
        })
    }

    pub fn get_feedback_for_run(&self, run_id: &str) -> Result<Vec<FeedbackRecord>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT feedback_id, run_id, node_id, executor_type, task_group,
                            task_domain, task_intent, success, latency_ms, retry_count,
                            quality_score, cost, error_domain, created_at
                     FROM scheduler_feedback
                     WHERE run_id = ?1
                     ORDER BY created_at ASC",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![run_id], feedback_row)
                .map_err(|e| e.to_string())?;
            collect_feedback(rows)
        })
    }

    pub fn get_feedback_for_task_group(
        &self,
        task_group: &str,
        limit: i64,
    ) -> Result<Vec<FeedbackRecord>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT feedback_id, run_id, node_id, executor_type, task_group,
                            task_domain, task_intent, success, latency_ms, retry_count,
                            quality_score, cost, error_domain, created_at
                     FROM scheduler_feedback
                     WHERE task_group = ?1
                     ORDER BY created_at DESC
                     LIMIT ?2",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![task_group, limit], feedback_row)
                .map_err(|e| e.to_string())?;
            collect_feedback(rows)
        })
    }

    pub fn get_feedback_stats(&self, task_group: &str) -> Result<FeedbackStoreStats, String> {
        self.with_conn(|conn| {
            let total_records: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM scheduler_feedback WHERE task_group = ?1",
                    params![task_group],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;

            if total_records == 0 {
                return Ok(FeedbackStoreStats {
                    total_records: 0,
                    success_rate: 0.0,
                    avg_latency_ms: 0.0,
                    avg_quality: 0.0,
                    avg_cost: 0.0,
                    by_executor_type: serde_json::json!({}),
                });
            }

            let success_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM scheduler_feedback WHERE task_group = ?1 AND success = 1",
                    params![task_group],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            let success_rate = success_count as f64 / total_records as f64;

            let avg_latency_ms: f64 = conn
                .query_row(
                    "SELECT COALESCE(AVG(latency_ms), 0) FROM scheduler_feedback WHERE task_group = ?1",
                    params![task_group],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;

            let avg_quality: f64 = conn
                .query_row(
                    "SELECT COALESCE(AVG(quality_score), 0) FROM scheduler_feedback WHERE task_group = ?1",
                    params![task_group],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;

            let avg_cost: f64 = conn
                .query_row(
                    "SELECT COALESCE(AVG(cost), 0) FROM scheduler_feedback WHERE task_group = ?1",
                    params![task_group],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;

            let mut exec_stmt = conn
                .prepare(
                    "SELECT executor_type, COUNT(*), SUM(CASE WHEN success = 1 THEN 1 ELSE 0 END),
                            COALESCE(AVG(latency_ms), 0), COALESCE(AVG(quality_score), 0), COALESCE(AVG(cost), 0)
                     FROM scheduler_feedback
                     WHERE task_group = ?1
                     GROUP BY executor_type",
                )
                .map_err(|e| e.to_string())?;
            let exec_rows = exec_stmt
                .query_map(params![task_group], |row| {
                    let executor_type: String = row.get(0)?;
                    let count: i64 = row.get(1)?;
                    let successes: i64 = row.get(2)?;
                    let avg_lat: f64 = row.get(3)?;
                    let avg_q: f64 = row.get(4)?;
                    let avg_c: f64 = row.get(5)?;
                    Ok(serde_json::json!({
                        "executor_type": executor_type,
                        "count": count,
                        "success_count": successes,
                        "success_rate": if count > 0 { successes as f64 / count as f64 } else { 0.0 },
                        "avg_latency_ms": avg_lat,
                        "avg_quality": avg_q,
                        "avg_cost": avg_c,
                    }))
                })
                .map_err(|e| e.to_string())?;

            let mut by_executor = Vec::new();
            for row in exec_rows {
                by_executor.push(row.map_err(|e| e.to_string())?);
            }

            Ok(FeedbackStoreStats {
                total_records,
                success_rate,
                avg_latency_ms,
                avg_quality,
                avg_cost,
                by_executor_type: serde_json::Value::Array(by_executor),
            })
        })
    }

    pub fn suggest_executor_type(&self, task_group: &str) -> Option<String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT executor_type,
                            COUNT(*) as cnt,
                            SUM(CASE WHEN success = 1 THEN 1 ELSE 0 END) as successes
                     FROM scheduler_feedback
                     WHERE task_group = ?1
                     GROUP BY executor_type
                     ORDER BY (CAST(successes AS REAL) / cnt) DESC, cnt DESC
                     LIMIT 1",
                )
                .map_err(|e| e.to_string())?;
            let result = stmt
                .query_row(params![task_group], |row| {
                    let executor_type: String = row.get(0)?;
                    let cnt: i64 = row.get(1)?;
                    let successes: i64 = row.get(2)?;
                    let rate = if cnt > 0 {
                        successes as f64 / cnt as f64
                    } else {
                        0.0
                    };
                    Ok((executor_type, cnt, rate))
                })
                .ok();

            Ok(result.and_then(|r| if r.2 > 0.0 { Some(r.0) } else { None }))
        })
        .ok()
        .flatten()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn feedback_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FeedbackRecord> {
    Ok(FeedbackRecord {
        feedback_id: row.get(0)?,
        run_id: row.get(1)?,
        node_id: row.get(2)?,
        executor_type: row.get(3)?,
        task_group: row.get(4)?,
        task_domain: row.get(5)?,
        task_intent: row.get(6)?,
        success: row.get::<_, i64>(7)? != 0,
        latency_ms: row.get(8)?,
        retry_count: row.get(9)?,
        quality_score: row.get(10)?,
        cost: row.get(11)?,
        error_domain: row.get(12)?,
        created_at: row.get(13)?,
    })
}

fn collect_feedback(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row) -> rusqlite::Result<FeedbackRecord>>,
) -> Result<Vec<FeedbackRecord>, String> {
    let mut records = Vec::new();
    for row in rows {
        records.push(row.map_err(|e| e.to_string())?);
    }
    Ok(records)
}
