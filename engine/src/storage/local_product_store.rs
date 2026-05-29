use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const LOCAL_PRODUCT_STORE_SCHEMA_VERSION: &str = "local_product_store.v1";
pub const LOCAL_TEAM_EXPORT_SCHEMA_VERSION: &str = "local_team_export.v1";
pub const LOCAL_DASHBOARD_SCHEMA_VERSION: &str = "local_dashboard.v1";

const LOCAL_NOW: &str = "2026-05-29T00:00:00Z";

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
    revoked_at TEXT
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
}

impl LocalProductStore {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, String> {
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
        };
        store.ensure_default_config()?;
        Ok(store)
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
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
                    params![key, value.to_string(), LOCAL_NOW, "system"],
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
        let created_at = str_at(bundle, &["record", "created_at"]).unwrap_or(LOCAL_NOW);
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
                params![key, value_json, LOCAL_NOW, actor],
            )
            .map_err(|e| e.to_string())?;
            append_audit_locked(conn, actor, "config.update", key, &json!({"key": key}))?;
            Ok(json!({"key": key, "value": value, "updated_at": LOCAL_NOW, "updated_by": actor}))
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
            conn.execute(
                "INSERT INTO team_members (user_id, display_name, role, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)
                 ON CONFLICT(user_id) DO UPDATE SET
                    display_name = excluded.display_name,
                    role = excluded.role,
                    updated_at = excluded.updated_at",
                params![user_id, display_name, role, LOCAL_NOW],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({
                "user_id": user_id,
                "display_name": display_name,
                "role": role,
                "created_at": LOCAL_NOW,
                "updated_at": LOCAL_NOW,
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
            conn.execute(
                "INSERT INTO api_key_metadata
                 (key_id, user_id, role, scopes_json, created_at, created_by, revoked_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)
                 ON CONFLICT(key_id) DO UPDATE SET
                    user_id = excluded.user_id,
                    role = excluded.role,
                    scopes_json = excluded.scopes_json,
                    created_by = excluded.created_by",
                params![key_id, user_id, role, scopes_json, LOCAL_NOW, actor],
            )
            .map_err(|e| e.to_string())?;
            append_audit_locked(
                conn,
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
                "created_at": LOCAL_NOW,
                "created_by": actor,
            }))
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
                        "SELECT key_id, user_id, role, scopes_json, created_at, created_by, revoked_at
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
            let mut stmt = conn
                .prepare(
                    "SELECT selected_tier, COUNT(*), COALESCE(SUM(reserved_cost), 0.0)
                     FROM dispatch_history
                     GROUP BY selected_tier
                     ORDER BY selected_tier",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(json!({
                        "selected_tier": row.get::<_, String>(0)?,
                        "dispatch_count": row.get::<_, i64>(1)?,
                        "reserved_cost": row.get::<_, f64>(2)?,
                    }))
                })
                .map_err(|e| e.to_string())?;
            Ok(json!({
                "schema_version": "local_cost_summary.v1",
                "currency": "USD",
                "dispatch_count": dispatch_count,
                "total_reserved_cost": total_reserved_cost,
                "by_tier": collect_values(rows)?,
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
            let audit_id = append_audit_locked(conn, actor, action, resource, details)?;
            Ok(json!({
                "audit_id": audit_id,
                "created_at": LOCAL_NOW,
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
            "generated_at": LOCAL_NOW,
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
    actor: &str,
    action: &str,
    resource: &str,
    details: &Value,
) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![LOCAL_NOW, actor, action, resource, details.to_string()],
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
