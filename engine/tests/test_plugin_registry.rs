use engine::infrastructure::plugin_registry::PluginRegistry;
use engine::infrastructure::plugin_system::*;

fn make_manifest(id: &str, name: &str, trust: &str) -> PluginManifest {
    PluginManifest {
        schema_version: "plugin_manifest.v1".to_string(),
        plugin_id: id.to_string(),
        name: name.to_string(),
        version: "1.0.0".to_string(),
        author: "test_author".to_string(),
        permissions: vec!["dispatch:read".to_string()],
        entrypoints: vec![],
        compatible_dispatcher_versions: vec![],
        required_env: vec![],
        network_access: false,
        filesystem_access: false,
        signature: None,
        trust_level: trust.to_string(),
    }
}

#[test]
fn test_register_and_list() {
    let mut registry = PluginRegistry::new();
    assert!(registry.register_plugin(&make_manifest("p1", "Plugin A", "community")));
    assert!(registry.register_plugin(&make_manifest("p2", "Plugin B", "community")));
    assert_eq!(registry.list_registered().len(), 2);
}

#[test]
fn test_register_duplicate() {
    let mut registry = PluginRegistry::new();
    assert!(registry.register_plugin(&make_manifest("p1", "Plugin A", "community")));
    assert!(!registry.register_plugin(&make_manifest("p1", "Plugin A2", "community")));
}

#[test]
fn test_register_invalid_manifest() {
    let mut registry = PluginRegistry::new();
    let mut manifest = make_manifest("p1", "Plugin A", "community");
    manifest.plugin_id = String::new();
    assert!(!registry.register_plugin(&manifest));
}

#[test]
fn test_unregister() {
    let mut registry = PluginRegistry::new();
    registry.register_plugin(&make_manifest("p1", "Plugin A", "community"));
    assert!(registry.unregister_plugin("p1"));
    assert!(registry.get_plugin("p1").is_none());
    assert!(!registry.unregister_plugin("nonexistent"));
}

#[test]
fn test_get_plugin() {
    let mut registry = PluginRegistry::new();
    registry.register_plugin(&make_manifest("p1", "Plugin A", "community"));
    let p = registry.get_plugin("p1").unwrap();
    assert_eq!(p.name, "Plugin A");
    assert!(registry.get_plugin("nonexistent").is_none());
}

#[test]
fn test_search_plugins_by_name() {
    let mut registry = PluginRegistry::new();
    registry.register_plugin(&make_manifest("p1", "Auth Plugin", "community"));
    registry.register_plugin(&make_manifest("p2", "Dashboard Plugin", "community"));
    registry.register_plugin(&make_manifest("p3", "Auth Helper", "community"));
    // "auth" matches "Auth Plugin", "Auth Helper", and "test_author" (author field)
    let results = registry.search_plugins("auth");
    assert_eq!(results.len(), 3);
}

#[test]
fn test_search_plugins_by_author() {
    let mut registry = PluginRegistry::new();
    registry.register_plugin(&make_manifest("p1", "Plugin A", "community"));
    let results = registry.search_plugins("test_author");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_search_no_match() {
    let mut registry = PluginRegistry::new();
    registry.register_plugin(&make_manifest("p1", "Plugin A", "community"));
    let results = registry.search_plugins("nonexistent");
    assert!(results.is_empty());
}

#[test]
fn test_validate_manifest_valid() {
    let registry = PluginRegistry::new();
    let manifest = make_manifest("p1", "Plugin A", "community");
    let errors = registry.validate_manifest(&manifest);
    assert!(errors.is_empty());
}

#[test]
fn test_validate_manifest_invalid_trust() {
    let registry = PluginRegistry::new();
    let mut manifest = make_manifest("p1", "Plugin A", "community");
    manifest.trust_level = "bad".to_string();
    let errors = registry.validate_manifest(&manifest);
    assert!(!errors.is_empty());
}

#[test]
fn test_validate_manifest_unknown_permission() {
    let registry = PluginRegistry::new();
    let mut manifest = make_manifest("p1", "Plugin A", "community");
    manifest.permissions = vec!["unknown:perm".to_string()];
    let errors = registry.validate_manifest(&manifest);
    assert!(errors.iter().any(|e| e.contains("unknown permission")));
}

#[test]
fn test_parse_manifest_from_json() {
    let json = serde_json::json!({
        "plugin_id": "test-plugin",
        "name": "Test Plugin",
        "version": "2.0.0",
        "author": "tester",
        "permissions": ["dispatch:read", "dispatch:write"],
        "entrypoints": ["main.py"],
        "compatible_dispatcher_versions": ["1.0"],
        "required_env": ["API_KEY"],
        "network_access": true,
        "filesystem_access": false,
        "trust_level": "verified",
        "schema_version": "plugin_manifest.v1"
    });
    let manifest = parse_manifest(&json).unwrap();
    assert_eq!(manifest.plugin_id, "test-plugin");
    assert_eq!(manifest.name, "Test Plugin");
    assert_eq!(manifest.version, "2.0.0");
    assert!(manifest.network_access);
    assert!(!manifest.filesystem_access);
    assert_eq!(
        manifest.permissions,
        vec!["dispatch:read", "dispatch:write"]
    );
    assert_eq!(manifest.required_env, vec!["API_KEY"]);
}
