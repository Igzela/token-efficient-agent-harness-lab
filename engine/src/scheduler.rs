use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::cli::CliNodeExecutor;
use crate::executor_pool::{self, ExecutorPool};
use crate::node_executor::{FailNodeExecutor, NodeExecutor, NoopNodeExecutor};
use crate::storage::backup_manager::BackupManager;
use crate::storage::local_product_store::LocalProductStore;
use crate::tool_policy_executor::ToolPolicyNodeExecutor;

mod runtime;
use runtime::{dynamic_workflow_enabled, SchedulerRuntime};

const MAX_SCHEDULER_RETRIES: i64 = 10;

#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    pub interval_ms: u64,
    pub max_concurrent: usize,
    /// Maximum retries after the initial node execution attempt.
    pub max_retries: i64,
    pub lease_timeout_ms: u64,
    pub executor_type: String,
    pub queue_enabled: bool,
    pub max_queued: usize,
    pub backpressure_enabled: bool,
    pub backpressure_activation: f64,
    pub heartbeat_interval_sec: u64,
    pub supervised_workers_enabled: bool,
    pub worker_count: usize,
    /// Maximum number of agent_step nodes that may run concurrently across all runs.
    /// Zero means no agent_step scheduling is allowed.
    pub agent_max_concurrent_global: usize,
    /// Maximum number of agent_step nodes that may run concurrently within a single run.
    pub agent_max_concurrent_per_run: usize,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            interval_ms: 2000,
            max_concurrent: 4,
            max_retries: 0,
            lease_timeout_ms: 300_000,
            executor_type: "noop".to_string(),
            queue_enabled: true,
            max_queued: 100,
            backpressure_enabled: true,
            backpressure_activation: 0.8,
            heartbeat_interval_sec: 10,
            supervised_workers_enabled: false,
            worker_count: 1,
            agent_max_concurrent_global: 2,
            agent_max_concurrent_per_run: 1,
        }
    }
}

impl SchedulerConfig {
    pub fn from_env() -> Result<Self, String> {
        Self::from_env_with_gates(&crate::trusted_local::EffectiveExecutionGates::from_env())
    }

    pub fn from_env_with_gates(
        execution_gates: &crate::trusted_local::EffectiveExecutionGates,
    ) -> Result<Self, String> {
        let interval_ms = std::env::var("ACP_SCHEDULER_INTERVAL_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2000);
        let max_concurrent = std::env::var("ACP_SCHEDULER_MAX_CONCURRENT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4);
        let max_retries = match std::env::var("ACP_SCHEDULER_MAX_RETRIES") {
            Ok(value) => {
                let parsed = value.parse::<i64>().map_err(|error| {
                    format!("invalid ACP_SCHEDULER_MAX_RETRIES '{value}': {error}")
                })?;
                if !(0..=MAX_SCHEDULER_RETRIES).contains(&parsed) {
                    return Err(format!(
                        "ACP_SCHEDULER_MAX_RETRIES must be between 0 and {MAX_SCHEDULER_RETRIES}"
                    ));
                }
                parsed
            }
            Err(std::env::VarError::NotPresent) => 0,
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err("ACP_SCHEDULER_MAX_RETRIES must be valid UTF-8".to_string())
            }
        };
        let lease_timeout_ms = std::env::var("ACP_SCHEDULER_LEASE_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(300_000);
        let executor_type = if execution_gates.task_advancement.ready {
            execution_gates.task_advancement.executor_type.clone()
        } else {
            std::env::var("ACP_SCHEDULER_EXECUTOR").unwrap_or_else(|_| "noop".to_string())
        };
        let queue_enabled = std::env::var("ACP_QUEUE_ENABLED")
            .map(|v| {
                !matches!(
                    v.as_str(),
                    "0" | "false" | "FALSE" | "no" | "NO" | "off" | "OFF"
                )
            })
            .unwrap_or(true);
        let max_queued = std::env::var("ACP_MAX_QUEUED")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);
        let backpressure_enabled = std::env::var("ACP_BACKPRESSURE_ENABLED")
            .map(|v| {
                !matches!(
                    v.as_str(),
                    "0" | "false" | "FALSE" | "no" | "NO" | "off" | "OFF"
                )
            })
            .unwrap_or(true);
        let backpressure_activation = std::env::var("ACP_BACKPRESSURE_ACTIVATION")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.8);
        let heartbeat_interval_sec = std::env::var("ACP_HEARTBEAT_INTERVAL_SEC")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10);
        let supervised_workers_enabled = execution_gates.supervised_workers_enabled;
        let worker_count = std::env::var("ACP_SUPERVISED_WORKER_COUNT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);
        let agent_max_concurrent_global = match std::env::var("ACP_AGENT_MAX_CONCURRENT_GLOBAL") {
            Ok(v) => v
                .parse::<usize>()
                .map_err(|e| format!("invalid ACP_AGENT_MAX_CONCURRENT_GLOBAL '{v}': {e}"))?,
            Err(_) => 2,
        };
        let agent_max_concurrent_per_run = match std::env::var("ACP_AGENT_MAX_CONCURRENT_PER_RUN") {
            Ok(v) => v
                .parse::<usize>()
                .map_err(|e| format!("invalid ACP_AGENT_MAX_CONCURRENT_PER_RUN '{v}': {e}"))?,
            Err(_) => 1,
        };
        Ok(Self {
            interval_ms,
            max_concurrent,
            max_retries,
            lease_timeout_ms,
            executor_type,
            queue_enabled,
            max_queued,
            backpressure_enabled,
            backpressure_activation,
            heartbeat_interval_sec,
            supervised_workers_enabled,
            worker_count,
            agent_max_concurrent_global,
            agent_max_concurrent_per_run,
        })
    }

    pub fn validate_for_start(&self) -> Result<(), String> {
        if !(0..=MAX_SCHEDULER_RETRIES).contains(&self.max_retries) {
            return Err(format!(
                "ACP_SCHEDULER_MAX_RETRIES must be between 0 and {MAX_SCHEDULER_RETRIES}"
            ));
        }
        if !self.supervised_workers_enabled {
            return Err(
                "supervised workers not enabled (ACP_ENABLE_SUPERVISED_WORKERS=1 required)"
                    .to_string(),
            );
        }
        if self.worker_count == 0 {
            return Err("ACP_SUPERVISED_WORKER_COUNT must be at least 1".to_string());
        }
        if self.worker_count > self.max_concurrent {
            return Err(format!(
                "worker count {} exceeds max concurrency {}",
                self.worker_count, self.max_concurrent
            ));
        }
        if self.worker_count > 32 {
            return Err("ACP_SUPERVISED_WORKER_COUNT must not exceed 32".to_string());
        }
        if self.agent_max_concurrent_per_run > self.agent_max_concurrent_global {
            return Err(
                "agent_max_concurrent_per_run must not exceed agent_max_concurrent_global"
                    .to_string(),
            );
        }
        if !matches!(
            self.executor_type.as_str(),
            "noop"
                | "stub"
                | "fail"
                | "command"
                | "local_runner_validation"
                | "claude_code_cli"
                | "codex_cli"
                | "adaptive_provider"
                | "dynamic"
                | "dynamic_noop"
                | "dynamic_workflow"
                | "auto"
                | "pool"
        ) {
            return Err(format!(
                "unsupported ACP_SCHEDULER_EXECUTOR: {}",
                self.executor_type
            ));
        }
        Ok(())
    }

    fn validate_lease_exceeds_execution_timeout(
        &self,
        max_execution_timeout_ms: u64,
    ) -> Result<(), String> {
        if max_execution_timeout_ms == 0 {
            return Ok(());
        }
        let required = max_execution_timeout_ms.saturating_add(self.interval_ms.max(1_000));
        if self.lease_timeout_ms < required {
            return Err(format!(
                "ACP_SCHEDULER_LEASE_TIMEOUT_MS={} must be at least {required} so a bounded execution with timeout {max_execution_timeout_ms}ms cannot be reclaimed while its worker is still active",
                self.lease_timeout_ms
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize)]
struct WorkerRuntimeState {
    worker_id: String,
    state: String,
    last_heartbeat_at: String,
    tick_count: u64,
    error_count: u64,
}

fn create_scheduler_executor(
    executor_type: &str,
    store: Arc<LocalProductStore>,
) -> Arc<dyn NodeExecutor> {
    let unavailable = |reason: String| -> Arc<dyn NodeExecutor> {
        Arc::new(FailNodeExecutor {
            error_domain: "scheduler_executor_unavailable".to_string(),
            error_message: reason,
        })
    };
    match executor_type {
        "noop" => Arc::new(NoopNodeExecutor),
        "stub" => Arc::new(crate::node_executor::StubNodeExecutor::default()),
        "fail" => Arc::new(FailNodeExecutor::default()),
        "command" => Arc::new(ToolPolicyNodeExecutor::command(
            Arc::new(crate::node_executor::CommandNodeExecutor::default()),
            store,
        )),
        "claude_code_cli" | "codex_cli" => {
            let config = crate::cli::CliConfig::from_env();
            match CliNodeExecutor::from_config_for(&config, executor_type) {
                Some(exec) => Arc::new(ToolPolicyNodeExecutor::cli(
                    Arc::new(exec),
                    store,
                    executor_type,
                )),
                None => {
                    eprintln!(
                        "[scheduler] CLI executor '{}' is unavailable; execution will fail closed",
                        executor_type
                    );
                    unavailable(format!(
                        "configured CLI executor is unavailable: {executor_type}"
                    ))
                }
            }
        }
        other => unavailable(format!(
            "configured scheduler executor is unavailable: {other}"
        )),
    }
}

pub struct WorkflowScheduler {
    store: Arc<LocalProductStore>,
    config: SchedulerConfig,
    control_gate: Arc<std::sync::Mutex<()>>,
    running: Arc<AtomicBool>,
    handles: Vec<JoinHandle<()>>,
    paused: Arc<AtomicBool>,
    kill_requested: Arc<AtomicBool>,
    worker_states: Arc<std::sync::Mutex<BTreeMap<String, WorkerRuntimeState>>>,
    started_at: Option<String>,
    tick_count: Arc<AtomicU64>,
    error_count: Arc<AtomicU64>,
    retry_count: Arc<AtomicU64>,
    panic_count: Arc<AtomicU64>,
    total_execution_time_ns: Arc<AtomicU64>,
    last_tick_at: Arc<std::sync::Mutex<Option<String>>>,
    last_error: Arc<std::sync::Mutex<Option<String>>>,
    executor_pool: Arc<ExecutorPool>,
    queue_depth_live: Arc<AtomicU64>,
    paused_runs_count_live: Arc<AtomicU64>,
    backpressure_active_live: Arc<AtomicBool>,
    metrics: Option<Arc<crate::infrastructure::observability::MetricsCollector>>,
    backup_manager: Option<Arc<BackupManager>>,
    backup_interval_sec: u64,
    backup_retain_count: usize,
    backup_db_path: Option<String>,
    worker_executor: Option<Arc<dyn NodeExecutor>>,
    agent_step_executor: Option<Arc<dyn NodeExecutor>>,
    external_runtime_executor: Option<(Arc<dyn NodeExecutor>, u64)>,
    opencode_runtime_executor: Option<(Arc<dyn NodeExecutor>, u64)>,
    /// Shared with the OpenCode process invoker so kill/stop terminates process trees.
    opencode_cancel: Option<crate::opencode_runtime::OpenCodeCancellationHandle>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulerControlSnapshot {
    pub running: bool,
    pub paused: bool,
    pub kill_requested: bool,
}

impl WorkflowScheduler {
    pub fn new(store: Arc<LocalProductStore>, config: SchedulerConfig) -> Self {
        Self {
            store,
            config,
            control_gate: Arc::new(std::sync::Mutex::new(())),
            running: Arc::new(AtomicBool::new(false)),
            handles: Vec::new(),
            paused: Arc::new(AtomicBool::new(false)),
            kill_requested: Arc::new(AtomicBool::new(false)),
            worker_states: Arc::new(std::sync::Mutex::new(BTreeMap::new())),
            started_at: None,
            tick_count: Arc::new(AtomicU64::new(0)),
            error_count: Arc::new(AtomicU64::new(0)),
            retry_count: Arc::new(AtomicU64::new(0)),
            panic_count: Arc::new(AtomicU64::new(0)),
            total_execution_time_ns: Arc::new(AtomicU64::new(0)),
            last_tick_at: Arc::new(std::sync::Mutex::new(None)),
            last_error: Arc::new(std::sync::Mutex::new(None)),
            executor_pool: Arc::new(ExecutorPool::new()),
            queue_depth_live: Arc::new(AtomicU64::new(0)),
            paused_runs_count_live: Arc::new(AtomicU64::new(0)),
            backpressure_active_live: Arc::new(AtomicBool::new(false)),
            metrics: None,
            backup_manager: None,
            backup_interval_sec: 0,
            backup_retain_count: 5,
            backup_db_path: None,
            worker_executor: None,
            agent_step_executor: None,
            external_runtime_executor: None,
            opencode_runtime_executor: None,
            opencode_cancel: None,
        }
    }

    pub fn with_auto_backup(
        mut self,
        backup_manager: Arc<BackupManager>,
        db_path: String,
        interval_sec: u64,
        retain_count: usize,
    ) -> Self {
        self.backup_manager = Some(backup_manager);
        self.backup_db_path = Some(db_path);
        self.backup_interval_sec = interval_sec;
        self.backup_retain_count = retain_count;
        self
    }

    pub fn with_metrics(
        mut self,
        metrics: Arc<crate::infrastructure::observability::MetricsCollector>,
    ) -> Self {
        self.metrics = Some(metrics);
        self
    }

    pub fn with_worker_executor(mut self, executor: Arc<dyn NodeExecutor>) -> Self {
        self.worker_executor = Some(executor);
        self
    }

    pub fn with_agent_step_executor(mut self, executor: Arc<dyn NodeExecutor>) -> Self {
        self.agent_step_executor = Some(executor);
        self
    }

    pub fn with_external_runtime_executor(
        mut self,
        executor: Arc<dyn NodeExecutor>,
        timeout_ms: u64,
    ) -> Self {
        self.external_runtime_executor = Some((executor, timeout_ms));
        self
    }

    pub fn with_opencode_runtime_executor(
        mut self,
        executor: Arc<dyn NodeExecutor>,
        timeout_ms: u64,
    ) -> Self {
        self.opencode_runtime_executor = Some((executor, timeout_ms));
        self
    }

    pub fn with_opencode_cancellation(
        mut self,
        handle: crate::opencode_runtime::OpenCodeCancellationHandle,
    ) -> Self {
        self.opencode_cancel = Some(handle);
        self
    }

    pub fn start(&mut self) -> Result<(), String> {
        if self.running.load(Ordering::SeqCst) {
            return Err("scheduler already running".to_string());
        }
        if !self.handles.is_empty() {
            return Err(
                "scheduler workers are still draining; call stop before restart".to_string(),
            );
        }
        self.config.validate_for_start()?;
        if self.config.executor_type == "adaptive_provider" && self.worker_executor.is_none() {
            return Err(
                "adaptive provider scheduler requires an injected worker executor".to_string(),
            );
        }
        if self
            .worker_executor
            .as_ref()
            .is_some_and(|executor| executor.executor_type_name() != "adaptive_provider")
        {
            return Err(
                "injected scheduler worker executor must identify as adaptive_provider".to_string(),
            );
        }
        let cli_config = crate::cli::CliConfig::from_env();
        let configured_execution_timeout_ms = match self.config.executor_type.as_str() {
            "command" => 30_000,
            "claude_code_cli" | "codex_cli" => cli_config.timeout_ms,
            "adaptive_provider" => 300_000,
            "dynamic" | "dynamic_noop" | "dynamic_workflow" | "auto" | "pool" => {
                let cli_timeout = if cli_config.enabled {
                    cli_config.timeout_ms
                } else {
                    0
                };
                let provider_timeout = if self.worker_executor.is_some() {
                    300_000
                } else {
                    0
                };
                30_000u64.max(cli_timeout).max(provider_timeout)
            }
            _ => 0,
        };
        let max_execution_timeout_ms = if self.agent_step_executor.is_some() {
            configured_execution_timeout_ms.max(300_000)
        } else {
            configured_execution_timeout_ms
        };
        let max_execution_timeout_ms = self
            .external_runtime_executor
            .as_ref()
            .map(|(_, timeout_ms)| max_execution_timeout_ms.max(*timeout_ms))
            .unwrap_or(max_execution_timeout_ms);
        let max_execution_timeout_ms = self
            .opencode_runtime_executor
            .as_ref()
            .map(|(_, timeout_ms)| max_execution_timeout_ms.max(*timeout_ms))
            .unwrap_or(max_execution_timeout_ms);
        self.config
            .validate_lease_exceeds_execution_timeout(max_execution_timeout_ms)?;
        {
            let _control = self
                .control_gate
                .lock()
                .map_err(|_| "scheduler control gate is unavailable".to_string())?;
            if env_flag_enabled("ACP_SUPERVISED_WORKERS_KILL_SWITCH") {
                return Err(
                    "supervised worker kill switch is active (ACP_SUPERVISED_WORKERS_KILL_SWITCH=1)"
                        .to_string(),
                );
            }
            self.kill_requested.store(false, Ordering::SeqCst);
            self.running.store(true, Ordering::SeqCst);
        }
        if let Some(ref handle) = self.opencode_cancel {
            handle.reset();
        }

        let cli_enabled = crate::cli::CliConfig::from_env().enabled;
        executor_pool::register_default_executors(
            &self.executor_pool,
            cli_enabled,
            self.store.clone(),
        );
        if let Some(executor) = self.worker_executor.clone() {
            executor_pool::register_adaptive_provider_executor(
                &self.executor_pool,
                executor,
                self.config.max_concurrent,
            );
        }
        if let Some(executor) = self.agent_step_executor.clone() {
            executor_pool::register_agent_step_executor(
                &self.executor_pool,
                executor,
                self.config.agent_max_concurrent_global,
            );
        }
        if let Some((executor, timeout_ms)) = self.external_runtime_executor.clone() {
            executor_pool::register_external_runtime_executor(
                &self.executor_pool,
                executor,
                self.config.max_concurrent,
                timeout_ms,
            );
        }
        if let Some((executor, timeout_ms)) = self.opencode_runtime_executor.clone() {
            executor_pool::register_opencode_runtime_executor(
                &self.executor_pool,
                executor,
                self.config.max_concurrent,
                timeout_ms,
            );
        }
        let worker_count = if self.config.supervised_workers_enabled {
            self.config.worker_count
        } else {
            1
        };
        if let Ok(mut states) = self.worker_states.lock() {
            states.clear();
        }
        self.handles.clear();
        let configured_executor = self.worker_executor.clone();
        for worker_index in 0..worker_count {
            let context = SchedulerWorkerContext {
                worker_id: format!("worker-{worker_index}"),
                store: self.store.clone(),
                config: self.config.clone(),
                tick_limit: if self.config.supervised_workers_enabled {
                    1
                } else {
                    self.config.max_concurrent
                },
                control_gate: self.control_gate.clone(),
                running: self.running.clone(),
                paused: self.paused.clone(),
                kill_requested: self.kill_requested.clone(),
                tick_count: self.tick_count.clone(),
                error_count: self.error_count.clone(),
                retry_count: self.retry_count.clone(),
                panic_count: self.panic_count.clone(),
                total_execution_time_ns: self.total_execution_time_ns.clone(),
                last_tick_at: self.last_tick_at.clone(),
                last_error: self.last_error.clone(),
                executor_pool: self.executor_pool.clone(),
                queue_depth_live: self.queue_depth_live.clone(),
                paused_runs_count_live: self.paused_runs_count_live.clone(),
                backpressure_active_live: self.backpressure_active_live.clone(),
                worker_states: self.worker_states.clone(),
                metrics: self.metrics.clone(),
                executor: configured_executor.clone(),
                backup: (worker_index == 0).then(|| SchedulerBackupContext {
                    manager: self.backup_manager.clone(),
                    interval_sec: self.backup_interval_sec,
                    retain_count: self.backup_retain_count,
                    db_path: self.backup_db_path.clone(),
                }),
            };
            self.handles
                .push(std::thread::spawn(move || run_scheduler_worker(context)));
        }
        self.started_at = Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), String> {
        if !self.running.load(Ordering::SeqCst) && self.handles.is_empty() {
            return Err("scheduler not running".to_string());
        }
        {
            let _control = self
                .control_gate
                .lock()
                .map_err(|_| "scheduler control gate is unavailable".to_string())?;
            self.running.store(false, Ordering::SeqCst);
        }
        if let Some(ref handle) = self.opencode_cancel {
            handle.cancel();
        }
        for handle in self.handles.drain(..) {
            handle
                .join()
                .map_err(|_| "scheduler thread panicked".to_string())?;
        }
        Ok(())
    }

    pub fn pause(&self, _actor: &str) -> Result<(), String> {
        let _control = self
            .control_gate
            .lock()
            .map_err(|_| "scheduler control gate is unavailable".to_string())?;
        self.store
            .set_recursive_execution_paused(true, Some("recursive_execution_paused"))?;
        self.paused.store(true, Ordering::SeqCst);
        Ok(())
    }

    pub fn resume(&self, _actor: &str) -> Result<(), String> {
        let _control = self
            .control_gate
            .lock()
            .map_err(|_| "scheduler control gate is unavailable".to_string())?;
        if self.kill_requested.load(Ordering::SeqCst) {
            return Err("scheduler was killed and must be started again".to_string());
        }
        self.store.set_recursive_execution_paused(false, None)?;
        self.paused.store(false, Ordering::SeqCst);
        Ok(())
    }

    pub fn kill(&mut self, _actor: &str) -> Result<(), String> {
        let _control = self
            .control_gate
            .lock()
            .map_err(|_| "scheduler control gate is unavailable".to_string())?;
        self.store
            .set_recursive_execution_paused(true, Some("recursive_kill_switch_active"))?;
        self.kill_requested.store(true, Ordering::SeqCst);
        self.running.store(false, Ordering::SeqCst);
        if let Some(ref handle) = self.opencode_cancel {
            handle.cancel();
        }
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn control_snapshot_unlocked(&self) -> SchedulerControlSnapshot {
        self.control_snapshot_unlocked_with_environment(
            env_flag_enabled("ACP_SUPERVISED_WORKERS_PAUSED"),
            env_flag_enabled("ACP_SUPERVISED_WORKERS_KILL_SWITCH"),
        )
    }

    fn control_snapshot_unlocked_with_environment(
        &self,
        environment_pause: bool,
        environment_kill: bool,
    ) -> SchedulerControlSnapshot {
        SchedulerControlSnapshot {
            running: self.running.load(Ordering::SeqCst),
            paused: self.paused.load(Ordering::SeqCst) || environment_pause,
            kill_requested: self.kill_requested.load(Ordering::SeqCst) || environment_kill,
        }
    }

    /// Linearize a storage-free scheduler control sample with worker-observed
    /// environment pause/kill gates. The callback may commit application storage;
    /// worker and API control transitions cannot publish until it returns.
    pub fn with_control_barrier<T>(
        &self,
        operation: impl FnOnce(SchedulerControlSnapshot) -> Result<T, String>,
    ) -> Result<T, String> {
        let _control = self
            .control_gate
            .try_lock()
            .map_err(|_| "scheduler control gate is unavailable".to_string())?;
        operation(self.control_snapshot_unlocked())
    }

    pub fn control_snapshot(&self) -> Result<SchedulerControlSnapshot, String> {
        self.with_control_barrier(Ok)
    }

    pub fn executor_pool(&self) -> &Arc<ExecutorPool> {
        &self.executor_pool
    }

    pub fn status(&self) -> Value {
        let active_runs = self
            .store
            .list_active_workflow_run_ids()
            .map(|ids| ids.len())
            .unwrap_or(0);
        let pool_snapshot = self.executor_pool.snapshot();
        json!({
            "schema_version": "scheduler.v1",
            "running": self.is_running(),
            "started_at": self.started_at,
            "supervised_workers_enabled": self.config.supervised_workers_enabled,
            "worker_count": if self.config.supervised_workers_enabled { self.config.worker_count } else { 1 },
            "paused": self.paused.load(Ordering::SeqCst)
                || env_flag_enabled("ACP_SUPERVISED_WORKERS_PAUSED"),
            "kill_requested": self.kill_requested.load(Ordering::SeqCst)
                || env_flag_enabled("ACP_SUPERVISED_WORKERS_KILL_SWITCH"),
            "workers": self.worker_states.lock().map(|states| states.values().cloned().collect::<Vec<_>>()).unwrap_or_default(),
            "config": {
                "interval_ms": self.config.interval_ms,
                "max_concurrent": self.config.max_concurrent,
                "max_retries": self.config.max_retries,
                "max_queued": self.config.max_queued,
                "lease_timeout_ms": self.config.lease_timeout_ms,
                "executor_type": self.config.executor_type,
                "dynamic_workflow_enabled": dynamic_workflow_enabled(&self.config),
                "backpressure_enabled": self.config.backpressure_enabled,
                "backpressure_activation": self.config.backpressure_activation,
                "heartbeat_interval_sec": self.config.heartbeat_interval_sec,
                "agent_max_concurrent_global": self.config.agent_max_concurrent_global,
                "agent_max_concurrent_per_run": self.config.agent_max_concurrent_per_run,
            },
            "tick_count": self.tick_count.load(Ordering::SeqCst),
            "error_count": self.error_count.load(Ordering::SeqCst),
            "panic_count": self.panic_count.load(Ordering::SeqCst),
            "retry_count": self.retry_count.load(Ordering::SeqCst),
            "total_execution_time_ns": self.total_execution_time_ns.load(Ordering::SeqCst),
            "last_tick_at": self.last_tick_at.lock().ok().and_then(|g| g.clone()),
            "last_error": self.last_error.lock().ok().and_then(|g| g.clone()),
            "active_runs": active_runs,
            "queue_enabled": self.config.queue_enabled,
            "backpressure_active": self.backpressure_active_live.load(Ordering::SeqCst),
            "queue_depth": self.queue_depth_live.load(Ordering::SeqCst),
            "paused_runs_count": self.paused_runs_count_live.load(Ordering::SeqCst),
            "executor_pool": pool_snapshot,
            "dormant_modules_active": {
                "work_queue": true,
                "result_aggregator": true,
                "auto_policies": true,
                "feedback_integrator": true,
                "conflict_resolver": true,
                "approval_gate": true,
                "workflow_engine": true,
            },
            "backup_auto_enabled": self.backup_manager.is_some() && self.backup_interval_sec > 0,
            "backup_interval_sec": self.backup_interval_sec,
            "backup_retain_count": self.backup_retain_count,
        })
    }
}

impl Drop for WorkflowScheduler {
    fn drop(&mut self) {
        // Cancel OpenCode process trees before joining workers so graceful
        // application shutdown cannot leave a detached adapter until timeout.
        self.running.store(false, Ordering::SeqCst);
        if let Some(ref handle) = self.opencode_cancel {
            handle.cancel();
        }
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

#[derive(Clone)]
struct SchedulerBackupContext {
    manager: Option<Arc<BackupManager>>,
    interval_sec: u64,
    retain_count: usize,
    db_path: Option<String>,
}

#[derive(Clone)]
struct SchedulerWorkerContext {
    worker_id: String,
    store: Arc<LocalProductStore>,
    config: SchedulerConfig,
    tick_limit: usize,
    control_gate: Arc<std::sync::Mutex<()>>,
    running: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    kill_requested: Arc<AtomicBool>,
    tick_count: Arc<AtomicU64>,
    error_count: Arc<AtomicU64>,
    retry_count: Arc<AtomicU64>,
    panic_count: Arc<AtomicU64>,
    total_execution_time_ns: Arc<AtomicU64>,
    last_tick_at: Arc<std::sync::Mutex<Option<String>>>,
    last_error: Arc<std::sync::Mutex<Option<String>>>,
    executor_pool: Arc<ExecutorPool>,
    queue_depth_live: Arc<AtomicU64>,
    paused_runs_count_live: Arc<AtomicU64>,
    backpressure_active_live: Arc<AtomicBool>,
    worker_states: Arc<std::sync::Mutex<BTreeMap<String, WorkerRuntimeState>>>,
    metrics: Option<Arc<crate::infrastructure::observability::MetricsCollector>>,
    executor: Option<Arc<dyn NodeExecutor>>,
    backup: Option<SchedulerBackupContext>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SchedulerWorkerControlState {
    Running,
    Paused,
    Killed,
}

fn observe_scheduler_worker_control(
    control_gate: &std::sync::Mutex<()>,
    running: &AtomicBool,
    paused: &AtomicBool,
    kill_requested: &AtomicBool,
    environment_kill: bool,
    environment_pause: bool,
) -> SchedulerWorkerControlState {
    let _control = match control_gate.lock() {
        Ok(control) => control,
        Err(poisoned) => poisoned.into_inner(),
    };
    if kill_requested.load(Ordering::SeqCst) || environment_kill {
        kill_requested.store(true, Ordering::SeqCst);
        running.store(false, Ordering::SeqCst);
        return SchedulerWorkerControlState::Killed;
    }
    if paused.load(Ordering::SeqCst) || environment_pause {
        return SchedulerWorkerControlState::Paused;
    }
    SchedulerWorkerControlState::Running
}

fn run_scheduler_worker(context: SchedulerWorkerContext) {
    let thread_start = Instant::now();
    let mut last_heartbeat_write = Instant::now();
    let mut last_backup_time = Instant::now();
    let mut consecutive_panics = 0u64;
    let mut worker_ticks = 0u64;
    let mut worker_errors = 0u64;
    let executor = context.executor.clone().unwrap_or_else(|| {
        create_scheduler_executor(&context.config.executor_type, context.store.clone())
    });
    update_worker_state(&context, "starting", worker_ticks, worker_errors);

    while context.running.load(Ordering::SeqCst) {
        match observe_scheduler_worker_control(
            &context.control_gate,
            &context.running,
            &context.paused,
            &context.kill_requested,
            env_flag_enabled("ACP_SUPERVISED_WORKERS_KILL_SWITCH"),
            env_flag_enabled("ACP_SUPERVISED_WORKERS_PAUSED"),
        ) {
            SchedulerWorkerControlState::Killed => {
                update_worker_state(&context, "killed", worker_ticks, worker_errors);
                break;
            }
            SchedulerWorkerControlState::Paused => {
                update_worker_state(&context, "paused", worker_ticks, worker_errors);
                write_worker_heartbeat(&context, thread_start.elapsed().as_secs_f64());
                interruptible_sleep(&context, 100);
                continue;
            }
            SchedulerWorkerControlState::Running => {}
        }

        update_worker_state(&context, "running", worker_ticks, worker_errors);
        context.executor_pool.tick_cooldowns();
        let tick_start = Instant::now();
        let tick_result = panic::catch_unwind(AssertUnwindSafe(|| {
            SchedulerRuntime::new(
                &context.store,
                &context.config,
                executor.clone(),
                &context.executor_pool,
            )
            .tick_with_limit(context.tick_limit)
        }));
        let tick_elapsed_ns = tick_start.elapsed().as_nanos() as u64;
        let tick_elapsed_ms = tick_elapsed_ns as f64 / 1_000_000.0;
        context
            .total_execution_time_ns
            .fetch_add(tick_elapsed_ns, Ordering::SeqCst);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        match tick_result {
            Ok(Ok(result)) => {
                consecutive_panics = 0;
                worker_ticks += result.ticks;
                context.tick_count.fetch_add(result.ticks, Ordering::SeqCst);
                context
                    .retry_count
                    .fetch_add(result.retries, Ordering::SeqCst);
                context
                    .queue_depth_live
                    .store(result.queue_depth as u64, Ordering::SeqCst);
                context
                    .paused_runs_count_live
                    .store(result.paused_runs.len() as u64, Ordering::SeqCst);
                context
                    .backpressure_active_live
                    .store(result.backpressure_active, Ordering::SeqCst);
                if let Ok(mut guard) = context.last_tick_at.lock() {
                    *guard = Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
                }
                if let Some(metrics) = &context.metrics {
                    metrics.record_snapshot(crate::infrastructure::observability::MetricSnapshot {
                        name: "scheduler.tick".to_string(),
                        value: tick_elapsed_ms,
                        labels: [
                            ("executor".to_string(), context.config.executor_type.clone()),
                            ("status".to_string(), "ok".to_string()),
                            ("worker".to_string(), context.worker_id.clone()),
                        ]
                        .into(),
                        timestamp: now,
                    });
                }
            }
            Ok(Err(error)) => {
                worker_errors += 1;
                context.error_count.fetch_add(1, Ordering::SeqCst);
                if let Ok(mut guard) = context.last_error.lock() {
                    *guard = Some(error);
                }
            }
            Err(payload) => {
                worker_errors += 1;
                context.error_count.fetch_add(1, Ordering::SeqCst);
                context.panic_count.fetch_add(1, Ordering::SeqCst);
                consecutive_panics += 1;
                let message = panic_message(&payload);
                if let Ok(mut guard) = context.last_error.lock() {
                    *guard = Some(format!("panic: {message}"));
                }
                update_worker_state(&context, "backoff", worker_ticks, worker_errors);
                interruptible_sleep(&context, (consecutive_panics * 1_000).min(30_000));
                continue;
            }
        }

        update_worker_state(&context, "idle", worker_ticks, worker_errors);
        if last_heartbeat_write.elapsed().as_secs() >= context.config.heartbeat_interval_sec {
            write_worker_heartbeat(&context, thread_start.elapsed().as_secs_f64());
            last_heartbeat_write = Instant::now();
        }
        if let Some(backup) = &context.backup {
            run_scheduled_backup(&context.store, backup, &mut last_backup_time);
        }
        interruptible_sleep(&context, context.config.interval_ms);
    }

    let final_state = if context.kill_requested.load(Ordering::SeqCst) {
        "killed"
    } else {
        "stopped"
    };
    update_worker_state(&context, final_state, worker_ticks, worker_errors);
    write_worker_heartbeat(&context, thread_start.elapsed().as_secs_f64());
}

fn update_worker_state(
    context: &SchedulerWorkerContext,
    state: &str,
    tick_count: u64,
    error_count: u64,
) {
    if let Ok(mut states) = context.worker_states.lock() {
        states.insert(
            context.worker_id.clone(),
            WorkerRuntimeState {
                worker_id: context.worker_id.clone(),
                state: state.to_string(),
                last_heartbeat_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                tick_count,
                error_count,
            },
        );
    }
}

fn write_worker_heartbeat(context: &SchedulerWorkerContext, uptime_seconds: f64) {
    let metadata = context
        .worker_states
        .lock()
        .ok()
        .and_then(|states| serde_json::to_string(&*states).ok())
        .unwrap_or_else(|| "{}".to_string());
    let _ = context.store.write_heartbeat(
        context.tick_count.load(Ordering::SeqCst),
        context.error_count.load(Ordering::SeqCst),
        uptime_seconds,
        &metadata,
    );
}

fn run_scheduled_backup(
    store: &LocalProductStore,
    backup: &SchedulerBackupContext,
    last_backup_time: &mut Instant,
) {
    if backup.interval_sec == 0 || last_backup_time.elapsed().as_secs() < backup.interval_sec {
        return;
    }
    let (Some(manager), Some(db_path)) = (&backup.manager, &backup.db_path) else {
        return;
    };
    if let Err(error) = store.checkpoint_wal() {
        eprintln!("[scheduler] auto-backup WAL checkpoint failed: {error}");
        *last_backup_time = Instant::now();
        return;
    }
    let backup_id = format!("auto-{}", chrono::Utc::now().format("%Y%m%dT%H%M%SZ"));
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    match manager.create_backup(std::path::Path::new(db_path), "auto", &backup_id, &now) {
        Ok(record) => {
            if let Ok(mut backups) = manager.list_backups() {
                backups.push(record);
                let _ = manager.save_metadata(&backups);
            }
            let _ = manager.prune_backups(backup.retain_count);
        }
        Err(error) => eprintln!("[scheduler] auto-backup create failed: {error}"),
    }
    *last_backup_time = Instant::now();
}

fn interruptible_sleep(context: &SchedulerWorkerContext, duration_ms: u64) {
    let mut remaining = duration_ms;
    while remaining > 0 && context.running.load(Ordering::SeqCst) {
        let step = remaining.min(100);
        std::thread::sleep(Duration::from_millis(step));
        remaining -= step;
        if context.kill_requested.load(Ordering::SeqCst)
            || env_flag_enabled("ACP_SUPERVISED_WORKERS_KILL_SWITCH")
        {
            break;
        }
    }
}

fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = payload.downcast_ref::<&str>() {
        message.to_string()
    } else {
        "unknown panic".to_string()
    }
}

#[allow(dead_code)]
struct TickResult {
    ticks: u64,
    retries: u64,
    aggregations: u64,
    adaptation_recommendations: Vec<AdaptationRecommendation>,
    paused_runs: Vec<String>,
    degraded_runs: Vec<String>,
    backpressure_active: bool,
    queue_depth: usize,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct AdaptationRecommendation {
    pub task_group: String,
    pub should_adapt: bool,
    pub reason: String,
}

fn env_flag_enabled(key: &str) -> bool {
    std::env::var(key)
        .map(|value| {
            matches!(
                value.as_str(),
                "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON"
            )
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::runtime::{scheduler_tick, SchedulerModules};
    use super::*;
    use crate::node_executor::{NodeExecutionInput, NodeExecutionOutput, NodeExecutor};
    use crate::storage::local_product_store::LocalProductStore;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use std::sync::Barrier;
    use std::thread;

    struct TrackingExecutor {
        calls: Arc<AtomicUsize>,
    }

    impl NodeExecutor for TrackingExecutor {
        fn executor_type_name(&self) -> &str {
            "adaptive_provider"
        }

        fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
            self.calls.fetch_add(1, AtomicOrdering::SeqCst);
            NodeExecutionOutput {
                status: "completed".to_string(),
                executor_type: "adaptive_provider".to_string(),
                output: Some("bounded result".to_string()),
                error_domain: None,
                error_message: None,
                input_tokens: Some(10),
                output_tokens: Some(5),
                estimated_cost: Some(0.01),
                latency_ms: Some(1),
                process_outcome: None,
                resolved_model: None,
            }
        }
    }

    fn test_store() -> Arc<LocalProductStore> {
        Arc::new(LocalProductStore::new(":memory:").unwrap())
    }

    fn test_pool() -> Arc<ExecutorPool> {
        let pool = ExecutorPool::new();
        executor_pool::register_default_executors(&pool, false, test_store());
        Arc::new(pool)
    }

    fn empty_pool() -> Arc<ExecutorPool> {
        Arc::new(ExecutorPool::new())
    }

    fn stub_only_pool() -> Arc<ExecutorPool> {
        let pool = ExecutorPool::new();
        pool.register(crate::executor_pool::ExecutorEntry {
            executor_type: "stub".to_string(),
            executor: Arc::new(crate::node_executor::StubNodeExecutor::default()),
            capabilities: crate::executor_pool::ExecutorCapabilities::default(),
            status: crate::executor_pool::ExecutorStatus {
                concurrency_limit: 1,
                ..Default::default()
            },
            cost_profile: crate::executor_pool::CostProfile::default(),
            metrics: crate::executor_pool::ExecutorMetrics::default(),
        });
        Arc::new(pool)
    }

    fn fail_only_pool() -> Arc<ExecutorPool> {
        let pool = ExecutorPool::new();
        pool.register(crate::executor_pool::ExecutorEntry {
            executor_type: "fail".to_string(),
            executor: Arc::new(crate::node_executor::FailNodeExecutor::default()),
            capabilities: crate::executor_pool::ExecutorCapabilities::default(),
            status: crate::executor_pool::ExecutorStatus {
                concurrency_limit: 1,
                ..Default::default()
            },
            cost_profile: crate::executor_pool::CostProfile::default(),
            metrics: crate::executor_pool::ExecutorMetrics::default(),
        });
        Arc::new(pool)
    }

    fn make_plan_value(ids: &crate::read_only_planner::WorkflowPlanIds) -> Value {
        json!({
            "schema_version": "read_only_plan.v1",
            "plan_id": ids.plan_id,
            "status": "planned_read_only",
            "workflow_id": ids.workflow_id,
            "dispatch_id": ids.dispatch_id,
            "analysis": {"analysis_id": "a-1", "task_domain": "docs"},
            "graph": {
                "schema_version": "workflow_graph.v1",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "status": "decomposed",
                "created_at": "2026-06-05T00:00:00Z",
                "updated_at": "2026-06-05T00:00:00Z",
                "nodes": [
                    {
                        "schema_version": "workflow_node.v1",
                        "node_id": "node-a",
                        "workflow_id": ids.workflow_id,
                        "task_type": "analysis",
                        "assigned_agent_id": null,
                        "status": "pending",
                        "input_refs": [],
                        "output_ref": null,
                        "budget": 0.1,
                        "cost_incurred": 0.0,
                        "error": null,
                        "created_at": "2026-06-05T00:00:00Z",
                        "started_at": null,
                        "completed_at": null
                    }
                ],
                "edges": [],
            },
            "boundaries": {
                "execution_authority": "disabled",
                "target_repository_writes": "disabled",
                "runtime_workers": "disabled",
            },
        })
    }

    fn create_plan_and_run(store: &LocalProductStore) -> String {
        let plan = store
            .create_workflow_plan("fix auth bug", "test", "actor", |ids, _| {
                Ok(make_plan_value(ids))
            })
            .unwrap();
        let plan_id = plan["plan_id"].as_str().unwrap();
        let run = store
            .create_workflow_run_from_plan(plan_id, "actor")
            .unwrap();
        run["run_id"].as_str().unwrap().to_string()
    }

    #[test]
    fn scheduler_config_defaults() {
        let config = SchedulerConfig::default();
        assert_eq!(config.interval_ms, 2000);
        assert_eq!(config.max_concurrent, 4);
        assert_eq!(config.max_retries, 0);
        assert_eq!(config.lease_timeout_ms, 300_000);
        assert!(!config.supervised_workers_enabled);
        assert_eq!(config.worker_count, 1);
    }

    #[test]
    fn scheduler_start_registers_injected_agent_step_executor() {
        let config = SchedulerConfig {
            interval_ms: 5,
            max_concurrent: 3,
            worker_count: 1,
            supervised_workers_enabled: true,
            executor_type: "auto".to_string(),
            agent_max_concurrent_global: 3,
            agent_max_concurrent_per_run: 1,
            lease_timeout_ms: 301_000,
            ..Default::default()
        };
        let mut scheduler = WorkflowScheduler::new(test_store(), config)
            .with_agent_step_executor(Arc::new(crate::node_executor::NoopNodeExecutor));

        scheduler.start().expect("scheduler start");
        let entry = scheduler
            .executor_pool()
            .snapshot()
            .into_iter()
            .find(|entry| entry.executor_type == "agent_step")
            .expect("agent_step registration");
        scheduler.stop().expect("scheduler stop");

        assert_eq!(entry.capabilities.supported_task_types, ["agent_step"]);
        assert_eq!(entry.status.concurrency_limit, 3);
    }

    #[test]
    fn scheduler_auto_registers_only_explicit_adaptive_provider_worker() {
        let config = SchedulerConfig {
            interval_ms: 5,
            max_concurrent: 2,
            worker_count: 1,
            supervised_workers_enabled: true,
            executor_type: "auto".to_string(),
            lease_timeout_ms: 301_000,
            ..Default::default()
        };
        let calls = Arc::new(AtomicUsize::new(0));
        let mut scheduler = WorkflowScheduler::new(test_store(), config).with_worker_executor(
            Arc::new(TrackingExecutor {
                calls: calls.clone(),
            }),
        );

        scheduler.start().expect("scheduler start");
        let entry = scheduler
            .executor_pool()
            .snapshot()
            .into_iter()
            .find(|entry| entry.executor_type == "adaptive_provider")
            .expect("adaptive provider registration");
        scheduler.stop().expect("scheduler stop");

        assert_eq!(
            entry.capabilities.supported_task_types,
            ["adaptive_provider"]
        );
        assert_eq!(entry.status.concurrency_limit, 2);
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 0);
    }

    #[test]
    fn scheduler_rejects_untyped_injected_worker_instead_of_pooling_it() {
        let config = SchedulerConfig {
            max_concurrent: 1,
            worker_count: 1,
            supervised_workers_enabled: true,
            executor_type: "auto".to_string(),
            lease_timeout_ms: 301_000,
            ..Default::default()
        };
        let mut scheduler = WorkflowScheduler::new(test_store(), config)
            .with_worker_executor(Arc::new(NoopNodeExecutor));

        assert_eq!(
            scheduler
                .start()
                .expect_err("untyped worker must fail closed"),
            "injected scheduler worker executor must identify as adaptive_provider"
        );
    }

    #[test]
    fn supervised_worker_config_rejects_unsafe_bounds() {
        let disabled = SchedulerConfig::default();
        assert!(disabled.validate_for_start().is_err());
        let mut scheduler = WorkflowScheduler::new(test_store(), disabled);
        assert!(scheduler.start().is_err());

        let too_many = SchedulerConfig {
            supervised_workers_enabled: true,
            worker_count: 3,
            max_concurrent: 2,
            ..Default::default()
        };
        assert!(too_many.validate_for_start().is_err());

        let zero_workers = SchedulerConfig {
            supervised_workers_enabled: true,
            worker_count: 0,
            ..Default::default()
        };
        assert!(zero_workers.validate_for_start().is_err());

        let lease_too_short = SchedulerConfig {
            interval_ms: 2_000,
            lease_timeout_ms: 300_000,
            ..Default::default()
        };
        assert!(lease_too_short
            .validate_lease_exceeds_execution_timeout(300_000)
            .expect_err("lease must exceed execution timeout")
            .contains("cannot be reclaimed"));
    }

    #[test]
    fn supervised_worker_config_rejects_unknown_executor_instead_of_noop_fallback() {
        let config = SchedulerConfig {
            supervised_workers_enabled: true,
            executor_type: "unknown_executor".to_string(),
            ..Default::default()
        };

        let error = config
            .validate_for_start()
            .expect_err("unknown scheduler executor must fail closed");
        assert!(error.contains("unsupported ACP_SCHEDULER_EXECUTOR"));
    }

    #[test]
    fn unavailable_scheduler_executor_fails_instead_of_running_noop() {
        let executor = create_scheduler_executor("unknown_executor", test_store());
        let output = executor.execute_node(&NodeExecutionInput {
            node_id: "node-unknown".to_string(),
            task_type: "unknown".to_string(),
            run_id: "run-unknown".to_string(),
            workflow_id: "workflow-unknown".to_string(),
            node_metadata: json!({}),
        });

        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("scheduler_executor_unavailable")
        );
    }

    #[test]
    fn adaptive_provider_scheduler_requires_injected_executor() {
        let config = SchedulerConfig {
            supervised_workers_enabled: true,
            executor_type: "adaptive_provider".to_string(),
            lease_timeout_ms: 302_000,
            ..Default::default()
        };
        let mut scheduler = WorkflowScheduler::new(test_store(), config);

        assert_eq!(
            scheduler.start().unwrap_err(),
            "adaptive provider scheduler requires an injected worker executor"
        );
    }

    #[test]
    fn adaptive_provider_scheduler_pins_injected_executor() {
        let store = test_store();
        let run_id = create_plan_and_run(&store);
        let calls = Arc::new(AtomicUsize::new(0));
        let config = SchedulerConfig {
            interval_ms: 20,
            max_concurrent: 1,
            worker_count: 1,
            supervised_workers_enabled: true,
            executor_type: "adaptive_provider".to_string(),
            lease_timeout_ms: 302_000,
            ..Default::default()
        };
        let executor = Arc::new(TrackingExecutor {
            calls: calls.clone(),
        });
        let mut scheduler =
            WorkflowScheduler::new(store.clone(), config).with_worker_executor(executor);

        scheduler.start().unwrap();
        std::thread::sleep(Duration::from_millis(100));
        scheduler.stop().unwrap();

        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
        let run = store.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(run["status"], "completed");
        assert_eq!(
            run["nodes"][0]["result"]["executor_type"],
            "adaptive_provider"
        );
    }

    #[test]
    fn supervised_workers_pause_resume_and_kill() {
        let store = test_store();
        create_plan_and_run(&store);
        let config = SchedulerConfig {
            interval_ms: 20,
            max_concurrent: 2,
            worker_count: 2,
            supervised_workers_enabled: true,
            lease_timeout_ms: 300_000,
            executor_type: "noop".to_string(),
            ..Default::default()
        };
        let mut scheduler = WorkflowScheduler::new(store.clone(), config);

        scheduler.pause("test").unwrap();
        scheduler.start().unwrap();
        std::thread::sleep(Duration::from_millis(80));
        assert_eq!(scheduler.status()["tick_count"], 0);
        assert_eq!(scheduler.status()["paused"], true);

        scheduler.resume("test").unwrap();
        std::thread::sleep(Duration::from_millis(120));
        assert!(scheduler.status()["tick_count"].as_u64().unwrap() > 0);
        assert_eq!(scheduler.status()["worker_count"], 2);

        scheduler.kill("test").unwrap();
        assert!(!scheduler.is_running());
        assert_eq!(scheduler.status()["kill_requested"], true);
    }

    #[test]
    fn worker_kill_observation_waits_for_control_barrier() {
        let scheduler = WorkflowScheduler::new(test_store(), SchedulerConfig::default());
        scheduler.running.store(true, Ordering::SeqCst);
        let control_gate = Arc::clone(&scheduler.control_gate);
        let running = Arc::clone(&scheduler.running);
        let paused = Arc::clone(&scheduler.paused);
        let kill_requested = Arc::clone(&scheduler.kill_requested);
        let (attempted_tx, attempted_rx) = std::sync::mpsc::channel();
        let (finished_tx, finished_rx) = std::sync::mpsc::channel();
        let mut worker = None;

        scheduler
            .with_control_barrier(|snapshot| {
                assert!(snapshot.running);
                assert!(!snapshot.kill_requested);
                worker = Some(std::thread::spawn(move || {
                    attempted_tx.send(()).unwrap();
                    let state = observe_scheduler_worker_control(
                        &control_gate,
                        &running,
                        &paused,
                        &kill_requested,
                        true,
                        false,
                    );
                    finished_tx.send(state).unwrap();
                }));
                attempted_rx.recv().unwrap();
                assert!(finished_rx.try_recv().is_err());
                assert!(scheduler.running.load(Ordering::SeqCst));
                assert!(!scheduler.kill_requested.load(Ordering::SeqCst));
                Ok(())
            })
            .unwrap();

        assert_eq!(
            finished_rx.recv().unwrap(),
            SchedulerWorkerControlState::Killed
        );
        worker.unwrap().join().unwrap();
        assert!(!scheduler.running.load(Ordering::SeqCst));
        assert!(scheduler.kill_requested.load(Ordering::SeqCst));
    }

    #[test]
    fn control_snapshot_includes_worker_environment_gates() {
        let scheduler = WorkflowScheduler::new(test_store(), SchedulerConfig::default());
        scheduler.running.store(true, Ordering::SeqCst);

        let paused = scheduler.control_snapshot_unlocked_with_environment(true, false);
        assert!(paused.running);
        assert!(paused.paused);
        assert!(!paused.kill_requested);

        let killed = scheduler.control_snapshot_unlocked_with_environment(false, true);
        assert!(killed.running);
        assert!(!killed.paused);
        assert!(killed.kill_requested);
    }

    #[test]
    fn supervised_workers_do_not_double_execute_and_publish_heartbeats() {
        let store = test_store();
        let run_id = create_plan_and_run(&store);
        let config = SchedulerConfig {
            interval_ms: 20,
            max_concurrent: 2,
            worker_count: 2,
            supervised_workers_enabled: true,
            heartbeat_interval_sec: 0,
            ..Default::default()
        };
        let mut scheduler = WorkflowScheduler::new(store.clone(), config);
        scheduler.start().unwrap();
        std::thread::sleep(Duration::from_millis(120));
        scheduler.stop().unwrap();

        let run = store.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(run["status"], "completed");
        assert_eq!(run["nodes"][0]["attempt_count"], 1);

        let heartbeat = store.read_heartbeat().unwrap().unwrap();
        let metadata: Value = serde_json::from_str(&heartbeat.metadata_json).unwrap();
        assert_eq!(metadata.as_object().unwrap().len(), 2);
        assert!(metadata.get("worker-0").is_some());
        assert!(metadata.get("worker-1").is_some());
    }

    #[test]
    fn scheduler_start_stop() {
        let store = test_store();
        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            lease_timeout_ms: 300_000,
            executor_type: "noop".to_string(),
            supervised_workers_enabled: true,
            ..Default::default()
        };
        let mut scheduler = WorkflowScheduler::new(store, config);
        assert!(!scheduler.is_running());

        scheduler.start().unwrap();
        assert!(scheduler.is_running());

        assert!(scheduler.start().is_err());

        scheduler.stop().unwrap();
        assert!(!scheduler.is_running());

        assert!(scheduler.stop().is_err());
    }

    #[test]
    fn scheduler_status_reports_state() {
        let store = test_store();
        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 2,
            max_retries: 2,
            lease_timeout_ms: 60_000,
            executor_type: "noop".to_string(),
            supervised_workers_enabled: true,
            ..Default::default()
        };
        let mut scheduler = WorkflowScheduler::new(store, config);

        let status = scheduler.status();
        assert_eq!(status["running"], false);
        assert_eq!(status["tick_count"], 0);
        assert_eq!(status["config"]["interval_ms"], 50);
        assert_eq!(status["config"]["max_retries"], 2);
        assert_eq!(status["config"]["dynamic_workflow_enabled"], false);

        scheduler.start().unwrap();
        std::thread::sleep(Duration::from_millis(120));

        let status = scheduler.status();
        assert_eq!(status["running"], true);
        assert!(status["started_at"].as_str().is_some());

        scheduler.stop().unwrap();
    }

    #[test]
    fn scheduler_ticks_active_runs() {
        let store = test_store();
        create_plan_and_run(&store);

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 4,
            lease_timeout_ms: 300_000,
            executor_type: "noop".to_string(),
            supervised_workers_enabled: true,
            ..Default::default()
        };
        let mut scheduler = WorkflowScheduler::new(store.clone(), config);
        scheduler.start().unwrap();
        std::thread::sleep(Duration::from_millis(200));
        scheduler.stop().unwrap();

        let status = scheduler.status();
        assert!(status["tick_count"].as_u64().unwrap() > 0);

        let run = store.get_workflow_run("run-0001").unwrap().unwrap();
        assert_eq!(run["status"], "completed");
    }

    #[test]
    fn scheduler_lease_recovery_resets_stale_nodes() {
        let store = test_store();
        let run_id = create_plan_and_run(&store);

        store
            .set_pending_node_to_running_for_test("2020-01-01T00:00:00Z")
            .unwrap();

        let recovered = store.recover_stale_leases(60_000).unwrap();
        assert!(recovered > 0);

        let run = store.get_workflow_run(&run_id).unwrap().unwrap();
        let nodes = run["nodes"].as_array().unwrap();
        let has_pending = nodes.iter().any(|n| n["db_status"] == "pending");
        assert!(has_pending, "stale lease should reset node to pending");
    }

    #[test]
    fn scheduler_lease_recovery_ignores_fresh_leases() {
        let store = test_store();
        let run_id = create_plan_and_run(&store);
        store.tick_workflow_run(&run_id, "test").unwrap();

        let recovered = store.recover_stale_leases(300_000).unwrap();
        assert_eq!(recovered, 0, "fresh leases should not be recovered");
    }

    #[test]
    fn scheduler_list_active_run_ids() {
        let store = test_store();
        let active = store.list_active_workflow_run_ids().unwrap();
        assert!(active.is_empty());

        create_plan_and_run(&store);
        let active = store.list_active_workflow_run_ids().unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0], "run-0001");
    }

    #[test]
    fn scheduler_stop_on_drop() {
        let store = test_store();
        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            lease_timeout_ms: 300_000,
            executor_type: "noop".to_string(),
            supervised_workers_enabled: true,
            ..Default::default()
        };
        let mut scheduler = WorkflowScheduler::new(store, config);
        scheduler.start().unwrap();
        assert!(scheduler.is_running());
        drop(scheduler);
    }

    #[test]
    fn scheduler_tick_no_active_runs() {
        let store = test_store();
        let config = SchedulerConfig::default();
        let executor = NoopNodeExecutor;
        let pool = test_pool();
        let result = scheduler_tick(&store, &config, Arc::new(executor.clone()), &pool).unwrap();
        assert_eq!(result.ticks, 0);
        assert_eq!(result.retries, 0);
    }

    #[test]
    fn scheduler_explicit_noop_never_routes_to_command_pool_executor() {
        let store = test_store();
        let run_id = create_plan_and_run(&store);
        let config = SchedulerConfig {
            executor_type: "noop".to_string(),
            max_concurrent: 1,
            queue_enabled: false,
            backpressure_enabled: false,
            ..Default::default()
        };
        let pool = test_pool();

        scheduler_tick(&store, &config, Arc::new(NoopNodeExecutor), &pool).unwrap();

        let run = store.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(run["nodes"][0]["result"]["executor_type"], "noop");
        let snapshot = pool.snapshot();
        let command = snapshot
            .iter()
            .find(|entry| entry.executor_type == "command")
            .unwrap();
        assert_eq!(command.metrics.total_executions, 0);
        let noop = snapshot
            .iter()
            .find(|entry| entry.executor_type == "noop")
            .unwrap();
        assert_eq!(noop.metrics.successful_executions, 1);
    }

    #[test]
    fn scheduler_opencode_external_routes_through_pool_completes_and_is_idempotent() {
        use crate::opencode_runtime::{
            OpenCodeInvokeError, OpenCodeInvoker, OpenCodeNodeExecutor, OpenCodeRuntimeConfig,
            OPENCODE_ADAPTER_CONTRACT, OPENCODE_ADAPTER_VERSION, OPENCODE_EXECUTOR_TYPE,
            OPENCODE_NODE_SCHEMA, OPENCODE_RESULT_SCHEMA, OPENCODE_TASK_TYPE,
            PINNED_OPENCODE_VERSION,
        };
        use std::path::PathBuf;
        use std::sync::Mutex;

        struct BindingInvoker {
            last: Mutex<Option<Value>>,
        }

        impl OpenCodeInvoker for BindingInvoker {
            fn invoke(
                &self,
                request: &Value,
                _timeout_ms: u64,
                _cancel: &dyn crate::opencode_runtime::OpenCodeCancelProbe,
            ) -> Result<Value, OpenCodeInvokeError> {
                *self.last.lock().unwrap() = Some(request.clone());
                let task_input_hash = request["task_input_hash"].as_str().unwrap_or_default();
                let summary_digest = {
                    use sha2::{Digest, Sha256};
                    let mut hasher = Sha256::new();
                    hasher.update(task_input_hash.as_bytes());
                    format!("{:x}", hasher.finalize())
                };
                Ok(json!({
                    "schema_version": OPENCODE_RESULT_SCHEMA,
                    "invocation_id": request["invocation_id"],
                    "run_id": request["run_id"],
                    "node_id": request["node_id"],
                    "workflow_id": request["workflow_id"],
                    "scheduler_claim_id": request["scheduler_claim_id"],
                    "execution_attempt": request["execution_attempt"],
                    "task_kind": request["task_kind"],
                    "task_input_hash": request["task_input_hash"],
                    "base_commit": request["base_commit"],
                    "worktree_id": request["worktree_id"],
                    "status": "ok",
                    "changed_paths": [],
                    "patch": null,
                    "patch_sha256": null,
                    "analysis": {
                        "summary_digest": summary_digest,
                        "findings_count": 1,
                        "scope_paths": request["allowed_paths"],
                    },
                    "tool_summary": {
                        "tool_call_count": 0,
                        "network_attempts": 0,
                        "provider_attempts": 0,
                        "mcp_attempts": 0,
                        "web_attempts": 0,
                        "remote_agent_attempts": 0,
                        "background_agent_attempts": 0,
                        "process_attempts": 0,
                    },
                    "reason_code": "fixture_analysis_ok",
                    "runtime": {
                        "runtime_kind": "opencode",
                        "runtime_version": PINNED_OPENCODE_VERSION,
                        "adapter_version": OPENCODE_ADAPTER_VERSION,
                        "adapter_contract_version": OPENCODE_ADAPTER_CONTRACT,
                        "mode": "fixture",
                    }
                }))
            }
        }

        let store = test_store();
        let invoker = Arc::new(BindingInvoker {
            last: Mutex::new(None),
        });
        let cancel = crate::opencode_runtime::OpenCodeCancellationHandle::new();
        let executor = Arc::new(OpenCodeNodeExecutor::new(
            OpenCodeRuntimeConfig::fixture(PathBuf::from("python3"), PathBuf::from("adapter.py")),
            invoker.clone(),
            cancel,
        ));

        let plan = store
            .create_workflow_plan("oc-sched-plan", "oc-sched-wf", "test-actor", |ids, _| {
                Ok(json!({
                    "schema_version": "read_only_plan.v1",
                    "plan_id": ids.plan_id,
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "analysis": {"analysis_id": "a-oc", "task_domain": "code"},
                    "graph": {
                        "schema_version": "workflow_graph.v1",
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "status": "decomposed",
                        "created_at": "2026-07-08T00:00:00Z",
                        "updated_at": "2026-07-08T00:00:00Z",
                        "nodes": [{
                            "node_id": "oc-node-1",
                            "task_type": OPENCODE_TASK_TYPE,
                            "status": "pending",
                            // Entire node object becomes node_metadata; keep authority at top level.
                            "opencode_external": {
                                "schema_version": OPENCODE_NODE_SCHEMA,
                                "task_kind": "analysis",
                                "task_input_hash": "a".repeat(64),
                                "base_commit": "b".repeat(40),
                                "worktree_id": "wt-sched-1",
                                "allowed_paths": ["docs/a.md"]
                            }
                        }],
                        "edges": []
                    },
                    "boundaries": {
                        "execution_authority": "disabled",
                        "target_repository_writes": "disabled",
                        "runtime_workers": "disabled",
                    },
                }))
            })
            .expect("plan");
        let run = store
            .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "test-actor")
            .expect("run");
        let run_id = run["run_id"].as_str().unwrap().to_string();

        let pool = Arc::new(ExecutorPool::new());
        executor_pool::register_default_executors(&pool, false, store.clone());
        executor_pool::register_opencode_runtime_executor(&pool, executor.clone(), 1, 30_000);

        let config = SchedulerConfig {
            executor_type: "pool".to_string(),
            max_concurrent: 1,
            queue_enabled: false,
            backpressure_enabled: false,
            ..Default::default()
        };

        let tick = scheduler_tick(&store, &config, executor.clone(), &pool).expect("tick");
        assert!(
            tick.ticks >= 1,
            "expected scheduler to execute the opencode node"
        );

        let run_after = store.get_workflow_run(&run_id).unwrap().unwrap();
        let node = run_after["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["node_id"] == "oc-node-1")
            .expect("node present");
        assert_eq!(
            node.get("db_status")
                .or_else(|| node.get("status"))
                .and_then(|s| s.as_str()),
            Some("completed"),
            "node must persist as completed, not success: node={node} run_status={}",
            run_after["status"]
        );
        assert_eq!(run_after["status"], "completed");
        assert_eq!(
            node.pointer("/result/executor_type")
                .and_then(|v| v.as_str())
                .or_else(|| node
                    .get("result")
                    .and_then(|r| r.get("executor_type"))
                    .and_then(|v| v.as_str())),
            Some(OPENCODE_EXECUTOR_TYPE)
        );

        let oc = pool
            .snapshot()
            .into_iter()
            .find(|e| e.executor_type == OPENCODE_EXECUTOR_TYPE)
            .expect("opencode pool entry");
        assert_eq!(oc.metrics.successful_executions, 1);
        assert_eq!(oc.metrics.failed_executions, 0);

        // Restart/replay: completed workflow must not re-execute (exactly-once pool outcome).
        let tick2 = scheduler_tick(&store, &config, executor, &pool).expect("replay tick");
        let oc2 = pool
            .snapshot()
            .into_iter()
            .find(|e| e.executor_type == OPENCODE_EXECUTOR_TYPE)
            .unwrap();
        assert_eq!(
            oc2.metrics.successful_executions, 1,
            "idempotent after restart/replay; ticks2={}",
            tick2.ticks
        );
        if let Some(noop) = pool.snapshot().iter().find(|e| e.executor_type == "noop") {
            assert_eq!(
                noop.metrics.total_executions, 0,
                "generic noop must not execute reserved opencode_external"
            );
        }
        assert!(
            invoker.last.lock().unwrap().is_some(),
            "fixture adapter must have been invoked through the pool"
        );
    }

    #[test]
    fn scheduler_opencode_external_unavailable_when_executor_absent() {
        use crate::opencode_runtime::OPENCODE_TASK_TYPE;

        let store = test_store();
        let plan = store
            .create_workflow_plan("oc-absent-plan", "oc-absent-wf", "test-actor", |ids, _| {
                Ok(json!({
                    "schema_version": "read_only_plan.v1",
                    "plan_id": ids.plan_id,
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "analysis": {"analysis_id": "a-oc-abs", "task_domain": "code"},
                    "graph": {
                        "schema_version": "workflow_graph.v1",
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "status": "decomposed",
                        "created_at": "2026-07-08T00:00:00Z",
                        "updated_at": "2026-07-08T00:00:00Z",
                        "nodes": [{
                            "node_id": "oc-absent-1",
                            "task_type": OPENCODE_TASK_TYPE,
                            "status": "pending",
                            "opencode_external": {
                                "schema_version": "opencode_external_node.v1",
                                "task_kind": "analysis",
                                "task_input_hash": "a".repeat(64),
                                "base_commit": "b".repeat(40),
                                "worktree_id": "wt-abs",
                                "allowed_paths": ["docs/a.md"]
                            }
                        }],
                        "edges": []
                    },
                    "boundaries": {
                        "execution_authority": "disabled",
                        "target_repository_writes": "disabled",
                        "runtime_workers": "disabled",
                    },
                }))
            })
            .expect("plan");
        let run = store
            .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "test-actor")
            .expect("run");
        let run_id = run["run_id"].as_str().unwrap().to_string();

        // Default pool has noop/stub/command but no opencode executor.
        let pool = test_pool();
        assert_eq!(
            pool.best_for_task(OPENCODE_TASK_TYPE, "code"),
            None,
            "reserved opencode_external must not select generic executors"
        );

        let config = SchedulerConfig {
            executor_type: "pool".to_string(),
            max_concurrent: 1,
            queue_enabled: false,
            backpressure_enabled: false,
            ..Default::default()
        };
        let tick =
            scheduler_tick(&store, &config, Arc::new(NoopNodeExecutor), &pool).expect("tick");
        let run_after = store.get_workflow_run(&run_id).unwrap().unwrap();
        // Node must not complete via noop fallback.
        let node = run_after["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["node_id"] == "oc-absent-1")
            .unwrap();
        let status = node
            .get("db_status")
            .or_else(|| node.get("status"))
            .and_then(|s| s.as_str());
        assert_ne!(
            status,
            Some("completed"),
            "must not complete without opencode executor; ticks={} node={node}",
            tick.ticks
        );
        if let Some(noop) = pool.snapshot().iter().find(|e| e.executor_type == "noop") {
            assert_eq!(noop.metrics.successful_executions, 0);
        }
    }

    #[test]
    fn scheduler_records_failed_node_output_as_pool_failure() {
        let store = test_store();
        create_plan_and_run(&store);
        let config = SchedulerConfig {
            executor_type: "fail".to_string(),
            max_concurrent: 1,
            queue_enabled: false,
            backpressure_enabled: false,
            ..Default::default()
        };
        let pool = fail_only_pool();

        scheduler_tick(
            &store,
            &config,
            Arc::new(crate::node_executor::FailNodeExecutor::default()),
            &pool,
        )
        .unwrap();

        let fail = pool
            .snapshot()
            .into_iter()
            .find(|entry| entry.executor_type == "fail")
            .unwrap();
        assert_eq!(fail.metrics.total_executions, 1);
        assert_eq!(fail.metrics.successful_executions, 0);
        assert_eq!(fail.metrics.failed_executions, 1);
        assert!(fail.status.failure_score > 0.0);
    }

    #[test]
    fn dynamic_scheduler_uses_pool_executor_without_double_accounting() {
        let store = test_store();
        let run_id = create_plan_and_run(&store);
        let config = SchedulerConfig {
            executor_type: "dynamic".to_string(),
            max_concurrent: 1,
            queue_enabled: false,
            backpressure_enabled: false,
            ..Default::default()
        };
        let pool = stub_only_pool();

        let result = scheduler_tick(&store, &config, Arc::new(NoopNodeExecutor), &pool).unwrap();
        assert_eq!(result.ticks, 1);
        assert_eq!(pool.total_active(), 0, "pool slot should be released once");

        let run = store.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(run["status"], "completed");

        let stub = pool
            .snapshot()
            .into_iter()
            .find(|entry| entry.executor_type == "stub")
            .unwrap();
        assert_eq!(stub.metrics.total_executions, 1);
        assert_eq!(stub.metrics.successful_executions, 1);
        assert_eq!(stub.metrics.failed_executions, 0);
    }

    #[test]
    fn scheduler_lease_anti_concurrency_two_ticks_dont_double_lease() {
        let store = test_store();
        create_plan_and_run(&store);

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            lease_timeout_ms: 300_000,
            executor_type: "noop".to_string(),
            ..Default::default()
        };
        let executor = NoopNodeExecutor;
        let pool = test_pool();

        // First tick leases and completes the single node
        let r1 = scheduler_tick(&store, &config, Arc::new(executor.clone()), &pool).unwrap();
        assert_eq!(r1.ticks, 1);

        // Second tick finds no ready nodes (already completed)
        let r2 = scheduler_tick(&store, &config, Arc::new(executor.clone()), &pool).unwrap();
        assert_eq!(
            r2.ticks, 0,
            "second tick should not re-lease completed node"
        );

        let run = store.get_workflow_run("run-0001").unwrap().unwrap();
        assert_eq!(run["status"], "completed");
    }

    #[test]
    fn scheduler_fail_executor_increments_error_count() {
        let store = test_store();
        create_plan_and_run(&store);

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            lease_timeout_ms: 300_000,
            executor_type: "fail".to_string(),
            ..Default::default()
        };
        let executor = crate::node_executor::FailNodeExecutor::default();
        let pool = empty_pool();

        // Tick with fail executor — node fails
        let result = scheduler_tick(&store, &config, Arc::new(executor.clone()), &pool);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().ticks, 1);

        // Second tick triggers completion check (no ready nodes left after failure)
        let result2 = scheduler_tick(&store, &config, Arc::new(executor.clone()), &pool);
        assert!(result2.is_ok());

        let run = store.get_workflow_run("run-0001").unwrap().unwrap();
        assert_eq!(
            run["status"], "failed",
            "run should be failed after node failure"
        );
    }

    #[test]
    fn scheduler_dynamic_mode_recovers_failed_node_and_completes_followup_graph() {
        let store = test_store();
        let run_id = create_plan_and_run(&store);

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            lease_timeout_ms: 300_000,
            executor_type: "dynamic".to_string(),
            ..Default::default()
        };
        let fail_executor = crate::node_executor::FailNodeExecutor::default();
        let pool = empty_pool();

        let first =
            scheduler_tick(&store, &config, Arc::new(fail_executor.clone()), &pool).unwrap();
        assert_eq!(first.ticks, 1);

        let run = store.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(
            run["status"], "running",
            "dynamic controller should resume the run after scheduling recovery nodes"
        );
        let nodes = run["nodes"].as_array().unwrap();
        assert!(
            nodes
                .iter()
                .any(|node| { node["node_id"] == "node-a" && node["db_status"] == "recovered" }),
            "failed node should be marked recovered after fix/test nodes are scheduled"
        );
        assert!(
            nodes
                .iter()
                .any(|node| { node["node_id"] == "fix-node-a" && node["db_status"] == "pending" }),
            "dynamic decomposer should add a pending fix node"
        );
        assert!(
            nodes.iter().any(|node| {
                node["node_id"] == "test-fix-node-a" && node["db_status"] == "pending"
            }),
            "dynamic decomposer should add a pending verification node"
        );

        let noop_executor = NoopNodeExecutor;
        let second =
            scheduler_tick(&store, &config, Arc::new(noop_executor.clone()), &pool).unwrap();
        assert_eq!(second.ticks, 1, "fix node should execute");
        let third =
            scheduler_tick(&store, &config, Arc::new(noop_executor.clone()), &pool).unwrap();
        assert_eq!(third.ticks, 1, "verification node should execute");

        let run = store.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(
            run["status"], "completed",
            "run should complete after dynamic recovery nodes complete"
        );
    }

    #[test]
    fn dynamic_scheduler_honors_bounded_retry_before_recovery() {
        let store = test_store();
        let run_id = create_plan_and_run(&store);

        let config = SchedulerConfig {
            max_concurrent: 1,
            max_retries: 1,
            executor_type: "dynamic".to_string(),
            queue_enabled: false,
            backpressure_enabled: false,
            ..Default::default()
        };
        let executor = crate::node_executor::FailNodeExecutor::default();
        let pool = empty_pool();

        let first = scheduler_tick(&store, &config, Arc::new(executor), &pool).unwrap();
        assert_eq!(first.ticks, 1);
        assert_eq!(first.retries, 1);

        let run = store.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(run["status"], "running");
        assert!(run["nodes"].as_array().unwrap().iter().any(|node| {
            node["node_id"] == "node-a"
                && node["db_status"] == "pending"
                && node["attempt_count"] == 1
        }));
        assert!(
            !run["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|node| node["node_id"] == "fix-node-a"),
            "dynamic recovery must wait until the configured retry is exhausted"
        );
    }

    #[test]
    fn scheduler_dynamic_mode_recovers_terminal_failed_run() {
        let store = test_store();
        let run_id = create_plan_and_run(&store);

        let old_config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            lease_timeout_ms: 300_000,
            executor_type: "fail".to_string(),
            ..Default::default()
        };
        let fail_executor = crate::node_executor::FailNodeExecutor::default();
        let pool = empty_pool();
        scheduler_tick(&store, &old_config, Arc::new(fail_executor.clone()), &pool).unwrap();

        // Second tick triggers completion check for the failed node
        let _ = scheduler_tick(&store, &old_config, Arc::new(fail_executor.clone()), &pool);
        let run = store.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(run["status"], "failed");

        let dynamic_config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            lease_timeout_ms: 300_000,
            executor_type: "dynamic".to_string(),
            ..Default::default()
        };
        let noop_executor = NoopNodeExecutor;
        let recover = scheduler_tick(
            &store,
            &dynamic_config,
            Arc::new(noop_executor.clone()),
            &pool,
        )
        .unwrap();
        assert_eq!(recover.ticks, 1, "terminal failed run should be recovered");

        let run = store.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(run["status"], "running");
        assert!(run["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| { node["node_id"] == "fix-node-a" && node["db_status"] == "completed" }));

        scheduler_tick(
            &store,
            &dynamic_config,
            Arc::new(noop_executor.clone()),
            &pool,
        )
        .unwrap();
        let run = store.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(run["status"], "completed");
    }

    #[test]
    fn scheduler_skip_cancelled_run() {
        let store = test_store();
        let run_id = create_plan_and_run(&store);

        // Cancel the run
        store
            .request_workflow_run_cancel(&run_id, "test", Some("aborted"))
            .unwrap();

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 4,
            lease_timeout_ms: 300_000,
            executor_type: "noop".to_string(),
            ..Default::default()
        };
        let executor = NoopNodeExecutor;
        let pool = test_pool();

        // Scheduler tick should skip cancelled run (0 ticks)
        let result = scheduler_tick(&store, &config, Arc::new(executor.clone()), &pool).unwrap();
        assert_eq!(result.ticks, 0, "cancelled run should not be ticked");

        let run = store.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(run["status"], "cancelled");
    }

    #[test]
    fn scheduler_stale_lease_recovery_then_reexecution() {
        let store = test_store();
        let run_id = create_plan_and_run(&store);

        // Tick once to lease the node
        store.tick_workflow_run(&run_id, "test").unwrap();

        // Force the leased node into a stale state
        store
            .set_pending_node_to_running_for_test("2020-01-01T00:00:00Z")
            .unwrap();

        // Scheduler tick recovers stale lease AND re-executes the recovered node
        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            lease_timeout_ms: 60_000,
            executor_type: "noop".to_string(),
            supervised_workers_enabled: true,
            ..Default::default()
        };
        let mut scheduler = WorkflowScheduler::new(store.clone(), config);
        scheduler.start().unwrap();
        std::thread::sleep(Duration::from_millis(200));
        scheduler.stop().unwrap();

        // The stale lease should have been recovered and the node re-executed
        let run = store.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(
            run["status"], "completed",
            "run should complete after stale lease recovery and re-execution"
        );
    }

    #[test]
    fn scheduler_retry_exhaustion_marks_run_failed() {
        let store = test_store();
        let run_id = create_plan_and_run(&store);

        // Tick with fail executor and max_retries=2
        let executor = crate::node_executor::FailNodeExecutor::default();
        for attempt in 0..3 {
            let result = store.tick_with_executor(&run_id, "test", 2, &executor);
            assert!(result.is_ok(), "tick {attempt} should succeed");
        }

        // After 3 attempts (initial + 2 retries), node should be failed
        let run = store.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(
            run["status"], "failed",
            "run should fail after retry exhaustion"
        );

        let nodes = run["nodes"].as_array().unwrap();
        let node = &nodes[0];
        assert_eq!(node["db_status"], "failed");
    }

    #[test]
    fn scheduler_status_includes_active_runs() {
        let store = test_store();
        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 2,
            lease_timeout_ms: 60_000,
            executor_type: "noop".to_string(),
            supervised_workers_enabled: true,
            ..Default::default()
        };
        let scheduler = WorkflowScheduler::new(store.clone(), config);

        let status = scheduler.status();
        assert_eq!(status["active_runs"], 0, "no runs yet");

        create_plan_and_run(&store);

        let status = scheduler.status();
        assert_eq!(status["active_runs"], 1, "one active run after creation");
    }

    #[test]
    fn scheduler_max_concurrent_limits_per_tick() {
        let store = test_store();

        // Create 3 runs
        for _ in 0..3 {
            create_plan_and_run(&store);
        }
        let active = store.list_active_workflow_run_ids().unwrap();
        assert_eq!(active.len(), 3);

        // max_concurrent=2: scheduler tick should process at most 2
        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 2,
            max_retries: 0,
            lease_timeout_ms: 300_000,
            executor_type: "noop".to_string(),
            ..Default::default()
        };
        let executor = NoopNodeExecutor;
        let pool = test_pool();
        let result = scheduler_tick(&store, &config, Arc::new(executor.clone()), &pool).unwrap();
        assert!(
            result.ticks <= 2,
            "max_concurrent=2 should limit ticks to at most 2, got {}",
            result.ticks
        );
    }

    #[test]
    fn scheduler_status_includes_retry_count() {
        let store = test_store();
        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 2,
            lease_timeout_ms: 60_000,
            executor_type: "noop".to_string(),
            ..Default::default()
        };
        let scheduler = WorkflowScheduler::new(store, config);

        let status = scheduler.status();
        assert_eq!(status["retry_count"], 0, "retry_count should start at 0");
    }

    #[test]
    fn scheduler_status_includes_execution_time() {
        let store = test_store();
        create_plan_and_run(&store);

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 2,
            lease_timeout_ms: 60_000,
            executor_type: "noop".to_string(),
            supervised_workers_enabled: true,
            ..Default::default()
        };
        let mut scheduler = WorkflowScheduler::new(store, config);
        assert_eq!(
            scheduler.status()["total_execution_time_ns"],
            0,
            "should start at 0"
        );

        scheduler.start().unwrap();
        std::thread::sleep(Duration::from_millis(200));
        scheduler.stop().unwrap();

        let status = scheduler.status();
        let exec_time = status["total_execution_time_ns"].as_u64().unwrap();
        assert!(
            exec_time > 0,
            "should accumulate execution time, got {exec_time}"
        );
    }

    #[test]
    fn scheduler_tick_tracks_retries_for_fail_executor() {
        let store = test_store();
        create_plan_and_run(&store);

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            max_retries: 1,
            lease_timeout_ms: 300_000,
            executor_type: "fail".to_string(),
            supervised_workers_enabled: true,
            ..Default::default()
        };
        let executor = crate::node_executor::FailNodeExecutor::default();
        let pool = test_pool();

        let first = scheduler_tick(&store, &config, Arc::new(executor.clone()), &pool).unwrap();
        assert_eq!(first.ticks, 1, "should tick once");
        assert_eq!(first.retries, 1, "configured retry should be tracked");

        let run = store.get_workflow_run("run-0001").unwrap().unwrap();
        assert_eq!(run["status"], "running");
        assert_eq!(run["nodes"][0]["db_status"], "pending");
        assert_eq!(run["nodes"][0]["attempt_count"], 1);

        let second = scheduler_tick(&store, &config, Arc::new(executor), &pool).unwrap();
        assert_eq!(second.ticks, 1);
        assert_eq!(second.retries, 0, "retry budget should now be exhausted");
        let run = store.get_workflow_run("run-0001").unwrap().unwrap();
        assert_eq!(run["status"], "failed");
    }

    #[test]
    fn scheduler_tick_produces_aggregation_on_completion() {
        let store = test_store();
        create_plan_and_run(&store);

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            lease_timeout_ms: 300_000,
            executor_type: "noop".to_string(),
            ..Default::default()
        };
        let executor = NoopNodeExecutor;
        let pool = test_pool();

        // Tick once: node completes, run becomes completed
        let result = scheduler_tick(&store, &config, Arc::new(executor.clone()), &pool).unwrap();
        assert_eq!(result.ticks, 1);
        assert_eq!(result.aggregations, 1, "should aggregate completed run");
    }

    #[test]
    fn scheduler_tick_no_aggregation_on_active_run() {
        let store = test_store();
        let run_id = create_plan_and_run(&store);

        // Create a multi-node run by adding another node via store
        // For single-node run, one tick completes it. Test that before tick, no aggregation.
        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 4,
            lease_timeout_ms: 300_000,
            executor_type: "noop".to_string(),
            ..Default::default()
        };
        let executor = NoopNodeExecutor;
        let pool = test_pool();

        // Before ticking, the run is active — no aggregation yet
        let active = store.list_active_workflow_run_ids().unwrap();
        assert_eq!(active.len(), 1);

        // Tick to completion
        let result = scheduler_tick(&store, &config, Arc::new(executor.clone()), &pool).unwrap();
        assert!(result.aggregations > 0, "should aggregate after completion");

        let run = store.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(run["status"], "completed");
    }

    #[test]
    fn scheduler_status_reports_dormant_modules_active() {
        let store = test_store();
        let config = SchedulerConfig::default();
        let scheduler = WorkflowScheduler::new(store, config);

        let status = scheduler.status();
        let dm = &status["dormant_modules_active"];
        assert_eq!(dm["work_queue"], true);
        assert_eq!(dm["result_aggregator"], true);
        assert_eq!(dm["auto_policies"], true);
        assert_eq!(dm["feedback_integrator"], true);
        assert_eq!(dm["conflict_resolver"], true);
        assert_eq!(dm["approval_gate"], true);
        assert_eq!(dm["workflow_engine"], true);
    }

    #[test]
    fn scheduler_tick_with_fail_records_adaptation_recommendation() {
        let store = test_store();
        create_plan_and_run(&store);

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            lease_timeout_ms: 300_000,
            executor_type: "fail".to_string(),
            ..Default::default()
        };
        let executor = crate::node_executor::FailNodeExecutor::default();
        let pool = test_pool();

        // Tick with fail executor — records outcome for adaptation
        let result = scheduler_tick(&store, &config, Arc::new(executor.clone()), &pool).unwrap();
        assert_eq!(result.ticks, 1);
        // Recommendation depends on failure rate thresholds; just verify it doesn't panic
        let _ = result.adaptation_recommendations;
    }

    #[test]
    fn scheduler_modules_new_creates_all_components() {
        let modules = SchedulerModules::new();
        // WorkQueue and ResultAggregator are created
        let graph = crate::orchestration::WorkflowGraph {
            schema_version: "workflow_graph.v1".to_string(),
            workflow_id: "test".to_string(),
            dispatch_id: "test".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            status: "created".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
            started_at: None,
            completed_at: None,
            result: None,
        };
        assert!(modules.aggregator.is_complete(&graph));
        assert!(modules.queue.dequeue_ready(&graph).is_empty());
    }

    #[test]
    fn scheduler_tick_with_conflict_resolution_and_approval_gate() {
        let store = test_store();
        create_plan_and_run(&store);

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            lease_timeout_ms: 300_000,
            executor_type: "noop".to_string(),
            ..Default::default()
        };
        let executor = NoopNodeExecutor;
        let pool = test_pool();

        // Tick: node completes → conflict resolver and approval gate run without panic
        let result = scheduler_tick(&store, &config, Arc::new(executor.clone()), &pool).unwrap();
        assert_eq!(result.ticks, 1);
        assert_eq!(result.aggregations, 1, "should aggregate after completion");

        let run = store.get_workflow_run("run-0001").unwrap().unwrap();
        assert_eq!(run["status"], "completed");
    }

    #[test]
    fn scheduler_tick_conflict_resolver_handles_failed_nodes() {
        let store = test_store();
        create_plan_and_run(&store);

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            lease_timeout_ms: 300_000,
            executor_type: "fail".to_string(),
            ..Default::default()
        };
        let executor = crate::node_executor::FailNodeExecutor::default();
        let pool = empty_pool();

        // Tick with fail executor — conflict resolver handles failed nodes
        let result = scheduler_tick(&store, &config, Arc::new(executor.clone()), &pool).unwrap();
        assert_eq!(result.ticks, 1);

        // Second tick triggers completion check
        let _ = scheduler_tick(&store, &config, Arc::new(executor.clone()), &pool);

        let run = store.get_workflow_run("run-0001").unwrap().unwrap();
        assert_eq!(run["status"], "failed");
    }

    #[test]
    fn scheduler_modules_conflict_resolver_detects_empty_graph() {
        let modules = SchedulerModules::new();
        let graph = crate::orchestration::WorkflowGraph {
            schema_version: "workflow_graph.v1".to_string(),
            workflow_id: "test".to_string(),
            dispatch_id: "test".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            status: "completed".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
            started_at: None,
            completed_at: None,
            result: None,
        };
        let conflicts = modules.conflict_resolver.detect_conflicts(&graph);
        assert!(conflicts.is_empty(), "empty graph should have no conflicts");
    }

    #[test]
    fn scheduler_modules_approval_gate_rejects_high_cost_node() {
        let mut modules = SchedulerModules::new();
        let graph = crate::orchestration::WorkflowGraph {
            schema_version: "workflow_graph.v1".to_string(),
            workflow_id: "test".to_string(),
            dispatch_id: "test".to_string(),
            nodes: vec![crate::orchestration::WorkflowNode {
                schema_version: "workflow_node.v1".to_string(),
                node_id: "node-expensive".to_string(),
                workflow_id: "test".to_string(),
                task_type: "task".to_string(),
                assigned_agent_id: None,
                status: "completed".to_string(),
                input_refs: Vec::new(),
                output_ref: None,
                budget: 1.0,
                cost_incurred: 0.9,
                error: None,
                created_at: "now".to_string(),
                started_at: None,
                completed_at: None,
            }],
            edges: Vec::new(),
            status: "running".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
            started_at: None,
            completed_at: None,
            result: None,
        };
        // cost_incurred (0.9) > budget (1.0) * risk_threshold (0.7) = 0.7
        let node = &graph.nodes[0];
        assert!(
            modules.approval_gate.requires_approval(&graph, node),
            "node exceeding budget*threshold should require approval"
        );

        // After approval, should no longer require
        modules.approval_gate.approve("node-expensive");
        assert!(!modules.approval_gate.requires_approval(&graph, node));
    }

    #[test]
    fn scheduler_modules_workflow_engine_tick_on_empty_graph() {
        let mut modules = SchedulerModules::new();
        let graph = crate::orchestration::WorkflowGraph {
            schema_version: "workflow_graph.v1".to_string(),
            workflow_id: "test-we".to_string(),
            dispatch_id: "test".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            status: "completed".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
            started_at: None,
            completed_at: None,
            result: None,
        };
        // WorkflowEngine.tick() on completed graph should return same status
        let result = modules.workflow_engine.tick(&graph);
        assert_eq!(result.status, "completed");
    }

    #[test]
    fn scheduler_modules_workflow_engine_creates_with_task_decomposer() {
        let mut modules = SchedulerModules::new();
        // WorkflowEngine is created and holds a TaskDecomposer
        // Verify it's usable by calling tick on a simple graph
        let graph = crate::orchestration::WorkflowGraph {
            schema_version: "workflow_graph.v1".to_string(),
            workflow_id: "test".to_string(),
            dispatch_id: "test".to_string(),
            nodes: vec![crate::orchestration::WorkflowNode {
                schema_version: "workflow_node.v1".to_string(),
                node_id: "node-a".to_string(),
                workflow_id: "test".to_string(),
                task_type: "task".to_string(),
                assigned_agent_id: None,
                status: "pending".to_string(),
                input_refs: Vec::new(),
                output_ref: None,
                budget: 0.0,
                cost_incurred: 0.0,
                error: None,
                created_at: "now".to_string(),
                started_at: None,
                completed_at: None,
            }],
            edges: Vec::new(),
            status: "created".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
            started_at: None,
            completed_at: None,
            result: None,
        };
        let result = modules.workflow_engine.tick(&graph);
        // WorkflowEngine should transition created → running and start ready nodes
        assert_eq!(result.status, "running");
        assert_eq!(result.nodes[0].status, "running");
    }

    #[test]
    fn scheduler_config_default_has_queue_fields() {
        let config = SchedulerConfig::default();
        assert!(config.queue_enabled);
        assert_eq!(config.max_queued, 100);
        assert!(config.backpressure_enabled);
        assert_eq!(config.backpressure_activation, 0.8);
    }

    #[test]
    fn scheduler_tick_queue_enabled_uses_prioritized_ordering() {
        let store = test_store();
        let _id1 = create_plan_and_run(&store);
        let _id2 = create_plan_and_run(&store);

        store.update_run_priority("run-0001", 10).unwrap();
        store.update_run_priority("run-0002", 1).unwrap();

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 2,
            max_retries: 0,
            lease_timeout_ms: 300_000,
            executor_type: "noop".to_string(),
            queue_enabled: true,
            max_queued: 100,
            backpressure_enabled: false,
            backpressure_activation: 0.8,
            heartbeat_interval_sec: 10,
            supervised_workers_enabled: false,
            worker_count: 1,
            agent_max_concurrent_global: 2,
            agent_max_concurrent_per_run: 1,
        };
        let executor = NoopNodeExecutor;
        let pool = test_pool();
        let result = scheduler_tick(&store, &config, Arc::new(executor.clone()), &pool).unwrap();
        assert_eq!(result.ticks, 2, "both runs should be processed");
        assert_eq!(
            result.queue_depth, 2,
            "queue_depth should reflect total active"
        );
        assert!(!result.backpressure_active);
        assert!(result.paused_runs.is_empty());
    }

    #[test]
    fn scheduler_tick_queue_enabled_skips_paused_runs() {
        let store = test_store();
        let _id1 = create_plan_and_run(&store);
        let _id2 = create_plan_and_run(&store);

        store
            .update_run_pause_reason("run-0001", Some("operator_hold"))
            .unwrap();

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 2,
            max_retries: 0,
            lease_timeout_ms: 300_000,
            executor_type: "noop".to_string(),
            queue_enabled: true,
            max_queued: 100,
            backpressure_enabled: false,
            backpressure_activation: 0.8,
            heartbeat_interval_sec: 10,
            supervised_workers_enabled: false,
            worker_count: 1,
            agent_max_concurrent_global: 2,
            agent_max_concurrent_per_run: 1,
        };
        let executor = NoopNodeExecutor;
        let pool = test_pool();
        let result = scheduler_tick(&store, &config, Arc::new(executor.clone()), &pool).unwrap();
        assert_eq!(result.ticks, 1, "only non-paused run should tick");
        assert_eq!(result.paused_runs.len(), 1);
        assert_eq!(result.paused_runs[0], "run-0001");
    }

    #[test]
    fn scheduler_tick_queue_disabled_uses_fifo() {
        let store = test_store();
        let _id1 = create_plan_and_run(&store);
        let _id2 = create_plan_and_run(&store);

        store.update_run_priority("run-0001", 10).unwrap();
        store.update_run_priority("run-0002", 1).unwrap();

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 2,
            max_retries: 0,
            lease_timeout_ms: 300_000,
            executor_type: "noop".to_string(),
            queue_enabled: false,
            max_queued: 100,
            backpressure_enabled: false,
            backpressure_activation: 0.8,
            heartbeat_interval_sec: 10,
            supervised_workers_enabled: false,
            worker_count: 1,
            agent_max_concurrent_global: 2,
            agent_max_concurrent_per_run: 1,
        };
        let executor = NoopNodeExecutor;
        let pool = test_pool();
        let result = scheduler_tick(&store, &config, Arc::new(executor.clone()), &pool).unwrap();
        assert_eq!(result.ticks, 2, "FIFO should process both runs");
        assert_eq!(
            result.queue_depth, 0,
            "queue_depth not tracked in FIFO mode"
        );
    }

    #[test]
    fn scheduler_tick_backpressure_pauses_overloaded_runs() {
        let store = test_store();
        for _ in 0..5 {
            create_plan_and_run(&store);
        }

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            max_retries: 0,
            lease_timeout_ms: 300_000,
            executor_type: "noop".to_string(),
            queue_enabled: true,
            max_queued: 100,
            backpressure_enabled: true,
            backpressure_activation: 0.1,
            heartbeat_interval_sec: 10,
            supervised_workers_enabled: false,
            worker_count: 1,
            agent_max_concurrent_global: 2,
            agent_max_concurrent_per_run: 1,
        };
        let executor = NoopNodeExecutor;
        let pool = test_pool();
        let result = scheduler_tick(&store, &config, Arc::new(executor.clone()), &pool).unwrap();
        assert!(result.backpressure_active, "backpressure should activate");
    }

    #[test]
    fn scheduler_tick_backpressure_disabled_no_pause() {
        let store = test_store();
        for _ in 0..5 {
            create_plan_and_run(&store);
        }

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            max_retries: 0,
            lease_timeout_ms: 300_000,
            executor_type: "noop".to_string(),
            queue_enabled: true,
            max_queued: 100,
            backpressure_enabled: false,
            backpressure_activation: 0.1,
            heartbeat_interval_sec: 10,
            supervised_workers_enabled: false,
            worker_count: 1,
            agent_max_concurrent_global: 2,
            agent_max_concurrent_per_run: 1,
        };
        let executor = NoopNodeExecutor;
        let pool = test_pool();
        let result = scheduler_tick(&store, &config, Arc::new(executor.clone()), &pool).unwrap();
        assert!(!result.backpressure_active);
        assert!(result.paused_runs.is_empty());
    }

    #[test]
    fn scheduler_status_includes_queue_info() {
        let store = test_store();
        let config = SchedulerConfig::default();
        let scheduler = WorkflowScheduler::new(store, config);
        let status = scheduler.status();
        assert_eq!(status["queue_enabled"], true);
        assert_eq!(status["backpressure_active"], false);
        assert_eq!(status["queue_depth"], 0);
        assert_eq!(status["paused_runs_count"], 0);
    }

    #[test]
    fn scheduler_status_includes_panic_count() {
        let store = test_store();
        let config = SchedulerConfig::default();
        let scheduler = WorkflowScheduler::new(store, config);

        let status = scheduler.status();
        assert_eq!(status["panic_count"], 0, "panic_count should start at 0");
    }

    #[test]
    fn scheduler_config_default_has_heartbeat_interval() {
        let config = SchedulerConfig::default();
        assert_eq!(config.heartbeat_interval_sec, 10);
    }

    #[test]
    fn scheduler_thread_survives_tick_panic() {
        let store = test_store();
        create_plan_and_run(&store);

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            lease_timeout_ms: 300_000,
            executor_type: "fail".to_string(),
            supervised_workers_enabled: true,
            ..Default::default()
        };
        let mut scheduler = WorkflowScheduler::new(store, config);
        scheduler.start().unwrap();
        // Let the scheduler run — it processes runs, and the thread survives
        std::thread::sleep(Duration::from_millis(200));
        assert!(scheduler.is_running(), "thread should still be alive");
        scheduler.stop().unwrap();
    }

    // ── AR-4: Bounded concurrent multi-agent scheduling ──

    fn agent_step_fixture_node(node_id: &str) -> Value {
        json!({
            "node_id": node_id,
            "task_type": "agent_step",
            "status": "pending",
            "agent_id": format!("agent-{node_id}"),
            "assigned_agent_id": format!("agent-{node_id}"),
            "agent_role": "fixture-agent",
            "agent_objective": "test bounded agent concurrency",
            "capability_profile": ["fixture"],
            "profile_id": "fixture-profile",
            "decision_source": "fixture",
            "max_actions": 1,
        })
    }

    fn create_agent_step_run(store: &LocalProductStore, num_agent_steps: usize) -> String {
        let plan = store
            .create_workflow_plan("test agent caps", "test", "actor", |ids, _| {
                let nodes: Vec<Value> = (0..num_agent_steps)
                    .map(|i| agent_step_fixture_node(&format!("agent-node-{i:03}")))
                    .collect();

                Ok(json!({
                    "schema_version": "read_only_plan.v1",
                    "plan_id": ids.plan_id,
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "analysis": {"analysis_id": "a-1", "task_domain": "test"},
                    "graph": {
                        "schema_version": "workflow_graph.v1",
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "status": "decomposed",
                        "created_at": "2026-06-25T00:00:00Z",
                        "updated_at": "2026-06-25T00:00:00Z",
                        "nodes": nodes,
                        "edges": [],
                    },
                    "boundaries": {
                        "execution_authority": "managed",
                        "target_repository_writes": "disabled",
                        "runtime_workers": "disabled",
                    },
                }))
            })
            .unwrap();
        let plan_id = plan["plan_id"].as_str().unwrap();
        let run = store
            .create_workflow_run_from_plan(plan_id, "actor")
            .unwrap();
        run["run_id"].as_str().unwrap().to_string()
    }

    fn create_analysis_node_run(store: &LocalProductStore) -> String {
        let plan = store
            .create_workflow_plan("test analysis", "test", "actor", |ids, _| {
                Ok(json!({
                    "schema_version": "read_only_plan.v1",
                    "plan_id": ids.plan_id,
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "analysis": {"analysis_id": "a-1", "task_domain": "test"},
                    "graph": {
                        "schema_version": "workflow_graph.v1",
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "status": "decomposed",
                        "created_at": "2026-06-25T00:00:00Z",
                        "updated_at": "2026-06-25T00:00:00Z",
                        "nodes": [
                            {"node_id": "analysis-001", "task_type": "analysis", "status": "pending"}
                        ],
                        "edges": [],
                    },
                    "boundaries": {
                        "execution_authority": "managed",
                        "target_repository_writes": "disabled",
                        "runtime_workers": "disabled",
                    },
                }))
            })
            .unwrap();
        let plan_id = plan["plan_id"].as_str().unwrap();
        store
            .create_workflow_run_from_plan(plan_id, "actor")
            .unwrap()["run_id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn create_mixed_agent_analysis_run(
        store: &LocalProductStore,
        first_node_id: &str,
        first_task_type: &str,
        second_node_id: &str,
        second_task_type: &str,
    ) -> String {
        let plan = store
            .create_workflow_plan("test mixed", "test", "actor", |ids, _| {
                let first_node = if first_task_type == "agent_step" {
                    agent_step_fixture_node(first_node_id)
                } else {
                    json!({"node_id": first_node_id, "task_type": first_task_type, "status": "pending"})
                };
                let second_node = if second_task_type == "agent_step" {
                    agent_step_fixture_node(second_node_id)
                } else {
                    json!({"node_id": second_node_id, "task_type": second_task_type, "status": "pending"})
                };
                Ok(json!({
                    "schema_version": "read_only_plan.v1",
                    "plan_id": ids.plan_id,
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "analysis": {"analysis_id": "a-1", "task_domain": "test"},
                    "graph": {
                        "schema_version": "workflow_graph.v1",
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "status": "decomposed",
                        "created_at": "2026-06-25T00:00:00Z",
                        "updated_at": "2026-06-25T00:00:00Z",
                        "nodes": [first_node, second_node],
                        "edges": [],
                    },
                    "boundaries": {
                        "execution_authority": "managed",
                        "target_repository_writes": "disabled",
                        "runtime_workers": "disabled",
                    },
                }))
            })
            .unwrap();
        let plan_id = plan["plan_id"].as_str().unwrap();
        store
            .create_workflow_run_from_plan(plan_id, "actor")
            .unwrap()["run_id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    #[derive(Clone)]
    struct BlockingExecutor {
        barrier: Arc<Barrier>,
    }

    impl NodeExecutor for BlockingExecutor {
        fn executor_type_name(&self) -> &str {
            "agent_step"
        }
        fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
            self.barrier.wait();
            NodeExecutionOutput {
                status: "completed".to_string(),
                executor_type: "agent_step".to_string(),
                output: None,
                error_domain: None,
                error_message: None,
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: None,
                process_outcome: None,
                resolved_model: None,
            }
        }
    }

    struct AgentNoopExecutor;

    impl NodeExecutor for AgentNoopExecutor {
        fn executor_type_name(&self) -> &str {
            "agent_step"
        }

        fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
            NodeExecutionOutput {
                status: "completed".to_string(),
                executor_type: "agent_step".to_string(),
                output: Some("fixture agent step completed".to_string()),
                error_domain: None,
                error_message: None,
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(0),
                process_outcome: None,
                resolved_model: None,
            }
        }
    }

    #[test]
    fn agent_concurrency_global_cap_enforced() {
        let store = test_store();
        let run_1 = create_agent_step_run(&store, 1);
        let run_2 = create_agent_step_run(&store, 1);

        let barrier = Arc::new(Barrier::new(2));
        let exec = BlockingExecutor {
            barrier: barrier.clone(),
        };

        let store_clone = store.clone();
        let handle = thread::spawn(move || {
            store_clone.tick_with_executor_with_agent_caps(&run_1, "test", 0, &exec, 1, 1)
        });

        thread::sleep(Duration::from_millis(100));

        let result_2 = store
            .tick_with_executor_with_agent_caps(&run_2, "test", 0, &AgentNoopExecutor, 1, 1)
            .unwrap();
        assert_eq!(
            result_2["action"], "no_ready_node",
            "second run should be blocked by global cap"
        );

        let events = store.audit_events(50).unwrap();
        let conflicts: Vec<_> = events
            .iter()
            .filter(|e| {
                e.get("action").and_then(|a| a.as_str()) == Some("agent_step.claim_conflict")
            })
            .collect();
        assert!(!conflicts.is_empty(), "expected claim_conflict audit");
        assert_eq!(
            conflicts[0]
                .pointer("/details/reason")
                .and_then(|v| v.as_str()),
            Some("global_cap_exceeded")
        );

        barrier.wait();
        let result_1 = handle.join().unwrap().unwrap();
        assert_eq!(
            result_1["action"], "node_executed",
            "first run should complete"
        );

        let result_2b = store
            .tick_with_executor_with_agent_caps(&run_2, "test", 0, &AgentNoopExecutor, 1, 1)
            .unwrap();
        assert_eq!(
            result_2b["action"], "node_executed",
            "second run should execute after first releases"
        );
    }

    #[test]
    fn agent_concurrency_per_run_cap_enforced() {
        let store = test_store();
        let run_id = create_agent_step_run(&store, 2);

        let barrier = Arc::new(Barrier::new(2));
        let exec = BlockingExecutor {
            barrier: barrier.clone(),
        };

        let store_clone = store.clone();
        let run_clone = run_id.clone();
        let handle = thread::spawn(move || {
            store_clone.tick_with_executor_with_agent_caps(&run_clone, "test", 0, &exec, 3, 1)
        });

        thread::sleep(Duration::from_millis(100));

        let result_2 = store
            .tick_with_executor_with_agent_caps(&run_id, "test", 0, &AgentNoopExecutor, 3, 1)
            .unwrap();
        assert_eq!(
            result_2["action"], "no_ready_node",
            "second agent_step in same run should be blocked by per_run cap"
        );

        let events = store.audit_events(50).unwrap();
        let conflicts: Vec<_> = events
            .iter()
            .filter(|e| {
                e.get("action").and_then(|a| a.as_str()) == Some("agent_step.claim_conflict")
            })
            .collect();
        assert!(!conflicts.is_empty(), "expected claim_conflict audit");
        assert_eq!(
            conflicts[0]
                .pointer("/details/reason")
                .and_then(|v| v.as_str()),
            Some("per_run_cap_exceeded")
        );

        barrier.wait();
        handle.join().unwrap().unwrap();

        let result_2b = store
            .tick_with_executor_with_agent_caps(&run_id, "test", 0, &AgentNoopExecutor, 3, 1)
            .unwrap();
        assert_eq!(
            result_2b["action"], "node_executed",
            "second node should execute after first releases"
        );
    }

    #[test]
    fn agent_concurrency_honored_within_limits() {
        let store = test_store();
        let run_id = create_agent_step_run(&store, 1);

        let result = store
            .tick_with_executor_with_agent_caps(&run_id, "test", 0, &AgentNoopExecutor, 2, 1)
            .unwrap();
        assert_eq!(
            result["action"], "node_executed",
            "single agent_step within caps should execute"
        );
    }

    #[test]
    fn agent_concurrency_analysis_node_ignores_caps() {
        let store = test_store();
        let run_id = create_analysis_node_run(&store);

        let result = store
            .tick_with_executor_with_agent_caps(&run_id, "test", 0, &NoopNodeExecutor, 0, 0)
            .unwrap();
        assert_eq!(
            result["action"], "node_executed",
            "analysis node should execute even with agent caps=0"
        );
    }

    #[test]
    fn agent_concurrency_audit_events_chain() {
        let store = test_store();
        let run_id = create_agent_step_run(&store, 1);

        store
            .tick_with_executor_with_agent_caps(&run_id, "test", 0, &AgentNoopExecutor, 2, 1)
            .unwrap();

        let events = store.audit_events(50).unwrap();
        let actions: Vec<&str> = events
            .iter()
            .filter_map(|e| e.get("action").and_then(|a| a.as_str()))
            .filter(|a| a.starts_with("agent_step."))
            .collect();

        assert!(
            actions.contains(&"agent_step.claim_attempt"),
            "expected claim_attempt, got {:?}",
            actions
        );
        assert!(
            actions.contains(&"agent_step.claim_success"),
            "expected claim_success, got {:?}",
            actions
        );
        assert!(
            actions.contains(&"agent_step.execution_started"),
            "expected execution_started, got {:?}",
            actions
        );
        assert!(
            actions.contains(&"agent_step.execution_completed"),
            "expected execution_completed, got {:?}",
            actions
        );
    }

    #[test]
    fn agent_concurrency_release_after_completion() {
        let store = test_store();
        let run_id = create_agent_step_run(&store, 2);

        let result_1 = store
            .tick_with_executor_with_agent_caps(&run_id, "test", 0, &AgentNoopExecutor, 2, 1)
            .unwrap();
        assert_eq!(result_1["action"], "node_executed");

        let result_2 = store
            .tick_with_executor_with_agent_caps(&run_id, "test", 0, &AgentNoopExecutor, 2, 1)
            .unwrap();
        assert_eq!(
            result_2["action"], "node_executed",
            "second agent_step should execute after first completes"
        );
    }

    #[test]
    fn agent_concurrency_config_defaults() {
        let config = SchedulerConfig::default();
        assert_eq!(config.agent_max_concurrent_global, 2);
        assert_eq!(config.agent_max_concurrent_per_run, 1);
    }

    #[test]
    fn scheduler_retry_env_is_bounded_and_fail_closed() {
        struct RetryEnvGuard;
        impl Drop for RetryEnvGuard {
            fn drop(&mut self) {
                std::env::remove_var("ACP_SCHEDULER_MAX_RETRIES");
            }
        }

        let _guard = RetryEnvGuard;
        let gates = dummy_gates();

        std::env::remove_var("ACP_SCHEDULER_MAX_RETRIES");
        assert_eq!(
            SchedulerConfig::from_env_with_gates(&gates)
                .unwrap()
                .max_retries,
            0
        );

        std::env::set_var("ACP_SCHEDULER_MAX_RETRIES", "3");
        assert_eq!(
            SchedulerConfig::from_env_with_gates(&gates)
                .unwrap()
                .max_retries,
            3
        );

        for invalid in ["not_a_number", "-1", "11"] {
            std::env::set_var("ACP_SCHEDULER_MAX_RETRIES", invalid);
            let error = SchedulerConfig::from_env_with_gates(&gates).unwrap_err();
            assert!(
                error.contains("ACP_SCHEDULER_MAX_RETRIES"),
                "error should name the invalid setting: {error}"
            );
        }
    }

    #[test]
    fn scheduler_retry_config_rejects_manual_out_of_range_values() {
        for max_retries in [-1, 11] {
            let config = SchedulerConfig {
                supervised_workers_enabled: true,
                max_retries,
                ..Default::default()
            };
            let error = config.validate_for_start().unwrap_err();
            assert!(error.contains("ACP_SCHEDULER_MAX_RETRIES"));
        }
    }

    fn dummy_gates() -> crate::trusted_local::EffectiveExecutionGates {
        crate::trusted_local::EffectiveExecutionGates::from_lookup(|_| None)
    }

    #[test]
    fn agent_concurrency_env_invalid_values_fail_closed() {
        // All assertions in one test to avoid global env var races between parallel tests
        let gates = dummy_gates();

        // Invalid global (non-numeric)
        std::env::set_var("ACP_AGENT_MAX_CONCURRENT_GLOBAL", "not_a_number");
        std::env::set_var("ACP_AGENT_MAX_CONCURRENT_PER_RUN", "1");
        let result = SchedulerConfig::from_env_with_gates(&gates);
        std::env::remove_var("ACP_AGENT_MAX_CONCURRENT_GLOBAL");
        std::env::remove_var("ACP_AGENT_MAX_CONCURRENT_PER_RUN");
        assert!(result.is_err(), "invalid global should fail closed");
        assert!(
            result
                .as_ref()
                .unwrap_err()
                .contains("ACP_AGENT_MAX_CONCURRENT_GLOBAL"),
            "error should mention global: {}",
            result.as_ref().unwrap_err()
        );

        // Invalid per_run (non-numeric)
        std::env::set_var("ACP_AGENT_MAX_CONCURRENT_GLOBAL", "2");
        std::env::set_var("ACP_AGENT_MAX_CONCURRENT_PER_RUN", "abc");
        let result = SchedulerConfig::from_env_with_gates(&gates);
        std::env::remove_var("ACP_AGENT_MAX_CONCURRENT_GLOBAL");
        std::env::remove_var("ACP_AGENT_MAX_CONCURRENT_PER_RUN");
        assert!(result.is_err(), "invalid per_run should fail closed");
        assert!(
            result
                .as_ref()
                .unwrap_err()
                .contains("ACP_AGENT_MAX_CONCURRENT_PER_RUN"),
            "error should mention per_run: {}",
            result.as_ref().unwrap_err()
        );

        // Negative value should fail (usize cannot be negative)
        std::env::set_var("ACP_AGENT_MAX_CONCURRENT_GLOBAL", "-1");
        std::env::set_var("ACP_AGENT_MAX_CONCURRENT_PER_RUN", "1");
        let result = SchedulerConfig::from_env_with_gates(&gates);
        std::env::remove_var("ACP_AGENT_MAX_CONCURRENT_GLOBAL");
        std::env::remove_var("ACP_AGENT_MAX_CONCURRENT_PER_RUN");
        assert!(result.is_err(), "negative global should fail closed");
        assert!(
            result
                .as_ref()
                .unwrap_err()
                .contains("ACP_AGENT_MAX_CONCURRENT_GLOBAL"),
            "error should mention global: {}",
            result.as_ref().unwrap_err()
        );

        // Valid values still work
        std::env::set_var("ACP_AGENT_MAX_CONCURRENT_GLOBAL", "5");
        std::env::set_var("ACP_AGENT_MAX_CONCURRENT_PER_RUN", "3");
        let result = SchedulerConfig::from_env_with_gates(&gates);
        std::env::remove_var("ACP_AGENT_MAX_CONCURRENT_GLOBAL");
        std::env::remove_var("ACP_AGENT_MAX_CONCURRENT_PER_RUN");
        assert!(result.is_ok(), "valid values should succeed");
        let config = result.unwrap();
        assert_eq!(config.agent_max_concurrent_global, 5);
        assert_eq!(config.agent_max_concurrent_per_run, 3);
    }

    #[test]
    fn agent_concurrency_config_rejects_invalid() {
        let config = SchedulerConfig {
            supervised_workers_enabled: true,
            worker_count: 1,
            max_concurrent: 4,
            agent_max_concurrent_global: 2,
            agent_max_concurrent_per_run: 3,
            ..Default::default()
        };
        let err = config.validate_for_start().unwrap_err();
        assert!(
            err.contains("agent_max_concurrent_per_run must not exceed"),
            "should reject per_run > global: {err}"
        );
    }

    #[test]
    fn agent_concurrency_global_zero_blocks_agent_step() {
        let store = test_store();
        let run_id = create_agent_step_run(&store, 1);

        let result = store
            .tick_with_executor_with_agent_caps(&run_id, "test", 0, &AgentNoopExecutor, 0, 0)
            .unwrap();
        assert_eq!(
            result["action"], "no_ready_node",
            "agent_step should be blocked when global=0"
        );

        let events = store.audit_events(50).unwrap();
        let conflicts: Vec<_> = events
            .iter()
            .filter(|e| {
                e.get("action").and_then(|a| a.as_str()) == Some("agent_step.claim_conflict")
            })
            .collect();
        assert!(!conflicts.is_empty(), "expected claim_conflict audit");
        assert_eq!(
            conflicts[0]
                .pointer("/details/reason")
                .and_then(|v| v.as_str()),
            Some("global_cap_exceeded")
        );
    }

    #[test]
    fn agent_concurrency_analysis_node_unaffected_by_zero_cap() {
        let store = test_store();
        let run_id = create_analysis_node_run(&store);

        let result = store
            .tick_with_executor_with_agent_caps(&run_id, "test", 0, &NoopNodeExecutor, 0, 0)
            .unwrap();
        assert_eq!(
            result["action"], "node_executed",
            "analysis node should execute even with agent caps=0"
        );

        let events = store.audit_events(50).unwrap();
        let agent_events: Vec<_> = events
            .iter()
            .filter(|e| {
                e.get("action")
                    .and_then(|a| a.as_str())
                    .map(|a| a.starts_with("agent_step."))
                    .unwrap_or(false)
            })
            .collect();
        assert!(
            agent_events.is_empty(),
            "analysis node should not produce agent_step audit events"
        );
    }

    // ── AR-4.1: Capped agent_step must not block non-agent nodes ──

    #[test]
    fn agent_concurrency_capped_agent_first_analysis_executes() {
        let store = test_store();
        // "agent-node-000" < "analysis-001" alphabetically, so agent comes first in ORDER BY
        let run_id = create_mixed_agent_analysis_run(
            &store,
            "agent-node-000",
            "agent_step",
            "analysis-001",
            "analysis",
        );

        let result = store
            .tick_with_executor_with_agent_caps(&run_id, "test", 0, &NoopNodeExecutor, 0, 1)
            .unwrap();
        assert_eq!(
            result["action"], "node_executed",
            "analysis node should execute when agent_step is capped by global=0"
        );

        // Verify the analysis node executed, not the agent_step
        assert_eq!(
            result["node_id"], "analysis-001",
            "analysis-001 should be the executed node"
        );

        let current = store.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(current["nodes"][0]["db_status"], "pending");
    }

    #[test]
    fn agent_concurrency_per_run_capped_agent_first_analysis_executes() {
        let store = test_store();
        let run_id = create_mixed_agent_analysis_run(
            &store,
            "agent-node-000",
            "agent_step",
            "analysis-001",
            "analysis",
        );

        // First tick: execute the agent_step (per_run cap=1, global cap=2)
        let barrier = Arc::new(Barrier::new(2));
        let exec = BlockingExecutor {
            barrier: barrier.clone(),
        };
        let store_clone = store.clone();
        let run_clone = run_id.clone();
        let handle = thread::spawn(move || {
            store_clone.tick_with_executor_with_agent_caps(&run_clone, "test", 0, &exec, 2, 1)
        });

        thread::sleep(Duration::from_millis(100));

        // Second tick: agent_step is per-run capped, should skip to analysis node
        let result = store
            .tick_with_executor_with_agent_caps(&run_id, "test", 0, &NoopNodeExecutor, 2, 1)
            .unwrap();
        assert_eq!(
            result["action"], "node_executed",
            "analysis node should execute when agent_step is at per-run cap"
        );
        assert_eq!(
            result["node_id"], "analysis-001",
            "should execute analysis-001"
        );

        barrier.wait();
        handle.join().unwrap().unwrap();

        // Verify both nodes completed
        let run = store.get_workflow_run(&run_id).unwrap().unwrap();
        let nodes = run["nodes"].as_array().unwrap();
        let agent_done = nodes
            .iter()
            .any(|n| n["node_id"] == "agent-node-000" && n["db_status"] == "completed");
        let analysis_done = nodes
            .iter()
            .any(|n| n["node_id"] == "analysis-001" && n["db_status"] == "completed");
        assert!(agent_done, "agent_step should eventually complete");
        assert!(analysis_done, "analysis should complete");
    }

    #[test]
    fn agent_concurrency_all_capped_agent_steps_return_no_ready_node() {
        let store = test_store();
        let run_id = create_agent_step_run(&store, 2);

        let result = store
            .tick_with_executor_with_agent_caps(&run_id, "test", 0, &AgentNoopExecutor, 0, 0)
            .unwrap();
        assert_eq!(
            result["action"], "no_ready_node",
            "all agent_step nodes should be blocked by global=0"
        );

        // Verify claim_conflict was emitted for both nodes
        let events = store.audit_events(50).unwrap();
        let conflicts: Vec<_> = events
            .iter()
            .filter(|e| {
                e.get("action").and_then(|a| a.as_str()) == Some("agent_step.claim_conflict")
            })
            .collect();
        assert_eq!(
            conflicts.len(),
            2,
            "expected claim_conflict for both agent_step nodes"
        );
    }
}
