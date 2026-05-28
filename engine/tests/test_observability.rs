use engine::infrastructure::observability::*;

fn make_metric(id: &str, component: &str, action: &str) -> RequestMetric {
    RequestMetric {
        request_id: id.to_string(),
        component: component.to_string(),
        action: action.to_string(),
        duration_ms: 100.0,
        status: "ok".to_string(),
        timestamp: 1000.0,
    }
}

#[test]
fn test_metrics_collector_record_and_query() {
    let collector = MetricsCollector::new(100);
    collector.record(make_metric("r1", "analyzer", "analyze"));
    collector.record(make_metric("r2", "selector", "select"));
    assert_eq!(collector.count(None), 2);
    assert_eq!(collector.count(Some("analyzer")), 1);
    assert_eq!(collector.count(Some("selector")), 1);
    assert_eq!(collector.count(Some("nonexistent")), 0);
}

#[test]
fn test_metrics_collector_query_with_filters() {
    let collector = MetricsCollector::new(100);
    collector.record(make_metric("r1", "analyzer", "analyze"));
    collector.record(make_metric("r2", "analyzer", "select"));
    collector.record(make_metric("r3", "selector", "select"));
    let result = collector.query(Some("analyzer"), Some("select"));
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].request_id, "r2");
}

#[test]
fn test_metrics_collector_ring_buffer_eviction() {
    let collector = MetricsCollector::new(3);
    collector.record(make_metric("r1", "a", "b"));
    collector.record(make_metric("r2", "a", "b"));
    collector.record(make_metric("r3", "a", "b"));
    collector.record(make_metric("r4", "a", "b")); // evicts r1
    assert_eq!(collector.count(None), 3);
    let all = collector.query(None, None);
    assert_eq!(all[0].request_id, "r2");
    assert_eq!(all[2].request_id, "r4");
}

#[test]
fn test_metrics_collector_snapshots() {
    let collector = MetricsCollector::new(100);
    collector.record_snapshot(MetricSnapshot {
        name: "cpu".to_string(),
        value: 0.5,
        labels: std::collections::HashMap::new(),
        timestamp: 1000.0,
    });
    collector.record_snapshot(MetricSnapshot {
        name: "memory".to_string(),
        value: 0.8,
        labels: std::collections::HashMap::new(),
        timestamp: 1001.0,
    });
    assert_eq!(collector.query_snapshots(None).len(), 2);
    assert_eq!(collector.query_snapshots(Some("cpu")).len(), 1);
}

#[test]
fn test_metrics_collector_clear() {
    let collector = MetricsCollector::new(100);
    collector.record(make_metric("r1", "a", "b"));
    collector.record_snapshot(MetricSnapshot {
        name: "test".to_string(),
        value: 1.0,
        labels: std::collections::HashMap::new(),
        timestamp: 0.0,
    });
    assert_eq!(collector.count(None), 1);
    collector.clear();
    assert_eq!(collector.count(None), 0);
    assert_eq!(collector.query_snapshots(None).len(), 0);
}

#[test]
fn test_request_tracer_start_and_end_span() {
    let tracer = RequestTracer::new();
    let (trace_id, span_id) = tracer.start_span("test", None, None, 1000.0);
    assert!(!trace_id.is_empty());
    assert!(!span_id.is_empty());

    let span = tracer.get_span(&span_id).unwrap();
    assert_eq!(span.name, "test");
    assert_eq!(span.status, "in_progress");
    assert!(span.end_time.is_none());

    let ended = tracer.end_span(&span_id, "ok", 1001.0).unwrap();
    assert_eq!(ended.status, "ok");
    assert_eq!(ended.end_time, Some(1001.0));
}

#[test]
fn test_request_tracer_parent_span() {
    let tracer = RequestTracer::new();
    let (trace_id, parent_id) = tracer.start_span("parent", None, None, 1000.0);
    let (_, child_id) = tracer.start_span("child", Some(&trace_id), Some(&parent_id), 1001.0);
    let child = tracer.get_span(&child_id).unwrap();
    assert_eq!(child.parent_span_id, Some(parent_id));
    assert_eq!(child.trace_id, trace_id);
}

#[test]
fn test_request_tracer_get_trace_spans() {
    let tracer = RequestTracer::new();
    let (tid1, _) = tracer.start_span("a", None, None, 1000.0);
    let (tid2, _) = tracer.start_span("b", None, None, 1001.0);
    let (_, _) = tracer.start_span("c", Some(&tid1), None, 1002.0);
    assert_eq!(tracer.get_trace_spans(&tid1).len(), 2);
    assert_eq!(tracer.get_trace_spans(&tid2).len(), 1);
}

#[test]
fn test_request_tracer_end_nonexistent_span() {
    let tracer = RequestTracer::new();
    assert!(tracer.end_span("nonexistent", "ok", 1000.0).is_none());
}

#[test]
fn test_request_tracer_clear() {
    let tracer = RequestTracer::new();
    let (_, sid) = tracer.start_span("test", None, None, 1000.0);
    assert!(tracer.get_span(&sid).is_some());
    tracer.clear();
    assert!(tracer.get_span(&sid).is_none());
}
