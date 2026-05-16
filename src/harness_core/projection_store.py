"""Projection Store for validated Stage 1 event streams."""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from .errors import ReplayPreflightError
from .event_store import ValidationIssue, replay_preflight

PROJECT_ITEM_STATE_CHANGED = "project_item_state_changed"
PROJECT_TO_QUEUE_HANDOFF_CREATED = "project_to_queue_handoff_created"
PROJECT_DEPENDENCY_RESOLVED = "project_dependency_resolved"
SUPPORTED_EVENT_TYPES = {
    PROJECT_ITEM_STATE_CHANGED,
    PROJECT_TO_QUEUE_HANDOFF_CREATED,
    PROJECT_DEPENDENCY_RESOLVED,
}


@dataclass(frozen=True)
class ProjectItemState:
    item_id: str
    status: str
    previous_status: str | None
    reason: str | None
    last_event_id: str
    last_updated: str


@dataclass
class ProjectStateProjection:
    items: dict[str, ProjectItemState] = field(default_factory=dict)
    warnings: list[ValidationIssue] = field(default_factory=list)


@dataclass(frozen=True)
class HandoffRecord:
    handoff_id: str
    item_id: str
    scheduling_policy: str
    event_id: str
    timestamp: str


@dataclass
class TaskQueueProjection:
    handoffs: list[HandoffRecord] = field(default_factory=list)
    warnings: list[ValidationIssue] = field(default_factory=list)


@dataclass(frozen=True)
class DependencyResolvedRecord:
    edge_id: str
    from_node: str
    to_node: str
    dependency_type: str
    resolution: str | None
    event_id: str
    timestamp: str


@dataclass
class DependencyProjection:
    resolved: list[DependencyResolvedRecord] = field(default_factory=list)
    warnings: list[ValidationIssue] = field(default_factory=list)


@dataclass
class ProjectionBundle:
    project: ProjectStateProjection
    task_queue: TaskQueueProjection
    dependencies: DependencyProjection
    warnings: list[ValidationIssue] = field(default_factory=list)


def replay_project_state(path: str | Path) -> ProjectStateProjection:
    events, warnings = _load_events_after_preflight(path)
    projection = ProjectStateProjection(warnings=list(warnings))
    for event in events:
        if event["event_type"] != PROJECT_ITEM_STATE_CHANGED:
            continue
        payload = event["payload"]
        item_id = payload.get("item_id")
        new_status = payload.get("new_status")
        if not item_id or not new_status:
            projection.warnings.append(_missing_payload_warning(event, "item_id/new_status"))
            continue
        projection.items[item_id] = ProjectItemState(
            item_id=item_id,
            status=new_status,
            previous_status=payload.get("previous_status"),
            reason=payload.get("reason"),
            last_event_id=event["event_id"],
            last_updated=event["timestamp"],
        )
    return projection


def replay_task_queue_state(path: str | Path) -> TaskQueueProjection:
    events, warnings = _load_events_after_preflight(path)
    projection = TaskQueueProjection(warnings=list(warnings))
    for event in events:
        if event["event_type"] != PROJECT_TO_QUEUE_HANDOFF_CREATED:
            continue
        payload = event["payload"]
        required = ("handoff_id", "item_id", "scheduling_policy")
        if any(not payload.get(field_name) for field_name in required):
            projection.warnings.append(_missing_payload_warning(event, "/".join(required)))
            continue
        projection.handoffs.append(
            HandoffRecord(
                handoff_id=payload["handoff_id"],
                item_id=payload["item_id"],
                scheduling_policy=payload["scheduling_policy"],
                event_id=event["event_id"],
                timestamp=event["timestamp"],
            )
        )
    return projection


def replay_dependency_state(path: str | Path) -> DependencyProjection:
    events, warnings = _load_events_after_preflight(path)
    projection = DependencyProjection(warnings=list(warnings))
    for event in events:
        if event["event_type"] != PROJECT_DEPENDENCY_RESOLVED:
            continue
        payload = event["payload"]
        required = ("edge_id", "from_node", "to_node", "dependency_type")
        if any(not payload.get(field_name) for field_name in required):
            projection.warnings.append(_missing_payload_warning(event, "/".join(required)))
            continue
        projection.resolved.append(
            DependencyResolvedRecord(
                edge_id=payload["edge_id"],
                from_node=payload["from_node"],
                to_node=payload["to_node"],
                dependency_type=payload["dependency_type"],
                resolution=payload.get("resolution"),
                event_id=event["event_id"],
                timestamp=event["timestamp"],
            )
        )
    return projection


def replay_all(path: str | Path) -> ProjectionBundle:
    events, warnings = _load_events_after_preflight(path)
    project = _project_state_from_events(events, warnings)
    task_queue = _task_queue_from_events(events, warnings)
    dependencies = _dependency_state_from_events(events, warnings)
    return ProjectionBundle(
        project=project,
        task_queue=task_queue,
        dependencies=dependencies,
        warnings=list(warnings),
    )


def _load_events_after_preflight(path: str | Path) -> tuple[list[dict[str, Any]], list[ValidationIssue]]:
    report = replay_preflight(path)
    if not report.ok:
        raise ReplayPreflightError("projection replay blocked by replay preflight errors")

    events: list[dict[str, Any]] = []
    warnings = list(report.warnings)
    with Path(path).open("r", encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, start=1):
            event = json.loads(line)
            if event["event_type"] not in SUPPORTED_EVENT_TYPES:
                warnings.append(
                    ValidationIssue(
                        line_number=line_number,
                        error_type="UnknownEventTypeWarning",
                        message=f"ignored unsupported event_type: {event['event_type']}",
                    )
                )
            events.append(event)
    return events, warnings


def _project_state_from_events(
    events: list[dict[str, Any]], warnings: list[ValidationIssue]
) -> ProjectStateProjection:
    projection = ProjectStateProjection(warnings=list(warnings))
    for event in events:
        if event["event_type"] == PROJECT_ITEM_STATE_CHANGED:
            payload = event["payload"]
            item_id = payload.get("item_id")
            new_status = payload.get("new_status")
            if item_id and new_status:
                projection.items[item_id] = ProjectItemState(
                    item_id=item_id,
                    status=new_status,
                    previous_status=payload.get("previous_status"),
                    reason=payload.get("reason"),
                    last_event_id=event["event_id"],
                    last_updated=event["timestamp"],
                )
    return projection


def _task_queue_from_events(
    events: list[dict[str, Any]], warnings: list[ValidationIssue]
) -> TaskQueueProjection:
    projection = TaskQueueProjection(warnings=list(warnings))
    for event in events:
        if event["event_type"] == PROJECT_TO_QUEUE_HANDOFF_CREATED:
            payload = event["payload"]
            if payload.get("handoff_id") and payload.get("item_id") and payload.get("scheduling_policy"):
                projection.handoffs.append(
                    HandoffRecord(
                        handoff_id=payload["handoff_id"],
                        item_id=payload["item_id"],
                        scheduling_policy=payload["scheduling_policy"],
                        event_id=event["event_id"],
                        timestamp=event["timestamp"],
                    )
                )
    return projection


def _dependency_state_from_events(
    events: list[dict[str, Any]], warnings: list[ValidationIssue]
) -> DependencyProjection:
    projection = DependencyProjection(warnings=list(warnings))
    for event in events:
        if event["event_type"] == PROJECT_DEPENDENCY_RESOLVED:
            payload = event["payload"]
            if (
                payload.get("edge_id")
                and payload.get("from_node")
                and payload.get("to_node")
                and payload.get("dependency_type")
            ):
                projection.resolved.append(
                    DependencyResolvedRecord(
                        edge_id=payload["edge_id"],
                        from_node=payload["from_node"],
                        to_node=payload["to_node"],
                        dependency_type=payload["dependency_type"],
                        resolution=payload.get("resolution"),
                        event_id=event["event_id"],
                        timestamp=event["timestamp"],
                    )
                )
    return projection


def _missing_payload_warning(event: dict[str, Any], missing: str) -> ValidationIssue:
    return ValidationIssue(
        line_number=None,
        error_type="ProjectionPayloadWarning",
        message=f"event {event['event_id']} missing projection payload field(s): {missing}",
    )
