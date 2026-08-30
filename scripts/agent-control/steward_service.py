"""Autonomous Steward Service: Control plane lifecycle and autonomous loop."""

from __future__ import annotations

from dataclasses import dataclass, replace
from datetime import datetime, timezone
import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import threading
from typing import Any, Callable, Mapping, Protocol
import uuid

import mission_contract
import shadow_steward
import steward_github
from steward_github import (
    GhGitHubWriter,
    GhReadOnlyGitHub,
    GitHubFactsError,
    GitHubMutationError,
    GitHubPreflightError,
    GitHubReadError,
    GitHubWriter,
    ReadOnlyGitHub,
    StagePRStatus,
    reconcile_stage_pr,
)
from steward_journal import JournalError, StewardJournal
import steward_workers
from steward_workers import (
    ReviewOutcome,
    ReviewerAdapter,
    WorkerAdapter,
    WorkerContext,
    WorkerError,
    WorkerOutcome,
    general_reviewer,
    general_worker,
    production_reviewer,
    production_worker,
)
import worktree_manager
from review_loop.locking import ChatLock, LockBusy


class StewardServiceError(RuntimeError):
    """Base exception for Steward service failures."""


_OWNER_APPROVAL_MARKER = re.compile(
    r"<!--\s*steward-owner-approval:v2\s+(\{.*?\})\s*-->", re.DOTALL
)


class OwnerApprovalSource(Protocol):
    """Read one externally authenticated approval; never create one."""

    def read(
        self,
        *,
        repository: str,
        issue_number: int,
        comment_id: int,
        mission_id: str,
        proposal_sha256: str,
        accepted_main_sha: str,
    ) -> mission_contract.OwnerApprovalEvidence:
        ...


class ControlStateSource(Protocol):
    """Read the canonical emergency-stop control; never infer it locally."""

    def emergency_stop_active(self, *, repository: str, issue_number: int) -> bool:
        ...


class GhControlStateSource:
    """Issue-label-backed control state used by the production service."""

    def __init__(self, *, timeout_seconds: int = 30):
        if type(timeout_seconds) is not int or not 1 <= timeout_seconds <= 300:
            raise ValueError("control_source_timeout_invalid")
        self.timeout_seconds = timeout_seconds

    def emergency_stop_active(self, *, repository: str, issue_number: int) -> bool:
        if mission_contract.REPOSITORY.fullmatch(repository) is None or type(issue_number) is not int or issue_number < 1:
            raise StewardServiceError("control_state_request_invalid")
        try:
            result = subprocess.run(
                ["gh", "api", f"repos/{repository}/issues/{issue_number}"],
                capture_output=True,
                text=True,
                timeout=self.timeout_seconds,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise StewardServiceError("control_state_unavailable") from exc
        if result.returncode != 0:
            raise StewardServiceError("control_state_read_failed")
        try:
            issue = json.loads(result.stdout)
        except json.JSONDecodeError as exc:
            raise StewardServiceError("control_state_malformed") from exc
        labels = issue.get("labels") if isinstance(issue, dict) else None
        if not isinstance(labels, list):
            raise StewardServiceError("control_state_malformed")
        return any(isinstance(label, dict) and label.get("name") == "agent-emergency-stop" for label in labels)


class GhOwnerApprovalSource:
    """GitHub-owner-comment approval transport.

    The GitHub API, not caller input, supplies the author identity.  The
    marker is only a signed-by-transport binding payload: it cannot turn a
    locally constructed ``OwnerApproval`` into authenticated evidence.
    """

    def __init__(self, *, timeout_seconds: int = 30):
        if type(timeout_seconds) is not int or not 1 <= timeout_seconds <= 300:
            raise ValueError("approval_source_timeout_invalid")
        self.timeout_seconds = timeout_seconds

    def _json(self, *args: str) -> dict[str, Any]:
        try:
            result = subprocess.run(
                ["gh", "api", *args],
                capture_output=True,
                text=True,
                timeout=self.timeout_seconds,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise StewardServiceError("approval_transport_unavailable") from exc
        if result.returncode != 0:
            raise StewardServiceError("approval_transport_read_failed")
        try:
            value = json.loads(result.stdout)
        except (TypeError, json.JSONDecodeError) as exc:
            raise StewardServiceError("approval_transport_malformed") from exc
        if not isinstance(value, dict):
            raise StewardServiceError("approval_transport_malformed")
        return value

    def read(
        self,
        *,
        repository: str,
        issue_number: int,
        comment_id: int,
        mission_id: str,
        proposal_sha256: str,
        accepted_main_sha: str,
    ) -> mission_contract.OwnerApprovalEvidence:
        if (
            mission_contract.REPOSITORY.fullmatch(repository) is None
            or type(issue_number) is not int
            or issue_number < 1
            or type(comment_id) is not int
            or comment_id < 1
            or mission_contract.IDENTIFIER.fullmatch(mission_id) is None
            or mission_contract.SHA256.fullmatch(proposal_sha256) is None
            or mission_contract.SHA40.fullmatch(accepted_main_sha) is None
        ):
            raise StewardServiceError("approval_request_invalid")
        # Fresh branch authority is checked before the comment is accepted;
        # stale approvals never enter the replay journal.
        branch = self._json(f"repos/{repository}/branches/main")
        current_main = branch.get("commit", {}).get("sha") if isinstance(branch.get("commit"), dict) else None
        if current_main != accepted_main_sha:
            raise StewardServiceError("approval_accepted_main_stale")
        comment = self._json(f"repos/{repository}/issues/comments/{comment_id}")
        if comment.get("id") != comment_id:
            raise StewardServiceError("approval_comment_identity_mismatch")
        expected_issue_url = f"https://api.github.com/repos/{repository}/issues/{issue_number}"
        if comment.get("issue_url") != expected_issue_url:
            raise StewardServiceError("approval_comment_issue_mismatch")
        if str(comment.get("author_association", "")).upper() != "OWNER":
            raise StewardServiceError("approval_comment_owner_required")
        author = comment.get("user")
        login = author.get("login") if isinstance(author, dict) else None
        if not isinstance(login, str) or not login:
            raise StewardServiceError("approval_comment_author_invalid")
        body = comment.get("body")
        if not isinstance(body, str) or len(body) > 16 * 1024:
            raise StewardServiceError("approval_comment_body_invalid")
        match = _OWNER_APPROVAL_MARKER.search(body)
        if match is None:
            raise StewardServiceError("approval_comment_marker_missing")
        try:
            marker = json.loads(match.group(1))
        except json.JSONDecodeError as exc:
            raise StewardServiceError("approval_comment_marker_invalid") from exc
        if not isinstance(marker, dict) or set(marker) != {
            "mission_id", "proposal_sha256", "accepted_main_sha", "approval_id"
        }:
            raise StewardServiceError("approval_comment_marker_invalid")
        if (
            marker.get("mission_id") != mission_id
            or marker.get("proposal_sha256") != proposal_sha256
            or marker.get("accepted_main_sha") != accepted_main_sha
            or not isinstance(marker.get("approval_id"), str)
            or mission_contract.IDENTIFIER.fullmatch(marker["approval_id"]) is None
        ):
            raise StewardServiceError("approval_comment_binding_mismatch")
        return mission_contract.OwnerApprovalEvidence(
            transport="github_issue_comment",
            repository=repository,
            mission_id=mission_id,
            approval_id=marker["approval_id"],
            owner_identity=f"github:{login}",
            proposal_sha256=proposal_sha256,
            accepted_main_sha=accepted_main_sha,
            evidence_id=f"github-comment-{comment_id}",
        )


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
        github_writer: GitHubWriter | None = None,
        repo_path: str | os.PathLike[str] | None = None,
        mission: mission_contract.MaintenanceMission | None = None,
        control_state: ControlStateSource | None = None,
        control_issue_number: int = 208,
    ):
        self.journal = journal
        self.github = github
        self.github_writer = github_writer or getattr(steward_github, "GhGitHubWriter", lambda: None)()
        self.repo_path = Path(repo_path).resolve() if repo_path is not None else Path.cwd().resolve()
        if type(control_issue_number) is not int or control_issue_number < 1:
            raise ValueError("control_issue_number_invalid")
        self.control_state = control_state or GhControlStateSource()
        self.control_issue_number = control_issue_number
        self._service_lease_held = False
        self._lease_epoch = 0
        self._lease_owner_id = uuid.uuid4().hex[:16]
        self._wakeup = threading.Event()

        registered = None
        if mission is not None:
            try:
                registered = (
                    mission_contract.validate_registered_campaign()
                    if mission.mission_id == mission_contract.CAMPAIGN_MISSION_ID
                    else mission_contract.validate_current_mission(
                        mission,
                        repository=mission.repository_identity.repository,
                        base_sha=mission.repository_identity.base_sha,
                        branch=mission.repository_identity.branch,
                        source_ref=mission.repository_identity.source_ref,
                        source_sha256=mission.repository_identity.source_sha256,
                    )
                )
            except mission_contract.MissionContractError as exc:
                raise ValueError("active_mission_invalid") from exc
            if mission_id is not None and mission_id != registered.mission_id:
                raise ValueError("mission_id_not_registered")
        elif mission_id is not None:
            if mission_id == mission_contract.CAMPAIGN_MISSION_ID:
                try:
                    registered = mission_contract.validate_registered_campaign()
                except mission_contract.MissionContractError as exc:
                    raise ValueError("active_mission_invalid") from exc
            else:
                rec = self.journal.active_mission_record()
                if rec is not None and rec.mission_id == mission_id and rec.data:
                    try:
                        registered = self._restore_journal_mission(rec)
                    except Exception:
                        pass
                if registered is None:
                    raise ValueError("mission_id_not_registered")
        else:
            rec = self.journal.active_mission_record()
            if rec is not None and rec.data:
                try:
                    registered = self._restore_journal_mission(rec)
                except Exception:
                    pass

        self.mission = registered
        self.mission_id = mission_id or (registered.mission_id if registered else None)

    @staticmethod
    def _restore_journal_mission(record: Any) -> mission_contract.MaintenanceMission:
        """Rehydrate only a durable activation or accepted-main rebind."""

        model = mission_contract.MaintenanceMission.from_wire(record.data)
        if record.event not in {"MISSION_ACTIVATED", "MISSION_BASE_ADVANCED", "MISSION_BASE_DRIFT_REBOUND"} or record.state != "RUNNING":
            raise StewardServiceError("journal_active_mission_invalid")
        if record.mission_id != model.mission_id or model.state != "RUNNING":
            raise StewardServiceError("journal_active_mission_invalid")
        return mission_contract.restore_durable_activation(model)

    def heartbeat(self, *, tick_id: str | None = None) -> dict[str, Any]:
        """Record one idempotent liveness fact without changing work state."""

        mid = self.mission_id or "service"
        key = _bounded_key(tick_id or f"heartbeat:{_now()}")
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
        base_sha: str | None = None,
        branch: str = "main",
        source_ref: str = "main",
        source_sha256: str = "",
        mission_id: str | None = None,
    ) -> tuple[mission_contract.MaintenanceMission, str]:
        """Serialize natural-language Mission proposal under the journal lease."""

        with self._service_lease():
            return self._propose_once(
                raw_request,
                repository=repository,
                base_sha=base_sha,
                branch=branch,
                source_ref=source_ref,
                source_sha256=source_sha256,
                mission_id=mission_id,
            )

    def _propose_once(
        self,
        raw_request: str,
        *,
        repository: str,
        base_sha: str | None = None,
        branch: str = "main",
        source_ref: str = "main",
        source_sha256: str = "",
        mission_id: str | None = None,
    ) -> tuple[mission_contract.MaintenanceMission, str]:
        """Compile and record a proposed mission from natural language."""

        if base_sha is None:
            read_main = getattr(self.github, "fetch_accepted_main", None)
            if not callable(read_main):
                raise StewardServiceError("accepted_main_authority_required_for_proposal")
            try:
                base_sha = read_main(repository)
            except (GitHubFactsError, GitHubReadError, OSError) as exc:
                raise StewardServiceError("accepted_main_authority_unavailable_for_proposal") from exc
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
            mission_id=mission.mission_id,
            proposal_sha256=proposal_sha256,
            mission_data=mission.to_wire(),
        )
        self.mission = mission
        self.mission_id = mission.mission_id
        return mission, proposal_sha256

    def approve(
        self,
        proposal_mission: mission_contract.MaintenanceMission | dict[str, Any],
        *,
        approval_comment_id: int,
        control_issue_number: int | None = None,
        approval_source: OwnerApprovalSource | None = None,
    ) -> mission_contract.MaintenanceMission:
        """Serialize externally authenticated approval consumption and activation."""

        with self._service_lease():
            return self._approve_once(
                proposal_mission,
                approval_comment_id=approval_comment_id,
                control_issue_number=control_issue_number,
                approval_source=approval_source,
            )

    def _approve_once(
        self,
        proposal_mission: mission_contract.MaintenanceMission | dict[str, Any],
        *,
        approval_comment_id: int,
        control_issue_number: int | None = None,
        approval_source: OwnerApprovalSource | None = None,
    ) -> mission_contract.MaintenanceMission:
        """Activate a proposal from one already-authenticated owner comment.

        The required ordering is intentional: first validate every external
        identity/digest/base fact, then atomically consume its unique approval
        identity in the journal, then persist the activation.  No caller can
        manufacture an ``OwnerApproval`` plus an identity-only validator to
        bypass the external GitHub transport.
        """

        model = (
            proposal_mission
            if isinstance(proposal_mission, mission_contract.MaintenanceMission)
            else mission_contract.MaintenanceMission.from_wire(proposal_mission)
        )
        proposal = next(
            (
                event
                for event in reversed(self.journal.replay())
                if event.event == "MISSION_PROPOSED" and event.mission_id == model.mission_id
            ),
            None,
        )
        if proposal is None or proposal.data.get("proposal_sha256") != model.proposal_sha256:
            raise mission_contract.MissionContractError("mission_proposal_not_registered")
        if self.journal.active_mission_record() is not None:
            raise mission_contract.MissionContractError("another_mission_already_active")
        issue_number = self.control_issue_number if control_issue_number is None else control_issue_number
        if approval_source is not None and (
            not getattr(approval_source, "__steward_test_fixture__", False)
            or not isinstance(self.github_writer, steward_github.FakeGitHubWriter)
        ):
            raise mission_contract.MissionContractError("untrusted_approval_source_forbidden")
        source = approval_source or GhOwnerApprovalSource()
        if type(approval_comment_id) is not int or approval_comment_id < 1 or not isinstance(issue_number, int) or issue_number < 1:
            raise mission_contract.MissionContractError("approval_transport_request_invalid")
        # The service obtains the attested facts itself.  It never accepts a
        # caller-created OwnerApproval or OwnerApprovalEvidence as authority.
        approval_evidence = source.read(
            repository=model.repository_identity.repository,
            issue_number=issue_number,
            comment_id=approval_comment_id,
            mission_id=model.mission_id,
            proposal_sha256=model.proposal_sha256,
            accepted_main_sha=model.repository_identity.base_sha,
        )
        if (
            not isinstance(approval_evidence, mission_contract.OwnerApprovalEvidence)
            or approval_evidence.repository != model.repository_identity.repository
            or approval_evidence.mission_id != model.mission_id
            or approval_evidence.proposal_sha256 != model.proposal_sha256
            or approval_evidence.accepted_main_sha != model.repository_identity.base_sha
        ):
            raise mission_contract.MissionContractError("approval_evidence_binding_mismatch")
        approval = mission_contract.OwnerApproval(
            owner_identity=approval_evidence.owner_identity,
            proposal_sha256=approval_evidence.proposal_sha256,
            approval_id=approval_evidence.approval_id,
            approved_at=_now(),
        )
        authenticator = mission_contract.AuthenticatedOwnerApprovalValidator(
            approval_evidence
        )
        mission_contract.validate_authenticated_owner_approval(
            approval, model.proposal_sha256, authenticator
        )
        consumed = self.journal.consume_owner_approval(
            repository=model.repository_identity.repository,
            mission_id=model.mission_id,
            approval_id=approval_evidence.evidence_id,
            proposal_sha256=model.proposal_sha256,
            accepted_main_sha=model.repository_identity.base_sha,
        )
        if not consumed:
            raise mission_contract.MissionContractError("approval_already_consumed_or_replayed")
        activated = mission_contract.activate_current_mission(
            repository=model.repository_identity.repository,
            base_sha=model.repository_identity.base_sha,
            branch=model.repository_identity.branch,
            source_ref=model.repository_identity.source_ref,
            source_sha256=model.repository_identity.source_sha256,
            proposal_sha256=model.proposal_sha256,
            owner_approval=approval,
            owner_authenticator=authenticator,
            mission=model,
        )
        self.journal.record_mission_activation(
            mission_id=activated.mission_id,
            proposal_sha256=activated.proposal_sha256,
            mission_data=activated.to_wire(),
        )
        self.mission = activated
        self.mission_id = activated.mission_id
        return activated

    def plan_stages(
        self, mission: mission_contract.MaintenanceMission | None = None
    ) -> shadow_steward.PlanProjection:
        """Plan stages and workcards for the active mission using authenticated approval."""

        active = mission or self.mission
        if active is None:
            rec = self.journal.active_mission_record()
            if rec and rec.data:
                try:
                    active = self._restore_journal_mission(rec)
                    self.mission = active
                    self.mission_id = active.mission_id
                except Exception:
                    active = None
        if active is None:
            raise StewardServiceError("no_active_mission")

        raw_request = active.objective
        proposal = shadow_steward.compile_proposal(raw_request)
        # The only owner approval is consumed at Mission activation.  Stage
        # planning and routine replanning derive their bounded authority from
        # that durable Mission; creating synthetic per-stage approvals would
        # let local code impersonate the owner and would also prompt twice.
        plan = shadow_steward.plan_stage(proposal, active)
        if plan.stage:
            self.journal.append(
                event="STAGE_PLANNED",
                idempotency_key=f"stage-planned:{active.mission_id}:{plan.stage.stage_id}",
                mission_id=active.mission_id,
                stage_id=plan.stage.stage_id,
                card_id="",
                state="RUNNING",
                detail="stage_planned",
                data=plan.to_wire(),
                enforce_transition=False,
            )
            for card in plan.workcards:
                self.journal.append(
                    event="CARD_QUEUED",
                    idempotency_key=f"card-queued:{active.mission_id}:{plan.stage.stage_id}:{card.card_id}",
                    mission_id=active.mission_id,
                    stage_id=plan.stage.stage_id,
                    card_id=card.card_id,
                    state="QUEUED",
                    detail="card_queued",
                    enforce_transition=True,
                )
        return plan

    def replan_stage(
        self,
        plan: shadow_steward.PlanProjection,
        failure_code: str,
        *,
        attempt_number: int = 1,
    ) -> shadow_steward.PlanProjection:
        """Replan a stage on failure without weakening safety boundaries."""

        active = self.mission
        if active is None:
            rec = self.journal.active_mission_record()
            if rec and rec.data:
                active = self._restore_journal_mission(rec)
                self.mission = active
        replan_proj = shadow_steward.replan(
            plan,
            failure_code,
            mission=active,
            attempt_number=attempt_number,
        )
        mid = active.mission_id if active else "mission"
        stage_id = plan.stage.stage_id if plan.stage else "unknown-stage"
        self.journal.append(
            event="STAGE_REPLANNED",
            idempotency_key=f"stage-replanned:{mid}:{stage_id}:{attempt_number}:{failure_code}",
            mission_id=mid,
            stage_id=stage_id,
            card_id="",
            state=replan_proj.disposition if replan_proj.disposition in ("PAUSED_FOR_OWNER", "BLOCKED") else "RUNNING",
            detail=f"replanned:{failure_code}",
            data=replan_proj.to_wire(),
            enforce_transition=False,
        )
        return replan_proj

    def _fixture_step(
        self,
        worker: WorkerAdapter | None = None,
        reviewer: ReviewerAdapter | None = None,
    ) -> dict[str, Any]:
        """Deterministic fixture-only cycle retained for unit/fault tests."""

        self.heartbeat()
        self.recover()
        self.reconcile(stage_bindings={})

        active = self.mission
        if active is None:
            rec = self.journal.active_mission_record()
            if rec and rec.data:
                try:
                    active = self._restore_journal_mission(rec)
                    self.mission = active
                    self.mission_id = active.mission_id
                except Exception:
                    active = None

        if active is None or active.state != "RUNNING":
            return {"status": "IDLE", "mission_id": active.mission_id if active else None}

        plan = self.plan_stages(active)
        if plan.disposition == "PAUSED_FOR_OWNER" or not plan.stage:
            return {
                "status": "PAUSED_FOR_OWNER",
                "mission_id": active.mission_id,
                "stop": plan.stop.to_wire() if plan.stop else None,
            }

        stage = plan.stage
        cards = plan.workcards

        proj = self.journal.projection(mission_id=active.mission_id, stage_id=stage.stage_id)
        card_states = proj.get("card_states", {})

        active_worker = worker or general_worker()
        active_reviewer = reviewer or general_reviewer()

        for card in cards:
            state = card_states.get(card.card_id)
            if state in ("COMPLETE", "WAITING_FOR_MERGE"):
                continue

            if state is None:
                self.journal.append(
                    event="CARD_QUEUED",
                    idempotency_key=f"card-queued:{active.mission_id}:{stage.stage_id}:{card.card_id}",
                    mission_id=active.mission_id,
                    stage_id=stage.stage_id,
                    card_id=card.card_id,
                    state="QUEUED",
                    detail="card_queued",
                    enforce_transition=True,
                )

            self.journal.append(
                event="WORKER_STARTED",
                idempotency_key=f"worker-start:{stage.stage_id}:{card.card_id}:1",
                mission_id=active.mission_id,
                stage_id=stage.stage_id,
                card_id=card.card_id,
                state="RUNNING",
                detail="worker_started",
                enforce_transition=True,
            )

            ctx = WorkerContext(
                mission_id=active.mission_id,
                stage_id=stage.stage_id,
                card_id=card.card_id,
                attempt=1,
                model_tier=card.model_tier if hasattr(card, "model_tier") else "T1",
                base_sha=stage.repository_identity.base_sha,
                worktree=self.repo_path,
                allowed_paths=card.allowed_paths,
                steps=card.steps if hasattr(card, "steps") else ("Apply bounded approved change.",),
                focused_tests=card.quality_checks if hasattr(card, "quality_checks") else (),
                negative_checks=card.forbidden_changes if hasattr(card, "forbidden_changes") else (),
                expected_evidence=card.expected_evidence if hasattr(card, "expected_evidence") else (),
                environment=steward_workers.child_environment(dict(os.environ), preserve_home=True),
                worktree_branch="main",
                forbidden_paths=card.forbidden_paths if hasattr(card, "forbidden_paths") else (),
                max_attempts=card.max_attempts if hasattr(card, "max_attempts") else 1,
                objective=active.objective,
            )

            outcome = active_worker.run(ctx)
            if outcome.status != "PASS":
                self.journal.append(
                    event="WORKER_FAILED",
                    idempotency_key=f"worker-fail:{stage.stage_id}:{card.card_id}:1",
                    mission_id=active.mission_id,
                    stage_id=stage.stage_id,
                    card_id=card.card_id,
                    state="RETRYING",
                    detail=outcome.detail,
                    enforce_transition=True,
                )
                self.replan_stage(plan, "WORKER_FAILED", attempt_number=1)
                return {"status": "CARD_FAILED", "card_id": card.card_id, "detail": outcome.detail}

            self.journal.append(
                event="WORKER_COMMITTED",
                idempotency_key=f"worker-commit:{stage.stage_id}:{card.card_id}:1",
                mission_id=active.mission_id,
                stage_id=stage.stage_id,
                card_id=card.card_id,
                state="VERIFYING",
                detail="worker_committed",
                data=outcome.to_wire(),
                enforce_transition=True,
            )

            self.journal.append(
                event="REVIEW_STARTED",
                idempotency_key=f"review-start:{stage.stage_id}:{card.card_id}:1",
                mission_id=active.mission_id,
                stage_id=stage.stage_id,
                card_id=card.card_id,
                state="REVIEWING",
                detail="review_started",
                enforce_transition=True,
            )

            rev_outcome = active_reviewer.review(ctx, outcome)
            if rev_outcome.status != "PASS":
                self.journal.append(
                    event="REVIEW_REJECTED",
                    idempotency_key=f"review-reject:{stage.stage_id}:{card.card_id}:1",
                    mission_id=active.mission_id,
                    stage_id=stage.stage_id,
                    card_id=card.card_id,
                    state="REVIEWING",
                    detail=rev_outcome.detail,
                    enforce_transition=True,
                )
                self.replan_stage(plan, "REVIEW_CHANGES_REQUESTED", attempt_number=1)
                return {"status": "REVIEW_REJECTED", "card_id": card.card_id, "detail": rev_outcome.detail}

            self.journal.append(
                event="REVIEW_PASSED",
                idempotency_key=f"review-pass:{stage.stage_id}:{card.card_id}:1",
                mission_id=active.mission_id,
                stage_id=stage.stage_id,
                card_id=card.card_id,
                state="WAITING_FOR_MERGE",
                detail="review_passed",
                data=rev_outcome.to_wire(),
                enforce_transition=True,
            )

        self.journal.append(
            event="STAGE_INTEGRATED",
            idempotency_key=f"stage-integrated:{active.mission_id}:{stage.stage_id}",
            mission_id=active.mission_id,
            stage_id=stage.stage_id,
            card_id="",
            state="INTEGRATING",
            detail="stage_integrated",
            data={"stage_id": stage.stage_id, "status": "INTEGRATED"},
            enforce_transition=False,
        )

        return {
            "status": "STAGE_INTEGRATED",
            "mission_id": active.mission_id,
            "stage_id": stage.stage_id,
        }

    def _active_mission(self) -> mission_contract.MaintenanceMission | None:
        """Restore the one active Mission only from durable journal proof.

        A caller can construct a Python ``MaintenanceMission`` with
        ``state=RUNNING``.  That object is never sufficient to drive the
        production loop: the service requires the matching activation/rebind
        fact written after external approval consumption.
        """

        record = self.journal.active_mission_record()
        if record is None or not record.data:
            return None
        try:
            active = self._restore_journal_mission(record)
        except Exception:
            return None
        self.mission = active
        self.mission_id = active.mission_id
        return active

    @staticmethod
    def _stage_groups(mission: mission_contract.MaintenanceMission) -> tuple[tuple[str, ...], ...]:
        """Deterministically split an approved Mission into bounded stages."""

        proposal = shadow_steward.compile_proposal(mission.objective)
        paths = tuple(proposal.requested_paths) or mission.allowed_paths
        # Two cards in a stage are independently eligible for K=2 when their
        # paths differ.  Later groups depend on the accepted-main receipt of
        # the preceding group, rather than on an ad-hoc external queue.
        return tuple(paths[offset : offset + 2] for offset in range(0, len(paths), 2))

    def _next_stage_plan(
        self,
        mission: mission_contract.MaintenanceMission,
        index: int,
        *,
        retry: int = 1,
        strategy: str = "primary",
    ) -> tuple[mission_contract.Stage, tuple[mission_contract.WorkCard, ...], int] | None:
        groups = self._stage_groups(mission)
        if index >= len(groups):
            return None
        paths = groups[index]
        if not paths:
            return None
        stage_number = index + 1
        if type(retry) is not int or not 1 <= retry <= mission.budget.max_attempts:
            raise StewardServiceError("stage_retry_budget_invalid")
        if strategy not in {"primary", "alternative"}:
            raise StewardServiceError("stage_replan_strategy_invalid")
        token = hashlib.sha256(
            f"{mission.mission_id}:{mission.proposal_sha256}:{stage_number}:{strategy}:{retry}:{mission.repository_identity.base_sha}".encode("utf-8")
        ).hexdigest()[:16]
        stage_id = f"steward-stage-{stage_number}-{token}"
        cards = tuple(
            mission_contract.WorkCard(
                card_id=f"{stage_id}:card-{card_index}",
                stage_id=stage_id,
                allowed_paths=(path,),
                forbidden_paths=("outside-approved-scope/",),
                steps=(
                    "Implement the approved bounded WorkCard objective."
                    if strategy == "primary"
                    else "Use a bounded alternative implementation after the prior candidate-repair budget was exhausted.",
                ),
                focused_tests=mission.quality_checks,
                negative_checks=("Reject path, authority, credential, and external-effect expansion.",),
                expected_evidence=("implementation head", "focused-check receipt", "independent review receipt"),
                dependencies=(),
                path_locks=(path,),
                max_attempts=min(mission.budget.max_attempts, 3),
                model_tier="T1",
                rollback=mission.rollback,
                result_state="PENDING",
            )
            for card_index, path in enumerate(paths, start=1)
        )
        stage = mission_contract.Stage(
            stage_id=stage_id,
            mission_id=mission.mission_id,
            objective=f"Autonomously planned bounded stage {stage_number}, {strategy} candidate {retry}.",
            repository_identity=mission.repository_identity,
            acceptance_checks=mission.quality_checks,
            compatibility_checks=("accepted-main dependency is journal-bound",),
            workcard_ids=tuple(card.card_id for card in cards),
            rollback=mission.rollback,
            integration_pr=None,
            exact_head=None,
        )
        mission_contract.validate_stage(stage, mission, cards)
        return stage, cards, len(groups)

    def _stage_records(
        self, mission_id: str,
    ) -> list[tuple[mission_contract.Stage, tuple[mission_contract.WorkCard, ...], dict[str, Any]]]:
        records: list[tuple[mission_contract.Stage, tuple[mission_contract.WorkCard, ...], dict[str, Any]]] = []
        for event in self.journal.replay():
            if event.mission_id != mission_id or event.event != "STAGE_PLANNED":
                continue
            try:
                stage = mission_contract.Stage.from_wire(event.data["stage"])
                cards = tuple(mission_contract.WorkCard.from_wire(item) for item in event.data["workcards"])
                index = event.data["stage_index"]
                total = event.data["stage_total"]
                dependency = event.data.get("depends_on_stage")
                retry = event.data.get("retry", 1)
                strategy = event.data.get("strategy", "primary")
                if type(index) is not int or type(total) is not int or index < 1 or total < index:
                    raise ValueError("stage_plan_index_invalid")
                if dependency is not None and (not isinstance(dependency, str) or mission_contract.IDENTIFIER.fullmatch(dependency) is None):
                    raise ValueError("stage_plan_dependency_invalid")
                if type(retry) is not int or retry < 1:
                    raise ValueError("stage_plan_retry_invalid")
                if strategy not in {"primary", "alternative"}:
                    raise ValueError("stage_plan_strategy_invalid")
            except (KeyError, TypeError, ValueError, mission_contract.MissionContractError):
                raise StewardServiceError("stage_plan_journal_invalid")
            records.append((stage, cards, {"stage_index": index, "stage_total": total, "depends_on_stage": dependency, "retry": retry, "strategy": strategy}))
        return records

    def _latest_stage_event(self, mission_id: str, stage_id: str, event_name: str) -> Any | None:
        for event in reversed(self.journal.replay()):
            if (
                event.mission_id == mission_id
                and event.stage_id == stage_id
                and event.event == event_name
            ):
                return event
        return None

    def _replan_stage(
        self,
        mission: mission_contract.MaintenanceMission,
        stage: mission_contract.Stage,
        metadata: Mapping[str, Any],
    ) -> dict[str, Any]:
        """Supersede one failed candidate and plan its bounded replacement.

        Candidate replacement is Mission-derived: it consumes no second owner
        approval and creates a fresh branch/head so stale CI or review receipts
        cannot be reused.  The failed branch is intentionally retained as a
        recovery artifact.
        """

        retry = int(metadata.get("retry", 1)) + 1
        strategy = metadata.get("strategy", "primary")
        if strategy not in {"primary", "alternative"}:
            raise StewardServiceError("stage_replan_strategy_invalid")
        # Three exact-head repairs are enough to distinguish a routine repair
        # from a persistent candidate shape.  The bounded alternative remains
        # inside the same approved Mission and receives the remaining Mission
        # attempt budget; it is not an owner prompt or a parallel lifecycle.
        primary_candidate_budget = min(mission.budget.max_attempts, 3)
        strategy_shift = strategy == "primary" and retry > primary_candidate_budget
        if strategy_shift:
            strategy = "alternative"
            retry = 1
        elif retry > mission.budget.max_attempts:
            self.journal.append(
                event="STAGE_REPLAN_EXHAUSTED",
                idempotency_key=f"stage-replan-exhausted:{mission.mission_id}:{stage.stage_id}",
                mission_id=mission.mission_id,
                stage_id=stage.stage_id,
                card_id="",
                state="RUNNING",
                detail="alternative_candidate_budget_exhausted",
                data={},
                enforce_transition=False,
            )
            return {"status": "REPLAN_EXHAUSTED", "stage_id": stage.stage_id}

        if strategy_shift:
            self.journal.append(
                event="STAGE_REPLAN_STRATEGY_SHIFT",
                idempotency_key=f"stage-replan-strategy-shift:{mission.mission_id}:{stage.stage_id}",
                mission_id=mission.mission_id,
                stage_id=stage.stage_id,
                card_id="",
                state="RUNNING",
                detail="candidate_repair_budget_exhausted_alternative_planned",
                data={"from_strategy": "primary", "to_strategy": "alternative"},
                enforce_transition=False,
            )

        binding = self._latest_stage_event(mission.mission_id, stage.stage_id, "STAGE_PR_BOUND")
        if binding is not None:
            data = binding.data
            pr_number = data.get("pr_number")
            expected_head = data.get("head_sha")
            if type(pr_number) is not int or not isinstance(expected_head, str):
                raise StewardServiceError("stage_binding_invalid")
            intent = self._latest_stage_event(mission.mission_id, stage.stage_id, "STAGE_SUPERSEDE_DISPATCH_INTENT")
            if intent is not None and self._latest_stage_event(mission.mission_id, stage.stage_id, "STAGE_SUPERSEDED") is None:
                return {"status": "OUTCOME_UNKNOWN", "stage_id": stage.stage_id}
            if intent is None:
                self.journal.append(
                    event="STAGE_SUPERSEDE_DISPATCH_INTENT",
                    idempotency_key=f"stage-supersede-intent:{mission.mission_id}:{stage.stage_id}:{pr_number}:{expected_head}",
                    mission_id=mission.mission_id,
                    stage_id=stage.stage_id,
                    card_id="",
                    state="RUNNING",
                    detail="failed_candidate_supersede_intent",
                    data={"pr_number": pr_number, "head_sha": expected_head},
                    enforce_transition=False,
                )
                try:
                    self.github_writer.supersede_stage_pr(
                        mission.repository_identity.repository, pr_number, expected_head
                    )
                except GitHubMutationError:
                    self.journal.append(
                        event="STAGE_OUTCOME_UNKNOWN",
                        idempotency_key=f"stage-supersede-outcome-unknown:{mission.mission_id}:{stage.stage_id}:{pr_number}:{expected_head}",
                        mission_id=mission.mission_id,
                        stage_id=stage.stage_id,
                        card_id="",
                        state="OUTCOME_UNKNOWN",
                        detail="failed_candidate_supersede_outcome_unknown",
                        data={},
                        enforce_transition=False,
                    )
                    return {"status": "OUTCOME_UNKNOWN", "stage_id": stage.stage_id}
                self.journal.append(
                    event="STAGE_SUPERSEDED",
                    idempotency_key=f"stage-superseded:{mission.mission_id}:{stage.stage_id}:{pr_number}:{expected_head}",
                    mission_id=mission.mission_id,
                    stage_id=stage.stage_id,
                    card_id="",
                    state="RUNNING",
                    detail="failed_candidate_closed_branch_retained",
                    data={"pr_number": pr_number, "head_sha": expected_head},
                    enforce_transition=False,
                )
        else:
            # A worker failure can happen before a Draft PR exists.  Mark the
            # failed journal candidate superseded as well, otherwise every
            # restart would keep selecting the same old pending record and
            # never advance to its deterministic replacement.
            self.journal.append(
                event="STAGE_SUPERSEDED",
                idempotency_key=f"stage-superseded-unbound:{mission.mission_id}:{stage.stage_id}",
                mission_id=mission.mission_id,
                stage_id=stage.stage_id,
                card_id="",
                state="RUNNING",
                detail="failed_unbound_candidate_replaced",
                data={},
                enforce_transition=False,
            )

        replacement = self._next_stage_plan(
            mission, int(metadata["stage_index"]) - 1, retry=retry, strategy=strategy
        )
        if replacement is None:
            raise StewardServiceError("stage_replan_group_missing")
        replacement_stage, replacement_cards, total = replacement
        self._record_stage_plan(
            mission,
            replacement_stage,
            replacement_cards,
            stage_index=int(metadata["stage_index"]),
            stage_total=total,
            depends_on_stage=metadata.get("depends_on_stage"),
            retry=retry,
            strategy=strategy,
        )
        return {"status": "STAGE_REPLANNED", "stage_id": replacement_stage.stage_id, "retry": retry, "strategy": strategy}

    def _record_stage_plan(
        self,
        mission: mission_contract.MaintenanceMission,
        stage: mission_contract.Stage,
        cards: tuple[mission_contract.WorkCard, ...],
        *,
        stage_index: int,
        stage_total: int,
        depends_on_stage: str | None = None,
        retry: int = 1,
        strategy: str = "primary",
    ) -> None:
        self.journal.append(
            event="STAGE_PLANNED",
            idempotency_key=f"stage-planned:{mission.mission_id}:{stage.stage_id}",
            mission_id=mission.mission_id,
            stage_id=stage.stage_id,
            card_id="",
            state="RUNNING",
            detail="stage_planned",
            data={
                "stage": stage.to_wire(),
                "workcards": [card.to_wire() for card in cards],
                "stage_index": stage_index,
                "stage_total": stage_total,
                "depends_on_stage": depends_on_stage,
                "retry": retry,
                "strategy": strategy,
            },
            enforce_transition=False,
        )

    def _execute_production_stage(
        self,
        mission: mission_contract.MaintenanceMission,
        stage: mission_contract.Stage,
        cards: tuple[mission_contract.WorkCard, ...],
        *,
        worker: WorkerAdapter | None,
        reviewer: ReviewerAdapter | None,
    ) -> dict[str, Any]:
        """Reuse the accepted isolated K=2 executor as a service child seam."""

        # Import lazily: the historical compatibility facade imports this
        # service, whereas the service is the only lifecycle owner.
        from steward import Steward, StewardError

        executor = Steward(
            repository=mission.repository_identity.repository,
            repo_path=self.repo_path,
            journal=self.journal,
            github=self.github,
            worker=worker or production_worker(),
            reviewer=reviewer or production_reviewer(),
            lock_dir=self.journal.path.parent / "locks",
        )
        try:
            result = executor.execute_stage_to_waiting_for_merge(
                mission,
                stage,
                cards,
                base_sha=stage.repository_identity.base_sha,
                title=f"steward: {stage.stage_id}",
                body="Autonomously integrated bounded Steward stage.",
            )
        except StewardError as exc:
            message = str(exc)
            unknown = "outcome_unknown" in message or "push" in message and "unknown" in message
            self.journal.append(
                event="STAGE_OUTCOME_UNKNOWN" if unknown else "STAGE_REPLAN_REQUESTED",
                idempotency_key=f"stage-exec-failure:{mission.mission_id}:{stage.stage_id}:{hashlib.sha256(message.encode()).hexdigest()[:12]}",
                mission_id=mission.mission_id,
                stage_id=stage.stage_id,
                card_id="",
                state="OUTCOME_UNKNOWN" if unknown else "RUNNING",
                detail="stage_execution_outcome_unknown" if unknown else "stage_execution_replan_requested",
                data={},
                enforce_transition=False,
            )
            return {"status": "OUTCOME_UNKNOWN" if unknown else "REPLAN_REQUIRED", "stage_id": stage.stage_id}
        if result.get("status") != "stage_pr_draft":
            return {"status": "WAITING", "stage_id": stage.stage_id}
        integration = result.get("integration")
        pr = result.get("pr")
        if integration is None or not isinstance(pr, Mapping) or type(pr.get("number")) is not int:
            raise StewardServiceError("stage_execution_binding_invalid")
        self.journal.append(
            event="STAGE_INTEGRATED",
            idempotency_key=f"stage-integrated:{mission.mission_id}:{stage.stage_id}:{integration.head_sha}",
            mission_id=mission.mission_id,
            stage_id=stage.stage_id,
            card_id="",
            state="INTEGRATING",
            detail="stage_integrated",
            data=integration.to_wire(),
            enforce_transition=False,
        )
        self.journal.append(
            event="STAGE_PR_BOUND",
            idempotency_key=f"stage-pr-bound:{mission.mission_id}:{stage.stage_id}:{pr['number']}:{integration.head_sha}",
            mission_id=mission.mission_id,
            stage_id=stage.stage_id,
            card_id="",
            state="RUNNING",
            detail="stage_draft_pr_bound",
            data={
                "repository": mission.repository_identity.repository,
                "pr_number": pr["number"],
                "base_sha": integration.base_sha,
                "head_sha": integration.head_sha,
                "base_branch": mission.repository_identity.branch,
                "head_branch": integration.branch,
            },
            enforce_transition=False,
        )
        # The card reviewers attest their isolated commits before assembly.
        # A Stage PR needs a second, complete-range independent review after
        # integration so the exact receipt can be consumed by canonical CI and
        # the guarded merge workflow without an operator routing step.
        stage_reviewer = reviewer or production_reviewer()
        try:
            review = self._review_integrated_stage(
                mission, stage, cards, integration, stage_reviewer
            )
        except (WorkerError, StewardServiceError) as exc:
            self.journal.append(
                event="STAGE_REPLAN_REQUESTED",
                idempotency_key=f"stage-integrated-review-failed:{mission.mission_id}:{stage.stage_id}:{integration.head_sha}",
                mission_id=mission.mission_id,
                stage_id=stage.stage_id,
                card_id="",
                state="RUNNING",
                detail="integrated_stage_review_failed",
                data={"error_class": type(exc).__name__},
                enforce_transition=False,
            )
            return {"status": "REPLAN_REQUIRED", "stage_id": stage.stage_id}
        review_intent = self._latest_stage_event(
            mission.mission_id, stage.stage_id, "STAGE_REVIEW_DISPATCH_INTENT"
        )
        review_published = self._latest_stage_event(
            mission.mission_id, stage.stage_id, "STAGE_REVIEW_RECEIPT_PUBLISHED"
        )
        if review_published is None:
            if review_intent is not None:
                return {"status": "OUTCOME_UNKNOWN", "stage_id": stage.stage_id}
            self.journal.append(
                event="STAGE_REVIEW_DISPATCH_INTENT",
                idempotency_key=f"stage-review-intent:{mission.mission_id}:{stage.stage_id}:{integration.head_sha}",
                mission_id=mission.mission_id,
                stage_id=stage.stage_id,
                card_id="",
                state="RUNNING",
                detail="exact_head_review_receipt_publish_intent",
                data={"pr_number": pr["number"], "head_sha": integration.head_sha},
                enforce_transition=False,
            )
            try:
                receipt = self.github_writer.publish_exact_head_review(
                    mission.repository_identity.repository,
                    int(pr["number"]),
                    integration.head_sha,
                    base_sha=integration.base_sha,
                    reviewer_session_id=review.reviewer_session_id,
                    implementation_session_id=review.implementation_session_id,
                    reviewed_range_sha256=review.reviewed_range_sha256,
                    review_receipt_sha256=review.review_receipt_sha256,
                )
            except GitHubPreflightError as exc:
                # Exact PR/head/base drift is proven by read-only preflight;
                # no comment request was sent, so this is a routine replan,
                # never an ambiguous external mutation.
                self.journal.append(
                    event="STAGE_REPLAN_REQUESTED",
                    idempotency_key=f"stage-review-preflight-replan:{mission.mission_id}:{stage.stage_id}:{integration.head_sha}",
                    mission_id=mission.mission_id,
                    stage_id=stage.stage_id,
                    card_id="",
                    state="RUNNING",
                    detail="stage_review_receipt_preflight_replan",
                    data={"error": str(exc)},
                    enforce_transition=False,
                )
                return {"status": "REPLAN_REQUIRED", "stage_id": stage.stage_id}
            except GitHubMutationError:
                self.journal.append(
                    event="STAGE_OUTCOME_UNKNOWN",
                    idempotency_key=f"stage-review-outcome-unknown:{mission.mission_id}:{stage.stage_id}:{integration.head_sha}",
                    mission_id=mission.mission_id,
                    stage_id=stage.stage_id,
                    card_id="",
                    state="OUTCOME_UNKNOWN",
                    detail="stage_review_receipt_outcome_unknown",
                    enforce_transition=False,
                )
                return {"status": "OUTCOME_UNKNOWN", "stage_id": stage.stage_id}
            self.journal.append(
                event="STAGE_REVIEW_RECEIPT_PUBLISHED",
                idempotency_key=f"stage-review-published:{mission.mission_id}:{stage.stage_id}:{integration.head_sha}",
                mission_id=mission.mission_id,
                stage_id=stage.stage_id,
                card_id="",
                state="RUNNING",
                detail="exact_head_review_receipt_published",
                data={
                    "pr_number": pr["number"],
                    "head_sha": integration.head_sha,
                    "base_sha": integration.base_sha,
                    "reviewer_session_id": review.reviewer_session_id,
                    "implementation_session_id": review.implementation_session_id,
                    "reviewed_range_sha256": review.reviewed_range_sha256,
                    "review_receipt_sha256": review.review_receipt_sha256,
                    "receipt": dict(receipt),
                },
                enforce_transition=False,
            )
        return {"status": "STAGE_PR_DRAFT", "stage_id": stage.stage_id, "pr_number": pr["number"]}

    def _review_integrated_stage(
        self,
        mission: mission_contract.MaintenanceMission,
        stage: mission_contract.Stage,
        cards: tuple[mission_contract.WorkCard, ...],
        integration: Any,
        reviewer: ReviewerAdapter,
    ) -> ReviewOutcome:
        """Run one independent reviewer over the complete integrated range."""
        implementation_session_id = None
        for event in reversed(self.journal.replay()):
            if event.mission_id != mission.mission_id or event.stage_id != stage.stage_id:
                continue
            if event.event != "LOCAL_REVIEW_OBSERVED" or not isinstance(event.data, Mapping):
                continue
            candidate = event.data.get("implementation_session_id")
            if isinstance(candidate, str):
                implementation_session_id = candidate
                break
        if implementation_session_id is None:
            implementation_session_id = f"stage-integration:{integration.head_sha[:24]}"
        allowed = tuple(sorted({path for card in cards for path in card.allowed_paths}))
        forbidden = tuple(sorted({path for card in cards for path in card.forbidden_paths}))
        try:
            changed_result = subprocess.run(
                [
                    "git", "diff", "--name-only", "--diff-filter=ACDMRTUXB",
                    f"{integration.base_sha}..{integration.head_sha}",
                ],
                cwd=self.repo_path,
                capture_output=True,
                text=True,
                timeout=60,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise StewardServiceError("integrated_stage_diff_unavailable") from exc
        if changed_result.returncode != 0:
            raise StewardServiceError("integrated_stage_diff_unavailable")
        changed_paths = tuple(sorted(path for path in changed_result.stdout.splitlines() if path))
        if not changed_paths or any(
            not mission_contract.path_in_scope(allowed, path) for path in changed_paths
        ):
            raise StewardServiceError("integrated_stage_changed_paths_invalid")
        context = WorkerContext(
            mission_id=mission.mission_id,
            stage_id=stage.stage_id,
            card_id="stage-review",
            attempt=1,
            model_tier="T1",
            base_sha=integration.base_sha,
            worktree=self.repo_path,
            allowed_paths=allowed,
            steps=(stage.objective,),
            focused_tests=mission.quality_checks,
            negative_checks=("Reject scope, credential, and external-effect expansion.",),
            expected_evidence=("complete integrated range", "exact-head review receipt"),
            environment=steward_workers.child_environment(dict(os.environ), preserve_home=True),
            worktree_branch=integration.branch,
            forbidden_paths=forbidden,
            max_attempts=1,
            objective=mission.objective,
        )
        outcome = WorkerOutcome(
            "PASS",
            implementation_session_id,
            integration.head_sha,
            changed_paths,
            "integrated_stage_candidate",
        )
        result = reviewer.review(context, outcome)
        if not isinstance(result, ReviewOutcome):
            raise StewardServiceError("integrated_stage_review_invalid")
        if (
            result.status != "PASS"
            or result.reviewed_base_sha != integration.base_sha
            or result.reviewed_head_sha != integration.head_sha
        ):
            raise StewardServiceError("integrated_stage_review_not_passed")
        return result

    def _advance_bound_stage(
        self,
        mission: mission_contract.MaintenanceMission,
        stage: mission_contract.Stage,
        metadata: Mapping[str, Any],
    ) -> dict[str, Any]:
        binding = self._latest_stage_event(mission.mission_id, stage.stage_id, "STAGE_PR_BOUND")
        if binding is None:
            raise StewardServiceError("stage_binding_missing")
        data = binding.data
        try:
            pr_number = data["pr_number"]
            expected_head = data["head_sha"]
            expected_base = data["base_sha"]
            if type(pr_number) is not int or expected_base != stage.repository_identity.base_sha:
                raise ValueError("stage_binding_invalid")
        except (KeyError, TypeError, ValueError):
            raise StewardServiceError("stage_binding_invalid")
        try:
            facts = self.github.fetch_stage_pr(mission.repository_identity.repository, pr_number)
            if facts.get("base_sha") != expected_base:
                read_main = getattr(self.github, "fetch_accepted_main", None)
                if not callable(read_main):
                    return {"status": "WAITING_GITHUB_READBACK", "stage_id": stage.stage_id}
                current_main = read_main(mission.repository_identity.repository)
                if not isinstance(current_main, str) or mission_contract.SHA40.fullmatch(current_main) is None:
                    return {"status": "WAITING_GITHUB_READBACK", "stage_id": stage.stage_id}
                if current_main == expected_base:
                    return {"status": "OUTCOME_UNKNOWN", "stage_id": stage.stage_id}
                rebound = replace(
                    mission,
                    repository_identity=replace(mission.repository_identity, base_sha=current_main),
                )
                self.journal.append(
                    event="MISSION_BASE_DRIFT_REBOUND",
                    idempotency_key=f"mission-base-drift-rebound:{mission.mission_id}:{current_main}",
                    mission_id=mission.mission_id,
                    stage_id="mission",
                    card_id="",
                    state="RUNNING",
                    detail="authoritative_accepted_main_drift_rebound",
                    data=rebound.to_wire(),
                    enforce_transition=False,
                )
                self.mission = rebound
                self.journal.append(
                    event="STAGE_REPLAN_REQUESTED",
                    idempotency_key=f"stage-main-drift-replan:{mission.mission_id}:{stage.stage_id}:{current_main}",
                    mission_id=mission.mission_id,
                    stage_id=stage.stage_id,
                    card_id="",
                    state="RUNNING",
                    detail="accepted_main_drift_requires_fresh_candidate",
                    data={"new_base_sha": current_main},
                    enforce_transition=False,
                )
                return {"status": "REPLAN_REQUIRED", "stage_id": stage.stage_id}
            status = reconcile_stage_pr(
                facts,
                repository=mission.repository_identity.repository,
                pr_number=pr_number,
                expected_base_sha=expected_base,
                expected_head_sha=expected_head,
                expected_base_branch=mission.repository_identity.branch,
                expected_head_branch=data.get("head_branch"),
            )
        except (GitHubReadError, GitHubFactsError, OSError):
            return {"status": "WAITING_GITHUB_READBACK", "stage_id": stage.stage_id}
        review_intent = self._latest_stage_event(
            mission.mission_id, stage.stage_id, "STAGE_REVIEW_DISPATCH_INTENT"
        )
        review_published = self._latest_stage_event(
            mission.mission_id, stage.stage_id, "STAGE_REVIEW_RECEIPT_PUBLISHED"
        )
        if review_intent is not None and review_published is None:
            # A process may have died after the receipt POST.  A live PASS
            # receipt proves the effect; otherwise remain OUTCOME_UNKNOWN and
            # never issue a second comment mutation.
            if facts.get("review_state") != "PASS":
                return {"status": "OUTCOME_UNKNOWN", "stage_id": stage.stage_id}
            self.journal.append(
                event="STAGE_REVIEW_RECEIPT_PUBLISHED",
                idempotency_key=f"stage-review-reconciled:{mission.mission_id}:{stage.stage_id}:{expected_head}",
                mission_id=mission.mission_id,
                stage_id=stage.stage_id,
                card_id="",
                state="RUNNING",
                detail="exact_head_review_receipt_reconciled_after_restart",
                data={"pr_number": pr_number, "head_sha": expected_head},
                enforce_transition=False,
            )
        if facts.get("draft") is True:
            ready_intent = self._latest_stage_event(
                mission.mission_id, stage.stage_id, "STAGE_READY_DISPATCH_INTENT"
            )
            if ready_intent is not None:
                # A prior process may have died during the mutation.  Keep
                # reconciling the remote PR; never issue a second Ready call.
                return {"status": "OUTCOME_UNKNOWN", "stage_id": stage.stage_id}
            self.journal.append(
                event="STAGE_READY_DISPATCH_INTENT",
                idempotency_key=f"stage-ready-intent:{mission.mission_id}:{stage.stage_id}:{pr_number}:{expected_head}",
                mission_id=mission.mission_id,
                stage_id=stage.stage_id,
                card_id="",
                state="RUNNING",
                detail="stage_ready_dispatch_intent",
                data={"pr_number": pr_number, "head_sha": expected_head},
                enforce_transition=False,
            )
            try:
                self.github_writer.mark_ready(
                    mission.repository_identity.repository, pr_number, expected_head
                )
            except GitHubMutationError:
                self.journal.append(
                    event="STAGE_OUTCOME_UNKNOWN",
                    idempotency_key=f"stage-ready-unknown:{mission.mission_id}:{stage.stage_id}:{pr_number}:{expected_head}",
                    mission_id=mission.mission_id,
                    stage_id=stage.stage_id,
                    card_id="",
                    state="OUTCOME_UNKNOWN",
                    detail="stage_ready_outcome_unknown",
                    enforce_transition=False,
                )
                return {"status": "OUTCOME_UNKNOWN", "stage_id": stage.stage_id}
            self.journal.append(
                event="STAGE_PR_READY",
                idempotency_key=f"stage-ready:{mission.mission_id}:{stage.stage_id}:{pr_number}:{expected_head}",
                mission_id=mission.mission_id,
                stage_id=stage.stage_id,
                card_id="",
                state="RUNNING",
                detail="stage_pr_ready",
                data={"pr_number": pr_number, "head_sha": expected_head},
                enforce_transition=False,
            )
            return {"status": "STAGE_PR_READY", "stage_id": stage.stage_id, "pr_number": pr_number}
        if status.outcome == "WAITING":
            if facts.get("ci_state") == "FAIL" or facts.get("review_state") == "FAIL":
                self.journal.append(
                    event="STAGE_REPLAN_REQUESTED",
                    idempotency_key=f"stage-gate-replan:{mission.mission_id}:{stage.stage_id}:{expected_head}",
                    mission_id=mission.mission_id,
                    stage_id=stage.stage_id,
                    card_id="",
                    state="RUNNING",
                    detail="stage_ci_or_review_repair_requested",
                    enforce_transition=False,
                )
                return {"status": "REPLAN_REQUIRED", "stage_id": stage.stage_id}
            return {"status": "WAITING_CI_REVIEW", "stage_id": stage.stage_id, "pr_number": pr_number}
        if status.outcome == "WAITING_FOR_MERGE":
            merge_intent = self._latest_stage_event(
                mission.mission_id, stage.stage_id, "STAGE_MERGE_DISPATCH_INTENT"
            )
            if merge_intent is not None:
                # The only safe action after an interrupted dispatch is
                # read-only reconciliation.  A possibly-issued merge must
                # never be retried merely because this service restarted.
                return {"status": "OUTCOME_UNKNOWN", "stage_id": stage.stage_id}
            self.journal.append(
                event="STAGE_MERGE_DISPATCH_INTENT",
                idempotency_key=f"stage-merge-intent:{mission.mission_id}:{stage.stage_id}:{pr_number}:{expected_head}",
                mission_id=mission.mission_id,
                stage_id=stage.stage_id,
                card_id="",
                state="RUNNING",
                detail="canonical_merge_workflow_dispatch_intent",
                data={"pr_number": pr_number, "head_sha": expected_head, "workflow": "agent-merge.yml"},
                enforce_transition=False,
            )
            try:
                receipt = self.github_writer.guarded_merge(
                    mission.repository_identity.repository, pr_number, expected_head
                )
            except GitHubMutationError:
                self.journal.append(
                    event="STAGE_OUTCOME_UNKNOWN",
                    idempotency_key=f"stage-merge-unknown:{mission.mission_id}:{stage.stage_id}:{pr_number}:{expected_head}",
                    mission_id=mission.mission_id,
                    stage_id=stage.stage_id,
                    card_id="",
                    state="OUTCOME_UNKNOWN",
                    detail="stage_merge_outcome_unknown",
                    enforce_transition=False,
                )
                return {"status": "OUTCOME_UNKNOWN", "stage_id": stage.stage_id}
            self.journal.append(
                event="STAGE_MERGE_DISPATCHED",
                idempotency_key=f"stage-merge-dispatched:{mission.mission_id}:{stage.stage_id}:{pr_number}:{expected_head}",
                mission_id=mission.mission_id,
                stage_id=stage.stage_id,
                card_id="",
                state="RUNNING",
                detail="stage_merge_dispatched",
                data=dict(receipt),
                enforce_transition=False,
            )
            return {"status": "MERGE_READBACK", "stage_id": stage.stage_id}
        if status.outcome != "COMPLETE":
            return {"status": "WAITING", "stage_id": stage.stage_id}
        readback = self.post_merge_readback(
            stage_id=stage.stage_id,
            pr_number=pr_number,
            expected_head_sha=expected_head,
            is_final_stage=metadata["stage_index"] == metadata["stage_total"],
        )
        if readback["status"] != "VERIFIED":
            return {"status": readback["status"], "stage_id": stage.stage_id}
        return {"status": "COMPLETE" if readback["mission_state"] == "COMPLETE" else "NEXT_STAGE", "stage_id": stage.stage_id}

    @contextmanager
    def _service_lease(self):
        """Hold the one real writer lease for a production run or one step."""

        if self._service_lease_held:
            yield
            return
        lock = ChatLock(
            self.journal.path.parent / "locks",
            # The journal, not a Mission identifier, defines the single
            # lifecycle writer.  An idle process must not be able to acquire a
            # different lock while another process is advancing a Mission.
            "steward-service",
        )
        try:
            lock.acquire()
        except LockBusy as exc:
            self.journal.append(
                event="SERVICE_LEASE_BUSY",
                idempotency_key=f"service-lease-busy:{self.mission_id or 'idle'}:{os.getpid()}",
                mission_id=self.mission_id or "service",
                stage_id="service",
                card_id="",
                state="BLOCKED",
                detail="single_writer_lease_held_elsewhere",
                data={},
                enforce_transition=False,
            )
            raise StewardServiceError("service_lease_unavailable") from exc
        self._service_lease_held = True
        self._lease_epoch += 1
        lease_epoch = self._lease_epoch
        self.journal.append(
            event="SERVICE_LEASE_ACQUIRED",
            idempotency_key=f"service-lease-acquired:{self.mission_id or 'idle'}:{self._lease_owner_id}:{lease_epoch}",
            mission_id=self.mission_id or "service",
            stage_id="service",
            card_id="",
            state="HEALTHY",
            detail="single_writer_lease_acquired",
            data={},
            enforce_transition=False,
        )
        try:
            yield
        finally:
            self._service_lease_held = False
            lock.release()
            self.journal.append(
                event="SERVICE_LEASE_RELEASED",
                idempotency_key=f"service-lease-released:{self.mission_id or 'idle'}:{self._lease_owner_id}:{lease_epoch}",
                mission_id=self.mission_id or "service",
                stage_id="service",
                card_id="",
                state="HEALTHY",
                detail="single_writer_lease_released",
                data={},
                enforce_transition=False,
            )

    def step(
        self,
        worker: WorkerAdapter | None = None,
        reviewer: ReviewerAdapter | None = None,
    ) -> dict[str, Any]:
        """Advance one production transition under the sole writer lease.

        Explicit adapters are restricted to the deterministic fixture path;
        they cannot accidentally become a production default.
        """

        if worker is not None or reviewer is not None:
            return self._fixture_step(worker=worker, reviewer=reviewer)
        with self._service_lease():
            return self._step_once()

    def _step_once(self) -> dict[str, Any]:
        """Advance the production state machine while its lease is held."""

        self.heartbeat()
        active = self._active_mission()
        if active is None or active.state != "RUNNING":
            return {"status": "IDLE", "mission_id": active.mission_id if active else None}
        try:
            emergency_stop = self.control_state.emergency_stop_active(
                repository=active.repository_identity.repository,
                issue_number=self.control_issue_number,
            )
        except StewardServiceError:
            self.journal.append(
                event="EMERGENCY_STOP_READ_UNAVAILABLE",
                idempotency_key=f"emergency-stop-read-unavailable:{active.mission_id}:{self.control_issue_number}",
                mission_id=active.mission_id,
                stage_id="mission",
                card_id="",
                state="BLOCKED",
                detail="control_state_unavailable_dispatch_halted",
                data={},
                enforce_transition=False,
            )
            return {"status": "WAITING_CONTROL_STATE", "mission_id": active.mission_id}
        if emergency_stop:
            self.journal.append(
                event="EMERGENCY_STOP_OBSERVED",
                idempotency_key=f"emergency-stop-observed:{active.mission_id}:{self.control_issue_number}",
                mission_id=active.mission_id,
                stage_id="mission",
                card_id="",
                state="BLOCKED",
                detail="agent_emergency_stop_active_dispatch_and_merge_halted",
                data={"control_issue": self.control_issue_number},
                enforce_transition=False,
            )
            return {"status": "EMERGENCY_STOP", "mission_id": active.mission_id}
        records = self._stage_records(active.mission_id)
        completed = {
            stage.stage_id
            for stage, _cards, _metadata in records
            if self._latest_stage_event(active.mission_id, stage.stage_id, "POST_MERGE_VERIFIED") is not None
        }
        superseded = {
            stage.stage_id
            for stage, _cards, _metadata in records
            if self._latest_stage_event(active.mission_id, stage.stage_id, "STAGE_SUPERSEDED") is not None
        }
        pending = [
            record for record in records
            if record[0].stage_id not in completed and record[0].stage_id not in superseded
        ]
        if not pending:
            complete_indices = {
                metadata["stage_index"]
                for stage, _cards, metadata in records
                if stage.stage_id in completed
            }
            next_index = 0
            while next_index + 1 in complete_indices:
                next_index += 1
            next_plan = self._next_stage_plan(active, next_index)
            if next_plan is None:
                self.journal.record_mission_completion(active.mission_id, {"final_head_sha": active.repository_identity.base_sha})
                self.mission = replace(active, state="COMPLETE")
                return {"status": "COMPLETE", "mission_id": active.mission_id}
            stage, cards, total = next_plan
            self._record_stage_plan(
                active,
                stage,
                cards,
                stage_index=next_index + 1,
                stage_total=total,
                depends_on_stage=next(
                    (
                        prior.stage_id
                        for prior, _prior_cards, prior_meta in reversed(records)
                        if prior_meta["stage_index"] == next_index and prior.stage_id in completed
                    ),
                    None,
                ),
            )
            return {"status": "STAGE_PLANNED", "mission_id": active.mission_id, "stage_id": stage.stage_id}
        stage, cards, metadata = pending[0]
        # An OUTCOME_UNKNOWN is a dispatch/merge halt, not a terminal state:
        # the next iterations may perform read-only remote reconciliation.
        # ``_advance_bound_stage`` sees the persisted intent and never repeats
        # a possibly-issued Ready, supersede, or merge mutation.
        if self._latest_stage_event(active.mission_id, stage.stage_id, "STAGE_REPLAN_REQUESTED") is not None:
            return self._replan_stage(active, stage, metadata)
        if self._latest_stage_event(active.mission_id, stage.stage_id, "STAGE_PR_BOUND") is None:
            return self._execute_production_stage(active, stage, cards, worker=None, reviewer=None)
        return self._advance_bound_stage(active, stage, metadata)

    def run(
        self,
        *,
        once: bool = False,
        interval_seconds: int = 60,
        worker: WorkerAdapter | None = None,
        reviewer: ReviewerAdapter | None = None,
    ) -> None:
        """Run the continuous autonomous service advancement loop."""

        if worker is not None or reviewer is not None:
            # Fixture adapters keep their explicit deterministic path and do
            # not pretend to be the long-running production service.
            self._fixture_step(worker=worker, reviewer=reviewer)
            return
        with self._service_lease():
            while True:
                self._step_once()
                if once:
                    break
                self.wait_for_wakeup(interval_seconds)

    def status(self) -> dict[str, Any]:
        """Query live projection and active mission status."""

        mid = self.mission_id or "service"
        proj = self.journal.projection(mission_id=mid)
        active = self.mission
        if active is None:
            rec = self.journal.active_mission_record()
            if rec and rec.data:
                try:
                    active = self._restore_journal_mission(rec)
                except Exception:
                    pass
        return {
            "schema_version": "steward_status.v1",
            "mission_id": mid,
            "mission_state": active.state if active else "IDLE",
            "mission_objective": active.objective if active else None,
            "projection": proj,
        }

    def stop(self, reason: str = "emergency_stop") -> dict[str, Any]:
        """Emergency stop active mission."""

        mid = (self.mission.mission_id if self.mission else None) or self.mission_id or "service"
        event = self.journal.record_mission_stop(mission_id=mid, reason=reason)
        if self.mission is not None:
            self.mission = replace(self.mission, state="STOPPED")
        return {
            "schema_version": "steward_stopped.v1",
            "mission_id": mid,
            "timestamp": event.timestamp,
            "status": "STOPPED",
            "reason": reason,
        }

    def publish_stage(
        self,
        stage: mission_contract.Stage,
        integration_head_sha: str,
        *,
        title: str,
        body: str,
    ) -> dict[str, Any]:
        """Compatibility helper for deterministic legacy tests only.

        Production reaches Draft creation solely through the journaled
        ``step`` loop; no operator should sequence this helper manually.
        """
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
        """Compatibility helper; production Ready promotion is loop-owned."""
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
    ) -> StagePRStatus:
        """Compatibility read helper; production polling is loop-owned."""
        if self.mission is None:
            raise ValueError("no_active_mission")
        repo = self.mission.repository_identity.repository
        facts = self.github.fetch_stage_pr(repo, pr_number)
        status = reconcile_stage_pr(
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
        """Compatibility helper; production merge dispatch is loop-owned."""
        if self.mission is None:
            raise ValueError("no_active_mission")
        repo = self.mission.repository_identity.repository
        status = self.observe_stage_ci(stage, pr_number, expected_head_sha)
        if status.outcome not in {"WAITING_FOR_MERGE", "COMPLETE"}:
            raise GitHubMutationError(f"pr_not_merge_eligible: {status.outcome} ({status.reason})")

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
        stage_id: str,
        pr_number: int,
        expected_head_sha: str,
        is_final_stage: bool,
    ) -> dict[str, Any]:
        """Prove and smoke-test the exact PR-produced accepted-main revision.

        This is intentionally a remote-authority gate.  A local checkout can
        be stale, detached, or have moved for unrelated work; it is never a
        substitute for the GitHub PR/head/main transition proof.
        """
        if self.mission is None:
            raise ValueError("no_active_mission")
        repo = self.mission.repository_identity.repository
        repo_dir = self.repo_path or Path.cwd()
        try:
            rb = self.github_writer.post_merge_readback(repo, pr_number, expected_head_sha)
        except (GitHubFactsError, GitHubMutationError, OSError, subprocess.TimeoutExpired) as exc:
            self.journal.append(
                event="POST_MERGE_READBACK_UNAVAILABLE",
                idempotency_key=f"post-merge-readback-unavailable:{self.mission.mission_id}:{stage_id}:{pr_number}:{expected_head_sha}",
                mission_id=self.mission.mission_id,
                stage_id=stage_id,
                card_id="",
                state="OUTCOME_UNKNOWN",
                detail="authoritative_post_merge_readback_unavailable",
                data={"error_class": type(exc).__name__},
                enforce_transition=False,
            )
            return {"status": "WAITING_READBACK", "mission_state": self.mission.state}
        accepted_main_sha = rb.get("accepted_main_sha")
        merge_commit_sha = rb.get("merge_commit_sha")
        if (
            rb.get("status") != "VERIFIED"
            or rb.get("repository") != repo
            or rb.get("pr_number") != pr_number
            or rb.get("expected_head_sha") != expected_head_sha
            or not isinstance(accepted_main_sha, str)
            or mission_contract.SHA40.fullmatch(accepted_main_sha) is None
            or accepted_main_sha != merge_commit_sha
        ):
            self.journal.append(
                event="POST_MERGE_READBACK_UNPROVEN",
                idempotency_key=f"post-merge-readback-unproven:{self.mission.mission_id}:{stage_id}:{pr_number}:{expected_head_sha}",
                mission_id=self.mission.mission_id,
                stage_id=stage_id,
                card_id="",
                state="OUTCOME_UNKNOWN",
                detail="authoritative_pr_main_transition_unproven",
                data={},
                enforce_transition=False,
            )
            return {"status": "RECOVERY_REQUIRED", "mission_state": self.mission.state}

        # The local fetch is a consistency check after (not instead of) the
        # remote proof.  The smoke runs against the named accepted object, so
        # an unrelated checked-out branch cannot change its meaning.
        fetch = subprocess.run(
            ["git", "fetch", "origin", "main"],
            cwd=repo_dir,
            capture_output=True,
            text=True,
            check=False,
        )
        if fetch.returncode != 0:
            self.journal.append(
                event="POST_MERGE_LOCAL_MIRROR_UNAVAILABLE",
                idempotency_key=f"post-merge-local-mirror-unavailable:{self.mission.mission_id}:{stage_id}:{accepted_main_sha}",
                mission_id=self.mission.mission_id,
                stage_id=stage_id,
                card_id="",
                state="OUTCOME_UNKNOWN",
                detail="accepted_main_local_mirror_unavailable",
                data={},
                enforce_transition=False,
            )
            return {"status": "WAITING_READBACK", "mission_state": self.mission.state}
        mirror = subprocess.run(
            ["git", "rev-parse", "origin/main"],
            cwd=repo_dir,
            capture_output=True,
            text=True,
            check=False,
        )
        if mirror.returncode != 0 or mirror.stdout.strip() != accepted_main_sha:
            self.journal.append(
                event="POST_MERGE_LOCAL_MIRROR_MISMATCH",
                idempotency_key=f"post-merge-local-mirror-mismatch:{self.mission.mission_id}:{stage_id}:{accepted_main_sha}",
                mission_id=self.mission.mission_id,
                stage_id=stage_id,
                card_id="",
                state="OUTCOME_UNKNOWN",
                detail="accepted_main_local_mirror_mismatch",
                data={},
                enforce_transition=False,
            )
            return {"status": "WAITING_READBACK", "mission_state": self.mission.state}
        smoke = subprocess.run(
            ["git", "diff-tree", "--check", "--no-commit-id", "-r", accepted_main_sha],
            cwd=repo_dir,
            capture_output=True,
            text=True,
            check=False,
        )
        if smoke.returncode != 0:
            self.journal.append(
                event="POST_MERGE_SMOKE_FAILED",
                idempotency_key=f"post-merge-smoke-failed:{self.mission.mission_id}:{stage_id}:{accepted_main_sha}",
                mission_id=self.mission.mission_id,
                stage_id=stage_id,
                card_id="",
                state="OUTCOME_UNKNOWN",
                detail="accepted_main_diff_check_failed",
                data={},
                enforce_transition=False,
            )
            return {"status": "POST_MERGE_SMOKE_FAILED", "mission_state": self.mission.state}

        self.journal.append(
            event="POST_MERGE_VERIFIED",
            idempotency_key=f"post-merge-verified:{self.mission.mission_id}:{stage_id}:{accepted_main_sha}",
            mission_id=self.mission.mission_id,
            stage_id=stage_id,
            card_id="",
            state="COMPLETE" if is_final_stage else "RUNNING",
            detail="post_merge_readback_verified",
            data={
                "pr_number": pr_number,
                "expected_head_sha": expected_head_sha,
                "merge_commit_sha": merge_commit_sha,
                "accepted_main_sha": accepted_main_sha,
                "smoke": [{"command": "git diff-tree --check --no-commit-id -r <accepted-main>", "exit_code": 0}],
            },
            enforce_transition=False,
        )
        if is_final_stage:
            self.journal.record_mission_completion(
                self.mission.mission_id,
                summary={"final_head_sha": accepted_main_sha, "stage_pr_number": pr_number},
            )
            self.mission = replace(self.mission, state="COMPLETE")
        else:
            rebound = replace(
                self.mission,
                repository_identity=replace(
                    self.mission.repository_identity, base_sha=accepted_main_sha
                ),
            )
            self.journal.record_mission_base_advance(
                rebound.mission_id,
                rebound.to_wire(),
                accepted_main_sha=accepted_main_sha,
            )
            self.mission = rebound
        return {
            "status": "VERIFIED",
            "head_sha": accepted_main_sha,
            "pr_number": pr_number,
            "merge_commit_sha": merge_commit_sha,
            "diff_clean": True,
            "mission_state": self.mission.state if self.mission else "UNKNOWN",
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
                        "merged", self.mission_id, stage_id, card_id, status.head_sha
                    ),
                    mission_id=self.mission_id,
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
                        "waiting", self.mission_id, stage_id, card_id, status.head_sha
                    ),
                    mission_id=self.mission_id,
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
                        "blocked", self.mission_id, stage_id, card_id, status.reason
                    ),
                    mission_id=self.mission_id,
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
                        "gates-revoked", self.mission_id, stage_id, card_id, status.reason
                    ),
                    mission_id=self.mission_id,
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
    p_prop.add_argument(
        "--base-sha",
        default=None,
        help="Optional already-authoritative main SHA; omitted reads GitHub main before proposing",
    )
    p_prop.add_argument("--branch", default="main")
    p_prop.add_argument("--mission-id", default=None)

    p_appr = subparsers.add_parser("approve", help="Approve and activate a proposed mission")
    p_appr.add_argument("--mission-id", default=None)
    p_appr.add_argument("--proposal-sha256", required=True)
    p_appr.add_argument(
        "--approval-comment-id",
        type=int,
        required=True,
        help="Existing authenticated GitHub owner-comment ID; never generated locally",
    )
    p_appr.add_argument(
        "--control-issue",
        type=int,
        default=int(os.environ.get("STEWARD_CONTROL_ISSUE", "208")),
        help="Issue that contains the authenticated owner approval comment",
    )

    p_stat = subparsers.add_parser("status", help="Query live Steward status")
    p_stat.add_argument("--mission-id", default=None)

    p_stop = subparsers.add_parser("stop", help="Emergency stop active mission")
    p_stop.add_argument("--mission-id", default=None)
    p_stop.add_argument("--reason", default="emergency_stop")

    p_run = subparsers.add_parser("run", help="Run steward execution loop")
    p_run.add_argument("--once", action="store_true")
    p_run.add_argument("--interval-seconds", type=int, default=60)
    p_run.add_argument("--mission-id", default=None)
    p_run.add_argument(
        "--control-issue",
        type=int,
        default=int(os.environ.get("STEWARD_CONTROL_ISSUE", "208")),
        help="Canonical Issue label source for agent-emergency-stop",
    )

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
        prop_event = next(
            (
                e
                for e in reversed(events)
                if e.event == "MISSION_PROPOSED"
                and (args.mission_id is None or e.mission_id == args.mission_id)
            ),
            None,
        )
        if prop_event is None:
            sys.stderr.write("No proposed mission found to approve\n")
            return 1
        model = mission_contract.MaintenanceMission.from_wire(prop_event.data)
        if model.proposal_sha256 != args.proposal_sha256:
            sys.stderr.write("Requested proposal digest does not match the proposed Mission\n")
            return 1
        activated = service.approve(
            prop_event.data,
            approval_comment_id=args.approval_comment_id,
            control_issue_number=args.control_issue,
        )
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
        control_issue_number=getattr(args, "control_issue", int(os.environ.get("STEWARD_CONTROL_ISSUE", "208"))),
    )
    once = args.once or (args.command == "run" and getattr(args, "once", False))
    interval = getattr(args, "interval_seconds", 60)
    if not once and not 5 <= interval <= 3600:
        parser.error("--interval-seconds must be between 5 and 3600")
    service.run(once=once, interval_seconds=interval)
    return 0


__all__ = [
    "ControlStateSource",
    "GhControlStateSource",
    "GhOwnerApprovalSource",
    "OwnerApprovalSource",
    "RecoveryItem",
    "ReconciliationReport",
    "StewardService",
    "StewardServiceError",
    "main",
]


if __name__ == "__main__":
    raise SystemExit(main())
