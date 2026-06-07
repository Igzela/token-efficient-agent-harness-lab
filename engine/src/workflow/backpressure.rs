use std::collections::HashMap;

// ---------------------------------------------------------------------------
// BackpressureConfig
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct BackpressureConfig {
    pub enabled: bool,
    pub activation_threshold: f64,
    pub deactivation_threshold: f64,
    pub max_paused_runs: usize,
    pub degrade_concurrency_factor: f64,
    pub cooldown_after_pause_ms: u64,
}

impl Default for BackpressureConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            activation_threshold: 0.8,
            deactivation_threshold: 0.5,
            max_paused_runs: 10,
            degrade_concurrency_factor: 0.5,
            cooldown_after_pause_ms: 60_000,
        }
    }
}

// ---------------------------------------------------------------------------
// PauseAction
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct PauseAction {
    pub run_id: String,
    pub reason: String,
    pub paused_at: String,
}

// ---------------------------------------------------------------------------
// BackpressureDecision
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct BackpressureDecision {
    pub active: bool,
    pub action: String,
    pub runs_to_pause: Vec<String>,
    pub degrade_mode: Option<String>,
    pub effective_concurrency: usize,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Backpressure
// ---------------------------------------------------------------------------

pub struct Backpressure {
    config: BackpressureConfig,
    paused_runs: HashMap<String, u64>,
    is_active: bool,
}

impl Backpressure {
    pub fn new(config: BackpressureConfig) -> Self {
        Self {
            config,
            paused_runs: HashMap::new(),
            is_active: false,
        }
    }

    pub fn evaluate(
        &mut self,
        pool_utilization: f64,
        queue_depth: usize,
        max_queue: usize,
        overdue_count: usize,
        current_timestamp_ms: u64,
    ) -> BackpressureDecision {
        if !self.config.enabled {
            return BackpressureDecision {
                active: false,
                action: "none".to_string(),
                runs_to_pause: Vec::new(),
                degrade_mode: None,
                effective_concurrency: 0,
                reason: "backpressure disabled".to_string(),
            };
        }

        let queue_depth_ratio = if max_queue > 0 {
            queue_depth as f64 / max_queue as f64
        } else {
            0.0
        };

        let should_activate = pool_utilization > self.config.activation_threshold
            || queue_depth_ratio > self.config.activation_threshold;

        let should_deactivate = pool_utilization < self.config.deactivation_threshold
            && queue_depth_ratio < self.config.deactivation_threshold;

        if should_activate && !self.is_active {
            self.is_active = true;
        } else if should_deactivate && self.is_active {
            self.is_active = false;
            self.paused_runs.clear();
        }

        if !self.is_active {
            return BackpressureDecision {
                active: false,
                action: "none".to_string(),
                runs_to_pause: Vec::new(),
                degrade_mode: None,
                effective_concurrency: 0,
                reason: "below threshold".to_string(),
            };
        }

        self.cleanup_expired_cooldowns(current_timestamp_ms);

        let mut runs_to_pause = Vec::new();
        if overdue_count > 0 && self.paused_runs.len() < self.config.max_paused_runs {
            let can_pause = self.config.max_paused_runs - self.paused_runs.len();
            let to_pause = overdue_count.min(can_pause);
            for i in 0..to_pause {
                runs_to_pause.push(format!("overdue_{}", i));
            }
        }

        let effective_concurrency = self.effective_max_concurrent(10, true); // Base concurrency 10

        let degrade_mode = if pool_utilization > 0.95 {
            Some("severe".to_string())
        } else {
            Some("throttle".to_string())
        };

        let reason = format!(
            "utilization {:.1}%, queue depth {:.1}%, overdue {}",
            pool_utilization * 100.0,
            queue_depth_ratio * 100.0,
            overdue_count
        );

        BackpressureDecision {
            active: true,
            action: "pause_and_throttle".to_string(),
            runs_to_pause,
            degrade_mode,
            effective_concurrency,
            reason,
        }
    }

    pub fn should_skip_run(&self, run_id: &str, current_timestamp_ms: u64) -> bool {
        if !self.is_active {
            return false;
        }

        if let Some(&paused_at) = self.paused_runs.get(run_id) {
            let elapsed = current_timestamp_ms.saturating_sub(paused_at);
            elapsed < self.config.cooldown_after_pause_ms
        } else {
            false
        }
    }

    pub fn record_pause(&mut self, run_id: &str, timestamp_ms: u64) {
        self.paused_runs.insert(run_id.to_string(), timestamp_ms);
    }

    pub fn effective_max_concurrent(&self, base: usize, active: bool) -> usize {
        if active {
            let effective = (base as f64 * self.config.degrade_concurrency_factor) as usize;
            effective.max(1)
        } else {
            base
        }
    }

    pub fn cleanup_expired_cooldowns(&mut self, current_timestamp_ms: u64) {
        self.paused_runs.retain(|_, paused_at| {
            let elapsed = current_timestamp_ms.saturating_sub(*paused_at);
            elapsed < self.config.cooldown_after_pause_ms
        });
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = BackpressureConfig::default();
        assert!(config.enabled);
        assert_eq!(config.activation_threshold, 0.8);
        assert_eq!(config.deactivation_threshold, 0.5);
        assert_eq!(config.max_paused_runs, 10);
        assert_eq!(config.degrade_concurrency_factor, 0.5);
        assert_eq!(config.cooldown_after_pause_ms, 60_000);
    }

    #[test]
    fn test_evaluate_below_threshold() {
        let config = BackpressureConfig::default();
        let mut bp = Backpressure::new(config);
        let decision = bp.evaluate(0.5, 30, 100, 0, 1000);
        assert!(!decision.active);
        assert_eq!(decision.action, "none");
    }

    #[test]
    fn test_evaluate_activates_on_high_utilization() {
        let config = BackpressureConfig::default();
        let mut bp = Backpressure::new(config);
        let decision = bp.evaluate(0.9, 30, 100, 0, 1000);
        assert!(decision.active);
        assert_eq!(decision.action, "pause_and_throttle");
        assert!(decision.degrade_mode.is_some());
    }

    #[test]
    fn test_evaluate_activates_on_high_queue_depth() {
        let config = BackpressureConfig::default();
        let mut bp = Backpressure::new(config);
        let decision = bp.evaluate(0.5, 90, 100, 0, 1000);
        assert!(decision.active);
    }

    #[test]
    fn test_evaluate_deactivates_on_low_metrics() {
        let config = BackpressureConfig::default();
        let mut bp = Backpressure::new(config);

        // Activate first
        bp.evaluate(0.9, 90, 100, 0, 1000);
        assert!(bp.is_active);

        // Deactivate
        let decision = bp.evaluate(0.3, 20, 100, 0, 2000);
        assert!(!decision.active);
        assert!(!bp.is_active);
    }

    #[test]
    fn test_evaluate_disabled() {
        let config = BackpressureConfig {
            enabled: false,
            ..Default::default()
        };
        let mut bp = Backpressure::new(config);
        let decision = bp.evaluate(0.99, 99, 100, 0, 1000);
        assert!(!decision.active);
        assert_eq!(decision.reason, "backpressure disabled");
    }

    #[test]
    fn test_should_skip_run_not_paused() {
        let config = BackpressureConfig::default();
        let mut bp = Backpressure::new(config);
        bp.is_active = true;
        assert!(!bp.should_skip_run("run-1", 5000));
    }

    #[test]
    fn test_should_skip_run_within_cooldown() {
        let config = BackpressureConfig::default();
        let mut bp = Backpressure::new(config);
        bp.is_active = true;
        bp.record_pause("run-1", 1000);
        assert!(bp.should_skip_run("run-1", 5000));
    }

    #[test]
    fn test_should_skip_run_cooldown_expired() {
        let config = BackpressureConfig::default();
        let mut bp = Backpressure::new(config);
        bp.is_active = true;
        bp.record_pause("run-1", 1000);
        // After 61 seconds (cooldown is 60s)
        assert!(!bp.should_skip_run("run-1", 62_000));
    }

    #[test]
    fn test_record_pause() {
        let config = BackpressureConfig::default();
        let mut bp = Backpressure::new(config);
        bp.record_pause("run-1", 1000);
        assert!(bp.paused_runs.contains_key("run-1"));
    }

    #[test]
    fn test_effective_max_concurrent_active() {
        let config = BackpressureConfig::default();
        let bp = Backpressure::new(config);
        let effective = bp.effective_max_concurrent(10, true);
        assert_eq!(effective, 5);
    }

    #[test]
    fn test_effective_max_concurrent_inactive() {
        let config = BackpressureConfig::default();
        let bp = Backpressure::new(config);
        let effective = bp.effective_max_concurrent(10, false);
        assert_eq!(effective, 10);
    }

    #[test]
    fn test_effective_max_concurrent_minimum_one() {
        let config = BackpressureConfig {
            degrade_concurrency_factor: 0.1,
            ..Default::default()
        };
        let bp = Backpressure::new(config);
        let effective = bp.effective_max_concurrent(5, true);
        assert_eq!(effective, 1);
    }

    #[test]
    fn test_cleanup_expired_cooldowns() {
        let config = BackpressureConfig::default();
        let mut bp = Backpressure::new(config);
        bp.record_pause("run-1", 1000);
        bp.record_pause("run-2", 60_000);

        // At 62 seconds: run-1 cooldown expired (61s elapsed), run-2 still active (2s elapsed)
        bp.cleanup_expired_cooldowns(62_000);
        assert!(!bp.paused_runs.contains_key("run-1"));
        assert!(bp.paused_runs.contains_key("run-2"));
    }

    #[test]
    fn test_evaluate_with_overdue_runs() {
        let config = BackpressureConfig::default();
        let mut bp = Backpressure::new(config);
        let decision = bp.evaluate(0.9, 50, 100, 3, 1000);
        assert!(decision.active);
        assert_eq!(decision.runs_to_pause.len(), 3);
    }

    #[test]
    fn test_evaluate_respects_max_paused() {
        let config = BackpressureConfig {
            max_paused_runs: 2,
            ..Default::default()
        };
        let mut bp = Backpressure::new(config);
        let decision = bp.evaluate(0.9, 50, 100, 5, 1000);
        assert!(decision.active);
        assert_eq!(decision.runs_to_pause.len(), 2);
    }
}
