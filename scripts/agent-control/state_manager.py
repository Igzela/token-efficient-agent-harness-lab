"""Issue/PR state management for the agent orchestrator.

All state is persisted in GitHub Issues, labels, and Issue comments.
This module never stores state locally -- it reads/writes via the `gh` CLI.
"""

import json
import os
import re
import subprocess
import sys
from pathlib import Path


GH = os.environ.get("AGENT_GH_CMD", "gh")

LABEL_DRAFT = "agent-draft"
LABEL_READY = "agent-ready"
LABEL_RUNNING = "agent-running"
LABEL_CI_REPAIRING = "ci-repairing"
LABEL_REVIEW_RUNNING = "review-running"
LABEL_REVIEW_PASSED = "review-passed"
LABEL_BLOCKED = "agent-blocked"
LABEL_COMPLETE = "agent-complete"

ACTIVE_LABELS = {LABEL_RUNNING, LABEL_CI_REPAIRING, LABEL_REVIEW_RUNNING}
TERMINAL_LABELS = {LABEL_COMPLETE, LABEL_BLOCKED}
ALL_LABELS = ACTIVE_LABELS | TERMINAL_LABELS | {LABEL_DRAFT, LABEL_READY, LABEL_REVIEW_PASSED}

MAX_REPAIR_ATTEMPTS = 2

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
    if repo:
        labels = _gh("issue", "view", str(issue_number), "--repo", repo, "--json", "labels")
    else:
        labels = _gh("issue", "view", str(issue_number), "--json", "labels")
    if not labels:
        return set()
    try:
        parsed = json.loads(labels)
        return {lbl["name"] for lbl in parsed.get("labels", [])}
    except (json.JSONDecodeError, KeyError):
        return set()


def get_issue_body(issue_number, repo=""):
    if repo:
        body = _gh("issue", "view", str(issue_number), "--repo", repo, "--json", "body")
    else:
        body = _gh("issue", "view", str(issue_number), "--json", "body")
    if not body:
        return ""
    try:
        return json.loads(body).get("body", "")
    except json.JSONDecodeError:
        return ""


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
    if not result:
        return []
    try:
        data = json.loads(result)
        comments = data.get("comments", [])
        return list(reversed(comments))
    except json.JSONDecodeError:
        return []


def get_issue_comment_bodies(issue_number, search_text, repo=""):
    """Search Issue comments (not PR comments) for matching text, newest first."""
    comments = get_issue_comments(issue_number, repo)
    for comment in comments:
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
        return json.loads(result)
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
    deps = parse_dependencies(get_issue_body(issue_number, repo))
    for dep in deps:
        labels = get_issue_labels(dep, repo)
        if LABEL_COMPLETE not in labels:
            return False, dep
    return True, None


def record_worker_state(issue_number, pr_number, head_sha, worker_type, extra=None, repo=""):
    state = {
        "kind": "agent-orchestrator-state",
        "version": 1,
        "pr_number": pr_number,
        "head_sha": head_sha,
        "worker_type": worker_type,
        "extra": extra or {},
    }
    body = json.dumps(state)
    return comment_on_issue(issue_number, body, repo)


def read_worker_state(issue_number, repo=""):
    """Read the most recent worker state from Issue comments."""
    body = get_issue_comment_bodies(issue_number, "agent-orchestrator-state", repo)
    if not body:
        return None
    try:
        return json.loads(body)
    except json.JSONDecodeError:
        return None


def record_ci_state(issue_number, pr_number, head_sha, ci_run_id, status, extra=None, repo=""):
    extra = extra or {}
    state = {
        "kind": "agent-orchestrator-ci-state",
        "version": 2,
        "pr_number": pr_number,
        "head_sha": head_sha,
        "workflow_run_id": int(ci_run_id) if str(ci_run_id).isdigit() else ci_run_id,
        "workflow_name": extra.get("workflow_name", "tests"),
        "required_jobs": extra.get("required_jobs", []),
        "successful_jobs": extra.get("successful_jobs", []),
        "status": status,
        "extra": extra,
    }
    return comment_on_issue(issue_number, json.dumps(state), repo)


def read_ci_state(issue_number, repo=""):
    """Read the most recent CI state from Issue comments."""
    body = get_issue_comment_bodies(issue_number, "agent-orchestrator-ci-state", repo)
    if not body:
        return None
    try:
        return json.loads(body)
    except json.JSONDecodeError:
        return None


def record_review_state(issue_number, pr_number, head_sha, verdict, summary, repo=""):
    state = {
        "kind": "agent-orchestrator-review-state",
        "version": 1,
        "pr_number": pr_number,
        "head_sha": head_sha,
        "verdict": verdict,
        "summary": summary,
    }
    return comment_on_issue(issue_number, json.dumps(state), repo)


def record_merge_state(issue_number, pr_number, expected_head_sha, merge_commit_sha, repo=""):
    state = {
        "kind": "agent-orchestrator-merge-state",
        "version": 1,
        "issue_number": int(issue_number),
        "pr_number": int(pr_number),
        "expected_head_sha": expected_head_sha,
        "merge_commit_sha": merge_commit_sha,
        "status": "confirmed",
    }
    return comment_on_issue(issue_number, json.dumps(state, sort_keys=True), repo)


def read_review_state(issue_number, repo=""):
    """Read the most recent review state from Issue comments."""
    body = get_issue_comment_bodies(issue_number, "agent-orchestrator-review-state", repo)
    if not body:
        return None
    try:
        return json.loads(body)
    except json.JSONDecodeError:
        return None


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
    state = {
        "kind": "agent-orchestrator-dispatch-state",
        "version": 1,
        "issue_number": int(issue_number),
        "dispatch_id": dispatch_id,
        "action": action,
        "status": status,
        "details": details or {},
    }
    return comment_on_issue(issue_number, json.dumps(state, sort_keys=True), repo)


def read_dispatch_state(issue_number, dispatch_id=None, repo=""):
    comments = get_issue_comments(issue_number, repo)
    for comment in comments:
        body = comment.get("body", "")
        if "agent-orchestrator-dispatch-state" not in body:
            continue
        try:
            state = json.loads(body)
        except json.JSONDecodeError:
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
        linked_labels = get_issue_labels(linked_issue, repo)
        if linked_labels & ACTIVE_LABELS:
            return False, "pr_has_another_active_issue"
    worker = read_worker_state(issue_number, repo)
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

    elif command == "record-review":
        issue_number = int(sys.argv[2])
        pr_number = int(sys.argv[3])
        head_sha = sys.argv[4]
        verdict = sys.argv[5]
        summary = " ".join(sys.argv[6:]) if len(sys.argv) > 6 else ""
        if not record_review_state(issue_number, pr_number, head_sha, verdict, summary, repo=repo):
            print("FATAL: unable to persist review state", file=sys.stderr)
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

    elif command == "select-task":
        issue_number = int(sys.argv[2])
        labels = get_issue_labels(issue_number, repo)
        if TERMINAL_LABELS & labels:
            print(f"Task #{issue_number} is already in terminal state", file=sys.stderr)
            sys.exit(1)
        if ACTIVE_LABELS & labels:
            print(f"Task #{issue_number} is already active ({labels})", file=sys.stderr)
            sys.exit(1)
        set_labels(issue_number, LABEL_READY, repo=repo)
        print(f"Task #{issue_number} selected as agent-ready")

    elif command == "next-task":
        result = _gh("issue", "list", "--label", LABEL_READY, "--state", "open", "--json", "number", "--jq", ".[0].number")
        if result:
            print(result)
        else:
            print("None")

    elif command == "retry-task":
        issue_number = int(sys.argv[2])
        set_labels(issue_number, LABEL_READY, repo=repo)
        print(f"Task #{issue_number} reset to {LABEL_READY} for retry")

    elif command == "block-task":
        issue_number = int(sys.argv[2])
        reason = " ".join(sys.argv[3:]) if len(sys.argv) > 3 else "Blocked by operator"
        set_labels(issue_number, LABEL_BLOCKED, repo=repo)
        comment_on_issue(issue_number, f"## Agent Orchestrator: Blocked\n**Reason:** {reason}", repo)
        print(f"Task #{issue_number} blocked: {reason}")

    elif command == "status":
        ready = _gh("issue", "list", "--label", LABEL_READY, "--state", "open", "--json", "number")
        running = _gh("issue", "list", "--label", LABEL_RUNNING, "--state", "open", "--json", "number")
        repairing = _gh("issue", "list", "--label", LABEL_CI_REPAIRING, "--state", "open", "--json", "number")
        review_running = _gh("issue", "list", "--label", LABEL_REVIEW_RUNNING, "--state", "open", "--json", "number")
        blocked = _gh("issue", "list", "--label", LABEL_BLOCKED, "--state", "open", "--json", "number")
        complete = _gh("issue", "list", "--label", LABEL_COMPLETE, "--state", "all", "--json", "number")
        try:
            print(f"ready: {len(json.loads(ready))}" if ready else "ready: 0")
            print(f"running: {len(json.loads(running))}" if running else "running: 0")
            print(f"ci-repairing: {len(json.loads(repairing))}" if repairing else "ci-repairing: 0")
            print(f"review-running: {len(json.loads(review_running))}" if review_running else "review-running: 0")
            print(f"blocked: {len(json.loads(blocked))}" if blocked else "blocked: 0")
            print(f"complete: {len(json.loads(complete))}" if complete else "complete: 0")
        except (json.JSONDecodeError, TypeError):
            pass

    else:
        print(f"Unknown command: {command}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
