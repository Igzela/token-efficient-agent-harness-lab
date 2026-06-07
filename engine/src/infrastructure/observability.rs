use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Mutex;

pub const OBSERVABILITY_SCHEMA_VERSION: &str = "observability.v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestMetric {
    pub request_id: String,
    pub component: String,
    pub action: String,
    pub duration_ms: f64,
    pub status: String,
    pub timestamp: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricSnapshot {
    pub name: String,
    pub value: f64,
    pub labels: std::collections::HashMap<String, String>,
    pub timestamp: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpanRecord {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub name: String,
    pub start_time: f64,
    pub end_time: Option<f64>,
    pub status: String,
}

pub struct MetricsCollector {
    max_size: usize,
    metrics: Mutex<VecDeque<RequestMetric>>,
    snapshots: Mutex<VecDeque<MetricSnapshot>>,
}

impl MetricsCollector {
    pub fn new(max_size: usize) -> Self {
        Self {
            max_size,
            metrics: Mutex::new(VecDeque::with_capacity(max_size.min(1000))),
            snapshots: Mutex::new(VecDeque::with_capacity(max_size.min(1000))),
        }
    }

    pub fn record(&self, metric: RequestMetric) {
        let mut metrics = self.metrics.lock().unwrap();
        if metrics.len() >= self.max_size {
            metrics.pop_front();
        }
        metrics.push_back(metric);
    }

    pub fn record_snapshot(&self, snapshot: MetricSnapshot) {
        let mut snapshots = self.snapshots.lock().unwrap();
        if snapshots.len() >= self.max_size {
            snapshots.pop_front();
        }
        snapshots.push_back(snapshot);
    }

    pub fn query(&self, component: Option<&str>, action: Option<&str>) -> Vec<RequestMetric> {
        let metrics = self.metrics.lock().unwrap();
        metrics
            .iter()
            .filter(|m| component.is_none_or(|c| m.component == c))
            .filter(|m| action.is_none_or(|a| m.action == a))
            .cloned()
            .collect()
    }

    pub fn query_snapshots(&self, name: Option<&str>) -> Vec<MetricSnapshot> {
        let snapshots = self.snapshots.lock().unwrap();
        snapshots
            .iter()
            .filter(|s| name.is_none_or(|n| s.name == n))
            .cloned()
            .collect()
    }

    pub fn count(&self, component: Option<&str>) -> usize {
        self.query(component, None).len()
    }

    pub fn clear(&self) {
        self.metrics.lock().unwrap().clear();
        self.snapshots.lock().unwrap().clear();
    }
}

pub struct RequestTracer {
    spans: Mutex<std::collections::HashMap<String, SpanRecord>>,
    counter: Mutex<u64>,
}

impl Default for RequestTracer {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestTracer {
    pub fn new() -> Self {
        Self {
            spans: Mutex::new(std::collections::HashMap::new()),
            counter: Mutex::new(0),
        }
    }

    pub fn new_trace_id(&self) -> String {
        let mut counter = self.counter.lock().unwrap();
        *counter += 1;
        let c = *counter;
        format!("trace-{c:08x}")
    }

    pub fn new_span_id(&self) -> String {
        let mut counter = self.counter.lock().unwrap();
        *counter += 1;
        let c = *counter;
        format!("span-{c:08x}")
    }

    pub fn start_span(
        &self,
        name: &str,
        trace_id: Option<&str>,
        parent_span_id: Option<&str>,
        now: f64,
    ) -> (String, String) {
        let trace_id = trace_id
            .map(String::from)
            .unwrap_or_else(|| self.new_trace_id());
        let span_id = self.new_span_id();
        let span = SpanRecord {
            trace_id: trace_id.clone(),
            span_id: span_id.clone(),
            parent_span_id: parent_span_id.map(String::from),
            name: name.to_string(),
            start_time: now,
            end_time: None,
            status: "in_progress".to_string(),
        };
        self.spans.lock().unwrap().insert(span_id.clone(), span);
        (trace_id, span_id)
    }

    pub fn end_span(&self, span_id: &str, status: &str, now: f64) -> Option<SpanRecord> {
        let mut spans = self.spans.lock().unwrap();
        let span = spans.get(span_id)?.clone();
        let updated = SpanRecord {
            end_time: Some(now),
            status: status.to_string(),
            ..span
        };
        spans.insert(span_id.to_string(), updated.clone());
        Some(updated)
    }

    pub fn get_span(&self, span_id: &str) -> Option<SpanRecord> {
        self.spans.lock().unwrap().get(span_id).cloned()
    }

    pub fn get_trace_spans(&self, trace_id: &str) -> Vec<SpanRecord> {
        self.spans
            .lock()
            .unwrap()
            .values()
            .filter(|s| s.trace_id == trace_id)
            .cloned()
            .collect()
    }

    pub fn clear(&self) {
        self.spans.lock().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_collector_records_and_queries() {
        let collector = MetricsCollector::new(100);
        collector.record(RequestMetric {
            request_id: "r1".into(),
            component: "http".into(),
            action: "GET /health".into(),
            duration_ms: 1.5,
            status: "200".into(),
            timestamp: 1000.0,
        });
        collector.record(RequestMetric {
            request_id: "r2".into(),
            component: "scheduler".into(),
            action: "tick".into(),
            duration_ms: 5.0,
            status: "error".into(),
            timestamp: 1001.0,
        });
        assert_eq!(collector.count(None), 2);
        assert_eq!(collector.count(Some("http")), 1);
        assert_eq!(collector.count(Some("scheduler")), 1);
        let errors = collector.query(Some("scheduler"), None);
        assert_eq!(errors[0].status, "error");
    }

    #[test]
    fn metrics_collector_bounded_ring() {
        let collector = MetricsCollector::new(2);
        for i in 0..5 {
            collector.record(RequestMetric {
                request_id: format!("r{}", i),
                component: "test".into(),
                action: "a".into(),
                duration_ms: 0.0,
                status: "ok".into(),
                timestamp: i as f64,
            });
        }
        assert_eq!(collector.count(None), 2);
        let metrics = collector.query(None, None);
        assert_eq!(metrics[0].request_id, "r3");
        assert_eq!(metrics[1].request_id, "r4");
    }

    #[test]
    fn tracer_spans_lifecycle() {
        let tracer = RequestTracer::new();
        let (trace_id, span_id) = tracer.start_span("request", None, None, 100.0);
        assert!(trace_id.starts_with("trace-"));
        assert!(span_id.starts_with("span-"));

        let span = tracer.get_span(&span_id).unwrap();
        assert_eq!(span.status, "in_progress");
        assert!(span.end_time.is_none());

        let ended = tracer.end_span(&span_id, "ok", 105.0).unwrap();
        assert_eq!(ended.status, "ok");
        assert_eq!(ended.end_time, Some(105.0));

        let trace_spans = tracer.get_trace_spans(&trace_id);
        assert_eq!(trace_spans.len(), 1);
    }

    #[test]
    fn snapshot_recording() {
        let collector = MetricsCollector::new(100);
        collector.record_snapshot(MetricSnapshot {
            name: "scheduler.tick".into(),
            value: 42.0,
            labels: [("executor".into(), "noop".into())].into(),
            timestamp: 1000.0,
        });
        let snaps = collector.query_snapshots(None);
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].name, "scheduler.tick");
        assert_eq!(snaps[0].value, 42.0);
    }
}
