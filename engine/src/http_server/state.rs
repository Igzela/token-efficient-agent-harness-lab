use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::dispatch_engine::DispatchEngine;
use crate::infrastructure::auth::TenantResolver;
use crate::infrastructure::rate_limiter::RateLimiter;
use crate::provider::Provider;
use crate::storage::local_product_store::LocalProductStore;

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

#[derive(Clone)]
pub struct AxumApiState {
    pub(crate) engine: Arc<DispatchEngine>,
    pub(crate) tenant_resolver: Option<Arc<Mutex<TenantResolver>>>,
    pub(crate) rate_limiter: Arc<Mutex<RateLimiter>>,
    pub(crate) default_rate_limit: Option<i64>,
    pub(crate) now: f64,
    pub(crate) dashboard_dir: Option<Arc<PathBuf>>,
    pub(crate) local_store: Option<Arc<LocalProductStore>>,
    pub(crate) backup_dir: Option<Arc<PathBuf>>,
    pub(crate) provider: Option<Arc<dyn Provider>>,
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
            now: 0.0,
            dashboard_dir: None,
            local_store: None,
            backup_dir: None,
            provider: None,
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
        self.now = now;
        self
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
            recorder,
        ));
        self.provider = Some(provider);
        self
    }

    pub fn with_engine(mut self, engine: DispatchEngine) -> Self {
        self.engine = Arc::new(engine);
        self
    }

    pub fn executor_type(&self) -> &str {
        self.engine.executor_type()
    }

    pub fn provider_enabled(&self) -> bool {
        self.provider.as_ref().map_or(false, |p| p.is_enabled())
    }
}
