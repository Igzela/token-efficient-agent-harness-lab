use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::errors::{EventStoreError, ReplayPreflightError};
use crate::event_schema::{stable_idempotency_hash, validate_event};

// ---------------------------------------------------------------------------
// Data structs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ValidationIssue {
    pub line_number: Option<u64>,
    pub error_type: String,
    pub message: String,
}

impl Default for ValidationIssue {
    fn default() -> Self {
        Self {
            line_number: None,
            error_type: String::new(),
            message: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ValidationReport {
    pub path: PathBuf,
    #[serde(default)]
    pub errors: Vec<ValidationIssue>,
    #[serde(default)]
    pub warnings: Vec<ValidationIssue>,
}

impl Default for ValidationReport {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

impl ValidationReport {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ReplayPreflightReport {
    pub path: PathBuf,
    #[serde(default)]
    pub errors: Vec<ValidationIssue>,
    #[serde(default)]
    pub warnings: Vec<ValidationIssue>,
    #[serde(default)]
    pub event_count: usize,
    #[serde(default)]
    pub event_ids: HashSet<String>,
}

impl Default for ReplayPreflightReport {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
            event_count: 0,
            event_ids: HashSet::new(),
        }
    }
}

impl ReplayPreflightReport {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

// ---------------------------------------------------------------------------
// EventStore — in-memory append-only store
// ---------------------------------------------------------------------------

pub struct EventStore {
    pub events: Vec<Value>,
}

impl Default for EventStore {
    fn default() -> Self {
        Self { events: Vec::new() }
    }
}

impl EventStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn append_event(&mut self, event: Value) -> Result<(), EventStoreError> {
        validate_event(&event)
            .map_err(|e| EventStoreError::new("SchemaViolation", &e.to_string()))?;

        let event_hash = stable_idempotency_hash(&event)
            .map_err(|e| EventStoreError::new("HashError", &e.to_string()))?;

        let idempotency_key = event
            .get("idempotency_key")
            .and_then(Value::as_str)
            .ok_or_else(|| EventStoreError::new("MissingField", "idempotency_key"))?;

        for existing in &self.events {
            let existing_key = existing
                .get("idempotency_key")
                .and_then(Value::as_str)
                .unwrap_or("");
            if existing_key == idempotency_key {
                let existing_hash = stable_idempotency_hash(existing)
                    .map_err(|e| EventStoreError::new("HashError", &e.to_string()))?;
                if existing_hash == event_hash {
                    return Ok(());
                }
                return Err(EventStoreError::new(
                    "DuplicateIdempotencyConflict",
                    &format!(
                        "idempotency_key already exists with different semantic hash: {}",
                        idempotency_key
                    ),
                ));
            }
        }

        let event_id = event
            .get("event_id")
            .and_then(Value::as_str)
            .ok_or_else(|| EventStoreError::new("MissingField", "event_id"))?;

        for existing in &self.events {
            let existing_id = existing
                .get("event_id")
                .and_then(Value::as_str)
                .unwrap_or("");
            if existing_id == event_id {
                return Err(EventStoreError::new(
                    "DuplicateEventId",
                    &format!("duplicate event_id: {}", event_id),
                ));
            }
        }

        self.events.push(event);
        Ok(())
    }

    pub fn get_event(&self, index: usize) -> Option<&Value> {
        self.events.get(index)
    }

    pub fn list_events(&self) -> &[Value] {
        &self.events
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn event_ids(&self) -> HashSet<String> {
        self.events
            .iter()
            .filter_map(|e| e.get("event_id").and_then(Value::as_str).map(String::from))
            .collect()
    }

    pub fn validate(&self) -> ValidationReport {
        let mut report = ValidationReport::default();
        for (i, event) in self.events.iter().enumerate() {
            let line_number = (i + 1) as u64;
            if let Err(e) = validate_event(event) {
                report.errors.push(ValidationIssue {
                    line_number: Some(line_number),
                    error_type: "SchemaViolation".to_string(),
                    message: e.to_string(),
                });
            }
        }
        report
    }
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

/// Validate JSONL content from a byte slice, one JSON object per newline-terminated line.
pub fn validate_jsonl_bytes(data: &[u8]) -> ValidationReport {
    let mut report = ValidationReport::default();
    validate_jsonl_into_report(data, &mut report);
    report
}

/// Replay preflight checks on JSONL content from a byte slice.
pub fn replay_preflight_bytes(data: &[u8]) -> ReplayPreflightReport {
    let mut report = ReplayPreflightReport::default();
    let events = validate_jsonl_into_report_preflight(data, &mut report);

    let mut seen_event_ids: HashMap<String, u64> = HashMap::new();
    let available_event_ids: HashSet<String> = events
        .iter()
        .filter_map(|(_, e)| e.get("event_id").and_then(Value::as_str).map(String::from))
        .collect();

    for (line_number, event) in &events {
        let event_id = match event.get("event_id").and_then(Value::as_str) {
            Some(id) => id.to_string(),
            None => continue,
        };
        if let Some(first_line) = seen_event_ids.get(&event_id) {
            report.errors.push(ValidationIssue {
                line_number: Some(*line_number),
                error_type: "DuplicateEventId".to_string(),
                message: format!(
                    "duplicate event_id {}; first seen on line {}",
                    event_id, first_line
                ),
            });
        } else {
            seen_event_ids.insert(event_id.clone(), *line_number);
        }

        if let Some(parent_id) = event.get("parent_event_id").and_then(Value::as_str) {
            if !parent_id.is_empty() && !available_event_ids.contains(parent_id) {
                report.warnings.push(ValidationIssue {
                    line_number: Some(*line_number),
                    error_type: "MissingParentEventWarning".to_string(),
                    message: format!("parent_event_id does not exist in stream: {}", parent_id),
                });
            }
        }
    }

    report.event_count = events.len();
    report.event_ids = seen_event_ids.into_keys().collect();
    report
}

/// Load event IDs from valid JSONL content.
pub fn load_event_ids_bytes(data: &[u8]) -> Result<HashSet<String>, ReplayPreflightError> {
    let report = replay_preflight_bytes(data);
    if !report.ok() {
        return Err(ReplayPreflightError::new(
            "PreflightFailed",
            "cannot load event IDs from invalid event stream",
        ));
    }
    Ok(report.event_ids)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn validate_jsonl_into_report(data: &[u8], report: &mut ValidationReport) -> Vec<(u64, Value)> {
    let mut events = Vec::new();
    for (idx, raw_line) in data.split(|&b| b == b'\n').enumerate() {
        let line_number = (idx + 1) as u64;
        let trimmed = if raw_line.ends_with(b"\r") {
            &raw_line[..raw_line.len() - 1]
        } else {
            raw_line
        };

        if trimmed.is_empty() {
            continue;
        }

        match serde_json::from_slice::<Value>(trimmed) {
            Ok(value) => {
                if !value.is_object() {
                    report.errors.push(ValidationIssue {
                        line_number: Some(line_number),
                        error_type: "InvalidJsonLine".to_string(),
                        message: "line must contain a JSON object".to_string(),
                    });
                    continue;
                }
                if let Err(e) = validate_event(&value) {
                    report.errors.push(ValidationIssue {
                        line_number: Some(line_number),
                        error_type: "SchemaViolation".to_string(),
                        message: e.to_string(),
                    });
                    continue;
                }
                events.push((line_number, value));
            }
            Err(e) => {
                report.errors.push(ValidationIssue {
                    line_number: Some(line_number),
                    error_type: "InvalidJsonLine".to_string(),
                    message: e.to_string(),
                });
            }
        }
    }
    events
}

fn validate_jsonl_into_report_preflight(
    data: &[u8],
    report: &mut ReplayPreflightReport,
) -> Vec<(u64, Value)> {
    let mut events = Vec::new();
    for (idx, raw_line) in data.split(|&b| b == b'\n').enumerate() {
        let line_number = (idx + 1) as u64;
        let trimmed = if raw_line.ends_with(b"\r") {
            &raw_line[..raw_line.len() - 1]
        } else {
            raw_line
        };

        if trimmed.is_empty() {
            continue;
        }

        match serde_json::from_slice::<Value>(trimmed) {
            Ok(value) => {
                if !value.is_object() {
                    report.errors.push(ValidationIssue {
                        line_number: Some(line_number),
                        error_type: "InvalidJsonLine".to_string(),
                        message: "line must contain a JSON object".to_string(),
                    });
                    continue;
                }
                if let Err(e) = validate_event(&value) {
                    report.errors.push(ValidationIssue {
                        line_number: Some(line_number),
                        error_type: "SchemaViolation".to_string(),
                        message: e.to_string(),
                    });
                    continue;
                }
                events.push((line_number, value));
            }
            Err(e) => {
                report.errors.push(ValidationIssue {
                    line_number: Some(line_number),
                    error_type: "InvalidJsonLine".to_string(),
                    message: e.to_string(),
                });
            }
        }
    }
    events
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_valid_event(event_id: &str, idempotency_key: &str) -> Value {
        json!({
            "event_id": event_id,
            "schema_version": "event.v1",
            "event_type": "test_event",
            "timestamp": "2026-01-01T00:00:00Z",
            "producer": {"component_id": "test", "component_type": "unit"},
            "correlation": {},
            "severity": "info",
            "payload": {},
            "idempotency_key": idempotency_key,
            "parent_event_id": null
        })
    }

    #[test]
    fn test_event_store_append_and_list() {
        let mut store = EventStore::new();
        let ev = make_valid_event("evt-1", "idem-1");
        store.append_event(ev.clone()).unwrap();
        assert_eq!(store.len(), 1);
        assert_eq!(
            store.get_event(0).unwrap().get("event_id").unwrap(),
            "evt-1"
        );
    }

    #[test]
    fn test_event_store_duplicate_event_id() {
        let mut store = EventStore::new();
        store
            .append_event(make_valid_event("evt-1", "idem-1"))
            .unwrap();
        let result = store.append_event(make_valid_event("evt-1", "idem-2"));
        assert!(result.is_err());
    }

    #[test]
    fn test_event_store_duplicate_idempotency_same_hash_is_noop() {
        let mut store = EventStore::new();
        let ev = make_valid_event("evt-1", "idem-1");
        store.append_event(ev.clone()).unwrap();
        store.append_event(ev).unwrap();
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn test_event_store_validate_clean() {
        let mut store = EventStore::new();
        store
            .append_event(make_valid_event("evt-1", "idem-1"))
            .unwrap();
        let report = store.validate();
        assert!(report.ok());
    }

    #[test]
    fn test_event_store_empty() {
        let store = EventStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
        assert!(store.event_ids().is_empty());
    }

    #[test]
    fn test_validate_jsonl_bytes_valid() {
        let ev = make_valid_event("evt-1", "idem-1");
        let line = format!("{}\n", serde_json::to_string(&ev).unwrap());
        let report = validate_jsonl_bytes(line.as_bytes());
        assert!(report.ok());
    }

    #[test]
    fn test_validate_jsonl_bytes_invalid_json() {
        let report = validate_jsonl_bytes(b"not json\n");
        assert!(!report.ok());
        assert_eq!(report.errors[0].error_type, "InvalidJsonLine");
    }

    #[test]
    fn test_replay_preflight_bytes_duplicate_event_id() {
        let ev = make_valid_event("evt-1", "idem-1");
        let line = format!("{}\n", serde_json::to_string(&ev).unwrap());
        let data = format!("{line}{line}");
        let report = replay_preflight_bytes(data.as_bytes());
        assert!(!report.ok());
        assert!(report
            .errors
            .iter()
            .any(|e| e.error_type == "DuplicateEventId"));
    }

    #[test]
    fn test_replay_preflight_bytes_missing_parent() {
        let mut ev = make_valid_event("evt-1", "idem-1");
        ev["parent_event_id"] = json!("nonexistent-parent");
        let line = format!("{}\n", serde_json::to_string(&ev).unwrap());
        let report = replay_preflight_bytes(line.as_bytes());
        assert!(report.ok());
        assert!(report
            .warnings
            .iter()
            .any(|w| w.error_type == "MissingParentEventWarning"));
    }

    #[test]
    fn test_load_event_ids_bytes() {
        let ev = make_valid_event("evt-1", "idem-1");
        let line = format!("{}\n", serde_json::to_string(&ev).unwrap());
        let ids = load_event_ids_bytes(line.as_bytes()).unwrap();
        assert!(ids.contains("evt-1"));
    }

    #[test]
    fn test_event_store_event_ids() {
        let mut store = EventStore::new();
        store
            .append_event(make_valid_event("evt-1", "idem-1"))
            .unwrap();
        store
            .append_event(make_valid_event("evt-2", "idem-2"))
            .unwrap();
        let ids = store.event_ids();
        assert!(ids.contains("evt-1"));
        assert!(ids.contains("evt-2"));
    }
}
