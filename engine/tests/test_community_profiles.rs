use engine::ecosystem::community_profiles::*;
use std::collections::HashMap;

fn make_test_profile() -> ModelProfile {
    ModelProfile {
        schema_version: COMMUNITY_PROFILE_SCHEMA_VERSION.to_string(),
        profile_id: "p1".to_string(),
        name: "GPT-4".to_string(),
        provider: "openai".to_string(),
        model_name: "gpt-4".to_string(),
        capabilities: vec!["chat".to_string(), "code".to_string()],
        cost_per_1k_tokens: 0.03,
        max_context: 8192,
        created_at: 1000.0,
        author: "test".to_string(),
        tags: vec!["general".to_string(), "coding".to_string()],
    }
}

#[test]
fn test_register_and_list() {
    let mut registry = CommunityProfileRegistry::new();
    assert!(registry.register_profile(&make_test_profile()));
    assert_eq!(registry.list_profiles().len(), 1);
}

#[test]
fn test_register_duplicate() {
    let mut registry = CommunityProfileRegistry::new();
    assert!(registry.register_profile(&make_test_profile()));
    assert!(!registry.register_profile(&make_test_profile()));
}

#[test]
fn test_unregister() {
    let mut registry = CommunityProfileRegistry::new();
    registry.register_profile(&make_test_profile());
    assert!(registry.unregister_profile("p1"));
    assert!(registry.get_profile("p1").is_none());
    assert!(!registry.unregister_profile("nonexistent"));
}

#[test]
fn test_search_by_provider() {
    let mut registry = CommunityProfileRegistry::new();
    registry.register_profile(&make_test_profile());
    let mut p2 = make_test_profile();
    p2.profile_id = "p2".to_string();
    p2.provider = "anthropic".to_string();
    registry.register_profile(&p2);
    assert_eq!(registry.search_by_provider("openai").len(), 1);
    assert_eq!(registry.search_by_provider("anthropic").len(), 1);
    assert_eq!(registry.search_by_provider("OPENAI").len(), 1); // case-insensitive
}

#[test]
fn test_search_by_tag() {
    let mut registry = CommunityProfileRegistry::new();
    registry.register_profile(&make_test_profile());
    assert_eq!(registry.search_by_tag("coding").len(), 1);
    assert_eq!(registry.search_by_tag("nonexistent").len(), 0);
}

#[test]
fn test_validate_valid() {
    let registry = CommunityProfileRegistry::new();
    let errors = registry.validate_profile(&make_test_profile());
    assert!(errors.is_empty());
}

#[test]
fn test_validate_missing_fields() {
    let registry = CommunityProfileRegistry::new();
    let mut profile = make_test_profile();
    profile.profile_id = String::new();
    let errors = registry.validate_profile(&profile);
    assert!(errors.iter().any(|e| e.contains("profile_id")));
}

#[test]
fn test_validate_negative_cost() {
    let registry = CommunityProfileRegistry::new();
    let mut profile = make_test_profile();
    profile.cost_per_1k_tokens = -1.0;
    let errors = registry.validate_profile(&profile);
    assert!(errors.iter().any(|e| e.contains("cost")));
}

#[test]
fn test_validate_zero_context() {
    let registry = CommunityProfileRegistry::new();
    let mut profile = make_test_profile();
    profile.max_context = 0;
    let errors = registry.validate_profile(&profile);
    assert!(errors.iter().any(|e| e.contains("max_context")));
}

#[test]
fn test_to_dict() {
    let profile = make_test_profile();
    let d = profile.to_dict();
    assert_eq!(d["profile_id"], "p1");
    assert_eq!(d["provider"], "openai");
}

#[test]
fn test_make_profile_defaults() {
    let profile = make_profile(HashMap::new());
    assert_eq!(profile.profile_id, "test-profile");
    assert_eq!(profile.provider, "openai");
}

#[test]
fn test_make_profile_overrides() {
    let mut overrides = HashMap::new();
    overrides.insert("profile_id".to_string(), serde_json::json!("custom-id"));
    overrides.insert("provider".to_string(), serde_json::json!("anthropic"));
    let profile = make_profile(overrides);
    assert_eq!(profile.profile_id, "custom-id");
    assert_eq!(profile.provider, "anthropic");
}
