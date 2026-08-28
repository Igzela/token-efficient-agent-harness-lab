"""Heartbeat, restart recovery, and read-only reconciliation for Steward."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timezone
import argparse
import os
from pathlib import Path
import time
from typing import Any, Callable, Mapping

import mission_contract
from steward_github import (
    GhReadOnlyGitHub,
    GitHubFactsError,
    GitHubReadError,
    ReadOnlyGitHub,
    StagePRStatus,
    reconcile_stage_pr,
)
from steward_journal import JournalError, StewardJournal
import steward_workers
import worktree_manager


def _bounded_key(value: str) -> str:
    if not isinstance(value, str) or not value or len(value) > 128 or "\n" in value:
        raise ValueError("idempotency key is invalid")
    return value


def _reconcile_key(kind: str, mission_id: str, stage_id: str, card_id: str, *parts: object) -> str:
    """Bind reconciliation idempotency to the complete card identity."""

    value = ":".join(("reconcile", kind, mission_id, stage_id, card_id, *(str(part) for part in parts)))
    if len(value) > 128:
        raise ValueError("reconciliation idempotency key is too long")
    return value


def _now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")


def _review_binding(
    event: Any, *, worktree: Path | None = None
) -> tuple[str, str, str] | None:
    """Return only a complete, canonical exact-head review binding."""

    if getattr(event, "event", None) != "REVIEW_PASSED":
        return None
    data = getattr(event, "data", {})
    if not isinstance(data, Mapping):
        return None
    required = (
        "implementation_session_id",
        "reviewer_session_id",
        "base_sha",
        "head_sha",
        "reviewed_range_sha256",
        "review_axes",
        "review_round",
        "review_mode",
        "review_receipt_sha256",
        "verdict",
        "finding_ledger_digest",
        "open_blocker_ids",
        "deferred_note_ids",
        "security_ok",
        "rollback_ok",
        "observed_ci_status",
    )
    if not all(key in data for key in required):
        return None
    base_sha = data["base_sha"]
    head_sha = data["head_sha"]
    try:
        expected_range = steward_workers.review_range_digest(
            base_sha, head_sha, worktree=worktree
        )
    except (TypeError, ValueError, steward_workers.WorkerError):
        return None
    if (
        not isinstance(base_sha, str)
        or not steward_workers.SHA40.fullmatch(base_sha)
        or not isinstance(head_sha, str)
        or not steward_workers.SHA40.fullmatch(head_sha)
        or data["reviewed_range_sha256"] != expected_range
        or not isinstance(data["review_axes"], list)
        or set(data["review_axes"]) != {"standards", "spec"}
        or type(data["review_round"]) is not int
        or data["review_round"] not in {1, 2}
        or data["review_mode"] not in {"full", "repair_verification"}
        or not isinstance(data["review_receipt_sha256"], str)
        or not steward_workers.SHA256.fullmatch(data["review_receipt_sha256"])
        or data["verdict"] != "PASS"
            or not isinstance(data["finding_ledger_digest"], str)
        or not steward_workers.SHA256.fullmatch(data["finding_ledger_digest"])
        or not isinstance(data["security_ok"], bool)
        or not isinstance(data["rollback_ok"], bool)
        or data["security_ok"] is not True
        or data["rollback_ok"] is not True
    ):
        return None
    if any(
        not isinstance(data.get(name, []), list)
        or not all(isinstance(item, str) and item for item in data.get(name, []))
        for name in ("open_blocker_ids", "deferred_note_ids", "decision_required_ids")
    ):
        return None
    try:
        steward_workers._safe_session(
            data["reviewer_session_id"], "reviewer_session_id"
        )
        steward_workers._safe_session(
            data["implementation_session_id"], "implementation_session_id"
        )
        if data["reviewer_session_id"] == data["implementation_session_id"]:
            return None
        steward_workers._safe_detail(
            data["observed_ci_status"], "review_observed_ci_status"
        )
        open_blockers = tuple(data["open_blocker_ids"])
        deferred_notes = tuple(data["deferred_note_ids"])
        decision_required = tuple(data.get("decision_required_ids", []))
        receipt_wire = {
            "schema_version": "steward_review_outcome.v1",
            "status": "PASS",
            "reviewer_session_id": data["reviewer_session_id"],
            "implementation_session_id": data["implementation_session_id"],
            "reviewed_head_sha": head_sha,
            "blockers": list(open_blockers),
            "detail": "",
            "reviewed_base_sha": base_sha,
            "reviewed_range_sha256": data["reviewed_range_sha256"],
            "review_axes": list(data["review_axes"]),
            "review_round": data["review_round"],
            "review_mode": data["review_mode"],
            "review_receipt_sha256": data["review_receipt_sha256"],
            "summary": "",
            "findings": None,
            "security_ok": data["security_ok"],
            "rollback_ok": data["rollback_ok"],
            "observed_ci_status": data["observed_ci_status"],
            "finding_ledger_digest": data["finding_ledger_digest"],
        }
    except (KeyError, TypeError, ValueError, steward_workers.WorkerError):
        return None
    if data["review_receipt_sha256"] != steward_workers.review_receipt_digest(receipt_wire):
        return None
    # The parent already normalized the full finding ledger before persisting
    # this bounded projection.  Rebuilding an empty ReviewOutcome here would
    # discard deferred findings and manufacture a different ledger digest.
    # The sealed wire receipt preserves that ledger identity across restart;
    # the bounded blocker/deferred/decision projections are checked above.
    if open_blockers or decision_required:
        return None
    return base_sha, head_sha, data["reviewed_range_sha256"]


@dataclass(frozen=True)
class RecoveryItem:
    card_id: str
    state: str
    outcome: str
    reason: str

    def to_wire(self) -> dict[str, str]:
        return {
            "card_id": self.card_id,
            "state": self.state,
            "outcome": self.outcome,
            "reason": self.reason,
        }


@dataclass(frozen=True)
class ReconciliationReport:
    observed_at: str
    items: tuple[RecoveryItem, ...]
    journal_projection: dict[str, Any]

    def to_wire(self) -> dict[str, Any]:
        return {
            "schema_version": "steward_reconciliation.v1",
            "observed_at": self.observed_at,
            "items": [item.to_wire() for item in self.items],
            "journal_projection": dict(self.journal_projection),
        }


class StewardService:
    """Non-authoritative service shell; all durable state is journal replay."""

    def __init__(
        self,
        *,
        mission_id: str,
        journal: StewardJournal,
        github: ReadOnlyGitHub,
        repo_path: str | os.PathLike[str] | None = None,
    ):
        try:
            registered = mission_contract.validate_registered_campaign()
        except mission_contract.MissionContractError as exc:
            raise ValueError("registered_mission_invalid") from exc
        if mission_id != registered.mission_id:
            raise ValueError("mission_id_not_registered")
        self.mission_id = mission_id
        self.journal = journal
        self.github = github
        self.repo_path = Path(repo_path).resolve() if repo_path is not None else None

    def heartbeat(self, *, tick_id: str | None = None) -> dict[str, Any]:
        """Record one idempotent liveness fact without changing work state."""

        key = _bounded_key(tick_id or f"heartbeat:{_now()}")
        event = self.journal.heartbeat(
            mission_id=self.mission_id,
            idempotency_key=key,
        )
        return {
            "schema_version": "steward_heartbeat.v1",
            "mission_id": self.mission_id,
            "timestamp": event.timestamp,
            "seq": event.seq,
            "tail_sha256": event.sha256,
        }

    def execute_stage(
        self,
        dispatch: Callable[..., dict[str, Any]],
        *,
        mission: mission_contract.MaintenanceMission,
        stage: mission_contract.Stage,
        cards: tuple[mission_contract.WorkCard, ...],
        base_sha: str,
        stage_pr: Mapping[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Run one admitted stage through the bounded Steward coordinator.

        The service owns the liveness/recovery preflight and invokes the
        packet-scoped coordinator supplied by the caller.  It does not load a
        plan, create a second queue, or perform a GitHub/product mutation.
        """

        if not callable(dispatch):
            raise TypeError("stage dispatcher must be callable")
        if mission.mission_id != self.mission_id:
            raise ValueError("mission_id_not_registered")
        self.heartbeat(tick_id=f"execute:{mission.mission_id}:{stage.stage_id}")
        recovery = self.recover()
        if any(item.outcome == "RECOVERY_REQUIRED" for item in recovery.items):
            raise ValueError("recovery_required_before_dispatch")
        result = dispatch(
            mission,
            stage,
            cards,
            base_sha=base_sha,
            stage_pr=stage_pr,
        )
        if not isinstance(result, dict):
            raise TypeError("stage dispatcher returned an invalid result")
        return result

    def recover(self) -> ReconciliationReport:
        """Rebuild local state and mark in-flight work for inspection.

        A worker-started card is never replayed blindly.  Only a subsequent
        explicit ``reconcile`` call with live read-only facts can converge it.
        """

        projection = self.journal.projection(mission_id=self.mission_id)
        events = self.journal.replay()
        started: dict[tuple[str, str, str], Any] = {}
        for event in events:
            if event.mission_id == self.mission_id and event.event == "WORKER_STARTED":
                started[(event.mission_id, event.stage_id, event.card_id)] = event
        registered = mission_contract.validate_registered_campaign()
        items: list[RecoveryItem] = []
        for binding in projection["bindings"]:
            state = binding["state"]
            key = (binding["mission_id"], binding["stage_id"], binding["card_id"])
            worker_event = started.get(key)
            worker_data = worker_event.data if worker_event is not None else {}
            expected_binding = worktree_manager.steward_binding_digest(
                binding["mission_id"], binding["stage_id"], binding["card_id"], registered.repository_identity.base_sha
            )
            binding_valid = (
                state not in {"RUNNING", "VERIFYING", "REVIEWING", "OUTCOME_UNKNOWN"}
                or (
                    isinstance(worker_data.get("base_sha"), str)
                    and worker_data["base_sha"] == registered.repository_identity.base_sha
                    and worker_data.get("worktree_binding_sha256") == expected_binding
                    and isinstance(worker_data.get("branch"), str)
                )
            )
            if binding_valid and state in {"RUNNING", "VERIFYING", "REVIEWING", "OUTCOME_UNKNOWN"}:
                try:
                    expected_path, expected_branch = worktree_manager.steward_worktree_location(
                        binding["mission_id"],
                        binding["stage_id"],
                        binding["card_id"],
                        registered.repository_identity.base_sha,
                    )
                    binding_valid = (
                        worker_data["branch"] == expected_branch
                        and self.repo_path is not None
                        and worktree_manager.verify_worktree(
                            expected_path,
                            expected_branch,
                            self.repo_path,
                            registered.repository_identity.base_sha,
                        )
                    )
                except (OSError, ValueError, TypeError):
                    binding_valid = False
            if not binding_valid:
                items.append(
                    RecoveryItem(binding["card_id"], state, "BLOCKED", "worker_binding_missing_or_invalid")
                )
            elif state in {"RUNNING", "VERIFYING", "REVIEWING"}:
                items.append(
                    RecoveryItem(binding["card_id"], state, "RECOVERY_REQUIRED", "in_flight_work_requires_read_only_reconciliation")
                )
            else:
                items.append(
                    RecoveryItem(binding["card_id"], state, "REBUILT", "journal_projection_rebuilt")
                )
        return ReconciliationReport(_now(), tuple(items), projection)

    def reconcile(
        self,
        *,
        stage_bindings: Mapping[str, Mapping[str, Any]],
    ) -> ReconciliationReport:
        """Read live PR facts and append only observed, idempotent transitions."""

        projection = self.journal.projection(mission_id=self.mission_id)
        review_bound_heads = {
            (event.card_id, event.stage_id, binding[1])
            for event in self.journal.replay()
            if event.mission_id == self.mission_id
            and (binding := _review_binding(event, worktree=self.repo_path)) is not None
        }
        items: list[RecoveryItem] = []
        for record in projection["active_bindings"]:
            card_id = record["card_id"]
            stage_id = record["stage_id"]
            # Recovery accepts only the append-only binding recorded by the
            # executor.  Caller-provided projections cannot redirect a card
            # to another repository, base, branch, or PR.
            binding = self.journal.stage_binding_for_card(
                card_id, mission_id=self.mission_id, stage_id=stage_id
            )
            state = record["state"]
            if not isinstance(binding, Mapping):
                items.append(
                    RecoveryItem(card_id, state, "BLOCKED", "stage_binding_missing")
                )
                continue
            try:
                repository = binding["repository"]
                pr_number = binding["pr_number"]
                base_sha = binding["base_sha"]
                head_sha = binding["head_sha"]
                base_branch = binding.get("base_branch")
                head_branch = binding.get("head_branch")
                if not isinstance(base_branch, str) or not isinstance(head_branch, str):
                    raise GitHubFactsError("stage_binding_branch_missing")
                registered = mission_contract.validate_registered_campaign()
                if (
                    repository != registered.repository_identity.repository
                    or base_sha != registered.repository_identity.base_sha
                    or base_branch != registered.repository_identity.branch
                ):
                    raise GitHubFactsError("stage_binding_mission_mismatch")
                facts = self.github.fetch_stage_pr(repository, pr_number)
                status = reconcile_stage_pr(
                    facts,
                    repository=repository,
                    pr_number=pr_number,
                    expected_base_sha=base_sha,
                    expected_head_sha=head_sha,
                    expected_base_branch=base_branch,
                    expected_head_branch=head_branch,
                )
                if (
                    status.outcome in {"WAITING_FOR_MERGE", "COMPLETE"}
                    and (card_id, stage_id, head_sha) not in review_bound_heads
                ):
                    status = StagePRStatus(
                        "BLOCKED",
                        "review_binding_missing",
                        status.repository,
                        status.pr_number,
                        status.base_sha,
                        status.head_sha,
                    )
            except (GitHubReadError, OSError):
                status = StagePRStatus(
                    "WAITING",
                    "github_facts_unavailable_or_invalid",
                    str(binding.get("repository", "unknown/unknown")),
                    int(binding.get("pr_number", 1)) if str(binding.get("pr_number", "")).isdigit() else 1,
                    str(binding.get("base_sha", "0" * 40)),
                    str(binding.get("head_sha", "0" * 40)),
                )
            except (KeyError, TypeError, GitHubFactsError):
                status = StagePRStatus(
                    "BLOCKED",
                    "github_facts_unavailable_or_invalid",
                    str(binding.get("repository", "unknown/unknown")),
                    int(binding.get("pr_number", 1)) if str(binding.get("pr_number", "")).isdigit() else 1,
                    str(binding.get("base_sha", "0" * 40)),
                    str(binding.get("head_sha", "0" * 40)),
                )
            if status.outcome == "COMPLETE" and state == "WAITING_FOR_MERGE":
                self.journal.append(
                    event="STAGE_MERGED_OBSERVED",
                    idempotency_key=_reconcile_key(
                        "merged", self.mission_id, stage_id, card_id, status.head_sha
                    ),
                    mission_id=self.mission_id,
                    stage_id=stage_id,
                    card_id=card_id,
                    state="COMPLETE",
                    detail="live_pr_merged",
                )
            elif status.outcome == "WAITING_FOR_MERGE" and state == "REVIEWING":
                self.journal.append(
                    event="STAGE_WAITING_FOR_MERGE",
                    idempotency_key=_reconcile_key(
                        "waiting", self.mission_id, stage_id, card_id, status.head_sha
                    ),
                    mission_id=self.mission_id,
                    stage_id=stage_id,
                    card_id=card_id,
                    state="WAITING_FOR_MERGE",
                    detail="reconciled_exact_head_ci_and_review_pass",
                    data={"pr_number": status.pr_number},
                )
            elif status.outcome == "BLOCKED" and state not in {"BLOCKED", "OUTCOME_UNKNOWN"}:
                self.journal.append(
                    event="RECONCILIATION_BLOCKED",
                    idempotency_key=_reconcile_key(
                        "blocked", self.mission_id, stage_id, card_id, status.reason
                    ),
                    mission_id=self.mission_id,
                    stage_id=stage_id,
                    card_id=card_id,
                    state="BLOCKED",
                    detail=status.reason,
                )
            items.append(
                RecoveryItem(card_id, state, status.outcome, status.reason)
            )
        return ReconciliationReport(
            _now(), tuple(items), self.journal.projection(mission_id=self.mission_id)
        )


def main(argv: list[str] | None = None) -> int:
    """Run heartbeat plus read-only recovery/reconciliation."""

    parser = argparse.ArgumentParser(prog="steward-service")
    parser.add_argument("--heartbeat-loop", action="store_true")
    parser.add_argument("--once", action="store_true")
    parser.add_argument(
        "--journal",
        default=os.environ.get("STEWARD_JOURNAL_PATH", "/var/lib/agent-steward/steward.sqlite3"),
    )
    parser.add_argument(
        "--mission-id",
        default="AUTONOMOUS-STEWARD-MIGRATION-2026-08-27",
    )
    parser.add_argument("--interval-seconds", type=int, default=60)
    args = parser.parse_args(argv)
    if not args.heartbeat_loop:
        parser.error("--heartbeat-loop is required")
    if not args.once and not 5 <= args.interval_seconds <= 3600:
        parser.error("--interval-seconds must be between 5 and 3600")
    service = StewardService(
        mission_id=args.mission_id,
        journal=StewardJournal(args.journal),
        github=GhReadOnlyGitHub(),
        repo_path=Path.cwd(),
    )
    tick = 0
    while True:
        tick += 1
        service.heartbeat(tick_id=f"heartbeat:{tick}")
        service.recover()
        service.reconcile(stage_bindings={})
        if args.once:
            return 0
        time.sleep(args.interval_seconds)


__all__ = ["RecoveryItem", "ReconciliationReport", "StewardService", "main"]


if __name__ == "__main__":
    raise SystemExit(main())
