"""Read-only GitHub live-state adapter interface and CI fake (pure logic).

The CLI must obtain live PR facts from this adapter — never from caller
asserted JSON.  The adapter is read-only: it lists comments and fetches PR
state; it never approves, merges, or writes protected branches.  The real
implementation is operator-side (uses gh or the REST API); CI uses the fake.
"""

from __future__ import annotations

import typing
from typing import Any


class LiveGitHub(typing.Protocol):
    """Read-only live GitHub facts a review-loop command needs."""

    def fetch_pr(self, repository: str, pr_number: int) -> dict[str, Any]:
        """Return live PR facts: repository, pr_number (observed identity),
        state, is_draft, merged, base_sha, head_sha, changed_files (complete
        base...head list).  Raises on unavailability."""
        ...

    def list_comments(self, repository: str, pr_number: int) -> list[str]:
        """Return existing issue-comment bodies, or raise when unavailable."""
        ...

    def create_comment(self, repository: str, pr_number: int, body: str) -> str:
        """Create one ordinary issue comment; returns its URL.  Must never
        approve, merge, or write protected branches."""
        ...


class FakeGitHub:
    """Deterministic in-memory GitHub for provider-free tests."""

    def __init__(self, pr_facts: dict[str, Any] | None = None, comments: list[str] | None = None):
        self.pr_facts = dict(pr_facts or {})
        self.comments: list[str] = list(comments or [])
        self.posted: list[str] = []
        self.fail_next_fetch = False
        self.fail_next_post = False

    def fetch_pr(self, repository: str, pr_number: int) -> dict[str, Any]:
        if self.fail_next_fetch:
            self.fail_next_fetch = False
            raise RuntimeError("live GitHub unavailable")
        facts = dict(self.pr_facts)
        facts.setdefault("repository", repository)
        facts.setdefault("pr_number", pr_number)
        return facts

    def list_comments(self, repository: str, pr_number: int) -> list[str]:
        return list(self.comments)

    def create_comment(self, repository: str, pr_number: int, body: str) -> str:
        if self.fail_next_post:
            self.fail_next_post = False
            raise RuntimeError("comment POST outcome unknown")
        url = f"https://fake/comments/{len(self.posted) + 1}"
        self.posted.append(body)
        self.comments.append(body)
        return url
