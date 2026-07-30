use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde_json::{json, Value};

use super::{schema, DatabaseConnection, LocalProductStore};

pub(super) const V22_SCHEMA_VERSION: i64 = 22;
pub(super) const V23_SCHEMA_VERSION: i64 = 23;
pub(super) const V24_SCHEMA_VERSION: i64 = 24;
pub(super) const V25_SCHEMA_VERSION: i64 = 25;
pub(super) const V26_SCHEMA_VERSION: i64 = 26;
pub(super) const V27_SCHEMA_VERSION: i64 = 27;
pub(super) const V28_SCHEMA_VERSION: i64 = 28;
pub(super) const V29_SCHEMA_VERSION: i64 = 29;
pub(super) const V30_SCHEMA_VERSION: i64 = 30;
pub(super) const V31_SCHEMA_VERSION: i64 = 31;
pub(super) const V32_SCHEMA_VERSION: i64 = 32;
pub(super) const V33_SCHEMA_VERSION: i64 = 33;
pub(super) const V34_SCHEMA_VERSION: i64 = 34;
pub(super) const V35_SCHEMA_VERSION: i64 = 35;
pub(super) const V36_SCHEMA_VERSION: i64 = 36;
const V21_SCHEMA_VERSION: i64 = 21;
pub(super) const V22_TABLES: [&str; 3] = [
    "agent_action_receipts",
    "tool_allowlist_profiles",
    "tool_execution_authorizations",
];
pub(super) const V23_TABLES: [&str; 6] = [
    "durable_memory_versions",
    "memory_retrieval_events",
    "production_jobs",
    "normalized_usage_observations",
    "replay_producer_bindings",
    "operator_acknowledgements",
];
pub(super) const V24_TABLES: [&str; 2] = [
    "external_runtime_checkpoints",
    "external_runtime_invocations",
];
pub(super) const V26_TABLES: [&str; 2] = ["recursive_execution_trees", "recursive_execution_nodes"];
pub(super) const V27_TABLES: [&str; 4] = [
    "harness_evolution_active_identity",
    "harness_evolution_proposals",
    "harness_evolution_candidates",
    "harness_evolution_receipts",
];
pub(super) const V28_TABLES: [&str; 4] = [
    "harness_evolution_sealed_holdouts",
    "harness_evolution_evaluations",
    "harness_evolution_pareto_archive",
    "harness_evolution_eval_receipts",
];
pub(super) const V29_TABLES: [&str; 2] = [
    "harness_evolution_pr_ready_bundles",
    "harness_evolution_pr_ready_receipts",
];
pub(super) const V30_TABLES: [&str; 1] = ["product_tasks"];
pub(super) const V31_TABLES: [&str; 1] = ["product_task_terminal_evidence"];
pub(super) const V32_TABLES: [&str; 4] = [
    "managed_acceptance_decisions",
    "managed_acceptance_authorizations",
    "managed_acceptance_attempts",
    "managed_acceptance_decision_transition_receipts",
];
pub(super) const V33_TABLES: [&str; 1] = ["managed_acceptance_spend_authorizations"];
pub(super) const V34_TABLES: [&str; 3] =
    ["rwe_run_authorizations", "rwe_runs", "rwe_task_attempts"];
pub(super) const V35_TABLES: [&str; 1] = ["product_task_workspace_preparations"];
pub(super) const V36_TABLES: [&str; 1] = ["managed_acceptance_delegations"];
pub(super) const V36_COLUMNS: [&str; 36] = [
    "delegation_id",
    "tenant_id",
    "principal_kind",
    "principal_id",
    "manifest_approver_id",
    "artifact_confirmer_id",
    "attempt_activator_id",
    "delegation_sha256",
    "body_json",
    "proposal_sha256",
    "proposal_json",
    "status",
    "executions_allowed",
    "executions_used",
    "max_total_cost_usd",
    "total_cost_usd",
    "spend_authorization_id",
    "manifest_approval_sha256",
    "manifest_approval_json",
    "spend_body_sha256",
    "spend_status",
    "spend_body_json",
    "manifest_json",
    "attempt_id",
    "attempt_lease_id",
    "attempt_lease_token",
    "attempt_status",
    "artifact_confirmation_sha256",
    "artifact_confirmation_json",
    "provider_request_journal_json",
    "terminal_receipt_json",
    "created_at",
    "updated_at",
    "expires_at",
    "terminal_at",
    "revoked_at",
];
pub(super) const V36_INDEXES: [&str; 4] = [
    "idx_managed_acceptance_delegations_status",
    "idx_managed_acceptance_delegations_spend",
    "idx_managed_acceptance_delegations_attempt",
    "idx_managed_acceptance_delegations_lease",
];

#[allow(dead_code)]
pub(super) const CURRENT_SCHEMA_VERSION: i64 = schema::CURRENT_SQLITE_SCHEMA_VERSION;

impl LocalProductStore {
    /// Roll back only an empty delegated-authority table. Delegation rows are
    /// durable authority evidence, so rollback refuses to erase any row.
    pub fn rollback_v36_to_v35(
        &self,
        actor: &str,
        confirm_destructive_rollback: bool,
    ) -> Result<(), String> {
        if !confirm_destructive_rollback {
            return Err("v36 rollback requires explicit destructive rollback confirmation".into());
        }
        let actor = actor.trim();
        if actor.is_empty() || actor.len() > 128 {
            return Err("v36 rollback actor must be between 1 and 128 bytes".into());
        }
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.rollback_sqlite_v36_to_v35(actor, &now),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.rollback_pg_v36_to_v35(actor, &now),
        }
    }

    pub(super) fn run_migrations(&self) -> Result<(), String> {
        self.with_conn(|conn| {
            for migration in schema::SQLITE_MIGRATIONS {
                let current_version: i64 = conn
                    .query_row("PRAGMA user_version", [], |row| row.get(0))
                    .map_err(|e| e.to_string())?;
                if migration.version == V25_SCHEMA_VERSION && current_version >= V25_SCHEMA_VERSION
                {
                    Self::migrate_v25_add_provider_embedding_bindings(conn)?;
                    continue;
                }
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
                    14 => Self::migrate_v14_add_agent_state_and_mailbox(conn)?,
                    15 => Self::migrate_v15_add_agent_proposals(conn)?,
                    16 => Self::migrate_v16_add_native_scorecard_artifacts(conn)?,
                    17 => Self::migrate_v17_add_regression_report_artifacts(conn)?,
                    18 => Self::migrate_v18_add_budget_evidence_artifacts(conn)?,
                    19 => Self::migrate_v19_add_budget_pause_decisions(conn)?,
                    20 => Self::migrate_v20_add_offline_replay_artifacts(conn)?,
                    21 => Self::migrate_v21_add_dispatch_trace_provenance(conn)?,
                    22 => Self::migrate_v22_add_agent_action_receipts_and_tool_profiles(conn)?,
                    23 => Self::migrate_v23_add_durable_memory_and_production_jobs(conn)?,
                    24 => Self::migrate_v24_add_external_runtime_state(conn)?,
                    25 => {
                        Self::migrate_v25_add_provider_embedding_bindings(conn)?;
                        continue;
                    }
                    V26_SCHEMA_VERSION => Self::migrate_v26_add_recursive_execution_state(conn)?,
                    V27_SCHEMA_VERSION => Self::migrate_v27_add_harness_evolution_state(conn)?,
                    V28_SCHEMA_VERSION => Self::migrate_v28_add_harness_evolution_eval_state(conn)?,
                    V29_SCHEMA_VERSION => {
                        Self::migrate_v29_add_harness_evolution_pr_ready_state(conn)?
                    }
                    V30_SCHEMA_VERSION => Self::migrate_v30_add_product_tasks(conn)?,
                    V31_SCHEMA_VERSION => Self::migrate_v31_add_product_terminal_evidence(conn)?,
                    V32_SCHEMA_VERSION => Self::migrate_v32_add_managed_acceptance(conn)?,
                    V33_SCHEMA_VERSION => Self::migrate_v33_add_managed_acceptance_spend(conn)?,
                    V34_SCHEMA_VERSION => Self::migrate_v34_add_rwe_authority(conn)?,
                    V35_SCHEMA_VERSION => {
                        Self::migrate_v35_add_product_workspace_preparations(conn)?
                    }
                    V36_SCHEMA_VERSION => Self::migrate_v36_add_delegations(conn)?,
                    _ => return Err(format!("unknown migration version: {}", migration.version)),
                }
                conn.execute_batch(&format!("PRAGMA user_version = {}", migration.version))
                    .map_err(|e| e.to_string())?;
            }
            let final_version: i64 = conn
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .map_err(|e| e.to_string())?;
            if final_version == V36_SCHEMA_VERSION {
                validate_sqlite_v36_schema(conn)?;
            } else if final_version == V35_SCHEMA_VERSION {
                validate_sqlite_v35_schema(conn)?;
            } else if final_version == V34_SCHEMA_VERSION {
                validate_sqlite_v34_schema(conn)?;
            } else if final_version == V33_SCHEMA_VERSION {
                // V33 is intentionally repaired in place as well as migrated
                // from v32. Older v33 databases predate the logical identity
                // column/check/index and must not be accepted by the validator.
                repair_sqlite_v33_spend_schema(conn)?;
                validate_sqlite_v33_schema(conn)?;
            } else if final_version == V32_SCHEMA_VERSION {
                validate_sqlite_v32_schema(conn)?;
            } else if final_version == V31_SCHEMA_VERSION {
                validate_sqlite_v31_schema(conn)?;
            } else if final_version == V30_SCHEMA_VERSION {
                validate_sqlite_v30_schema(conn)?;
            } else if final_version == V29_SCHEMA_VERSION {
                validate_sqlite_v29_schema(conn)?;
            } else if final_version == V28_SCHEMA_VERSION {
                validate_sqlite_v28_schema(conn)?;
            } else if final_version == V27_SCHEMA_VERSION {
                validate_sqlite_v27_schema(conn)?;
            } else if final_version == V26_SCHEMA_VERSION {
                validate_sqlite_v26_schema(conn)?;
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
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row("PRAGMA user_version", [], |row| row.get(0))
                    .map_err(|e| e.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_one(
                        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                        &[],
                    )
                    .map(|row| row.get::<_, i64>(0))
                    .map_err(|error| error.to_string())
            }),
        }
    }

    /// Roll back the additive v22 schema to v21 only when no v22 authority rows exist.
    ///
    /// The caller must stop v22 runtime writers before invoking this operation. The method
    /// locks the version marker and v22 tables, refuses to discard receipts or authorization
    /// state, records the successful rollback in the v1 audit log, and changes the version
    /// marker in the same transaction as the table removal.
    pub fn rollback_v22_to_v21(
        &self,
        actor: &str,
        confirm_destructive_rollback: bool,
    ) -> Result<(), String> {
        if !confirm_destructive_rollback {
            return Err(
                "v22 rollback requires explicit destructive rollback confirmation".to_string(),
            );
        }
        let actor = actor.trim();
        if actor.is_empty() || actor.len() > 128 {
            return Err("v22 rollback actor must be between 1 and 128 bytes".to_string());
        }
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.rollback_sqlite_v22_to_v21(actor, &now),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.rollback_pg_v22_to_v21_internal(actor, &now),
        }
    }

    /// Roll back the additive v23 schema to v22 only when no v23 authority rows exist.
    ///
    /// Runtime writers must be stopped before this operation. The version marker, empty
    /// table removal, and rollback audit are committed atomically.
    pub fn rollback_v23_to_v22(
        &self,
        actor: &str,
        confirm_destructive_rollback: bool,
    ) -> Result<(), String> {
        if !confirm_destructive_rollback {
            return Err(
                "v23 rollback requires explicit destructive rollback confirmation".to_string(),
            );
        }
        let actor = actor.trim();
        if actor.is_empty() || actor.len() > 128 {
            return Err("v23 rollback actor must be between 1 and 128 bytes".to_string());
        }
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.rollback_sqlite_v23_to_v22(actor, &now),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.rollback_pg_v23_to_v22_internal(actor, &now),
        }
    }

    /// Roll back v25 only when no provider embedding binding has been stored.
    /// Runtime writers must be stopped before this operation.
    pub fn rollback_v25_to_v24(
        &self,
        actor: &str,
        confirm_destructive_rollback: bool,
    ) -> Result<(), String> {
        if !confirm_destructive_rollback {
            return Err(
                "v25 rollback requires explicit destructive rollback confirmation".to_string(),
            );
        }
        let actor = actor.trim();
        if actor.is_empty() || actor.len() > 128 {
            return Err("v25 rollback actor must be between 1 and 128 bytes".to_string());
        }
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.rollback_sqlite_v25_to_v24(actor, &now),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.rollback_pg_v25_to_v24_internal(actor, &now),
        }
    }

    /// Roll back durable ProductTask preparation receipts only after the
    /// preparation surface has been drained. A receipt pins a physical
    /// worktree path, so dropping an occupied table would make an older binary
    /// unsafe to resume or compensate.
    pub fn rollback_v35_to_v34(
        &self,
        actor: &str,
        confirm_destructive_rollback: bool,
    ) -> Result<(), String> {
        if !confirm_destructive_rollback {
            return Err(
                "v35 rollback requires explicit destructive rollback confirmation".to_string(),
            );
        }
        let actor = actor.trim();
        if actor.is_empty() || actor.len() > 128 {
            return Err("v35 rollback actor must be between 1 and 128 bytes".to_string());
        }
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.rollback_sqlite_v35_to_v34(actor, &now),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.rollback_pg_v35_to_v34_internal(actor, &now),
        }
    }

    /// Roll back canonical terminal evidence only when no evidence rows exist.
    pub fn rollback_v34_to_v33(
        &self,
        actor: &str,
        confirm_destructive_rollback: bool,
    ) -> Result<(), String> {
        if !confirm_destructive_rollback {
            return Err(
                "v34 rollback requires explicit destructive rollback confirmation".to_string(),
            );
        }
        let actor = actor.trim();
        if actor.is_empty() || actor.len() > 128 {
            return Err("v34 rollback actor must be between 1 and 128 bytes".to_string());
        }
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.rollback_sqlite_v34_to_v33(actor, &now),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.rollback_pg_v34_to_v33_internal(actor, &now),
        }
    }

    pub fn rollback_v33_to_v32(
        &self,
        actor: &str,
        confirm_destructive_rollback: bool,
    ) -> Result<(), String> {
        if !confirm_destructive_rollback {
            return Err(
                "v33 rollback requires explicit destructive rollback confirmation".to_string(),
            );
        }
        let actor = actor.trim();
        if actor.is_empty() || actor.len() > 128 {
            return Err("v33 rollback actor must be between 1 and 128 bytes".to_string());
        }
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.rollback_sqlite_v33_to_v32(actor, &now),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.rollback_pg_v33_to_v32_internal(actor, &now),
        }
    }

    pub fn rollback_v32_to_v31(
        &self,
        actor: &str,
        confirm_destructive_rollback: bool,
    ) -> Result<(), String> {
        if !confirm_destructive_rollback {
            return Err(
                "v32 rollback requires explicit destructive rollback confirmation".to_string(),
            );
        }
        let actor = actor.trim();
        if actor.is_empty() || actor.len() > 128 {
            return Err("v32 rollback actor must be between 1 and 128 bytes".to_string());
        }
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.rollback_sqlite_v32_to_v31(actor, &now),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.rollback_pg_v32_to_v31_internal(actor, &now),
        }
    }

    pub fn rollback_v31_to_v30(
        &self,
        actor: &str,
        confirm_destructive_rollback: bool,
    ) -> Result<(), String> {
        if !confirm_destructive_rollback {
            return Err(
                "v31 rollback requires explicit destructive rollback confirmation".to_string(),
            );
        }
        let actor = actor.trim();
        if actor.is_empty() || actor.len() > 128 {
            return Err("v31 rollback actor must be between 1 and 128 bytes".to_string());
        }
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.rollback_sqlite_v31_to_v30(actor, &now),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.rollback_pg_v31_to_v30_internal(actor, &now),
        }
    }

    /// Roll back product golden-path task schema only when no product task rows exist.
    pub fn rollback_v30_to_v29(
        &self,
        actor: &str,
        confirm_destructive_rollback: bool,
    ) -> Result<(), String> {
        if !confirm_destructive_rollback {
            return Err(
                "v30 rollback requires explicit destructive rollback confirmation".to_string(),
            );
        }
        let actor = actor.trim();
        if actor.is_empty() || actor.len() > 128 {
            return Err("v30 rollback actor must be between 1 and 128 bytes".to_string());
        }
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.rollback_sqlite_v30_to_v29(actor, &now),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.rollback_pg_v30_to_v29_internal(actor, &now),
        }
    }

    fn rollback_sqlite_v35_to_v34(&self, actor: &str, now: &str) -> Result<(), String> {
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|error| error.to_string())?;
            let current_version: i64 = tx
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .map_err(|error| error.to_string())?;
            if current_version != V35_SCHEMA_VERSION {
                return Err(format!(
                    "v35 rollback requires current schema version 35; found {current_version}"
                ));
            }
            let occupied = occupied_sqlite_tables(&tx, &V35_TABLES)?;
            if !occupied.is_empty() {
                return Err(format!(
                    "v35 rollback blocked: ProductTask preparation receipts exist in {}",
                    occupied.join(", ")
                ));
            }
            tx.execute_batch(
                "DROP INDEX IF EXISTS idx_product_task_workspace_preparations_state;
                 DROP TABLE IF EXISTS product_task_workspace_preparations;",
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES (?1, ?2, 'schema.rollback.v35_to_v34', 'local_product_store', ?3)",
                rusqlite::params![
                    now,
                    actor,
                    serde_json::json!({
                        "from_version": V35_SCHEMA_VERSION,
                        "to_version": V34_SCHEMA_VERSION,
                        "tables": V35_TABLES,
                    })
                    .to_string()
                ],
            )
            .map_err(|error| error.to_string())?;
            tx.pragma_update(None, "user_version", V34_SCHEMA_VERSION)
                .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())?;
            Ok(())
        })
    }

    fn rollback_sqlite_v36_to_v35(&self, actor: &str, now: &str) -> Result<(), String> {
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            let version: i64 = tx
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .map_err(|e| e.to_string())?;
            if version != V36_SCHEMA_VERSION {
                return Err(format!(
                    "v36 rollback requires current schema version 36; found {version}"
                ));
            }
            let count: i64 = tx
                .query_row(
                    "SELECT COUNT(*) FROM managed_acceptance_delegations",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            if count != 0 {
                return Err("v36 rollback blocked: delegated authority rows exist".into());
            }
            tx.execute_batch(
                "DROP INDEX IF EXISTS idx_managed_acceptance_delegations_lease;
                 DROP INDEX IF EXISTS idx_managed_acceptance_delegations_attempt;
                 DROP INDEX IF EXISTS idx_managed_acceptance_delegations_spend;
                 DROP INDEX IF EXISTS idx_managed_acceptance_delegations_status;
                 DROP TABLE IF EXISTS managed_acceptance_delegations;",
            )
            .map_err(|e| e.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES (?1,?2,'schema.rollback.v36_to_v35','local_product_store',?3)",
                params![
                    now,
                    actor,
                    json!({"from_version": 36, "to_version": 35, "tables": V36_TABLES}).to_string()
                ],
            )
            .map_err(|e| e.to_string())?;
            tx.pragma_update(None, "user_version", V35_SCHEMA_VERSION)
                .map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| e.to_string())
        })
    }

    fn rollback_sqlite_v34_to_v33(&self, actor: &str, now: &str) -> Result<(), String> {
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|error| error.to_string())?;
            let current_version: i64 = tx
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .map_err(|error| error.to_string())?;
            if current_version != V34_SCHEMA_VERSION {
                return Err(format!(
                    "v34 rollback requires current schema version 34; found {current_version}"
                ));
            }
            let occupied = occupied_sqlite_tables(&tx, &V34_TABLES)?;
            if !occupied.is_empty() {
                return Err(format!(
                    "v34 rollback blocked: RWE authority exists in {}",
                    occupied.join(", ")
                ));
            }
            tx.execute_batch(
                "DROP TABLE IF EXISTS rwe_task_attempts;
                 DROP TABLE IF EXISTS rwe_runs;
                 DROP TABLE IF EXISTS rwe_run_authorizations;",
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES (?1, ?2, 'schema.rollback.v34_to_v33', 'local_product_store', ?3)",
                rusqlite::params![
                    now,
                    actor,
                    serde_json::json!({
                        "from_version": V34_SCHEMA_VERSION,
                        "to_version": V33_SCHEMA_VERSION,
                        "tables": V34_TABLES,
                    })
                    .to_string()
                ],
            )
            .map_err(|error| error.to_string())?;
            tx.pragma_update(None, "user_version", V33_SCHEMA_VERSION)
                .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())?;
            Ok(())
        })
    }

    fn rollback_sqlite_v33_to_v32(&self, actor: &str, now: &str) -> Result<(), String> {
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|error| error.to_string())?;
            let current_version: i64 = tx
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .map_err(|error| error.to_string())?;
            if current_version != V33_SCHEMA_VERSION {
                return Err(format!(
                    "v33 rollback requires current schema version 33; found {current_version}"
                ));
            }
            let occupied = occupied_sqlite_tables(&tx, &V33_TABLES)?;
            if !occupied.is_empty() {
                return Err(format!(
                    "v33 rollback blocked: managed acceptance spend exists in {}",
                    occupied.join(", ")
                ));
            }
            tx.execute_batch("DROP TABLE IF EXISTS managed_acceptance_spend_authorizations;")
                .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES (?1, ?2, 'schema.rollback.v33_to_v32', 'local_product_store', ?3)",
                rusqlite::params![
                    now,
                    actor,
                    serde_json::json!({
                        "from_version": V33_SCHEMA_VERSION,
                        "to_version": V32_SCHEMA_VERSION,
                        "tables": V33_TABLES,
                    })
                    .to_string()
                ],
            )
            .map_err(|error| error.to_string())?;
            tx.pragma_update(None, "user_version", V32_SCHEMA_VERSION)
                .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())?;
            Ok(())
        })
    }

    fn rollback_sqlite_v32_to_v31(&self, actor: &str, now: &str) -> Result<(), String> {
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|error| error.to_string())?;
            let current_version: i64 = tx
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .map_err(|error| error.to_string())?;
            require_v32_rollback_source(current_version)?;
            let occupied = occupied_sqlite_tables(&tx, &V32_TABLES)?;
            if !occupied.is_empty() {
                return Err(format!(
                    "v32 rollback blocked: managed acceptance authority exists in {}",
                    occupied.join(", ")
                ));
            }
            tx.execute_batch(
                "DROP TABLE managed_acceptance_decision_transition_receipts;
                 DROP TABLE managed_acceptance_attempts;
                 DROP TABLE managed_acceptance_authorizations;
                 DROP TABLE managed_acceptance_decisions;",
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES (?1, ?2, 'schema.rollback.v32_to_v31', 'local_product_store', ?3)",
                rusqlite::params![
                    now,
                    actor,
                    serde_json::json!({
                        "from_version": V32_SCHEMA_VERSION,
                        "to_version": V31_SCHEMA_VERSION,
                        "tables": V32_TABLES,
                    })
                    .to_string()
                ],
            )
            .map_err(|error| error.to_string())?;
            tx.pragma_update(None, "user_version", V31_SCHEMA_VERSION)
                .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())
        })
    }

    fn rollback_sqlite_v31_to_v30(&self, actor: &str, now: &str) -> Result<(), String> {
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|error| error.to_string())?;
            let current_version: i64 = tx
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .map_err(|error| error.to_string())?;
            require_v31_rollback_source(current_version)?;
            let occupied = occupied_sqlite_tables(&tx, &V31_TABLES)?;
            if !occupied.is_empty() {
                return Err(format!(
                    "v31 rollback blocked: authoritative terminal evidence exists in {}",
                    occupied.join(", ")
                ));
            }
            tx.execute_batch("DROP TABLE product_task_terminal_evidence;")
                .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES (?1, ?2, 'schema.rollback.v31_to_v30', 'local_product_store', ?3)",
                rusqlite::params![
                    now,
                    actor,
                    serde_json::json!({
                        "from_version": V31_SCHEMA_VERSION,
                        "to_version": V30_SCHEMA_VERSION,
                        "tables": V31_TABLES,
                    })
                    .to_string()
                ],
            )
            .map_err(|error| error.to_string())?;
            tx.pragma_update(None, "user_version", V30_SCHEMA_VERSION)
                .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())
        })
    }

    /// Roll back PR_READY schema only when no PR_READY authority rows exist.
    pub fn rollback_v29_to_v28(
        &self,
        actor: &str,
        confirm_destructive_rollback: bool,
    ) -> Result<(), String> {
        if !confirm_destructive_rollback {
            return Err(
                "v29 rollback requires explicit destructive rollback confirmation".to_string(),
            );
        }
        let actor = actor.trim();
        if actor.is_empty() || actor.len() > 128 {
            return Err("v29 rollback actor must be between 1 and 128 bytes".to_string());
        }
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.rollback_sqlite_v29_to_v28(actor, &now),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.rollback_pg_v29_to_v28_internal(actor, &now),
        }
    }

    /// Roll back evaluation/archive schema only when no evaluation authority rows exist.
    pub fn rollback_v28_to_v27(
        &self,
        actor: &str,
        confirm_destructive_rollback: bool,
    ) -> Result<(), String> {
        if !confirm_destructive_rollback {
            return Err(
                "v28 rollback requires explicit destructive rollback confirmation".to_string(),
            );
        }
        let actor = actor.trim();
        if actor.is_empty() || actor.len() > 128 {
            return Err("v28 rollback actor must be between 1 and 128 bytes".to_string());
        }
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.rollback_sqlite_v28_to_v27(actor, &now),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.rollback_pg_v28_to_v27_internal(actor, &now),
        }
    }

    /// Roll back the additive harness-evolution evidence schema only when empty.
    pub fn rollback_v27_to_v26(
        &self,
        actor: &str,
        confirm_destructive_rollback: bool,
    ) -> Result<(), String> {
        if !confirm_destructive_rollback {
            return Err(
                "v27 rollback requires explicit destructive rollback confirmation".to_string(),
            );
        }
        let actor = actor.trim();
        if actor.is_empty() || actor.len() > 128 {
            return Err("v27 rollback actor must be between 1 and 128 bytes".to_string());
        }
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.rollback_sqlite_v27_to_v26(actor, &now),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.rollback_pg_v27_to_v26_internal(actor, &now),
        }
    }

    pub fn rollback_v26_to_v25(
        &self,
        actor: &str,
        confirm_destructive_rollback: bool,
    ) -> Result<(), String> {
        if !confirm_destructive_rollback {
            return Err(
                "v26 rollback requires explicit destructive rollback confirmation".to_string(),
            );
        }
        let actor = actor.trim();
        if actor.is_empty() || actor.len() > 128 {
            return Err("v26 rollback actor must be between 1 and 128 bytes".to_string());
        }
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.rollback_sqlite_v26_to_v25(actor, &now),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.rollback_pg_v26_to_v25_internal(actor, &now),
        }
    }

    fn rollback_sqlite_v30_to_v29(&self, actor: &str, now: &str) -> Result<(), String> {
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|error| error.to_string())?;
            let current_version: i64 = tx
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .map_err(|error| error.to_string())?;
            require_v30_rollback_source(current_version)?;
            let occupied = occupied_sqlite_tables(&tx, &V30_TABLES)?;
            if !occupied.is_empty() {
                return Err(format!(
                    "v30 rollback blocked: authoritative product task data exists in {}",
                    occupied.join(", ")
                ));
            }
            tx.execute_batch("DROP TABLE product_tasks;")
                .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES (?1, ?2, 'schema.rollback.v30_to_v29', 'local_product_store', ?3)",
                rusqlite::params![
                    now,
                    actor,
                    serde_json::json!({
                        "from_version": V30_SCHEMA_VERSION,
                        "to_version": V29_SCHEMA_VERSION,
                        "tables": V30_TABLES,
                    })
                    .to_string()
                ],
            )
            .map_err(|error| error.to_string())?;
            tx.pragma_update(None, "user_version", V29_SCHEMA_VERSION)
                .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())
        })
    }

    fn rollback_sqlite_v29_to_v28(&self, actor: &str, now: &str) -> Result<(), String> {
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|error| error.to_string())?;
            let current_version: i64 = tx
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .map_err(|error| error.to_string())?;
            require_v29_rollback_source(current_version)?;
            let occupied = occupied_sqlite_tables(&tx, &V29_TABLES)?;
            if !occupied.is_empty() {
                return Err(format!(
                    "v29 rollback blocked: authoritative harness evolution PR_READY data exists in {}",
                    occupied.join(", ")
                ));
            }
            tx.execute_batch(
                "DROP TABLE harness_evolution_pr_ready_receipts;
                 DROP TABLE harness_evolution_pr_ready_bundles;",
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES (?1, ?2, 'schema.rollback.v29_to_v28', 'local_product_store', ?3)",
                rusqlite::params![
                    now,
                    actor,
                    serde_json::json!({
                        "from_version": V29_SCHEMA_VERSION,
                        "to_version": V28_SCHEMA_VERSION,
                        "tables": V29_TABLES,
                    })
                    .to_string()
                ],
            )
            .map_err(|error| error.to_string())?;
            tx.pragma_update(None, "user_version", V28_SCHEMA_VERSION)
                .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())
        })
    }

    fn rollback_sqlite_v28_to_v27(&self, actor: &str, now: &str) -> Result<(), String> {
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|error| error.to_string())?;
            let current_version: i64 = tx
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .map_err(|error| error.to_string())?;
            require_v28_rollback_source(current_version)?;
            let occupied = occupied_sqlite_tables(&tx, &V28_TABLES)?;
            if !occupied.is_empty() {
                return Err(format!(
                    "v28 rollback blocked: authoritative harness evolution evaluation data exists in {}",
                    occupied.join(", ")
                ));
            }
            tx.execute_batch(
                "DROP TABLE harness_evolution_eval_receipts;
                 DROP TABLE harness_evolution_pareto_archive;
                 DROP TABLE harness_evolution_evaluations;
                 DROP TABLE harness_evolution_sealed_holdouts;",
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES (?1, ?2, 'schema.rollback.v28_to_v27', 'local_product_store', ?3)",
                rusqlite::params![
                    now,
                    actor,
                    serde_json::json!({
                        "from_version": V28_SCHEMA_VERSION,
                        "to_version": V27_SCHEMA_VERSION,
                        "tables": V28_TABLES,
                    })
                    .to_string()
                ],
            )
            .map_err(|error| error.to_string())?;
            tx.pragma_update(None, "user_version", V27_SCHEMA_VERSION)
                .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())
        })
    }

    fn rollback_sqlite_v27_to_v26(&self, actor: &str, now: &str) -> Result<(), String> {
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|error| error.to_string())?;
            let current_version: i64 = tx
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .map_err(|error| error.to_string())?;
            require_v27_rollback_source(current_version)?;
            let occupied = occupied_sqlite_tables(&tx, &V27_TABLES)?;
            if !occupied.is_empty() {
                return Err(format!(
                    "v27 rollback blocked: authoritative harness evolution data exists in {}",
                    occupied.join(", ")
                ));
            }
            tx.execute_batch(
                "DROP TABLE harness_evolution_receipts;
                 DROP TABLE harness_evolution_candidates;
                 DROP TABLE harness_evolution_proposals;
                 DROP TABLE harness_evolution_active_identity;",
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES (?1, ?2, 'schema.rollback.v27_to_v26', 'local_product_store', ?3)",
                rusqlite::params![
                    now,
                    actor,
                    serde_json::json!({
                        "from_version": V27_SCHEMA_VERSION,
                        "to_version": V26_SCHEMA_VERSION,
                        "tables": V27_TABLES,
                    })
                    .to_string()
                ],
            )
            .map_err(|error| error.to_string())?;
            tx.pragma_update(None, "user_version", V26_SCHEMA_VERSION)
                .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())
        })
    }

    fn rollback_sqlite_v26_to_v25(&self, actor: &str, now: &str) -> Result<(), String> {
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|error| error.to_string())?;
            let current_version: i64 = tx
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .map_err(|error| error.to_string())?;
            require_v26_rollback_source(current_version)?;
            let occupied = occupied_sqlite_tables(&tx, &V26_TABLES)?;
            require_empty_v26_tables(&occupied)?;
            tx.execute_batch(
                "DROP TABLE recursive_execution_nodes;
                 DROP TABLE recursive_execution_trees;",
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES (?1, ?2, 'schema.rollback.v26_to_v25', 'local_product_store', ?3)",
                rusqlite::params![now, actor, v26_rollback_audit_details()],
            )
            .map_err(|error| error.to_string())?;
            tx.pragma_update(None, "user_version", V25_SCHEMA_VERSION)
                .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())
        })
    }

    fn rollback_sqlite_v25_to_v24(&self, actor: &str, now: &str) -> Result<(), String> {
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|error| error.to_string())?;
            let current_version: i64 = tx
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .map_err(|error| error.to_string())?;
            require_v25_rollback_source(current_version)?;
            let occupied_bindings: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM durable_memory_versions
                     WHERE embedding_metadata_json IS NOT NULL
                        OR embedding_binding_sha256 IS NOT NULL LIMIT 1)",
                    [],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            let occupied_operations: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM provider_embedding_operations LIMIT 1)",
                    [],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            require_empty_v25_bindings(occupied_bindings || occupied_operations)?;
            tx.execute_batch(
                "DROP TABLE provider_embedding_operations;
                 ALTER TABLE durable_memory_versions DROP COLUMN embedding_binding_sha256;
                 ALTER TABLE durable_memory_versions DROP COLUMN embedding_metadata_json;",
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES (?1, ?2, 'schema.rollback.v25_to_v24', 'local_product_store', ?3)",
                rusqlite::params![now, actor, v25_rollback_audit_details()],
            )
            .map_err(|error| error.to_string())?;
            tx.pragma_update(None, "user_version", V24_SCHEMA_VERSION)
                .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())
        })
    }

    /// Roll back the additive v24 schema to v23 only when no external-runtime
    /// checkpoint or invocation receipt exists. Runtime writers must be stopped.
    pub fn rollback_v24_to_v23(
        &self,
        actor: &str,
        confirm_destructive_rollback: bool,
    ) -> Result<(), String> {
        if !confirm_destructive_rollback {
            return Err(
                "v24 rollback requires explicit destructive rollback confirmation".to_string(),
            );
        }
        let actor = actor.trim();
        if actor.is_empty() || actor.len() > 128 {
            return Err("v24 rollback actor must be between 1 and 128 bytes".to_string());
        }
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.rollback_sqlite_v24_to_v23(actor, &now),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.rollback_pg_v24_to_v23_internal(actor, &now),
        }
    }

    fn rollback_sqlite_v24_to_v23(&self, actor: &str, now: &str) -> Result<(), String> {
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|error| error.to_string())?;
            let current_version: i64 = tx
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .map_err(|error| error.to_string())?;
            require_v24_rollback_source(current_version)?;
            let occupied = occupied_sqlite_tables(&tx, &V24_TABLES)?;
            require_empty_v24_tables(&occupied)?;

            tx.execute_batch(
                "DROP TABLE external_runtime_invocations;
                 DROP TABLE external_runtime_checkpoints;",
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES (?1, ?2, 'schema.rollback.v24_to_v23', 'local_product_store', ?3)",
                rusqlite::params![now, actor, v24_rollback_audit_details()],
            )
            .map_err(|error| error.to_string())?;
            tx.pragma_update(None, "user_version", V23_SCHEMA_VERSION)
                .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())
        })
    }

    fn rollback_sqlite_v23_to_v22(&self, actor: &str, now: &str) -> Result<(), String> {
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|error| error.to_string())?;
            let current_version: i64 = tx
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .map_err(|error| error.to_string())?;
            require_v23_rollback_source(current_version)?;
            let occupied = occupied_sqlite_tables(&tx, &V23_TABLES)?;
            require_empty_v23_tables(&occupied)?;

            tx.execute_batch(
                "DROP TABLE operator_acknowledgements;
                 DROP TABLE replay_producer_bindings;
                 DROP TABLE normalized_usage_observations;
                 DROP TABLE production_jobs;
                 DROP TABLE memory_retrieval_events;
                 DROP TABLE durable_memory_versions;",
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES (?1, ?2, 'schema.rollback.v23_to_v22', 'local_product_store', ?3)",
                rusqlite::params![now, actor, v23_rollback_audit_details()],
            )
            .map_err(|error| error.to_string())?;
            tx.pragma_update(None, "user_version", V22_SCHEMA_VERSION)
                .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())
        })
    }

    fn rollback_sqlite_v22_to_v21(&self, actor: &str, now: &str) -> Result<(), String> {
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|error| error.to_string())?;
            let current_version: i64 = tx
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .map_err(|error| error.to_string())?;
            require_v22_rollback_source(current_version)?;

            let occupied = V22_TABLES
                .iter()
                .filter_map(|table| {
                    let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} LIMIT 1)");
                    match tx.query_row(&sql, [], |row| row.get::<_, bool>(0)) {
                        Ok(true) => Some(Ok((*table).to_string())),
                        Ok(false) => None,
                        Err(error) => Some(Err(error.to_string())),
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            require_empty_v22_tables(&occupied)?;

            tx.execute_batch(
                "DROP INDEX IF EXISTS idx_agent_action_receipts_agent;
                 DROP INDEX IF EXISTS idx_tool_execution_authorizations_status;
                 DROP TABLE agent_action_receipts;
                 DROP TABLE tool_execution_authorizations;
                 DROP TABLE tool_allowlist_profiles;",
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                 VALUES (?1, ?2, 'schema.rollback.v22_to_v21', 'local_product_store', ?3)",
                rusqlite::params![now, actor, v22_rollback_audit_details()],
            )
            .map_err(|error| error.to_string())?;
            tx.pragma_update(None, "user_version", V21_SCHEMA_VERSION)
                .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())
        })
    }

    fn migrate_v14_add_agent_state_and_mailbox(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "
CREATE TABLE IF NOT EXISTS agent_state (
    agent_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    role TEXT NOT NULL,
    capability_profile_json TEXT NOT NULL DEFAULT '[]',
    objective TEXT,
    status TEXT NOT NULL DEFAULT 'idle',
    scratchpad_summary TEXT,
    redaction_filter TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (agent_id, run_id)
);

CREATE TABLE IF NOT EXISTS agent_mailbox (
    message_sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id TEXT NOT NULL UNIQUE,
    correlation_id TEXT,
    from_agent_id TEXT NOT NULL,
    to_agent_id TEXT NOT NULL,
    run_id TEXT,
    node_id TEXT,
    message_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    body TEXT,
    body_summary TEXT,
    redaction_status TEXT NOT NULL DEFAULT 'none',
    created_at TEXT NOT NULL,
    read_at TEXT,
    ack_at TEXT,
    reply_to_message_id TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS idx_agent_mailbox_to ON agent_mailbox(to_agent_id);
CREATE INDEX IF NOT EXISTS idx_agent_mailbox_run ON agent_mailbox(run_id);
CREATE INDEX IF NOT EXISTS idx_agent_mailbox_status ON agent_mailbox(status);
CREATE INDEX IF NOT EXISTS idx_agent_mailbox_correlation ON agent_mailbox(correlation_id);
",
        )
        .map_err(|e| e.to_string())
    }

    fn migrate_v15_add_agent_proposals(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "
CREATE TABLE IF NOT EXISTS agent_proposals (
    proposal_id TEXT NOT NULL,
    correlation_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    parent_node_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    target_agent_id TEXT,
    proposed_node_id TEXT,
    proposed_edge_id TEXT,
    proposal_type TEXT NOT NULL DEFAULT 'child_task',
    objective TEXT NOT NULL,
    context_summary TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (proposal_id)
);
CREATE INDEX IF NOT EXISTS idx_agent_proposals_run ON agent_proposals(run_id);
CREATE INDEX IF NOT EXISTS idx_agent_proposals_correlation ON agent_proposals(correlation_id);
CREATE INDEX IF NOT EXISTS idx_agent_proposals_status ON agent_proposals(status);
CREATE INDEX IF NOT EXISTS idx_agent_proposals_agent ON agent_proposals(agent_id);
",
        )
        .map_err(|e| e.to_string())
    }

    fn migrate_v16_add_native_scorecard_artifacts(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "
CREATE TABLE IF NOT EXISTS native_scorecard_artifacts (
    artifact_sequence INTEGER PRIMARY KEY,
    artifact_id TEXT NOT NULL UNIQUE,
    run_id TEXT NOT NULL,
    dispatch_id TEXT,
    scorecard_schema_version TEXT NOT NULL,
    content_sha256 TEXT NOT NULL,
    read_only INTEGER NOT NULL DEFAULT 1,
    redaction_status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    artifact_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_native_scorecard_artifacts_run ON native_scorecard_artifacts(run_id);
CREATE INDEX IF NOT EXISTS idx_native_scorecard_artifacts_dispatch ON native_scorecard_artifacts(dispatch_id);
CREATE INDEX IF NOT EXISTS idx_native_scorecard_artifacts_created ON native_scorecard_artifacts(created_at);
",
        )
        .map_err(|e| e.to_string())
    }

    fn migrate_v17_add_regression_report_artifacts(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "
CREATE TABLE IF NOT EXISTS regression_report_artifacts (
    artifact_sequence INTEGER PRIMARY KEY,
    artifact_id TEXT NOT NULL UNIQUE,
    artifact_kind TEXT NOT NULL,
    report_schema_version TEXT NOT NULL,
    registry_id TEXT NOT NULL,
    registry_sha256 TEXT NOT NULL,
    scenario_id TEXT,
    content_sha256 TEXT NOT NULL,
    created_at TEXT NOT NULL,
    artifact_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_regression_report_artifacts_registry ON regression_report_artifacts(registry_id, artifact_sequence);
CREATE INDEX IF NOT EXISTS idx_regression_report_artifacts_scenario ON regression_report_artifacts(scenario_id, artifact_sequence);
CREATE INDEX IF NOT EXISTS idx_regression_report_artifacts_created ON regression_report_artifacts(created_at);
",
        )
        .map_err(|e| e.to_string())
    }

    fn migrate_v18_add_budget_evidence_artifacts(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "
CREATE TABLE IF NOT EXISTS budget_evidence_artifacts (
    artifact_sequence INTEGER PRIMARY KEY,
    artifact_id TEXT NOT NULL UNIQUE,
    artifact_kind TEXT NOT NULL,
    evidence_id TEXT NOT NULL,
    evidence_sha256 TEXT NOT NULL,
    created_at TEXT NOT NULL,
    artifact_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_budget_evidence_artifacts_kind ON budget_evidence_artifacts(artifact_kind, artifact_sequence);
CREATE INDEX IF NOT EXISTS idx_budget_evidence_artifacts_created ON budget_evidence_artifacts(created_at);
",
        )
        .map_err(|e| e.to_string())
    }

    fn migrate_v19_add_budget_pause_decisions(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS budget_pause_decisions (
                decision_id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                artifact_id TEXT NOT NULL,
                evidence_sha256 TEXT NOT NULL,
                state TEXT NOT NULL,
                cause TEXT NOT NULL,
                policy_json TEXT NOT NULL,
                actor TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                recovery_reason TEXT,
                UNIQUE(run_id, artifact_id)
            );
            CREATE INDEX IF NOT EXISTS idx_budget_pause_decisions_run ON budget_pause_decisions(run_id, created_at);",
        )
        .map_err(|e| e.to_string())
    }

    fn migrate_v20_add_offline_replay_artifacts(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS offline_replay_artifacts (
                artifact_sequence INTEGER PRIMARY KEY,
                artifact_id TEXT NOT NULL UNIQUE,
                report_schema_version TEXT NOT NULL,
                status TEXT NOT NULL,
                eligibility_content_sha256 TEXT NOT NULL,
                content_sha256 TEXT NOT NULL,
                created_at TEXT NOT NULL,
                artifact_json TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_offline_replay_artifacts_status ON offline_replay_artifacts(status, artifact_sequence);
            CREATE INDEX IF NOT EXISTS idx_offline_replay_artifacts_created ON offline_replay_artifacts(created_at);",
        )
        .map_err(|e| e.to_string())
    }

    fn migrate_v21_add_dispatch_trace_provenance(conn: &Connection) -> Result<(), String> {
        if !column_exists(conn, "dispatch_history", "trace_schema_version")? {
            conn.execute_batch(
                "ALTER TABLE dispatch_history ADD COLUMN trace_schema_version TEXT;",
            )
            .map_err(|e| e.to_string())?;
        }
        if !column_exists(conn, "dispatch_history", "trace_content_sha256")? {
            conn.execute_batch(
                "ALTER TABLE dispatch_history ADD COLUMN trace_content_sha256 TEXT;",
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn migrate_v22_add_agent_action_receipts_and_tool_profiles(
        conn: &Connection,
    ) -> Result<(), String> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS agent_action_receipts (
                run_id TEXT NOT NULL,
                node_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                action_sha256 TEXT NOT NULL CHECK (length(action_sha256) = 64),
                action_type TEXT NOT NULL,
                result_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                PRIMARY KEY (run_id, node_id)
            );
            CREATE INDEX IF NOT EXISTS idx_agent_action_receipts_agent
                ON agent_action_receipts(agent_id, run_id);
            CREATE TABLE IF NOT EXISTS tool_allowlist_profiles (
                profile_id TEXT PRIMARY KEY,
                configured_at TEXT NOT NULL
            );
            INSERT OR IGNORE INTO tool_allowlist_profiles (profile_id, configured_at)
                SELECT profile_id, COALESCE(MIN(created_at), 'migration-v22')
                FROM tool_allowlists GROUP BY profile_id;
            CREATE TABLE IF NOT EXISTS tool_execution_authorizations (
                run_id TEXT NOT NULL,
                node_id TEXT NOT NULL,
                action_sha256 TEXT NOT NULL CHECK (length(action_sha256) = 64),
                tool_name TEXT NOT NULL,
                profile_id TEXT NOT NULL,
                status TEXT NOT NULL CHECK (status IN ('requested', 'approved', 'rejected', 'consumed')),
                requested_approval_id TEXT NOT NULL UNIQUE,
                resolved_by TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (run_id, node_id)
            );
            CREATE INDEX IF NOT EXISTS idx_tool_execution_authorizations_status
                ON tool_execution_authorizations(status, run_id);",
        )
        .map_err(|error| error.to_string())
    }

    fn migrate_v23_add_durable_memory_and_production_jobs(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(schema::V23_DDL)
            .map_err(|error| error.to_string())
    }

    fn migrate_v24_add_external_runtime_state(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(schema::V24_DDL)
            .map_err(|error| error.to_string())
    }

    fn migrate_v25_add_provider_embedding_bindings(conn: &Connection) -> Result<(), String> {
        let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        for column in ["embedding_metadata_json", "embedding_binding_sha256"] {
            let exists = tx
                .prepare("PRAGMA table_info(durable_memory_versions)")
                .and_then(|mut statement| {
                    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
                    rows.collect::<Result<Vec<_>, _>>()
                })
                .map_err(|error| error.to_string())?
                .iter()
                .any(|existing| existing == column);
            if !exists {
                tx.execute_batch(&format!(
                    "ALTER TABLE durable_memory_versions ADD COLUMN {column} TEXT;"
                ))
                .map_err(|error| error.to_string())?;
            }
        }
        let operation_ddl = "CREATE TABLE IF NOT EXISTS provider_embedding_operations (
                operation_id TEXT PRIMARY KEY,
                operation_kind TEXT NOT NULL CHECK (operation_kind IN ('memory_version','retrieval_query')),
                target_memory_id TEXT NOT NULL,
                target_version BIGINT NOT NULL,
                tenant_id TEXT NOT NULL,
                workspace_id TEXT NOT NULL,
                agent_id TEXT,
                run_id TEXT,
                task_id TEXT,
                source_id TEXT NOT NULL,
                source_sha256 TEXT NOT NULL CHECK (length(source_sha256) = 64),
                node_id TEXT,
                query_sha256 TEXT CHECK (query_sha256 IS NULL OR length(query_sha256) = 64),
                request_identity_sha256 TEXT NOT NULL CHECK (length(request_identity_sha256) = 64),
                operation_binding_sha256 TEXT NOT NULL CHECK (length(operation_binding_sha256) = 64),
                content_sha256 TEXT NOT NULL CHECK (length(content_sha256) = 64),
                contract_json TEXT NOT NULL,
                contract_sha256 TEXT NOT NULL CHECK (length(contract_sha256) = 64),
                receipt_sha256 TEXT NOT NULL CHECK (length(receipt_sha256) = 64),
                provider_id TEXT NOT NULL,
                requested_model_id TEXT NOT NULL,
                resolved_model_id TEXT NOT NULL,
                dimensions BIGINT NOT NULL CHECK (dimensions > 0),
                reservation_event_id TEXT NOT NULL,
                send_event_id TEXT,
                outcome_event_id TEXT,
                result_kind TEXT CHECK (result_kind IS NULL OR result_kind IN ('memory_version','retrieval_event')),
                result_id TEXT,
                result_sha256 TEXT CHECK (result_sha256 IS NULL OR length(result_sha256) = 64),
                state TEXT NOT NULL CHECK (state IN ('preflight_reserved','reserved','sending','network_succeeded','succeeded','result_erased','failed_before_send','failed_known_outcome','outcome_unknown','outcome_unknown_acknowledged','retry_authorized')),
                attempt_count BIGINT NOT NULL DEFAULT 1 CHECK (attempt_count BETWEEN 1 AND 4),
                vector_json TEXT,
                metadata_json TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE (target_memory_id, target_version),
                FOREIGN KEY (reservation_event_id) REFERENCES provider_audit_events(event_id),
                FOREIGN KEY (send_event_id) REFERENCES provider_audit_events(event_id),
                FOREIGN KEY (outcome_event_id) REFERENCES provider_audit_events(event_id),
                CHECK ((result_kind IS NULL AND result_id IS NULL AND result_sha256 IS NULL)
                    OR (result_kind IS NOT NULL AND result_id IS NOT NULL AND result_sha256 IS NOT NULL)),
                CHECK ((operation_kind='memory_version' AND node_id IS NULL AND query_sha256 IS NULL)
                    OR (operation_kind='retrieval_query' AND run_id IS NOT NULL AND node_id IS NOT NULL
                        AND query_sha256 IS NOT NULL AND query_sha256=source_sha256))
            );";
        tx.execute_batch(operation_ddl)
            .map_err(|error| error.to_string())?;
        let required_columns = [
            "operation_id",
            "operation_kind",
            "target_memory_id",
            "target_version",
            "tenant_id",
            "workspace_id",
            "agent_id",
            "run_id",
            "task_id",
            "source_id",
            "source_sha256",
            "node_id",
            "query_sha256",
            "request_identity_sha256",
            "operation_binding_sha256",
            "content_sha256",
            "contract_json",
            "contract_sha256",
            "receipt_sha256",
            "provider_id",
            "requested_model_id",
            "resolved_model_id",
            "dimensions",
            "reservation_event_id",
            "send_event_id",
            "outcome_event_id",
            "result_kind",
            "result_id",
            "result_sha256",
            "state",
            "attempt_count",
            "vector_json",
            "metadata_json",
            "created_at",
            "updated_at",
        ];
        let mut missing_columns = Vec::new();
        for column in required_columns {
            if !column_exists(&tx, "provider_embedding_operations", column)? {
                missing_columns.push(column);
            }
        }
        let invalid_schema =
            !missing_columns.is_empty() || !sqlite_v25_operation_schema_valid(&tx)?;
        if invalid_schema {
            let occupied: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM provider_embedding_operations LIMIT 1)",
                    [],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            if occupied {
                return Err(format!(
                    "migration 25 cannot repair an occupied partial operation table; missing or invalid {}",
                    if missing_columns.is_empty() { "constraints/indexes/foreign-keys".to_string() } else { missing_columns.join(",") }
                ));
            }
            tx.execute_batch("DROP TABLE provider_embedding_operations;")
                .map_err(|error| error.to_string())?;
            tx.execute_batch(operation_ddl)
                .map_err(|error| error.to_string())?;
        }
        for column in required_columns {
            if !column_exists(&tx, "provider_embedding_operations", column)? {
                return Err(format!(
                    "migration 25 operation table verification failed: missing {column}"
                ));
            }
        }
        tx.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_provider_embedding_operations_state
             ON provider_embedding_operations(state, updated_at);
             CREATE UNIQUE INDEX IF NOT EXISTS idx_provider_embedding_operations_retrieval_identity
             ON provider_embedding_operations(tenant_id,workspace_id,run_id,node_id,query_sha256,provider_id,
                 requested_model_id,resolved_model_id,dimensions,request_identity_sha256)
             WHERE operation_kind='retrieval_query';",
        )
        .map_err(|error| error.to_string())?;
        tx.execute_batch("PRAGMA user_version = 25")
            .map_err(|error| error.to_string())?;
        if !sqlite_v25_operation_schema_valid(&tx)? {
            return Err("migration 25 operation schema verification failed".to_string());
        }
        let version: i64 = tx
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(|error| error.to_string())?;
        if version != V25_SCHEMA_VERSION {
            return Err("migration 25 schema version verification failed".to_string());
        }
        tx.commit().map_err(|error| error.to_string())
    }

    fn migrate_v26_add_recursive_execution_state(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(schema::V26_DDL)
            .map_err(|error| error.to_string())
    }

    fn migrate_v27_add_harness_evolution_state(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(schema::V27_DDL)
            .map_err(|error| error.to_string())
    }

    fn migrate_v28_add_harness_evolution_eval_state(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(schema::V28_DDL)
            .map_err(|error| error.to_string())
    }

    fn migrate_v29_add_harness_evolution_pr_ready_state(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(schema::V29_DDL)
            .map_err(|error| error.to_string())
    }

    fn migrate_v30_add_product_tasks(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(schema::V30_DDL)
            .map_err(|error| error.to_string())
    }

    fn migrate_v31_add_product_terminal_evidence(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(schema::V31_DDL)
            .map_err(|error| error.to_string())
    }

    fn migrate_v32_add_managed_acceptance(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(schema::V32_DDL)
            .map_err(|error| error.to_string())
    }

    fn migrate_v34_add_rwe_authority(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(schema::V34_DDL)
            .map_err(|error| error.to_string())
    }

    fn migrate_v35_add_product_workspace_preparations(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(schema::V35_DDL)
            .map_err(|error| error.to_string())
    }

    fn migrate_v36_add_delegations(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(schema::V36_DDL)
            .map_err(|error| error.to_string())
    }

    fn migrate_v33_add_managed_acceptance_spend(conn: &Connection) -> Result<(), String> {
        repair_sqlite_v32_transition_schema(conn)?;
        let spend_table_exists =
            sqlite_table_exists(conn, "managed_acceptance_spend_authorizations")?;
        if !spend_table_exists {
            conn.execute_batch(schema::V33_DDL)
                .map_err(|error| error.to_string())?;
        }
        repair_sqlite_v33_spend_schema(conn)?;
        Ok(())
    }
}

fn repair_sqlite_v32_transition_schema(conn: &Connection) -> Result<(), String> {
    if !sqlite_table_exists(conn, "managed_acceptance_decision_transition_receipts")? {
        return Ok(());
    }
    let columns = conn
        .prepare("PRAGMA table_info(managed_acceptance_decision_transition_receipts)")
        .and_then(|mut statement| {
            statement
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(|error| error.to_string())?;
    if !columns.iter().any(|column| column == "sequence") {
        conn.execute("ALTER TABLE managed_acceptance_decision_transition_receipts ADD COLUMN sequence INTEGER NOT NULL DEFAULT 1", [])
            .map_err(|error| error.to_string())?;
    }
    if !columns
        .iter()
        .any(|column| column == "previous_transition_sequence")
    {
        conn.execute("ALTER TABLE managed_acceptance_decision_transition_receipts ADD COLUMN previous_transition_sequence INTEGER", [])
            .map_err(|error| error.to_string())?;
    }
    conn.execute_batch(
        "UPDATE managed_acceptance_decision_transition_receipts
         SET sequence = (SELECT COUNT(*) FROM managed_acceptance_decision_transition_receipts newer
                         WHERE newer.decision_id = managed_acceptance_decision_transition_receipts.decision_id
                           AND (newer.created_at < managed_acceptance_decision_transition_receipts.created_at
                                OR (newer.created_at = managed_acceptance_decision_transition_receipts.created_at
                                    AND newer.transition_receipt_id <= managed_acceptance_decision_transition_receipts.transition_receipt_id)));
         UPDATE managed_acceptance_decision_transition_receipts
         SET previous_transition_sequence = CASE WHEN previous_transition_sha256 IS NULL THEN NULL ELSE sequence - 1 END;
         CREATE UNIQUE INDEX IF NOT EXISTS idx_managed_acceptance_transition_sequence
         ON managed_acceptance_decision_transition_receipts(decision_id, sequence);",
    )
    .map_err(|error| error.to_string())
}

fn sqlite_table_exists(conn: &Connection, table: &str) -> Result<bool, String> {
    conn.query_row(
        "SELECT EXISTS(
             SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1
         )",
        [table],
        |row| row.get::<_, i64>(0),
    )
    .map(|value| value != 0)
    .map_err(|error| error.to_string())
}

fn repair_sqlite_v33_spend_schema(conn: &Connection) -> Result<(), String> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    // Columns may already exist when the full current DDL bootstrapped the
    // database. Keep repairs and legacy upgrades in the same transaction so a
    // failed v33 repair cannot leave a partial marker/schema.
    for (table, col, decl) in [
        (
            "managed_acceptance_attempts",
            "spend_authorization_id",
            "TEXT",
        ),
        ("managed_acceptance_attempts", "lease_token", "TEXT"),
        ("managed_acceptance_attempts", "receipt_sha256", "TEXT"),
    ] {
        if !column_exists(&tx, table, col)? {
            tx.execute(&format!("ALTER TABLE {table} ADD COLUMN {col} {decl}"), [])
                .map_err(|error| error.to_string())?;
        }
    }
    if !column_exists(
        &tx,
        "managed_acceptance_spend_authorizations",
        "logical_authorization_sha256",
    )? {
        tx.execute(
            "ALTER TABLE managed_acceptance_spend_authorizations
             ADD COLUMN logical_authorization_sha256 TEXT",
            [],
        )
        .map_err(|error| error.to_string())?;
    }
    let rows = {
        let mut statement = tx
            .prepare(
                "SELECT spend_authorization_id, body_json, spend_body_sha256,
                        logical_authorization_sha256
                 FROM managed_acceptance_spend_authorizations",
            )
            .map_err(|error| error.to_string())?;
        let mapped = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(|error| error.to_string())?;
        mapped
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
    };
    for (spend_id, raw_body, stored_body_sha, stored_logical) in rows {
        let mut body: Value = serde_json::from_str(&raw_body)
            .map_err(|error| format!("v33 spend {spend_id} body_json is invalid: {error}"))?;
        let original_sha = super::managed_acceptance::sha256_hex(
            super::managed_acceptance::canonical_json(&body)?.as_bytes(),
        );
        if original_sha != stored_body_sha {
            return Err(format!(
                "v33 spend {spend_id} body hash does not match its persisted body"
            ));
        }
        let logical = super::managed_acceptance::stable_spend_authorization_identity(&body)?;
        if let Some(stored) = stored_logical.as_deref() {
            if stored != logical {
                return Err(format!(
                    "v33 spend {spend_id} logical authorization hash is inconsistent"
                ));
            }
        }
        body.as_object_mut()
            .ok_or_else(|| format!("v33 spend {spend_id} body_json must be an object"))?
            .insert(
                "logical_authorization_sha256".to_string(),
                Value::String(logical.clone()),
            );
        let body_sha = super::managed_acceptance::sha256_hex(
            super::managed_acceptance::canonical_json(&body)?.as_bytes(),
        );
        tx.execute(
            "UPDATE managed_acceptance_spend_authorizations
             SET logical_authorization_sha256=?1, body_json=?2, spend_body_sha256=?3
             WHERE spend_authorization_id=?4",
            rusqlite::params![logical, body.to_string(), body_sha, spend_id],
        )
        .map_err(|error| format!("v33 spend {spend_id} backfill failed: {error}"))?;
    }

    let definition: String = tx
        .query_row(
            "SELECT sql FROM sqlite_master
             WHERE type='table' AND name='managed_acceptance_spend_authorizations'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let normalized = definition.to_ascii_lowercase().replace(['\n', '\t'], " ");
    let has_active_identity_check = normalized
        .contains("check (status <> 'active' or logical_authorization_sha256 is not null)");
    if !has_active_identity_check {
        let rebuild_ddl = schema::V33_DDL
            .replace(
                "managed_acceptance_spend_authorizations",
                "managed_acceptance_spend_authorizations_v33_rebuild",
            )
            .replace(
                "idx_managed_acceptance_spend_tenant",
                "idx_managed_acceptance_spend_tenant_v33_rebuild",
            )
            .replace(
                "idx_managed_acceptance_spend_active_logical",
                "idx_managed_acceptance_spend_active_logical_v33_rebuild",
            );
        tx.execute_batch(&rebuild_ddl)
            .map_err(|error| format!("v33 spend rebuild create failed: {error}"))?;
        tx.execute(
            "INSERT INTO managed_acceptance_spend_authorizations_v33_rebuild (
                spend_authorization_id, decision_id, risk_authorization_id, tenant_id,
                principal_kind, principal_id, spend_body_sha256, logical_authorization_sha256,
                risk_authorization_sha256, decision_body_sha256, residual_finding_sha256,
                fixture_only, status, body_json, created_at, updated_at, expires_at,
                consumed_at, consumed_by_attempt_id, revoked_at
             )
             SELECT spend_authorization_id, decision_id, risk_authorization_id, tenant_id,
                    principal_kind, principal_id, spend_body_sha256, logical_authorization_sha256,
                    risk_authorization_sha256, decision_body_sha256, residual_finding_sha256,
                    fixture_only, status, body_json, created_at, updated_at, expires_at,
                    consumed_at, consumed_by_attempt_id, revoked_at
             FROM managed_acceptance_spend_authorizations",
            [],
        )
        .map_err(|error| format!("v33 spend rebuild copy failed: {error}"))?;
        tx.execute_batch(
            "DROP TABLE managed_acceptance_spend_authorizations;
             ALTER TABLE managed_acceptance_spend_authorizations_v33_rebuild
                 RENAME TO managed_acceptance_spend_authorizations;
             DROP INDEX IF EXISTS idx_managed_acceptance_spend_tenant_v33_rebuild;
             DROP INDEX IF EXISTS idx_managed_acceptance_spend_active_logical_v33_rebuild;
             CREATE INDEX IF NOT EXISTS idx_managed_acceptance_spend_tenant
                 ON managed_acceptance_spend_authorizations(tenant_id, status, expires_at);
             CREATE UNIQUE INDEX IF NOT EXISTS idx_managed_acceptance_spend_active_logical
                 ON managed_acceptance_spend_authorizations(tenant_id, logical_authorization_sha256)
                 WHERE status = 'active';",
        )
        .map_err(|error| format!("v33 spend rebuild swap failed: {error}"))?;
    } else {
        tx.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_managed_acceptance_spend_tenant
                 ON managed_acceptance_spend_authorizations(tenant_id, status, expires_at);
             CREATE UNIQUE INDEX IF NOT EXISTS idx_managed_acceptance_spend_active_logical
                 ON managed_acceptance_spend_authorizations(tenant_id, logical_authorization_sha256)
                 WHERE status = 'active';",
        )
        .map_err(|error| format!("v33 spend index repair failed: {error}"))?;
    }
    tx.commit().map_err(|error| error.to_string())
}

fn validate_sqlite_v34_schema(conn: &Connection) -> Result<(), String> {
    validate_sqlite_v33_schema(conn)?;
    for table in V34_TABLES {
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if exists != 1 {
            return Err(format!("SQLite v34 schema missing table {table}"));
        }
    }
    Ok(())
}

fn validate_sqlite_v35_schema(conn: &Connection) -> Result<(), String> {
    validate_sqlite_v34_schema(conn)?;
    for table in V35_TABLES {
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if exists != 1 {
            return Err(format!("SQLite v35 schema missing table {table}"));
        }
    }
    Ok(())
}

fn validate_sqlite_v36_schema(conn: &Connection) -> Result<(), String> {
    validate_sqlite_v35_schema(conn)?;
    for table in V36_TABLES {
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if exists != 1 {
            return Err(format!("SQLite v36 schema missing table {table}"));
        }
    }
    let mut statement = conn
        .prepare("PRAGMA table_info(managed_acceptance_delegations)")
        .map_err(|error| error.to_string())?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?
        .collect::<Result<std::collections::HashSet<_>, _>>()
        .map_err(|error| error.to_string())?;
    for column in V36_COLUMNS {
        if !columns.contains(column) {
            return Err(format!("SQLite v36 schema missing column {column}"));
        }
    }
    for index in V36_INDEXES {
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
                [index],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if exists != 1 {
            return Err(format!("SQLite v36 schema missing index {index}"));
        }
    }
    Ok(())
}

fn validate_sqlite_v33_schema(conn: &Connection) -> Result<(), String> {
    validate_sqlite_v32_schema(conn)?;
    for table in V33_TABLES {
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if exists != 1 {
            return Err(format!("SQLite v33 schema missing table {table}"));
        }
    }
    if !column_exists(
        conn,
        "managed_acceptance_spend_authorizations",
        "logical_authorization_sha256",
    )? {
        return Err("SQLite v33 spend logical authorization identity is missing".to_string());
    }
    let table_definition: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master
             WHERE type='table' AND name='managed_acceptance_spend_authorizations'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let active_identity_constraint_ok = table_definition
        .to_ascii_lowercase()
        .replace(['\n', '\t'], " ")
        .contains("check (status <> 'active' or logical_authorization_sha256 is not null)");
    if !active_identity_constraint_ok {
        return Err(
            "SQLite v33 active spend rows may omit logical authorization identity".to_string(),
        );
    }
    let index = "idx_managed_acceptance_spend_active_logical";
    let properties: Option<(i64, i64)> = conn
        .query_row(
            "SELECT \"unique\", partial FROM pragma_index_list('managed_acceptance_spend_authorizations')
             WHERE name=?1",
            [index],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let columns = conn
        .prepare("SELECT name FROM pragma_index_info(?1) ORDER BY seqno")
        .and_then(|mut statement| {
            statement
                .query_map([index], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(|error| error.to_string())?;
    let definition: Option<String> = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='index' AND name=?1",
            [index],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let predicate_ok = definition.is_some_and(|sql| {
        let normalized = sql.to_ascii_lowercase().replace(['\n', '\t'], " ");
        normalized.contains("where status = 'active'")
            && !normalized.contains("logical_authorization_sha256 is not null")
    });
    let expected_columns = vec![
        "tenant_id".to_string(),
        "logical_authorization_sha256".to_string(),
    ];
    if properties != Some((1, 1)) || columns != expected_columns || !predicate_ok {
        return Err("SQLite v33 active logical spend index is missing or malformed".to_string());
    }
    Ok(())
}

fn validate_sqlite_v32_schema(conn: &Connection) -> Result<(), String> {
    validate_sqlite_v31_schema(conn)?;
    for table in V32_TABLES {
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if exists != 1 {
            return Err(format!("SQLite v32 schema missing table {table}"));
        }
    }
    for (index, expected_columns, predicate) in [
        (
            "idx_managed_acceptance_transition_one_child",
            vec![
                "decision_id".to_string(),
                "previous_transition_sha256".to_string(),
            ],
            "where previous_transition_sha256 is not null",
        ),
        (
            "idx_managed_acceptance_transition_one_genesis",
            vec!["decision_id".to_string()],
            "where previous_transition_sha256 is null",
        ),
    ] {
        let properties: Option<(i64, i64)> = conn
            .query_row(
                "SELECT \"unique\", partial
                 FROM pragma_index_list('managed_acceptance_decision_transition_receipts')
                 WHERE name=?1",
                [index],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let columns = conn
            .prepare("SELECT name FROM pragma_index_info(?1) ORDER BY seqno")
            .and_then(|mut statement| {
                statement
                    .query_map([index], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(|error| error.to_string())?;
        let definition: Option<String> = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='index' AND name=?1",
                [index],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let predicate_ok = definition.is_some_and(|sql| {
            sql.to_ascii_lowercase()
                .replace(['\n', '\t'], " ")
                .contains(predicate)
        });
        if properties != Some((1, 1)) || columns != expected_columns || !predicate_ok {
            return Err(format!(
                "SQLite v32 transition index {index} is missing or malformed"
            ));
        }
    }
    Ok(())
}

fn validate_sqlite_v31_schema(conn: &Connection) -> Result<(), String> {
    validate_sqlite_v30_tables(conn)?;
    for table in V31_TABLES {
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if exists != 1 {
            return Err(format!("SQLite v31 schema missing table {table}"));
        }
    }
    Ok(())
}

fn validate_sqlite_v30_schema(conn: &Connection) -> Result<(), String> {
    validate_sqlite_v30_tables(conn)
}

fn validate_sqlite_v30_tables(conn: &Connection) -> Result<(), String> {
    validate_sqlite_v29_schema(conn)?;
    for table in V30_TABLES {
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if exists != 1 {
            return Err(format!("SQLite v30 schema missing table {table}"));
        }
    }
    Ok(())
}

fn validate_sqlite_v29_schema(conn: &Connection) -> Result<(), String> {
    validate_sqlite_v28_schema(conn)?;
    for table in V29_TABLES {
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if exists != 1 {
            return Err(format!("SQLite v29 schema missing table {table}"));
        }
    }
    Ok(())
}

fn validate_sqlite_v28_schema(conn: &Connection) -> Result<(), String> {
    validate_sqlite_v27_schema(conn)?;
    for table in V28_TABLES {
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if exists != 1 {
            return Err(format!("SQLite v28 schema missing table {table}"));
        }
    }
    for (table, fragment) in [
        (
            "harness_evolution_evaluations",
            "unique(candidate_id, budget_seed, family_id)",
        ),
        (
            "harness_evolution_pareto_archive",
            "archive_id text primary key",
        ),
        (
            "harness_evolution_eval_receipts",
            "receipt_id text primary key",
        ),
    ] {
        let ddl: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        let normalized = ddl.to_ascii_lowercase().replace(['\n', '\t'], " ");
        if !normalized.contains(fragment) {
            return Err(format!("SQLite v28 schema missing constraint on {table}"));
        }
    }
    Ok(())
}

fn validate_sqlite_v27_schema(conn: &Connection) -> Result<(), String> {
    validate_sqlite_v26_schema(conn)?;
    for table in V27_TABLES {
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if exists != 1 {
            return Err(format!("SQLite v27 schema missing table {table}"));
        }
    }
    // Fail closed if primary keys / uniqueness for exactly-once receipts are missing.
    for (table, fragment) in [
        (
            "harness_evolution_candidates",
            "unique(lineage_id, content_hash)",
        ),
        (
            "harness_evolution_proposals",
            "proposal_id text primary key",
        ),
        ("harness_evolution_receipts", "receipt_id text primary key"),
    ] {
        let ddl: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        let normalized = ddl.to_ascii_lowercase().replace(['\n', '\t'], " ");
        if !normalized.contains(fragment) {
            return Err(format!("SQLite v27 schema missing constraint on {table}"));
        }
    }
    Ok(())
}

fn validate_sqlite_v26_schema(conn: &Connection) -> Result<(), String> {
    let required: &[(&str, &[(&str, &str)])] = &[
        (
            "recursive_execution_trees",
            &[
                ("root_run_id", "TEXT"),
                ("workflow_id", "TEXT"),
                ("root_node_id", "TEXT"),
                ("tree_schema_version", "TEXT"),
                ("tree_json", "TEXT"),
                ("version", "BIGINT"),
                ("created_at", "TEXT"),
                ("updated_at", "TEXT"),
            ],
        ),
        (
            "recursive_execution_nodes",
            &[
                ("node_id", "TEXT"),
                ("root_run_id", "TEXT"),
                ("parent_node_id", "TEXT"),
                ("proposal_id", "TEXT"),
                ("depth", "BIGINT"),
                ("objective_fingerprint", "TEXT"),
                ("status", "TEXT"),
                ("version", "BIGINT"),
                ("created_at", "TEXT"),
                ("updated_at", "TEXT"),
            ],
        ),
    ];
    for (table, columns) in required {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(|error| error.to_string())?;
        let actual = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(5)?,
                ))
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        for &(name, type_name) in *columns {
            let Some((_, actual_type, not_null, primary_key)) = actual
                .iter()
                .find(|(actual_name, _, _, _)| actual_name == name)
            else {
                return Err(format!("SQLite v26 schema missing {table}.{name}"));
            };
            if !actual_type.eq_ignore_ascii_case(type_name) {
                return Err(format!(
                    "SQLite v26 schema type mismatch for {table}.{name}"
                ));
            }
            let expected_not_null = !(*table == "recursive_execution_nodes"
                && matches!(name, "parent_node_id" | "proposal_id"));
            let expected_primary_key = match (*table, name) {
                ("recursive_execution_trees" | "recursive_execution_nodes", "root_run_id") => 1,
                ("recursive_execution_nodes", "node_id") => 2,
                _ => 0,
            };
            if expected_primary_key == 0 && (*not_null == 1) != expected_not_null {
                return Err(format!(
                    "SQLite v26 schema nullability mismatch for {table}.{name}"
                ));
            }
            if *primary_key != expected_primary_key {
                return Err(format!(
                    "SQLite v26 schema primary key mismatch for {table}.{name}"
                ));
            }
        }
    }
    for (table, fragments) in [
        ("recursive_execution_trees", ["primary key"].as_slice()),
        (
            "recursive_execution_nodes",
            [
                "primary key(root_run_id, node_id)",
                "unique(root_run_id, objective_fingerprint)",
                "check (depth between 0 and 2)",
                "check (length(objective_fingerprint) = 64)",
            ]
            .as_slice(),
        ),
    ] {
        let ddl: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        let normalized = ddl.to_ascii_lowercase().replace(['\n', '\t'], " ");
        for fragment in fragments {
            if !normalized.contains(fragment) {
                return Err(format!("SQLite v26 schema missing constraint on {table}"));
            }
        }
    }
    for (table, index, expected_columns) in [
        (
            "recursive_execution_trees",
            "idx_recursive_execution_trees_workflow",
            ["workflow_id", "updated_at"].as_slice(),
        ),
        (
            "recursive_execution_nodes",
            "idx_recursive_execution_nodes_root",
            ["root_run_id", "depth", "node_id"].as_slice(),
        ),
        (
            "recursive_execution_nodes",
            "idx_recursive_execution_nodes_parent",
            ["root_run_id", "parent_node_id", "status", "node_id"].as_slice(),
        ),
    ] {
        let properties: Option<(i64, i64)> = conn
            .query_row(
                "SELECT \"unique\", partial FROM pragma_index_list(?1) WHERE name=?2",
                rusqlite::params![table, index],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let columns = conn
            .prepare("SELECT name FROM pragma_index_info(?1) ORDER BY seqno")
            .and_then(|mut statement| {
                statement
                    .query_map([index], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(|error| error.to_string())?;
        if properties != Some((0, 0))
            || columns
                != expected_columns
                    .iter()
                    .map(|value| value.to_string())
                    .collect::<Vec<_>>()
        {
            return Err(format!(
                "SQLite v26 schema missing or malformed index {index}"
            ));
        }
    }
    let foreign_key: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_foreign_key_list('recursive_execution_nodes')
             WHERE \"table\"='recursive_execution_trees' AND \"from\"='root_run_id' AND \"to\"='root_run_id'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if foreign_key != 1 {
        return Err("SQLite v26 schema missing recursive root foreign key".to_string());
    }
    Ok(())
}

fn sqlite_v25_operation_schema_valid(tx: &Transaction<'_>) -> Result<bool, String> {
    let ddl: Option<String> = tx
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='provider_embedding_operations'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some(ddl) = ddl else {
        return Ok(false);
    };
    let normalized = ddl
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<String>();
    for fragment in [
        "operation_idtextprimarykey",
        "unique(target_memory_id,target_version)",
        "check(operation_kindin('memory_version','retrieval_query'))",
        "check(length(source_sha256)=64)",
        "check(query_sha256isnullorlength(query_sha256)=64)",
        "check(length(request_identity_sha256)=64)",
        "check(length(operation_binding_sha256)=64)",
        "check(length(content_sha256)=64)",
        "check(length(contract_sha256)=64)",
        "check(length(receipt_sha256)=64)",
        "check(dimensions>0)",
        "check(result_kindisnullorresult_kindin('memory_version','retrieval_event'))",
        "check(result_sha256isnullorlength(result_sha256)=64)",
        "check(statein('preflight_reserved','reserved','sending','network_succeeded','succeeded','result_erased','failed_before_send','failed_known_outcome','outcome_unknown','outcome_unknown_acknowledged','retry_authorized'))",
        "check(attempt_countbetween1and4)",
        "check((result_kindisnullandresult_idisnullandresult_sha256isnull)or(result_kindisnotnullandresult_idisnotnullandresult_sha256isnotnull))",
        "check((operation_kind='memory_version'andnode_idisnullandquery_sha256isnull)or(operation_kind='retrieval_query'andrun_idisnotnullandnode_idisnotnullandquery_sha256isnotnullandquery_sha256=source_sha256))",
        "foreignkey(reservation_event_id)referencesprovider_audit_events(event_id)",
        "foreignkey(send_event_id)referencesprovider_audit_events(event_id)",
        "foreignkey(outcome_event_id)referencesprovider_audit_events(event_id)",
    ] {
        if !normalized.contains(fragment) {
            return Ok(false);
        }
    }
    let mut not_null = std::collections::BTreeMap::new();
    let mut statement = tx
        .prepare("PRAGMA table_info(provider_embedding_operations)")
        .map_err(|error| error.to_string())?;
    for row in statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, i64>(3)?))
        })
        .map_err(|error| error.to_string())?
    {
        let (name, required) = row.map_err(|error| error.to_string())?;
        not_null.insert(name, required == 1);
    }
    for column in [
        "operation_kind",
        "target_memory_id",
        "target_version",
        "tenant_id",
        "workspace_id",
        "source_id",
        "source_sha256",
        "request_identity_sha256",
        "operation_binding_sha256",
        "content_sha256",
        "contract_json",
        "contract_sha256",
        "receipt_sha256",
        "provider_id",
        "requested_model_id",
        "resolved_model_id",
        "dimensions",
        "reservation_event_id",
        "state",
        "attempt_count",
        "created_at",
        "updated_at",
    ] {
        if not_null.get(column) != Some(&true) {
            return Ok(false);
        }
    }
    let normalized_index = |name: &str| -> Result<Option<String>, String> {
        tx.query_row(
            "SELECT sql FROM sqlite_master WHERE type='index' AND name=?1",
            [name],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map(|sql| {
            sql.flatten().map(|sql| {
                sql.to_ascii_lowercase()
                    .split_whitespace()
                    .collect::<String>()
            })
        })
        .map_err(|error| error.to_string())
    };
    let state_index = normalized_index("idx_provider_embedding_operations_state")?
        .is_some_and(|sql| {
            matches!(
                sql.as_str(),
                "createindexidx_provider_embedding_operations_stateonprovider_embedding_operations(state,updated_at)"
                    | "createindexifnotexistsidx_provider_embedding_operations_stateonprovider_embedding_operations(state,updated_at)"
            )
        });
    let retrieval_index = normalized_index(
        "idx_provider_embedding_operations_retrieval_identity",
    )?
    .is_some_and(|sql| {
        sql.starts_with("createuniqueindex")
            && sql.contains(
                "onprovider_embedding_operations(tenant_id,workspace_id,run_id,node_id,query_sha256,provider_id,requested_model_id,resolved_model_id,dimensions,request_identity_sha256)",
            )
            && sql.ends_with("whereoperation_kind='retrieval_query'")
    });
    let target_unique: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_index_list('provider_embedding_operations')
         WHERE [unique]=1 AND origin='u')",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let foreign_keys: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM pragma_foreign_key_list('provider_embedding_operations')
         WHERE [table]='provider_audit_events' AND [to]='event_id'
           AND [from] IN ('reservation_event_id','send_event_id','outcome_event_id')",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    Ok(state_index && retrieval_index && target_unique && foreign_keys == 3)
}

fn occupied_sqlite_tables(tx: &Transaction<'_>, tables: &[&str]) -> Result<Vec<String>, String> {
    tables
        .iter()
        .filter_map(|table| {
            let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} LIMIT 1)");
            match tx.query_row(&sql, [], |row| row.get::<_, bool>(0)) {
                Ok(true) => Some(Ok((*table).to_string())),
                Ok(false) => None,
                Err(error) => Some(Err(error.to_string())),
            }
        })
        .collect()
}

pub(super) fn require_v23_rollback_source(current_version: i64) -> Result<(), String> {
    if current_version == V23_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(format!(
            "v23 rollback requires current schema version 23; found {current_version}"
        ))
    }
}

pub(super) fn require_v24_rollback_source(current_version: i64) -> Result<(), String> {
    if current_version == V24_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(format!(
            "v24 rollback requires current schema version 24; found {current_version}"
        ))
    }
}

pub(super) fn require_v25_rollback_source(current_version: i64) -> Result<(), String> {
    if current_version == V25_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(format!(
            "v25 rollback requires current schema version 25; found {current_version}"
        ))
    }
}

pub(super) fn require_v26_rollback_source(current_version: i64) -> Result<(), String> {
    if current_version == V26_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(format!(
            "v26 rollback requires current schema version 26; found {current_version}"
        ))
    }
}

pub(super) fn require_v27_rollback_source(current_version: i64) -> Result<(), String> {
    if current_version == V27_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(format!(
            "v27 rollback requires current schema version 27; found {current_version}"
        ))
    }
}

pub(super) fn require_v28_rollback_source(current_version: i64) -> Result<(), String> {
    if current_version == V28_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(format!(
            "v28 rollback requires current schema version 28; found {current_version}"
        ))
    }
}

pub(super) fn require_v29_rollback_source(current_version: i64) -> Result<(), String> {
    if current_version == V29_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(format!(
            "v29 rollback requires current schema version 29; found {current_version}"
        ))
    }
}

pub(super) fn require_v30_rollback_source(current_version: i64) -> Result<(), String> {
    if current_version == V30_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(format!(
            "v30 rollback requires current schema version 30; found {current_version}"
        ))
    }
}

pub(super) fn require_v32_rollback_source(current_version: i64) -> Result<(), String> {
    if current_version == V32_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(format!(
            "v32 rollback requires current schema version 32; found {current_version}"
        ))
    }
}

pub(super) fn require_v31_rollback_source(current_version: i64) -> Result<(), String> {
    if current_version == V31_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(format!(
            "v31 rollback requires current schema version 31; found {current_version}"
        ))
    }
}

pub(super) fn require_empty_v26_tables(occupied: &[String]) -> Result<(), String> {
    if occupied.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "v26 rollback blocked: authoritative recursive execution data exists in {}",
            occupied.join(", ")
        ))
    }
}

pub(super) fn v26_rollback_audit_details() -> &'static str {
    r#"{"from_version":26,"to_version":25,"dropped_empty_tables":["recursive_execution_trees","recursive_execution_nodes"]}"#
}

pub(super) fn require_empty_v25_bindings(occupied: bool) -> Result<(), String> {
    if occupied {
        Err("v25 rollback blocked: authoritative provider embedding bindings exist".to_string())
    } else {
        Ok(())
    }
}

pub(super) fn v25_rollback_audit_details() -> &'static str {
    r#"{"from_version":25,"to_version":24,"dropped_empty_columns":["embedding_metadata_json","embedding_binding_sha256"],"dropped_empty_tables":["provider_embedding_operations"]}"#
}

pub(super) fn require_empty_v24_tables(occupied: &[String]) -> Result<(), String> {
    if occupied.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "v24 rollback blocked: authoritative v24 data exists in {}",
            occupied.join(", ")
        ))
    }
}

pub(super) fn v24_rollback_audit_details() -> &'static str {
    r#"{"from_version":24,"to_version":23,"dropped_empty_tables":["external_runtime_checkpoints","external_runtime_invocations"]}"#
}

pub(super) fn require_empty_v23_tables(occupied: &[String]) -> Result<(), String> {
    if occupied.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "v23 rollback blocked: authoritative v23 data exists in {}",
            occupied.join(", ")
        ))
    }
}

pub(super) fn v23_rollback_audit_details() -> &'static str {
    r#"{"from_version":23,"to_version":22,"dropped_empty_tables":["durable_memory_versions","memory_retrieval_events","production_jobs","normalized_usage_observations","replay_producer_bindings","operator_acknowledgements"]}"#
}

pub(super) fn require_v22_rollback_source(current_version: i64) -> Result<(), String> {
    if current_version == V22_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(format!(
            "v22 rollback requires current schema version 22; found {current_version}"
        ))
    }
}

pub(super) fn require_empty_v22_tables(occupied: &[String]) -> Result<(), String> {
    if occupied.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "v22 rollback blocked: authoritative v22 data exists in {}",
            occupied.join(", ")
        ))
    }
}

pub(super) fn v22_rollback_audit_details() -> &'static str {
    r#"{"from_version":22,"to_version":21,"dropped_empty_tables":["agent_action_receipts","tool_allowlist_profiles","tool_execution_authorizations"]}"#
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn table_exists(store: &LocalProductStore, table: &str) -> bool {
        store
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT EXISTS(
                         SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1
                     )",
                    [table],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())
            })
            .unwrap()
    }

    fn store_at_v25(path: impl AsRef<std::path::Path>) -> LocalProductStore {
        let store = LocalProductStore::new(path).unwrap();
        store.rollback_v36_to_v35("migration-test", true).unwrap();
        store.rollback_v35_to_v34("migration-test", true).unwrap();
        store.rollback_v34_to_v33("migration-test", true).unwrap();
        store.rollback_v33_to_v32("migration-test", true).unwrap();
        store.rollback_v32_to_v31("migration-test", true).unwrap();
        store.rollback_v31_to_v30("migration-test", true).unwrap();
        store.rollback_v30_to_v29("migration-test", true).unwrap();
        store.rollback_v29_to_v28("migration-test", true).unwrap();
        store.rollback_v28_to_v27("migration-test", true).unwrap();
        store.rollback_v27_to_v26("migration-test", true).unwrap();
        store.rollback_v26_to_v25("migration-test", true).unwrap();
        store
    }

    fn store_at_v22(path: impl AsRef<std::path::Path>) -> LocalProductStore {
        let store = store_at_v25(path);
        store.rollback_v25_to_v24("migration-test", true).unwrap();
        store.rollback_v24_to_v23("migration-test", true).unwrap();
        store.rollback_v23_to_v22("migration-test", true).unwrap();
        store
    }

    #[test]
    fn sqlite_v22_rollback_is_atomic_and_can_be_migrated_forward_again() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("rollback.db");
        let store = store_at_v22(&path);
        assert_eq!(store.schema_version().unwrap(), V22_SCHEMA_VERSION);

        store.rollback_v22_to_v21("migration-test", true).unwrap();
        assert_eq!(store.schema_version().unwrap(), V21_SCHEMA_VERSION);
        for table in V22_TABLES {
            assert!(!table_exists(&store, table), "{table} should be removed");
        }
        let rollback_audit = store
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT actor, resource, details_json FROM audit_log
                     WHERE action = 'schema.rollback.v22_to_v21'",
                    [],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .map_err(|error| error.to_string())
            })
            .unwrap();
        assert_eq!(rollback_audit.0, "migration-test");
        assert_eq!(rollback_audit.1, "local_product_store");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&rollback_audit.2).unwrap(),
            serde_json::json!({
                "from_version": 22,
                "to_version": 21,
                "dropped_empty_tables": [
                    "agent_action_receipts",
                    "tool_allowlist_profiles",
                    "tool_execution_authorizations"
                ]
            })
        );

        drop(store);
        let upgraded = LocalProductStore::new(&path).unwrap();
        assert_eq!(upgraded.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
        for table in V22_TABLES {
            assert!(table_exists(&upgraded, table), "{table} should be restored");
        }
        for table in V23_TABLES.iter().chain(V24_TABLES.iter()) {
            assert!(table_exists(&upgraded, table), "{table} should be restored");
        }
    }

    #[test]
    fn sqlite_v22_rollback_refuses_authoritative_rows_without_moving_marker() {
        let dir = tempdir().unwrap();
        let store = store_at_v22(dir.path().join("occupied.db"));
        store
            .configure_tool_allowlist("migration-test", "configured-empty", &[], None)
            .unwrap();
        store
            .with_conn(|conn| {
                conn.execute_batch(
                    "INSERT INTO agent_action_receipts
                     (run_id, node_id, agent_id, action_sha256, action_type, result_json, created_at)
                     VALUES
                     ('occupied-run', 'occupied-agent-node', 'occupied-agent',
                      'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                      'complete', '{}', '2026-07-14T00:00:00Z');
                     INSERT INTO tool_execution_authorizations
                     (run_id, node_id, action_sha256, tool_name, profile_id, status,
                      requested_approval_id, created_at, updated_at)
                     VALUES
                     ('occupied-run', 'occupied-tool-node',
                      'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
                      'echo', 'configured-empty', 'requested', 'occupied-approval',
                      '2026-07-14T00:00:00Z', '2026-07-14T00:00:00Z');",
                )
                .map_err(|error| error.to_string())
            })
            .unwrap();

        let error = store
            .rollback_v22_to_v21("migration-test", true)
            .unwrap_err();
        assert!(error.contains("authoritative v22 data exists"));
        for table in V22_TABLES {
            assert!(error.contains(table), "refusal must identify {table}");
        }
        assert_eq!(store.schema_version().unwrap(), V22_SCHEMA_VERSION);
        for table in V22_TABLES {
            assert!(
                table_exists(&store, table),
                "{table} must remain after refusal"
            );
        }
        assert!(store
            .read_tool_allowlist_policy("configured-empty")
            .unwrap()
            .is_some());
    }

    #[test]
    fn sqlite_v22_rollback_requires_explicit_confirmation() {
        let dir = tempdir().unwrap();
        let store = store_at_v22(dir.path().join("confirmation.db"));
        let error = store
            .rollback_v22_to_v21("migration-test", false)
            .unwrap_err();
        assert!(error.contains("explicit destructive rollback confirmation"));
        assert_eq!(store.schema_version().unwrap(), V22_SCHEMA_VERSION);
    }

    #[test]
    fn sqlite_v22_rollback_audit_failure_rolls_back_tables_and_version_marker() {
        let dir = tempdir().unwrap();
        let store = store_at_v22(dir.path().join("audit-failure.db"));
        store
            .with_conn(|conn| {
                conn.execute_batch(
                    "CREATE TRIGGER reject_v22_rollback_audit
                     BEFORE INSERT ON audit_log
                     WHEN NEW.action = 'schema.rollback.v22_to_v21'
                     BEGIN
                         SELECT RAISE(ABORT, 'injected v22 rollback audit failure');
                     END;",
                )
                .map_err(|error| error.to_string())
            })
            .unwrap();

        let error = store
            .rollback_v22_to_v21("migration-test", true)
            .unwrap_err();
        assert!(
            error.contains("injected v22 rollback audit failure"),
            "unexpected rollback error: {error}"
        );
        assert_eq!(store.schema_version().unwrap(), V22_SCHEMA_VERSION);
        for table in V22_TABLES {
            assert!(
                table_exists(&store, table),
                "{table} must be restored when rollback audit fails"
            );
        }
        let rollback_audits = store
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM audit_log
                     WHERE action = 'schema.rollback.v22_to_v21'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|error| error.to_string())
            })
            .unwrap();
        assert_eq!(rollback_audits, 0);
    }

    #[test]
    fn sqlite_v22_rollback_fails_closed_while_another_writer_holds_the_database() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("concurrent-writer.db");
        let store = store_at_v22(&path);
        let blocker = Connection::open(&path).unwrap();
        blocker.execute_batch("BEGIN IMMEDIATE").unwrap();

        let error = store
            .rollback_v22_to_v21("migration-test", true)
            .unwrap_err();
        assert!(
            error.contains("locked"),
            "unexpected rollback error: {error}"
        );
        assert_eq!(store.schema_version().unwrap(), V22_SCHEMA_VERSION);
        for table in V22_TABLES {
            assert!(
                table_exists(&store, table),
                "{table} must remain after lock conflict"
            );
        }

        blocker.execute_batch("ROLLBACK").unwrap();
    }

    #[test]
    fn sqlite_v23_rollback_refuses_authority_and_can_upgrade_empty_schema() {
        let dir = tempdir().unwrap();
        let occupied_path = dir.path().join("v23-occupied.db");
        let occupied = store_at_v25(&occupied_path);
        occupied
            .rollback_v25_to_v24("migration-test", true)
            .unwrap();
        occupied
            .rollback_v24_to_v23("migration-test", true)
            .unwrap();
        occupied
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO production_jobs
                     (job_key,job_kind,scope_sha256,input_sha256,state,created_at,updated_at)
                     VALUES ('job','budget',?1,?1,'failed','2026-07-14T00:00:00Z','2026-07-14T00:00:00Z')",
                    ["a".repeat(64)],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
        let error = occupied
            .rollback_v23_to_v22("migration-test", true)
            .unwrap_err();
        assert!(error.contains("authoritative v23 data exists in production_jobs"));
        assert_eq!(occupied.schema_version().unwrap(), V23_SCHEMA_VERSION);

        let empty_path = dir.path().join("v23-empty.db");
        let empty = store_at_v25(&empty_path);
        empty.rollback_v25_to_v24("migration-test", true).unwrap();
        empty.rollback_v24_to_v23("migration-test", true).unwrap();
        empty.rollback_v23_to_v22("migration-test", true).unwrap();
        assert_eq!(empty.schema_version().unwrap(), V22_SCHEMA_VERSION);
        for table in V23_TABLES {
            assert!(!table_exists(&empty, table));
        }
        drop(empty);
        let upgraded = LocalProductStore::new(&empty_path).unwrap();
        assert_eq!(upgraded.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
        for table in V23_TABLES {
            assert!(table_exists(&upgraded, table));
        }
        for table in V24_TABLES {
            assert!(table_exists(&upgraded, table));
        }
    }

    #[test]
    fn sqlite_v24_rollback_refuses_authority_and_reapplies_cleanly() {
        let dir = tempdir().unwrap();
        let occupied_path = dir.path().join("v24-occupied.db");
        let occupied = store_at_v25(&occupied_path);
        occupied
            .rollback_v25_to_v24("migration-test", true)
            .unwrap();
        occupied
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO external_runtime_invocations
                     (invocation_id,tenant_id,workspace_id,run_id,node_id,thread_id,
                      idempotency_sha256,checkpoint_id,lease_token,status,created_at,updated_at)
                     VALUES ('inv-1','tenant','workspace','run','node','thread',?1,
                             'checkpoint','lease','failed','2026-07-15T00:00:00Z','2026-07-15T00:00:00Z')",
                    ["a".repeat(64)],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
        let error = occupied
            .rollback_v24_to_v23("migration-test", true)
            .unwrap_err();
        assert!(error.contains("authoritative v24 data exists"));
        assert_eq!(occupied.schema_version().unwrap(), V24_SCHEMA_VERSION);

        let empty_path = dir.path().join("v24-empty.db");
        let empty = store_at_v25(&empty_path);
        empty.rollback_v25_to_v24("migration-test", true).unwrap();
        empty.rollback_v24_to_v23("migration-test", true).unwrap();
        assert_eq!(empty.schema_version().unwrap(), V23_SCHEMA_VERSION);
        for table in V24_TABLES {
            assert!(!table_exists(&empty, table));
        }
        drop(empty);
        let upgraded = LocalProductStore::new(&empty_path).unwrap();
        assert_eq!(upgraded.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
        for table in V24_TABLES {
            assert!(table_exists(&upgraded, table));
        }
    }

    #[test]
    fn sqlite_v25_rollback_refuses_provider_bindings_and_reapplies_cleanly() {
        let dir = tempdir().unwrap();
        let occupied = store_at_v25(dir.path().join("v25-occupied.db"));
        occupied
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO durable_memory_versions
                     (memory_id,version,tenant_id,workspace_id,source_id,source_sha256,
                      conflict_key,state,confidence,content_json,embedding_provenance,
                      embedding_metadata_json,embedding_binding_sha256,record_sha256,
                      created_at,created_by)
                     VALUES ('memory',1,'tenant','workspace','source',?1,'fact','current',1.0,
                             '{}','provider_reported','{}',?1,?1,'2026-07-15T00:00:00Z','test')",
                    ["a".repeat(64)],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
        let error = occupied
            .rollback_v25_to_v24("migration-test", true)
            .unwrap_err();
        assert!(error.contains("provider embedding bindings exist"));
        assert_eq!(occupied.schema_version().unwrap(), V25_SCHEMA_VERSION);

        let path = dir.path().join("v25-empty.db");
        let empty = store_at_v25(&path);
        empty
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO durable_memory_versions
                     (memory_id,version,tenant_id,workspace_id,source_id,source_sha256,
                      conflict_key,state,confidence,content_json,embedding_provenance,
                      record_sha256,created_at,created_by)
                     VALUES ('legacy-memory',1,'tenant','workspace','source',?1,'fact','current',
                             1.0,'{}','unavailable',?1,'2026-07-15T00:00:00Z','test')",
                    ["b".repeat(64)],
                )
                .map(|_| ())
                .map_err(|error| error.to_string())
            })
            .unwrap();
        empty.rollback_v25_to_v24("migration-test", true).unwrap();
        assert_eq!(empty.schema_version().unwrap(), V24_SCHEMA_VERSION);
        drop(empty);
        let upgraded = LocalProductStore::new(&path).unwrap();
        assert_eq!(upgraded.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
        assert_eq!(
            upgraded
                .inspect_durable_memory("legacy-memory")
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn sqlite_v25_migration_is_atomic_and_concurrent_restart_safe() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("v25-atomic.db");
        let store = store_at_v25(&path);
        store.rollback_v25_to_v24("migration-test", true).unwrap();
        drop(store);
        let barrier = std::sync::Barrier::new(2);
        let (left, right) = std::thread::scope(|scope| {
            let left = scope.spawn(|| {
                barrier.wait();
                LocalProductStore::new(&path).map(|store| store.schema_version().unwrap())
            });
            let right = scope.spawn(|| {
                barrier.wait();
                LocalProductStore::new(&path).map(|store| store.schema_version().unwrap())
            });
            (left.join().unwrap(), right.join().unwrap())
        });
        assert_eq!(left.unwrap(), CURRENT_SCHEMA_VERSION);
        assert_eq!(right.unwrap(), CURRENT_SCHEMA_VERSION);

        let repair_path = dir.path().join("v25-empty-partial.db");
        let repair = store_at_v25(&repair_path);
        repair.rollback_v25_to_v24("migration-test", true).unwrap();
        repair
            .with_conn(|conn| {
                conn.execute_batch(
                    "CREATE TABLE provider_embedding_operations (
                        operation_id TEXT PRIMARY KEY,target_memory_id TEXT NOT NULL,target_version BIGINT NOT NULL
                     );",
                )
                .map_err(|error| error.to_string())
            })
            .unwrap();
        repair.run_migrations().unwrap();
        assert_eq!(repair.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
        repair
            .with_conn(|conn| {
                assert!(column_exists(
                    conn,
                    "provider_embedding_operations",
                    "contract_sha256"
                )?);
                assert!(column_exists(
                    conn,
                    "provider_embedding_operations",
                    "receipt_sha256"
                )?);
                Ok(())
            })
            .unwrap();

        let constraint_path = dir.path().join("v25-empty-constraint-partial.db");
        let constraint_repair = LocalProductStore::new(&constraint_path).unwrap();
        constraint_repair
            .with_conn(|conn| {
                conn.execute_batch(
                    "PRAGMA foreign_keys=OFF;
                     ALTER TABLE provider_embedding_operations RENAME TO provider_embedding_operations_valid;
                     CREATE TABLE provider_embedding_operations AS
                         SELECT * FROM provider_embedding_operations_valid WHERE 0;
                     DROP TABLE provider_embedding_operations_valid;
                     PRAGMA user_version=24;
                     PRAGMA foreign_keys=ON;",
                )
                .map_err(|error| error.to_string())
            })
            .unwrap();
        constraint_repair.run_migrations().unwrap();
        assert_eq!(
            constraint_repair.schema_version().unwrap(),
            CURRENT_SCHEMA_VERSION
        );
        constraint_repair
            .with_conn(|conn| {
                let tx = conn
                    .unchecked_transaction()
                    .map_err(|error| error.to_string())?;
                assert!(sqlite_v25_operation_schema_valid(&tx)?);
                tx.rollback().map_err(|error| error.to_string())
            })
            .unwrap();

        let occupied_constraint_path = dir.path().join("v25-occupied-constraint-partial.db");
        let occupied_constraint = LocalProductStore::new(&occupied_constraint_path).unwrap();
        occupied_constraint
            .with_conn(|conn| {
                conn.execute_batch(
                    "PRAGMA foreign_keys=OFF;
                     ALTER TABLE provider_embedding_operations RENAME TO provider_embedding_operations_valid;
                     CREATE TABLE provider_embedding_operations AS
                         SELECT * FROM provider_embedding_operations_valid WHERE 0;
                     DROP TABLE provider_embedding_operations_valid;
                     INSERT INTO provider_embedding_operations (operation_id) VALUES ('occupied-malformed');
                     PRAGMA user_version=24;
                     PRAGMA foreign_keys=ON;",
                )
                .map_err(|error| error.to_string())
            })
            .unwrap();
        let occupied_error = occupied_constraint.run_migrations().unwrap_err();
        assert!(
            occupied_error.contains("occupied partial operation table"),
            "unexpected occupied constraint failure: {occupied_error}"
        );
        assert_eq!(
            occupied_constraint.schema_version().unwrap(),
            V24_SCHEMA_VERSION
        );

        let partial_index_path = dir.path().join("v25-occupied-partial-index.db");
        let partial_index = LocalProductStore::new(&partial_index_path).unwrap();
        partial_index
            .with_conn(|conn| {
                conn.execute_batch(&format!(
                    "INSERT INTO provider_audit_events
                     (event_id,dispatch_id,provider_id,event_type,redaction_status,created_at)
                     VALUES ('paudit-preflight-{hash}','memory-embedding-{hash}','openrouter',
                             'contract_check_reserved','redacted','2026-07-15T00:00:00Z');
                     INSERT INTO provider_embedding_operations
                     (operation_id,operation_kind,target_memory_id,target_version,tenant_id,workspace_id,
                      source_id,source_sha256,request_identity_sha256,operation_binding_sha256,
                      content_sha256,contract_json,contract_sha256,receipt_sha256,provider_id,
                      requested_model_id,resolved_model_id,dimensions,reservation_event_id,state,
                      attempt_count,created_at,updated_at)
                     VALUES ('embedding-operation-{hash}','memory_version','memory',1,'tenant','workspace',
                             'source','{hash}','{hash}','{hash}','{hash}','{{}}','{hash}','{hash}',
                             'openrouter','model:free','model:free',1,'paudit-preflight-{hash}',
                             'preflight_reserved',1,'2026-07-15T00:00:00Z','2026-07-15T00:00:00Z');
                     DROP INDEX idx_provider_embedding_operations_state;
                     CREATE INDEX idx_provider_embedding_operations_state
                       ON provider_embedding_operations(state,updated_at)
                       WHERE state='succeeded';
                     PRAGMA user_version=24;",
                    hash = "a".repeat(64),
                ))
                .map_err(|error| error.to_string())
            })
            .unwrap();
        let partial_index_error = partial_index.run_migrations().unwrap_err();
        assert!(partial_index_error.contains("occupied partial operation table"));
        assert_eq!(partial_index.schema_version().unwrap(), V24_SCHEMA_VERSION);

        let failure_path = dir.path().join("v25-atomic-failure.db");
        let failure = store_at_v25(&failure_path);
        failure.rollback_v25_to_v24("migration-test", true).unwrap();
        failure.with_conn(|conn|conn.execute_batch(
            "CREATE TABLE provider_embedding_operations (
                operation_id TEXT PRIMARY KEY,target_memory_id TEXT NOT NULL,target_version BIGINT NOT NULL
             );
             INSERT INTO provider_embedding_operations VALUES ('partial','memory',1);"
        ).map_err(|error|error.to_string())).unwrap();
        let error = failure.run_migrations().unwrap_err();
        assert!(
            error.contains("occupied partial operation table") || error.contains("no such column"),
            "unexpected partial-v25 failure: {error}"
        );
        assert_eq!(failure.schema_version().unwrap(), V24_SCHEMA_VERSION);
        failure
            .with_conn(|conn| {
                assert!(!column_exists(
                    conn,
                    "durable_memory_versions",
                    "embedding_metadata_json"
                )?);
                assert!(!column_exists(
                    conn,
                    "durable_memory_versions",
                    "embedding_binding_sha256"
                )?);
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn sqlite_v33_existing_marker_repairs_legacy_spend_schema_idempotently() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("v33-legacy-repair.db");
        let store = LocalProductStore::new(&path).unwrap();
        store
            .with_conn(|conn| {
                conn.execute_batch(
                    "DROP TABLE managed_acceptance_spend_authorizations;
                     CREATE TABLE managed_acceptance_spend_authorizations (
                        spend_authorization_id TEXT PRIMARY KEY,
                        decision_id TEXT NOT NULL,
                        risk_authorization_id TEXT NOT NULL,
                        tenant_id TEXT NOT NULL,
                        principal_kind TEXT NOT NULL
                            CHECK (principal_kind IN ('operator_api_key','fixture_principal')),
                        principal_id TEXT NOT NULL,
                        spend_body_sha256 TEXT NOT NULL CHECK (length(spend_body_sha256) = 64),
                        risk_authorization_sha256 TEXT NOT NULL CHECK (length(risk_authorization_sha256) = 64),
                        decision_body_sha256 TEXT NOT NULL CHECK (length(decision_body_sha256) = 64),
                        residual_finding_sha256 TEXT NOT NULL CHECK (length(residual_finding_sha256) = 64),
                        fixture_only INTEGER NOT NULL CHECK (fixture_only IN (0,1)),
                        status TEXT NOT NULL CHECK (status IN ('active','consumed','revoked','expired')),
                        body_json TEXT NOT NULL,
                        created_at TEXT NOT NULL,
                        updated_at TEXT NOT NULL,
                        expires_at TEXT NOT NULL,
                        consumed_at TEXT,
                        consumed_by_attempt_id TEXT,
                        revoked_at TEXT,
                        UNIQUE (tenant_id, spend_body_sha256),
                        FOREIGN KEY(decision_id) REFERENCES managed_acceptance_decisions(decision_id),
                        FOREIGN KEY(risk_authorization_id) REFERENCES managed_acceptance_authorizations(authorization_id)
                     );
                     CREATE INDEX idx_managed_acceptance_spend_tenant
                         ON managed_acceptance_spend_authorizations(tenant_id, status, expires_at);
                     PRAGMA user_version=33;",
                )
                .map_err(|error| error.to_string())?;
                let body = serde_json::json!({
                    "one_use": true,
                    "fixture_only": true,
                });
                let body_sha = super::super::managed_acceptance::sha256_hex(
                    super::super::managed_acceptance::canonical_json(&body)
                        .unwrap()
                        .as_bytes(),
                );
                conn.execute(
                    "INSERT INTO managed_acceptance_decisions (
                        decision_id, tenant_id, decision_body_sha256, residual_finding_sha256,
                        status, principal_kind, principal_id, body_json, created_at, updated_at,
                        expires_at, revoked_at
                     ) VALUES ('legacy-decision','legacy-tenant',?1,?2,'draft_pending_operator',
                               'fixture_principal','legacy-principal','{}',
                               '2026-07-25T00:00:00Z','2026-07-25T00:00:00Z',
                               '2026-07-26T00:00:00Z',NULL)",
                    rusqlite::params!["a".repeat(64), "b".repeat(64)],
                )
                .map_err(|error| error.to_string())?;
                conn.execute(
                    "INSERT INTO managed_acceptance_authorizations (
                        authorization_id, decision_id, tenant_id, principal_kind, principal_id,
                        decision_body_sha256, residual_finding_sha256, authorization_sha256,
                        scope_json, status, mutation_authority, execution_granted, body_json,
                        created_at, updated_at, expires_at, revoked_at
                     ) VALUES ('legacy-risk','legacy-decision','legacy-tenant','fixture_principal',
                               'legacy-principal',?1,?2,?3,'{}','active',
                               'authorization_receipt_only',0,'{}',
                               '2026-07-25T00:00:00Z','2026-07-25T00:00:00Z',
                               '2026-07-26T00:00:00Z',NULL)",
                    rusqlite::params![
                        "a".repeat(64),
                        "b".repeat(64),
                        "c".repeat(64)
                    ],
                )
                .map_err(|error| error.to_string())?;
                conn.execute(
                    "INSERT INTO managed_acceptance_spend_authorizations (
                        spend_authorization_id, decision_id, risk_authorization_id, tenant_id,
                        principal_kind, principal_id, spend_body_sha256, risk_authorization_sha256,
                        decision_body_sha256, residual_finding_sha256, fixture_only, status,
                        body_json, created_at, updated_at, expires_at, consumed_at,
                        consumed_by_attempt_id, revoked_at
                     ) VALUES ('legacy-spend','legacy-decision','legacy-risk','legacy-tenant',
                               'fixture_principal','legacy-principal',?1,?2,?3,?4,1,'active',
                               ?5,'2026-07-25T00:00:00Z','2026-07-25T00:00:00Z',
                               '2026-07-26T00:00:00Z',NULL,NULL,NULL)",
                    rusqlite::params![
                        body_sha,
                        "c".repeat(64),
                        "a".repeat(64),
                        "b".repeat(64),
                        body.to_string()
                    ],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
        drop(store);

        let repaired = LocalProductStore::new(&path).unwrap();
        repaired.with_conn(validate_sqlite_v33_schema).unwrap();
        repaired
            .with_conn(|conn| {
                let (logical, body_json, body_sha): (String, String, String) = conn
                    .query_row(
                        "SELECT logical_authorization_sha256, body_json, spend_body_sha256
                         FROM managed_acceptance_spend_authorizations
                         WHERE spend_authorization_id='legacy-spend'",
                        [],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .map_err(|error| error.to_string())?;
                let body: Value = serde_json::from_str(&body_json).unwrap();
                assert_eq!(
                    logical,
                    super::super::managed_acceptance::stable_spend_authorization_identity(&body)
                        .unwrap()
                );
                assert_eq!(
                    body_sha,
                    super::super::managed_acceptance::sha256_hex(
                        super::super::managed_acceptance::canonical_json(&body)
                            .unwrap()
                            .as_bytes()
                    )
                );
                Ok(())
            })
            .unwrap();
        repaired.run_migrations().unwrap();
        drop(repaired);
        let restarted = LocalProductStore::new(&path).unwrap();
        restarted.with_conn(validate_sqlite_v33_schema).unwrap();
    }

    #[test]
    fn already_versioned_partial_v26_schema_is_rejected_fail_closed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("partial-v26.db");
        {
            let conn = rusqlite::Connection::open(&path).expect("sqlite");
            conn.execute_batch(
                "PRAGMA user_version=26;
                 CREATE TABLE recursive_execution_trees (
                    root_run_id TEXT PRIMARY KEY,
                    workflow_id TEXT NOT NULL
                 );
                 CREATE TABLE recursive_execution_nodes (
                    node_id TEXT NOT NULL,
                    root_run_id TEXT NOT NULL,
                    PRIMARY KEY(root_run_id, node_id)
                 );",
            )
            .expect("partial v26 schema");
        }
        let error = match LocalProductStore::new(&path) {
            Ok(_) => panic!("partial schema must fail closed"),
            Err(error) => error,
        };
        assert!(
            error.contains("SQLite v26 schema missing") || error.contains("no such column"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn already_versioned_v26_rejects_required_null_parent_identity() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("not-null-parent-v26.db");
        {
            let conn = rusqlite::Connection::open(&path).expect("sqlite");
            conn.execute_batch(
                "PRAGMA foreign_keys=ON;
                 PRAGMA user_version=26;
                 CREATE TABLE recursive_execution_trees (
                    root_run_id TEXT PRIMARY KEY,
                    workflow_id TEXT NOT NULL,
                    root_node_id TEXT NOT NULL,
                    tree_schema_version TEXT NOT NULL,
                    tree_json TEXT NOT NULL,
                    version BIGINT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE INDEX idx_recursive_execution_trees_workflow
                    ON recursive_execution_trees(workflow_id, updated_at);
                 CREATE TABLE recursive_execution_nodes (
                    node_id TEXT NOT NULL,
                    root_run_id TEXT NOT NULL,
                    parent_node_id TEXT NOT NULL,
                    proposal_id TEXT NOT NULL,
                    depth BIGINT NOT NULL CHECK (depth BETWEEN 0 AND 2),
                    objective_fingerprint TEXT NOT NULL CHECK (length(objective_fingerprint) = 64),
                    status TEXT NOT NULL,
                    version BIGINT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    PRIMARY KEY(root_run_id, node_id),
                    UNIQUE(root_run_id, objective_fingerprint),
                    FOREIGN KEY(root_run_id) REFERENCES recursive_execution_trees(root_run_id)
                 );
                 CREATE INDEX idx_recursive_execution_nodes_root
                    ON recursive_execution_nodes(root_run_id, depth, node_id);
                 CREATE INDEX idx_recursive_execution_nodes_parent
                    ON recursive_execution_nodes(root_run_id, parent_node_id, status, node_id);",
            )
            .expect("malformed v26 schema");
        }
        let error = match LocalProductStore::new(&path) {
            Ok(_) => panic!("incorrectly non-null parent identity must fail closed"),
            Err(error) => error,
        };
        assert!(
            error.contains("SQLite v26 schema nullability mismatch"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn already_versioned_v26_rejects_malformed_named_indexes() {
        for (suffix, replacement) in [
            (
                "wrong-columns",
                "CREATE INDEX idx_recursive_execution_nodes_parent
                 ON recursive_execution_nodes(status);",
            ),
            (
                "partial",
                "CREATE INDEX idx_recursive_execution_nodes_parent
                 ON recursive_execution_nodes(root_run_id, parent_node_id, status, node_id)
                 WHERE status = 'ready';",
            ),
        ] {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join(format!("malformed-index-{suffix}.db"));
            drop(LocalProductStore::new(&path).expect("valid v26 store"));
            let conn = rusqlite::Connection::open(&path).expect("sqlite");
            conn.execute_batch(&format!(
                "DROP INDEX idx_recursive_execution_nodes_parent; {replacement}"
            ))
            .expect("malform named index");
            drop(conn);
            let error = match LocalProductStore::new(&path) {
                Ok(_) => panic!("malformed named v26 index must fail closed"),
                Err(error) => error,
            };
            assert!(
                error.contains(
                    "SQLite v26 schema missing or malformed index idx_recursive_execution_nodes_parent"
                ),
                "unexpected error: {error}"
            );
        }
    }
}
