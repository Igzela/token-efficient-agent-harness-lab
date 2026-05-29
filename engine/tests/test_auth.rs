use engine::infrastructure::auth::*;
use std::collections::HashSet;

fn make_tenant(id: &str, name: &str) -> Tenant {
    Tenant {
        tenant_id: id.to_string(),
        name: name.to_string(),
        scopes: HashSet::new(),
        rate_limit: None,
    }
}

#[test]
fn test_hash_api_key_deterministic() {
    let h1 = hash_api_key("test_key", "test_salt");
    let h2 = hash_api_key("test_key", "test_salt");
    assert_eq!(h1, h2);
    assert_eq!(h1.len(), 64); // SHA-256 hex = 64 chars
}

#[test]
fn test_hash_api_key_different_salt() {
    let h1 = hash_api_key("test_key", "salt1");
    let h2 = hash_api_key("test_key", "salt2");
    assert_ne!(h1, h2);
}

#[test]
fn test_validate_token_shape_valid() {
    assert!(validate_token_shape(
        "harness_abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
    ));
}

#[test]
fn test_validate_token_shape_wrong_prefix() {
    assert!(!validate_token_shape(
        "wrong_abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
    ));
}

#[test]
fn test_validate_token_shape_wrong_length() {
    assert!(!validate_token_shape("harness_abc"));
    assert!(!validate_token_shape(
        "harness_abcdef0123456789abcdef0123456789abcdef0123456789abcdef012345678900"
    ));
}

#[test]
fn test_validate_token_shape_non_hex() {
    assert!(!validate_token_shape(
        "harness_gg0123456789abcdef0123456789abcdef0123456789abcdef0123456789zz"
    ));
}

#[test]
fn test_tenant_resolver_create_and_resolve() {
    let mut resolver = TenantResolver::new();
    resolver.add_tenant(make_tenant("t1", "Test Tenant"));
    let (key, raw) = resolver.create_api_key("t1", None, None, 1000.0).unwrap();
    assert!(raw.starts_with("harness_"));
    assert!(!key.key_id.is_empty());

    let decision = resolver.resolve_mut(Some(&format!("Bearer {raw}")), 1000.0);
    assert!(decision.allowed);
    assert_eq!(decision.tenant_id.as_deref(), Some("t1"));
    assert_eq!(decision.api_key_id.as_deref(), Some(key.key_id.as_str()));
}

#[test]
fn test_tenant_resolver_missing_header() {
    let mut resolver = TenantResolver::new();
    let decision = resolver.resolve_mut(None, 1000.0);
    assert!(!decision.allowed);
    assert!(decision.reason.contains("missing"));
}

#[test]
fn test_tenant_resolver_invalid_format() {
    let mut resolver = TenantResolver::new();
    let decision = resolver.resolve_mut(Some("Basic abc"), 1000.0);
    assert!(!decision.allowed);
    assert!(decision.reason.contains("invalid authorization format"));
}

#[test]
fn test_tenant_resolver_invalid_token_shape() {
    let mut resolver = TenantResolver::new();
    let header = ["Bearer", "malformed"].join(" ");
    let decision = resolver.resolve_mut(Some(&header), 1000.0);
    assert!(!decision.allowed);
    assert!(decision.reason.contains("invalid api key"));
}

#[test]
fn test_tenant_resolver_wrong_key() {
    let mut resolver = TenantResolver::new();
    resolver.add_tenant(make_tenant("t1", "Test"));
    let (_, _) = resolver.create_api_key("t1", None, None, 1000.0).unwrap();
    let wrong_key = "harness_ffff00000000000000000000000000000000000000000000000000000000ffff";
    let decision = resolver.resolve_mut(Some(&format!("Bearer {wrong_key}")), 1000.0);
    assert!(!decision.allowed);
}

#[test]
fn test_tenant_resolver_expired_key() {
    let mut resolver = TenantResolver::new();
    resolver.add_tenant(make_tenant("t1", "Test"));
    let (_, raw) = resolver
        .create_api_key("t1", None, Some(500.0), 1000.0)
        .unwrap();
    let decision = resolver.resolve_mut(Some(&format!("Bearer {raw}")), 600.0);
    assert!(!decision.allowed);
    assert!(decision.reason.contains("expired"));
}

#[test]
fn test_tenant_resolver_unknown_tenant() {
    let mut resolver = TenantResolver::new();
    let result = resolver.create_api_key("nonexistent", None, None, 1000.0);
    assert!(result.is_err());
}

#[test]
fn test_tenant_resolver_scope_subset() {
    let mut resolver = TenantResolver::new();
    let mut tenant = make_tenant("t1", "Test");
    let mut scopes = HashSet::new();
    scopes.insert("dispatch:read".to_string());
    scopes.insert("dispatch:write".to_string());
    tenant.scopes = scopes;
    resolver.add_tenant(tenant);

    let mut key_scopes = HashSet::new();
    key_scopes.insert("dispatch:read".to_string());
    let result = resolver.create_api_key("t1", Some(key_scopes), None, 1000.0);
    assert!(result.is_ok());
}

#[test]
fn test_tenant_resolver_scope_exceeds_tenant() {
    let mut resolver = TenantResolver::new();
    let mut tenant = make_tenant("t1", "Test");
    let mut scopes = HashSet::new();
    scopes.insert("dispatch:read".to_string());
    tenant.scopes = scopes;
    resolver.add_tenant(tenant);

    let mut key_scopes = HashSet::new();
    key_scopes.insert("dispatch:read".to_string());
    key_scopes.insert("provider:execute".to_string());
    let result = resolver.create_api_key("t1", Some(key_scopes), None, 1000.0);
    assert!(result.is_err());
}

#[test]
fn test_auth_decision_serde() {
    let decision = AuthDecision {
        allowed: true,
        tenant_id: Some("t1".to_string()),
        api_key_id: Some("k1".to_string()),
        scopes: HashSet::new(),
        reason: String::new(),
    };
    let json = serde_json::to_string(&decision).unwrap();
    let d: AuthDecision = serde_json::from_str(&json).unwrap();
    assert!(d.allowed);
    assert_eq!(d.tenant_id, Some("t1".to_string()));
}
