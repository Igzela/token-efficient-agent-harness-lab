use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use super::durable_store::DurableStore;

pub const STORAGE_MIGRATOR_SCHEMA_VERSION: &str = "storage_migrator.v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MigrationReport {
    pub source: String,
    pub target: String,
    pub records_migrated: i64,
    pub errors: Vec<String>,
    pub duration_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FullMigrationReport {
    pub plans: MigrationReport,
    pub repos: MigrationReport,
    pub events: MigrationReport,
    pub total_duration_ms: f64,
}

fn read_json_file(path: &Path) -> Option<serde_json::Value> {
    if !path.exists() {
        return None;
    }
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

fn read_jsonl_file(path: &Path) -> (Vec<serde_json::Value>, Vec<String>) {
    if !path.exists() {
        return (Vec::new(), Vec::new());
    }
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return (Vec::new(), Vec::new()),
    };
    let mut records = Vec::new();
    let mut errors = Vec::new();
    for (line_num, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<serde_json::Value>(line) {
            Ok(v) => records.push(v),
            Err(e) => errors.push(format!("line {}: {}", line_num, e)),
        }
    }
    (records, errors)
}

pub fn migrate_plans_json_to_sqlite(
    json_path: &Path,
    store: &DurableStore,
    start: f64,
    now: f64,
) -> MigrationReport {
    let mut errors = Vec::new();
    let mut migrated = 0i64;

    let data = match read_json_file(json_path) {
        Some(d) => d,
        None => {
            return MigrationReport {
                source: json_path.display().to_string(),
                target: "sqlite".to_string(),
                records_migrated: 0,
                errors: vec!["file not found or invalid".to_string()],
                duration_ms: (now - start) * 1000.0,
            }
        }
    };

    if let Some(plans) = data.get("plans").and_then(|v| v.as_array()) {
        for plan in plans {
            let plan_id = match plan.get("plan_id").and_then(|v| v.as_str()) {
                Some(id) => id.to_string(),
                None => {
                    errors.push(format!(
                        "plan missing plan_id: {}",
                        serde_json::to_string(plan)
                            .map(|s| s.chars().take(100).collect::<String>())
                            .unwrap_or_default()
                    ));
                    continue;
                }
            };
            let sv = plan
                .get("schema_version")
                .and_then(|v| v.as_str())
                .map(String::from);
            match store.save_plan(&plan_id, plan, sv.as_deref(), None, true) {
                Ok(_) => migrated += 1,
                Err(e) => errors.push(format!("plan {plan_id}: {e}")),
            }
        }
    }

    MigrationReport {
        source: json_path.display().to_string(),
        target: "sqlite".to_string(),
        records_migrated: migrated,
        errors,
        duration_ms: (now - start) * 1000.0,
    }
}

pub fn migrate_repos_json_to_sqlite(
    json_path: &Path,
    store: &DurableStore,
    start: f64,
    now: f64,
) -> MigrationReport {
    let mut errors = Vec::new();
    let mut migrated = 0i64;

    let data = match read_json_file(json_path) {
        Some(d) => d,
        None => {
            return MigrationReport {
                source: json_path.display().to_string(),
                target: "sqlite".to_string(),
                records_migrated: 0,
                errors: vec!["file not found or invalid".to_string()],
                duration_ms: (now - start) * 1000.0,
            }
        }
    };

    if let Some(repos) = data.get("repos").and_then(|v| v.as_array()) {
        for repo in repos {
            let repo_id = match repo.get("id").and_then(|v| v.as_str()) {
                Some(id) => id.to_string(),
                None => {
                    errors.push(format!(
                        "repo missing id: {}",
                        serde_json::to_string(repo)
                            .map(|s| s.chars().take(100).collect::<String>())
                            .unwrap_or_default()
                    ));
                    continue;
                }
            };
            let sv = repo
                .get("schema_version")
                .and_then(|v| v.as_str())
                .map(String::from);
            match store.save_repo(&repo_id, repo, sv.as_deref(), None, true) {
                Ok(_) => migrated += 1,
                Err(e) => errors.push(format!("repo {repo_id}: {e}")),
            }
        }
    }

    MigrationReport {
        source: json_path.display().to_string(),
        target: "sqlite".to_string(),
        records_migrated: migrated,
        errors,
        duration_ms: (now - start) * 1000.0,
    }
}

pub fn migrate_events_jsonl_to_sqlite(
    jsonl_path: &Path,
    store: &DurableStore,
    start: f64,
    now: f64,
) -> MigrationReport {
    let (events, mut errors) = read_jsonl_file(jsonl_path);
    let mut migrated = 0i64;

    if events.is_empty() {
        return MigrationReport {
            source: jsonl_path.display().to_string(),
            target: "sqlite".to_string(),
            records_migrated: 0,
            errors,
            duration_ms: (now - start) * 1000.0,
        };
    }

    for event in &events {
        let event_id = match event.get("event_id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => {
                errors.push(format!(
                    "event missing event_id: {}",
                    serde_json::to_string(event)
                        .map(|s| s.chars().take(100).collect::<String>())
                        .unwrap_or_default()
                ));
                continue;
            }
        };
        let sv = event
            .get("schema_version")
            .and_then(|v| v.as_str())
            .map(String::from);
        match store.save_event(&event_id, event, sv.as_deref(), None, true) {
            Ok(_) => migrated += 1,
            Err(e) => errors.push(format!("event {event_id}: {e}")),
        }
    }

    MigrationReport {
        source: jsonl_path.display().to_string(),
        target: "sqlite".to_string(),
        records_migrated: migrated,
        errors,
        duration_ms: (now - start) * 1000.0,
    }
}

pub fn full_migration(
    plans_json: &Path,
    repos_json: &Path,
    events_jsonl: &Path,
    store: &DurableStore,
    start: f64,
    now: f64,
) -> FullMigrationReport {
    let plans_report = migrate_plans_json_to_sqlite(plans_json, store, start, now);
    let repos_report = migrate_repos_json_to_sqlite(repos_json, store, start, now);
    let events_report = migrate_events_jsonl_to_sqlite(events_jsonl, store, start, now);
    FullMigrationReport {
        plans: plans_report,
        repos: repos_report,
        events: events_report,
        total_duration_ms: (now - start) * 1000.0,
    }
}
