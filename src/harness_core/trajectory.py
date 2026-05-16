"""Trajectory Monitor for Stage 2 — structural anomaly detection."""

from __future__ import annotations

import json
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass(frozen=True)
class TrajectoryAnomaly:
    anomaly_type: str
    item_id: str | None = None
    event_ids: tuple[str, ...] = ()
    message: str = ""
    severity: str = "info"


@dataclass(frozen=True)
class TrajectoryReport:
    ok: bool = True
    anomalies: tuple[TrajectoryAnomaly, ...] = ()
    retry_count: int = 0
    loop_detected: bool = False
    missing_handoff_count: int = 0


_DEFAULT_FAILURE_THRESHOLD = 3
_DEFAULT_LOOP_THRESHOLD = 2


class TrajectoryMonitor:
    """Detect trajectory anomalies from event streams. No semantic model judge."""

    def __init__(
        self,
        failure_threshold: int = _DEFAULT_FAILURE_THRESHOLD,
        loop_threshold: int = _DEFAULT_LOOP_THRESHOLD,
    ):
        self.failure_threshold = failure_threshold
        self.loop_threshold = loop_threshold

    def analyze_project_stream(self, event_log_path: str | Path) -> TrajectoryReport:
        path = Path(event_log_path)
        if not path.exists():
            return TrajectoryReport(
                ok=False,
                anomalies=(
                    TrajectoryAnomaly(
                        anomaly_type="missing_file",
                        message=f"event log not found: {path}",
                        severity="error",
                    ),
                ),
            )

        events = self._load_events(path)
        if events is None:
            return TrajectoryReport(
                ok=False,
                anomalies=(
                    TrajectoryAnomaly(
                        anomaly_type="malformed_stream",
                        message="event log contains invalid JSON",
                        severity="error",
                    ),
                ),
            )

        anomalies: list[TrajectoryAnomaly] = []

        self._check_repeated_failures(events, anomalies)
        self._check_loops(events, anomalies)
        self._check_missing_handoffs(events, anomalies)

        return TrajectoryReport(
            ok=len([a for a in anomalies if a.severity == "error"]) == 0,
            anomalies=tuple(anomalies),
            loop_detected=any(a.anomaly_type == "loop_detected" for a in anomalies),
            missing_handoff_count=sum(
                1 for a in anomalies if a.anomaly_type == "missing_handoff"
            ),
        )

    def analyze_task_stream(
        self, task_events_path: str | Path, item_id: str
    ) -> TrajectoryReport:
        path = Path(task_events_path)
        if not path.exists():
            return TrajectoryReport(
                ok=False,
                anomalies=(
                    TrajectoryAnomaly(
                        anomaly_type="missing_file",
                        message=f"task events not found: {path}",
                        severity="error",
                    ),
                ),
            )

        events = self._load_events(path)
        if events is None:
            return TrajectoryReport(
                ok=False,
                anomalies=(
                    TrajectoryAnomaly(
                        anomaly_type="malformed_stream",
                        message="task events contain invalid JSON",
                        severity="error",
                    ),
                ),
            )

        anomalies: list[TrajectoryAnomaly] = []
        self._check_retries_from_events(events, anomalies)

        return TrajectoryReport(
            ok=len([a for a in anomalies if a.severity == "error"]) == 0,
            anomalies=tuple(anomalies),
            retry_count=sum(a.severity == "warn" for a in anomalies if a.anomaly_type == "excessive_retry"),
        )

    def _load_events(self, path: Path) -> list[dict[str, Any]] | None:
        events: list[dict[str, Any]] = []
        try:
            with path.open("r", encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if not line:
                        continue
                    events.append(json.loads(line))
        except (json.JSONDecodeError, UnicodeDecodeError):
            return None
        return events

    def _check_repeated_failures(
        self,
        events: list[dict[str, Any]],
        anomalies: list[TrajectoryAnomaly],
    ) -> None:
        failure_counts: Counter[str] = Counter()
        failure_event_ids: dict[str, list[str]] = defaultdict(list)

        for event in events:
            if event.get("event_type") != "project_item_state_changed":
                continue
            payload = event.get("payload", {})
            if payload.get("new_status") == "failed":
                item_id = payload.get("item_id", "<unknown>")
                failure_counts[item_id] += 1
                failure_event_ids[item_id].append(event.get("event_id", ""))

        for item_id, count in failure_counts.items():
            if count >= self.failure_threshold:
                anomalies.append(
                    TrajectoryAnomaly(
                        anomaly_type="repeated_failure",
                        item_id=item_id,
                        event_ids=tuple(failure_event_ids[item_id]),
                        message=f"item {item_id} failed {count} times (threshold: {self.failure_threshold})",
                        severity="error",
                    )
                )

    def _check_loops(
        self,
        events: list[dict[str, Any]],
        anomalies: list[TrajectoryAnomaly],
    ) -> None:
        transitions: dict[str, list[tuple[str, str, str]]] = defaultdict(list)

        for event in events:
            if event.get("event_type") != "project_item_state_changed":
                continue
            payload = event.get("payload", {})
            item_id = payload.get("item_id", "<unknown>")
            prev = payload.get("previous_status", "")
            new = payload.get("new_status", "")
            event_id = event.get("event_id", "")
            transitions[item_id].append((prev, new, event_id))

        for item_id, trans_list in transitions.items():
            if len(trans_list) < 4:
                continue
            # Check for repeated transition pairs
            pair_counts: Counter[tuple[str, str]] = Counter()
            for prev, new, _ in trans_list:
                pair_counts[(prev, new)] += 1

            for (prev, new), count in pair_counts.items():
                if count >= self.loop_threshold:
                    event_ids = tuple(
                        eid for p, n, eid in trans_list if p == prev and n == new
                    )
                    anomalies.append(
                        TrajectoryAnomaly(
                            anomaly_type="loop_detected",
                            item_id=item_id,
                            event_ids=event_ids,
                            message=f"item {item_id}: transition {prev}->{new} repeated {count} times",
                            severity="error",
                        )
                    )

    def _check_missing_handoffs(
        self,
        events: list[dict[str, Any]],
        anomalies: list[TrajectoryAnomaly],
    ) -> None:
        handoff_items: set[str] = set()
        running_items: dict[str, str] = {}  # item_id -> event_id

        for event in events:
            event_type = event.get("event_type")
            payload = event.get("payload", {})

            if event_type == "project_to_queue_handoff_created":
                handoff_items.add(payload.get("item_id", ""))

            if event_type == "project_item_state_changed" and payload.get("new_status") == "running":
                item_id = payload.get("item_id", "")
                running_items[item_id] = event.get("event_id", "")

        for item_id, event_id in running_items.items():
            if item_id not in handoff_items:
                anomalies.append(
                    TrajectoryAnomaly(
                        anomaly_type="missing_handoff",
                        item_id=item_id,
                        event_ids=(event_id,),
                        message=f"item {item_id} reached running without handoff event",
                        severity="warn",
                    )
                )

    def _check_retries_from_events(
        self,
        events: list[dict[str, Any]],
        anomalies: list[TrajectoryAnomaly],
    ) -> None:
        for event in events:
            payload = event.get("payload", {})
            retry_count = payload.get("retry_count", 0)
            if isinstance(retry_count, int) and retry_count >= 3:
                anomalies.append(
                    TrajectoryAnomaly(
                        anomaly_type="excessive_retry",
                        item_id=payload.get("item_id"),
                        event_ids=(event.get("event_id", ""),),
                        message=f"retry_count={retry_count} exceeds threshold",
                        severity="warn",
                    )
                )
