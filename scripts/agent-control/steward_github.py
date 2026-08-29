"""Read-only GitHub facts for the provider-free Steward.

This adapter can prove the state of an already integrated Stage PR, but it
cannot create, approve, merge, or otherwise mutate GitHub.  The merge owner
and exact-head CI/review owners remain outside the Steward lifecycle.
"""

from __future__ import annotations

from dataclasses import dataclass
import json
from pathlib import Path
import re
import subprocess
from typing import Any, Protocol


SHA40 = re.compile(r"^[0-9a-f]{40}$")
REPOSITORY = re.compile(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")
BRANCH = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._/-]{0,199}$")
PR_STATES = frozenset({"OPEN", "CLOSED", "MERGED"})
CHECK_STATES = frozenset({"PASS", "PENDING", "FAIL", "UNKNOWN"})


class GitHubFactsError(RuntimeError):
    """Facts were unavailable, malformed, or bound to a different subject."""


class GitHubReadError(GitHubFactsError):
    """The read-only GitHub query did not return usable facts."""


class ReadOnlyGitHub(Protocol):
    def fetch_stage_pr(self, repository: str, pr_number: int) -> dict[str, Any]:
        """Return bounded facts for one PR; never perform a mutation."""


@dataclass(frozen=True)
class StagePRFacts:
    repository: str
    pr_number: int
    state: str
    draft: bool
    merged: bool
    base_sha: str
    head_sha: str
    ci_state: str
    review_state: str
    base_branch: str | None = None
    head_branch: str | None = None

    @classmethod
    def from_wire(cls, value: object) -> "StagePRFacts":
        if not isinstance(value, dict):
            raise GitHubFactsError("github_facts_invalid")
        required = {
            "repository",
            "pr_number",
            "state",
            "draft",
            "merged",
            "base_sha",
            "head_sha",
            "ci_state",
            "review_state",
            "base_branch",
            "head_branch",
        }
        if set(value) != required:
            raise GitHubFactsError("github_facts_fields_invalid")
        repository = value["repository"]
        number = value["pr_number"]
        state = value["state"]
        base_sha = value["base_sha"]
        head_sha = value["head_sha"]
        if not isinstance(repository, str) or REPOSITORY.fullmatch(repository) is None:
            raise GitHubFactsError("github_repository_invalid")
        if type(number) is not int or not 1 <= number <= 1_000_000_000:
            raise GitHubFactsError("github_pr_number_invalid")
        if state not in PR_STATES:
            raise GitHubFactsError("github_pr_state_invalid")
        if type(value["draft"]) is not bool or type(value["merged"]) is not bool:
            raise GitHubFactsError("github_pr_flags_invalid")
        if not isinstance(base_sha, str) or SHA40.fullmatch(base_sha) is None:
            raise GitHubFactsError("github_base_sha_invalid")
        if not isinstance(head_sha, str) or SHA40.fullmatch(head_sha) is None:
            raise GitHubFactsError("github_head_sha_invalid")
        if value["ci_state"] not in CHECK_STATES or value["review_state"] not in CHECK_STATES:
            raise GitHubFactsError("github_gate_state_invalid")
        for field in ("base_branch", "head_branch"):
            branch = value[field]
            if (
                not isinstance(branch, str)
                or BRANCH.fullmatch(branch) is None
                or ".." in branch
                or branch.endswith("/")
            ):
                raise GitHubFactsError("github_branch_invalid")
        return cls(
            repository,
            number,
            state,
            value["draft"],
            value["merged"],
            base_sha,
            head_sha,
            value["ci_state"],
            value["review_state"],
            value.get("base_branch"),
            value.get("head_branch"),
        )


@dataclass(frozen=True)
class StagePRStatus:
    outcome: str
    reason: str
    repository: str
    pr_number: int
    base_sha: str
    head_sha: str

    @property
    def waiting_for_merge(self) -> bool:
        return self.outcome == "WAITING_FOR_MERGE"

    def to_wire(self) -> dict[str, Any]:
        return {
            "schema_version": "steward_github_status.v1",
            "outcome": self.outcome,
            "reason": self.reason,
            "repository": self.repository,
            "pr_number": self.pr_number,
            "base_sha": self.base_sha,
            "head_sha": self.head_sha,
        }


def reconcile_stage_pr(
    facts: StagePRFacts | dict[str, Any],
    *,
    repository: str,
    pr_number: int,
    expected_base_sha: str,
    expected_head_sha: str,
    expected_base_branch: str | None = None,
    expected_head_branch: str | None = None,
) -> StagePRStatus:
    """Classify live PR facts without issuing a write or merge request."""

    observed = facts if isinstance(facts, StagePRFacts) else StagePRFacts.from_wire(facts)
    if (
        observed.repository.casefold() != repository.casefold()
        or observed.pr_number != pr_number
    ):
        raise GitHubFactsError("github_pr_identity_mismatch")
    if not SHA40.fullmatch(expected_base_sha) or not SHA40.fullmatch(expected_head_sha):
        raise GitHubFactsError("expected_pr_sha_invalid")
    if observed.base_sha != expected_base_sha or observed.head_sha != expected_head_sha:
        raise GitHubFactsError("github_pr_head_or_base_mismatch")
    if expected_base_branch is not None and observed.base_branch != expected_base_branch:
        raise GitHubFactsError("github_pr_base_branch_mismatch")
    if expected_head_branch is not None and observed.head_branch != expected_head_branch:
        raise GitHubFactsError("github_pr_head_branch_mismatch")
    if observed.merged and observed.ci_state != "PASS":
        return StagePRStatus(
            "WAITING",
            f"merged_pr_ci_{observed.ci_state.lower()}",
            observed.repository,
            observed.pr_number,
            observed.base_sha,
            observed.head_sha,
        )
    if observed.merged and observed.review_state != "PASS":
        return StagePRStatus(
            "WAITING",
            f"merged_pr_review_{observed.review_state.lower()}",
            observed.repository,
            observed.pr_number,
            observed.base_sha,
            observed.head_sha,
        )
    if observed.merged:
        return StagePRStatus(
            "COMPLETE",
            "pr_already_merged",
            observed.repository,
            observed.pr_number,
            observed.base_sha,
            observed.head_sha,
        )
    if observed.state != "OPEN":
        return StagePRStatus(
            "BLOCKED",
            "pr_closed_without_merge",
            observed.repository,
            observed.pr_number,
            observed.base_sha,
            observed.head_sha,
        )
    if observed.draft:
        return StagePRStatus(
            "BLOCKED",
            "pr_is_draft",
            observed.repository,
            observed.pr_number,
            observed.base_sha,
            observed.head_sha,
        )
    if observed.ci_state != "PASS":
        return StagePRStatus(
            "WAITING",
            f"ci_{observed.ci_state.lower()}",
            observed.repository,
            observed.pr_number,
            observed.base_sha,
            observed.head_sha,
        )
    if observed.review_state != "PASS":
        return StagePRStatus(
            "WAITING",
            f"review_{observed.review_state.lower()}",
            observed.repository,
            observed.pr_number,
            observed.base_sha,
            observed.head_sha,
        )
    return StagePRStatus(
        "WAITING_FOR_MERGE",
        "exact_head_ci_and_review_pass",
        observed.repository,
        observed.pr_number,
        observed.base_sha,
        observed.head_sha,
    )


REQUIRED_CI_JOBS = frozenset({
    "docker-build",
    "exact-head-check",
    "native-runtime",
    "pg-integration-tests",
    "python-tests",
    "rust-tests",
    "rust-typescript-cutover",
    "typescript-tests",
    "context-capsule",
})
CI_CHECK_ALIASES = {
    "exact-head": "exact-head-check",
    "exact-head-check": "exact-head-check",
}


MAX_GRAPHQL_PAGES = 20


def _graphql_page(
    repository: str,
    pr_number: int,
    query: str,
    *,
    cursor: str | None,
) -> dict[str, Any]:
    """Read one bounded GraphQL page through ``gh``; never mutate GitHub."""

    owner, name = repository.split("/", 1)
    command = [
        "gh",
        "api",
        "graphql",
        "-f",
        f"query={query}",
        "-f",
        f"owner={owner}",
        "-f",
        f"name={name}",
        "-F",
        f"number={pr_number}",
    ]
    if cursor is not None:
        command.extend(["-f", f"after={cursor}"])
    try:
        result = subprocess.run(
            command,
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise GitHubReadError("github_review_read_unavailable") from exc
    if result.returncode != 0:
        raise GitHubReadError("github_review_read_failed")
    try:
        payload = json.loads(result.stdout)
    except (TypeError, json.JSONDecodeError) as exc:
        raise GitHubReadError("github_review_read_malformed") from exc
    if not isinstance(payload, dict) or payload.get("errors"):
        raise GitHubReadError("github_review_read_malformed")
    data = payload.get("data")
    if not isinstance(data, dict):
        raise GitHubReadError("github_review_read_malformed")
    repository_data = data.get("repository")
    if not isinstance(repository_data, dict):
        raise GitHubReadError("github_review_read_malformed")
    pull_request = repository_data.get("pullRequest")
    if not isinstance(pull_request, dict):
        raise GitHubReadError("github_review_read_malformed")
    return pull_request


def _review_connection_pages(
    pr_number: int, head_sha: str, repository: str
) -> tuple[str | None, list[dict[str, Any]]]:
    """Fetch every review page and retain only bounded review facts."""

    query = """
      query($owner:String!, $name:String!, $number:Int!, $after:String) {
        repository(owner:$owner, name:$name) {
          pullRequest(number:$number) {
            reviewDecision
            reviews(first:100, after:$after) {
              nodes {
                author { login }
                state
                submittedAt
                commit { oid }
              }
              pageInfo { hasNextPage endCursor }
            }
          }
        }
      }
    """
    cursor: str | None = None
    decision: str | None = None
    nodes: list[dict[str, Any]] = []
    for _ in range(MAX_GRAPHQL_PAGES):
        page = _graphql_page(repository, pr_number, query, cursor=cursor)
        page_decision = page.get("reviewDecision")
        if page_decision is not None and not isinstance(page_decision, str):
            raise GitHubReadError("github_review_decision_malformed")
        if decision is None:
            decision = page_decision
        connection = page.get("reviews")
        if not isinstance(connection, dict):
            raise GitHubReadError("github_review_read_malformed")
        page_nodes = connection.get("nodes")
        page_info = connection.get("pageInfo")
        if not isinstance(page_nodes, list) or not isinstance(page_info, dict):
            raise GitHubReadError("github_review_read_malformed")
        for node in page_nodes:
            if isinstance(node, dict):
                nodes.append(node)
        if page_info.get("hasNextPage") is not True:
            break
        next_cursor = page_info.get("endCursor")
        if not isinstance(next_cursor, str) or not next_cursor or next_cursor == cursor:
            raise GitHubReadError("github_review_cursor_invalid")
        cursor = next_cursor
    else:
        raise GitHubReadError("github_review_page_limit_exceeded")
    return decision, nodes


def _authoritative_plan_review(
    pr_number: int, head_sha: str, repository: str, base_sha: str
) -> bool:
    """Return true only after live current-head review and thread proof."""

    try:
        effective = current_effective_reviews(pr_number, head_sha, repository)
        threads = review_threads_status(pr_number, head_sha, repository)
    except GitHubReadError:
        return False
    return (
        effective.get("complete") is True
        and effective.get("review_decision") == "APPROVED"
        and not effective.get("requested_changes")
        and any(
            isinstance(item, dict)
            and item.get("state") == "APPROVED"
            and item.get("is_current_head") is True
            for item in effective.get("effective_reviews", [])
        )
        and threads.get("complete") is True
        and not threads.get("unresolved_thread_ids")
    )


def current_effective_reviews(
    pr_number: int, head_sha: str, repository: str
) -> dict[str, Any]:
    if (
        REPOSITORY.fullmatch(repository) is None
        or type(pr_number) is not int
        or pr_number < 1
        or SHA40.fullmatch(head_sha) is None
    ):
        raise GitHubReadError("github_review_query_identity_invalid")
    review_decision, nodes = _review_connection_pages(pr_number, head_sha, repository)
    effective_by_author: dict[str, dict[str, Any]] = {}
    for node in nodes:
        author = node.get("author")
        login = author.get("login") if isinstance(author, dict) else None
        state = node.get("state")
        commit = node.get("commit")
        commit_oid = commit.get("oid") if isinstance(commit, dict) else None
        submitted_at = node.get("submittedAt")
        if (
            not isinstance(login, str)
            or not login
            or not isinstance(state, str)
            or not isinstance(submitted_at, str)
        ):
            continue
        if not isinstance(commit_oid, str):
            commit_oid = ""
        effective_by_author[login.casefold()] = {
            "state": state,
            "is_current_head": commit_oid == head_sha,
        }
    effective = list(effective_by_author.values())
    requested_changes = [
        item for item in effective if item.get("state") == "CHANGES_REQUESTED"
    ]
    return {
        "complete": True,
        "review_decision": review_decision,
        "requested_changes": requested_changes,
        "effective_reviews": effective,
    }


def review_threads_status(
    pr_number: int, head_sha: str, repository: str
) -> dict[str, Any]:
    if (
        REPOSITORY.fullmatch(repository) is None
        or type(pr_number) is not int
        or pr_number < 1
        or SHA40.fullmatch(head_sha) is None
    ):
        raise GitHubReadError("github_review_query_identity_invalid")
    query = """
      query($owner:String!, $name:String!, $number:Int!, $after:String) {
        repository(owner:$owner, name:$name) {
          pullRequest(number:$number) {
            reviewThreads(first:100, after:$after) {
              nodes { id isResolved }
              pageInfo { hasNextPage endCursor }
            }
          }
        }
      }
    """
    cursor: str | None = None
    unresolved: list[str] = []
    for _ in range(MAX_GRAPHQL_PAGES):
        page = _graphql_page(repository, pr_number, query, cursor=cursor)
        connection = page.get("reviewThreads")
        if not isinstance(connection, dict):
            raise GitHubReadError("github_review_thread_read_malformed")
        nodes = connection.get("nodes")
        page_info = connection.get("pageInfo")
        if not isinstance(nodes, list) or not isinstance(page_info, dict):
            raise GitHubReadError("github_review_thread_read_malformed")
        for node in nodes:
            if not isinstance(node, dict):
                raise GitHubReadError("github_review_thread_read_malformed")
            thread_id = node.get("id")
            resolved = node.get("isResolved")
            if not isinstance(thread_id, str) or type(resolved) is not bool:
                raise GitHubReadError("github_review_thread_read_malformed")
            if not resolved:
                unresolved.append(thread_id)
        if page_info.get("hasNextPage") is not True:
            break
        next_cursor = page_info.get("endCursor")
        if not isinstance(next_cursor, str) or not next_cursor or next_cursor == cursor:
            raise GitHubReadError("github_review_cursor_invalid")
        cursor = next_cursor
    else:
        raise GitHubReadError("github_review_page_limit_exceeded")
    return {"complete": True, "unresolved_thread_ids": unresolved}


class GhReadOnlyGitHub:
    """Live read-only GitHub query adapter through gh CLI."""

    def __init__(self, *, timeout_seconds: int = 30):
        if not 1 <= timeout_seconds <= 120:
            raise ValueError("timeout_seconds is outside the bounded range")
        self.timeout_seconds = timeout_seconds

    def fetch_stage_pr(self, repository: str, pr_number: int) -> dict[str, Any]:
        if REPOSITORY.fullmatch(repository) is None or type(pr_number) is not int or pr_number < 1:
            raise GitHubFactsError("github_query_identity_invalid")
        command = [
            "gh",
            "pr",
            "view",
            str(pr_number),
            "--repo",
            repository,
            "--json",
            "state,isDraft,mergedAt,baseRefName,headRefName,baseRefOid,headRefOid,statusCheckRollup,reviewDecision",
        ]
        try:
            result = subprocess.run(
                command,
                capture_output=True,
                text=True,
                timeout=self.timeout_seconds,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise GitHubReadError("github_read_unavailable") from exc
        if result.returncode != 0:
            raise GitHubReadError("github_read_failed")
        try:
            payload = json.loads(result.stdout)
        except (TypeError, json.JSONDecodeError) as exc:
            raise GitHubReadError("github_read_malformed") from exc
        if not isinstance(payload, dict):
            raise GitHubReadError("github_read_malformed")
        checks = payload.get("statusCheckRollup")
        ci_state = "UNKNOWN"
        if isinstance(checks, list) and checks:
            check_items = [item for item in checks if isinstance(item, dict)]
            conclusions = [item.get("conclusion") for item in check_items]
            statuses = {
                item.get("status")
                for item in check_items
            }
            required_jobs = set(REQUIRED_CI_JOBS)
            observed_names = {
                CI_CHECK_ALIASES.get(item["name"], item["name"])
                for item in check_items
                if isinstance(item.get("name"), str)
            }
            if "FAILURE" in conclusions or "CANCELLED" in conclusions:
                ci_state = "FAIL"
            elif (
                len(check_items) != len(checks)
                or None in conclusions
            ):
                ci_state = "PENDING"
            elif any(status != "COMPLETED" for status in statuses):
                # A successful conclusion is not terminal evidence unless the
                # corresponding check is explicitly COMPLETED.  Unknown
                # status values remain unknown rather than being treated as
                # green.
                ci_state = (
                    "PENDING"
                    if statuses
                    & {"IN_PROGRESS", "QUEUED", "REQUESTED", "WAITING", "PENDING"}
                    else "UNKNOWN"
                )
            elif not required_jobs.issubset(observed_names):
                ci_state = "UNKNOWN"
            elif len(conclusions) == len(checks) and all(
                conclusion == "SUCCESS" for conclusion in conclusions
            ):
                ci_state = "PASS"
        review = payload.get("reviewDecision")
        if review not in (None, "", "APPROVED", "CHANGES_REQUESTED", "REVIEW_REQUIRED"):
            raise GitHubReadError("github_review_decision_malformed")
        state = payload.get("state")
        draft = payload.get("isDraft")
        merged_at = payload.get("mergedAt")
        base_sha = payload.get("baseRefOid")
        head_sha = payload.get("headRefOid")
        base_branch = payload.get("baseRefName")
        head_branch = payload.get("headRefName")
        if (
            state not in PR_STATES
            or type(draft) is not bool
            or (merged_at is not None and not isinstance(merged_at, str))
            or not isinstance(base_sha, str)
            or not isinstance(head_sha, str)
            or not isinstance(base_branch, str)
            or not isinstance(head_branch, str)
        ):
            raise GitHubReadError("github_read_malformed")
        review_state = "PENDING" if review in (None, "", "REVIEW_REQUIRED") else "FAIL"
        if review_state == "PENDING" and merged_at is not None:
            review_state = (
                "PASS"
                if _authoritative_plan_review(
                    pr_number, head_sha, repository, base_sha
                )
                else "PENDING"
            )
        if review == "APPROVED":
            effective = current_effective_reviews(
                pr_number, head_sha, repository
            )
            threads = review_threads_status(
                pr_number, head_sha, repository
            )
            if (
                effective.get("complete") is not True
                or threads.get("complete") is not True
                or threads.get("unresolved_thread_ids")
            ):
                raise GitHubReadError("exact_head_review_incomplete")
            if effective.get("review_decision") != "APPROVED":
                raise GitHubReadError("review_decision_changed_during_read")
            if effective.get("requested_changes"):
                review_state = "FAIL"
            else:
                current_head_approvals = [
                    item
                    for item in effective.get("effective_reviews", [])
                    if isinstance(item, dict)
                    and item.get("state") == "APPROVED"
                    and item.get("is_current_head") is True
                ]
                if not current_head_approvals:
                    raise GitHubReadError("current_head_review_approval_missing")
                review_state = "PASS"
        return {
            "repository": repository,
            "pr_number": pr_number,
            "state": state,
            "draft": draft,
            "merged": merged_at is not None,
            "base_sha": base_sha,
            "head_sha": head_sha,
            "ci_state": ci_state,
            "review_state": review_state,
            "base_branch": base_branch,
            "head_branch": head_branch,
        }


class FakeGitHubReader:
    """Read-only deterministic reader for service and fault tests."""

    def __init__(self, facts: dict[str, Any] | None = None):
        self.facts = dict(facts or {})
        self.reads: list[tuple[str, int]] = []
        self.fail = False

    def fetch_stage_pr(self, repository: str, pr_number: int) -> dict[str, Any]:
        if self.fail:
            raise GitHubReadError("github_read_unavailable")
        self.reads.append((repository, pr_number))
        facts = dict(self.facts)
        facts.setdefault("repository", repository)
        facts.setdefault("pr_number", pr_number)
        return facts


__all__ = [
    "FakeGitHubReader",
    "GhReadOnlyGitHub",
    "GitHubFactsError",
    "GitHubReadError",
    "ReadOnlyGitHub",
    "StagePRFacts",
    "StagePRStatus",
    "reconcile_stage_pr",
]
