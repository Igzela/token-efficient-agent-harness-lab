use serde::{Deserialize, Serialize};
use std::path::Path;

pub const APP_PLANS_SCHEMA_VERSION: &str = "app_plans.v1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlanStoreError {
    pub message: String,
}

impl PlanStoreError {
    pub fn new(message: &str) -> Self {
        Self { message: message.to_string() }
    }
}

impl std::fmt::Display for PlanStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PlanStoreError: {}", self.message)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlanStoreData {
    pub schema_version: String,
    pub plans: Vec<serde_json::Value>,
}

impl Default for PlanStoreData {
    fn default() -> Self {
        Self {
            schema_version: APP_PLANS_SCHEMA_VERSION.to_string(),
            plans: Vec::new(),
        }
    }
}

pub fn load_plans(path: &Path) -> Result<PlanStoreData, PlanStoreError> {
    if !path.exists() {
        return Ok(PlanStoreData::default());
    }
    let text = std::fs::read_to_string(path)
        .map_err(|_| PlanStoreError::new("plan store is unreadable or invalid JSON"))?;
    let data: PlanStoreData = serde_json::from_str(&text)
        .map_err(|_| PlanStoreError::new("plan store is unreadable or invalid JSON"))?;
    if data.schema_version != APP_PLANS_SCHEMA_VERSION {
        return Err(PlanStoreError::new("unsupported plan store schema version"));
    }
    Ok(data)
}

pub fn save_plan(path: &Path, plan: &serde_json::Value) -> Result<PlanStoreData, PlanStoreError> {
    let mut data = load_plans(path)?;
    data.plans.push(plan.clone());
    atomic_write_json(path, &data)?;
    Ok(data)
}

pub fn get_plan(path: &Path, plan_id: &str) -> Result<Option<serde_json::Value>, PlanStoreError> {
    let data = load_plans(path)?;
    for plan in &data.plans {
        if let Some(id) = plan.get("plan_id").and_then(|v| v.as_str()) {
            if id == plan_id {
                return Ok(Some(plan.clone()));
            }
        }
    }
    Ok(None)
}

fn atomic_write_json(path: &Path, data: &PlanStoreData) -> Result<(), PlanStoreError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|_| PlanStoreError::new("cannot create plan store directory"))?;
    }
    let tmp_path = path.with_extension("tmp");
    let json = serde_json::to_string_pretty(data)
        .map_err(|_| PlanStoreError::new("cannot serialize plan store"))?;
    std::fs::write(&tmp_path, format!("{}\n", json))
        .map_err(|_| PlanStoreError::new("cannot write plan store tmp file"))?;
    std::fs::rename(&tmp_path, path)
        .map_err(|_| PlanStoreError::new("cannot replace plan store file"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_plans_missing_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("plans.json");
        let data = load_plans(&path).unwrap();
        assert_eq!(data.schema_version, APP_PLANS_SCHEMA_VERSION);
        assert!(data.plans.is_empty());
    }

    #[test]
    fn save_and_get_plan() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("plans.json");
        let plan = serde_json::json!({"plan_id": "p-001", "name": "test"});
        save_plan(&path, &plan).unwrap();
        let found = get_plan(&path, "p-001").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap()["plan_id"], "p-001");
    }

    #[test]
    fn get_plan_not_found() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("plans.json");
        let found = get_plan(&path, "nonexistent").unwrap();
        assert!(found.is_none());
    }

    #[test]
    fn save_multiple_plans() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("plans.json");
        save_plan(&path, &serde_json::json!({"plan_id": "p-001"})).unwrap();
        save_plan(&path, &serde_json::json!({"plan_id": "p-002"})).unwrap();
        let data = load_plans(&path).unwrap();
        assert_eq!(data.plans.len(), 2);
    }

    #[test]
    fn load_plans_bad_schema_version() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("plans.json");
        std::fs::write(&path, r#"{"schema_version": "wrong", "plans": []}"#).unwrap();
        let err = load_plans(&path).unwrap_err();
        assert!(err.message.contains("unsupported"));
    }

    #[test]
    fn atomic_write_no_corruption() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("plans.json");
        for i in 0..10 {
            save_plan(&path, &serde_json::json!({"plan_id": format!("p-{:03}", i)})).unwrap();
        }
        let data = load_plans(&path).unwrap();
        assert_eq!(data.plans.len(), 10);
    }
}
