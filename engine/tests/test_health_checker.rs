use engine::storage::durable_store::DurableStore;
use engine::storage::health_checker::HealthChecker;

#[test]
fn test_check_storage_no_store() {
    let checker = HealthChecker::new(None);
    let check = checker.check_storage(1.0);
    assert_eq!(check.status, "unhealthy");
    assert!(check.message.contains("no store"));
}

#[test]
fn test_check_storage_with_store() {
    let store = DurableStore::new_memory().unwrap();
    let checker = HealthChecker::new(Some(&store));
    let check = checker.check_storage(1.0);
    assert_eq!(check.status, "healthy");
    assert!(check.message.contains("plans="));
}

#[test]
fn test_check_events_no_store() {
    let checker = HealthChecker::new(None);
    let check = checker.check_events(1.0);
    assert_eq!(check.status, "unhealthy");
}

#[test]
fn test_check_events_with_store() {
    let store = DurableStore::new_memory().unwrap();
    let checker = HealthChecker::new(Some(&store));
    let check = checker.check_events(1.0);
    assert_eq!(check.status, "healthy");
    assert!(check.message.contains("accessible"));
}

#[test]
fn test_check_plans_no_store() {
    let checker = HealthChecker::new(None);
    let check = checker.check_plans(1.0);
    assert_eq!(check.status, "unhealthy");
}

#[test]
fn test_check_plans_with_store() {
    let store = DurableStore::new_memory().unwrap();
    let checker = HealthChecker::new(Some(&store));
    let check = checker.check_plans(1.0);
    assert_eq!(check.status, "healthy");
}

#[test]
fn test_health_all_healthy() {
    let store = DurableStore::new_memory().unwrap();
    let checker = HealthChecker::new(Some(&store));
    let report = checker.health(1.0);
    assert_eq!(report.status, "healthy");
    assert_eq!(report.checks.len(), 3);
}

#[test]
fn test_health_no_store() {
    let checker = HealthChecker::new(None);
    let report = checker.health(1.0);
    assert_eq!(report.status, "unhealthy");
}

#[test]
fn test_readiness_ready() {
    let store = DurableStore::new_memory().unwrap();
    let checker = HealthChecker::new(Some(&store));
    let report = checker.readiness(1.0);
    assert_eq!(report.status, "ready");
}

#[test]
fn test_readiness_not_ready() {
    let checker = HealthChecker::new(None);
    let report = checker.readiness(1.0);
    assert_eq!(report.status, "not_ready");
}
