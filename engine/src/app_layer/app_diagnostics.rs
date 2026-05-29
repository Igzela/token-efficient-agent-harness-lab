use serde_json::json;

pub const APP_DIAGNOSTICS_SCHEMA_VERSION: &str = "app_diagnostics.v1";
pub const APP_STATUS_SCHEMA_VERSION: &str = "app_status.v1";
pub const DIAGNOSTICS_BOUNDARY_NOTICE: &str = "Operations diagnostics are read-only.";

pub fn build_app_status(registry_path: &str, plans_path: &str) -> serde_json::Value {
    let now = chrono::Utc::now().to_rfc3339();
    let registry_ok = std::path::Path::new(registry_path).exists();
    let plans_ok = std::path::Path::new(plans_path).exists();
    let overall = if registry_ok && plans_ok { "healthy" } else { "degraded" };
    json!({
        "schema_version": APP_STATUS_SCHEMA_VERSION,
        "status": overall,
        "mode": "local_read_only_control_plane",
        "last_checked": now,
        "component_count": 2,
        "components": [
            {"component_id": "app_registry", "status": if registry_ok { "healthy" } else { "not_configured" }},
            {"component_id": "plan_store", "status": if plans_ok { "healthy" } else { "not_configured" }}
        ],
        "boundary_notice": DIAGNOSTICS_BOUNDARY_NOTICE,
    })
}

pub fn build_app_diagnostics() -> serde_json::Value {
    json!({
        "schema_version": APP_DIAGNOSTICS_SCHEMA_VERSION,
        "notice": DIAGNOSTICS_BOUNDARY_NOTICE,
        "capabilities": ["status", "diagnostics", "recent_errors"],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_degraded_when_missing() {
        let s = build_app_status("/nonexistent/r.json", "/nonexistent/p.json");
        assert_eq!(s["status"], "degraded");
    }

    #[test]
    fn status_has_components() {
        let s = build_app_status("/x", "/y");
        assert_eq!(s["components"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn diagnostics_schema_version() {
        let d = build_app_diagnostics();
        assert_eq!(d["schema_version"], APP_DIAGNOSTICS_SCHEMA_VERSION);
    }

    #[test]
    fn diagnostics_has_notice() {
        let d = build_app_diagnostics();
        assert!(d["notice"].as_str().unwrap().contains("read-only"));
    }

    #[test]
    fn status_with_real_paths() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("r.json"), "{}").unwrap();
        std::fs::write(dir.path().join("p.json"), "{}").unwrap();
        let s = build_app_status(
            dir.path().join("r.json").to_str().unwrap(),
            dir.path().join("p.json").to_str().unwrap(),
        );
        assert_eq!(s["status"], "healthy");
    }
}
