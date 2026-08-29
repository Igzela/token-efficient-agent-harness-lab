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

class PRBindingError(RuntimeError):
    """Raised when an Issue cannot be bound to exactly one safe PR."""


def parse_binding_marker(text: str) -> dict[str, Any] | None:
    """Parse JSON payload from an agent-orchestrator-binding HTML comment."""
    match = re.search(r"<!--\s*agent-orchestrator-binding:\s*(\{.*?\})\s*-->", text)
    if not match:
        return None
    try:
        data = json.loads(match.group(1))
        return data if isinstance(data, dict) else None
    except Exception:
        return None


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
        "--json", "number,headRefName,headRefOid,state,baseRefName,body,url,isDraft",
    )
    if not isinstance(data, list):
        raise PRBindingError("open PR response was not a list")
    return [item for item in data if isinstance(item, dict)]


def _candidate_prs(prs: list[dict[str, Any]], issue_number: int, branch: str) -> list[dict[str, Any]]:
    candidates: list[dict[str, Any]] = []
    for pr in prs:
        marker = parse_binding_marker(pr.get("body", ""))
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
    if pr.get("isDraft") is not True:
        raise PRBindingError("bound PR is not a Draft")
    if pr.get("baseRefName") != "main":
        raise PRBindingError("bound PR does not target main")
    if pr.get("headRefName") != branch or pr.get("headRefOid") != expected_sha:
        raise PRBindingError("bound PR branch or head does not match")
    marker = parse_binding_marker(pr.get("body", ""))
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
        "--json", "number,state,baseRefName,headRefName,headRefOid,body,url,isDraft,headRepository",
    )
    if not isinstance(data, dict):
        raise PRBindingError("PR view response was not an object")
    return data


def find_issue_pr(
    issue_number: int, branch: str, expected_sha: str, repo: str | None = None
) -> dict[str, Any]:
    """Return the authoritative final view of the single open Issue-bound PR.

    The trusted handoff gateway must know the exact PR before it can verify
    anything else.  The ``pr list`` snapshot is discovery only: it identifies
    the single open candidate (matching the canonical branch or the Issue
    binding marker) and its number.  Zero and multiple candidates both fail
    closed so a handoff can never guess which PR to bind.

    The authoritative PR number, base/head refs, Draft state, head
    repository, binding marker, and closing link are read from the final
    ``pr view`` and checked through the canonical ``_verify_pr`` owner;
    incomplete list data is never trusted for base/head.  A view whose
    number disagrees with the discovered candidate, a non-open or non-Draft
    view, a view whose head branch or exact head sha differs from the
    expected canonical branch/head, a view without the Issue binding marker
    or closing link, and a fork head repository all fail closed.
    """

    target = _repo(repo)
    open_prs = _open_prs(target)
    candidates = _candidate_prs(open_prs, issue_number, branch)
    if len(candidates) != 1:
        raise PRBindingError("zero or multiple open PRs bound to the Issue branch")
    candidate = candidates[0]
    if not isinstance(candidate.get("number"), int):
        raise PRBindingError("bound PR number is invalid")
    if str(candidate.get("state", "")).upper() != "OPEN":
        raise PRBindingError("bound PR is not open")
    verified = _view_pr(target, int(candidate["number"]))
    if verified.get("number") != int(candidate["number"]):
        raise PRBindingError("bound PR final view is inconsistent")
    _verify_pr(verified, issue_number, branch, expected_sha, open_prs)
    head_repo = verified.get("headRepository")
    if not isinstance(head_repo, dict) or head_repo.get("nameWithOwner") != target:
        raise PRBindingError("PR head repository is not the target repository")
    return verified


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
        if candidates[0].get("isDraft") is not True:
            raise PRBindingError("existing PR is not a Draft; refusing to mutate it")
        _gh(
            "api", "--method", "PATCH", f"repos/{target}/pulls/{number}",
            "--field", f"body={body}",
        )
    else:
        created = _gh_json(
            "api", "--method", "POST", f"repos/{target}/pulls",
            "--field", f"title={title}", "--field", f"head={branch}",
            "--field", "base=main", "--field", f"body={body}",
            "--field", "draft=true",
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


def _candidate_plan_prs(prs: list[dict[str, Any]], subject_id: str, branch: str) -> list[dict[str, Any]]:
    candidates: list[dict[str, Any]] = []
    for pr in prs:
        marker = parse_binding_marker(pr.get("body", ""))
        if pr.get("headRefName") == branch or (
            marker
            and marker.get("subject_kind") == "plan-packet"
            and marker.get("subject_id") == subject_id
        ):
            candidates.append(pr)
    return candidates


def _verify_plan_pr(
    pr: dict[str, Any], subject_id: str, branch: str, expected_sha: str,
    source_main_sha: str, task_spec_sha256: str, prs: list[dict[str, Any]],
) -> dict[str, Any]:
    number = pr.get("number")
    if not isinstance(number, int):
        raise PRBindingError("plan PR number is missing")
    if str(pr.get("state", "")).upper() not in {"OPEN", ""}:
        raise PRBindingError("plan PR is not open")
    if pr.get("isDraft") is not True or pr.get("baseRefName") != "main":
        raise PRBindingError("plan PR is not a Draft targeting main")
    if pr.get("headRefName") != branch or pr.get("headRefOid") != expected_sha:
        raise PRBindingError("plan PR branch or head does not match")
    marker = parse_binding_marker(pr.get("body", ""))
    if not marker or any(
        marker.get(key) != value
        for key, value in {
            "subject_kind": "plan-packet",
            "subject_id": subject_id,
            "source_main_sha": source_main_sha,
            "task_spec_sha256": task_spec_sha256,
            "branch": branch,
        }.items()
    ):
        raise PRBindingError("plan PR binding marker is invalid")
    competitors = [
        item for item in _candidate_plan_prs(prs, subject_id, branch)
        if item.get("number") != number
    ]
    if competitors:
        raise PRBindingError("multiple open PRs are bound to the plan packet")
    return {"number": number, "head_sha": expected_sha, "url": pr.get("url", "")}


def find_plan_pr(
    subject_id: str,
    branch: str,
    expected_sha: str,
    source_main_sha: str,
    task_spec_sha256: str,
    repo: str | None = None,
) -> dict[str, Any]:
    """Return the authoritative final view of one exact plan Draft PR."""

    target = _repo(repo)
    open_prs = _open_prs(target)
    candidates = _candidate_plan_prs(open_prs, subject_id, branch)
    if len(candidates) != 1:
        raise PRBindingError("zero or multiple open PRs bound to the plan packet")
    number = candidates[0].get("number")
    if not isinstance(number, int):
        raise PRBindingError("plan PR number is invalid")
    verified = _view_pr(target, number)
    if verified.get("number") != number:
        raise PRBindingError("plan PR final view is inconsistent")
    _verify_plan_pr(
        verified, subject_id, branch, expected_sha, source_main_sha, task_spec_sha256, open_prs
    )
    head_repo = verified.get("headRepository")
    if not isinstance(head_repo, dict) or head_repo.get("nameWithOwner") != target:
        raise PRBindingError("plan PR head repository is not the target repository")
    return verified


def create_or_update_plan_pr(
    subject_id: str,
    branch: str,
    expected_sha: str,
    source_main_sha: str,
    task_spec_sha256: str,
    title: str,
    body: str,
    repo: str | None = None,
) -> dict[str, Any]:
    """Create or update exactly one Draft PR bound to an immutable plan pointer."""

    target = _repo(repo)
    open_prs = _open_prs(target)
    candidates = _candidate_plan_prs(open_prs, subject_id, branch)
    if len(candidates) > 1:
        raise PRBindingError("multiple open PRs are already bound to the plan packet")
    if candidates:
        number = candidates[0].get("number")
        if not isinstance(number, int) or candidates[0].get("isDraft") is not True:
            raise PRBindingError("existing plan PR is not a Draft")
        _gh("api", "--method", "PATCH", f"repos/{target}/pulls/{number}", "--field", f"body={body}")
    else:
        created = _gh_json(
            "api", "--method", "POST", f"repos/{target}/pulls",
            "--field", f"title={title}", "--field", f"head={branch}",
            "--field", "base=main", "--field", f"body={body}", "--field", "draft=true",
        )
        if not isinstance(created, dict) or not isinstance(created.get("number"), int):
            raise PRBindingError("plan PR creation returned no number")
        number = int(created["number"])
    verified = _view_pr(target, number)
    return _verify_plan_pr(
        verified, subject_id, branch, expected_sha, source_main_sha, task_spec_sha256,
        _open_prs(target),
    ) | verified


def verify_post_push_plan_binding(
    subject_id: str,
    pr_number: int,
    branch: str,
    expected_sha: str,
    source_main_sha: str,
    task_spec_sha256: str,
    repo: str | None = None,
    timeout_seconds: int = 30,
) -> dict[str, Any]:
    target = _repo(repo)
    deadline = time.monotonic() + timeout_seconds
    last_error: PRBindingError | None = None
    while time.monotonic() < deadline:
        try:
            open_prs = _open_prs(target)
            matching = [pr for pr in open_prs if pr.get("number") == pr_number]
            if len(matching) != 1:
                raise PRBindingError("post-push plan PR is absent or ambiguous")
            return _verify_plan_pr(
                matching[0], subject_id, branch, expected_sha, source_main_sha,
                task_spec_sha256, open_prs,
            )
        except PRBindingError as exc:
            last_error = exc
            time.sleep(2)
    raise last_error or PRBindingError("post-push plan PR binding was not observable")


def _candidate_stage_prs(prs: list[dict[str, Any]], stage_id: str, branch: str) -> list[dict[str, Any]]:
    candidates: list[dict[str, Any]] = []
    for pr in prs:
        marker = parse_binding_marker(pr.get("body", ""))
        if pr.get("headRefName") == branch or (
            marker
            and marker.get("subject_kind") == "steward-stage"
            and marker.get("stage_id") == stage_id
        ):
            candidates.append(pr)
    return candidates


def _verify_stage_pr(
    pr: dict[str, Any], stage_id: str, mission_id: str, branch: str,
    expected_sha: str, base_sha: str, prs: list[dict[str, Any]],
) -> dict[str, Any]:
    number = pr.get("number")
    if not isinstance(number, int):
        raise PRBindingError("stage PR number is missing")
    if str(pr.get("state", "")).upper() not in {"OPEN", ""}:
        raise PRBindingError("stage PR is not open")
    if pr.get("isDraft") is not True or pr.get("baseRefName") != "main":
        raise PRBindingError("stage PR is not a Draft targeting main")
    if pr.get("headRefName") != branch or pr.get("headRefOid") != expected_sha:
        raise PRBindingError("stage PR branch or head does not match")
    marker = parse_binding_marker(pr.get("body", ""))
    expected = {
        "subject_kind": "steward-stage",
        "stage_id": stage_id,
        "mission_id": mission_id,
        "base_sha": base_sha,
        "branch": branch,
    }
    if not marker or any(marker.get(key) != value for key, value in expected.items()):
        raise PRBindingError("stage PR binding marker is invalid")
    competitors = [
        item for item in _candidate_stage_prs(prs, stage_id, branch)
        if item.get("number") != number
    ]
    if competitors:
        raise PRBindingError("multiple open PRs are bound to the Stage")
    return {"number": number, "head_sha": expected_sha, "url": pr.get("url", "")}


def find_stage_pr(
    stage_id: str, mission_id: str, branch: str, expected_sha: str, base_sha: str,
    repo: str | None = None,
) -> dict[str, Any]:
    """Return the authoritative final view of one exact Stage Draft PR."""

    target = _repo(repo)
    open_prs = _open_prs(target)
    candidates = _candidate_stage_prs(open_prs, stage_id, branch)
    if len(candidates) != 1:
        raise PRBindingError("zero or multiple open PRs bound to the Stage")
    number = candidates[0].get("number")
    if not isinstance(number, int):
        raise PRBindingError("stage PR number is invalid")
    verified = _view_pr(target, number)
    if verified.get("number") != number:
        raise PRBindingError("stage PR final view is inconsistent")
    _verify_stage_pr(verified, stage_id, mission_id, branch, expected_sha, base_sha, open_prs)
    head_repo = verified.get("headRepository")
    if not isinstance(head_repo, dict) or head_repo.get("nameWithOwner") != target:
        raise PRBindingError("stage PR head repository is not the target repository")
    return verified


def create_or_update_stage_pr(
    stage_id: str,
    mission_id: str,
    branch: str,
    expected_sha: str,
    base_sha: str,
    title: str,
    body: str,
    repo: str | None = None,
) -> dict[str, Any]:
    """Create/update one parent-owned Draft PR bound to a Stage identity."""

    target = _repo(repo)
    open_prs = _open_prs(target)
    candidates = _candidate_stage_prs(open_prs, stage_id, branch)
    if len(candidates) > 1:
        raise PRBindingError("multiple open PRs are already bound to the Stage")
    if candidates:
        number = candidates[0].get("number")
        if not isinstance(number, int) or candidates[0].get("isDraft") is not True:
            raise PRBindingError("existing stage PR is not a Draft")
        _verify_stage_pr(
            candidates[0], stage_id, mission_id, branch, expected_sha, base_sha, open_prs
        )
        _gh(
            "api", "--method", "PATCH", f"repos/{target}/pulls/{number}",
            "--field", f"body={body}",
        )
    else:
        created = _gh_json(
            "api", "--method", "POST", f"repos/{target}/pulls",
            "--field", f"title={title}", "--field", f"head={branch}",
            "--field", "base=main", "--field", f"body={body}", "--field", "draft=true",
        )
        if not isinstance(created, dict) or not isinstance(created.get("number"), int):
            raise PRBindingError("stage PR creation returned no number")
        number = int(created["number"])
    verified = _view_pr(target, number)
    result = _verify_stage_pr(
        verified, stage_id, mission_id, branch, expected_sha, base_sha, _open_prs(target)
    )
    head_repo = verified.get("headRepository")
    if not isinstance(head_repo, dict) or head_repo.get("nameWithOwner") != target:
        raise PRBindingError("stage PR head repository is not the target repository")
    return result | verified


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
