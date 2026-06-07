mod agent_profiles;
mod audit;
mod boundaries;
mod config;
mod costs;
mod decisions;
mod dispatch;
mod executor_pool_store;
mod export_import;
pub mod feedback;
mod integrity;
mod keys;
mod migrations;
mod plans;
mod provider_audit;
mod supervised_patch;
mod team;
mod tool_registry;
mod workflow_runs;

#[cfg(test)]
mod workflow_runs_mutation_tests;

use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub use crate::read_only_planner::WorkflowPlanIds;
pub use boundaries::local_boundaries;
pub use export_import::{ImportCounts, ImportResult, LOCAL_IMPORT_SCHEMA_VERSION};
pub use integrity::{IntegrityReport, TableIntegrity};

pub const LOCAL_PRODUCT_STORE_SCHEMA_VERSION: &str = "local_product_store.v1";
pub const LOCAL_TEAM_EXPORT_SCHEMA_VERSION: &str = "local_team_export.v1";
pub const LOCAL_DASHBOARD_SCHEMA_VERSION: &str = "local_dashboard.v1";

fn utc_now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

const DDL: &str = "
CREATE TABLE IF NOT EXISTS dispatch_history (
    history_id INTEGER PRIMARY KEY AUTOINCREMENT,
    dispatch_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    raw_request TEXT NOT NULL,
    request_source TEXT NOT NULL,
    final_status TEXT NOT NULL,
    selected_tier TEXT NOT NULL,
    risk_level TEXT NOT NULL,
    reserved_cost REAL NOT NULL DEFAULT 0.0,
    bundle_json TEXT NOT NULL,
    input_tokens INTEGER,
    output_tokens INTEGER,
    estimated_cost_usd REAL,
    executor_type TEXT NOT NULL DEFAULT 'noop',
    latency_ms INTEGER
);
CREATE INDEX IF NOT EXISTS idx_dispatch_history_created ON dispatch_history(created_at);
CREATE INDEX IF NOT EXISTS idx_dispatch_history_dispatch_id ON dispatch_history(dispatch_id);

CREATE TABLE IF NOT EXISTS local_config (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    updated_by TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS team_members (
    user_id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    role TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS api_key_metadata (
    key_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    role TEXT NOT NULL,
    scopes_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    created_by TEXT NOT NULL,
    revoked_at TEXT,
    last_used_at TEXT,
    expires_at TEXT
);

CREATE TABLE IF NOT EXISTS audit_log (
    audit_id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT NOT NULL,
    actor TEXT NOT NULL,
    action TEXT NOT NULL,
    resource TEXT NOT NULL,
    details_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_audit_created ON audit_log(created_at);

CREATE TABLE IF NOT EXISTS provider_audit_events (
    event_id TEXT PRIMARY KEY,
    dispatch_id TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    input_token_count INTEGER,
    output_token_count INTEGER,
    cost REAL,
    currency TEXT,
    latency_ms INTEGER,
    error_domain TEXT,
    redaction_status TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_provider_audit_dispatch ON provider_audit_events(dispatch_id);
CREATE INDEX IF NOT EXISTS idx_provider_audit_created ON provider_audit_events(created_at);

CREATE TABLE IF NOT EXISTS workflow_plans (
    plan_sequence INTEGER PRIMARY KEY,
    plan_id TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    raw_request TEXT NOT NULL,
    request_source TEXT NOT NULL,
    status TEXT NOT NULL,
    workflow_id TEXT NOT NULL,
    dispatch_id TEXT NOT NULL,
    graph_json TEXT NOT NULL,
    analysis_json TEXT NOT NULL,
    boundaries_json TEXT NOT NULL,
    plan_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_workflow_plans_created ON workflow_plans(created_at);
CREATE INDEX IF NOT EXISTS idx_workflow_plans_status ON workflow_plans(status);

CREATE TABLE IF NOT EXISTS workflow_runs (
    run_sequence INTEGER PRIMARY KEY,
    run_id TEXT NOT NULL UNIQUE,
    plan_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    status TEXT NOT NULL,
    workflow_id TEXT NOT NULL,
    dispatch_id TEXT,
    started_at TEXT,
    completed_at TEXT,
    last_heartbeat_at TEXT,
    result_json TEXT,
    boundaries_json TEXT NOT NULL,
    run_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_workflow_runs_created ON workflow_runs(created_at);
CREATE INDEX IF NOT EXISTS idx_workflow_runs_status ON workflow_runs(status);
CREATE INDEX IF NOT EXISTS idx_workflow_runs_plan ON workflow_runs(plan_id);

CREATE TABLE IF NOT EXISTS workflow_run_nodes (
    run_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    task_type TEXT NOT NULL,
    status TEXT NOT NULL,
    node_json TEXT NOT NULL,
    started_at TEXT,
    completed_at TEXT,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    timeout_ms INTEGER,
    blocked_reason TEXT,
    leased_at TEXT,
    profile_id TEXT,
    PRIMARY KEY (run_id, node_id)
);
CREATE INDEX IF NOT EXISTS idx_workflow_run_nodes_run ON workflow_run_nodes(run_id);

CREATE TABLE IF NOT EXISTS workflow_run_edges (
    run_id TEXT NOT NULL,
    edge_id TEXT NOT NULL,
    from_node_id TEXT NOT NULL,
    to_node_id TEXT NOT NULL,
    edge_type TEXT NOT NULL,
    edge_json TEXT NOT NULL,
    PRIMARY KEY (run_id, edge_id)
);
CREATE INDEX IF NOT EXISTS idx_workflow_run_edges_run ON workflow_run_edges(run_id);

CREATE TABLE IF NOT EXISTS workflow_run_events (
    event_sequence INTEGER PRIMARY KEY,
    event_id TEXT NOT NULL UNIQUE,
    run_id TEXT NOT NULL,
    node_id TEXT,
    event_type TEXT NOT NULL,
    actor TEXT NOT NULL,
    created_at TEXT NOT NULL,
    details_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_workflow_run_events_run ON workflow_run_events(run_id);
CREATE INDEX IF NOT EXISTS idx_workflow_run_events_created ON workflow_run_events(created_at);

CREATE TABLE IF NOT EXISTS workflow_run_approvals (
    approval_sequence INTEGER PRIMARY KEY,
    approval_id TEXT NOT NULL UNIQUE,
    run_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    decision TEXT NOT NULL,
    actor TEXT NOT NULL,
    reason TEXT,
    created_at TEXT NOT NULL,
    approval_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_workflow_run_approvals_run ON workflow_run_approvals(run_id);

CREATE TABLE IF NOT EXISTS supervised_patch_workspaces (
    workspace_sequence INTEGER PRIMARY KEY,
    workspace_id TEXT NOT NULL UNIQUE,
    plan_id TEXT,
    run_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    target_repo_path TEXT NOT NULL,
    target_repo_canonical_path TEXT NOT NULL,
    workspace_path TEXT NOT NULL,
    workspace_canonical_path TEXT NOT NULL,
    source_revision TEXT NOT NULL,
    source_tree_hash TEXT,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    boundary_json TEXT NOT NULL,
    workspace_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_supervised_patch_workspaces_run ON supervised_patch_workspaces(run_id);
CREATE INDEX IF NOT EXISTS idx_supervised_patch_workspaces_target ON supervised_patch_workspaces(target_id, source_revision);
CREATE INDEX IF NOT EXISTS idx_supervised_patch_workspaces_status ON supervised_patch_workspaces(status);

CREATE TABLE IF NOT EXISTS supervised_patch_artifacts (
    artifact_sequence INTEGER PRIMARY KEY,
    artifact_id TEXT NOT NULL UNIQUE,
    workspace_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    plan_id TEXT,
    target_id TEXT NOT NULL,
    source_revision TEXT NOT NULL,
    artifact_type TEXT NOT NULL,
    patch_hash TEXT NOT NULL,
    changed_files_json TEXT NOT NULL,
    redaction_status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    artifact_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_supervised_patch_artifacts_workspace ON supervised_patch_artifacts(workspace_id);
CREATE INDEX IF NOT EXISTS idx_supervised_patch_artifacts_run ON supervised_patch_artifacts(run_id);
CREATE INDEX IF NOT EXISTS idx_supervised_patch_artifacts_created ON supervised_patch_artifacts(created_at);

CREATE TABLE IF NOT EXISTS scheduler_feedback (
    feedback_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    node_id TEXT,
    executor_type TEXT NOT NULL,
    task_group TEXT NOT NULL,
    task_domain TEXT NOT NULL,
    task_intent TEXT NOT NULL,
    success INTEGER NOT NULL DEFAULT 0,
    latency_ms INTEGER NOT NULL DEFAULT 0,
    retry_count INTEGER NOT NULL DEFAULT 0,
    quality_score REAL NOT NULL DEFAULT 0.0,
    cost REAL NOT NULL DEFAULT 0.0,
    error_domain TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_scheduler_feedback_run ON scheduler_feedback(run_id);
CREATE INDEX IF NOT EXISTS idx_scheduler_feedback_task_group ON scheduler_feedback(task_group);
CREATE INDEX IF NOT EXISTS idx_scheduler_feedback_created ON scheduler_feedback(created_at);

CREATE TABLE IF NOT EXISTS agent_profiles (
    profile_id TEXT PRIMARY KEY,
    role TEXT NOT NULL,
    tools_json TEXT NOT NULL,
    model_hint TEXT,
    context_budget_tokens INTEGER,
    workspace_scope TEXT NOT NULL DEFAULT 'task',
    executor_preference TEXT,
    max_retries INTEGER NOT NULL DEFAULT 3,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tool_capabilities (
    tool_name TEXT PRIMARY KEY,
    description TEXT NOT NULL,
    input_schema_json TEXT,
    output_schema_json TEXT,
    requires_approval INTEGER NOT NULL DEFAULT 0,
    risk_level TEXT NOT NULL DEFAULT 'low',
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tool_allowlists (
    profile_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (profile_id, tool_name)
);

CREATE TABLE IF NOT EXISTS tool_hooks (
    hook_id TEXT PRIMARY KEY,
    hook_type TEXT NOT NULL,
    tool_name TEXT,
    condition_json TEXT,
    action TEXT NOT NULL,
    action_config_json TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS orchestration_decisions (
    decision_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    node_id TEXT,
    action TEXT NOT NULL,
    action_reason TEXT NOT NULL,
    selected_executor TEXT NOT NULL,
    blocked_reason TEXT,
    confidence TEXT NOT NULL,
    confidence_score REAL NOT NULL,
    input_signals_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_orchestration_decisions_run ON orchestration_decisions(run_id);
CREATE INDEX IF NOT EXISTS idx_orchestration_decisions_action ON orchestration_decisions(action);
CREATE INDEX IF NOT EXISTS idx_orchestration_decisions_created ON orchestration_decisions(created_at);
";

pub struct LocalProductStore {
    db_path: PathBuf,
    conn: Mutex<Connection>,
    clock: Box<dyn Fn() -> String + Send + Sync>,
}

impl LocalProductStore {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, String> {
        Self::new_with_clock(path, utc_now)
    }

    pub fn new_with_clock(
        path: impl AsRef<Path>,
        clock: impl Fn() -> String + Send + Sync + 'static,
    ) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }
        let conn = Connection::open(&path).map_err(|e| e.to_string())?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .map_err(|e| e.to_string())?;
        conn.execute_batch(DDL).map_err(|e| e.to_string())?;
        let store = Self {
            db_path: path,
            conn: Mutex::new(conn),
            clock: Box::new(clock),
        };
        store.ensure_default_config()?;
        store.run_migrations()?;
        Ok(store)
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub(super) fn now(&self) -> String {
        (self.clock)()
    }

    pub fn is_memory(&self) -> bool {
        self.db_path == Path::new(":memory:")
    }

    pub(super) fn with_conn<F, R>(&self, f: F) -> Result<R, String>
    where
        F: FnOnce(&Connection) -> Result<R, String>,
    {
        let guard = self.conn.lock().map_err(|e| e.to_string())?;
        f(&guard)
    }

    pub fn checkpoint_wal(&self) -> Result<(), String> {
        self.with_conn(|conn| {
            conn.execute_batch("PRAGMA wal_checkpoint(FULL);")
                .map_err(|e| e.to_string())
        })
    }

    pub fn stats(&self) -> Result<Value, String> {
        self.with_conn(|conn| {
            let secret_block_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM supervised_patch_artifacts WHERE artifact_json LIKE '%\"secret_scan_status\":\"blocked\"%'",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            let queue_length: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM workflow_run_nodes WHERE status = 'pending'",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            let artifact_count: i64 = count_table(conn, "supervised_patch_artifacts")?;
            let approval_count: i64 = count_table(conn, "workflow_run_approvals")?;
            let executor_latency_avg_ms: Value = conn
                .query_row(
                    "SELECT COALESCE(AVG(latency_ms), 0) FROM dispatch_history WHERE latency_ms IS NOT NULL",
                    [],
                    |row| {
                        let avg: f64 = row.get(0)?;
                        Ok(json!(avg))
                    },
                )
                .unwrap_or(Value::Null);
            Ok(json!({
                "dispatches": count_table(conn, "dispatch_history")?,
                "team_members": count_table(conn, "team_members")?,
                "api_keys": count_table(conn, "api_key_metadata")?,
                "audit_events": count_table(conn, "audit_log")?,
                "plans": count_table(conn, "workflow_plans")?,
                "workflow_runs": count_table(conn, "workflow_runs")?,
                "supervised_patch_workspaces": count_table(conn, "supervised_patch_workspaces")?,
                "supervised_patch_artifacts": artifact_count,
                "secret_block_count": secret_block_count,
                "queue_length": queue_length,
                "artifact_count": artifact_count,
                "approval_count": approval_count,
                "executor_latency_avg_ms": executor_latency_avg_ms,
            }))
        })
    }

    pub fn dashboard_snapshot(
        &self,
        limit: i64,
        executor_type: &str,
        provider_enabled: bool,
    ) -> Result<Value, String> {
        let dispatches = self.list_dispatches(limit)?;
        let team = self.team_snapshot()?;
        let config = self.config_snapshot()?;
        let costs = self.cost_summary()?;
        let counts = self.stats()?;
        Ok(json!({
            "schema_version": LOCAL_DASHBOARD_SCHEMA_VERSION,
            "status": "ready",
            "counts": counts,
            "dispatches": dispatches,
            "team": team,
            "config": config,
            "costs": costs,
            "boundaries": local_boundaries(executor_type, provider_enabled),
        }))
    }
}

pub(super) fn append_audit_locked(
    conn: &Connection,
    now: &str,
    actor: &str,
    action: &str,
    resource: &str,
    details: &Value,
) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![now, actor, action, resource, details.to_string()],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

fn count_table(conn: &Connection, table: &str) -> Result<i64, String> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    conn.query_row(&sql, [], |row| row.get(0))
        .map_err(|e| e.to_string())
}

pub(super) fn collect_values(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row) -> rusqlite::Result<Value>>,
) -> Result<Vec<Value>, String> {
    let mut values = Vec::new();
    for row in rows {
        values.push(row.map_err(|e| e.to_string())?);
    }
    Ok(values)
}

pub(super) fn str_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for part in path {
        current = current.get(*part)?;
    }
    current.as_str()
}
