use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::LocalProductStore;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchedulerHeartbeatRow {
    pub last_heartbeat_at: String,
    pub tick_count: u64,
    pub error_count: u64,
    pub uptime_seconds: f64,
    pub metadata_json: String,
    pub updated_at: String,
}

impl LocalProductStore {
    pub fn write_heartbeat(
        &self,
        tick_count: u64,
        error_count: u64,
        uptime_seconds: f64,
        metadata_json: &str,
    ) -> Result<(), String> {
        let now = self.now();
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE scheduler_heartbeat
                 SET last_heartbeat_at = ?1,
                     tick_count = ?2,
                     error_count = ?3,
                     uptime_seconds = ?4,
                     metadata_json = ?5,
                     updated_at = ?6
                 WHERE id = 1",
                params![
                    now,
                    tick_count as i64,
                    error_count as i64,
                    uptime_seconds,
                    metadata_json,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })
    }

    pub fn read_heartbeat(&self) -> Result<Option<SchedulerHeartbeatRow>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT last_heartbeat_at, tick_count, error_count,
                            uptime_seconds, metadata_json, updated_at
                     FROM scheduler_heartbeat WHERE id = 1",
                )
                .map_err(|e| e.to_string())?;

            let mut rows = stmt
                .query_map([], |row| {
                    let tick_i64: i64 = row.get(1)?;
                    let err_i64: i64 = row.get(2)?;
                    Ok(SchedulerHeartbeatRow {
                        last_heartbeat_at: row.get(0)?,
                        tick_count: tick_i64 as u64,
                        error_count: err_i64 as u64,
                        uptime_seconds: row.get(3)?,
                        metadata_json: row.get(4)?,
                        updated_at: row.get(5)?,
                    })
                })
                .map_err(|e| e.to_string())?;

            match rows.next() {
                Some(row) => Ok(Some(row.map_err(|e| e.to_string())?)),
                None => Ok(None),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heartbeat_roundtrip() {
        let store = LocalProductStore::new(":memory:").unwrap();
        let seeded = store
            .read_heartbeat()
            .unwrap()
            .expect("migration seeds row");
        assert_eq!(seeded.tick_count, 0);
        assert_eq!(seeded.error_count, 0);

        store
            .write_heartbeat(42, 3, 120.5, r#"{"key":"value"}"#)
            .unwrap();

        let row = store
            .read_heartbeat()
            .unwrap()
            .expect("heartbeat should exist");
        assert_eq!(row.tick_count, 42);
        assert_eq!(row.error_count, 3);
        assert!((row.uptime_seconds - 120.5).abs() < f64::EPSILON);
        assert_eq!(row.metadata_json, r#"{"key":"value"}"#);
        assert!(!row.last_heartbeat_at.is_empty());
        assert!(!row.updated_at.is_empty());
    }

    #[test]
    fn test_heartbeat_update_overwrites() {
        let store = LocalProductStore::new(":memory:").unwrap();

        store.write_heartbeat(10, 1, 50.0, "{}").unwrap();
        store.write_heartbeat(20, 2, 100.0, "{}").unwrap();

        let row = store.read_heartbeat().unwrap().unwrap();
        assert_eq!(row.tick_count, 20);
        assert_eq!(row.error_count, 2);
    }
}
