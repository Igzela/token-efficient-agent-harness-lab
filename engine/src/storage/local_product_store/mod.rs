mod adaptive_observation;
mod adaptive_policy;
mod agent_profiles;
mod agent_runtime;
mod audit;
mod auto_adjustments;
mod boundaries;
mod config;
mod costs;
mod decisions;
mod dispatch;
mod executor_pool_store;
mod export_import;
pub mod feedback;
mod heartbeat;
mod integrity;
mod keys;
mod migrations;
#[cfg(feature = "pg")]
pub mod pg_backend;
mod plans;
mod policy_proposals;
mod provider_audit;
mod schema;
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
pub use boundaries::local_boundaries;
pub use export_import::{ImportCounts, ImportResult, LOCAL_IMPORT_SCHEMA_VERSION};
pub use integrity::{IntegrityReport, TableIntegrity};

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
        conn.execute_batch(schema::ddl_for(schema::Dialect::Sqlite))
            .map_err(|e| e.to_string())?;
        let store = Self {
            db_path: path,
            db: DatabaseConnection::Sqlite(Mutex::new(conn)),
            clock: Box::new(clock),
            encryption_active: encryption_key.is_some(),
        };
        store.ensure_default_config()?;
        store.run_migrations()?;
        Ok(store)
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

    #[cfg(feature = "pg")]
    pub fn new_postgres(
        pg_url: &str,
        clock: impl Fn() -> String + Send + Sync + 'static,
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
            client
                .batch_execute(schema::ddl_for(schema::Dialect::Postgres))
                .map_err(|e| format!("PG DDL execution failed: {e}"))?;
        }
        let store = Self {
            db_path: PathBuf::from(pg_url),
            db: DatabaseConnection::Pg(pool),
            clock: Box::new(clock),
            encryption_active: false,
        };
        store.ensure_default_config()?;
        store.run_pg_migrations()?;
        Ok(store)
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
