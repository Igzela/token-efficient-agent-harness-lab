"""Read-only front edge for a GitHub-controlled, locally executed agent loop.

The repository and GitHub remain authoritative.  This module performs one
bounded poll and returns a versioned decision; it owns no daemon, lease, task
state, durable approval, budget, output, review, merge, or retry authority.
"""

from __future__ import annotations

import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path
import re
import subprocess
from dataclasses import dataclass, field
from typing import Any, Callable, Mapping, Protocol

import artifact_contract
import control_state
import mission_contract
import plan_lane
import state_manager


POLL_KIND = "repo-agent-loop-poll.v1"
TASK_MARKER = re.compile(r"<!--\s*repo-agent-task:v1\s*(\{.*?\})\s*-->", re.DOTALL)
REPOSITORY = re.compile(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")
BRANCH = re.compile(r"^[A-Za-z0-9._/-]{1,200}$")
HEX40 = re.compile(r"^[0-9a-f]{40}$")
OWNER_APPROVAL_MARKER = re.compile(
    r"^<!--\s*steward-owner-approval:v1\s*(\{.*\})\s*-->$", re.DOTALL
)
_CONSUMED_APPROVAL_IDS: set[tuple[str, str]] = set()


class LoopUnavailable(RuntimeError):
    """Raised when a required live fact cannot be read unambiguously."""


class GitHubReader(Protocol):
    def read_control_state(self) -> dict[str, Any]: ...
    def repository_metadata(self) -> dict[str, Any]: ...
    def current_user(self) -> str: ...
    def accepted_main_sha(self, branch: str) -> str: ...
    def list_ready_issues(self) -> list[dict[str, Any]]: ...
    def labels_for_issue(self, issue_number: int) -> set[str]: ...
    def has_open_issue_pr(self, issue_number: int) -> bool: ...
    def active_issue_scopes(self) -> dict[int, list[str]]: ...
    def accepted_plan_document(self, source_main_sha: str) -> str: ...
    def accepted_route_document(self, source_main_sha: str) -> str: ...
    def accepted_status_document(self, source_main_sha: str) -> str: ...
    def plan_ledger_issue(self) -> int: ...

    def issue_snapshot(self, issue_number: int) -> dict[str, str]: ...
    def issue_comments(self, issue_number: int) -> list[dict[str, Any]]: ...

    def dispatch_controller(self, command: str, fields: dict[str, object]) -> None: ...


class GitReader(Protocol):
    def refresh_origin_main(self, repo_path: Path, branch: str) -> None: ...
    def origin_main_sha(self, repo_path: Path, branch: str) -> str: ...


class GitHubOwnerApprovalAuthenticator:
    """Authenticate a one-time owner approval carried by a GitHub Issue.

    The wire ``owner_identity`` is only a claim.  Authentication comes from
    the authoritative comment author, repository owner, exact proposal digest,
    current accepted-main binding, timestamp freshness, and a process-local
    replay set.  This adapter is read-only and never posts or edits approval
    state.
    """

    def __init__(
        self,
        github: GitHubReader,
        *,
        repository: str,
        issue_number: int,
        accepted_main_sha: str,
        now: Callable[[], datetime] | None = None,
        max_age_seconds: int = 7 * 24 * 60 * 60,
        replay_store: Any | None = None,
    ) -> None:
        if not REPOSITORY.fullmatch(repository):
            raise ValueError("repository must be owner/name")
        if type(issue_number) is not int or issue_number <= 0:
            raise ValueError("approval Issue number is invalid")
        if HEX40.fullmatch(accepted_main_sha) is None:
            raise ValueError("accepted-main SHA is invalid")
        if type(max_age_seconds) is not int or not 60 <= max_age_seconds <= 31 * 24 * 60 * 60:
            raise ValueError("approval freshness bound is invalid")
        self.github = github
        self.repository = repository
        self.issue_number = issue_number
        self.accepted_main_sha = accepted_main_sha
        self.now = now or (lambda: datetime.now(timezone.utc))
        self.max_age_seconds = max_age_seconds
        self.replay_store = replay_store
        self._used_approval_ids: set[str] = set()

    @staticmethod
    def _author_login(comment: Mapping[str, Any]) -> str | None:
        author = comment.get("author")
        if isinstance(author, dict):
            login = author.get("login")
            return login if isinstance(login, str) else None
        return author if isinstance(author, str) else None

    def _candidates(
        self, proposal_sha256: str
    ) -> list[tuple[mission_contract.OwnerApproval, str, dict[str, Any]]]:
        if not isinstance(proposal_sha256, str) or not mission_contract.SHA256.fullmatch(proposal_sha256):
            return []
        try:
            metadata = self.github.repository_metadata()
            owner = metadata.get("owner") if isinstance(metadata, dict) else None
            name_with_owner = metadata.get("name_with_owner") if isinstance(metadata, dict) else None
            current_user = self.github.current_user()
            comments = self.github.issue_comments(self.issue_number)
        except (AttributeError, LoopUnavailable, TypeError, ValueError):
            return []
        if (
            not isinstance(owner, str)
            or not isinstance(name_with_owner, str)
            or name_with_owner.casefold() != self.repository.casefold()
            or not isinstance(current_user, str)
        ):
            return []
        if owner.casefold() != current_user.casefold():
            return []
        candidates: list[tuple[mission_contract.OwnerApproval, str, dict[str, Any]]] = []
        for comment in comments:
            if not isinstance(comment, Mapping):
                continue
            author = self._author_login(comment)
            if not isinstance(author, str) or author.casefold() != owner.casefold():
                continue
            body = comment.get("body")
            match = OWNER_APPROVAL_MARKER.fullmatch(body.strip()) if isinstance(body, str) else None
            if match is None:
                continue
            try:
                payload = json.loads(match.group(1))
                if not isinstance(payload, dict) or set(payload) != {
                    "owner_identity", "proposal_sha256", "approval_id", "approved_at", "accepted_main_sha"
                }:
                    continue
                if payload["accepted_main_sha"] != self.accepted_main_sha:
                    continue
                approval = mission_contract.OwnerApproval.from_wire(
                    {
                        key: payload[key]
                        for key in ("owner_identity", "proposal_sha256", "approval_id", "approved_at")
                    }
                )
            except (KeyError, TypeError, ValueError, json.JSONDecodeError, mission_contract.MissionContractError):
                continue
            if approval.owner_identity != "repository-owner" or approval.proposal_sha256 != proposal_sha256:
                continue
            created_at = comment.get("createdAt")
            if not isinstance(created_at, str) or created_at != approval.approved_at:
                continue
            candidates.append((approval, self.accepted_main_sha, comment))
        return candidates

    def read_approval(self, proposal_sha256: str) -> mission_contract.OwnerApproval | None:
        """Return exactly one current owner approval, without consuming it."""

        candidates = self._candidates(proposal_sha256)
        if len(candidates) != 1:
            return None
        approval = candidates[0][0]
        try:
            approved_at = datetime.fromisoformat(approval.approved_at.replace("Z", "+00:00"))
            now = self.now()
            if approved_at.tzinfo is None or now.tzinfo is None:
                return None
            age = (now - approved_at).total_seconds()
        except (TypeError, ValueError, OverflowError):
            return None
        if age < -300 or age > self.max_age_seconds:
            return None
        return approval

    def verify(self, approval: mission_contract.OwnerApproval, proposal_sha256: str) -> bool:
        if not isinstance(approval, mission_contract.OwnerApproval):
            return False
        replay_key = (self.repository.casefold(), approval.approval_id)
        if (
            approval.approval_id in self._used_approval_ids
            or replay_key in _CONSUMED_APPROVAL_IDS
        ):
            return False
        candidates = self._candidates(proposal_sha256)
        if len(candidates) != 1 or candidates[0][0] != approval:
            return False
        try:
            approved_at = datetime.fromisoformat(approval.approved_at.replace("Z", "+00:00"))
            now = self.now()
            if approved_at.tzinfo is None or now.tzinfo is None:
                return False
            age = (now - approved_at).total_seconds()
        except (TypeError, ValueError, OverflowError):
            return False
        if age < -300 or age > self.max_age_seconds:
            return False
        if self.replay_store is not None:
            consume = getattr(self.replay_store, "consume_owner_approval", None)
            if not callable(consume):
                return False
            try:
                consumed = consume(
                    repository=self.repository,
                    mission_id=mission_contract.CAMPAIGN_MISSION_ID,
                    approval_id=approval.approval_id,
                    proposal_sha256=proposal_sha256,
                    accepted_main_sha=self.accepted_main_sha,
                )
            except Exception:
                return False
            if consumed is not True:
                return False
        self._used_approval_ids.add(approval.approval_id)
        _CONSUMED_APPROVAL_IDS.add(replay_key)
        return True


class _AuthenticatedApprovalReplay:
    """Pass one already-authenticated approval through the planning seam."""

    def __init__(self, approval: mission_contract.OwnerApproval) -> None:
        self.approval = approval

    def verify(self, approval: mission_contract.OwnerApproval, proposal_sha256: str) -> bool:
        return approval == self.approval and approval.proposal_sha256 == proposal_sha256


@dataclass(frozen=True)
class LocalRunOnceResult:
    """Bounded result for one local attempt; durable state remains on GitHub."""

    status: str
    issue_number: int
    attempt_id: str
    details: dict[str, Any] = field(default_factory=dict)

    def to_wire(self) -> dict[str, Any]:
        return {
            "kind": "repo-agent-local-run-once.v1",
            "status": self.status,
            "issue_number": self.issue_number,
            "attempt_id": self.attempt_id,
            "details": self.details,
        }


def _decision(status: str, *, action: str = "none", **fields: Any) -> dict[str, Any]:
    return {
        "kind": POLL_KIND,
        "status": status,
        "action": action,
        "selected": [],
        "rejected": [],
        "deferred_issue_numbers": [],
        **fields,
    }


def task_main_sha(body: str) -> str:
    """Parse the task body's accepted-main binding marker.

    This is the canonical parser for the ``repo-agent-task:v1`` marker; the
    trusted controller gateway (``dispatcher.claim_local``) reuses it so the
    local loop and the server-side claim gate can never disagree about which
    marker binds a task to its accepted main SHA.
    """

    matches = TASK_MARKER.findall(body or "")
    if len(matches) != 1:
        raise ValueError("task must contain exactly one repo-agent-task.v1 marker")
    try:
        value = json.loads(matches[0])
    except json.JSONDecodeError as exc:
        raise ValueError("repo-agent-task.v1 marker is invalid JSON") from exc
    if not isinstance(value, dict) or set(value) != {"accepted_main_sha"}:
        raise ValueError("repo-agent-task.v1 must contain only accepted_main_sha")
    sha = value["accepted_main_sha"]
    if not isinstance(sha, str) or not HEX40.fullmatch(sha):
        raise ValueError("repo-agent-task.v1 accepted_main_sha must be 40 lowercase hex")
    return sha


def _normalized_issue(raw: dict[str, Any]) -> dict[str, Any]:
    try:
        number = int(raw["number"])
        title = raw["title"]
        url = raw["url"]
        author = raw["author"]
        body = raw["body"]
        labels = raw["labels"]
    except (KeyError, TypeError, ValueError) as exc:
        raise LoopUnavailable("GitHub returned a malformed ready Issue") from exc
    if (
        number <= 0
        or not isinstance(title, str)
        or not isinstance(url, str)
        or not isinstance(author, str)
        or not isinstance(body, str)
        or not isinstance(labels, list)
        or not all(isinstance(label, str) and label for label in labels)
    ):
        raise LoopUnavailable("GitHub returned a malformed ready Issue")
    return {
        "number": number,
        "title": title,
        "url": url,
        "author": author,
        "body": body,
        "labels": set(labels),
    }


class LoopController:
    """Deep read-only module for one deterministic local-loop poll."""

    def __init__(
        self,
        github: GitHubReader,
        git: GitReader,
        *,
        repository: str,
        repo_path: Path,
        max_active: int = state_manager.MAX_ACTIVE,
    ) -> None:
        if not REPOSITORY.fullmatch(repository):
            raise ValueError("repository must be owner/name")
        if max_active < 1 or max_active > state_manager.MAX_ACTIVE:
            raise ValueError(
                f"max_active must be between 1 and {state_manager.MAX_ACTIVE}"
            )
        self.github = github
        self.git = git
        self.repository = repository
        self.repo_path = Path(repo_path).expanduser().resolve()
        self.max_active = max_active
        self._active_mission: mission_contract.MaintenanceMission | None = None
        self._active_approval: mission_contract.OwnerApproval | None = None

    @staticmethod
    def _fresh_stage_pr_facts(steward: Any, repository: str, stage_pr: Any) -> dict[str, Any]:
        """Read fresh facts for the supplied Stage PR number."""

        if type(stage_pr) is int:
            pr_number = stage_pr
        elif isinstance(stage_pr, Mapping):
            pr_number = stage_pr.get("pr_number")
        else:
            pr_number = getattr(stage_pr, "pr_number", None)
        if type(pr_number) is not int or not 1 <= pr_number <= 1_000_000_000:
            raise LoopUnavailable("stage_pr_number_invalid")
        reader = getattr(steward, "github", None)
        fetch = getattr(reader, "fetch_stage_pr", None)
        if not callable(fetch):
            raise LoopUnavailable("stage_pr_reader_unavailable")
        try:
            facts = fetch(repository, pr_number)
        except Exception as exc:
            raise LoopUnavailable("stage_pr_facts_unavailable") from exc
        if not isinstance(facts, dict):
            raise LoopUnavailable("stage_pr_facts_invalid")
        return facts

    def poll(self) -> dict[str, Any]:
        try:
            control = self.github.read_control_state()
            if control.get("emergency_stop") or not control.get("orchestrator_enabled"):
                return _decision("control_stopped", reason="orchestrator_disabled_or_stopped")

            metadata = self.github.repository_metadata()
            if str(metadata.get("name_with_owner", "")).casefold() != self.repository.casefold():
                return _decision("identity_rejected", reason="repository_identity_mismatch")
            owner = metadata.get("owner")
            current_user = self.github.current_user()
            if not isinstance(owner, str) or current_user.casefold() != owner.casefold():
                return _decision("identity_rejected", reason="authenticated_user_is_not_owner")
            branch = metadata.get("default_branch")
            if not isinstance(branch, str) or not BRANCH.fullmatch(branch):
                raise LoopUnavailable("repository default branch is unavailable or invalid")

            accepted_main = self.github.accepted_main_sha(branch)
            local_main = self.git.origin_main_sha(self.repo_path, branch)
            if not HEX40.fullmatch(accepted_main) or not HEX40.fullmatch(local_main):
                raise LoopUnavailable("accepted main identity is unavailable or invalid")
            if accepted_main != local_main:
                return _decision(
                    "stale_checkout",
                    accepted_main_sha=accepted_main,
                    local_origin_main_sha=local_main,
                    reason="fetch_required",
                )

            active_plan_claims: list[dict[str, Any]] = []
            if hasattr(self.github, "active_execution_scopes"):
                execution = self.github.active_execution_scopes()
                if not isinstance(execution, dict):
                    raise LoopUnavailable("active execution capacity is unavailable or invalid")
                active_scopes = execution.get("scopes")
                active = execution.get("issue_numbers")
                active_plan_claims = execution.get("plans", [])
            else:
                active_scopes = self.github.active_issue_scopes()
                active = set(active_scopes) if isinstance(active_scopes, dict) else None
            if (
                not isinstance(active_scopes, dict)
                or not all(
                    (
                        (isinstance(issue, int) and issue > 0)
                        or (isinstance(issue, str) and issue.startswith("plan:"))
                    )
                    and isinstance(paths, list)
                    and bool(paths)
                    and all(isinstance(path, str) and path for path in paths)
                    for issue, paths in active_scopes.items()
                )
            ):
                raise LoopUnavailable("active Issue scope state is unavailable or invalid")
            if not isinstance(active, set) or not all(isinstance(issue, int) and issue > 0 for issue in active):
                raise LoopUnavailable("active Issue capacity state is unavailable or invalid")
            if not isinstance(active_plan_claims, list) or not all(isinstance(item, dict) for item in active_plan_claims):
                raise LoopUnavailable("active plan capacity state is unavailable or invalid")
            if len(active) + len(active_plan_claims) >= self.max_active:
                return _decision(
                    "capacity_full",
                    accepted_main_sha=accepted_main,
                    active_issue_numbers=sorted(active),
                    active_plan_subject_ids=sorted(
                        item.get("subject_id") for item in active_plan_claims
                        if isinstance(item.get("subject_id"), str)
                    ),
                )

            rejected: list[dict[str, Any]] = []
            eligible: list[dict[str, Any]] = []
            # Plan-derived candidates are admitted only when every terminal
            # owner can provably bind the plan subject.  Active plan capacity
            # still counts toward K so a leftover ledger claim cannot be
            # ignored when sizing Issue work.
            try:
                plan_document = self.github.accepted_plan_document(accepted_main)
            except AttributeError:
                plan_document = None
            except LoopUnavailable:
                # An unreadable plan document must not prevent Issue admission.
                # Issues still fail closed on their own markers and capacity
                # checks; the plan candidate is rejected with its reason.
                plan_document = None
            if plan_document is not None:
                try:
                    status_document = self.github.accepted_status_document(accepted_main)
                    completed_packet_ids = plan_lane.accepted_completed_packet_ids(status_document)
                    plan = plan_lane.parse_optional(
                        plan_document,
                        accepted_main,
                        completed_packet_ids=completed_packet_ids,
                    )
                except plan_lane.PlanLaneError as exc:
                    # Structural plan parse failures are non-admission signals,
                    # not Issue-path blockers.
                    plan = None
                    rejected.append(
                        {
                            "candidate_kind": "plan",
                            "reason": f"plan_lane_deferred:{exc.reason}",
                        }
                    )
                if plan is not None:
                    ready, missing = self._plan_terminal_owner_readiness()
                    if ready:
                        eligible.append(plan.to_wire())
                    else:
                        rejected.append(
                            {
                                "candidate_kind": "plan",
                                "subject_id": plan.packet_id,
                                "reason": f"plan_lane_not_ready:{','.join(missing)}",
                            }
                        )
            for issue in sorted(
                (_normalized_issue(item) for item in self.github.list_ready_issues()),
                key=lambda item: item["number"],
            ):
                rejection = self._evaluate_issue(issue, owner, accepted_main)
                if rejection is not None:
                    rejected.append({"issue_number": issue["number"], **rejection})
                    continue
                allowed_paths = artifact_contract.parse_issue_scope(issue["body"])
                eligible.append(
                    {
                        "candidate_kind": "issue",
                        "issue_number": issue["number"],
                        "title": issue["title"],
                        "url": issue["url"],
                        "author": issue["author"],
                        "allowed_paths": allowed_paths,
                        "task_body_sha256": hashlib.sha256(
                            issue["body"].encode("utf-8")
                        ).hexdigest(),
                    }
                )

            selected: list[dict[str, Any]] = []
            deferred: list[int] = []
            occupied = [(issue, active_scopes[issue]) for issue in sorted(active)]
            occupied.extend(
                (f"plan:{item.get('subject_id')}", item.get("allowed_paths"))
                for item in active_plan_claims
                if isinstance(item.get("subject_id"), str)
                and isinstance(item.get("allowed_paths"), list)
            )
            available_slots = self.max_active - len(active) - len(active_plan_claims)
            for candidate in eligible:
                conflict = next(
                    (
                        issue
                        for issue, paths in occupied
                        if artifact_contract.scopes_overlap(
                            candidate["allowed_paths"], paths
                        )
                    ),
                    None,
                )
                if conflict is not None:
                    rejected.append(
                        {
                            **(
                                {"issue_number": candidate["issue_number"]}
                                if candidate.get("candidate_kind") == "issue"
                                else {"candidate_kind": "plan", "subject_id": candidate["subject_id"]}
                            ),
                            "reason": "scope_conflict",
                            "conflicts_with": conflict,
                        }
                    )
                    continue
                if len(selected) >= available_slots:
                    if candidate.get("candidate_kind") == "issue":
                        deferred.append(candidate["issue_number"])
                    continue
                selected.append(candidate)
                occupied.append(
                    (
                        candidate["issue_number"]
                        if candidate.get("candidate_kind") == "issue"
                        else f"plan:{candidate['subject_id']}",
                        candidate["allowed_paths"],
                    )
                )

            if not selected:
                return _decision(
                    "no_eligible_task",
                    accepted_main_sha=accepted_main,
                    active_issue_numbers=sorted(active),
                    rejected=rejected,
                    deferred_issue_numbers=deferred,
                )
            return _decision(
                "ready",
                action="run_many" if len(selected) > 1 else "run_once",
                accepted_main_sha=accepted_main,
                active_issue_numbers=sorted(active),
                selected=selected,
                rejected=rejected,
                deferred_issue_numbers=deferred,
            )
        except LoopUnavailable as exc:
            return _decision("unavailable", reason=str(exc)[:300])

    def _current_identity(self) -> tuple[str, str, str, str, str]:
        """Read and cross-check the current accepted-main Mission identity."""

        control = self.github.read_control_state()
        if control.get("emergency_stop") or not control.get("orchestrator_enabled"):
            raise LoopUnavailable("orchestrator_disabled_or_stopped")
        metadata = self.github.repository_metadata()
        if str(metadata.get("name_with_owner", "")).casefold() != self.repository.casefold():
            raise LoopUnavailable("repository_identity_mismatch")
        owner = metadata.get("owner")
        if not isinstance(owner, str) or self.github.current_user().casefold() != owner.casefold():
            raise LoopUnavailable("authenticated_user_is_not_owner")
        branch = metadata.get("default_branch")
        if not isinstance(branch, str) or not BRANCH.fullmatch(branch):
            raise LoopUnavailable("repository default branch is unavailable or invalid")
        accepted_main = self.github.accepted_main_sha(branch)
        self.git.refresh_origin_main(self.repo_path, branch)
        local_main = self.git.origin_main_sha(self.repo_path, branch)
        if not HEX40.fullmatch(accepted_main) or accepted_main != local_main:
            raise LoopUnavailable("accepted main and local checkout are not identical")
        return (
            self.repository,
            accepted_main,
            branch,
            mission_contract.CAMPAIGN_SOURCE_REF,
            mission_contract.CAMPAIGN_SOURCE_SHA256,
        )

    def run_mission_stage(
        self,
        proposal: object,
        *,
        approval_issue: int,
        steward: Any,
        stage_pr: Any = None,
        now: Callable[[], datetime] | None = None,
    ) -> dict[str, Any]:
        """Load, authenticate, plan, and execute one current-main Mission.

        This is the non-test production bridge from the sole legacy loop
        writer into the provider-free Steward.  It performs no lifecycle
        write itself; the existing controller remains responsible for that
        boundary and the Steward owns only its journal/Stage execution facts.
        """

        try:
            repository, accepted_main, branch, source_ref, source_sha256 = self._current_identity()
            proposal_sha256 = getattr(proposal, "proposal_sha256", "")
            active_mission = self._active_mission
            active_approval = self._active_approval
            can_continue = (
                stage_pr is not None
                and active_mission is not None
                and active_approval is not None
                and active_approval.proposal_sha256 == proposal_sha256
                and active_mission.repository_identity.repository == repository
                and active_mission.repository_identity.base_sha == accepted_main
                and active_mission.repository_identity.branch == branch
                and active_mission.repository_identity.source_ref == source_ref
                and active_mission.repository_identity.source_sha256 == source_sha256
            )
            if can_continue:
                mission = active_mission
                approval = active_approval
            else:
                authenticator = GitHubOwnerApprovalAuthenticator(
                    self.github,
                    repository=self.repository,
                    issue_number=approval_issue,
                    accepted_main_sha=accepted_main,
                    now=now,
                    replay_store=getattr(steward, "journal", None),
                )
                approval = authenticator.read_approval(proposal_sha256)
                if approval is None:
                    return {
                        "status": "waiting_approval",
                        "mission_id": mission_contract.CAMPAIGN_MISSION_ID,
                        "proposal_sha256": proposal_sha256,
                        "stage": None,
                    }
                mission = mission_contract.activate_current_mission(
                    repository=repository,
                    base_sha=accepted_main,
                    branch=branch,
                    source_ref=source_ref,
                    source_sha256=source_sha256,
                    proposal_sha256=proposal_sha256,
                    owner_approval=approval,
                    owner_authenticator=authenticator,
                )
                self._active_mission = mission
                self._active_approval = approval
            import shadow_steward

            plan = shadow_steward.plan_stage(
                proposal,
                mission,
                approval,
                owner_authenticator=_AuthenticatedApprovalReplay(approval),
            )
            if plan.disposition != "PLANNED" or plan.stage is None:
                return {
                    "status": plan.disposition.lower(),
                    "mission_id": mission.mission_id,
                    "proposal_sha256": proposal_sha256,
                    "stage": None,
                }
            if stage_pr is not None:
                fresh_stage_pr = self._fresh_stage_pr_facts(
                    steward, repository, stage_pr
                )
                execution = steward.continue_stage_to_waiting_for_merge(
                    mission,
                    plan.stage,
                    plan.workcards,
                    stage_pr=fresh_stage_pr,
                )
            else:
                execution = steward.execute_stage_to_waiting_for_merge(
                    mission,
                    plan.stage,
                    plan.workcards,
                    base_sha=accepted_main,
                    title=f"feat: Autonomous Steward {plan.stage.stage_id}",
                    body=(
                        "Provider-free repository-maintenance Stage.\n\n"
                        f"Mission: {mission.mission_id}\n"
                        f"Stage: {plan.stage.stage_id}"
                    ),
                )
            return {
                "status": execution.get("status", "executed"),
                "mission_id": mission.mission_id,
                "accepted_main_sha": accepted_main,
                "stage_id": plan.stage.stage_id,
                "workcard_count": len(plan.workcards),
                "stage": (
                    execution.get("stage").to_wire()
                    if hasattr(execution.get("stage"), "to_wire")
                    else execution.get("stage")
                ),
                "integration": (
                    execution.get("integration").to_wire()
                    if hasattr(execution.get("integration"), "to_wire")
                    else execution.get("integration")
                ),
                "pr": execution.get("pr"),
                "results": {
                    card_id: result.to_wire() if hasattr(result, "to_wire") else result
                    for card_id, result in execution.get("results", {}).items()
                },
            }
        except (AttributeError, TypeError, ValueError, LoopUnavailable) as exc:
            return _decision("unavailable", reason=str(exc)[:300])

    def _evaluate_issue(
        self, issue: dict[str, Any], owner: str, accepted_main: str
    ) -> dict[str, Any] | None:
        labels = issue["labels"]
        if (
            state_manager.LABEL_READY not in labels
            or labels & (state_manager.ACTIVE_LABELS | state_manager.TERMINAL_LABELS)
        ):
            return {"reason": "invalid_state_labels"}
        if issue["author"].casefold() != owner.casefold():
            return {"reason": "untrusted_author"}
        try:
            task_main = task_main_sha(issue["body"])
        except ValueError:
            return {"reason": "invalid_task_binding"}
        if task_main != accepted_main:
            return {"reason": "accepted_main_mismatch"}
        try:
            artifact_contract.parse_issue_scope(issue["body"])
        except (artifact_contract.ArtifactContractError, TypeError, ValueError):
            return {"reason": "invalid_scope"}
        for dependency in sorted(state_manager.parse_dependencies(issue["body"])):
            labels = self.github.labels_for_issue(dependency)
            if state_manager.LABEL_COMPLETE not in labels:
                return {"reason": "dependency_incomplete", "dependency": dependency}
        if self.github.has_open_issue_pr(issue["number"]):
            return {"reason": "open_pr_exists"}
        return None

    def _plan_terminal_owner_readiness(self) -> tuple[bool, list[str]]:
        """Prove the terminal owners can bind a plan subject before admission.

        Each owner is verified from authoritative repository state; a missing
        or unverifiable owner rejects the candidate.  The checks are read-only
        and provider-free: the ledger Issue resolves, the canonical CI
        workflow and CI monitor workflow exist in the accepted checkout, the
        review owner accepts PR-bound subjects, the repository-maintenance
        merge owner is documented, and the canonical closeout owner accepts
        the packet.  The current accepted-main checkout is the sole evidence
        source; no owner may be inferred.
        """

        workflow_dir = Path(self.repo_path) / ".github" / "workflows"
        canonical_tests = workflow_dir / "tests.yml"
        ci_monitor = workflow_dir / "agent-ci-monitor.yml"
        review_owner = Path(self.repo_path) / "scripts" / "agent-control" / "review_loop_cli.py"
        merge_owner = Path(self.repo_path) / "docs" / "REAL_WORLD_TESTING_PLAYBOOK.md"
        closeout_owner = Path(self.repo_path) / "docs" / "CURRENT_STATUS.md"
        ledger_issue = 0
        try:
            ledger_issue = self.github.plan_ledger_issue()
        except LoopUnavailable:
            ledger_issue = 0
        return plan_lane.terminal_owner_readiness(
            ledger_issue=ledger_issue,
            canonical_tests_workflow_present=canonical_tests.is_file(),
            ci_monitor_workflow_present=ci_monitor.is_file(),
            review_owner_present=review_owner.is_file(),
            merge_owner_present=merge_owner.is_file(),
            closeout_owner_present=closeout_owner.is_file(),
        )


class GitHubAdapter:
    """Read-only gh adapter.  It never executes Issue text or writes GitHub."""

    def __init__(self, repository: str, *, timeout_seconds: int = 30) -> None:
        if not REPOSITORY.fullmatch(repository):
            raise ValueError("repository must be owner/name")
        self.repository = repository
        self.timeout_seconds = timeout_seconds

    def _gh_json(self, *args: str) -> Any:
        try:
            result = subprocess.run(
                ["gh", *args],
                capture_output=True,
                text=True,
                timeout=self.timeout_seconds,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise LoopUnavailable("GitHub CLI is unavailable") from exc
        if result.returncode != 0:
            raise LoopUnavailable("GitHub query failed")
        try:
            return json.loads(result.stdout)
        except json.JSONDecodeError as exc:
            raise LoopUnavailable("GitHub returned invalid JSON") from exc

    def read_control_state(self) -> dict[str, Any]:
        try:
            return control_state.read_control_state(self.repository)
        except control_state.ControlStateError as exc:
            raise LoopUnavailable("control state is unavailable") from exc

    def repository_metadata(self) -> dict[str, Any]:
        value = self._gh_json(
            "repo",
            "view",
            self.repository,
            "--json",
            "nameWithOwner,isPrivate,owner,defaultBranchRef",
        )
        try:
            return {
                "name_with_owner": value["nameWithOwner"],
                "owner": value["owner"]["login"],
                "is_private": value["isPrivate"],
                "default_branch": value["defaultBranchRef"]["name"],
            }
        except (KeyError, TypeError) as exc:
            raise LoopUnavailable("repository metadata is malformed") from exc

    def current_user(self) -> str:
        value = self._gh_json("api", "user")
        login = value.get("login") if isinstance(value, dict) else None
        if not isinstance(login, str) or not login:
            raise LoopUnavailable("authenticated GitHub identity is unavailable")
        return login

    def accepted_main_sha(self, branch: str) -> str:
        if not BRANCH.fullmatch(branch):
            raise LoopUnavailable("default branch is invalid")
        value = self._gh_json("api", f"repos/{self.repository}/git/ref/heads/{branch}")
        sha = value.get("object", {}).get("sha") if isinstance(value, dict) else None
        if not isinstance(sha, str):
            raise LoopUnavailable("accepted main SHA is unavailable")
        return sha

    def list_ready_issues(self) -> list[dict[str, Any]]:
        value = self._gh_json(
            "issue",
            "list",
            "--repo",
            self.repository,
            "--state",
            "open",
            "--label",
            state_manager.LABEL_READY,
            "--limit",
            "100",
            "--json",
            "number,title,url,author,body,labels",
        )
        if not isinstance(value, list):
            raise LoopUnavailable("ready Issue list is malformed")
        result = []
        for item in value:
            if not isinstance(item, dict):
                raise LoopUnavailable("ready Issue list is malformed")
            author = item.get("author")
            labels = item.get("labels")
            result.append(
                {
                    **item,
                    "author": author.get("login") if isinstance(author, dict) else None,
                    "labels": [
                        label.get("name") for label in labels
                    ] if isinstance(labels, list) else None,
                }
            )
        return result

    def labels_for_issue(self, issue_number: int) -> set[str]:
        value = self._gh_json(
            "issue", "view", str(issue_number), "--repo", self.repository, "--json", "labels"
        )
        labels = value.get("labels") if isinstance(value, dict) else None
        if not isinstance(labels, list):
            raise LoopUnavailable("dependency label state is unavailable")
        names = {item.get("name") for item in labels if isinstance(item, dict)}
        if None in names:
            raise LoopUnavailable("dependency label state is malformed")
        return names

    def has_open_issue_pr(self, issue_number: int) -> bool:
        value = state_manager.has_open_issue_pr(issue_number, self.repository)
        if value is None:
            raise LoopUnavailable("Issue PR binding is unavailable")
        return value

    def active_issue_scopes(self) -> dict[int, list[str]]:
        active = state_manager.get_active_issue_numbers(self.repository)
        if active is None:
            raise LoopUnavailable("active capacity state is unavailable")
        # Only trusted claim-bound scopes count as active scope.  The mutable
        # active Issue bodies are never re-read here.
        scopes = state_manager.get_active_issue_scopes(active, self.repository)
        if scopes is None:
            raise LoopUnavailable("active Issue scope state is unavailable or invalid")
        return scopes

    def accepted_plan_document(self, source_main_sha: str) -> str:
        """Read the canonical plan document at the already-verified SHA."""

        return self._accepted_document(
            "docs/NEXT_DECISION.md", source_main_sha, plan_lane.MAX_DOCUMENT_BYTES
        )

    def accepted_route_document(self, source_main_sha: str) -> str:
        """Read the canonical route index at the already-verified SHA."""

        return self._accepted_document(
            "docs/FUTURE_ROUTE.md", source_main_sha, plan_lane.MAX_DOCUMENT_BYTES
        )

    def accepted_status_document(self, source_main_sha: str) -> str:
        """Read the canonical accepted-status document at the verified SHA."""

        return self._accepted_document(
            "docs/CURRENT_STATUS.md", source_main_sha, plan_lane.MAX_DOCUMENT_BYTES
        )

    def _accepted_document(self, path: str, source_main_sha: str, max_bytes: int) -> str:
        import base64

        if not HEX40.fullmatch(source_main_sha):
            raise LoopUnavailable("accepted SHA is invalid")
        value = self._gh_json(
            "api", f"repos/{self.repository}/contents/{path}?ref={source_main_sha}"
        )
        if not isinstance(value, dict) or value.get("encoding") != "base64":
            raise LoopUnavailable(f"canonical document {path} is unavailable")
        content = value.get("content")
        if not isinstance(content, str):
            raise LoopUnavailable(f"canonical document {path} is malformed")
        try:
            # GitHub content API returns base64 with newlines; strip whitespace
            # before decode so a valid UTF-8 document is not fail-closed as
            # binary garbage.
            cleaned = "".join(content.split())
            decoded = base64.b64decode(cleaned, validate=True).decode("utf-8")
        except (ValueError, UnicodeDecodeError) as exc:
            raise LoopUnavailable(f"canonical document {path} is not valid UTF-8") from exc
        if len(decoded.encode("utf-8")) > max_bytes:
            raise LoopUnavailable(f"canonical document {path} exceeds the bounded contract")
        return decoded

    def plan_ledger_issue(self) -> int:
        try:
            ledger = control_state.read_plan_ledger(self.repository)
        except control_state.ControlStateError as exc:
            raise LoopUnavailable("plan execution ledger is unavailable") from exc
        number = ledger.get("number") if isinstance(ledger, dict) else None
        if type(number) is not int or number <= 0:
            raise LoopUnavailable("plan execution ledger is malformed")
        return number

    def active_execution_scopes(self) -> dict[str, Any]:
        """Return one capacity snapshot for Issue and plan-run subjects."""

        active = state_manager.get_active_issue_numbers(self.repository)
        if active is None:
            raise LoopUnavailable("active capacity state is unavailable")
        plans = state_manager.get_active_plan_claims(self.repository)
        if plans is None:
            raise LoopUnavailable("active plan capacity state is unavailable")
        for plan in plans:
            ledger_issue = plan.get("ledger_issue_number")
            if type(ledger_issue) is int:
                active.discard(ledger_issue)
        scopes = state_manager.get_active_issue_scopes(active, self.repository)
        if scopes is None:
            raise LoopUnavailable("active Issue scope state is unavailable or invalid")
        scopes = {**scopes}
        for item in plans:
            subject = item.get("subject_id") if isinstance(item, dict) else None
            paths = item.get("allowed_paths") if isinstance(item, dict) else None
            if not isinstance(subject, str) or not isinstance(paths, list):
                raise LoopUnavailable("active plan scope state is malformed")
            scopes[f"plan:{subject}"] = paths
        return {"issue_numbers": active, "plans": plans, "scopes": scopes}

    def issue_snapshot(self, issue_number: int) -> dict[str, str]:
        value = self._gh_json(
            "issue", "view", str(issue_number), "--repo", self.repository,
            "--json", "title,body",
        )
        if not isinstance(value, dict):
            raise LoopUnavailable("Issue snapshot is unavailable")
        title, body = value.get("title"), value.get("body")
        if not isinstance(title, str) or not isinstance(body, str):
            raise LoopUnavailable("Issue snapshot is malformed")
        return {"title": title, "body": body}

    def issue_comments(self, issue_number: int) -> list[dict[str, Any]]:
        value = self._gh_json(
            "issue", "view", str(issue_number), "--repo", self.repository,
            "--json", "comments",
        )
        comments = value.get("comments") if isinstance(value, dict) else None
        if not isinstance(comments, list) or not all(isinstance(item, dict) for item in comments):
            raise LoopUnavailable("Issue approval comments are unavailable or malformed")
        return list(reversed(comments))

    def dispatch_controller(self, command: str, fields: dict[str, object]) -> None:
        """Submit one allowlisted controller workflow request.

        The workflow's global concurrency group is the serialization point for
        claims and handoffs.  Values are passed as argv items, never through a
        shell, and the method deliberately returns no provider output.
        """

        if command not in {
            "claim-local", "handoff-local", "release-local", "block-local", "claim-plan",
            "handoff-plan", "lifecycle-plan", "promote-plan", "release-plan", "block-plan",
        }:
            raise LoopUnavailable("controller command is not allowed")
        allowed = {
            "claim-local": {"issue", "attempt_id", "client_token"},
            "handoff-local": {
                "issue", "attempt_id", "client_token", "head_sha", "claim_nonce",
            },
            "release-local": {
                "issue", "attempt_id", "client_token", "reason_code", "claim_nonce",
            },
            "block-local": {
                "issue", "attempt_id", "client_token", "reason_code", "claim_nonce",
            },
            "claim-plan": {"packet_id", "attempt_id"},
            "handoff-plan": {"packet_id", "attempt_id", "head_sha", "claim_nonce"},
            "lifecycle-plan": {"packet_id", "attempt_id", "stage"},
            "promote-plan": {"packet_id", "attempt_id"},
            "release-plan": {
                "packet_id", "attempt_id", "source_main_sha", "reason_code", "claim_nonce",
            },
            "block-plan": {
                "packet_id", "attempt_id", "source_main_sha", "claim_nonce",
            },
        }[command]
        if set(fields) != allowed:
            raise LoopUnavailable("controller fields are invalid")
        args = [
            "gh", "workflow", "run", "agent-controller.yml",
            "--repo", self.repository, "--ref", "main", "-f", f"command={command}",
        ]
        for key in sorted(fields):
            value = fields[key]
            if isinstance(value, bool) or not isinstance(value, (str, int)):
                raise LoopUnavailable("controller field value is invalid")
            text = str(value)
            if not text or "\x00" in text or len(text) > 256:
                raise LoopUnavailable("controller field value is invalid")
            args.extend(["-f", f"{key}={text}"])
        try:
            result = subprocess.run(
                args, capture_output=True, text=True, timeout=self.timeout_seconds
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise LoopUnavailable("controller workflow dispatch is unavailable") from exc
        if result.returncode != 0:
            raise LoopUnavailable("controller workflow dispatch failed")


class GitAdapter:
    def refresh_origin_main(self, repo_path: Path, branch: str) -> None:
        """Refresh precisely one remote-tracking default-branch ref.

        This is a local, read-only-from-the-server refresh.  It neither checks
        out a branch nor writes a remote ref, and the explicit refspec avoids
        widening a route run into a general remote-prune operation.
        """

        if not repo_path.is_dir() or not BRANCH.fullmatch(branch):
            raise LoopUnavailable("local repository path or branch is invalid")
        try:
            result = subprocess.run(
                [
                    "git", "fetch", "--no-tags", "origin",
                    f"+refs/heads/{branch}:refs/remotes/origin/{branch}",
                ],
                cwd=repo_path,
                capture_output=True,
                text=True,
                timeout=60,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise LoopUnavailable("local origin main refresh is unavailable") from exc
        if result.returncode != 0:
            raise LoopUnavailable("local origin main refresh failed")

    def origin_main_sha(self, repo_path: Path, branch: str) -> str:
        if not repo_path.is_dir() or not BRANCH.fullmatch(branch):
            raise LoopUnavailable("local repository path or branch is invalid")
        try:
            result = subprocess.run(
                ["git", "rev-parse", f"refs/remotes/origin/{branch}"],
                cwd=repo_path,
                capture_output=True,
                text=True,
                timeout=30,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise LoopUnavailable("local Git state is unavailable") from exc
        sha = result.stdout.strip()
        if result.returncode != 0 or not HEX40.fullmatch(sha):
            raise LoopUnavailable("local origin main SHA is unavailable")
        return sha


def local_client_token(repository: str, issue: int, attempt: str) -> str:
    """Derive the bounded retry-safe token shared with the controller."""

    return hashlib.sha256(
        f"local-run:{repository}:{issue}:{attempt}".encode("utf-8")
    ).hexdigest()[:32]


def plan_execution_token(repository: str, packet_id: str, source_main_sha: str, attempt: str) -> str:
    """Derive the bounded owner token for one exact plan generation."""

    return hashlib.sha256(
        f"plan-run:{repository}:{packet_id}:{source_main_sha}:{attempt}".encode("utf-8")
    ).hexdigest()[:32]
