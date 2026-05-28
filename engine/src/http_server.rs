use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::dispatch_engine::DispatchEngine;
use crate::infrastructure::auth::{AuthDecision, TenantResolver};
use crate::infrastructure::rate_limiter::RateLimiter;

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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DispatchApiRequest {
    pub raw_request: String,
    pub request_source: Option<String>,
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
        .with_state(state)
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
    authorize(&state, &headers, "dispatch:read")?;
    if request.raw_request.trim().is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "raw_request is required",
        ));
    }
    let request_source = request.request_source.as_deref().unwrap_or("api");
    let bundle = state.engine.dispatch(&request.raw_request, request_source);
    Ok((cors_headers(), Json(bundle)))
}

async fn api_openapi(
    State(state): State<AxumApiState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    authorize(&state, &headers, "health:read")?;
    Ok((cors_headers(), Json(openapi_document())))
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
