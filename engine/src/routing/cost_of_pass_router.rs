use super::history_store::RoutingHistoryStore;

pub struct CostOfPassRouter {
    min_samples: usize,
    _min_cost_reduction: f64,
}

impl CostOfPassRouter {
    pub fn new(min_sample_count: usize, min_cost_reduction_pct: f64) -> Self {
        Self {
            min_samples: min_sample_count,
            _min_cost_reduction: min_cost_reduction_pct,
        }
    }

    pub fn best_tier_for_task_group(
        &self,
        store: &mut RoutingHistoryStore,
        task_group: &str,
    ) -> Option<(String, f64)> {
        let tiers = store.tiers_observed(task_group);
        if tiers.is_empty() {
            return None;
        }

        let mut best_tier: Option<String> = None;
        let mut best_cop: Option<f64> = None;

        for tier in &tiers {
            if let Some(agg) = store.aggregate_by_tier_and_task_group(tier, task_group) {
                if let Some(cop) = agg.cost_of_pass {
                    if agg.total_count < self.min_samples {
                        continue;
                    }
                    if best_cop.is_none() || Some(cop) < best_cop {
                        best_cop = Some(cop);
                        best_tier = Some(tier.clone());
                    }
                }
            }
        }

        match (best_tier, best_cop) {
            (Some(t), Some(c)) => Some((t, c)),
            _ => None,
        }
    }

    pub fn can_route_adaptively(&self, store: &mut RoutingHistoryStore, task_group: &str) -> bool {
        self.best_tier_for_task_group(store, task_group).is_some()
    }

    pub fn cost_comparison(
        &self,
        store: &mut RoutingHistoryStore,
        task_group: &str,
        tier_a: &str,
        tier_b: &str,
    ) -> Option<(f64, f64, f64)> {
        let agg_a = store.aggregate_by_tier_and_task_group(tier_a, task_group)?;
        let agg_b = store.aggregate_by_tier_and_task_group(tier_b, task_group)?;
        let cop_a = agg_a.cost_of_pass?;
        let cop_b = agg_b.cost_of_pass?;
        if cop_b == 0.0 {
            return None;
        }
        let delta_pct = ((cop_a - cop_b) / cop_b) * 100.0;
        Some((cop_a, cop_b, delta_pct))
    }

    pub fn failure_rate(
        &self,
        store: &mut RoutingHistoryStore,
        tier: &str,
        task_group: &str,
    ) -> f64 {
        match store.aggregate_by_tier_and_task_group(tier, task_group) {
            Some(agg) if agg.total_count > 0 => agg.failure_count as f64 / agg.total_count as f64,
            _ => 0.0,
        }
    }

    pub fn tier_cost_of_pass(
        &self,
        store: &mut RoutingHistoryStore,
        tier: &str,
        task_group: &str,
    ) -> Option<f64> {
        store
            .aggregate_by_tier_and_task_group(tier, task_group)
            .and_then(|agg| agg.cost_of_pass)
    }
}
