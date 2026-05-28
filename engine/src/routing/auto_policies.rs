use crate::dispatch_decision::MODEL_TIERS;
use std::collections::HashMap;

use super::history_store::RoutingHistoryStore;

fn tier_cost_order() -> HashMap<String, usize> {
    MODEL_TIERS
        .iter()
        .enumerate()
        .map(|(i, t)| (t.to_string(), i))
        .collect()
}

#[derive(Debug, Clone)]
pub struct AutoDowngradePolicy {
    pub policy_id: String,
    pub quality_risk_threshold: f64,
    pub min_quality_score: f64,
    pub min_sample_count: usize,
    pub description: String,
}

impl AutoDowngradePolicy {
    pub fn new(policy_id: &str) -> Self {
        Self {
            policy_id: policy_id.to_string(),
            quality_risk_threshold: 0.1,
            min_quality_score: 0.7,
            min_sample_count: 30,
            description: String::new(),
        }
    }

    pub fn should_downgrade(
        &self,
        task_group: &str,
        current_tier: &str,
        candidate_tier: &str,
        quality_score: f64,
        _cost_of_pass: Option<f64>,
        history_store: &mut RoutingHistoryStore,
    ) -> (bool, String) {
        if quality_score < self.min_quality_score {
            return (false, "quality_score_below_threshold".to_string());
        }

        let order = tier_cost_order();
        let current_idx = order.get(current_tier).copied().unwrap_or(1);
        let candidate_idx = order.get(candidate_tier).copied().unwrap_or(0);
        if candidate_idx >= current_idx {
            return (false, "candidate_not_cheaper".to_string());
        }

        let sample_count = history_store.sample_count(task_group);
        if sample_count < self.min_sample_count {
            return (false, "insufficient_samples".to_string());
        }

        (true, "cost_optimization".to_string())
    }
}

#[derive(Debug, Clone)]
pub struct AutoUpgradePolicy {
    pub policy_id: String,
    pub uncertainty_threshold: f64,
    pub failure_rate_threshold: f64,
    pub description: String,
}

impl AutoUpgradePolicy {
    pub fn new(policy_id: &str) -> Self {
        Self {
            policy_id: policy_id.to_string(),
            uncertainty_threshold: 0.4,
            failure_rate_threshold: 0.2,
            description: String::new(),
        }
    }

    pub fn should_upgrade(
        &self,
        _task_group: &str,
        current_tier: &str,
        candidate_tier: &str,
        quality_score: f64,
        failure_rate: f64,
        risk_level: &str,
    ) -> (bool, String) {
        let order = tier_cost_order();
        let current_idx = order.get(current_tier).copied().unwrap_or(1);
        let candidate_idx = order.get(candidate_tier).copied().unwrap_or(2);
        if candidate_idx <= current_idx {
            return (false, "candidate_not_stronger".to_string());
        }

        if risk_level == "critical" {
            return (true, "critical_task".to_string());
        }

        if failure_rate > self.failure_rate_threshold {
            return (true, "failure_rate".to_string());
        }

        if quality_score < self.uncertainty_threshold {
            return (true, "high_uncertainty".to_string());
        }

        (false, "no_upgrade_needed".to_string())
    }
}
