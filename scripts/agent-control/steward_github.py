"""Bounded GitHub facts and mutation transport for the Autonomous Steward.

The read adapter is the authority for exact PR/head/CI/review facts.  The
writer may create Draft PRs, publish idempotent review receipts, promote Ready,
supersede failed candidates, and dispatch the canonical merge workflow; it
never performs a direct merge.
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timezone
import hashlib
import json
import re
import subprocess
from typing import Any, Mapping, Protocol

import review_convergence


SHA40 = re.compile(r"^[0-9a-f]{40}$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")
IDENTIFIER = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$")
TIMESTAMP = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,6})?Z$")
REPOSITORY = re.compile(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")
BRANCH = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._/-]{0,199}$")
PR_STATES = frozenset({"OPEN", "CLOSED", "MERGED"})
CHECK_STATES = frozenset({"PASS", "PENDING", "FAIL", "UNKNOWN"})
REVIEW_STATES = frozenset(
    {"APPROVED", "CHANGES_REQUESTED", "COMMENTED", "DISMISSED", "PENDING"}
)
ORPHAN_DISPATCH_RECOVERY_MARKER = "steward-orphan-dispatch-recovery:v1"


def merge_dispatch_identity(
    repository: str,
    pr_number: int,
    expected_base_sha: str,
    expected_head_sha: str,
    *,
    workflow_file: str = "agent-merge.yml",
    ref: str = "main",
    intent_key: str = "",
) -> dict[str, Any]:
    """Build the immutable identity for one logical workflow dispatch.

    The intent key is journal-derived and therefore makes a later retry a new
    identity even when a provider reuses the same PR/head.  The returned
    ``dispatch_id`` is a digest, not a secret and never contains raw prompts or
    credentials.
    """
    if (
        not isinstance(repository, str)
        or REPOSITORY.fullmatch(repository) is None
        or type(pr_number) is not int
        or pr_number < 1
        or not isinstance(expected_base_sha, str)
        or SHA40.fullmatch(expected_base_sha) is None
        or not isinstance(expected_head_sha, str)
        or SHA40.fullmatch(expected_head_sha) is None
        or not isinstance(workflow_file, str)
        or re.fullmatch(r"[A-Za-z0-9_.-]{1,128}", workflow_file) is None
        or not isinstance(ref, str)
        or ref != "main"
        or not isinstance(intent_key, str)
        or len(intent_key) > 512
        or "\n" in intent_key
    ):
        raise GitHubFactsError("merge_dispatch_identity_invalid")
    wire = {
        "repository": repository,
        "pr_number": pr_number,
        "base_sha": expected_base_sha,
        "head_sha": expected_head_sha,
        "workflow_file": workflow_file,
        "ref": ref,
        "intent_key": intent_key,
    }
    encoded = json.dumps(wire, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return {
        "schema_version": "merge_dispatch_identity.v1",
        **wire,
        "dispatch_id": hashlib.sha256(encoded).hexdigest(),
    }


class GitHubFactsError(RuntimeError):
    """Facts were unavailable, malformed, or bound to a different subject."""


class GitHubReadError(GitHubFactsError):
    """The read-only GitHub query did not return usable facts."""


class ReadOnlyGitHub(Protocol):
    def fetch_stage_pr(self, repository: str, pr_number: int) -> dict[str, Any]:
        """Return bounded facts for one PR; never perform a mutation."""

    def fetch_accepted_main(self, repository: str) -> str:
        """Return the authoritative accepted main SHA; never perform a mutation."""


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
            required_jobs = set(REQUIRED_CI_JOBS)
            # A PR can retain historical advisory/Draft checks (for example a
            # failed ``fast-pr-checks`` run) after every canonical required
            # check succeeds.  Merge eligibility is defined by the required
            # matrix only, so collapse duplicate required names to the newest
            # observed run and ignore unrelated checks.
            named_items = [
                (CI_CHECK_ALIASES.get(item["name"], item["name"]), item, index)
                for index, item in enumerate(check_items)
                if isinstance(item.get("name"), str)
            ]
            malformed_named_items = bool(named_items) and any(
                not isinstance(item.get("name"), str) or not item["name"].strip()
                for item in check_items
            )
            if named_items:
                latest: dict[str, tuple[tuple[str, str, str, int], dict[str, Any]]] = {}
                for logical, item, index in named_items:
                    if logical not in required_jobs:
                        continue
                    stamp = (
                        str(item.get("completedAt") or ""),
                        str(item.get("startedAt") or ""),
                        str(item.get("databaseId") or ""),
                        index,
                    )
                    previous = latest.get(logical)
                    if previous is None or stamp >= previous[0]:
                        latest[logical] = (stamp, item)
                canonical_items = [item for _stamp, item in latest.values()]
            else:
                # Preserve the strict pending/unknown behavior for malformed
                # legacy payloads that omit check names entirely.
                canonical_items = check_items
            conclusions = [item.get("conclusion") for item in canonical_items]
            statuses = {item.get("status") for item in canonical_items}
            observed_names = {
                CI_CHECK_ALIASES.get(item["name"], item["name"])
                for item in canonical_items
                if isinstance(item.get("name"), str)
            }
            if "FAILURE" in conclusions or "CANCELLED" in conclusions:
                ci_state = "FAIL"
            elif malformed_named_items:
                # A named rollup that also contains an unnamed/malformed
                # entry cannot prove that the canonical matrix is complete.
                # Do not silently discard that entry and return PASS.
                ci_state = "UNKNOWN"
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
            elif len(canonical_items) == len(required_jobs) and all(
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

    def __init__(self, message: str, *, evidence: Mapping[str, Any] | None = None):
        super().__init__(message)
        self.evidence = dict(evidence or {})


class GitHubPreflightError(GitHubMutationError):
    """A mutation was rejected by read-only checks before any request was sent."""


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
        expected_base_sha: str | None = None,
        dispatch_id: str | None = None,
        intent_key: str = "compatibility-call",
        merge_method: str = "squash",
        workflow_file: str = "agent-merge.yml",
        timeout_seconds: int = 120,
    ) -> dict[str, Any]:
        ...

    def reconcile_merge_dispatch(
        self,
        repository: str,
        pr_number: int,
        expected_head_sha: str,
        *,
        workflow_file: str = "agent-merge.yml",
        not_before: str | None = None,
        expected_base_sha: str | None = None,
        dispatch_id: str,
        workflow_run_id: int | None = None,
    ) -> dict[str, Any]:
        """Read-only reconciliation for an interrupted merge workflow dispatch."""
        ...

    def read_orphan_dispatch_recovery_authorization(
        self,
        repository: str,
        control_issue_number: int,
        *,
        mission_id: str,
        proposal_sha256: str,
        stage_id: str,
        pr_number: int,
        expected_base_sha: str,
        expected_head_sha: str,
        workflow_file: str,
        dispatch_id: str,
        owner_identity: str,
    ) -> dict[str, Any] | None:
        """Read one owner-authenticated recovery authorization marker."""
        ...

    def quarantine_stage_pr(
        self,
        repository: str,
        pr_number: int,
        *,
        expected_base_sha: str,
        expected_head_sha: str,
    ) -> dict[str, Any]:
        """Close one exact unmerged PR after readback-safe preflight."""
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

    def fetch_accepted_main(self, repository: str) -> str:
        return self.reader.fetch_accepted_main(repository)

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
            raise GitHubPreflightError("review_receipt_exact_binding_mismatch")
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
        expected_base_sha: str | None = None,
        dispatch_id: str | None = None,
        intent_key: str = "compatibility-call",
        merge_method: str = "squash",
        workflow_file: str = "agent-merge.yml",
        timeout_seconds: int = 120,
    ) -> dict[str, Any]:
        """Delegate merge strictly to the canonical agent-merge.yml workflow."""
        if (
            not isinstance(repository, str)
            or REPOSITORY.fullmatch(repository) is None
            or type(pr_number) is not int
            or pr_number < 1
            or not isinstance(expected_head_sha, str)
            or SHA40.fullmatch(expected_head_sha) is None
        ):
            raise GitHubMutationError("github_merge_identity_invalid")

        facts = self.fetch_stage_pr(repository, pr_number)
        if facts.get("head_sha") != expected_head_sha:
            raise GitHubMutationError("head_sha_mismatch")
        if expected_base_sha is not None and facts.get("base_sha") != expected_base_sha:
            raise GitHubPreflightError("base_sha_mismatch")

        actual_base_sha = expected_base_sha or facts.get("base_sha")
        if not isinstance(actual_base_sha, str) or SHA40.fullmatch(actual_base_sha) is None:
            raise GitHubMutationError("merge_base_sha_unavailable")
        identity = merge_dispatch_identity(
            repository,
            pr_number,
            actual_base_sha,
            expected_head_sha,
            workflow_file=workflow_file,
            intent_key=intent_key,
        )
        if dispatch_id is not None and dispatch_id != identity["dispatch_id"]:
            raise GitHubMutationError("merge_dispatch_id_mismatch")

        if facts.get("merged") is True:
            return {
                "merged": True,
                "repository": repository,
                "pr_number": pr_number,
                "head_sha": expected_head_sha,
                "dispatch_id": identity["dispatch_id"],
            }

        dispatch_cmd = [
            "gh", "api", "--method", "POST",
            f"repos/{repository}/actions/workflows/{workflow_file}/dispatches",
            "--input", "-",
        ]
        dispatch_payload = {
            "ref": "main",
            "return_run_details": True,
            "inputs": {
                "pr_number": str(pr_number),
                "head_sha": expected_head_sha,
                "dispatch_id": identity["dispatch_id"],
            },
        }
        dispatch_evidence = {
            "repository": repository,
            "pr_number": pr_number,
            "expected_base_sha": actual_base_sha,
            "expected_head_sha": expected_head_sha,
            "workflow_file": workflow_file,
            "ref": "main",
            "dispatch_id": identity["dispatch_id"],
        }
        try:
            dispatch_res = subprocess.run(
                dispatch_cmd,
                capture_output=True,
                text=True,
                input=json.dumps(dispatch_payload, sort_keys=True, separators=(",", ":")),
                timeout=min(self.timeout_seconds, 30),
                check=False,
            )
            if dispatch_res.returncode != 0:
                raise GitHubMutationError(
                    "merge_workflow_dispatch_failed",
                    evidence=dispatch_evidence,
                )
            try:
                dispatch_response = json.loads(dispatch_res.stdout or "")
            except json.JSONDecodeError as exc:
                raise GitHubMutationError(
                    "merge_dispatch_run_id_unavailable",
                    evidence=dispatch_evidence,
                ) from exc
            if not isinstance(dispatch_response, dict):
                raise GitHubMutationError(
                    "merge_dispatch_run_id_unavailable",
                    evidence=dispatch_evidence,
                )
            run_id = dispatch_response.get("workflow_run_id")
            run_url = dispatch_response.get("run_url")
            html_url = dispatch_response.get("html_url")
            if (
                type(run_id) is not int
                or run_id < 1
                or not isinstance(run_url, str)
                or not isinstance(html_url, str)
            ):
                raise GitHubMutationError(
                    "merge_dispatch_run_id_unavailable",
                    evidence=dispatch_evidence,
                )
            dispatch_evidence.update(
                {
                    "workflow_run_id": run_id,
                    "run_url": run_url,
                    "html_url": html_url,
                }
            )
        except subprocess.TimeoutExpired as exc:
            try:
                if self.fetch_stage_pr(repository, pr_number).get("merged") is True:
                    return {
                        "merged": True,
                        "repository": repository,
                        "pr_number": pr_number,
                        "head_sha": expected_head_sha,
                        **dispatch_evidence,
                    }
            except Exception:
                pass
            raise GitHubMutationError("merge_outcome_unknown", evidence=dispatch_evidence) from exc
        except OSError as exc:
            raise GitHubMutationError("gh_cli_unavailable", evidence=dispatch_evidence) from exc
        # The run identity is returned before any merge outcome is inferred.
        # The caller must durably journal this receipt first, then use the
        # read-only run/PR/main reconciler.  A successful dispatch is not a
        # successful merge and this method intentionally does not poll by
        # elapsed time.
        return {
            "status": "DISPATCHED",
            "merged": False,
            "repository": repository,
            "pr_number": pr_number,
            "head_sha": expected_head_sha,
            **dispatch_evidence,
        }

    def reconcile_merge_dispatch(
        self,
        repository: str,
        pr_number: int,
        expected_head_sha: str,
        *,
        workflow_file: str = "agent-merge.yml",
        not_before: str | None = None,
        expected_base_sha: str | None = None,
        dispatch_id: str,
        workflow_run_id: int | None = None,
    ) -> dict[str, Any]:
        """Reconcile one interrupted workflow dispatch without writing.

        A new dispatch records the run ID returned by the REST dispatch API.
        An older intent may have no run ID, so the read-only fallback scans
        exact-head workflow runs and binds terminal runs through their complete
        PR/head/dispatch log markers. An empty scan is deliberately
        ``NOT_PROVEN``; only an authenticated owner resolution can terminate
        an orphan with no durable run identity.
        """

        if (
            not isinstance(repository, str)
            or REPOSITORY.fullmatch(repository) is None
            or type(pr_number) is not int
            or pr_number < 1
            or not isinstance(expected_head_sha, str)
            or SHA40.fullmatch(expected_head_sha) is None
            or not isinstance(workflow_file, str)
            or re.fullmatch(r"[A-Za-z0-9_.-]{1,128}", workflow_file) is None
            or (not_before is not None and not isinstance(not_before, str))
            or (
                expected_base_sha is not None
                and (
                    not isinstance(expected_base_sha, str)
                    or SHA40.fullmatch(expected_base_sha) is None
                )
            )
            or not isinstance(dispatch_id, str)
            or SHA256.fullmatch(dispatch_id) is None
            or (workflow_run_id is not None and (type(workflow_run_id) is not int or workflow_run_id < 1))
        ):
            raise GitHubReadError("merge_reconcile_identity_invalid")
        result_base: dict[str, Any] = {
            "repository": repository,
            "pr_number": pr_number,
            "expected_head_sha": expected_head_sha,
        }
        if expected_base_sha is not None:
            result_base["expected_base_sha"] = expected_base_sha
        result_base["dispatch_id"] = dispatch_id

        known_run = workflow_run_id is not None
        not_before_at = None
        if not_before is not None:
            try:
                not_before_at = _review_timestamp(not_before)
            except GitHubReadError as exc:
                raise GitHubReadError("merge_reconcile_not_before_malformed") from exc
        if known_run:
            list_command = [
                "gh", "api",
                f"repos/{repository}/actions/runs/{workflow_run_id}",
            ]
        else:
            # ``workflow_dispatch`` runs are created from the selected ref.
            # The canonical merge caller dispatches ``main`` and carries the
            # PR head as an input, so the run list's ``head_sha`` is the
            # selected main ref SHA, not the PR head.  Querying by the PR head
            # would hide the very run needed to reconcile an orphan.
            run_query = "event=workflow_dispatch&branch=main&per_page=100"
            if not_before is not None:
                # The API's created filter is only a narrowing hint; the
                # precise timestamp fence is still checked below.
                run_query += f"&created=>={not_before[:10]}"
            list_command = [
                "gh", "api", "--paginate", "--slurp",
                f"repos/{repository}/actions/workflows/{workflow_file}/runs"
                f"?{run_query}",
            ]
        try:
            listed = subprocess.run(
                list_command,
                capture_output=True,
                text=True,
                timeout=self.timeout_seconds,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise GitHubReadError("merge_reconcile_runs_unavailable") from exc
        if listed.returncode != 0:
            raise GitHubReadError("merge_reconcile_runs_failed")
        try:
            payload = json.loads(listed.stdout or "")
        except json.JSONDecodeError as exc:
            raise GitHubReadError("merge_reconcile_runs_malformed") from exc
        if known_run:
            if not isinstance(payload, dict):
                raise GitHubReadError("merge_reconcile_run_malformed")
            runs = [payload]
        else:
            if not isinstance(payload, list) or any(not isinstance(page, dict) for page in payload):
                raise GitHubReadError("merge_reconcile_runs_malformed")
            runs = []
            for page in payload:
                page_runs = page.get("workflow_runs")
                if not isinstance(page_runs, list) or any(not isinstance(item, dict) for item in page_runs):
                    raise GitHubReadError("merge_reconcile_runs_malformed")
                runs.extend(page_runs)

        matches: list[dict[str, Any]] = []
        active_run_ids: list[int] = []
        for run in runs:
            run_id = run.get("id")
            status = run.get("status")
            conclusion = run.get("conclusion")
            created_at = run.get("created_at")
            head_sha = run.get("head_sha")
            head_branch = run.get("head_branch")
            event = run.get("event")
            path = run.get("path")
            if (
                type(run_id) is not int
                or run_id < 1
                or not isinstance(status, str)
                or not isinstance(created_at, str)
                or not isinstance(head_sha, str)
                or not isinstance(head_branch, str)
                or not isinstance(event, str)
                or not isinstance(path, str)
            ):
                raise GitHubReadError("merge_reconcile_run_identity_malformed")
            if known_run and run_id != workflow_run_id:
                return {
                    **result_base,
                    "status": "NOT_PROVEN",
                    "run_ids": [run_id],
                    "binding_mismatch": True,
                }
            if status not in {
                "queued",
                "requested",
                "waiting",
                "pending",
                "in_progress",
                "completed",
            }:
                raise GitHubReadError("merge_reconcile_run_status_malformed")
            if conclusion is not None and not isinstance(conclusion, str):
                raise GitHubReadError("merge_reconcile_run_conclusion_malformed")
            try:
                created_at_value = _review_timestamp(created_at)
            except GitHubReadError as exc:
                raise GitHubReadError("merge_reconcile_run_timestamp_malformed") from exc
            if not_before_at is not None and created_at_value < not_before_at:
                continue
            workflow_path = f".github/workflows/{workflow_file}"
            path_matches = (
                path in {workflow_file, workflow_path}
                or path.startswith(workflow_path + "@")
            )
            exact_run_binding = (
                (expected_base_sha is None or head_sha == expected_base_sha)
                and head_branch == "main"
                and event == "workflow_dispatch"
                and path_matches
            )
            if not exact_run_binding:
                if known_run:
                    return {
                        **result_base,
                        "status": "NOT_PROVEN",
                        "run_ids": [run_id],
                        "binding_mismatch": True,
                    }
                continue
            if status in {"queued", "requested", "waiting", "pending", "in_progress"}:
                # A list response does not expose workflow_dispatch inputs. An
                # active exact-head run could still target this PR, so keep the
                # intent pending until it is terminal.
                active_run_ids.append(run_id)
                continue
            # Logs are the only durable source available for workflow_dispatch
            # inputs.  This is required even for a run ID returned directly by
            # the dispatch API: the workflow-run object does not expose the
            # workflow_dispatch inputs, and its head is the dispatch ref
            # (`main`), not the PR head.  Read the complete terminal log, not
            # only failed-step output: the exact PR/head binding is emitted by
            # a successful preflight step before a later check can fail.
            # Never dispatch, cancel, or rerun here.
            try:
                detail = subprocess.run(
                    [
                        "gh",
                        "run",
                        "view",
                        str(run_id),
                        "--repo",
                        repository,
                        "--log",
                    ],
                    capture_output=True,
                    text=True,
                    timeout=min(self.timeout_seconds, 15),
                    check=False,
                )
            except (OSError, subprocess.TimeoutExpired) as exc:
                raise GitHubReadError("merge_reconcile_run_log_unavailable") from exc
            if detail.returncode != 0:
                raise GitHubReadError("merge_reconcile_run_log_failed")
            log = detail.stdout
            # GitHub prefixes log lines with step/timestamp text, so parse the
            # emitted fields rather than requiring a literal whole line.  The
            # captured values themselves must be complete fields: a run for
            # PR 170 must never bind an intent for PR 17.
            logged_pr_numbers = {
                int(match.group(1))
                for line in log.splitlines()
                if (
                    match := re.search(
                        r"\bPR_NUMBER:\s*(\d+)\s*$", line.strip()
                    )
                )
            }
            logged_heads = {
                match.group(1)
                for line in log.splitlines()
                if (
                    match := re.search(
                        r"\bEXPECTED_HEAD:\s*([0-9a-f]{40})\s*$",
                        line.strip(),
                    )
                )
            }
            logged_dispatch_ids = {
                match.group(1)
                for line in log.splitlines()
                if (
                    match := re.search(
                        r"\bDISPATCH_ID:\s*([0-9a-f]{64})\s*$",
                        line.strip(),
                    )
                )
            }
            if (
                pr_number not in logged_pr_numbers
                or expected_head_sha not in logged_heads
                or dispatch_id not in logged_dispatch_ids
            ):
                if known_run:
                    return {
                        **result_base,
                        "status": "NOT_PROVEN",
                        "run_ids": [run_id],
                        "binding_mismatch": True,
                    }
                continue
            matches.append(
                {
                    "run_id": run_id,
                    "status": status,
                    "conclusion": conclusion,
                    "created_at": created_at,
                }
            )

        if active_run_ids:
            return {
                **result_base,
                "status": "PENDING",
                "repository": repository,
                "pr_number": pr_number,
                "expected_head_sha": expected_head_sha,
                "run_ids": active_run_ids + [item["run_id"] for item in matches],
            }
        if not matches:
            return {
                **result_base,
                "status": "NOT_PROVEN",
                "repository": repository,
                "pr_number": pr_number,
                "expected_head_sha": expected_head_sha,
                "run_ids": [],
            }
        if any(item["conclusion"] == "success" for item in matches):
            return {
                **result_base,
                "status": "SUCCEEDED",
                "repository": repository,
                "pr_number": pr_number,
                "expected_head_sha": expected_head_sha,
                "run_ids": [item["run_id"] for item in matches],
            }
        if all(item["conclusion"] in {"failure", "cancelled", "timed_out", "action_required"} for item in matches):
            return {
                **result_base,
                "status": "REJECTED",
                "repository": repository,
                "pr_number": pr_number,
                "expected_head_sha": expected_head_sha,
                "run_ids": [item["run_id"] for item in matches],
            }
        return {
            **result_base,
            "status": "NOT_PROVEN",
            "repository": repository,
            "pr_number": pr_number,
            "expected_head_sha": expected_head_sha,
            "run_ids": [item["run_id"] for item in matches],
        }

    def read_orphan_dispatch_recovery_authorization(
        self,
        repository: str,
        control_issue_number: int,
        *,
        mission_id: str,
        proposal_sha256: str,
        stage_id: str,
        pr_number: int,
        expected_base_sha: str,
        expected_head_sha: str,
        workflow_file: str,
        dispatch_id: str,
        owner_identity: str,
    ) -> dict[str, Any] | None:
        """Read one owner-authenticated recovery authorization marker.

        The comment author and ``created_at`` returned by GitHub are the only
        authority and temporal evidence.  The marker is deliberately not
        allowed to carry a factual resolution or caller-supplied timestamp.
        """
        if (
            not isinstance(repository, str)
            or REPOSITORY.fullmatch(repository) is None
            or type(control_issue_number) is not int
            or control_issue_number < 1
            or not isinstance(mission_id, str)
            or not IDENTIFIER.fullmatch(mission_id)
            or not isinstance(proposal_sha256, str)
            or not SHA256.fullmatch(proposal_sha256)
            or not isinstance(stage_id, str)
            or not IDENTIFIER.fullmatch(stage_id)
            or type(pr_number) is not int
            or pr_number < 1
            or not isinstance(expected_base_sha, str)
            or not SHA40.fullmatch(expected_base_sha)
            or not isinstance(expected_head_sha, str)
            or not SHA40.fullmatch(expected_head_sha)
            or not isinstance(workflow_file, str)
            or re.fullmatch(r"[A-Za-z0-9_.-]{1,128}", workflow_file) is None
            or not isinstance(dispatch_id, str)
            or not SHA256.fullmatch(dispatch_id)
            or owner_identity not in {"github:Igzela"}
        ):
            raise GitHubReadError("merge_resolution_identity_invalid")
        try:
            result = subprocess.run(
                [
                    "gh", "api", "--paginate", "--slurp",
                    f"repos/{repository}/issues/{control_issue_number}/comments?per_page=100",
                ],
                capture_output=True,
                text=True,
                timeout=self.timeout_seconds,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise GitHubReadError("merge_resolution_read_unavailable") from exc
        if result.returncode != 0:
            raise GitHubReadError("merge_resolution_read_failed")
        try:
            pages = json.loads(result.stdout or "[]")
        except json.JSONDecodeError as exc:
            raise GitHubReadError("merge_resolution_read_malformed") from exc
        if not isinstance(pages, list) or any(not isinstance(page, list) for page in pages):
            raise GitHubReadError("merge_resolution_read_malformed")

        expected_fields = {
            "mission_id", "proposal_sha256", "stage_id", "repository",
            "control_issue_number", "pr_number", "base_sha", "head_sha",
            "workflow_file", "ref", "dispatch_id", "authorization",
            "action", "authorization_id",
        }
        matches: list[dict[str, Any]] = []
        for page in pages:
            for comment in page:
                if not isinstance(comment, dict):
                    continue
                body = comment.get("body")
                if not isinstance(body, str) or len(body) > 16 * 1024:
                    continue
                marker_match = re.search(
                    rf"<!--\s*{re.escape(ORPHAN_DISPATCH_RECOVERY_MARKER)}\s+(\{{.*?\}})\s*-->",
                    body,
                    re.DOTALL,
                )
                if marker_match is None:
                    continue
                if str(comment.get("author_association", "")).upper() != "OWNER":
                    continue
                author = comment.get("user")
                login = author.get("login") if isinstance(author, dict) else None
                if owner_identity != f"github:{login}":
                    continue
                try:
                    marker = json.loads(marker_match.group(1))
                except json.JSONDecodeError as exc:
                    raise GitHubFactsError("merge_resolution_marker_invalid") from exc
                if not isinstance(marker, dict) or set(marker) != expected_fields:
                    raise GitHubFactsError("merge_resolution_marker_invalid")
                if (
                    marker.get("mission_id") != mission_id
                    or marker.get("proposal_sha256") != proposal_sha256
                    or marker.get("stage_id") != stage_id
                    or marker.get("repository") != repository
                    or marker.get("control_issue_number") != control_issue_number
                    or marker.get("pr_number") != pr_number
                    or marker.get("base_sha") != expected_base_sha
                    or marker.get("head_sha") != expected_head_sha
                    or marker.get("workflow_file") != workflow_file
                    or marker.get("ref") != "main"
                    or marker.get("dispatch_id") != dispatch_id
                    or marker.get("authorization") != "ORPHAN_DISPATCH_RECOVERY"
                    or marker.get("action") != "QUARANTINE_EXACT_PR"
                    or not isinstance(marker.get("authorization_id"), str)
                    or not IDENTIFIER.fullmatch(marker["authorization_id"])
                ):
                    continue
                comment_created_at = comment.get("created_at")
                if (
                    not isinstance(comment_created_at, str)
                    or TIMESTAMP.fullmatch(comment_created_at) is None
                ):
                    raise GitHubFactsError("merge_resolution_comment_timestamp_invalid")
                try:
                    _review_timestamp(comment_created_at)
                except GitHubReadError as exc:
                    raise GitHubFactsError("merge_resolution_comment_timestamp_invalid") from exc
                comment_id = comment.get("id")
                if type(comment_id) is not int or comment_id < 1:
                    raise GitHubFactsError("merge_resolution_comment_identity_invalid")
                matches.append({
                    **marker,
                    "comment_id": comment_id,
                    "comment_created_at": comment_created_at,
                    "owner_identity": owner_identity,
                })
        if len(matches) > 1:
            raise GitHubFactsError("duplicate_merge_resolution_markers")
        return matches[0] if matches else None

    def quarantine_stage_pr(
        self,
        repository: str,
        pr_number: int,
        *,
        expected_base_sha: str,
        expected_head_sha: str,
        comment: str | None = None,
    ) -> dict[str, Any]:
        """Quarantine one exact orphan candidate and return GitHub facts.

        This is the recovery mutation for an unresolved merge candidate under
        standing Mission repository-maintenance recovery authority (or legacy
        owner authorization).  It performs a fresh PR/base/main preflight,
        issues only the exact close operation, then reads PR and accepted-main
        again.  The result is never an assumption: ``MERGED`` or
        ``CLOSED_UNMERGED`` comes only from that authoritative post-mutation
        readback; anything else is unknown.
        """
        if (
            not isinstance(repository, str)
            or REPOSITORY.fullmatch(repository) is None
            or type(pr_number) is not int
            or pr_number < 1
            or not isinstance(expected_base_sha, str)
            or SHA40.fullmatch(expected_base_sha) is None
            or not isinstance(expected_head_sha, str)
            or SHA40.fullmatch(expected_head_sha) is None
        ):
            raise GitHubFactsError("quarantine_identity_invalid")

        facts = self.fetch_stage_pr(repository, pr_number)
        if (
            facts.get("repository") != repository
            or facts.get("pr_number") != pr_number
            or facts.get("base_sha") != expected_base_sha
            or facts.get("head_sha") != expected_head_sha
        ):
            raise GitHubFactsError("quarantine_pr_identity_mismatch")

        current_main = self.fetch_accepted_main(repository)
        if not isinstance(current_main, str) or SHA40.fullmatch(current_main) is None:
            raise GitHubReadError("quarantine_accepted_main_malformed")
        if facts.get("merged") is True:
            return {
                "status": "MERGED",
                "repository": repository,
                "pr_number": pr_number,
                "base_sha": expected_base_sha,
                "head_sha": expected_head_sha,
                "accepted_main_sha": current_main,
            }
        if facts.get("state") == "CLOSED" and facts.get("merged") is False:
            return {
                "status": "CLOSED_UNMERGED",
                "repository": repository,
                "pr_number": pr_number,
                "base_sha": expected_base_sha,
                "head_sha": expected_head_sha,
                "accepted_main_sha": current_main,
            }
        if facts.get("state") != "OPEN" or facts.get("merged") is not False:
            raise GitHubFactsError("quarantine_pr_state_unproven")
        evidence = {
            "repository": repository,
            "pr_number": pr_number,
            "expected_base_sha": expected_base_sha,
            "expected_head_sha": expected_head_sha,
            "preflight_accepted_main_sha": current_main,
            "action": "QUARANTINE_EXACT_PR",
        }
        close_comment = (
            comment
            if comment is not None
            else "Quarantined under Mission repository-maintenance recovery authority; branch retained."
        )
        try:
            result = subprocess.run(
                [
                    "gh", "pr", "close", str(pr_number), "--repo", repository,
                    "--comment",
                    close_comment,
                ],
                capture_output=True,
                text=True,
                timeout=self.timeout_seconds,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise GitHubMutationError("quarantine_outcome_unknown", evidence=evidence) from exc

        try:
            observed = self.fetch_stage_pr(repository, pr_number)
            observed_main = self.fetch_accepted_main(repository)
        except (GitHubReadError, GitHubFactsError, OSError) as exc:
            raise GitHubMutationError("quarantine_outcome_unknown", evidence=evidence) from exc
        if (
            not isinstance(observed_main, str)
            or SHA40.fullmatch(observed_main) is None
            or observed.get("repository") != repository
            or observed.get("pr_number") != pr_number
            or observed.get("base_sha") != expected_base_sha
            or observed.get("head_sha") != expected_head_sha
        ):
            raise GitHubMutationError("quarantine_outcome_unknown", evidence=evidence)
        if observed.get("merged") is True:
            return {
                **evidence,
                "status": "MERGED",
                "head_sha": expected_head_sha,
                "accepted_main_sha": observed_main,
            }
        if observed.get("state") == "CLOSED" and observed.get("merged") is False:
            return {
                **evidence,
                "status": "CLOSED_UNMERGED",
                "head_sha": expected_head_sha,
                "accepted_main_sha": observed_main,
            }
        if result.returncode != 0:
            # A close request can lose a race with the old workflow and
            # return an error for an already-merged PR.  The post-request
            # authoritative readback above, not the CLI exit code, decides
            # whether that race produced MERGED, CLOSED_UNMERGED, or remains
            # unknown.
            raise GitHubMutationError("quarantine_outcome_unknown", evidence=evidence)
        raise GitHubMutationError("quarantine_outcome_unknown", evidence=evidence)

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
        self.review_receipts: set[tuple[int, str, str]] = set()
        self.merge_dispatch_resolutions: list[dict[str, Any]] = []
        self._next_workflow_run_id = 10_000
        # Fault-injection seam: an already-issued old workflow can win the
        # race between quarantine preflight and the close request.
        self.quarantine_race_merge = False

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
                raise GitHubPreflightError("review_receipt_exact_binding_mismatch")
            pr["review_state"] = "PASS"
        self.review_receipts.add((pr_number, expected_head_sha, review_receipt_sha256))
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
        expected_base_sha: str | None = None,
        dispatch_id: str | None = None,
        intent_key: str = "compatibility-call",
        merge_method: str = "squash",
        workflow_file: str = "agent-merge.yml",
        timeout_seconds: int = 120,
    ) -> dict[str, Any]:
        actual_base_sha = expected_base_sha
        if actual_base_sha is None and pr_number in self.prs:
            actual_base_sha = self.prs[pr_number].get("base_sha")
        if not isinstance(actual_base_sha, str):
            raise GitHubMutationError("merge_base_sha_unavailable")
        identity = merge_dispatch_identity(
            repository,
            pr_number,
            actual_base_sha,
            expected_head_sha,
            workflow_file=workflow_file,
            intent_key=intent_key,
        )
        if dispatch_id is not None and dispatch_id != identity["dispatch_id"]:
            raise GitHubMutationError("merge_dispatch_id_mismatch")
        if self.fail_merge:
            raise GitHubMutationError("guarded_merge_rejected", evidence={"dispatch_id": identity["dispatch_id"]})
        if self.merge_outcome_unknown:
            raise GitHubMutationError(
                "merge_outcome_unknown",
                evidence={"dispatch_id": identity["dispatch_id"]},
            )
        pr = self.prs.get(pr_number)
        if pr is None or pr.get("repository") != repository:
            raise GitHubReadError("merge_pr_read_unavailable")
        if (
            pr.get("head_sha") != expected_head_sha
            or pr.get("base_sha") != actual_base_sha
        ):
            raise GitHubMutationError("merge_head_or_base_mismatch")
        if pr.get("state") != "OPEN" or pr.get("merged") is not False:
            # This models the canonical workflow's open-PR preflight.  A
            # delayed old workflow therefore cannot merge a quarantined PR.
            raise GitHubMutationError("merge_pr_not_open")
        self.actions.append(("merge", {"pr_number": pr_number, "head_sha": expected_head_sha, "workflow": workflow_file}))
        run_id = self._next_workflow_run_id
        self._next_workflow_run_id += 1
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
            "dispatch_id": identity["dispatch_id"],
            "workflow_run_id": run_id,
        }

    def read_orphan_dispatch_recovery_authorization(
        self,
        repository: str,
        control_issue_number: int,
        *,
        mission_id: str,
        proposal_sha256: str,
        stage_id: str,
        pr_number: int,
        expected_base_sha: str,
        expected_head_sha: str,
        workflow_file: str,
        dispatch_id: str,
        owner_identity: str,
    ) -> dict[str, Any] | None:
        expected = {
            "mission_id": mission_id,
            "proposal_sha256": proposal_sha256,
            "stage_id": stage_id,
            "repository": repository,
            "control_issue_number": control_issue_number,
            "pr_number": pr_number,
            "base_sha": expected_base_sha,
            "head_sha": expected_head_sha,
            "workflow_file": workflow_file,
            "ref": "main",
            "dispatch_id": dispatch_id,
            "authorization": "ORPHAN_DISPATCH_RECOVERY",
            "action": "QUARANTINE_EXACT_PR",
            "owner_identity": owner_identity,
        }
        # The fake transport stores authenticated response metadata beside
        # the payload. Keep its accepted shape strict like the real GitHub
        # parser: outcome claims and caller-supplied timestamps must never be
        # silently ignored by a test adapter.
        transport_fields = {
            *expected.keys(),
            "authorization_id",
            "comment_id",
            "comment_created_at",
        }
        matches = [
            item for item in self.merge_dispatch_resolutions
            if set(item) == transport_fields
            and isinstance(item.get("authorization_id"), str)
            and IDENTIFIER.fullmatch(item["authorization_id"]) is not None
            and all(item.get(key) == value for key, value in expected.items())
        ]
        if len(matches) > 1:
            raise GitHubFactsError("duplicate_merge_resolution_markers")
        return dict(matches[0]) if matches else None

    def quarantine_stage_pr(
        self,
        repository: str,
        pr_number: int,
        *,
        expected_base_sha: str,
        expected_head_sha: str,
        comment: str | None = None,
    ) -> dict[str, Any]:
        pr = self.prs.get(pr_number)
        if pr is None:
            raise GitHubReadError("quarantine_pr_read_unavailable")
        if (
            pr.get("repository") != repository
            or pr.get("pr_number") != pr_number
            or pr.get("base_sha") != expected_base_sha
            or pr.get("head_sha") != expected_head_sha
        ):
            raise GitHubFactsError("quarantine_pr_identity_mismatch")
        if pr.get("merged") is True:
            return {
                "status": "MERGED",
                "repository": repository,
                "pr_number": pr_number,
                "base_sha": expected_base_sha,
                "head_sha": expected_head_sha,
                "accepted_main_sha": self.remote_main_sha,
            }
        if pr.get("state") == "CLOSED":
            return {
                "status": "CLOSED_UNMERGED",
                "repository": repository,
                "pr_number": pr_number,
                "base_sha": expected_base_sha,
                "head_sha": expected_head_sha,
                "accepted_main_sha": self.remote_main_sha,
            }
        if pr.get("state") != "OPEN":
            raise GitHubFactsError("quarantine_preflight_not_proven")
        self.actions.append(("quarantine", {"pr_number": pr_number, "head_sha": expected_head_sha}))
        if self.quarantine_race_merge:
            # Model the old workflow winning after the final read-only
            # preflight but before the close mutation reaches GitHub.
            pr["merged"] = True
            pr["state"] = "CLOSED"
            pr["merge_commit_sha"] = expected_head_sha
            self.remote_main_sha = expected_head_sha
            return {
                "status": "MERGED",
                "repository": repository,
                "pr_number": pr_number,
                "base_sha": expected_base_sha,
                "head_sha": expected_head_sha,
                "accepted_main_sha": self.remote_main_sha,
            }
        pr["state"] = "CLOSED"
        return {
            "status": "CLOSED_UNMERGED",
            "repository": repository,
            "pr_number": pr_number,
            "base_sha": expected_base_sha,
            "head_sha": expected_head_sha,
            "preflight_accepted_main_sha": self.remote_main_sha,
            "accepted_main_sha": self.remote_main_sha,
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
    "merge_dispatch_identity",
    "reconcile_stage_pr",
]
