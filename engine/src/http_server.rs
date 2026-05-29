use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
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

    pub fn with_backup_dir(mut self, backup_dir: impl Into<PathBuf>) -> Self {
        self.backup_dir = Some(Arc::new(backup_dir.into()));
        self
    }

    pub fn with_provider(mut self, provider: Arc<dyn Provider>) -> Self {
        self.engine = Arc::new(DispatchEngine::with_provider_executor(provider.clone()));
        self.provider = Some(provider);
        self
    }

    pub fn with_engine(mut self, engine: DispatchEngine) -> Self {
        self.engine = Arc::new(engine);
        self
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
            "/api/v1/dashboard",
            get(api_dashboard).options(cors_preflight),
        )
        .route("/api/v1/config", get(api_config).options(cors_preflight))
        .route("/api/v1/team", get(api_team).options(cors_preflight))
        .route("/api/v1/costs", get(api_costs).options(cors_preflight))
        .route("/api/v1/export", get(api_export).options(cors_preflight))
        .route("/api/v1/audit", get(api_audit).options(cors_preflight))
        .route(
            "/api/v1/backups",
            post(api_create_backup).options(cors_preflight),
        )
        .route(
            "/api/v1/provider/health",
            get(api_provider_health).options(cors_preflight),
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
    let request_source = request.request_source.as_deref().unwrap_or("api");
    let bundle = state.engine.dispatch(&request.raw_request, request_source);
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
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "dispatch:read")?;
    let store = require_store(&state)?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "dispatches": store.list_dispatches(100).map_err(internal_error)?,
        })),
    ))
}

async fn api_dashboard(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "health:read")?;
    let body = if let Some(store) = &state.local_store {
        store.dashboard_snapshot(20).map_err(internal_error)?
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
                "schema_version": "local_cost_summary.v1",
                "currency": "USD",
                "dispatch_count": 0,
                "total_reserved_cost": 0.0,
                "by_tier": [],
            },
            "boundaries": local_boundaries(),
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
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "config": store.config_snapshot().map_err(internal_error)?,
            "boundaries": local_boundaries(),
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

async fn api_export(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "export:read")?;
    let store = require_store(&state)?;
    Ok((
        cors_headers(),
        Json(store.export_snapshot().map_err(internal_error)?),
    ))
}

async fn api_audit(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "audit:read")?;
    let store = require_store(&state)?;
    Ok((
        cors_headers(),
        Json(json!({
            "schema_version": AXUM_API_SCHEMA_VERSION,
            "events": store.audit_events(100).map_err(internal_error)?,
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
    let guard = resolver
        .lock()
        .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "auth unavailable"))?;
    let decision = guard.resolve(auth_header, state.now);
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
        HeaderValue::from_static("GET,POST,OPTIONS"),
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
                }
            },
            "/api/v1/costs": {
                "get": {
                    "summary": "Read local cost summary from persisted dispatches",
                    "responses": {"200": {"description": "Cost summary"}}
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
            "/api/v1/provider/health": {
                "get": {
                    "summary": "Provider health check",
                    "description": "Reports provider status: noop if no provider configured, ok if enabled, error if disabled or unavailable.",
                    "responses": {
                        "200": {"description": "Provider health status"}
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
}
