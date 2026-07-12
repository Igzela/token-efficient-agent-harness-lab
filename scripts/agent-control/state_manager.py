"""Issue/PR state management for the agent orchestrator.

All state is persisted in GitHub Issues, labels, and Issue comments.
This module never stores state locally -- it reads/writes via the `gh` CLI.
"""

import json
import os
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

EMERGENCY_STOP_VAR = "AGENT_EMERGENCY_STOP"


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
    except subprocess.TimeoutExpired:
        print(f"gh timed out: {' '.join(cmd)}", file=sys.stderr)
        return None


def is_emergency_stopped():
    val = os.environ.get(EMERGENCY_STOP_VAR, "false")
    return val.lower() == "true"


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
    _gh(*args)


def remove_labels(issue_number, *labels, repo=""):
    for label in labels:
        args = ["issue", "edit", str(issue_number)]
        if repo:
            args.extend(["--repo", repo])
        args.extend(["--remove-label", label])
        _gh(*args)


def set_labels(issue_number, *labels, repo=""):
    args = ["issue", "edit", str(issue_number)]
    if repo:
        args.extend(["--repo", repo])
    args.extend(["--add-label", ",".join(labels)])
    for label in ALL_LABELS:
        if label not in labels:
            args.extend(["--remove-label", label])
    _gh(*args)


def comment_on_issue(issue_number, body, repo=""):
    args = ["issue", "comment", str(issue_number)]
    if repo:
        args.extend(["--repo", repo])
    args.extend(["--body", body])
    _gh(*args)


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
    args = ["pr", "view", str(pr_number), "--json", "headRefName,headRefOid,state,mergeable,labels,baseRefName"]
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
    comment_on_issue(issue_number, body, repo)


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
    state = {
        "kind": "agent-orchestrator-ci-state",
        "version": 1,
        "pr_number": pr_number,
        "head_sha": head_sha,
        "ci_run_id": ci_run_id,
        "status": status,
        "extra": extra or {},
    }
    comment_on_issue(issue_number, json.dumps(state), repo)


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
    comment_on_issue(issue_number, json.dumps(state), repo)


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
    result = _gh("issue", "list", "--label", LABEL_RUNNING, "--state", "open", "--json", "number")
    if not result:
        return []
    try:
        return [item["number"] for item in json.loads(result)]
    except (json.JSONDecodeError, KeyError):
        return []


def get_ci_repairing_issues(repo=""):
    result = _gh("issue", "list", "--label", LABEL_CI_REPAIRING, "--state", "open", "--json", "number")
    if not result:
        return []
    try:
        return [item["number"] for item in json.loads(result)]
    except (json.JSONDecodeError, KeyError):
        return []


def get_review_running_issues(repo=""):
    result = _gh("issue", "list", "--label", LABEL_REVIEW_RUNNING, "--state", "open", "--json", "number")
    if not result:
        return []
    try:
        return [item["number"] for item in json.loads(result)]
    except (json.JSONDecodeError, KeyError):
        return []


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
        set_labels(issue_number, *sys.argv[3:], repo=repo)

    elif command == "add-labels":
        issue_number = int(sys.argv[2])
        add_labels(issue_number, *sys.argv[3:], repo=repo)

    elif command == "remove-labels":
        issue_number = int(sys.argv[2])
        remove_labels(issue_number, *sys.argv[3:], repo=repo)

    elif command == "comment":
        issue_number = int(sys.argv[2])
        body = " ".join(sys.argv[3:])
        comment_on_issue(issue_number, body, repo)

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
        record_worker_state(issue_number, pr_number, head_sha, worker_type, repo=repo)

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
        record_ci_state(issue_number, pr_number, head_sha, ci_run_id, status, repo=repo)

    elif command == "record-review":
        issue_number = int(sys.argv[2])
        pr_number = int(sys.argv[3])
        head_sha = sys.argv[4]
        verdict = sys.argv[5]
        summary = " ".join(sys.argv[6:]) if len(sys.argv) > 6 else ""
        record_review_state(issue_number, pr_number, head_sha, verdict, summary, repo=repo)

    elif command == "read-review":
        issue_number = int(sys.argv[2])
        state = read_review_state(issue_number, repo)
        if state:
            print(json.dumps(state))
        else:
            print("null")

    elif command == "record-merge":
        issue_number = int(sys.argv[2])
        pr_number = int(sys.argv[3])
        head_sha = sys.argv[4]
        record_worker_state(issue_number, pr_number, head_sha, "merge",
                            extra={"status": "merged"}, repo=repo)

    elif command == "verify-merge":
        pr_number = int(sys.argv[2])
        issue_number = int(sys.argv[3])
        expected_sha = sys.argv[4]

        if is_emergency_stopped():
            print("FATAL: Emergency stop is active. Merge blocked.", file=sys.stderr)
            sys.exit(1)

        labels = get_issue_labels(issue_number, repo)
        if TERMINAL_LABELS & labels:
            print(f"FATAL: Issue #{issue_number} is in terminal state ({labels})", file=sys.stderr)
            sys.exit(1)

        pr = get_pr_info(pr_number, repo)
        if not pr:
            print(f"FATAL: PR #{pr_number} not found", file=sys.stderr)
            sys.exit(1)
        if pr.get("state") != "OPEN":
            print(f"FATAL: PR #{pr_number} is not open (state={pr.get('state')})", file=sys.stderr)
            sys.exit(1)
        if pr.get("baseRefName") != "main":
            print(f"FATAL: PR #{pr_number} does not target main (targets {pr.get('baseRefName')})", file=sys.stderr)
            sys.exit(1)
        actual_sha = pr.get("headRefOid")
        if actual_sha != expected_sha:
            print(f"FATAL: PR head SHA mismatch: expected {expected_sha}, got {actual_sha}", file=sys.stderr)
            sys.exit(1)

        if LABEL_REVIEW_PASSED not in labels:
            print(f"FATAL: Issue #{issue_number} does not have {LABEL_REVIEW_PASSED} label", file=sys.stderr)
            sys.exit(1)

        review_state = read_review_state(issue_number, repo)
        if not review_state:
            print(f"FATAL: No review state found for issue #{issue_number}", file=sys.stderr)
            sys.exit(1)
        if review_state.get("verdict") != "PASS":
            print(f"FATAL: Review verdict is {review_state.get('verdict')}, expected PASS", file=sys.stderr)
            sys.exit(1)
        if review_state.get("head_sha") != expected_sha:
            print(f"FATAL: Review SHA {review_state.get('head_sha')} does not match expected {expected_sha}", file=sys.stderr)
            sys.exit(1)

        print(f"Merge conditions verified: PR #{pr_number} @ {actual_sha}")

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

    elif command == "emergency-stop":
        print("Emergency stop requires setting the AGENT_EMERGENCY_STOP repository variable to 'true'.")
        print("This must be done through the GitHub UI or API, not through this script.")
        sys.exit(0)

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
