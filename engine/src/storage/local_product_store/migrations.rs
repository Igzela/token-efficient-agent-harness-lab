use rusqlite::Connection;

use super::LocalProductStore;

pub(super) const CURRENT_SCHEMA_VERSION: i64 = 13;

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
    Migration {
        version: 5,
        description: "add agent_profiles table and profile_id column on workflow_run_nodes",
    },
    Migration {
        version: 6,
        description: "add tool_capabilities, tool_allowlists, and tool_hooks tables",
    },
    Migration {
        version: 7,
        description: "add orchestration_decisions table for policy decision trace",
    },
    Migration {
        version: 8,
        description: "add executor_pool table for resource/executor pool tracking",
    },
    Migration {
        version: 9,
        description: "add queue/priority/backpressure columns to workflow_runs",
    },
    Migration {
        version: 10,
        description: "add policy signal columns to orchestration_decisions",
    },
    Migration {
        version: 11,
        description: "add scheduler_heartbeat table for persistent heartbeat",
    },
    Migration {
        version: 12,
        description: "add controlled loop policy proposal table",
    },
    Migration {
        version: 13,
        description: "add controlled loop policy snapshot table",
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
                    5 => Self::migrate_v5_add_agent_profiles(conn)?,
                    6 => Self::migrate_v6_add_tool_registry(conn)?,
                    7 => Self::migrate_v7_add_orchestration_decisions(conn)?,
                    8 => Self::migrate_v8_add_executor_pool(conn)?,
                    9 => Self::migrate_v9_add_queue_priority(conn)?,
                    10 => Self::migrate_v10_add_decision_policy_signals(conn)?,
                    11 => Self::migrate_v11_add_scheduler_heartbeat(conn)?,
                    12 => Self::migrate_v12_add_policy_proposals(conn)?,
                    13 => Self::migrate_v13_add_policy_snapshots(conn)?,
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

    fn migrate_v5_add_agent_profiles(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "
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
",
        )
        .map_err(|e| e.to_string())?;

        // Add profile_id column to workflow_run_nodes if missing
        let columns: Vec<String> = {
            let mut stmt = conn
                .prepare("PRAGMA table_info(workflow_run_nodes)")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(|e| e.to_string())?;
            rows.filter_map(|r| r.ok()).collect()
        };
        if !columns.contains(&"profile_id".to_string()) {
            conn.execute_batch("ALTER TABLE workflow_run_nodes ADD COLUMN profile_id TEXT;")
                .map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    fn migrate_v6_add_tool_registry(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "
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
",
        )
        .map_err(|e| e.to_string())
    }

    fn migrate_v7_add_orchestration_decisions(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "
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
",
        )
        .map_err(|e| e.to_string())
    }

    fn migrate_v8_add_executor_pool(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "
CREATE TABLE IF NOT EXISTS executor_pool (
    executor_type TEXT PRIMARY KEY,
    capabilities_json TEXT NOT NULL DEFAULT '{}',
    status_json TEXT NOT NULL DEFAULT '{}',
    cost_profile_json TEXT NOT NULL DEFAULT '{}',
    metrics_json TEXT NOT NULL DEFAULT '{}',
    updated_at TEXT NOT NULL
);
",
        )
        .map_err(|e| e.to_string())
    }

    fn migrate_v9_add_queue_priority(conn: &Connection) -> Result<(), String> {
        let has_priority = column_exists(conn, "workflow_runs", "priority")?;
        if !has_priority {
            conn.execute_batch(
                "ALTER TABLE workflow_runs ADD COLUMN priority INTEGER NOT NULL DEFAULT 5;
                 ALTER TABLE workflow_runs ADD COLUMN deadline_at TEXT;
                 ALTER TABLE workflow_runs ADD COLUMN sla_ms INTEGER;
                 ALTER TABLE workflow_runs ADD COLUMN tenant_id TEXT;
                 ALTER TABLE workflow_runs ADD COLUMN queue_position INTEGER;
                 ALTER TABLE workflow_runs ADD COLUMN pause_reason TEXT;
                 ALTER TABLE workflow_runs ADD COLUMN degrade_mode TEXT;",
            )
            .map_err(|e| e.to_string())?;
        }
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_workflow_runs_priority ON workflow_runs(priority, created_at);",
        )
        .map_err(|e| e.to_string())
    }

    fn migrate_v10_add_decision_policy_signals(conn: &Connection) -> Result<(), String> {
        let has_col = column_exists(conn, "orchestration_decisions", "quality_signal_json")?;
        if !has_col {
            conn.execute_batch(
                "ALTER TABLE orchestration_decisions ADD COLUMN quality_signal_json TEXT;
                 ALTER TABLE orchestration_decisions ADD COLUMN routing_signal_json TEXT;
                 ALTER TABLE orchestration_decisions ADD COLUMN cost_signal_json TEXT;
                 ALTER TABLE orchestration_decisions ADD COLUMN approval_signal_json TEXT;
                 ALTER TABLE orchestration_decisions ADD COLUMN queue_signal_json TEXT;
                 ALTER TABLE orchestration_decisions ADD COLUMN executor_pool_signal_json TEXT;
                 ALTER TABLE orchestration_decisions ADD COLUMN candidate_executors_json TEXT;
                 ALTER TABLE orchestration_decisions ADD COLUMN degraded_reason TEXT;",
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn migrate_v11_add_scheduler_heartbeat(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "
CREATE TABLE IF NOT EXISTS scheduler_heartbeat (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    last_heartbeat_at TEXT NOT NULL DEFAULT '',
    tick_count INTEGER NOT NULL DEFAULT 0,
    error_count INTEGER NOT NULL DEFAULT 0,
    uptime_seconds REAL NOT NULL DEFAULT 0.0,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    updated_at TEXT NOT NULL DEFAULT ''
);
INSERT OR IGNORE INTO scheduler_heartbeat (id, last_heartbeat_at, tick_count, error_count, uptime_seconds, metadata_json, updated_at)
VALUES (1, '', 0, 0, 0.0, '{}', '');
",
        )
        .map_err(|e| e.to_string())
    }

    fn migrate_v12_add_policy_proposals(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "
CREATE TABLE IF NOT EXISTS controlled_loop_policy_proposals (
    proposal_sequence INTEGER PRIMARY KEY,
    proposal_id TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    status TEXT NOT NULL,
    title TEXT NOT NULL,
    summary TEXT,
    task_domain TEXT NOT NULL,
    task_intent TEXT NOT NULL,
    target_tier TEXT NOT NULL,
    evidence_json TEXT NOT NULL,
    approval_json TEXT,
    proposal_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_policy_proposals_status ON controlled_loop_policy_proposals(status);
CREATE INDEX IF NOT EXISTS idx_policy_proposals_key ON controlled_loop_policy_proposals(task_domain, task_intent, status);
",
        )
        .map_err(|e| e.to_string())
    }

    fn migrate_v13_add_policy_snapshots(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "
CREATE TABLE IF NOT EXISTS controlled_loop_policy_snapshots (
    snapshot_sequence INTEGER PRIMARY KEY,
    adjustment_id TEXT NOT NULL UNIQUE,
    snapshot_id TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    status TEXT NOT NULL,
    actor TEXT NOT NULL,
    created_by TEXT NOT NULL,
    source TEXT NOT NULL,
    candidate_id TEXT NOT NULL,
    proposal_id TEXT NOT NULL,
    policy_key TEXT NOT NULL,
    target_tier TEXT NOT NULL,
    active_policy_before_json TEXT NOT NULL,
    rollback_target_json TEXT NOT NULL,
    evidence_ids_json TEXT NOT NULL,
    safety_hash TEXT NOT NULL,
    snapshot_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_policy_snapshots_status ON controlled_loop_policy_snapshots(status);
CREATE INDEX IF NOT EXISTS idx_policy_snapshots_proposal ON controlled_loop_policy_snapshots(proposal_id);
CREATE INDEX IF NOT EXISTS idx_policy_snapshots_adjustment ON controlled_loop_policy_snapshots(adjustment_id);
CREATE INDEX IF NOT EXISTS idx_policy_snapshots_policy_key ON controlled_loop_policy_snapshots(policy_key);
CREATE UNIQUE INDEX IF NOT EXISTS idx_policy_snapshots_active_policy_key
    ON controlled_loop_policy_snapshots(policy_key)
    WHERE status = 'active';
",
        )
        .map_err(|e| e.to_string())
    }

    pub fn schema_version(&self) -> Result<i64, String> {
        self.with_conn(|conn| {
            conn.query_row("PRAGMA user_version", [], |row| row.get(0))
                .map_err(|e| e.to_string())
        })
    }
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, String> {
    let sql = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let columns: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(columns.contains(&column.to_string()))
}
