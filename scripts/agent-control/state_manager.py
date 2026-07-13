"""Issue/PR state management for the agent orchestrator.

All state is persisted in GitHub Issues, labels, and Issue comments.
This module never stores state locally -- it reads/writes via the `gh` CLI.
"""

import json
import os
import re
import subprocess
import sys
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
    pr_number: int
    head_sha: str
    verdict: str
    summary: str

    def to_wire(self):
        return _state_wire("agent-orchestrator-review-state", 1, self)


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

    if verdict == "PASS":
        return {LABEL_REVIEW_PASSED, LABEL_MERGE_READY}
    return {LABEL_REVIEW_BLOCKED}


def finalize_review_labels(issue_number, verdict, repo=""):
    """Release review capacity into the verdict's non-active state."""

    return set_labels(issue_number, *sorted(labels_for_review_verdict(verdict)), repo=repo)


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
        "headRefName,headRefOid,state,mergeable,mergeStateStatus,labels,baseRefName,body,reviews,reviewDecision,mergeCommit,mergedAt",
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


def record_ci_acquisition(
    issue_number, pr_number, head_sha, run_id, source, duplicate_run_ids=None,
    repo="", metadata=None,
):
    metadata = metadata or {}
    state = CIAcquisitionState(
        int(pr_number), head_sha, int(run_id), source, "bound",
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


def record_review_state(issue_number, pr_number, head_sha, verdict, summary, repo=""):
    state = ReviewState(pr_number, head_sha, verdict, summary).to_wire()
    return comment_on_issue(issue_number, json.dumps(state), repo)


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
        if len(active) != 1:
            return False, "active_state_mismatch"
    elif active != {expected_active}:
        if terminal_label in labels and not active:
            return True, "already_released"
        return False, "active_state_mismatch"
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
    if not set_labels(issue_number, terminal_label, repo=repo):
        return False, "label_transition_failed"
    return True, "released"


def release_rejected_worker(
    issue_number, gate_enabled, validate_result, can_start, repo=""
):
    """Release a dispatcher claim when a worker safely rejects before Vader."""

    rejected = gate_enabled != "true" or (
        validate_result == "success" and can_start != "true"
    )
    if not rejected:
        return True, "worker_not_rejected"
    return release_failed_capacity(
        issue_number, LABEL_RUNNING, LABEL_BLOCKED, repo=repo
    )


def unresolved_review_threads(pr_number, repo=""):
    target = repo or os.environ.get("AGENT_REPO") or os.environ.get("GITHUB_REPOSITORY", "")
    if "/" not in target:
        return None
    owner, name = target.split("/", 1)
    query = (
        "query($owner:String!, $name:String!, $number:Int!) {"
        "repository(owner:$owner,name:$name){pullRequest(number:$number){"
        "reviewThreads(first:100){nodes{isResolved}}}}}"
    )
    raw = _gh(
        "api", "graphql", "-f", f"query={query}", "-F", f"owner={owner}",
        "-F", f"name={name}", "-F", f"number={int(pr_number)}",
    )
    if raw is None:
        return None
    try:
        data = json.loads(raw)
        nodes = data["data"]["repository"]["pullRequest"]["reviewThreads"]["nodes"]
        return [node for node in nodes if not node.get("isResolved", False)]
    except (json.JSONDecodeError, KeyError, TypeError):
        return None


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
    if any(item.get("state") == "CHANGES_REQUESTED" for item in pr.get("reviews", [])):
        raise RuntimeError("active requested-changes review exists")
    threads = unresolved_review_threads(pr_number, repo)
    if threads is None:
        raise RuntimeError("review thread state is unavailable")
    if threads:
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

    elif command == "release-rejected-worker":
        issue_number = int(sys.argv[2])
        ok, reason = release_rejected_worker(
            issue_number, sys.argv[3], sys.argv[4], sys.argv[5], repo
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
        verdict = sys.argv[5]
        summary = " ".join(sys.argv[6:]) if len(sys.argv) > 6 else ""
        if not record_review_state(issue_number, pr_number, head_sha, verdict, summary, repo=repo):
            print("FATAL: unable to persist review state", file=sys.stderr)
            sys.exit(1)

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
