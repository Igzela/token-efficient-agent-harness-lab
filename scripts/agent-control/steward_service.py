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


def _parse_utc_timestamp(value: object) -> datetime | None:
    if not isinstance(value, str) or mission_contract.TIMESTAMP.fullmatch(value) is None:
        return None
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None
    return parsed if parsed.tzinfo is not None else None


def _owner_recovery_journal_data(
    marker: Mapping[str, Any],
    dispatch_identity: Mapping[str, Any],
) -> dict[str, Any]:
    """Return redacted, journal-safe metadata for an owner recovery marker.

    The append-only journal rejects credential-shaped keys, including keys
    containing ``AUTH``.  More importantly, retaining the complete comment
    payload would blur owner authority with external outcome evidence.  Keep
    only the authenticated comment metadata and exact non-secret binding.
    """
    return {
        "owner_marker_id": marker["authorization_id"],
        "owner_comment_id": marker["comment_id"],
        "owner_comment_created_at": marker["comment_created_at"],
        "owner_identity": marker["owner_identity"],
        "owner_action": marker["action"],
        "dispatch_identity": {
            key: dispatch_identity[key]
            for key in (
                "schema_version",
                "repository",
                "pr_number",
                "base_sha",
                "head_sha",
                "workflow_file",
                "ref",
                "intent_key",
                "dispatch_id",
            )
        },
    }


def _standing_recovery_journal_data(
    mission: mission_contract.MaintenanceMission,
    grant: mission_contract.Grant,
    dispatch_identity: Mapping[str, Any],
    *,
    use_number: int,
) -> dict[str, Any]:
    """Return redacted, journal-safe metadata for standing Mission recovery authority.

    The append-only journal rejects credential-shaped keys, including keys
    containing ``AUTH``.  Keep only the authenticated mission approval metadata
    and exact non-secret binding.
    """
    return {
        "grant_kind": "MISSION_STANDING_RECOVERY",
        "grant_id": grant.grant_id,
        "grant_type": grant.grant_type,
        "mission_grant_use": use_number,
        "mission_grant_max_uses": grant.max_uses,
        "recovery_action": "QUARANTINE_EXACT_PR",
        "action": "QUARANTINE_EXACT_PR",
        "mission_id": mission.mission_id,
        "proposal_sha256": mission.proposal_sha256,
        "owner_identity": (
            mission.owner_approval.owner_identity
            if mission.owner_approval is not None
            else "standing_mission"
        ),
        "approval_id": (
            mission.owner_approval.approval_id
            if mission.owner_approval is not None
            else "standing_approval"
        ),
        "dispatch_identity": {
            key: dispatch_identity[key]
            for key in (
                "schema_version",
                "repository",
                "pr_number",
                "base_sha",
                "head_sha",
                "workflow_file",
                "ref",
                "intent_key",
                "dispatch_id",
            )
        },
    }


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

    def _recover_merge_dispatch_while_stopped(
        self, mission: mission_contract.MaintenanceMission
    ) -> dict[str, Any] | None:
        """Run only read-only merge reconciliation while the stop is active.

        Emergency-stop blocks every new work, Ready, supersede, and merge
        effect, but it must not strand an already-persisted external outcome
        forever.  This helper selects only a bound Stage with an unresolved
        merge intent and delegates to the canonical reconciler.  A returned
        replan request is reported to the caller but is never executed on
        this stopped path.
        """

        for stage, cards, metadata in self._stage_records(mission.mission_id):
            if self._latest_stage_event(
                mission.mission_id, stage.stage_id, "STAGE_PR_BOUND"
            ) is None:
                continue
            pending = self._bound_stage_mutation_pending(
                mission.mission_id, stage.stage_id
            )
            merge_read_waiting = self._latest_stage_event(
                mission.mission_id, stage.stage_id, "STAGE_MERGE_READ_WAITING"
            )
            if pending not in {"MERGE", "QUARANTINE"} and merge_read_waiting is None:
                continue
            return self._advance_bound_stage(
                mission, stage, metadata, cards, read_only_recovery=True
            )
        return None

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

    def _merge_dispatch_identity(
        self,
        mission: mission_contract.MaintenanceMission,
        stage: mission_contract.Stage,
        *,
        pr_number: int,
        expected_base_sha: str,
        expected_head_sha: str,
        intent_event: Any | None,
        intent_key: str | None = None,
    ) -> dict[str, Any]:
        """Derive one stable dispatch identity from the journal intent.

        Older intents did not persist a run ID or dispatch digest. Their
        idempotency key is still a durable logical-intent nonce, so it can be
        deterministically upgraded without replaying the external request.
        """
        intent_key = (
            intent_event.idempotency_key
            if intent_event is not None
            else intent_key
            or (
                "stage-merge-intent:"
                + hashlib.sha256(
                    f"{mission.mission_id}:{stage.stage_id}:{pr_number}:{expected_head_sha}".encode()
                ).hexdigest()[:32]
            )
        )
        identity = steward_github.merge_dispatch_identity(
            mission.repository_identity.repository,
            pr_number,
            expected_base_sha,
            expected_head_sha,
            workflow_file="agent-merge.yml",
            intent_key=intent_key,
        )
        if intent_event is not None:
            recorded = intent_event.data.get("dispatch_id")
            if recorded is not None and recorded != identity["dispatch_id"]:
                raise StewardServiceError("merge_dispatch_identity_drift")
        return identity

    def _bound_stage_mutation_pending(
        self, mission_id: str, stage_id: str
    ) -> str | None:
        """Return the first non-terminal bound-stage mutation intent.

        Accepted-main drift is only actionable when no external mutation is
        still unresolved.  In particular, a merge dispatch intent can
        represent a workflow that was issued but whose result was not yet
        observed; rebinding the Mission and superseding that PR in this state
        could duplicate or race the merge effect.  The journal is the sole
        source for this recovery gate, so it remains restart-safe and does not
        require a second lifecycle store.
        """

        review_intent = self._latest_stage_event(
            mission_id, stage_id, "STAGE_REVIEW_DISPATCH_INTENT"
        )
        review_terminal = self._latest_stage_event(
            mission_id, stage_id, "STAGE_REVIEW_RECEIPT_PUBLISHED"
        )
        review_reconciled = self._latest_stage_event(
            mission_id, stage_id, "STAGE_REVIEW_DISPATCH_RECONCILED"
        )
        review_preflight_rejected = self._latest_stage_event(
            mission_id, stage_id, "STAGE_REVIEW_DISPATCH_PREFLIGHT_REJECTED"
        )
        if (
            review_intent is not None
            and review_terminal is None
            and review_reconciled is None
            and review_preflight_rejected is None
        ):
            return "REVIEW"

        ready_intent = self._latest_stage_event(
            mission_id, stage_id, "STAGE_READY_DISPATCH_INTENT"
        )
        ready_terminal = self._latest_stage_event(
            mission_id, stage_id, "STAGE_PR_READY"
        )
        if ready_intent is not None and ready_terminal is None:
            return "READY"

        merge_intent = self._latest_stage_event(
            mission_id, stage_id, "STAGE_MERGE_DISPATCH_INTENT"
        )
        merge_read_waiting = self._latest_stage_event(
            mission_id, stage_id, "STAGE_MERGE_READ_WAITING"
        )
        merge_dispatched = self._latest_stage_event(
            mission_id, stage_id, "STAGE_MERGE_DISPATCHED"
        )
        merge_unknown = self._latest_stage_event(
            mission_id, stage_id, "STAGE_OUTCOME_UNKNOWN"
        )
        if (
            merge_unknown is not None
            and merge_unknown.detail == "stage_review_receipt_outcome_unknown"
        ):
            merge_unknown = None
        merge_terminal = self._latest_stage_event(
            mission_id, stage_id, "POST_MERGE_VERIFIED"
        ) or self._latest_stage_event(
            mission_id, stage_id, "STAGE_MERGE_DISPATCH_RECONCILED"
        )
        quarantine_intent = self._latest_stage_event(
            mission_id, stage_id, "STAGE_ORPHAN_QUARANTINE_INTENT"
        )
        # A read failure before ``guarded_merge`` could issue the workflow is
        # a durable no-effect fact.  It must not strand the older intent as an
        # ambiguous external mutation; a later tick may safely retry the
        # writer preflight.  Once a subsequent dispatch fact exists, the
        # intent is unresolved again until merge/readback reconciliation.
        pre_dispatch_read_waiting = (
            merge_read_waiting is not None
            and (merge_intent is None or merge_read_waiting.seq > merge_intent.seq)
            and (merge_dispatched is None or merge_dispatched.seq < merge_read_waiting.seq)
            and (merge_unknown is None or merge_unknown.seq < merge_read_waiting.seq)
        )
        if pre_dispatch_read_waiting:
            merge_intent = None
        if quarantine_intent is not None and merge_terminal is None:
            # A restart after the quarantine request must reconcile the
            # exact PR/main facts, never issue the close mutation twice.
            return "QUARANTINE"
        if merge_intent is not None and merge_terminal is None:
            # STAGE_MERGE_DISPATCHED records that dispatch returned, not that
            # the workflow's merge effect was proven.  A terminal rejected
            # dispatch is also a no-effect terminal fact; successful merges
            # still require post-merge readback.
            return "MERGE"
        stage_unknown = self._latest_stage_event(
            mission_id, stage_id, "STAGE_OUTCOME_UNKNOWN"
        )
        review_unknown_resolved = (
            stage_unknown is not None
            and stage_unknown.detail == "stage_review_receipt_outcome_unknown"
            and (
                review_terminal is not None
                or review_reconciled is not None
                or review_preflight_rejected is not None
            )
        )
        if stage_unknown is not None and not review_unknown_resolved:
            # A stage-level unknown may be the marker written by the merge
            # dispatch exception itself.  Once the merge intent has a terminal
            # readback/rejection fact, that older marker is resolved for this
            # stage and must not hide the bounded recovery path.
            if merge_intent is None or merge_terminal is None:
                return "OUTCOME_UNKNOWN"
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

        pending_mutation = self._bound_stage_mutation_pending(
            mission.mission_id, stage.stage_id
        )
        if pending_mutation is not None:
            return {
                "status": "OUTCOME_UNKNOWN"
                if pending_mutation == "OUTCOME_UNKNOWN"
                else "WAITING_GITHUB_READBACK",
                "stage_id": stage.stage_id,
                "pending_mutation": pending_mutation,
            }

        replan_requests = [
            event
            for event in self.journal.replay()
            if event.mission_id == mission.mission_id
            and event.stage_id == stage.stage_id
            and event.event == "STAGE_REPLAN_REQUESTED"
        ]
        has_base_drift_replan = any(
            event.detail == "accepted_main_drift_requires_fresh_candidate"
            for event in replan_requests
        )
        has_candidate_repair = any(
            event.detail != "accepted_main_drift_requires_fresh_candidate"
            for event in replan_requests
        )
        base_drift_replan = has_base_drift_replan and not has_candidate_repair
        quarantine_reconciled = self._latest_stage_event(
            mission.mission_id,
            stage.stage_id,
            "STAGE_MERGE_DISPATCH_RECONCILED",
        )
        owner_authorized_quarantine = any(
            event.detail in {
                "legacy_orphan_closed_unmerged_replacement_authorized",
                "standing_recovery_closed_unmerged_replacement_authorized",
                "standing_recovery_closed_unmerged_replacement_permitted",
                "orphan_closed_unmerged_replacement_authorized",
            }
            for event in replan_requests
        ) or (
            quarantine_reconciled is not None
            and quarantine_reconciled.detail in {
                "legacy_orphan_quarantine_closed_unmerged_observed",
                "standing_recovery_quarantine_closed_unmerged_observed",
                "standing_recovery_bootstrap_github_readback",
                "orphan_quarantine_closed_unmerged_observed",
            }
        )
        # Accepted-main drift invalidates the base, not the candidate's work.
        # Replanning on the fresh authoritative base therefore keeps the same
        # bounded attempt slot and strategy instead of exhausting a routine
        # recovery budget or forcing an unrelated strategy shift.
        retry = int(metadata.get("retry", 1)) + (0 if base_drift_replan else 1)
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
            supersede_key = hashlib.sha256(
                f"{mission.mission_id}:{stage.stage_id}:{pr_number}:{expected_head}".encode()
            ).hexdigest()[:32]
            if owner_authorized_quarantine:
                # The exact owner authorization or standing recovery grant has
                # already been consumed by quarantine_stage_pr. Retire only this
                # journal candidate and retain its remote branch as an auditable
                # recovery artifact; never issue the ordinary supersede mutation.
                quarantine_detail = "legacy_orphan_closed_unmerged_branch_retained"
                if (
                    quarantine_reconciled is not None
                    and "standing_recovery" in quarantine_reconciled.detail
                ) or any("standing_recovery" in event.detail for event in replan_requests):
                    quarantine_detail = "standing_recovery_closed_unmerged_branch_retained"
                self.journal.append(
                    event="STAGE_SUPERSEDED",
                    idempotency_key=f"stage-superseded-orphan-quarantined:{supersede_key}",
                    mission_id=mission.mission_id,
                    stage_id=stage.stage_id,
                    card_id="",
                    state="RUNNING",
                    detail=quarantine_detail,
                    data={
                        "pr_number": pr_number,
                        "head_sha": expected_head,
                        "remote_branch_retained": True,
                        "external_dispatch_replay_forbidden": True,
                    },
                    enforce_transition=False,
                )
            else:
                intent = self._latest_stage_event(mission.mission_id, stage.stage_id, "STAGE_SUPERSEDE_DISPATCH_INTENT")
                if intent is not None and self._latest_stage_event(mission.mission_id, stage.stage_id, "STAGE_SUPERSEDED") is None:
                    return {"status": "OUTCOME_UNKNOWN", "stage_id": stage.stage_id}
                if intent is None:
                    self.journal.append(
                        event="STAGE_SUPERSEDE_DISPATCH_INTENT",
                        idempotency_key=f"stage-supersede-intent:{supersede_key}",
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
                            idempotency_key=f"stage-supersede-outcome-unknown:{supersede_key}",
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
                        idempotency_key=f"stage-superseded:{supersede_key}",
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
        reason = (
            "candidate_repair_with_base_drift"
            if has_base_drift_replan and has_candidate_repair
            else "accepted_main_drift"
            if base_drift_replan
            else "candidate_repair"
        )
        return {
            "status": "STAGE_REPLANNED",
            "stage_id": replacement_stage.stage_id,
            "retry": retry,
            "strategy": strategy,
            "reason": reason,
        }

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
        except ValueError as exc:
            if str(exc) != "recovery_required_before_dispatch":
                raise
            # A true in-flight ambiguity halts dispatch but must not kill the
            # long-running owner.  Subsequent ticks remain read-only until a
            # reconciliation or Stage supersession makes recovery terminal.
            self.journal.append(
                event="RECOVERY_RECONCILIATION_WAITING",
                idempotency_key=f"recovery-waiting:{mission.mission_id}:{stage.stage_id}",
                mission_id=mission.mission_id,
                stage_id=stage.stage_id,
                card_id="",
                state="OUTCOME_UNKNOWN",
                detail="recovery_required_dispatch_halted",
                data={},
                enforce_transition=False,
            )
            return {"status": "OUTCOME_UNKNOWN", "stage_id": stage.stage_id}
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
                idempotency_key=(
                    "stage-integrated-review-failed:"
                    + hashlib.sha256(
                        f"{mission.mission_id}:{stage.stage_id}:{integration.head_sha}".encode()
                    ).hexdigest()[:32]
                ),
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
                # A read failure before the authenticated POST is retryable;
                # a mutation failure remains fail-closed as OUTCOME_UNKNOWN.
                return {"status": "OUTCOME_UNKNOWN", "stage_id": stage.stage_id}
            self.journal.append(
                event="STAGE_REVIEW_DISPATCH_INTENT",
                idempotency_key=f"stage-review-intent:{mission.mission_id}:{stage.stage_id}:{integration.head_sha}",
                mission_id=mission.mission_id,
                stage_id=stage.stage_id,
                card_id="",
                state="RUNNING",
                detail="exact_head_review_receipt_publish_intent",
                data={
                    "pr_number": pr["number"],
                    "head_sha": integration.head_sha,
                    "base_sha": integration.base_sha,
                    "reviewer_session_id": review.reviewer_session_id,
                    "implementation_session_id": review.implementation_session_id,
                    "reviewed_range_sha256": review.reviewed_range_sha256,
                    "review_receipt_sha256": review.review_receipt_sha256,
                },
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
                # no comment request was sent, so this is a routine replan.
                self.journal.append(
                    event="STAGE_REVIEW_DISPATCH_PREFLIGHT_REJECTED",
                    idempotency_key=(
                        "stage-review-preflight-rejected:"
                        + hashlib.sha256(
                            f"{mission.mission_id}:{stage.stage_id}:{integration.head_sha}".encode()
                        ).hexdigest()[:32]
                    ),
                    mission_id=mission.mission_id,
                    stage_id=stage.stage_id,
                    card_id="",
                    state="RUNNING",
                    detail="stage_review_receipt_rejected_before_post",
                    data={"error": str(exc)},
                    enforce_transition=False,
                )
                self.journal.append(
                    event="STAGE_REPLAN_REQUESTED",
                    idempotency_key=(
                        "stage-review-preflight-replan:"
                        + hashlib.sha256(
                            f"{mission.mission_id}:{stage.stage_id}:{integration.head_sha}".encode()
                        ).hexdigest()[:32]
                    ),
                    mission_id=mission.mission_id,
                    stage_id=stage.stage_id,
                    card_id="",
                    state="RUNNING",
                    detail="stage_review_receipt_preflight_replan",
                    data={"error": str(exc)},
                    enforce_transition=False,
                )
                return {"status": "REPLAN_REQUIRED", "stage_id": stage.stage_id}
            except GitHubReadError:
                # The writer's PR/head preflight precedes every POST.  A
                # failed read therefore cannot represent an external effect;
                # keep the durable intent and let the next tick retry.
                self.journal.append(
                    event="STAGE_REVIEW_READ_WAITING",
                    idempotency_key=(
                        "stage-review-read-waiting:"
                        + hashlib.sha256(
                            f"{mission.mission_id}:{stage.stage_id}:{integration.head_sha}".encode()
                        ).hexdigest()[:32]
                    ),
                    mission_id=mission.mission_id,
                    stage_id=stage.stage_id,
                    card_id="",
                    state="RUNNING",
                    detail="stage_review_receipt_read_unavailable",
                    data={},
                    enforce_transition=False,
                )
                return {"status": "WAITING_GITHUB_READBACK", "stage_id": stage.stage_id}
            except GitHubMutationError:
                self.journal.append(
                    event="STAGE_OUTCOME_UNKNOWN",
                    idempotency_key=(
                        "stage-review-outcome-unknown:"
                        + hashlib.sha256(
                            f"{mission.mission_id}:{stage.stage_id}:{integration.head_sha}".encode()
                        ).hexdigest()[:32]
                    ),
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

    def _preflight_bound_stage_accepted_main(
        self,
        mission: mission_contract.MaintenanceMission,
        stage: mission_contract.Stage,
        expected_base: str,
        *,
        rebind_on_drift: bool = True,
    ) -> tuple[mission_contract.MaintenanceMission, dict[str, Any] | None]:
        """Require current accepted main before any bound-Stage mutation.

        Recovery of an unresolved external mutation uses the same authoritative
        read with ``rebind_on_drift=False``.  That mode proves only whether the
        bound base is still current; it deliberately does not append a replan
        or change Mission identity while the mutation outcome is ambiguous.
        """

        read_main = getattr(self.github, "fetch_accepted_main", None)
        try:
            if not callable(read_main):
                raise GitHubReadError("accepted_main_read_unavailable")
            current_main = read_main(mission.repository_identity.repository)
            if (
                not isinstance(current_main, str)
                or mission_contract.SHA40.fullmatch(current_main) is None
            ):
                raise GitHubReadError("accepted_main_read_malformed")
        except (GitHubReadError, GitHubFactsError, OSError):
            self.journal.append(
                event="ACCEPTED_MAIN_READ_UNAVAILABLE",
                idempotency_key=(
                    "accepted-main-read-unavailable:"
                    + hashlib.sha256(
                        f"{mission.mission_id}:{stage.stage_id}:{expected_base}".encode()
                    ).hexdigest()[:32]
                ),
                mission_id=mission.mission_id,
                stage_id=stage.stage_id,
                card_id="",
                state="BLOCKED",
                detail="authoritative_accepted_main_required_before_bound_mutation",
                data={},
                enforce_transition=False,
            )
            return mission, {"status": "WAITING_GITHUB_READBACK", "stage_id": stage.stage_id}

        if current_main == expected_base:
            return mission, None

        if not rebind_on_drift:
            return mission, {
                "status": "OUTCOME_UNKNOWN",
                "stage_id": stage.stage_id,
                "pending_mutation": "unresolved_bound_mutation",
            }

        rebound = replace(
            mission,
            repository_identity=replace(
                mission.repository_identity,
                base_sha=current_main,
            ),
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
            idempotency_key=(
                "stage-main-drift-replan:"
                + hashlib.sha256(
                    f"{mission.mission_id}:{stage.stage_id}:{current_main}".encode()
                ).hexdigest()[:32]
            ),
            mission_id=mission.mission_id,
            stage_id=stage.stage_id,
            card_id="",
            state="RUNNING",
            detail="accepted_main_drift_requires_fresh_candidate",
            data={"new_base_sha": current_main, "old_base_sha": expected_base},
            enforce_transition=False,
        )
        return rebound, {"status": "REPLAN_REQUIRED", "stage_id": stage.stage_id}

    def _advance_bound_stage(
        self,
        mission: mission_contract.MaintenanceMission,
        stage: mission_contract.Stage,
        metadata: Mapping[str, Any],
        cards: tuple[mission_contract.WorkCard, ...] = (),
        *,
        read_only_recovery: bool = False,
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
            pending_mutation = self._bound_stage_mutation_pending(
                mission.mission_id, stage.stage_id
            )
            merge_read_waiting = self._latest_stage_event(
                mission.mission_id, stage.stage_id, "STAGE_MERGE_READ_WAITING"
            )
            if read_only_recovery and merge_read_waiting is not None:
                merge_intent_for_read_waiting = self._latest_stage_event(
                    mission.mission_id,
                    stage.stage_id,
                    "STAGE_MERGE_DISPATCH_INTENT",
                )
                if (
                    merge_intent_for_read_waiting is None
                    or merge_read_waiting.seq > merge_intent_for_read_waiting.seq
                ) and pending_mutation is None:
                    # A pre-dispatch identity read failed.  In particular,
                    # never fall through to WAITING_FOR_MERGE and issue a
                    # workflow while emergency-stop recovery is read-only.
                    return {
                        "status": "WAITING_GITHUB_READBACK",
                        "stage_id": stage.stage_id,
                    }
            merge_intent = self._latest_stage_event(
                mission.mission_id,
                stage.stage_id,
                "STAGE_MERGE_DISPATCH_INTENT",
            )
            workflow_run_id = next(
                (
                    event.data.get("workflow_run_id")
                    for event in (
                        merge_intent,
                        self._latest_stage_event(
                            mission.mission_id,
                            stage.stage_id,
                            "STAGE_MERGE_DISPATCHED",
                        ),
                        self._latest_stage_event(
                            mission.mission_id,
                            stage.stage_id,
                            "STAGE_OUTCOME_UNKNOWN",
                        ),
                    )
                    if event is not None and type(event.data.get("workflow_run_id")) is int
                ),
                None,
            )
            reconciliation: Mapping[str, Any] | None = None
            if pending_mutation in {"MERGE", "QUARANTINE"} and facts.get("merged") is not True:
                reconcile_dispatch = getattr(
                    self.github_writer, "reconcile_merge_dispatch", None
                )
                if callable(reconcile_dispatch):
                    try:
                        identity = self._merge_dispatch_identity(
                            mission,
                            stage,
                            pr_number=pr_number,
                            expected_base_sha=expected_base,
                            expected_head_sha=expected_head,
                            intent_event=merge_intent,
                        )
                        reconciliation = reconcile_dispatch(
                            mission.repository_identity.repository,
                            pr_number,
                            expected_head,
                            workflow_file="agent-merge.yml",
                            not_before=(
                                merge_intent.timestamp
                                if merge_intent is not None
                                else None
                            ),
                            expected_base_sha=expected_base,
                            dispatch_id=identity["dispatch_id"],
                            workflow_run_id=workflow_run_id,
                        )
                    except (GitHubReadError, GitHubFactsError, OSError):
                        reconciliation = None
                    if (
                        isinstance(reconciliation, Mapping)
                        and reconciliation.get("status") == "SUCCEEDED"
                    ):
                        try:
                            current_facts = self.github.fetch_stage_pr(
                                mission.repository_identity.repository, pr_number
                            )
                        except (GitHubReadError, GitHubFactsError, OSError):
                            return {
                                "status": "WAITING_GITHUB_READBACK",
                                "stage_id": stage.stage_id,
                            }
                        if current_facts.get("merged") is not True:
                            self.journal.append(
                                event="STAGE_OUTCOME_UNKNOWN",
                                idempotency_key=(
                                    "stage-merge-success-without-merged-pr:"
                                    + hashlib.sha256(
                                        f"{mission.mission_id}:{stage.stage_id}:{pr_number}:{expected_head}".encode()
                                    ).hexdigest()[:32]
                                ),
                                mission_id=mission.mission_id,
                                stage_id=stage.stage_id,
                                card_id="",
                                state="OUTCOME_UNKNOWN",
                                detail="merge_run_success_without_authoritative_merged_pr",
                                data=dict(reconciliation),
                                enforce_transition=False,
                            )
                            return {"status": "OUTCOME_UNKNOWN", "stage_id": stage.stage_id}
                        facts = current_facts
                    orphan_dispatch_unresolved = (
                        isinstance(reconciliation, Mapping)
                        and merge_intent is not None
                        and workflow_run_id is None
                        and (
                            reconciliation.get("status") == "NOT_PROVEN"
                            or (
                                reconciliation.get("status") == "REJECTED"
                                and not reconciliation.get("run_ids")
                                and not reconciliation.get("workflow_run_id")
                            )
                        )
                    )
                    if orphan_dispatch_unresolved:
                        authorization_reader = getattr(
                            self.github_writer,
                            "read_orphan_dispatch_recovery_authorization",
                            None,
                        )
                        legacy_authorization = None
                        if callable(authorization_reader):
                            try:
                                legacy_authorization = authorization_reader(
                                    mission.repository_identity.repository,
                                    self.control_issue_number,
                                    mission_id=mission.mission_id,
                                    proposal_sha256=mission.proposal_sha256,
                                    stage_id=stage.stage_id,
                                    pr_number=pr_number,
                                    expected_base_sha=expected_base,
                                    expected_head_sha=expected_head,
                                    workflow_file="agent-merge.yml",
                                    dispatch_id=identity["dispatch_id"],
                                    owner_identity=mission.owner_approval.owner_identity,
                                )
                            except (GitHubReadError, GitHubFactsError, OSError):
                                legacy_authorization = None

                        authorization_binding = {
                            "mission_id": mission.mission_id,
                            "proposal_sha256": mission.proposal_sha256,
                            "stage_id": stage.stage_id,
                            "repository": mission.repository_identity.repository,
                            "control_issue_number": self.control_issue_number,
                            "pr_number": pr_number,
                            "base_sha": expected_base,
                            "head_sha": expected_head,
                            "workflow_file": "agent-merge.yml",
                            "ref": "main",
                            "dispatch_id": identity["dispatch_id"],
                            "authorization": "ORPHAN_DISPATCH_RECOVERY",
                            "action": "QUARANTINE_EXACT_PR",
                        }
                        legacy_authorized = False
                        if isinstance(legacy_authorization, Mapping):
                            intent_at = _parse_utc_timestamp(merge_intent.timestamp)
                            comment_at = _parse_utc_timestamp(
                                legacy_authorization.get("comment_created_at")
                            )
                            legacy_authorized = (
                                intent_at is not None
                                and comment_at is not None
                                and comment_at > intent_at
                                and legacy_authorization.get("owner_identity")
                                == mission.owner_approval.owner_identity
                                and type(legacy_authorization.get("comment_id")) is int
                                and legacy_authorization.get("comment_id") > 0
                                and isinstance(legacy_authorization.get("authorization_id"), str)
                                and mission_contract.IDENTIFIER.fullmatch(
                                    legacy_authorization["authorization_id"]
                                ) is not None
                                and all(
                                    legacy_authorization.get(key) == value
                                    for key, value in authorization_binding.items()
                                )
                            )

                        standing_recovery_grant = None
                        try:
                            standing_recovery_grant = mission_contract.validate_standing_recovery_grant(
                                mission,
                                repository=mission.repository_identity.repository,
                            )
                        except mission_contract.MissionContractError:
                            standing_recovery_grant = None

                        recorded_mission = (
                            merge_intent.mission_id
                            or merge_intent.data.get("mission_id")
                        )
                        recorded_stage = (
                            merge_intent.stage_id
                            or merge_intent.data.get("stage_id")
                        )
                        recorded_dispatch = merge_intent.data.get("dispatch_id")
                        recorded_pr = merge_intent.data.get("pr_number")
                        recorded_head = merge_intent.data.get("head_sha")
                        recorded_base = merge_intent.data.get("base_sha")
                        recorded_workflow = merge_intent.data.get("workflow")
                        recorded_ref = merge_intent.data.get("ref")
                        recorded_repo = merge_intent.data.get("repository")

                        bindings_match = (
                            mission.mission_id == self.mission.mission_id
                            and recorded_mission == mission.mission_id
                            and recorded_stage == stage.stage_id
                            and (
                                recorded_repo is None
                                or recorded_repo == mission.repository_identity.repository
                            )
                            and mission.repository_identity.repository
                            == reconciliation.get(
                                "repository", mission.repository_identity.repository
                            )
                            and recorded_dispatch is not None
                            and recorded_dispatch == identity["dispatch_id"]
                            and recorded_pr is not None
                            and recorded_pr == pr_number
                            and recorded_head is not None
                            and recorded_head == expected_head
                            and recorded_base is not None
                            and recorded_base == expected_base
                            and recorded_workflow == "agent-merge.yml"
                            and recorded_ref == "main"
                        )
                        if not bindings_match:
                            return {
                                "status": "OUTCOME_UNKNOWN",
                                "stage_id": stage.stage_id,
                                "reason": "orphan_binding_mismatch",
                            }

                        quarantine_intent = self._latest_stage_event(
                            mission.mission_id,
                            stage.stage_id,
                            "STAGE_ORPHAN_QUARANTINE_INTENT",
                        )

                        standing_recovery_uses = 0
                        if standing_recovery_grant is not None:
                            standing_recovery_uses = sum(
                                1
                                for event in self.journal.replay()
                                if event.mission_id == mission.mission_id
                                and event.event == "STAGE_ORPHAN_QUARANTINE_INTENT"
                                and event.data.get("recovery_source")
                                == "MISSION_STANDING_RECOVERY"
                                and event.data.get("grant_id")
                                == standing_recovery_grant.grant_id
                            )
                            if (
                                not legacy_authorized
                                and quarantine_intent is None
                                and standing_recovery_uses
                                >= standing_recovery_grant.max_uses
                            ):
                                self.journal.append(
                                    event="STAGE_RECOVERY_CEILING_EXHAUSTED",
                                    idempotency_key=(
                                        "stage-standing-recovery-ceiling:"
                                        f"{mission.mission_id}:{standing_recovery_grant.grant_id}"
                                    ),
                                    mission_id=mission.mission_id,
                                    stage_id=stage.stage_id,
                                    card_id="",
                                    state="PAUSED_FOR_OWNER",
                                    detail="standing_recovery_use_ceiling_exhausted",
                                    data={
                                        "grant_id": standing_recovery_grant.grant_id,
                                        "dispatch_id": identity["dispatch_id"],
                                        "consumed_uses": standing_recovery_uses,
                                        "max_uses": standing_recovery_grant.max_uses,
                                    },
                                    enforce_transition=False,
                                )
                                return {
                                    "status": "PAUSED_FOR_OWNER",
                                    "stage_id": stage.stage_id,
                                    "reason": "standing_recovery_use_ceiling_exhausted",
                                }

                        is_authorized = False
                        recovery_data: dict[str, Any] = {}
                        quarantine_key: str = ""
                        intent_detail = ""
                        merged_detail = ""
                        closed_detail = ""
                        replan_detail = ""
                        quarantine_comment: str | None = None

                        if legacy_authorized:
                            is_authorized = True
                            recovery_data = {
                                "recovery_source": "LEGACY_OWNER_MARKER",
                                **_owner_recovery_journal_data(
                                    legacy_authorization, identity
                                ),
                            }
                            quarantine_key = hashlib.sha256(
                                f"{mission.mission_id}:{stage.stage_id}:{identity['dispatch_id']}:{legacy_authorization['comment_id']}".encode()
                            ).hexdigest()[:32]
                            intent_detail = "owner_authorized_exact_orphan_quarantine_intent"
                            merged_detail = "legacy_orphan_merge_observed_from_github"
                            closed_detail = "legacy_orphan_quarantine_closed_unmerged_observed"
                            replan_detail = "legacy_orphan_closed_unmerged_replacement_authorized"
                            quarantine_comment = "Quarantined by the owner-authorized legacy orphan-dispatch recovery; branch retained."
                        elif standing_recovery_grant is not None:
                            is_authorized = True
                            recovery_data = {
                                "recovery_source": "MISSION_STANDING_RECOVERY",
                                **_standing_recovery_journal_data(
                                    mission,
                                    standing_recovery_grant,
                                    identity,
                                    use_number=(
                                        standing_recovery_uses
                                        if quarantine_intent is not None
                                        else standing_recovery_uses + 1
                                    ),
                                ),
                            }
                            approval_marker = (
                                mission.owner_approval.approval_id
                                if mission.owner_approval is not None
                                else "standing_approval"
                            )
                            quarantine_key = hashlib.sha256(
                                f"{mission.mission_id}:{stage.stage_id}:{identity['dispatch_id']}:{approval_marker}".encode()
                            ).hexdigest()[:32]
                            intent_detail = "standing_recovery_exact_orphan_quarantine_intent"
                            merged_detail = "standing_recovery_orphan_merge_observed_from_github"
                            closed_detail = "standing_recovery_quarantine_closed_unmerged_observed"
                            replan_detail = "standing_recovery_closed_unmerged_replacement_authorized"
                            quarantine_comment = "Quarantined under Mission repository-maintenance recovery authority; branch retained."
                        elif quarantine_intent is not None:
                            is_authorized = True
                            quarantine_data = quarantine_intent.data
                            is_legacy = (
                                quarantine_data.get("recovery_source") == "LEGACY_OWNER_MARKER"
                                or "owner_marker_id" in quarantine_data
                            )
                            if is_legacy:
                                merged_detail = "legacy_orphan_merge_observed_from_github"
                                closed_detail = "legacy_orphan_quarantine_closed_unmerged_observed"
                                replan_detail = "legacy_orphan_closed_unmerged_replacement_authorized"
                            else:
                                merged_detail = "standing_recovery_orphan_merge_observed_from_github"
                                closed_detail = "standing_recovery_quarantine_closed_unmerged_observed"
                                replan_detail = "standing_recovery_closed_unmerged_replacement_authorized"
                            recovery_data = {
                                k: v
                                for k, v in quarantine_data.items()
                                if k not in {"preflight_pr", "preflight_accepted_main_sha"}
                            }

                        if is_authorized:
                            quarantine_result: Mapping[str, Any] | None = None
                            if quarantine_intent is None:
                                if read_only_recovery:
                                    return {
                                        "status": "WAITING_CONTROL_STATE",
                                        "stage_id": stage.stage_id,
                                        "reason": "emergency_stop_active_for_quarantine",
                                    }
                                try:
                                    stop_active = self.control_state.emergency_stop_active(
                                        repository=mission.repository_identity.repository,
                                        issue_number=self.control_issue_number,
                                    )
                                except StewardServiceError:
                                    return {
                                        "status": "WAITING_CONTROL_STATE",
                                        "stage_id": stage.stage_id,
                                    }
                                if stop_active:
                                    return {
                                        "status": "WAITING_CONTROL_STATE",
                                        "stage_id": stage.stage_id,
                                        "reason": "emergency_stop_active_for_quarantine",
                                    }
                                try:
                                    current_facts = self.github.fetch_stage_pr(
                                        mission.repository_identity.repository, pr_number
                                    )
                                    read_main = getattr(self.github, "fetch_accepted_main", None)
                                    if not callable(read_main):
                                        raise GitHubReadError("accepted_main_read_unavailable")
                                    current_main = read_main(
                                        mission.repository_identity.repository
                                    )
                                except (GitHubReadError, GitHubFactsError, OSError):
                                    return {
                                        "status": "WAITING_GITHUB_READBACK",
                                        "stage_id": stage.stage_id,
                                    }
                                if (
                                    current_facts.get("repository")
                                    != mission.repository_identity.repository
                                    or current_facts.get("pr_number") != pr_number
                                    or current_facts.get("base_sha") != expected_base
                                    or current_facts.get("head_sha") != expected_head
                                    or not isinstance(current_main, str)
                                    or mission_contract.SHA40.fullmatch(current_main)
                                    is None
                                    or current_facts.get("state") != "OPEN"
                                    or current_facts.get("merged") is not False
                                ):
                                    return {
                                        "status": "OUTCOME_UNKNOWN",
                                        "stage_id": stage.stage_id,
                                        "reason": "legacy_orphan_preflight_not_exact",
                                    }
                                try:
                                    if self.control_state.emergency_stop_active(
                                        repository=mission.repository_identity.repository,
                                        issue_number=self.control_issue_number,
                                    ):
                                        return {
                                            "status": "WAITING_CONTROL_STATE",
                                            "stage_id": stage.stage_id,
                                            "reason": "emergency_stop_active_for_quarantine",
                                        }
                                except StewardServiceError:
                                    return {
                                        "status": "WAITING_CONTROL_STATE",
                                        "stage_id": stage.stage_id,
                                    }
                                self.journal.append(
                                    event="STAGE_ORPHAN_QUARANTINE_INTENT",
                                    idempotency_key=f"stage-orphan-quarantine-intent:{quarantine_key}",
                                    mission_id=mission.mission_id,
                                    stage_id=stage.stage_id,
                                    card_id="",
                                    state="RUNNING",
                                    detail=intent_detail,
                                    data={
                                        **recovery_data,
                                        "preflight_pr": dict(current_facts),
                                        "preflight_accepted_main_sha": current_main,
                                    },
                                    enforce_transition=False,
                                )
                                try:
                                    if self.control_state.emergency_stop_active(
                                        repository=mission.repository_identity.repository,
                                        issue_number=self.control_issue_number,
                                    ):
                                        return {
                                            "status": "WAITING_CONTROL_STATE",
                                            "stage_id": stage.stage_id,
                                            "reason": "emergency_stop_active_for_quarantine",
                                        }
                                except StewardServiceError:
                                    return {
                                        "status": "WAITING_CONTROL_STATE",
                                        "stage_id": stage.stage_id,
                                    }
                                try:
                                    quarantine_result = self.github_writer.quarantine_stage_pr(
                                        mission.repository_identity.repository,
                                        pr_number,
                                        expected_base_sha=expected_base,
                                        expected_head_sha=expected_head,
                                        comment=quarantine_comment,
                                    )
                                except GitHubReadError:
                                    return {
                                        "status": "WAITING_GITHUB_READBACK",
                                        "stage_id": stage.stage_id,
                                    }
                                except GitHubFactsError as exc:
                                    self.journal.append(
                                        event="STAGE_OUTCOME_UNKNOWN",
                                        idempotency_key=(
                                            "stage-orphan-quarantine-preflight-unknown:"
                                            + hashlib.sha256(
                                                f"{mission.mission_id}:{stage.stage_id}:{identity['dispatch_id']}".encode()
                                            ).hexdigest()[:32]
                                        ),
                                        mission_id=mission.mission_id,
                                        stage_id=stage.stage_id,
                                        card_id="",
                                        state="OUTCOME_UNKNOWN",
                                        detail="legacy_orphan_quarantine_preflight_unproven",
                                        data={"error_class": type(exc).__name__},
                                        enforce_transition=False,
                                    )
                                    return {"status": "OUTCOME_UNKNOWN", "stage_id": stage.stage_id}
                                except GitHubMutationError as exc:
                                    self.journal.append(
                                        event="STAGE_OUTCOME_UNKNOWN",
                                        idempotency_key=(
                                            "stage-orphan-quarantine-unknown:"
                                            + hashlib.sha256(
                                                f"{mission.mission_id}:{stage.stage_id}:{identity['dispatch_id']}".encode()
                                            ).hexdigest()[:32]
                                        ),
                                        mission_id=mission.mission_id,
                                        stage_id=stage.stage_id,
                                        card_id="",
                                        state="OUTCOME_UNKNOWN",
                                        detail="legacy_orphan_quarantine_outcome_unknown",
                                        data=dict(getattr(exc, "evidence", {}) or {}),
                                        enforce_transition=False,
                                    )
                                    return {"status": "OUTCOME_UNKNOWN", "stage_id": stage.stage_id}
                            else:
                                # A persisted quarantine intent fences the close
                                # mutation across restart.
                                try:
                                    current_facts = self.github.fetch_stage_pr(
                                        mission.repository_identity.repository, pr_number
                                    )
                                    read_main = getattr(self.github, "fetch_accepted_main", None)
                                    if not callable(read_main):
                                        raise GitHubReadError("accepted_main_read_unavailable")
                                    current_main = read_main(
                                        mission.repository_identity.repository
                                    )
                                except (GitHubReadError, GitHubFactsError, OSError):
                                    return {
                                        "status": "WAITING_GITHUB_READBACK",
                                        "stage_id": stage.stage_id,
                                    }
                                if current_facts.get("merged") is True:
                                    quarantine_result = {
                                        "status": "MERGED",
                                        "repository": mission.repository_identity.repository,
                                        "pr_number": pr_number,
                                        "base_sha": expected_base,
                                        "head_sha": expected_head,
                                        "accepted_main_sha": current_main,
                                    }
                                elif (
                                    current_facts.get("state") == "CLOSED"
                                    and current_facts.get("merged") is False
                                ):
                                    quarantine_result = {
                                        "status": "CLOSED_UNMERGED",
                                        "repository": mission.repository_identity.repository,
                                        "pr_number": pr_number,
                                        "base_sha": expected_base,
                                        "head_sha": expected_head,
                                        "accepted_main_sha": current_main,
                                    }
                                else:
                                    return {"status": "OUTCOME_UNKNOWN", "stage_id": stage.stage_id}

                            if quarantine_result.get("status") == "MERGED":
                                try:
                                    current_facts = self.github.fetch_stage_pr(
                                        mission.repository_identity.repository, pr_number
                                    )
                                except (GitHubReadError, GitHubFactsError, OSError):
                                    return {
                                        "status": "WAITING_GITHUB_READBACK",
                                        "stage_id": stage.stage_id,
                                    }
                                if current_facts.get("merged") is not True:
                                    return {"status": "OUTCOME_UNKNOWN", "stage_id": stage.stage_id}
                                evidence = {
                                    "reconciliation": dict(reconciliation),
                                    **recovery_data,
                                    "quarantine": dict(quarantine_result),
                                    "current_pr": dict(current_facts),
                                }
                                evidence_key = hashlib.sha256(
                                    f"{mission.mission_id}:{stage.stage_id}:{identity['dispatch_id']}".encode()
                                ).hexdigest()[:32]
                                self.journal.append(
                                    event="STAGE_MERGE_DISPATCH_RECONCILED",
                                    idempotency_key=f"stage-legacy-orphan-merged:{evidence_key}",
                                    mission_id=mission.mission_id,
                                    stage_id=stage.stage_id,
                                    card_id="",
                                    state="RUNNING",
                                    detail=merged_detail,
                                    data=evidence,
                                    enforce_transition=False,
                                )
                                facts = current_facts
                            elif quarantine_result.get("status") == "CLOSED_UNMERGED":
                                accepted_main_sha = quarantine_result.get(
                                    "accepted_main_sha"
                                )
                                if (
                                    not isinstance(accepted_main_sha, str)
                                    or mission_contract.SHA40.fullmatch(
                                        accepted_main_sha
                                    )
                                    is None
                                ):
                                    return {
                                        "status": "OUTCOME_UNKNOWN",
                                        "stage_id": stage.stage_id,
                                    }
                                evidence = {
                                    "reconciliation": dict(reconciliation),
                                    **recovery_data,
                                    "quarantine": dict(quarantine_result),
                                }
                                evidence_key = hashlib.sha256(
                                    f"{mission.mission_id}:{stage.stage_id}:{identity['dispatch_id']}".encode()
                                ).hexdigest()[:32]
                                if (
                                    accepted_main_sha
                                    != mission.repository_identity.base_sha
                                ):
                                    rebound = replace(
                                        mission,
                                        repository_identity=replace(
                                            mission.repository_identity,
                                            base_sha=accepted_main_sha,
                                        ),
                                    )
                                    self.journal.append(
                                        event="MISSION_BASE_DRIFT_REBOUND",
                                        idempotency_key=(
                                            "mission-base-drift-rebound:"
                                            f"{mission.mission_id}:{accepted_main_sha}"
                                        ),
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
                                    event="STAGE_MERGE_DISPATCH_RECONCILED",
                                    idempotency_key=f"stage-legacy-orphan-closed:{evidence_key}",
                                    mission_id=mission.mission_id,
                                    stage_id=stage.stage_id,
                                    card_id="",
                                    state="RUNNING",
                                    detail=closed_detail,
                                    data=evidence,
                                    enforce_transition=False,
                                )
                                if read_only_recovery:
                                    return {
                                        "status": "CLOSED_UNMERGED",
                                        "stage_id": stage.stage_id,
                                    }
                                self.journal.append(
                                    event="STAGE_REPLAN_REQUESTED",
                                    idempotency_key=f"stage-legacy-orphan-replan:{evidence_key}",
                                    mission_id=mission.mission_id,
                                    stage_id=stage.stage_id,
                                    card_id="",
                                    state="RUNNING",
                                    detail=replan_detail,
                                    data=evidence,
                                    enforce_transition=False,
                                )
                                return {"status": "REPLAN_REQUIRED", "stage_id": stage.stage_id}
                    if isinstance(reconciliation, Mapping) and reconciliation.get(
                        "status"
                    ) == "REJECTED":
                        evidence = {
                            key: value
                            for key, value in reconciliation.items()
                            if key
                            in {
                                "status",
                                "repository",
                                "pr_number",
                                "expected_head_sha",
                                "run_ids",
                            }
                        }
                        evidence_key = hashlib.sha256(
                            f"{mission.mission_id}:{stage.stage_id}:{expected_head}".encode()
                        ).hexdigest()[:32]
                        self.journal.append(
                            event="STAGE_MERGE_DISPATCH_RECONCILED",
                            idempotency_key=f"stage-merge-reconciled:{evidence_key}",
                            mission_id=mission.mission_id,
                            stage_id=stage.stage_id,
                            card_id="",
                            state="RUNNING",
                            detail="merge_workflow_terminal_rejection_observed",
                            data=evidence,
                            enforce_transition=False,
                        )
                        if read_only_recovery:
                            return {
                                "status": "REJECTED",
                                "stage_id": stage.stage_id,
                            }
                        pending_mutation = None
                        # Re-read accepted main before allowing the ordinary
                        # replan path to supersede this candidate.  If main
                        # moved, this call performs the normal Mission rebound;
                        # it never races the now-proven failed dispatch.
                        mission, bound_preflight = self._preflight_bound_stage_accepted_main(
                            mission, stage, expected_base
                        )
                        if bound_preflight is not None:
                            return bound_preflight
                        self.journal.append(
                            event="STAGE_REPLAN_REQUESTED",
                            idempotency_key=f"stage-merge-rejected-replan:{evidence_key}",
                            mission_id=mission.mission_id,
                            stage_id=stage.stage_id,
                            card_id="",
                            state="RUNNING",
                            detail="stage_merge_workflow_rejected_replan_requested",
                            data=evidence,
                            enforce_transition=False,
                        )
                        return {"status": "REPLAN_REQUIRED", "stage_id": stage.stage_id}
            # A merged PR is already in the readback phase: accepted main is
            # expected to have advanced to its merge commit, so do not treat
            # that proven transition as drift.  For every still-open PR,
            # however, current accepted main must match the Stage base before
            # review, Ready, or merge work can continue.  An unresolved
            # mutation intent is checked first, though: rebinding here could
            # cause a replan/supersede while the external effect is still
            # ambiguous.
            if facts.get("merged") is not True and pending_mutation is not None:
                # Read current accepted main, but never rebind or request a
                # replacement while a prior review/Ready/merge effect is
                # unresolved.  This is the read-only reconciliation boundary.
                _mission, pending_guard = self._preflight_bound_stage_accepted_main(
                    mission,
                    stage,
                    expected_base,
                    rebind_on_drift=False,
                )
                if pending_guard is not None:
                    pending_guard["pending_mutation"] = pending_mutation
                    return pending_guard
            if facts.get("merged") is not True and pending_mutation is None:
                mission, bound_preflight = self._preflight_bound_stage_accepted_main(
                    mission, stage, expected_base
                )
                if bound_preflight is not None:
                    return bound_preflight
            if facts.get("base_sha") != expected_base and pending_mutation is None:
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
                    idempotency_key=(
                        "stage-main-drift-replan:"
                        + hashlib.sha256(
                            f"{mission.mission_id}:{stage.stage_id}:{current_main}".encode()
                        ).hexdigest()[:32]
                    ),
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
            # A mutation failure is permanently fail-closed.  A read-waiting
            # marker, or an older intent without review fields, is safe to
            # retry because the writer reconciles any existing receipt before
            # issuing a POST.
            if self._latest_stage_event(
                mission.mission_id, stage.stage_id, "STAGE_OUTCOME_UNKNOWN"
            ) is not None:
                return {"status": "OUTCOME_UNKNOWN", "stage_id": stage.stage_id}
            intent_data = review_intent.data if isinstance(review_intent.data, Mapping) else {}
            review_fields = (
                "base_sha",
                "reviewer_session_id",
                "implementation_session_id",
                "reviewed_range_sha256",
                "review_receipt_sha256",
            )
            if not all(isinstance(intent_data.get(field), str) for field in review_fields):
                integrated_event = self._latest_stage_event(
                    mission.mission_id, stage.stage_id, "STAGE_INTEGRATED"
                )
                if integrated_event is None or not isinstance(integrated_event.data, Mapping):
                    return {"status": "WAITING_GITHUB_READBACK", "stage_id": stage.stage_id}
                try:
                    from steward import StageIntegration

                    integration_data = integrated_event.data
                    integration = StageIntegration(
                        stage_id=str(integration_data["stage_id"]),
                        branch=str(integration_data["branch"]),
                        base_sha=str(integration_data["base_sha"]),
                        head_sha=str(integration_data["head_sha"]),
                        card_heads=tuple(
                            (str(item["card_id"]), str(item["head_sha"]))
                            for item in integration_data["card_heads"]
                        ),
                    )
                    review = self._review_integrated_stage(
                        mission, stage, cards, integration, production_reviewer()
                    )
                except (KeyError, TypeError, ValueError, WorkerError, StewardServiceError):
                    return {"status": "WAITING_GITHUB_READBACK", "stage_id": stage.stage_id}
                intent_data = {
                    **intent_data,
                    "base_sha": integration.base_sha,
                    "reviewer_session_id": review.reviewer_session_id,
                    "implementation_session_id": review.implementation_session_id,
                    "reviewed_range_sha256": review.reviewed_range_sha256,
                    "review_receipt_sha256": review.review_receipt_sha256,
                }
            if pending_mutation is None:
                mission, bound_preflight = self._preflight_bound_stage_accepted_main(
                    mission, stage, expected_base
                )
                if bound_preflight is not None:
                    return bound_preflight
            try:
                receipt = self.github_writer.publish_exact_head_review(
                    mission.repository_identity.repository,
                    int(intent_data["pr_number"]),
                    expected_head,
                    base_sha=str(intent_data["base_sha"]),
                    reviewer_session_id=str(intent_data["reviewer_session_id"]),
                    implementation_session_id=str(intent_data["implementation_session_id"]),
                    reviewed_range_sha256=str(intent_data["reviewed_range_sha256"]),
                    review_receipt_sha256=str(intent_data["review_receipt_sha256"]),
                )
            except GitHubPreflightError:
                # The writer performed its complete read-only identity
                # preflight and rejected before issuing a POST.  Persist that
                # terminal no-effect observation so the candidate can be
                # safely superseded on the next tick; an unresolved network
                # or mutation error remains OUTCOME_UNKNOWN instead.
                self.journal.append(
                    event="STAGE_REVIEW_DISPATCH_PREFLIGHT_REJECTED",
                    idempotency_key=(
                        "stage-review-preflight-rejected:"
                        + hashlib.sha256(
                            f"{mission.mission_id}:{stage.stage_id}:{expected_head}".encode()
                        ).hexdigest()[:32]
                    ),
                    mission_id=mission.mission_id,
                    stage_id=stage.stage_id,
                    card_id="",
                    state="RUNNING",
                    detail="stage_review_receipt_rejected_before_post",
                    data={},
                    enforce_transition=False,
                )
                self.journal.append(
                    event="STAGE_REPLAN_REQUESTED",
                    idempotency_key=(
                        "stage-review-recovery-preflight-replan:"
                        + hashlib.sha256(
                            f"{mission.mission_id}:{stage.stage_id}:{expected_head}".encode()
                        ).hexdigest()[:32]
                    ),
                    mission_id=mission.mission_id,
                    stage_id=stage.stage_id,
                    card_id="",
                    state="RUNNING",
                    detail="stage_review_receipt_preflight_replan",
                    data={},
                    enforce_transition=False,
                )
                return {"status": "REPLAN_REQUIRED", "stage_id": stage.stage_id}
            except GitHubReadError:
                self.journal.append(
                    event="STAGE_REVIEW_READ_WAITING",
                    idempotency_key=(
                        "stage-review-read-waiting:"
                        + hashlib.sha256(
                            f"{mission.mission_id}:{stage.stage_id}:{expected_head}".encode()
                        ).hexdigest()[:32]
                    ),
                    mission_id=mission.mission_id,
                    stage_id=stage.stage_id,
                    card_id="",
                    state="RUNNING",
                    detail="stage_review_receipt_read_unavailable",
                    data={},
                    enforce_transition=False,
                )
                return {"status": "WAITING_GITHUB_READBACK", "stage_id": stage.stage_id}
            except GitHubMutationError:
                self.journal.append(
                    event="STAGE_OUTCOME_UNKNOWN",
                    idempotency_key=(
                        "stage-review-recovery-outcome-unknown:"
                        + hashlib.sha256(
                            f"{mission.mission_id}:{stage.stage_id}:{expected_head}".encode()
                        ).hexdigest()[:32]
                    ),
                    mission_id=mission.mission_id,
                    stage_id=stage.stage_id,
                    card_id="",
                    state="OUTCOME_UNKNOWN",
                    detail="stage_review_receipt_outcome_unknown",
                    data={},
                    enforce_transition=False,
                )
                return {"status": "OUTCOME_UNKNOWN", "stage_id": stage.stage_id}
            self.journal.append(
                event="STAGE_REVIEW_RECEIPT_PUBLISHED",
                idempotency_key=(
                    "stage-review-reconciled:"
                    + hashlib.sha256(
                        f"{mission.mission_id}:{stage.stage_id}:{expected_head}".encode()
                    ).hexdigest()[:32]
                ),
                mission_id=mission.mission_id,
                stage_id=stage.stage_id,
                card_id="",
                state="RUNNING",
                detail="exact_head_review_receipt_reconciled_after_restart",
                data={**intent_data, "receipt": dict(receipt)},
                enforce_transition=False,
            )
        if facts.get("draft") is True:
            pending_mutation = self._bound_stage_mutation_pending(
                mission.mission_id, stage.stage_id
            )
            if pending_mutation is None:
                mission, bound_preflight = self._preflight_bound_stage_accepted_main(
                    mission, stage, expected_base
                )
                if bound_preflight is not None:
                    return bound_preflight
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
            pending_mutation = self._bound_stage_mutation_pending(
                mission.mission_id, stage.stage_id
            )
            if pending_mutation is None:
                mission, bound_preflight = self._preflight_bound_stage_accepted_main(
                    mission, stage, expected_base
                )
                if bound_preflight is not None:
                    return bound_preflight
            merge_intent = self._latest_stage_event(
                mission.mission_id, stage.stage_id, "STAGE_MERGE_DISPATCH_INTENT"
            )
            merge_reconciled = self._latest_stage_event(
                mission.mission_id,
                stage.stage_id,
                "STAGE_MERGE_DISPATCH_RECONCILED",
            )
            if merge_intent is not None and merge_reconciled is None:
                # The only safe action after an interrupted dispatch is
                # read-only reconciliation.  A possibly-issued merge must
                # never be retried merely because this service restarted.
                merge_read_waiting = self._latest_stage_event(
                    mission.mission_id,
                    stage.stage_id,
                    "STAGE_MERGE_READ_WAITING",
                )
                merge_dispatched = self._latest_stage_event(
                    mission.mission_id,
                    stage.stage_id,
                    "STAGE_MERGE_DISPATCHED",
                )
                merge_unknown = self._latest_stage_event(
                    mission.mission_id,
                    stage.stage_id,
                    "STAGE_OUTCOME_UNKNOWN",
                )
                pre_dispatch_read_waiting = (
                    merge_read_waiting is not None
                    and merge_read_waiting.seq > merge_intent.seq
                    and (merge_dispatched is None or merge_dispatched.seq < merge_read_waiting.seq)
                    and (merge_unknown is None or merge_unknown.seq < merge_read_waiting.seq)
                )
                if not pre_dispatch_read_waiting:
                    return {"status": "OUTCOME_UNKNOWN", "stage_id": stage.stage_id}
            merge_intent_key = (
                "stage-merge-intent:"
                + hashlib.sha256(
                    f"{mission.mission_id}:{stage.stage_id}:{pr_number}:{expected_head}".encode()
                ).hexdigest()[:32]
            )
            merge_identity = self._merge_dispatch_identity(
                mission,
                stage,
                pr_number=pr_number,
                expected_base_sha=expected_base,
                expected_head_sha=expected_head,
                intent_event=None,
                intent_key=merge_intent_key,
            )
            self.journal.append(
                event="STAGE_MERGE_DISPATCH_INTENT",
                idempotency_key=merge_intent_key,
                mission_id=mission.mission_id,
                stage_id=stage.stage_id,
                card_id="",
                state="RUNNING",
                detail="canonical_merge_workflow_dispatch_intent",
                data={
                    "repository": mission.repository_identity.repository,
                    "mission_id": mission.mission_id,
                    "stage_id": stage.stage_id,
                    "pr_number": pr_number,
                    "head_sha": expected_head,
                    "base_sha": expected_base,
                    "workflow": "agent-merge.yml",
                    "ref": "main",
                    "dispatch_id": merge_identity["dispatch_id"],
                },
                enforce_transition=False,
            )
            try:
                receipt = self.github_writer.guarded_merge(
                    mission.repository_identity.repository,
                    pr_number,
                    expected_head,
                    expected_base_sha=expected_base,
                    dispatch_id=merge_identity["dispatch_id"],
                    intent_key=merge_intent_key,
                )
            except GitHubReadError:
                # The writer's initial identity preflight did not reach the
                # dispatch boundary.  Keep the long-running service alive and
                # retry the read on a later tick; this is not an ambiguous
                # external mutation and must not be recorded as one.
                read_wait_key = hashlib.sha256(
                    f"{mission.mission_id}:{stage.stage_id}:{pr_number}:{expected_head}".encode()
                ).hexdigest()[:32]
                self.journal.append(
                    event="STAGE_MERGE_READ_WAITING",
                    idempotency_key=f"stage-merge-read-waiting:{read_wait_key}",
                    mission_id=mission.mission_id,
                    stage_id=stage.stage_id,
                    card_id="",
                    state="RUNNING",
                    detail="stage_merge_identity_read_unavailable_before_dispatch",
                    enforce_transition=False,
                )
                return {"status": "WAITING_GITHUB_READBACK", "stage_id": stage.stage_id}
            except GitHubMutationError as exc:
                unknown_data = dict(getattr(exc, "evidence", {}) or {})
                self.journal.append(
                    event="STAGE_OUTCOME_UNKNOWN",
                    idempotency_key=(
                        "stage-merge-unknown:"
                        + hashlib.sha256(
                            f"{mission.mission_id}:{stage.stage_id}:{pr_number}:{expected_head}".encode()
                        ).hexdigest()[:32]
                    ),
                    mission_id=mission.mission_id,
                    stage_id=stage.stage_id,
                    card_id="",
                    state="OUTCOME_UNKNOWN",
                    detail="stage_merge_outcome_unknown",
                    data=unknown_data,
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

    def _preflight_unbound_accepted_main(
        self,
        mission: mission_contract.MaintenanceMission,
        stage: mission_contract.Stage | None,
    ) -> tuple[mission_contract.MaintenanceMission, dict[str, Any] | None]:
        """Bind unissued work to authoritative main before planning or dispatch.

        A bound PR has its own mutation-intent/readback reconciliation in
        ``_advance_bound_stage``.  Unbound work has no such external identity,
        so it must never reach a worker or a fresh Stage plan until GitHub main
        is readable and matches the durable Mission binding.
        """

        stage_id = stage.stage_id if stage is not None else "next-stage"
        read_main = getattr(self.github, "fetch_accepted_main", None)
        try:
            if not callable(read_main):
                raise GitHubReadError("accepted_main_read_unavailable")
            current_main = read_main(mission.repository_identity.repository)
            if (
                not isinstance(current_main, str)
                or mission_contract.SHA40.fullmatch(current_main) is None
            ):
                raise GitHubReadError("accepted_main_read_malformed")
        except (GitHubReadError, GitHubFactsError, OSError):
            self.journal.append(
                event="ACCEPTED_MAIN_READ_UNAVAILABLE",
                idempotency_key=(
                    "accepted-main-read-unavailable:"
                    + hashlib.sha256(
                        f"{mission.mission_id}:{stage_id}:{mission.repository_identity.base_sha}".encode()
                    ).hexdigest()[:32]
                ),
                mission_id=mission.mission_id,
                stage_id=stage_id,
                card_id="",
                state="BLOCKED",
                detail="authoritative_accepted_main_required_before_unbound_dispatch",
                data={},
                enforce_transition=False,
            )
            return mission, {
                "status": "WAITING_GITHUB_READBACK",
                "mission_id": mission.mission_id,
                "stage_id": stage.stage_id if stage is not None else None,
            }

        rebound = mission
        if current_main != mission.repository_identity.base_sha:
            rebound = replace(
                mission,
                repository_identity=replace(
                    mission.repository_identity,
                    base_sha=current_main,
                ),
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

        if stage is None:
            if rebound is not mission:
                return rebound, {
                    "status": "MISSION_BASE_REBOUND",
                    "mission_id": mission.mission_id,
                    "base_sha": current_main,
                }
            return rebound, None

        if stage.repository_identity.base_sha != current_main:
            existing_replan = self._latest_stage_event(
                mission.mission_id, stage.stage_id, "STAGE_REPLAN_REQUESTED"
            )
            if (
                existing_replan is not None
                and existing_replan.detail == "accepted_main_drift_requires_fresh_candidate"
                and existing_replan.data.get("new_base_sha") == current_main
            ):
                # The prior iteration durably requested this exact replan.
                # Let ``_step_once`` enter the idempotent replan transition
                # instead of returning REPLAN_REQUIRED forever after restart.
                return rebound, None
            self.journal.append(
                event="STAGE_REPLAN_REQUESTED",
                idempotency_key=(
                    f"stage-main-drift-replan:{mission.mission_id}:"
                    f"{stage.stage_id}:{current_main}"
                ),
                mission_id=mission.mission_id,
                stage_id=stage.stage_id,
                card_id="",
                state="RUNNING",
                detail="accepted_main_drift_requires_fresh_candidate",
                data={"new_base_sha": current_main},
                enforce_transition=False,
            )
            return rebound, {"status": "REPLAN_REQUIRED", "stage_id": stage.stage_id}
        return rebound, None

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
            try:
                recovery = self._recover_merge_dispatch_while_stopped(active)
            except (GitHubReadError, GitHubFactsError, OSError):
                recovery = {"status": "WAITING_GITHUB_READBACK"}
            return {
                "status": "EMERGENCY_STOP",
                "mission_id": active.mission_id,
                "read_only_recovery": recovery,
            }
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
            active, preflight = self._preflight_unbound_accepted_main(active, None)
            if preflight is not None:
                return preflight
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
        if self._latest_stage_event(active.mission_id, stage.stage_id, "STAGE_PR_BOUND") is None:
            active, preflight = self._preflight_unbound_accepted_main(active, stage)
            if preflight is not None:
                return preflight
        if self._latest_stage_event(active.mission_id, stage.stage_id, "STAGE_REPLAN_REQUESTED") is not None:
            return self._replan_stage(active, stage, metadata)
        if self._latest_stage_event(active.mission_id, stage.stage_id, "STAGE_PR_BOUND") is None:
            return self._execute_production_stage(active, stage, cards, worker=None, reviewer=None)
        return self._advance_bound_stage(active, stage, metadata, cards)

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
                idempotency_key=(
                    "post-merge-local-mirror-unavailable:"
                    + hashlib.sha256(
                        f"{self.mission.mission_id}:{stage_id}:{accepted_main_sha}".encode()
                    ).hexdigest()[:32]
                ),
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
        superseded_stages = {
            event.stage_id
            for event in events
            if event.mission_id == self.mission_id
            and event.event == "STAGE_SUPERSEDED"
        }
        registered = self.mission
        items: list[RecoveryItem] = []
        for binding in projection["bindings"]:
            state = binding["state"]
            if binding["stage_id"] in superseded_stages:
                items.append(
                    RecoveryItem(
                        binding["card_id"],
                        state,
                        "SUPERSEDED",
                        "stage_superseded",
                    )
                )
                continue
            card_events = [
                event
                for event in events
                if event.mission_id == binding["mission_id"]
                and event.stage_id == binding["stage_id"]
                and event.card_id == binding["card_id"]
            ]
            card_tail = card_events[-1] if card_events else None
            attempt_events = (
                [event for event in card_events if event.attempt == card_tail.attempt]
                if card_tail is not None
                else []
            )
            attempt_by_event = {
                event.event: event
                for event in attempt_events
                if event.event
                in {
                    "WORKER_STARTED",
                    "WORKER_CHECKPOINT",
                    "FOCUSED_CHECKS_PASSED",
                    "LOCAL_REVIEW_OBSERVED",
                }
            }
            worker_event = attempt_by_event.get("WORKER_STARTED")
            worker_data = worker_event.data if worker_event is not None else {}
            expected_binding = worktree_manager.steward_binding_digest(
                binding["mission_id"], binding["stage_id"], binding["card_id"], registered.repository_identity.base_sha
            )
            checkpoint = attempt_by_event.get("WORKER_CHECKPOINT")
            focused = attempt_by_event.get("FOCUSED_CHECKS_PASSED")
            local_review = attempt_by_event.get("LOCAL_REVIEW_OBSERVED")
            checkpoint_data = checkpoint.data if checkpoint is not None else {}
            review_data = local_review.data if local_review is not None else {}
            expected_head = checkpoint_data.get("head_sha")
            checkpoint_identity = (
                worker_event is not None
                and checkpoint is not None
                and worker_event.attempt == checkpoint.attempt
                and worker_event.seq < checkpoint.seq
                and checkpoint_data.get("base_sha") == registered.repository_identity.base_sha
                and checkpoint_data.get("worktree_binding_sha256") == expected_binding
                and isinstance(expected_head, str)
                and steward_workers.SHA40.fullmatch(expected_head) is not None
            )
            checkpoint_only = (
                state == "VERIFYING"
                and card_tail is not None
                and card_tail.event == "WORKER_CHECKPOINT"
                and checkpoint_identity
                and focused is None
                and local_review is None
            )
            focused_checkpoint = (
                state == "REVIEWING"
                and card_tail is not None
                and card_tail.event == "FOCUSED_CHECKS_PASSED"
                and checkpoint_identity
                and focused is not None
                and checkpoint.attempt == focused.attempt
                and checkpoint.seq < focused.seq
            )
            reviewed_checkpoint = (
                state == "REVIEWING"
                and card_tail is not None
                and card_tail.event == "LOCAL_REVIEW_OBSERVED"
                and checkpoint_identity
                and focused is not None
                and card_tail is not None
                and local_review is not None
                and worker_event.attempt == checkpoint.attempt == focused.attempt == local_review.attempt
                and worker_event.seq < checkpoint.seq < focused.seq < local_review.seq
                and focused.attempt == local_review.attempt
                and focused.seq < local_review.seq
                and review_data.get("base_sha") == registered.repository_identity.base_sha
                and review_data.get("head_sha") == expected_head
                and review_data.get("verdict") == "PASS"
                and review_data.get("open_blocker_ids") == []
                and review_data.get("security_ok") is True
                and review_data.get("rollback_ok") is True
            )
            resumable_checkpoint = checkpoint_only or reviewed_checkpoint or (
                focused_checkpoint and local_review is None
            )
            if resumable_checkpoint and self.repo_path is not None:
                try:
                    expected_path, expected_branch = worktree_manager.steward_worktree_location(
                        binding["mission_id"],
                        binding["stage_id"],
                        binding["card_id"],
                        registered.repository_identity.base_sha,
                    )
                    restored = (
                        worker_data.get("base_sha") == registered.repository_identity.base_sha
                        and worker_data.get("worktree_binding_sha256") == expected_binding
                        and worker_data.get("branch") == expected_branch
                        and worktree_manager.restore_steward_checkpoint_worktree(
                            binding["mission_id"],
                            binding["stage_id"],
                            binding["card_id"],
                            str(self.repo_path),
                            registered.repository_identity.base_sha,
                            expected_head,
                        )
                    )
                    if restored and worktree_manager.verify_worktree(
                        expected_path,
                        expected_branch,
                        self.repo_path,
                        expected_head,
                    ):
                        items.append(
                            RecoveryItem(
                                binding["card_id"],
                                state,
                                "RESUMABLE",
                                "reviewed_checkpoint_restored"
                                if reviewed_checkpoint
                                else "focused_checkpoint_restored"
                                if focused_checkpoint
                                else "worker_checkpoint_restored",
                            )
                        )
                        continue
                except (OSError, ValueError, TypeError):
                    pass
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
