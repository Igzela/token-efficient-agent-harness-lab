use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RoutingPolicy {
    pub policy_id: String,
    pub tier_map: serde_json::Value,
    pub description: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RoutingExperimentResult {
    pub policy_id: String,
    pub score: f64,
    pub pass_count: usize,
    pub total_count: usize,
}

pub struct RoutingExperimentManager;

impl Default for RoutingExperimentManager {
    fn default() -> Self {
        Self
    }
}

impl RoutingExperimentManager {
    pub fn new() -> Self {
        Self
    }

    pub fn evaluate_policy(
        &self,
        policy: &RoutingPolicy,
        test_cases: &[serde_json::Value],
    ) -> RoutingExperimentResult {
        let total = test_cases.len();
        let passed = test_cases
            .iter()
            .filter(|tc| {
                let task_type = tc.get("task_type").and_then(|v| v.as_str()).unwrap_or("");
                policy.tier_map.get(task_type).is_some()
            })
            .count();
        RoutingExperimentResult {
            policy_id: policy.policy_id.clone(),
            score: if total > 0 {
                passed as f64 / total as f64
            } else {
                0.0
            },
            pass_count: passed,
            total_count: total,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> RoutingPolicy {
        RoutingPolicy {
            policy_id: "p1".into(),
            tier_map: serde_json::json!({"code": "balanced"}),
            description: "test".into(),
        }
    }

    #[test]
    fn evaluate_all_match() {
        let r = RoutingExperimentManager::new()
            .evaluate_policy(&policy(), &[serde_json::json!({"task_type": "code"})]);
        assert_eq!(r.pass_count, 1);
    }

    #[test]
    fn evaluate_no_match() {
        let r = RoutingExperimentManager::new()
            .evaluate_policy(&policy(), &[serde_json::json!({"task_type": "unknown"})]);
        assert_eq!(r.pass_count, 0);
    }

    #[test]
    fn evaluate_empty() {
        let r = RoutingExperimentManager::new().evaluate_policy(&policy(), &[]);
        assert_eq!(r.score, 0.0);
    }

    #[test]
    fn result_serializes() {
        let v =
            serde_json::to_value(&RoutingExperimentManager::new().evaluate_policy(&policy(), &[]))
                .unwrap();
        assert_eq!(v["policy_id"], "p1");
    }

    #[test]
    fn policy_serializes() {
        let v = serde_json::to_value(&policy()).unwrap();
        assert_eq!(v["policy_id"], "p1");
    }
}
