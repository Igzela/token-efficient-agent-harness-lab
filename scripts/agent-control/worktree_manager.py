"""Fail-closed git worktree and PR management for the agent orchestrator.

The helper treats Git's registered worktree metadata as the ownership source.
It never removes an unregistered directory and emits JSON at command
boundaries so callers cannot accidentally split values on whitespace.
"""

from __future__ import annotations

import json
import os
import pathlib
import re
import subprocess
import sys
import time
from typing import Any


WORKTREE_BASE = pathlib.Path(os.environ.get("AGENT_WORKTREE_BASE", "/tmp/agent-worktrees"))
ORCHESTRATOR_PREFIXES = ("issue-",)
ORCHESTRATOR_BRANCH_PREFIX = "agent/issue-"
ACTIVE_LABELS = {"agent-running", "ci-repairing", "review-running"}
TERMINAL_LABELS = {"agent-complete", "agent-blocked", "agent-review-blocked"}
LAST_CLEANUP_REPORT: list[dict[str, str]] = []


def _git(*args: str, cwd: str | os.PathLike[str] | None = None) -> str | None:
    result = subprocess.run(
        ["git", *args], capture_output=True, text=True, timeout=60, cwd=cwd
    )
    if result.returncode != 0:
        print(f"git error: {result.stderr.strip()}", file=sys.stderr)
        return None
    return result.stdout.strip()


def _gh(*args: str, cwd: str | os.PathLike[str] | None = None) -> str | None:
    result = subprocess.run(
        ["gh", *args], capture_output=True, text=True, timeout=60, cwd=cwd
    )
    if result.returncode != 0:
        print(f"gh error: {result.stderr.strip()}", file=sys.stderr)
        return None
    return result.stdout.strip()


def _gh_json(*args: str, cwd: str | os.PathLike[str] | None = None) -> Any | None:
    output = _gh(*args, cwd=cwd)
    if output is None:
        return None
    try:
        return json.loads(output)
    except json.JSONDecodeError:
        return None


def _issue_number_from_path(path: pathlib.Path) -> int | None:
    match = re.fullmatch(r"issue-(\d+)", path.name)
    if not match:
        return None
    issue = int(match.group(1))
    return issue if issue > 0 else None


def _is_orchestrator_path(path: str | os.PathLike[str]) -> bool:
    """Require a direct child named exactly ``issue-<positive integer>``."""
    try:
        resolved = pathlib.Path(path).resolve()
        base = WORKTREE_BASE.resolve()
        relative = resolved.relative_to(base)
        return len(relative.parts) == 1 and _issue_number_from_path(resolved) is not None
    except (OSError, ValueError):
        return False


def _worktree_records(repo_path: str | os.PathLike[str]) -> list[dict[str, str]] | None:
    output = _git("worktree", "list", "--porcelain", cwd=repo_path)
    if output is None:
        return None
    records: list[dict[str, str]] = []
    current: dict[str, str] = {}
    for line in output.splitlines() + [""]:
        if not line:
            if current:
                records.append(current)
                current = {}
            continue
        key, _, value = line.partition(" ")
        if key and value:
            current[key] = value.strip()
    return records


def _record_for_path(
    path: pathlib.Path, repo_path: str | os.PathLike[str]
) -> dict[str, str] | None:
    records = _worktree_records(repo_path)
    if records is None:
        return None
    expected = str(path.resolve())
    for record in records:
        if pathlib.Path(record.get("worktree", "")).resolve() == pathlib.Path(expected):
            return record
    return None


def verify_worktree(
    path: str | os.PathLike[str],
    expected_branch: str,
    repo_path: str | os.PathLike[str],
    expected_sha: str | None = None,
) -> bool:
    """Verify path, repository registration, branch, and optionally HEAD."""
    candidate = pathlib.Path(path)
    if not _is_orchestrator_path(candidate) or not candidate.is_dir():
        return False
    repo_root = _git("rev-parse", "--show-toplevel", cwd=repo_path)
    if repo_root is None:
        return False
    record = _record_for_path(candidate, repo_path)
    if not record or record.get("branch") != f"refs/heads/{expected_branch}":
        return False
    if expected_sha is not None and record.get("HEAD") != expected_sha:
        return False
    return True


def _is_orchestrator_worktree(path: str | os.PathLike[str], repo_path: str) -> bool:
    issue = _issue_number_from_path(pathlib.Path(path).resolve())
    return issue is not None and verify_worktree(
        path, f"{ORCHESTRATOR_BRANCH_PREFIX}{issue}", repo_path
    )


def _remote_sha(branch: str, repo_path: str) -> str | None:
    return _git("rev-parse", f"refs/remotes/origin/{branch}", cwd=repo_path)


def create_worktree(
    issue_number: int,
    branch_name: str | None,
    repo_path: str,
    expected_sha: str | None = None,
) -> tuple[str, str, str, str | None] | None:
    branch = branch_name or f"{ORCHESTRATOR_BRANCH_PREFIX}{issue_number}"
    if not re.fullmatch(r"agent/issue-\d+", branch):
        print("FATAL: refusing non-orchestrator branch", file=sys.stderr)
        return None
    worktree_path = WORKTREE_BASE / f"issue-{issue_number}"
    if not _is_orchestrator_path(worktree_path):
        return None

    if _git("fetch", "origin", cwd=repo_path) is None:
        return None
    previous_remote_sha = _remote_sha(branch, repo_path)

    local_branch = _git("rev-parse", "--verify", f"refs/heads/{branch}", cwd=repo_path)
    if local_branch is None:
        source = expected_sha or previous_remote_sha or _git("rev-parse", "origin/main", cwd=repo_path)
        if source is None or _git("branch", branch, source, cwd=repo_path) is None:
            return None
    elif expected_sha is not None and local_branch != expected_sha:
        # A repair must start from the exact remote head. Updating a stale
        # local branch is safe only when Git confirms it is not checked out in
        # another registered worktree; branch -f then fails closed otherwise.
        if _git("branch", "-f", branch, expected_sha, cwd=repo_path) is None:
            print("FATAL: local repair branch cannot be moved to expected SHA", file=sys.stderr)
            return None

    base_sha = expected_sha or _git("rev-parse", branch, cwd=repo_path)
    if base_sha is None:
        return None

    if worktree_path.exists() or worktree_path.is_symlink():
        if verify_worktree(worktree_path, branch, repo_path, expected_sha):
            return str(worktree_path), branch, base_sha, previous_remote_sha
        print(
            f"FATAL: worktree path {worktree_path} exists but ownership cannot be proven; manual cleanup required",
            file=sys.stderr,
        )
        return None

    WORKTREE_BASE.mkdir(parents=True, exist_ok=True)
    if _git("worktree", "add", str(worktree_path), branch, cwd=repo_path) is None:
        return None
    if not verify_worktree(worktree_path, branch, repo_path, expected_sha):
        _git("worktree", "remove", "--force", str(worktree_path), cwd=repo_path)
        print("FATAL: newly created worktree failed registration verification", file=sys.stderr)
        return None
    return str(worktree_path), branch, base_sha, previous_remote_sha


def remove_worktree(issue_number: int, repo_path: str, branch: str | None = None) -> bool:
    worktree_path = WORKTREE_BASE / f"issue-{issue_number}"
    expected_branch = branch or f"{ORCHESTRATOR_BRANCH_PREFIX}{issue_number}"
    if not worktree_path.exists():
        _git("worktree", "prune", cwd=repo_path)
        return True
    if not verify_worktree(worktree_path, expected_branch, repo_path):
        print(
            f"MANUAL CLEANUP REQUIRED: refusing to remove unverified worktree {worktree_path}",
            file=sys.stderr,
        )
        return False
    if _git("worktree", "remove", "--force", str(worktree_path), cwd=repo_path) is None:
        return False
    _git("worktree", "prune", cwd=repo_path)
    if worktree_path.exists():
        print(
            f"MANUAL CLEANUP REQUIRED: git worktree remove left {worktree_path}",
            file=sys.stderr,
        )
        return False
    return True


def _active_issue_or_workflow(issue: int, branch: str) -> bool | None:
    issue_data = _gh_json("issue", "view", str(issue), "--json", "state,labels")
    if issue_data is None:
        return None
    labels = {item.get("name") for item in issue_data.get("labels", [])}
    # Open non-terminal Issues remain protected. A terminal-labelled Issue can
    # be cleaned only after the workflow query below proves no run is active.
    if issue_data.get("state") == "OPEN" and not (
        labels & TERMINAL_LABELS and not labels & ACTIVE_LABELS
    ):
        return True
    runs = _gh_json(
        "run",
        "list",
        "--limit",
        "100",
        "--json",
        "status,headBranch,displayTitle,workflowName",
    )
    if runs is None:
        return None
    for run in runs:
        if run.get("status") not in {"queued", "in_progress"}:
            continue
        if run.get("headBranch") == branch or f"issue-{issue}" in run.get("displayTitle", ""):
            return True
    return False


def cleanup_stale_worktrees(repo_path: str, max_age_hours: int = 24) -> int:
    """Remove only proven stale registered worktrees; otherwise report manual cleanup."""
    global LAST_CLEANUP_REPORT
    LAST_CLEANUP_REPORT = []
    count = 0
    try:
        entries = list(WORKTREE_BASE.iterdir()) if WORKTREE_BASE.exists() else []
    except OSError as exc:
        LAST_CLEANUP_REPORT.append({"path": str(WORKTREE_BASE), "reason": f"cannot_list:{exc}"})
        return 0

    now = time.time()
    for entry in entries:
        try:
            is_directory = entry.is_dir()
        except OSError as exc:
            LAST_CLEANUP_REPORT.append({"path": str(entry), "reason": f"type_check_failed:{exc}"})
            continue
        if not is_directory or not _is_orchestrator_path(entry):
            continue
        issue = _issue_number_from_path(entry)
        if issue is None:
            continue
        try:
            if now - entry.stat().st_mtime <= max_age_hours * 3600:
                continue
        except OSError as exc:
            LAST_CLEANUP_REPORT.append({"path": str(entry), "reason": f"stat_failed:{exc}"})
            continue

        branch = f"{ORCHESTRATOR_BRANCH_PREFIX}{issue}"
        if not verify_worktree(entry, branch, repo_path):
            LAST_CLEANUP_REPORT.append({"path": str(entry), "reason": "unverified_registration_or_branch"})
            continue
        active = _active_issue_or_workflow(issue, branch)
        if active is None:
            LAST_CLEANUP_REPORT.append({"path": str(entry), "reason": "activity_state_unavailable"})
            continue
        if active:
            LAST_CLEANUP_REPORT.append({"path": str(entry), "reason": "active_issue_or_workflow"})
            continue
        if remove_worktree(issue, repo_path, branch):
            count += 1
        else:
            LAST_CLEANUP_REPORT.append({"path": str(entry), "reason": "git_remove_failed"})
    _git("worktree", "prune", cwd=repo_path)
    return count


def main() -> None:
    if len(sys.argv) < 2:
        print("Usage: worktree_manager.py <command> [args...]", file=sys.stderr)
        sys.exit(1)
    command = sys.argv[1]
    repo_path = os.environ.get("AGENT_REPO_PATH", os.getcwd())

    try:
        if command == "create":
            if len(sys.argv) not in {3, 4, 5}:
                raise ValueError("create requires issue, optional branch, optional expected SHA")
            issue = int(sys.argv[2])
            branch = sys.argv[3] if len(sys.argv) >= 4 else None
            expected_sha = sys.argv[4] if len(sys.argv) == 5 else None
            result = create_worktree(issue, branch, repo_path, expected_sha)
            if result is None:
                raise RuntimeError("worktree creation or verification failed")
            path, actual_branch, base_sha, previous_remote_sha = result
            print(json.dumps({
                "worktree_path": path,
                "branch": actual_branch,
                "base_sha": base_sha,
                "previous_remote_sha": previous_remote_sha,
            }, sort_keys=True))
        elif command == "remove":
            if len(sys.argv) not in {3, 4}:
                raise ValueError("remove requires issue and optional branch")
            if not remove_worktree(int(sys.argv[2]), repo_path, sys.argv[3] if len(sys.argv) == 4 else None):
                raise RuntimeError("worktree removal refused or failed")
            print(json.dumps({"removed": True}))
        elif command == "cleanup-stale":
            if len(sys.argv) not in {2, 3}:
                raise ValueError("cleanup-stale accepts optional max age")
            count = cleanup_stale_worktrees(repo_path, int(sys.argv[2]) if len(sys.argv) == 3 else 24)
            print(json.dumps({"cleaned": count, "manual_cleanup": LAST_CLEANUP_REPORT}, sort_keys=True))
        else:
            raise ValueError(f"Unknown command: {command}")
    except (ValueError, RuntimeError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
