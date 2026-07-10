use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::dispatch_engine::DispatchEngine;
use crate::feedback::ModelEndpointRegistrySnapshot;
use crate::infrastructure::auth::TenantResolver;
use crate::infrastructure::circuit_breaker::CircuitBreakerRegistry;
use crate::infrastructure::observability::{MetricsCollector, RequestTracer};
use crate::infrastructure::rate_limiter::RateLimiter;
use crate::provider::adaptive_execution::{
    persisted_adaptive_provider_endpoint_configs, AdaptiveExecutionExecutor,
    AdaptiveExecutionKillSwitch,
};
use crate::provider::Provider;
use crate::scheduler::WorkflowScheduler;
use crate::storage::local_product_store::LocalProductStore;
use crate::trusted_local::EffectiveExecutionGates;

#[derive(Clone)]
pub(crate) struct AdaptiveProviderRuntime {
    pub(crate) config_hash: String,
    pub(crate) executor: Arc<AdaptiveExecutionExecutor>,
    pub(crate) registry_snapshot: Arc<ModelEndpointRegistrySnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub api_prefix: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            api_prefix: "/api/v1".to_string(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CliCapability {
    pub enabled: bool,
    pub claude_code: bool,
    pub codex: bool,
}

impl From<&crate::cli::CliConfig> for CliCapability {
    fn from(config: &crate::cli::CliConfig) -> Self {
        Self {
            enabled: config.enabled,
            claude_code: config.claude_code_enabled,
            codex: config.codex_enabled,
        }
    }
}

#[derive(Clone)]
pub struct AxumApiState {
    pub(crate) engine: Arc<DispatchEngine>,
    pub(crate) tenant_resolver: Option<Arc<Mutex<TenantResolver>>>,
    pub(crate) rate_limiter: Arc<Mutex<RateLimiter>>,
    pub(crate) default_rate_limit: Option<i64>,
    pub(crate) fixed_now: Option<f64>,
    pub(crate) dashboard_dir: Option<Arc<PathBuf>>,
    pub(crate) local_store: Option<Arc<LocalProductStore>>,
    pub(crate) backup_dir: Option<Arc<PathBuf>>,
    pub(crate) provider: Option<Arc<dyn Provider>>,
    pub(crate) adaptive_provider_executor: Option<Arc<AdaptiveExecutionExecutor>>,
    pub(crate) adaptive_registry_snapshot: Option<Arc<ModelEndpointRegistrySnapshot>>,
    pub(crate) adaptive_local_config_runtime: Arc<Mutex<Option<AdaptiveProviderRuntime>>>,
    pub(crate) scheduler: Option<Arc<Mutex<WorkflowScheduler>>>,
    pub(crate) metrics: Arc<MetricsCollector>,
    pub(crate) tracer: Arc<RequestTracer>,
    pub(crate) circuit_breaker_registry: Arc<CircuitBreakerRegistry>,
    pub(crate) cli_capability: CliCapability,
}

impl Default for AxumApiState {
    fn default() -> Self {
        Self::new()
    }
}

impl AxumApiState {
    pub fn new() -> Self {
        Self {
            engine: Arc::new(DispatchEngine::new()),
            tenant_resolver: None,
            rate_limiter: Arc::new(Mutex::new(RateLimiter::new(60.0, 10_000))),
            default_rate_limit: None,
            fixed_now: None,
            dashboard_dir: None,
            local_store: None,
            backup_dir: None,
            provider: None,
            adaptive_provider_executor: None,
            adaptive_registry_snapshot: None,
            adaptive_local_config_runtime: Arc::new(Mutex::new(None)),
            scheduler: None,
            metrics: Arc::new(MetricsCollector::new(10_000)),
            tracer: Arc::new(RequestTracer::new()),
            circuit_breaker_registry: Arc::new(CircuitBreakerRegistry::new()),
            cli_capability: CliCapability::default(),
        }
    }

    pub fn with_auth(
        mut self,
        tenant_resolver: TenantResolver,
        rate_limiter: RateLimiter,
        default_rate_limit: Option<i64>,
        now: f64,
    ) -> Self {
        self.tenant_resolver = Some(Arc::new(Mutex::new(tenant_resolver)));
        self.rate_limiter = Arc::new(Mutex::new(rate_limiter));
        self.default_rate_limit = default_rate_limit;
        self.fixed_now = Some(now);
        self
    }

    pub fn with_auth_live(
        mut self,
        tenant_resolver: TenantResolver,
        rate_limiter: RateLimiter,
        default_rate_limit: Option<i64>,
    ) -> Self {
        self.tenant_resolver = Some(Arc::new(Mutex::new(tenant_resolver)));
        self.rate_limiter = Arc::new(Mutex::new(rate_limiter));
        self.default_rate_limit = default_rate_limit;
        self.fixed_now = None;
        self
    }

    pub(crate) fn now(&self) -> f64 {
        self.fixed_now.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_secs_f64())
                .unwrap_or(f64::MAX)
        })
    }

    pub fn with_dashboard_dir(mut self, dashboard_dir: impl Into<PathBuf>) -> Self {
        self.dashboard_dir = Some(Arc::new(dashboard_dir.into()));
        self
    }

    pub fn with_local_store(mut self, store: LocalProductStore) -> Self {
        self.local_store = Some(Arc::new(store));
        self
    }

    pub fn with_local_store_arc(mut self, store: Arc<LocalProductStore>) -> Self {
        self.local_store = Some(store);
        self
    }

    pub fn with_backup_dir(mut self, backup_dir: impl Into<PathBuf>) -> Self {
        self.backup_dir = Some(Arc::new(backup_dir.into()));
        self
    }

    pub fn with_provider(mut self, provider: Arc<dyn Provider>) -> Self {
        self.engine = Arc::new(DispatchEngine::with_provider_executor(provider.clone()));
        self.adaptive_provider_executor = Some(Arc::new(AdaptiveExecutionExecutor::new(
            std::collections::BTreeMap::from([(
                provider.provider_id().to_string(),
                provider.clone(),
            )]),
            Arc::new(crate::provider::ProviderAuditRecorder::new()),
            AdaptiveExecutionKillSwitch::new(),
        )));
        self.provider = Some(provider);
        self
    }

    pub fn with_provider_and_audit(
        mut self,
        provider: Arc<dyn Provider>,
        recorder: Arc<crate::provider::ProviderAuditRecorder>,
    ) -> Self {
        self.engine = Arc::new(DispatchEngine::with_provider_executor_and_audit(
            provider.clone(),
            recorder.clone(),
        ));
        self.adaptive_provider_executor = Some(Arc::new(AdaptiveExecutionExecutor::new(
            std::collections::BTreeMap::from([(
                provider.provider_id().to_string(),
                provider.clone(),
            )]),
            recorder,
            AdaptiveExecutionKillSwitch::new(),
        )));
        self.provider = Some(provider);
        self
    }

    pub fn with_adaptive_provider_executor(
        mut self,
        executor: Arc<AdaptiveExecutionExecutor>,
    ) -> Self {
        self.adaptive_provider_executor = Some(executor);
        self
    }

    pub fn with_adaptive_registry_snapshot(
        mut self,
        snapshot: ModelEndpointRegistrySnapshot,
    ) -> Self {
        self.adaptive_registry_snapshot = Some(Arc::new(snapshot));
        self
    }

    pub(crate) fn adaptive_local_config_runtime_for_hash(
        &self,
        config_hash: &str,
    ) -> Option<AdaptiveProviderRuntime> {
        self.adaptive_local_config_runtime
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .filter(|runtime| runtime.config_hash == config_hash)
            .cloned()
    }

    pub(crate) fn install_adaptive_local_config_runtime(
        &self,
        config_hash: String,
        executor: Arc<AdaptiveExecutionExecutor>,
        registry_snapshot: ModelEndpointRegistrySnapshot,
    ) -> AdaptiveProviderRuntime {
        let runtime = AdaptiveProviderRuntime {
            config_hash,
            executor,
            registry_snapshot: Arc::new(registry_snapshot),
        };
        *self
            .adaptive_local_config_runtime
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(runtime.clone());
        runtime
    }

    pub fn with_engine(mut self, engine: DispatchEngine) -> Self {
        self.engine = Arc::new(engine);
        self
    }

    pub fn with_scheduler(mut self, scheduler: Arc<Mutex<WorkflowScheduler>>) -> Self {
        self.scheduler = Some(scheduler);
        self
    }

    pub fn with_observability(
        mut self,
        metrics: Arc<MetricsCollector>,
        tracer: Arc<RequestTracer>,
    ) -> Self {
        self.metrics = metrics;
        self.tracer = tracer;
        self
    }

    pub fn with_circuit_breaker_registry(mut self, registry: Arc<CircuitBreakerRegistry>) -> Self {
        self.circuit_breaker_registry = registry;
        self
    }

    pub fn with_cli_capability(mut self, capability: CliCapability) -> Self {
        self.cli_capability = capability;
        self
    }

    pub fn cli_capability(&self) -> &CliCapability {
        &self.cli_capability
    }

    pub(crate) fn effective_execution_gates(&self) -> EffectiveExecutionGates {
        let endpoint_configs = self
            .local_store
            .as_deref()
            .map(persisted_adaptive_provider_endpoint_configs)
            .and_then(Result::ok)
            .flatten();
        EffectiveExecutionGates::from_lookup_with_endpoint_configs(
            |key| std::env::var(key).ok(),
            endpoint_configs.as_deref(),
        )
    }

    pub fn executor_type(&self) -> &str {
        self.engine.executor_type()
    }

    pub fn provider_enabled(&self) -> bool {
        self.provider.as_ref().map_or(false, |p| p.is_enabled())
    }
}
