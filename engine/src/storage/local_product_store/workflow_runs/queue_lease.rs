use rusqlite::Row;
use serde_json::{json, Value};

pub(super) const ACTIVE_RUN_IDS_SQL: &str = "SELECT run_id FROM workflow_runs
     WHERE status IN ('running', 'created') AND pause_reason IS NULL
     ORDER BY run_sequence";

pub(super) const ACTIVE_RUNS_PRIORITIZED_SQL: &str =
    "SELECT run_id, run_sequence, workflow_id, status, CAST(priority AS BIGINT), deadline_at,
                                CAST(sla_ms AS BIGINT), tenant_id, CAST(queue_position AS BIGINT), pause_reason, degrade_mode,
                                created_at, started_at
                         FROM workflow_runs
                         WHERE status IN ('running', 'created')
                           AND COALESCE(pause_reason, '') <> 'api_owned_supervised_patch'
                         ORDER BY CASE WHEN pause_reason IS NOT NULL THEN 1 ELSE 0 END,
                                  priority ASC, created_at ASC";
pub(super) const ACTIVE_RUN_COUNT_SQL: &str =
    "SELECT COUNT(*) FROM workflow_runs WHERE status IN ('running', 'created')
     AND COALESCE(pause_reason, '') <> 'api_owned_supervised_patch'";

pub(super) const QUEUED_COUNT_SQL: &str =
    "SELECT COUNT(*) FROM workflow_runs WHERE status IN ('created') AND pause_reason IS NULL";
pub(super) const RUNNING_COUNT_SQL: &str =
    "SELECT COUNT(*) FROM workflow_runs WHERE status = 'running'";
pub(super) const PAUSED_COUNT_SQL: &str =
    "SELECT COUNT(*) FROM workflow_runs WHERE status IN ('running', 'created')
     AND pause_reason IS NOT NULL AND pause_reason <> 'api_owned_supervised_patch'";
pub(super) const COMPLETED_COUNT_SQL: &str =
    "SELECT COUNT(*) FROM workflow_runs WHERE status = 'completed'";
pub(super) const FAILED_COUNT_SQL: &str =
    "SELECT COUNT(*) FROM workflow_runs WHERE status = 'failed'";
pub(super) const AVG_PRIORITY_SQL: &str =
    "SELECT CAST(COALESCE(AVG(priority), 5.0) AS DOUBLE PRECISION) FROM workflow_runs WHERE status IN ('created', 'running')";
pub(super) const SQLITE_OVERDUE_COUNT_SQL: &str = "SELECT COUNT(*) FROM workflow_runs
                         WHERE deadline_at IS NOT NULL AND deadline_at < datetime('now')
                           AND status IN ('created', 'running')";
#[allow(dead_code)] // pg-parity SQL; consumed under --features pg-tests
pub(super) const PG_OVERDUE_COUNT_SQL: &str = "SELECT COUNT(*) FROM workflow_runs
                         WHERE deadline_at IS NOT NULL
                           AND CAST(deadline_at AS TIMESTAMPTZ) < now()
                           AND status IN ('created', 'running')";

pub(super) const TENANTS_WITH_QUOTA_SQL: &str = "SELECT tenant_id, COUNT(*) as run_count,
                                CAST(AVG(priority) AS DOUBLE PRECISION) as avg_priority
                         FROM workflow_runs
                         WHERE status IN ('created', 'running') AND tenant_id IS NOT NULL
                         GROUP BY tenant_id";

pub(super) const PENDING_NODE_FOR_TEST_SQL: &str =
    "SELECT node_id FROM workflow_run_nodes WHERE status = 'pending' LIMIT 1";
pub(super) const SQLITE_SET_PENDING_NODE_RUNNING_SQL: &str =
    "UPDATE workflow_run_nodes SET status = 'running', leased_at = ?1 WHERE node_id = ?2";
#[allow(dead_code)] // pg-parity SQL; consumed under --features pg-tests
pub(super) const PG_SET_PENDING_NODE_RUNNING_SQL: &str =
    "UPDATE workflow_run_nodes SET status = 'running', leased_at = $1 WHERE node_id = $2";

pub(super) const STALE_LEASE_SELECT_SQL: &str = "SELECT run_id, node_id, leased_at FROM workflow_run_nodes WHERE status = 'running' AND leased_at IS NOT NULL";

pub(super) fn prioritized_sqlite_row(row: &Row<'_>) -> rusqlite::Result<Value> {
    Ok(json!({
        "run_id": row.get::<_, String>(0)?,
        "run_sequence": row.get::<_, i64>(1)?,
        "workflow_id": row.get::<_, String>(2)?,
        "status": row.get::<_, String>(3)?,
        "priority": row.get::<_, i64>(4)?,
        "deadline_at": row.get::<_, Option<String>>(5)?,
        "sla_ms": row.get::<_, Option<i64>>(6)?,
        "tenant_id": row.get::<_, Option<String>>(7)?,
        "queue_position": row.get::<_, Option<i64>>(8)?,
        "pause_reason": row.get::<_, Option<String>>(9)?,
        "degrade_mode": row.get::<_, Option<String>>(10)?,
        "created_at": row.get::<_, String>(11)?,
        "started_at": row.get::<_, Option<String>>(12)?,
    }))
}

pub(super) fn tenant_quota_sqlite_row(row: &Row<'_>) -> rusqlite::Result<Value> {
    Ok(json!({
        "tenant_id": row.get::<_, String>(0)?,
        "run_count": row.get::<_, i64>(1)?,
        "avg_priority": row.get::<_, f64>(2)?,
    }))
}

#[cfg(feature = "pg")]
pub(super) fn prioritized_pg_row(row: &postgres::Row) -> Value {
    json!({
        "run_id": row.get::<_, String>(0),
        "run_sequence": row.get::<_, i64>(1),
        "workflow_id": row.get::<_, String>(2),
        "status": row.get::<_, String>(3),
        "priority": row.get::<_, i64>(4),
        "deadline_at": row.get::<_, Option<String>>(5),
        "sla_ms": row.get::<_, Option<i64>>(6),
        "tenant_id": row.get::<_, Option<String>>(7),
        "queue_position": row.get::<_, Option<i64>>(8),
        "pause_reason": row.get::<_, Option<String>>(9),
        "degrade_mode": row.get::<_, Option<String>>(10),
        "created_at": row.get::<_, String>(11),
        "started_at": row.get::<_, Option<String>>(12),
    })
}

#[cfg(feature = "pg")]
pub(super) fn tenant_quota_pg_row(row: &postgres::Row) -> Value {
    json!({
        "tenant_id": row.get::<_, String>(0),
        "run_count": row.get::<_, i64>(1),
        "avg_priority": row.get::<_, f64>(2),
    })
}

pub(super) fn queue_status_value(
    total_queued: i64,
    total_running: i64,
    total_paused: i64,
    total_completed: i64,
    total_failed: i64,
    avg_priority: Value,
    overdue_count: i64,
) -> Value {
    json!({
        "total_queued": total_queued,
        "total_running": total_running,
        "total_paused": total_paused,
        "total_completed": total_completed,
        "total_failed": total_failed,
        "avg_priority": avg_priority,
        "overdue_count": overdue_count,
    })
}

pub(super) fn stale_lease_is_expired(leased_at: &str, now: &str, lease_timeout_ms: u64) -> bool {
    if let (Ok(lease_time), Ok(now_time)) = (
        chrono::NaiveDateTime::parse_from_str(leased_at, "%Y-%m-%dT%H:%M:%SZ"),
        chrono::NaiveDateTime::parse_from_str(now, "%Y-%m-%dT%H:%M:%SZ"),
    ) {
        (now_time - lease_time).num_milliseconds() as u64 > lease_timeout_ms
    } else {
        false
    }
}

pub(super) fn stale_lease_audit_payload(
    run_id: &str,
    leased_at: &str,
    lease_timeout_ms: u64,
) -> Value {
    json!({
        "run_id": run_id,
        "leased_at": leased_at,
        "lease_timeout_ms": lease_timeout_ms,
    })
}
