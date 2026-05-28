use std::collections::HashMap;

use super::auto_policies::{AutoDowngradePolicy, AutoUpgradePolicy};
use super::history_store::RoutingHistoryStore;
use super::promotion_gate::RoutingObservationStore;
use super::schemas::{
    make_task_group, parse_task_group, RoutingObservation, ROUTING_OBSERVATION_SCHEMA_VERSION,
};

pub struct FeedbackIntegrator {
    downgrade: Option<AutoDowngradePolicy>,
    upgrade: Option<AutoUpgradePolicy>,
}

impl FeedbackIntegrator {
    pub fn new(downgrade: Option<AutoDowngradePolicy>, upgrade: Option<AutoUpgradePolicy>) -> Self {
        Self { downgrade, upgrade }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_outcome(
        &self,
        observation_store: &mut RoutingObservationStore,
        dispatch_id: &str,
        task_domain: &str,
        task_intent: &str,
        selected_tier: &str,
        baseline_tier: &str,
        quality_score: f64,
        cost: f64,
        latency_ms: i64,
        success: bool,
        failure_domain: Option<String>,
        budget_violation: bool,
        runtime: &mut crate::runtime::FixtureRuntime,
    ) -> RoutingObservation {
        let obs = RoutingObservation {
            schema_version: ROUTING_OBSERVATION_SCHEMA_VERSION.to_string(),
            observation_id: runtime.id("obs"),
            arm_id: format!("arm-{selected_tier}"),
            dispatch_id: dispatch_id.to_string(),
            task_domain: task_domain.to_string(),
            task_intent: task_intent.to_string(),
            selected_tier: selected_tier.to_string(),
            baseline_tier: baseline_tier.to_string(),
            quality_score,
            cost,
            latency_ms,
            success,
            failure_domain,
            budget_violation,
            observed_at: runtime.now(),
        };
        observation_store.add_observation(obs.clone());
        obs
    }

    pub fn should_adapt(
        &self,
        observation_store: &RoutingObservationStore,
        history_store: &mut RoutingHistoryStore,
        task_group: &str,
        current_tier: &str,
    ) -> (bool, String) {
        let (domain, intent) = parse_task_group(task_group);

        let obs: Vec<&RoutingObservation> =
            observation_store.observations_for_tier_and_group(current_tier, &domain, &intent);
        if obs.is_empty() {
            return (false, "no_observations".to_string());
        }

        let quality_score: f64 =
            obs.iter().map(|o| o.quality_score).sum::<f64>() / obs.len() as f64;
        let failure_count = obs.iter().filter(|o| !o.success).count();
        let failure_rate = failure_count as f64 / obs.len() as f64;

        if let Some(ref upgrade) = self.upgrade {
            for tier in &["strong_planner", "balanced_worker", "cheap_executor"] {
                if *tier == current_tier {
                    continue;
                }
                let (should, reason) = upgrade.should_upgrade(
                    task_group,
                    current_tier,
                    tier,
                    quality_score,
                    failure_rate,
                    "low",
                );
                if should {
                    return (true, format!("upgrade_to_{tier}:{reason}"));
                }
            }
        }

        if let Some(ref downgrade) = self.downgrade {
            for tier in &["cheap_executor"] {
                if *tier == current_tier {
                    continue;
                }
                let cop = history_store
                    .aggregate_by_tier_and_task_group(tier, task_group)
                    .and_then(|agg| agg.cost_of_pass);
                let (should, reason) = downgrade.should_downgrade(
                    task_group,
                    current_tier,
                    tier,
                    quality_score,
                    cop,
                    history_store,
                );
                if should {
                    return (true, format!("downgrade_to_{tier}:{reason}"));
                }
            }
        }

        (false, "no_adaptation_needed".to_string())
    }

    pub fn summary(
        &self,
        observation_store: &RoutingObservationStore,
        task_group: &str,
    ) -> HashMap<String, serde_json::Value> {
        let (domain, intent) = parse_task_group(task_group);

        let all_obs: Vec<&RoutingObservation> = observation_store
            .all_observations()
            .iter()
            .filter(|o| o.task_domain == domain && o.task_intent == intent)
            .collect();

        let mut result = HashMap::new();
        result.insert(
            "task_group".to_string(),
            serde_json::Value::String(task_group.to_string()),
        );

        if all_obs.is_empty() {
            result.insert("sample_count".to_string(), serde_json::json!(0));
            result.insert("tiers".to_string(), serde_json::json!([]));
            result.insert("best_tier".to_string(), serde_json::Value::Null);
            result.insert("avg_quality".to_string(), serde_json::json!(0.0));
            result.insert("avg_cost".to_string(), serde_json::json!(0.0));
            return result;
        }

        let mut tier_quality: HashMap<String, Vec<f64>> = HashMap::new();
        let mut tier_cost: HashMap<String, Vec<f64>> = HashMap::new();
        for o in &all_obs {
            tier_quality
                .entry(o.selected_tier.clone())
                .or_default()
                .push(o.quality_score);
            tier_cost
                .entry(o.selected_tier.clone())
                .or_default()
                .push(o.cost);
        }

        let best_tier = tier_quality
            .iter()
            .max_by(|a, b| {
                let avg_a: f64 = a.1.iter().sum::<f64>() / a.1.len() as f64;
                let avg_b: f64 = b.1.iter().sum::<f64>() / b.1.len() as f64;
                avg_a
                    .partial_cmp(&avg_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(k, _)| k.clone())
            .unwrap_or_default();

        let mut tiers: Vec<String> = tier_quality.keys().cloned().collect();
        tiers.sort();

        let avg_quality: f64 =
            all_obs.iter().map(|o| o.quality_score).sum::<f64>() / all_obs.len() as f64;
        let avg_cost: f64 = all_obs.iter().map(|o| o.cost).sum::<f64>() / all_obs.len() as f64;

        result.insert("sample_count".to_string(), serde_json::json!(all_obs.len()));
        result.insert("tiers".to_string(), serde_json::json!(tiers));
        result.insert(
            "best_tier".to_string(),
            serde_json::Value::String(best_tier),
        );
        result.insert("avg_quality".to_string(), serde_json::json!(avg_quality));
        result.insert("avg_cost".to_string(), serde_json::json!(avg_cost));
        result
    }

    pub fn task_group_for(dispatch_domain: &str, dispatch_intent: &str) -> String {
        make_task_group(dispatch_domain, dispatch_intent)
    }
}
