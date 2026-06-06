//! Reference-only module: event store and schema error types.
//!
//! Retained for parity with the wire contract error schemas.
//! Not used by the active runtime (which uses `LocalProductStore` errors).

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EventStoreError {
    pub schema_version: String,
    pub error_type: String,
    pub message: String,
    pub event_id: Option<String>,
    pub line_number: Option<u64>,
}

impl EventStoreError {
    pub fn new(error_type: &str, message: &str) -> Self {
        Self {
            schema_version: "event_store_error.v1".to_string(),
            error_type: error_type.to_string(),
            message: message.to_string(),
            event_id: None,
            line_number: None,
        }
    }
    pub fn with_event_id(mut self, event_id: &str) -> Self {
        self.event_id = Some(event_id.to_string());
        self
    }
    pub fn with_line_number(mut self, line: u64) -> Self {
        self.line_number = Some(line);
        self
    }
}

impl std::fmt::Display for EventStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.error_type, self.message)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MissingNewlineError {
    pub line_number: u64,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct InvalidJsonLineError {
    pub line_number: u64,
    pub raw_line: String,
    pub parse_error: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DuplicateEventIdError {
    pub event_id: String,
    pub first_line: u64,
    pub duplicate_line: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DuplicateIdempotencyConflictError {
    pub idempotency_key: String,
    pub first_event_id: String,
    pub conflict_event_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SchemaViolationError {
    pub schema_version: String,
    pub violations: Vec<String>,
}

impl SchemaViolationError {
    pub fn new(violations: Vec<String>) -> Self {
        Self {
            schema_version: "schema_violation.v1".to_string(),
            violations,
        }
    }
}

impl std::fmt::Display for SchemaViolationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Schema violation: {}", self.violations.join(", "))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ReplayPreflightError {
    pub schema_version: String,
    pub error_type: String,
    pub message: String,
}

impl ReplayPreflightError {
    pub fn new(error_type: &str, message: &str) -> Self {
        Self {
            schema_version: "replay_preflight_error.v1".to_string(),
            error_type: error_type.to_string(),
            message: message.to_string(),
        }
    }
}
