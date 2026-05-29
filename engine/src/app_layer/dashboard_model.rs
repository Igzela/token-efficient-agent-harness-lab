use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DashboardSnapshot {
    pub schema_version: String,
    pub generated_at: String,
    pub dag_id: String,
    pub dag_version: i64,
    pub node_count: usize,
    pub edge_count: usize,
    pub healthy: bool,
    pub component_count: usize,
}

pub fn build_snapshot(
    dag_id: &str, dag_version: i64, node_count: usize, edge_count: usize,
    healthy: bool, component_count: usize,
) -> DashboardSnapshot {
    DashboardSnapshot {
        schema_version: "dashboard_snapshot.v1".to_string(),
        generated_at: chrono::Utc::now().to_rfc3339(),
        dag_id: dag_id.to_string(),
        dag_version,
        node_count,
        edge_count,
        healthy,
        component_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_and_serialize() {
        let s = build_snapshot("dag-1", 3, 5, 4, true, 2);
        assert_eq!(s.dag_id, "dag-1");
        assert!(s.healthy);
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["node_count"], 5);
    }

    #[test]
    fn snapshot_unhealthy() {
        let s = build_snapshot("d", 1, 0, 0, false, 0);
        assert!(!s.healthy);
    }

    #[test]
    fn snapshot_schema_version() {
        let s = build_snapshot("d", 1, 0, 0, true, 1);
        assert_eq!(s.schema_version, "dashboard_snapshot.v1");
    }

    #[test]
    fn snapshot_has_timestamp() {
        let s = build_snapshot("d", 1, 0, 0, true, 0);
        assert!(!s.generated_at.is_empty());
    }

    #[test]
    fn snapshot_roundtrip() {
        let s = build_snapshot("d1", 2, 3, 4, true, 5);
        let v = serde_json::to_value(&s).unwrap();
        let d: DashboardSnapshot = serde_json::from_value(v).unwrap();
        assert_eq!(d, s);
    }
}
