mod adaptive_observation;
mod adaptive_policy;
mod agent_action_receipts;
mod agent_profiles;
mod agent_runtime;
mod audit;
mod auto_adjustments;
mod boundaries;
mod budget_evidence_artifacts;
mod budget_intelligence;
mod budget_pause_decisions;
mod config;
mod costs;
mod decisions;
mod dispatch;
mod durable_memory;
mod executor_pool_store;
mod export_import;
mod external_runtime;
pub mod feedback;
mod harness_evolution;
mod heartbeat;
mod integrity;
mod keys;
mod managed_acceptance;
mod migrations;
mod native_scorecard_artifacts;
mod offline_replay_artifacts;
mod operator_acknowledgements;
mod operator_decision_queue;
#[cfg(feature = "pg")]
pub mod pg_backend;
mod plans;
mod policy_proposals;
mod policy_replay_producer;
mod product_tasks;
mod provider_audit;
mod recursive_execution;
mod regression_report_artifacts;
mod rwe_authority;
mod schema;
mod supervised_patch;
pub use supervised_patch::TargetOutputClaim;

/// Shared workspace content hashing for product golden-path worktree binding.
pub(crate) fn supervised_patch_compute_manifest(
    dir: &std::path::Path,
) -> Result<serde_json::Value, String> {
    supervised_patch::fs_utils::compute_manifest(dir)
}
mod team;
mod tool_execution_policy;
mod tool_policy_management;
mod tool_registry;
mod workflow_runs;

#[cfg(test)]
mod workflow_runs_mutation_tests;

use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

#[cfg(feature = "pg")]
use postgres::NoTls;
#[cfg(feature = "pg")]
use r2d2::Pool;
#[cfg(feature = "pg")]
use r2d2_postgres::PostgresConnectionManager;

pub use crate::read_only_planner::WorkflowPlanIds;
pub use adaptive_observation::{
    AdaptiveObservationInput, AdaptiveObservationSummary, ADAPTIVE_OBSERVATION_SCHEMA_VERSION,
};
pub(crate) use agent_action_receipts::{AgentActionMutation, AgentMutationOp};
pub use boundaries::local_boundaries;
pub use budget_intelligence::{
    NormalizedUsageObservation, BUDGET_PRODUCER_SCHEMA_VERSION, NORMALIZED_USAGE_SCHEMA_VERSION,
};
pub use budget_pause_decisions::{BudgetAutoPausePolicy, BUDGET_AUTO_PAUSE_POLICY_SCHEMA_VERSION};
pub use durable_memory::{
    DurableMemoryCreate, DurableMemoryRevision, MemoryReference, MemoryRetrievalRequest,
    MemoryRetrievalResult, MemoryScope, DURABLE_MEMORY_SCHEMA_VERSION,
    MEMORY_RETRIEVAL_SCHEMA_VERSION,
};
pub use export_import::{ImportCounts, ImportResult, LOCAL_IMPORT_SCHEMA_VERSION};
pub use external_runtime::{
    validate_memory_strategy, ExternalRuntimeInvocationClaim, ExternalRuntimeScope,
    EXTERNAL_RUNTIME_CHECKPOINT_SCHEMA_VERSION, EXTERNAL_RUNTIME_INVOCATION_SCHEMA_VERSION,
    MEMORY_STRATEGIES,
};
pub use integrity::{IntegrityReport, TableIntegrity};
pub use managed_acceptance::{
    build_attempt_authority_manifest, AuthenticatedPrincipal, CostAuthority,
    ManagedCodexLaunchFacts, ManagedCodexSpawnLease, PrincipalKind, RiskAcknowledgementRequest,
    SpendAuthorizationRequest, ALL_MANAGED_ACCEPTANCE_SCOPES, SCOPE_ATTEMPT_ADMIT, SCOPE_REVOKE,
    SCOPE_RISK_ACKNOWLEDGE, SCOPE_SPEND_AUTHORIZE,
};
pub use policy_replay_producer::{
    EvidenceChainPromotionRequest, ReplayProductionProfile, ReplayProductionRequest,
    REPLAY_PRODUCER_SCHEMA_VERSION,
};
pub use provider_audit::{ProviderEmbeddingResolutionAction, ProviderEmbeddingResolutionRequest};
pub(crate) use tool_execution_policy::ToolExecutionGate;
pub(crate) use workflow_runs::is_execution_owner_conflict;

pub const LOCAL_PRODUCT_STORE_SCHEMA_VERSION: &str = "local_product_store.v1";
pub const LOCAL_TEAM_EXPORT_SCHEMA_VERSION: &str = "local_team_export.v1";
pub const LOCAL_DASHBOARD_SCHEMA_VERSION: &str = "local_dashboard.v1";

fn utc_now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

pub enum DatabaseConnection {
    Sqlite(Mutex<Connection>),
    #[cfg(feature = "pg")]
    Pg(Pool<PostgresConnectionManager<NoTls>>),
}

pub struct LocalProductStore {
    db_path: PathBuf,
    db: DatabaseConnection,
    clock: Box<dyn Fn() -> String + Send + Sync>,
    encryption_active: bool,
    pub(super) embedding_client: crate::provider::embedding::ProviderEmbeddingClient,
}

impl LocalProductStore {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, String> {
        let key = std::env::var("ACP_DB_ENCRYPTION_KEY").ok();
        Self::new_with_encryption(path, utc_now, key.as_deref())
    }

    pub fn new_with_clock(
        path: impl AsRef<Path>,
        clock: impl Fn() -> String + Send + Sync + 'static,
    ) -> Result<Self, String> {
        let key = std::env::var("ACP_DB_ENCRYPTION_KEY").ok();
        Self::new_with_encryption(path, clock, key.as_deref())
    }

    pub fn new_with_encryption(
        path: impl AsRef<Path>,
        clock: impl Fn() -> String + Send + Sync + 'static,
        encryption_key: Option<&str>,
    ) -> Result<Self, String> {
        Self::new_with_components(
            path,
            clock,
            encryption_key,
            crate::provider::embedding::ProviderEmbeddingClient::default(),
        )
    }

    fn new_with_components(
        path: impl AsRef<Path>,
        clock: impl Fn() -> String + Send + Sync + 'static,
        encryption_key: Option<&str>,
        embedding_client: crate::provider::embedding::ProviderEmbeddingClient,
    ) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }
        let conn = Connection::open(&path).map_err(|e| e.to_string())?;
        if let Some(key) = encryption_key {
            conn.execute_batch(&format!("PRAGMA key = '{}';", key.replace('\'', "''")))
                .map_err(|e| format!("Failed to set encryption key: {}", e))?;
        }
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .map_err(|e| e.to_string())?;
        // The current full DDL creates the v33 logical-spend index. An
        // existing legacy spend table can predate that column (regardless of
        // its migration marker), so add the nullable compatibility column
        // before replaying the DDL; the v33 repair then backfills, constrains,
        // and validates it.
        let spend_table_exists: bool = conn
            .query_row(
                "SELECT EXISTS(
                     SELECT 1 FROM sqlite_master
                     WHERE type='table' AND name='managed_acceptance_spend_authorizations'
                 )",
                [],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        let logical_column_exists: bool = conn
            .prepare("PRAGMA table_info(managed_acceptance_spend_authorizations)")
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| row.get::<_, String>(1))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .map(|columns: Vec<String>| {
                columns
                    .iter()
                    .any(|column| column == "logical_authorization_sha256")
            })
            .map_err(|e| e.to_string())?;
        if spend_table_exists && !logical_column_exists {
            conn.execute(
                "ALTER TABLE managed_acceptance_spend_authorizations
                 ADD COLUMN logical_authorization_sha256 TEXT",
                [],
            )
            .map_err(|e| format!("legacy v33 logical spend column repair failed: {e}"))?;
        }
        conn.execute_batch(schema::ddl_for(schema::Dialect::Sqlite))
            .map_err(|e| e.to_string())?;
        let store = Self {
            db_path: path,
            db: DatabaseConnection::Sqlite(Mutex::new(conn)),
            clock: Box::new(clock),
            encryption_active: encryption_key.is_some(),
            embedding_client,
        };
        store.ensure_default_config()?;
        store.run_migrations()?;
        Ok(store)
    }

    #[cfg(test)]
    pub(crate) fn new_with_embedding_transport(
        path: impl AsRef<Path>,
        clock: impl Fn() -> String + Send + Sync + 'static,
        transport: std::sync::Arc<dyn crate::provider::transport::HttpTransport>,
    ) -> Result<Self, String> {
        Self::new_with_components(
            path,
            clock,
            None,
            crate::provider::embedding::ProviderEmbeddingClient::new(transport),
        )
    }

    pub fn is_encrypted(&self) -> bool {
        self.encryption_active
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub(super) fn now(&self) -> String {
        (self.clock)()
    }

    pub fn is_postgres(&self) -> bool {
        #[cfg(feature = "pg")]
        {
            matches!(&self.db, DatabaseConnection::Pg(_))
        }
        #[cfg(not(feature = "pg"))]
        {
            let _ = &self.db;
            false
        }
    }

    pub fn is_memory(&self) -> bool {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.db_path == Path::new(":memory:"),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => false,
        }
    }

    pub(super) fn with_conn<F, R>(&self, f: F) -> Result<R, String>
    where
        F: FnOnce(&Connection) -> Result<R, String>,
    {
        match &self.db {
            DatabaseConnection::Sqlite(conn) => {
                let guard = conn.lock().map_err(|e| e.to_string())?;
                f(&guard)
            }
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => {
                Err("with_conn called on PostgreSQL store; use with_pg_conn".into())
            }
        }
    }

    pub fn checkpoint_wal(&self) -> Result<(), String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute_batch("PRAGMA wal_checkpoint(FULL);")
                    .map_err(|e| e.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => Ok(()),
        }
    }

    /// Restore a verified SQLite backup into the connection owned by this
    /// store. Replacing the database path while the connection is open would
    /// leave the process attached to the old inode, so live restore must use
    /// SQLite's online backup API under the store mutex.
    pub(crate) fn restore_verified_sqlite_backup(&self, backup_path: &Path) -> Result<(), String> {
        if self.is_memory() {
            return Err("file-backed local store is required for restore".to_string());
        }
        match &self.db {
            DatabaseConnection::Sqlite(connection) => {
                let source = Connection::open_with_flags(
                    backup_path,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                )
                .map_err(|error| format!("failed to open verified backup: {error}"))?;
                let source_integrity: String = source
                    .query_row("PRAGMA integrity_check", [], |row| row.get(0))
                    .map_err(|error| format!("failed to check verified backup: {error}"))?;
                if source_integrity != "ok" {
                    return Err(format!(
                        "verified backup integrity changed before restore: {source_integrity}"
                    ));
                }

                let mut destination = connection.lock().map_err(|error| error.to_string())?;
                let backup = rusqlite::backup::Backup::new(&source, &mut destination)
                    .map_err(|error| format!("failed to initialize live restore: {error}"))?;
                backup
                    .run_to_completion(64, Duration::from_millis(5), None)
                    .map_err(|error| format!("live restore failed: {error}"))?;
                drop(backup);
                destination
                    .execute_batch("PRAGMA foreign_keys=ON; PRAGMA wal_checkpoint(TRUNCATE);")
                    .map_err(|error| format!("failed to finalize live restore: {error}"))?;
                let restored_integrity: String = destination
                    .query_row("PRAGMA integrity_check", [], |row| row.get(0))
                    .map_err(|error| format!("failed to verify restored database: {error}"))?;
                if restored_integrity != "ok" {
                    return Err(format!(
                        "restored database integrity check failed: {restored_integrity}"
                    ));
                }
                Ok(())
            }
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => Err(
                "PostgreSQL restore requires the external PostgreSQL recovery owner".to_string(),
            ),
        }
    }

    #[cfg(feature = "pg")]
    pub fn new_postgres(
        pg_url: &str,
        clock: impl Fn() -> String + Send + Sync + 'static,
    ) -> Result<Self, String> {
        Self::new_postgres_with_components(
            pg_url,
            clock,
            crate::provider::embedding::ProviderEmbeddingClient::default(),
        )
    }

    #[cfg(feature = "pg")]
    fn new_postgres_with_components(
        pg_url: &str,
        clock: impl Fn() -> String + Send + Sync + 'static,
        embedding_client: crate::provider::embedding::ProviderEmbeddingClient,
    ) -> Result<Self, String> {
        let manager = PostgresConnectionManager::new(
            pg_url.parse().map_err(|e| format!("invalid PG URL: {e}"))?,
            NoTls,
        );
        let pool = Pool::builder()
            .max_size(5)
            .build(manager)
            .map_err(|e| format!("r2d2 pool creation failed: {e}"))?;
        {
            let mut client = pool.get().map_err(|e| format!("PG pool get failed: {e}"))?;
            let spend_table_exists: bool = client
                .query_one(
                    "SELECT to_regclass('managed_acceptance_spend_authorizations') IS NOT NULL",
                    &[],
                )
                .map_err(|e| format!("PG legacy v33 table probe failed: {e}"))?
                .get(0);
            let logical_column_exists: bool = client
                .query_one(
                    "SELECT EXISTS(
                         SELECT 1 FROM information_schema.columns
                         WHERE table_schema=current_schema()
                           AND table_name='managed_acceptance_spend_authorizations'
                           AND column_name='logical_authorization_sha256'
                     )",
                    &[],
                )
                .map_err(|e| format!("PG legacy v33 column probe failed: {e}"))?
                .get(0);
            if spend_table_exists && !logical_column_exists {
                client
                    .batch_execute(
                        "ALTER TABLE managed_acceptance_spend_authorizations
                         ADD COLUMN logical_authorization_sha256 TEXT",
                    )
                    .map_err(|e| {
                        format!("PG legacy v33 logical spend column repair failed: {e}")
                    })?;
            }
            client
                .batch_execute(schema::ddl_for(schema::Dialect::Postgres))
                .map_err(|e| format!("PG DDL execution failed: {e}"))?;
        }
        let store = Self {
            db_path: PathBuf::from(pg_url),
            db: DatabaseConnection::Pg(pool),
            clock: Box::new(clock),
            encryption_active: false,
            embedding_client,
        };
        store.ensure_default_config()?;
        store.run_pg_migrations()?;
        Ok(store)
    }

    #[cfg(feature = "pg-tests")]
    #[doc(hidden)]
    pub fn new_postgres_with_embedding_transport_for_test(
        pg_url: &str,
        clock: impl Fn() -> String + Send + Sync + 'static,
        transport: std::sync::Arc<dyn crate::provider::transport::HttpTransport>,
    ) -> Result<Self, String> {
        Self::new_postgres_with_components(
            pg_url,
            clock,
            crate::provider::embedding::ProviderEmbeddingClient::new(transport),
        )
    }

    #[cfg(feature = "pg")]
    pub(super) fn with_pg_conn<F, R>(&self, f: F) -> Result<R, String>
    where
        F: FnOnce(&mut postgres::Client) -> Result<R, String>,
    {
        match &self.db {
            DatabaseConnection::Pg(pool) => {
                let mut client = pool.get().map_err(|e| format!("PG pool get failed: {e}"))?;
                f(&mut client)
            }
            _ => Err("with_pg_conn called on SQLite store".into()),
        }
    }

    #[cfg(feature = "pg-tests")]
    pub fn inject_pg_config_transaction_failure_for_test(
        &self,
        key: &str,
        value: &Value,
    ) -> Result<(), String> {
        if !key.starts_with("pe6-drill-") {
            return Err("PE-6 PostgreSQL fault keys must use the disposable prefix".into());
        }
        self.with_pg_conn(|client| {
            let mut transaction = client.transaction().map_err(|error| error.to_string())?;
            let now = self.now();
            let value_json = value.to_string();
            transaction
                .execute(
                    "INSERT INTO local_config (key, value_json, updated_at, updated_by)
                     VALUES ($1, $2, $3, $4)
                     ON CONFLICT(key) DO UPDATE SET
                        value_json = excluded.value_json,
                        updated_at = excluded.updated_at,
                        updated_by = excluded.updated_by",
                    &[&key, &value_json, &now, &"pe6-test"],
                )
                .map_err(|error| error.to_string())?;
            Err("PE-6 injected interruption before audit and commit".into())
        })
    }

    #[cfg(feature = "pg-tests")]
    pub fn cleanup_pg_fault_drill_for_test(&self, key: &str) -> Result<(), String> {
        if !key.starts_with("pe6-drill-") {
            return Err("PE-6 PostgreSQL cleanup key is outside the disposable prefix".into());
        }
        self.with_pg_conn(|client| {
            let mut transaction = client.transaction().map_err(|error| error.to_string())?;
            transaction
                .execute("DELETE FROM audit_log WHERE resource = $1", &[&key])
                .map_err(|error| error.to_string())?;
            transaction
                .execute("DELETE FROM local_config WHERE key = $1", &[&key])
                .map_err(|error| error.to_string())?;
            transaction.commit().map_err(|error| error.to_string())
        })
    }

    #[cfg(feature = "pg")]
    fn run_pg_migrations(&self) -> Result<(), String> {
        self.run_pg_migrations_internal()
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
        receipt_visibility: ProviderEmbeddingReceiptVisibility,
    ) -> Result<Value, String> {
        let dispatches = self.list_dispatches(limit)?;
        let team = self.team_snapshot()?;
        let config = self.config_snapshot()?;
        let costs = self.cost_summary()?;
        let counts = self.stats()?;
        let provider_embedding_receipts =
            self.authorized_provider_embedding_receipt_evidence(limit, receipt_visibility)?;
        Ok(json!({
            "schema_version": LOCAL_DASHBOARD_SCHEMA_VERSION,
            "status": "ready",
            "counts": counts,
            "dispatches": dispatches,
            "team": team,
            "config": config,
            "costs": costs,
            "provider_embedding_receipts": provider_embedding_receipts,
            "boundaries": local_boundaries(executor_type, provider_enabled),
        }))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderEmbeddingReceiptVisibility {
    TenantOperator { tenant_id: String },
    Hidden,
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
