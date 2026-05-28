use engine::infrastructure::plugin_system::*;

fn make_manifest(id: &str, trust: &str, perms: Vec<String>) -> PluginManifest {
    PluginManifest {
        schema_version: "plugin_manifest.v1".to_string(),
        plugin_id: id.to_string(),
        name: format!("Plugin {id}"),
        version: "1.0.0".to_string(),
        author: "test".to_string(),
        permissions: perms,
        entrypoints: vec!["main".to_string()],
        compatible_dispatcher_versions: vec!["1.0".to_string()],
        required_env: vec![],
        network_access: false,
        filesystem_access: false,
        signature: None,
        trust_level: trust.to_string(),
    }
}

#[test]
fn test_load_community_plugin() {
    let mut system = PluginSystem::new();
    let manifest = make_manifest("p1", "community", vec!["dispatch:read".to_string()]);
    let loaded = system.load_plugin_from_manifest(manifest, 1000.0).unwrap();
    assert_eq!(loaded.status, "loaded");
    assert_eq!(loaded.manifest.plugin_id, "p1");
}

#[test]
fn test_load_verified_plugin() {
    let mut system = PluginSystem::new();
    let manifest = make_manifest(
        "p1",
        "verified",
        vec!["dispatch:read".to_string(), "dispatch:write".to_string()],
    );
    let loaded = system.load_plugin_from_manifest(manifest, 1000.0).unwrap();
    assert_eq!(loaded.manifest.trust_level, "verified");
}

#[test]
fn test_load_official_plugin_unrestricted() {
    let mut system = PluginSystem::new();
    let manifest = make_manifest(
        "p1",
        "official",
        vec![
            "dispatch:read".to_string(),
            "dispatch:write".to_string(),
            "provider:execute".to_string(),
        ],
    );
    let loaded = system.load_plugin_from_manifest(manifest, 1000.0).unwrap();
    assert_eq!(loaded.manifest.trust_level, "official");
}

#[test]
fn test_community_plugin_denied_write() {
    let mut system = PluginSystem::new();
    let manifest = make_manifest(
        "p1",
        "community",
        vec!["dispatch:read".to_string(), "dispatch:write".to_string()],
    );
    let result = system.load_plugin_from_manifest(manifest, 1000.0);
    assert!(result.is_err());
}

#[test]
fn test_unload_plugin() {
    let mut system = PluginSystem::new();
    let manifest = make_manifest("p1", "community", vec!["dispatch:read".to_string()]);
    system.load_plugin_from_manifest(manifest, 1000.0).unwrap();
    assert!(system.unload_plugin("p1"));
    assert!(system.get_plugin("p1").is_none());
    assert!(!system.unload_plugin("nonexistent"));
}

#[test]
fn test_check_permission() {
    let mut system = PluginSystem::new();
    let manifest = make_manifest("p1", "community", vec!["dispatch:read".to_string()]);
    system.load_plugin_from_manifest(manifest, 1000.0).unwrap();
    assert!(system.check_permission("p1", "dispatch:read"));
    assert!(!system.check_permission("p1", "dispatch:write"));
    assert!(!system.check_permission("nonexistent", "dispatch:read"));
    assert!(!system.check_permission("p1", "unknown:perm"));
}

#[test]
fn test_list_plugins() {
    let mut system = PluginSystem::new();
    system
        .load_plugin_from_manifest(
            make_manifest("p1", "community", vec!["dispatch:read".to_string()]),
            1000.0,
        )
        .unwrap();
    system
        .load_plugin_from_manifest(
            make_manifest("p2", "community", vec!["dispatch:read".to_string()]),
            1000.0,
        )
        .unwrap();
    assert_eq!(system.list_plugins().len(), 2);
}

#[test]
fn test_validate_manifest_fields_valid() {
    let manifest = make_manifest("p1", "community", vec!["dispatch:read".to_string()]);
    let errors = validate_manifest_fields(&manifest);
    assert!(errors.is_empty());
}

#[test]
fn test_validate_manifest_fields_missing_id() {
    let mut manifest = make_manifest("p1", "community", vec!["dispatch:read".to_string()]);
    manifest.plugin_id = String::new();
    let errors = validate_manifest_fields(&manifest);
    assert!(!errors.is_empty());
    assert!(errors.iter().any(|e| e.contains("plugin_id")));
}

#[test]
fn test_validate_manifest_fields_invalid_trust() {
    let mut manifest = make_manifest("p1", "community", vec!["dispatch:read".to_string()]);
    manifest.trust_level = "invalid".to_string();
    let errors = validate_manifest_fields(&manifest);
    assert!(errors.iter().any(|e| e.contains("trust_level")));
}

#[test]
fn test_validate_manifest_fields_unknown_permission() {
    let manifest = make_manifest("p1", "community", vec!["unknown:perm".to_string()]);
    let errors = validate_manifest_fields(&manifest);
    assert!(errors.iter().any(|e| e.contains("unknown permission")));
}

#[test]
fn test_trust_permission_allowed_official() {
    assert!(trust_permission_allowed(
        "official",
        &["dispatch:read".to_string(), "provider:execute".to_string()]
    ));
}

#[test]
fn test_trust_permission_allowed_community() {
    assert!(trust_permission_allowed(
        "community",
        &["dispatch:read".to_string()]
    ));
    assert!(!trust_permission_allowed(
        "community",
        &["dispatch:write".to_string()]
    ));
}

#[test]
fn test_trust_permission_allowed_verified() {
    assert!(trust_permission_allowed(
        "verified",
        &["dispatch:read".to_string(), "dispatch:write".to_string()]
    ));
    assert!(!trust_permission_allowed(
        "verified",
        &["provider:execute".to_string()]
    ));
}

#[test]
fn test_plugin_manifest_to_dict() {
    let manifest = make_manifest("p1", "community", vec!["dispatch:read".to_string()]);
    let d = manifest.to_dict();
    assert_eq!(d["plugin_id"], "p1");
    assert_eq!(d["trust_level"], "community");
}

#[test]
fn test_constants() {
    assert!(VALID_TRUST_LEVELS.contains(&"community"));
    assert!(VALID_TRUST_LEVELS.contains(&"verified"));
    assert!(VALID_TRUST_LEVELS.contains(&"official"));
    assert!(ALL_KNOWN_PERMISSIONS.contains(&"dispatch:read"));
    assert!(ALL_KNOWN_PERMISSIONS.contains(&"provider:execute"));
}
