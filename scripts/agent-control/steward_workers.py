"""Bounded WorkCard worker and reviewer adapters for the Steward.

The production adapter invokes the authenticated local Codex CLI inside a
filesystem-scoped child. Provider-free and marker adapters
remain test-only fixtures. The parent owns WorkCard binding, exact-base
identity, path validation, and review-session separation. Child environments
are derived from the existing fail-closed local-run owner.
"""

from __future__ import annotations

from contextlib import AbstractContextManager
from dataclasses import dataclass
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import tempfile
import shutil
from typing import Any, Callable, Mapping, Protocol
import uuid

import local_verification
import mission_contract
import review_convergence
from review_loop.locking import ChatLock, LockBusy

MAX_ACTIVE_WORKERS = 2
BWRAP_PATH = Path("/usr/bin/bwrap")
SERVICE_CODEX_BINARY = Path("/usr/local/libexec/agent-steward/codex")
SYSTEMD_CODEX_CREDENTIAL_NAME = "codex-auth"


SHA40 = re.compile(r"^[0-9a-f]{40}$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")
CODEX_MODEL_ID = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$")
MAX_REVIEW_DIFF_BYTES = 8 * 1024 * 1024
SESSION_ID = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$")
SAFE_STATUSES = frozenset({"PASS", "FAIL", "TIMEOUT", "BLOCKED", "OUTCOME_UNKNOWN"})
REVIEW_STATUSES = frozenset({"PASS", "FAIL", "BLOCKED", "OUTCOME_UNKNOWN"})
REVIEW_AXES = frozenset({"standards", "spec"})
REVIEW_MODES = frozenset({"full", "repair_verification"})
_WORKCARD_GATE_NAMES = frozenset(
    {
        "focused_checks_required",
        "full_checks_required",
        "exact_head_review_required",
        "k2_scheduler_observed",
    }
)
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


_LOOPBACK_PROXY = re.compile(
    r"^(?P<scheme>https?|socks5h?)://"
    r"(?P<host>127(?:\.\d{1,3}){3}|\[::1\]):(?P<port>\d{1,5})/?$"
)


def _safe_loopback_proxy(value: str) -> bool:
    """Accept only an unauthenticated proxy on a literal loopback address."""

    if not isinstance(value, str) or len(value) > 256:
        return False
    match = _LOOPBACK_PROXY.fullmatch(value)
    if match is None:
        return False
    host = match.group("host")
    if host != "[::1]" and any(int(octet) > 255 for octet in host.split(".")):
        return False
    port = int(match.group("port"))
    return 1 <= port <= 65535


def child_environment(
    base: Mapping[str, str] | None = None, *, preserve_home: bool = False
) -> dict[str, str]:
    """Return the existing repository-owned credential-free child environment."""

    source = None if base is None else dict(base)
    environment = local_verification._child_env(source)
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
    # Production Codex needs the already-authenticated local login selected by
    # the operator. The wrapper still receives only this filtered mapping
    # and applies its own explicit allowlist; fixture/sandbox callers retain
    # the historical credential-free /nonexistent home by default.
    if preserve_home:
        candidate_home = (source or {}).get("HOME") or os.environ.get("HOME")
        if not candidate_home or Path(candidate_home).is_absolute() is False:
            raise WorkerError("authenticated_home_invalid")
        environment["HOME"] = candidate_home
    else:
        environment["HOME"] = "/nonexistent"
    if preserve_home:
        # Model selection is an optional operator setting, never a free-form
        # command fragment. Invalid values are rejected before they reach the
        # provider CLI; an absent value deliberately selects Codex's account
        # default.
        model = (source or {}).get("AGENT_CODEX_MODEL")
        if model is not None:
            if not isinstance(model, str) or CODEX_MODEL_ID.fullmatch(model) is None:
                raise WorkerError("codex_model_invalid")
            environment["AGENT_CODEX_MODEL"] = model
    if preserve_home:
        # Codex uses the host's local egress proxy in this environment. Permit
        # only literal-loopback, unauthenticated proxy endpoints; never pass
        # a remote proxy or a value containing embedded credentials.
        proxy_source = source or dict(os.environ)
        for key in ("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY"):
            value = proxy_source.get(key)
            if _safe_loopback_proxy(value):
                environment[key] = value
    path_entries = ["/usr/bin", "/bin"]
    if preserve_home:
        # The accepted local Codex CLI is installed in the operator's
        # authenticated home, not in the system image. Add only its known
        # installation directory; no caller-controlled PATH entries are
        # forwarded to the child.
        codex_dir = Path(environment["HOME"]) / ".local" / "bin"
        if (codex_dir / "codex").is_file() and os.access(codex_dir / "codex", os.X_OK):
            path_entries.append(str(codex_dir))
    environment["PATH"] = ":".join(path_entries)
    return environment


def _systemd_codex_auth_source() -> Path | None:
    """Resolve the manager-provided auth credential without forwarding its path."""

    raw_directory = os.environ.get("CREDENTIALS_DIRECTORY", "")
    if not raw_directory:
        return None
    directory = Path(raw_directory)
    if not directory.is_absolute():
        return None
    try:
        directory_metadata = os.lstat(directory)
    except OSError:
        return None
    if (
        not stat.S_ISDIR(directory_metadata.st_mode)
        or directory_metadata.st_mode & 0o022
    ):
        return None
    source = directory / SYSTEMD_CODEX_CREDENTIAL_NAME
    try:
        source_metadata = os.lstat(source)
    except OSError:
        return None
    if (
        not stat.S_ISREG(source_metadata.st_mode)
        or source_metadata.st_mode & 0o077
        or source_metadata.st_size < 2
        or source_metadata.st_size > 64 * 1024
    ):
        return None
    try:
        payload = json.loads(source.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError):
        return None
    if not isinstance(payload, dict):
        return None
    api_key = payload.get("OPENAI_API_KEY")
    tokens = payload.get("tokens")
    access_token = tokens.get("access_token") if isinstance(tokens, dict) else None
    if not (
        isinstance(api_key, str)
        and bool(api_key.strip())
        or isinstance(access_token, str)
        and bool(access_token.strip())
    ):
        return None
    return source


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
    forbidden_paths: tuple[str, ...] = ()
    max_attempts: int = 1
    objective: str = ""

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
        if type(self.max_attempts) is not int or self.max_attempts < self.attempt:
            raise WorkerError("worker_attempt_budget_invalid")
        if not isinstance(self.objective, str) or len(self.objective) > 8 * 1024:
            raise WorkerError("worker_objective_invalid")
        for path in (*self.allowed_paths, *self.forbidden_paths):
            _safe_path(path)
        if any(
            not isinstance(key, str)
            or any(marker in key.upper() for marker in _CREDENTIAL_MARKERS)
            for key in self.environment
        ):
            raise WorkerError("credential_shaped_child_environment")
        # Production Codex workers retain the operator's authenticated HOME so
        # the local CLI can resolve its login. Re-apply the
        # same filtering policy while preserving that explicitly bounded HOME;
        # fixture/sandbox contexts continue to use /nonexistent.
        preserve_home = self.environment.get("HOME") not in (None, "/nonexistent")
        sanitized = child_environment(dict(self.environment), preserve_home=preserve_home)
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


_GENERAL_WORKER_CHILD = r'''import hashlib,json,os,socket,subprocess,sys,tempfile
from pathlib import Path
def g(*a,raw=0):
 r=subprocess.run(["git",*a],capture_output=True,text=not raw)
 if r.returncode: raise RuntimeError(f"git:{r.stderr}")
 return r.stdout if raw else r.stdout.strip()
op,c=sys.argv[1:3]
if op=="worker":
 att,sess,b=sys.argv[3:6]; p_arg=sys.argv[6] if len(sys.argv)>6 else ""
 paths=[p.strip() for p in p_arg.split(",") if p.strip()]
 try:
  ch=[]
  for ps in paths:
   p=Path(ps)
   if not p.exists():
    p.parent.mkdir(parents=True,exist_ok=True); p.write_text(f"# WorkCard: {c}\n"); ch.append(ps)
   else:
    lines=p.read_text().splitlines(keepends=True); m=f"# WorkCard: {c}\n"
    if not any(m.strip()==l.strip() for l in lines): lines.append(m); p.write_text("".join(lines))
    ch.append(ps)
   g("add",ps)
  if [l for l in g("diff","--cached","--name-only").splitlines() if l]: g("commit","-m",f"chore: {c}")
  h=g("rev-parse","HEAD"); cp=[l for l in g("diff","--name-only",f"{b}..{h}").splitlines() if l]
  print(json.dumps({"schema_version":"steward_worker_outcome.v1","status":"PASS","session_id":sess,"head_sha":h,"changed_paths":cp or ch or (paths[:1] if paths else []),"detail":f"ok_{c}"},separators=(",",":")))
 except Exception as e:
  print(json.dumps({"schema_version":"steward_worker_outcome.v1","status":"FAIL","session_id":sess,"head_sha":b,"changed_paths":[],"detail":f"err:{e}"},separators=(",",":")))
else:
 b,h,impl,rev=sys.argv[3:7]; allowed=[p.strip() for p in (sys.argv[7] if len(sys.argv)>7 else "").split(",") if p.strip()]
 d=g("diff","--binary","--no-ext-diff",f"{b}...{h}",raw=1); names=[l for l in g("diff","--name-only",f"{b}...{h}").splitlines() if l]; bad=[]
 for n in names:
  if n not in allowed and not any(n.startswith(p.rstrip("/")+"/") for p in allowed): bad.append("outside_scope")
 if g("diff","--check",f"{b}...{h}"): bad.append("footprint")
 for n in names:
  ls=g("ls-tree","-r",h,"--",n).split()
  if ls and ls[:1] not in (["100644"],["100755"]): bad.append("regular_file")
 try:
  tp=Path(allowed[0] if allowed else "README.md"); tp.write_bytes(tp.read_bytes() if tp.exists() else b""); wb=False
 except OSError: wb=True
 try:
  s=socket.create_connection(("192.0.2.1",80),0.2); s.close(); nb=False
 except OSError: nb=True
 cf=not any(any(v in k.upper() for v in ("TOKEN","SECRET","PASSWORD","API_KEY","CREDENTIAL")) for k in os.environ)
 if not wb: bad.append("reviewer_write_not_blocked")
 if not nb: bad.append("network_not_isolated")
 if not cf: bad.append("credential_environment_not_clean")
 if d:
  with tempfile.TemporaryDirectory() as z:
   for n in names:
    try:
     t=g("show",f"{h}:{n}",raw=1); q=Path(z)/n; q.parent.mkdir(parents=True,exist_ok=True); q.write_bytes(t)
    except Exception: pass
   rr=subprocess.run(["git","apply","--check","--reverse"],input=d,cwd=z,capture_output=True); rb=(rr.returncode==0)
 else: rb=True
 if not rb: bad.append("rollback")
 st="PASS" if not bad else "FAIL"
 rows=[{"id":f"blocker-{i+1}","disposition":"block_current_head","status":"open","severity":"blocker","origin_head":h,"acceptance_condition":v} for i,v in enumerate(bad)]
 ledger=hashlib.sha256(json.dumps(rows,sort_keys=True,separators=(",",":")).encode()).hexdigest()
 x={"schema_version":"steward_review_outcome.v1","status":st,"reviewer_session_id":rev,"implementation_session_id":impl,"reviewed_head_sha":h,"blockers":bad,"detail":f"rev:wb={wb},nb={nb},cf={cf},rb={rb}","reviewed_base_sha":b,"reviewed_range_sha256":hashlib.sha256(d).hexdigest(),"review_axes":["standards","spec"],"review_round":1,"review_mode":"full","review_receipt_sha256":"","summary":"review","findings":None,"security_ok":wb and nb and cf,"rollback_ok":rb,"observed_ci_status":"unknown","finding_ledger_digest":ledger}
 y=dict(x); y.pop("detail"); y.pop("findings"); y.pop("summary"); y["review_receipt_sha256"]=""
 x["review_receipt_sha256"]=hashlib.sha256(json.dumps(y,sort_keys=True,separators=(",",":")).encode()).hexdigest()
 print(json.dumps(x,separators=(",",":")))
'''


def general_worker() -> BoundedProcessWorker:
    """TEST-ONLY legacy marker worker retained for deterministic fixtures.

    It is deliberately not admitted by the production ``Steward`` route.
    Remove it when historical fixture receipts no longer need compatibility.
    """

    def command(context: WorkerContext) -> list[str]:
        return [
            "/usr/bin/python3",
            "-c",
            _GENERAL_WORKER_CHILD,
            "worker",
            context.card_id,
            str(context.attempt),
            process_session_id(context),
            context.base_sha,
            ",".join(context.allowed_paths),
        ]

    return BoundedProcessWorker(command, timeout_seconds=300)


def general_reviewer() -> BoundedProcessReviewer:
    """TEST-ONLY legacy marker reviewer paired only with fixture workers."""

    def command(context: WorkerContext, outcome: WorkerOutcome) -> list[str]:
        return [
            "/usr/bin/python3",
            "-c",
            _GENERAL_WORKER_CHILD,
            "review",
            context.card_id,
            context.base_sha,
            outcome.head_sha,
            outcome.session_id,
            reviewer_session_id(context, outcome),
            ",".join(context.allowed_paths),
        ]

    return BoundedProcessReviewer(command, timeout_seconds=300)


class FakeTestWorker:
    """Explicit fake worker for deterministic unit tests and fault fixtures."""

    def __init__(
        self,
        *,
        status: str = "PASS",
        changed_paths: tuple[str, ...] | list[str] | None = None,
        detail: str = "fake_test_worker_outcome",
    ):
        self.status = status
        self.changed_paths = tuple(changed_paths) if changed_paths is not None else None
        self.detail = detail

    def run(self, context: WorkerContext) -> WorkerOutcome:
        session_id = process_session_id(context)
        head_sha = _head_or_base(context)
        paths = tuple(self.changed_paths) if self.changed_paths is not None else context.allowed_paths
        return WorkerOutcome(
            status=self.status,
            session_id=session_id,
            head_sha=head_sha,
            changed_paths=paths,
            detail=self.detail,
        )


class FakeTestReviewer:
    """Explicit fake reviewer for deterministic unit tests and fault fixtures."""

    def __init__(
        self,
        *,
        status: str = "PASS",
        blockers: tuple[str, ...] | list[str] = (),
        detail: str = "fake_test_reviewer_outcome",
    ):
        self.status = status
        self.blockers = tuple(blockers)
        self.detail = detail

    def review(self, context: WorkerContext, outcome: WorkerOutcome) -> ReviewOutcome:
        rev_session_id = reviewer_session_id(context, outcome)
        raw = {
            "schema_version": "steward_review_outcome.v1",
            "status": self.status,
            "reviewer_session_id": rev_session_id,
            "implementation_session_id": outcome.session_id,
            "reviewed_head_sha": outcome.head_sha,
            "blockers": list(self.blockers),
            "detail": self.detail,
            "reviewed_base_sha": context.base_sha,
            "reviewed_range_sha256": hashlib.sha256(b"").hexdigest(),
            "review_axes": ["standards", "spec"],
            "review_round": 1,
            "review_mode": "full",
            "summary": "bounded independent review",
            "findings": None,
            "security_ok": True,
            "rollback_ok": True,
            "observed_ci_status": "unknown",
            "finding_ledger_digest": "",
            "review_receipt_sha256": "",
        }
        sealed = seal_review_outcome_wire(raw)
        return ReviewOutcome.from_wire(sealed)

    run = review


class ProviderFreeWorker:
    """TEST-ONLY compatibility adapter for historical marker fixtures.

    Production construction rejects this type and uses
    :class:`CodexWorkCardWorker`; deletion is permitted once downstream
    fixture consumers have moved to ``FakeTestWorker``.
    """

    def __init__(self, worker: BoundedProcessWorker | None = None, *, configured: bool = False):
        self._worker = worker
        self._configured = configured or (worker is not None)

    def run(self, context: WorkerContext) -> WorkerOutcome:
        if not self._configured:
            raise WorkerUnavailable("provider_free_worker_not_configured")
        active_worker = self._worker or general_worker()
        return active_worker.run(context)


class CodexWorkCardWorker:
    """Production WorkCard implementation adapter using the accepted wrapper.

    The wrapper owns authenticated Codex invocation and its credential
    allowlist.  This adapter owns the WorkCard prompt projection, exact
    worktree hygiene, allowed-path enforcement, bounded timeout, and the one
    implementation commit.  It never pushes, creates a PR, or merges.
    """

    def __init__(
        self, *, wrapper_path: str | Path | None = None, timeout_seconds: int = 1800
    ) -> None:
        if type(timeout_seconds) is not int or not 1 <= timeout_seconds <= 3600:
            raise ValueError("codex_timeout_invalid")
        wrapper = Path(wrapper_path) if wrapper_path is not None else Path(__file__).with_name("codex_wrapper.sh")
        if wrapper.is_symlink() or not wrapper.is_file():
            raise ValueError("codex_wrapper_invalid")
        self.wrapper_path = wrapper.resolve()
        self.timeout_seconds = timeout_seconds

    @staticmethod
    def _git(worktree: Path, *args: str, timeout: int = 60) -> str:
        try:
            result = subprocess.run(
                ["git", *args], cwd=worktree, capture_output=True, text=True,
                timeout=timeout, check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise WorkerError("codex_git_unavailable") from exc
        if result.returncode != 0:
            raise WorkerError("codex_git_failed")
        return result.stdout.strip()

    @classmethod
    def _changed_paths(cls, context: WorkerContext) -> tuple[str, ...]:
        tracked = cls._git(context.worktree, "diff", "--name-only", "--diff-filter=ACDMRTUXB")
        staged = cls._git(context.worktree, "diff", "--cached", "--name-only", "--diff-filter=ACDMRTUXB")
        untracked = cls._git(context.worktree, "ls-files", "--others", "--exclude-standard")
        paths = tuple(sorted({*filter(None, tracked.splitlines()), *filter(None, staged.splitlines()), *filter(None, untracked.splitlines())}))
        for path in paths:
            _safe_path(path)
            if any(
                mission_contract.path_in_scope((forbidden,), path)
                for forbidden in context.forbidden_paths
            ) or not mission_contract.path_in_scope(context.allowed_paths, path):
                raise WorkerError("codex_path_outside_workcard")
        return paths

    @staticmethod
    def _sandbox_head(git_sandbox: _SandboxGit) -> str | None:
        try:
            result = subprocess.run(
                [
                    "/usr/bin/git",
                    "--git-dir",
                    str(git_sandbox.git_dir),
                    "rev-parse",
                    "--verify",
                    f"refs/heads/{git_sandbox.branch}",
                ],
                capture_output=True,
                text=True,
                timeout=30,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired):
            return None
        head = result.stdout.strip()
        return head if result.returncode == 0 and SHA40.fullmatch(head) else None

    @staticmethod
    def _validate_workcard_contract(context: WorkerContext) -> None:
        """Reject declarations the production adapter cannot execute safely."""

        if not context.steps or not context.focused_tests or not context.negative_checks or not context.expected_evidence:
            raise WorkerError("codex_workcard_contract_incomplete")
        for check in context.focused_tests:
            if check not in _WORKCARD_GATE_NAMES and local_verification.allowlisted_command(check) is None:
                raise WorkerError("codex_focused_check_not_allowlisted")

    @staticmethod
    def _run_workcard_checks(
        context: WorkerContext, changed_paths: tuple[str, ...]
    ) -> list[dict[str, Any]]:
        """Execute declared allowlisted checks, retaining symbolic parent gates."""

        displays = local_verification.select_issue_checks(list(changed_paths))
        for check in context.focused_tests:
            if check in _WORKCARD_GATE_NAMES:
                continue
            if check not in displays:
                displays.append(check)
        return local_verification.run_focused_checks(context.worktree, displays)

    @staticmethod
    def _prompt(context: WorkerContext) -> str:
        # This temporary prompt is intentionally not journaled.  It carries
        # only the bounded WorkCard contract and is removed after the wrapper
        # exits; raw model text is never retained as Steward evidence.
        rows = [
            "You are the implementation worker for one bounded repository WorkCard.",
            "Work only in the provided repository worktree.",
            "Do not access external directories, do not push, do not create PRs, and do not merge.",
            "Do not edit paths outside the allowed list. Do not commit; the Steward parent validates and commits.",
            "Produce one concrete, testable diff within the allowed scope; a prose-only response or no-op is not a completed WorkCard.",
            "Inspect the existing implementation first. If the requested capability is already present, address a concrete uncovered invariant or add a focused regression test within the allowed scope; do not make cosmetic or speculative changes.",
            "Never claim, synthesize, or infer live provider evidence, spend authorization, adoption authority, or scientific results from a repository diff.",
            f"Mission: {context.mission_id}",
            f"Mission objective: {context.objective}",
            f"Stage: {context.stage_id}",
            f"WorkCard: {context.card_id}",
            f"Attempt: {context.attempt}",
            "Allowed paths:",
            *(f"- {path}" for path in context.allowed_paths),
            "Forbidden paths:",
            *(f"- {path}" for path in context.forbidden_paths),
            "Required steps:",
            *(f"- {step}" for step in context.steps),
            "Focused tests:",
            *(f"- {check}" for check in context.focused_tests),
            "Negative checks:",
            *(f"- {check}" for check in context.negative_checks),
            "Expected evidence:",
            *(f"- {item}" for item in context.expected_evidence),
            f"Maximum attempts for this WorkCard: {context.max_attempts}",
            "Environment constraints: use only the supplied allowlisted child environment; no credential-shaped variables or Git metadata variables are available. Any proxy is a parent-validated unauthenticated literal-loopback endpoint.",
            "Allowlisted environment keys:",
            *(f"- {key}" for key in sorted(context.environment)),
            "Make the smallest complete change, run relevant focused tests, then stop.",
        ]
        return "\n".join(rows) + "\n"

    def _invoke(
        self,
        worker_type: str,
        prompt: str,
        worktree: Path,
        *,
        environment: Mapping[str, str],
        model_tier: str = "T1",
        git_sandbox: _SandboxGit | None = None,
        worktree_writable: bool = True,
    ) -> tuple[int, Path, str | None]:
        if worker_type not in {"implement", "ci-repair", "review"}:
            raise WorkerError("codex_worker_type_invalid")
        if model_tier not in {"T0", "T1", "T2"}:
            raise WorkerError("codex_model_tier_invalid")
        with tempfile.TemporaryDirectory(prefix="steward-codex-") as temp:
            root = Path(temp)
            prompt_path = root / "workcard.txt"
            output_dir = root / "output"
            prompt_path.write_text(prompt, encoding="utf-8")
            wrapper_copy = root / "codex_wrapper.sh"
            shutil.copyfile(self.wrapper_path, wrapper_copy)
            wrapper_copy.chmod(0o700)
            home = root / "home"
            home.mkdir(parents=True)
            codex_home = home / ".codex"
            codex_home.mkdir(parents=True)
            source_home = Path(str(environment.get("HOME", "")))
            readonly_paths: list[tuple[Path, Path]] = [
                (wrapper_copy, wrapper_copy),
                (prompt_path, prompt_path),
            ]
            # The Codex CLI reads its selected login from CODEX_HOME/auth.json
            # at runtime.  Keep the account-pool metadata below for account
            # identity/reconciliation, but also mount the operator-selected
            # runtime auth file at the path the CLI actually consumes.  Both
            # mounts are read-only and no other ~/.codex state is exposed.
            service_credential_declared = bool(
                os.environ.get("CREDENTIALS_DIRECTORY", "")
            )
            service_auth_source = _systemd_codex_auth_source()
            if service_credential_declared and service_auth_source is None:
                return 1, root / "missing", "authentication_failure"
            runtime_auth_source = (
                service_auth_source
                if service_auth_source is not None
                else source_home / ".codex" / "auth.json"
            )
            if (
                runtime_auth_source.is_file()
                and not runtime_auth_source.is_symlink()
            ):
                readonly_paths.append(
                    (runtime_auth_source, codex_home / "auth.json")
                )
            # Mount only the active Codex account registry and the auth file
            # selected by its authenticated active_account_key. Never mount
            # the operator's complete ~/.codex tree, which contains sessions,
            # rollout data, and unrelated private state.
            registry_source = source_home / ".codex" / "accounts" / "registry.json"
            if registry_source.is_file() and not registry_source.is_symlink():
                try:
                    registry = json.loads(registry_source.read_text(encoding="utf-8"))
                    active_key = registry.get("active_account_key")
                    accounts = registry.get("accounts")
                    known = (
                        isinstance(active_key, str)
                        and isinstance(accounts, list)
                        and any(
                            isinstance(account, dict)
                            and account.get("account_key") == active_key
                            for account in accounts
                        )
                    )
                    encoded = (
                        base64.urlsafe_b64encode(active_key.encode("utf-8"))
                        .decode("ascii")
                        .rstrip("=")
                        if known
                        else ""
                    )
                    auth_source = source_home / ".codex" / "accounts" / f"{encoded}.auth.json"
                    if (
                        known
                        and auth_source.is_file()
                        and not auth_source.is_symlink()
                    ):
                        registry_destination = codex_home / "accounts" / "registry.json"
                        auth_destination = codex_home / "accounts" / auth_source.name
                        readonly_paths.extend(
                            (
                                (registry_source, registry_destination),
                                (auth_source, auth_destination),
                            )
                        )
                except (OSError, UnicodeError, json.JSONDecodeError):
                    pass
            child_environment = dict(environment)
            child_environment["HOME"] = str(home)
            child_environment["CODEX_HOME"] = str(codex_home)
            child_environment["AGENT_CODEX_MODEL_TIER"] = model_tier
            child_environment["AGENT_CODEX_TIMEOUT_SECONDS"] = str(self.timeout_seconds)
            codex_path: Path | None = None
            if service_auth_source is not None:
                codex_path = SERVICE_CODEX_BINARY
                if codex_path.is_symlink():
                    codex_path = None
            else:
                codex_bin = shutil.which(
                    "codex", path=child_environment.get("PATH", "")
                )
                if codex_bin:
                    candidate = Path(codex_bin)
                    try:
                        candidate_metadata = os.lstat(candidate)
                    except OSError:
                        candidate_metadata = None
                    if (
                        candidate.is_absolute()
                        and candidate_metadata is not None
                        and stat.S_ISREG(candidate_metadata.st_mode)
                    ):
                        codex_path = candidate
            if codex_path is not None and codex_path.is_file():
                codex_metadata = os.lstat(codex_path)
                if (
                    stat.S_ISREG(codex_metadata.st_mode)
                    and not codex_metadata.st_mode & 0o022
                    and os.access(codex_path, os.X_OK)
                ):
                    codex_destination = root / "codex"
                    readonly_paths.append((codex_path, codex_destination))
            sandboxed_command = _sandbox_command(
                [
                    str(wrapper_copy),
                    worker_type,
                    str(prompt_path),
                    str(output_dir),
                    str(worktree),
                ],
                worktree,
                child_environment,
                git_sandbox=git_sandbox,
                worktree_writable=worktree_writable,
                writable_paths=(root,),
                readonly_paths=tuple(readonly_paths),
                # The Codex provider transport needs egress. Its filesystem
                # remains namespace-isolated and the wrapper strips all
                # proxy/network selector variables and external paths.
                isolate_network=False,
            )
            try:
                exit_code, _stdout, _stderr = local_verification._bounded_process(
                    sandboxed_command,
                    cwd=worktree,
                    timeout_seconds=self.timeout_seconds + 30,
                    env=child_environment,
                )
            except local_verification.LocalVerificationError as exc:
                raise WorkerError("codex_sandbox_unavailable") from exc
            failure_reason: str | None = None
            if exit_code != 0:
                failure_reason = self._bounded_failure_reason(output_dir)
            # The review adapter needs the bounded response for immediate
            # validation, but never persists it.  Copy it into a second
            # short-lived directory controlled by the caller.
            if worker_type == "review" and exit_code == 0:
                message = output_dir / "codex-last-message.txt"
                if message.is_file() and not message.is_symlink():
                    retained = Path(tempfile.mkdtemp(prefix="steward-codex-review-")) / "message.txt"
                    retained.write_bytes(message.read_bytes())
                    return exit_code, retained, None
            return exit_code, root / "missing", failure_reason

    @staticmethod
    def _bounded_failure_reason(output_dir: Path) -> str | None:
        """Return only a wrapper-owned failure category, never provider output."""

        path = output_dir / "failure_reason.json"
        if path.is_symlink() or not path.is_file():
            return None
        try:
            raw = path.read_bytes()
            if not raw or len(raw) > 4096:
                return None
            value = json.loads(raw)
        except (OSError, UnicodeDecodeError, json.JSONDecodeError):
            return None
        reasons = {
            "authentication_failure",
            "cli_missing",
            "environment_invalid",
            "malformed_output",
            "model_execution_failure",
            "model_execution_timeout",
            "prompt_missing",
            "timeout_invalid",
            "timeout_unavailable",
            "unsupported_flags",
            "unsupported_worker_type",
            "usage_or_credit_exhaustion",
            "workspace_invalid",
        }
        if (
            not isinstance(value, dict)
            or value.get("kind") != "agent-orchestrator-failure"
            or value.get("reason") not in reasons
        ):
            return None
        return value["reason"]

    def run(self, context: WorkerContext) -> WorkerOutcome:
        self._validate_workcard_contract(context)
        if self._git(context.worktree, "status", "--porcelain=v1", "--untracked-files=all"):
            raise WorkerError("codex_worktree_not_clean")
        before = self._git(context.worktree, "rev-parse", "--verify", "HEAD")
        if before != context.base_sha:
            raise WorkerError("codex_base_head_mismatch")
        git_sandbox = _sandbox_for_context(context)
        try:
            exit_code, _unused, failure_reason = self._invoke(
                "implement", self._prompt(context), context.worktree,
                environment=context.environment,
                model_tier=context.model_tier,
                git_sandbox=git_sandbox,
            )
            if git_sandbox is not None:
                child_head = self._sandbox_head(git_sandbox)
                if child_head is not None and child_head != context.base_sha:
                    git_sandbox.import_head(
                        base_sha=context.base_sha,
                        head_sha=child_head,
                        branch=git_sandbox.branch,
                    )
        finally:
            if git_sandbox is not None:
                git_sandbox.cleanup()
        after = self._git(context.worktree, "rev-parse", "--verify", "HEAD")
        if after != before:
            # The agent performed an unapproved Git effect.  Preserve the
            # worktree for read-only reconciliation and never retry blindly.
            return WorkerOutcome("OUTCOME_UNKNOWN", process_session_id(context), after, (), "codex_unexpected_commit")
        paths = self._changed_paths(context)
        if exit_code == 124 or failure_reason == "model_execution_timeout":
            return WorkerOutcome("OUTCOME_UNKNOWN" if paths else "TIMEOUT", process_session_id(context), before, (), "codex_timeout")
        if exit_code != 0:
            detail = f"codex_{failure_reason or 'execution_failed'}"
            return WorkerOutcome("OUTCOME_UNKNOWN" if paths else "FAIL", process_session_id(context), before, (), detail)
        if not paths:
            return WorkerOutcome("FAIL", process_session_id(context), before, (), "codex_no_change")
        try:
            checks = self._run_workcard_checks(context, paths)
            validate_check_results(checks)
        except Exception:
            return WorkerOutcome("FAIL", process_session_id(context), before, (), "codex_focused_checks_failed")
        if self._git(context.worktree, "diff", "--check"):
            raise WorkerError("codex_diff_check_failed")
        self._git(context.worktree, "add", "--", *paths)
        if not self._git(context.worktree, "diff", "--cached", "--name-only"):
            raise WorkerError("codex_stage_empty")
        self._git(
            context.worktree,
            "-c", "user.name=Autonomous Steward Codex",
            "-c", "user.email=steward-codex@localhost.invalid",
            "commit", "-m", f"steward: {context.stage_id} {context.card_id}",
            timeout=120,
        )
        head = self._git(context.worktree, "rev-parse", "--verify", "HEAD")
        if head == before or not SHA40.fullmatch(head):
            raise WorkerError("codex_commit_invalid")
        if self._git(context.worktree, "status", "--porcelain=v1", "--untracked-files=all"):
            raise WorkerError("codex_worktree_dirty_after_commit")
        return WorkerOutcome("PASS", process_session_id(context), head, paths, "codex_workcard_completed")


class CodexWorkCardReviewer:
    """Production independent reviewer using the accepted read-only wrapper."""

    def __init__(self, *, worker: CodexWorkCardWorker | None = None) -> None:
        self.worker = worker or CodexWorkCardWorker()

    @staticmethod
    def _prompt(context: WorkerContext, outcome: WorkerOutcome) -> str:
        return "\n".join((
            "Independently review the complete exact Git range below.",
            "You are read-only: do not edit, commit, push, create PRs, or merge.",
            f"Mission: {context.mission_id}",
            f"Stage: {context.stage_id}",
            f"WorkCard: {context.card_id}",
            f"Review objective: {context.objective}",
            f"Base SHA: {context.base_sha}",
            f"Head SHA: {outcome.head_sha}",
            "The repository checkout and exact Git range are available for read-only inspection.",
            "Inspect the complete base-to-head range with Git before deciding; provider availability is not a review finding.",
            "Allowed paths:",
            *(f"- {path}" for path in context.allowed_paths),
            "Review Standards and Spec. Return exactly one compact JSON object:",
            '{"verdict":"PASS"|"FAIL","blockers":["bounded-id"],"summary":"bounded summary"}',
            "Do not add prose. A single optional ```json code fence is tolerated by the transport.",
            "Keep summary on one line and at most 240 characters.",
            "Use PASS only when the change is within scope, safe, reversible, and complete.",
        )) + "\n"

    @staticmethod
    def _decode_response(raw: bytes) -> Any:
        """Decode one bounded review object, tolerating one JSON code fence.

        Codex transports model text rather than a native structured-output
        channel.  Models commonly preserve an otherwise exact JSON response in
        a Markdown JSON fence or bounded narration.  Extract exactly one JSON
        object, then leave authority to the strict schema checks below;
        multiple objects and malformed envelopes still fail closed.
        """

        try:
            text = raw.decode("utf-8").strip()
        except UnicodeDecodeError as exc:
            raise WorkerError("codex_review_output_invalid") from exc
        if text.startswith("```"):
            match = re.fullmatch(
                r"```(?:json)?[ \t]*\r?\n(?P<body>.*?)\r?\n```",
                text,
                flags=re.DOTALL | re.IGNORECASE,
            )
            if match is None:
                raise WorkerError("codex_review_output_invalid")
            text = match.group("body").strip()
        try:
            return json.loads(text)
        except json.JSONDecodeError:
            pass
        # Some providers add a short bounded sentence around the requested
        # object.  Admit exactly one JSON object and let the strict schema
        # validation below decide authority; ambiguity still fails closed.
        decoder = json.JSONDecoder()
        candidates: list[Any] = []
        for index, character in enumerate(text):
            if character != "{":
                continue
            try:
                candidate, _end = decoder.raw_decode(text, index)
            except json.JSONDecodeError:
                continue
            if isinstance(candidate, dict):
                candidates.append(candidate)
        if len(candidates) != 1:
            raise WorkerError("codex_review_output_invalid")
        return candidates[0]

    @staticmethod
    def _bounded_summary(value: str) -> str:
        """Normalize non-authoritative reviewer prose for the bounded wire."""

        summary = " ".join(value.split())
        if not summary:
            summary = "structured review verdict"
        return summary[:512]

    def review(self, context: WorkerContext, outcome: WorkerOutcome) -> ReviewOutcome:
        git_sandbox = _sandbox_for_context(context)
        try:
            exit_code, response_path, failure_reason = self.worker._invoke(
                "review", self._prompt(context, outcome), context.worktree,
                environment=context.environment,
                model_tier=context.model_tier,
                git_sandbox=git_sandbox,
                worktree_writable=False,
            )
        finally:
            if git_sandbox is not None:
                git_sandbox.cleanup()
        try:
            if exit_code != 0 or not response_path.is_file() or response_path.is_symlink():
                detail = f"codex_review_{failure_reason or 'execution_failed'}"
                raise WorkerError(detail)
            raw = response_path.read_bytes()
            if not raw or len(raw) > 16 * 1024:
                raise WorkerError("codex_review_output_invalid")
            value = self._decode_response(raw)
            if (
                not isinstance(value, dict)
                or set(value) != {"verdict", "blockers", "summary"}
                or value["verdict"] not in {"PASS", "FAIL"}
                or not isinstance(value["blockers"], list)
                or not all(isinstance(item, str) and SESSION_ID.fullmatch(item) for item in value["blockers"])
                or not isinstance(value["summary"], str)
            ):
                raise WorkerError("codex_review_output_invalid")
            blockers = tuple(value["blockers"])
            if (value["verdict"] == "PASS") != (not blockers):
                raise WorkerError("codex_review_verdict_invalid")
            raw_wire = {
                "schema_version": "steward_review_outcome.v1",
                "status": value["verdict"],
                "reviewer_session_id": reviewer_session_id(context, outcome),
                "implementation_session_id": outcome.session_id,
                "reviewed_head_sha": outcome.head_sha,
                "blockers": list(blockers),
                "detail": "codex_independent_review",
                "reviewed_base_sha": context.base_sha,
                "reviewed_range_sha256": review_range_digest(context.base_sha, outcome.head_sha, worktree=context.worktree),
                "review_axes": ["standards", "spec"],
                "review_round": 1,
                "review_mode": "full",
                "review_receipt_sha256": "",
                "summary": self._bounded_summary(value["summary"]),
                "findings": None,
                "security_ok": value["verdict"] == "PASS",
                "rollback_ok": value["verdict"] == "PASS",
                "observed_ci_status": "unknown",
                "finding_ledger_digest": "",
            }
            return ReviewOutcome.from_wire(seal_review_outcome_wire(raw_wire))
        finally:
            if response_path.parent.name.startswith("steward-codex-review-"):
                try:
                    response_path.unlink(missing_ok=True)
                    response_path.parent.rmdir()
                except OSError:
                    pass


def production_worker() -> CodexWorkCardWorker:
    """Return the sole production implementation adapter."""

    return CodexWorkCardWorker()


def production_reviewer() -> CodexWorkCardReviewer:
    """Return the independent production reviewer adapter."""

    return CodexWorkCardReviewer()


PR4B_CANARY_PROPOSAL_SHA256 = (
    "3a55ac107a2cae2a049e37804ea851036849c37aa84f95138db7d7f611db7eae"
)
PR4B_CANARY_APPROVAL_ISSUE = 208
PR4B_CANARY_ALLOWED_PATHS = ("docs/ARCHITECTURE.md",)

# TEST-ONLY historical PR4B canary compatibility.  It is neither selected nor
# admitted by the production route; retain it only while old deterministic
# receipt fixtures reference it, then delete it with those fixtures.
_PR4B_CANARY_CHILD = r'''import hashlib,json,os,socket,subprocess,sys,tempfile
from pathlib import Path
T="docs/ARCHITECTURE.md"; A={"leaf-a":"Rust `engine/` as the sole runtime, scheduler, policy, and application-owned storage authority.","leaf-b":"Autonomous Steward as the repository-maintenance outer loop coordinating missions, stages, and workcards without creating parallel schedulers or state stores."}
def g(*a,raw=0):
 r=subprocess.run(["git",*a],capture_output=True,text=not raw)
 if r.returncode: raise RuntimeError("git")
 return r.stdout if raw else r.stdout.strip()
def leaf(c):
 s=c.rsplit(":",1)[-1]
 if s not in A: raise ValueError("card")
 return s
op=sys.argv[1]; c=sys.argv[2]; s=leaf(c)
if op=="worker":
 if sys.argv[3]!="1": raise ValueError("attempt")
 p=Path(T); lines=p.read_text(encoding="utf-8").splitlines(keepends=True); mark=f"- PR4B canary WorkCard `{s}` executed by the bounded provider-free runtime; this is an execution receipt, not final cutover acceptance.\n"
 if sum(A[s] in x for x in lines)!=1 or mark in lines: raise RuntimeError("anchor")
 i=next(i for i,x in enumerate(lines) if A[s] in x); lines.insert(i+1,mark); p.write_text("".join(lines),encoding="utf-8"); g("add",T)
 if g("diff","--cached","--name-only").splitlines()!=[T]: raise RuntimeError("stage")
 g("commit","-m",f"chore: record PR4B canary {s}"); h=g("rev-parse","HEAD")
 if g("diff","--name-only","HEAD^","HEAD").splitlines()!=[T]: raise RuntimeError("commit")
 print(json.dumps({"schema_version":"steward_worker_outcome.v1","status":"PASS","session_id":sys.argv[4],"head_sha":h,"changed_paths":[T],"detail":f"canary_receipt_{s}"},separators=(",",":")))
else:
 b,h,impl,rev=sys.argv[3:7]; d=g("diff","--binary","--no-ext-diff",f"{b}...{h}",raw=True); dt=d.decode("utf-8"); names=g("diff","--name-only",f"{b}...{h}").splitlines(); text=g("show",f"{h}:{T}"); bad=[]
 if names!=[T] or g("diff","--check",f"{b}...{h}") or g("diff","--summary",f"{b}...{h}"): bad.append("footprint")
 if g("ls-tree","-r",h,"--",T).split()[:1]!=["100644"]: bad.append("regular_file")
 mark=f"PR4B canary WorkCard `{s}`"; ok=text.count(mark)==1 and mark in dt; ok=ok and text.count(A[s])==1
 try:
  Path(T).write_bytes(Path(T).read_bytes()); write_blocked=False
 except OSError: write_blocked=True
 try:
  socket.create_connection(("192.0.2.1",80),.2); network_blocked=False
 except OSError: network_blocked=True
 credential_free=not any(any(v in k.upper() for v in ("TOKEN","SECRET","PASSWORD","API_KEY","CREDENTIAL")) for k in os.environ)
 if not write_blocked: bad.append("reviewer_write_not_blocked")
 if not network_blocked: bad.append("network_not_isolated")
 if not credential_free: bad.append("credential_environment_not_clean")
 if not ok: bad.append("receipt")
 with tempfile.TemporaryDirectory() as z:
  q=Path(z)/T; q.parent.mkdir(); q.write_text(text); rr=subprocess.run(["git","apply","--check","--reverse"],input=d,cwd=z,capture_output=True); rollback=rr.returncode==0
 if not rollback: bad.append("rollback")
 status="PASS" if not bad else "FAIL"; rows=[{"id":f"blocker-{i+1}","disposition":"block_current_head","status":"open","severity":"blocker","origin_head":h,"acceptance_condition":v} for i,v in enumerate(bad)]; ledger=hashlib.sha256(json.dumps(rows,sort_keys=True,separators=(",",":")).encode()).hexdigest(); x={"schema_version":"steward_review_outcome.v1","status":status,"reviewer_session_id":rev,"implementation_session_id":impl,"reviewed_head_sha":h,"blockers":bad,"detail":f"canary_review:write_blocked={write_blocked},network_blocked={network_blocked},credential_free={credential_free},rollback={rollback}","reviewed_base_sha":b,"reviewed_range_sha256":hashlib.sha256(d).hexdigest(),"review_axes":["standards","spec"],"review_round":1,"review_mode":"full","review_receipt_sha256":"","summary":"review","findings":None,"security_ok":write_blocked and network_blocked and credential_free,"rollback_ok":rollback,"observed_ci_status":"unknown","finding_ledger_digest":ledger}; y=dict(x); y.pop("detail"); y.pop("findings"); y.pop("summary"); y["review_receipt_sha256"]=""; x["review_receipt_sha256"]=hashlib.sha256(json.dumps(y,sort_keys=True,separators=(",",":")).encode()).hexdigest(); print(json.dumps(x,separators=(",",":")))
'''


def pr4b_canary_worker() -> BoundedProcessWorker:
    """Build the TEST-ONLY fixed worker for the historical PR4B canary."""

    def command(context: WorkerContext) -> list[str]:
        if context.allowed_paths != PR4B_CANARY_ALLOWED_PATHS:
            raise WorkerUnavailable("pr4b_canary_scope_not_supported")
        return [
            "/usr/bin/python3",
            "-c",
            _PR4B_CANARY_CHILD,
            "worker",
            context.card_id,
            str(context.attempt),
            process_session_id(context),
        ]

    return BoundedProcessWorker(command, timeout_seconds=300)


def pr4b_canary_reviewer() -> BoundedProcessReviewer:
    """Build the TEST-ONLY reviewer for the historical PR4B canary."""

    def command(context: WorkerContext, outcome: WorkerOutcome) -> list[str]:
        if context.allowed_paths != PR4B_CANARY_ALLOWED_PATHS:
            raise WorkerUnavailable("pr4b_canary_review_scope_not_supported")
        return [
            "/usr/bin/python3",
            "-c",
            _PR4B_CANARY_CHILD,
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
    writable_paths: tuple[Path, ...] = (),
    readonly_paths: tuple[tuple[Path, Path], ...] = (),
    isolate_network: bool = True,
) -> list[str]:
    """Run a bounded child with explicitly scoped worktree and Git access."""

    try:
        metadata = os.lstat(BWRAP_PATH)
    except OSError as exc:
        raise WorkerError("sandbox_unavailable") from exc
    if (
        not stat.S_ISREG(metadata.st_mode)
        or metadata.st_uid != 0
        or metadata.st_mode & 0o022
        or not os.access(BWRAP_PATH, os.X_OK)
    ):
        raise WorkerError("sandbox_unavailable")
    bubblewrap = BWRAP_PATH
    args = [
        str(bubblewrap),
        "--die-with-parent",
        "--unshare-user",
        "--unshare-pid",
        "--clearenv",
        "--tmpfs",
        "/",
    ]
    if isolate_network:
        args.insert(4, "--unshare-net")
    for system_path in ("/usr", "/bin", "/lib", "/lib64"):
        if Path(system_path).exists():
            args.extend(("--ro-bind", system_path, system_path))
    # A child needs loader/account metadata, but must not receive a readable
    # copy of the host's complete /etc (which can contain credentials or
    # operator configuration).  Provider-backed adapters may explicitly
    # retain host egress while still using this filesystem namespace, so they
    # receive only the resolver and hosts files required for name lookup.
    args.extend(("--dir", "/etc"))
    for system_file in (
        "/etc/ld.so.cache",
        "/etc/nsswitch.conf",
        "/etc/passwd",
        "/etc/group",
        "/etc/localtime",
        "/etc/ssl/certs/ca-certificates.crt",
    ):
        if Path(system_file).is_file():
            args.extend(("--ro-bind", system_file, system_file))
    # Keep provider-backed workers able to resolve their authenticated
    # endpoint without exposing the rest of the host's /etc.  resolv.conf is
    # commonly a symlink into /run, so bind the resolved regular file to a
    # fresh namespace path rather than mounting /run wholesale.
    resolver_source = Path("/etc/resolv.conf").resolve()
    if resolver_source.is_file():
        args.extend(("--ro-bind", str(resolver_source), "/etc/resolv.conf"))
    if Path("/etc/hosts").is_file():
        args.extend(("--ro-bind", "/etc/hosts", "/etc/hosts"))
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
    for path in writable_paths:
        source = path.resolve()
        if not source.exists() or source.is_symlink():
            raise WorkerError("sandbox_bind_path_invalid")
        add_parent_dirs(source)
        args.extend(("--bind", str(source), str(source)))
    for source_path, destination_path in readonly_paths:
        source = source_path.resolve()
        destination = destination_path.resolve()
        if not source.exists() or source.is_symlink() or (destination.exists() and destination.is_symlink()):
            raise WorkerError("sandbox_bind_path_invalid")
        add_parent_dirs(destination)
        args.extend(("--ro-bind", str(source), str(destination)))
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
                exit_code, stdout, _stderr = local_verification._bounded_process(
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
                exit_code, stdout, _stderr = local_verification._bounded_process(
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
                exit_code, stdout, _stderr = local_verification._bounded_process(
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
                exit_code, stdout, _stderr = local_verification._bounded_process(
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
    # Non-success outcomes are terminal worker reports. They are not claims
    # of a committed head or changed-path set, so preserve their bounded
    # reason instead of turning them into a misleading binding failure.
    if outcome.status != "PASS":
        return outcome
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
        for slot in range(MAX_ACTIVE_WORKERS):
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
    "FakeTestReviewer",
    "FakeTestWorker",
    "CodexWorkCardReviewer",
    "CodexWorkCardWorker",
    "PathConflict",
    "PathLockSet",
    "ProviderFreeWorker",
    "PR4B_CANARY_PROPOSAL_SHA256",
    "pr4b_canary_reviewer",
    "pr4b_canary_worker",
    "production_reviewer",
    "production_worker",
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
    "general_reviewer",
    "general_worker",
    "lock_footprint",
    "run_allowlisted_checks",
    "select_model_tier",
    "validate_changed_paths",
    "validate_check_results",
    "validate_worker_outcome",
]
