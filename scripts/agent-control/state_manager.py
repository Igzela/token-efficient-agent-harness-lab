"""Issue/PR state management for the agent orchestrator.

All state is persisted in GitHub Issues, labels, and Issue comments.
This module never stores state locally -- it reads/writes via the `gh` CLI.
"""

import json
import os
import re
import subprocess
import sys
from datetime import datetime
from dataclasses import asdict, dataclass, field
from pathlib import Path


GH = os.environ.get("AGENT_GH_CMD", "gh")

LABEL_DRAFT = "agent-draft"
LABEL_READY = "agent-ready"
LABEL_RUNNING = "agent-running"
LABEL_CI_REPAIRING = "ci-repairing"
LABEL_REVIEW_RUNNING = "review-running"
LABEL_REVIEW_PASSED = "review-passed"
LABEL_REVIEW_BLOCKED = "agent-review-blocked"
LABEL_MERGE_READY = "agent-merge-ready"
LABEL_BLOCKED = "agent-blocked"
LABEL_COMPLETE = "agent-complete"

ACTIVE_LABELS = {LABEL_RUNNING, LABEL_CI_REPAIRING, LABEL_REVIEW_RUNNING}
TERMINAL_LABELS = {LABEL_COMPLETE, LABEL_BLOCKED, LABEL_REVIEW_BLOCKED}
TRUSTED_STATE_AUTHORS = frozenset({"github-actions", "github-actions[bot]"})
ALL_LABELS = ACTIVE_LABELS | TERMINAL_LABELS | {
    LABEL_DRAFT, LABEL_READY, LABEL_REVIEW_PASSED, LABEL_MERGE_READY,
}

MAX_REPAIR_ATTEMPTS = 2
MAX_REVIEW_EVIDENCE_BYTES = 64 * 1024
MAX_REVIEW_API_PAGES = 20
MAX_REVIEW_API_NODES = 1000
MAX_REVIEW_THREAD_PAGES = 20
MAX_REVIEW_THREADS = 2000
REVIEW_VERDICTS = frozenset({"PASS", "PASS_WITH_NOTES", "BLOCKED", "FAIL"})
WORKFLOW_FAILURE_KINDS = frozenset({"implementation", "review", "ci-repair"})
PREFLIGHT_FAILURE_REASONS = frozenset({
    "control_not_live",
    "github_actions_pr_creation_disabled",
    "github_workflow_permissions_malformed",
    "github_workflow_permissions_unavailable",
    "invalid_issue_scope",
    "dispatcher_claim_invalid",
})
WORKFLOW_JOB_ORDER = {
    "implementation": ("gate", "validate", "vader-implementation", "finalize"),
    "review": ("prepare", "vader-review", "finalize"),
    "ci-repair": ("prepare", "vader-repair", "finalize"),
}
JOB_PHASES = {
    "gate": "control_gate",
    "validate": "claim_validation",
    "prepare": "trusted_input_preparation",
    "vader-implementation": "isolated_implementation",
    "vader-review": "isolated_review",
    "vader-repair": "isolated_ci_repair",
    "finalize": "trusted_finalization",
}


class StateUnavailableError(RuntimeError):
    """Raised when durable GitHub-hosted state cannot be read unambiguously."""


def _state_wire(kind, version, state):
    return {"kind": kind, "version": version, **asdict(state)}


@dataclass(frozen=True)
class WorkerState:
    pr_number: int
    head_sha: str
    worker_type: str
    extra: dict

    def to_wire(self):
        return _state_wire("agent-orchestrator-state", 1, self)


@dataclass(frozen=True)
class CIState:
    pr_number: int
    head_sha: str
    workflow_run_id: object
    workflow_name: str
    required_jobs: list
    successful_jobs: list
    status: str
    extra: dict

    def to_wire(self):
        return _state_wire("agent-orchestrator-ci-state", 2, self)


@dataclass(frozen=True)
class CITerminalResolutionState:
    issue_number: int
    pr_number: int
    head_sha: str
    ci_run_id: int
    terminal_status: str
    reason: str
    observed_status: str
    capacity_release_outcome: str
    capacity_release_reason: str

    def to_wire(self):
        return _state_wire("agent-orchestrator-ci-terminal-resolution", 1, self)


@dataclass(frozen=True)
class CIAcquisitionState:
    pr_number: int
    head_sha: str
    workflow_run_id: int
    source: str
    status: str
    duplicate_run_ids: list[int]
    observed_run_ids: list[int] = field(default_factory=list)
    selection_reason: str = ""
    superseded_run_ids: list[int] = field(default_factory=list)
    unsupported_run_ids: list[int] = field(default_factory=list)
    fallback_dispatched: bool = False

    def to_wire(self):
        return _state_wire("agent-orchestrator-ci-acquisition", 2, self)


@dataclass(frozen=True)
class ReviewState:
    issue_number: int
    pr_number: int
    head_sha: str
    verdict: str
    summary: str
    blockers: list[str] = field(default_factory=list)
    major_notes: list[str] = field(default_factory=list)
    minor_notes: list[str] = field(default_factory=list)
    artifact_sha256: str = ""
    review_workflow_run_id: int | None = None

    def to_wire(self):
        return _state_wire("agent-orchestrator-review-state", 2, self)


@dataclass(frozen=True)
class ReviewValidationFailureState:
    issue_number: int
    pr_number: int
    head_sha: str
    failure_code: str
    artifact_sha256: str | None
    review_workflow_run_id: int | None

    def to_wire(self):
        return _state_wire("agent-orchestrator-review-validation-failure", 1, self)


@dataclass(frozen=True)
class WorkflowFailureState:
    issue_number: int
    workflow_kind: str
    workflow_run_id: int
    failed_job: str
    failed_phase: str
    reason_code: str
    capacity_release_outcome: str
    capacity_release_reason: str

    def to_wire(self):
        return _state_wire("agent-orchestrator-worker-failure", 1, self)


@dataclass(frozen=True)
class MergeState:
    issue_number: int
    pr_number: int
    expected_head_sha: str
    merge_commit_sha: str
    status: str

    def to_wire(self):
        return _state_wire("agent-orchestrator-merge-state", 1, self)


@dataclass(frozen=True)
class DispatchState:
    issue_number: int
    dispatch_id: str
    action: str
    status: str
    details: dict

    def to_wire(self):
        return _state_wire("agent-orchestrator-dispatch-state", 1, self)


def labels_for_review_verdict(verdict):
    """Return the non-active state labels for a finalized review verdict."""

    if verdict not in REVIEW_VERDICTS:
        raise ValueError("unsupported review verdict")
    if verdict == "PASS":
        return {LABEL_REVIEW_PASSED, LABEL_MERGE_READY}
    return {LABEL_REVIEW_BLOCKED}


def finalize_review_labels(issue_number, verdict, repo=""):
    """Release review capacity into a verified, non-active verdict state."""

    try:
        expected = labels_for_review_verdict(verdict)
    except ValueError:
        return False
    if get_issue_labels_checked(issue_number, repo) is None:
        return False
    if not set_labels(issue_number, *sorted(expected), repo=repo):
        return False
    resulting = get_issue_labels_checked(issue_number, repo)
    if resulting is None or not expected.issubset(resulting):
        return False
    if resulting & ACTIVE_LABELS:
        return False
    if verdict == "PASS":
        return LABEL_REVIEW_BLOCKED not in resulting
    return not ({LABEL_REVIEW_PASSED, LABEL_MERGE_READY} & resulting)


def _gh(*args, **kwargs):
    cmd = [GH] + list(args)
    input_data = kwargs.get("input_data")
    stdin_val = None
    if input_data is not None:
        stdin_val = input_data.encode() if isinstance(input_data, str) else input_data
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            input=stdin_val,
            timeout=30,
        )
        if result.returncode != 0:
            print(f"gh error (exit {result.returncode}): {result.stderr.strip()}", file=sys.stderr)
            return None
        return result.stdout.strip()
    except (subprocess.TimeoutExpired, OSError):
        print(f"gh timed out: {' '.join(cmd)}", file=sys.stderr)
        return None


def get_issue_labels(issue_number, repo=""):
    labels = get_issue_labels_checked(issue_number, repo)
    return labels if labels is not None else set()


def get_issue_labels_checked(issue_number, repo=""):
    """Return Issue labels, preserving API/parse failure as ``None``."""

    if repo:
        labels = _gh("issue", "view", str(issue_number), "--repo", repo, "--json", "labels")
    else:
        labels = _gh("issue", "view", str(issue_number), "--json", "labels")
    if labels is None:
        return None
    try:
        parsed = json.loads(labels)
        if not isinstance(parsed, dict):
            return None
        return {lbl["name"] for lbl in parsed.get("labels", [])}
    except (json.JSONDecodeError, KeyError, TypeError):
        return None


def get_issue_body(issue_number, repo=""):
    if repo:
        body = _gh("issue", "view", str(issue_number), "--repo", repo, "--json", "body")
    else:
        body = _gh("issue", "view", str(issue_number), "--json", "body")
    if not body:
        return None
    try:
        parsed = json.loads(body)
        return parsed.get("body", "") if isinstance(parsed, dict) else None
    except json.JSONDecodeError:
        return None


def add_labels(issue_number, *labels, repo=""):
    args = ["issue", "edit", str(issue_number)]
    if repo:
        args.extend(["--repo", repo])
    args.extend(["--add-label", ",".join(labels)])
    return _gh(*args) is not None


def remove_labels(issue_number, *labels, repo=""):
    ok = True
    for label in labels:
        args = ["issue", "edit", str(issue_number)]
        if repo:
            args.extend(["--repo", repo])
        args.extend(["--remove-label", label])
        ok = _gh(*args) is not None and ok
    return ok


def set_labels(issue_number, *labels, repo=""):
    args = ["issue", "edit", str(issue_number)]
    if repo:
        args.extend(["--repo", repo])
    args.extend(["--add-label", ",".join(labels)])
    for label in ALL_LABELS:
        if label not in labels:
            args.extend(["--remove-label", label])
    return _gh(*args) is not None


def comment_on_issue(issue_number, body, repo=""):
    args = ["issue", "comment", str(issue_number)]
    if repo:
        args.extend(["--repo", repo])
    args.extend(["--body", body])
    return _gh(*args) is not None


def get_issue_comments(issue_number, repo=""):
    """Get all comments on an Issue, newest first."""
    args = ["issue", "view", str(issue_number), "--json", "comments"]
    if repo:
        args.extend(["--repo", repo])
    result = _gh(*args)
    if result is None:
        raise StateUnavailableError("Issue comment state is unavailable")
    try:
        data = json.loads(result)
        comments = data.get("comments", [])
        if not isinstance(comments, list) or not all(isinstance(item, dict) for item in comments):
            raise StateUnavailableError("Issue comment state is invalid")
        return list(reversed(comments))
    except json.JSONDecodeError as exc:
        raise StateUnavailableError("Issue comment state is invalid") from exc


def get_issue_comment_bodies(issue_number, search_text, repo=""):
    """Search Issue comments (not PR comments) for matching text, newest first."""
    comments = get_issue_comments(issue_number, repo)
    for comment in comments:
        if (comment.get("author") or {}).get("login") not in TRUSTED_STATE_AUTHORS:
            continue
        body = comment.get("body", "")
        if search_text in body:
            return body
    return None


def get_pr_info(pr_number, repo=""):
    args = [
        "pr", "view", str(pr_number), "--json",
        "headRefName,headRefOid,state,mergeable,mergeStateStatus,labels,baseRefName,body,mergeCommit,mergedAt",
    ]
    if repo:
        args.extend(["--repo", repo])
    result = _gh(*args)
    if not result:
        return None
    try:
        parsed = json.loads(result)
        return parsed if isinstance(parsed, dict) else None
    except json.JSONDecodeError:
        return None


def parse_dependencies(body):
    """Parse dependencies from issue body. Returns set of issue numbers or empty set."""
    import re
    deps = set()
    for match in re.finditer(r"#(\d+)", body):
        num = int(match.group(1))
        start = max(0, match.start() - 30)
        end = min(len(body), match.end() + 30)
        context = body[start:end].lower()
        if any(kw in context for kw in ("depends on", "dependency", "prerequisite", "depends:", "blocked by")):
            deps.add(num)
        elif "needs" in context and re.search(r"needs\s+#\d+", context):
            deps.add(num)
        elif "must" in context and re.search(r"#\d+\s+must", context):
            deps.add(num)
    return deps


def check_dependencies_complete(issue_number, repo=""):
    body = get_issue_body(issue_number, repo)
    if body is None:
        return False, "dependency_state_unavailable"
    deps = parse_dependencies(body)
    for dep in deps:
        labels = get_issue_labels_checked(dep, repo)
        if labels is None:
            return False, "dependency_state_unavailable"
        if LABEL_COMPLETE not in labels:
            return False, dep
    return True, None


def record_worker_state(issue_number, pr_number, head_sha, worker_type, extra=None, repo=""):
    state = WorkerState(pr_number, head_sha, worker_type, extra or {}).to_wire()
    body = json.dumps(state)
    return comment_on_issue(issue_number, body, repo)


def read_worker_state(issue_number, repo=""):
    """Read the most recent worker state from Issue comments."""
    body = get_issue_comment_bodies(issue_number, "agent-orchestrator-state", repo)
    if not body:
        return None
    try:
        state = json.loads(body)
    except json.JSONDecodeError:
        return None
    return state if isinstance(state, dict) and state.get("kind") == "agent-orchestrator-state" else None


def record_ci_state(issue_number, pr_number, head_sha, ci_run_id, status, extra=None, repo=""):
    extra = extra or {}
    state = CIState(
        pr_number,
        head_sha,
        int(ci_run_id) if str(ci_run_id).isdigit() else ci_run_id,
        extra.get("workflow_name", "tests"),
        extra.get("required_jobs", []),
        extra.get("successful_jobs", []),
        status,
        extra,
    ).to_wire()
    return comment_on_issue(issue_number, json.dumps(state), repo)


def record_ci_terminal_state(
    issue_number,
    pr_number,
    head_sha,
    ci_run_id,
    terminal_status,
    observed_status,
    reason,
    repo="",
    extra=None,
):
    """Persist bounded terminal CI evidence idempotently."""

    if (
        not isinstance(terminal_status, str)
        or re.fullmatch(r"terminal_[a-z0-9_]+", terminal_status) is None
        or not isinstance(observed_status, str)
        or re.fullmatch(r"[a-z0-9_]{1,40}", observed_status) is None
        or not isinstance(reason, str)
        or re.fullmatch(r"[a-z0-9_:.()/-]{1,240}", reason) is None
    ):
        return False

    details = dict(extra or {})
    details.update({
        "issue_number": int(issue_number),
        "observed_status": str(observed_status),
        "reason": str(reason),
    })
    state = CIState(
        int(pr_number),
        head_sha,
        int(ci_run_id) if str(ci_run_id).isdigit() else ci_run_id,
        details.get("workflow_name", "tests"),
        details.get("required_jobs", []),
        details.get("successful_jobs", []),
        terminal_status,
        details,
    ).to_wire()
    try:
        comments = get_issue_comments(issue_number, repo)
    except StateUnavailableError:
        return False
    for comment in comments:
        if (comment.get("author") or {}).get("login") not in TRUSTED_STATE_AUTHORS:
            continue
        try:
            previous = json.loads(comment.get("body", ""))
        except (json.JSONDecodeError, TypeError):
            continue
        if previous == state:
            return True
    return comment_on_issue(issue_number, json.dumps(state, sort_keys=True), repo)


def release_and_record_ci_terminal(
    issue_number,
    pr_number,
    head_sha,
    ci_run_id,
    terminal_status,
    reason,
    observed_status,
    repo="",
):
    """Record terminal intent, release matching capacity, and finalize evidence."""

    if (
        not isinstance(terminal_status, str)
        or re.fullmatch(r"terminal_[a-z0-9_]+", terminal_status) is None
        or not isinstance(reason, str)
        or re.fullmatch(r"[a-z0-9_:.()/-]{1,240}", reason) is None
        or not isinstance(observed_status, str)
        or re.fullmatch(r"[a-z0-9_]{1,40}", observed_status) is None
    ):
        return False, "terminal_resolution_evidence_invalid"

    state = CITerminalResolutionState(
        int(issue_number),
        int(pr_number),
        head_sha,
        int(ci_run_id),
        terminal_status,
        reason,
        observed_status,
        "pending",
        "release_pending",
    ).to_wire()
    try:
        comments = get_issue_comments(issue_number, repo)
    except StateUnavailableError:
        return False, "terminal_resolution_state_unavailable"
    pending_found = False
    for comment in comments:
        if (comment.get("author") or {}).get("login") not in TRUSTED_STATE_AUTHORS:
            continue
        try:
            previous = json.loads(comment.get("body", ""))
        except (json.JSONDecodeError, TypeError):
            continue
        if not isinstance(previous, dict):
            continue
        if (
            previous.get("kind") == state["kind"]
            and previous.get("issue_number") == state["issue_number"]
            and previous.get("pr_number") == state["pr_number"]
            and previous.get("head_sha") == state["head_sha"]
            and previous.get("ci_run_id") == state["ci_run_id"]
            and previous.get("terminal_status") == state["terminal_status"]
            and previous.get("reason") == state["reason"]
            and previous.get("observed_status") == state["observed_status"]
        ):
            previous_outcome = previous.get("capacity_release_outcome")
            if previous_outcome in {"released", "already_released"}:
                return True, "already_recorded"
            state = previous
            pending_found = previous_outcome == "pending"
    if not pending_found:
        if not comment_on_issue(issue_number, json.dumps(state, sort_keys=True), repo):
            return False, "terminal_resolution_write_failed"

    release_ok, release_reason = release_failed_capacity(
        issue_number,
        "any",
        LABEL_BLOCKED,
        expected_sha=head_sha,
        repo=repo,
        expected_pr=pr_number,
        expected_run_id=ci_run_id,
    )
    finalized = CITerminalResolutionState(
        int(issue_number),
        int(pr_number),
        head_sha,
        int(ci_run_id),
        terminal_status,
        reason,
        observed_status,
        "released" if release_ok else "failed",
        release_reason,
    ).to_wire()
    if not comment_on_issue(issue_number, json.dumps(finalized, sort_keys=True), repo):
        return False, "terminal_resolution_write_failed"
    return release_ok, release_reason


def record_ci_acquisition(
    issue_number, pr_number, head_sha, run_id, source, duplicate_run_ids=None,
    repo="", metadata=None,
):
    metadata = metadata or {}
    state = CIAcquisitionState(
        int(pr_number), head_sha, int(run_id), source,
        str(metadata.get("status", "bound")),
        [int(value) for value in (duplicate_run_ids or [])],
        [int(value) for value in metadata.get("observed_run_ids", [])],
        str(metadata.get("selection_reason", "")),
        [int(value) for value in metadata.get("superseded_run_ids", [])],
        [int(value) for value in metadata.get("unsupported_run_ids", [])],
        bool(metadata.get("fallback_dispatched", False)),
    ).to_wire()
    return comment_on_issue(issue_number, json.dumps(state, sort_keys=True), repo)


def read_ci_acquisition(issue_number, pr_number=None, head_sha=None, repo=""):
    comments = get_issue_comments(issue_number, repo)
    for comment in comments:
        if (comment.get("author") or {}).get("login") not in TRUSTED_STATE_AUTHORS:
            continue
        body = comment.get("body", "")
        if "agent-orchestrator-ci-acquisition" not in body:
            continue
        try:
            state = json.loads(body)
        except json.JSONDecodeError:
            continue
        if state.get("kind") != "agent-orchestrator-ci-acquisition":
            continue
        if pr_number is not None and state.get("pr_number") != int(pr_number):
            continue
        if head_sha is not None and state.get("head_sha") != head_sha:
            continue
        return state
    return None


def read_ci_state(issue_number, repo=""):
    """Read the most recent CI state from Issue comments."""
    body = get_issue_comment_bodies(issue_number, "agent-orchestrator-ci-state", repo)
    if not body:
        return None
    try:
        state = json.loads(body)
    except json.JSONDecodeError:
        return None
    return state if isinstance(state, dict) and state.get("kind") == "agent-orchestrator-ci-state" else None


def validate_task_scope(issue_number, repo=""):
    """Validate the canonical scope marker before a task can be dispatched."""

    try:
        import artifact_contract
        scope = artifact_contract.parse_issue_scope(get_issue_body(issue_number, repo))
        return True, scope
    except (RuntimeError, ValueError, TypeError, json.JSONDecodeError) as exc:
        return False, str(exc)


def record_review_state(
    issue_number,
    pr_number,
    head_sha,
    verdict,
    summary,
    repo="",
    blockers=None,
    major_notes=None,
    minor_notes=None,
    artifact_sha256="",
    review_workflow_run_id=None,
):
    """Persist only bounded review evidence already accepted by the validator."""

    state = ReviewState(
        int(issue_number),
        int(pr_number),
        head_sha,
        verdict,
        summary,
        list(blockers or []),
        list(major_notes or []),
        list(minor_notes or []),
        artifact_sha256,
        review_workflow_run_id,
    ).to_wire()
    return comment_on_issue(issue_number, json.dumps(state, sort_keys=True), repo)


def _bounded_review_strings(value, field_name):
    if not isinstance(value, list) or len(value) > 50:
        raise ValueError(f"{field_name} is invalid")
    if not all(isinstance(item, str) and len(item) <= 2000 and "\0" not in item and "\r" not in item for item in value):
        raise ValueError(f"{field_name} is invalid")
    return list(value)


def _load_review_validation_sidecar(path, expected_classification):
    """Read the validator's fixed-size JSON handoff, never raw model output."""

    try:
        raw = Path(path).read_bytes()
    except (OSError, TypeError):
        raise ValueError("validation sidecar is unavailable")
    if not raw or len(raw) > MAX_REVIEW_EVIDENCE_BYTES:
        raise ValueError("validation sidecar exceeds bounds")
    try:
        value = json.loads(raw)
    except (json.JSONDecodeError, UnicodeDecodeError):
        raise ValueError("validation sidecar is invalid")
    if not isinstance(value, dict):
        raise ValueError("validation sidecar is invalid")
    if value.get("kind") != "agent-orchestrator-review-validation" or value.get("version") != 1:
        raise ValueError("validation sidecar identity is invalid")
    if value.get("classification") != expected_classification:
        raise ValueError("validation sidecar classification is invalid")
    return value


def _validated_review_evidence(path, pr_number, head_sha):
    value = _load_review_validation_sidecar(path, "valid_verdict")
    required = {
        "kind", "version", "classification", "pr_number", "reviewed_head_sha",
        "verdict", "summary", "blockers", "major_notes", "minor_notes",
        "artifact_sha256", "review_workflow_run_id",
    }
    if set(value) != required:
        raise ValueError("review validation sidecar fields are invalid")
    if value.get("pr_number") != int(pr_number) or value.get("reviewed_head_sha") != head_sha:
        raise ValueError("review validation sidecar binding is invalid")
    if value.get("verdict") not in REVIEW_VERDICTS:
        raise ValueError("review validation verdict is invalid")
    summary = value.get("summary")
    if not isinstance(summary, str) or not (1 <= len(summary) <= 2000) or "\0" in summary or "\r" in summary:
        raise ValueError("review validation summary is invalid")
    digest = value.get("artifact_sha256")
    if not isinstance(digest, str) or re.fullmatch(r"[0-9a-f]{64}", digest) is None:
        raise ValueError("review validation digest is invalid")
    run_id = value.get("review_workflow_run_id")
    if run_id is not None and (type(run_id) is not int or run_id < 1):
        raise ValueError("review workflow identity is invalid")
    return {
        "verdict": value["verdict"],
        "summary": summary,
        "blockers": _bounded_review_strings(value["blockers"], "blockers"),
        "major_notes": _bounded_review_strings(value["major_notes"], "major_notes"),
        "minor_notes": _bounded_review_strings(value["minor_notes"], "minor_notes"),
        "artifact_sha256": digest,
        "review_workflow_run_id": run_id,
    }


def record_validated_review(issue_number, pr_number, head_sha, sidecar_path, repo=""):
    """Exact-head bind and durably record one schema-valid review verdict."""

    try:
        evidence = _validated_review_evidence(sidecar_path, pr_number, head_sha)
    except ValueError as exc:
        return False, str(exc)
    binding_ok, binding_reason = verify_issue_pr_binding(issue_number, pr_number, head_sha, repo)
    if not binding_ok:
        return False, f"binding_rejected:{binding_reason}"
    try:
        previous = read_review_state(issue_number, repo)
    except StateUnavailableError:
        return False, "review_state_unavailable"
    if previous:
        same = (
            previous.get("issue_number", int(issue_number)) == int(issue_number)
            and previous.get("pr_number") == int(pr_number)
            and previous.get("head_sha") == head_sha
            and previous.get("verdict") == evidence["verdict"]
            and previous.get("summary") == evidence["summary"]
            and previous.get("blockers", []) == evidence["blockers"]
            and previous.get("major_notes", []) == evidence["major_notes"]
            and previous.get("minor_notes", []) == evidence["minor_notes"]
            and previous.get("artifact_sha256", "") == evidence["artifact_sha256"]
        )
        if same:
            return True, "already_recorded"
        if previous.get("head_sha") == head_sha and previous.get("verdict") != "INVALIDATED":
            return False, "conflicting_review_state_exists"
    if not record_review_state(
        issue_number,
        pr_number,
        head_sha,
        evidence["verdict"],
        evidence["summary"],
        repo,
        evidence["blockers"],
        evidence["major_notes"],
        evidence["minor_notes"],
        evidence["artifact_sha256"],
        evidence["review_workflow_run_id"],
    ):
        return False, "review_state_write_failed"
    return True, "recorded"


def record_review_validation_failure(issue_number, pr_number, head_sha, sidecar_path, repo=""):
    """Record a bounded malformed/infrastructure reason without altering a verdict."""

    try:
        value = _load_review_validation_sidecar(sidecar_path, "invalid_artifact")
    except ValueError as exc:
        return False, str(exc)
    required = {
        "kind", "version", "classification", "pr_number", "reviewed_head_sha",
        "failure_code", "artifact_sha256", "review_workflow_run_id",
    }
    if set(value) != required or value.get("pr_number") != int(pr_number) or value.get("reviewed_head_sha") != head_sha:
        return False, "review validation failure binding is invalid"
    failure_code = value.get("failure_code")
    if not isinstance(failure_code, str) or not re.fullmatch(r"[a-z_]+", failure_code):
        return False, "review validation failure code is invalid"
    digest = value.get("artifact_sha256")
    if digest is not None and (not isinstance(digest, str) or re.fullmatch(r"[0-9a-f]{64}", digest) is None):
        return False, "review validation failure digest is invalid"
    run_id = value.get("review_workflow_run_id")
    if run_id is not None and (type(run_id) is not int or run_id < 1):
        return False, "review validation failure workflow identity is invalid"
    binding_ok, binding_reason = verify_issue_pr_binding(issue_number, pr_number, head_sha, repo)
    if not binding_ok:
        return False, f"binding_rejected:{binding_reason}"
    try:
        previous = read_review_state(issue_number, repo)
    except StateUnavailableError:
        return False, "review_state_unavailable"
    if previous and previous.get("pr_number") == int(pr_number) and previous.get("head_sha") == head_sha:
        return True, "newer_or_current_review_state_exists"
    return _record_review_validation_failure(
        issue_number, pr_number, head_sha, failure_code, digest, run_id, repo
    )


def _latest_review_validation_failure(issue_number, pr_number, head_sha, repo=""):
    comments = get_issue_comments(issue_number, repo)
    for comment in comments:
        if (comment.get("author") or {}).get("login") not in TRUSTED_STATE_AUTHORS:
            continue
        try:
            state = json.loads(comment.get("body", ""))
        except (json.JSONDecodeError, TypeError):
            continue
        if (
            state.get("kind") == "agent-orchestrator-review-validation-failure"
            and state.get("pr_number") == int(pr_number)
            and state.get("head_sha") == head_sha
        ):
            return state
    return None


def _record_review_validation_failure(
    issue_number, pr_number, head_sha, failure_code, digest, run_id, repo=""
):
    try:
        previous_failure = _latest_review_validation_failure(
            issue_number, pr_number, head_sha, repo
        )
    except StateUnavailableError:
        return False, "review_state_unavailable"
    if previous_failure:
        return True, "already_recorded"
    state = ReviewValidationFailureState(
        int(issue_number), int(pr_number), head_sha, failure_code, digest, run_id
    ).to_wire()
    if not comment_on_issue(issue_number, json.dumps(state, sort_keys=True), repo):
        return False, "review_validation_failure_write_failed"
    return True, "recorded"


def record_review_infrastructure_failure(issue_number, pr_number, head_sha, repo=""):
    """Record a fixed workflow-failure reason when no validator sidecar exists."""

    binding_ok, binding_reason = verify_issue_pr_binding(issue_number, pr_number, head_sha, repo)
    if not binding_ok:
        return False, f"binding_rejected:{binding_reason}"
    try:
        previous = read_review_state(issue_number, repo)
    except StateUnavailableError:
        return False, "review_state_unavailable"
    if previous and previous.get("pr_number") == int(pr_number) and previous.get("head_sha") == head_sha:
        return True, "newer_or_current_review_state_exists"
    run_value = os.environ.get("GITHUB_RUN_ID", "")
    run_id = int(run_value) if run_value.isdigit() else None
    return _record_review_validation_failure(
        issue_number, pr_number, head_sha, "workflow_infrastructure_failure", None, run_id, repo
    )


def invalidate_evidence(issue_number, pr_number, new_head_sha, old_head_sha, repo=""):
    """Bind explicit non-authorizing CI/review state to a newly pushed head."""

    previous_ci = read_ci_state(issue_number, repo) or {}
    previous_run = previous_ci.get("workflow_run_id") or previous_ci.get("ci_run_id") or 0
    if not record_ci_state(
        issue_number, pr_number, new_head_sha, previous_run, "invalidated",
        extra={"invalidated_head": old_head_sha, "reason": "new_head"}, repo=repo,
    ):
        return False
    return record_review_state(
        issue_number, pr_number, new_head_sha, "INVALIDATED",
        f"prior evidence for head {old_head_sha} was invalidated by new head {new_head_sha}", repo,
    )


def record_merge_state(issue_number, pr_number, expected_head_sha, merge_commit_sha, repo=""):
    state = MergeState(
        int(issue_number), int(pr_number), expected_head_sha, merge_commit_sha, "confirmed"
    ).to_wire()
    return comment_on_issue(issue_number, json.dumps(state, sort_keys=True), repo)


def read_review_state(issue_number, repo=""):
    """Read the most recent review state from Issue comments."""
    body = get_issue_comment_bodies(issue_number, "agent-orchestrator-review-state", repo)
    if not body:
        return None
    try:
        state = json.loads(body)
    except json.JSONDecodeError:
        return None
    return state if isinstance(state, dict) and state.get("kind") == "agent-orchestrator-review-state" else None


def get_active_workers(repo=""):
    args = ["issue", "list", "--label", LABEL_RUNNING, "--state", "open", "--json", "number", "--limit", "100"]
    if repo:
        args.extend(["--repo", repo])
    result = _gh(*args)
    if not result:
        return []
    try:
        return [item["number"] for item in json.loads(result)]
    except (json.JSONDecodeError, KeyError):
        return []


def get_ci_repairing_issues(repo=""):
    args = ["issue", "list", "--label", LABEL_CI_REPAIRING, "--state", "open", "--json", "number", "--limit", "100"]
    if repo:
        args.extend(["--repo", repo])
    result = _gh(*args)
    if not result:
        return []
    try:
        return [item["number"] for item in json.loads(result)]
    except (json.JSONDecodeError, KeyError):
        return []


def get_review_running_issues(repo=""):
    args = ["issue", "list", "--label", LABEL_REVIEW_RUNNING, "--state", "open", "--json", "number", "--limit", "100"]
    if repo:
        args.extend(["--repo", repo])
    result = _gh(*args)
    if not result:
        return []
    try:
        return [item["number"] for item in json.loads(result)]
    except (json.JSONDecodeError, KeyError):
        return []


def get_active_issue_numbers(repo=""):
    """Return the authoritative union of active Issue labels, or None on API failure."""
    result = set()
    for label in (LABEL_RUNNING, LABEL_CI_REPAIRING, LABEL_REVIEW_RUNNING):
        args = ["issue", "list", "--label", label, "--state", "open", "--json", "number", "--limit", "100"]
        if repo:
            args.extend(["--repo", repo])
        raw = _gh(*args)
        if raw is None:
            return None
        try:
            result.update(int(item["number"]) for item in json.loads(raw))
        except (json.JSONDecodeError, KeyError, TypeError, ValueError):
            return None
    return result


def has_open_issue_pr(issue_number, repo=""):
    """Return whether the canonical Issue branch already has an open PR.

    ``None`` means the query failed; callers must not treat that as proof that
    the Issue is unassociated.
    """
    args = [
        "pr", "list", "--state", "open", "--head", f"agent/issue-{int(issue_number)}",
        "--limit", "100", "--json", "number,headRefName",
    ]
    if repo:
        args.extend(["--repo", repo])
    raw = _gh(*args)
    if raw is None:
        return None
    try:
        candidates = json.loads(raw)
    except (json.JSONDecodeError, TypeError):
        return None
    return any(item.get("headRefName") == f"agent/issue-{int(issue_number)}" for item in candidates)


def record_dispatch_state(issue_number, dispatch_id, action, status, details=None, repo=""):
    try:
        existing = read_dispatch_state(issue_number, dispatch_id, repo)
    except StateUnavailableError:
        return False
    if existing and existing.get("status") == status and existing.get("action") == action:
        return True
    state = DispatchState(
        int(issue_number), dispatch_id, action, status, details or {}
    ).to_wire()
    return comment_on_issue(issue_number, json.dumps(state, sort_keys=True), repo)


def read_dispatch_state(issue_number, dispatch_id=None, repo=""):
    comments = get_issue_comments(issue_number, repo)
    for comment in comments:
        if (comment.get("author") or {}).get("login") not in TRUSTED_STATE_AUTHORS:
            continue
        body = comment.get("body", "")
        if "agent-orchestrator-dispatch-state" not in body:
            continue
        try:
            state = json.loads(body)
        except json.JSONDecodeError:
            continue
        if state.get("kind") != "agent-orchestrator-dispatch-state":
            continue
        if dispatch_id is None or state.get("dispatch_id") == dispatch_id:
            return state
    return None


def parse_binding_marker(body):
    match = re.search(r"<!-- agent-orchestrator-binding:\s*(\{.*?\})\s*-->", body or "", re.DOTALL)
    if not match:
        return None
    try:
        value = json.loads(match.group(1))
    except json.JSONDecodeError:
        return None
    return value if isinstance(value, dict) else None


def verify_issue_pr_binding(issue_number, pr_number, expected_sha, repo=""):
    """Verify the durable worker binding and reject ambiguous open PR associations."""
    pr = get_pr_info(pr_number, repo)
    if not pr:
        return False, "pr_not_found"
    expected_branch = f"agent/issue-{issue_number}"
    if pr.get("headRefName") != expected_branch:
        return False, "branch_mismatch"
    if pr.get("headRefOid") != expected_sha:
        return False, "head_mismatch"
    body = pr.get("body", "")
    marker = parse_binding_marker(body)
    if not marker or marker.get("issue_number") != int(issue_number) or marker.get("branch") != expected_branch:
        return False, "binding_marker_mismatch"
    if not re.search(rf"(?:Closes|Fixes|Resolves|Implements)\s+#?{int(issue_number)}\b", body, re.IGNORECASE):
        return False, "missing_issue_link"
    linked_issues = {
        int(match.group(1))
        for match in re.finditer(
            r"(?:Closes|Fixes|Resolves|Implements)\s+#?(\d+)\b", body, re.IGNORECASE
        )
    }
    for linked_issue in linked_issues - {int(issue_number)}:
        linked_labels = get_issue_labels_checked(linked_issue, repo)
        if linked_labels is None:
            return False, "linked_issue_state_unavailable"
        if linked_labels & ACTIVE_LABELS:
            return False, "pr_has_another_active_issue"
    try:
        worker = read_worker_state(issue_number, repo)
    except StateUnavailableError:
        return False, "worker_state_unavailable"
    if not worker or worker.get("pr_number") != int(pr_number) or worker.get("head_sha") != expected_sha:
        return False, "worker_state_mismatch"
    if worker.get("extra", {}).get("branch") not in (None, expected_branch):
        return False, "worker_branch_history_mismatch"

    args = ["pr", "list", "--state", "open", "--limit", "100", "--json", "number,headRefName,body,headRefOid"]
    if repo:
        args.extend(["--repo", repo])
    raw = _gh(*args)
    if raw is None:
        return False, "open_pr_query_failed"
    try:
        prs = json.loads(raw)
    except json.JSONDecodeError:
        return False, "open_pr_query_invalid"
    for candidate in prs:
        number = int(candidate.get("number", 0))
        if number == int(pr_number):
            continue
        candidate_marker = parse_binding_marker(candidate.get("body", ""))
        if (
            candidate.get("headRefName") == expected_branch
            or (candidate_marker and candidate_marker.get("issue_number") == int(issue_number))
        ):
            return False, "issue_has_another_active_pr"
    return True, "ok"


def release_failed_capacity(
    issue_number,
    expected_active,
    terminal_label,
    expected_sha=None,
    repo="",
    failed_run_id=None,
    repair_attempt=None,
    expected_pr=None,
    expected_run_id=None,
):
    """Idempotently release only the current failed workflow's active capacity."""

    if expected_active != "any" and expected_active not in ACTIVE_LABELS:
        return False, "invalid_expected_active"
    if terminal_label not in {LABEL_BLOCKED, LABEL_REVIEW_BLOCKED}:
        return False, "invalid_terminal_label"
    labels = get_issue_labels_checked(issue_number, repo)
    if labels is None:
        return False, "label_state_unavailable"
    active = labels & ACTIVE_LABELS
    if expected_active == "any":
        if not active and terminal_label in labels:
            return True, "already_released"
        if len(active) != 1:
            return False, "active_state_mismatch"
    elif active != {expected_active}:
        if terminal_label in labels and not active:
            return True, "already_released"
        return False, "active_state_mismatch"
    if expected_run_id is not None and active in (
        {LABEL_REVIEW_RUNNING},
        {LABEL_CI_REPAIRING},
    ):
        return False, "ci_active_phase_mismatch"
    if labels & (TERMINAL_LABELS | {LABEL_REVIEW_PASSED, LABEL_MERGE_READY}):
        return False, "newer_terminal_state_exists"
    if expected_sha:
        try:
            worker = read_worker_state(issue_number, repo)
        except StateUnavailableError:
            return False, "worker_state_unavailable"
        same_head = worker and worker.get("head_sha") == expected_sha
        extra = (worker or {}).get("extra", {})
        same_repair = (
            expected_active == LABEL_CI_REPAIRING
            and failed_run_id is not None
            and repair_attempt is not None
            and str(extra.get("failed_run_id")) == str(failed_run_id)
            and str(extra.get("repair_attempt")) == str(repair_attempt)
        )
        if not same_head and not same_repair:
            return False, "worker_head_mismatch"
    if expected_pr is not None:
        try:
            worker = read_worker_state(issue_number, repo)
        except StateUnavailableError:
            return False, "worker_state_unavailable"
        if not worker or worker.get("pr_number") != int(expected_pr):
            return False, "worker_pr_mismatch"
    if expected_run_id is not None:
        try:
            ci_state = read_ci_state(issue_number, repo)
        except StateUnavailableError:
            return False, "ci_state_unavailable"
        state_run_id = (ci_state or {}).get("workflow_run_id") or (ci_state or {}).get("ci_run_id")
        if state_run_id is None or str(state_run_id) != str(expected_run_id):
            return False, "ci_run_mismatch"
    # Re-read the state owner immediately before the label mutation.  The
    # earlier checks authorize the evidence; this check prevents a newer
    # active phase observed during the authorization window from being
    # demoted by a stale terminal result.
    latest_labels = get_issue_labels_checked(issue_number, repo)
    if latest_labels is None:
        return False, "label_state_unavailable"
    latest_active = latest_labels & ACTIVE_LABELS
    if latest_active != active:
        return False, "capacity_state_changed"
    if latest_labels & (TERMINAL_LABELS | {LABEL_REVIEW_PASSED, LABEL_MERGE_READY}):
        return False, "newer_terminal_state_exists"
    if not set_labels(issue_number, terminal_label, repo=repo):
        return False, "label_transition_failed"
    return True, "released"


def _workflow_failure_details(workflow_kind, workflow_run_id, repo=""):
    """Return allowlisted run/job/phase evidence without copying logs or names."""

    if workflow_kind not in WORKFLOW_FAILURE_KINDS:
        return "unknown", "workflow", "workflow_identity_invalid"
    if type(workflow_run_id) is not int or workflow_run_id < 1:
        return "unknown", "workflow", "workflow_identity_invalid"
    target = repo or os.environ.get("GITHUB_REPOSITORY", "")
    if re.fullmatch(r"[^/\s]+/[^/\s]+", target) is None:
        return "unknown", "workflow", "workflow_failure_details_unavailable"
    raw = _gh("api", f"repos/{target}/actions/runs/{workflow_run_id}/jobs?per_page=100")
    try:
        value = json.loads(raw) if raw is not None else None
    except json.JSONDecodeError:
        value = None
    if not isinstance(value, dict) or type(value.get("total_count")) is not int:
        return "unknown", "workflow", "workflow_failure_details_unavailable"
    jobs = value.get("jobs")
    if not isinstance(jobs, list) or value["total_count"] > len(jobs) or len(jobs) > 100:
        return "unknown", "workflow", "workflow_failure_details_unavailable"
    allowed = WORKFLOW_JOB_ORDER[workflow_kind]
    indexed = {name: index for index, name in enumerate(allowed)}
    failed = [
        job for job in jobs
        if isinstance(job, dict)
        and job.get("name") in indexed
        and job.get("conclusion") in {
            "failure", "cancelled", "timed_out", "action_required", "startup_failure", "stale"
        }
    ]
    if not failed:
        return "unknown", "workflow", "workflow_failure_details_unavailable"
    job = min(failed, key=lambda item: indexed[item["name"]])
    job_name = job["name"]
    conclusion = job.get("conclusion")
    if conclusion == "cancelled":
        return job_name, JOB_PHASES[job_name], "workflow_cancelled"
    if conclusion == "timed_out":
        return job_name, JOB_PHASES[job_name], "workflow_timeout"
    steps = job.get("steps") if isinstance(job.get("steps"), list) else []
    failed_step = next(
        (
            step.get("name", "") for step in steps
            if isinstance(step, dict)
            and step.get("conclusion") in {"failure", "cancelled", "timed_out"}
            and isinstance(step.get("name"), str)
        ),
        "",
    ).lower()
    mappings = (
        (("no_workspace_changes",), "model_execution", "no_workspace_changes"),
        (("codex",), "model_execution", "model_execution_failure"),
        (("artifact", "validate"), "artifact_validation", "artifact_validation_failure"),
        (("artifact", "upload"), "artifact_upload", "artifact_upload_failure"),
        (("create or update issue-bound pr",), "pr_creation", "pr_creation_failure"),
        (("push branch",), "branch_push", "branch_push_failure"),
        (("worktree",), "worktree", "worktree_failure"),
        (("exact-head ci",), "ci_acquisition", "ci_acquisition_failure"),
    )
    for fragments, phase, reason in mappings:
        if all(fragment in failed_step for fragment in fragments):
            return job_name, phase, reason
    phase = JOB_PHASES[job_name]
    return job_name, phase, f"{phase}_failure"


def _record_workflow_failure(
    issue_number,
    workflow_kind,
    workflow_run_id,
    failed_job,
    failed_phase,
    reason_code,
    release_ok,
    release_reason,
    repo="",
):
    if workflow_kind not in WORKFLOW_FAILURE_KINDS:
        return False, "workflow_kind_invalid"
    if type(workflow_run_id) is not int or workflow_run_id < 1:
        return False, "workflow_run_id_invalid"
    fixed_values = (failed_job, failed_phase, reason_code, release_reason)
    if any(not isinstance(item, str) or re.fullmatch(r"[a-z0-9_-]+", item) is None for item in fixed_values):
        return False, "failure_evidence_invalid"
    state = WorkflowFailureState(
        int(issue_number),
        workflow_kind,
        workflow_run_id,
        failed_job,
        failed_phase,
        reason_code,
        "released" if release_ok else "failed",
        release_reason,
    ).to_wire()
    try:
        comments = get_issue_comments(issue_number, repo)
    except StateUnavailableError:
        return False, "failure_state_unavailable"
    for comment in comments:
        if (comment.get("author") or {}).get("login") not in TRUSTED_STATE_AUTHORS:
            continue
        try:
            previous = json.loads(comment.get("body", ""))
        except (json.JSONDecodeError, TypeError):
            continue
        if (
            previous.get("kind") == "agent-orchestrator-worker-failure"
            and previous.get("workflow_kind") == workflow_kind
            and previous.get("workflow_run_id") == workflow_run_id
            and previous == state
        ):
            return True, "already_recorded"
    if not comment_on_issue(issue_number, json.dumps(state, sort_keys=True), repo):
        return False, "failure_state_write_failed"
    return True, "recorded"


def release_and_record_failure(
    issue_number,
    expected_active,
    terminal_label,
    workflow_kind,
    workflow_run_id,
    expected_sha=None,
    repo="",
    failed_run_id=None,
    repair_attempt=None,
):
    """Release capacity and durably record bounded exact-run failure evidence."""

    release_ok, release_reason = release_failed_capacity(
        issue_number,
        expected_active,
        terminal_label,
        expected_sha,
        repo,
        failed_run_id,
        repair_attempt,
    )
    failed_job, failed_phase, reason_code = _workflow_failure_details(
        workflow_kind, workflow_run_id, repo
    )
    recorded, record_reason = _record_workflow_failure(
        issue_number,
        workflow_kind,
        workflow_run_id,
        failed_job,
        failed_phase,
        reason_code,
        release_ok,
        release_reason,
        repo,
    )
    if not recorded:
        return False, record_reason
    return release_ok, release_reason


def release_rejected_worker(
    issue_number, gate_enabled, validate_result, can_start, repo="", workflow_run_id=None,
    rejection_reason=None,
):
    """Release a dispatcher claim when a worker safely rejects before Vader."""

    rejected = gate_enabled != "true" or (
        validate_result == "success" and can_start != "true"
    )
    if not rejected:
        return True, "worker_not_rejected"
    released, release_reason = release_failed_capacity(
        issue_number, LABEL_RUNNING, LABEL_BLOCKED, repo=repo
    )
    if workflow_run_id is not None:
        reason = rejection_reason if rejection_reason in PREFLIGHT_FAILURE_REASONS else "dispatcher_claim_invalid"
        recorded, record_reason = _record_workflow_failure(
            issue_number,
            "implementation",
            int(workflow_run_id),
            "validate",
            "claim_validation",
            reason,
            released,
            release_reason,
            repo,
        )
        if not recorded:
            return False, record_reason
    return released, release_reason


def _repo_parts(repo):
    target = repo or os.environ.get("AGENT_REPO") or os.environ.get("GITHUB_REPOSITORY", "")
    if target.count("/") != 1:
        raise StateUnavailableError("repository identity is unavailable")
    owner, name = target.split("/", 1)
    if not owner or not name:
        raise StateUnavailableError("repository identity is unavailable")
    return owner, name


def _graphql_review_page(owner, name, pr_number, cursor=None):
    query = (
        "query($owner:String!,$name:String!,$number:Int!,$after:String){"
        "repository(owner:$owner,name:$name){pullRequest(number:$number){"
        "headRefOid reviewDecision reviews(first:100,after:$after){"
        "nodes{id state submittedAt author{login __typename ... on User{id}} commit{oid}}"
        "pageInfo{hasNextPage endCursor}}}}}"
    )
    args = [
        "api", "graphql", "-f", f"query={query}", "-F", f"owner={owner}",
        "-F", f"name={name}", "-F", f"number={int(pr_number)}",
    ]
    if cursor is not None:
        args.extend(["-f", f"after={cursor}"])
    return _gh(*args)


def _parse_review_timestamp(value):
    if not isinstance(value, str) or not value:
        raise StateUnavailableError("review submission time is unavailable")
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00")).timestamp()
    except ValueError as exc:
        raise StateUnavailableError("review submission time is malformed") from exc


def _validated_review_node(node, current_head):
    if not isinstance(node, dict):
        raise StateUnavailableError("review node is malformed")
    review_id = node.get("id")
    state = node.get("state")
    if not isinstance(review_id, str) or not review_id:
        raise StateUnavailableError("review identity is unavailable")
    if state not in {"APPROVED", "CHANGES_REQUESTED", "COMMENTED", "DISMISSED", "PENDING"}:
        raise StateUnavailableError("review state is malformed")
    author = node.get("author")
    if not isinstance(author, dict) or author.get("__typename") != "User":
        raise StateUnavailableError("review author is unavailable or non-human")
    author_id = author.get("id")
    login = author.get("login")
    if not isinstance(author_id, str) or not author_id or not isinstance(login, str) or not login:
        raise StateUnavailableError("review author identity is malformed")
    commit = node.get("commit")
    if state == "PENDING":
        submitted_at = None
        commit_oid = None
    else:
        submitted_at = _parse_review_timestamp(node.get("submittedAt"))
        if not isinstance(commit, dict) or not isinstance(commit.get("oid"), str):
            raise StateUnavailableError("review commit binding is unavailable")
        commit_oid = commit["oid"]
    return {
        "id": review_id,
        "state": state,
        "submitted_at": submitted_at,
        "author_id": author_id,
        "author_login": login,
        "commit_oid": commit_oid,
        "is_current_head": commit_oid == current_head,
    }


def fetch_pr_reviews(pr_number, expected_head_sha, repo=""):
    """Fetch every PR review page with strict identity and pagination checks."""

    owner, name = _repo_parts(repo)
    cursor = None
    seen_cursors = set()
    seen_reviews = {}
    pages = 0
    review_decision = None
    while True:
        if pages >= MAX_REVIEW_API_PAGES:
            raise StateUnavailableError("review page bound exceeded")
        raw = _graphql_review_page(owner, name, pr_number, cursor)
        if raw is None:
            raise StateUnavailableError("review API is unavailable")
        try:
            payload = json.loads(raw)
            if not isinstance(payload, dict) or payload.get("errors"):
                raise ValueError
            pr = payload["data"]["repository"]["pullRequest"]
            connection = pr["reviews"]
            page_info = connection["pageInfo"]
        except (json.JSONDecodeError, KeyError, TypeError, ValueError) as exc:
            raise StateUnavailableError("review API response is malformed") from exc
        if not isinstance(pr, dict) or pr.get("headRefOid") != expected_head_sha:
            raise StateUnavailableError("review API head binding is stale or unavailable")
        decision = pr.get("reviewDecision")
        if decision not in (None, "APPROVED", "CHANGES_REQUESTED", "REVIEW_REQUIRED"):
            raise StateUnavailableError("review decision is malformed")
        if pages and decision != review_decision:
            raise StateUnavailableError("review decision changed during pagination")
        review_decision = decision
        if not isinstance(connection, dict) or not isinstance(connection.get("nodes"), list):
            raise StateUnavailableError("review connection is malformed")
        if not isinstance(page_info, dict) or type(page_info.get("hasNextPage")) is not bool:
            raise StateUnavailableError("review pagination metadata is malformed")
        for raw_node in connection["nodes"]:
            node = _validated_review_node(raw_node, expected_head_sha)
            previous = seen_reviews.get(node["id"])
            if previous is not None:
                if previous != node:
                    raise StateUnavailableError("duplicate review identity is inconsistent")
                continue
            seen_reviews[node["id"]] = node
            if len(seen_reviews) > MAX_REVIEW_API_NODES:
                raise StateUnavailableError("review node bound exceeded")
        pages += 1
        if not page_info["hasNextPage"]:
            end_cursor = page_info.get("endCursor")
            if end_cursor is not None and not isinstance(end_cursor, str):
                raise StateUnavailableError("review pagination cursor is malformed")
            break
        next_cursor = page_info.get("endCursor")
        if not isinstance(next_cursor, str) or not next_cursor or next_cursor in seen_cursors:
            raise StateUnavailableError("review pagination is incomplete")
        seen_cursors.add(next_cursor)
        cursor = next_cursor
    return {
        "complete": True,
        "review_decision": review_decision,
        "reviews": list(seen_reviews.values()),
        "pages": pages,
        "total_reviews": len(seen_reviews),
    }


def current_effective_reviews(pr_number, expected_head_sha, repo=""):
    """Evaluate latest effective human review per reviewer, not raw history."""

    fetched = fetch_pr_reviews(pr_number, expected_head_sha, repo)
    by_reviewer = {}
    for review in fetched["reviews"]:
        if review["state"] == "PENDING":
            continue
        by_reviewer.setdefault(review["author_id"], []).append(review)
    effective = []
    for reviews in by_reviewer.values():
        latest = None
        for review in sorted(reviews, key=lambda item: (item["submitted_at"], item["id"])):
            if review["state"] == "DISMISSED":
                latest = None
            elif review["state"] in {"APPROVED", "CHANGES_REQUESTED"}:
                latest = review
        if latest is not None:
            effective.append(latest)
    effective.sort(key=lambda item: (item["author_id"], item["submitted_at"], item["id"]))
    requested_changes = [item for item in effective if item["state"] == "CHANGES_REQUESTED"]
    decision = fetched["review_decision"]
    if decision == "APPROVED" and requested_changes:
        raise StateUnavailableError("review decision contradicts effective review nodes")
    if decision == "CHANGES_REQUESTED" and not requested_changes:
        raise StateUnavailableError("review decision contradicts effective review nodes")
    if decision == "REVIEW_REQUIRED" and requested_changes:
        raise StateUnavailableError("review decision contradicts effective review nodes")
    return {
        **fetched,
        "effective_reviews": effective,
        "requested_changes": requested_changes,
        "requested_change_review_ids": [item["id"] for item in requested_changes],
        "obsolete_head_requested_change_review_ids": [
            item["id"] for item in requested_changes if not item["is_current_head"]
        ],
        "current_head_requested_change_review_ids": [
            item["id"] for item in requested_changes if item["is_current_head"]
        ],
    }


def review_threads_status(pr_number, expected_head_sha=None, repo=""):
    """Fetch every review-thread page; partial results never authorize merge."""

    # Preserve the former ``review_threads_status(pr, repo)`` call shape for
    # read-only compatibility helpers.  Merge authorization always supplies
    # an exact expected head and therefore cannot use this compatibility path.
    if repo == "" and isinstance(expected_head_sha, str) and "/" in expected_head_sha:
        repo = expected_head_sha
        expected_head_sha = None
    owner, name = _repo_parts(repo)
    query = (
        "query($owner:String!,$name:String!,$number:Int!,$after:String){"
        "repository(owner:$owner,name:$name){pullRequest(number:$number){"
        "headRefOid reviewThreads(first:100,after:$after){nodes{id isResolved}"
        "pageInfo{hasNextPage endCursor}}}}}"
    )
    cursor = None
    seen_cursors = set()
    seen_thread_ids = set()
    unresolved = []
    pages = 0
    while True:
        if pages >= MAX_REVIEW_THREAD_PAGES:
            raise StateUnavailableError("review thread page bound exceeded")
        args = [
            "api", "graphql", "-f", f"query={query}", "-F", f"owner={owner}",
            "-F", f"name={name}", "-F", f"number={int(pr_number)}",
        ]
        if cursor is not None:
            args.extend(["-f", f"after={cursor}"])
        raw = _gh(*args)
        if raw is None:
            raise StateUnavailableError("review thread API is unavailable")
        try:
            payload = json.loads(raw)
            if not isinstance(payload, dict) or payload.get("errors"):
                raise ValueError
            pull_request = payload["data"]["repository"]["pullRequest"]
            connection = pull_request["reviewThreads"]
            nodes = connection["nodes"]
            page_info = connection["pageInfo"]
        except (json.JSONDecodeError, KeyError, TypeError, ValueError) as exc:
            raise StateUnavailableError("review thread API response is malformed") from exc
        if not isinstance(pull_request, dict):
            raise StateUnavailableError("review thread pull request is malformed")
        if expected_head_sha is not None and pull_request.get("headRefOid") != expected_head_sha:
            raise StateUnavailableError("review thread API head binding is stale or unavailable")
        if not isinstance(nodes, list) or not isinstance(page_info, dict) or type(page_info.get("hasNextPage")) is not bool:
            raise StateUnavailableError("review thread pagination metadata is malformed")
        for node in nodes:
            if not isinstance(node, dict) or not isinstance(node.get("id"), str) or not node["id"] or type(node.get("isResolved")) is not bool:
                raise StateUnavailableError("review thread node is malformed")
            if node["id"] in seen_thread_ids:
                raise StateUnavailableError("duplicate review thread identity")
            seen_thread_ids.add(node["id"])
            if len(seen_thread_ids) > MAX_REVIEW_THREADS:
                raise StateUnavailableError("review thread bound exceeded")
            if not node["isResolved"]:
                unresolved.append(node["id"])
        pages += 1
        if not page_info["hasNextPage"]:
            end_cursor = page_info.get("endCursor")
            if end_cursor is not None and not isinstance(end_cursor, str):
                raise StateUnavailableError("review thread cursor is malformed")
            return {
                "complete": True,
                "total_threads": len(seen_thread_ids),
                "unresolved_thread_ids": unresolved,
                "pages": pages,
            }
        next_cursor = page_info.get("endCursor")
        if not isinstance(next_cursor, str) or not next_cursor or next_cursor in seen_cursors:
            raise StateUnavailableError("review thread pagination is incomplete")
        seen_cursors.add(next_cursor)
        cursor = next_cursor


def unresolved_review_threads(pr_number, repo=""):
    """Compatibility helper; callers needing authorization use review_threads_status."""

    try:
        status = review_threads_status(pr_number, repo=repo)
    except StateUnavailableError:
        return None
    return status["unresolved_thread_ids"]


def verify_merge_requirements(pr_number, issue_number, expected_sha, repo=""):
    from ci_verifier import verify_exact_head_ci, CIVerificationError
    import control_state

    try:
        control_state.require_auto_merge(repo or None)
    except (RuntimeError, ValueError) as exc:
        raise RuntimeError(f"control state rejected merge: {exc}") from exc
    labels = get_issue_labels(issue_number, repo)
    if LABEL_REVIEW_PASSED not in labels:
        raise RuntimeError("review-passed label is absent")
    if LABEL_MERGE_READY not in labels:
        raise RuntimeError("agent-merge-ready label is absent")
    if LABEL_REVIEW_RUNNING in labels:
        raise RuntimeError("review-running label remains")
    pr = get_pr_info(pr_number, repo)
    if not pr or pr.get("state") != "OPEN":
        raise RuntimeError("PR is not open")
    if pr.get("baseRefName") != "main" or pr.get("headRefOid") != expected_sha:
        raise RuntimeError("PR target or exact head is invalid")
    if pr.get("mergeable") != "MERGEABLE" or pr.get("mergeStateStatus") != "CLEAN":
        raise RuntimeError("PR is not mergeable under current governance")
    binding_ok, binding_reason = verify_issue_pr_binding(issue_number, pr_number, expected_sha, repo)
    if not binding_ok:
        raise RuntimeError(f"Issue/PR binding rejected: {binding_reason}")
    review = read_review_state(issue_number, repo)
    if not review or review.get("pr_number") != int(pr_number) or review.get("verdict") != "PASS" or review.get("head_sha") != expected_sha:
        raise RuntimeError("review state is missing, mismatched, or not PASS")
    try:
        reviews = current_effective_reviews(pr_number, expected_sha, repo)
    except StateUnavailableError as exc:
        raise RuntimeError(f"current review state is unavailable: {exc}") from exc
    if reviews["review_decision"] == "REVIEW_REQUIRED":
        raise RuntimeError("current GitHub review decision requires review")
    if reviews["requested_changes"]:
        raise RuntimeError("current effective requested-changes review exists")
    try:
        threads = review_threads_status(pr_number, expected_sha, repo)
    except StateUnavailableError as exc:
        raise RuntimeError(f"review thread state is unavailable: {exc}") from exc
    if not threads.get("complete"):
        raise RuntimeError("review thread pagination is incomplete")
    if threads["unresolved_thread_ids"]:
        raise RuntimeError("unresolved review thread exists")
    ci_state = read_ci_state(issue_number, repo)
    if not ci_state or ci_state.get("pr_number") != int(pr_number) or ci_state.get("head_sha") != expected_sha:
        raise RuntimeError("stored CI state is missing or mismatched")
    run_id = ci_state.get("workflow_run_id") or ci_state.get("ci_run_id")
    try:
        evidence = verify_exact_head_ci(pr_number, expected_sha, run_id, pr)
    except CIVerificationError as exc:
        raise RuntimeError(f"exact-head CI rejected: {exc}") from exc
    return evidence


def main():
    """CLI entry point for state operations."""
    if len(sys.argv) < 2:
        print("Usage: state_manager.py <command> [args...]", file=sys.stderr)
        sys.exit(1)

    command = sys.argv[1]
    repo = os.environ.get("AGENT_REPO", "")

    if command == "check-deps":
        issue_number = int(sys.argv[2])
        ok, blocker = check_dependencies_complete(issue_number, repo)
        if not ok:
            print(f"blocked by #{blocker}")
            sys.exit(1)
        print("ok")

    elif command == "get-labels":
        issue_number = int(sys.argv[2])
        labels = get_issue_labels(issue_number, repo)
        print(" ".join(sorted(labels)))

    elif command == "set-labels":
        issue_number = int(sys.argv[2])
        if not set_labels(issue_number, *sys.argv[3:], repo=repo):
            print("FATAL: unable to set Issue labels", file=sys.stderr)
            sys.exit(1)

    elif command == "finalize-review-labels":
        issue_number = int(sys.argv[2])
        verdict = sys.argv[3]
        if not finalize_review_labels(issue_number, verdict, repo):
            print("FATAL: unable to finalize review labels", file=sys.stderr)
            sys.exit(1)

    elif command == "release-failed":
        issue_number = int(sys.argv[2])
        expected_active = sys.argv[3]
        terminal_label = sys.argv[4]
        expected_sha = sys.argv[5] if len(sys.argv) > 5 else None
        failed_run_id = sys.argv[6] if len(sys.argv) > 6 else None
        repair_attempt = sys.argv[7] if len(sys.argv) > 7 else None
        ok, reason = release_failed_capacity(
            issue_number,
            expected_active,
            terminal_label,
            expected_sha,
            repo,
            failed_run_id,
            repair_attempt,
        )
        if not ok:
            print(f"FATAL: unable to release failed capacity: {reason}", file=sys.stderr)
            sys.exit(1)
        print(reason)

    elif command == "release-ci-terminal":
        if len(sys.argv) != 9:
            print(
                "Usage: state_manager.py release-ci-terminal <issue> <pr> <head> <run> <status> <reason> <observed-status>",
                file=sys.stderr,
            )
            sys.exit(1)
        ok, reason = release_and_record_ci_terminal(
            int(sys.argv[2]), int(sys.argv[3]), sys.argv[4], int(sys.argv[5]),
            sys.argv[6], sys.argv[7], sys.argv[8], repo=repo,
        )
        if not ok:
            print(f"FATAL: unable to release and record terminal CI state: {reason}", file=sys.stderr)
            sys.exit(1)
        print(reason)

    elif command == "release-failed-evidence":
        issue_number = int(sys.argv[2])
        expected_active = sys.argv[3]
        terminal_label = sys.argv[4]
        workflow_kind = sys.argv[5]
        workflow_run_id = int(sys.argv[6])
        expected_sha = sys.argv[7] or None
        failed_run_id = sys.argv[8] or None
        repair_attempt = sys.argv[9] or None
        ok, reason = release_and_record_failure(
            issue_number,
            expected_active,
            terminal_label,
            workflow_kind,
            workflow_run_id,
            expected_sha,
            repo,
            failed_run_id,
            repair_attempt,
        )
        if not ok:
            print(f"FATAL: unable to release and record failed capacity: {reason}", file=sys.stderr)
            sys.exit(1)
        print(reason)

    elif command == "release-rejected-worker":
        issue_number = int(sys.argv[2])
        ok, reason = release_rejected_worker(
            issue_number,
            sys.argv[3],
            sys.argv[4],
            sys.argv[5],
            repo,
            int(sys.argv[6]) if len(sys.argv) > 6 and sys.argv[6] else None,
            sys.argv[7] if len(sys.argv) > 7 else None,
        )
        if not ok:
            print(f"FATAL: unable to release rejected worker: {reason}", file=sys.stderr)
            sys.exit(1)
        print(reason)

    elif command == "validate-scope":
        issue_number = int(sys.argv[2])
        valid, value = validate_task_scope(issue_number, repo)
        if not valid:
            print(f"FATAL: invalid task scope: {value}", file=sys.stderr)
            sys.exit(1)
        print(json.dumps({"allowed_paths": value}, sort_keys=True))

    elif command == "add-labels":
        issue_number = int(sys.argv[2])
        if not add_labels(issue_number, *sys.argv[3:], repo=repo):
            print("FATAL: unable to add Issue labels", file=sys.stderr)
            sys.exit(1)

    elif command == "remove-labels":
        issue_number = int(sys.argv[2])
        if not remove_labels(issue_number, *sys.argv[3:], repo=repo):
            print("FATAL: unable to remove Issue labels", file=sys.stderr)
            sys.exit(1)

    elif command == "comment":
        issue_number = int(sys.argv[2])
        body = " ".join(sys.argv[3:])
        if not comment_on_issue(issue_number, body, repo):
            print("FATAL: unable to comment on Issue", file=sys.stderr)
            sys.exit(1)

    elif command == "active-workers":
        workers = get_active_workers(repo)
        print(" ".join(str(w) for w in workers))

    elif command == "parse-deps":
        issue_number = int(sys.argv[2])
        deps = parse_dependencies(get_issue_body(issue_number, repo))
        print(" ".join(str(d) for d in deps))

    elif command == "record-worker":
        issue_number = int(sys.argv[2])
        pr_number = int(sys.argv[3])
        head_sha = sys.argv[4]
        worker_type = sys.argv[5]
        extra = json.loads(sys.argv[6]) if len(sys.argv) > 6 else None
        if not record_worker_state(issue_number, pr_number, head_sha, worker_type, extra=extra, repo=repo):
            print("FATAL: unable to persist worker state", file=sys.stderr)
            sys.exit(1)

    elif command == "read-worker":
        issue_number = int(sys.argv[2])
        state = read_worker_state(issue_number, repo)
        if state:
            print(json.dumps(state))
        else:
            print("null")

    elif command == "record-ci":
        issue_number = int(sys.argv[2])
        pr_number = int(sys.argv[3])
        head_sha = sys.argv[4]
        ci_run_id = sys.argv[5]
        status = sys.argv[6]
        extra = json.loads(sys.argv[7]) if len(sys.argv) > 7 else None
        if not record_ci_state(issue_number, pr_number, head_sha, ci_run_id, status, extra=extra, repo=repo):
            print("FATAL: unable to persist CI state", file=sys.stderr)
            sys.exit(1)

    elif command == "record-ci-acquisition":
        issue_number = int(sys.argv[2])
        pr_number = int(sys.argv[3])
        head_sha = sys.argv[4]
        run_id = sys.argv[5]
        source = sys.argv[6]
        duplicate_ids = json.loads(sys.argv[7]) if len(sys.argv) > 7 else []
        metadata = json.loads(sys.argv[8]) if len(sys.argv) > 8 else {}
        if not record_ci_acquisition(
            issue_number, pr_number, head_sha, run_id, source, duplicate_ids, repo, metadata,
        ):
            print("FATAL: unable to persist CI acquisition", file=sys.stderr)
            sys.exit(1)

    elif command == "read-ci-acquisition":
        issue_number = int(sys.argv[2])
        pr_number = int(sys.argv[3]) if len(sys.argv) > 3 else None
        head_sha = sys.argv[4] if len(sys.argv) > 4 else None
        state = read_ci_acquisition(issue_number, pr_number, head_sha, repo)
        print(json.dumps(state) if state else "null")

    elif command == "record-dispatch":
        issue_number = int(sys.argv[2])
        dispatch_id = sys.argv[3]
        action = sys.argv[4]
        status = sys.argv[5]
        details = json.loads(sys.argv[6]) if len(sys.argv) > 6 else None
        if not record_dispatch_state(issue_number, dispatch_id, action, status, details, repo):
            print("FATAL: unable to persist dispatch state", file=sys.stderr)
            sys.exit(1)

    elif command == "record-review":
        issue_number = int(sys.argv[2])
        pr_number = int(sys.argv[3])
        head_sha = sys.argv[4]
        if len(sys.argv) != 7 or sys.argv[5] != "--evidence-file":
            print(
                "Usage: state_manager.py record-review <issue> <pr> <head> --evidence-file <validated-json>",
                file=sys.stderr,
            )
            sys.exit(1)
        ok, reason = record_validated_review(
            issue_number, pr_number, head_sha, sys.argv[6], repo=repo
        )
        if not ok:
            print(f"FATAL: unable to persist validated review state: {reason}", file=sys.stderr)
            sys.exit(1)
        print(reason)

    elif command == "record-review-failure":
        issue_number = int(sys.argv[2])
        pr_number = int(sys.argv[3])
        head_sha = sys.argv[4]
        if len(sys.argv) != 7 or sys.argv[5] != "--evidence-file":
            print(
                "Usage: state_manager.py record-review-failure <issue> <pr> <head> --evidence-file <validation-json>",
                file=sys.stderr,
            )
            sys.exit(1)
        ok, reason = record_review_validation_failure(
            issue_number, pr_number, head_sha, sys.argv[6], repo=repo
        )
        if not ok:
            print(f"FATAL: unable to persist review validation failure: {reason}", file=sys.stderr)
            sys.exit(1)
        print(reason)

    elif command == "record-review-infrastructure-failure":
        issue_number = int(sys.argv[2])
        pr_number = int(sys.argv[3])
        head_sha = sys.argv[4]
        if len(sys.argv) != 5:
            print(
                "Usage: state_manager.py record-review-infrastructure-failure <issue> <pr> <head>",
                file=sys.stderr,
            )
            sys.exit(1)
        ok, reason = record_review_infrastructure_failure(
            issue_number, pr_number, head_sha, repo=repo
        )
        if not ok:
            print(f"FATAL: unable to persist review infrastructure failure: {reason}", file=sys.stderr)
            sys.exit(1)
        print(reason)

    elif command == "invalidate-evidence":
        issue_number = int(sys.argv[2])
        pr_number = int(sys.argv[3])
        new_head_sha = sys.argv[4]
        old_head_sha = sys.argv[5]
        if not invalidate_evidence(issue_number, pr_number, new_head_sha, old_head_sha, repo):
            print("FATAL: unable to invalidate prior evidence", file=sys.stderr)
            sys.exit(1)

    elif command == "read-review":
        issue_number = int(sys.argv[2])
        state = read_review_state(issue_number, repo)
        if state:
            print(json.dumps(state))
        else:
            print("null")

    elif command == "verify-binding":
        issue_number = int(sys.argv[2])
        pr_number = int(sys.argv[3])
        expected_sha = sys.argv[4]
        ok, reason = verify_issue_pr_binding(issue_number, pr_number, expected_sha, repo)
        if not ok:
            print(f"FATAL: Issue/PR binding rejected: {reason}", file=sys.stderr)
            sys.exit(1)
        print("binding-ok")

    elif command == "record-merge":
        issue_number = int(sys.argv[2])
        pr_number = int(sys.argv[3])
        head_sha = sys.argv[4]
        merge_commit_sha = sys.argv[5] if len(sys.argv) > 5 else ""
        if not record_merge_state(issue_number, pr_number, head_sha, merge_commit_sha, repo=repo):
            print("FATAL: unable to persist merge state", file=sys.stderr)
            sys.exit(1)

    elif command == "verify-merge":
        pr_number = int(sys.argv[2])
        issue_number = int(sys.argv[3])
        expected_sha = sys.argv[4]

        try:
            evidence = verify_merge_requirements(pr_number, issue_number, expected_sha, repo)
        except RuntimeError as exc:
            print(f"FATAL: {exc}", file=sys.stderr)
            sys.exit(1)
        print(json.dumps(evidence, sort_keys=True))

    else:
        print(f"Unknown command: {command}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
