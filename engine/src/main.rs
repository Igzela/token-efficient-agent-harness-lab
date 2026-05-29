use engine::http_server::{build_axum_router, build_axum_router_with_dashboard, AxumApiState};
use engine::infrastructure::auth::{
    hash_api_key, validate_token_shape, APIKey, Tenant, TenantResolver,
};
use engine::infrastructure::rate_limiter::RateLimiter;
use engine::storage::local_product_store::LocalProductStore;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[tokio::main]
async fn main() {
    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("{}:{}", host, port);

    let db_path = local_db_path();
    let backup_dir = local_backup_dir(&db_path);
    let store = LocalProductStore::new(&db_path).expect("failed to open local SQLite store");
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
    let state = configure_auth(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(backup_dir),
    );
    let dashboard_dir =
        std::env::var("ACP_DASHBOARD_DIR").or_else(|_| std::env::var("DASHBOARD_DIR"));
    let router = match dashboard_dir {
        Ok(path) if !path.trim().is_empty() => {
            println!("dashboard assets served from {}", path);
            build_axum_router_with_dashboard(state, path)
        }
        _ => build_axum_router(state),
    };

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("engine listening on {}", addr);
    axum::serve(listener, router).await.unwrap();
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
    });
    state.with_auth(resolver, RateLimiter::new(60.0, 10_000), Some(10_000), 0.0)
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
        "dispatch:read",
        "export:read",
        "health:read",
        "team:admin",
        "team:read",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}
