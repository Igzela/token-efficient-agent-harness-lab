"""Heartbeat, restart recovery, and read-only reconciliation for Steward."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timezone
import argparse
import os
import time
from typing import Any, Mapping

from steward_github import (
    GitHubFactsError,
    GitHubReadError,
    ReadOnlyGitHub,
    StagePRStatus,
    reconcile_stage_pr,
)
from steward_journal import JournalError, StewardJournal


def _bounded_key(value: str) -> str:
    if not isinstance(value, str) or not value or len(value) > 128 or "\n" in value:
        raise ValueError("idempotency key is invalid")
    return value


def _now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")


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
    ):
        self.mission_id = mission_id
        self.journal = journal
        self.github = github

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

    def recover(self) -> ReconciliationReport:
        """Rebuild local state and mark in-flight work for inspection.

        A worker-started card is never replayed blindly.  Only a subsequent
        explicit ``reconcile`` call with live read-only facts can converge it.
        """

        projection = self.journal.projection()
        items = tuple(
            RecoveryItem(
                card,
                state,
                "RECOVERY_REQUIRED" if state in {"RUNNING", "VERIFYING", "REVIEWING"} else "REBUILT",
                "in_flight_work_requires_read_only_reconciliation"
                if state in {"RUNNING", "VERIFYING", "REVIEWING"}
                else "journal_projection_rebuilt",
            )
            for card, state in sorted(projection["card_states"].items())
        )
        return ReconciliationReport(_now(), items, projection)

    def reconcile(
        self,
        *,
        stage_bindings: Mapping[str, Mapping[str, Any]],
    ) -> ReconciliationReport:
        """Read live PR facts and append only observed, idempotent transitions."""

        projection = self.journal.projection()
        review_bound_heads = {
            (event.card_id, event.data["reviewed_head_sha"])
            for event in self.journal.replay()
            if event.event == "REVIEW_PASSED"
            and isinstance(event.data.get("implementation_session_id"), str)
            and isinstance(event.data.get("reviewer_session_id"), str)
            and event.data["implementation_session_id"] != event.data["reviewer_session_id"]
            and isinstance(event.data.get("reviewed_head_sha"), str)
        }
        items: list[RecoveryItem] = []
        for card_id in projection["active_cards"]:
            binding = stage_bindings.get(card_id)
            state = projection["card_states"][card_id]
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
                facts = self.github.fetch_stage_pr(repository, pr_number)
                status = reconcile_stage_pr(
                    facts,
                    repository=repository,
                    pr_number=pr_number,
                    expected_base_sha=base_sha,
                    expected_head_sha=head_sha,
                )
                if (
                    status.outcome in {"WAITING_FOR_MERGE", "COMPLETE"}
                    and (card_id, head_sha) not in review_bound_heads
                ):
                    status = StagePRStatus(
                        "BLOCKED",
                        "review_binding_missing",
                        status.repository,
                        status.pr_number,
                        status.base_sha,
                        status.head_sha,
                    )
            except (KeyError, TypeError, GitHubFactsError, GitHubReadError, OSError):
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
                    idempotency_key=f"reconcile:merged:{card_id}:{status.head_sha}",
                    mission_id=self.mission_id,
                    stage_id=f"stage:{card_id}",
                    card_id=card_id,
                    state="COMPLETE",
                    detail="live_pr_merged",
                )
            elif status.outcome == "WAITING_FOR_MERGE" and state == "REVIEWING":
                self.journal.append(
                    event="STAGE_WAITING_FOR_MERGE",
                    idempotency_key=f"reconcile:waiting:{card_id}:{status.head_sha}",
                    mission_id=self.mission_id,
                    stage_id=f"stage:{card_id}",
                    card_id=card_id,
                    state="WAITING_FOR_MERGE",
                    detail="reconciled_exact_head_ci_and_review_pass",
                    data={"pr_number": status.pr_number},
                )
            elif status.outcome == "BLOCKED" and state not in {"BLOCKED", "OUTCOME_UNKNOWN"}:
                self.journal.append(
                    event="RECONCILIATION_BLOCKED",
                    idempotency_key=f"reconcile:blocked:{card_id}:{status.reason}",
                    mission_id=self.mission_id,
                    stage_id=f"stage:{card_id}",
                    card_id=card_id,
                    state="BLOCKED",
                    detail=status.reason,
                )
            items.append(
                RecoveryItem(card_id, state, status.outcome, status.reason)
            )
        return ReconciliationReport(_now(), tuple(items), self.journal.projection())


class _HeartbeatOnlyGitHub:
    """Service CLI dependency; it cannot satisfy a PR read by accident."""

    def fetch_stage_pr(self, repository: str, pr_number: int) -> dict[str, Any]:
        raise GitHubReadError("heartbeat_only_service_has_no_github_reader")


def main(argv: list[str] | None = None) -> int:
    """Run a liveness-only loop suitable for an explicitly installed unit."""

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
        github=_HeartbeatOnlyGitHub(),
    )
    tick = 0
    while True:
        tick += 1
        service.heartbeat(tick_id=f"heartbeat:{tick}")
        if args.once:
            return 0
        time.sleep(args.interval_seconds)


__all__ = ["RecoveryItem", "ReconciliationReport", "StewardService", "main"]


if __name__ == "__main__":
    raise SystemExit(main())
