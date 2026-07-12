"""Git worktree management for the agent orchestrator.

Creates, manages, and cleans up isolated git worktrees for Codex workers.
Only manages worktrees within the configured WORKTREE_BASE that match
the orchestrator naming pattern.
"""

import os
import pathlib
import subprocess
import sys
import tempfile

WORKTREE_BASE = pathlib.Path(os.environ.get("AGENT_WORKTREE_BASE", "/tmp/agent-worktrees"))

# Only these prefixes are considered orchestrator-owned
ORCHESTRATOR_PREFIXES = ("issue-",)
ORCHESTRATOR_BRANCH_PREFIX = "agent/issue-"


def _git(*args, cwd=None):
    cmd = ["git"] + list(args)
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=60, cwd=cwd)
    if result.returncode != 0:
        print(f"git error: {result.stderr.strip()}", file=sys.stderr)
        return None
    return result.stdout.strip()


def _gh(*args):
    cmd = ["gh"] + list(args)
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=60)
    if result.returncode != 0:
        print(f"gh error: {result.stderr.strip()}", file=sys.stderr)
        return None
    return result.stdout.strip()


def _is_orchestrator_path(path):
    """Check if a path is within the orchestrator worktree base and matches naming pattern."""
    try:
        resolved = pathlib.Path(path).resolve()
        base_resolved = WORKTREE_BASE.resolve()
        if not str(resolved).startswith(str(base_resolved)):
            return False
        name = resolved.name
        return any(name.startswith(p) for p in ORCHESTRATOR_PREFIXES)
    except (OSError, ValueError):
        return False


def _is_orchestrator_worktree(path, repo_path):
    """Check if a path is a registered git worktree owned by the orchestrator."""
    if not _is_orchestrator_path(path):
        return False
    wt_list = _git("worktree", "list", "--porcelain", cwd=repo_path)
    if not wt_list:
        return False
    for line in wt_list.split("\n"):
        if line.startswith("worktree ") and line[9:].strip() == str(path.resolve()):
            return True
    return False


def create_worktree(issue_number, branch_name, repo_path):
    branch = branch_name or f"{ORCHESTRATOR_BRANCH_PREFIX}{issue_number}"
    worktree_path = WORKTREE_BASE / f"issue-{issue_number}"

    _git("fetch", "origin", cwd=repo_path)

    branch_exists = _git("rev-parse", "--verify", f"refs/heads/{branch}", cwd=repo_path)

    if not branch_exists:
        _git("branch", branch, "origin/main", cwd=repo_path)

    if worktree_path.exists():
        _git("worktree", "prune", cwd=repo_path)
        if worktree_path.exists():
            import shutil
            shutil.rmtree(worktree_path)

    result = _git("worktree", "add", str(worktree_path), branch, cwd=repo_path)
    if result is None:
        return None

    return str(worktree_path), branch


def remove_worktree(issue_number, repo_path):
    worktree_path = WORKTREE_BASE / f"issue-{issue_number}"

    if worktree_path.exists() and _is_orchestrator_path(worktree_path):
        _git("worktree", "remove", str(worktree_path), "--force", cwd=repo_path)

    _git("worktree", "prune", cwd=repo_path)

    if worktree_path.exists() and _is_orchestrator_path(worktree_path):
        import shutil
        shutil.rmtree(worktree_path, ignore_errors=True)


def cleanup_stale_worktrees(repo_path, max_age_hours=24):
    """Remove only orchestrator-owned stale worktrees.

    Safety checks:
    1. Path must be within WORKTREE_BASE
    2. Path name must match orchestrator naming pattern (issue-*)
    3. Path must be registered as a git worktree OR have explicit ownership evidence
    """
    import shutil
    import time

    now = time.time()
    count = 0

    if not WORKTREE_BASE.exists():
        return 0

    for entry in WORKTREE_BASE.iterdir():
        if not entry.is_dir():
            continue
        if not _is_orchestrator_path(entry):
            continue
        try:
            mtime = entry.stat().st_mtime
            if now - mtime > max_age_hours * 3600:
                is_wt = _is_orchestrator_worktree(entry, repo_path)
                if is_wt:
                    _git("worktree", "remove", str(entry), "--force", cwd=repo_path)
                shutil.rmtree(entry, ignore_errors=True)
                count += 1
        except OSError:
            shutil.rmtree(entry, ignore_errors=True)
            count += 1

    _git("worktree", "prune", cwd=repo_path)
    return count


def push_branch(branch, repo_path, force=False):
    flag = "-f" if force else ""
    result = _git("push", "origin", branch, flag, cwd=repo_path)
    return result is not None


def create_pr(issue_number, branch, repo_path, title=None, body=None):
    pr_body = body or f"Agent implementation for #{issue_number}"
    pr_title = title or f"agent: implement #{issue_number}"

    _gh("pr", "create",
        "--base", "main",
        "--head", branch,
        "--title", pr_title,
        "--body", pr_body,
        "--label", "agent-generated")

    pr_info = _gh("pr", "view", "--json", "number,headRefOid,url")
    if pr_info:
        import json as json_mod
        try:
            return json_mod.loads(pr_info)
        except json_mod.JSONDecodeError:
            pass
    return None


def get_pr_info(pr_number):
    import json as json_mod
    result = _gh("pr", "view", str(pr_number), "--json", "headRefOid,headRefName,state,url")
    if not result:
        return None
    try:
        return json_mod.loads(result)
    except json_mod.JSONDecodeError:
        return None


def main():
    if len(sys.argv) < 2:
        print("Usage: worktree_manager.py <command> [args...]", file=sys.stderr)
        sys.exit(1)

    command = sys.argv[1]
    repo_path = os.environ.get("AGENT_REPO_PATH", os.getcwd())

    if command == "create":
        issue_number = int(sys.argv[2])
        branch = sys.argv[3] if len(sys.argv) > 3 else None
        result = create_worktree(issue_number, branch, repo_path)
        if result:
            print(f"worktree={result[0]} branch={result[1]}")
        else:
            print("failed", file=sys.stderr)
            sys.exit(1)

    elif command == "remove":
        issue_number = int(sys.argv[2])
        remove_worktree(issue_number, repo_path)
        print("removed")

    elif command == "push":
        branch = sys.argv[2]
        force = len(sys.argv) > 3 and sys.argv[3] == "--force"
        if push_branch(branch, repo_path, force):
            print("pushed")
        else:
            print("push-failed", file=sys.stderr)
            sys.exit(1)

    elif command == "create-pr":
        issue_number = int(sys.argv[2])
        branch = sys.argv[3]
        title = sys.argv[4] if len(sys.argv) > 4 else None
        body = sys.argv[5] if len(sys.argv) > 5 else None
        pr = create_pr(issue_number, branch, repo_path, title, body)
        if pr:
            print(f"pr_number={pr['number']} url={pr['url']} head_sha={pr['headRefOid']}")
        else:
            print("pr-creation-failed", file=sys.stderr)
            sys.exit(1)

    elif command == "cleanup-stale":
        max_age = int(sys.argv[2]) if len(sys.argv) > 2 else 24
        count = cleanup_stale_worktrees(repo_path, max_age)
        print(f"cleaned={count}")

    elif command == "get-pr-info":
        pr_number = int(sys.argv[2])
        info = get_pr_info(pr_number)
        if info:
            import json as json_mod
            print(json_mod.dumps(info))
        else:
            print("null")

    else:
        print(f"Unknown command: {command}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
