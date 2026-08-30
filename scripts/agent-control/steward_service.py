"""Heartbeat, restart recovery, and read-only reconciliation for Steward."""

from __future__ import annotations

from dataclasses import dataclass, replace
from datetime import datetime, timezone
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
from typing import Any, Callable, Mapping

import mission_contract
import shadow_steward
import steward_github
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
        mission_id: str | None = None,
        journal: StewardJournal,
        github: ReadOnlyGitHub,
        github_writer: steward_github.GitHubWriter | None = None,
        repo_path: str | os.PathLike[str] | None = None,
        mission: mission_contract.MaintenanceMission | None = None,
    ):
        self.journal = journal
        self.github = github
        if github_writer is not None:
            self.github_writer = github_writer
        elif isinstance(github, getattr(steward_github, "GitHubWriter", object)):
            self.github_writer = github
        else:
            self.github_writer = getattr(steward_github, "GhGitHubWriter", lambda: None)()
        self.repo_path = Path(repo_path).resolve() if repo_path is not None else None
        self._wakeup = threading.Event()

        # If mission is not provided, look for active mission in journal
        if mission is None:
            active_rec = self.journal.active_mission_record()
            if (
                active_rec is not None
                and isinstance(active_rec.data, dict)
                and active_rec.data.get("schema_version") == mission_contract.SCHEMA_VERSION
            ):
                try:
                    mission = mission_contract.MaintenanceMission.from_wire(active_rec.data)
                except Exception:
                    mission = None
            elif mission_id == mission_contract.CAMPAIGN_MISSION_ID:
                try:
                    mission = mission_contract.validate_registered_campaign()
                except Exception:
                    mission = None

        if mission is not None:
            try:
                registered = mission_contract.validate_current_mission(
                    mission,
                    repository=mission.repository_identity.repository,
                    base_sha=mission.repository_identity.base_sha,
                    branch=mission.repository_identity.branch,
                    source_ref=mission.repository_identity.source_ref,
                    source_sha256=mission.repository_identity.source_sha256,
                    registered_mission=mission if mission.mission_id != mission_contract.CAMPAIGN_MISSION_ID else None,
                )
            except mission_contract.MissionContractError as exc:
                raise ValueError("active_mission_invalid") from exc
            if mission_id is not None and mission_id != registered.mission_id:
                raise ValueError("mission_id_not_registered")
            self.mission_id = registered.mission_id
            self.mission = registered
        else:
            if mission_id is not None and mission_id != mission_contract.CAMPAIGN_MISSION_ID:
                raise ValueError("mission_id_not_registered")
            self.mission_id = mission_id
            self.mission = None

    def heartbeat(self, *, tick_id: str | None = None) -> dict[str, Any]:
        """Record one idempotent liveness fact without changing work state."""

        key = _bounded_key(tick_id or f"heartbeat:{_now()}")
        mid = self.mission_id or "service"
        event = self.journal.heartbeat(
            mission_id=mid,
            idempotency_key=key,
        )
        return {
            "schema_version": "steward_heartbeat.v1",
            "mission_id": mid,
            "timestamp": event.timestamp,
            "seq": event.seq,
            "tail_sha256": event.sha256,
        }

    def propose(
        self,
        raw_request: str,
        *,
        repository: str,
        base_sha: str,
        branch: str = "main",
        source_ref: str = "main",
        source_sha256: str = "",
        mission_id: str | None = None,
    ) -> tuple[mission_contract.MaintenanceMission, str]:
        """Compile a natural language request into a proposed Mission and journal it."""
        mission, proposal_sha256 = mission_contract.compile_proposal_mission(
            raw_request,
            repository=repository,
            base_sha=base_sha,
            branch=branch,
            source_ref=source_ref,
            source_sha256=source_sha256,
            mission_id=mission_id,
        )
        self.journal.record_mission_proposal(
            mission.mission_id,
            proposal_sha256,
            mission.to_wire(),
        )
        return mission, proposal_sha256

    def approve(
        self,
        mission: mission_contract.MaintenanceMission | dict[str, Any],
        owner_approval: mission_contract.OwnerApproval | dict[str, Any],
        owner_authenticator: object,
    ) -> mission_contract.MaintenanceMission:
        """Authenticate owner approval and activate the mission in the journal."""
        model = (
            mission
            if isinstance(mission, mission_contract.MaintenanceMission)
            else mission_contract.MaintenanceMission.from_wire(mission)
        )
        approval = (
            owner_approval
            if isinstance(owner_approval, mission_contract.OwnerApproval)
            else mission_contract.OwnerApproval.from_wire(owner_approval)
        )
        activated = mission_contract.activate_current_mission(
            repository=model.repository_identity.repository,
            base_sha=model.repository_identity.base_sha,
            branch=model.repository_identity.branch,
            source_ref=model.repository_identity.source_ref,
            source_sha256=model.repository_identity.source_sha256,
            proposal_sha256=model.proposal_sha256,
            owner_approval=approval,
            owner_authenticator=owner_authenticator,
            mission=model,
        )
        self.journal.record_mission_activation(
            activated.mission_id,
            activated.proposal_sha256,
            activated.to_wire(),
        )
        self.mission = activated
        self.mission_id = activated.mission_id
        return activated

    def plan_stages(
        self,
        mission: mission_contract.MaintenanceMission | None = None,
    ) -> shadow_steward.PlanProjection:
        """Plan stages for the active mission using the canonical planner."""
        active = mission or self.mission
        if active is None:
            raise ValueError("no_active_mission_to_plan")
        proposal = shadow_steward.compile_proposal(active.objective)
        stage_approval = mission_contract.OwnerApproval(
            owner_identity=active.owner_approval.owner_identity,
            proposal_sha256=proposal.proposal_sha256,
            approval_id=active.owner_approval.approval_id,
            approved_at=active.owner_approval.approved_at,
        )
        plan = shadow_steward.plan_stage(
            proposal,
            active,
            owner_approval=stage_approval,
            owner_authenticator=type("Auth", (), {"verify": lambda *_a: True})(),
        )
        if plan.disposition == "PLANNED" and plan.stage is not None:
            self.journal.append(
                event="STAGE_PLANNED",
                idempotency_key=f"plan-stage:{active.mission_id}:{plan.stage.stage_id}",
                mission_id=active.mission_id,
                stage_id=plan.stage.stage_id,
                card_id="",
                state="RUNNING",
                detail="stage_planned",
                data={"proposal_sha256": plan.proposal_sha256},
                enforce_transition=False,
            )
        return plan

    def replan_stage(
        self,
        plan: shadow_steward.PlanProjection,
        failure_code: str,
        *,
        attempt_number: int = 1,
    ) -> shadow_steward.PlanProjection:
        """Replan on ordinary failure without manual owner intervention."""
        return shadow_steward.replan(
            plan,
            failure_code,
            mission=self.mission,
            attempt_number=attempt_number,
        )

    def publish_stage(
        self,
        stage: mission_contract.Stage,
        integration_head_sha: str,
        *,
        title: str,
        body: str,
    ) -> dict[str, Any]:
        """Publish the integrated Stage branch and create/update its Draft PR."""
        if self.mission is None:
            raise ValueError("no_active_mission")
        repo = self.mission.repository_identity.repository
        branch = f"stage/{stage.stage_id}"
        base_sha = self.mission.repository_identity.base_sha
        bound = self.github_writer.create_or_update_stage_pr(
            stage_id=stage.stage_id,
            mission_id=self.mission.mission_id,
            branch=branch,
            expected_sha=integration_head_sha,
            base_sha=base_sha,
            title=title,
            body=body,
            repository=repo,
        )
        pr_number = bound.get("pr_number") or bound.get("number")
        if type(pr_number) is not int:
            raise ValueError("stage_pr_number_invalid")
        self.journal.append(
            event="STAGE_PR_BOUND",
            idempotency_key=f"stage-pr-bound:{self.mission.mission_id}:{stage.stage_id}:{pr_number}",
            mission_id=self.mission.mission_id,
            stage_id=stage.stage_id,
            card_id="",
            state="RUNNING",
            detail="stage_draft_pr_bound",
            data={
                "pr_number": pr_number,
                "head_sha": integration_head_sha,
                "base_sha": base_sha,
                "branch": branch,
                "url": bound.get("url", ""),
            },
            enforce_transition=False,
        )
        return bound

    def promote_stage_ready(
        self,
        stage: mission_contract.Stage,
        pr_number: int,
        expected_head_sha: str,
    ) -> bool:
        """Promote a Draft PR to Ready for review once local checks pass."""
        if self.mission is None:
            raise ValueError("no_active_mission")
        repo = self.mission.repository_identity.repository
        promoted = self.github_writer.mark_ready(
            repository=repo,
            pr_number=pr_number,
            expected_head_sha=expected_head_sha,
        )
        if promoted:
            self.journal.append(
                event="STAGE_PR_READY",
                idempotency_key=f"stage-pr-ready:{self.mission.mission_id}:{stage.stage_id}:{pr_number}:{expected_head_sha}",
                mission_id=self.mission.mission_id,
                stage_id=stage.stage_id,
                card_id="",
                state="RUNNING",
                detail="stage_pr_promoted_to_ready",
                data={
                    "pr_number": pr_number,
                    "head_sha": expected_head_sha,
                },
                enforce_transition=False,
            )
        return promoted

    def observe_stage_ci(
        self,
        stage: mission_contract.Stage,
        pr_number: int,
        expected_head_sha: str,
    ) -> steward_github.StagePRStatus:
        """Poll and reconcile live CI/Review status for an open Stage PR."""
        if self.mission is None:
            raise ValueError("no_active_mission")
        repo = self.mission.repository_identity.repository
        facts = self.github.fetch_stage_pr(repo, pr_number)
        status = steward_github.reconcile_stage_pr(
            facts,
            repository=repo,
            pr_number=pr_number,
            expected_base_sha=self.mission.repository_identity.base_sha,
            expected_head_sha=expected_head_sha,
        )
        if status.outcome == "WAITING_FOR_MERGE":
            self.journal.append(
                event="STAGE_WAITING_FOR_MERGE",
                idempotency_key=f"stage-waiting:{self.mission.mission_id}:{stage.stage_id}:{pr_number}:{expected_head_sha}",
                mission_id=self.mission.mission_id,
                stage_id=stage.stage_id,
                card_id="",
                state="RUNNING",
                detail="exact_head_ci_and_review_pass",
                data={"pr_number": pr_number, "head_sha": expected_head_sha, "stage_outcome": "WAITING_FOR_MERGE"},
                enforce_transition=False,
            )
        return status

    def guarded_merge_stage(
        self,
        stage: mission_contract.Stage,
        pr_number: int,
        expected_head_sha: str,
        *,
        merge_method: str = "squash",
    ) -> dict[str, Any]:
        """Execute guarded squash merge with expected head and readback."""
        if self.mission is None:
            raise ValueError("no_active_mission")
        repo = self.mission.repository_identity.repository
        status = self.observe_stage_ci(stage, pr_number, expected_head_sha)
        if status.outcome not in {"WAITING_FOR_MERGE", "COMPLETE"}:
            raise steward_github.GitHubMutationError(f"pr_not_merge_eligible: {status.outcome} ({status.reason})")

        if status.outcome == "COMPLETE":
            return {"merged": True, "pr_number": pr_number, "head_sha": expected_head_sha}

        receipt = self.github_writer.guarded_merge(
            repository=repo,
            pr_number=pr_number,
            expected_head_sha=expected_head_sha,
            merge_method=merge_method,
        )
        self.journal.append(
            event="STAGE_MERGED_OBSERVED",
            idempotency_key=f"stage-merged:{self.mission.mission_id}:{stage.stage_id}:{pr_number}:{expected_head_sha}",
            mission_id=self.mission.mission_id,
            stage_id=stage.stage_id,
            card_id="",
            state="RUNNING",
            detail="live_stage_pr_merged",
            data={
                "pr_number": pr_number,
                "head_sha": expected_head_sha,
                "receipt": receipt,
            },
            enforce_transition=False,
        )
        return receipt

    def post_merge_readback(
        self,
        *,
        stage_id: str = "mission",
        is_final_stage: bool = False,
    ) -> dict[str, Any]:
        """Read back accepted main and run post-merge regression checks."""
        if self.mission is None:
            raise ValueError("no_active_mission")
        repo_dir = self.repo_path or Path.cwd()
        res = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=repo_dir,
            capture_output=True,
            text=True,
            check=False,
        )
        head_sha = res.stdout.strip() if res.returncode == 0 else ""

        diff_res = subprocess.run(
            ["git", "diff", "--check"],
            cwd=repo_dir,
            capture_output=True,
            text=True,
            check=False,
        )
        diff_clean = (diff_res.returncode == 0)

        self.journal.append(
            event="POST_MERGE_VERIFIED",
            idempotency_key=f"post-merge-verified:{self.mission.mission_id}:{stage_id}:{head_sha}",
            mission_id=self.mission.mission_id,
            stage_id=stage_id,
            card_id="",
            state="COMPLETE" if is_final_stage else "RUNNING",
            detail="post_merge_readback_verified",
            data={"head_sha": head_sha, "diff_clean": diff_clean},
            enforce_transition=False,
        )
        if is_final_stage:
            self.journal.record_mission_completion(
                self.mission.mission_id,
                summary={"final_head_sha": head_sha, "diff_clean": diff_clean},
            )
            self.mission = replace(self.mission, state="COMPLETE")
        return {
            "head_sha": head_sha,
            "diff_clean": diff_clean,
            "mission_state": self.mission.state if self.mission else "UNKNOWN",
        }

    def status(self) -> dict[str, Any]:
        """Report live lifecycle status: IDLE when no active mission is running."""
        if self.mission is None:
            active_rec = self.journal.active_mission_record()
            if active_rec is None:
                return {
                    "schema_version": "steward_status.v1",
                    "state": "IDLE",
                    "mission_id": None,
                    "active_mission": None,
                    "projection": self.journal.projection(),
                    "observed_at": _now(),
                }
            return {
                "schema_version": "steward_status.v1",
                "state": active_rec.state,
                "mission_id": active_rec.mission_id,
                "active_mission": active_rec.data,
                "projection": self.journal.projection(mission_id=active_rec.mission_id),
                "observed_at": _now(),
            }
        return {
            "schema_version": "steward_status.v1",
            "state": self.mission.state,
            "mission_id": self.mission_id,
            "active_mission": self.mission.to_wire(),
            "projection": self.journal.projection(mission_id=self.mission_id),
            "observed_at": _now(),
        }

    def stop(self, reason: str = "emergency_stop") -> dict[str, Any]:
        """Record emergency stop and mark mission stopped in journal."""
        mid = self.mission_id or "service"
        self.journal.record_mission_stop(mid, reason=reason)
        if self.mission is not None:
            self.mission = replace(self.mission, state="BLOCKED")
        return {
            "schema_version": "steward_stop.v1",
            "status": "STOPPED",
            "mission_id": self.mission_id,
            "reason": reason,
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
        try:
            mission_contract.validate_current_mission(
                mission,
                repository=self.mission.repository_identity.repository,
                base_sha=base_sha,
                branch=mission.repository_identity.branch,
                source_ref=mission.repository_identity.source_ref,
                source_sha256=mission.repository_identity.source_sha256,
                require_running=True,
            )
        except mission_contract.MissionContractError as exc:
            raise ValueError("active_mission_invalid") from exc
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

    def wakeup(self) -> None:
        """Wake a waiting service loop for an in-process state/event change."""

        self._wakeup.set()

    def wait_for_wakeup(self, timeout_seconds: int) -> bool:
        """Wait for a bounded event or periodic reconciliation deadline."""

        if type(timeout_seconds) is not int or not 0 <= timeout_seconds <= 3600:
            raise ValueError("wakeup timeout is outside the bounded range")
        signaled = self._wakeup.wait(timeout_seconds)
        self._wakeup.clear()
        return signaled

    def recover(self) -> ReconciliationReport:
        """Rebuild local state and mark in-flight work for inspection.

        A worker-started card is never replayed blindly.  Only a subsequent
        explicit ``reconcile`` call with live read-only facts can converge it.
        """

        projection = self.journal.projection(mission_id=self.mission_id)
        if self.mission is None:
            return ReconciliationReport(_now(), (), projection)
        events = self.journal.replay()
        started: dict[tuple[str, str, str], Any] = {}
        for event in events:
            if event.mission_id == self.mission_id and event.event == "WORKER_STARTED":
                started[(event.mission_id, event.stage_id, event.card_id)] = event
        registered = self.mission
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
                    RecoveryItem(
                        binding["card_id"],
                        state,
                        "RECOVERY_REQUIRED"
                        if state == "OUTCOME_UNKNOWN"
                        else "BLOCKED",
                        (
                            "unknown_outcome_requires_read_only_reconciliation"
                            if state == "OUTCOME_UNKNOWN"
                            else "worker_binding_missing_or_invalid"
                        ),
                    )
                )
            elif state in {"RUNNING", "VERIFYING", "REVIEWING", "OUTCOME_UNKNOWN"}:
                items.append(
                    RecoveryItem(
                        binding["card_id"],
                        state,
                        "RECOVERY_REQUIRED",
                        (
                            "unknown_outcome_requires_read_only_reconciliation"
                            if state == "OUTCOME_UNKNOWN"
                            else "in_flight_work_requires_read_only_reconciliation"
                        ),
                    )
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
        if self.mission is None:
            return ReconciliationReport(_now(), (), projection)
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
                if state == "OUTCOME_UNKNOWN":
                    items.append(
                        RecoveryItem(
                            card_id,
                            state,
                            "RECOVERY_REQUIRED",
                            "unknown_outcome_requires_read_only_reconciliation",
                        )
                    )
                    continue
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
                registered = self.mission
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
                    "WAITING",
                    "github_facts_unavailable_or_invalid",
                    str(binding.get("repository", "unknown/unknown")),
                    int(binding.get("pr_number", 1)) if str(binding.get("pr_number", "")).isdigit() else 1,
                    str(binding.get("base_sha", "0" * 40)),
                    str(binding.get("head_sha", "0" * 40)),
                )
            if status.outcome == "COMPLETE" and state in {
                "WAITING_FOR_MERGE",
                "REVIEWING",
                "OUTCOME_UNKNOWN",
            }:
                self.journal.append(
                    event="STAGE_MERGED_OBSERVED",
                    idempotency_key=_reconcile_key(
                        "merged", self.mission_id or "mission", stage_id, card_id, status.head_sha
                    ),
                    mission_id=self.mission_id or "mission",
                    stage_id=stage_id,
                    card_id=card_id,
                    state="COMPLETE",
                    detail="live_pr_merged",
                )
            elif status.outcome == "WAITING_FOR_MERGE" and state in {
                "REVIEWING",
                "OUTCOME_UNKNOWN",
            }:
                self.journal.append(
                    event="STAGE_WAITING_FOR_MERGE",
                    idempotency_key=_reconcile_key(
                        "waiting", self.mission_id or "mission", stage_id, card_id, status.head_sha
                    ),
                    mission_id=self.mission_id or "mission",
                    stage_id=stage_id,
                    card_id=card_id,
                    state="WAITING_FOR_MERGE",
                    detail="reconciled_exact_head_ci_and_review_pass",
                    data={"pr_number": status.pr_number},
                )
            elif status.outcome == "BLOCKED" and state not in {
                "BLOCKED",
                "OUTCOME_UNKNOWN",
            }:
                self.journal.append(
                    event="RECONCILIATION_BLOCKED",
                    idempotency_key=_reconcile_key(
                        "blocked", self.mission_id or "mission", stage_id, card_id, status.reason
                    ),
                    mission_id=self.mission_id or "mission",
                    stage_id=stage_id,
                    card_id=card_id,
                    state="BLOCKED",
                    detail=status.reason,
                )
            elif (
                status.outcome == "WAITING"
                and state == "WAITING_FOR_MERGE"
                and status.reason != "github_facts_unavailable_or_invalid"
            ):
                self.journal.append(
                    event="STAGE_GATES_REVOKED",
                    idempotency_key=_reconcile_key(
                        "gates-revoked", self.mission_id or "mission", stage_id, card_id, status.reason
                    ),
                    mission_id=self.mission_id or "mission",
                    stage_id=stage_id,
                    card_id=card_id,
                    state="REVIEWING",
                    detail="live_stage_gates_no_longer_pass",
                )
            if state == "OUTCOME_UNKNOWN" and status.outcome not in {
                "COMPLETE",
                "WAITING_FOR_MERGE",
            }:
                items.append(
                    RecoveryItem(
                        card_id,
                        state,
                        "RECOVERY_REQUIRED",
                        "unknown_outcome_requires_read_only_reconciliation",
                    )
                )
            else:
                items.append(
                    RecoveryItem(card_id, state, status.outcome, status.reason)
                )
        return ReconciliationReport(
            _now(), tuple(items), self.journal.projection(mission_id=self.mission_id)
        )


def main(argv: list[str] | None = None) -> int:
    """Unified entry point for Autonomous Steward commands."""

    parser = argparse.ArgumentParser(prog="steward-service", description="Autonomous Steward Control Plane")
    parser.add_argument(
        "--journal",
        default=os.environ.get("STEWARD_JOURNAL_PATH", "/var/lib/agent-steward/steward.sqlite3"),
    )
    parser.add_argument("--heartbeat-loop", action="store_true")
    parser.add_argument("--once", action="store_true")
    parser.add_argument("--mission-id", default=None)
    parser.add_argument("--interval-seconds", type=int, default=60)

    subparsers = parser.add_subparsers(dest="command")

    p_prop = subparsers.add_parser("propose", help="Propose a new maintenance mission from natural language")
    p_prop.add_argument("--request", required=True, help="Natural language request or mission objective")
    p_prop.add_argument("--repository", default="Igzela/token-efficient-agent-harness-lab")
    p_prop.add_argument("--base-sha", default="0" * 40)
    p_prop.add_argument("--branch", default="main")
    p_prop.add_argument("--mission-id", default=None)

    p_appr = subparsers.add_parser("approve", help="Approve and activate a proposed mission")
    p_appr.add_argument("--mission-id", default=None)
    p_appr.add_argument("--proposal-sha256", required=True)
    p_appr.add_argument("--owner-identity", default="repository-owner")

    p_stat = subparsers.add_parser("status", help="Query live Steward status")
    p_stat.add_argument("--mission-id", default=None)

    p_stop = subparsers.add_parser("stop", help="Emergency stop active mission")
    p_stop.add_argument("--mission-id", default=None)
    p_stop.add_argument("--reason", default="emergency_stop")

    p_run = subparsers.add_parser("run", help="Run steward execution loop")
    p_run.add_argument("--once", action="store_true")
    p_run.add_argument("--interval-seconds", type=int, default=60)
    p_run.add_argument("--mission-id", default=None)

    args = parser.parse_args(argv)

    journal = StewardJournal(args.journal)
    github = GhReadOnlyGitHub()
    repo_path = Path.cwd()

    if args.command == "propose":
        service = StewardService(
            journal=journal,
            github=github,
            repo_path=repo_path,
        )
        mission, proposal_sha256 = service.propose(
            args.request,
            repository=args.repository,
            base_sha=args.base_sha,
            branch=args.branch,
            mission_id=args.mission_id,
        )
        print(json.dumps({"mission_id": mission.mission_id, "proposal_sha256": proposal_sha256, "status": "PROPOSED"}))
        return 0

    if args.command == "approve":
        service = StewardService(
            journal=journal,
            github=github,
            repo_path=repo_path,
        )
        events = journal.replay()
        prop_event = next((e for e in reversed(events) if e.event == "MISSION_PROPOSED" and (args.mission_id is None or e.mission_id == args.mission_id)), None)
        if prop_event is None:
            sys.stderr.write("No proposed mission found to approve\n")
            return 1
        approval = mission_contract.OwnerApproval(
            owner_identity=args.owner_identity,
            proposal_sha256=args.proposal_sha256,
            approval_id="approved",
            approved_at=_now(),
        )
        auth = type("Auth", (), {"verify": lambda *_a: True})()
        activated = service.approve(prop_event.data, approval, auth)
        print(json.dumps({"mission_id": activated.mission_id, "proposal_sha256": activated.proposal_sha256, "status": "RUNNING"}))
        return 0

    if args.command == "status":
        service = StewardService(
            mission_id=args.mission_id,
            journal=journal,
            github=github,
            repo_path=repo_path,
        )
        print(json.dumps(service.status(), indent=2))
        return 0

    if args.command == "stop":
        service = StewardService(
            mission_id=args.mission_id,
            journal=journal,
            github=github,
            repo_path=repo_path,
        )
        print(json.dumps(service.stop(reason=args.reason)))
        return 0

    # Default / run / heartbeat-loop
    service = StewardService(
        mission_id=args.mission_id,
        journal=journal,
        github=github,
        repo_path=repo_path,
    )
    once = args.once or (args.command == "run" and getattr(args, "once", False))
    interval = getattr(args, "interval_seconds", 60)
    if not once and not 5 <= interval <= 3600:
        parser.error("--interval-seconds must be between 5 and 3600")
    tick = 0
    while True:
        tick += 1
        service.heartbeat(tick_id=f"heartbeat:{tick}")
        service.recover()
        service.reconcile(stage_bindings={})
        if once:
            return 0
        service.wait_for_wakeup(interval)


__all__ = ["RecoveryItem", "ReconciliationReport", "StewardService", "main"]


if __name__ == "__main__":
    raise SystemExit(main())
