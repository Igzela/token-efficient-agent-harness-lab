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
import tempfile
from typing import Any, Callable, Mapping, Protocol
import uuid

import local_verification
import mission_contract
import review_convergence
from review_loop.locking import ChatLock, LockBusy
import state_manager


SHA40 = re.compile(r"^[0-9a-f]{40}$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")
MAX_REVIEW_DIFF_BYTES = 8 * 1024 * 1024
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
_NETWORK_ENVIRONMENT_KEYS = frozenset(
    {
        "CODEX_HOME",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "no_proxy",
        "XDG_CONFIG_HOME",
        "XDG_CACHE_HOME",
        "XDG_RUNTIME_DIR",
    }
)
_GIT_ENVIRONMENT_PREFIX = "GIT_"
_SAFE_EXECUTABLES = frozenset({"python", "python3", "git"})
_SAFE_ABSOLUTE_EXECUTABLES = frozenset({"/usr/bin/python3", "/usr/bin/git"})
_GIT_FORBIDDEN_ARGUMENTS = frozenset(
    {
        "clone",
        "fetch",
        "merge",
        "pull",
        "push",
        "remote",
        "reset",
        "clean",
        "worktree",
        "submodule",
    }
)
_GIT_ALLOWED_COMMANDS = frozenset(
    {"add", "commit", "diff", "log", "ls-files", "rev-parse", "show", "status"}
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
    # Steward children are repository-maintenance workers, not provider or
    # operator sessions.  Remove the reusable local configuration and egress
    # selectors that the broader local-run owner may preserve for other lanes.
    for key in _NETWORK_ENVIRONMENT_KEYS:
        environment.pop(key, None)
    for key in tuple(environment):
        if key.startswith(_GIT_ENVIRONMENT_PREFIX):
            environment.pop(key, None)
    environment["HOME"] = "/nonexistent"
    environment["PATH"] = "/usr/bin:/bin"
    return environment


def select_model_tier(base_tier: str, attempt: int) -> str:
    """Escalate only within T0-T2 as bounded retry pressure increases."""

    if base_tier not in {"T0", "T1", "T2"} or type(attempt) is not int or attempt < 1:
        raise WorkerError("model_tier_invalid")
    tiers = ("T0", "T1", "T2")
    index = min(2, tiers.index(base_tier) + attempt - 1)
    return tiers[index]


def review_range_digest(
    base_sha: str, head_sha: str, *, worktree: Path | None = None
) -> str:
    """Digest the exact reviewed range, including its complete Git diff."""

    if not SHA40.fullmatch(base_sha) or not SHA40.fullmatch(head_sha):
        raise WorkerError("review_range_invalid")
    if worktree is not None:
        try:
            result = subprocess.run(
                ["git", "diff", "--binary", "--no-ext-diff", f"{base_sha}...{head_sha}"],
                cwd=worktree,
                capture_output=True,
                timeout=30,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise WorkerError("review_range_unavailable") from exc
        if result.returncode != 0 or len(result.stdout) > MAX_REVIEW_DIFF_BYTES:
            raise WorkerError("review_range_unavailable")
        return hashlib.sha256(result.stdout).hexdigest()
    # The no-worktree form is retained for typed receipt construction and
    # deterministic tests; the coordinator supplies the live worktree.
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
    worktree_branch: str = ""

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
    summary: str = "bounded independent review"
    findings: tuple[dict[str, Any], ...] | None = None
    security_ok: bool = True
    rollback_ok: bool = True
    observed_ci_status: str = "unknown"
    finding_ledger_digest: str = ""

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
        _safe_detail(self.summary, "review_summary")
        _safe_detail(self.observed_ci_status, "review_observed_ci_status")
        if type(self.security_ok) is not bool or type(self.rollback_ok) is not bool:
            raise WorkerError("review_gate_flags_invalid")
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
        try:
            decision = canonical_review_decision(self)
        except (TypeError, ValueError, review_convergence.ConvergenceError) as exc:
            raise WorkerError("review_convergence_invalid") from exc
        if self.finding_ledger_digest and self.finding_ledger_digest != decision.finding_ledger_digest:
            raise WorkerError("review_finding_ledger_mismatch")
        object.__setattr__(self, "finding_ledger_digest", decision.finding_ledger_digest)
        if self.review_receipt_sha256 != review_receipt_digest(self):
            raise WorkerError("review_receipt_digest_mismatch")

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
            "summary": self.summary,
            "findings": None if self.findings is None else [dict(item) for item in self.findings],
            "security_ok": self.security_ok,
            "rollback_ok": self.rollback_ok,
            "observed_ci_status": self.observed_ci_status,
            "finding_ledger_digest": self.finding_ledger_digest,
        }

    @classmethod
    def from_wire(cls, value: object) -> "ReviewOutcome":
        if not isinstance(value, dict) or set(value) != {
            "schema_version", "status", "reviewer_session_id", "implementation_session_id",
            "reviewed_head_sha", "blockers", "detail", "reviewed_base_sha",
            "reviewed_range_sha256", "review_axes", "review_round", "review_mode",
            "review_receipt_sha256", "summary", "findings", "security_ok", "rollback_ok",
            "observed_ci_status", "finding_ledger_digest",
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
        findings = value.get("findings")
        if findings is not None and (
            not isinstance(findings, list)
            or not all(isinstance(item, dict) for item in findings)
        ):
            raise WorkerError("review_outcome_wire_invalid")
        return cls(
            value["status"], value["reviewer_session_id"], value["implementation_session_id"],
            value["reviewed_head_sha"], tuple(blockers), value["detail"], value["reviewed_base_sha"],
            value["reviewed_range_sha256"], tuple(axes), value["review_round"], value["review_mode"],
            value["review_receipt_sha256"], value["summary"],
            None if findings is None else tuple(dict(item) for item in findings),
            value["security_ok"], value["rollback_ok"], value["observed_ci_status"],
            value["finding_ledger_digest"],
        )


def _review_artifact(value: ReviewOutcome | Mapping[str, Any]) -> dict[str, Any]:
    """Build the bounded input accepted by the canonical convergence owner."""

    if isinstance(value, ReviewOutcome):
        status = value.status
        blockers = value.blockers
        findings = value.findings
        summary = value.summary
        head = value.reviewed_head_sha
        base = value.reviewed_base_sha
        mode = value.review_mode
        review_round = value.review_round
        security_ok = value.security_ok
        rollback_ok = value.rollback_ok
        observed_ci_status = value.observed_ci_status
    else:
        status = value["status"]
        blockers = tuple(value.get("blockers", ()))
        findings = value.get("findings")
        summary = value["summary"]
        head = value["reviewed_head_sha"]
        base = value["reviewed_base_sha"]
        mode = value["review_mode"]
        review_round = value["review_round"]
        security_ok = value["security_ok"]
        rollback_ok = value["rollback_ok"]
        observed_ci_status = value["observed_ci_status"]
    if findings is None:
        artifact: dict[str, Any] = {
            "blockers": list(blockers),
            "summary": summary,
        }
    else:
        artifact = {
            "findings": [dict(item) for item in findings],
            "summary": summary,
        }
    artifact.update(
        {
            "verdict": status,
            "reviewed_head_sha": head,
            "reviewed_base": base,
            "reviewed_range": f"{base}...{head}",
            "review_mode": mode,
            "review_round": review_round,
            "security_ok": security_ok,
            "rollback_ok": rollback_ok,
            "observed_ci_status": observed_ci_status,
        }
    )
    return artifact


def canonical_review_decision(
    outcome: ReviewOutcome | Mapping[str, Any],
) -> review_convergence.ReviewDecision:
    """Normalize the bounded outcome through the canonical R1/R2 owner."""

    artifact = _review_artifact(outcome)
    decision = review_convergence.decision_from_legacy_artifact(
        artifact,
        base_sha=artifact["reviewed_base"],
        review_mode=artifact["review_mode"],
        review_round=artifact["review_round"],
    )
    if decision.reviewed_range != artifact["reviewed_range"]:
        raise review_convergence.ConvergenceError(
            "reviewed range is not the complete base...head range"
        )
    return decision


def review_receipt_digest(outcome: ReviewOutcome | Mapping[str, Any]) -> str:
    """Seal every bounded review identity field except detail and the digest."""

    payload = dict(outcome.to_wire() if isinstance(outcome, ReviewOutcome) else outcome)
    # Detail is an operator-facing bounded note, not acceptance evidence.  It
    # is intentionally excluded so restart recovery can revalidate the same
    # receipt without persisting raw reviewer prose.
    payload.pop("detail", None)
    # Finding evidence is also transient.  The canonical ledger digest and
    # bounded blocker/deferred projections remain sealed in the receipt.
    payload.pop("findings", None)
    payload.pop("summary", None)
    payload["review_receipt_sha256"] = ""
    return hashlib.sha256(
        json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()


def seal_review_outcome_wire(value: Mapping[str, Any]) -> dict[str, Any]:
    """Return a wire outcome with its self-contained receipt digest sealed."""

    payload = dict(value)
    payload["finding_ledger_digest"] = canonical_review_decision(payload).finding_ledger_digest
    payload["review_receipt_sha256"] = ""
    payload["review_receipt_sha256"] = review_receipt_digest(payload)
    return payload


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


PR4B_CANARY_PROPOSAL_SHA256 = (
    "3a55ac107a2cae2a049e37804ea851036849c37aa84f95138db7d7f611db7eae"
)


def pr4b_canary_worker() -> BoundedProcessWorker:
    """Build the fixed provider-free worker for the approved PR4B canary."""

    def command(context: WorkerContext) -> list[str]:
        if context.allowed_paths != ("docs/CURRENT_STATUS.md",):
            raise WorkerUnavailable("pr4b_canary_scope_not_supported")
        return [
            "/usr/bin/python3",
            "scripts/agent-control/steward_canary.py",
            "worker",
            context.card_id,
            str(context.attempt),
            process_session_id(context),
        ]

    return BoundedProcessWorker(command, timeout_seconds=300)


def pr4b_canary_reviewer() -> BoundedProcessReviewer:
    """Build the separate read-only reviewer for the approved PR4B canary."""

    def command(context: WorkerContext, outcome: WorkerOutcome) -> list[str]:
        if context.allowed_paths != ("docs/CURRENT_STATUS.md",):
            raise WorkerUnavailable("pr4b_canary_review_scope_not_supported")
        return [
            "/usr/bin/python3",
            "scripts/agent-control/steward_canary.py",
            "review",
            context.card_id,
            context.base_sha,
            outcome.head_sha,
            outcome.session_id,
            reviewer_session_id(context, outcome),
        ]

    return BoundedProcessReviewer(command, timeout_seconds=300)


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


def process_session_id(context: WorkerContext) -> str:
    """Derive a parent-bound implementation session identity."""

    return f"steward-process:{context.card_id}:{context.attempt}"


def reviewer_session_id(context: WorkerContext, outcome: WorkerOutcome) -> str:
    """Derive an independent reviewer identity from parent-owned bindings."""

    material = "\x00".join(
        (context.mission_id, context.stage_id, context.card_id, str(context.attempt), outcome.session_id)
    )
    digest = hashlib.sha256(material.encode("utf-8")).hexdigest()[:32]
    return str(uuid.UUID(hex=digest))


def _sandbox_for_context(context: WorkerContext) -> _SandboxGit | None:
    """Prepare a private Git view only for a real linked worktree."""

    branch = context.worktree_branch
    if not branch:
        try:
            result = subprocess.run(
                ["/usr/bin/git", "branch", "--show-current"],
                cwd=context.worktree,
                capture_output=True,
                text=True,
                timeout=30,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise WorkerError("sandbox_branch_unavailable") from exc
        branch = result.stdout.strip() if result.returncode == 0 else ""
    return _SandboxGit.create(context.worktree, base_sha=context.base_sha, branch=branch)


def _validate_command(command: object) -> list[str]:
    if (
        not isinstance(command, (list, tuple))
        or not command
        or len(command) > 32
        or not all(isinstance(item, str) and item and len(item) <= 4096 and "\x00" not in item for item in command)
    ):
        raise WorkerError("worker_command_invalid")
    argv = list(command)
    executable = Path(argv[0]).name
    if argv[0] not in _SAFE_ABSOLUTE_EXECUTABLES and (
        Path(argv[0]).is_absolute() or argv[0] not in _SAFE_EXECUTABLES
    ):
        raise WorkerError("worker_executable_not_allowlisted")
    if executable == "git":
        arguments = [item.casefold() for item in argv[1:]]
        if any(item in _GIT_FORBIDDEN_ARGUMENTS for item in arguments):
            raise WorkerError("worker_git_effect_forbidden")
        subcommands = [item for item in arguments if not item.startswith("-")]
        if not subcommands or subcommands[0] not in _GIT_ALLOWED_COMMANDS:
            raise WorkerError("worker_git_command_not_allowlisted")
        if any(item in {"-c", "--config-env", "--config"} for item in arguments):
            raise WorkerError("worker_git_config_forbidden")
    return argv


@dataclass
class _SandboxGit:
    """Disposable private Git dir used by one child process."""

    temporary: tempfile.TemporaryDirectory[str]
    worktree: Path
    git_dir: Path
    common_dir: Path
    marker_copy: Path
    branch: str
    guest_git_dir: Path = Path("/steward-sandbox/git")

    def cleanup(self) -> None:
        self.temporary.cleanup()

    @classmethod
    def create(
        cls, worktree: Path, *, base_sha: str, branch: str
    ) -> "_SandboxGit | None":
        marker = worktree / ".git"
        if not marker.is_file() or marker.is_symlink():
            return None
        gitdir, common = _linked_git_metadata(worktree)
        if gitdir is None or common is None:
            raise WorkerError("sandbox_git_metadata_invalid")
        if (
            not branch
            or not branch.startswith("agent/")
            or ".." in Path(branch).parts
            or "\\" in branch
        ):
            raise WorkerError("sandbox_branch_invalid")
        temporary = tempfile.TemporaryDirectory(prefix="steward-git-")
        try:
            clone_path = Path(temporary.name) / "repo"
            result = subprocess.run(
                [
                    "/usr/bin/git", "clone", "--shared", "--no-checkout", "--no-tags",
                    str(worktree), str(clone_path),
                ],
                capture_output=True,
                timeout=60,
                check=False,
            )
            if result.returncode != 0:
                raise WorkerError("sandbox_git_clone_failed")
            sandbox_git = clone_path / ".git"
            ref = f"refs/heads/{branch}"
            for command in (
                ["config", "--remove-section", "remote.origin"],
                ["update-ref", ref, base_sha],
                ["symbolic-ref", "HEAD", ref],
                ["config", "core.hooksPath", "/dev/null"],
                ["config", "user.name", "Steward Worker"],
                ["config", "user.email", "steward-worker@localhost.invalid"],
                ["read-tree", base_sha],
            ):
                result = subprocess.run(
                    ["/usr/bin/git", "--git-dir", str(sandbox_git), *command],
                    capture_output=True,
                    timeout=30,
                    check=False,
                )
                if result.returncode != 0:
                    raise WorkerError("sandbox_git_init_failed")
            marker_copy = Path(temporary.name) / "git-marker"
            marker_copy.write_bytes(marker.read_bytes())
            return cls(temporary, worktree.resolve(), sandbox_git, common, marker_copy, branch)
        except Exception:
            temporary.cleanup()
            raise

    def import_head(self, *, base_sha: str, head_sha: str, branch: str) -> None:
        """Import only the child branch tip and its reachable objects."""

        if head_sha == base_sha:
            return
        ref = f"refs/heads/{branch}"
        import_ref = f"refs/steward-import/{head_sha}"
        try:
            for command in (
                [
                    "/usr/bin/git", "fetch", "--no-tags", "--no-write-fetch-head",
                    str(self.git_dir), f"{ref}:{import_ref}",
                ],
                ["/usr/bin/git", "update-ref", ref, head_sha, base_sha],
            ):
                result = subprocess.run(
                    command,
                    cwd=self.worktree,
                    capture_output=True,
                    timeout=60,
                    check=False,
                )
                if result.returncode != 0:
                    raise WorkerError("sandbox_git_import_failed")
            result = subprocess.run(
                ["/usr/bin/git", "update-ref", "-d", import_ref, head_sha],
                cwd=self.worktree,
                capture_output=True,
                timeout=30,
                check=False,
            )
            if result.returncode != 0:
                raise WorkerError("sandbox_git_import_cleanup_failed")
            result = subprocess.run(
                ["/usr/bin/git", "read-tree", "--reset", head_sha],
                cwd=self.worktree,
                capture_output=True,
                timeout=30,
                check=False,
            )
            if result.returncode != 0:
                raise WorkerError("sandbox_git_index_sync_failed")
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise WorkerError("sandbox_git_import_unavailable") from exc


def _linked_git_metadata(worktree: Path) -> tuple[Path | None, Path | None]:
    """Resolve linked-worktree metadata without exposing it as writable child state."""

    marker = worktree / ".git"
    if not marker.is_file() or marker.is_symlink():
        return None, None
    try:
        line = marker.read_text(encoding="utf-8").strip()
        if not line.startswith("gitdir: "):
            return None, None
        gitdir = Path(line[8:])
        gitdir = (marker.parent / gitdir).resolve() if not gitdir.is_absolute() else gitdir.resolve()
        commondir = gitdir / "commondir"
        if not gitdir.is_dir() or not commondir.is_file() or commondir.is_symlink():
            return None, None
        common = Path(commondir.read_text(encoding="utf-8").strip())
        common = (gitdir / common).resolve() if not common.is_absolute() else common.resolve()
        return (gitdir, common) if common.is_dir() else (None, None)
    except (OSError, UnicodeError):
        return None, None


def _sandbox_command(
    command: list[str],
    worktree: Path,
    environment: Mapping[str, str],
    *,
    git_sandbox: _SandboxGit | None = None,
    worktree_writable: bool = True,
) -> list[str]:
    """Run a bounded child with explicitly scoped worktree and Git access."""

    bubblewrap = Path("/usr/bin/bwrap")
    if not bubblewrap.is_file():
        raise WorkerError("sandbox_unavailable")
    args = [
        str(bubblewrap),
        "--die-with-parent",
        "--unshare-net",
        "--unshare-pid",
        "--clearenv",
        "--tmpfs",
        "/",
    ]
    for system_path in ("/usr", "/bin", "/lib", "/lib64"):
        if Path(system_path).exists():
            args.extend(("--ro-bind", system_path, system_path))
    # A child needs loader/account metadata, but must not receive a readable
    # copy of the host's complete /etc (which can contain credentials or
    # operator configuration).  Network files and package configuration stay
    # outside the namespace; the child has no network and GIT_CONFIG_NOSYSTEM.
    args.extend(("--dir", "/etc"))
    for system_file in (
        "/etc/ld.so.cache",
        "/etc/nsswitch.conf",
        "/etc/passwd",
        "/etc/group",
        "/etc/localtime",
    ):
        if Path(system_file).is_file():
            args.extend(("--ro-bind", system_file, system_file))
    args.extend(("--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp"))
    created_dirs: set[str] = set()

    def add_parent_dirs(destination: Path) -> None:
        for parent in reversed(destination.parents):
            if parent == Path("/"):
                break
            parent_text = str(parent)
            if parent_text not in created_dirs:
                args.extend(("--dir", parent_text))
                created_dirs.add(parent_text)
    worktree = worktree.resolve()
    add_parent_dirs(worktree)
    args.extend(
        (
            "--bind" if worktree_writable else "--ro-bind",
            str(worktree),
            str(worktree),
        )
    )
    if git_sandbox is not None:
        add_parent_dirs(git_sandbox.common_dir)
        args.extend(("--ro-bind", str(git_sandbox.common_dir), str(git_sandbox.common_dir)))
        add_parent_dirs(git_sandbox.guest_git_dir)
        args.extend(("--bind", str(git_sandbox.git_dir.parent), str(git_sandbox.guest_git_dir)))
        args.extend(("--ro-bind", str(git_sandbox.marker_copy), str(worktree / ".git")))
        environment = dict(environment)
        environment["GIT_DIR"] = str(git_sandbox.guest_git_dir / ".git")
        environment["GIT_WORK_TREE"] = str(worktree)
        environment["GIT_CONFIG_NOSYSTEM"] = "1"
        environment["GIT_TERMINAL_PROMPT"] = "0"
    args.extend(("--chdir", str(worktree)))
    for key, value in sorted(environment.items()):
        if "\x00" in key or "\x00" in value or "\n" in key or "\n" in value:
            raise WorkerError("sandbox_environment_invalid")
        args.extend(("--setenv", key, value))
    return [*args, "--", *command]


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
        session_id = process_session_id(context)
        git_sandbox = _sandbox_for_context(context)
        try:
            if git_sandbox is None:
                sandboxed_command = _sandbox_command(
                    command,
                    context.worktree,
                    context.environment,
                )
                exit_code, stdout, _stderr = local_run_once._bounded_process(
                    sandboxed_command,
                    cwd=context.worktree,
                    timeout_seconds=self.timeout_seconds,
                    env=dict(context.environment),
                )
            else:
                sandboxed_command = _sandbox_command(
                    command,
                    context.worktree,
                    context.environment,
                    git_sandbox=git_sandbox,
                )
                child_environment = dict(context.environment)
                child_environment["GIT_DIR"] = str(git_sandbox.guest_git_dir / ".git")
                child_environment["GIT_WORK_TREE"] = str(context.worktree.resolve())
                child_environment["GIT_CONFIG_NOSYSTEM"] = "1"
                child_environment["GIT_TERMINAL_PROMPT"] = "0"
                exit_code, stdout, _stderr = local_run_once._bounded_process(
                    sandboxed_command,
                    cwd=context.worktree,
                    timeout_seconds=self.timeout_seconds,
                    env=child_environment,
                )
            if exit_code == 124:
                return WorkerOutcome("TIMEOUT", session_id, _head_or_base(context), (), "worker_timeout")
            if exit_code != 0:
                return WorkerOutcome("FAIL", session_id, _head_or_base(context), (), "worker_process_failed")
            try:
                payload = json.loads(stdout)
                outcome = WorkerOutcome.from_wire(payload)
            except (TypeError, ValueError, json.JSONDecodeError, WorkerError) as exc:
                raise WorkerError("worker_output_invalid") from exc
            if outcome.session_id != session_id:
                raise WorkerError("worker_session_binding_mismatch")
            if git_sandbox is not None and outcome.status == "PASS":
                git_sandbox.import_head(
                    base_sha=context.base_sha,
                    head_sha=outcome.head_sha,
                    branch=git_sandbox.branch,
                )
            return outcome
        except WorkerError:
            raise
        except Exception as exc:
            raise WorkerError("worker_process_unavailable") from exc
        finally:
            if git_sandbox is not None:
                git_sandbox.cleanup()


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
        git_sandbox = _sandbox_for_context(context)
        try:
            if git_sandbox is None:
                sandboxed_command = _sandbox_command(
                    command,
                    context.worktree,
                    context.environment,
                    worktree_writable=False,
                )
                exit_code, stdout, _stderr = local_run_once._bounded_process(
                    sandboxed_command,
                    cwd=context.worktree,
                    timeout_seconds=self.timeout_seconds,
                    env=dict(context.environment),
                )
            else:
                sandboxed_command = _sandbox_command(
                    command,
                    context.worktree,
                    context.environment,
                    git_sandbox=git_sandbox,
                    worktree_writable=False,
                )
                child_environment = dict(context.environment)
                child_environment["GIT_DIR"] = str(git_sandbox.guest_git_dir / ".git")
                child_environment["GIT_WORK_TREE"] = str(context.worktree.resolve())
                child_environment["GIT_CONFIG_NOSYSTEM"] = "1"
                child_environment["GIT_TERMINAL_PROMPT"] = "0"
                exit_code, stdout, _stderr = local_run_once._bounded_process(
                    sandboxed_command,
                    cwd=context.worktree,
                    timeout_seconds=self.timeout_seconds,
                    env=child_environment,
                )
            if exit_code != 0:
                raise WorkerError("review_process_failed")
            try:
                review = ReviewOutcome.from_wire(json.loads(stdout))
            except (TypeError, ValueError, json.JSONDecodeError, WorkerError) as exc:
                raise WorkerError("review_output_invalid") from exc
            expected_session = reviewer_session_id(context, outcome)
            if review.reviewer_session_id != expected_session:
                raise WorkerError("reviewer_session_binding_mismatch")
            if review.implementation_session_id != outcome.session_id:
                raise WorkerError("review_implementation_session_mismatch")
            try:
                expected_range = review_range_digest(
                    context.base_sha,
                    outcome.head_sha,
                    worktree=context.worktree,
                )
            except WorkerError:
                raise
            if review.reviewed_base_sha != context.base_sha or review.reviewed_head_sha != outcome.head_sha:
                raise WorkerError("review_head_binding_mismatch")
            if review.reviewed_range_sha256 != expected_range:
                raise WorkerError("review_range_binding_mismatch")
            return review
        except WorkerError:
            raise
        except Exception as exc:
            raise WorkerError("review_process_unavailable") from exc
        finally:
            if git_sandbox is not None:
                git_sandbox.cleanup()


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
    "CapacityLock",
    "PathConflict",
    "PathLockSet",
    "ProviderFreeWorker",
    "PR4B_CANARY_PROPOSAL_SHA256",
    "pr4b_canary_reviewer",
    "pr4b_canary_worker",
    "ReviewOutcome",
    "review_receipt_digest",
    "seal_review_outcome_wire",
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
