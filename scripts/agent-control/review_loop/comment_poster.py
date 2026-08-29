"""Idempotent ordinary GitHub comment posting (pure logic + thin client).

The repository owns the receipt-comment contract: a stable marker plus the
receipt JSON.  Posting is idempotent by (request sha, receipt sha); a conflict
(same request sha, different receipt sha) stops instead of double posting.
The actual GitHub HTTP calls are delegated to a caller-supplied client so CI
uses a fake.
"""

from __future__ import annotations

import hashlib
import json
import re
import typing

import review_convergence

from . import models

COMMENT_MARKER = "independent-review-receipt"
REVIEW_STATE_KIND = review_convergence.REVIEW_STATE_KIND
MARKER_RE = re.compile(
    r"<!--\s*" + COMMENT_MARKER + r":([0-9a-f]{64}):([0-9a-f]{64})\s*-->"
)


def comment_marker_line(request_sha: str, receipt_sha: str) -> str:
    return f"<!-- {COMMENT_MARKER}:{request_sha}:{receipt_sha} -->"


def receipt_sha256(receipt: models.ReviewReceipt) -> str:
    return hashlib.sha256(receipt.to_json().encode("utf-8")).hexdigest()


def build_comment_body(
    envelope: models.ReviewRequestEnvelope,
    receipt: models.ReviewReceipt,
) -> str:
    return "\n".join(
        [
            "**Exact-Head Independent Review Receipt**",
            "",
            f"Reviewed head: `{receipt.head_sha}`",
            f"Base: `{receipt.base_sha}`",
            f"PR: {receipt.pr_number}",
            f"Verdict: **{receipt.verdict}**"
            + (
                f" (deferred notes: {len(receipt.deferred_notes)})"
                if getattr(receipt, "deferred_notes", ())
                else ""
            ),
            "",
            "```json",
            receipt.to_json(),
            "```",
            "",
            comment_marker_line(envelope.request_text_sha256, receipt_sha256(receipt)),
        ]
    )


def build_review_state_body(
    state: review_convergence.ReviewRoundState,
    *,
    issue_number: int,
    pr_number: int,
) -> str:
    """Build the exact JSON body used for durable linked-Issue recovery."""

    return review_convergence.review_state_json(
        state,
        issue_number=issue_number,
        pr_number=pr_number,
    )


def reconcile_review_state_comments(
    existing_comments: list[str],
    state_body: str,
) -> tuple[str, list[str]]:
    """Return ``skip``/``post``/``conflict`` for a durable state comment.

    A different state for the same linked Issue is a conflict.  This keeps a
    retry from overwriting the recovery history or silently selecting an old
    state after an uncertain POST.
    """

    if existing_comments is None:
        return "conflict", ["existing review-state comments unavailable"]
    try:
        expected = typing.cast(dict[str, typing.Any], json.loads(state_body))
    except (TypeError, ValueError):
        return "conflict", ["new review-state body is not valid JSON"]
    if not isinstance(expected, dict) or expected.get("kind") != REVIEW_STATE_KIND:
        return "conflict", ["new review-state body has an invalid kind"]
    identical = False
    for body in existing_comments:
        text = body or ""
        if REVIEW_STATE_KIND not in text:
            continue
        try:
            candidate = json.loads(text)
        except (TypeError, ValueError):
            return "conflict", ["malformed durable review-state comment present"]
        if not isinstance(candidate, dict) or candidate.get("kind") != REVIEW_STATE_KIND:
            return "conflict", ["invalid durable review-state comment present"]
        if candidate == expected:
            identical = True
        else:
            return "conflict", ["different durable review state already posted"]
    if identical:
        return "skip", ["identical durable review state already posted"]
    return "post", []


def publish_review_state(
    client: "GitHubCommentClient",
    repository: str,
    *,
    issue_number: int,
    pr_number: int,
    state: review_convergence.ReviewRoundState,
) -> tuple[str, str | None]:
    """Publish one state to the linked Issue with read-before-write recovery.

    The client must be the authenticated parent/orchestrator transport.  A
    raised create call remains outcome-unknown; callers must re-query this
    same Issue before any retry.
    """

    body = build_review_state_body(
        state,
        issue_number=issue_number,
        pr_number=pr_number,
    )
    action, reasons = reconcile_review_state_comments(
        client.list_comments(repository, issue_number), body
    )
    if action == "skip":
        return "skipped", None
    if action != "post":
        raise RuntimeError("review-state comment conflict: " + "; ".join(reasons))
    # A raised create call is intentionally not translated into success or
    # failure.  The caller must re-list this Issue and reconcile the exact
    # state before attempting another POST.
    return "posted", client.create_comment(repository, issue_number, body)


def reconcile_comments(
    existing_comments: list[str],
    request_sha: str,
    receipt_sha: str,
) -> tuple[str, list[str]]:
    """Decide posting action: skip / post / conflict / stop.

    Every marker occurrence across every comment is scanned and classified
    before any decision (R2-B4/R3-B4).  A malformed marker occurrence wins
    over a valid identical marker even inside the same comment body; one
    same-request marker with a different receipt is a conflict regardless of
    what else is present.  Only when all relevant markers are exactly
    identical is posting skipped.
    """
    if existing_comments is None:
        return "unknown", ["existing comments unavailable"]
    identical_count = 0
    reasons: list[str] = []
    for body in existing_comments:
        text = body or ""
        # Remove every valid marker so any remaining COMMENT_MARKER token is a
        # malformed/partial occurrence that must win (R3-B4).
        scrubbed = text
        for match in MARKER_RE.finditer(text):
            found_request, found_receipt = match.group(1), match.group(2)
            if found_request == request_sha:
                if found_receipt == receipt_sha:
                    identical_count += 1
                else:
                    return "conflict", [
                        f"same request {request_sha[:12]}... but different receipt "
                        f"{found_receipt[:12]}... already posted"
                    ]
            scrubbed = scrubbed.replace(match.group(0), "", 1)
        if COMMENT_MARKER in scrubbed:
            # A marker token remains that is not part of any valid full
            # marker: cannot tell whether a prior receipt exists; stop
            # instead of double posting.
            return "conflict", ["malformed or partial review-receipt marker present"]
    if identical_count:
        return "skip", [f"identical receipt already posted ({request_sha[:12]}...)"]
    return "post", reasons


class GitHubCommentClient(typing.Protocol):
    """Thin GitHub comment client supplied by the caller (fake in CI)."""

    def list_comments(self, repository: str, pr_number: int) -> list[str]: ...

    def create_comment(self, repository: str, pr_number: int, body: str) -> str: ...
