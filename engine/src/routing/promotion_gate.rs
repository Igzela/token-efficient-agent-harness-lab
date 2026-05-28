use super::schemas::{
    parse_task_group, PromotionVerdict, RoutingObservation, PROMOTION_GATE_MAX_FAILURE_RATE_DELTA,
    PROMOTION_GATE_MIN_COST_REDUCTION_PCT, PROMOTION_GATE_MIN_SAMPLE_COUNT,
    PROMOTION_VERDICT_SCHEMA_VERSION,
};

#[derive(Debug, Default)]
pub struct RoutingObservationStore {
    observations: Vec<RoutingObservation>,
    by_arm: std::collections::HashMap<String, Vec<usize>>,
}

impl RoutingObservationStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_observation(&mut self, obs: RoutingObservation) {
        let idx = self.observations.len();
        self.by_arm.entry(obs.arm_id.clone()).or_default().push(idx);
        self.observations.push(obs);
    }

    pub fn observations_for_arm(&self, arm_id: &str) -> Vec<&RoutingObservation> {
        self.by_arm
            .get(arm_id)
            .map(|indices| indices.iter().map(|&i| &self.observations[i]).collect())
            .unwrap_or_default()
    }

    pub fn observations_for_tier_and_group(
        &self,
        tier: &str,
        task_domain: &str,
        task_intent: &str,
    ) -> Vec<&RoutingObservation> {
        self.observations
            .iter()
            .filter(|o| {
                o.selected_tier == tier
                    && o.task_domain == task_domain
                    && o.task_intent == task_intent
            })
            .collect()
    }

    pub fn count_by_arm(&self, arm_id: &str) -> usize {
        self.by_arm.get(arm_id).map(|v| v.len()).unwrap_or(0)
    }

    pub fn count_for_tier_and_group(
        &self,
        tier: &str,
        task_domain: &str,
        task_intent: &str,
    ) -> usize {
        self.observations_for_tier_and_group(tier, task_domain, task_intent)
            .len()
    }

    pub fn all_observations(&self) -> &[RoutingObservation] {
        &self.observations
    }

    pub fn total_count(&self) -> usize {
        self.observations.len()
    }
}

pub struct PromotionGate {
    min_samples: usize,
    max_failure_delta: f64,
    min_cost_reduction: f64,
    require_human: bool,
}

impl PromotionGate {
    pub fn new(
        min_sample_count: Option<usize>,
        max_failure_rate_delta: Option<f64>,
        min_cost_reduction_pct: Option<f64>,
        require_human_review: bool,
    ) -> Self {
        Self {
            min_samples: min_sample_count.unwrap_or(PROMOTION_GATE_MIN_SAMPLE_COUNT),
            max_failure_delta: max_failure_rate_delta
                .unwrap_or(PROMOTION_GATE_MAX_FAILURE_RATE_DELTA),
            min_cost_reduction: min_cost_reduction_pct
                .unwrap_or(PROMOTION_GATE_MIN_COST_REDUCTION_PCT),
            require_human: require_human_review,
        }
    }

    pub fn evaluate(
        &self,
        store: &RoutingObservationStore,
        task_group: &str,
        candidate_tier: &str,
        baseline_tier: &str,
    ) -> PromotionVerdict {
        let (domain, intent) = parse_task_group(task_group);

        let sample_count = store.count_for_tier_and_group(candidate_tier, &domain, &intent);
        let baseline_count = store.count_for_tier_and_group(baseline_tier, &domain, &intent);
        let quality_delta =
            self.quality_delta(store, candidate_tier, baseline_tier, &domain, &intent);
        let cost_reduction =
            self.cost_reduction(store, candidate_tier, baseline_tier, &domain, &intent);
        let failure_delta =
            self.failure_rate_delta(store, candidate_tier, baseline_tier, &domain, &intent);

        let mut reasons: Vec<String> = Vec::new();

        if sample_count < self.min_samples {
            reasons.push(format!(
                "insufficient_samples:{sample_count}<{}",
                self.min_samples
            ));
            return self.build_verdict(
                "insufficient_data",
                task_group,
                candidate_tier,
                baseline_tier,
                sample_count,
                quality_delta,
                cost_reduction,
                failure_delta,
                reasons,
                false,
            );
        }

        if baseline_count < self.min_samples {
            reasons.push(format!(
                "insufficient_baseline_samples:{baseline_count}<{}",
                self.min_samples
            ));
            return self.build_verdict(
                "insufficient_data",
                task_group,
                candidate_tier,
                baseline_tier,
                sample_count,
                quality_delta,
                cost_reduction,
                failure_delta,
                reasons,
                false,
            );
        }

        if quality_delta < 0.0 {
            reasons.push(format!("quality_regression:{quality_delta:.4}"));
            return self.build_verdict(
                "hold",
                task_group,
                candidate_tier,
                baseline_tier,
                sample_count,
                quality_delta,
                cost_reduction,
                failure_delta,
                reasons,
                false,
            );
        }

        if cost_reduction < self.min_cost_reduction {
            reasons.push(format!(
                "cost_reduction_below_threshold:{cost_reduction:.2}<{}",
                self.min_cost_reduction
            ));
            return self.build_verdict(
                "hold",
                task_group,
                candidate_tier,
                baseline_tier,
                sample_count,
                quality_delta,
                cost_reduction,
                failure_delta,
                reasons,
                false,
            );
        }

        if failure_delta > self.max_failure_delta {
            reasons.push(format!(
                "failure_rate_worse:{failure_delta:.4}>{}",
                self.max_failure_delta
            ));
            return self.build_verdict(
                "hold",
                task_group,
                candidate_tier,
                baseline_tier,
                sample_count,
                quality_delta,
                cost_reduction,
                failure_delta,
                reasons,
                false,
            );
        }

        if self.require_human {
            reasons.push("human_review_required".to_string());
            return self.build_verdict(
                "hold",
                task_group,
                candidate_tier,
                baseline_tier,
                sample_count,
                quality_delta,
                cost_reduction,
                failure_delta,
                reasons,
                true,
            );
        }

        reasons.push("all_gate_conditions_met".to_string());
        self.build_verdict(
            "promote",
            task_group,
            candidate_tier,
            baseline_tier,
            sample_count,
            quality_delta,
            cost_reduction,
            failure_delta,
            reasons,
            false,
        )
    }

    pub fn check_sample_count(
        &self,
        store: &RoutingObservationStore,
        task_group: &str,
        tier: &str,
    ) -> (bool, usize) {
        let (domain, intent) = parse_task_group(task_group);
        let count = store.count_for_tier_and_group(tier, &domain, &intent);
        (count >= self.min_samples, count)
    }

    fn quality_delta(
        &self,
        store: &RoutingObservationStore,
        candidate: &str,
        baseline: &str,
        domain: &str,
        intent: &str,
    ) -> f64 {
        let c_obs = store.observations_for_tier_and_group(candidate, domain, intent);
        let b_obs = store.observations_for_tier_and_group(baseline, domain, intent);
        let c_avg = if c_obs.is_empty() {
            0.0
        } else {
            c_obs.iter().map(|o| o.quality_score).sum::<f64>() / c_obs.len() as f64
        };
        let b_avg = if b_obs.is_empty() {
            0.0
        } else {
            b_obs.iter().map(|o| o.quality_score).sum::<f64>() / b_obs.len() as f64
        };
        c_avg - b_avg
    }

    fn cost_reduction(
        &self,
        store: &RoutingObservationStore,
        candidate: &str,
        baseline: &str,
        domain: &str,
        intent: &str,
    ) -> f64 {
        let c_obs = store.observations_for_tier_and_group(candidate, domain, intent);
        let b_obs = store.observations_for_tier_and_group(baseline, domain, intent);
        let c_avg = if c_obs.is_empty() {
            0.0
        } else {
            c_obs.iter().map(|o| o.cost).sum::<f64>() / c_obs.len() as f64
        };
        let b_avg = if b_obs.is_empty() {
            0.0
        } else {
            b_obs.iter().map(|o| o.cost).sum::<f64>() / b_obs.len() as f64
        };
        if b_avg == 0.0 {
            return 0.0;
        }
        ((b_avg - c_avg) / b_avg) * 100.0
    }

    fn failure_rate_delta(
        &self,
        store: &RoutingObservationStore,
        candidate: &str,
        baseline: &str,
        domain: &str,
        intent: &str,
    ) -> f64 {
        let c_obs = store.observations_for_tier_and_group(candidate, domain, intent);
        let b_obs = store.observations_for_tier_and_group(baseline, domain, intent);
        let c_fail = if c_obs.is_empty() {
            0.0
        } else {
            c_obs.iter().filter(|o| !o.success).count() as f64 / c_obs.len() as f64
        };
        let b_fail = if b_obs.is_empty() {
            0.0
        } else {
            b_obs.iter().filter(|o| !o.success).count() as f64 / b_obs.len() as f64
        };
        c_fail - b_fail
    }

    #[allow(clippy::too_many_arguments)]
    fn build_verdict(
        &self,
        verdict: &str,
        task_group: &str,
        candidate_tier: &str,
        baseline_tier: &str,
        sample_count: usize,
        quality_delta: f64,
        cost_reduction_pct: f64,
        failure_rate_delta: f64,
        reasons: Vec<String>,
        requires_human_review: bool,
    ) -> PromotionVerdict {
        PromotionVerdict {
            schema_version: PROMOTION_VERDICT_SCHEMA_VERSION.to_string(),
            verdict: verdict.to_string(),
            task_group: task_group.to_string(),
            candidate_tier: candidate_tier.to_string(),
            baseline_tier: baseline_tier.to_string(),
            sample_count,
            quality_delta,
            cost_reduction_pct,
            failure_rate_delta,
            reasons,
            requires_human_review,
        }
    }
}
