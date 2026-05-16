"""Batch Digest generator stub for Stage 1 Day 5."""

from __future__ import annotations

from dataclasses import dataclass, field

from .projection_store import ProjectionBundle


@dataclass(frozen=True)
class BatchDigest:
    completed_items: tuple[str, ...] = field(default_factory=tuple)
    blocked_or_waiting_approval: tuple[str, ...] = field(default_factory=tuple)
    failed_items: tuple[str, ...] = field(default_factory=tuple)
    handoff_count: int = 0
    resolved_dependency_count: int = 0


def generate_batch_digest(projections: ProjectionBundle) -> BatchDigest:
    """Generate a minimal digest from projections.

    This is intentionally a stub: it summarizes existing projections without
    creating files or implementing the full Stage 1 digest renderer.
    """
    completed: list[str] = []
    blocked: list[str] = []
    failed: list[str] = []

    for item_id, item in sorted(projections.project.items.items()):
        if item.status == "done":
            completed.append(item_id)
        elif item.status == "blocked":
            blocked.append(item_id)
        elif item.status == "failed":
            failed.append(item_id)

    return BatchDigest(
        completed_items=tuple(completed),
        blocked_or_waiting_approval=tuple(blocked),
        failed_items=tuple(failed),
        handoff_count=len(projections.task_queue.handoffs),
        resolved_dependency_count=len(projections.dependencies.resolved),
    )
