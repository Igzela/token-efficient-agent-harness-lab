//! Reference-only module: event.v1 schema validation and idempotency hashing.
//!
//! Retained for parity with wire_contract/v1 event schemas and golden fixtures.
//! The active runtime uses `LocalProductStore` audit events instead.

use std::collections::HashSet;
use std::sync::LazyLock;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub static REQUIRED_FIELDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut s = HashSet::with_capacity(10);
    s.insert("event_id");
    s.insert("schema_version");
    s.insert("event_type");
    s.insert("timestamp");
    s.insert("producer");
    s.insert("correlation");
    s.insert("severity");
    s.insert("payload");
    s.insert("idempotency_key");
    s.insert("parent_event_id");
    s
});

pub static REQUIRED_PRODUCER_FIELDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut s = HashSet::with_capacity(2);
    s.insert("component_id");
    s.insert("component_type");
    s
});

pub static VALID_SEVERITIES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut s = HashSet::with_capacity(3);
    s.insert("info");
    s.insert("warn");
    s.insert("error");
    s
});

pub static IDEMPOTENCY_HASH_EXCLUDED_FIELDS: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| {
        let mut s = HashSet::with_capacity(2);
        s.insert("event_id");
        s.insert("timestamp");
        s
    });

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SchemaViolationError(pub String);

impl std::fmt::Display for SchemaViolationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for SchemaViolationError {}

// ---------------------------------------------------------------------------
// Canonical JSON (sorted keys, compact separators — mirrors Python json.dumps)
// ---------------------------------------------------------------------------

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut pairs: Vec<_> = map.iter().collect();
            pairs.sort_by_key(|(k, _)| k.as_str());
            let inner: String = pairs
                .iter()
                .map(|(k, v)| format!("\"{}\":{}", k, canonical_json(v)))
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{}}}", inner)
        }
        Value::Array(arr) => {
            let inner: String = arr.iter().map(canonical_json).collect::<Vec<_>>().join(",");
            format!("[{}]", inner)
        }
        Value::String(s) => serde_json::to_string(s).unwrap(),
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn validate_event(event: &Value) -> Result<(), SchemaViolationError> {
    let object = event
        .as_object()
        .ok_or_else(|| SchemaViolationError("event must be a JSON object".to_string()))?;

    // Check all required fields are present
    let missing: Vec<&str> = REQUIRED_FIELDS
        .iter()
        .copied()
        .filter(|field| !object.contains_key(*field))
        .collect();
    if !missing.is_empty() {
        let mut sorted = missing;
        sorted.sort();
        return Err(SchemaViolationError(format!(
            "missing required field(s): {}",
            sorted.join(", ")
        )));
    }

    // Non-empty string checks
    require_non_empty_string(object, "event_id")?;
    require_non_empty_string(object, "event_type")?;
    require_non_empty_string(object, "timestamp")?;
    require_non_empty_string(object, "idempotency_key")?;

    // schema_version must be exactly "event.v1"
    if object.get("schema_version").and_then(Value::as_str) != Some("event.v1") {
        return Err(SchemaViolationError(
            "schema_version must be event.v1".to_string(),
        ));
    }

    // severity must be one of valid values
    match object.get("severity").and_then(Value::as_str) {
        Some(s) if VALID_SEVERITIES.contains(s) => {}
        _ => {
            return Err(SchemaViolationError(
                "severity must be one of: error, info, warn".to_string(),
            ))
        }
    }

    // producer must be an object with required sub-fields
    let producer = object
        .get("producer")
        .and_then(Value::as_object)
        .ok_or_else(|| SchemaViolationError("producer must be an object".to_string()))?;
    let missing_producer: Vec<&str> = REQUIRED_PRODUCER_FIELDS
        .iter()
        .copied()
        .filter(|field| !producer.contains_key(*field))
        .collect();
    if !missing_producer.is_empty() {
        let mut sorted = missing_producer;
        sorted.sort();
        return Err(SchemaViolationError(format!(
            "producer missing required field(s): {}",
            sorted.join(", ")
        )));
    }
    require_non_empty_string(producer, "component_id")?;
    require_non_empty_string(producer, "component_type")?;

    // correlation must be an object
    if !object.get("correlation").is_some_and(Value::is_object) {
        return Err(SchemaViolationError(
            "correlation must be an object".to_string(),
        ));
    }

    // payload must be an object
    if !object.get("payload").is_some_and(Value::is_object) {
        return Err(SchemaViolationError(
            "payload must be an object".to_string(),
        ));
    }

    // parent_event_id must be null or string
    match object.get("parent_event_id") {
        Some(Value::Null) | Some(Value::String(_)) => {}
        _ => {
            return Err(SchemaViolationError(
                "parent_event_id must be a string or null".to_string(),
            ))
        }
    }

    Ok(())
}

pub fn canonical_event_json(event: &Value) -> Result<String, SchemaViolationError> {
    Ok(canonical_json(event))
}

pub fn stable_idempotency_hash(event: &Value) -> Result<String, SchemaViolationError> {
    let object = event
        .as_object()
        .ok_or_else(|| SchemaViolationError("event must be a JSON object".to_string()))?;

    let mut filtered = Map::with_capacity(object.len());
    for (key, value) in object {
        if !IDEMPOTENCY_HASH_EXCLUDED_FIELDS.contains(key.as_str()) {
            filtered.insert(key.clone(), value.clone());
        }
    }

    let canonical = canonical_json(&Value::Object(filtered));
    let digest = Sha256::digest(canonical.as_bytes());
    Ok(to_hex(&digest))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn require_non_empty_string(
    object: &Map<String, Value>,
    key: &str,
) -> Result<(), SchemaViolationError> {
    match object.get(key).and_then(Value::as_str) {
        Some(s) if !s.is_empty() => Ok(()),
        _ => Err(SchemaViolationError(format!(
            "{} must be a non-empty string",
            key
        ))),
    }
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
