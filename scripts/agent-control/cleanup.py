"""Cleanup script for abandoned worktrees, stale locks, and orphaned branches.

Can be run manually or on a schedule.
"""

import json
import os
import pathlib
import subprocess
import sys


LOCK_DIR = pathlib.Path("/tmp/agent-orchestrator-locks")
WORKTREE_BASE = pathlib.Path(os.environ.get("AGENT_WORKTREE_BASE", "/tmp/agent-worktrees"))


def _git(*args, cwd=None):
    cmd = ["git"] + list(args)
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=60, cwd=cwd)
        return result.stdout.strip() if result.returncode == 0 else None
    except (subprocess.TimeoutExpired, OSError):
        return None


def _gh(*args):
    cmd = ["gh"] + list(args)
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        return result.stdout.strip() if result.returncode == 0 else None
    except (subprocess.TimeoutExpired, OSError):
        return None


def cleanup_locks():
    count = 0
    if not LOCK_DIR.exists():
        return 0
    for lock_file in LOCK_DIR.iterdir():
        if not lock_file.name.endswith(".lock"):
            continue
        try:
            pid = int(lock_file.read_text().split("\n")[0].strip())
            try:
                os.kill(pid, 0)
            except ProcessLookupError:
                lock_file.unlink()
                count += 1
        except (ValueError, IndexError, OSError):
            lock_file.unlink(missing_ok=True)
            count += 1
    return count


def cleanup_orphaned_branches(repo_path, prefix="agent/"):
    branches = _git("branch", "--list", f"{prefix}*", cwd=repo_path)
    if not branches:
        return 0
    count = 0
    for branch in branches.split("\n"):
        branch = branch.strip().replace("* ", "")
        if not branch:
            continue
        has_pr = _gh("pr", "list", "--head", branch, "--state", "open", "--json", "number")
        if has_pr and has_pr != "[]":
            continue
        _git("branch", "-D", branch, cwd=repo_path)
        count += 1
    return count


def cleanup_orphaned_worktrees(repo_path):
    wt_list = _git("worktree", "list", cwd=repo_path)
    if not wt_list:
        return 0
    count = 0
    repo_path_resolved = pathlib.Path(repo_path).resolve()
    for line in wt_list.split("\n"):
        parts = line.strip().split()
        if not parts:
            continue
        wt_path = pathlib.Path(parts[0])
        if wt_path.resolve() == repo_path_resolved:
            continue
        if not wt_path.exists():
            _git("worktree", "prune", cwd=repo_path)
            count += 1
    return count


def main():
    repo_path = os.environ.get("AGENT_REPO_PATH", os.getcwd())

    locks_cleaned = cleanup_locks()
    branches_cleaned = cleanup_orphaned_branches(repo_path)
    worktrees_cleaned = cleanup_orphaned_worktrees(repo_path)

    result = {
        "locks_cleaned": locks_cleaned,
        "branches_cleaned": branches_cleaned,
        "worktrees_cleaned": worktrees_cleaned,
    }
    print(json.dumps(result))


if __name__ == "__main__":
    main()
