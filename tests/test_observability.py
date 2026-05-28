"""Tests for dispatch/observability.py — structured logging, metrics, request tracing."""

import json
import logging
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.observability import (
    OBSERVABILITY_SCHEMA_VERSION,
    MetricSnapshot,
    MetricsCollector,
    RequestMetric,
    RequestTracer,
    SpanRecord,
    StructuredFormatter,
    setup_structured_logging,
)


class RequestMetricTests(unittest.TestCase):
    def test_fields(self):
        m = RequestMetric(
            request_id="r1", component="dispatch", action="analyze",
            duration_ms=12.5, status="ok", timestamp=1000.0,
        )
        self.assertEqual(m.request_id, "r1")
        self.assertEqual(m.component, "dispatch")
        self.assertEqual(m.duration_ms, 12.5)
        self.assertEqual(m.status, "ok")

    def test_immutable(self):
        m = RequestMetric(
            request_id="r1", component="dispatch", action="analyze",
            duration_ms=1.0, status="ok", timestamp=1.0,
        )
        with self.assertRaises(AttributeError):
            m.request_id = "r2"  # type: ignore[misc]


class MetricSnapshotTests(unittest.TestCase):
    def test_fields(self):
        s = MetricSnapshot(name="latency", value=42.0, labels={"tier": "fast"})
        self.assertEqual(s.name, "latency")
        self.assertEqual(s.value, 42.0)
        self.assertIn("tier", s.labels)

    def test_default_timestamp(self):
        before = time.time()
        s = MetricSnapshot(name="x", value=1.0)
        self.assertGreaterEqual(s.timestamp, before)

    def test_immutable(self):
        s = MetricSnapshot(name="x", value=1.0)
        with self.assertRaises(AttributeError):
            s.value = 2.0  # type: ignore[misc]


class StructuredFormatterTests(unittest.TestCase):
    def test_json_output(self):
        formatter = StructuredFormatter()
        record = logging.LogRecord(
            name="test", level=logging.INFO, pathname="",
            lineno=0, msg="hello %s", args=("world",), exc_info=None,
        )
        output = formatter.format(record)
        parsed = json.loads(output)
        self.assertEqual(parsed["level"], "INFO")
        self.assertEqual(parsed["message"], "hello world")
        self.assertEqual(parsed["logger"], "test")

    def test_exception_includes_traceback(self):
        formatter = StructuredFormatter()
        try:
            raise ValueError("boom")
        except ValueError:
            record = logging.LogRecord(
                name="test", level=logging.ERROR, pathname="",
                lineno=0, msg="error occurred", args=(), exc_info=sys.exc_info(),
            )
        output = formatter.format(record)
        parsed = json.loads(output)
        self.assertIn("exception", parsed)
        self.assertIn("ValueError", parsed["exception"])

    def test_trace_id_in_record(self):
        formatter = StructuredFormatter()
        record = logging.LogRecord(
            name="test", level=logging.INFO, pathname="",
            lineno=0, msg="msg", args=(), exc_info=None,
        )
        record.trace_id = "trace-abc"  # type: ignore[attr-defined]
        record.span_id = "span-123"  # type: ignore[attr-defined]
        output = formatter.format(record)
        parsed = json.loads(output)
        self.assertEqual(parsed["trace_id"], "trace-abc")
        self.assertEqual(parsed["span_id"], "span-123")


class MetricsCollectorTests(unittest.TestCase):
    def test_record_and_count(self):
        collector = MetricsCollector()
        collector.record(RequestMetric(
            request_id="r1", component="d", action="a",
            duration_ms=1.0, status="ok", timestamp=1.0,
        ))
        self.assertEqual(collector.count(), 1)

    def test_query_by_component(self):
        collector = MetricsCollector()
        collector.record(RequestMetric("r1", "comp_a", "act", 1.0, "ok", 1.0))
        collector.record(RequestMetric("r2", "comp_b", "act", 1.0, "ok", 1.0))
        self.assertEqual(len(collector.query(component="comp_a")), 1)

    def test_query_by_action(self):
        collector = MetricsCollector()
        collector.record(RequestMetric("r1", "c", "act1", 1.0, "ok", 1.0))
        collector.record(RequestMetric("r2", "c", "act2", 1.0, "ok", 1.0))
        self.assertEqual(len(collector.query(action="act1")), 1)

    def test_query_all_filters(self):
        collector = MetricsCollector()
        collector.record(RequestMetric("r1", "c", "a", 1.0, "ok", 1.0))
        collector.record(RequestMetric("r2", "c", "b", 1.0, "ok", 1.0))
        collector.record(RequestMetric("r3", "d", "a", 1.0, "ok", 1.0))
        self.assertEqual(len(collector.query(component="c", action="a")), 1)

    def test_ring_buffer_eviction(self):
        collector = MetricsCollector(max_size=3)
        for i in range(5):
            collector.record(RequestMetric(f"r{i}", "c", "a", 1.0, "ok", float(i)))
        self.assertEqual(collector.count(), 3)
        results = collector.query()
        self.assertEqual(results[0].request_id, "r2")

    def test_record_snapshot(self):
        collector = MetricsCollector()
        collector.record_snapshot(MetricSnapshot(name="lat", value=10.0))
        self.assertEqual(len(collector.query_snapshots()), 1)

    def test_query_snapshots_by_name(self):
        collector = MetricsCollector()
        collector.record_snapshot(MetricSnapshot(name="lat", value=10.0))
        collector.record_snapshot(MetricSnapshot(name="mem", value=50.0))
        self.assertEqual(len(collector.query_snapshots(name="lat")), 1)

    def test_clear(self):
        collector = MetricsCollector()
        collector.record(RequestMetric("r1", "c", "a", 1.0, "ok", 1.0))
        collector.record_snapshot(MetricSnapshot(name="x", value=1.0))
        collector.clear()
        self.assertEqual(collector.count(), 0)
        self.assertEqual(len(collector.query_snapshots()), 0)

    def test_empty_query(self):
        collector = MetricsCollector()
        self.assertEqual(collector.query(), [])
        self.assertEqual(collector.query_snapshots(), [])

    def test_count_by_component(self):
        collector = MetricsCollector()
        collector.record(RequestMetric("r1", "c1", "a", 1.0, "ok", 1.0))
        collector.record(RequestMetric("r2", "c1", "a", 1.0, "ok", 1.0))
        collector.record(RequestMetric("r3", "c2", "a", 1.0, "ok", 1.0))
        self.assertEqual(collector.count(component="c1"), 2)


class SpanRecordTests(unittest.TestCase):
    def test_fields(self):
        s = SpanRecord(
            trace_id="t1", span_id="s1", parent_span_id=None,
            name="dispatch", start_time=1.0, end_time=2.0, status="ok",
        )
        self.assertEqual(s.trace_id, "t1")
        self.assertEqual(s.status, "ok")
        self.assertIsNone(s.parent_span_id)


class RequestTracerTests(unittest.TestCase):
    def test_new_trace_id(self):
        tracer = RequestTracer()
        tid = tracer.new_trace_id()
        self.assertEqual(len(tid), 32)

    def test_new_span_id(self):
        tracer = RequestTracer()
        sid = tracer.new_span_id()
        self.assertEqual(len(sid), 16)

    def test_start_and_end_span(self):
        tracer = RequestTracer()
        trace_id, span_id = tracer.start_span("dispatch")
        self.assertIsNotNone(trace_id)
        self.assertIsNotNone(span_id)
        span = tracer.end_span(span_id, "ok")
        self.assertIsNotNone(span)
        self.assertEqual(span.status, "ok")  # type: ignore[union-attr]
        self.assertIsNotNone(span.end_time)  # type: ignore[union-attr]

    def test_start_span_with_explicit_trace_id(self):
        tracer = RequestTracer()
        trace_id, span_id = tracer.start_span("op", trace_id="custom-trace")
        self.assertEqual(trace_id, "custom-trace")

    def test_end_span_unknown_returns_none(self):
        tracer = RequestTracer()
        self.assertIsNone(tracer.end_span("nonexistent"))

    def test_get_span(self):
        tracer = RequestTracer()
        _, span_id = tracer.start_span("op")
        span = tracer.get_span(span_id)
        self.assertIsNotNone(span)
        self.assertEqual(span.name, "op")  # type: ignore[union-attr]

    def test_get_trace_spans(self):
        tracer = RequestTracer()
        trace_id, sid1 = tracer.start_span("op1")
        tracer.start_span("op2", trace_id=trace_id)
        spans = tracer.get_trace_spans(trace_id)
        self.assertEqual(len(spans), 2)

    def test_parent_child_relationship(self):
        tracer = RequestTracer()
        trace_id, parent_id = tracer.start_span("parent")
        _, child_id = tracer.start_span("child", trace_id=trace_id, parent_span_id=parent_id)
        child = tracer.get_span(child_id)
        self.assertEqual(child.parent_span_id, parent_id)  # type: ignore[union-attr]

    def test_clear(self):
        tracer = RequestTracer()
        tracer.start_span("op")
        tracer.clear()
        self.assertEqual(len(tracer.get_trace_spans("any")), 0)

    def test_get_trace_spans_empty(self):
        tracer = RequestTracer()
        self.assertEqual(tracer.get_trace_spans("nonexistent"), [])


class SetupStructuredLoggingTests(unittest.TestCase):
    def test_returns_logger(self):
        logger = setup_structured_logging("test_harness_obs")
        self.assertIsInstance(logger, logging.Logger)

    def test_handler_added(self):
        logger = setup_structured_logging("test_harness_obs2")
        has_structured = any(isinstance(h.formatter, StructuredFormatter) for h in logger.handlers)
        self.assertTrue(has_structured)

    def test_no_duplicate_handlers(self):
        logger_name = "test_harness_no_dup_" + str(id(object()))
        logger = setup_structured_logging(logger_name)
        count_before = len(logger.handlers)
        setup_structured_logging(logger_name)
        self.assertEqual(len(logger.handlers), count_before)


class SchemaVersionTests(unittest.TestCase):
    def test_schema_version_defined(self):
        self.assertEqual(OBSERVABILITY_SCHEMA_VERSION, "observability.v1")


if __name__ == "__main__":
    unittest.main()
