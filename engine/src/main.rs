use engine::cli::{ClaudeCodeCliExecutor, CliConfig, CodexCliExecutor, MultiExecutor};
use engine::dispatch_engine::DispatchEngine;
use engine::executor::HybridExecutor;
use engine::executor_adapter::NoopExecutor;
use engine::http_server::{
    build_axum_router, build_axum_router_with_dashboard, AxumApiState, CliCapability,
};
use engine::infrastructure::auth::{
    hash_api_key, validate_token_shape, APIKey, Tenant, TenantResolver,
};
use engine::infrastructure::circuit_breaker::{CircuitBreaker, CircuitBreakerRegistry};
use engine::infrastructure::rate_limiter::RateLimiter;
use engine::node_executor::NodeExecutor;
#[cfg(test)]
use engine::provider::adaptive_execution::parse_adaptive_provider_endpoints_json;
use engine::provider::adaptive_execution::{
    adaptive_provider_endpoint_configs_from_sources, build_adaptive_provider_runtime_from_configs,
    persisted_adaptive_provider_endpoint_configs, validate_adaptive_provider_endpoint_config,
    AdaptiveExecutionExecutor, AdaptiveExecutionGate, AdaptiveProviderEndpointConfig,
    ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON,
};
use engine::provider::adaptive_observation::PersistingAdaptiveProviderNodeExecutor;
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
use engine::trusted_local::EffectiveExecutionGates;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const ACP_CLAUDE_CODE_CONFIG_PATH: &str = "ACP_CLAUDE_CODE_CONFIG_PATH";
const DEFAULT_PROVIDER_MODEL: &str = "default";

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
    let persisted_endpoint_configs = match persisted_adaptive_provider_endpoint_configs(&store_arc)
    {
        Ok(configs) => configs,
        Err(error) => {
            eprintln!(
                "[acp-warning] persisted adaptive provider config ignored: {}",
                error
            );
            None
        }
    };
    let execution_gates = EffectiveExecutionGates::from_lookup_with_endpoint_configs(
        |key| std::env::var(key).ok(),
        persisted_endpoint_configs.as_deref(),
    );
    let execution_mode = std::env::var("ACP_EXECUTION_MODE")
        .unwrap_or_else(|_| "off".to_string())
        .to_lowercase();

    let (base_engine, _exec_type_label) = match execution_mode.as_str() {
        "provider" => {
            let provider = build_provider_for_engine(&store_arc, &cb_registry)
                .expect("ACP_EXECUTION_MODE=provider requires ACP_PROVIDER_TYPE plus ACP_ENABLE_PROVIDER_EXECUTION=1 or a ready ACP_TRUSTED_LOCAL_PROFILE=1");
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

    let adaptive_execution_enabled = execution_gates.adaptive_execution;
    let require_auth = env_enabled("ACP_REQUIRE_AUTH");
    let has_single_provider =
        std::env::var("ACP_PROVIDER_TYPE").is_ok_and(|value| !value.trim().is_empty());
    let has_endpoint_config = std::env::var(ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON)
        .is_ok_and(|value| !value.trim().is_empty())
        || persisted_endpoint_configs
            .as_ref()
            .is_some_and(|configs| !configs.is_empty());
    validate_adaptive_startup(
        adaptive_execution_enabled,
        execution_gates.provider_execution,
        require_auth,
    )
    .unwrap_or_else(|error| panic!("{error}"));
    if adaptive_single_provider_validation_required(
        adaptive_execution_enabled,
        has_single_provider,
        has_endpoint_config,
    ) {
        validate_adaptive_single_provider_from_env().unwrap_or_else(|error| {
            panic!("adaptive single-provider configuration failed: {error}")
        });
    }

    let mut state = build_state_with_provider(
        AxumApiState::new().with_engine(base_engine),
        &store_arc,
        &cb_registry,
    );
    let mut adaptive_executor_for_workers = None;
    if adaptive_execution_enabled {
        if let Some((executor, registry_snapshot)) = build_adaptive_provider_executor_from_sources(
            &store_arc,
            &cb_registry,
            persisted_endpoint_configs.as_deref(),
        )
        .unwrap_or_else(|error| panic!("adaptive provider configuration failed: {error}"))
        {
            adaptive_executor_for_workers = Some(executor.clone());
            state = state
                .with_adaptive_provider_executor(executor)
                .with_adaptive_registry_snapshot(registry_snapshot);
        }
    }
    let state = configure_auth(
        state
            .with_local_store_arc(store_arc.clone())
            .with_backup_dir(backup_dir)
            .with_circuit_breaker_registry(cb_registry.clone())
            .with_cli_capability(CliCapability::from(&cli_config)),
    );

    let may_use_provider = state.executor_type() == "provider"
        || (execution_mode == "auto"
            && build_provider_for_engine(&store_arc, &cb_registry).is_ok())
        || adaptive_execution_enabled;
    if may_use_provider && !require_auth {
        panic!(
            "ACP_REQUIRE_AUTH=1 is required when provider execution is enabled and a real provider is configured"
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
        "[acp-startup] execution_mode={} executor={} cli=[{}] auth={} host={} budget_per_dispatch={} budget_daily={} trusted_local_requested={} trusted_local_ready={} task_advancement_requested={} task_advancement_ready={} lan={}",
        execution_mode,
        exec_type_display,
        cli_summary,
        if require_auth { "on" } else { "off" },
        addr,
        cost_per_dispatch,
        cost_daily,
        execution_gates.profile.requested,
        execution_gates.profile.ready,
        execution_gates.task_advancement.requested,
        execution_gates.task_advancement.ready,
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

    let enable_scheduler = execution_gates.scheduler_enabled;
    let backup_interval_sec: u64 = std::env::var("ACP_BACKUP_INTERVAL_SEC")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let backup_retain_count: usize = std::env::var("ACP_BACKUP_RETAIN_COUNT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    let state = if enable_scheduler {
        let scheduler_config = SchedulerConfig::from_env_with_gates(&execution_gates)
            .expect("invalid scheduler env configuration");
        scheduler_config
            .validate_for_start()
            .expect("invalid supervised worker configuration");
        let executor_type = scheduler_config.executor_type.clone();
        let interval = scheduler_config.interval_ms;
        let worker_count = scheduler_config.worker_count;
        let mut scheduler = WorkflowScheduler::new(store_for_scheduler, scheduler_config);
        if execution_gates.task_advancement.ready {
            let adaptive_executor = adaptive_executor_for_workers
                .clone()
                .expect("trusted task advancement requires adaptive provider executor");
            let worker_executor = build_trusted_adaptive_worker_executor(
                adaptive_executor,
                store_arc.clone(),
                require_auth,
                &execution_gates,
            )
            .expect("failed to build trusted adaptive worker executor");
            scheduler = scheduler.with_worker_executor(worker_executor);
        }
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

fn env_enabled(key: &str) -> bool {
    std::env::var(key)
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn provider_model_from_env() -> String {
    if let Ok(model) = std::env::var("ACP_MODEL") {
        let trimmed = model.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    claude_code_config_model_for_current_dir().unwrap_or_else(|| DEFAULT_PROVIDER_MODEL.to_string())
}

fn claude_code_config_model_for_current_dir() -> Option<String> {
    let config_path = claude_code_config_path()?;
    let current_dir = std::env::current_dir().ok()?;
    let raw = std::fs::read_to_string(config_path).ok()?;
    let config: serde_json::Value = serde_json::from_str(&raw).ok()?;
    claude_code_config_model_for_project(&config, &current_dir)
}

fn claude_code_config_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var(ACP_CLAUDE_CODE_CONFIG_PATH) {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".claude.json"))
}

fn claude_code_config_model_for_project(
    config: &serde_json::Value,
    project_dir: &Path,
) -> Option<String> {
    let project_key = project_dir.to_string_lossy();
    let project = config
        .get("projects")
        .and_then(serde_json::Value::as_object)
        .and_then(|projects| projects.get(project_key.as_ref()))?;
    project
        .get("model")
        .and_then(serde_json::Value::as_str)
        .and_then(valid_claude_code_model)
        .or_else(|| {
            project
                .get("lastModelUsage")
                .and_then(serde_json::Value::as_object)
                .and_then(|usage| {
                    usage
                        .keys()
                        .find_map(|model| valid_claude_code_model(model))
                })
        })
}

fn valid_claude_code_model(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.len() > 160
        || engine::provider::redaction::contains_sensitive_patterns(trimmed)
        || !trimmed
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.:/@[]".contains(character))
    {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn validate_adaptive_startup(
    adaptive_enabled: bool,
    provider_enabled: bool,
    require_auth: bool,
) -> Result<(), &'static str> {
    if !adaptive_enabled {
        return Ok(());
    }
    if !provider_enabled {
        return Err(
            "ACP_ENABLE_PROVIDER_EXECUTION=1 or a ready ACP_TRUSTED_LOCAL_PROFILE=1 is required when adaptive execution is enabled",
        );
    }
    if !require_auth {
        return Err("ACP_REQUIRE_AUTH=1 is required when adaptive execution is enabled");
    }
    Ok(())
}

fn adaptive_single_provider_validation_required(
    adaptive_enabled: bool,
    has_single_provider: bool,
    has_endpoint_config: bool,
) -> bool {
    adaptive_enabled && has_single_provider && !has_endpoint_config
}

fn validate_adaptive_single_provider_from_env() -> Result<(), String> {
    let provider_type = std::env::var("ACP_PROVIDER_TYPE")
        .map_err(|_| "ACP_PROVIDER_TYPE is required".to_string())?;
    let model = provider_model_from_env();
    let base_url = std::env::var("ACP_BASE_URL").ok();
    let credential_env = std::env::var("ACP_API_KEY").ok();
    let endpoint_id = match provider_type.as_str() {
        "stub" => "stub-env",
        "openai_compatible" => "openai-env",
        "anthropic" => "anthropic-env",
        other => return Err(format!("unknown ACP_PROVIDER_TYPE: {other}")),
    };
    validate_adaptive_single_provider_config(
        endpoint_id,
        &provider_type,
        base_url.as_deref(),
        &model,
        credential_env.as_deref(),
        credential_env.as_deref().is_some_and(|name| {
            CredentialBoundary::new("env").is_ok_and(|boundary| boundary.validate(name))
        }),
    )
}

fn single_provider_execution_enabled() -> bool {
    single_provider_execution_enabled_from_parts(
        env_enabled("ACP_ENABLE_PROVIDER_EXECUTION"),
        EffectiveExecutionGates::from_env().provider_execution,
        validate_adaptive_single_provider_from_env().is_ok(),
    )
}

fn single_provider_execution_enabled_from_parts(
    legacy_provider_gate: bool,
    trusted_local_provider_execution: bool,
    single_provider_config_ok: bool,
) -> bool {
    legacy_provider_gate || (trusted_local_provider_execution && single_provider_config_ok)
}

fn validate_adaptive_single_provider_config(
    endpoint_id: &str,
    provider_type: &str,
    base_url: Option<&str>,
    model: &str,
    credential_env: Option<&str>,
    credential_available: bool,
) -> Result<(), String> {
    let config = AdaptiveProviderEndpointConfig {
        endpoint_id: endpoint_id.to_string(),
        provider_type: provider_type.to_string(),
        base_url: base_url.map(str::to_string),
        model: model.to_string(),
        credential_env: credential_env.map(str::to_string),
        timeout_ms: 30_000,
        input_cost_per_1k_usd: None,
        output_cost_per_1k_usd: None,
    };
    validate_adaptive_provider_endpoint_config(&config).map_err(|error| error.to_string())?;
    if provider_type != "stub" && !credential_available {
        return Err("referenced adaptive provider credential is not set".to_string());
    }
    Ok(())
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
        Ok(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => return state,
    };

    let enable_execution = single_provider_execution_enabled();
    if provider_type != "stub" && !enable_execution {
        eprintln!(
            "ACP_PROVIDER_TYPE={} requires ACP_ENABLE_PROVIDER_EXECUTION=1 or a ready ACP_TRUSTED_LOCAL_PROFILE=1; falling back to noop",
            provider_type
        );
        return state;
    }

    let provider = match build_single_provider_from_env(cb_registry) {
        Ok(provider) => provider,
        Err(error) => {
            eprintln!("{error}; falling back to noop");
            return state;
        }
    };
    let recorder = Arc::new(ProviderAuditRecorder::with_store(store.clone()));
    state.with_provider_and_audit(provider, recorder)
}

fn build_adaptive_provider_executor_from_sources(
    store: &Arc<LocalProductStore>,
    cb_registry: &Arc<CircuitBreakerRegistry>,
    persisted_configs: Option<&[AdaptiveProviderEndpointConfig]>,
) -> Result<
    Option<(
        Arc<AdaptiveExecutionExecutor>,
        engine::feedback::ModelEndpointRegistrySnapshot,
    )>,
    String,
> {
    let env_raw = std::env::var(ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON).ok();
    let configs = match adaptive_provider_endpoint_configs_from_sources(
        env_raw.as_deref(),
        persisted_configs,
    )
    .map_err(|error| error.to_string())?
    {
        Some(configs) => configs,
        None => return Ok(None),
    };
    build_adaptive_provider_runtime_from_configs(&configs, store, cb_registry)
        .map(Some)
        .map_err(|error| error.to_string())
}

fn build_trusted_adaptive_worker_executor(
    executor: Arc<AdaptiveExecutionExecutor>,
    store: Arc<LocalProductStore>,
    auth_enabled: bool,
    execution_gates: &EffectiveExecutionGates,
) -> Result<Arc<dyn NodeExecutor>, String> {
    let gate = AdaptiveExecutionGate::from_flags(
        execution_gates.provider_execution,
        execution_gates.adaptive_execution,
        auth_enabled,
    );
    if !gate.is_enabled() {
        return Err("adaptive provider worker gate is not enabled".to_string());
    }
    Ok(Arc::new(
        PersistingAdaptiveProviderNodeExecutor::new_with_effective_gates(
            executor,
            gate,
            execution_gates.clone(),
            store,
            "scheduler",
        ),
    ))
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

fn build_single_provider_from_env(
    cb_registry: &Arc<CircuitBreakerRegistry>,
) -> Result<Arc<dyn Provider>, String> {
    let provider_type = std::env::var("ACP_PROVIDER_TYPE")
        .map_err(|_| "ACP_PROVIDER_TYPE not set".to_string())?;
    let provider_type = provider_type.trim();
    if provider_type.is_empty() {
        return Err("ACP_PROVIDER_TYPE is empty".to_string());
    }

    if provider_type == "stub" {
        return Ok(Arc::new(StubProvider::new("stub-env")));
    }

    let api_key_env = std::env::var("ACP_API_KEY").unwrap_or_default();
    let model = provider_model_from_env();
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

    let base_provider: Arc<dyn Provider> = match provider_type {
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

/// Build a provider Arc for engine injection (separate from the API state provider).
/// Returns `Err` if ACP_PROVIDER_TYPE is unset or provider execution is not enabled.
fn build_provider_for_engine(
    _store: &Arc<LocalProductStore>,
    cb_registry: &Arc<CircuitBreakerRegistry>,
) -> Result<Arc<dyn Provider>, String> {
    if !single_provider_execution_enabled() {
        return Err(
            "provider execution not enabled by ACP_ENABLE_PROVIDER_EXECUTION or ready ACP_TRUSTED_LOCAL_PROFILE"
                .to_string(),
        );
    }
    build_single_provider_from_env(cb_registry)
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
    use std::sync::OnceLock;

    fn main_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn clear_trusted_provider_env() {
        for key in [
            "ACP_ENABLE_PROVIDER_EXECUTION",
            "ACP_TRUSTED_LOCAL_PROFILE",
            "ACP_REQUIRE_AUTH",
            "ACP_ADMIN_API_KEY",
            "ACP_PROVIDER_TYPE",
            "ACP_MODEL",
            ACP_CLAUDE_CODE_CONFIG_PATH,
            ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON,
            "ACP_COST_PER_DISPATCH_USD",
            "ACP_COST_DAILY_USD",
        ] {
            std::env::remove_var(key);
        }
    }

    #[test]
    fn local_admin_scope_list_covers_operator_mutations() {
        let scopes = local_admin_scope_list();
        assert!(scopes.iter().any(|scope| scope == "backup:admin"));
        assert!(scopes.iter().any(|scope| scope == "dispatch:write"));
        assert!(scopes.iter().any(|scope| scope == "health:read"));
    }

    #[test]
    fn adaptive_startup_requires_provider_and_auth_but_allows_deferred_endpoint_configuration() {
        assert!(validate_adaptive_startup(false, false, false).is_ok());
        assert!(validate_adaptive_startup(true, false, true).is_err());
        assert!(validate_adaptive_startup(true, true, false).is_err());
        assert!(validate_adaptive_startup(true, true, true).is_ok());
    }

    #[test]
    fn adaptive_single_provider_validation_skips_deferred_endpoint_bootstrap() {
        assert!(!adaptive_single_provider_validation_required(
            true, false, false
        ));
        assert!(adaptive_single_provider_validation_required(
            true, true, false
        ));
        assert!(!adaptive_single_provider_validation_required(
            true, true, true
        ));
        assert!(!adaptive_single_provider_validation_required(
            false, true, false
        ));
    }

    #[test]
    fn adaptive_single_provider_startup_rejects_invalid_or_unavailable_configuration() {
        assert!(validate_adaptive_single_provider_config(
            "stub-env",
            "stub",
            None,
            "stub-model",
            None,
            false,
        )
        .is_ok());
        assert!(validate_adaptive_single_provider_config(
            "openai-env",
            "openai_compatible",
            Some("https://api.example.com/v1"),
            "mimo-v2.5-pro[1M]",
            Some("OPENAI_KEY"),
            true,
        )
        .is_ok());
        assert!(validate_adaptive_single_provider_config(
            "openai-env",
            "openai_compatible",
            Some("http://api.example.com/v1"),
            "quality-model",
            Some("OPENAI_KEY"),
            true,
        )
        .is_err());
        assert!(validate_adaptive_single_provider_config(
            "anthropic-env",
            "anthropic",
            Some("https://api.example.com"),
            "quality-model",
            Some("ANTHROPIC_KEY"),
            false,
        )
        .is_err());
        assert!(validate_adaptive_single_provider_config(
            "unknown-env",
            "unknown",
            None,
            "model",
            None,
            false,
        )
        .is_err());
    }

    #[test]
    fn single_provider_execution_accepts_legacy_or_ready_trusted_local_path() {
        assert!(single_provider_execution_enabled_from_parts(
            true, false, false
        ));
        assert!(single_provider_execution_enabled_from_parts(
            false, true, true
        ));
        assert!(!single_provider_execution_enabled_from_parts(
            false, true, false
        ));
        assert!(!single_provider_execution_enabled_from_parts(
            false, false, true
        ));
    }

    #[test]
    fn provider_model_prefers_explicit_env() {
        let _guard = main_env_lock().lock().unwrap();
        clear_trusted_provider_env();
        std::env::set_var("ACP_MODEL", "explicit-model");

        assert_eq!(provider_model_from_env(), "explicit-model");
        clear_trusted_provider_env();
    }

    #[test]
    fn provider_model_reads_current_project_claude_code_config() {
        let _guard = main_env_lock().lock().unwrap();
        clear_trusted_provider_env();
        let original_dir = std::env::current_dir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let project_dir = dir.path().join("project");
        std::fs::create_dir(&project_dir).unwrap();
        let config_path = dir.path().join("claude.json");
        let config = serde_json::json!({
            "projects": {
                project_dir.to_string_lossy().as_ref(): {
                    "model": "mimo-v2.5-pro[1M]"
                }
            }
        });
        std::fs::write(&config_path, config.to_string()).unwrap();
        std::env::set_var(ACP_CLAUDE_CODE_CONFIG_PATH, &config_path);
        std::env::set_current_dir(&project_dir).unwrap();

        assert_eq!(provider_model_from_env(), "mimo-v2.5-pro[1M]");

        std::env::set_current_dir(original_dir).unwrap();
        clear_trusted_provider_env();
    }

    #[test]
    fn claude_code_config_model_falls_back_to_valid_usage_key() {
        let project_dir = PathBuf::from("/tmp/acp-project");
        let config = serde_json::json!({
            "projects": {
                "/tmp/acp-project": {
                    "lastModelUsage": {
                        "mimo-v2.5": {"inputTokens": 1},
                        "mimo-v2.5-pro[1M]": {"inputTokens": 2}
                    }
                }
            }
        });

        assert_eq!(
            claude_code_config_model_for_project(&config, &project_dir).as_deref(),
            Some("mimo-v2.5")
        );
    }

    #[test]
    fn trusted_local_profile_can_build_single_provider_engine_without_legacy_gate() {
        let _guard = main_env_lock().lock().unwrap();
        clear_trusted_provider_env();
        std::env::set_var("ACP_TRUSTED_LOCAL_PROFILE", "1");
        std::env::set_var("ACP_REQUIRE_AUTH", "1");
        std::env::set_var("ACP_ADMIN_API_KEY", format!("harness_{}", "a".repeat(64)));
        std::env::set_var("ACP_PROVIDER_TYPE", "stub");
        std::env::set_var("ACP_MODEL", "stub-model");
        std::env::set_var(
            ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON,
            r#"[{"endpoint_id":"local-stub","provider_type":"stub","model":"stub-model","timeout_ms":30000,"input_cost_per_1k_usd":0.001,"output_cost_per_1k_usd":0.002}]"#,
        );
        std::env::set_var("ACP_COST_PER_DISPATCH_USD", "1.0");
        std::env::set_var("ACP_COST_DAILY_USD", "10.0");

        let store = Arc::new(LocalProductStore::new(":memory:").unwrap());
        let registry = Arc::new(CircuitBreakerRegistry::new());
        let provider = build_provider_for_engine(&store, &registry)
            .expect("ready trusted-local profile should enable single-provider engine");

        assert!(provider.is_enabled());
        assert_eq!(provider.provider_id(), "stub-env");
        clear_trusted_provider_env();
    }

    #[test]
    fn adaptive_provider_builder_creates_multiple_stub_endpoints() {
        let configs = parse_adaptive_provider_endpoints_json(
            r#"[
                {"endpoint_id":"fast","provider_type":"stub","model":"stub-fast"},
                {"endpoint_id":"quality","provider_type":"stub","model":"stub-quality"}
            ]"#,
        )
        .unwrap();
        let store = Arc::new(LocalProductStore::new(":memory:").unwrap());
        let registry = Arc::new(CircuitBreakerRegistry::new());

        let (_, snapshot) =
            build_adaptive_provider_runtime_from_configs(&configs, &store, &registry).unwrap();

        assert_eq!(
            snapshot
                .endpoints
                .iter()
                .map(|endpoint| endpoint.endpoint_id.clone())
                .collect::<Vec<_>>(),
            vec!["fast", "quality"]
        );
    }

    #[test]
    fn adaptive_provider_config_source_prefers_explicit_env_and_falls_back_to_persisted() {
        let persisted = vec![AdaptiveProviderEndpointConfig {
            endpoint_id: "persisted".to_string(),
            provider_type: "stub".to_string(),
            base_url: None,
            model: "persisted-model".to_string(),
            credential_env: None,
            timeout_ms: 30_000,
            input_cost_per_1k_usd: Some(0.01),
            output_cost_per_1k_usd: Some(0.02),
        }];
        let env_raw = r#"[{
            "endpoint_id":"env",
            "provider_type":"stub",
            "model":"env-model",
            "input_cost_per_1k_usd":0.03,
            "output_cost_per_1k_usd":0.04
        }]"#;

        let from_env =
            adaptive_provider_endpoint_configs_from_sources(Some(env_raw), Some(&persisted))
                .unwrap();
        assert_eq!(from_env.unwrap()[0].endpoint_id, "env");

        let from_persisted =
            adaptive_provider_endpoint_configs_from_sources(None, Some(&persisted)).unwrap();
        assert_eq!(from_persisted.unwrap()[0].endpoint_id, "persisted");

        assert!(
            adaptive_provider_endpoint_configs_from_sources(Some("invalid"), Some(&persisted))
                .is_err()
        );
    }

    #[test]
    fn adaptive_provider_builder_requires_referenced_credentials() {
        std::env::remove_var("_ACP_MISSING_ADAPTIVE_TEST_KEY_");
        let configs = parse_adaptive_provider_endpoints_json(
            r#"[{
                "endpoint_id":"quality",
                "provider_type":"openai_compatible",
                "base_url":"https://api.example.com/v1",
                "model":"quality-model",
                "credential_env":"_ACP_MISSING_ADAPTIVE_TEST_KEY_"
            }]"#,
        )
        .unwrap();
        let store = Arc::new(LocalProductStore::new(":memory:").unwrap());
        let registry = Arc::new(CircuitBreakerRegistry::new());

        let error = match build_adaptive_provider_runtime_from_configs(&configs, &store, &registry)
        {
            Ok(_) => panic!("missing adaptive credential should fail"),
            Err(error) => error,
        };

        assert_eq!(error.code, "credential_env_unavailable");
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
