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


DEFERRED_REASONS = frozenset({"active_issue_or_workflow"})


def cleanup_orphaned_worktrees(repo_path: str, max_age_hours: int = 24) -> int:
    return worktree_manager.cleanup_stale_worktrees(repo_path, max_age_hours)


def summarize_cleanup_report(report: list[dict[str, str]]) -> tuple[dict[str, int], dict[str, int]]:
    manual: dict[str, int] = {}
    deferred: dict[str, int] = {}
    for item in report:
        reason = item.get("reason", "unknown").split(":", 1)[0]
        target = deferred if reason in DEFERRED_REASONS else manual
        target[reason] = target.get(reason, 0) + 1
    return manual, deferred


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
    reason_counts, deferred_counts = summarize_cleanup_report(
        worktree_manager.LAST_CLEANUP_REPORT
    )
    print(json.dumps({
        "worktrees_cleaned": cleaned,
        "manual_cleanup_required": bool(reason_counts),
        "manual_cleanup_reason_counts": reason_counts,
        "safe_deferred_reason_counts": deferred_counts,
    }, sort_keys=True))
    if reason_counts:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
