"""Provider-free autonomous Steward coordinator.

The coordinator drives approved Mission/Stage/WorkCard projections through an
isolated worktree, bounded verification, independent review, and read-only
Stage PR reconciliation.  It deliberately stops at ``WAITING_FOR_MERGE``;
manual exact-head CI/review/merge owners retain their authority.
"""

from __future__ import annotations

from concurrent.futures import FIRST_COMPLETED, Future, ThreadPoolExecutor, wait
from dataclasses import dataclass, replace
import hashlib
import json
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


@dataclass(frozen=True)
class StageIntegration:
    """Parent-owned local Stage branch assembled from verified card heads."""

    stage_id: str
    branch: str
    base_sha: str
    head_sha: str
    card_heads: tuple[tuple[str, str], ...]

    def to_wire(self) -> dict[str, Any]:
        return {
            "schema_version": "steward_stage_integration.v1",
            "stage_id": self.stage_id,
            "branch": self.branch,
            "base_sha": self.base_sha,
            "head_sha": self.head_sha,
            "card_heads": [
                {"card_id": card_id, "head_sha": head_sha}
                for card_id, head_sha in self.card_heads
            ],
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


def _stage_branch(
    mission: contract.MaintenanceMission,
    stage: contract.Stage,
    base_sha: str,
) -> str:
    digest = hashlib.sha256(
        "\x00".join((mission.mission_id, stage.stage_id, base_sha)).encode("utf-8")
    ).hexdigest()[:24]
    return f"agent/stage-{digest}"


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


def _git_metadata_snapshot(
    worktree: Path, *, branch: str | None = None
) -> tuple[dict[str, str], str]:
    """Capture the bound branch and local config without retaining raw contents."""

    try:
        ref_args = ["git", "for-each-ref", "--format=%(refname) %(objectname)"]
        if branch is not None:
            ref_args.append(f"refs/heads/{branch}")
        refs = subprocess.run(
            ref_args,
            cwd=worktree,
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
        config = subprocess.run(
            ["git", "config", "--local", "--null", "--list"],
            cwd=worktree,
            capture_output=True,
            timeout=30,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise StewardError("worktree_metadata_unavailable") from exc
    if refs.returncode != 0 or config.returncode != 0:
        raise StewardError("worktree_metadata_unavailable")
    ref_map: dict[str, str] = {}
    for line in refs.stdout.splitlines():
        name, separator, object_id = line.partition(" ")
        if not separator or not name or not object_id:
            raise StewardError("worktree_metadata_invalid")
        ref_map[name] = object_id
    return ref_map, hashlib.sha256(config.stdout).hexdigest()


def _digest(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()[:24]


def _journal_detail(value: str) -> str:
    """Keep child/provider-shaped text out of the durable operator journal."""

    if isinstance(value, str) and value and _SAFE_IDENTIFIER.fullmatch(value):
        return value
    return f"reason_{_digest(str(value))}"


def _journal_key(
    kind: str,
    mission: contract.MaintenanceMission,
    stage: contract.Stage,
    card: contract.WorkCard,
    *parts: object,
) -> str:
    """Bind every Steward key to the full Mission/Stage/card identity."""

    raw = ":".join(
        (
            kind,
            mission.mission_id,
            stage.stage_id,
            card.card_id,
            *(str(part) for part in parts),
        )
    )
    return raw if len(raw) <= 128 else f"{kind}:{_digest(raw)}"


def _stage_pr_facts(
    value: steward_github.StagePRFacts | dict[str, Any] | None,
) -> steward_github.StagePRFacts | None:
    if value is None:
        return None
    if isinstance(value, steward_github.StagePRFacts):
        return steward_github.StagePRFacts.from_wire(value.__dict__)
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
        if verifier is not None:
            raise StewardError("verifier_injection_forbidden")
        self.repository = repository
        self.repo_path = Path(repo_path).resolve()
        self.journal = journal
        self.github = github
        self.worker = worker or workers.ProviderFreeWorker()
        self.reviewer = reviewer
        self.verifier = workers.run_allowlisted_checks
        self.lock_dir = Path(lock_dir).resolve()
        self.max_concurrency = max_concurrency
        self.service: StewardService | None = None
        self.mission_id: str | None = None
        self.mission_binding: tuple[str, ...] | None = None

    def _service_for(self, mission: contract.MaintenanceMission) -> StewardService:
        binding = (
            mission.mission_id,
            mission.state,
            mission.repository_identity.repository,
            mission.repository_identity.base_sha,
            mission.repository_identity.branch,
            mission.repository_identity.source_ref,
            mission.repository_identity.source_sha256,
        )
        if self.service is None or self.mission_binding != binding:
            self.service = StewardService(
                mission_id=mission.mission_id,
                journal=self.journal,
                github=self.github,
                repo_path=self.repo_path,
                mission=mission,
            )
            self.mission_id = mission.mission_id
            self.mission_binding = binding
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

    def execute_stage(
        self,
        mission: contract.MaintenanceMission,
        stage: contract.Stage,
        cards: tuple[contract.WorkCard, ...],
        *,
        base_sha: str,
        stage_pr: steward_github.StagePRFacts | dict[str, Any] | None = None,
    ) -> dict[str, ExecutionResult]:
        """Enter one full stage through service preflight and bounded dispatch."""

        return self._service_for(mission).execute_stage(
            self.dispatch_cards,
            mission=mission,
            stage=stage,
            cards=cards,
            base_sha=base_sha,
            stage_pr=stage_pr,
        )

    def execute_stage_to_waiting_for_merge(
        self,
        mission: contract.MaintenanceMission,
        stage: contract.Stage,
        cards: tuple[contract.WorkCard, ...],
        *,
        base_sha: str,
        title: str,
        body: str,
    ) -> dict[str, Any]:
        """Run the parent-owned provider-free Stage promotion path.

        The method stops before merge.  Child cards only produce reviewed
        commits; this parent then assembles and pushes one exact Stage branch,
        binds its Draft PR through the existing owner, and re-reads live facts.
        A newly created Draft remains ``stage_pr_draft`` until the existing
        Ready, CI, and review owners make the exact head merge-eligible.
        """

        results = self.execute_stage(
            mission, stage, cards, base_sha=base_sha, stage_pr=None
        )
        if any(
            result.status not in {"WAITING_FOR_PR", "COMPLETE"}
            for result in results.values()
        ) or set(results) != {card.card_id for card in cards}:
            raise StewardError("stage_execution_not_ready")
        integration = self.assemble_stage(
            mission, stage, cards, results, base_sha=base_sha
        )
        self.publish_stage_branch(integration)
        bound_stage, pr = self.bind_stage_draft_pr(
            mission,
            stage,
            cards,
            integration,
            title=title,
            body=body,
        )
        observed = self.github.fetch_stage_pr(self.repository, int(pr["number"]))
        facts = _stage_pr_facts(observed)
        if facts is None:
            raise StewardError("stage_pr_facts_required")
        if facts.draft:
            return {
                "status": "stage_pr_draft",
                "stage": bound_stage,
                "integration": integration,
                "pr": pr,
                "results": results,
            }
        reconciled = self.reconcile_stage_pr(
            mission, bound_stage, cards, stage_pr=observed
        )
        status = (
            "waiting_for_merge"
            if all(result.status == "WAITING_FOR_MERGE" for result in reconciled.values())
            else "stage_pr_waiting"
        )
        return {
            "status": status,
            "stage": bound_stage,
            "integration": integration,
            "pr": pr,
            "results": reconciled,
        }

    def continue_stage_to_waiting_for_merge(
        self,
        mission: contract.MaintenanceMission,
        stage: contract.Stage,
        cards: tuple[contract.WorkCard, ...],
        *,
        stage_pr: steward_github.StagePRFacts | dict[str, Any],
    ) -> dict[str, Any]:
        """Resume the parent-owned Stage gate after Draft promotion.

        Ready/CI/review owners operate on the existing Stage PR.  This seam
        only rebinds the live exact PR facts and reuses the canonical
        reconciliation owner; it never calls Draft create/update again or
        attempts to mark Ready, merge, or alter lifecycle ownership.
        """

        facts = _stage_pr_facts(stage_pr)
        if facts is None:
            raise StewardError("stage_pr_facts_required")
        bound_stage = replace(
            stage,
            integration_pr=facts.pr_number,
            exact_head=facts.head_sha,
        )
        if facts.draft:
            return {
                "status": "stage_pr_draft",
                "stage": bound_stage,
                "pr": {"number": facts.pr_number, "head_sha": facts.head_sha},
                "results": {},
            }
        reconciled = self.reconcile_stage_pr(
            mission, bound_stage, cards, stage_pr=facts
        )
        status = (
            "waiting_for_merge"
            if all(result.status == "WAITING_FOR_MERGE" for result in reconciled.values())
            else "stage_pr_waiting"
        )
        return {
            "status": status,
            "stage": bound_stage,
            "pr": {"number": facts.pr_number, "head_sha": facts.head_sha},
            "results": reconciled,
        }

    def assemble_stage(
        self,
        mission: contract.MaintenanceMission,
        stage: contract.Stage,
        cards: tuple[contract.WorkCard, ...],
        results: Mapping[str, ExecutionResult],
        *,
        base_sha: str,
    ) -> StageIntegration:
        """Assemble verified card commits into one parent-owned Stage branch.

        Card worktrees are exact-base and isolated.  This method is the only
        parent-side assembly seam: it rechecks each committed diff against its
        WorkCard before cherry-picking, refuses stale/ambiguous branches, and
        never gives the child worker a PR or GitHub write path.
        """

        if not SHA40.fullmatch(base_sha):
            raise StewardError("base_sha_invalid")
        try:
            contract.validate_current_mission(
                mission,
                repository=self.repository,
                base_sha=base_sha,
                branch=mission.repository_identity.branch,
                source_ref=mission.repository_identity.source_ref,
                source_sha256=mission.repository_identity.source_sha256,
                require_running=True,
            )
            contract.validate_execution_scope(
                mission,
                tuple(sorted({path for card in cards for path in card.allowed_paths})),
                ("draft_pr",),
            )
            contract.validate_stage(stage, mission, cards)
            contract.validate_execution_scope(
                mission,
                tuple(sorted({path for card in cards for path in card.allowed_paths})),
                ("read", "write", "test", "branch", "draft_pr"),
            )
        except contract.MissionContractError as exc:
            raise StewardError("stage_integration_scope_invalid") from exc
        if set(results) != {card.card_id for card in cards}:
            raise StewardError("stage_integration_results_incomplete")
        card_heads: list[tuple[str, str]] = []
        for card in cards:
            result = results[card.card_id]
            if result.status not in {"WAITING_FOR_PR", "COMPLETE"}:
                raise StewardError("stage_card_not_ready_for_integration")
            if not isinstance(result.head_sha, str) or not SHA40.fullmatch(result.head_sha):
                raise StewardError("stage_card_head_missing")
            card_heads.append((card.card_id, result.head_sha))

        branch = _stage_branch(mission, stage, base_sha)
        digest = branch.removeprefix("agent/stage-")
        projects_root: Path | None = None
        for parent in (self.repo_path, *self.repo_path.parents):
            if parent.name == "Projects":
                projects_root = parent
                break
        if projects_root is not None:
            if (
                self.repo_path.parent.parent.name == ".worktrees"
                and self.repo_path.parent.parent.parent == projects_root
            ):
                repository_name = self.repo_path.parent.name
            elif self.repo_path.parent == projects_root:
                repository_name = self.repo_path.name
            else:
                repository_name = self.repo_path.name
            worktree_root = projects_root / ".worktrees" / repository_name
        else:
            # Test and externally embedded checkouts may not have the shared
            # Projects root; keep their fallback local and bounded.
            worktree_root = self.repo_path.parent / ".worktrees" / self.repo_path.name
        stage_path = worktree_root / f"steward-stage-{digest}"
        worktree_root.mkdir(parents=True, exist_ok=True)
        if stage_path.exists() or stage_path.is_symlink():
            raise StewardError("stage_integration_path_occupied")
        existing = self._git_text("rev-parse", "--verify", f"refs/heads/{branch}", allow_failure=True)
        if existing is not None and existing != base_sha:
            raise StewardError("stage_integration_branch_stale")
        if existing is None:
            self._git_text("branch", branch, base_sha)
        self._git_text("worktree", "add", str(stage_path), branch)
        try:
            for card, (_card_id, head_sha) in zip(cards, card_heads):
                if self._git_text("merge-base", "--is-ancestor", base_sha, head_sha, cwd=stage_path, allow_failure=True) is None:
                    raise StewardError("stage_card_head_not_descendant")
                paths = _git_changed_paths(stage_path, base_sha, head_sha)
                workers.validate_changed_paths(card, paths)
                if head_sha == base_sha:
                    continue
                if self._git_text("cherry-pick", "--no-commit", head_sha, cwd=stage_path, allow_failure=True) is None:
                    self._git_text("cherry-pick", "--abort", cwd=stage_path, allow_failure=True)
                    raise StewardError("stage_integration_conflict")
            if not self._git_text("diff", "--cached", "--name-only", cwd=stage_path):
                raise StewardError("stage_integration_no_changes")
            self._git_text(
                "-c", "user.name=Autonomous Steward", "-c",
                "user.email=steward@localhost.invalid", "commit", "-m",
                f"feat: integrate Steward stage {stage.stage_id}", cwd=stage_path,
            )
            head_sha = self._git_text("rev-parse", "HEAD", cwd=stage_path)
            if not SHA40.fullmatch(head_sha):
                raise StewardError("stage_integration_head_invalid")
        finally:
            if stage_path.exists():
                self._git_text("worktree", "remove", str(stage_path), allow_failure=True)
        return StageIntegration(stage.stage_id, branch, base_sha, head_sha, tuple(card_heads))

    def _git_text(
        self, *args: str, cwd: Path | None = None, allow_failure: bool = False
    ) -> str | None:
        try:
            result = subprocess.run(
                ["git", *args], cwd=cwd or self.repo_path, capture_output=True,
                text=True, timeout=120, check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise StewardError("stage_git_unavailable") from exc
        if result.returncode != 0:
            if allow_failure:
                return None
            raise StewardError("stage_git_command_failed")
        return result.stdout.strip()

    def publish_stage_branch(self, integration: StageIntegration) -> None:
        """Push one exact Stage branch and reconcile the remote head."""

        if (
            not isinstance(integration, StageIntegration)
            or not re.fullmatch(r"agent/stage-[0-9a-f]{24}", integration.branch)
            or not SHA40.fullmatch(integration.base_sha)
            or not SHA40.fullmatch(integration.head_sha)
        ):
            raise StewardError("stage_integration_binding_invalid")
        local_head = self._git_text(
            "rev-parse", "--verify", f"refs/heads/{integration.branch}",
            allow_failure=True,
        )
        if local_head != integration.head_sha:
            raise StewardError("stage_local_branch_head_mismatch")

        def remote_matches(observed: str | None) -> bool:
            if not observed:
                return False
            parts = observed.split()
            return len(parts) == 2 and parts == [
                integration.head_sha,
                f"refs/heads/{integration.branch}",
            ]

        remote = self._git_text(
            "ls-remote", "origin", f"refs/heads/{integration.branch}", allow_failure=True
        )
        if remote:
            parts = remote.split()
            if len(parts) != 2 or parts[1] != f"refs/heads/{integration.branch}":
                raise StewardError("stage_remote_head_ambiguous")
            if parts[0] != integration.head_sha:
                raise StewardError("stage_remote_branch_not_reusable")
            return
        try:
            self._git_text(
                "push", "origin",
                f"refs/heads/{integration.branch}:refs/heads/{integration.branch}"
            )
        except StewardError as exc:
            observed = self._git_text(
                "ls-remote", "origin", f"refs/heads/{integration.branch}", allow_failure=True
            )
            if not remote_matches(observed):
                raise StewardError("stage_push_outcome_unknown") from exc
        observed = self._git_text("ls-remote", "origin", f"refs/heads/{integration.branch}")
        if not remote_matches(observed):
            raise StewardError("stage_remote_head_mismatch")

    def bind_stage_draft_pr(
        self,
        mission: contract.MaintenanceMission,
        stage: contract.Stage,
        cards: tuple[contract.WorkCard, ...],
        integration: StageIntegration,
        *,
        title: str,
        body: str,
    ) -> tuple[contract.Stage, dict[str, Any]]:
        """Create/update exactly one parent-owned Stage Draft PR."""

        try:
            contract.validate_current_mission(
                mission,
                repository=self.repository,
                base_sha=integration.base_sha,
                branch=mission.repository_identity.branch,
                source_ref=mission.repository_identity.source_ref,
                source_sha256=mission.repository_identity.source_sha256,
                require_running=True,
            )
            contract.validate_stage(stage, mission, cards)
            contract.validate_execution_scope(
                mission,
                tuple(sorted({path for card in cards for path in card.allowed_paths})),
                ("draft_pr",),
            )
        except contract.MissionContractError:
            raise StewardError("stage_pr_stage_invalid")
        if integration.stage_id != stage.stage_id or integration.base_sha != mission.repository_identity.base_sha:
            raise StewardError("stage_pr_integration_binding_invalid")
        if integration.branch != _stage_branch(mission, stage, integration.base_sha):
            raise StewardError("stage_pr_branch_binding_invalid")
        marker = {
            "subject_kind": "steward-stage",
            "stage_id": stage.stage_id,
            "mission_id": mission.mission_id,
            "base_sha": integration.base_sha,
            "branch": integration.branch,
        }
        pr_body = (
            f"<!-- agent-orchestrator-binding: {json.dumps(marker, sort_keys=True, separators=(',', ':'))} -->\n\n"
            f"{body.strip()}"
        )
        import pr_binding

        bound = pr_binding.create_or_update_stage_pr(
            stage.stage_id,
            mission.mission_id,
            integration.branch,
            integration.head_sha,
            integration.base_sha,
            title,
            pr_body,
            self.repository,
        )
        number = bound.get("number")
        if type(number) is not int:
            raise StewardError("stage_pr_number_invalid")
        return replace(stage, integration_pr=number, exact_head=integration.head_sha), bound

    def reconcile_stage_pr(
        self,
        mission: contract.MaintenanceMission,
        stage: contract.Stage,
        cards: tuple[contract.WorkCard, ...],
        *,
        stage_pr: steward_github.StagePRFacts | dict[str, Any],
    ) -> dict[str, ExecutionResult]:
        """Reconcile one Stage PR for every locally reviewed WorkCard."""

        facts = _stage_pr_facts(stage_pr)
        if facts is None or stage.integration_pr is None or stage.exact_head is None:
            raise StewardError("stage_pr_facts_required")
        try:
            contract.validate_current_mission(
                mission,
                repository=self.repository,
                base_sha=mission.repository_identity.base_sha,
                branch=mission.repository_identity.branch,
                source_ref=mission.repository_identity.source_ref,
                source_sha256=mission.repository_identity.source_sha256,
                require_running=True,
            )
            contract.validate_stage(
                stage,
                mission,
                cards,
                observed_integration_pr=facts.pr_number,
                observed_exact_head=facts.head_sha,
            )
            live = _stage_pr_facts(self.github.fetch_stage_pr(self.repository, facts.pr_number))
            if live is None:
                raise StewardError("stage_pr_live_facts_missing")
            status = steward_github.reconcile_stage_pr(
                live,
                repository=self.repository,
                pr_number=facts.pr_number,
                expected_base_sha=mission.repository_identity.base_sha,
                expected_head_sha=stage.exact_head,
                expected_base_branch=stage.repository_identity.branch,
                expected_head_branch=_stage_branch(
                    mission, stage, mission.repository_identity.base_sha
                ),
            )
        except (contract.MissionContractError, steward_github.GitHubFactsError) as exc:
            raise StewardError(str(exc)) from exc
        except steward_github.GitHubReadError as exc:
            return {
                card.card_id: ExecutionResult(
                    card.card_id, "WAITING", 0, stage.exact_head,
                    "github_facts_unavailable", None, facts.pr_number,
                )
                for card in cards
            }
        results: dict[str, ExecutionResult] = {}
        for card in cards:
            latest = self.journal.latest_for_card(
                card.card_id, mission_id=mission.mission_id, stage_id=stage.stage_id
            )
            attempt = latest.attempt if latest is not None else 1
            if status.outcome == "WAITING_FOR_MERGE":
                self._record(
                    event="STAGE_WAITING_FOR_MERGE",
                    key=_journal_key("stage-waiting", mission, stage, card, facts.pr_number, stage.exact_head),
                    mission=mission,
                    stage=stage,
                    card=card,
                    attempt=attempt,
                    state="WAITING_FOR_MERGE",
                    detail="exact_head_ci_and_review_pass",
                    data={"pr_number": facts.pr_number, "stage_head_sha": stage.exact_head},
                )
                results[card.card_id] = ExecutionResult(
                    card.card_id, "WAITING_FOR_MERGE", attempt, stage.exact_head,
                    status.reason, None, facts.pr_number,
                )
            elif status.outcome == "COMPLETE":
                self._record(
                    event="STAGE_MERGED_OBSERVED",
                    key=_journal_key("stage-complete", mission, stage, card, facts.pr_number, stage.exact_head),
                    mission=mission,
                    stage=stage,
                    card=card,
                    attempt=attempt,
                    state="COMPLETE",
                    detail="live_stage_pr_merged",
                    data={"pr_number": facts.pr_number, "stage_head_sha": stage.exact_head},
                )
                results[card.card_id] = ExecutionResult(
                    card.card_id, "COMPLETE", attempt, stage.exact_head,
                    status.reason, None, facts.pr_number,
                )
            else:
                results[card.card_id] = ExecutionResult(
                    card.card_id, "WAITING", attempt, stage.exact_head,
                    status.reason, None, facts.pr_number,
                )
        return results

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
                key=_journal_key("retry", mission, stage, card, attempt, _digest(safe_reason)),
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
            key=_journal_key("blocked", mission, stage, card, attempt, _digest(safe_reason)),
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
                and event.event in {
                    "REVIEW_FAILED",
                    "REVIEW_PASSED",
                    "LOCAL_REVIEW_OBSERVED",
                }
            ):
                previous = dict(event.data)
                break
        try:
            attempt = review_convergence.derive_next_review_attempt(previous, head_sha)
            if previous is not None:
                attempt["_previous_review_data"] = previous
            return attempt
        except (TypeError, ValueError, KeyError):
            return {
                "allowed": False,
                "deny_reason": "review_attempt_state_invalid",
                "review_mode": "full",
                "review_round": 1,
            }

    @staticmethod
    def _prior_review_state(data: Mapping[str, Any]) -> review_convergence.ReviewRoundState:
        """Rebuild the bounded convergence projection needed for R2."""

        return review_convergence.ReviewRoundState(
            review_protocol_version=review_convergence.REVIEW_PROTOCOL_VERSION,
            review_mode=str(data["review_mode"]),
            review_round=int(data["review_round"]),
            prior_reviewed_head=str(data.get("prior_reviewed_head", "")),
            reviewed_base=str(data["base_sha"]),
            reviewed_head=str(data["head_sha"]),
            reviewed_range=f"{data['base_sha']}...{data['head_sha']}",
            verdict=str(data["verdict"]),
            findings=(),
            finding_ledger_digest=str(data["finding_ledger_digest"]),
            open_blocker_ids=tuple(data.get("open_blocker_ids", ())),
            deferred_note_ids=tuple(data.get("deferred_note_ids", ())),
            decision_required_ids=tuple(data.get("decision_required_ids", ())),
            autonomous_repairs_remaining=int(data.get("autonomous_repairs_remaining", 0)),
            stop_reason=str(data.get("stop_reason", "")),
            summary="",
        )

    @staticmethod
    def _review_convergence_data(
        decision: review_convergence.ReviewDecision,
        *,
        base_sha: str,
        head_sha: str,
    ) -> dict[str, Any]:
        """Project only bounded canonical convergence facts into the journal."""

        return {
            "base_sha": base_sha,
            "head_sha": head_sha,
            "review_round": decision.review_round,
            "review_mode": decision.review_mode,
            "verdict": decision.verdict,
            "open_blocker_ids": list(decision.open_blocker_ids),
            "deferred_note_ids": list(decision.deferred_note_ids),
            "decision_required_ids": list(decision.decision_required_ids),
            "finding_ledger_digest": decision.finding_ledger_digest,
            "security_ok": decision.security_ok,
            "rollback_ok": decision.rollback_ok,
            "observed_ci_status": decision.observed_ci_status,
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
            validated_mission = contract.validate_current_mission(
                mission,
                repository=self.repository,
                base_sha=base_sha,
                branch=mission.repository_identity.branch,
                source_ref=mission.repository_identity.source_ref,
                source_sha256=mission.repository_identity.source_sha256,
                require_running=True,
            )
            contract.validate_execution_scope(
                validated_mission,
                card.allowed_paths,
                ("read", "write", "test", "branch", "review", "ci_repair"),
            )
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
                key=_journal_key("queue", mission, stage, card, attempt, base_sha),
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
                    expected_worktree_path, expected_worktree_branch = (
                        worktree_manager.steward_worktree_location(
                            mission.mission_id, stage.stage_id, card.card_id, base_sha
                        )
                    )
                    if (
                        worktree_path.name != expected_worktree_path.name
                        or worktree_branch != expected_worktree_branch
                        or created[2] != base_sha
                    ):
                        raise StewardError("worktree_binding_mismatch")
                    metadata_before = _git_metadata_snapshot(
                        worktree_path, branch=worktree_branch
                    )
                    self._record(
                        event="WORKER_STARTED",
                        key=_journal_key("start", mission, stage, card, attempt, base_sha),
                        mission=mission,
                        stage=stage,
                        card=card,
                        attempt=attempt,
                        state="RUNNING",
                        detail="isolated_worktree_bound",
                        data={
                            "base_sha": base_sha,
                            "worktree_binding_sha256": worktree_binding_sha256,
                            "branch": worktree_branch,
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
                        worktree_branch=worktree_branch,
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
                        integrity = "unavailable"
                        integrity_data: dict[str, Any] = {}
                        try:
                            failure_head = _git_head(worktree_path)
                            failure_clean = True
                            try:
                                _git_worktree_clean(worktree_path)
                            except (StewardError, workers.WorkerError):
                                failure_clean = False
                            failure_metadata = _git_metadata_snapshot(
                                worktree_path, branch=worktree_branch
                            )
                            metadata_unchanged = failure_metadata == (
                                metadata_before[0],
                                metadata_before[1],
                            )
                            integrity = (
                                "clean_unchanged"
                                if failure_clean and metadata_unchanged and failure_head == base_sha
                                else "changed_or_dirty"
                            )
                            integrity_data = {
                                "head_sha": failure_head,
                                "worktree_clean": failure_clean,
                                "metadata_unchanged": metadata_unchanged,
                            }
                        except (StewardError, workers.WorkerError, OSError):
                            pass
                        self._record(
                            event="WORKER_OUTCOME_UNKNOWN",
                            key=_journal_key("unknown-worker", mission, stage, card, attempt),
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            state="OUTCOME_UNKNOWN",
                            detail=f"worker_exception_after_admission_{integrity}",
                            data=integrity_data,
                        )
                        return ExecutionResult(
                            card.card_id,
                            "OUTCOME_UNKNOWN",
                            attempt,
                            None,
                            f"worker_exception_after_admission_{integrity}",
                        )
                    try:
                        observed_head = _git_head(worktree_path)
                        workers.validate_worker_outcome(
                            card, outcome, expected_head_sha=observed_head
                        )
                        actual_paths = _git_changed_paths(worktree_path, base_sha, observed_head)
                        workers.validate_changed_paths(card, actual_paths)
                        _git_worktree_clean(worktree_path)
                        expected_refs = dict(metadata_before[0])
                        expected_refs[f"refs/heads/{worktree_branch}"] = observed_head
                        if _git_metadata_snapshot(worktree_path, branch=worktree_branch) != (
                            expected_refs,
                            metadata_before[1],
                        ):
                            raise workers.WorkerError("worker_git_metadata_changed")
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
                            key=_journal_key("unknown-head", mission, stage, card, attempt),
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
                                key=_journal_key("unknown-reported", mission, stage, card, attempt),
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
                    # This is the restart checkpoint for the admitted worker
                    # result.  It binds the exact head, scope projection,
                    # worktree identity, and implementation session before
                    # any verification or review proceeds.
                    self._record(
                        event="WORKER_CHECKPOINT",
                        key=_journal_key("verify", mission, stage, card, attempt, observed_head),
                        mission=mission,
                        stage=stage,
                        card=card,
                        attempt=attempt,
                        state="VERIFYING",
                        detail="allowlisted_checks_only",
                        data={
                            "base_sha": base_sha,
                            "head_sha": observed_head,
                            "changed_paths_digest": _digest("\x00".join(actual_paths)),
                            "worktree_binding_sha256": worktree_binding_sha256,
                            "implementation_session_id": outcome.session_id,
                        },
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
                        key=_journal_key("checks-passed", mission, stage, card, attempt, observed_head),
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
                        repair_state = None
                        if review_attempt["review_round"] == 2:
                            previous_data = review_attempt.get("_previous_review_data")
                            if not isinstance(previous_data, Mapping):
                                raise workers.WorkerError(
                                    "R2 prior review state is missing"
                                )
                            repair_state = review_convergence.after_repair_batch_consumed(
                                self._prior_review_state(previous_data),
                                new_head_sha=observed_head,
                            )
                            persisted_repair = repair_state.to_persistence_fields()
                            persisted_repair.pop("findings", None)
                            self._record(
                                event="REVIEW_REPAIR_BATCH_CONSUMED",
                                key=_journal_key(
                                    "repair-consumed",
                                    mission,
                                    stage,
                                    card,
                                    attempt,
                                    observed_head,
                                ),
                                mission=mission,
                                stage=stage,
                                card=card,
                                attempt=attempt,
                                state="REVIEWING",
                                detail="review_repair_batch_consumed",
                                data=persisted_repair,
                            )
                        review = self.reviewer.review(context, outcome)
                        if not isinstance(review, workers.ReviewOutcome):
                            raise workers.WorkerError("review_adapter_return_invalid")
                        if review.implementation_session_id != outcome.session_id:
                            raise workers.WorkerError("review_implementation_session_mismatch")
                        review_decision = workers.canonical_review_decision(review)
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
                        if _git_metadata_snapshot(worktree_path, branch=worktree_branch) != (
                            expected_refs,
                            metadata_before[1],
                        ):
                            raise workers.WorkerError("review_git_metadata_changed")
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
                            key=_journal_key("unknown-review-head", mission, stage, card, attempt),
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
                    try:
                        expected_review_range = workers.review_range_digest(
                            base_sha, observed_head, worktree=worktree_path
                        )
                    except workers.WorkerError:
                        return self._failure(
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            reason="review_range_unavailable",
                            retryable=False,
                            head_sha=observed_head,
                        )
                    if (
                        review.reviewed_base_sha != base_sha
                        or review.reviewed_range_sha256 != expected_review_range
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
                    try:
                        if review_attempt["review_round"] == 1:
                            convergence_state = review_convergence.initial_r1_state(
                                review_decision
                            )
                        else:
                            previous_data = review_attempt.get("_previous_review_data")
                            if not isinstance(previous_data, Mapping):
                                raise review_convergence.ConvergenceError(
                                    "R2 prior review state is missing"
                                )
                            invalidated_state = repair_state
                            if invalidated_state is None:
                                raise review_convergence.ConvergenceError(
                                    "R2 repair transition is missing"
                                )
                            convergence_state = review_convergence.apply_r2_decision(
                                invalidated_state, review_decision
                            )
                    except (TypeError, ValueError, KeyError, review_convergence.ConvergenceError) as exc:
                        return self._failure(
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            reason="review_convergence_invalid",
                            retryable=False,
                            head_sha=observed_head,
                        )
                    if review.status != "PASS":
                        convergence_data = self._review_convergence_data(
                            review_decision, base_sha=base_sha, head_sha=observed_head
                        )
                        convergence_data.update(
                            {
                                "verdict": convergence_state.verdict,
                                "stop_reason": convergence_state.stop_reason,
                                "autonomous_repairs_remaining": convergence_state.autonomous_repairs_remaining,
                            }
                        )
                        self._record(
                            event="REVIEW_FAILED",
                            key=_journal_key("review-failed", mission, stage, card, attempt, observed_head),
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            state="REVIEWING",
                            detail="independent_review_not_passed",
                            data=convergence_data,
                        )
                        result = self._failure(
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            reason=review.detail or "independent_review_not_passed",
                            retryable=(
                                review.status in {"FAIL", "BLOCKED"}
                                and bool(review_decision.open_blocker_ids)
                            ),
                            head_sha=observed_head,
                        )
                        if result.status == "RETRY_SCHEDULED":
                            attempt += 1
                            continue
                        return result
                    convergence_data = self._review_convergence_data(
                        review_decision, base_sha=base_sha, head_sha=observed_head
                    )
                    convergence_data.pop("decision_required_ids", None)
                    convergence_data.update(
                        {
                            "implementation_session_id": outcome.session_id,
                            "reviewer_session_id": review.reviewer_session_id,
                            "reviewed_range_sha256": review.reviewed_range_sha256,
                            "review_axes": list(review.review_axes),
                            "review_receipt_sha256": review.review_receipt_sha256,
                        }
                    )
                    self._record(
                        event="LOCAL_REVIEW_OBSERVED",
                        key=_journal_key(
                            "review", mission, stage, card, attempt, observed_head,
                            _digest(review.reviewer_session_id),
                        ),
                        mission=mission,
                        stage=stage,
                        card=card,
                        attempt=attempt,
                        state="REVIEWING",
                        detail="local_review_observation_only",
                        data=convergence_data,
                    )
                    if stage_facts is None:
                        return ExecutionResult(card.card_id, "WAITING_FOR_PR", attempt, observed_head, "stage_pr_facts_required", review.reviewer_session_id)
                    try:
                        if (
                            stage_facts.repository.casefold() != self.repository.casefold()
                            or stage_facts.base_sha != base_sha
                            or stage_facts.head_sha != observed_head
                            or stage_facts.base_branch != stage.repository_identity.branch
                            or stage_facts.head_branch != worktree_branch
                        ):
                            raise steward_github.GitHubFactsError(
                                "stage_pr_binding_input_mismatch"
                            )
                        # Preserve a validated binding before the live read so
                        # a transient GitHub outage remains recoverable rather
                        # than turning the external identity into a missing
                        # binding.  The subsequent live read must still prove
                        # the same exact identity and gates.
                        self._record(
                            event="STAGE_PR_BOUND",
                            key=_journal_key(
                                "stage-bind", mission, stage, card,
                                stage_facts.pr_number, observed_head
                            ),
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            state="REVIEWING",
                            detail="stage_pr_binding_observed",
                            data={
                                "repository": stage_facts.repository,
                                "pr_number": stage_facts.pr_number,
                                "base_sha": stage_facts.base_sha,
                                "head_sha": stage_facts.head_sha,
                                "stage_id": stage.stage_id,
                                "base_branch": stage_facts.base_branch,
                                "head_branch": stage_facts.head_branch,
                            },
                        )
                    except steward_github.GitHubFactsError as exc:
                        return self._failure(
                            mission=mission,
                            stage=stage,
                            card=card,
                            attempt=attempt,
                            reason=str(exc),
                            retryable=False,
                            head_sha=observed_head,
                        )
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
                            key=_journal_key("github-read", mission, stage, card, observed_head),
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
                        key=_journal_key(
                            "stage-bind", mission, stage, card, status.pr_number, observed_head
                        ),
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
                            key=_journal_key(
                                "waiting", mission, stage, card, observed_head, status.pr_number
                            ),
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
                            key=_journal_key(
                                "complete", mission, stage, card, observed_head, status.pr_number
                            ),
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
                        key=_journal_key(
                            "gates", mission, stage, card, observed_head, _digest(status.reason)
                        ),
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
                        if any(
                            item.status not in {"COMPLETE", "WAITING_FOR_PR"}
                            for item in dependency_results
                            if item is not None
                        ):
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


__all__ = [
    "ExecutionResult",
    "MAX_CONCURRENCY",
    "StageIntegration",
    "Steward",
    "StewardError",
]
