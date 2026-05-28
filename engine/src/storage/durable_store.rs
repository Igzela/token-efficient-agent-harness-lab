use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

pub const DURABLE_STORE_SCHEMA_VERSION: &str = "durable_store.v1";

const DDL: &str = "
CREATE TABLE IF NOT EXISTS plans (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    schema_version TEXT,
    data TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS repos (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    schema_version TEXT,
    data TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    schema_version TEXT,
    event_type TEXT,
    data TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS migration_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    source TEXT NOT NULL,
    target TEXT NOT NULL,
    records_migrated INTEGER DEFAULT 0,
    status TEXT DEFAULT 'running'
);
CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
CREATE INDEX IF NOT EXISTS idx_events_created ON events(created_at);
";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredRecord {
    pub record_id: String,
    pub created_at: String,
    pub schema_version: Option<String>,
    pub data: serde_json::Value,
}

pub struct DurableStore {
    db_path: String,
    conn: Mutex<Option<Connection>>,
}

impl DurableStore {
    pub fn new(db_path: &str) -> Result<Self, String> {
        let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .map_err(|e| e.to_string())?;
        conn.execute_batch(DDL).map_err(|e| e.to_string())?;

        Ok(Self {
            db_path: db_path.to_string(),
            conn: Mutex::new(Some(conn)),
        })
    }

    pub fn new_memory() -> Result<Self, String> {
        Self::new(":memory:")
    }

    pub fn db_path(&self) -> &str {
        &self.db_path
    }

    fn with_conn<F, R>(&self, f: F) -> Result<R, String>
    where
        F: FnOnce(&Connection) -> Result<R, String>,
    {
        let guard = self.conn.lock().map_err(|e| e.to_string())?;
        let conn = guard.as_ref().ok_or("store is closed")?;
        f(conn)
    }

    pub fn close(&self) -> Result<(), String> {
        let mut guard = self.conn.lock().map_err(|e| e.to_string())?;
        if let Some(conn) = guard.take() {
            conn.close().map_err(|_| "failed to close".to_string())?;
        }
        Ok(())
    }

    // ── Plans ──

    pub fn save_plan(
        &self,
        plan_id: &str,
        data: &serde_json::Value,
        schema_version: Option<&str>,
        created_at: Option<&str>,
        upsert: bool,
    ) -> Result<StoredRecord, String> {
        let ts = created_at.map(String::from).unwrap_or_else(chrono_now);
        let sv = schema_version.map(String::from).or_else(|| {
            data.get("schema_version")
                .and_then(|v| v.as_str())
                .map(String::from)
        });
        let blob = serde_json::to_string(data).map_err(|e| e.to_string())?;
        let sql = if upsert {
            "INSERT OR REPLACE INTO plans (id, created_at, schema_version, data) VALUES (?1, ?2, ?3, ?4)"
        } else {
            "INSERT INTO plans (id, created_at, schema_version, data) VALUES (?1, ?2, ?3, ?4)"
        };
        self.with_conn(|conn| {
            conn.execute(sql, params![plan_id, ts, sv, blob])
                .map_err(|e| e.to_string())?;
            Ok(StoredRecord {
                record_id: plan_id.to_string(),
                created_at: ts,
                schema_version: sv,
                data: data.clone(),
            })
        })
    }

    pub fn get_plan(&self, plan_id: &str) -> Result<Option<StoredRecord>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare("SELECT id, created_at, schema_version, data FROM plans WHERE id = ?1")
                .map_err(|e| e.to_string())?;
            let mut rows = stmt
                .query_map(params![plan_id], |row| {
                    Ok(RowTuple {
                        id: row.get(0)?,
                        created_at: row.get(1)?,
                        schema_version: row.get(2)?,
                        data: row.get(3)?,
                    })
                })
                .map_err(|e| e.to_string())?;
            match rows.next() {
                Some(Ok(row)) => {
                    let data: serde_json::Value =
                        serde_json::from_str(&row.data).map_err(|e| e.to_string())?;
                    Ok(Some(StoredRecord {
                        record_id: row.id,
                        created_at: row.created_at,
                        schema_version: row.schema_version,
                        data,
                    }))
                }
                Some(Err(e)) => Err(e.to_string()),
                None => Ok(None),
            }
        })
    }

    pub fn list_plans(&self) -> Result<Vec<StoredRecord>, String> {
        self.list_table("plans")
    }

    pub fn delete_plan(&self, plan_id: &str) -> Result<bool, String> {
        self.delete_from_table("plans", plan_id)
    }

    // ── Repos ──

    pub fn save_repo(
        &self,
        repo_id: &str,
        data: &serde_json::Value,
        schema_version: Option<&str>,
        created_at: Option<&str>,
        upsert: bool,
    ) -> Result<StoredRecord, String> {
        let ts = created_at.map(String::from).unwrap_or_else(chrono_now);
        let sv = schema_version.map(String::from).or_else(|| {
            data.get("schema_version")
                .and_then(|v| v.as_str())
                .map(String::from)
        });
        let blob = serde_json::to_string(data).map_err(|e| e.to_string())?;
        let sql = if upsert {
            "INSERT OR REPLACE INTO repos (id, created_at, schema_version, data) VALUES (?1, ?2, ?3, ?4)"
        } else {
            "INSERT INTO repos (id, created_at, schema_version, data) VALUES (?1, ?2, ?3, ?4)"
        };
        self.with_conn(|conn| {
            conn.execute(sql, params![repo_id, ts, sv, blob])
                .map_err(|e| e.to_string())?;
            Ok(StoredRecord {
                record_id: repo_id.to_string(),
                created_at: ts,
                schema_version: sv,
                data: data.clone(),
            })
        })
    }

    pub fn get_repo(&self, repo_id: &str) -> Result<Option<StoredRecord>, String> {
        self.get_from_table("repos", repo_id)
    }

    pub fn list_repos(&self) -> Result<Vec<StoredRecord>, String> {
        self.list_table("repos")
    }

    pub fn delete_repo(&self, repo_id: &str) -> Result<bool, String> {
        self.delete_from_table("repos", repo_id)
    }

    // ── Events ──

    pub fn save_event(
        &self,
        event_id: &str,
        data: &serde_json::Value,
        schema_version: Option<&str>,
        created_at: Option<&str>,
        upsert: bool,
    ) -> Result<StoredRecord, String> {
        let ts = created_at.map(String::from).unwrap_or_else(chrono_now);
        let sv = schema_version.map(String::from).or_else(|| {
            data.get("schema_version")
                .and_then(|v| v.as_str())
                .map(String::from)
        });
        let event_type = data
            .get("event_type")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let blob = serde_json::to_string(data).map_err(|e| e.to_string())?;
        let sql = if upsert {
            "INSERT OR REPLACE INTO events (id, created_at, schema_version, event_type, data) VALUES (?1, ?2, ?3, ?4, ?5)"
        } else {
            "INSERT INTO events (id, created_at, schema_version, event_type, data) VALUES (?1, ?2, ?3, ?4, ?5)"
        };
        self.with_conn(|conn| {
            conn.execute(sql, params![event_id, ts, sv, event_type, blob])
                .map_err(|e| e.to_string())?;
            Ok(StoredRecord {
                record_id: event_id.to_string(),
                created_at: ts,
                schema_version: sv,
                data: data.clone(),
            })
        })
    }

    pub fn get_event(&self, event_id: &str) -> Result<Option<StoredRecord>, String> {
        self.get_from_table("events", event_id)
    }

    pub fn get_events(
        &self,
        event_type: Option<&str>,
        limit: i64,
    ) -> Result<Vec<StoredRecord>, String> {
        self.with_conn(|conn| {
            if let Some(et) = event_type {
                let mut stmt = conn
                    .prepare("SELECT id, created_at, schema_version, data FROM events WHERE event_type = ?1 ORDER BY created_at DESC LIMIT ?2")
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![et, limit], row_to_record)
                    .map_err(|e| e.to_string())?;
                collect_rows(rows)
            } else {
                let mut stmt = conn
                    .prepare("SELECT id, created_at, schema_version, data FROM events ORDER BY created_at DESC LIMIT ?1")
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![limit], row_to_record)
                    .map_err(|e| e.to_string())?;
                collect_rows(rows)
            }
        })
    }

    pub fn delete_event(&self, event_id: &str) -> Result<bool, String> {
        self.delete_from_table("events", event_id)
    }

    // ── Migration Log ──

    pub fn log_migration_start(&self, source: &str, target: &str) -> Result<i64, String> {
        let ts = chrono_now();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO migration_log (started_at, source, target) VALUES (?1, ?2, ?3)",
                params![ts, source, target],
            )
            .map_err(|e| e.to_string())?;
            Ok(conn.last_insert_rowid())
        })
    }

    pub fn log_migration_finish(
        &self,
        migration_id: i64,
        records_migrated: i64,
        status: &str,
    ) -> Result<(), String> {
        let ts = chrono_now();
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE migration_log SET finished_at = ?1, records_migrated = ?2, status = ?3 WHERE id = ?4",
                params![ts, records_migrated, status, migration_id],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })
    }

    pub fn get_migration_log(&self) -> Result<Vec<serde_json::Value>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare("SELECT id, started_at, finished_at, source, target, records_migrated, status FROM migration_log ORDER BY started_at")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(serde_json::json!({
                        "id": row.get::<_, i64>(0)?,
                        "started_at": row.get::<_, String>(1)?,
                        "finished_at": row.get::<_, Option<String>>(2)?,
                        "source": row.get::<_, String>(3)?,
                        "target": row.get::<_, String>(4)?,
                        "records_migrated": row.get::<_, i64>(5)?,
                        "status": row.get::<_, String>(6)?,
                    }))
                })
                .map_err(|e| e.to_string())?;
            collect_rows(rows)
        })
    }

    // ── Stats ──

    pub fn stats(&self) -> Result<serde_json::Value, String> {
        self.with_conn(|conn| {
            let plans: i64 = conn
                .query_row("SELECT COUNT(*) FROM plans", [], |r| r.get(0))
                .map_err(|e| e.to_string())?;
            let repos: i64 = conn
                .query_row("SELECT COUNT(*) FROM repos", [], |r| r.get(0))
                .map_err(|e| e.to_string())?;
            let events: i64 = conn
                .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
                .map_err(|e| e.to_string())?;
            let migrations: i64 = conn
                .query_row("SELECT COUNT(*) FROM migration_log", [], |r| r.get(0))
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({
                "plans": plans,
                "repos": repos,
                "events": events,
                "migrations": migrations,
            }))
        })
    }

    // ── Helpers ──

    fn list_table(&self, table: &str) -> Result<Vec<StoredRecord>, String> {
        let sql =
            format!("SELECT id, created_at, schema_version, data FROM {table} ORDER BY created_at");
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], row_to_record)
                .map_err(|e| e.to_string())?;
            collect_rows(rows)
        })
    }

    fn get_from_table(&self, table: &str, id: &str) -> Result<Option<StoredRecord>, String> {
        let sql = format!("SELECT id, created_at, schema_version, data FROM {table} WHERE id = ?1");
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
            let mut rows = stmt
                .query_map(params![id], row_to_record)
                .map_err(|e| e.to_string())?;
            match rows.next() {
                Some(Ok(r)) => Ok(Some(r)),
                Some(Err(e)) => Err(e.to_string()),
                None => Ok(None),
            }
        })
    }

    fn delete_from_table(&self, table: &str, id: &str) -> Result<bool, String> {
        let sql = format!("DELETE FROM {table} WHERE id = ?1");
        self.with_conn(|conn| {
            let count = conn.execute(&sql, params![id]).map_err(|e| e.to_string())?;
            Ok(count > 0)
        })
    }
}

struct RowTuple {
    id: String,
    created_at: String,
    schema_version: Option<String>,
    data: String,
}

fn row_to_record(row: &rusqlite::Row) -> rusqlite::Result<StoredRecord> {
    let id: String = row.get(0)?;
    let created_at: String = row.get(1)?;
    let schema_version: Option<String> = row.get(2)?;
    let data_str: String = row.get(3)?;
    let data: serde_json::Value =
        serde_json::from_str(&data_str).unwrap_or(serde_json::Value::Null);
    Ok(StoredRecord {
        record_id: id,
        created_at,
        schema_version,
        data,
    })
}

fn collect_rows<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row) -> rusqlite::Result<T>>,
) -> Result<Vec<T>, String> {
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| e.to_string())?);
    }
    Ok(result)
}

fn chrono_now() -> String {
    "2025-01-01T00:00:00Z".to_string()
}
