use crate::dispatch_engine::DispatchEngine;
use crate::storage::durable_store::DurableStore;
use crate::storage::health_checker::HealthChecker;

pub const SDK_SCHEMA_VERSION: &str = "sdk.v1";

pub struct HarnessSDK {
    store: DurableStore,
    engine: DispatchEngine,
}

impl HarnessSDK {
    pub fn new(store_path: Option<&str>) -> Result<Self, String> {
        let store = match store_path {
            Some(path) => DurableStore::new(path)?,
            None => DurableStore::new_memory()?,
        };
        let engine = DispatchEngine::new();
        Ok(Self { store, engine })
    }

    pub fn create_dispatch(&self, raw_request: &str, request_source: &str) -> serde_json::Value {
        self.engine.dispatch(raw_request, request_source)
    }

    pub fn list_plans(&self) -> Result<Vec<serde_json::Value>, String> {
        let records = self.store.list_plans()?;
        Ok(records
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.record_id,
                    "created_at": r.created_at,
                    "schema_version": r.schema_version,
                    "data": r.data,
                })
            })
            .collect())
    }

    pub fn get_plan(&self, plan_id: &str) -> Result<Option<serde_json::Value>, String> {
        let record = self.store.get_plan(plan_id)?;
        Ok(record.map(|r| {
            serde_json::json!({
                "id": r.record_id,
                "created_at": r.created_at,
                "schema_version": r.schema_version,
                "data": r.data,
            })
        }))
    }

    pub fn health_check(&self, now: f64) -> serde_json::Value {
        let checker = HealthChecker::new(Some(&self.store));
        let report = checker.health(now);
        serde_json::json!({
            "status": report.status,
            "checks": report.checks.iter().map(|c| {
                serde_json::json!({
                    "name": c.name,
                    "status": c.status,
                    "message": c.message,
                    "latency_ms": c.latency_ms,
                })
            }).collect::<Vec<_>>(),
            "timestamp": report.timestamp,
        })
    }

    pub fn get_status(&self, now: f64) -> Result<serde_json::Value, String> {
        let health = self.health_check(now);
        let stats = self.store.stats()?;
        Ok(serde_json::json!({
            "schema_version": SDK_SCHEMA_VERSION,
            "health": health,
            "storage": stats,
            "timestamp": now,
        }))
    }

    pub fn store(&self) -> &DurableStore {
        &self.store
    }

    pub fn close(&self) -> Result<(), String> {
        self.store.close()
    }
}
