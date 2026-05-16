"""Minimal Project Board Manager for Stage 1 Day 3."""

from __future__ import annotations

from dataclasses import dataclass, field, replace

PROJECT_STATUSES = {"todo", "ready", "running", "blocked", "review", "done", "failed"}
LEGAL_TRANSITIONS = {
    "todo": {"ready"},
    "ready": {"running"},
    "running": {"blocked", "review"},
    "blocked": {"running", "failed"},
    "review": {"done", "review", "failed"},
    "done": set(),
    "failed": set(),
}


@dataclass(frozen=True)
class ProjectBoardItem:
    item_id: str
    status: str
    allowed_files: tuple[str, ...] = ()
    blocked_reason: str | None = None
    last_reason: str | None = None


@dataclass(frozen=True)
class TransitionResult:
    item: ProjectBoardItem
    previous_status: str
    new_status: str
    reason: str


@dataclass(frozen=True)
class FinalGateResult:
    item: ProjectBoardItem
    decision: str
    reason: str


@dataclass(frozen=True)
class AllowedFilesCheck:
    ok: bool
    missing_files: tuple[str, ...] = field(default_factory=tuple)


def transition_item(
    item: ProjectBoardItem,
    new_status: str,
    reason: str,
    blocked_reason: str | None = None,
) -> TransitionResult:
    """Validate and apply one Project Board status transition."""
    _validate_status(item.status)
    _validate_status(new_status)
    if new_status not in LEGAL_TRANSITIONS[item.status]:
        raise ValueError(f"illegal project board transition: {item.status} -> {new_status}")

    updated = replace(
        item,
        status=new_status,
        blocked_reason=blocked_reason if new_status == "blocked" else None,
        last_reason=reason,
    )
    return TransitionResult(
        item=updated,
        previous_status=item.status,
        new_status=new_status,
        reason=reason,
    )


def complete_task_to_review(item: ProjectBoardItem, reason: str) -> TransitionResult:
    """Map task completion to review, preserving task completed != item done."""
    if item.status != "running":
        raise ValueError("task completion can only move a running item to review")
    return transition_item(item, "review", reason)


def final_gate(item: ProjectBoardItem, decision: str, reason: str) -> FinalGateResult:
    """Apply Final Gate decision from review state."""
    if item.status != "review":
        raise ValueError("Final Gate can only run for items in review")
    if decision == "pass":
        result = transition_item(item, "done", reason)
    elif decision == "pass_with_notes":
        result = transition_item(item, "review", reason)
    elif decision == "fail":
        result = transition_item(item, "failed", reason)
    else:
        raise ValueError("Final Gate decision must be pass, pass_with_notes, or fail")
    return FinalGateResult(item=result.item, decision=decision, reason=reason)


def check_allowed_files(
    allowed_files: list[str] | tuple[str, ...], required_files: list[str] | tuple[str, ...]
) -> AllowedFilesCheck:
    """Check whether a task's allowed_files covers all planned writes."""
    allowed = set(allowed_files)
    missing = tuple(file_path for file_path in required_files if file_path not in allowed)
    return AllowedFilesCheck(ok=not missing, missing_files=missing)


def _validate_status(status: str) -> None:
    if status not in PROJECT_STATUSES:
        raise ValueError(f"unknown project board status: {status}")
