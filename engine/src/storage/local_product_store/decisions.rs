use rusqlite::params;
use serde_json::{json, Value};

use super::{DatabaseConnection, LocalProductStore};

pub const ORCHESTRATION_DECISION_LOG_SCHEMA_VERSION: &str = "orchestration_decision_log.v1";

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionRecord {
    pub decision_id: String,
    pub run_id: String,
    pub node_id: Option<String>,
    pub action: String,
    pub action_reason: String,
    pub selected_executor: String,
    pub blocked_reason: Option<String>,
    pub confidence: String,
    pub confidence_score: f64,
    pub input_signals: Value,
    pub created_at: String,
    pub quality_signal: Option<Value>,
    pub routing_signal: Option<Value>,
    pub cost_signal: Option<Value>,
    pub approval_signal: Option<Value>,
    pub queue_signal: Option<Value>,
    pub executor_pool_signal: Option<Value>,
    pub candidate_executors: Option<String>,
    pub degraded_reason: Option<String>,
}

impl DecisionRecord {
    pub fn to_value(&self) -> Value {
        let confidence_label = self.confidence.clone();
        let confidence_numeric = self.confidence_score;
        let candidates_value: Value = match &self.candidate_executors {
            Some(json_str) => serde_json::from_str(json_str).unwrap_or(Value::Null),
            None => Value::Null,
        };
        json!({
            "schema_version": ORCHESTRATION_DECISION_LOG_SCHEMA_VERSION,
            "decision_id": self.decision_id,
            "run_id": self.run_id,
            "node_id": self.node_id,
            "action": self.action,
            "reason": self.action_reason,
            "action_reason": self.action_reason,
            "executor": self.selected_executor,
            "selected_executor": self.selected_executor,
            "blocked_reason": self.blocked_reason,
            "confidence": confidence_numeric,
            "confidence_score": self.confidence_score,
            "confidence_label": confidence_label,
            "input_signals": self.input_signals,
            "created_at": self.created_at,
            "quality_signal": self.quality_signal,
            "routing_signal": self.routing_signal,
            "cost_signal": self.cost_signal,
            "approval_signal": self.approval_signal,
            "queue_signal": self.queue_signal,
            "executor_pool_signal": self.executor_pool_signal,
            "candidate_executors": candidates_value,
            "degraded_reason": self.degraded_reason,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionLogStats {
    pub total_decisions: i64,
    pub by_action: Value,
    pub avg_confidence: f64,
}

impl LocalProductStore {
    pub fn record_orchestration_decision(
        &self,
        run_id: &str,
        node_id: Option<&str>,
        action: &str,
        action_reason: &str,
        selected_executor: &str,
        blocked_reason: Option<&str>,
        confidence: &str,
        confidence_score: f64,
        input_signals: &Value,
    ) -> Result<DecisionRecord, String> {
        self.record_orchestration_decision_enriched(
            run_id,
            node_id,
            action,
            action_reason,
            selected_executor,
            blocked_reason,
            confidence,
            confidence_score,
            input_signals,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
    }

    pub fn record_orchestration_decision_enriched(
        &self,
        run_id: &str,
        node_id: Option<&str>,
        action: &str,
        action_reason: &str,
        selected_executor: &str,
        blocked_reason: Option<&str>,
        confidence: &str,
        confidence_score: f64,
        input_signals: &Value,
        quality_signal: Option<&Value>,
        routing_signal: Option<&Value>,
        cost_signal: Option<&Value>,
        approval_signal: Option<&Value>,
        queue_signal: Option<&Value>,
        executor_pool_signal: Option<&Value>,
        candidate_executors: Option<&[String]>,
        degraded_reason: Option<&str>,
    ) -> Result<DecisionRecord, String> {
        let created_at = self.now();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let decision_id = format!("decision-{}-{}", run_id, nanos);

        let quality_json = quality_signal.map(|v| v.to_string());
        let routing_json = routing_signal.map(|v| v.to_string());
        let cost_json = cost_signal.map(|v| v.to_string());
        let approval_json = approval_signal.map(|v| v.to_string());
        let queue_json = queue_signal.map(|v| v.to_string());
        let pool_json = executor_pool_signal.map(|v| v.to_string());
        let candidates_json = candidate_executors.map(|c| json!(c).to_string());

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute(
                    "INSERT INTO orchestration_decisions
                     (decision_id, run_id, node_id, action, action_reason, selected_executor,
                      blocked_reason, confidence, confidence_score, input_signals_json, created_at,
                      quality_signal_json, routing_signal_json, cost_signal_json, approval_signal_json,
                      queue_signal_json, executor_pool_signal_json, candidate_executors_json, degraded_reason)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
                    params![
                        decision_id,
                        run_id,
                        node_id,
                        action,
                        action_reason,
                        selected_executor,
                        blocked_reason,
                        confidence,
                        confidence_score,
                        input_signals.to_string(),
                        created_at,
                        quality_json,
                        routing_json,
                        cost_json,
                        approval_json,
                        queue_json,
                        pool_json,
                        candidates_json,
                        degraded_reason,
                    ],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .execute(
                        "INSERT INTO orchestration_decisions
                         (decision_id, run_id, node_id, action, action_reason, selected_executor,
                          blocked_reason, confidence, confidence_score, input_signals_json, created_at,
                          quality_signal_json, routing_signal_json, cost_signal_json, approval_signal_json,
                          queue_signal_json, executor_pool_signal_json, candidate_executors_json, degraded_reason)
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)",
                        &[
                            &decision_id,
                            &run_id,
                            &node_id,
                            &action,
                            &action_reason,
                            &selected_executor,
                            &blocked_reason,
                            &confidence,
                            &confidence_score,
                            &input_signals.to_string(),
                            &created_at,
                            &quality_json,
                            &routing_json,
                            &cost_json,
                            &approval_json,
                            &queue_json,
                            &pool_json,
                            &candidates_json,
                            &degraded_reason,
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(())
            })?,
        }

        Ok(DecisionRecord {
            decision_id,
            run_id: run_id.to_string(),
            node_id: node_id.map(str::to_string),
            action: action.to_string(),
            action_reason: action_reason.to_string(),
            selected_executor: selected_executor.to_string(),
            blocked_reason: blocked_reason.map(str::to_string),
            confidence: confidence.to_string(),
            confidence_score,
            input_signals: input_signals.clone(),
            created_at,
            quality_signal: quality_signal.cloned(),
            routing_signal: routing_signal.cloned(),
            cost_signal: cost_signal.cloned(),
            approval_signal: approval_signal.cloned(),
            queue_signal: queue_signal.cloned(),
            executor_pool_signal: executor_pool_signal.cloned(),
            candidate_executors: candidates_json,
            degraded_reason: degraded_reason.map(str::to_string),
        })
    }

    pub fn get_decisions_for_run(
        &self,
        run_id: &str,
        limit: i64,
    ) -> Result<Vec<DecisionRecord>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT decision_id, run_id, node_id, action, action_reason,
                                selected_executor, blocked_reason, confidence, confidence_score,
                                input_signals_json, created_at,
                                quality_signal_json, routing_signal_json, cost_signal_json,
                                approval_signal_json, queue_signal_json, executor_pool_signal_json,
                                candidate_executors_json, degraded_reason
                         FROM orchestration_decisions
                         WHERE run_id = ?1
                         ORDER BY decision_id ASC
                         LIMIT ?2",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![run_id, limit], decision_row)
                    .map_err(|e| e.to_string())?;
                collect_decisions(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT decision_id, run_id, node_id, action, action_reason,
                                selected_executor, blocked_reason, confidence, confidence_score,
                                input_signals_json, created_at,
                                quality_signal_json, routing_signal_json, cost_signal_json,
                                approval_signal_json, queue_signal_json, executor_pool_signal_json,
                                candidate_executors_json, degraded_reason
                         FROM orchestration_decisions
                         WHERE run_id = $1
                         ORDER BY decision_id ASC
                         LIMIT $2",
                        &[&run_id, &limit],
                    )
                    .map_err(|e| e.to_string())?;
                pg_collect_decisions(rows)
            }),
        }
    }

    pub fn search_decisions(
        &self,
        limit: i64,
        offset: i64,
        search: Option<&str>,
    ) -> Result<Vec<DecisionRecord>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                if let Some(raw_search) = search {
                    let trimmed = raw_search.trim().to_lowercase();
                    if !trimmed.is_empty() {
                        let needle = format!("%{trimmed}%");
                        let mut stmt = conn
                            .prepare(
                                "SELECT decision_id, run_id, node_id, action, action_reason,
                                        selected_executor, blocked_reason, confidence, confidence_score,
                                        input_signals_json, created_at,
                                        quality_signal_json, routing_signal_json, cost_signal_json,
                                        approval_signal_json, queue_signal_json, executor_pool_signal_json,
                                        candidate_executors_json, degraded_reason
                                 FROM orchestration_decisions
                                 WHERE lower(run_id) LIKE ?1
                                    OR lower(action) LIKE ?1
                                    OR lower(selected_executor) LIKE ?1
                                    OR lower(confidence) LIKE ?1
                                 ORDER BY decision_id DESC
                                 LIMIT ?2 OFFSET ?3",
                            )
                            .map_err(|e| e.to_string())?;
                        let rows = stmt
                            .query_map(params![needle, limit, offset], decision_row)
                            .map_err(|e| e.to_string())?;
                        return collect_decisions(rows);
                    }
                }

                let mut stmt = conn
                    .prepare(
                        "SELECT decision_id, run_id, node_id, action, action_reason,
                                selected_executor, blocked_reason, confidence, confidence_score,
                                input_signals_json, created_at,
                                quality_signal_json, routing_signal_json, cost_signal_json,
                                approval_signal_json, queue_signal_json, executor_pool_signal_json,
                                candidate_executors_json, degraded_reason
                         FROM orchestration_decisions
                         ORDER BY decision_id DESC
                         LIMIT ?1 OFFSET ?2",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![limit, offset], decision_row)
                    .map_err(|e| e.to_string())?;
                collect_decisions(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                if let Some(raw_search) = search {
                    let trimmed = raw_search.trim().to_lowercase();
                    if !trimmed.is_empty() {
                        let needle = format!("%{trimmed}%");
                        let rows = client
                            .query(
                                "SELECT decision_id, run_id, node_id, action, action_reason,
                                        selected_executor, blocked_reason, confidence, confidence_score,
                                        input_signals_json, created_at,
                                        quality_signal_json, routing_signal_json, cost_signal_json,
                                        approval_signal_json, queue_signal_json, executor_pool_signal_json,
                                        candidate_executors_json, degraded_reason
                                 FROM orchestration_decisions
                                 WHERE lower(run_id) LIKE $1
                                    OR lower(action) LIKE $1
                                    OR lower(selected_executor) LIKE $1
                                    OR lower(confidence) LIKE $1
                                 ORDER BY decision_id DESC
                                 LIMIT $2 OFFSET $3",
                                &[&needle, &limit, &offset],
                            )
                            .map_err(|e| e.to_string())?;
                        return pg_collect_decisions(rows);
                    }
                }

                let rows = client
                    .query(
                        "SELECT decision_id, run_id, node_id, action, action_reason,
                                selected_executor, blocked_reason, confidence, confidence_score,
                                input_signals_json, created_at,
                                quality_signal_json, routing_signal_json, cost_signal_json,
                                approval_signal_json, queue_signal_json, executor_pool_signal_json,
                                candidate_executors_json, degraded_reason
                         FROM orchestration_decisions
                         ORDER BY decision_id DESC
                         LIMIT $1 OFFSET $2",
                        &[&limit, &offset],
                    )
                    .map_err(|e| e.to_string())?;
                pg_collect_decisions(rows)
            }),
        }
    }

    pub fn get_decision_by_id(&self, decision_id: &str) -> Result<Option<DecisionRecord>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT decision_id, run_id, node_id, action, action_reason,
                                selected_executor, blocked_reason, confidence, confidence_score,
                                input_signals_json, created_at,
                                quality_signal_json, routing_signal_json, cost_signal_json,
                                approval_signal_json, queue_signal_json, executor_pool_signal_json,
                                candidate_executors_json, degraded_reason
                         FROM orchestration_decisions
                         WHERE decision_id = ?1",
                    )
                    .map_err(|e| e.to_string())?;
                let mut rows = stmt
                    .query_map(params![decision_id], decision_row)
                    .map_err(|e| e.to_string())?;
                match rows.next() {
                    Some(row) => Ok(Some(row.map_err(|e| e.to_string())?)),
                    None => Ok(None),
                }
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT decision_id, run_id, node_id, action, action_reason,
                                selected_executor, blocked_reason, confidence, confidence_score,
                                input_signals_json, created_at,
                                quality_signal_json, routing_signal_json, cost_signal_json,
                                approval_signal_json, queue_signal_json, executor_pool_signal_json,
                                candidate_executors_json, degraded_reason
                         FROM orchestration_decisions
                         WHERE decision_id = $1",
                        &[&decision_id],
                    )
                    .map_err(|e| e.to_string())?;
                match rows.into_iter().next() {
                    Some(row) => Ok(Some(pg_decision_row(&row))),
                    None => Ok(None),
                }
            }),
        }
    }

    pub fn decision_log_stats(&self) -> Result<DecisionLogStats, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let total_decisions: i64 = conn
                    .query_row("SELECT COUNT(*) FROM orchestration_decisions", [], |row| {
                        row.get(0)
                    })
                    .map_err(|e| e.to_string())?;

                if total_decisions == 0 {
                    return Ok(DecisionLogStats {
                        total_decisions: 0,
                        by_action: json!({}),
                        avg_confidence: 0.0,
                    });
                }

                let avg_confidence: f64 = conn
                    .query_row(
                        "SELECT COALESCE(AVG(confidence_score), 0) FROM orchestration_decisions",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;

                let mut stmt = conn
                    .prepare(
                        "SELECT action, COUNT(*), COALESCE(AVG(confidence_score), 0)
                         FROM orchestration_decisions
                         GROUP BY action",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map([], |row| {
                        let action: String = row.get(0)?;
                        let count: i64 = row.get(1)?;
                        let avg_conf: f64 = row.get(2)?;
                        Ok(json!({
                            "action": action,
                            "count": count,
                            "avg_confidence": avg_conf,
                        }))
                    })
                    .map_err(|e| e.to_string())?;
                let by_action: Vec<Value> = rows.filter_map(|r| r.ok()).collect();

                Ok(DecisionLogStats {
                    total_decisions,
                    by_action: json!(by_action),
                    avg_confidence,
                })
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let total_decisions: i64 = client
                    .query_one("SELECT COUNT(*) FROM orchestration_decisions", &[])
                    .map_err(|e| e.to_string())?
                    .get(0);

                if total_decisions == 0 {
                    return Ok(DecisionLogStats {
                        total_decisions: 0,
                        by_action: json!({}),
                        avg_confidence: 0.0,
                    });
                }

                let avg_confidence: f64 = client
                    .query_one(
                        "SELECT COALESCE(AVG(confidence_score), 0) FROM orchestration_decisions",
                        &[],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);

                let rows = client
                    .query(
                        "SELECT action, COUNT(*), COALESCE(AVG(confidence_score), 0)
                         FROM orchestration_decisions
                         GROUP BY action",
                        &[],
                    )
                    .map_err(|e| e.to_string())?;
                let by_action: Vec<Value> = rows
                    .iter()
                    .map(|row| {
                        json!({
                            "action": row.get::<_, String>(0),
                            "count": row.get::<_, i64>(1),
                            "avg_confidence": row.get::<_, f64>(2),
                        })
                    })
                    .collect();

                Ok(DecisionLogStats {
                    total_decisions,
                    by_action: json!(by_action),
                    avg_confidence,
                })
            }),
        }
    }
}

fn decision_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DecisionRecord> {
    let input_text: String = row.get(9)?;
    let input_signals: Value = serde_json::from_str(&input_text).unwrap_or(Value::Null);

    let quality_json: Option<String> = row.get(11)?;
    let routing_json: Option<String> = row.get(12)?;
    let cost_json: Option<String> = row.get(13)?;
    let approval_json: Option<String> = row.get(14)?;
    let queue_json: Option<String> = row.get(15)?;
    let pool_json: Option<String> = row.get(16)?;

    Ok(DecisionRecord {
        decision_id: row.get(0)?,
        run_id: row.get(1)?,
        node_id: row.get(2)?,
        action: row.get(3)?,
        action_reason: row.get(4)?,
        selected_executor: row.get(5)?,
        blocked_reason: row.get(6)?,
        confidence: row.get(7)?,
        confidence_score: row.get(8)?,
        input_signals,
        created_at: row.get(10)?,
        quality_signal: quality_json.and_then(|s| serde_json::from_str(&s).ok()),
        routing_signal: routing_json.and_then(|s| serde_json::from_str(&s).ok()),
        cost_signal: cost_json.and_then(|s| serde_json::from_str(&s).ok()),
        approval_signal: approval_json.and_then(|s| serde_json::from_str(&s).ok()),
        queue_signal: queue_json.and_then(|s| serde_json::from_str(&s).ok()),
        executor_pool_signal: pool_json.and_then(|s| serde_json::from_str(&s).ok()),
        candidate_executors: row.get(17)?,
        degraded_reason: row.get(18)?,
    })
}

#[cfg(feature = "pg")]
fn pg_decision_row(row: &postgres::Row) -> DecisionRecord {
    let input_text: String = row.get(9);
    let input_signals: Value = serde_json::from_str(&input_text).unwrap_or(Value::Null);

    let quality_json: Option<String> = row.get(11);
    let routing_json: Option<String> = row.get(12);
    let cost_json: Option<String> = row.get(13);
    let approval_json: Option<String> = row.get(14);
    let queue_json: Option<String> = row.get(15);
    let pool_json: Option<String> = row.get(16);

    DecisionRecord {
        decision_id: row.get(0),
        run_id: row.get(1),
        node_id: row.get(2),
        action: row.get(3),
        action_reason: row.get(4),
        selected_executor: row.get(5),
        blocked_reason: row.get(6),
        confidence: row.get(7),
        confidence_score: row.get(8),
        input_signals,
        created_at: row.get(10),
        quality_signal: quality_json.and_then(|s| serde_json::from_str(&s).ok()),
        routing_signal: routing_json.and_then(|s| serde_json::from_str(&s).ok()),
        cost_signal: cost_json.and_then(|s| serde_json::from_str(&s).ok()),
        approval_signal: approval_json.and_then(|s| serde_json::from_str(&s).ok()),
        queue_signal: queue_json.and_then(|s| serde_json::from_str(&s).ok()),
        executor_pool_signal: pool_json.and_then(|s| serde_json::from_str(&s).ok()),
        candidate_executors: row.get(17),
        degraded_reason: row.get(18),
    }
}

fn collect_decisions(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row) -> rusqlite::Result<DecisionRecord>>,
) -> Result<Vec<DecisionRecord>, String> {
    let mut records = Vec::new();
    for row in rows {
        records.push(row.map_err(|e| e.to_string())?);
    }
    Ok(records)
}

#[cfg(feature = "pg")]
fn pg_collect_decisions(rows: Vec<postgres::Row>) -> Result<Vec<DecisionRecord>, String> {
    Ok(rows.iter().map(pg_decision_row).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> LocalProductStore {
        LocalProductStore::new(":memory:").expect("failed to create in-memory store")
    }

    fn record_test_decision(
        store: &LocalProductStore,
        run_id: &str,
        action: &str,
        executor: &str,
    ) -> DecisionRecord {
        store
            .record_orchestration_decision(
                run_id,
                Some("node-1"),
                action,
                "test reason",
                executor,
                None,
                "high",
                0.95,
                &json!({"source": "test"}),
            )
            .expect("failed to record decision")
    }

    #[test]
    fn test_record_decision_returns_valid_record() {
        let store = test_store();
        let rec = record_test_decision(&store, "run-1", "dispatch", "executor-a");
        assert!(rec.decision_id.starts_with("decision-run-1-"));
        assert_eq!(rec.run_id, "run-1");
        assert_eq!(rec.action, "dispatch");
        assert_eq!(rec.selected_executor, "executor-a");
        assert_eq!(rec.confidence, "high");
        assert_eq!(rec.confidence_score, 0.95);
        assert_eq!(rec.input_signals, json!({"source": "test"}));
    }

    #[test]
    fn test_record_decision_persists_to_store() {
        let store = test_store();
        let rec = record_test_decision(&store, "run-1", "dispatch", "executor-a");
        let found = store
            .get_decision_by_id(&rec.decision_id)
            .expect("lookup failed");
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.decision_id, rec.decision_id);
        assert_eq!(found.action, "dispatch");
    }

    #[test]
    fn test_get_decisions_for_run_empty() {
        let store = test_store();
        let results = store
            .get_decisions_for_run("nonexistent-run", 100)
            .expect("query failed");
        assert!(results.is_empty());
    }

    #[test]
    fn test_get_decisions_for_run_ordering() {
        let store = test_store();
        let rec1 = record_test_decision(&store, "run-2", "dispatch", "executor-a");
        let rec2 = record_test_decision(&store, "run-2", "block", "executor-b");
        let rec3 = record_test_decision(&store, "run-2", "retry", "executor-a");
        let results = store
            .get_decisions_for_run("run-2", 100)
            .expect("query failed");
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].decision_id, rec1.decision_id);
        assert_eq!(results[1].decision_id, rec2.decision_id);
        assert_eq!(results[2].decision_id, rec3.decision_id);
    }

    #[test]
    fn test_get_decisions_for_run_respects_limit() {
        let store = test_store();
        for i in 0..5 {
            record_test_decision(&store, "run-3", &format!("action-{i}"), "executor-a");
        }
        let results = store
            .get_decisions_for_run("run-3", 2)
            .expect("query failed");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_get_decision_by_id_found() {
        let store = test_store();
        let rec = record_test_decision(&store, "run-4", "dispatch", "executor-x");
        let found = store
            .get_decision_by_id(&rec.decision_id)
            .expect("lookup failed");
        assert!(found.is_some());
        assert_eq!(found.unwrap(), rec);
    }

    #[test]
    fn test_get_decision_by_id_not_found() {
        let store = test_store();
        let found = store
            .get_decision_by_id("nonexistent-id")
            .expect("lookup failed");
        assert!(found.is_none());
    }

    #[test]
    fn test_search_decisions_no_filter() {
        let store = test_store();
        record_test_decision(&store, "run-5", "dispatch", "executor-a");
        record_test_decision(&store, "run-5", "block", "executor-b");
        let results = store.search_decisions(100, 0, None).expect("search failed");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_decisions_by_action() {
        let store = test_store();
        record_test_decision(&store, "run-6", "dispatch", "executor-a");
        record_test_decision(&store, "run-6", "block", "executor-b");
        record_test_decision(&store, "run-6", "retry", "executor-a");
        let results = store
            .search_decisions(100, 0, Some("dispatch"))
            .expect("search failed");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].action, "dispatch");
    }

    #[test]
    fn test_search_decisions_by_executor() {
        let store = test_store();
        record_test_decision(&store, "run-7", "dispatch", "executor-alpha");
        record_test_decision(&store, "run-7", "block", "executor-beta");
        record_test_decision(&store, "run-7", "retry", "executor-alpha");
        let results = store
            .search_decisions(100, 0, Some("alpha"))
            .expect("search failed");
        assert_eq!(results.len(), 2);
        for r in &results {
            assert_eq!(r.selected_executor, "executor-alpha");
        }
    }

    #[test]
    fn test_decision_log_stats_empty() {
        let store = test_store();
        let stats = store.decision_log_stats().expect("stats failed");
        assert_eq!(stats.total_decisions, 0);
        assert_eq!(stats.by_action, json!({}));
        assert_eq!(stats.avg_confidence, 0.0);
    }

    #[test]
    fn test_decision_log_stats_with_data() {
        let store = test_store();
        record_test_decision(&store, "run-8", "dispatch", "executor-a");
        record_test_decision(&store, "run-8", "dispatch", "executor-b");
        record_test_decision(&store, "run-8", "block", "executor-a");
        let stats = store.decision_log_stats().expect("stats failed");
        assert_eq!(stats.total_decisions, 3);
        assert!(stats.avg_confidence > 0.0);
        let by_action = stats.by_action.as_array().expect("expected array");
        assert_eq!(by_action.len(), 2);
    }
}
