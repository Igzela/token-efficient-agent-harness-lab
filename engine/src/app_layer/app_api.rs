use serde_json::json;

pub fn handle_api_request(path: &str, method: &str, body: Option<&serde_json::Value>) -> serde_json::Value {
    match (method, path) {
        ("GET", "/status") => json!({"status": "ok", "mode": "local_read_only"}),
        ("GET", "/diagnostics") => json!({"schema_version": "app_diagnostics.v1", "notice": "read-only"}),
        ("GET", "/repos") => json!({"repos": []}),
        ("GET", "/plans") => json!({"plans": []}),
        _ => json!({"error": "not_found", "path": path, "method": method}),
    }
}

pub fn parse_query(query: &str) -> std::collections::HashMap<String, String> {
    let mut params = std::collections::HashMap::new();
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            params.insert(k.to_string(), v.to_string());
        }
    }
    params
}

pub fn split_path(path: &str) -> Vec<&str> {
    path.trim_start_matches('/').split('/').filter(|s| !s.is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_status() {
        let r = handle_api_request("/status", "GET", None);
        assert_eq!(r["status"], "ok");
    }

    #[test]
    fn api_not_found() {
        let r = handle_api_request("/unknown", "GET", None);
        assert_eq!(r["error"], "not_found");
    }

    #[test]
    fn parse_query_basic() {
        let p = parse_query("a=1&b=2");
        assert_eq!(p["a"], "1");
        assert_eq!(p["b"], "2");
    }

    #[test]
    fn split_path_simple() {
        assert_eq!(split_path("/api/v1/plans"), vec!["api", "v1", "plans"]);
    }

    #[test]
    fn split_path_empty() {
        assert!(split_path("/").is_empty());
    }
}
