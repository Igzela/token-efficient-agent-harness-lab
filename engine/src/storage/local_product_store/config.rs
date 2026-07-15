use rusqlite::params;
use serde_json::{json, Map, Value};

use super::{append_audit_locked, DatabaseConnection, LocalProductStore};

impl LocalProductStore {
    pub(super) fn ensure_default_config(&self) -> Result<(), String> {
        let defaults = [
            ("workspace_name", json!("Local Team")),
            ("provider_transport", json!("stub/off")),
            ("target_repository_writes", json!("disabled")),
            ("sandbox_process_execution", json!("disabled")),
            ("runtime_workers", json!("env_gated_supervised")),
            ("docker_required", json!(false)),
        ];
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                for (key, value) in defaults {
                    conn.execute(
                        "INSERT OR IGNORE INTO local_config (key, value_json, updated_at, updated_by)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![key, value.to_string(), self.now(), "system"],
                    )
                    .map_err(|e| e.to_string())?;
                }
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                for (key, value) in defaults {
                    client.execute(
                        "INSERT INTO local_config (key, value_json, updated_at, updated_by)
                         VALUES ($1, $2, $3, $4)
                         ON CONFLICT DO NOTHING",
                        &[&key, &value.to_string(), &self.now(), &"system"],
                    ).map_err(|e| e.to_string())?;
                }
                Ok(())
            }),
        }
    }

    pub fn set_config_value(&self, key: &str, value: Value, actor: &str) -> Result<Value, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = conn
                    .unchecked_transaction()
                    .map_err(|error| error.to_string())?;
                let value_json = value.to_string();
                let now = self.now();
                tx.execute(
                    "INSERT INTO local_config (key, value_json, updated_at, updated_by)
                     VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(key) DO UPDATE SET
                        value_json = excluded.value_json,
                        updated_at = excluded.updated_at,
                        updated_by = excluded.updated_by",
                    params![key, value_json, now, actor],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(&tx, &now, actor, "config.update", key, &json!({"key": key}))?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(json!({"key": key, "value": value, "updated_at": now, "updated_by": actor}))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut transaction = client.transaction().map_err(|error| error.to_string())?;
                let value_json = value.to_string();
                let now = self.now();
                transaction
                    .execute(
                        "INSERT INTO local_config (key, value_json, updated_at, updated_by)
                     VALUES ($1, $2, $3, $4)
                     ON CONFLICT(key) DO UPDATE SET
                        value_json = excluded.value_json,
                        updated_at = excluded.updated_at,
                        updated_by = excluded.updated_by",
                        &[&key, &value_json, &now, &actor],
                    )
                    .map_err(|e| e.to_string())?;
                let details = json!({"key": key}).to_string();
                transaction
                    .execute(
                        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                             VALUES ($1, $2, $3, $4, $5)",
                        &[&now, &actor, &"config.update", &key, &details],
                    )
                    .map_err(|e| e.to_string())?;
                transaction.commit().map_err(|error| error.to_string())?;
                Ok(json!({"key": key, "value": value, "updated_at": now, "updated_by": actor}))
            }),
        }
    }

    pub fn config_snapshot(&self) -> Result<Value, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
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
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query("SELECT key, value_json FROM local_config ORDER BY key", &[])
                    .map_err(|e| e.to_string())?;
                let mut config = Map::new();
                for row in &rows {
                    let key: String = row.get(0);
                    let value_text: String = row.get(1);
                    let value = serde_json::from_str(&value_text).unwrap_or(Value::Null);
                    config.insert(key, value);
                }
                Ok(Value::Object(config))
            }),
        }
    }
}
