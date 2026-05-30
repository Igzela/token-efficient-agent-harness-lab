use rusqlite::Connection;

use super::LocalProductStore;

pub(super) const CURRENT_SCHEMA_VERSION: i64 = 1;

struct Migration {
    version: i64,
    description: &'static str,
}

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    description: "add last_used_at and expires_at to api_key_metadata",
}];

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
}
