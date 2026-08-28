"""Provider-free autonomous Steward coordinator.

The coordinator drives approved Mission/Stage/WorkCard projections through an
isolated worktree, bounded verification, independent review, and read-only
Stage PR reconciliation.  It deliberately stops at ``WAITING_FOR_MERGE``;
manual exact-head CI/review/merge owners retain their authority.
"""

from __future__ import annotations

from concurrent.futures import FIRST_COMPLETED, Future, ThreadPoolExecutor, wait
from dataclasses import dataclass
import hashlib
from pathlib import Path
import re
import subprocess
from typing import Any, Callable, Mapping

import mission_contract as contract
import review_convergence
import state_manager
import steward_github
from steward_journal import JournalError, StewardJournal
from steward_service import ReconciliationReport, StewardService
import steward_workers as workers
import worktree_manager


SHA40 = workers.SHA40
MAX_CONCURRENCY = state_manager.MAX_ACTIVE
RETRYABLE_WORKER_STATUSES = frozenset({"FAIL", "TIMEOUT"})
RECOVERY_STATES = frozenset({"RUNNING", "VERIFYING", "REVIEWING", "OUTCOME_UNKNOWN"})
_SAFE_IDENTIFIER = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$")


class StewardError(RuntimeError):
    """A bounded coordinator operation was refused or could not be proved."""


@dataclass(frozen=True)
class ExecutionResult:
    card_id: str
    status: str
    attempt: int
    head_sha: str | None
    reason: str
    reviewer_session_id: str | None = None
    pr_number: int | None = None

    def to_wire(self) -> dict[str, Any]:
        return {
            "schema_version": "steward_execution_result.v1",
            "card_id": self.card_id,
            "status": self.status,
            "attempt": self.attempt,
            "head_sha": self.head_sha,
            "reason": self.reason,
            "reviewer_session_id": self.reviewer_session_id,
            "pr_number": self.pr_number,
            "automatic_merge": False,
        }


def _git_head(worktree: Path) -> str:
    try:
        result = subprocess.run(
            ["git", "rev-parse", "--verify", "HEAD"],
            cwd=worktree,
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise StewardError("worktree_head_unavailable") from exc
    head = result.stdout.strip()
    if result.returncode != 0 or SHA40.fullmatch(head) is None:
        raise StewardError("worktree_head_invalid")
    return head


def _git_repository_identity(repo_path: Path, repository: str) -> bool:
    """Prove the checkout and its origin name before creating a worktree."""

    try:
        top = subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            cwd=repo_path,
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
        remote = subprocess.run(
            ["git", "remote", "get-url", "origin"],
            cwd=repo_path,
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return False
    if top.returncode != 0 or remote.returncode != 0:
        return False
    try:
        if Path(top.stdout.strip()).resolve() != repo_path.resolve():
            return False
    except OSError:
        return False
    origin = remote.stdout.strip().removesuffix(".git").rstrip("/")
    if origin.startswith("git@"):
        host, separator, path = origin[4:].partition(":")
        if not separator or host.casefold() != "github.com":
            return False
    else:
        https_prefix = "https://github.com/"
        ssh_prefix = "ssh://git@github.com/"
        if origin.casefold().startswith(https_prefix):
            path = origin[len(https_prefix):]
        elif origin.casefold().startswith(ssh_prefix):
            path = origin[len(ssh_prefix):]
        else:
            return False
        if contract.REPOSITORY.fullmatch(path) is None:
            return False
    return path.casefold() == repository.casefold()


def _git_changed_paths(worktree: Path, base_sha: str, head_sha: str) -> tuple[str, ...]:
    """Read committed paths from the exact base-to-head diff."""

    try:
        ancestry = subprocess.run(
            ["git", "merge-base", "--is-ancestor", base_sha, head_sha],
            cwd=worktree,
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
        if ancestry.returncode != 0:
            raise StewardError("worker_head_not_descendant")
        result = subprocess.run(
            ["git", "diff", "--name-only", "--diff-filter=ACDMRTUXB", f"{base_sha}..{head_sha}"],
            cwd=worktree,
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise StewardError("worktree_diff_unavailable") from exc
    if result.returncode != 0:
        raise StewardError("worktree_diff_unavailable")
    paths = tuple(line for line in result.stdout.splitlines() if line)
    if any(
        not path
        or Path(path).is_absolute()
        or "\\" in path
        or ".." in Path(path).parts
        for path in paths
    ):
        raise StewardError("worktree_diff_path_invalid")
    return paths


def _git_worktree_clean(worktree: Path) -> None:
    """Refuse uncommitted or untracked residue after a worker attempt."""

    try:
        result = subprocess.run(
            ["git", "status", "--porcelain=v1", "--untracked-files=all"],
            cwd=worktree,
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise StewardError("worktree_status_unavailable") from exc
    if result.returncode != 0:
        raise StewardError("worktree_status_unavailable")
    if result.stdout.strip():
        raise workers.WorkerError("worktree_dirty_after_worker")


def _digest(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()[:24]


def _journal_detail(value: str) -> str:
    """Keep child/provider-shaped text out of the durable operator journal."""

    if isinstance(value, str) and value and _SAFE_IDENTIFIER.fullmatch(value):
        return value
    return f"reason_{_digest(str(value))}"


def _stage_pr_facts(
    value: steward_github.StagePRFacts | dict[str, Any] | None,
) -> steward_github.StagePRFacts | None:
    if value is None:
        return None
    if isinstance(value, steward_github.StagePRFacts):
        return value
    return steward_github.StagePRFacts.from_wire(value)


class Steward:
    """One provider-free bounded executor and its rebuildable service shell."""

    def __init__(
        self,
        *,
        repository: str,
        repo_path: str | Path,
        journal: StewardJournal,
        github: steward_github.ReadOnlyGitHub,
        worker: workers.WorkerAdapter | None = None,
        reviewer: workers.ReviewerAdapter | None = None,
        verifier: Callable[[Path, list[str]], list[dict[str, Any]]] | None = None,
        lock_dir: str | Path,
        max_concurrency: int = MAX_CONCURRENCY,
    ):
        if contract.REPOSITORY.fullmatch(repository) is None:
            raise StewardError("repository_invalid")
        if max_concurrency != MAX_CONCURRENCY:
            raise StewardError("steward_concurrency_must_be_two")
        if worker is not None and not isinstance(
            worker, (workers.BoundedProcessWorker, workers.ProviderFreeWorker)
        ):
            raise StewardError("worker_adapter_must_be_bounded_process")
        if reviewer is not None and not isinstance(
            reviewer, workers.BoundedProcessReviewer
        ):
            raise StewardError("reviewer_adapter_must_be_bounded_process")
        self.repository = repository
        self.repo_path = Path(repo_path).resolve()
        self.journal = journal
        self.github = github
        self.worker = worker or workers.ProviderFreeWorker()
        self.reviewer = reviewer
        self.verifier = verifier or workers.run_allowlisted_checks
        self.lock_dir = Path(lock_dir).resolve()
        self.max_concurrency = max_concurrency
        self.service: StewardService | None = None
        self.mission_id: str | None = None

    def _service_for(self, mission: contract.MaintenanceMission) -> StewardService:
        if self.service is None or self.mission_id != mission.mission_id:
            self.service = StewardService(
                mission_id=mission.mission_id,
                journal=self.journal,
                github=self.github,
            )
            self.mission_id = mission.mission_id
        return self.service

    def heartbeat(self, mission: contract.MaintenanceMission, *, tick_id: str) -> dict[str, Any]:
        return self._service_for(mission).heartbeat(tick_id=tick_id)

    def recover(self, mission: contract.MaintenanceMission) -> ReconciliationReport:
        return self._service_for(mission).recover()

    def reconcile(
        self,
        mission: contract.MaintenanceMission,
        *,
        stage_bindings: Mapping[str, Mapping[str, Any]],
    ) -> ReconciliationReport:
        return self._service_for(mission).reconcile(stage_bindings=stage_bindings)

    def _record(
        self,
        *,
        event: str,
        key: str,
        mission: contract.MaintenanceMission,
        stage: contract.Stage,
        card: contract.WorkCard,
        attempt: int,
        state: str,
        detail: str,
        data: dict[str, Any] | None = None,
    ) -> None:
        self.journal.append(
            event=event,
            idempotency_key=key,
            mission_id=mission.mission_id,
            stage_id=stage.stage_id,
            card_id=card.card_id,
            attempt=attempt,
            state=state,
            detail=_journal_detail(detail),
            data=data,
        )

    def _failure(
        self,
        *,
        mission: contract.MaintenanceMission,
        stage: contract.Stage,
        card: contract.WorkCard,
        attempt: int,
        reason: str,
        retryable: bool,
        head_sha: str | None = None,
    ) -> ExecutionResult:
        safe_reason = _journal_detail(str(reason))
        if retryable and attempt < card.max_attempts:
            self._record(
                event="ATTEMPT_RETRY_SCHEDULED",
                key=f"retry:{card.card_id}:{attempt}:{_digest(safe_reason)}",
                mission=mission,
                stage=stage,
                card=card,
                attempt=attempt,
                state="RETRYING",
                detail=safe_reason,
            )
            return ExecutionResult(card.card_id, "RETRY_SCHEDULED", attempt, head_sha, safe_reason)
        self._record(
            event="CARD_BLOCKED",
            key=f"blocked:{card.card_id}:{attempt}:{_digest(safe_reason)}",
            mission=mission,
            stage=stage,
            card=card,
            attempt=attempt,
            state="BLOCKED",
            detail=safe_reason,
        )
        return ExecutionResult(card.card_id, "BLOCKED", attempt, head_sha, safe_reason)

    def _existing_result(
        self, card: contract.WorkCard, latest: Any
    ) -> ExecutionResult | None:
        if latest is None:
            return None
        if latest.state == "WAITING_FOR_MERGE":
            return ExecutionResult(card.card_id, "WAITING_FOR_MERGE", latest.attempt, None, "already_waiting_for_merge")
        if latest.state == "COMPLETE":
            return ExecutionResult(card.card_id, "COMPLETE", latest.attempt, None, "already_complete")
        if latest.state in RECOVERY_STATES:
            return ExecutionResult(card.card_id, "RECOVERY_REQUIRED", latest.attempt, None, "in_flight_state_requires_reconciliation")
        if latest.state == "BLOCKED":
            return ExecutionResult(card.card_id, "BLOCKED", latest.attempt, None, latest.detail)
        return None

    def _review_attempt(self, mission: contract.MaintenanceMission, card: contract.WorkCard, head_sha: str) -> dict[str, Any]:
        previous: dict[str, Any] | None = None
        for event in reversed(self.journal.replay()):
            if (
                event.mission_id == mission.mission_id
                and event.stage_id == card.stage_id
                and event.card_id == card.card_id
                and event.event in {"REVIEW_FAILED", "REVIEW_PASSED"}
            ):
                previous = dict(event.data)
                break
        try:
            return review_convergence.derive_next_review_attempt(previous, head_sha)
        except (TypeError, ValueError, KeyError):
            return {
                "allowed": False,
                "deny_reason": "review_attempt_state_invalid",
                "review_mode": "full",
                "review_round": 1,
            }

    def dispatch_card(
        self,
        mission: contract.MaintenanceMission,
        stage: contract.Stage,
        card: contract.WorkCard,
        *,
        base_sha: str,
        stage_pr: steward_github.StagePRFacts | dict[str, Any] | None = None,
    ) -> ExecutionResult:
        """Run one card; restart never replays an in-flight attempt blindly."""

        if not SHA40.fullmatch(base_sha):
            raise StewardError("base_sha_invalid")
        try:
            validated_mission = contract.validate_owner_approval(mission)
            if validated_mission != contract.campaign_mission():
                raise StewardError("mission_registration_invalid")
            if validated_mission.repository_identity.repository != self.repository:
                raise StewardError("mission_repository_mismatch")
            if validated_mission.repository_identity.base_sha != base_sha:
                raise StewardError("mission_base_sha_mismatch")
            contract.validate_workcard(card, stage, mission)
        except contract.MissionContractError as exc:
            raise StewardError("mission_or_stage_or_card_invalid") from exc
        if not _git_repository_identity(self.repo_path, self.repository):
            raise StewardError("repository_identity_unavailable")
        existing = self._existing_result(
            card,
            self.journal.latest_for_card(
                card.card_id, mission_id=mission.mission_id, stage_id=stage.stage_id
            ),
        )
        if existing is not None:
            return existing
        latest = self.journal.latest_for_card(
            card.card_id, mission_id=mission.mission_id, stage_id=stage.stage_id
        )
        attempt = 1 if latest is None else latest.attempt + (1 if latest.state == "RETRYING" else 0)
        stage_facts = _stage_pr_facts(stage_pr)
        if (
            stage_facts is not None
            and stage.integration_pr is not None
            and stage_facts.pr_number != stage.integration_pr
        ):
            raise StewardError("stage_pr_number_mismatch")
        while attempt <= card.max_attempts:
            self._record(
                event="CARD_QUEUED",
                key=f"queue:{card.card_id}:{attempt}:{base_sha}",
                mission=mission,
                stage=stage,
                card=card,
                attempt=attempt,
                state="QUEUED",
                detail="bounded_workcard_admitted",
            )
            try:
                with workers.CapacityLock(self.lock_dir), workers.PathLockSet(
                    self.lock_dir, card.path_locks
                ):
                    created = worktree_manager.create_steward_worktree(
                        card.card_id,
                        str(self.repo_path),
                        base_sha,
                        binding_key="\x00".join(
                            (mission.mission_id, stage.stage_id, card.card_id, base_sha)
                        ),
                    )
                    if not created:
                        result = self._failure(
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            reason="worktree_creation_refused",
                            retryable=True,
                        )
                        if result.status == "RETRY_SCHEDULED":
                            attempt += 1
                            continue
                        return result
                    worktree_path = Path(created[0])
                    worktree_branch = created[1]
                    worktree_binding_sha256 = worktree_manager.steward_binding_digest(
                        mission.mission_id, stage.stage_id, card.card_id, base_sha
                    )
                    expected_worktree_suffix = worktree_binding_sha256[:24]
                    if (
                        worktree_path.name != f"steward-{expected_worktree_suffix}"
                        or worktree_branch != f"agent/steward-{expected_worktree_suffix}"
                        or created[2] != base_sha
                    ):
                        raise StewardError("worktree_binding_mismatch")
                    self._record(
                        event="WORKER_STARTED",
                        key=f"start:{card.card_id}:{attempt}:{base_sha}",
                        mission=mission,
                        stage=stage,
                        card=card,
                        attempt=attempt,
                        state="RUNNING",
                        detail="isolated_worktree_bound",
                        data={
                            "base_sha": base_sha,
                            "worktree_binding_sha256": worktree_binding_sha256,
                            "branch_binding_sha256": _digest(worktree_branch),
                        },
                    )
                    context = workers.WorkerContext(
                        mission_id=mission.mission_id,
                        stage_id=stage.stage_id,
                        card_id=card.card_id,
                        attempt=attempt,
                        model_tier=workers.select_model_tier(card.model_tier, attempt),
                        base_sha=base_sha,
                        worktree=worktree_path,
                        allowed_paths=card.allowed_paths,
                        steps=card.steps,
                        focused_tests=card.focused_tests,
                        negative_checks=card.negative_checks,
                        expected_evidence=card.expected_evidence,
                        environment=workers.child_environment(),
                    )
                    try:
                        outcome = self.worker.run(context)
                    except workers.WorkerUnavailable as exc:
                        return self._failure(
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            reason=str(exc)[:200],
                            retryable=False,
                        )
                    except Exception:
                        self._record(
                            event="WORKER_OUTCOME_UNKNOWN",
                            key=f"unknown:worker:{card.card_id}:{attempt}",
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            state="OUTCOME_UNKNOWN",
                            detail="worker_exception_after_admission",
                        )
                        return ExecutionResult(card.card_id, "OUTCOME_UNKNOWN", attempt, None, "worker_exception_after_admission")
                    try:
                        observed_head = _git_head(worktree_path)
                        workers.validate_worker_outcome(
                            card, outcome, expected_head_sha=observed_head
                        )
                        actual_paths = _git_changed_paths(worktree_path, base_sha, observed_head)
                        workers.validate_changed_paths(card, actual_paths)
                        _git_worktree_clean(worktree_path)
                    except workers.WorkerError as exc:
                        return self._failure(
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            reason=str(exc)[:200],
                            retryable=False,
                        )
                    except StewardError:
                        self._record(
                            event="WORKER_OUTCOME_UNKNOWN",
                            key=f"unknown:head:{card.card_id}:{attempt}",
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            state="OUTCOME_UNKNOWN",
                            detail="worker_head_unavailable_after_attempt",
                        )
                        return ExecutionResult(card.card_id, "OUTCOME_UNKNOWN", attempt, None, "worker_head_unavailable_after_attempt")
                    if outcome.status != "PASS":
                        if outcome.status == "OUTCOME_UNKNOWN":
                            self._record(
                                event="WORKER_OUTCOME_UNKNOWN",
                                key=f"unknown:reported:{card.card_id}:{attempt}",
                                mission=mission,
                                stage=stage,
                                card=card,
                                attempt=attempt,
                                state="OUTCOME_UNKNOWN",
                                detail="worker_reported_unknown_outcome",
                            )
                            return ExecutionResult(
                                card.card_id,
                                "OUTCOME_UNKNOWN",
                                attempt,
                                observed_head,
                                _journal_detail(outcome.detail or "worker_reported_unknown_outcome"),
                            )
                        result = self._failure(
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            reason=outcome.detail or f"worker_{outcome.status.lower()}",
                            retryable=outcome.status in RETRYABLE_WORKER_STATUSES,
                            head_sha=observed_head,
                        )
                        if result.status == "RETRY_SCHEDULED":
                            attempt += 1
                            continue
                        return result
                    if set(actual_paths) != set(outcome.changed_paths):
                        return self._failure(
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            reason="worker_changed_paths_mismatch",
                            retryable=False,
                            head_sha=observed_head,
                        )
                    if observed_head == base_sha and not actual_paths:
                        return self._failure(
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            reason="no_change",
                            retryable=False,
                            head_sha=observed_head,
                        )
                    self._record(
                        event="FOCUSED_CHECKS_STARTED",
                        key=f"verify:{card.card_id}:{attempt}:{observed_head}",
                        mission=mission,
                        stage=stage,
                        card=card,
                        attempt=attempt,
                        state="VERIFYING",
                        detail="allowlisted_checks_only",
                    )
                    try:
                        checks = self.verifier(worktree_path, list(outcome.changed_paths))
                        checks = workers.validate_check_results(checks)
                    except Exception as exc:
                        result = self._failure(
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            reason=str(exc)[:200] or "focused_checks_failed",
                            retryable=True,
                            head_sha=observed_head,
                        )
                        if result.status == "RETRY_SCHEDULED":
                            attempt += 1
                            continue
                        return result
                    self._record(
                        event="FOCUSED_CHECKS_PASSED",
                        key=f"checks-passed:{card.card_id}:{attempt}:{observed_head}",
                        mission=mission,
                        stage=stage,
                        card=card,
                        attempt=attempt,
                        state="REVIEWING",
                        detail="repository_owned_checks_passed",
                        data={"check_count": len(checks)},
                    )
                    if self.reviewer is None:
                        return self._failure(
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            reason="independent_reviewer_unavailable",
                            retryable=False,
                            head_sha=observed_head,
                        )
                    try:
                        review_attempt = self._review_attempt(
                            mission, card, observed_head
                        )
                        if not review_attempt.get("allowed"):
                            raise workers.WorkerError(
                                str(review_attempt.get("deny_reason", "review_attempt_denied"))
                            )
                        review = self.reviewer.review(context, outcome)
                        if not isinstance(review, workers.ReviewOutcome):
                            raise workers.WorkerError("review_adapter_return_invalid")
                    except workers.WorkerError as exc:
                        return self._failure(
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            reason=str(exc)[:200],
                            retryable=False,
                            head_sha=observed_head,
                        )
                    if review.reviewed_head_sha != observed_head:
                        return self._failure(
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            reason="review_head_binding_mismatch",
                            retryable=False,
                            head_sha=observed_head,
                        )
                    if (
                        review.review_round != review_attempt["review_round"]
                        or review.review_mode != review_attempt["review_mode"]
                    ):
                        return self._failure(
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            reason="review_convergence_binding_mismatch",
                            retryable=False,
                            head_sha=observed_head,
                        )
                    try:
                        reviewed_head = _git_head(worktree_path)
                        reviewed_paths = _git_changed_paths(
                            worktree_path, base_sha, reviewed_head
                        )
                        workers.validate_changed_paths(card, reviewed_paths)
                        _git_worktree_clean(worktree_path)
                    except workers.WorkerError as exc:
                        return self._failure(
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            reason=str(exc)[:200],
                            retryable=False,
                            head_sha=observed_head,
                        )
                    except StewardError:
                        self._record(
                            event="REVIEW_OUTCOME_UNKNOWN",
                            key=f"unknown:review-head:{card.card_id}:{attempt}",
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            state="OUTCOME_UNKNOWN",
                            detail="review_head_unavailable_after_review",
                        )
                        return ExecutionResult(
                            card.card_id,
                            "OUTCOME_UNKNOWN",
                            attempt,
                            None,
                            "review_head_unavailable_after_review",
                        )
                    if reviewed_head != observed_head or set(reviewed_paths) != set(actual_paths):
                        return self._failure(
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            reason="reviewed_head_or_paths_changed",
                            retryable=False,
                            head_sha=reviewed_head,
                        )
                    if (
                        review.reviewed_base_sha != base_sha
                        or review.reviewed_range_sha256
                        != workers.review_range_digest(base_sha, observed_head)
                    ):
                        return self._failure(
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            reason="review_range_binding_mismatch",
                            retryable=False,
                            head_sha=observed_head,
                        )
                    if review.status != "PASS":
                        self._record(
                            event="REVIEW_FAILED",
                            key=f"review-failed:{card.card_id}:{attempt}:{observed_head}",
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            state="REVIEWING",
                            detail="independent_review_not_passed",
                            data={
                                "head_sha": observed_head,
                                "review_round": review.review_round,
                                "review_mode": review.review_mode,
                                "verdict": review.status,
                                "open_blocker_ids": [
                                    _digest(item)
                                    for item in (review.blockers or (review.detail or "review_failed",))
                                ],
                                "finding_ledger_digest": _digest(
                                    "|".join(review.blockers or (review.detail or "review_failed",))
                                ),
                                "autonomous_repairs_remaining": 1
                                if review.review_round < 2
                                else 0,
                            },
                        )
                        result = self._failure(
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            reason=review.detail or "independent_review_not_passed",
                            retryable=review.status == "FAIL",
                            head_sha=observed_head,
                        )
                        if result.status == "RETRY_SCHEDULED":
                            attempt += 1
                            continue
                        return result
                    self._record(
                        event="REVIEW_PASSED",
                        key=f"review:{card.card_id}:{attempt}:{observed_head}:{_digest(review.reviewer_session_id)}",
                        mission=mission,
                        stage=stage,
                        card=card,
                        attempt=attempt,
                        state="REVIEWING",
                        detail="independent_review_passed",
                        data={
                            "implementation_session_digest": _digest(outcome.session_id),
                            "reviewer_session_digest": _digest(review.reviewer_session_id),
                            "base_sha": base_sha,
                            "head_sha": observed_head,
                            "reviewed_range_sha256": review.reviewed_range_sha256,
                            "review_axes": list(review.review_axes),
                            "review_round": review.review_round,
                            "review_mode": review.review_mode,
                            "review_receipt_sha256": review.review_receipt_sha256,
                            "verdict": review.status,
                            "autonomous_repairs_remaining": 0,
                        },
                    )
                    if stage_facts is None:
                        return ExecutionResult(card.card_id, "WAITING_FOR_PR", attempt, observed_head, "stage_pr_facts_required", review.reviewer_session_id)
                    try:
                        live_stage_facts = _stage_pr_facts(
                            self.github.fetch_stage_pr(
                                self.repository, stage_facts.pr_number
                            )
                        )
                        if live_stage_facts is None:
                            raise steward_github.GitHubFactsError(
                                "github_live_facts_missing"
                            )
                        if (
                            stage.integration_pr is not None
                            and live_stage_facts.pr_number != stage.integration_pr
                        ):
                            raise steward_github.GitHubFactsError(
                                "stage_pr_number_mismatch"
                            )
                        if (
                            stage.exact_head is not None
                            and live_stage_facts.head_sha != stage.exact_head
                        ):
                            raise steward_github.GitHubFactsError(
                                "stage_exact_head_mismatch"
                            )
                        status = steward_github.reconcile_stage_pr(
                            live_stage_facts,
                            repository=self.repository,
                            pr_number=stage_facts.pr_number,
                            expected_base_sha=base_sha,
                            expected_head_sha=observed_head,
                            expected_base_branch=stage.repository_identity.branch,
                            expected_head_branch=worktree_branch,
                        )
                    except steward_github.GitHubReadError as exc:
                        self._record(
                            event="STAGE_GATES_PENDING",
                            key=f"github-read:{card.card_id}:{observed_head}",
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            state="REVIEWING",
                            detail="github_facts_unavailable",
                        )
                        return ExecutionResult(
                            card.card_id,
                            "WAITING",
                            attempt,
                            observed_head,
                            "github_facts_unavailable",
                            review.reviewer_session_id,
                        )
                    except steward_github.GitHubFactsError as exc:
                        return self._failure(
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            reason=str(exc)[:200],
                            retryable=False,
                            head_sha=observed_head,
                        )
                    self._record(
                        event="STAGE_PR_BOUND",
                        key=f"stage-bind:{card.card_id}:{status.pr_number}:{observed_head}",
                        mission=mission,
                        stage=stage,
                        card=card,
                        attempt=attempt,
                        state="REVIEWING",
                        detail="stage_pr_binding_observed",
                        data={
                            "repository": status.repository,
                            "pr_number": status.pr_number,
                            "base_sha": status.base_sha,
                            "head_sha": status.head_sha,
                            "stage_id": stage.stage_id,
                            "base_branch": stage.repository_identity.branch,
                            "head_branch": worktree_branch,
                        },
                    )
                    if status.outcome == "WAITING_FOR_MERGE":
                        self._record(
                            event="STAGE_WAITING_FOR_MERGE",
                            key=f"waiting:{card.card_id}:{observed_head}:{status.pr_number}",
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            state="WAITING_FOR_MERGE",
                            detail="exact_head_ci_and_review_pass",
                            data={"pr_number": status.pr_number},
                        )
                        return ExecutionResult(card.card_id, "WAITING_FOR_MERGE", attempt, observed_head, status.reason, review.reviewer_session_id, status.pr_number)
                    if status.outcome == "COMPLETE":
                        self._record(
                            event="STAGE_MERGED_OBSERVED",
                            key=f"complete:{card.card_id}:{observed_head}:{status.pr_number}",
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            state="COMPLETE",
                            detail="live_pr_merge_observed",
                            data={"pr_number": status.pr_number},
                        )
                        return ExecutionResult(card.card_id, "COMPLETE", attempt, observed_head, status.reason, review.reviewer_session_id, status.pr_number)
                    if status.outcome == "WAITING":
                        self._record(
                            event="STAGE_GATES_PENDING",
                            key=f"gates:{card.card_id}:{observed_head}:{_digest(status.reason)}",
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            state="REVIEWING",
                            detail=status.reason,
                        )
                        return ExecutionResult(card.card_id, "WAITING", attempt, observed_head, status.reason, review.reviewer_session_id, status.pr_number)
                    return self._failure(
                        mission=mission,
                        stage=stage,
                        card=card,
                        attempt=attempt,
                        reason=status.reason,
                        retryable=False,
                        head_sha=observed_head,
                    )
            except workers.PathConflict:
                result = self._failure(
                    mission=mission,
                    stage=stage,
                    card=card,
                    attempt=attempt,
                    reason="path_lock_conflict",
                    retryable=True,
                )
                if result.status == "RETRY_SCHEDULED":
                    attempt += 1
                    continue
                return result
            except (JournalError, StewardError) as exc:
                return ExecutionResult(
                    card.card_id,
                    "RECOVERY_REQUIRED",
                    attempt,
                    None,
                    _journal_detail(str(exc)),
                )
        return ExecutionResult(card.card_id, "BLOCKED", attempt - 1, None, "attempt_budget_exhausted")

    def dispatch_cards(
        self,
        mission: contract.MaintenanceMission,
        stage: contract.Stage,
        cards: tuple[contract.WorkCard, ...],
        *,
        base_sha: str,
        stage_pr: steward_github.StagePRFacts | dict[str, Any] | None = None,
    ) -> dict[str, ExecutionResult]:
        """Dispatch dependency-ready disjoint cards with at most K=2 workers."""

        try:
            contract.validate_stage(stage, mission, cards)
        except contract.MissionContractError as exc:
            raise StewardError("stage_graph_invalid") from exc
        pending = {card.card_id: card for card in cards}
        results: dict[str, ExecutionResult] = {}
        running: dict[Future[ExecutionResult], tuple[contract.WorkCard, set[str]]] = {}
        executor = ThreadPoolExecutor(max_workers=MAX_CONCURRENCY, thread_name_prefix="steward-card")
        try:
            while pending or running:
                launched = True
                while launched and len(running) < MAX_CONCURRENCY:
                    launched = False
                    occupied = set().union(*(paths for _, paths in running.values())) if running else set()
                    for card_id in sorted(pending):
                        card = pending[card_id]
                        dependency_results = [results.get(item) for item in card.dependencies]
                        if any(item is None for item in dependency_results):
                            continue
                        if any(item.status != "COMPLETE" for item in dependency_results if item is not None):
                            results[card_id] = ExecutionResult(card_id, "BLOCKED", 0, None, "dependency_not_complete")
                            del pending[card_id]
                            launched = True
                            break
                        paths = set(workers.lock_footprint(card.path_locks))
                        if occupied & paths:
                            continue
                        del pending[card_id]
                        future = executor.submit(
                            self.dispatch_card,
                            mission,
                            stage,
                            card,
                            base_sha=base_sha,
                            stage_pr=stage_pr,
                        )
                        running[future] = (card, paths)
                        launched = True
                        break
                if running:
                    done, _ = wait(tuple(running), return_when=FIRST_COMPLETED)
                    for future in done:
                        card, _paths = running.pop(future)
                        try:
                            results[card.card_id] = future.result()
                        except Exception as exc:
                            results[card.card_id] = ExecutionResult(
                                card.card_id,
                                "RECOVERY_REQUIRED",
                                0,
                                None,
                                _journal_detail(str(exc)),
                            )
                elif pending:
                    for card_id, card in list(pending.items()):
                        if any(dep not in pending for dep in card.dependencies):
                            results[card_id] = ExecutionResult(card_id, "BLOCKED", 0, None, "dependency_cycle_or_unresolved")
                            del pending[card_id]
                    if pending:
                        raise StewardError("dispatch_graph_cannot_progress")
        finally:
            executor.shutdown(wait=True)
        return results


__all__ = ["ExecutionResult", "MAX_CONCURRENCY", "Steward", "StewardError"]
