"""Issue/PR state management for the agent orchestrator.

All state is persisted in GitHub Issues, PRs, labels, and comments.
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
LABEL_FINAL_REVIEW = "final-review"
LABEL_BLOCKED = "agent-blocked"
LABEL_COMPLETE = "agent-complete"

ACTIVE_LABELS = {LABEL_RUNNING, LABEL_CI_REPAIRING, LABEL_FINAL_REVIEW}
TERMINAL_LABELS = {LABEL_COMPLETE, LABEL_BLOCKED}
ALL_LABELS = ACTIVE_LABELS | TERMINAL_LABELS | {LABEL_DRAFT, LABEL_READY}

MAX_REPAIR_ATTEMPTS = 2


def _gh(*args, **kwargs):
    input_data = kwargs.get("input_data")
    cmd = [GH] + list(args)
    stdin = None
    if input_data is not None:
        stdin = input_data.encode() if isinstance(input_data, str) else input_data
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            input=input_data if isinstance(input_data, str) else None,
            timeout=30,
        )
        if result.returncode != 0:
            print(f"gh error (exit {result.returncode}): {result.stderr.strip()}", file=sys.stderr)
            return None
        return result.stdout.strip()
    except subprocess.TimeoutExpired:
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
    _gh(*args, input_data="")


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


def get_pr_comment_body(pr_number, search_text, repo=""):
    args = ["pr", "view", str(pr_number), "--json", "comments", "--jq", ".comments[]"]
    if repo:
        args.extend(["--repo", repo])
    result = _gh(*args)
    if not result:
        return None
    try:
        comments = json.loads(f"[{result.replace('}\n{', '},{')}]")
        for comment in comments:
            if search_text in comment.get("body", ""):
                return comment["body"]
    except json.JSONDecodeError:
        pass
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
    body = get_pr_comment_body(issue_number, "agent-orchestrator-state", repo)
    if not body:
        return None
    try:
        return json.loads(body)
    except json.JSONDecodeError:
        return None


def record_ci_state(issue_number, pr_number, head_sha, ci_run_id, status, repo=""):
    state = {
        "kind": "agent-orchestrator-ci-state",
        "version": 1,
        "pr_number": pr_number,
        "head_sha": head_sha,
        "ci_run_id": ci_run_id,
        "status": status,
    }
    comment_on_issue(issue_number, json.dumps(state), repo)


def read_ci_state(issue_number, repo=""):
    body = get_pr_comment_body(issue_number, "agent-orchestrator-ci-state", repo)
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


def get_final_review_issues(repo=""):
    result = _gh("issue", "list", "--label", LABEL_FINAL_REVIEW, "--state", "open", "--json", "number")
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
        record_ci_state(issue_number, pr_number, head_sha, ci_run_id, status, repo)

    elif command == "read-ci":
        issue_number = int(sys.argv[2])
        state = read_ci_state(issue_number, repo)
        if state:
            print(json.dumps(state))
        else:
            print("null")

    else:
        print(f"Unknown command: {command}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
