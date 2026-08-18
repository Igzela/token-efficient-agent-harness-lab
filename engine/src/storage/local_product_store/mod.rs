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
pub(crate) mod product_tasks;
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

use rusqlite::{params, Connection, OpenFlags};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

#[cfg(feature = "pg")]
use postgres::NoTls;
#[cfg(feature = "pg")]
use r2d2::Pool;
#[cfg(feature = "pg")]
use r2d2_postgres::PostgresConnectionManager;

pub use crate::product_golden_path::ValidatedProductTaskIntake;
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
    build_attempt_authority_manifest, compute_attempt_manifest_sha256,
    confirm_delegated_artifact_output, derive_final_execution_manifest,
    validate_managed_acceptance_role_scopes, AuthenticatedPrincipal, CostAuthority,
    DelegationContract, ManagedCodexLaunchFacts, ManagedCodexSpawnLease, PrincipalKind,
    RiskAcknowledgementRequest, SpendAuthorizationRequest, ALL_MANAGED_ACCEPTANCE_SCOPES,
    BOOTSTRAP_MANAGED_ACCEPTANCE_DELEGATION_SCOPES, DELEGATION_SCHEMA_VERSION,
    MANAGED_OUTPUT_OPERATOR_KEY_SCOPES, MANAGED_REVIEWER_KEY_SCOPES, SCOPE_ATTEMPT_ADMIT,
    SCOPE_DELEGATED_ARTIFACT_CONFIRM, SCOPE_DELEGATED_AUTONOMY, SCOPE_DELEGATED_EXECUTE,
    SCOPE_DELEGATED_MANIFEST_APPROVE, SCOPE_IDENTITY_DELEGATE, SCOPE_REVOKE,
    SCOPE_RISK_ACKNOWLEDGE, SCOPE_SPEND_AUTHORIZE,
};
pub use policy_replay_producer::{
    EvidenceChainPromotionRequest, ReplayProductionProfile, ReplayProductionRequest,
    REPLAY_PRODUCER_SCHEMA_VERSION,
};
pub use provider_audit::{ProviderEmbeddingResolutionAction, ProviderEmbeddingResolutionRequest};
pub(crate) use rwe_authority::validate_rwe_corpus_envelope;
pub use rwe_authority::{
    RweAuthorizationIssueRequest, RweAuthorizationV2IssueRequest, RwePerTaskBudget,
};
pub(crate) use tool_execution_policy::ToolExecutionGate;
pub(crate) use workflow_runs::is_execution_owner_conflict;

pub const LOCAL_PRODUCT_STORE_SCHEMA_VERSION: &str = "local_product_store.v1";
pub const LOCAL_TEAM_EXPORT_SCHEMA_VERSION: &str = "local_team_export.v1";
pub const LOCAL_DASHBOARD_SCHEMA_VERSION: &str = "local_dashboard.v1";

fn utc_now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn sqlite_companion_path(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    value.into()
}

fn sqlite_snapshot_digest(path: &Path) -> Result<String, String> {
    let mut digest = Sha256::new();
    for (label, component) in [
        ("main", path.to_path_buf()),
        ("wal", sqlite_companion_path(path, "-wal")),
        ("shm", sqlite_companion_path(path, "-shm")),
        ("journal", sqlite_companion_path(path, "-journal")),
    ] {
        digest.update(label.as_bytes());
        match std::fs::read(&component) {
            Ok(bytes) => {
                digest.update([1]);
                digest.update((bytes.len() as u64).to_le_bytes());
                digest.update(bytes);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                digest.update([0]);
            }
            Err(error) => {
                return Err(format!(
                    "read-only store snapshot component could not be read: {error}"
                ));
            }
        }
    }
    Ok(hex::encode(digest.finalize()))
}

fn sqlite_read_only_uri(path: &Path) -> String {
    let mut uri = String::from("file:");
    for byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'.' | b'_' | b'~') {
            uri.push(*byte as char);
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri.push_str("?mode=ro&immutable=1");
    uri
}

fn sqlite_companion_non_empty(path: &Path, suffix: &str) -> Result<bool, String> {
    match std::fs::metadata(sqlite_companion_path(path, suffix)) {
        Ok(metadata) => Ok(metadata.len() > 0),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!(
            "read-only store could not inspect its {suffix} companion: {error}"
        )),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
}

fn file_identity(file: &File) -> Result<FileIdentity, String> {
    let metadata = file
        .metadata()
        .map_err(|error| format!("store lock identity is unavailable: {error}"))?;
    Ok(FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

fn path_identity(path: &Path) -> Result<FileIdentity, String> {
    let metadata =
        std::fs::metadata(path).map_err(|_| "store path identity is unavailable".to_string())?;
    Ok(FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

fn ensure_lock_matches_path(path: &Path, file: &File) -> Result<FileIdentity, String> {
    let identity = file_identity(file)?;
    if path_identity(path)? != identity {
        return Err("store path identity changed while acquiring its lock".into());
    }
    Ok(identity)
}

fn lock_exclusive(file: &File) -> Result<(), String> {
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
    if result == -1 {
        return Err(format!(
            "mutable store could not acquire its process lock: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

fn unlock(file: &File) -> Result<(), String> {
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
    if result == -1 {
        return Err(format!(
            "store process lock could not be released: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

fn acquire_read_only_lock(path: &Path) -> Result<File, String> {
    let file = File::open(path)
        .map_err(|error| format!("read-only store could not open lock handle: {error}"))?;
    ensure_lock_matches_path(path, &file)?;
    let shared_result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_SH) };
    if shared_result == -1 {
        return Err(format!(
            "read-only store could not acquire its process snapshot lock: {}",
            std::io::Error::last_os_error()
        ));
    }
    let lock = libc::flock {
        l_type: libc::F_RDLCK as libc::c_short,
        l_whence: libc::SEEK_SET as libc::c_short,
        l_start: 0,
        l_len: 0,
        l_pid: 0,
    };
    let result = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETLK, &lock) };
    if result == -1 {
        return Err(format!(
            "read-only store could not acquire a stable snapshot lock: {}",
            std::io::Error::last_os_error()
        ));
    }
    ensure_lock_matches_path(path, &file)?;
    Ok(file)
}

fn acquire_process_write_lock(
    path: &Path,
    create_if_missing: bool,
    anchor: Option<&File>,
) -> Result<Option<File>, String> {
    if path == Path::new(":memory:") {
        return Ok(None);
    }
    let file = if create_if_missing {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|error| format!("mutable store could not open its process lock: {error}"))?
    } else {
        match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let anchor = anchor.ok_or_else(|| {
                    "mutable store path disappeared before lock acquisition".to_string()
                })?;
                let file = anchor.try_clone().map_err(|error| {
                    format!("mutable store lock anchor is unavailable: {error}")
                })?;
                lock_exclusive(&file)?;
                return Ok(Some(file));
            }
            Err(error) => {
                return Err(format!(
                    "mutable store could not open its process lock: {error}"
                ));
            }
        }
    };
    let expected = anchor.map(file_identity).transpose()?;
    let identity = file_identity(&file)?;
    if expected.is_some_and(|expected| expected != identity) {
        return Err("mutable store path identity changed since it was opened".into());
    }
    lock_exclusive(&file)?;
    ensure_lock_matches_path(path, &file)?;
    Ok(Some(file))
}

pub enum DatabaseConnection {
    Sqlite(SqliteDatabase),
    #[cfg(feature = "pg")]
    Pg(Pool<PostgresConnectionManager<NoTls>>),
}

pub struct SqliteDatabase {
    connection: Mutex<Connection>,
    process_lock: Option<File>,
    process_lock_held: bool,
    _read_only_lock: Option<File>,
    read_only_snapshot_digest: Option<String>,
    read_only_path_identity: Option<FileIdentity>,
}

impl SqliteDatabase {
    fn writable(connection: Connection, process_lock: Option<File>) -> Self {
        Self {
            connection: Mutex::new(connection),
            process_lock_held: process_lock.is_some(),
            process_lock,
            _read_only_lock: None,
            read_only_snapshot_digest: None,
            read_only_path_identity: None,
        }
    }

    fn read_only(
        connection: Connection,
        read_only_lock: File,
        read_only_snapshot_digest: String,
    ) -> Result<Self, String> {
        let read_only_path_identity = file_identity(&read_only_lock)?;
        Ok(Self {
            connection: Mutex::new(connection),
            process_lock: None,
            process_lock_held: false,
            _read_only_lock: Some(read_only_lock),
            read_only_snapshot_digest: Some(read_only_snapshot_digest),
            read_only_path_identity: Some(read_only_path_identity),
        })
    }

    fn release_initialization_lock(&mut self) -> Result<(), String> {
        if !self.process_lock_held {
            return Ok(());
        }
        if let Some(file) = self.process_lock.as_ref() {
            unlock(file)?;
        }
        self.process_lock_held = false;
        Ok(())
    }
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

    /// Open an existing SQLite store without acquiring any mutation path.
    ///
    /// This is intentionally separate from [`Self::new`]: the normal
    /// constructor creates parent directories, enables WAL, applies DDL, and
    /// runs migrations/default configuration. A preflight must not do any of
    /// those things merely by inspecting an operator store.
    pub fn open_existing_read_only(path: impl AsRef<Path>) -> Result<Self, String> {
        Self::open_existing_read_only_inner(path, None)
    }

    #[cfg(test)]
    pub(crate) fn open_existing_read_only_with_encryption(
        path: impl AsRef<Path>,
        encryption_key: Option<&str>,
    ) -> Result<Self, String> {
        Self::open_existing_read_only_inner(path, encryption_key)
    }

    fn open_existing_read_only_inner(
        path: impl AsRef<Path>,
        encryption_key: Option<&str>,
    ) -> Result<Self, String> {
        let requested_path = path.as_ref();
        if !requested_path.is_file() {
            return Err("read-only store path is not an existing file".into());
        }
        let path = requested_path
            .canonicalize()
            .map_err(|error| format!("read-only store path canonicalization failed: {error}"))?;
        let read_only_lock = acquire_read_only_lock(&path)?;
        let initial_snapshot_digest = sqlite_snapshot_digest(&path)?;
        for (label, suffix) in [
            ("WAL", "-wal"),
            ("SHM", "-shm"),
            ("rollback journal", "-journal"),
        ] {
            if sqlite_companion_non_empty(&path, suffix)? {
                return Err(format!(
                    "read-only store refuses a non-empty {label} companion"
                ));
            }
        }
        let uri = sqlite_read_only_uri(&path);
        let conn = Connection::open_with_flags(
            &uri,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        )
        .map_err(|e| format!("read-only store open failed: {e}"))?;
        if let Some(key) = encryption_key {
            conn.execute_batch(&format!("PRAGMA key = '{}';", key.replace('\'', "''")))
                .map_err(|e| format!("read-only store encryption setup failed: {e}"))?;
        }
        conn.query_row("SELECT name FROM sqlite_master LIMIT 1", [], |row| {
            row.get::<_, Option<String>>(0)
        })
        .map_err(|_| "encryption_readiness_unavailable".to_string())?;
        // Connection-local query_only is an additional guard against an
        // accidental mutation through a future read-only caller. It does not
        // create schema, write WAL state, or persist connection settings.
        conn.execute_batch("PRAGMA query_only=ON;")
            .map_err(|e| format!("read-only store guard failed: {e}"))?;
        // Keep one connection-local read transaction for the lifetime of the
        // inspection owner. The immutable URI prevents SQLite from creating
        // WAL/SHM state; the shared lock and component digest still fail
        // closed if another process changes the underlying store.
        conn.execute_batch("BEGIN;")
            .map_err(|e| format!("read-only store snapshot transaction failed: {e}"))?;
        let final_snapshot_digest = sqlite_snapshot_digest(&path)?;
        if initial_snapshot_digest != final_snapshot_digest {
            return Err("read-only store changed while opening its snapshot".into());
        }
        Ok(Self {
            db_path: path,
            db: DatabaseConnection::Sqlite(SqliteDatabase::read_only(
                conn,
                read_only_lock,
                final_snapshot_digest,
            )?),
            clock: Box::new(utc_now),
            encryption_active: encryption_key.is_some(),
            embedding_client: crate::provider::embedding::ProviderEmbeddingClient::default(),
        })
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
        let process_write_lock = acquire_process_write_lock(&path, true, None)?;
        let conn = Connection::open(&path).map_err(|e| e.to_string())?;
        if let Some(lock) = process_write_lock.as_ref() {
            ensure_lock_matches_path(&path, lock)?;
        }
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
        let mut store = Self {
            db_path: path,
            db: DatabaseConnection::Sqlite(SqliteDatabase::writable(conn, process_write_lock)),
            clock: Box::new(clock),
            encryption_active: encryption_key.is_some(),
            embedding_client,
        };
        let initialization = (|| {
            store.ensure_default_config()?;
            store.run_migrations()
        })();
        let release = match &mut store.db {
            DatabaseConnection::Sqlite(database) => database.release_initialization_lock(),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => Ok(()),
        };
        initialization?;
        release?;
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

    pub fn now(&self) -> String {
        (self.clock)()
    }

    /// Store-owned parseable clock. Production uses `utc_now`; fixture clocks
    /// remain non-authoritative. Unparseable clocks fail closed.
    pub fn require_now(&self) -> Result<String, String> {
        let raw = self.now();
        let dt = chrono::DateTime::parse_from_rfc3339(raw.trim())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .map_err(|_| "store clock must be canonical RFC3339/UTC".to_string())?;
        Ok(dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
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

    pub(crate) fn ensure_read_only_snapshot_stable(&self) -> Result<(), String> {
        match &self.db {
            DatabaseConnection::Sqlite(database) => {
                let Some(expected) = database.read_only_snapshot_digest.as_deref() else {
                    return Ok(());
                };
                let expected_path_identity = database
                    .read_only_path_identity
                    .ok_or_else(|| "read-only store path identity is unavailable".to_string())?;
                if path_identity(&self.db_path)? != expected_path_identity {
                    return Err("read-only store path identity changed during inspection".into());
                }
                let current = sqlite_snapshot_digest(&self.db_path)?;
                if current != expected {
                    return Err("read-only store changed during inspection".into());
                }
                Ok(())
            }
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => Ok(()),
        }
    }

    pub(super) fn with_conn<F, R>(&self, f: F) -> Result<R, String>
    where
        F: FnOnce(&Connection) -> Result<R, String>,
    {
        match &self.db {
            DatabaseConnection::Sqlite(conn) => {
                let _process_write_lock = if conn.read_only_snapshot_digest.is_none()
                    && !conn.process_lock_held
                {
                    acquire_process_write_lock(&self.db_path, false, conn.process_lock.as_ref())?
                } else {
                    None
                };
                self.ensure_read_only_snapshot_stable()?;
                let guard = conn.connection.lock().map_err(|e| e.to_string())?;
                let result = f(&guard);
                drop(guard);
                self.ensure_read_only_snapshot_stable()
                    .map(|()| result)
                    .and_then(|result| result)
            }
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => {
                Err("with_conn called on PostgreSQL store; use with_pg_conn".into())
            }
        }
    }

    /// Run one SQLite mutation and its audit append as a single store-owned
    /// transaction. Callers must not expose the connection or perform a
    /// compensating mutation outside this owner.
    pub(super) fn with_sqlite_transaction<F, R>(&self, f: F) -> Result<R, String>
    where
        F: FnOnce(&Connection) -> Result<R, String>,
    {
        match &self.db {
            DatabaseConnection::Sqlite(database)
                if database.read_only_snapshot_digest.is_some() =>
            {
                return Err("read-only store cannot start a mutation transaction".into());
            }
            _ => {}
        }
        self.with_conn(|conn| {
            conn.execute_batch("BEGIN IMMEDIATE")
                .map_err(|error| error.to_string())?;
            match f(conn) {
                Ok(value) => match conn.execute_batch("COMMIT") {
                    Ok(()) => Ok(value),
                    Err(error) => {
                        let _ = conn.execute_batch("ROLLBACK");
                        Err(error.to_string())
                    }
                },
                Err(error) => {
                    conn.execute_batch("ROLLBACK")
                        .map_err(|rollback| format!("{error}; rollback failed: {rollback}"))?;
                    Err(error)
                }
            }
        })
    }

    /// Execute cross-domain operations under a single atomic store-owned transaction.
    ///
    /// The transaction boundary is owned exclusively by `LocalProductStore`.
    /// The closure receives a mutable reference to `StoreTransaction`, from which
    /// borrowed domain views (`WorkflowTx`, `ProductTaskTx`, `ManagedAcceptanceTx`, `RweTx`)
    /// can be accessed. Views have no independent `commit` or `rollback` operations.
    pub fn with_transaction<F, R>(&self, f: F) -> Result<R, String>
    where
        F: for<'a, 'c> FnOnce(&mut StoreTransaction<'a, 'c>) -> Result<R, String>,
    {
        match &self.db {
            DatabaseConnection::Sqlite(conn) => {
                if conn.read_only_snapshot_digest.is_some() {
                    return Err("read-only store cannot start a mutation transaction".into());
                }
                let _process_write_lock = if conn.process_lock_held {
                    None
                } else {
                    acquire_process_write_lock(&self.db_path, false, conn.process_lock.as_ref())?
                };
                let guard = conn.connection.lock().map_err(|e| e.to_string())?;
                guard
                    .execute_batch("BEGIN IMMEDIATE")
                    .map_err(|e| e.to_string())?;
                let outcome = {
                    let mut store_tx = StoreTransaction {
                        backend: BackendTx::Sqlite(&guard),
                        store: self,
                    };
                    f(&mut store_tx)
                };
                match outcome {
                    Ok(val) => {
                        guard.execute_batch("COMMIT").map_err(|e| {
                            let _ = guard.execute_batch("ROLLBACK");
                            e.to_string()
                        })?;
                        Ok(val)
                    }
                    Err(err) => {
                        let _ = guard.execute_batch("ROLLBACK");
                        Err(err)
                    }
                }
            }
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(pool) => {
                let mut client = pool.get().map_err(|e| format!("PG pool get failed: {e}"))?;
                let tx = client.transaction().map_err(|e| e.to_string())?;
                let tx_mutex = Mutex::new(tx);
                let outcome = {
                    let mut store_tx = StoreTransaction {
                        backend: BackendTx::Pg(&tx_mutex),
                        store: self,
                    };
                    f(&mut store_tx)
                };
                let tx = tx_mutex.into_inner().map_err(|e| e.to_string())?;
                match outcome {
                    Ok(val) => {
                        tx.commit().map_err(|e| e.to_string())?;
                        Ok(val)
                    }
                    Err(err) => {
                        let _ = tx.rollback();
                        Err(err)
                    }
                }
            }
        }
    }

    pub fn checkpoint_wal(&self) -> Result<(), String> {
        match &self.db {
            DatabaseConnection::Sqlite(database)
                if database.read_only_snapshot_digest.is_some() =>
            {
                Err("read-only store cannot checkpoint WAL".into())
            }
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
                if connection.read_only_snapshot_digest.is_some() {
                    return Err("read-only store cannot restore a backup".into());
                }
                let _process_write_lock = if connection.process_lock_held {
                    None
                } else {
                    acquire_process_write_lock(
                        &self.db_path,
                        false,
                        connection.process_lock.as_ref(),
                    )?
                };
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

                let mut destination = connection
                    .connection
                    .lock()
                    .map_err(|error| error.to_string())?;
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

/// The underlying active database transaction handle for SQLite or PostgreSQL.
pub enum BackendTx<'a, 'c> {
    Sqlite(&'a Connection),
    #[cfg(feature = "pg")]
    Pg(&'a Mutex<postgres::Transaction<'c>>),
    #[cfg(not(feature = "pg"))]
    #[doc(hidden)]
    _PgPhantom(std::marker::PhantomData<&'c ()>),
}

/// Unified borrowed store transaction wrapper.
///
/// Borrows the active transaction for lifetime `'a`. Does not expose independent
/// commit or rollback methods; commits happen exactly once on `Ok(())` at the
/// boundary of `LocalProductStore::with_transaction`.
pub struct StoreTransaction<'a, 'c> {
    pub(crate) backend: BackendTx<'a, 'c>,
    pub(crate) store: &'a LocalProductStore,
}

impl<'a, 'c> StoreTransaction<'a, 'c> {
    pub fn workflow<'b>(&'b mut self) -> WorkflowTx<'b, 'a, 'c> {
        WorkflowTx { tx: self }
    }

    pub fn product_task<'b>(&'b mut self) -> ProductTaskTx<'b, 'a, 'c> {
        ProductTaskTx { tx: self }
    }

    pub fn managed_acceptance<'b>(&'b mut self) -> ManagedAcceptanceTx<'b, 'a, 'c> {
        ManagedAcceptanceTx { tx: self }
    }

    pub fn rwe<'b>(&'b mut self) -> RweTx<'b, 'a, 'c> {
        RweTx { tx: self }
    }

    pub fn append_audit(
        &mut self,
        actor: &str,
        action: &str,
        resource: &str,
        details: &Value,
    ) -> Result<(), String> {
        match &mut self.backend {
            BackendTx::Sqlite(conn) => {
                append_audit_locked(conn, &self.store.now(), actor, action, resource, details)?;
                Ok(())
            }
            #[cfg(feature = "pg")]
            BackendTx::Pg(tx_mutex) => {
                let mut tx = tx_mutex.lock().map_err(|e| e.to_string())?;
                let now = self.store.now();
                let details_json = details.to_string();
                tx.execute(
                    "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                     VALUES ($1, $2, $3, $4, $5)",
                    &[&now, &actor, &action, &resource, &details_json],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            }
            #[cfg(not(feature = "pg"))]
            BackendTx::_PgPhantom(_) => unreachable!(),
        }
    }

    pub fn store(&self) -> &'a LocalProductStore {
        self.store
    }
}

/// Borrowed transaction view for the Workflow Runtime domain.
pub struct WorkflowTx<'b, 'a, 'c> {
    pub(crate) tx: &'b mut StoreTransaction<'a, 'c>,
}

impl<'b, 'a, 'c> WorkflowTx<'b, 'a, 'c> {
    pub fn append_audit(
        &mut self,
        actor: &str,
        action: &str,
        resource: &str,
        details: &Value,
    ) -> Result<(), String> {
        self.tx.append_audit(actor, action, resource, details)
    }

    pub fn get_plan(&self, plan_id: &str) -> Result<Option<Value>, String> {
        self.tx.store.get_workflow_plan(plan_id)
    }

    pub fn get_run(&self, run_id: &str) -> Result<Option<Value>, String> {
        self.tx.store.get_workflow_run(run_id)
    }
}

/// Borrowed transaction view for the Product Task domain.
pub struct ProductTaskTx<'b, 'a, 'c> {
    pub(crate) tx: &'b mut StoreTransaction<'a, 'c>,
}

impl<'b, 'a, 'c> ProductTaskTx<'b, 'a, 'c> {
    pub fn admit_task(
        &mut self,
        intake: &ValidatedProductTaskIntake,
        actor: &str,
    ) -> Result<Value, String> {
        self.tx.store.admit_product_task(intake, actor)
    }

    pub fn get_task(&self, task_id: &str) -> Result<Option<Value>, String> {
        match &self.tx.backend {
            BackendTx::Sqlite(conn) => LocalProductStore::get_product_task_locked(conn, task_id),
            #[cfg(feature = "pg")]
            BackendTx::Pg(tx_mutex) => {
                let mut tx = tx_mutex.lock().map_err(|e| e.to_string())?;
                let row = tx
                    .query_opt(
                        &format!("{} WHERE task_id = $1", product_tasks::PRODUCT_TASK_SELECT),
                        &[&task_id],
                    )
                    .map_err(|e| e.to_string())?;
                row.map(|r| product_tasks::product_task_row_to_json_pg(&r))
                    .transpose()
            }
            #[cfg(not(feature = "pg"))]
            BackendTx::_PgPhantom(_) => unreachable!(),
        }
    }

    pub fn bind_plan_run(
        &mut self,
        task_id: &str,
        plan_id: &str,
        run_id: &str,
        actor: &str,
    ) -> Result<(), String> {
        let now = self.tx.store.now();
        match &mut self.tx.backend {
            BackendTx::Sqlite(conn) => LocalProductStore::bind_product_task_plan_run_sqlite(
                conn, &now, task_id, plan_id, run_id, actor,
            ),
            #[cfg(feature = "pg")]
            BackendTx::Pg(tx_mutex) => {
                let mut tx = tx_mutex.lock().map_err(|e| e.to_string())?;
                LocalProductStore::bind_product_task_plan_run_pg(
                    &mut tx, &now, task_id, plan_id, run_id, actor,
                )
            }
            #[cfg(not(feature = "pg"))]
            BackendTx::_PgPhantom(_) => unreachable!(),
        }
    }

    pub fn append_audit(
        &mut self,
        actor: &str,
        action: &str,
        resource: &str,
        details: &Value,
    ) -> Result<(), String> {
        self.tx.append_audit(actor, action, resource, details)
    }
}

/// Borrowed transaction view for the Managed Acceptance domain.
pub struct ManagedAcceptanceTx<'b, 'a, 'c> {
    pub(crate) tx: &'b mut StoreTransaction<'a, 'c>,
}

impl<'b, 'a, 'c> ManagedAcceptanceTx<'b, 'a, 'c> {
    pub fn append_audit(
        &mut self,
        actor: &str,
        action: &str,
        resource: &str,
        details: &Value,
    ) -> Result<(), String> {
        self.tx.append_audit(actor, action, resource, details)
    }

    pub fn validate_task_phase(
        &self,
        tenant_id: &str,
        task_id: &str,
        mode: &str,
        rev: &str,
    ) -> Result<Value, String> {
        self.tx
            .store
            .validate_managed_acceptance_product_task_phase(tenant_id, task_id, mode, rev)
    }
}

/// Borrowed transaction view for the Real Workload Evidence domain.
pub struct RweTx<'b, 'a, 'c> {
    pub(crate) tx: &'b mut StoreTransaction<'a, 'c>,
}

impl<'b, 'a, 'c> RweTx<'b, 'a, 'c> {
    pub fn append_audit(
        &mut self,
        actor: &str,
        action: &str,
        resource: &str,
        details: &Value,
    ) -> Result<(), String> {
        self.tx.append_audit(actor, action, resource, details)
    }
}
