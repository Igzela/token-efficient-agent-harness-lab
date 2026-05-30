use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const LOCAL_PRODUCT_STORE_SCHEMA_VERSION: &str = "local_product_store.v1";
pub const LOCAL_TEAM_EXPORT_SCHEMA_VERSION: &str = "local_team_export.v1";
pub const LOCAL_DASHBOARD_SCHEMA_VERSION: &str = "local_dashboard.v1";
pub const LOCAL_IMPORT_SCHEMA_VERSION: &str = "local_team_export.v1";

fn utc_now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

const CURRENT_SCHEMA_VERSION: i64 = 1;

struct Migration {
    version: i64,
    description: &'static str,
}

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    description: "add last_used_at and expires_at to api_key_metadata",
}];

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

    fn now(&self) -> String {
        (self.clock)()
    }

    pub fn is_memory(&self) -> bool {
        self.db_path == Path::new(":memory:")
    }

    fn with_conn<F, R>(&self, f: F) -> Result<R, String>
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

    fn run_migrations(&self) -> Result<(), String> {
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
                    _ => return Err(format!("unknown migration version: {}", migration.version)),
                }
                conn.execute_batch(&format!("PRAGMA user_version = {}", migration.version))
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        })
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

    pub fn check_integrity(&self) -> Result<IntegrityReport, String> {
        self.with_conn(|conn| {
            let status: String = conn
                .query_row("PRAGMA integrity_check", [], |row| row.get(0))
                .map_err(|e| e.to_string())?;

            let tables = [
                "dispatch_history",
                "local_config",
                "team_members",
                "api_key_metadata",
                "audit_log",
                "provider_audit_events",
            ];
            let mut table_reports = Vec::new();
            for table in &tables {
                let row_count = count_table(conn, table)?;
                table_reports.push(TableIntegrity {
                    name: table.to_string(),
                    row_count,
                    status: if status == "ok" {
                        "ok".to_string()
                    } else {
                        "corrupt".to_string()
                    },
                });
            }

            Ok(IntegrityReport {
                status,
                tables: table_reports,
                schema_version: conn
                    .query_row("PRAGMA user_version", [], |row| row.get(0))
                    .unwrap_or(0),
            })
        })
    }

    pub fn import_snapshot(&self, snapshot: &Value) -> Result<ImportResult, String> {
        let schema_version = snapshot
            .get("schema_version")
            .and_then(Value::as_str)
            .unwrap_or("");
        if schema_version != LOCAL_IMPORT_SCHEMA_VERSION {
            return Err(format!(
                "unsupported schema version: {schema_version} (expected {LOCAL_IMPORT_SCHEMA_VERSION})"
            ));
        }

        let mut errors = Vec::new();
        let mut counts = ImportCounts::default();

        if let Some(config) = snapshot.get("config").and_then(Value::as_object) {
            for (key, value) in config {
                match self.set_config_value(key, value.clone(), "import") {
                    Ok(_) => counts.config += 1,
                    Err(e) => errors.push(format!("config.{key}: {e}")),
                }
            }
        }

        if let Some(team) = snapshot.get("team") {
            if let Some(members) = team.get("members").and_then(Value::as_array) {
                for member in members {
                    let user_id = member.get("user_id").and_then(Value::as_str).unwrap_or("");
                    let display_name = member
                        .get("display_name")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let role = member
                        .get("role")
                        .and_then(Value::as_str)
                        .unwrap_or("member");
                    if user_id.is_empty() {
                        errors.push("team member missing user_id".to_string());
                        continue;
                    }
                    match self.upsert_team_member(user_id, display_name, role) {
                        Ok(_) => counts.team += 1,
                        Err(e) => errors.push(format!("team.{user_id}: {e}")),
                    }
                }
            }
        }

        if let Some(audit) = snapshot.get("audit").and_then(Value::as_array) {
            for event in audit {
                let actor = event
                    .get("actor")
                    .and_then(Value::as_str)
                    .unwrap_or("import");
                let action = event
                    .get("action")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let resource = event
                    .get("resource")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let details = event.get("details").cloned().unwrap_or(Value::Null);
                match self.append_audit(actor, action, resource, &details) {
                    Ok(_) => counts.audit += 1,
                    Err(e) => errors.push(format!("audit.{action}: {e}")),
                }
            }
        }

        if let Some(dispatches) = snapshot.get("dispatches").and_then(Value::as_array) {
            for dispatch in dispatches {
                let raw_request = dispatch
                    .get("raw_request")
                    .and_then(Value::as_str)
                    .unwrap_or("{}");
                let request_source = dispatch
                    .get("request_source")
                    .and_then(Value::as_str)
                    .unwrap_or("import");
                let bundle = dispatch.get("bundle").cloned().unwrap_or(Value::Null);
                match self.record_dispatch(raw_request, request_source, &bundle, "import") {
                    Ok(_) => counts.dispatches += 1,
                    Err(e) => errors.push(format!("dispatch: {e}")),
                }
            }
        }

        Ok(ImportResult {
            imported: counts,
            errors,
        })
    }

    fn ensure_default_config(&self) -> Result<(), String> {
        let defaults = [
            ("workspace_name", json!("Local Team")),
            ("provider_transport", json!("stub/off")),
            ("target_repository_writes", json!("disabled")),
            ("sandbox_process_execution", json!("disabled")),
            ("runtime_workers", json!("disabled")),
            ("docker_required", json!(false)),
        ];
        self.with_conn(|conn| {
            for (key, value) in defaults {
                conn.execute(
                    "INSERT OR IGNORE INTO local_config (key, value_json, updated_at, updated_by)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![key, value.to_string(), self.now(), "system"],
                )
                .map_err(|e| e.to_string())?;
            }
            Ok(())
        })
    }

    pub fn record_dispatch(
        &self,
        raw_request: &str,
        request_source: &str,
        bundle: &Value,
        actor: &str,
    ) -> Result<Value, String> {
        let dispatch_id = str_at(bundle, &["record", "dispatch_id"]).unwrap_or("unknown");
        let default_created_at = self.now();
        let created_at = str_at(bundle, &["record", "created_at"]).unwrap_or(&default_created_at);
        let final_status = str_at(bundle, &["record", "final_status"]).unwrap_or("unknown");
        let selected_tier = str_at(bundle, &["decision", "selected_tier"]).unwrap_or("unknown");
        let risk_level = str_at(bundle, &["analysis", "risk_level"]).unwrap_or("unknown");
        let reserved_cost = bundle
            .pointer("/decision/budget_reservation/reserved_cost")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let bundle_json = serde_json::to_string(bundle).map_err(|e| e.to_string())?;

        let input_tokens = bundle
            .pointer("/execution_result/input_tokens")
            .and_then(Value::as_i64);
        let output_tokens = bundle
            .pointer("/execution_result/output_tokens")
            .and_then(Value::as_i64);
        let estimated_cost_usd = bundle
            .pointer("/execution_result/estimated_cost")
            .and_then(Value::as_f64);
        let executor_type = bundle
            .pointer("/execution_result/executor_type")
            .and_then(Value::as_str)
            .unwrap_or("noop");
        let latency_ms = bundle
            .pointer("/execution_result/latency_ms")
            .and_then(Value::as_i64);

        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO dispatch_history
                 (dispatch_id, created_at, raw_request, request_source, final_status,
                  selected_tier, risk_level, reserved_cost, bundle_json,
                  input_tokens, output_tokens, estimated_cost_usd, executor_type, latency_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    dispatch_id,
                    created_at,
                    raw_request,
                    request_source,
                    final_status,
                    selected_tier,
                    risk_level,
                    reserved_cost,
                    bundle_json,
                    input_tokens,
                    output_tokens,
                    estimated_cost_usd,
                    executor_type,
                    latency_ms,
                ],
            )
            .map_err(|e| e.to_string())?;
            let history_id = conn.last_insert_rowid();
            append_audit_locked(
                conn,
                &self.now(),
                actor,
                "dispatch.record",
                dispatch_id,
                &json!({"history_id": history_id, "request_source": request_source}),
            )?;
            Ok(json!({
                "history_id": history_id,
                "dispatch_id": dispatch_id,
                "created_at": created_at,
                "raw_request": raw_request,
                "request_source": request_source,
                "final_status": final_status,
                "selected_tier": selected_tier,
                "risk_level": risk_level,
                "reserved_cost": reserved_cost,
                "input_tokens": input_tokens,
                "output_tokens": output_tokens,
                "estimated_cost_usd": estimated_cost_usd,
                "executor_type": executor_type,
                "latency_ms": latency_ms,
                "bundle": bundle,
            }))
        })
    }

    pub fn list_dispatches(&self, limit: i64) -> Result<Vec<Value>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT history_id, dispatch_id, created_at, raw_request, request_source,
                            final_status, selected_tier, risk_level, reserved_cost, bundle_json,
                            input_tokens, output_tokens, estimated_cost_usd, executor_type, latency_ms
                     FROM dispatch_history
                     ORDER BY history_id DESC
                     LIMIT ?1",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![limit], |row| {
                    let bundle_text: String = row.get(9)?;
                    let bundle: Value = serde_json::from_str(&bundle_text).unwrap_or(Value::Null);
                    Ok(json!({
                        "history_id": row.get::<_, i64>(0)?,
                        "dispatch_id": row.get::<_, String>(1)?,
                        "created_at": row.get::<_, String>(2)?,
                        "raw_request": row.get::<_, String>(3)?,
                        "request_source": row.get::<_, String>(4)?,
                        "final_status": row.get::<_, String>(5)?,
                        "selected_tier": row.get::<_, String>(6)?,
                        "risk_level": row.get::<_, String>(7)?,
                        "reserved_cost": row.get::<_, f64>(8)?,
                        "bundle": bundle,
                        "input_tokens": row.get::<_, Option<i64>>(10)?,
                        "output_tokens": row.get::<_, Option<i64>>(11)?,
                        "estimated_cost_usd": row.get::<_, Option<f64>>(12)?,
                        "executor_type": row.get::<_, String>(13)?,
                        "latency_ms": row.get::<_, Option<i64>>(14)?,
                    }))
                })
                .map_err(|e| e.to_string())?;
            collect_values(rows)
        })
    }

    pub fn get_dispatch(&self, dispatch_id: &str) -> Result<Option<Value>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT history_id, dispatch_id, created_at, raw_request, request_source,
                            final_status, selected_tier, risk_level, reserved_cost, bundle_json,
                            input_tokens, output_tokens, estimated_cost_usd, executor_type, latency_ms
                     FROM dispatch_history
                     WHERE dispatch_id = ?1
                     ORDER BY history_id DESC
                     LIMIT 1",
                )
                .map_err(|e| e.to_string())?;
            let mut rows = stmt
                .query_map(params![dispatch_id], |row| {
                    let bundle_text: String = row.get(9)?;
                    let bundle: Value = serde_json::from_str(&bundle_text).unwrap_or(Value::Null);
                    Ok(json!({
                        "history_id": row.get::<_, i64>(0)?,
                        "dispatch_id": row.get::<_, String>(1)?,
                        "created_at": row.get::<_, String>(2)?,
                        "raw_request": row.get::<_, String>(3)?,
                        "request_source": row.get::<_, String>(4)?,
                        "final_status": row.get::<_, String>(5)?,
                        "selected_tier": row.get::<_, String>(6)?,
                        "risk_level": row.get::<_, String>(7)?,
                        "reserved_cost": row.get::<_, f64>(8)?,
                        "bundle": bundle,
                        "input_tokens": row.get::<_, Option<i64>>(10)?,
                        "output_tokens": row.get::<_, Option<i64>>(11)?,
                        "estimated_cost_usd": row.get::<_, Option<f64>>(12)?,
                        "executor_type": row.get::<_, String>(13)?,
                        "latency_ms": row.get::<_, Option<i64>>(14)?,
                    }))
                })
                .map_err(|e| e.to_string())?;
            match rows.next() {
                Some(Ok(val)) => Ok(Some(val)),
                Some(Err(e)) => Err(e.to_string()),
                None => Ok(None),
            }
        })
    }

    pub fn set_config_value(&self, key: &str, value: Value, actor: &str) -> Result<Value, String> {
        self.with_conn(|conn| {
            let value_json = value.to_string();
            conn.execute(
                "INSERT INTO local_config (key, value_json, updated_at, updated_by)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(key) DO UPDATE SET
                    value_json = excluded.value_json,
                    updated_at = excluded.updated_at,
                    updated_by = excluded.updated_by",
                params![key, value_json, self.now(), actor],
            )
            .map_err(|e| e.to_string())?;
            let now = self.now();
            append_audit_locked(
                conn,
                &now,
                actor,
                "config.update",
                key,
                &json!({"key": key}),
            )?;
            Ok(json!({"key": key, "value": value, "updated_at": now, "updated_by": actor}))
        })
    }

    pub fn config_snapshot(&self) -> Result<Value, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare("SELECT key, value_json FROM local_config ORDER BY key")
                .map_err(|e| e.to_string())?;
            let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
            let mut config = Map::new();
            while let Some(row) = rows.next().map_err(|e| e.to_string())? {
                let key: String = row.get(0).map_err(|e| e.to_string())?;
                let value_text: String = row.get(1).map_err(|e| e.to_string())?;
                let value = serde_json::from_str(&value_text).unwrap_or(Value::Null);
                config.insert(key, value);
            }
            Ok(Value::Object(config))
        })
    }

    pub fn upsert_team_member(
        &self,
        user_id: &str,
        display_name: &str,
        role: &str,
    ) -> Result<Value, String> {
        self.with_conn(|conn| {
            let now = self.now();
            conn.execute(
                "INSERT INTO team_members (user_id, display_name, role, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)
                 ON CONFLICT(user_id) DO UPDATE SET
                    display_name = excluded.display_name,
                    role = excluded.role,
                    updated_at = excluded.updated_at",
                params![user_id, display_name, role, now],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({
                "user_id": user_id,
                "display_name": display_name,
                "role": role,
                "created_at": now,
                "updated_at": now,
            }))
        })
    }

    pub fn record_api_key_metadata(
        &self,
        key_id: &str,
        user_id: &str,
        role: &str,
        scopes: &[String],
        actor: &str,
    ) -> Result<Value, String> {
        let scopes_json = serde_json::to_string(scopes).map_err(|e| e.to_string())?;
        self.with_conn(|conn| {
            let now = self.now();
            conn.execute(
                "INSERT INTO api_key_metadata
                 (key_id, user_id, role, scopes_json, created_at, created_by, revoked_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)
                 ON CONFLICT(key_id) DO UPDATE SET
                    user_id = excluded.user_id,
                    role = excluded.role,
                    scopes_json = excluded.scopes_json,
                    created_by = excluded.created_by",
                params![key_id, user_id, role, scopes_json, now, actor],
            )
            .map_err(|e| e.to_string())?;
            append_audit_locked(
                conn,
                &now,
                actor,
                "api_key.record_metadata",
                key_id,
                &json!({"key_id": key_id, "user_id": user_id, "role": role}),
            )?;
            Ok(json!({
                "key_id": key_id,
                "user_id": user_id,
                "role": role,
                "scopes": scopes,
                "created_at": now,
                "created_by": actor,
            }))
        })
    }

    pub fn get_api_key_metadata(&self, key_id: &str) -> Result<Option<Value>, String> {
        self.with_conn(|conn| {
            let result = conn.query_row(
                "SELECT key_id, user_id, role, scopes_json, created_at, created_by,
                        revoked_at, last_used_at, expires_at
                 FROM api_key_metadata WHERE key_id = ?1",
                params![key_id],
                |row| {
                    let scopes_text: String = row.get(3)?;
                    let scopes: Vec<String> =
                        serde_json::from_str(&scopes_text).unwrap_or_default();
                    Ok(json!({
                        "key_id": row.get::<_, String>(0)?,
                        "user_id": row.get::<_, String>(1)?,
                        "role": row.get::<_, String>(2)?,
                        "scopes": scopes,
                        "created_at": row.get::<_, String>(4)?,
                        "created_by": row.get::<_, String>(5)?,
                        "revoked_at": row.get::<_, Option<String>>(6)?,
                        "last_used_at": row.get::<_, Option<String>>(7)?,
                        "expires_at": row.get::<_, Option<String>>(8)?,
                    }))
                },
            );
            match result {
                Ok(value) => Ok(Some(value)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.to_string()),
            }
        })
    }

    pub fn revoke_api_key_metadata(&self, key_id: &str, actor: &str) -> Result<bool, String> {
        self.with_conn(|conn| {
            let now = self.now();
            let rows = conn
                .execute(
                    "UPDATE api_key_metadata SET revoked_at = ?1 WHERE key_id = ?2 AND revoked_at IS NULL",
                    params![now, key_id],
                )
                .map_err(|e| e.to_string())?;
            if rows > 0 {
                append_audit_locked(
                    conn,
                    &now,
                    actor,
                    "team.key.revoked",
                    key_id,
                    &json!({"key_id": key_id}),
                )?;
            }
            Ok(rows > 0)
        })
    }

    pub fn delete_api_key_metadata(&self, key_id: &str, actor: &str) -> Result<bool, String> {
        self.with_conn(|conn| {
            let rows = conn
                .execute(
                    "DELETE FROM api_key_metadata WHERE key_id = ?1",
                    params![key_id],
                )
                .map_err(|e| e.to_string())?;
            if rows > 0 {
                append_audit_locked(
                    conn,
                    &self.now(),
                    actor,
                    "team.key.deleted",
                    key_id,
                    &json!({"key_id": key_id}),
                )?;
            }
            Ok(rows > 0)
        })
    }

    pub fn update_api_key_scopes(
        &self,
        key_id: &str,
        scopes: &[String],
        actor: &str,
    ) -> Result<bool, String> {
        let scopes_json = serde_json::to_string(scopes).map_err(|e| e.to_string())?;
        self.with_conn(|conn| {
            let rows = conn
                .execute(
                    "UPDATE api_key_metadata SET scopes_json = ?1 WHERE key_id = ?2",
                    params![scopes_json, key_id],
                )
                .map_err(|e| e.to_string())?;
            if rows > 0 {
                append_audit_locked(
                    conn,
                    &self.now(),
                    actor,
                    "team.key.scopes_updated",
                    key_id,
                    &json!({"key_id": key_id, "scopes": scopes}),
                )?;
            }
            Ok(rows > 0)
        })
    }

    pub fn update_team_member_role(
        &self,
        user_id: &str,
        role: &str,
        actor: &str,
    ) -> Result<bool, String> {
        self.with_conn(|conn| {
            let now = self.now();
            let rows = conn
                .execute(
                    "UPDATE team_members SET role = ?1, updated_at = ?2 WHERE user_id = ?3",
                    params![role, now, user_id],
                )
                .map_err(|e| e.to_string())?;
            if rows > 0 {
                append_audit_locked(
                    conn,
                    &now,
                    actor,
                    "team.member.role_updated",
                    user_id,
                    &json!({"user_id": user_id, "role": role}),
                )?;
            }
            Ok(rows > 0)
        })
    }

    pub fn delete_team_member(&self, user_id: &str, actor: &str) -> Result<bool, String> {
        self.with_conn(|conn| {
            let rows = conn
                .execute(
                    "DELETE FROM team_members WHERE user_id = ?1",
                    params![user_id],
                )
                .map_err(|e| e.to_string())?;
            if rows > 0 {
                append_audit_locked(
                    conn,
                    &self.now(),
                    actor,
                    "team.member.deleted",
                    user_id,
                    &json!({"user_id": user_id}),
                )?;
            }
            Ok(rows > 0)
        })
    }

    pub fn touch_api_key_last_used(&self, key_id: &str) -> Result<(), String> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE api_key_metadata SET last_used_at = ?1 WHERE key_id = ?2",
                params![self.now(), key_id],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })
    }

    pub fn team_snapshot(&self) -> Result<Value, String> {
        self.with_conn(|conn| {
            let members = {
                let mut stmt = conn
                    .prepare(
                        "SELECT user_id, display_name, role, created_at, updated_at
                         FROM team_members
                         ORDER BY user_id",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map([], |row| {
                        Ok(json!({
                            "user_id": row.get::<_, String>(0)?,
                            "display_name": row.get::<_, String>(1)?,
                            "role": row.get::<_, String>(2)?,
                            "created_at": row.get::<_, String>(3)?,
                            "updated_at": row.get::<_, String>(4)?,
                        }))
                    })
                    .map_err(|e| e.to_string())?;
                collect_values(rows)?
            };

            let api_keys = {
                let mut stmt = conn
                    .prepare(
                        "SELECT key_id, user_id, role, scopes_json, created_at, created_by,
                                revoked_at, last_used_at, expires_at
                         FROM api_key_metadata
                         ORDER BY key_id",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map([], |row| {
                        let scopes_text: String = row.get(3)?;
                        let scopes: Vec<String> =
                            serde_json::from_str(&scopes_text).unwrap_or_default();
                        Ok(json!({
                            "key_id": row.get::<_, String>(0)?,
                            "user_id": row.get::<_, String>(1)?,
                            "role": row.get::<_, String>(2)?,
                            "scopes": scopes,
                            "created_at": row.get::<_, String>(4)?,
                            "created_by": row.get::<_, String>(5)?,
                            "revoked_at": row.get::<_, Option<String>>(6)?,
                            "last_used_at": row.get::<_, Option<String>>(7)?,
                            "expires_at": row.get::<_, Option<String>>(8)?,
                        }))
                    })
                    .map_err(|e| e.to_string())?;
                collect_values(rows)?
            };

            Ok(json!({
                "schema_version": "local_team.v1",
                "members": members,
                "api_keys": api_keys,
            }))
        })
    }

    pub fn cost_summary(&self) -> Result<Value, String> {
        self.with_conn(|conn| {
            let dispatch_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM dispatch_history", [], |row| {
                    row.get(0)
                })
                .map_err(|e| e.to_string())?;
            let total_reserved_cost: f64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(reserved_cost), 0.0) FROM dispatch_history",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            let total_estimated_cost_usd: f64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(estimated_cost_usd), 0.0) FROM dispatch_history",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            let total_input_tokens: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(input_tokens), 0) FROM dispatch_history",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            let total_output_tokens: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(output_tokens), 0) FROM dispatch_history",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            let cost_utilization = if total_reserved_cost > 0.0 {
                total_estimated_cost_usd / total_reserved_cost
            } else {
                0.0
            };
            let mut tier_stmt = conn
                .prepare(
                    "SELECT selected_tier, COUNT(*),
                            COALESCE(SUM(reserved_cost), 0.0),
                            COALESCE(SUM(estimated_cost_usd), 0.0),
                            COALESCE(SUM(input_tokens), 0),
                            COALESCE(SUM(output_tokens), 0)
                     FROM dispatch_history
                     GROUP BY selected_tier
                     ORDER BY selected_tier",
                )
                .map_err(|e| e.to_string())?;
            let tier_rows = tier_stmt
                .query_map([], |row| {
                    Ok(json!({
                        "selected_tier": row.get::<_, String>(0)?,
                        "dispatch_count": row.get::<_, i64>(1)?,
                        "reserved_cost": row.get::<_, f64>(2)?,
                        "estimated_cost_usd": row.get::<_, f64>(3)?,
                        "input_tokens": row.get::<_, i64>(4)?,
                        "output_tokens": row.get::<_, i64>(5)?,
                    }))
                })
                .map_err(|e| e.to_string())?;
            let mut daily_stmt = conn
                .prepare(
                    "SELECT substr(created_at, 1, 10) as dt, COUNT(*),
                            COALESCE(SUM(reserved_cost), 0.0),
                            COALESCE(SUM(estimated_cost_usd), 0.0)
                     FROM dispatch_history
                     GROUP BY dt
                     ORDER BY dt DESC
                     LIMIT 30",
                )
                .map_err(|e| e.to_string())?;
            let daily_rows = daily_stmt
                .query_map([], |row| {
                    Ok(json!({
                        "date": row.get::<_, String>(0)?,
                        "dispatch_count": row.get::<_, i64>(1)?,
                        "reserved_cost": row.get::<_, f64>(2)?,
                        "estimated_cost_usd": row.get::<_, f64>(3)?,
                    }))
                })
                .map_err(|e| e.to_string())?;
            Ok(json!({
                "schema_version": "local_cost_summary.v2",
                "currency": "USD",
                "dispatch_count": dispatch_count,
                "total_reserved_cost": total_reserved_cost,
                "total_estimated_cost_usd": total_estimated_cost_usd,
                "total_input_tokens": total_input_tokens,
                "total_output_tokens": total_output_tokens,
                "cost_utilization": cost_utilization,
                "by_tier": collect_values(tier_rows)?,
                "daily": collect_values(daily_rows)?,
            }))
        })
    }

    pub fn dispatch_cost_details(&self, limit: i64) -> Result<Value, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT history_id, dispatch_id, created_at, selected_tier,
                            reserved_cost,
                            COALESCE(input_tokens, 0),
                            COALESCE(output_tokens, 0),
                            COALESCE(estimated_cost_usd, 0.0),
                            executor_type,
                            latency_ms
                     FROM dispatch_history
                     ORDER BY history_id DESC
                     LIMIT ?1",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![limit], |row| {
                    Ok(json!({
                        "history_id": row.get::<_, i64>(0)?,
                        "dispatch_id": row.get::<_, String>(1)?,
                        "created_at": row.get::<_, String>(2)?,
                        "selected_tier": row.get::<_, String>(3)?,
                        "reserved_cost": row.get::<_, f64>(4)?,
                        "input_tokens": row.get::<_, i64>(5)?,
                        "output_tokens": row.get::<_, i64>(6)?,
                        "estimated_cost_usd": row.get::<_, f64>(7)?,
                        "executor_type": row.get::<_, String>(8)?,
                        "latency_ms": row.get::<_, Option<i64>>(9)?,
                    }))
                })
                .map_err(|e| e.to_string())?;
            Ok(json!({
                "schema_version": "local_dispatch_cost_detail.v1",
                "dispatches": collect_values(rows)?,
            }))
        })
    }

    pub fn daily_estimated_cost_usd(&self, date_prefix: &str) -> Result<f64, String> {
        self.with_conn(|conn| {
            let like_pattern = format!("{}%", date_prefix);
            conn.query_row(
                "SELECT COALESCE(SUM(estimated_cost_usd), 0.0)
                 FROM dispatch_history
                 WHERE created_at LIKE ?1",
                params![like_pattern],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())
        })
    }

    pub fn audit_events(&self, limit: i64) -> Result<Vec<Value>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT audit_id, created_at, actor, action, resource, details_json
                     FROM audit_log
                     ORDER BY audit_id DESC
                     LIMIT ?1",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![limit], |row| {
                    let details_text: String = row.get(5)?;
                    let details: Value = serde_json::from_str(&details_text).unwrap_or(Value::Null);
                    Ok(json!({
                        "audit_id": row.get::<_, i64>(0)?,
                        "created_at": row.get::<_, String>(1)?,
                        "actor": row.get::<_, String>(2)?,
                        "action": row.get::<_, String>(3)?,
                        "resource": row.get::<_, String>(4)?,
                        "details": details,
                    }))
                })
                .map_err(|e| e.to_string())?;
            collect_values(rows)
        })
    }

    pub fn append_audit(
        &self,
        actor: &str,
        action: &str,
        resource: &str,
        details: &Value,
    ) -> Result<Value, String> {
        self.with_conn(|conn| {
            let now = self.now();
            let audit_id = append_audit_locked(conn, &now, actor, action, resource, details)?;
            Ok(json!({
                "audit_id": audit_id,
                "created_at": now,
                "actor": actor,
                "action": action,
                "resource": resource,
                "details": details,
            }))
        })
    }

    pub fn stats(&self) -> Result<Value, String> {
        self.with_conn(|conn| {
            Ok(json!({
                "dispatches": count_table(conn, "dispatch_history")?,
                "team_members": count_table(conn, "team_members")?,
                "api_keys": count_table(conn, "api_key_metadata")?,
                "audit_events": count_table(conn, "audit_log")?,
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

    pub fn export_snapshot(
        &self,
        executor_type: &str,
        provider_enabled: bool,
    ) -> Result<Value, String> {
        Ok(json!({
            "schema_version": LOCAL_TEAM_EXPORT_SCHEMA_VERSION,
            "generated_at": self.now(),
            "dispatches": self.list_dispatches(10_000)?,
            "config": self.config_snapshot()?,
            "team": self.team_snapshot()?,
            "costs": self.cost_summary()?,
            "audit": self.audit_events(10_000)?,
            "boundaries": local_boundaries(executor_type, provider_enabled),
        }))
    }

    pub fn record_provider_audit_event(
        &self,
        event: &crate::provider::ProviderAuditEvent,
    ) -> Result<(), String> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT OR IGNORE INTO provider_audit_events
                 (event_id, dispatch_id, provider_id, event_type,
                  input_token_count, output_token_count, cost, currency,
                  latency_ms, error_domain, redaction_status, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    event.event_id,
                    event.dispatch_id,
                    event.provider_id,
                    event.event_type,
                    event.input_token_count,
                    event.output_token_count,
                    event.cost,
                    event.currency,
                    event.latency_ms,
                    event.error_domain,
                    event.redaction_status,
                    event.created_at,
                ],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })
    }

    pub fn provider_audit_events(&self, limit: i64) -> Result<Vec<Value>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT event_id, dispatch_id, provider_id, event_type,
                            input_token_count, output_token_count, cost, currency,
                            latency_ms, error_domain, redaction_status, created_at
                     FROM provider_audit_events
                     ORDER BY created_at DESC
                     LIMIT ?1",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![limit], |row| {
                    Ok(json!({
                        "event_id": row.get::<_, String>(0)?,
                        "dispatch_id": row.get::<_, String>(1)?,
                        "provider_id": row.get::<_, String>(2)?,
                        "event_type": row.get::<_, String>(3)?,
                        "input_token_count": row.get::<_, Option<i64>>(4)?,
                        "output_token_count": row.get::<_, Option<i64>>(5)?,
                        "cost": row.get::<_, Option<f64>>(6)?,
                        "currency": row.get::<_, Option<String>>(7)?,
                        "latency_ms": row.get::<_, Option<i64>>(8)?,
                        "error_domain": row.get::<_, Option<String>>(9)?,
                        "redaction_status": row.get::<_, String>(10)?,
                        "created_at": row.get::<_, String>(11)?,
                    }))
                })
                .map_err(|e| e.to_string())?;
            collect_values(rows)
        })
    }

    pub fn provider_audit_events_for_dispatch(
        &self,
        dispatch_id: &str,
    ) -> Result<Vec<Value>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT event_id, dispatch_id, provider_id, event_type,
                            input_token_count, output_token_count, cost, currency,
                            latency_ms, error_domain, redaction_status, created_at
                     FROM provider_audit_events
                     WHERE dispatch_id = ?1
                     ORDER BY created_at DESC",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![dispatch_id], |row| {
                    Ok(json!({
                        "event_id": row.get::<_, String>(0)?,
                        "dispatch_id": row.get::<_, String>(1)?,
                        "provider_id": row.get::<_, String>(2)?,
                        "event_type": row.get::<_, String>(3)?,
                        "input_token_count": row.get::<_, Option<i64>>(4)?,
                        "output_token_count": row.get::<_, Option<i64>>(5)?,
                        "cost": row.get::<_, Option<f64>>(6)?,
                        "currency": row.get::<_, Option<String>>(7)?,
                        "latency_ms": row.get::<_, Option<i64>>(8)?,
                        "error_domain": row.get::<_, Option<String>>(9)?,
                        "redaction_status": row.get::<_, String>(10)?,
                        "created_at": row.get::<_, String>(11)?,
                    }))
                })
                .map_err(|e| e.to_string())?;
            collect_values(rows)
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableIntegrity {
    pub name: String,
    pub row_count: i64,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IntegrityReport {
    pub status: String,
    pub tables: Vec<TableIntegrity>,
    pub schema_version: i64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportCounts {
    pub dispatches: i64,
    pub config: i64,
    pub team: i64,
    pub audit: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportResult {
    pub imported: ImportCounts,
    pub errors: Vec<String>,
}

pub fn local_boundaries(executor_type: &str, provider_enabled: bool) -> Value {
    let provider_transport = match executor_type {
        "provider" if provider_enabled => "provider/enabled",
        "provider" => "provider/disabled",
        "stub" => "stub",
        _ => "noop",
    };
    json!({
        "provider_transport": provider_transport,
        "target_repository_writes": "disabled",
        "sandbox_process_execution": "disabled",
        "runtime_workers": "disabled",
        "deployment": "local-only",
        "docker_required": false,
    })
}

fn append_audit_locked(
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

fn collect_values(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row) -> rusqlite::Result<Value>>,
) -> Result<Vec<Value>, String> {
    let mut values = Vec::new();
    for row in rows {
        values.push(row.map_err(|e| e.to_string())?);
    }
    Ok(values)
}

fn str_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for part in path {
        current = current.get(*part)?;
    }
    current.as_str()
}
