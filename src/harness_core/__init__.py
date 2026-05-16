"""Core primitives for the token-efficient agent harness."""

from .event_store import (
    EventStore,
    ReplayPreflightReport,
    ValidationIssue,
    ValidationReport,
    load_event_ids,
    replay_preflight,
    validate_jsonl_file,
)
from .event_schema import stable_idempotency_hash, validate_event
from .projection_store import (
    DependencyProjection,
    DependencyResolvedRecord,
    HandoffRecord,
    ProjectItemState,
    ProjectStateProjection,
    ProjectionBundle,
    TaskQueueProjection,
    replay_all,
    replay_dependency_state,
    replay_project_state,
    replay_task_queue_state,
)
from .project_board import (
    FinalGateResult,
    ProjectBoardItem,
    TransitionResult,
    check_allowed_files,
    complete_task_to_review,
    final_gate,
    transition_item,
)

__all__ = [
    "DependencyProjection",
    "DependencyResolvedRecord",
    "EventStore",
    "FinalGateResult",
    "HandoffRecord",
    "ProjectItemState",
    "ProjectBoardItem",
    "ProjectStateProjection",
    "ProjectionBundle",
    "ReplayPreflightReport",
    "TaskQueueProjection",
    "TransitionResult",
    "check_allowed_files",
    "complete_task_to_review",
    "final_gate",
    "ValidationIssue",
    "ValidationReport",
    "load_event_ids",
    "replay_all",
    "replay_dependency_state",
    "replay_preflight",
    "replay_project_state",
    "replay_task_queue_state",
    "stable_idempotency_hash",
    "transition_item",
    "validate_event",
    "validate_jsonl_file",
]
