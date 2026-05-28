use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const HTTP_SERVER_SCHEMA_VERSION: &str = "http_server.v1";
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
