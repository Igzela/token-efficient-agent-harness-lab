use crate::node_executor::{LocalRunnerValidationExecutor, NodeExecutor, NoopNodeExecutor};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

const DECAY_FACTOR: f64 = 0.95;
const FAILURE_WEIGHT: f64 = 0.2;
const COOLDOWN_THRESHOLD: f64 = 0.8;
const BASE_COOLDOWN_MS: u64 = 10_000;

pub const EXECUTOR_POOL_SCHEMA_VERSION: &str = "executor_pool.v1";

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExecutorCapabilities {
    pub supported_task_types: Vec<String>,
    pub supported_task_domains: Vec<String>,
    pub requires_auth: bool,
    pub requires_cli: bool,
    pub max_timeout_ms: u64,
}

impl Default for ExecutorCapabilities {
    fn default() -> Self {
        Self {
            supported_task_types: vec![],
            supported_task_domains: vec![],
            requires_auth: false,
            requires_cli: false,
            max_timeout_ms: 300_000,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExecutorStatus {
    pub available: bool,
    pub active_count: u64,
    pub concurrency_limit: u64,
    pub cooldown_until: Option<String>,
    pub failure_score: f64,
}

impl Default for ExecutorStatus {
    fn default() -> Self {
        Self {
            available: true,
            active_count: 0,
            concurrency_limit: 10,
            cooldown_until: None,
            failure_score: 0.0,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CostProfile {
    pub cost_per_execution_usd: Option<f64>,
    pub daily_cost_usd: Option<f64>,
    pub daily_cost_limit_usd: Option<f64>,
}

impl Default for CostProfile {
    fn default() -> Self {
        Self {
            cost_per_execution_usd: None,
            daily_cost_usd: None,
            daily_cost_limit_usd: None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExecutorMetrics {
    pub total_executions: u64,
    pub successful_executions: u64,
    pub failed_executions: u64,
    pub avg_latency_ms: f64,
    pub total_latency_ms: u64,
    pub last_executed_at: Option<String>,
}

impl Default for ExecutorMetrics {
    fn default() -> Self {
        Self {
            total_executions: 0,
            successful_executions: 0,
            failed_executions: 0,
            avg_latency_ms: 0.0,
            total_latency_ms: 0,
            last_executed_at: None,
        }
    }
}

pub struct ExecutorEntry {
    pub executor_type: String,
    pub executor: Arc<dyn NodeExecutor>,
    pub capabilities: ExecutorCapabilities,
    pub status: ExecutorStatus,
    pub cost_profile: CostProfile,
    pub metrics: ExecutorMetrics,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExecutorPoolEntry {
    pub executor_type: String,
    pub capabilities: ExecutorCapabilities,
    pub status: ExecutorStatus,
    pub cost_profile: CostProfile,
    pub metrics: ExecutorMetrics,
}

pub struct ExecutorPool {
    entries: RwLock<HashMap<String, ExecutorEntry>>,
}

impl Default for ExecutorPool {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutorPool {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    pub fn register(&self, entry: ExecutorEntry) {
        let mut entries = self.entries.write().expect("pool lock poisoned");
        entries.insert(entry.executor_type.clone(), entry);
    }

    pub fn get(&self, executor_type: &str) -> Option<Arc<dyn NodeExecutor>> {
        let entries = self.entries.read().expect("pool lock poisoned");
        entries.get(executor_type).map(|e| Arc::clone(&e.executor))
    }

    pub fn best_for_task(&self, task_type: &str, task_domain: &str) -> Option<String> {
        let entries = self.entries.read().expect("pool lock poisoned");

        let mut candidates: Vec<(&str, bool, bool, f64)> = entries
            .values()
            .filter(|e| {
                e.status.available
                    && e.status.active_count < e.status.concurrency_limit
                    && e.status.cooldown_until.is_none()
                    && (e.capabilities.supported_task_types.is_empty()
                        || e.capabilities
                            .supported_task_types
                            .iter()
                            .any(|supported| supported == task_type))
                    && (e.capabilities.supported_task_domains.is_empty()
                        || e.capabilities
                            .supported_task_domains
                            .iter()
                            .any(|supported| supported == task_domain))
            })
            .map(|e| {
                let success_rate = if e.metrics.total_executions > 0 {
                    e.metrics.successful_executions as f64 / e.metrics.total_executions as f64
                } else {
                    1.0
                };
                let health_score = 1.0 - e.status.failure_score;
                (
                    e.executor_type.as_str(),
                    !e.capabilities.supported_task_types.is_empty(),
                    !e.capabilities.supported_task_domains.is_empty(),
                    success_rate * health_score,
                )
            })
            .collect();

        candidates.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal))
                .then_with(|| a.0.cmp(b.0))
        });
        candidates
            .first()
            .map(|(executor_type, ..)| executor_type.to_string())
    }

    pub fn acquire(&self, executor_type: &str) -> bool {
        let mut entries = self.entries.write().expect("pool lock poisoned");
        if let Some(entry) = entries.get_mut(executor_type) {
            if entry.status.available
                && entry.status.active_count < entry.status.concurrency_limit
                && entry.status.cooldown_until.is_none()
            {
                entry.status.active_count += 1;
                return true;
            }
        }
        false
    }

    pub fn release(&self, executor_type: &str, success: bool, latency_ms: u64, cost: Option<f64>) {
        let mut entries = self.entries.write().expect("pool lock poisoned");
        if let Some(entry) = entries.get_mut(executor_type) {
            if entry.status.active_count > 0 {
                entry.status.active_count -= 1;
            }

            entry.metrics.total_executions += 1;
            entry.metrics.total_latency_ms += latency_ms;
            entry.metrics.avg_latency_ms =
                entry.metrics.total_latency_ms as f64 / entry.metrics.total_executions as f64;

            if success {
                entry.metrics.successful_executions += 1;
            } else {
                entry.metrics.failed_executions += 1;
            }

            entry.status.failure_score = entry.status.failure_score * DECAY_FACTOR
                + if success { 0.0 } else { FAILURE_WEIGHT };
            entry.status.failure_score = entry.status.failure_score.clamp(0.0, 1.0);

            if entry.status.failure_score >= COOLDOWN_THRESHOLD {
                let multiplier = 1u64 + ((entry.status.failure_score * 5.0).floor() as u64);
                let cooldown_ms = std::cmp::min(60_000, BASE_COOLDOWN_MS * multiplier);
                let cooldown_until =
                    chrono::Utc::now() + chrono::Duration::milliseconds(cooldown_ms as i64);
                entry.status.available = false;
                entry.status.cooldown_until = Some(cooldown_until.to_rfc3339());
            }

            if let Some(c) = cost {
                if let Some(ref mut daily) = entry.cost_profile.daily_cost_usd {
                    *daily += c;
                } else {
                    entry.cost_profile.daily_cost_usd = Some(c);
                }
            }

            entry.metrics.last_executed_at = Some(chrono::Utc::now().to_rfc3339());
        }
    }

    pub fn release_without_recording(&self, executor_type: &str) {
        let mut entries = self.entries.write().expect("pool lock poisoned");
        if let Some(entry) = entries.get_mut(executor_type) {
            entry.status.active_count = entry.status.active_count.saturating_sub(1);
        }
    }

    pub fn snapshot(&self) -> Vec<ExecutorPoolEntry> {
        let entries = self.entries.read().expect("pool lock poisoned");
        entries
            .values()
            .map(|e| ExecutorPoolEntry {
                executor_type: e.executor_type.clone(),
                capabilities: e.capabilities.clone(),
                status: e.status.clone(),
                cost_profile: e.cost_profile.clone(),
                metrics: e.metrics.clone(),
            })
            .collect()
    }

    pub fn start_cooldown(&self, executor_type: &str, duration_ms: u64) {
        let mut entries = self.entries.write().expect("pool lock poisoned");
        if let Some(entry) = entries.get_mut(executor_type) {
            let cooldown_until =
                chrono::Utc::now() + chrono::Duration::milliseconds(duration_ms as i64);
            entry.status.available = false;
            entry.status.cooldown_until = Some(cooldown_until.to_rfc3339());
        }
    }

    pub fn tick_cooldowns(&self) {
        let now = chrono::Utc::now();
        let mut entries = self.entries.write().expect("pool lock poisoned");
        for entry in entries.values_mut() {
            if let Some(ref cooldown_str) = entry.status.cooldown_until {
                if let Ok(cooldown_time) = chrono::DateTime::parse_from_rfc3339(cooldown_str) {
                    if now >= cooldown_time.with_timezone(&chrono::Utc) {
                        entry.status.available = true;
                        entry.status.cooldown_until = None;
                    }
                }
            }
        }
    }

    pub fn total_active(&self) -> u64 {
        let entries = self.entries.read().expect("pool lock poisoned");
        entries.values().map(|e| e.status.active_count).sum()
    }

    pub fn total_capacity(&self) -> u64 {
        let entries = self.entries.read().expect("pool lock poisoned");
        entries.values().map(|e| e.status.concurrency_limit).sum()
    }
}

pub fn register_default_executors(pool: &ExecutorPool, cli_enabled: bool) {
    pool.register(ExecutorEntry {
        executor_type: "noop".to_string(),
        executor: Arc::new(NoopNodeExecutor),
        capabilities: ExecutorCapabilities {
            supported_task_types: vec![],
            supported_task_domains: vec![],
            requires_auth: false,
            requires_cli: false,
            max_timeout_ms: 1_000,
        },
        status: ExecutorStatus {
            concurrency_limit: 100,
            ..Default::default()
        },
        cost_profile: CostProfile::default(),
        metrics: ExecutorMetrics::default(),
    });

    pool.register(ExecutorEntry {
        executor_type: "local_runner_validation".to_string(),
        executor: Arc::new(LocalRunnerValidationExecutor),
        capabilities: ExecutorCapabilities {
            supported_task_types: vec!["local_runner_validation".to_string()],
            supported_task_domains: vec!["scorecard".to_string()],
            requires_auth: false,
            requires_cli: false,
            max_timeout_ms: 60_000,
        },
        status: ExecutorStatus {
            concurrency_limit: 2,
            ..Default::default()
        },
        cost_profile: CostProfile::default(),
        metrics: ExecutorMetrics::default(),
    });

    pool.register(ExecutorEntry {
        executor_type: "stub".to_string(),
        executor: Arc::new(crate::node_executor::StubNodeExecutor::default()),
        capabilities: ExecutorCapabilities {
            supported_task_types: vec![],
            supported_task_domains: vec![],
            requires_auth: false,
            requires_cli: false,
            max_timeout_ms: 5_000,
        },
        status: ExecutorStatus {
            concurrency_limit: 100,
            ..Default::default()
        },
        cost_profile: CostProfile::default(),
        metrics: ExecutorMetrics::default(),
    });

    pool.register(ExecutorEntry {
        executor_type: "command".to_string(),
        executor: Arc::new(crate::node_executor::CommandNodeExecutor::default()),
        capabilities: ExecutorCapabilities {
            supported_task_types: vec![],
            supported_task_domains: vec![],
            requires_auth: false,
            requires_cli: false,
            max_timeout_ms: 300_000,
        },
        status: ExecutorStatus {
            concurrency_limit: 4,
            ..Default::default()
        },
        cost_profile: CostProfile::default(),
        metrics: ExecutorMetrics::default(),
    });

    if cli_enabled {
        let cli_config = crate::cli::CliConfig::from_env();
        if let Some(cli_exec) = crate::cli::CliNodeExecutor::from_config(&cli_config) {
            if cli_exec.claude_bin.is_some() {
                pool.register(ExecutorEntry {
                    executor_type: "claude_code_cli".to_string(),
                    executor: Arc::new(crate::cli::CliNodeExecutor::new(
                        cli_exec.claude_bin.clone(),
                        None,
                        cli_exec.timeout_ms,
                    )),
                    capabilities: ExecutorCapabilities {
                        supported_task_types: vec![],
                        supported_task_domains: vec![
                            "code".to_string(),
                            "architecture".to_string(),
                        ],
                        requires_auth: true,
                        requires_cli: true,
                        max_timeout_ms: 300_000,
                    },
                    status: ExecutorStatus {
                        concurrency_limit: 2,
                        ..Default::default()
                    },
                    cost_profile: CostProfile {
                        cost_per_execution_usd: Some(0.01),
                        ..Default::default()
                    },
                    metrics: ExecutorMetrics::default(),
                });
            }
            if cli_exec.codex_bin.is_some() {
                pool.register(ExecutorEntry {
                    executor_type: "codex_cli".to_string(),
                    executor: Arc::new(crate::cli::CliNodeExecutor::new(
                        None,
                        cli_exec.codex_bin.clone(),
                        cli_exec.timeout_ms,
                    )),
                    capabilities: ExecutorCapabilities {
                        supported_task_types: vec![],
                        supported_task_domains: vec!["code".to_string()],
                        requires_auth: true,
                        requires_cli: true,
                        max_timeout_ms: 300_000,
                    },
                    status: ExecutorStatus {
                        concurrency_limit: 2,
                        ..Default::default()
                    },
                    cost_profile: CostProfile {
                        cost_per_execution_usd: Some(0.005),
                        ..Default::default()
                    },
                    metrics: ExecutorMetrics::default(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pool() -> ExecutorPool {
        ExecutorPool::new()
    }

    fn register_noop(pool: &ExecutorPool, concurrency: u64) {
        pool.register(ExecutorEntry {
            executor_type: "noop".to_string(),
            executor: Arc::new(NoopNodeExecutor),
            capabilities: ExecutorCapabilities::default(),
            status: ExecutorStatus {
                concurrency_limit: concurrency,
                ..Default::default()
            },
            cost_profile: CostProfile::default(),
            metrics: ExecutorMetrics::default(),
        });
    }

    fn register_code_executor(pool: &ExecutorPool, name: &str, concurrency: u64) {
        pool.register(ExecutorEntry {
            executor_type: name.to_string(),
            executor: Arc::new(NoopNodeExecutor),
            capabilities: ExecutorCapabilities {
                supported_task_domains: vec!["code".to_string()],
                ..Default::default()
            },
            status: ExecutorStatus {
                concurrency_limit: concurrency,
                ..Default::default()
            },
            cost_profile: CostProfile::default(),
            metrics: ExecutorMetrics::default(),
        });
    }

    #[test]
    fn register_and_get_executor() {
        let pool = test_pool();
        register_noop(&pool, 10);
        assert!(pool.get("noop").is_some());
        assert!(pool.get("nonexistent").is_none());
    }

    #[test]
    fn acquire_and_release() {
        let pool = test_pool();
        register_noop(&pool, 2);

        assert!(pool.acquire("noop"));
        assert_eq!(pool.total_active(), 1);
        assert!(pool.acquire("noop"));
        assert_eq!(pool.total_active(), 2);
        assert!(!pool.acquire("noop"));
        assert_eq!(pool.total_capacity(), 2);

        pool.release("noop", true, 100, None);
        assert_eq!(pool.total_active(), 1);
        assert!(pool.acquire("noop"));
        pool.release("noop", true, 100, None);
        pool.release("noop", true, 100, None);
        assert_eq!(pool.total_active(), 0);
    }

    #[test]
    fn acquire_nonexistent_returns_false() {
        let pool = test_pool();
        assert!(!pool.acquire("nonexistent"));
    }

    #[test]
    fn release_nonexistent_does_not_panic() {
        let pool = test_pool();
        pool.release("nonexistent", true, 100, None);
    }

    #[test]
    fn failure_score_increases_on_failure() {
        let pool = test_pool();
        register_noop(&pool, 10);

        pool.acquire("noop");
        pool.release("noop", false, 100, None);

        let snap = pool.snapshot();
        assert!(snap[0].status.failure_score > 0.0);
        assert!((snap[0].status.failure_score - FAILURE_WEIGHT).abs() < 0.01);
    }

    #[test]
    fn failure_score_decays_on_success() {
        let pool = test_pool();
        register_noop(&pool, 10);

        pool.acquire("noop");
        pool.release("noop", false, 100, None);
        let score_after_fail = pool.snapshot()[0].status.failure_score;

        pool.acquire("noop");
        pool.release("noop", true, 100, None);
        let score_after_success = pool.snapshot()[0].status.failure_score;

        assert!(score_after_success < score_after_fail);
    }

    #[test]
    fn cooldown_triggers_on_high_failure_score() {
        let pool = test_pool();
        register_noop(&pool, 10);

        for _ in 0..10 {
            pool.acquire("noop");
            pool.release("noop", false, 100, None);
        }

        let snap = pool.snapshot();
        assert!(snap[0].status.failure_score >= COOLDOWN_THRESHOLD);
        assert!(!snap[0].status.available);
        assert!(snap[0].status.cooldown_until.is_some());
    }

    #[test]
    fn cannot_acquire_during_cooldown() {
        let pool = test_pool();
        register_noop(&pool, 10);

        pool.start_cooldown("noop", 60_000);
        assert!(!pool.acquire("noop"));
    }

    #[test]
    fn tick_cooldowns_expires() {
        let pool = test_pool();
        register_noop(&pool, 10);

        pool.start_cooldown("noop", 1);
        assert!(!pool.acquire("noop"));

        std::thread::sleep(std::time::Duration::from_millis(50));
        pool.tick_cooldowns();
        assert!(pool.acquire("noop"));
    }

    #[test]
    fn best_for_task_matches_domain() {
        let pool = test_pool();
        pool.register(ExecutorEntry {
            executor_type: "noop".to_string(),
            executor: Arc::new(NoopNodeExecutor),
            capabilities: ExecutorCapabilities::default(),
            status: ExecutorStatus {
                concurrency_limit: 10,
                ..Default::default()
            },
            cost_profile: CostProfile::default(),
            metrics: ExecutorMetrics {
                total_executions: 10,
                successful_executions: 5,
                failed_executions: 5,
                ..Default::default()
            },
        });
        register_code_executor(&pool, "code_exec", 5);

        let best = pool.best_for_task("code_generate", "code");
        assert_eq!(best.as_deref(), Some("code_exec"));
    }

    #[test]
    fn best_for_task_returns_none_when_all_at_capacity() {
        let pool = test_pool();
        register_noop(&pool, 1);

        pool.acquire("noop");
        assert!(pool.best_for_task("any", "any").is_none());
        pool.release("noop", true, 100, None);
    }

    #[test]
    fn best_for_task_returns_none_when_all_in_cooldown() {
        let pool = test_pool();
        register_noop(&pool, 10);

        pool.start_cooldown("noop", 60_000);
        assert!(pool.best_for_task("any", "any").is_none());
    }

    #[test]
    fn best_for_task_prefers_higher_success_rate() {
        let pool = test_pool();
        pool.register(ExecutorEntry {
            executor_type: "slow".to_string(),
            executor: Arc::new(NoopNodeExecutor),
            capabilities: ExecutorCapabilities::default(),
            status: ExecutorStatus {
                concurrency_limit: 10,
                ..Default::default()
            },
            cost_profile: CostProfile::default(),
            metrics: ExecutorMetrics {
                total_executions: 100,
                successful_executions: 50,
                ..Default::default()
            },
        });
        pool.register(ExecutorEntry {
            executor_type: "fast".to_string(),
            executor: Arc::new(NoopNodeExecutor),
            capabilities: ExecutorCapabilities::default(),
            status: ExecutorStatus {
                concurrency_limit: 10,
                ..Default::default()
            },
            cost_profile: CostProfile::default(),
            metrics: ExecutorMetrics {
                total_executions: 100,
                successful_executions: 95,
                ..Default::default()
            },
        });

        let best = pool.best_for_task("any", "any");
        assert_eq!(best.as_deref(), Some("fast"));
    }

    #[test]
    fn best_for_task_prefers_exact_capability_over_healthier_wildcard() {
        let pool = test_pool();
        pool.register(ExecutorEntry {
            executor_type: "generic".to_string(),
            executor: Arc::new(NoopNodeExecutor),
            capabilities: ExecutorCapabilities::default(),
            status: ExecutorStatus {
                concurrency_limit: 10,
                ..Default::default()
            },
            cost_profile: CostProfile::default(),
            metrics: ExecutorMetrics {
                total_executions: 100,
                successful_executions: 100,
                ..Default::default()
            },
        });
        pool.register(ExecutorEntry {
            executor_type: "scorecard_validator".to_string(),
            executor: Arc::new(NoopNodeExecutor),
            capabilities: ExecutorCapabilities {
                supported_task_types: vec!["local_runner_validation".to_string()],
                supported_task_domains: vec!["scorecard".to_string()],
                ..Default::default()
            },
            status: ExecutorStatus {
                concurrency_limit: 10,
                ..Default::default()
            },
            cost_profile: CostProfile::default(),
            metrics: ExecutorMetrics {
                total_executions: 100,
                successful_executions: 50,
                failed_executions: 50,
                ..Default::default()
            },
        });

        let best = pool.best_for_task("local_runner_validation", "scorecard");

        assert_eq!(best.as_deref(), Some("scorecard_validator"));
    }

    #[test]
    fn best_for_task_breaks_equal_scores_by_executor_type() {
        for _ in 0..32 {
            let pool = test_pool();
            for executor_type in ["zeta", "alpha"] {
                pool.register(ExecutorEntry {
                    executor_type: executor_type.to_string(),
                    executor: Arc::new(NoopNodeExecutor),
                    capabilities: ExecutorCapabilities::default(),
                    status: ExecutorStatus {
                        concurrency_limit: 10,
                        ..Default::default()
                    },
                    cost_profile: CostProfile::default(),
                    metrics: ExecutorMetrics::default(),
                });
            }

            assert_eq!(pool.best_for_task("any", "any").as_deref(), Some("alpha"));
        }
    }

    #[test]
    fn snapshot_includes_all_entries() {
        let pool = test_pool();
        register_noop(&pool, 10);
        register_code_executor(&pool, "code_exec", 5);

        let snap = pool.snapshot();
        assert_eq!(snap.len(), 2);
        let types: Vec<&str> = snap.iter().map(|e| e.executor_type.as_str()).collect();
        assert!(types.contains(&"noop"));
        assert!(types.contains(&"code_exec"));
    }

    #[test]
    fn cost_profile_tracks_daily_cost() {
        let pool = test_pool();
        pool.register(ExecutorEntry {
            executor_type: "noop".to_string(),
            executor: Arc::new(NoopNodeExecutor),
            capabilities: ExecutorCapabilities::default(),
            status: ExecutorStatus {
                concurrency_limit: 10,
                ..Default::default()
            },
            cost_profile: CostProfile::default(),
            metrics: ExecutorMetrics::default(),
        });

        pool.acquire("noop");
        pool.release("noop", true, 100, Some(0.05));
        pool.acquire("noop");
        pool.release("noop", true, 100, Some(0.03));

        let snap = pool.snapshot();
        assert!((snap[0].cost_profile.daily_cost_usd.unwrap() - 0.08).abs() < 0.001);
    }

    #[test]
    fn metrics_track_latency() {
        let pool = test_pool();
        register_noop(&pool, 10);

        pool.acquire("noop");
        pool.release("noop", true, 100, None);
        pool.acquire("noop");
        pool.release("noop", true, 200, None);

        let snap = pool.snapshot();
        assert_eq!(snap[0].metrics.total_executions, 2);
        assert_eq!(snap[0].metrics.successful_executions, 2);
        assert!((snap[0].metrics.avg_latency_ms - 150.0).abs() < 0.01);
        assert_eq!(snap[0].metrics.total_latency_ms, 300);
    }

    #[test]
    fn metrics_track_success_failure_counts() {
        let pool = test_pool();
        register_noop(&pool, 10);

        pool.acquire("noop");
        pool.release("noop", true, 100, None);
        pool.acquire("noop");
        pool.release("noop", false, 100, None);
        pool.acquire("noop");
        pool.release("noop", true, 100, None);

        let snap = pool.snapshot();
        assert_eq!(snap[0].metrics.total_executions, 3);
        assert_eq!(snap[0].metrics.successful_executions, 2);
        assert_eq!(snap[0].metrics.failed_executions, 1);
    }
}
