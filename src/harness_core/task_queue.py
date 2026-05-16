"""Minimal Task Queue Manager for Stage 1 Day 4."""

from __future__ import annotations

from dataclasses import dataclass, replace

from .project_board import ProjectBoardItem

TASK_STATUSES = {
    "QUEUED",
    "TRIAGED",
    "READY",
    "READY_READONLY",
    "READY_WRITE",
    "RUNNING",
    "WAITING_APPROVAL",
    "PAUSED_BUDGET",
    "WAITING_DEPENDENCY",
    "BLOCKED",
    "BLOCKED_UPSTREAM_FAILED",
    "BLOCKED_APPROVAL",
    "BLOCKED_PROVIDER",
    "COMPLETED",
    "FAILED",
    "CANCELLED_BY_DEPENDENCY",
}

LEGAL_TASK_TRANSITIONS = {
    "QUEUED": {"TRIAGED", "READY", "RUNNING"},
    "TRIAGED": {"READY", "READY_READONLY", "READY_WRITE", "BLOCKED"},
    "READY": {"RUNNING", "BLOCKED"},
    "READY_READONLY": {"RUNNING", "WAITING_DEPENDENCY", "READY_WRITE"},
    "READY_WRITE": {"RUNNING", "BLOCKED"},
    "RUNNING": {
        "WAITING_APPROVAL",
        "PAUSED_BUDGET",
        "WAITING_DEPENDENCY",
        "BLOCKED",
        "BLOCKED_APPROVAL",
        "BLOCKED_PROVIDER",
        "COMPLETED",
        "FAILED",
    },
    "WAITING_APPROVAL": {"RUNNING", "BLOCKED_APPROVAL", "FAILED"},
    "PAUSED_BUDGET": {"RUNNING", "FAILED"},
    "WAITING_DEPENDENCY": {"READY", "BLOCKED_UPSTREAM_FAILED", "CANCELLED_BY_DEPENDENCY"},
    "BLOCKED": {"RUNNING", "FAILED"},
    "BLOCKED_UPSTREAM_FAILED": {"CANCELLED_BY_DEPENDENCY", "FAILED"},
    "BLOCKED_APPROVAL": {"RUNNING", "FAILED"},
    "BLOCKED_PROVIDER": {"RUNNING", "FAILED"},
    "COMPLETED": set(),
    "FAILED": set(),
    "CANCELLED_BY_DEPENDENCY": set(),
}

TASK_TO_PROJECT_BOARD = {
    "QUEUED": ("ready", None),
    "TRIAGED": ("ready", None),
    "READY": ("ready", None),
    "READY_READONLY": ("ready", None),
    "READY_WRITE": ("ready", None),
    "RUNNING": ("running", None),
    "WAITING_APPROVAL": ("blocked", "approval"),
    "PAUSED_BUDGET": ("blocked", "budget"),
    "WAITING_DEPENDENCY": ("blocked", "dependency"),
    "BLOCKED": ("blocked", "generic"),
    "BLOCKED_UPSTREAM_FAILED": ("blocked", "upstream_failed"),
    "BLOCKED_APPROVAL": ("blocked", "approval"),
    "BLOCKED_PROVIDER": ("blocked", "provider"),
    "COMPLETED": ("review", None),
    "FAILED": ("failed", None),
    "CANCELLED_BY_DEPENDENCY": ("failed", None),
}


@dataclass(frozen=True)
class TaskQueueEntry:
    task_id: str
    item_id: str
    status: str
    handoff_id: str
    scheduling_policy: str


@dataclass(frozen=True)
class HandoffResult:
    task: TaskQueueEntry
    accepted: bool


@dataclass(frozen=True)
class TaskTransitionResult:
    task: TaskQueueEntry
    previous_status: str
    new_status: str
    project_board_status: str
    blocked_reason: str | None


def receive_handoff(
    item: ProjectBoardItem,
    handoff_id: str,
    scheduling_policy: str = "sequential",
    task_id: str | None = None,
) -> HandoffResult:
    """Accept a ready project item into the sequential task queue."""
    if item.status != "ready":
        raise ValueError("handoff only accepts project board items in ready status")
    if scheduling_policy != "sequential":
        raise ValueError("only sequential scheduling is supported in Stage 1 Day 4")
    if not handoff_id:
        raise ValueError("handoff_id is required")

    task = TaskQueueEntry(
        task_id=task_id or f"task_for_{item.item_id}",
        item_id=item.item_id,
        status="QUEUED",
        handoff_id=handoff_id,
        scheduling_policy=scheduling_policy,
    )
    return HandoffResult(task=task, accepted=True)


def transition_task(task: TaskQueueEntry, new_status: str) -> TaskTransitionResult:
    """Validate and apply one task queue status transition."""
    _validate_task_status(task.status)
    _validate_task_status(new_status)
    if new_status not in LEGAL_TASK_TRANSITIONS[task.status]:
        raise ValueError(f"illegal task queue transition: {task.status} -> {new_status}")

    updated = replace(task, status=new_status)
    board_status, blocked_reason = map_task_status_to_project_board(new_status)
    return TaskTransitionResult(
        task=updated,
        previous_status=task.status,
        new_status=new_status,
        project_board_status=board_status,
        blocked_reason=blocked_reason,
    )


def map_task_status_to_project_board(task_status: str) -> tuple[str, str | None]:
    """Map Task Queue status to Project Board status and blocked reason."""
    _validate_task_status(task_status)
    return TASK_TO_PROJECT_BOARD[task_status]


def _validate_task_status(status: str) -> None:
    if status not in TASK_STATUSES:
        raise ValueError(f"unknown task queue status: {status}")
