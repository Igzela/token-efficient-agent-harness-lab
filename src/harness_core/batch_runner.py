"""Deterministic Batch Runner skeleton for Stage 1 Week 3."""

from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Any

from .digest import BatchDigest, generate_batch_digest
from .event_schema import validate_event
from .kernel import Kernel
from .projection_store import ProjectItemState

EVENT_ID_PATTERN = re.compile(r"^(?P<prefix>.*_)(?P<number>\d+)$")


@dataclass(frozen=True)
class RunResult:
    item_id: str
    appended_event_ids: tuple[str, ...]
    digest: BatchDigest


class BatchRunner:
    """Simulate one deterministic local batch step."""

    def __init__(self, kernel: Kernel):
        self.kernel = kernel

    def list_ready_items(self) -> list[ProjectItemState]:
        project = self.kernel.project_state()
        task_queue = self.kernel.task_queue_state()
        handed_off_item_ids = {handoff.item_id for handoff in task_queue.handoffs}
        return [
            item
            for item_id, item in sorted(project.items.items())
            if item.status == "ready" and item_id not in handed_off_item_ids
        ]

    def run_one_ready_item(self, item_id: str) -> RunResult:
        self.kernel.validate()
        ready_items = {item.item_id: item for item in self.list_ready_items()}
        if item_id not in ready_items:
            raise ValueError(f"item is not ready or already handed off: {item_id}")

        planned_events = self._plan_events(item_id)
        for event in planned_events:
            validate_event(event)

        for event in planned_events:
            self.kernel.append_project_event(event)

        digest = generate_batch_digest(self.kernel.projections())
        return RunResult(
            item_id=item_id,
            appended_event_ids=tuple(event["event_id"] for event in planned_events),
            digest=digest,
        )

    def _plan_events(self, item_id: str) -> list[dict[str, Any]]:
        event_ids = _next_event_ids(self.kernel.event_log_path, count=3)
        timestamp_base = "2026-05-16T00:00:"
        return [
            _project_state_event(
                event_id=event_ids[0],
                timestamp=f"{timestamp_base}00+08:00",
                item_id=item_id,
                previous_status="ready",
                new_status="running",
                reason="BatchRunner deterministic skeleton: ready item started",
                idempotency_key=f"{item_id}:ready:running:v1",
                parent_event_id=None,
            ),
            _handoff_event(
                event_id=event_ids[1],
                timestamp=f"{timestamp_base}01+08:00",
                item_id=item_id,
                handoff_id=f"handoff_{item_id}",
                idempotency_key=f"{item_id}:handoff:v1",
                parent_event_id=event_ids[0],
            ),
            _project_state_event(
                event_id=event_ids[2],
                timestamp=f"{timestamp_base}02+08:00",
                item_id=item_id,
                previous_status="running",
                new_status="review",
                reason="BatchRunner deterministic skeleton: simulated task completed",
                idempotency_key=f"{item_id}:running:review:v1",
                parent_event_id=event_ids[1],
            ),
        ]


def _next_event_ids(event_log_path, count: int) -> list[str]:
    max_suffix = 0
    prefix = "evt_"
    width = 6
    if event_log_path.exists():
        for line in event_log_path.read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            event_id = _extract_event_id(line)
            match = EVENT_ID_PATTERN.match(event_id)
            if not match:
                continue
            number_text = match.group("number")
            number = int(number_text)
            if number >= max_suffix:
                max_suffix = number
                prefix = match.group("prefix")
                width = len(number_text)

    return [f"{prefix}{number:0{width}d}" for number in range(max_suffix + 1, max_suffix + count + 1)]


def _extract_event_id(line: str) -> str:
    import json

    return str(json.loads(line)["event_id"])


def _base_event(
    event_id: str,
    timestamp: str,
    event_type: str,
    payload: dict[str, Any],
    idempotency_key: str,
    parent_event_id: str | None,
) -> dict[str, Any]:
    return {
        "event_id": event_id,
        "schema_version": "event.v1",
        "event_type": event_type,
        "timestamp": timestamp,
        "producer": {
            "component_id": "kernel_batch_runner",
            "component_type": "deterministic_skeleton",
        },
        "correlation": {
            "batch_id": "batch_stage1_week3",
            "project_id": "proj_stage1_week3",
            "run_id": "run_stage1_week3",
        },
        "severity": "info",
        "payload": payload,
        "idempotency_key": idempotency_key,
        "parent_event_id": parent_event_id,
    }


def _project_state_event(
    event_id: str,
    timestamp: str,
    item_id: str,
    previous_status: str,
    new_status: str,
    reason: str,
    idempotency_key: str,
    parent_event_id: str | None,
) -> dict[str, Any]:
    return _base_event(
        event_id=event_id,
        timestamp=timestamp,
        event_type="project_item_state_changed",
        payload={
            "project_id": "proj_stage1_week3",
            "board_version": 1,
            "item_id": item_id,
            "previous_status": previous_status,
            "new_status": new_status,
            "reason": reason,
        },
        idempotency_key=idempotency_key,
        parent_event_id=parent_event_id,
    )


def _handoff_event(
    event_id: str,
    timestamp: str,
    item_id: str,
    handoff_id: str,
    idempotency_key: str,
    parent_event_id: str,
) -> dict[str, Any]:
    return _base_event(
        event_id=event_id,
        timestamp=timestamp,
        event_type="project_to_queue_handoff_created",
        payload={
            "project_id": "proj_stage1_week3",
            "item_id": item_id,
            "handoff_id": handoff_id,
            "scheduling_policy": "sequential",
        },
        idempotency_key=idempotency_key,
        parent_event_id=parent_event_id,
    )
