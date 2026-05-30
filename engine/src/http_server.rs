use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::dispatch_engine::DispatchEngine;
use crate::infrastructure::auth::{AuthDecision, TenantResolver};
use crate::infrastructure::rate_limiter::RateLimiter;
use crate::provider::cost_gate::{check_cost_gates, CostGateConfig};
use crate::provider::Provider;
use crate::storage::backup_manager::BackupManager;
use crate::storage::local_product_store::{local_boundaries, LocalProductStore};

pub const HTTP_SERVER_SCHEMA_VERSION: &str = "http_server.v1";
pub const AXUM_API_SCHEMA_VERSION: &str = "axum_api.v1";
pub const MAX_BODY_SIZE: usize = 1_048_576; // 1 MB

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteMatch {
    pub method: String,
    pub path: String,
    pub route_pattern: String,
    pub params: HashMap<String, String>,
}

pub type RouteHandler = fn(&RouteMatch, Option<&serde_json::Value>) -> serde_json::Value;

#[derive(Clone)]
pub struct AxumApiState {
    engine: Arc<DispatchEngine>,
    tenant_resolver: Option<Arc<Mutex<TenantResolver>>>,
    rate_limiter: Arc<Mutex<RateLimiter>>,
    default_rate_limit: Option<i64>,
    now: f64,
    dashboard_dir: Option<Arc<PathBuf>>,
    local_store: Option<Arc<LocalProductStore>>,
    backup_dir: Option<Arc<PathBuf>>,
    provider: Option<Arc<dyn Provider>>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DispatchApiRequest {
    pub raw_request: String,
    pub request_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BackupApiRequest {
    pub label: Option<String>,
    pub confirm_local_backup: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CreateApiKeyRequest {
    pub user_id: String,
    pub role: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct UpdateKeyScopesRequest {
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CreateTeamMemberRequest {
    pub user_id: String,
    pub display_name: String,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct UpdateMemberRoleRequest {
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ImportApiRequest {
    pub snapshot: serde_json::Value,
    pub confirm_import: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RestoreApiRequest {
    pub confirm_restore: Option<bool>,
}

#[derive(Debug, Clone)]
struct ApiRequestContext {
    tenant_id: String,
    api_key_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct ApiErrorBody {
    error: String,
    schema_version: String,
}

#[derive(Debug, Clone)]
struct ApiError {
    status: StatusCode,
    error: String,
}

impl ApiError {
    fn new(status: StatusCode, error: impl Into<String>) -> Self {
        Self {
            status,
            error: error.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            cors_headers(),
            Json(ApiErrorBody {
                error: self.error,
                schema_version: AXUM_API_SCHEMA_VERSION.to_string(),
            }),
        )
            .into_response()
    }
}

pub fn build_axum_router(state: AxumApiState) -> Router {
    axum_routes().with_state(state)
}

pub fn build_axum_router_with_dashboard(
    state: AxumApiState,
    dashboard_dir: impl Into<PathBuf>,
) -> Router {
    axum_routes()
        .fallback(serve_dashboard_asset)
        .with_state(state.with_dashboard_dir(dashboard_dir))
}

fn axum_routes() -> Router<AxumApiState> {
    Router::new()
        .route("/api/v1/health", get(api_health).options(cors_preflight))
        .route("/api/v1/ready", get(api_ready).options(cors_preflight))
        .route(
            "/api/v1/openapi.json",
            get(api_openapi).options(cors_preflight),
        )
        .route(
            "/api/v1/dispatch",
            post(api_dispatch).options(cors_preflight),
        )
        .route(
            "/api/v1/dispatches",
            get(api_dispatches).options(cors_preflight),
        )
        .route(
            "/api/v1/dispatches/:dispatch_id",
            get(api_dispatch_detail).options(cors_preflight),
        )
        .route(
            "/api/v1/dashboard",
            get(api_dashboard).options(cors_preflight),
        )
        .route("/api/v1/config", get(api_config).options(cors_preflight))
        .route(
            "/api/v1/team",
            get(api_team)
                .post(api_create_member)
                .options(cors_preflight),
        )
        .route(
            "/api/v1/team/:user_id",
            put(api_update_member_role)
                .delete(api_delete_member)
                .options(cors_preflight),
        )
        .route("/api/v1/costs", get(api_costs).options(cors_preflight))
        .route(
            "/api/v1/costs/dispatches",
            get(api_cost_details).options(cors_preflight),
        )
        .route("/api/v1/export", get(api_export).options(cors_preflight))
        .route("/api/v1/audit", get(api_audit).options(cors_preflight))
        .route(
            "/api/v1/backups",
            get(api_list_backups)
                .post(api_create_backup)
                .options(cors_preflight),
        )
        .route(
            "/api/v1/backups/:backup_id",
            delete(api_delete_backup).options(cors_preflight),
        )
        .route(
            "/api/v1/keys",
            get(api_list_keys)
                .post(api_create_key)
                .options(cors_preflight),
        )
        .route(
            "/api/v1/keys/:key_id/revoke",
            post(api_revoke_key).options(cors_preflight),
        )
        .route(
            "/api/v1/keys/:key_id/rotate",
            post(api_rotate_key).options(cors_preflight),
        )
        .route(
            "/api/v1/keys/:key_id",
            delete(api_delete_key).options(cors_preflight),
        )
        .route(
            "/api/v1/keys/:key_id/scopes",
            post(api_update_key_scopes).options(cors_preflight),
        )
        .route(
            "/api/v1/provider/health",
            get(api_provider_health).options(cors_preflight),
        )
        .route(
            "/api/v1/provider/audit",
            get(api_provider_audit).options(cors_preflight),
        )
        .route(
            "/api/v1/storage/integrity",
            get(api_integrity).options(cors_preflight),
        )
        .route("/api/v1/import", post(api_import).options(cors_preflight))
        .route(
            "/api/v1/backups/:backup_id/restore",
            post(api_restore_backup).options(cors_preflight),
        )
}

async fn serve_dashboard_asset(State(state): State<AxumApiState>, uri: Uri) -> Response {
    if uri.path().starts_with("/api/") {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let Some(dashboard_dir) = &state.dashboard_dir else {
        return (StatusCode::NOT_FOUND, "dashboard not configured").into_response();
    };
    let Some(path) = dashboard_asset_path(dashboard_dir.as_ref(), uri.path()) else {
        return (StatusCode::BAD_REQUEST, "invalid dashboard path").into_response();
    };

    match fs::read(&path) {
        Ok(bytes) => {
            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static(content_type_for_path(&path)),
            );
            (headers, bytes).into_response()
        }
        Err(_) => match fs::read(dashboard_dir.join("index.html")) {
            Ok(bytes) => {
                let mut headers = HeaderMap::new();
                headers.insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/html; charset=utf-8"),
                );
                (headers, bytes).into_response()
            }
            Err(_) => (StatusCode::NOT_FOUND, "dashboard asset not found").into_response(),
        },
    }
}

fn dashboard_asset_path(root: &Path, uri_path: &str) -> Option<PathBuf> {
    let relative = uri_path.trim_start_matches('/');
    if relative.is_empty() {
        return Some(root.join("index.html"));
    }

    let mut path = PathBuf::from(root);
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(part) => path.push(part),
            _ => return None,
        }
    }
    Some(if path.is_dir() {
        path.join("index.html")
    } else {
        path
    })
}

fn content_type_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "css" => "text/css; charset=utf-8",
        "html" => "text/html; charset=utf-8",
        "ico" => "image/x-icon",
        "js" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "txt" => "text/plain; charset=utf-8",
        "wasm" => "application/wasm",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

async fn api_health(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "health:read")?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "status": "healthy",
            "tenant_id": context.tenant_id,
        })),
    ))
}

async fn api_ready(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "health:read")?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "status": "ready",
            "tenant_id": context.tenant_id,
        })),
    ))
}

async fn api_dispatch(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    Json(request): Json<DispatchApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "dispatch:read")?;
    if request.raw_request.trim().is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "raw_request is required",
        ));
    }

    let is_provider = state.executor_type() == "provider";
    if is_provider {
        authorize(&state, &headers, "dispatch:execute")?;
    }

    let request_source = request.request_source.as_deref().unwrap_or("api");

    if is_provider {
        let cost_config = CostGateConfig::from_env();
        if cost_config.is_active() {
            let reserved = state
                .engine
                .preflight_reserved_cost(&request.raw_request, request_source);
            let daily_cost = if let Some(store) = &state.local_store {
                let today_prefix = &chrono_free_today()[..10];
                store.daily_estimated_cost_usd(today_prefix).unwrap_or(0.0)
            } else {
                0.0
            };
            if check_cost_gates(&cost_config, reserved, daily_cost).is_err() {
                let raw = request.raw_request.clone();
                let src = request_source.to_string();
                let eng = Arc::clone(&state.engine);
                let bundle = tokio::task::spawn_blocking(move || eng.dispatch(&raw, &src))
                    .await
                    .map_err(|e| internal_error(e.to_string()))?;
                if let Some(store) = &state.local_store {
                    store
                        .record_dispatch(
                            &request.raw_request,
                            request_source,
                            &bundle,
                            &context.api_key_id,
                        )
                        .map_err(internal_error)?;
                }
                return Ok((cors_headers(), Json(bundle)));
            }
        }
    }

    let raw = request.raw_request.clone();
    let src = request_source.to_string();
    let eng = Arc::clone(&state.engine);
    let bundle = tokio::task::spawn_blocking(move || eng.dispatch(&raw, &src))
        .await
        .map_err(|e| internal_error(e.to_string()))?;
    if let Some(store) = &state.local_store {
        store
            .record_dispatch(
                &request.raw_request,
                request_source,
                &bundle,
                &context.api_key_id,
            )
            .map_err(internal_error)?;
    }
    Ok((cors_headers(), Json(bundle)))
}

async fn api_dispatches(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read")?;
    let store = require_store(&state)?;
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(100)
        .min(500);
    let offset = params
        .get("offset")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0)
        .max(0);
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "dispatches": store.list_dispatches_with_offset(limit, offset).map_err(internal_error)?,
        })),
    ))
}

async fn api_dispatch_detail(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    AxumPath(dispatch_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read")?;
    let store = require_store(&state)?;
    match store.get_dispatch(&dispatch_id).map_err(internal_error)? {
        Some(dispatch) => Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "dispatch": dispatch,
            })),
        )),
        None => Err(ApiError::new(StatusCode::NOT_FOUND, "dispatch not found")),
    }
}

async fn api_dashboard(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "health:read")?;
    let exec_type = state.executor_type();
    let prov_enabled = state.provider_enabled();
    let body = if let Some(store) = &state.local_store {
        store
            .dashboard_snapshot(20, exec_type, prov_enabled)
            .map_err(internal_error)?
    } else {
        json!({
            "schema_version": "local_dashboard.v1",
            "status": "ready",
            "counts": {
                "dispatches": 0,
                "team_members": 0,
                "api_keys": 0,
                "audit_events": 0,
            },
            "dispatches": [],
            "team": {"schema_version": "local_team.v1", "members": [], "api_keys": []},
            "config": {},
            "costs": {
                "schema_version": "local_cost_summary.v2",
                "currency": "USD",
                "dispatch_count": 0,
                "total_reserved_cost": 0.0,
                "total_estimated_cost_usd": 0.0,
                "total_input_tokens": 0,
                "total_output_tokens": 0,
                "cost_utilization": 0.0,
                "by_tier": [],
                "daily": [],
            },
            "boundaries": local_boundaries(exec_type, prov_enabled),
        })
    };
    Ok((cors_headers(), Json(body)))
}

async fn api_config(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "config:read")?;
    let store = require_store(&state)?;
    let exec_type = state.executor_type();
    let prov_enabled = state.provider_enabled();
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "config": store.config_snapshot().map_err(internal_error)?,
            "boundaries": local_boundaries(exec_type, prov_enabled),
        })),
    ))
}

async fn api_team(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "team:read")?;
    let store = require_store(&state)?;
    Ok((
        cors_headers(),
        Json(store.team_snapshot().map_err(internal_error)?),
    ))
}

async fn api_costs(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "cost:read")?;
    let store = require_store(&state)?;
    Ok((
        cors_headers(),
        Json(store.cost_summary().map_err(internal_error)?),
    ))
}

async fn api_cost_details(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "cost:read")?;
    let store = require_store(&state)?;
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(50)
        .min(500);
    Ok((
        cors_headers(),
        Json(store.dispatch_cost_details(limit).map_err(internal_error)?),
    ))
}

async fn api_export(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "export:read")?;
    let store = require_store(&state)?;
    let exec_type = state.executor_type();
    let prov_enabled = state.provider_enabled();
    Ok((
        cors_headers(),
        Json(
            store
                .export_snapshot(exec_type, prov_enabled)
                .map_err(internal_error)?,
        ),
    ))
}

async fn api_audit(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "audit:read")?;
    let store = require_store(&state)?;
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(100)
        .min(500);
    let offset = params
        .get("offset")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0)
        .max(0);
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "events": store.audit_events_with_offset(limit, offset).map_err(internal_error)?,
        })),
    ))
}

async fn api_list_backups(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    if state.tenant_resolver.is_none() {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "admin auth is required for backups",
        ));
    }
    authorize(&state, &headers, "backup:admin")?;
    let store = require_store(&state)?;
    let backup_dir = backup_dir_for_state(&state, store.db_path());
    let manager = BackupManager::new(&backup_dir).map_err(internal_error)?;
    let backups = manager.list_backups().map_err(internal_error)?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "backups": backups,
        })),
    ))
}

async fn api_create_backup(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    Json(request): Json<BackupApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if state.tenant_resolver.is_none() {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "admin auth is required for local backup",
        ));
    }
    let context = authorize(&state, &headers, "backup:admin")?;
    if request.confirm_local_backup != Some(true) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "confirm_local_backup must be true",
        ));
    }
    let store = require_store(&state)?;
    if store.is_memory() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "file-backed local store is required for backup",
        ));
    }
    store.checkpoint_wal().map_err(internal_error)?;

    let backup_dir = backup_dir_for_state(&state, store.db_path());
    let manager = BackupManager::new(&backup_dir).map_err(internal_error)?;
    let mut backups = manager.list_backups().map_err(internal_error)?;
    let backup_id = format!("backup-{:04}", backups.len() + 1);
    let label = request.label.as_deref().unwrap_or("manual");
    let backup = manager
        .create_backup(store.db_path(), label, &backup_id, "2026-05-29T00:00:00Z")
        .map_err(internal_error)?;
    backups.push(backup.clone());
    manager.save_metadata(&backups).map_err(internal_error)?;
    store
        .append_audit(
            &context.api_key_id,
            "backup.create",
            &backup.backup_id,
            &json!({"label": label, "backup_path": backup.backup_path}),
        )
        .map_err(internal_error)?;
    Ok((
        cors_headers(),
        Json(json!({"schema_version": AXUM_API_SCHEMA_VERSION, "backup": backup})),
    ))
}

async fn api_delete_backup(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    AxumPath(backup_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    if state.tenant_resolver.is_none() {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "admin auth is required for backups",
        ));
    }
    let context = authorize(&state, &headers, "backup:admin")?;
    let store = require_store(&state)?;
    let backup_dir = backup_dir_for_state(&state, store.db_path());
    let manager = BackupManager::new(&backup_dir).map_err(internal_error)?;
    let deleted = manager.delete_backup(&backup_id).map_err(internal_error)?;
    if !deleted {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "backup not found"));
    }
    store
        .append_audit(
            &context.api_key_id,
            "backup.delete",
            &backup_id,
            &json!({"backup_id": backup_id}),
        )
        .map_err(internal_error)?;
    Ok((
        cors_headers(),
        Json(
            json!({"schema_version": AXUM_API_SCHEMA_VERSION, "ok": true, "backup_id": backup_id}),
        ),
    ))
}

async fn api_list_keys(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "team:read")?;
    let store = require_store(&state)?;
    let keys = store.list_api_key_metadata(100).map_err(internal_error)?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "keys": keys,
        })),
    ))
}

async fn api_create_key(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    Json(request): Json<CreateApiKeyRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "team:admin")?;
    let store = require_store(&state)?;

    let mut guard = state
        .tenant_resolver
        .as_ref()
        .ok_or_else(|| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "auth unavailable"))?
        .lock()
        .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "auth unavailable"))?;

    let scopes_set: std::collections::HashSet<String> = request.scopes.iter().cloned().collect();
    let (key, raw_key) = guard
        .create_api_key(
            &context.tenant_id,
            Some(scopes_set),
            request.expires_at,
            state.now,
        )
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e))?;

    store
        .record_api_key_metadata(
            &key.key_id,
            &request.user_id,
            &request.role,
            &request.scopes,
            &context.api_key_id,
        )
        .map_err(internal_error)?;

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "key_id": key.key_id,
            "raw_key": raw_key,
            "user_id": request.user_id,
            "role": request.role,
            "scopes": request.scopes,
            "created_at": key.created_at,
        })),
    ))
}

async fn api_revoke_key(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    AxumPath(key_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "team:admin")?;
    let store = require_store(&state)?;

    let revoked = store
        .revoke_api_key_metadata(&key_id, &context.api_key_id)
        .map_err(internal_error)?;

    if let Some(resolver) = &state.tenant_resolver {
        let mut guard = resolver
            .lock()
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "auth unavailable"))?;
        guard.remove_api_key(&key_id);
    }

    if !revoked {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "key not found or already revoked",
        ));
    }

    Ok((
        cors_headers(),
        Json(json!({"schema_version": AXUM_API_SCHEMA_VERSION, "ok": true, "key_id": key_id})),
    ))
}

async fn api_rotate_key(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    AxumPath(key_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "team:admin")?;
    let store = require_store(&state)?;

    let old_key = store
        .get_api_key_metadata(&key_id)
        .map_err(internal_error)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "key not found"))?;

    let user_id = old_key["user_id"]
        .as_str()
        .ok_or_else(|| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "invalid key metadata"))?;
    let role = old_key["role"]
        .as_str()
        .ok_or_else(|| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "invalid key metadata"))?;
    let scopes: Vec<String> = old_key["scopes"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let expires_at = old_key["expires_at"].as_f64();

    store
        .revoke_api_key_metadata(&key_id, &context.api_key_id)
        .map_err(internal_error)?;

    if let Some(resolver) = &state.tenant_resolver {
        let mut guard = resolver
            .lock()
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "auth unavailable"))?;
        guard.remove_api_key(&key_id);

        let scopes_set: std::collections::HashSet<String> = scopes.iter().cloned().collect();
        let (new_key, raw_key) = guard
            .create_api_key(&context.tenant_id, Some(scopes_set), expires_at, state.now)
            .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e))?;

        store
            .record_api_key_metadata(&new_key.key_id, user_id, role, &scopes, &context.api_key_id)
            .map_err(internal_error)?;

        return Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "key_id": new_key.key_id,
                "raw_key": raw_key,
                "user_id": user_id,
                "role": role,
                "scopes": scopes,
                "created_at": new_key.created_at,
                "rotated_from": key_id,
            })),
        ));
    }

    Err(ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "auth unavailable",
    ))
}

async fn api_delete_key(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    AxumPath(key_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "team:admin")?;
    let store = require_store(&state)?;

    let deleted = store
        .delete_api_key_metadata(&key_id, &context.api_key_id)
        .map_err(internal_error)?;

    if let Some(resolver) = &state.tenant_resolver {
        let mut guard = resolver
            .lock()
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "auth unavailable"))?;
        guard.remove_api_key(&key_id);
    }

    if !deleted {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "key not found"));
    }

    Ok((
        cors_headers(),
        Json(json!({"schema_version": AXUM_API_SCHEMA_VERSION, "ok": true, "key_id": key_id})),
    ))
}

async fn api_update_key_scopes(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    AxumPath(key_id): AxumPath<String>,
    Json(request): Json<UpdateKeyScopesRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let _context = authorize(&state, &headers, "team:admin")?;
    let store = require_store(&state)?;

    let updated = store
        .update_api_key_scopes(&key_id, &request.scopes, &_context.api_key_id)
        .map_err(internal_error)?;

    if !updated {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "key not found"));
    }

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "ok": true,
            "key_id": key_id,
            "scopes": request.scopes,
        })),
    ))
}

async fn api_create_member(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    Json(request): Json<CreateTeamMemberRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "team:admin")?;
    let store = require_store(&state)?;

    store
        .upsert_team_member(&request.user_id, &request.display_name, &request.role)
        .map_err(internal_error)?;

    store
        .append_audit(
            &context.api_key_id,
            "team.member.created",
            &request.user_id,
            &json!({"user_id": request.user_id, "display_name": request.display_name, "role": request.role}),
        )
        .map_err(internal_error)?;

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "ok": true,
            "user_id": request.user_id,
            "display_name": request.display_name,
            "role": request.role,
        })),
    ))
}

async fn api_update_member_role(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    AxumPath(user_id): AxumPath<String>,
    Json(request): Json<UpdateMemberRoleRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "team:admin")?;
    let store = require_store(&state)?;

    let updated = store
        .update_team_member_role(&user_id, &request.role, &context.api_key_id)
        .map_err(internal_error)?;

    if !updated {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "member not found"));
    }

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "ok": true,
            "user_id": user_id,
            "role": request.role,
        })),
    ))
}

async fn api_delete_member(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    AxumPath(user_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let context = authorize(&state, &headers, "team:admin")?;
    let store = require_store(&state)?;

    let deleted = store
        .delete_team_member(&user_id, &context.api_key_id)
        .map_err(internal_error)?;

    if !deleted {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "member not found"));
    }

    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "ok": true,
            "user_id": user_id,
        })),
    ))
}

async fn api_openapi(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "health:read")?;
    Ok((cors_headers(), Json(openapi_document())))
}

async fn api_provider_health(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "health:read")?;
    if state.engine.executor_type() == "noop" {
        return Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "status": "noop",
                "message": "no provider configured",
            })),
        ));
    }
    let Some(provider) = &state.provider else {
        return Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "status": "error",
                "message": "provider reference not available",
            })),
        ));
    };
    let enabled = provider.is_enabled();
    let provider_id = provider.provider_id();
    if enabled {
        Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "status": "ok",
                "provider_id": provider_id,
                "enabled": true,
            })),
        ))
    } else {
        Ok((
            cors_headers(),
            Json(json!({
                "schema_version": AXUM_API_SCHEMA_VERSION,
                "status": "error",
                "provider_id": provider_id,
                "enabled": false,
                "message": "provider is disabled",
            })),
        ))
    }
}

async fn api_provider_audit(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "audit:read")?;
    let store = require_store(&state)?;
    let events = store.provider_audit_events(100).map_err(internal_error)?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "events": events,
        })),
    ))
}

async fn api_integrity(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "health:read")?;
    let store = require_store(&state)?;
    let report = store.check_integrity().map_err(internal_error)?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "integrity": {
                "status": report.status,
                "schema_version": report.schema_version,
                "tables": report.tables.iter().map(|t| json!({
                    "name": t.name,
                    "row_count": t.row_count,
                    "status": t.status,
                })).collect::<Vec<_>>(),
            },
        })),
    ))
}

async fn api_import(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    Json(request): Json<ImportApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if state.tenant_resolver.is_none() {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "admin auth is required for import",
        ));
    }
    let context = authorize(&state, &headers, "config:admin")?;
    if request.confirm_import != Some(true) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "confirm_import must be true",
        ));
    }
    let store = require_store(&state)?;
    let result = store
        .import_snapshot(&request.snapshot)
        .map_err(internal_error)?;
    store
        .append_audit(
            &context.api_key_id,
            "data.import",
            "local_product_store",
            &json!({
                "imported": {
                    "dispatches": result.imported.dispatches,
                    "config": result.imported.config,
                    "team": result.imported.team,
                    "audit": result.imported.audit,
                },
                "error_count": result.errors.len(),
            }),
        )
        .map_err(internal_error)?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "imported": {
                "dispatches": result.imported.dispatches,
                "config": result.imported.config,
                "team": result.imported.team,
                "audit": result.imported.audit,
            },
            "errors": result.errors,
        })),
    ))
}

async fn api_restore_backup(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
    AxumPath(backup_id): AxumPath<String>,
    Json(request): Json<RestoreApiRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if state.tenant_resolver.is_none() {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "admin auth is required for restore",
        ));
    }
    let context = authorize(&state, &headers, "backup:admin")?;
    if request.confirm_restore != Some(true) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "confirm_restore must be true",
        ));
    }
    let store = require_store(&state)?;
    if store.is_memory() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "file-backed local store is required for restore",
        ));
    }
    let backup_dir = backup_dir_for_state(&state, store.db_path());
    let manager = BackupManager::new(&backup_dir).map_err(internal_error)?;
    let result = manager
        .restore_backup_with_verify(&backup_id, store.db_path(), state.now)
        .map_err(internal_error)?;
    store
        .append_audit(
            &context.api_key_id,
            "backup.restore",
            &backup_id,
            &json!({
                "success": result.success,
                "records_restored": result.records_restored,
                "errors": result.errors,
            }),
        )
        .map_err(internal_error)?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "restore": {
                "success": result.success,
                "records_restored": result.records_restored,
                "errors": result.errors,
                "duration_ms": result.duration_ms,
            },
        })),
    ))
}

async fn cors_preflight() -> impl IntoResponse {
    (cors_headers(), StatusCode::NO_CONTENT)
}

fn authorize(
    state: &AxumApiState,
    headers: &HeaderMap,
    required_scope: &str,
) -> Result<ApiRequestContext, ApiError> {
    let Some(resolver) = &state.tenant_resolver else {
        return Ok(ApiRequestContext {
            tenant_id: "local".to_string(),
            api_key_id: "none".to_string(),
        });
    };

    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    let mut guard = resolver
        .lock()
        .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "auth unavailable"))?;
    let decision = guard.resolve_mut(auth_header, state.now);
    let context = auth_context_from_decision(decision, required_scope)?;
    let tenant_limit = guard.tenant_rate_limit(&context.tenant_id);
    drop(guard);

    let rate_limit = tenant_limit.or(state.default_rate_limit);
    let mut limiter = state.rate_limiter.lock().map_err(|_| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "rate limiter unavailable",
        )
    })?;
    let rate = limiter.check(
        &context.tenant_id,
        &context.api_key_id,
        rate_limit,
        state.now,
    );
    if !rate.allowed {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate limit exceeded",
        ));
    }

    Ok(context)
}

fn require_store(state: &AxumApiState) -> Result<Arc<LocalProductStore>, ApiError> {
    state
        .local_store
        .as_ref()
        .cloned()
        .ok_or_else(|| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "local store unavailable"))
}

fn backup_dir_for_state(state: &AxumApiState, db_path: &Path) -> PathBuf {
    if let Some(dir) = &state.backup_dir {
        return dir.as_ref().clone();
    }
    db_path
        .parent()
        .map(|parent| parent.join("backups"))
        .unwrap_or_else(|| PathBuf::from("backups"))
}

fn internal_error(error: String) -> ApiError {
    ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error)
}

fn auth_context_from_decision(
    decision: AuthDecision,
    required_scope: &str,
) -> Result<ApiRequestContext, ApiError> {
    if !decision.allowed {
        return Err(ApiError::new(StatusCode::UNAUTHORIZED, "unauthorized"));
    }
    if !decision.scopes.contains(required_scope) {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "forbidden"));
    }
    Ok(ApiRequestContext {
        tenant_id: decision.tenant_id.unwrap_or_else(|| "unknown".to_string()),
        api_key_id: decision.api_key_id.unwrap_or_else(|| "unknown".to_string()),
    })
}

fn cors_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET,POST,PUT,DELETE,OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("authorization,content-type"),
    );
    headers
}

pub fn openapi_document() -> serde_json::Value {
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Agent Control Plane Local API",
            "version": "0.1.0",
            "description": "Deterministic local API. Real providers, sandbox execution, target writes, and runtime workers are disabled by default."
        },
        "paths": {
            "/api/v1/health": {
                "get": {
                    "summary": "Health check",
                    "responses": {
                        "200": {"description": "API is healthy"}
                    }
                }
            },
            "/api/v1/ready": {
                "get": {
                    "summary": "Readiness check",
                    "responses": {
                        "200": {"description": "API is ready"}
                    }
                }
            },
            "/api/v1/openapi.json": {
                "get": {
                    "summary": "OpenAPI document",
                    "responses": {
                        "200": {"description": "OpenAPI JSON document"}
                    }
                }
            },
            "/api/v1/dispatch": {
                "post": {
                    "summary": "Create deterministic dispatch bundle",
                    "description": "Runs local rule-based dispatch only. The default executor is noop and does not call real providers.",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["raw_request"],
                                    "properties": {
                                        "raw_request": {"type": "string"},
                                        "request_source": {"type": "string", "default": "api"}
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {"description": "Dispatch bundle"},
                        "400": {"description": "Invalid request"},
                        "401": {"description": "Unauthorized"},
                        "403": {"description": "Forbidden"},
                        "429": {"description": "Rate limited"}
                    }
                }
            }
            ,
            "/api/v1/dispatches": {
                "get": {
                    "summary": "List persisted local dispatch history",
                    "responses": {"200": {"description": "Dispatch history"}}
                }
            },
            "/api/v1/dispatches/{dispatch_id}": {
                "get": {
                    "summary": "Get a single dispatch by ID",
                    "responses": {
                        "200": {"description": "Dispatch detail"},
                        "404": {"description": "Dispatch not found"}
                    }
                }
            },
            "/api/v1/dashboard": {
                "get": {
                    "summary": "Read local dashboard state from SQLite-backed runtime state",
                    "responses": {"200": {"description": "Dashboard state"}}
                }
            },
            "/api/v1/config": {
                "get": {
                    "summary": "Read local configuration",
                    "responses": {"200": {"description": "Local config"}}
                }
            },
            "/api/v1/team": {
                "get": {
                    "summary": "Read local team and redacted API key metadata",
                    "responses": {"200": {"description": "Team state"}}
                },
                "post": {
                    "summary": "Create or update a team member",
                    "description": "Requires team:admin scope.",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["user_id", "display_name", "role"],
                                    "properties": {
                                        "user_id": {"type": "string"},
                                        "display_name": {"type": "string"},
                                        "role": {"type": "string"}
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {"description": "Member created"},
                        "403": {"description": "Forbidden"}
                    }
                }
            },
            "/api/v1/team/{user_id}": {
                "put": {
                    "summary": "Update a team member's role",
                    "description": "Requires team:admin scope.",
                    "responses": {
                        "200": {"description": "Member updated"},
                        "404": {"description": "Member not found"}
                    }
                },
                "delete": {
                    "summary": "Remove a team member",
                    "description": "Requires team:admin scope.",
                    "responses": {
                        "200": {"description": "Member removed"},
                        "404": {"description": "Member not found"}
                    }
                }
            },
            "/api/v1/costs": {
                "get": {
                    "summary": "Read local cost summary from persisted dispatches",
                    "responses": {"200": {"description": "Cost summary"}}
                }
            },
            "/api/v1/costs/dispatches": {
                "get": {
                    "summary": "Read per-dispatch cost details",
                    "parameters": [
                        {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 50, "maximum": 500}}
                    ],
                    "responses": {"200": {"description": "Per-dispatch cost details"}}
                }
            },
            "/api/v1/export": {
                "get": {
                    "summary": "Export local app-owned state",
                    "responses": {"200": {"description": "Local export"}}
                }
            },
            "/api/v1/audit": {
                "get": {
                    "summary": "Read local audit log",
                    "responses": {"200": {"description": "Audit log"}}
                }
            },
            "/api/v1/backups": {
                "get": {
                    "summary": "List local SQLite backups",
                    "description": "Requires backup:admin scope.",
                    "responses": {
                        "200": {"description": "Backup list"},
                        "403": {"description": "Forbidden"}
                    }
                },
                "post": {
                    "summary": "Create a local SQLite backup",
                    "description": "Requires backup:admin scope and confirm_local_backup=true.",
                    "responses": {
                        "200": {"description": "Backup metadata"},
                        "400": {"description": "Missing explicit confirmation"},
                        "403": {"description": "Forbidden"}
                    }
                }
            },
            "/api/v1/backups/{backup_id}": {
                "delete": {
                    "summary": "Delete a local backup",
                    "description": "Requires backup:admin scope.",
                    "responses": {
                        "200": {"description": "Backup deleted"},
                        "404": {"description": "Backup not found"},
                        "403": {"description": "Forbidden"}
                    }
                }
            },
            "/api/v1/keys": {
                "get": {
                    "summary": "List API key metadata",
                    "description": "Requires team:read scope. Returns metadata only — no raw keys.",
                    "responses": {
                        "200": {"description": "List of API key metadata"},
                        "403": {"description": "Forbidden"}
                    }
                },
                "post": {
                    "summary": "Create a new API key",
                    "description": "Requires team:admin scope. Returns the raw key once — it cannot be retrieved later.",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["user_id", "role", "scopes"],
                                    "properties": {
                                        "user_id": {"type": "string"},
                                        "role": {"type": "string"},
                                        "scopes": {"type": "array", "items": {"type": "string"}},
                                        "expires_at": {"type": "number"}
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {"description": "Created key with raw_key"},
                        "400": {"description": "Invalid scopes or tenant"},
                        "403": {"description": "Forbidden"}
                    }
                }
            },
            "/api/v1/keys/{key_id}/revoke": {
                "post": {
                    "summary": "Revoke an API key",
                    "description": "Requires team:admin scope. The key will no longer authenticate.",
                    "responses": {
                        "200": {"description": "Key revoked"},
                        "404": {"description": "Key not found"}
                    }
                }
            },
            "/api/v1/keys/{key_id}/rotate": {
                "post": {
                    "summary": "Rotate an API key",
                    "description": "Requires team:admin scope. Creates a new key and revokes the old one.",
                    "responses": {
                        "200": {"description": "New key with raw_key"},
                        "404": {"description": "Key not found"}
                    }
                }
            },
            "/api/v1/keys/{key_id}": {
                "delete": {
                    "summary": "Delete an API key",
                    "description": "Requires team:admin scope. Hard-deletes key metadata.",
                    "responses": {
                        "200": {"description": "Key deleted"},
                        "404": {"description": "Key not found"}
                    }
                }
            },
            "/api/v1/keys/{key_id}/scopes": {
                "post": {
                    "summary": "Update an API key's scopes",
                    "description": "Requires team:admin scope.",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["scopes"],
                                    "properties": {
                                        "scopes": {"type": "array", "items": {"type": "string"}}
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {"description": "Scopes updated"},
                        "404": {"description": "Key not found"}
                    }
                }
            },
            "/api/v1/provider/health": {
                "get": {
                    "summary": "Provider health check",
                    "description": "Reports provider status: noop if no provider configured, ok if enabled, error if disabled or unavailable.",
                    "responses": {
                        "200": {"description": "Provider health status"}
                    }
                }
            },
            "/api/v1/storage/integrity": {
                "get": {
                    "summary": "SQLite integrity check and table row counts",
                    "responses": {
                        "200": {"description": "Integrity report with per-table status"}
                    }
                }
            },
            "/api/v1/import": {
                "post": {
                    "summary": "Import data from an export snapshot",
                    "description": "Requires config:admin scope and confirm_import=true. Imports config, team, audit, and dispatches idempotently.",
                    "responses": {
                        "200": {"description": "Import result with counts and errors"},
                        "400": {"description": "Missing confirmation or invalid schema"}
                    }
                }
            },
            "/api/v1/backups/{backup_id}/restore": {
                "post": {
                    "summary": "Restore a backup with integrity verification",
                    "description": "Requires backup:admin scope and confirm_restore=true. Restores from backup, runs integrity check, reports row counts.",
                    "responses": {
                        "200": {"description": "Restore result"},
                        "400": {"description": "Missing confirmation"},
                        "404": {"description": "Backup not found"}
                    }
                }
            }
        }
    })
}

pub struct ServerContext {
    pub config: ServerConfig,
    routes: HashMap<(String, String), RouteHandler>,
    route_scopes: HashMap<(String, String), Vec<String>>,
}

impl ServerContext {
    pub fn new(config: ServerConfig) -> Self {
        Self {
            config,
            routes: HashMap::new(),
            route_scopes: HashMap::new(),
        }
    }

    pub fn register_route(
        &mut self,
        method: &str,
        path: &str,
        handler: RouteHandler,
        required_scopes: Option<Vec<String>>,
    ) {
        let key = (method.to_string(), path.to_string());
        if let Some(scopes) = required_scopes {
            self.route_scopes.insert(key.clone(), scopes);
        } else {
            self.route_scopes.remove(&key);
        }
        self.routes.insert(key, handler);
    }

    pub fn clear_routes(&mut self) {
        self.routes.clear();
        self.route_scopes.clear();
    }

    pub fn match_route(&self, method: &str, path: &str) -> Option<(RouteHandler, RouteMatch)> {
        let clean_path = if let Some(idx) = path.find('?') {
            &path[..idx]
        } else {
            path
        };

        let prefix = &self.config.api_prefix;
        let stripped = if clean_path.starts_with(prefix) {
            &clean_path[prefix.len()..]
        } else {
            clean_path
        };
        let normalized = if stripped.starts_with('/') {
            stripped.to_string()
        } else {
            format!("/{stripped}")
        };

        for ((route_method, route_path), handler) in &self.routes {
            if route_method != method {
                continue;
            }
            if let Some(params) = match_path(route_path, &normalized) {
                return Some((
                    *handler,
                    RouteMatch {
                        method: method.to_string(),
                        path: normalized.clone(),
                        route_pattern: route_path.clone(),
                        params,
                    },
                ));
            }
        }
        None
    }

    pub fn check_scopes(
        &self,
        route_key: &(String, String),
        granted_scopes: &[String],
    ) -> (bool, String) {
        let required = match self.route_scopes.get(route_key) {
            Some(s) if !s.is_empty() => s,
            _ => return (true, String::new()),
        };
        let granted_set: std::collections::HashSet<&str> =
            granted_scopes.iter().map(|s| s.as_str()).collect();
        let required_set: std::collections::HashSet<&str> =
            required.iter().map(|s| s.as_str()).collect();
        if required_set.is_subset(&granted_set) {
            (true, String::new())
        } else {
            let missing: Vec<String> = required_set
                .difference(&granted_set)
                .map(|s| s.to_string())
                .collect();
            (false, format!("missing scopes: {}", missing.join(", ")))
        }
    }
}

fn match_path(pattern: &str, path: &str) -> Option<HashMap<String, String>> {
    let pattern_parts: Vec<&str> = pattern.trim_matches('/').split('/').collect();
    let path_parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    if pattern_parts.len() != path_parts.len() {
        return None;
    }
    let mut params = HashMap::new();
    for (pp, rp) in pattern_parts.iter().zip(path_parts.iter()) {
        if pp.starts_with('{') && pp.ends_with('}') {
            params.insert(pp[1..pp.len() - 1].to_string(), rp.to_string());
        } else if pp != rp {
            return None;
        }
    }
    Some(params)
}

fn chrono_free_today() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86400;
    let mut y = 1970i64;
    loop {
        let leap = is_leap(y);
        let day_count = if leap { 366 } else { 365 };
        if days < day_count as u64 {
            break;
        }
        y += 1;
    }
    let mut remaining = days;
    let leap = is_leap(y);
    let month_days: [i64; 12] = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut m = 0u32;
    for (i, &md) in month_days.iter().enumerate() {
        if remaining < md as u64 {
            m = i as u32 + 1;
            break;
        }
        remaining -= md as u64;
    }
    let d = remaining + 1;
    format!("{:04}-{:02}-{:02}", y, m, d)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_handler(_rm: &RouteMatch, _body: Option<&serde_json::Value>) -> serde_json::Value {
        serde_json::json!({"status": "ok"})
    }

    #[test]
    fn test_match_route_exact() {
        let mut ctx = ServerContext::new(ServerConfig::default());
        ctx.register_route("GET", "/health", dummy_handler, None);
        let result = ctx.match_route("GET", "/api/v1/health");
        assert!(result.is_some());
        let (_, rm) = result.unwrap();
        assert_eq!(rm.route_pattern, "/health");
    }

    #[test]
    fn test_match_route_with_params() {
        let mut ctx = ServerContext::new(ServerConfig::default());
        ctx.register_route("GET", "/plans/{plan_id}", dummy_handler, None);
        let result = ctx.match_route("GET", "/api/v1/plans/p123");
        assert!(result.is_some());
        let (_, rm) = result.unwrap();
        assert_eq!(rm.params.get("plan_id").unwrap(), "p123");
    }

    #[test]
    fn test_match_route_not_found() {
        let mut ctx = ServerContext::new(ServerConfig::default());
        ctx.register_route("GET", "/health", dummy_handler, None);
        assert!(ctx.match_route("GET", "/api/v1/nonexistent").is_none());
    }

    #[test]
    fn test_match_route_method_mismatch() {
        let mut ctx = ServerContext::new(ServerConfig::default());
        ctx.register_route("GET", "/health", dummy_handler, None);
        assert!(ctx.match_route("POST", "/api/v1/health").is_none());
    }

    #[test]
    fn test_match_path_simple() {
        assert!(match_path("/health", "/health").is_some());
        assert!(match_path("/health", "/other").is_none());
    }

    #[test]
    fn test_match_path_params() {
        let params = match_path("/plans/{id}", "/plans/abc").unwrap();
        assert_eq!(params.get("id").unwrap(), "abc");
    }

    #[test]
    fn test_match_path_wrong_segment_count() {
        assert!(match_path("/a/b", "/a").is_none());
        assert!(match_path("/a", "/a/b").is_none());
    }

    #[test]
    fn test_check_scopes_empty_required() {
        let ctx = ServerContext::new(ServerConfig::default());
        let (ok, _) = ctx.check_scopes(&("GET".to_string(), "/health".to_string()), &[]);
        assert!(ok);
    }

    #[test]
    fn test_check_scopes_granted() {
        let mut ctx = ServerContext::new(ServerConfig::default());
        ctx.register_route(
            "GET",
            "/plans",
            dummy_handler,
            Some(vec!["dispatch:read".to_string()]),
        );
        let (ok, _) = ctx.check_scopes(
            &("GET".to_string(), "/plans".to_string()),
            &["dispatch:read".to_string()],
        );
        assert!(ok);
    }

    #[test]
    fn test_check_scopes_missing() {
        let mut ctx = ServerContext::new(ServerConfig::default());
        ctx.register_route(
            "GET",
            "/plans",
            dummy_handler,
            Some(vec!["dispatch:write".to_string()]),
        );
        let (ok, reason) = ctx.check_scopes(
            &("GET".to_string(), "/plans".to_string()),
            &["dispatch:read".to_string()],
        );
        assert!(!ok);
        assert!(reason.contains("missing scopes"));
    }

    #[test]
    fn test_clear_routes() {
        let mut ctx = ServerContext::new(ServerConfig::default());
        ctx.register_route("GET", "/health", dummy_handler, None);
        assert!(ctx.match_route("GET", "/api/v1/health").is_some());
        ctx.clear_routes();
        assert!(ctx.match_route("GET", "/api/v1/health").is_none());
    }

    #[test]
    fn test_query_string_stripped() {
        let mut ctx = ServerContext::new(ServerConfig::default());
        ctx.register_route("GET", "/plans", dummy_handler, None);
        let result = ctx.match_route("GET", "/api/v1/plans?limit=10");
        assert!(result.is_some());
    }

    #[test]
    fn test_server_config_default() {
        let config = ServerConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8080);
        assert_eq!(config.api_prefix, "/api/v1");
    }

    #[test]
    fn test_openapi_integrity_route_matches_router() {
        let doc = openapi_document();
        let paths = doc["paths"].as_object().expect("paths should be an object");
        assert!(
            paths.contains_key("/api/v1/storage/integrity"),
            "OpenAPI document must include /api/v1/storage/integrity to match the axum router registration"
        );
        assert!(
            !paths.contains_key("/api/v1/integrity"),
            "OpenAPI document must NOT include /api/v1/integrity (the correct path is /api/v1/storage/integrity)"
        );
    }
}
