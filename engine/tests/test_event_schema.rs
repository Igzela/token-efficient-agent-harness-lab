use serde_json::{json, Value};

use engine::event_schema::{canonical_event_json, stable_idempotency_hash, validate_event};

fn valid_event() -> Value {
    json!({
        "event_id": "evt-001",
        "schema_version": "event.v1",
        "event_type": "project_item_state_changed",
        "timestamp": "2026-01-01T00:00:00Z",
        "producer": {"component_id": "test", "component_type": "unit_test"},
        "correlation": {},
        "severity": "info",
        "payload": {"item_id": "item_1"},
        "idempotency_key": "idem-001",
        "parent_event_id": null
    })
}

// ---------------------------------------------------------------------------
// ValidateEventTests
// ---------------------------------------------------------------------------

#[test]
fn test_valid_event_passes() {
    assert!(validate_event(&valid_event()).is_ok());
}

#[test]
fn test_non_dict_raises() {
    assert!(validate_event(&json!("not a dict")).is_err());
}

#[test]
fn test_missing_required_field_raises() {
    let mut e = valid_event();
    e.as_object_mut().unwrap().remove("event_id");
    assert!(validate_event(&e).is_err());
}

#[test]
fn test_wrong_schema_version_raises() {
    let mut e = valid_event();
    e["schema_version"] = json!("event.v2");
    assert!(validate_event(&e).is_err());
}

#[test]
fn test_invalid_severity_raises() {
    let mut e = valid_event();
    e["severity"] = json!("critical");
    assert!(validate_event(&e).is_err());
}

#[test]
fn test_non_dict_producer_raises() {
    let mut e = valid_event();
    e["producer"] = json!("not a dict");
    assert!(validate_event(&e).is_err());
}

#[test]
fn test_missing_producer_field_raises() {
    let mut e = valid_event();
    e["producer"]
        .as_object_mut()
        .unwrap()
        .remove("component_type");
    assert!(validate_event(&e).is_err());
}

#[test]
fn test_empty_string_field_raises() {
    let mut e = valid_event();
    e["event_id"] = json!("");
    assert!(validate_event(&e).is_err());
}

#[test]
fn test_non_dict_correlation_raises() {
    let mut e = valid_event();
    e["correlation"] = json!("not a dict");
    assert!(validate_event(&e).is_err());
}

#[test]
fn test_non_dict_payload_raises() {
    let mut e = valid_event();
    e["payload"] = json!("not a dict");
    assert!(validate_event(&e).is_err());
}

#[test]
fn test_parent_event_id_none_ok() {
    let e = valid_event(); // parent_event_id is null
    assert!(validate_event(&e).is_ok());
}

#[test]
fn test_parent_event_id_string_ok() {
    let mut e = valid_event();
    e["parent_event_id"] = json!("evt-000");
    assert!(validate_event(&e).is_ok());
}

#[test]
fn test_parent_event_id_invalid_type_raises() {
    let mut e = valid_event();
    e["parent_event_id"] = json!(123);
    assert!(validate_event(&e).is_err());
}

// ---------------------------------------------------------------------------
// StableIdempotencyHashTests
// ---------------------------------------------------------------------------

#[test]
fn test_same_event_same_hash() {
    let e = valid_event();
    let h1 = stable_idempotency_hash(&e).unwrap();
    let h2 = stable_idempotency_hash(&e).unwrap();
    assert_eq!(h1, h2);
}

#[test]
fn test_different_payload_different_hash() {
    let e1 = valid_event();
    let mut e2 = valid_event();
    e2["payload"] = json!({"item_id": "item_2"});
    assert_ne!(
        stable_idempotency_hash(&e1).unwrap(),
        stable_idempotency_hash(&e2).unwrap()
    );
}

#[test]
fn test_excludes_event_id_and_timestamp() {
    let e1 = valid_event();
    let mut e2 = valid_event();
    e2["event_id"] = json!("evt-999");
    e2["timestamp"] = json!("2099-12-31T23:59:59Z");
    assert_eq!(
        stable_idempotency_hash(&e1).unwrap(),
        stable_idempotency_hash(&e2).unwrap()
    );
}

// ---------------------------------------------------------------------------
// CanonicalEventJsonTests
// ---------------------------------------------------------------------------

#[test]
fn test_deterministic_output() {
    let e = valid_event();
    let j1 = canonical_event_json(&e).unwrap();
    let j2 = canonical_event_json(&e).unwrap();
    assert_eq!(j1, j2);
}

#[test]
fn test_is_valid_json() {
    let e = valid_event();
    let j = canonical_event_json(&e).unwrap();
    let parsed: Value = serde_json::from_str(&j).unwrap();
    assert_eq!(parsed["event_id"], "evt-001");
}
