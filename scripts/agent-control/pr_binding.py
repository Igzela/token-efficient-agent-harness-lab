"""Deterministic create/update and post-push verification for Issue-bound PRs."""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import time
from pathlib import Path
from typing import Any

import state_manager


class PRBindingError(RuntimeError):
    """Raised when an Issue cannot be bound to exactly one safe PR."""


def _repo(repo: str | None = None) -> str:
    target = repo or os.environ.get("AGENT_REPO") or os.environ.get("GITHUB_REPOSITORY", "")
    if "/" not in target:
        raise PRBindingError("repository is unavailable")
    return target


def _gh(*args: str) -> str:
    executable = os.environ.get("AGENT_GH_CMD", "gh")
    result = subprocess.run([executable, *args], capture_output=True, text=True, timeout=60)
    if result.returncode != 0:
        raise PRBindingError(result.stderr.strip() or "GitHub CLI command failed")
    return result.stdout.strip()


def _gh_json(*args: str) -> Any:
    raw = _gh(*args)
    try:
        return json.loads(raw)
    except json.JSONDecodeError as exc:
        raise PRBindingError("GitHub CLI returned invalid JSON") from exc


def _open_prs(repo: str) -> list[dict[str, Any]]:
    data = _gh_json(
        "pr", "list", "--repo", repo, "--state", "open", "--limit", "100",
        "--json", "number,headRefName,headRefOid,state,baseRefName,body,url",
    )
    if not isinstance(data, list):
        raise PRBindingError("open PR response was not a list")
    return [item for item in data if isinstance(item, dict)]


def _candidate_prs(prs: list[dict[str, Any]], issue_number: int, branch: str) -> list[dict[str, Any]]:
    candidates: list[dict[str, Any]] = []
    for pr in prs:
        marker = state_manager.parse_binding_marker(pr.get("body", ""))
        if pr.get("headRefName") == branch or (
            marker and marker.get("issue_number") == issue_number
        ):
            candidates.append(pr)
    return candidates


def _verify_pr(
    pr: dict[str, Any], issue_number: int, branch: str, expected_sha: str, prs: list[dict[str, Any]]
) -> dict[str, Any]:
    number = pr.get("number")
    if not isinstance(number, int):
        raise PRBindingError("PR number is missing")
    state = str(pr.get("state", "")).upper()
    if state not in {"OPEN", ""}:
        raise PRBindingError("bound PR is not open")
    if pr.get("baseRefName") != "main":
        raise PRBindingError("bound PR does not target main")
    if pr.get("headRefName") != branch or pr.get("headRefOid") != expected_sha:
        raise PRBindingError("bound PR branch or head does not match")
    marker = state_manager.parse_binding_marker(pr.get("body", ""))
    if not marker or marker.get("issue_number") != issue_number or marker.get("branch") != branch:
        raise PRBindingError("bound PR Issue marker is invalid")
    if not re.search(rf"(?:Closes|Fixes|Resolves|Implements)\s+#?{issue_number}\b", pr.get("body", ""), re.I):
        raise PRBindingError("bound PR lacks the Issue closing link")
    competitors = [item for item in _candidate_prs(prs, issue_number, branch) if item.get("number") != number]
    if competitors:
        raise PRBindingError("multiple open PRs are bound to the Issue branch")
    return {"number": number, "head_sha": expected_sha, "url": pr.get("url", "")}


def _view_pr(repo: str, number: int) -> dict[str, Any]:
    data = _gh_json(
        "pr", "view", str(number), "--repo", repo,
        "--json", "number,state,baseRefName,headRefName,headRefOid,body,url",
    )
    if not isinstance(data, dict):
        raise PRBindingError("PR view response was not an object")
    return data


def create_or_update_pr(
    issue_number: int,
    branch: str,
    expected_sha: str,
    title: str,
    body: str,
    repo: str | None = None,
) -> dict[str, Any]:
    target = _repo(repo)
    open_prs = _open_prs(target)
    candidates = _candidate_prs(open_prs, issue_number, branch)
    if len(candidates) > 1:
        raise PRBindingError("multiple open PRs are already bound to the Issue branch")

    if candidates:
        number = candidates[0].get("number")
        if not isinstance(number, int):
            raise PRBindingError("existing PR number is invalid")
        _gh(
            "api", "--method", "PATCH", f"repos/{target}/pulls/{number}",
            "--field", f"body={body}",
        )
    else:
        created = _gh_json(
            "api", "--method", "POST", f"repos/{target}/pulls",
            "--field", f"title={title}", "--field", f"head={branch}",
            "--field", "base=main", "--field", f"body={body}",
        )
        if not isinstance(created, dict) or not created.get("html_url") or not isinstance(created.get("number"), int):
            raise PRBindingError("PR creation returned no URL or number")
        number = int(created["number"])

    verified = _verify_pr(_view_pr(target, number), issue_number, branch, expected_sha, _open_prs(target))
    return verified


def verify_post_push_binding(
    issue_number: int,
    pr_number: int,
    branch: str,
    expected_sha: str,
    repo: str | None = None,
    timeout_seconds: int = 30,
) -> dict[str, Any]:
    """Verify the newly pushed head without requiring worker state for that head."""

    target = _repo(repo)
    deadline = time.monotonic() + timeout_seconds
    last_error: PRBindingError | None = None
    while time.monotonic() < deadline:
        try:
            open_prs = _open_prs(target)
            matching = [pr for pr in open_prs if pr.get("number") == pr_number]
            if len(matching) != 1:
                raise PRBindingError("post-push PR is absent or ambiguous")
            return _verify_pr(matching[0], issue_number, branch, expected_sha, open_prs)
        except PRBindingError as exc:
            last_error = exc
            time.sleep(2)
    raise last_error or PRBindingError("post-push PR binding was not observable")


def main() -> None:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    create = subparsers.add_parser("create-or-update")
    create.add_argument("issue", type=int)
    create.add_argument("branch")
    create.add_argument("expected_sha")
    create.add_argument("title")
    create.add_argument("body_file", type=Path)
    create.add_argument("--repo")
    post = subparsers.add_parser("verify-post-push")
    post.add_argument("issue", type=int)
    post.add_argument("pr", type=int)
    post.add_argument("branch")
    post.add_argument("expected_sha")
    post.add_argument("--repo")
    args = parser.parse_args()
    try:
        if args.command == "create-or-update":
            result = create_or_update_pr(
                args.issue, args.branch, args.expected_sha, args.title,
                args.body_file.read_text(encoding="utf-8"), args.repo,
            )
        else:
            result = verify_post_push_binding(args.issue, args.pr, args.branch, args.expected_sha, args.repo)
        print(json.dumps(result, sort_keys=True))
    except (OSError, PRBindingError) as exc:
        print(f"PR_BINDING_ERROR: {exc}", file=os.sys.stderr)
        raise SystemExit(1) from exc


if __name__ == "__main__":
    main()
