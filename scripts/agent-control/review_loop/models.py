"""Versioned data contracts for the non-authoritative independent-review transport.

This package is repository automation only.  It never decides task state,
budget, approval, output, merge, release, or deployment; those owners remain
in the Rust runtime and its canonical documents.
"""

from __future__ import annotations

import dataclasses
import enum
import json
import re
from typing import Any

ENVELOPE_SCHEMA = "independent_review_request.v1"
RECEIPT_SCHEMA = "independent_review_receipt.v1"
JOURNAL_SCHEMA = "review_loop_journal.v1"

HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")


class DeliveryOutcome(str, enum.Enum):
    BUILT = "BUILT"
    LIVE_VALIDATED = "LIVE_VALIDATED"
    DELIVERY_INSPECTED = "DELIVERY_INSPECTED"
    SENT_CONFIRMED = "SENT_CONFIRMED"
    ALREADY_PRESENT = "ALREADY_PRESENT"
    DELIVERY_OUTCOME_UNKNOWN = "DELIVERY_OUTCOME_UNKNOWN"
    RESPONSE_CAPTURED = "RESPONSE_CAPTURED"
    RECEIPT_PARSED = "RECEIPT_PARSED"
    HEAD_REVALIDATED = "HEAD_REVALIDATED"
    COMMENT_POSTED = "COMMENT_POSTED"
    COMPLETE = "COMPLETE"
    AUTH_REQUIRED = "AUTH_REQUIRED"
    LOCK_BUSY = "LOCK_BUSY"
    FAILED = "FAILED"


VALID_RECEIPT_VERDICTS = frozenset({"PASS"})


@dataclasses.dataclass(frozen=True)
class ReviewRequestEnvelope:
    """Immutable request envelope bound to one PR and one evidence index."""

    schema_version: str
    repository: str
    pr_number: int
    base_sha: str
    head_sha: str
    chat_key: str
    evidence_index_sha256: str
    request_text_sha256: str
    implementation_session_id: str

    def validate(self) -> list[str]:
        errors = []
        if self.schema_version != ENVELOPE_SCHEMA:
            errors.append(f"unexpected schema_version {self.schema_version!r}")
        if not self.repository or "/" not in self.repository:
            errors.append(f"invalid repository {self.repository!r}")
        if not isinstance(self.pr_number, int) or self.pr_number <= 0:
            errors.append(f"invalid pr_number {self.pr_number!r}")
        for name, value in (
            ("base_sha", self.base_sha),
            ("head_sha", self.head_sha),
        ):
            if not HEX40.fullmatch(value):
                errors.append(f"{name} is not a 40-hex git object id: {value!r}")
        for name, value in (
            ("evidence_index_sha256", self.evidence_index_sha256),
            ("request_text_sha256", self.request_text_sha256),
        ):
            if not HEX64.fullmatch(value):
                errors.append(f"{name} is not a 64-hex sha: {value!r}")
        if not self.chat_key or not self.implementation_session_id:
            errors.append("chat_key and implementation_session_id are required")
        return errors

    def to_json(self) -> str:
        return json.dumps(dataclasses.asdict(self), sort_keys=True, indent=2)

    @classmethod
    def from_json(cls, raw: str) -> "ReviewRequestEnvelope":
        data = json.loads(raw)
        return cls(**data)


@dataclasses.dataclass(frozen=True)
class ReviewReceipt:
    """Structured independent-review verdict; only exact PASS is acceptable."""

    schema_version: str
    verdict: str
    repository: str
    pr_number: int
    base_sha: str
    head_sha: str
    diff_scope: str
    blockers: tuple[str, ...]
    unresolved_objections: tuple[str, ...]
    reviewer_session_id: str
    implementation_session_id: str
    transport: str

    def validate(self) -> list[str]:
        errors = []
        if self.schema_version != RECEIPT_SCHEMA:
            errors.append(f"unexpected schema_version {self.schema_version!r}")
        if self.verdict not in VALID_RECEIPT_VERDICTS:
            errors.append(f"verdict {self.verdict!r} is not an exact PASS")
        for name, value in (
            ("base_sha", self.base_sha),
            ("head_sha", self.head_sha),
        ):
            if not HEX40.fullmatch(value):
                errors.append(f"{name} is not a 40-hex git object id: {value!r}")
        if self.blockers:
            errors.append(f"receipt has blockers: {self.blockers}")
        if self.unresolved_objections:
            errors.append(f"receipt has unresolved objections: {self.unresolved_objections}")
        if self.diff_scope != "complete_base_head":
            errors.append(f"diff_scope {self.diff_scope!r} is not complete_base_head")
        if not self.reviewer_session_id:
            errors.append("reviewer_session_id is empty")
        if self.reviewer_session_id == self.implementation_session_id:
            errors.append("reviewer session must differ from implementation session")
        allowed_transports = {
            "parent-posted-on-behalf-of-independent-session",
        }
        if self.transport not in allowed_transports:
            errors.append(f"transport {self.transport!r} is not in the allowed set")
        return errors

    def matches_envelope(self, envelope: ReviewRequestEnvelope) -> list[str]:
        errors = []
        if self.repository != envelope.repository:
            errors.append(f"repository mismatch {self.repository} != {envelope.repository}")
        if self.pr_number != envelope.pr_number:
            errors.append(f"pr_number mismatch {self.pr_number} != {envelope.pr_number}")
        if self.base_sha != envelope.base_sha:
            errors.append(f"base_sha mismatch {self.base_sha} != {envelope.base_sha}")
        if self.head_sha != envelope.head_sha:
            errors.append(f"head_sha mismatch {self.head_sha} != {envelope.head_sha}")
        if self.implementation_session_id != envelope.implementation_session_id:
            errors.append(
                "implementation_session_id mismatch "
                f"{self.implementation_session_id} != {envelope.implementation_session_id}"
            )
        return errors

    def to_json(self) -> str:
        return json.dumps(
            {
                "schema_version": self.schema_version,
                "verdict": self.verdict,
                "repository": self.repository,
                "pr_number": self.pr_number,
                "base_sha": self.base_sha,
                "head_sha": self.head_sha,
                "diff_scope": self.diff_scope,
                "blockers": list(self.blockers),
                "unresolved_objections": list(self.unresolved_objections),
                "reviewer_session_id": self.reviewer_session_id,
                "implementation_session_id": self.implementation_session_id,
                "transport": self.transport,
            },
            sort_keys=True,
            indent=2,
        )

    @classmethod
    def from_json(cls, raw: str | dict[str, Any]) -> "ReviewReceipt":
        data = json.loads(raw) if isinstance(raw, str) else raw
        return cls(
            schema_version=data["schema_version"],
            verdict=data["verdict"],
            repository=data["repository"],
            pr_number=data["pr_number"],
            base_sha=data["base_sha"],
            head_sha=data["head_sha"],
            diff_scope=data["diff_scope"],
            blockers=tuple(data.get("blockers", [])),
            unresolved_objections=tuple(data.get("unresolved_objections", [])),
            reviewer_session_id=data["reviewer_session_id"],
            implementation_session_id=data["implementation_session_id"],
            transport=data["transport"],
        )


@dataclasses.dataclass(frozen=True)
class TransportEvent:
    """One append-only journal record."""

    seq: int
    ts: str
    event: str
    chat_key: str
    request_text_sha256: str
    detail: str = ""
    prev_sha: str = ""
    sha: str = ""

    @classmethod
    def from_json(cls, raw: str | dict[str, Any]) -> "TransportEvent":
        data = json.loads(raw) if isinstance(raw, str) else raw
        return cls(**data)

    def to_json(self) -> str:
        return json.dumps(dataclasses.asdict(self), sort_keys=True)
