use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

const DEFAULT_FAILURE_THRESHOLD: usize = 3;
const DEFAULT_LOOP_THRESHOLD: usize = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrajectoryAnomaly {
    pub anomaly_type: String,
    pub item_id: Option<String>,
    pub event_ids: Vec<String>,
    pub message: String,
    pub severity: String,
}

impl Default for TrajectoryAnomaly {
    fn default() -> Self {
        Self {
            anomaly_type: String::new(),
            item_id: None,
            event_ids: Vec::new(),
            message: String::new(),
            severity: "info".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrajectoryReport {
    pub ok: bool,
    pub anomalies: Vec<TrajectoryAnomaly>,
    pub retry_count: usize,
    pub loop_detected: bool,
    pub missing_handoff_count: usize,
}

impl Default for TrajectoryReport {
    fn default() -> Self {
        Self {
            ok: true,
            anomalies: Vec::new(),
            retry_count: 0,
            loop_detected: false,
            missing_handoff_count: 0,
        }
    }
}

pub struct TrajectoryMonitor {
    pub failure_threshold: usize,
    pub loop_threshold: usize,
}

impl Default for TrajectoryMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl TrajectoryMonitor {
    pub fn new() -> Self {
        Self {
            failure_threshold: DEFAULT_FAILURE_THRESHOLD,
            loop_threshold: DEFAULT_LOOP_THRESHOLD,
        }
    }

    pub fn with_thresholds(failure_threshold: usize, loop_threshold: usize) -> Self {
        Self {
            failure_threshold,
            loop_threshold,
        }
    }

    pub fn analyze_project_stream_from_path(&self, event_log_path: &str) -> TrajectoryReport {
        let path = Path::new(event_log_path);
        if !path.exists() {
            return TrajectoryReport {
                ok: false,
                anomalies: vec![TrajectoryAnomaly {
                    anomaly_type: "missing_file".to_string(),
                    item_id: None,
                    event_ids: Vec::new(),
                    message: format!("event log not found: {}", event_log_path),
                    severity: "error".to_string(),
                }],
                ..Default::default()
            };
        }
        match fs::read_to_string(path) {
            Ok(content) => self.analyze_project_stream(&content),
            Err(_) => TrajectoryReport {
                ok: false,
                anomalies: vec![TrajectoryAnomaly {
                    anomaly_type: "malformed_stream".to_string(),
                    item_id: None,
                    event_ids: Vec::new(),
                    message: "event log contains invalid JSON".to_string(),
                    severity: "error".to_string(),
                }],
                ..Default::default()
            },
        }
    }

    pub fn analyze_project_stream(&self, content: &str) -> TrajectoryReport {
        let events = match parse_jsonl(content) {
            Some(e) => e,
            None => {
                return TrajectoryReport {
                    ok: false,
                    anomalies: vec![TrajectoryAnomaly {
                        anomaly_type: "malformed_stream".to_string(),
                        item_id: None,
                        event_ids: Vec::new(),
                        message: "event log contains invalid JSON".to_string(),
                        severity: "error".to_string(),
                    }],
                    ..Default::default()
                };
            }
        };
        let mut anomalies = Vec::new();
        self.check_repeated_failures(&events, &mut anomalies);
        self.check_loops(&events, &mut anomalies);
        self.check_missing_handoffs(&events, &mut anomalies);

        let has_error = anomalies.iter().any(|a| a.severity == "error");
        let loop_detected = anomalies.iter().any(|a| a.anomaly_type == "loop_detected");
        let missing_handoff_count = anomalies
            .iter()
            .filter(|a| a.anomaly_type == "missing_handoff")
            .count();

        TrajectoryReport {
            ok: !has_error,
            anomalies,
            retry_count: 0,
            loop_detected,
            missing_handoff_count,
        }
    }

    pub fn analyze_task_stream_from_path(&self, task_events_path: &str) -> TrajectoryReport {
        let path = Path::new(task_events_path);
        if !path.exists() {
            return TrajectoryReport {
                ok: false,
                anomalies: vec![TrajectoryAnomaly {
                    anomaly_type: "missing_file".to_string(),
                    item_id: None,
                    event_ids: Vec::new(),
                    message: format!("task events not found: {}", task_events_path),
                    severity: "error".to_string(),
                }],
                ..Default::default()
            };
        }
        match fs::read_to_string(path) {
            Ok(content) => self.analyze_task_stream(&content),
            Err(_) => TrajectoryReport {
                ok: false,
                anomalies: vec![TrajectoryAnomaly {
                    anomaly_type: "malformed_stream".to_string(),
                    item_id: None,
                    event_ids: Vec::new(),
                    message: "task events contain invalid JSON".to_string(),
                    severity: "error".to_string(),
                }],
                ..Default::default()
            },
        }
    }

    pub fn analyze_task_stream(&self, content: &str) -> TrajectoryReport {
        let events = match parse_jsonl(content) {
            Some(e) => e,
            None => {
                return TrajectoryReport {
                    ok: false,
                    anomalies: vec![TrajectoryAnomaly {
                        anomaly_type: "malformed_stream".to_string(),
                        item_id: None,
                        event_ids: Vec::new(),
                        message: "task events contain invalid JSON".to_string(),
                        severity: "error".to_string(),
                    }],
                    ..Default::default()
                };
            }
        };
        let mut anomalies = Vec::new();
        self.check_retries_from_events(&events, &mut anomalies);

        let has_error = anomalies.iter().any(|a| a.severity == "error");
        let retry_count = anomalies
            .iter()
            .filter(|a| a.anomaly_type == "excessive_retry" && a.severity == "warn")
            .count();

        TrajectoryReport {
            ok: !has_error,
            anomalies,
            retry_count,
            ..Default::default()
        }
    }

    fn check_repeated_failures(
        &self,
        events: &[serde_json::Value],
        anomalies: &mut Vec<TrajectoryAnomaly>,
    ) {
        let mut failure_counts: HashMap<String, usize> = HashMap::new();
        let mut failure_event_ids: HashMap<String, Vec<String>> = HashMap::new();

        for event in events {
            if event.get("event_type").and_then(|v| v.as_str())
                != Some("project_item_state_changed")
            {
                continue;
            }
            let payload = event
                .get("payload")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            if payload.get("new_status").and_then(|v| v.as_str()) == Some("failed") {
                let item_id = payload
                    .get("item_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("<unknown>")
                    .to_string();
                let event_id = event
                    .get("event_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                *failure_counts.entry(item_id.clone()).or_insert(0) += 1;
                failure_event_ids.entry(item_id).or_default().push(event_id);
            }
        }

        for (item_id, count) in &failure_counts {
            if *count >= self.failure_threshold {
                anomalies.push(TrajectoryAnomaly {
                    anomaly_type: "repeated_failure".to_string(),
                    item_id: Some(item_id.clone()),
                    event_ids: failure_event_ids.get(item_id).cloned().unwrap_or_default(),
                    message: format!(
                        "item {} failed {} times (threshold: {})",
                        item_id, count, self.failure_threshold
                    ),
                    severity: "error".to_string(),
                });
            }
        }
    }

    fn check_loops(&self, events: &[serde_json::Value], anomalies: &mut Vec<TrajectoryAnomaly>) {
        let mut transitions: HashMap<String, Vec<(String, String, String)>> = HashMap::new();

        for event in events {
            if event.get("event_type").and_then(|v| v.as_str())
                != Some("project_item_state_changed")
            {
                continue;
            }
            let payload = event
                .get("payload")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let item_id = payload
                .get("item_id")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>")
                .to_string();
            let prev = payload
                .get("previous_status")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let new = payload
                .get("new_status")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let event_id = event
                .get("event_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            transitions
                .entry(item_id)
                .or_default()
                .push((prev, new, event_id));
        }

        for (item_id, trans_list) in &transitions {
            if trans_list.len() < 4 {
                continue;
            }
            let mut pair_counts: HashMap<(String, String), usize> = HashMap::new();
            for (prev, new, _) in trans_list {
                *pair_counts.entry((prev.clone(), new.clone())).or_insert(0) += 1;
            }

            for ((prev, new), count) in &pair_counts {
                if *count >= self.loop_threshold {
                    let event_ids: Vec<String> = trans_list
                        .iter()
                        .filter(|(p, n, _)| p == prev && n == new)
                        .map(|(_, _, eid)| eid.clone())
                        .collect();
                    anomalies.push(TrajectoryAnomaly {
                        anomaly_type: "loop_detected".to_string(),
                        item_id: Some(item_id.clone()),
                        event_ids,
                        message: format!(
                            "item {}: transition {}->{} repeated {} times",
                            item_id, prev, new, count
                        ),
                        severity: "error".to_string(),
                    });
                }
            }
        }
    }

    fn check_missing_handoffs(
        &self,
        events: &[serde_json::Value],
        anomalies: &mut Vec<TrajectoryAnomaly>,
    ) {
        let mut handoff_items: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut running_items: HashMap<String, String> = HashMap::new();

        for event in events {
            let event_type = event.get("event_type").and_then(|v| v.as_str());
            let payload = event
                .get("payload")
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            if event_type == Some("project_to_queue_handoff_created") {
                if let Some(item_id) = payload.get("item_id").and_then(|v| v.as_str()) {
                    handoff_items.insert(item_id.to_string());
                }
            }

            if event_type == Some("project_item_state_changed")
                && payload.get("new_status").and_then(|v| v.as_str()) == Some("running")
            {
                let item_id = payload
                    .get("item_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let event_id = event
                    .get("event_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                running_items.insert(item_id, event_id);
            }
        }

        for (item_id, event_id) in &running_items {
            if !handoff_items.contains(item_id) {
                anomalies.push(TrajectoryAnomaly {
                    anomaly_type: "missing_handoff".to_string(),
                    item_id: Some(item_id.clone()),
                    event_ids: vec![event_id.clone()],
                    message: format!("item {} reached running without handoff event", item_id),
                    severity: "warn".to_string(),
                });
            }
        }
    }

    fn check_retries_from_events(
        &self,
        events: &[serde_json::Value],
        anomalies: &mut Vec<TrajectoryAnomaly>,
    ) {
        for event in events {
            let payload = event
                .get("payload")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let retry_count = payload.get("retry_count").and_then(|v| v.as_u64());
            if let Some(rc) = retry_count {
                if rc >= 3 {
                    let item_id = payload
                        .get("item_id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let event_id = event
                        .get("event_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    anomalies.push(TrajectoryAnomaly {
                        anomaly_type: "excessive_retry".to_string(),
                        item_id,
                        event_ids: vec![event_id],
                        message: format!("retry_count={} exceeds threshold", rc),
                        severity: "warn".to_string(),
                    });
                }
            }
        }
    }
}

fn parse_jsonl(content: &str) -> Option<Vec<serde_json::Value>> {
    let mut events = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<serde_json::Value>(trimmed) {
            Ok(v) => events.push(v),
            Err(_) => return None,
        }
    }
    Some(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repeated_failures_detected() {
        let monitor = TrajectoryMonitor::new();
        let mut lines = Vec::new();
        for i in 0..4 {
            lines.push(format!(
                r#"{{"event_id":"e{}","event_type":"project_item_state_changed","payload":{{"item_id":"item1","previous_status":"running","new_status":"failed"}}}}"#,
                i
            ));
        }
        let content = lines.join("\n");
        let report = monitor.analyze_project_stream(&content);
        assert!(!report.ok);
        assert!(report
            .anomalies
            .iter()
            .any(|a| a.anomaly_type == "repeated_failure"));
    }

    #[test]
    fn test_loop_detected() {
        let monitor = TrajectoryMonitor::new();
        let mut lines = Vec::new();
        for i in 0..4 {
            lines.push(format!(
                r#"{{"event_id":"e{}","event_type":"project_item_state_changed","payload":{{"item_id":"item1","previous_status":"review","new_status":"in_progress"}}}}"#,
                i
            ));
        }
        let content = lines.join("\n");
        let report = monitor.analyze_project_stream(&content);
        assert!(report.loop_detected);
    }

    #[test]
    fn test_missing_handoff_detected() {
        let monitor = TrajectoryMonitor::new();
        let content = r#"{"event_id":"e1","event_type":"project_item_state_changed","payload":{"item_id":"item1","new_status":"running"}}"#;
        let report = monitor.analyze_project_stream(&content);
        assert_eq!(report.missing_handoff_count, 1);
        assert!(report
            .anomalies
            .iter()
            .any(|a| a.anomaly_type == "missing_handoff"));
    }

    #[test]
    fn test_handoff_present_no_anomaly() {
        let monitor = TrajectoryMonitor::new();
        let content = r#"{"event_id":"e1","event_type":"project_to_queue_handoff_created","payload":{"item_id":"item1"}}
{"event_id":"e2","event_type":"project_item_state_changed","payload":{"item_id":"item1","new_status":"running"}}"#;
        let report = monitor.analyze_project_stream(&content);
        assert_eq!(report.missing_handoff_count, 0);
    }

    #[test]
    fn test_excessive_retry_detected() {
        let monitor = TrajectoryMonitor::new();
        let content = r#"{"event_id":"e1","payload":{"item_id":"item1","retry_count":5}}"#;
        let report = monitor.analyze_task_stream(content);
        assert_eq!(report.retry_count, 1);
    }

    #[test]
    fn test_malformed_stream_returns_error() {
        let monitor = TrajectoryMonitor::new();
        let content = "not valid json";
        let report = monitor.analyze_project_stream(content);
        assert!(!report.ok);
        assert!(report
            .anomalies
            .iter()
            .any(|a| a.anomaly_type == "malformed_stream"));
    }

    #[test]
    fn test_missing_file_returns_error() {
        let monitor = TrajectoryMonitor::new();
        let report = monitor.analyze_project_stream_from_path("/nonexistent/path.jsonl");
        assert!(!report.ok);
        assert!(report
            .anomalies
            .iter()
            .any(|a| a.anomaly_type == "missing_file"));
    }

    #[test]
    fn test_clean_stream_no_anomalies() {
        let monitor = TrajectoryMonitor::new();
        let content = r#"{"event_id":"e1","event_type":"project_to_queue_handoff_created","payload":{"item_id":"item1"}}
{"event_id":"e2","event_type":"project_item_state_changed","payload":{"item_id":"item1","new_status":"running"}}
{"event_id":"e3","event_type":"project_item_state_changed","payload":{"item_id":"item1","previous_status":"running","new_status":"completed"}}"#;
        let report = monitor.analyze_project_stream(content);
        assert!(report.ok);
        assert!(report.anomalies.is_empty());
    }

    #[test]
    fn test_excessive_retry_below_threshold_ignored() {
        let monitor = TrajectoryMonitor::new();
        let content = r#"{"event_id":"e1","payload":{"item_id":"item1","retry_count":2}}"#;
        let report = monitor.analyze_task_stream(content);
        assert_eq!(report.retry_count, 0);
        assert!(report.ok);
    }

    #[test]
    fn test_anomaly_default_values() {
        let a = TrajectoryAnomaly::default();
        assert_eq!(a.severity, "info");
        assert!(a.event_ids.is_empty());
    }
}
