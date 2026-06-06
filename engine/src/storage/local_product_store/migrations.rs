use rusqlite::Connection;

use super::LocalProductStore;

pub(super) const CURRENT_SCHEMA_VERSION: i64 = 4;

struct Migration {
    version: i64,
    description: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        description: "add last_used_at and expires_at to api_key_metadata",
    },
    Migration {
        version: 2,
        description: "add inert workflow run state tables",
    },
    Migration {
        version: 3,
        description: "add supervised patch metadata tables",
    },
    Migration {
        version: 4,
        description: "add scheduler feedback table for feedback-driven routing",
    },
];

impl LocalProductStore {
    pub(super) fn run_migrations(&self) -> Result<(), String> {
        self.with_conn(|conn| {
            let current_version: i64 = conn
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .map_err(|e| e.to_string())?;

            for migration in MIGRATIONS {
                if migration.version <= current_version {
                    continue;
                }
                match migration.version {
                    1 => Self::migrate_v1_add_key_columns(conn)?,
                    2 => Self::migrate_v2_add_workflow_run_tables(conn)?,
                    3 => Self::migrate_v3_add_supervised_patch_tables(conn)?,
                    4 => Self::migrate_v4_add_scheduler_feedback(conn)?,
                    _ => return Err(format!("unknown migration version: {}", migration.version)),
                }
                conn.execute_batch(&format!("PRAGMA user_version = {}", migration.version))
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        })
    }

    fn migrate_v2_add_workflow_run_tables(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "
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
",
        )
        .map_err(|e| e.to_string())
    }

    fn migrate_v3_add_supervised_patch_tables(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "
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
",
        )
        .map_err(|e| e.to_string())
    }

    fn migrate_v4_add_scheduler_feedback(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "
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
",
        )
        .map_err(|e| e.to_string())
    }

    fn migrate_v1_add_key_columns(conn: &Connection) -> Result<(), String> {
        let columns: Vec<String> = {
            let mut stmt = conn
                .prepare("PRAGMA table_info(api_key_metadata)")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(|e| e.to_string())?;
            rows.filter_map(|r| r.ok()).collect()
        };
        if !columns.contains(&"last_used_at".to_string()) {
            conn.execute_batch(
                "ALTER TABLE api_key_metadata ADD COLUMN last_used_at TEXT;
                 ALTER TABLE api_key_metadata ADD COLUMN expires_at TEXT;",
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn schema_version(&self) -> Result<i64, String> {
        self.with_conn(|conn| {
            conn.query_row("PRAGMA user_version", [], |row| row.get(0))
                .map_err(|e| e.to_string())
        })
    }
}
