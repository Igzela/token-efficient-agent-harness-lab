use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::event_store::{replay_preflight_bytes, ValidationIssue};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const PROJECT_ITEM_STATE_CHANGED: &str = "project_item_state_changed";
pub const PROJECT_TO_QUEUE_HANDOFF_CREATED: &str = "project_to_queue_handoff_created";
pub const PROJECT_DEPENDENCY_RESOLVED: &str = "project_dependency_resolved";

pub fn supported_event_types() -> &'static [&'static str] {
    &[
        PROJECT_ITEM_STATE_CHANGED,
        PROJECT_TO_QUEUE_HANDOFF_CREATED,
        PROJECT_DEPENDENCY_RESOLVED,
    ]
}

// ---------------------------------------------------------------------------
// Data structs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProjectItemState {
    pub item_id: String,
    pub status: String,
    pub previous_status: Option<String>,
    pub reason: Option<String>,
    pub last_event_id: String,
    pub last_updated: String,
}

impl Default for ProjectItemState {
    fn default() -> Self {
        Self {
            item_id: String::new(),
            status: String::new(),
            previous_status: None,
            reason: None,
            last_event_id: String::new(),
            last_updated: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProjectStateProjection {
    #[serde(default)]
    pub items: HashMap<String, ProjectItemState>,
    #[serde(default)]
    pub warnings: Vec<ValidationIssue>,
}

impl Default for ProjectStateProjection {
    fn default() -> Self {
        Self {
            items: HashMap::new(),
            warnings: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HandoffRecord {
    pub handoff_id: String,
    pub item_id: String,
    pub scheduling_policy: String,
    pub event_id: String,
    pub timestamp: String,
}

impl Default for HandoffRecord {
    fn default() -> Self {
        Self {
            handoff_id: String::new(),
            item_id: String::new(),
            scheduling_policy: String::new(),
            event_id: String::new(),
            timestamp: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TaskQueueProjection {
    #[serde(default)]
    pub handoffs: Vec<HandoffRecord>,
    #[serde(default)]
    pub warnings: Vec<ValidationIssue>,
}

impl Default for TaskQueueProjection {
    fn default() -> Self {
        Self {
            handoffs: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DependencyResolvedRecord {
    pub edge_id: String,
    pub from_node: String,
    pub to_node: String,
    pub dependency_type: String,
    pub resolution: Option<String>,
    pub event_id: String,
    pub timestamp: String,
}

impl Default for DependencyResolvedRecord {
    fn default() -> Self {
        Self {
            edge_id: String::new(),
            from_node: String::new(),
            to_node: String::new(),
            dependency_type: String::new(),
            resolution: None,
            event_id: String::new(),
            timestamp: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DependencyProjection {
    #[serde(default)]
    pub resolved: Vec<DependencyResolvedRecord>,
    #[serde(default)]
    pub warnings: Vec<ValidationIssue>,
}

impl Default for DependencyProjection {
    fn default() -> Self {
        Self {
            resolved: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProjectionBundle {
    pub project: ProjectStateProjection,
    pub task_queue: TaskQueueProjection,
    pub dependencies: DependencyProjection,
    #[serde(default)]
    pub warnings: Vec<ValidationIssue>,
}

impl Default for ProjectionBundle {
    fn default() -> Self {
        Self {
            project: ProjectStateProjection::default(),
            task_queue: TaskQueueProjection::default(),
            dependencies: DependencyProjection::default(),
            warnings: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Replay functions — operate on JSONL bytes
// ---------------------------------------------------------------------------

/// Replay project item state changes from JSONL event bytes.
pub fn replay_project_state(data: &[u8]) -> ProjectStateProjection {
    let (events, warnings) = load_events_after_preflight(data);
    let mut projection = ProjectStateProjection {
        warnings,
        ..Default::default()
    };
    for event in &events {
        let event_type = event
            .get("event_type")
            .and_then(Value::as_str)
            .unwrap_or("");
        if event_type != PROJECT_ITEM_STATE_CHANGED {
            continue;
        }
        let Some(payload) = event.get("payload").and_then(Value::as_object) else {
            continue;
        };
        let item_id = payload.get("item_id").and_then(Value::as_str);
        let new_status = payload.get("new_status").and_then(Value::as_str);
        match (item_id, new_status) {
            (Some(item_id), Some(new_status)) => {
                let event_id = event
                    .get("event_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let timestamp = event
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                projection.items.insert(
                    item_id.to_string(),
                    ProjectItemState {
                        item_id: item_id.to_string(),
                        status: new_status.to_string(),
                        previous_status: payload
                            .get("previous_status")
                            .and_then(Value::as_str)
                            .map(String::from),
                        reason: payload
                            .get("reason")
                            .and_then(Value::as_str)
                            .map(String::from),
                        last_event_id: event_id,
                        last_updated: timestamp,
                    },
                );
            }
            _ => {
                let event_id = event
                    .get("event_id")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                projection
                    .warnings
                    .push(missing_payload_warning(event_id, "item_id/new_status"));
            }
        }
    }
    projection
}

/// Replay task queue handoff events from JSONL event bytes.
pub fn replay_task_queue_state(data: &[u8]) -> TaskQueueProjection {
    let (events, warnings) = load_events_after_preflight(data);
    let mut projection = TaskQueueProjection {
        warnings,
        ..Default::default()
    };
    for event in &events {
        let event_type = event
            .get("event_type")
            .and_then(Value::as_str)
            .unwrap_or("");
        if event_type != PROJECT_TO_QUEUE_HANDOFF_CREATED {
            continue;
        }
        let Some(payload) = event.get("payload").and_then(Value::as_object) else {
            continue;
        };
        let handoff_id = payload.get("handoff_id").and_then(Value::as_str);
        let item_id = payload.get("item_id").and_then(Value::as_str);
        let scheduling_policy = payload.get("scheduling_policy").and_then(Value::as_str);
        match (handoff_id, item_id, scheduling_policy) {
            (Some(hid), Some(iid), Some(sp)) => {
                let event_id = event
                    .get("event_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let timestamp = event
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                projection.handoffs.push(HandoffRecord {
                    handoff_id: hid.to_string(),
                    item_id: iid.to_string(),
                    scheduling_policy: sp.to_string(),
                    event_id,
                    timestamp,
                });
            }
            _ => {
                let event_id = event
                    .get("event_id")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                projection.warnings.push(missing_payload_warning(
                    event_id,
                    "handoff_id/item_id/scheduling_policy",
                ));
            }
        }
    }
    projection
}

/// Replay dependency resolution events from JSONL event bytes.
pub fn replay_dependency_state(data: &[u8]) -> DependencyProjection {
    let (events, warnings) = load_events_after_preflight(data);
    let mut projection = DependencyProjection {
        warnings,
        ..Default::default()
    };
    for event in &events {
        let event_type = event
            .get("event_type")
            .and_then(Value::as_str)
            .unwrap_or("");
        if event_type != PROJECT_DEPENDENCY_RESOLVED {
            continue;
        }
        let Some(payload) = event.get("payload").and_then(Value::as_object) else {
            continue;
        };
        let edge_id = payload.get("edge_id").and_then(Value::as_str);
        let from_node = payload.get("from_node").and_then(Value::as_str);
        let to_node = payload.get("to_node").and_then(Value::as_str);
        let dependency_type = payload.get("dependency_type").and_then(Value::as_str);
        match (edge_id, from_node, to_node, dependency_type) {
            (Some(eid), Some(fn_), Some(tn), Some(dt)) => {
                let event_id = event
                    .get("event_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let timestamp = event
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                projection.resolved.push(DependencyResolvedRecord {
                    edge_id: eid.to_string(),
                    from_node: fn_.to_string(),
                    to_node: tn.to_string(),
                    dependency_type: dt.to_string(),
                    resolution: payload
                        .get("resolution")
                        .and_then(Value::as_str)
                        .map(String::from),
                    event_id,
                    timestamp,
                });
            }
            _ => {
                let event_id = event
                    .get("event_id")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                projection.warnings.push(missing_payload_warning(
                    event_id,
                    "edge_id/from_node/to_node/dependency_type",
                ));
            }
        }
    }
    projection
}

/// Replay all projections from JSONL event bytes.
pub fn replay_all(data: &[u8]) -> ProjectionBundle {
    let (events, warnings) = load_events_after_preflight(data);
    let project = project_state_from_events(&events, &warnings);
    let task_queue = task_queue_from_events(&events, &warnings);
    let dependencies = dependency_state_from_events(&events, &warnings);
    ProjectionBundle {
        project,
        task_queue,
        dependencies,
        warnings,
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn load_events_after_preflight(data: &[u8]) -> (Vec<Value>, Vec<ValidationIssue>) {
    let preflight = replay_preflight_bytes(data);
    if !preflight.ok() {
        // In Python this raises; here we return empty with the errors as warnings
        let mut warnings = preflight.warnings;
        for err in &preflight.errors {
            warnings.push(err.clone());
        }
        return (Vec::new(), warnings);
    }

    let mut events = Vec::new();
    let mut warnings = preflight.warnings;
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
        if let Ok(event) = serde_json::from_slice::<Value>(trimmed) {
            let event_type = event
                .get("event_type")
                .and_then(Value::as_str)
                .unwrap_or("");
            if !supported_event_types().contains(&event_type) {
                warnings.push(ValidationIssue {
                    line_number: Some(line_number),
                    error_type: "UnknownEventTypeWarning".to_string(),
                    message: format!("ignored unsupported event_type: {}", event_type),
                });
            }
            events.push(event);
        }
    }
    (events, warnings)
}

fn project_state_from_events(
    events: &[Value],
    warnings: &[ValidationIssue],
) -> ProjectStateProjection {
    let mut projection = ProjectStateProjection {
        warnings: warnings.to_vec(),
        ..Default::default()
    };
    for event in events {
        let event_type = event
            .get("event_type")
            .and_then(Value::as_str)
            .unwrap_or("");
        if event_type != PROJECT_ITEM_STATE_CHANGED {
            continue;
        }
        let Some(payload) = event.get("payload").and_then(Value::as_object) else {
            continue;
        };
        let item_id = payload.get("item_id").and_then(Value::as_str);
        let new_status = payload.get("new_status").and_then(Value::as_str);
        if let (Some(item_id), Some(new_status)) = (item_id, new_status) {
            let event_id = event
                .get("event_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let timestamp = event
                .get("timestamp")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            projection.items.insert(
                item_id.to_string(),
                ProjectItemState {
                    item_id: item_id.to_string(),
                    status: new_status.to_string(),
                    previous_status: payload
                        .get("previous_status")
                        .and_then(Value::as_str)
                        .map(String::from),
                    reason: payload
                        .get("reason")
                        .and_then(Value::as_str)
                        .map(String::from),
                    last_event_id: event_id,
                    last_updated: timestamp,
                },
            );
        }
    }
    projection
}

fn task_queue_from_events(events: &[Value], warnings: &[ValidationIssue]) -> TaskQueueProjection {
    let mut projection = TaskQueueProjection {
        warnings: warnings.to_vec(),
        ..Default::default()
    };
    for event in events {
        let event_type = event
            .get("event_type")
            .and_then(Value::as_str)
            .unwrap_or("");
        if event_type != PROJECT_TO_QUEUE_HANDOFF_CREATED {
            continue;
        }
        let Some(payload) = event.get("payload").and_then(Value::as_object) else {
            continue;
        };
        let handoff_id = payload.get("handoff_id").and_then(Value::as_str);
        let item_id = payload.get("item_id").and_then(Value::as_str);
        let scheduling_policy = payload.get("scheduling_policy").and_then(Value::as_str);
        if let (Some(hid), Some(iid), Some(sp)) = (handoff_id, item_id, scheduling_policy) {
            let event_id = event
                .get("event_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let timestamp = event
                .get("timestamp")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            projection.handoffs.push(HandoffRecord {
                handoff_id: hid.to_string(),
                item_id: iid.to_string(),
                scheduling_policy: sp.to_string(),
                event_id,
                timestamp,
            });
        }
    }
    projection
}

fn dependency_state_from_events(
    events: &[Value],
    warnings: &[ValidationIssue],
) -> DependencyProjection {
    let mut projection = DependencyProjection {
        warnings: warnings.to_vec(),
        ..Default::default()
    };
    for event in events {
        let event_type = event
            .get("event_type")
            .and_then(Value::as_str)
            .unwrap_or("");
        if event_type != PROJECT_DEPENDENCY_RESOLVED {
            continue;
        }
        let Some(payload) = event.get("payload").and_then(Value::as_object) else {
            continue;
        };
        let edge_id = payload.get("edge_id").and_then(Value::as_str);
        let from_node = payload.get("from_node").and_then(Value::as_str);
        let to_node = payload.get("to_node").and_then(Value::as_str);
        let dependency_type = payload.get("dependency_type").and_then(Value::as_str);
        if let (Some(eid), Some(fn_), Some(tn), Some(dt)) =
            (edge_id, from_node, to_node, dependency_type)
        {
            let event_id = event
                .get("event_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let timestamp = event
                .get("timestamp")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            projection.resolved.push(DependencyResolvedRecord {
                edge_id: eid.to_string(),
                from_node: fn_.to_string(),
                to_node: tn.to_string(),
                dependency_type: dt.to_string(),
                resolution: payload
                    .get("resolution")
                    .and_then(Value::as_str)
                    .map(String::from),
                event_id,
                timestamp,
            });
        }
    }
    projection
}

fn missing_payload_warning(event_id: &str, missing: &str) -> ValidationIssue {
    ValidationIssue {
        line_number: None,
        error_type: "ProjectionPayloadWarning".to_string(),
        message: format!(
            "event {} missing projection payload field(s): {}",
            event_id, missing
        ),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_event_line(event_type: &str, event_id: &str, payload: Value) -> String {
        let ev = json!({
            "event_id": event_id,
            "schema_version": "event.v1",
            "event_type": event_type,
            "timestamp": "2026-01-01T00:00:00Z",
            "producer": {"component_id": "test", "component_type": "unit"},
            "correlation": {},
            "severity": "info",
            "payload": payload,
            "idempotency_key": format!("idem-{}", event_id),
            "parent_event_id": null
        });
        format!("{}\n", serde_json::to_string(&ev).unwrap())
    }

    #[test]
    fn test_replay_project_state_single_item() {
        let line = make_event_line(
            PROJECT_ITEM_STATE_CHANGED,
            "evt-1",
            json!({"item_id": "item-1", "new_status": "running", "previous_status": "ready", "reason": "started"}),
        );
        let projection = replay_project_state(line.as_bytes());
        assert!(
            projection.warnings.is_empty()
                || projection
                    .warnings
                    .iter()
                    .all(|w| w.error_type == "UnknownEventTypeWarning")
        );
        let item = projection.items.get("item-1").unwrap();
        assert_eq!(item.status, "running");
        assert_eq!(item.previous_status.as_deref(), Some("ready"));
    }

    #[test]
    fn test_replay_project_state_missing_payload_fields() {
        let line = make_event_line(
            PROJECT_ITEM_STATE_CHANGED,
            "evt-1",
            json!({"item_id": "item-1"}),
        );
        let projection = replay_project_state(line.as_bytes());
        assert!(projection
            .warnings
            .iter()
            .any(|w| w.error_type == "ProjectionPayloadWarning"));
    }

    #[test]
    fn test_replay_task_queue_state() {
        let line = make_event_line(
            PROJECT_TO_QUEUE_HANDOFF_CREATED,
            "evt-1",
            json!({"handoff_id": "hof-1", "item_id": "item-1", "scheduling_policy": "sequential"}),
        );
        let projection = replay_task_queue_state(line.as_bytes());
        assert_eq!(projection.handoffs.len(), 1);
        assert_eq!(projection.handoffs[0].handoff_id, "hof-1");
    }

    #[test]
    fn test_replay_dependency_state() {
        let line = make_event_line(
            PROJECT_DEPENDENCY_RESOLVED,
            "evt-1",
            json!({"edge_id": "edge-1", "from_node": "a", "to_node": "b", "dependency_type": "hard", "resolution": "resolved"}),
        );
        let projection = replay_dependency_state(line.as_bytes());
        assert_eq!(projection.resolved.len(), 1);
        assert_eq!(projection.resolved[0].edge_id, "edge-1");
        assert_eq!(
            projection.resolved[0].resolution.as_deref(),
            Some("resolved")
        );
    }

    #[test]
    fn test_replay_all_bundle() {
        let mut data = String::new();
        data.push_str(&make_event_line(
            PROJECT_ITEM_STATE_CHANGED,
            "evt-1",
            json!({"item_id": "item-1", "new_status": "done", "reason": "complete"}),
        ));
        data.push_str(&make_event_line(
            PROJECT_TO_QUEUE_HANDOFF_CREATED,
            "evt-2",
            json!({"handoff_id": "hof-1", "item_id": "item-1", "scheduling_policy": "sequential"}),
        ));
        data.push_str(&make_event_line(
            PROJECT_DEPENDENCY_RESOLVED,
            "evt-3",
            json!({"edge_id": "edge-1", "from_node": "a", "to_node": "b", "dependency_type": "hard"}),
        ));
        let bundle = replay_all(data.as_bytes());
        assert_eq!(bundle.project.items.len(), 1);
        assert_eq!(bundle.task_queue.handoffs.len(), 1);
        assert_eq!(bundle.dependencies.resolved.len(), 1);
    }

    #[test]
    fn test_replay_project_state_latest_wins() {
        let mut data = String::new();
        data.push_str(&make_event_line(
            PROJECT_ITEM_STATE_CHANGED,
            "evt-1",
            json!({"item_id": "item-1", "new_status": "running", "reason": "started"}),
        ));
        data.push_str(&make_event_line(
            PROJECT_ITEM_STATE_CHANGED,
            "evt-2",
            json!({"item_id": "item-1", "new_status": "done", "previous_status": "running", "reason": "finished"}),
        ));
        let projection = replay_project_state(data.as_bytes());
        let item = projection.items.get("item-1").unwrap();
        assert_eq!(item.status, "done");
        assert_eq!(item.last_event_id, "evt-2");
    }

    #[test]
    fn test_empty_input() {
        let projection = replay_project_state(b"");
        assert!(projection.items.is_empty());
    }
}
