use crate::node_executor::{LocalRunnerValidationExecutor, NodeExecutor, NoopNodeExecutor};
use crate::storage::local_product_store::LocalProductStore;
use crate::tool_policy_executor::ToolPolicyNodeExecutor;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

const DECAY_FACTOR: f64 = 0.95;
const FAILURE_WEIGHT: f64 = 0.2;
const COOLDOWN_THRESHOLD: f64 = 0.8;
const BASE_COOLDOWN_MS: u64 = 10_000;

fn task_type_requires_exact_capability(task_type: &str) -> bool {
    matches!(
        task_type,
        "agent_step"
            | "adaptive_provider"
            | "command"
            | "claude_code_cli"
            | "codex_cli"
            | crate::external_runtime::LANGGRAPH_TASK_TYPE
            | crate::opencode_runtime::OPENCODE_TASK_TYPE
    )
}

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
                let exact_capability_required = task_type_requires_exact_capability(task_type);
                e.status.available
                    && e.status.active_count < e.status.concurrency_limit
                    && e.status.cooldown_until.is_none()
                    && ((!exact_capability_required
                        && e.capabilities.supported_task_types.is_empty())
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

    pub fn supports_task(
        &self,
        executor_type: &str,
        task_type: &str,
        task_domain: &str,
        allow_cli_domain_route: bool,
    ) -> bool {
        let entries = self.entries.read().expect("pool lock poisoned");
        entries.get(executor_type).is_some_and(|entry| {
            let task_matches = entry.capabilities.supported_task_types.is_empty()
                || entry
                    .capabilities
                    .supported_task_types
                    .iter()
                    .any(|supported| supported == task_type)
                || (allow_cli_domain_route && entry.capabilities.requires_cli);
            let domain_matches = entry.capabilities.supported_task_domains.is_empty()
                || entry
                    .capabilities
                    .supported_task_domains
                    .iter()
                    .any(|supported| supported == task_domain);
            task_matches && domain_matches
        })
    }

    pub fn best_cli_for_domain(&self, task_domain: &str) -> Option<String> {
        let entries = self.entries.read().expect("pool lock poisoned");
        let mut candidates = entries
            .values()
            .filter(|entry| {
                entry.capabilities.requires_cli
                    && entry
                        .capabilities
                        .supported_task_domains
                        .iter()
                        .any(|supported| supported == task_domain)
                    && entry.status.available
                    && entry.status.active_count < entry.status.concurrency_limit
                    && entry.status.cooldown_until.is_none()
            })
            .map(|entry| {
                let success_rate = if entry.metrics.total_executions > 0 {
                    entry.metrics.successful_executions as f64
                        / entry.metrics.total_executions as f64
                } else {
                    1.0
                };
                (
                    entry.executor_type.as_str(),
                    success_rate * (1.0 - entry.status.failure_score),
                )
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            right
                .1
                .partial_cmp(&left.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.0.cmp(right.0))
        });
        candidates
            .first()
            .map(|(executor_type, _)| executor_type.to_string())
    }

    pub fn has_cli_for_domain(&self, task_domain: &str) -> bool {
        let entries = self.entries.read().expect("pool lock poisoned");
        entries.values().any(|entry| {
            entry.capabilities.requires_cli
                && entry
                    .capabilities
                    .supported_task_domains
                    .iter()
                    .any(|supported| supported == task_domain)
        })
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

pub fn register_default_executors(
    pool: &ExecutorPool,
    cli_enabled: bool,
    store: Arc<LocalProductStore>,
) {
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
        executor: Arc::new(ToolPolicyNodeExecutor::command(
            Arc::new(crate::node_executor::CommandNodeExecutor::default()),
            store.clone(),
        )),
        capabilities: ExecutorCapabilities {
            supported_task_types: vec!["command".to_string()],
            supported_task_domains: vec![],
            requires_auth: false,
            requires_cli: false,
            max_timeout_ms: 30_000,
        },
        status: ExecutorStatus {
            concurrency_limit: 4,
            ..Default::default()
        },
        cost_profile: CostProfile::default(),
        metrics: ExecutorMetrics::default(),
    });

    if cli_enabled {
        register_cli_executors(pool, &crate::cli::CliConfig::from_env(), store);
    }
}

pub fn register_cli_executors(
    pool: &ExecutorPool,
    config: &crate::cli::CliConfig,
    store: Arc<LocalProductStore>,
) {
    if !config.enabled {
        return;
    }
    if let Some(cli_exec) = crate::cli::CliNodeExecutor::from_config_for(config, "claude_code_cli")
    {
        pool.register(ExecutorEntry {
            executor_type: "claude_code_cli".to_string(),
            executor: Arc::new(ToolPolicyNodeExecutor::cli(
                Arc::new(cli_exec.with_managed_acceptance_store(Arc::clone(&store))),
                Arc::clone(&store),
                "claude_code_cli",
            )),
            capabilities: ExecutorCapabilities {
                supported_task_types: vec!["claude_code_cli".to_string()],
                // The exact task type and product binding are authoritative. The
                // planner's heuristic domain (docs/config/code/architecture) must
                // not make an explicitly admitted managed node unrunnable.
                supported_task_domains: vec![],
                requires_auth: true,
                requires_cli: true,
                max_timeout_ms: config.timeout_ms,
            },
            status: ExecutorStatus {
                concurrency_limit: 1,
                ..Default::default()
            },
            cost_profile: CostProfile {
                cost_per_execution_usd: config
                    .claude_code_admission
                    .as_ref()
                    .map(|admission| admission.max_budget_usd),
                ..Default::default()
            },
            metrics: ExecutorMetrics::default(),
        });
    }
    if let Some(cli_exec) = crate::cli::CliNodeExecutor::from_config_for(config, "codex_cli") {
        pool.register(ExecutorEntry {
            executor_type: "codex_cli".to_string(),
            executor: Arc::new(ToolPolicyNodeExecutor::cli(
                Arc::new(cli_exec.with_managed_acceptance_store(Arc::clone(&store))),
                store,
                "codex_cli",
            )),
            capabilities: ExecutorCapabilities {
                supported_task_types: vec!["codex_cli".to_string()],
                supported_task_domains: vec!["code".to_string()],
                requires_auth: true,
                requires_cli: true,
                max_timeout_ms: config.timeout_ms,
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

pub fn register_adaptive_provider_executor(
    pool: &ExecutorPool,
    executor: Arc<dyn NodeExecutor>,
    concurrency_limit: usize,
) {
    pool.register(ExecutorEntry {
        executor_type: "adaptive_provider".to_string(),
        executor,
        capabilities: ExecutorCapabilities {
            supported_task_types: vec!["adaptive_provider".to_string()],
            supported_task_domains: vec!["adaptive".to_string()],
            requires_auth: true,
            requires_cli: false,
            max_timeout_ms: 300_000,
        },
        status: ExecutorStatus {
            concurrency_limit: concurrency_limit as u64,
            ..Default::default()
        },
        cost_profile: CostProfile::default(),
        metrics: ExecutorMetrics::default(),
    });
}

pub fn register_agent_step_executor(
    pool: &ExecutorPool,
    executor: Arc<dyn NodeExecutor>,
    concurrency_limit: usize,
) {
    pool.register(ExecutorEntry {
        executor_type: "agent_step".to_string(),
        executor,
        capabilities: ExecutorCapabilities {
            supported_task_types: vec!["agent_step".to_string()],
            supported_task_domains: vec!["agent_runtime".to_string()],
            requires_auth: true,
            requires_cli: false,
            max_timeout_ms: 300_000,
        },
        status: ExecutorStatus {
            concurrency_limit: concurrency_limit as u64,
            ..Default::default()
        },
        cost_profile: CostProfile::default(),
        metrics: ExecutorMetrics::default(),
    });
}

pub fn register_external_runtime_executor(
    pool: &ExecutorPool,
    executor: Arc<dyn NodeExecutor>,
    concurrency_limit: usize,
    timeout_ms: u64,
) {
    pool.register(ExecutorEntry {
        executor_type: crate::external_runtime::LANGGRAPH_EXECUTOR_TYPE.to_string(),
        executor,
        capabilities: ExecutorCapabilities {
            supported_task_types: vec![crate::external_runtime::LANGGRAPH_TASK_TYPE.to_string()],
            supported_task_domains: vec!["external_runtime".to_string(), "benchmark".to_string()],
            requires_auth: true,
            requires_cli: false,
            max_timeout_ms: timeout_ms,
        },
        status: ExecutorStatus {
            concurrency_limit: concurrency_limit.max(1) as u64,
            ..Default::default()
        },
        cost_profile: CostProfile::default(),
        metrics: ExecutorMetrics::default(),
    });
}

pub fn register_opencode_runtime_executor(
    pool: &ExecutorPool,
    executor: Arc<dyn NodeExecutor>,
    concurrency_limit: usize,
    timeout_ms: u64,
) {
    pool.register(ExecutorEntry {
        executor_type: crate::opencode_runtime::OPENCODE_EXECUTOR_TYPE.to_string(),
        executor,
        capabilities: ExecutorCapabilities {
            supported_task_types: vec![crate::opencode_runtime::OPENCODE_TASK_TYPE.to_string()],
            supported_task_domains: vec!["external_runtime".to_string(), "code".to_string()],
            requires_auth: false,
            requires_cli: false,
            max_timeout_ms: timeout_ms,
        },
        status: ExecutorStatus {
            concurrency_limit: concurrency_limit.max(1) as u64,
            ..Default::default()
        },
        cost_profile: CostProfile::default(),
        metrics: ExecutorMetrics::default(),
    });
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
    fn reserved_agent_step_task_requires_exact_executor_capability() {
        let pool = test_pool();
        register_noop(&pool, 10);

        assert_eq!(pool.best_for_task("agent_step", "agent_runtime"), None);

        pool.register(ExecutorEntry {
            executor_type: "agent_step".to_string(),
            executor: Arc::new(NoopNodeExecutor),
            capabilities: ExecutorCapabilities {
                supported_task_types: vec!["agent_step".to_string()],
                supported_task_domains: vec!["agent_runtime".to_string()],
                requires_auth: true,
                requires_cli: false,
                max_timeout_ms: 300_000,
            },
            status: ExecutorStatus {
                concurrency_limit: 2,
                ..Default::default()
            },
            cost_profile: CostProfile::default(),
            metrics: ExecutorMetrics::default(),
        });

        assert_eq!(
            pool.best_for_task("agent_step", "agent_runtime"),
            Some("agent_step".to_string())
        );
    }

    #[test]
    fn agent_step_registration_declares_bounded_reserved_capability() {
        let pool = test_pool();
        register_agent_step_executor(&pool, Arc::new(NoopNodeExecutor), 3);

        let entry = pool
            .snapshot()
            .into_iter()
            .find(|entry| entry.executor_type == "agent_step")
            .expect("agent_step executor should be registered");
        assert_eq!(entry.capabilities.supported_task_types, ["agent_step"]);
        assert_eq!(entry.capabilities.supported_task_domains, ["agent_runtime"]);
        assert!(entry.capabilities.requires_auth);
        assert_eq!(entry.status.concurrency_limit, 3);
    }

    #[test]
    fn reserved_opencode_external_never_falls_back_to_generic_executor() {
        let pool = test_pool();
        register_noop(&pool, 10);
        register_code_executor(&pool, "code-wildcard", 5);
        // Generic/stub/command-like wildcards must not execute opencode_external.
        assert_eq!(
            pool.best_for_task(crate::opencode_runtime::OPENCODE_TASK_TYPE, "code"),
            None
        );
        assert_eq!(
            pool.best_for_task(
                crate::opencode_runtime::OPENCODE_TASK_TYPE,
                "external_runtime"
            ),
            None
        );

        register_opencode_runtime_executor(&pool, Arc::new(NoopNodeExecutor), 1, 30_000);
        assert_eq!(
            pool.best_for_task(crate::opencode_runtime::OPENCODE_TASK_TYPE, "code"),
            Some(crate::opencode_runtime::OPENCODE_EXECUTOR_TYPE.to_string())
        );
    }

    #[test]
    fn opencode_registration_declares_exact_capability_only() {
        let pool = test_pool();
        register_opencode_runtime_executor(&pool, Arc::new(NoopNodeExecutor), 2, 15_000);
        let entry = pool
            .snapshot()
            .into_iter()
            .find(|entry| entry.executor_type == crate::opencode_runtime::OPENCODE_EXECUTOR_TYPE)
            .expect("opencode executor registered");
        assert_eq!(
            entry.capabilities.supported_task_types,
            [crate::opencode_runtime::OPENCODE_TASK_TYPE]
        );
        assert_eq!(entry.status.concurrency_limit, 2);
        assert_eq!(entry.capabilities.max_timeout_ms, 15_000);
    }

    #[test]
    fn adaptive_provider_registration_is_explicit_bounded_and_authenticated() {
        let pool = test_pool();
        register_adaptive_provider_executor(&pool, Arc::new(NoopNodeExecutor), 2);

        let entry = pool
            .snapshot()
            .into_iter()
            .find(|entry| entry.executor_type == "adaptive_provider")
            .expect("adaptive provider registration");
        assert_eq!(
            entry.capabilities.supported_task_types,
            ["adaptive_provider"]
        );
        assert_eq!(entry.capabilities.supported_task_domains, ["adaptive"]);
        assert!(entry.capabilities.requires_auth);
        assert!(!entry.capabilities.requires_cli);
        assert_eq!(entry.status.concurrency_limit, 2);
        assert_eq!(
            pool.best_for_task("adaptive_provider", "adaptive"),
            Some("adaptive_provider".to_string())
        );
    }

    #[test]
    fn registered_cli_executor_is_policy_wrapped_and_cannot_spawn_when_denied() {
        let pool = test_pool();
        let store = Arc::new(LocalProductStore::new(":memory:").unwrap());
        let config = crate::cli::CliConfig {
            enabled: true,
            claude_code_bin: None,
            claude_code_enabled: false,
            claude_code_admission: None,
            codex_bin: Some("/definitely/not/executed".to_string()),
            codex_enabled: true,
            codex_admission: None,
            timeout_ms: 1_000,
        };
        register_cli_executors(&pool, &config, store);

        let entry = pool
            .snapshot()
            .into_iter()
            .find(|entry| entry.executor_type == "codex_cli")
            .expect("codex CLI registration");
        assert_eq!(entry.capabilities.supported_task_types, ["codex_cli"]);
        assert!(entry.capabilities.requires_auth);
        assert!(entry.capabilities.requires_cli);

        let output = pool
            .get("codex_cli")
            .expect("codex CLI executor")
            .execute_node(&crate::node_executor::NodeExecutionInput {
                node_id: "node-cli".to_string(),
                task_type: "codex_cli".to_string(),
                run_id: "run-cli".to_string(),
                workflow_id: "workflow-cli".to_string(),
                node_metadata: serde_json::json!({
                    "executor": "codex_cli",
                    "prompt": "bounded fixture prompt",
                    "profile_id": "default",
                }),
            });
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("cli_workspace_not_bound")
        );
    }

    #[cfg(unix)]
    #[test]
    fn exact_claude_admission_registers_one_policy_wrapped_managed_executor() {
        use sha2::{Digest, Sha256};
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("claude-2.1.217");
        std::fs::write(
            &binary,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then printf '2.1.217 (Claude Code)\\n'; exit 0; fi\nexit 2\n",
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let digest = hex::encode(Sha256::digest(std::fs::read(&binary).unwrap()));
        let admission = crate::cli::config::ClaudeCodeAdmission::validate(
            &binary,
            "2.1.217",
            &digest,
            Some(crate::cli::config::ADMITTED_CLAUDE_CODE_MODEL),
            3,
            2.16,
        )
        .unwrap();
        let pool = test_pool();
        let config = crate::cli::CliConfig {
            enabled: true,
            claude_code_bin: binary.to_str().map(str::to_string),
            claude_code_enabled: true,
            claude_code_admission: Some(admission),
            codex_bin: None,
            codex_enabled: false,
            codex_admission: None,
            timeout_ms: 1_000,
        };
        register_cli_executors(
            &pool,
            &config,
            Arc::new(LocalProductStore::new(":memory:").unwrap()),
        );

        let entry = pool
            .snapshot()
            .into_iter()
            .find(|entry| entry.executor_type == "claude_code_cli")
            .expect("exact Claude admission registration");
        assert_eq!(entry.capabilities.supported_task_types, ["claude_code_cli"]);
        assert!(entry.capabilities.supported_task_domains.is_empty());
        assert_eq!(entry.status.concurrency_limit, 1);
        assert_eq!(entry.cost_profile.cost_per_execution_usd, Some(2.16));
        assert_eq!(
            pool.best_for_task("claude_code_cli", "docs"),
            Some("claude_code_cli".to_string())
        );
    }

    #[test]
    fn policy_bound_cli_registers_only_sandboxed_codex_and_rejects_cross_identity() {
        let pool = test_pool();
        let store = Arc::new(LocalProductStore::new(":memory:").unwrap());
        let config = crate::cli::CliConfig {
            enabled: true,
            claude_code_bin: Some("/definitely/not/executed-claude".to_string()),
            claude_code_enabled: true,
            claude_code_admission: None,
            codex_bin: Some("/definitely/not/executed-codex".to_string()),
            codex_enabled: true,
            codex_admission: None,
            timeout_ms: 1_000,
        };
        register_cli_executors(&pool, &config, store);

        assert!(pool.get("claude_code_cli").is_none());
        let output = pool
            .get("codex_cli")
            .expect("policy-bound Codex CLI")
            .execute_node(&crate::node_executor::NodeExecutionInput {
                node_id: "node-codex-cli".to_string(),
                task_type: "codex_cli".to_string(),
                run_id: "run-cli-identity".to_string(),
                workflow_id: "workflow-cli-identity".to_string(),
                node_metadata: serde_json::json!({
                    "executor": "claude_code_cli",
                    "prompt": "must not launch",
                }),
            });
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("tool_policy_executor_mismatch")
        );
    }

    #[test]
    fn command_task_selects_policy_wrapped_command_never_noop() {
        let pool = test_pool();
        let store = Arc::new(LocalProductStore::new(":memory:").unwrap());
        store
            .configure_tool_allowlist("operator", "default", &[], None)
            .expect("configure authoritative empty allowlist");
        register_default_executors(&pool, false, store);

        assert_eq!(
            pool.best_for_task("command", "command"),
            Some("command".to_string())
        );
        let output = pool.get("command").expect("command executor").execute_node(
            &crate::node_executor::NodeExecutionInput {
                node_id: "node-command".to_string(),
                task_type: "command".to_string(),
                run_id: "run-command".to_string(),
                workflow_id: "workflow-command".to_string(),
                node_metadata: serde_json::json!({
                    "command": "echo bounded",
                    "profile_id": "default",
                }),
            },
        );
        assert_eq!(output.executor_type, "command");
        assert_eq!(output.error_domain.as_deref(), Some("tool_not_allowed"));
    }

    #[test]
    fn exact_execution_task_does_not_fall_back_to_wildcard_executor() {
        let pool = test_pool();
        register_noop(&pool, 10);

        assert_eq!(pool.best_for_task("adaptive_provider", "adaptive"), None);
        assert_eq!(pool.best_for_task("codex_cli", "code"), None);
        assert_eq!(pool.best_for_task("command", "command"), None);
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
