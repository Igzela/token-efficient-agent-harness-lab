"""Bounded WorkCard worker and reviewer adapters for the Steward.

Workers are injected adapters: this module does not select a Provider or
forward credentials.  The parent owns the WorkCard binding, exact-base
identity, path validation, and review-session separation.  Child environments
are derived from the existing fail-closed local-run owner.
"""

from __future__ import annotations

from contextlib import AbstractContextManager
from dataclasses import dataclass
from pathlib import Path
import re
from typing import Any, Callable, Mapping, Protocol

import local_verification
import mission_contract
from review_loop.locking import ChatLock, LockBusy


SHA40 = re.compile(r"^[0-9a-f]{40}$")
SESSION_ID = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$")
SAFE_STATUSES = frozenset({"PASS", "FAIL", "TIMEOUT", "BLOCKED", "OUTCOME_UNKNOWN"})
REVIEW_STATUSES = frozenset({"PASS", "FAIL", "BLOCKED", "OUTCOME_UNKNOWN"})
_CREDENTIAL_MARKERS = (
    "TOKEN",
    "SECRET",
    "PASSWORD",
    "API_KEY",
    "APIKEY",
    "CREDENTIAL",
    "AUTH",
)


class WorkerError(RuntimeError):
    """A worker result or isolated execution boundary is invalid."""


class WorkerUnavailable(WorkerError):
    """No provider-free worker adapter was supplied."""


class PathConflict(WorkerError):
    """A declared WorkCard path lock is currently owned by another card."""


def child_environment(base: Mapping[str, str] | None = None) -> dict[str, str]:
    """Return the existing repository-owned credential-free child environment."""

    import local_run_once

    source = None if base is None else dict(base)
    environment = local_run_once.child_env(source)
    for key in environment:
        if any(marker in key.upper() for marker in _CREDENTIAL_MARKERS):
            raise WorkerError("credential_shaped_child_environment")
    return environment


def select_model_tier(base_tier: str, attempt: int) -> str:
    """Escalate only within T0-T2 as bounded retry pressure increases."""

    if base_tier not in {"T0", "T1", "T2"} or type(attempt) is not int or attempt < 1:
        raise WorkerError("model_tier_invalid")
    tiers = ("T0", "T1", "T2")
    index = min(2, tiers.index(base_tier) + attempt - 1)
    return tiers[index]


def _safe_detail(value: object, field: str) -> str:
    if not isinstance(value, str) or len(value) > 512 or "\n" in value or "\r" in value:
        raise WorkerError(f"{field}_invalid")
    return value


def _safe_session(value: object, field: str) -> str:
    if not isinstance(value, str) or SESSION_ID.fullmatch(value) is None:
        raise WorkerError(f"{field}_invalid")
    return value


def _safe_path(value: object) -> str:
    if not isinstance(value, str) or not value or "\x00" in value:
        raise WorkerError("changed_path_invalid")
    path = Path(value)
    if path.is_absolute() or ".." in path.parts or "\\" in value:
        raise WorkerError("changed_path_unsafe")
    return value


@dataclass(frozen=True)
class WorkerContext:
    mission_id: str
    stage_id: str
    card_id: str
    attempt: int
    model_tier: str
    base_sha: str
    worktree: Path
    allowed_paths: tuple[str, ...]
    environment: Mapping[str, str]

    def __post_init__(self) -> None:
        if not SHA40.fullmatch(self.base_sha):
            raise WorkerError("worker_base_sha_invalid")
        if type(self.attempt) is not int or self.attempt < 1:
            raise WorkerError("worker_attempt_invalid")
        if self.model_tier not in {"T0", "T1", "T2"}:
            raise WorkerError("worker_model_tier_invalid")
        _safe_session(self.mission_id, "mission_id")
        _safe_session(self.stage_id, "stage_id")
        _safe_session(self.card_id, "card_id")
        if not self.allowed_paths:
            raise WorkerError("worker_scope_empty")
        if any(
            not isinstance(key, str)
            or any(marker in key.upper() for marker in _CREDENTIAL_MARKERS)
            for key in self.environment
        ):
            raise WorkerError("credential_shaped_child_environment")
        sanitized = child_environment(dict(self.environment))
        if dict(self.environment) != sanitized:
            raise WorkerError("child_environment_not_allowlisted")


@dataclass(frozen=True)
class WorkerOutcome:
    status: str
    session_id: str
    head_sha: str
    changed_paths: tuple[str, ...]
    detail: str = ""

    def __post_init__(self) -> None:
        if self.status not in SAFE_STATUSES:
            raise WorkerError("worker_status_invalid")
        _safe_session(self.session_id, "worker_session_id")
        if not SHA40.fullmatch(self.head_sha):
            raise WorkerError("worker_head_sha_invalid")
        if len(self.changed_paths) > 100:
            raise WorkerError("worker_changed_paths_too_large")
        paths = tuple(_safe_path(path) for path in self.changed_paths)
        if len(set(paths)) != len(paths):
            raise WorkerError("worker_changed_paths_duplicated")
        _safe_detail(self.detail, "worker_detail")

    def to_wire(self) -> dict[str, Any]:
        return {
            "schema_version": "steward_worker_outcome.v1",
            "status": self.status,
            "session_id": self.session_id,
            "head_sha": self.head_sha,
            "changed_paths": list(self.changed_paths),
            "detail": self.detail,
        }


@dataclass(frozen=True)
class ReviewOutcome:
    status: str
    reviewer_session_id: str
    implementation_session_id: str
    reviewed_head_sha: str
    blockers: tuple[str, ...] = ()
    detail: str = ""

    def __post_init__(self) -> None:
        if self.status not in REVIEW_STATUSES:
            raise WorkerError("review_status_invalid")
        _safe_session(self.reviewer_session_id, "reviewer_session_id")
        _safe_session(self.implementation_session_id, "implementation_session_id")
        if self.reviewer_session_id == self.implementation_session_id:
            raise WorkerError("self_review_forbidden")
        if not SHA40.fullmatch(self.reviewed_head_sha):
            raise WorkerError("review_head_sha_invalid")
        if len(self.blockers) > 16 or any(
            not isinstance(item, str) or len(item) > 256 or "\n" in item
            for item in self.blockers
        ):
            raise WorkerError("review_blockers_invalid")
        _safe_detail(self.detail, "review_detail")

    def to_wire(self) -> dict[str, Any]:
        return {
            "schema_version": "steward_review_outcome.v1",
            "status": self.status,
            "reviewer_session_id": self.reviewer_session_id,
            "implementation_session_id": self.implementation_session_id,
            "reviewed_head_sha": self.reviewed_head_sha,
            "blockers": list(self.blockers),
            "detail": self.detail,
        }


class WorkerAdapter(Protocol):
    def run(self, context: WorkerContext) -> WorkerOutcome:
        """Perform one bounded repository-maintenance attempt."""


class ReviewerAdapter(Protocol):
    def review(
        self, context: WorkerContext, outcome: WorkerOutcome
    ) -> ReviewOutcome:
        """Review one exact implementation head in a separate session."""


class ProviderFreeWorker:
    """Explicit default that cannot accidentally call a Provider."""

    def run(self, context: WorkerContext) -> WorkerOutcome:
        raise WorkerUnavailable("provider_free_worker_not_configured")


class CallableWorker:
    def __init__(self, callback: Callable[[WorkerContext], WorkerOutcome]):
        self.callback = callback

    def run(self, context: WorkerContext) -> WorkerOutcome:
        value = self.callback(context)
        if not isinstance(value, WorkerOutcome):
            raise WorkerError("worker_adapter_return_invalid")
        return value


class CallableReviewer:
    def __init__(self, callback: Callable[[WorkerContext, WorkerOutcome], ReviewOutcome]):
        self.callback = callback

    def review(self, context: WorkerContext, outcome: WorkerOutcome) -> ReviewOutcome:
        value = self.callback(context, outcome)
        if not isinstance(value, ReviewOutcome):
            raise WorkerError("review_adapter_return_invalid")
        return value


def validate_worker_outcome(
    card: mission_contract.WorkCard,
    outcome: WorkerOutcome,
    *,
    expected_head_sha: str,
) -> WorkerOutcome:
    """Bind worker-reported paths and head to the parent WorkCard."""

    if not isinstance(outcome, WorkerOutcome):
        raise WorkerError("worker_outcome_invalid")
    if not SHA40.fullmatch(expected_head_sha):
        raise WorkerError("expected_head_sha_invalid")
    if not SHA40.fullmatch(outcome.head_sha):
        raise WorkerError("worker_head_sha_invalid")
    validate_changed_paths(card, outcome.changed_paths)
    return outcome


def validate_changed_paths(
    card: mission_contract.WorkCard, paths: tuple[str, ...] | list[str]
) -> None:
    """Enforce the WorkCard path boundary on observed or reported paths."""

    for path in paths:
        if any(mission_contract.path_in_scope((forbidden,), path) for forbidden in card.forbidden_paths):
            raise WorkerError("worker_forbidden_path")
        if not mission_contract.path_in_scope(card.allowed_paths, path):
            raise WorkerError("worker_path_outside_card")


class PathLockSet(AbstractContextManager["PathLockSet"]):
    """Acquire all WorkCard path locks in order, or acquire none."""

    def __init__(self, lock_dir: str | Path, paths: tuple[str, ...] | list[str]):
        self.lock_dir = Path(lock_dir)
        self.paths = tuple(sorted(set(paths)))
        if len(self.paths) > 100:
            raise PathConflict("path_lock_invalid")
        try:
            for path in self.paths:
                _safe_path(path)
        except WorkerError as exc:
            raise PathConflict("path_lock_invalid") from exc
        self._locks: list[ChatLock] = []

    def acquire(self) -> "PathLockSet":
        try:
            for path in self.paths:
                lock = ChatLock(self.lock_dir, f"steward-path:{path}")
                lock.acquire()
                self._locks.append(lock)
        except (LockBusy, OSError) as exc:
            self.release()
            raise PathConflict("path_lock_busy") from exc
        return self

    def release(self) -> None:
        for lock in reversed(self._locks):
            lock.release()
        self._locks.clear()

    def __enter__(self) -> "PathLockSet":
        return self.acquire()

    def __exit__(self, *exc: object) -> None:
        self.release()


def run_allowlisted_checks(
    worktree: Path,
    changed_paths: list[str],
    *,
    runner: Callable[..., tuple[int, str, str]] | None = None,
) -> list[dict[str, Any]]:
    """Run repository-owned checks selected from observed changed paths."""

    displays = local_verification.select_issue_checks(changed_paths)
    return local_verification.run_focused_checks(
        worktree, displays, runner=runner
    )


__all__ = [
    "CallableReviewer",
    "CallableWorker",
    "PathConflict",
    "PathLockSet",
    "ProviderFreeWorker",
    "ReviewOutcome",
    "ReviewerAdapter",
    "SAFE_STATUSES",
    "WorkerAdapter",
    "WorkerContext",
    "WorkerError",
    "WorkerOutcome",
    "WorkerUnavailable",
    "child_environment",
    "run_allowlisted_checks",
    "select_model_tier",
    "validate_worker_outcome",
    "validate_changed_paths",
]
