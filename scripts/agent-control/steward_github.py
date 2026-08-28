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

import ci_verifier


SHA40 = re.compile(r"^[0-9a-f]{40}$")
REPOSITORY = re.compile(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")
PR_STATES = frozenset({"OPEN", "CLOSED"})
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


class GhReadOnlyGitHub:
    """Small operator-side ``gh pr view`` reader with no write command."""

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
            "state,isDraft,mergedAt,baseRefOid,headRefOid,statusCheckRollup,reviewDecision",
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
            try:
                required_jobs = set(ci_verifier.load_requirements()["required_jobs"])
            except (OSError, ValueError, ci_verifier.CIVerificationError) as exc:
                raise GitHubReadError("canonical_ci_requirements_unavailable") from exc
            observed_names = {
                item.get("name") for item in check_items if isinstance(item.get("name"), str)
            }
            if "FAILURE" in conclusions or "CANCELLED" in conclusions:
                ci_state = "FAIL"
            elif (
                len(check_items) != len(checks)
                or None in conclusions
                or bool(
                    statuses
                    & {"IN_PROGRESS", "QUEUED", "REQUESTED", "WAITING", "PENDING"}
                )
            ):
                ci_state = "PENDING"
            elif not required_jobs.issubset(observed_names):
                ci_state = "UNKNOWN"
            elif len(conclusions) == len(checks) and all(
                conclusion == "SUCCESS" for conclusion in conclusions
            ):
                ci_state = "PASS"
        review = payload.get("reviewDecision")
        review_state = {
            "APPROVED": "PASS",
            "CHANGES_REQUESTED": "FAIL",
        }.get(review, "PENDING" if review is None else "UNKNOWN")
        state = payload.get("state")
        draft = payload.get("isDraft")
        merged_at = payload.get("mergedAt")
        base_sha = payload.get("baseRefOid")
        head_sha = payload.get("headRefOid")
        if (
            state not in PR_STATES
            or type(draft) is not bool
            or (merged_at is not None and not isinstance(merged_at, str))
            or not isinstance(base_sha, str)
            or not isinstance(head_sha, str)
        ):
            raise GitHubReadError("github_read_malformed")
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
