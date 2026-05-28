"""Phase 6A: Observability — structured logging, metrics, request tracing."""

from __future__ import annotations

import json
import logging
import threading
import time
import uuid
from collections import deque
from dataclasses import asdict, dataclass, field
from typing import Any


OBSERVABILITY_SCHEMA_VERSION = "observability.v1"


@dataclass(frozen=True)
class RequestMetric:
    request_id: str
    component: str
    action: str
    duration_ms: float
    status: str
    timestamp: float


@dataclass(frozen=True)
class MetricSnapshot:
    name: str
    value: float
    labels: dict[str, str] = field(default_factory=dict)
    timestamp: float = field(default_factory=time.time)


@dataclass(frozen=True)
class SpanRecord:
    trace_id: str
    span_id: str
    parent_span_id: str | None
    name: str
    start_time: float
    end_time: float | None
    status: str


class StructuredFormatter(logging.Formatter):
    """JSON log formatter for structured logging."""

    def format(self, record: logging.LogRecord) -> str:
        log_entry: dict[str, Any] = {
            "timestamp": record.created,
            "level": record.levelname,
            "logger": record.name,
            "message": record.getMessage(),
        }
        if hasattr(record, "trace_id"):
            log_entry["trace_id"] = record.trace_id  # type: ignore[attr-defined]
        if hasattr(record, "span_id"):
            log_entry["span_id"] = record.span_id  # type: ignore[attr-defined]
        if record.exc_info and record.exc_info[0] is not None:
            log_entry["exception"] = self.formatException(record.exc_info)
        return json.dumps(log_entry, default=str)


class MetricsCollector:
    """Ring-buffer metrics collector with thread-safe recording."""

    def __init__(self, max_size: int = 1000) -> None:
        self._max_size = max_size
        self._metrics: deque[RequestMetric] = deque(maxlen=max_size)
        self._snapshots: deque[MetricSnapshot] = deque(maxlen=max_size)
        self._lock = threading.Lock()

    def record(self, metric: RequestMetric) -> None:
        with self._lock:
            self._metrics.append(metric)

    def record_snapshot(self, snapshot: MetricSnapshot) -> None:
        with self._lock:
            self._snapshots.append(snapshot)

    def query(self, component: str | None = None, action: str | None = None) -> list[RequestMetric]:
        with self._lock:
            result = list(self._metrics)
        if component is not None:
            result = [m for m in result if m.component == component]
        if action is not None:
            result = [m for m in result if m.action == action]
        return result

    def query_snapshots(self, name: str | None = None) -> list[MetricSnapshot]:
        with self._lock:
            result = list(self._snapshots)
        if name is not None:
            result = [s for s in result if s.name == name]
        return result

    def count(self, component: str | None = None) -> int:
        return len(self.query(component=component))

    def clear(self) -> None:
        with self._lock:
            self._metrics.clear()
            self._snapshots.clear()


class RequestTracer:
    """Trace/span propagation for request lifecycle tracking."""

    def __init__(self) -> None:
        self._spans: dict[str, SpanRecord] = {}
        self._lock = threading.Lock()

    def new_trace_id(self) -> str:
        return uuid.uuid4().hex

    def new_span_id(self) -> str:
        return uuid.uuid4().hex[:16]

    def start_span(self, name: str, trace_id: str | None = None,
                   parent_span_id: str | None = None) -> tuple[str, str]:
        if trace_id is None:
            trace_id = self.new_trace_id()
        span_id = self.new_span_id()
        span = SpanRecord(
            trace_id=trace_id,
            span_id=span_id,
            parent_span_id=parent_span_id,
            name=name,
            start_time=time.time(),
            end_time=None,
            status="in_progress",
        )
        with self._lock:
            self._spans[span_id] = span
        return trace_id, span_id

    def end_span(self, span_id: str, status: str = "ok") -> SpanRecord | None:
        with self._lock:
            span = self._spans.get(span_id)
            if span is None:
                return None
            span = SpanRecord(
                trace_id=span.trace_id,
                span_id=span.span_id,
                parent_span_id=span.parent_span_id,
                name=span.name,
                start_time=span.start_time,
                end_time=time.time(),
                status=status,
            )
            self._spans[span_id] = span
        return span

    def get_span(self, span_id: str) -> SpanRecord | None:
        with self._lock:
            return self._spans.get(span_id)

    def get_trace_spans(self, trace_id: str) -> list[SpanRecord]:
        with self._lock:
            return [s for s in self._spans.values() if s.trace_id == trace_id]

    def clear(self) -> None:
        with self._lock:
            self._spans.clear()


def setup_structured_logging(
    logger_name: str = "harness_core",
    level: int = logging.INFO,
    handler: logging.Handler | None = None,
) -> logging.Logger:
    """Configure structured JSON logging for the harness_core namespace."""
    logger = logging.getLogger(logger_name)
    logger.setLevel(level)
    if handler is None:
        handler = logging.StreamHandler()
    handler.setFormatter(StructuredFormatter())
    if not any(isinstance(h.formatter, StructuredFormatter) for h in logger.handlers):
        logger.addHandler(handler)
    return logger
