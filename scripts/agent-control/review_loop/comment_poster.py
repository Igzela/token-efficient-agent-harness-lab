"""Idempotent ordinary GitHub comment posting (pure logic + thin client).

The repository owns the receipt-comment contract: a stable marker plus the
receipt JSON.  Posting is idempotent by (request sha, receipt sha); a conflict
(same request sha, different receipt sha) stops instead of double posting.
The actual GitHub HTTP calls are delegated to a caller-supplied client so CI
uses a fake.
"""

from __future__ import annotations

import hashlib
import re
import typing

from . import models

COMMENT_MARKER = "independent-review-receipt"
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
            f"Verdict: **{receipt.verdict}**",
            "",
            "```json",
            receipt.to_json(),
            "```",
            "",
            comment_marker_line(envelope.request_text_sha256, receipt_sha256(receipt)),
        ]
    )


def reconcile_comments(
    existing_comments: list[str],
    request_sha: str,
    receipt_sha: str,
) -> tuple[str, list[str]]:
    """Decide posting action: skip / post / conflict / stop.

    Marker matching is strict (anchored full-marker regex), not substring.
    A malformed marker is treated as a conflict (stop) so accidental or
    malicious text cannot cause a false skip.
    """
    reasons: list[str] = []
    if existing_comments is None:
        return "unknown", ["existing comments unavailable"]
    for body in existing_comments:
        matches = list(MARKER_RE.finditer(body or ""))
        for match in matches:
            found_request, found_receipt = match.group(1), match.group(2)
            if found_request == request_sha:
                if found_receipt == receipt_sha:
                    return "skip", [f"identical receipt already posted ({request_sha[:12]}...)"]
                return "conflict", [
                    f"same request {request_sha[:12]}... but different receipt sha already posted"
                ]
        if COMMENT_MARKER in (body or "") and not matches:
            # The marker token appears but no valid full marker: cannot tell
            # whether a prior receipt exists; stop instead of double posting.
            return "conflict", ["malformed or partial review-receipt marker present"]
    return "post", reasons


class GitHubCommentClient(typing.Protocol):
    """Thin GitHub comment client supplied by the caller (fake in CI)."""

    def list_comments(self, repository: str, pr_number: int) -> list[str]: ...

    def create_comment(self, repository: str, pr_number: int, body: str) -> str: ...
