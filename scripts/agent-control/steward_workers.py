"""Bounded WorkCard worker and reviewer adapters for the Steward.

Workers are injected adapters: this module does not select a Provider or
forward credentials.  The parent owns the WorkCard binding, exact-base
identity, path validation, and review-session separation.  Child environments
are derived from the existing fail-closed local-run owner.
"""

from __future__ import annotations

from contextlib import AbstractContextManager
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import re
import subprocess
from typing import Any, Callable, Mapping, Protocol

import local_verification
import mission_contract
from review_loop.locking import ChatLock, LockBusy
import state_manager


SHA40 = re.compile(r"^[0-9a-f]{40}$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")
SESSION_ID = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$")
SAFE_STATUSES = frozenset({"PASS", "FAIL", "TIMEOUT", "BLOCKED", "OUTCOME_UNKNOWN"})
REVIEW_STATUSES = frozenset({"PASS", "FAIL", "BLOCKED", "OUTCOME_UNKNOWN"})
REVIEW_AXES = frozenset({"standards", "spec"})
REVIEW_MODES = frozenset({"full", "repair_verification"})
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


def review_range_digest(base_sha: str, head_sha: str) -> str:
    """Digest the exact base...head range a reviewer claims to inspect."""

    if not SHA40.fullmatch(base_sha) or not SHA40.fullmatch(head_sha):
        raise WorkerError("review_range_invalid")
    return hashlib.sha256(f"{base_sha}...{head_sha}".encode("ascii")).hexdigest()


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
    steps: tuple[str, ...]
    focused_tests: tuple[str, ...]
    negative_checks: tuple[str, ...]
    expected_evidence: tuple[str, ...]
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

    @classmethod
    def from_wire(cls, value: object) -> "WorkerOutcome":
        if not isinstance(value, dict) or set(value) != {
            "schema_version", "status", "session_id", "head_sha", "changed_paths", "detail"
        } or value.get("schema_version") != "steward_worker_outcome.v1":
            raise WorkerError("worker_outcome_wire_invalid")
        paths = value.get("changed_paths")
        if not isinstance(paths, list) or not all(isinstance(path, str) for path in paths):
            raise WorkerError("worker_outcome_wire_invalid")
        return cls(
            value["status"], value["session_id"], value["head_sha"], tuple(paths), value["detail"]
        )


@dataclass(frozen=True)
class ReviewOutcome:
    status: str
    reviewer_session_id: str
    implementation_session_id: str
    reviewed_head_sha: str
    blockers: tuple[str, ...] = ()
    detail: str = ""
    reviewed_base_sha: str = ""
    reviewed_range_sha256: str = ""
    review_axes: tuple[str, ...] = ()
    review_round: int = 1
    review_mode: str = "full"
    review_receipt_sha256: str = ""

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
        if self.status == "PASS" and self.blockers:
            raise WorkerError("review_pass_has_blockers")
        _safe_detail(self.detail, "review_detail")
        if not SHA40.fullmatch(self.reviewed_base_sha):
            raise WorkerError("review_base_sha_invalid")
        if not SHA256.fullmatch(self.reviewed_range_sha256):
            raise WorkerError("review_range_digest_invalid")
        if (
            not self.review_axes
            or len(self.review_axes) != len(set(self.review_axes))
            or set(self.review_axes) != REVIEW_AXES
        ):
            raise WorkerError("review_axes_invalid")
        if type(self.review_round) is not int or not 1 <= self.review_round <= 2:
            raise WorkerError("review_round_invalid")
        if self.review_mode not in REVIEW_MODES:
            raise WorkerError("review_mode_invalid")
        if not SHA256.fullmatch(self.review_receipt_sha256):
            raise WorkerError("review_receipt_digest_invalid")

    def to_wire(self) -> dict[str, Any]:
        return {
            "schema_version": "steward_review_outcome.v1",
            "status": self.status,
            "reviewer_session_id": self.reviewer_session_id,
            "implementation_session_id": self.implementation_session_id,
            "reviewed_head_sha": self.reviewed_head_sha,
            "blockers": list(self.blockers),
            "detail": self.detail,
            "reviewed_base_sha": self.reviewed_base_sha,
            "reviewed_range_sha256": self.reviewed_range_sha256,
            "review_axes": list(self.review_axes),
            "review_round": self.review_round,
            "review_mode": self.review_mode,
            "review_receipt_sha256": self.review_receipt_sha256,
        }

    @classmethod
    def from_wire(cls, value: object) -> "ReviewOutcome":
        if not isinstance(value, dict) or set(value) != {
            "schema_version", "status", "reviewer_session_id", "implementation_session_id",
            "reviewed_head_sha", "blockers", "detail", "reviewed_base_sha",
            "reviewed_range_sha256", "review_axes", "review_round", "review_mode",
            "review_receipt_sha256",
        } or value.get("schema_version") != "steward_review_outcome.v1":
            raise WorkerError("review_outcome_wire_invalid")
        blockers = value.get("blockers")
        axes = value.get("review_axes")
        if (
            not isinstance(blockers, list)
            or not all(isinstance(item, str) for item in blockers)
            or not isinstance(axes, list)
            or not all(isinstance(item, str) for item in axes)
        ):
            raise WorkerError("review_outcome_wire_invalid")
        return cls(
            value["status"], value["reviewer_session_id"], value["implementation_session_id"],
            value["reviewed_head_sha"], tuple(blockers), value["detail"], value["reviewed_base_sha"],
            value["reviewed_range_sha256"], tuple(axes), value["review_round"], value["review_mode"],
            value["review_receipt_sha256"],
        )


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


def _head_or_base(context: WorkerContext) -> str:
    try:
        result = subprocess.run(
            ["git", "rev-parse", "--verify", "HEAD"],
            cwd=context.worktree,
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return context.base_sha
    head = result.stdout.strip()
    return head if SHA40.fullmatch(head) else context.base_sha


def _validate_command(command: object) -> list[str]:
    if (
        not isinstance(command, (list, tuple))
        or not command
        or len(command) > 32
        or not all(isinstance(item, str) and item and len(item) <= 512 and "\x00" not in item for item in command)
    ):
        raise WorkerError("worker_command_invalid")
    return list(command)


class BoundedProcessWorker:
    """Run one operator-supplied provider-free worker in an isolated child.

    The command is argv-only and receives the existing credential-free child
    environment. Its bounded JSON stdout is an untrusted WorkerOutcome; all
    head, path, clean-worktree, verification, and review checks remain owned
    by the parent Steward.
    """

    def __init__(
        self,
        command_builder: Callable[[WorkerContext], list[str] | tuple[str, ...]],
        *,
        timeout_seconds: int = 1800,
    ):
        if not callable(command_builder):
            raise ValueError("command_builder must be callable")
        if type(timeout_seconds) is not int or not 1 <= timeout_seconds <= 3600:
            raise ValueError("timeout_seconds is outside the bounded range")
        self.command_builder = command_builder
        self.timeout_seconds = timeout_seconds

    def run(self, context: WorkerContext) -> WorkerOutcome:
        import local_run_once

        command = _validate_command(self.command_builder(context))
        session_id = f"steward-process:{context.card_id}:{context.attempt}"
        try:
            exit_code, stdout, _stderr = local_run_once._bounded_process(
                command,
                cwd=context.worktree,
                timeout_seconds=self.timeout_seconds,
                env=dict(context.environment),
            )
        except Exception as exc:
            raise WorkerError("worker_process_unavailable") from exc
        if exit_code == 124:
            return WorkerOutcome("TIMEOUT", session_id, _head_or_base(context), (), "worker_timeout")
        if exit_code != 0:
            return WorkerOutcome("FAIL", session_id, _head_or_base(context), (), "worker_process_failed")
        try:
            payload = json.loads(stdout)
            outcome = WorkerOutcome.from_wire(payload)
        except (TypeError, ValueError, json.JSONDecodeError, WorkerError) as exc:
            raise WorkerError("worker_output_invalid") from exc
        return outcome


class BoundedProcessReviewer:
    """Run an independent review child with the same bounded process owner."""

    def __init__(
        self,
        command_builder: Callable[[WorkerContext, WorkerOutcome], list[str] | tuple[str, ...]],
        *,
        timeout_seconds: int = 1800,
    ):
        if not callable(command_builder):
            raise ValueError("command_builder must be callable")
        if type(timeout_seconds) is not int or not 1 <= timeout_seconds <= 3600:
            raise ValueError("timeout_seconds is outside the bounded range")
        self.command_builder = command_builder
        self.timeout_seconds = timeout_seconds

    def review(self, context: WorkerContext, outcome: WorkerOutcome) -> ReviewOutcome:
        import local_run_once

        command = _validate_command(self.command_builder(context, outcome))
        try:
            exit_code, stdout, _stderr = local_run_once._bounded_process(
                command,
                cwd=context.worktree,
                timeout_seconds=self.timeout_seconds,
                env=dict(context.environment),
            )
        except Exception as exc:
            raise WorkerError("review_process_unavailable") from exc
        if exit_code != 0:
            raise WorkerError("review_process_failed")
        try:
            return ReviewOutcome.from_wire(json.loads(stdout))
        except (TypeError, ValueError, json.JSONDecodeError, WorkerError) as exc:
            raise WorkerError("review_output_invalid") from exc


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
    if outcome.head_sha != expected_head_sha:
        raise WorkerError("worker_head_binding_mismatch")
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
        try:
            self.paths = lock_footprint(paths)
        except WorkerError as exc:
            raise PathConflict("path_lock_invalid") from exc
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


class CapacityLock(AbstractContextManager["CapacityLock"]):
    """Reserve one of two host-wide Steward slots across service instances."""

    def __init__(self, lock_dir: str | Path):
        self.lock_dir = Path(lock_dir)
        self._lock: ChatLock | None = None

    def acquire(self) -> "CapacityLock":
        for slot in range(state_manager.MAX_ACTIVE):
            lock = ChatLock(self.lock_dir, f"steward-capacity:{slot}")
            try:
                lock.acquire()
            except (LockBusy, OSError):
                continue
            self._lock = lock
            return self
        raise PathConflict("steward_capacity_busy")

    def release(self) -> None:
        if self._lock is not None:
            self._lock.release()
            self._lock = None

    def __enter__(self) -> "CapacityLock":
        return self.acquire()

    def __exit__(self, *exc: object) -> None:
        self.release()


def lock_footprint(paths: tuple[str, ...] | list[str]) -> tuple[str, ...]:
    """Expand declared paths to stable parent locks to prevent directory overlap."""

    footprint: set[str] = set()
    for path in paths:
        try:
            _safe_path(path)
        except WorkerError as exc:
            raise PathConflict("path_lock_invalid") from exc
        parts = Path(path.rstrip("/")).parts
        if not parts:
            raise PathConflict("path_lock_invalid")
        footprint.update("/".join(parts[:index]) for index in range(1, len(parts) + 1))
    if len(footprint) > 256:
        raise PathConflict("path_lock_invalid")
    return tuple(sorted(footprint))


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


def validate_check_results(checks: object) -> list[dict[str, Any]]:
    """Require repository-owned allowlisted commands to pass exactly."""

    if not isinstance(checks, list) or not checks:
        raise WorkerError("focused_checks_empty")
    validated: list[dict[str, Any]] = []
    for result in checks:
        if not isinstance(result, dict):
            raise WorkerError("focused_check_result_invalid")
        command = result.get("command")
        exit_code = result.get("exit_code")
        if (
            not isinstance(command, str)
            or local_verification.allowlisted_command(command) is None
            or type(exit_code) is not int
            or exit_code != 0
        ):
            raise WorkerError("focused_check_not_passed")
        validated.append({"command": command, "exit_code": exit_code})
    return validated


__all__ = [
    "CallableReviewer",
    "CallableWorker",
    "CapacityLock",
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
    "lock_footprint",
    "validate_check_results",
    "select_model_tier",
    "validate_worker_outcome",
    "validate_changed_paths",
]
