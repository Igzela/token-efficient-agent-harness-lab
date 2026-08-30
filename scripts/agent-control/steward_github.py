"""Bounded GitHub facts and mutation transport for the Autonomous Steward.

The read adapter is the authority for exact PR/head/CI/review facts.  The
writer may create Draft PRs, publish idempotent review receipts, promote Ready,
supersede failed candidates, and dispatch the canonical merge workflow; it
never performs a direct merge.
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timezone
import json
import re
import subprocess
from typing import Any, Protocol

import review_convergence


SHA40 = re.compile(r"^[0-9a-f]{40}$")
REPOSITORY = re.compile(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")
BRANCH = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._/-]{0,199}$")
PR_STATES = frozenset({"OPEN", "CLOSED", "MERGED"})
CHECK_STATES = frozenset({"PASS", "PENDING", "FAIL", "UNKNOWN"})
REVIEW_STATES = frozenset(
    {"APPROVED", "CHANGES_REQUESTED", "COMMENTED", "DISMISSED", "PENDING"}
)


class GitHubFactsError(RuntimeError):
    """Facts were unavailable, malformed, or bound to a different subject."""


class GitHubReadError(GitHubFactsError):
    """The read-only GitHub query did not return usable facts."""


class ReadOnlyGitHub(Protocol):
    def fetch_stage_pr(self, repository: str, pr_number: int) -> dict[str, Any]:
        """Return bounded facts for one PR; never perform a mutation."""


def _exact_head_review_receipt_pass(
    repository: str,
    pr_number: int,
    head_sha: str,
    base_sha: str,
    pr_author_identity: str | None,
    *,
    timeout_seconds: int,
) -> bool:
    """Read the repository-owned exact-head receipt from live GitHub comments.

    The receipt is deliberately a comment rather than an aggregate review
    state: the canonical action validates the complete bounded diff, reviewer
    identity, and unresolved-objection field from this same comment.  This
    helper is read-only and returns false for absent or malformed evidence.
    """
    if not pr_author_identity:
        return False
    try:
        result = subprocess.run(
            [
                "gh", "api", "--paginate", "--slurp",
                f"repos/{repository}/issues/{pr_number}/comments?per_page=100",
            ],
            capture_output=True,
            text=True,
            timeout=timeout_seconds,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise GitHubReadError("github_review_receipt_read_unavailable") from exc
    if result.returncode != 0:
        raise GitHubReadError("github_review_receipt_read_failed")
    try:
        pages = json.loads(result.stdout or "[]")
    except json.JSONDecodeError as exc:
        raise GitHubReadError("github_review_receipt_read_malformed") from exc
    if not isinstance(pages, list):
        raise GitHubReadError("github_review_receipt_read_malformed")
    comments: list[dict[str, Any]] = []
    for page in pages:
        if not isinstance(page, list):
            raise GitHubReadError("github_review_receipt_read_malformed")
        comments.extend(item for item in page if isinstance(item, dict))
    marker = "EXACT-HEAD REVIEW RECEIPT"
    receipt_comments = [item for item in comments if marker in str(item.get("body") or "")]
    if not receipt_comments:
        return False
    try:
        return review_convergence.exact_head_review_confirmed(
            comments,
            expected_head_sha=head_sha,
            expected_base_sha=base_sha,
            expected_pr_author_identity=pr_author_identity,
        )
    except (TypeError, ValueError, review_convergence.ConvergenceError):
        return False


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


def _validate_review_page_head(page: dict[str, Any], head_sha: str) -> None:
    observed_head = page.get("headRefOid")
    if not isinstance(observed_head, str) or SHA40.fullmatch(observed_head) is None:
        raise GitHubReadError("github_review_head_malformed")
    if observed_head != head_sha:
        raise GitHubReadError("github_review_head_changed")


def _next_review_page(
    page_info: object, *, error: str, cursor: str | None
) -> str | None:
    if not isinstance(page_info, dict) or type(page_info.get("hasNextPage")) is not bool:
        raise GitHubReadError(error)
    if not page_info["hasNextPage"]:
        return None
    next_cursor = page_info.get("endCursor")
    if not isinstance(next_cursor, str) or not next_cursor or next_cursor == cursor:
        raise GitHubReadError("github_review_cursor_invalid")
    return next_cursor


def _review_timestamp(value: object) -> datetime:
    if not isinstance(value, str) or not value:
        raise GitHubReadError("github_review_timestamp_malformed")
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise GitHubReadError("github_review_timestamp_malformed") from exc
    if parsed.tzinfo is None:
        raise GitHubReadError("github_review_timestamp_malformed")
    return parsed


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
            headRefOid
            reviewDecision
            reviews(first:100, after:$after) {
              nodes {
                id
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
    decision_seen = False
    for _ in range(MAX_GRAPHQL_PAGES):
        page = _graphql_page(repository, pr_number, query, cursor=cursor)
        _validate_review_page_head(page, head_sha)
        page_decision = page.get("reviewDecision")
        if page_decision is not None and (
            not isinstance(page_decision, str)
            or page_decision not in {"", "APPROVED", "CHANGES_REQUESTED", "REVIEW_REQUIRED"}
        ):
            raise GitHubReadError("github_review_decision_malformed")
        if not decision_seen:
            decision = page_decision
            decision_seen = True
        elif page_decision != decision:
            raise GitHubReadError("github_review_decision_changed_during_read")
        connection = page.get("reviews")
        if not isinstance(connection, dict):
            raise GitHubReadError("github_review_read_malformed")
        page_nodes = connection.get("nodes")
        page_info = connection.get("pageInfo")
        if not isinstance(page_nodes, list) or not isinstance(page_info, dict):
            raise GitHubReadError("github_review_read_malformed")
        for node in page_nodes:
            if not isinstance(node, dict):
                raise GitHubReadError("github_review_read_malformed")
            nodes.append(node)
        next_cursor = _next_review_page(
            page_info, error="github_review_read_malformed", cursor=cursor
        )
        if next_cursor is None:
            break
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
    effective_by_author: dict[str, tuple[datetime, str, dict[str, Any]]] = {}
    seen_ids: set[str] = set()
    for node in nodes:
        review_id = node.get("id")
        if not isinstance(review_id, str) or not review_id or review_id in seen_ids:
            raise GitHubReadError("github_review_node_malformed")
        seen_ids.add(review_id)
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
            or state not in REVIEW_STATES
            or not isinstance(commit_oid, str)
            or SHA40.fullmatch(commit_oid) is None
        ):
            raise GitHubReadError("github_review_node_malformed")
        submitted_time = _review_timestamp(submitted_at)
        normalized = {
            "review_id": review_id,
            "login": login,
            "state": state,
            "submitted_at": submitted_at,
            "is_current_head": commit_oid == head_sha,
        }
        author_key = login.casefold()
        previous = effective_by_author.get(author_key)
        candidate = (submitted_time, review_id, normalized)
        if previous is None or candidate[:2] > previous[:2]:
            effective_by_author[author_key] = candidate
    effective = [
        value[2]
        for value in sorted(
            effective_by_author.values(), key=lambda item: (item[0], item[1])
        )
    ]
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
            headRefOid
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
    seen_ids: set[str] = set()
    for _ in range(MAX_GRAPHQL_PAGES):
        page = _graphql_page(repository, pr_number, query, cursor=cursor)
        _validate_review_page_head(page, head_sha)
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
            if (
                not isinstance(thread_id, str)
                or not thread_id
                or thread_id in seen_ids
                or type(resolved) is not bool
            ):
                raise GitHubReadError("github_review_thread_read_malformed")
            seen_ids.add(thread_id)
            if not resolved:
                unresolved.append(thread_id)
        next_cursor = _next_review_page(
            page_info,
            error="github_review_thread_read_malformed",
            cursor=cursor,
        )
        if next_cursor is None:
            break
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

    def fetch_accepted_main(self, repository: str) -> str:
        """Read the authoritative accepted-main tip for Mission proposal."""

        if REPOSITORY.fullmatch(repository) is None:
            raise GitHubFactsError("github_query_identity_invalid")
        try:
            result = subprocess.run(
                ["gh", "api", f"repos/{repository}/branches/main", "--jq", ".commit.sha"],
                capture_output=True,
                text=True,
                timeout=self.timeout_seconds,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise GitHubReadError("accepted_main_read_unavailable") from exc
        if result.returncode != 0:
            raise GitHubReadError("accepted_main_read_failed")
        sha = result.stdout.strip()
        if SHA40.fullmatch(sha) is None:
            raise GitHubReadError("accepted_main_read_malformed")
        return sha

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
            "state,isDraft,mergedAt,baseRefName,headRefName,baseRefOid,headRefOid,statusCheckRollup,reviewDecision,author",
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
        if review_state == "PENDING":
            author = payload.get("author")
            author_login = author.get("login") if isinstance(author, dict) else None
            if isinstance(author_login, str) and _exact_head_review_receipt_pass(
                repository,
                pr_number,
                head_sha,
                base_sha,
                author_login,
                timeout_seconds=self.timeout_seconds,
            ):
                review_state = "PASS"
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


class GitHubMutationError(GitHubFactsError):
    """A GitHub mutation request failed, timed out, or had ambiguous outcome."""


class GitHubWriter(Protocol):
    def fetch_stage_pr(self, repository: str, pr_number: int) -> dict[str, Any]:
        ...

    def create_or_update_stage_pr(
        self,
        stage_id: str,
        mission_id: str,
        branch: str,
        expected_sha: str,
        base_sha: str,
        title: str,
        body: str,
        repository: str,
    ) -> dict[str, Any]:
        ...

    def mark_ready(
        self,
        repository: str,
        pr_number: int,
        expected_head_sha: str,
    ) -> bool:
        ...

    def publish_exact_head_review(
        self,
        repository: str,
        pr_number: int,
        expected_head_sha: str,
        *,
        base_sha: str,
        reviewer_session_id: str,
        implementation_session_id: str,
        reviewed_range_sha256: str,
        review_receipt_sha256: str,
    ) -> dict[str, Any]:
        """Publish or reconcile one exact-head independent review receipt."""
        ...

    def guarded_merge(
        self,
        repository: str,
        pr_number: int,
        expected_head_sha: str,
        *,
        merge_method: str = "squash",
        workflow_file: str = "agent-merge.yml",
        timeout_seconds: int = 120,
    ) -> dict[str, Any]:
        ...

    def supersede_stage_pr(
        self,
        repository: str,
        pr_number: int,
        expected_head_sha: str,
    ) -> bool:
        ...

    def post_merge_readback(
        self,
        repository: str,
        pr_number: int,
        expected_head_sha: str,
        *,
        timeout_seconds: int = 30,
    ) -> dict[str, Any]:
        ...


class GhGitHubWriter:
    """Bounded GitHub writer that delegates all merges to the canonical merge workflow."""

    def __init__(self, *, timeout_seconds: int = 30):
        if not 1 <= timeout_seconds <= 300:
            raise ValueError("timeout_seconds is outside the bounded range")
        self.timeout_seconds = timeout_seconds
        self.reader = GhReadOnlyGitHub(timeout_seconds=min(timeout_seconds, 30))

    def fetch_stage_pr(self, repository: str, pr_number: int) -> dict[str, Any]:
        return self.reader.fetch_stage_pr(repository, pr_number)

    def create_or_update_stage_pr(
        self,
        stage_id: str,
        mission_id: str,
        branch: str,
        expected_sha: str,
        base_sha: str,
        title: str,
        body: str,
        repository: str,
    ) -> dict[str, Any]:
        if (
            REPOSITORY.fullmatch(repository) is None
            or BRANCH.fullmatch(branch) is None
            or SHA40.fullmatch(expected_sha) is None
            or SHA40.fullmatch(base_sha) is None
        ):
            raise GitHubMutationError("github_mutation_identity_invalid")

        list_cmd = [
            "gh", "pr", "list",
            "--repo", repository,
            "--head", branch,
            "--json", "number,headRefOid,isDraft,state",
        ]
        try:
            res = subprocess.run(list_cmd, capture_output=True, text=True, timeout=self.timeout_seconds, check=False)
            if res.returncode == 0:
                prs = json.loads(res.stdout or "[]")
                open_prs = [p for p in prs if p.get("state") == "OPEN"]
                if open_prs:
                    pr_num = open_prs[0]["number"]
                    edit_cmd = ["gh", "pr", "edit", str(pr_num), "--repo", repository, "--title", title, "--body", body]
                    subprocess.run(edit_cmd, capture_output=True, text=True, timeout=self.timeout_seconds, check=False)
                    return self.fetch_stage_pr(repository, pr_num)
        except (OSError, subprocess.TimeoutExpired, json.JSONDecodeError):
            pass

        create_cmd = [
            "gh", "pr", "create",
            "--draft",
            "--repo", repository,
            "--head", branch,
            "--base", "main",
            "--title", title,
            "--body", body,
        ]
        try:
            result = subprocess.run(create_cmd, capture_output=True, text=True, timeout=self.timeout_seconds, check=False)
            if result.returncode != 0:
                raise GitHubMutationError(f"create_stage_pr_failed: {result.stderr.strip()}")
            url = result.stdout.strip()
            pr_num = int(url.rstrip("/").split("/")[-1])
            return self.fetch_stage_pr(repository, pr_num)
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise GitHubMutationError("create_stage_pr_unavailable") from exc

    def mark_ready(
        self,
        repository: str,
        pr_number: int,
        expected_head_sha: str,
    ) -> bool:
        if (
            REPOSITORY.fullmatch(repository) is None
            or type(pr_number) is not int
            or pr_number < 1
            or SHA40.fullmatch(expected_head_sha) is None
        ):
            raise GitHubMutationError("github_mutation_identity_invalid")

        facts = self.fetch_stage_pr(repository, pr_number)
        if facts.get("head_sha") != expected_head_sha:
            raise GitHubMutationError("exact_head_mismatch_before_mark_ready")

        if facts.get("draft") is False:
            return True

        command = ["gh", "pr", "ready", str(pr_number), "--repo", repository]
        try:
            result = subprocess.run(command, capture_output=True, text=True, timeout=self.timeout_seconds, check=False)
            if result.returncode == 0:
                return True
            raise GitHubMutationError(f"mark_ready_failed: {result.stderr.strip()}")
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise GitHubMutationError("mark_ready_unavailable") from exc

    def publish_exact_head_review(
        self,
        repository: str,
        pr_number: int,
        expected_head_sha: str,
        *,
        base_sha: str,
        reviewer_session_id: str,
        implementation_session_id: str,
        reviewed_range_sha256: str,
        review_receipt_sha256: str,
    ) -> dict[str, Any]:
        """Publish a bounded receipt on the PR using the authenticated gh user.

        The OpenCode reviewer is a distinct read-only session.  The parent
        Steward posts its sealed result through the already-authenticated
        GitHub transport, matching the repository's accepted review protocol.
        Existing exact-head receipts are reconciled before any POST, so a
        restart or lost response cannot duplicate the external effect.
        """
        if (
            REPOSITORY.fullmatch(repository) is None
            or type(pr_number) is not int
            or pr_number < 1
            or SHA40.fullmatch(expected_head_sha) is None
            or SHA40.fullmatch(base_sha) is None
            or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}", reviewer_session_id)
            or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}", implementation_session_id)
            or not re.fullmatch(r"[0-9a-f]{64}", reviewed_range_sha256)
            or not re.fullmatch(r"[0-9a-f]{64}", review_receipt_sha256)
        ):
            raise GitHubMutationError("review_receipt_identity_invalid")
        facts = self.fetch_stage_pr(repository, pr_number)
        if facts.get("head_sha") != expected_head_sha or facts.get("base_sha") != base_sha:
            raise GitHubMutationError("review_receipt_exact_binding_mismatch")
        try:
            identity_result = subprocess.run(
                ["gh", "api", "user", "--jq", ".login"],
                capture_output=True,
                text=True,
                timeout=self.timeout_seconds,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise GitHubMutationError("review_receipt_identity_unavailable") from exc
        if identity_result.returncode != 0:
            raise GitHubMutationError("review_receipt_identity_unavailable")
        authenticated_identity = identity_result.stdout.strip()
        if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,127}", authenticated_identity):
            raise GitHubMutationError("review_receipt_identity_malformed")

        def read_comments() -> list[dict[str, Any]]:
            try:
                result = subprocess.run(
                    [
                        "gh", "api", "--paginate", "--slurp",
                        f"repos/{repository}/issues/{pr_number}/comments?per_page=100",
                    ],
                    capture_output=True,
                    text=True,
                    timeout=self.timeout_seconds,
                    check=False,
                )
            except (OSError, subprocess.TimeoutExpired) as exc:
                raise GitHubMutationError("review_receipt_read_unavailable") from exc
            if result.returncode != 0:
                raise GitHubMutationError("review_receipt_read_failed")
            try:
                pages = json.loads(result.stdout or "[]")
            except json.JSONDecodeError as exc:
                raise GitHubMutationError("review_receipt_read_malformed") from exc
            if not isinstance(pages, list) or any(not isinstance(page, list) for page in pages):
                raise GitHubMutationError("review_receipt_read_malformed")
            return [item for page in pages for item in page if isinstance(item, dict)]

        marker = "EXACT-HEAD REVIEW RECEIPT"
        reviewed_marker = f"Reviewed SHA: {expected_head_sha}"
        current = [
            item for item in read_comments()
            if marker in str(item.get("body") or "") and reviewed_marker in str(item.get("body") or "")
        ]
        if len(current) > 1:
            raise GitHubMutationError("duplicate_exact_head_review_receipts")
        if current:
            return {
                "status": "ALREADY_PRESENT",
                "repository": repository,
                "pr_number": pr_number,
                "expected_head_sha": expected_head_sha,
            }
        observed_at = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
        body = "\n".join((
            "EXACT-HEAD REVIEW RECEIPT",
            f"Reviewed SHA: {expected_head_sha}",
            f"Reviewed range: {base_sha}...{expected_head_sha}",
            f"Reviewed range SHA256: {reviewed_range_sha256}",
            f"Reviewer session identity: {reviewer_session_id}",
            f"Reviewer authenticated identity: {authenticated_identity}",
            "Review transport: parent-posted-on-behalf-of-independent-session",
            f"Implementation session identity: {implementation_session_id}",
            f"Observed at: {observed_at}",
            "Axes: architecture, authority, compatibility, security, audit, rollback, scope/path binding",
            "Outcome: PASS",
            "Unresolved objections: none",
            f"Review receipt SHA256: {review_receipt_sha256}",
        )) + "\n"
        try:
            result = subprocess.run(
                ["gh", "api", "--method", "POST", f"repos/{repository}/issues/{pr_number}/comments", "-f", f"body={body}"],
                capture_output=True,
                text=True,
                timeout=self.timeout_seconds,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise GitHubMutationError("review_receipt_post_outcome_unknown") from exc
        if result.returncode != 0:
            raise GitHubMutationError("review_receipt_post_outcome_unknown")
        # Confirm the durable comment before reporting success.
        confirmed = [
            item for item in read_comments()
            if marker in str(item.get("body") or "") and reviewed_marker in str(item.get("body") or "")
        ]
        if len(confirmed) != 1:
            raise GitHubMutationError("review_receipt_post_unproven")
        return {
            "status": "PUBLISHED",
            "repository": repository,
            "pr_number": pr_number,
            "expected_head_sha": expected_head_sha,
        }

    def guarded_merge(
        self,
        repository: str,
        pr_number: int,
        expected_head_sha: str,
        *,
        merge_method: str = "squash",
        workflow_file: str = "agent-merge.yml",
        timeout_seconds: int = 120,
    ) -> dict[str, Any]:
        """Delegate merge strictly to the canonical agent-merge.yml workflow."""
        if (
            REPOSITORY.fullmatch(repository) is None
            or type(pr_number) is not int
            or pr_number < 1
            or SHA40.fullmatch(expected_head_sha) is None
        ):
            raise GitHubMutationError("github_merge_identity_invalid")

        facts = self.fetch_stage_pr(repository, pr_number)
        if facts.get("head_sha") != expected_head_sha:
            raise GitHubMutationError("head_sha_mismatch")

        if facts.get("merged") is True:
            return {
                "merged": True,
                "repository": repository,
                "pr_number": pr_number,
                "head_sha": expected_head_sha,
            }

        dispatch_cmd = [
            "gh", "workflow", "run", workflow_file,
            "--repo", repository,
            "-f", f"pr_number={pr_number}",
            "-f", f"head_sha={expected_head_sha}",
        ]
        try:
            dispatch_res = subprocess.run(
                dispatch_cmd,
                capture_output=True,
                text=True,
                timeout=min(self.timeout_seconds, 30),
                check=False,
            )
            if dispatch_res.returncode != 0:
                raise GitHubMutationError(f"merge_workflow_dispatch_failed: {dispatch_res.stderr.strip()}")
        except subprocess.TimeoutExpired as exc:
            try:
                if self.fetch_stage_pr(repository, pr_number).get("merged") is True:
                    return {"merged": True, "repository": repository, "pr_number": pr_number, "head_sha": expected_head_sha}
            except Exception:
                pass
            raise GitHubMutationError("merge_outcome_unknown") from exc
        except OSError as exc:
            raise GitHubMutationError("gh_cli_unavailable") from exc

        import time
        start_time = time.time()
        while time.time() - start_time < timeout_seconds:
            try:
                live_facts = self.fetch_stage_pr(repository, pr_number)
                if live_facts.get("merged") is True:
                    return {
                        "merged": True,
                        "repository": repository,
                        "pr_number": pr_number,
                        "head_sha": expected_head_sha,
                    }
                if live_facts.get("state") == "CLOSED" and not live_facts.get("merged"):
                    raise GitHubMutationError("stage_pr_closed_without_merge")
            except GitHubReadError:
                pass
            time.sleep(2)

        try:
            final_facts = self.fetch_stage_pr(repository, pr_number)
            if final_facts.get("merged") is True:
                return {
                    "merged": True,
                    "repository": repository,
                    "pr_number": pr_number,
                    "head_sha": expected_head_sha,
                }
        except Exception:
            pass
        raise GitHubMutationError("merge_outcome_unknown")

    def supersede_stage_pr(
        self,
        repository: str,
        pr_number: int,
        expected_head_sha: str,
    ) -> bool:
        """Close a failed candidate without deleting its recovery branch.

        This is not merge authority.  A replacement candidate gets a new
        branch and fresh exact-head review/CI; the closed PR remains the
        evidence trail for the failed attempt.
        """
        facts = self.fetch_stage_pr(repository, pr_number)
        if facts.get("head_sha") != expected_head_sha:
            raise GitHubMutationError("exact_head_mismatch_before_supersede")
        if facts.get("merged") is True:
            raise GitHubMutationError("merged_stage_cannot_be_superseded")
        if facts.get("state") == "CLOSED":
            return True
        try:
            result = subprocess.run(
                ["gh", "pr", "close", str(pr_number), "--repo", repository,
                 "--comment", "Superseded by an autonomous bounded repair candidate; branch retained for recovery."],
                capture_output=True,
                text=True,
                timeout=self.timeout_seconds,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise GitHubMutationError("supersede_stage_pr_outcome_unknown") from exc
        if result.returncode != 0:
            raise GitHubMutationError("supersede_stage_pr_failed")
        observed = self.fetch_stage_pr(repository, pr_number)
        if observed.get("state") != "CLOSED" or observed.get("merged") is True:
            raise GitHubMutationError("supersede_stage_pr_outcome_unknown")
        return True

    def post_merge_readback(
        self,
        repository: str,
        pr_number: int,
        expected_head_sha: str,
        *,
        timeout_seconds: int = 30,
    ) -> dict[str, Any]:
        """Prove that one exact merged PR advanced accepted ``main``.

        Reading a branch tip alone is not a merge receipt: another PR can
        advance ``main`` between the mutation and this read.  GitHub's PR
        object supplies the exact reviewed head and merge commit, and the
        branch endpoint proves that that merge commit is the accepted tip.
        This method deliberately has no local-Git fallback.
        """
        if (
            REPOSITORY.fullmatch(repository) is None
            or type(pr_number) is not int
            or pr_number < 1
            or SHA40.fullmatch(expected_head_sha) is None
        ):
            raise GitHubFactsError("github_query_identity_invalid")
        try:
            pr_res = subprocess.run(
                ["gh", "api", f"repos/{repository}/pulls/{pr_number}"],
                capture_output=True,
                text=True,
                timeout=timeout_seconds,
                check=False,
            )
            if pr_res.returncode != 0:
                raise GitHubFactsError("post_merge_pr_read_failed")
            pr = json.loads(pr_res.stdout)
            if not isinstance(pr, dict):
                raise GitHubFactsError("post_merge_pr_read_malformed")
            pr_head = pr.get("head", {}).get("sha") if isinstance(pr.get("head"), dict) else None
            merge_sha = pr.get("merge_commit_sha")
            if (
                pr.get("number") != pr_number
                or pr.get("state") != "closed"
                or pr.get("merged") is not True
                or pr_head != expected_head_sha
                or not isinstance(merge_sha, str)
                or SHA40.fullmatch(merge_sha) is None
            ):
                raise GitHubFactsError("post_merge_pr_binding_invalid")
            main_res = subprocess.run(
                ["gh", "api", f"repos/{repository}/branches/main"],
                capture_output=True,
                text=True,
                timeout=timeout_seconds,
                check=False,
            )
            if main_res.returncode != 0:
                raise GitHubFactsError("post_merge_main_read_failed")
            branch = json.loads(main_res.stdout)
            main_sha = branch.get("commit", {}).get("sha") if isinstance(branch, dict) and isinstance(branch.get("commit"), dict) else None
            if not isinstance(main_sha, str) or SHA40.fullmatch(main_sha) is None:
                raise GitHubFactsError("accepted_main_sha_invalid")
            if main_sha != merge_sha:
                raise GitHubFactsError("post_merge_main_transition_unproven")
            return {
                "schema_version": "post_merge_readback.v2",
                "repository": repository,
                "pr_number": pr_number,
                "expected_head_sha": expected_head_sha,
                "merge_commit_sha": merge_sha,
                "accepted_main_sha": main_sha,
                "status": "VERIFIED",
            }
        except json.JSONDecodeError as exc:
            raise GitHubFactsError("post_merge_readback_malformed") from exc
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise GitHubFactsError("post_merge_readback_unavailable") from exc


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

    def fetch_accepted_main(self, repository: str) -> str:
        value = self.facts.get("accepted_main_sha", "1" * 40)
        if not isinstance(value, str) or SHA40.fullmatch(value) is None:
            raise GitHubReadError("accepted_main_read_malformed")
        return value


class FakeGitHubWriter:
    """Deterministic in-memory writer and reader for testing."""

    def __init__(
        self,
        reader: FakeGitHubReader | None = None,
        *,
        initial_pr_number: int = 101,
    ):
        self._next_pr = initial_pr_number
        self.prs: dict[int, dict[str, Any]] = {}
        self.reader = reader or FakeGitHubReader()
        self.actions: list[tuple[str, dict[str, Any]]] = []
        self.fail_merge = False
        self.merge_outcome_unknown = False
        self.remote_main_sha = "1" * 40

    def fetch_stage_pr(self, repository: str, pr_number: int) -> dict[str, Any]:
        if pr_number in self.prs:
            pr = self.prs[pr_number]
            self.reader.reads.append((repository, pr_number))
            return {
                "repository": pr["repository"],
                "pr_number": pr["pr_number"],
                "state": pr["state"],
                "draft": pr["draft"],
                "merged": pr["merged"],
                "base_sha": pr["base_sha"],
                "head_sha": pr["head_sha"],
                "ci_state": pr["ci_state"],
                "review_state": pr["review_state"],
                "base_branch": pr["base_branch"],
                "head_branch": pr["head_branch"],
            }
        return self.reader.fetch_stage_pr(repository, pr_number)

    def fetch_accepted_main(self, repository: str) -> str:
        return self.remote_main_sha

    def create_or_update_stage_pr(
        self,
        stage_id: str,
        mission_id: str,
        branch: str,
        expected_sha: str,
        base_sha: str,
        title: str,
        body: str,
        repository: str,
    ) -> dict[str, Any]:
        for num, pr in self.prs.items():
            if pr.get("head_branch") == branch and pr.get("repository") == repository:
                pr["head_sha"] = expected_sha
                pr["base_sha"] = base_sha
                pr["title"] = title
                pr["body"] = body
                self.actions.append(("update_pr", {"pr_number": num, "stage_id": stage_id}))
                return dict(pr)
        num = self._next_pr
        self._next_pr += 1
        record = {
            "repository": repository,
            "pr_number": num,
            "number": num,
            "state": "OPEN",
            "draft": True,
            "merged": False,
            "base_sha": base_sha,
            "head_sha": expected_sha,
            "ci_state": "PENDING",
            "review_state": "PENDING",
            "base_branch": "main",
            "head_branch": branch,
            "stage_id": stage_id,
            "mission_id": mission_id,
            "title": title,
            "body": body,
        }
        self.prs[num] = record
        self.actions.append(("create_pr", {"pr_number": num, "stage_id": stage_id}))
        return dict(record)

    def mark_ready(
        self,
        repository: str,
        pr_number: int,
        expected_head_sha: str,
    ) -> bool:
        self.actions.append(("mark_ready", {"pr_number": pr_number, "head_sha": expected_head_sha}))
        if pr_number in self.prs:
            pr = self.prs[pr_number]
            if pr.get("head_sha") != expected_head_sha:
                raise GitHubMutationError("exact_head_mismatch_before_mark_ready")
            pr["draft"] = False
            return True
        return True

    def publish_exact_head_review(
        self,
        repository: str,
        pr_number: int,
        expected_head_sha: str,
        *,
        base_sha: str,
        reviewer_session_id: str,
        implementation_session_id: str,
        reviewed_range_sha256: str,
        review_receipt_sha256: str,
    ) -> dict[str, Any]:
        pr = self.prs.get(pr_number)
        if pr is not None:
            if pr.get("head_sha") != expected_head_sha or pr.get("base_sha") != base_sha:
                raise GitHubMutationError("review_receipt_exact_binding_mismatch")
            pr["review_state"] = "PASS"
        self.actions.append(("publish_review", {
            "pr_number": pr_number,
            "head_sha": expected_head_sha,
            "reviewer_session_id": reviewer_session_id,
        }))
        return {
            "status": "PUBLISHED",
            "repository": repository,
            "pr_number": pr_number,
            "expected_head_sha": expected_head_sha,
        }

    def guarded_merge(
        self,
        repository: str,
        pr_number: int,
        expected_head_sha: str,
        *,
        merge_method: str = "squash",
        workflow_file: str = "agent-merge.yml",
        timeout_seconds: int = 120,
    ) -> dict[str, Any]:
        self.actions.append(("merge", {"pr_number": pr_number, "head_sha": expected_head_sha, "workflow": workflow_file}))
        if self.fail_merge:
            raise GitHubMutationError("guarded_merge_rejected")
        if self.merge_outcome_unknown:
            raise GitHubMutationError("merge_outcome_unknown")
        if pr_number in self.prs:
            pr = self.prs[pr_number]
            if pr.get("head_sha") != expected_head_sha:
                raise GitHubMutationError("head_sha_mismatch")
            pr["merged"] = True
            pr["state"] = "CLOSED"
            # A deterministic stand-in for GitHub's merge commit.  Tests may
            # replace ``remote_main_sha`` to model unrelated main drift.
            pr["merge_commit_sha"] = expected_head_sha
            self.remote_main_sha = expected_head_sha
        return {
            "merged": True,
            "repository": repository,
            "pr_number": pr_number,
            "head_sha": expected_head_sha,
        }

    def supersede_stage_pr(
        self,
        repository: str,
        pr_number: int,
        expected_head_sha: str,
    ) -> bool:
        self.actions.append(("supersede", {"pr_number": pr_number, "head_sha": expected_head_sha}))
        pr = self.prs.get(pr_number)
        if pr is None or pr.get("head_sha") != expected_head_sha:
            raise GitHubMutationError("exact_head_mismatch_before_supersede")
        if pr.get("merged") is True:
            raise GitHubMutationError("merged_stage_cannot_be_superseded")
        pr["state"] = "CLOSED"
        return True

    def post_merge_readback(
        self,
        repository: str,
        pr_number: int,
        expected_head_sha: str,
        *,
        timeout_seconds: int = 30,
    ) -> dict[str, Any]:
        self.actions.append(("post_merge_readback", {"repository": repository, "pr_number": pr_number, "head_sha": expected_head_sha}))
        pr = self.prs.get(pr_number, {})
        merge_sha = pr.get("merge_commit_sha", self.remote_main_sha)
        if pr.get("head_sha") not in {None, expected_head_sha} or merge_sha != self.remote_main_sha:
            raise GitHubFactsError("post_merge_main_transition_unproven")
        return {
            "schema_version": "post_merge_readback.v2",
            "repository": repository,
            "pr_number": pr_number,
            "expected_head_sha": expected_head_sha,
            "merge_commit_sha": merge_sha,
            "accepted_main_sha": self.remote_main_sha,
            "status": "VERIFIED",
        }


__all__ = [
    "FakeGitHubReader",
    "FakeGitHubWriter",
    "GhGitHubWriter",
    "GhReadOnlyGitHub",
    "GitHubFactsError",
    "GitHubMutationError",
    "GitHubReadError",
    "GitHubWriter",
    "ReadOnlyGitHub",
    "StagePRFacts",
    "StagePRStatus",
    "reconcile_stage_pr",
]
