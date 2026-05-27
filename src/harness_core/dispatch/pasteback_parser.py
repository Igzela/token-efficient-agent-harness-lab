"""PastebackSubmission schema and PastebackParser for human-pasted output."""

from __future__ import annotations

import hashlib
import uuid
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any

# ---------------------------------------------------------------------------
# Schema version
# ---------------------------------------------------------------------------

PASTEBACK_SUBMISSION_SCHEMA_VERSION = "pasteback_submission.v1"

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

MAX_OUTPUT_LENGTH: int = 100_000  # 100k chars
ESTIMATED_CHARS_PER_TOKEN: int = 4
DEFAULT_COST_PER_1K_TOKENS: float = 0.002


# ---------------------------------------------------------------------------
# Schema
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class PastebackSubmission:
    submission_id: str
    dispatch_id: str
    submitted_by: str
    model_used: str | None
    provider_used: str | None
    raw_output: str
    output_hash: str
    claimed_input_tokens: int | None
    claimed_output_tokens: int | None
    claimed_cost: float | None
    submitted_at: str
    schema_version: str = PASTEBACK_SUBMISSION_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "submission_id": self.submission_id,
            "dispatch_id": self.dispatch_id,
            "submitted_by": self.submitted_by,
            "model_used": self.model_used,
            "provider_used": self.provider_used,
            "raw_output": self.raw_output,
            "output_hash": self.output_hash,
            "claimed_input_tokens": self.claimed_input_tokens,
            "claimed_output_tokens": self.claimed_output_tokens,
            "claimed_cost": self.claimed_cost,
            "submitted_at": self.submitted_at,
        }


# ---------------------------------------------------------------------------
# Parser
# ---------------------------------------------------------------------------


class PastebackParser:
    """Validates and parses human-pasted output into PastebackSubmission."""

    def parse(
        self,
        dispatch_id: str,
        raw_output: str,
        submitted_by: str = "human",
        model_used: str | None = None,
        provider_used: str | None = None,
        claimed_input_tokens: int | None = None,
        claimed_output_tokens: int | None = None,
        claimed_cost: float | None = None,
    ) -> PastebackSubmission:
        validated_output = self._validate_output(raw_output)
        output_hash = self._hash_output(validated_output)

        return PastebackSubmission(
            submission_id=f"pb-{uuid.uuid4().hex[:12]}",
            dispatch_id=dispatch_id,
            submitted_by=submitted_by,
            model_used=model_used,
            provider_used=provider_used,
            raw_output=validated_output,
            output_hash=output_hash,
            claimed_input_tokens=claimed_input_tokens,
            claimed_output_tokens=claimed_output_tokens,
            claimed_cost=claimed_cost,
            submitted_at=datetime.now(timezone.utc).isoformat(),
        )

    def estimate_tokens(self, text: str) -> int:
        return max(1, len(text) // ESTIMATED_CHARS_PER_TOKEN)

    def estimate_cost(
        self, input_tokens: int, output_tokens: int, cost_per_1k: float = DEFAULT_COST_PER_1K_TOKENS
    ) -> float:
        return round((input_tokens + output_tokens) / 1000 * cost_per_1k, 6)

    def _validate_output(self, raw_output: str) -> str:
        if not raw_output or not raw_output.strip():
            raise ValueError("Pasteback output cannot be empty")
        output = raw_output.strip()
        if len(output) > MAX_OUTPUT_LENGTH:
            raise ValueError(
                f"Output exceeds max length ({len(output)} > {MAX_OUTPUT_LENGTH})"
            )
        return output

    def _hash_output(self, output: str) -> str:
        return hashlib.sha256(output.encode("utf-8")).hexdigest()[:16]
