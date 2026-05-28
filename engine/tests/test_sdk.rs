use engine::sdk::HarnessSDK;

#[test]
fn test_sdk_new_memory() {
    let sdk = HarnessSDK::new(None).unwrap();
    let status = sdk.get_status(1.0).unwrap();
    assert_eq!(status["schema_version"], "sdk.v1");
    assert_eq!(status["health"]["status"], "healthy");
}

#[test]
fn test_sdk_create_dispatch() {
    let sdk = HarnessSDK::new(None).unwrap();
    let result = sdk.create_dispatch("review this code for bugs", "test");
    assert!(result.get("record").is_some());
    assert!(result.get("decision").is_some());
}

#[test]
fn test_sdk_list_plans_empty() {
    let sdk = HarnessSDK::new(None).unwrap();
    let plans = sdk.list_plans().unwrap();
    assert!(plans.is_empty());
}

#[test]
fn test_sdk_get_plan_not_found() {
    let sdk = HarnessSDK::new(None).unwrap();
    let plan = sdk.get_plan("nonexistent").unwrap();
    assert!(plan.is_none());
}

#[test]
fn test_sdk_health_check() {
    let sdk = HarnessSDK::new(None).unwrap();
    let health = sdk.health_check(1.0);
    assert_eq!(health["status"], "healthy");
    let checks = health["checks"].as_array().unwrap();
    assert_eq!(checks.len(), 3);
}

#[test]
fn test_sdk_store_accessible() {
    let sdk = HarnessSDK::new(None).unwrap();
    let stats = sdk.store().stats().unwrap();
    assert_eq!(stats["plans"], 0);
}

#[test]
fn test_sdk_close() {
    let sdk = HarnessSDK::new(None).unwrap();
    sdk.close().unwrap();
}
