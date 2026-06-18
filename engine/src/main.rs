use engine::cli::{ClaudeCodeCliExecutor, CliConfig, CodexCliExecutor, MultiExecutor};
use engine::dispatch_engine::DispatchEngine;
use engine::executor::HybridExecutor;
use engine::executor_adapter::NoopExecutor;
use engine::http_server::{build_axum_router, build_axum_router_with_dashboard, AxumApiState};
use engine::infrastructure::auth::{
    hash_api_key, validate_token_shape, APIKey, Tenant, TenantResolver,
};
use engine::infrastructure::circuit_breaker::{CircuitBreaker, CircuitBreakerRegistry};
use engine::infrastructure::rate_limiter::RateLimiter;
use engine::provider::anthropic::AnthropicProvider;
use engine::provider::audit::ProviderAuditRecorder;
use engine::provider::circuit_breaker_provider::CircuitBreakerProvider;
use engine::provider::config::CredentialRef;
use engine::provider::config::{provider_pricing_from_env, ProviderConfig};
use engine::provider::credential::CredentialBoundary;
use engine::provider::openai::OpenAiProvider;
use engine::provider::stub::StubProvider;
use engine::provider::transport::ReqwestTransport;
use engine::provider::Provider;
use engine::scheduler::{SchedulerConfig, WorkflowScheduler};
use engine::storage::local_product_store::LocalProductStore;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("{}:{}", host, port);
    let profile = std::env::var("ACP_PROFILE")
        .unwrap_or_else(|_| "local".to_string())
        .to_lowercase();

    if profile != "local" && profile != "production" {
        eprintln!(
            "[acp-fatal] ACP_PROFILE must be 'local' or 'production', got '{}'",
            profile
        );
        std::process::exit(1);
    }

    let db_path = local_db_path();
    let backup_dir = local_backup_dir(&db_path);
    let backup_dir_for_auto = backup_dir.clone();

    let store = if let Ok(_pg_url) = std::env::var("ACP_DATABASE_URL") {
        #[cfg(feature = "pg")]
        {
            println!("[acp-startup] db_backend=postgresql");
            LocalProductStore::new_postgres(&_pg_url, || {
                chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
            })
            .expect("failed to open PostgreSQL store")
        }
        #[cfg(not(feature = "pg"))]
        {
            eprintln!("[acp-fatal] ACP_DATABASE_URL is set but the 'pg' feature is not enabled. Rebuild with --features pg.");
            std::process::exit(1);
        }
    } else {
        let s = LocalProductStore::new(&db_path).expect("failed to open local SQLite store");
        if s.is_encrypted() {
            println!("[acp-startup] db_backend=sqlite db_encryption=enabled");
        } else {
            println!("[acp-startup] db_backend=sqlite db_encryption=disabled (set ACP_DB_ENCRYPTION_KEY to enable)");
        }
        s
    };
    if std::env::var("ACP_ADMIN_API_KEY").is_ok() {
        store
            .upsert_team_member("local-admin", "Local Admin", "admin")
            .expect("failed to record local admin team member");
        store
            .record_api_key_metadata(
                "local-admin-env",
                "local-admin",
                "admin",
                &local_admin_scope_list(),
                "bootstrap",
            )
            .expect("failed to record local admin API key metadata");
    }
    let store_arc = Arc::new(store);
    let store_for_scheduler = store_arc.clone();
    let cb_registry = Arc::new(CircuitBreakerRegistry::new());
    let cli_config = CliConfig::from_env();
    let execution_mode = std::env::var("ACP_EXECUTION_MODE")
        .unwrap_or_else(|_| "off".to_string())
        .to_lowercase();

    let (base_engine, _exec_type_label) = match execution_mode.as_str() {
        "provider" => {
            let provider = build_provider_for_engine(&store_arc, &cb_registry)
                .expect("ACP_EXECUTION_MODE=provider requires ACP_PROVIDER_TYPE + ACP_ENABLE_PROVIDER_EXECUTION=1");
            let engine = DispatchEngine::with_provider_executor(provider);
            (engine, "provider".to_string())
        }
        "cli" => {
            let multi_executor = build_multi_executor(&cli_config);
            let engine = DispatchEngine::with_multi_executor(multi_executor);
            (engine, "cli".to_string())
        }
        "auto" => {
            let provider = build_provider_for_engine(&store_arc, &cb_registry).ok();
            let cli_executors = build_cli_executor_map(&cli_config);
            let threshold: f64 = std::env::var("ACP_HYBRID_COMPLEXITY_THRESHOLD")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.5);
            let hybrid =
                HybridExecutor::new(provider, cli_executors, Box::new(NoopExecutor), threshold);
            let engine = DispatchEngine::with_executor(Box::new(hybrid));
            (engine, "auto".to_string())
        }
        _ => {
            // "off" or unrecognized — default noop-fallback multi executor
            let multi_executor = build_multi_executor(&cli_config);
            let engine = DispatchEngine::with_multi_executor(multi_executor);
            (engine, "noop".to_string())
        }
    };

    let state = configure_auth(
        build_state_with_provider(
            AxumApiState::new().with_engine(base_engine),
            &store_arc,
            &cb_registry,
        )
        .with_local_store_arc(store_arc.clone())
        .with_backup_dir(backup_dir)
        .with_circuit_breaker_registry(cb_registry.clone()),
    );

    let require_auth = std::env::var("ACP_REQUIRE_AUTH")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let may_use_provider = state.executor_type() == "provider"
        || (execution_mode == "auto"
            && build_provider_for_engine(&store_arc, &cb_registry).is_ok());
    if may_use_provider && !require_auth {
        panic!(
            "ACP_REQUIRE_AUTH=1 is required when ACP_ENABLE_PROVIDER_EXECUTION=1 and a real provider is configured"
        );
    }

    let exec_type = state.executor_type();
    let exec_type_display = if execution_mode == "auto" {
        "auto(hybrid)".to_string()
    } else {
        exec_type.to_string()
    };
    let _prov_enabled = state.provider_enabled();
    let lan = if host == "0.0.0.0" {
        "lan-exposed"
    } else {
        "local-only"
    };
    let cost_per_dispatch = std::env::var("ACP_COST_PER_DISPATCH_USD")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "unlimited".to_string());
    let cost_daily = std::env::var("ACP_COST_DAILY_USD")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "unlimited".to_string());
    let cli_summary = format!(
        "claude={} codex={}",
        cli_config.claude_code_enabled, cli_config.codex_enabled
    );
    println!(
        "[acp-startup] execution_mode={} executor={} cli=[{}] auth={} host={} budget_per_dispatch={} budget_daily={} lan={}",
        execution_mode,
        exec_type_display,
        cli_summary,
        if require_auth { "on" } else { "off" },
        addr,
        cost_per_dispatch,
        cost_daily,
        lan,
    );

    if host == "0.0.0.0" && !require_auth {
        eprintln!("[acp-warning] LAN-exposed without auth — set ACP_REQUIRE_AUTH=1 for production");
    }
    if host == "0.0.0.0" {
        let cors = std::env::var("ACP_CORS_ORIGINS").unwrap_or_default();
        if cors.is_empty() || cors == "*" {
            eprintln!(
                "[acp-warning] CORS allows all origins — set ACP_CORS_ORIGINS for production"
            );
        }
    }

    // Production profile gate — hard fail on unsafe config
    if profile == "production" {
        let violations = production_profile_violations(&host, require_auth);
        if !violations.is_empty() {
            eprintln!("[acp-fatal] Production profile violations:");
            for v in &violations {
                eprintln!("  - {}", v);
            }
            std::process::exit(1);
        }
    }

    let enable_scheduler = std::env::var("ACP_ENABLE_SCHEDULER")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let backup_interval_sec: u64 = std::env::var("ACP_BACKUP_INTERVAL_SEC")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let backup_retain_count: usize = std::env::var("ACP_BACKUP_RETAIN_COUNT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    let state = if enable_scheduler {
        let scheduler_config = SchedulerConfig::from_env();
        scheduler_config
            .validate_for_start()
            .expect("invalid supervised worker configuration");
        let executor_type = scheduler_config.executor_type.clone();
        let interval = scheduler_config.interval_ms;
        let worker_count = scheduler_config.worker_count;
        let mut scheduler = WorkflowScheduler::new(store_for_scheduler, scheduler_config);
        if backup_interval_sec > 0 {
            if std::env::var("ACP_DATABASE_URL").is_ok() {
                eprintln!("[acp-warning] ACP_BACKUP_INTERVAL_SEC={} is ignored in PostgreSQL mode — use pg_dump or your managed backup service. App auto-backup disabled.", backup_interval_sec);
            } else {
                let bm = engine::storage::backup_manager::BackupManager::new(&backup_dir_for_auto)
                    .expect("failed to create backup manager for auto-backup");
                scheduler = scheduler.with_auto_backup(
                    Arc::new(bm),
                    db_path.display().to_string(),
                    backup_interval_sec,
                    backup_retain_count,
                );
                println!(
                    "[acp-startup] auto_backup=enabled interval={}s retain={}",
                    backup_interval_sec, backup_retain_count
                );
            }
        } else {
            println!("[acp-startup] auto_backup=disabled");
        }
        scheduler.start().expect("failed to start scheduler");
        let scheduler_arc = Arc::new(Mutex::new(scheduler));
        println!(
            "[acp-startup] scheduler=enabled supervised_workers=enabled workers={} interval={}ms executor={}",
            worker_count, interval, executor_type
        );
        state.with_scheduler(scheduler_arc)
    } else {
        state
    };

    let dashboard_dir =
        std::env::var("ACP_DASHBOARD_DIR").or_else(|_| std::env::var("DASHBOARD_DIR"));
    let router = match dashboard_dir {
        Ok(path) if !path.trim().is_empty() => {
            println!("dashboard assets served from {}", path);
            build_axum_router_with_dashboard(state, path)
        }
        _ => build_axum_router(state),
    };

    let tls_cert_path = std::env::var("ACP_TLS_CERT_PATH").ok();
    let tls_key_path = std::env::var("ACP_TLS_KEY_PATH").ok();

    if let Err(e) = validate_tls_config(tls_cert_path.as_deref(), tls_key_path.as_deref()) {
        eprintln!("[acp-fatal] {}", e);
        std::process::exit(1);
    }

    match (tls_cert_path, tls_key_path) {
        (Some(cert_path), Some(key_path)) => {
            let tls_config =
                axum_server::tls_rustls::RustlsConfig::from_pem_chain_file(&cert_path, &key_path)
                    .await
                    .unwrap_or_else(|e| {
                        eprintln!(
                            "[acp-fatal] Failed to load TLS cert/key: cert={}, key={}, error={}",
                            cert_path, key_path, e
                        );
                        std::process::exit(1);
                    });
            println!(
                "[acp-startup] TLS enabled, cert={}, key={}",
                cert_path, key_path
            );
            println!("engine listening on {} (HTTPS)", addr);
            let handle = axum_server::Handle::new();
            tokio::spawn({
                let handle = handle.clone();
                async move {
                    shutdown_signal().await;
                    handle.graceful_shutdown(Some(std::time::Duration::from_secs(10)));
                }
            });
            let socket_addr: std::net::SocketAddr = addr.parse().unwrap_or_else(|e| {
                eprintln!("[acp-fatal] Invalid bind address '{}': {}", addr, e);
                std::process::exit(1);
            });
            axum_server::bind_rustls(socket_addr, tls_config)
                .handle(handle)
                .serve(router.into_make_service())
                .await
                .unwrap_or_else(|e| {
                    eprintln!("[acp-fatal] TLS server error: {}", e);
                    std::process::exit(1);
                });
        }
        (None, None) => {
            println!(
                "[acp-startup] TLS disabled (neither ACP_TLS_CERT_PATH nor ACP_TLS_KEY_PATH set)"
            );
            println!("engine listening on {}", addr);
            let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
            axum::serve(listener, router)
                .with_graceful_shutdown(shutdown_signal())
                .await
                .unwrap();
        }
        _ => unreachable!("validate_tls_config already rejected single-sided TLS env vars"),
    }
    println!("[acp-shutdown] engine stopped gracefully");
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => println!("[acp-shutdown] received SIGINT"),
        _ = terminate => println!("[acp-shutdown] received SIGTERM"),
    }
}

fn local_db_path() -> PathBuf {
    std::env::var("ACP_DB_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".agent-control-plane/local-team.db"))
}

fn local_backup_dir(db_path: &Path) -> PathBuf {
    std::env::var("ACP_BACKUP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            db_path
                .parent()
                .map(|parent| parent.join("backups"))
                .unwrap_or_else(|| PathBuf::from(".agent-control-plane/backups"))
        })
}

fn configure_auth(state: AxumApiState) -> AxumApiState {
    let require_auth = std::env::var("ACP_REQUIRE_AUTH")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let admin_api_key = std::env::var("ACP_ADMIN_API_KEY").ok();
    if require_auth && admin_api_key.is_none() {
        panic!("ACP_ADMIN_API_KEY is required when ACP_REQUIRE_AUTH=1");
    }
    let Some(raw_key) = admin_api_key else {
        return state;
    };
    if !validate_token_shape(&raw_key) {
        panic!("ACP_ADMIN_API_KEY must use the harness_<64 hex chars> local key shape");
    }

    let scopes = local_admin_scopes();
    let mut resolver = TenantResolver::new();
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local Team".to_string(),
        scopes: scopes.clone(),
        rate_limit: Some(10_000),
    });
    let salt = "local-admin-env-salt";
    resolver.add_api_key(APIKey {
        key_id: "local-admin-env".to_string(),
        tenant_id: "local".to_string(),
        key_hash: hash_api_key(&raw_key, salt),
        key_salt: salt.to_string(),
        scopes,
        created_at: 0.0,
        expires_at: None,
        revoked_at: None,
        last_used_at: None,
    });
    state.with_auth(resolver, RateLimiter::new(60.0, 10_000), Some(10_000), 0.0)
}

fn build_state_with_provider(
    state: AxumApiState,
    store: &Arc<LocalProductStore>,
    cb_registry: &Arc<CircuitBreakerRegistry>,
) -> AxumApiState {
    let provider_type = match std::env::var("ACP_PROVIDER_TYPE") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => return state,
    };

    let enable_execution = std::env::var("ACP_ENABLE_PROVIDER_EXECUTION")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if provider_type == "stub" {
        let recorder = Arc::new(ProviderAuditRecorder::with_store(store.clone()));
        let provider: Arc<dyn Provider> = Arc::new(StubProvider::new("stub-env"));
        return state.with_provider_and_audit(provider, recorder);
    }

    if !enable_execution {
        eprintln!(
            "ACP_PROVIDER_TYPE={} requires ACP_ENABLE_PROVIDER_EXECUTION=1; falling back to noop",
            provider_type
        );
        return state;
    }

    let api_key_env = std::env::var("ACP_API_KEY").unwrap_or_default();
    let model = std::env::var("ACP_MODEL").unwrap_or_else(|_| "default".to_string());
    let base_url = std::env::var("ACP_BASE_URL").unwrap_or_default();
    let pricing = provider_pricing_from_env();
    if !pricing.configured() {
        eprintln!(
            "provider token usage will be tracked, but ACP_PROVIDER_INPUT_COST_PER_1K_USD/ACP_PROVIDER_OUTPUT_COST_PER_1K_USD are not fully configured"
        );
    }

    let boundary = CredentialBoundary::new("env").expect("env credential backend");
    let cred_ref = CredentialRef::new(
        &api_key_env,
        "env",
        "***",
        "provider:auto",
        "2026-01-01T00:00:00Z",
    );

    let base_provider: Arc<dyn Provider> = match provider_type.as_str() {
        "stub" => Arc::new(StubProvider::new("stub-env")),
        "openai_compatible" => {
            let mut config = ProviderConfig::new(
                "openai-env",
                "openai_compatible",
                &base_url,
                &model,
                &api_key_env,
                "2026-01-01T00:00:00Z",
            );
            config.apply_pricing(&pricing);
            Arc::new(OpenAiProvider::new(
                config,
                boundary,
                cred_ref,
                Arc::new(ReqwestTransport::new()),
                None,
            ))
        }
        "anthropic" => {
            let mut config = ProviderConfig::new(
                "anthropic-env",
                "anthropic",
                &base_url,
                &model,
                &api_key_env,
                "2026-01-01T00:00:00Z",
            );
            config.apply_pricing(&pricing);
            Arc::new(AnthropicProvider::new(
                config,
                boundary,
                cred_ref,
                Arc::new(ReqwestTransport::new()),
                None,
            ))
        }
        other => {
            eprintln!("unknown ACP_PROVIDER_TYPE: {other}, falling back to noop");
            return state;
        }
    };

    // Wrap provider with circuit breaker protection.
    let cb_threshold = std::env::var("ACP_CIRCUIT_BREAKER_THRESHOLD")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(5);
    let cb_recovery_ms = std::env::var("ACP_CIRCUIT_BREAKER_RECOVERY_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30_000);
    let provider_cb = Arc::new(CircuitBreaker::new(
        format!("provider:{}", base_provider.provider_id()),
        cb_threshold,
        cb_recovery_ms,
    ));
    cb_registry.register(provider_cb.clone());
    let provider: Arc<dyn Provider> =
        Arc::new(CircuitBreakerProvider::new(base_provider, provider_cb));

    let recorder = Arc::new(ProviderAuditRecorder::with_store(store.clone()));
    state.with_provider_and_audit(provider, recorder)
}

fn local_admin_scopes() -> HashSet<String> {
    local_admin_scope_list().into_iter().collect()
}

fn local_admin_scope_list() -> Vec<String> {
    [
        "audit:read",
        "backup:admin",
        "config:admin",
        "config:read",
        "cost:read",
        "dispatch:execute",
        "dispatch:read",
        "dispatch:write",
        "export:read",
        "health:read",
        "team:admin",
        "team:read",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

/// Build a provider Arc for engine injection (separate from the API state provider).
/// Returns `Err` if ACP_PROVIDER_TYPE is unset or ACP_ENABLE_PROVIDER_EXECUTION is not "1".
fn build_provider_for_engine(
    _store: &Arc<LocalProductStore>,
    cb_registry: &Arc<CircuitBreakerRegistry>,
) -> Result<Arc<dyn engine::provider::Provider>, String> {
    let provider_type =
        std::env::var("ACP_PROVIDER_TYPE").map_err(|_| "ACP_PROVIDER_TYPE not set".to_string())?;
    if provider_type.trim().is_empty() {
        return Err("ACP_PROVIDER_TYPE is empty".to_string());
    }

    let enable_execution = std::env::var("ACP_ENABLE_PROVIDER_EXECUTION")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if !enable_execution {
        return Err("ACP_ENABLE_PROVIDER_EXECUTION not enabled".to_string());
    }

    if provider_type == "stub" {
        let provider: Arc<dyn engine::provider::Provider> = Arc::new(StubProvider::new("stub-env"));
        return Ok(provider);
    }

    let api_key_env = std::env::var("ACP_API_KEY").unwrap_or_default();
    let model = std::env::var("ACP_MODEL").unwrap_or_else(|_| "default".to_string());
    let base_url = std::env::var("ACP_BASE_URL").unwrap_or_default();
    let pricing = provider_pricing_from_env();

    let boundary = CredentialBoundary::new("env").expect("env credential backend");
    let cred_ref = CredentialRef::new(
        &api_key_env,
        "env",
        "***",
        "provider:auto",
        "2026-01-01T00:00:00Z",
    );

    let base_provider: Arc<dyn engine::provider::Provider> = match provider_type.as_str() {
        "openai_compatible" => {
            let mut config = ProviderConfig::new(
                "openai-env",
                "openai_compatible",
                &base_url,
                &model,
                &api_key_env,
                "2026-01-01T00:00:00Z",
            );
            config.apply_pricing(&pricing);
            Arc::new(OpenAiProvider::new(
                config,
                boundary,
                cred_ref,
                Arc::new(ReqwestTransport::new()),
                None,
            ))
        }
        "anthropic" => {
            let mut config = ProviderConfig::new(
                "anthropic-env",
                "anthropic",
                &base_url,
                &model,
                &api_key_env,
                "2026-01-01T00:00:00Z",
            );
            config.apply_pricing(&pricing);
            Arc::new(AnthropicProvider::new(
                config,
                boundary,
                cred_ref,
                Arc::new(ReqwestTransport::new()),
                None,
            ))
        }
        other => return Err(format!("unknown ACP_PROVIDER_TYPE: {other}")),
    };

    // Wrap with circuit breaker
    let cb_threshold = std::env::var("ACP_CIRCUIT_BREAKER_THRESHOLD")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(5);
    let cb_recovery_ms = std::env::var("ACP_CIRCUIT_BREAKER_RECOVERY_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30_000);
    let provider_cb = Arc::new(CircuitBreaker::new(
        format!("provider:{}", base_provider.provider_id()),
        cb_threshold,
        cb_recovery_ms,
    ));
    cb_registry.register(provider_cb.clone());
    Ok(Arc::new(CircuitBreakerProvider::new(
        base_provider,
        provider_cb,
    )))
}

/// Build a HashMap of CLI executors for HybridExecutor construction.
fn build_cli_executor_map(
    config: &CliConfig,
) -> HashMap<String, Box<dyn engine::executor_adapter::Executor>> {
    let mut executors: HashMap<String, Box<dyn engine::executor_adapter::Executor>> =
        HashMap::new();

    if config.claude_code_enabled {
        if let Some(ref bin) = config.claude_code_bin {
            executors.insert(
                "claude_code_cli".to_string(),
                Box::new(ClaudeCodeCliExecutor::new(bin.clone(), config.timeout_ms)),
            );
        }
    }

    if config.codex_enabled {
        if let Some(ref bin) = config.codex_bin {
            executors.insert(
                "codex_cli".to_string(),
                Box::new(CodexCliExecutor::new(bin.clone(), config.timeout_ms)),
            );
        }
    }

    executors
}

fn build_multi_executor(config: &CliConfig) -> MultiExecutor {
    let mut executors: HashMap<String, Box<dyn engine::executor_adapter::Executor>> =
        HashMap::new();

    if config.claude_code_enabled {
        if let Some(ref bin) = config.claude_code_bin {
            println!("[acp-cli] claude_code_cli enabled: {}", bin);
            executors.insert(
                "claude_code_cli".to_string(),
                Box::new(ClaudeCodeCliExecutor::new(bin.clone(), config.timeout_ms)),
            );
        }
    }

    if config.codex_enabled {
        if let Some(ref bin) = config.codex_bin {
            println!("[acp-cli] codex_cli enabled: {}", bin);
            executors.insert(
                "codex_cli".to_string(),
                Box::new(CodexCliExecutor::new(bin.clone(), config.timeout_ms)),
            );
        }
    }

    if executors.is_empty() {
        println!("[acp-cli] no CLI executors available; using noop default");
    }

    MultiExecutor::new(executors).with_default(Box::new(NoopExecutor))
}

/// Validates TLS env var consistency. Returns `Err` when exactly one of
/// `ACP_TLS_CERT_PATH` / `ACP_TLS_KEY_PATH` is set, because both are
/// required to enable TLS.
pub fn validate_tls_config(cert: Option<&str>, key: Option<&str>) -> Result<(), String> {
    match (cert, key) {
        (Some(_), Some(_)) => Ok(()),
        (None, None) => Ok(()),
        (Some(_), None) => Err(
            "ACP_TLS_CERT_PATH is set but ACP_TLS_KEY_PATH is not. Both must be set to enable TLS."
                .to_string(),
        ),
        (None, Some(_)) => Err(
            "ACP_TLS_KEY_PATH is set but ACP_TLS_CERT_PATH is not. Both must be set to enable TLS."
                .to_string(),
        ),
    }
}

pub fn production_profile_violations(host: &str, require_auth: bool) -> Vec<&'static str> {
    let cors = std::env::var("ACP_CORS_ORIGINS").unwrap_or_default();
    let has_backup_dir = std::env::var("ACP_BACKUP_DIR").is_ok();
    production_profile_violations_inner(host, require_auth, &cors, has_backup_dir)
}

fn production_profile_violations_inner(
    host: &str,
    require_auth: bool,
    cors: &str,
    has_backup_dir: bool,
) -> Vec<&'static str> {
    let mut violations = Vec::new();
    if !require_auth {
        violations.push("ACP_REQUIRE_AUTH must be enabled (set ACP_REQUIRE_AUTH=1)");
    }
    if cors.is_empty() || cors == "*" {
        violations.push("ACP_CORS_ORIGINS must not be '*' (set explicit origin allowlist)");
    }
    if !has_backup_dir {
        violations.push("ACP_BACKUP_DIR must be configured");
    }
    if host == "0.0.0.0" && !require_auth {
        violations.push("LAN-exposed (HOST=0.0.0.0) requires ACP_REQUIRE_AUTH=1");
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_admin_scope_list_covers_operator_mutations() {
        let scopes = local_admin_scope_list();
        assert!(scopes.iter().any(|scope| scope == "backup:admin"));
        assert!(scopes.iter().any(|scope| scope == "dispatch:write"));
        assert!(scopes.iter().any(|scope| scope == "health:read"));
    }

    #[test]
    fn ga2_production_profile_clean_config_passes() {
        let violations =
            production_profile_violations_inner("127.0.0.1", true, "https://example.com", true);
        assert!(
            violations.is_empty(),
            "clean production config should have no violations: {:?}",
            violations
        );
    }

    #[test]
    fn ga2_production_profile_no_auth_fails() {
        let violations =
            production_profile_violations_inner("127.0.0.1", false, "https://example.com", true);
        assert!(
            violations.iter().any(|v| v.contains("ACP_REQUIRE_AUTH")),
            "should require auth: {:?}",
            violations
        );
    }

    #[test]
    fn ga2_production_profile_wildcard_cors_fails() {
        let violations = production_profile_violations_inner("127.0.0.1", true, "*", true);
        assert!(
            violations.iter().any(|v| v.contains("ACP_CORS_ORIGINS")),
            "should reject wildcard CORS: {:?}",
            violations
        );
    }

    #[test]
    fn ga2_production_profile_no_backup_dir_fails() {
        let violations =
            production_profile_violations_inner("127.0.0.1", true, "https://example.com", false);
        assert!(
            violations.iter().any(|v| v.contains("ACP_BACKUP_DIR")),
            "should require backup dir: {:?}",
            violations
        );
    }

    #[test]
    fn ga2_production_profile_lan_no_auth_fails() {
        let violations =
            production_profile_violations_inner("0.0.0.0", false, "https://example.com", true);
        assert!(
            violations.iter().any(|v| v.contains("LAN-exposed")),
            "should reject LAN without auth: {:?}",
            violations
        );
    }

    #[test]
    fn tls_single_sided_env_not_allowed() {
        // both unset → ok
        assert!(validate_tls_config(None, None).is_ok());
        // both set → ok
        assert!(validate_tls_config(Some("/tmp/cert.pem"), Some("/tmp/key.pem")).is_ok());
        // cert only → err
        let err = validate_tls_config(Some("/tmp/cert.pem"), None).unwrap_err();
        assert!(
            err.contains("ACP_TLS_CERT_PATH"),
            "should mention cert env var: {err}"
        );
        assert!(
            err.contains("ACP_TLS_KEY_PATH"),
            "should mention key env var: {err}"
        );
        // key only → err
        let err = validate_tls_config(None, Some("/tmp/key.pem")).unwrap_err();
        assert!(
            err.contains("ACP_TLS_KEY_PATH"),
            "should mention key env var: {err}"
        );
        assert!(
            err.contains("ACP_TLS_CERT_PATH"),
            "should mention cert env var: {err}"
        );
    }
}
