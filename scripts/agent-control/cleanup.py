"""Fail-closed cleanup entry point for orchestrator worktrees.

Worktree ownership is proven by Git's registered metadata and the current
GitHub Issue/workflow state in ``worktree_manager``. This wrapper never force
deletes branches or directories and reports anything that needs manual action.
"""

from __future__ import annotations

import json
import os
import sys

import worktree_manager


def cleanup_orphaned_worktrees(repo_path: str, max_age_hours: int = 24) -> int:
    return worktree_manager.cleanup_stale_worktrees(repo_path, max_age_hours)


def main() -> None:
    repo_path = os.environ.get("AGENT_REPO_PATH", os.getcwd())
    try:
        max_age_hours = int(os.environ.get("AGENT_WORKTREE_MAX_AGE_HOURS", "24"))
    except ValueError as exc:
        print(f"ERROR: invalid worktree age: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
    if max_age_hours < 0:
        print("ERROR: worktree age must be non-negative", file=sys.stderr)
        raise SystemExit(1)
    cleaned = cleanup_orphaned_worktrees(repo_path, max_age_hours)
    print(json.dumps({
        "worktrees_cleaned": cleaned,
        "manual_cleanup": worktree_manager.LAST_CLEANUP_REPORT,
    }, sort_keys=True))


if __name__ == "__main__":
    main()
