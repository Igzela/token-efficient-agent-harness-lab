use std::collections::HashMap;

use engine::routing::history_store::RoutingHistoryStore;
use engine::routing::schemas::*;

fn make_row(
    id: &str,
    profile_id: &str,
    group: &str,
    cost: f64,
    quality: f64,
    success: bool,
) -> UsageLedgerRow {
    UsageLedgerRow {
        row_id: id.to_string(),
        dispatch_id: format!("disp-{id}"),
        model_profile_id: profile_id.to_string(),
        cost_of_pass_group: group.to_string(),
        input_tokens: 100,
        output_tokens: 50,
        estimated_cost: cost,
        quality_score: quality,
        success,
        failure_domain: None,
        latency_ms: 50,
    }
}

fn make_store() -> RoutingHistoryStore {
    let mut tier_map = HashMap::new();
    tier_map.insert("profile-cheap".to_string(), "cheap_executor".to_string());
    tier_map.insert(
        "profile-balanced".to_string(),
        "balanced_worker".to_string(),
    );
    let mut store = RoutingHistoryStore::new(Some(tier_map));
    store.add_row(make_row(
        "r1",
        "profile-cheap",
        "scope/code/review/accuracy",
        0.01,
        0.8,
        true,
    ));
    store.add_row(make_row(
        "r2",
        "profile-cheap",
        "scope/code/review/accuracy",
        0.02,
        0.6,
        false,
    ));
    store.add_row(make_row(
        "r3",
        "profile-balanced",
        "scope/code/review/accuracy",
        0.03,
        0.9,
        true,
    ));
    store.add_row(make_row(
        "r4",
        "profile-cheap",
        "scope/docs/generate/completeness",
        0.005,
        0.7,
        true,
    ));
    store
}

#[test]
fn test_rows_by_tier() {
    let mut store = make_store();
    let cheap_rows = store.rows_by_tier("cheap_executor");
    assert_eq!(cheap_rows.len(), 3);
    let balanced_rows = store.rows_by_tier("balanced_worker");
    assert_eq!(balanced_rows.len(), 1);
}

#[test]
fn test_rows_by_task_group() {
    let mut store = make_store();
    let review_rows = store.rows_by_task_group("code/review");
    assert_eq!(review_rows.len(), 3);
    let generate_rows = store.rows_by_task_group("docs/generate");
    assert_eq!(generate_rows.len(), 1);
}

#[test]
fn test_rows_by_tier_and_task_group() {
    let mut store = make_store();
    let rows = store.rows_by_tier_and_task_group("cheap_executor", "code/review");
    assert_eq!(rows.len(), 2);
}

#[test]
fn test_tiers_observed() {
    let mut store = make_store();
    let tiers = store.tiers_observed("code/review");
    assert_eq!(tiers.len(), 2);
    assert!(tiers.contains(&"cheap_executor".to_string()));
    assert!(tiers.contains(&"balanced_worker".to_string()));
}

#[test]
fn test_sample_count() {
    let mut store = make_store();
    assert_eq!(store.sample_count("code/review"), 3);
    assert_eq!(store.sample_count("docs/generate"), 1);
    assert_eq!(store.sample_count("nonexistent"), 0);
}

#[test]
fn test_aggregate_by_tier_and_task_group() {
    let mut store = make_store();
    let agg = store
        .aggregate_by_tier_and_task_group("cheap_executor", "code/review")
        .unwrap();
    assert_eq!(agg.total_count, 2);
    assert_eq!(agg.failure_count, 1);
    assert!((agg.total_cost - 0.03).abs() < 0.0001);
}

#[test]
fn test_total_rows() {
    let store = make_store();
    assert_eq!(store.total_rows(), 4);
}

#[test]
fn test_empty_store() {
    let mut store = RoutingHistoryStore::new(None);
    assert_eq!(store.total_rows(), 0);
    assert!(store.tiers_observed("code/review").is_empty());
    assert!(store.aggregate_by_tier("cheap_executor").is_none());
}
