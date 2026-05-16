"""Deterministic Kernel skeleton for Stage 1 Week 3."""

from __future__ import annotations

from pathlib import Path
from typing import Any

from .event_store import (
    EventStore,
    ReplayPreflightReport,
    replay_preflight,
    validate_jsonl_file,
)
from .errors import ReplayPreflightError
from .projection_store import (
    ProjectStateProjection,
    ProjectionBundle,
    TaskQueueProjection,
    replay_all,
    replay_project_state,
    replay_task_queue_state,
)

_FORBIDDEN_APPEND_SUFFIXES = {
    Path("docs/stage0/events.jsonl"),
    Path("tests/fixtures/stage0_events_with_line17_issue.jsonl"),
    Path("tests/fixtures/stage0_events_sanitized.jsonl"),
}


class Kernel:
    """Thin coordination wrapper over Event Store and projections."""

    def __init__(self, event_log_path: str | Path):
        self.event_log_path = Path(event_log_path)

    def validate(self) -> ReplayPreflightReport:
        report = replay_preflight(self.event_log_path)
        if not report.ok:
            raise ReplayPreflightError("event log failed replay preflight")
        return report

    def project_state(self) -> ProjectStateProjection:
        self.validate()
        return replay_project_state(self.event_log_path)

    def task_queue_state(self) -> TaskQueueProjection:
        self.validate()
        return replay_task_queue_state(self.event_log_path)

    def projections(self) -> ProjectionBundle:
        self.validate()
        return replay_all(self.event_log_path)

    def append_project_event(self, event: dict[str, Any]) -> None:
        self._reject_forbidden_append_target()
        if self.event_log_path.exists():
            self.validate()
        EventStore(self.event_log_path).append_event(event)

        report = validate_jsonl_file(self.event_log_path)
        if not report.ok:
            raise ReplayPreflightError("event append produced invalid JSONL")

    def _reject_forbidden_append_target(self) -> None:
        normalized = Path(*self.event_log_path.parts[-3:])
        for forbidden in _FORBIDDEN_APPEND_SUFFIXES:
            if self.event_log_path == forbidden or normalized == forbidden:
                raise PermissionError(f"refusing to append to protected event log: {self.event_log_path}")
