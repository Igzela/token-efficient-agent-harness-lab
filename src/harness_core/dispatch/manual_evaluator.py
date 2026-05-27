"""ManualEvaluator: evaluates pasted output against evaluation checklist."""

from __future__ import annotations

import uuid
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any

from .pasteback_parser import PastebackSubmission
from .prompt_pack_gen import PromptPack

# ---------------------------------------------------------------------------
# Schema version
# ---------------------------------------------------------------------------

MANUAL_EVAL_RESULT_SCHEMA_VERSION = "manual_eval_result.v1"

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

MANUAL_EVAL_STATUSES: tuple[str, ...] = ("pass", "fail", "needs_human_review")

# ---------------------------------------------------------------------------
# Boundary violation heuristics
# ---------------------------------------------------------------------------

# Maps forbidden output constraint types to violation markers
_BOUNDARY_VIOLATION_MARKERS: dict[str, tuple[str, ...]] = {
    "no_target_write": (
        "committed", "pushed to", "modified file", "wrote to",
        "git push", "git commit", "saved to",
    ),
    "no_provider_call": (
        "called openai", "used anthropic api", "sent request to",
        "api call to", "provider request",
    ),
    "no_sandbox_execution": (
        "ran docker", "executed shell", "ran command",
        "started process", "spawned process",
    ),
}


# ---------------------------------------------------------------------------
# Schema
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class ManualEvalCheck:
    check_id: str
    name: str
    status: str  # "pass" | "fail" | "warning"
    reason: str

    def to_dict(self) -> dict[str, Any]:
        return {
            "check_id": self.check_id,
            "name": self.name,
            "status": self.status,
            "reason": self.reason,
        }


@dataclass(frozen=True)
class ManualEvalResult:
    eval_id: str
    dispatch_id: str
    submission_id: str
    status: str  # from MANUAL_EVAL_STATUSES
    checks: tuple[ManualEvalCheck, ...]
    created_at: str
    quality_score: float | None = None
    schema_version: str = MANUAL_EVAL_RESULT_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "eval_id": self.eval_id,
            "dispatch_id": self.dispatch_id,
            "submission_id": self.submission_id,
            "status": self.status,
            "checks": [c.to_dict() for c in self.checks],
            "quality_score": self.quality_score,
            "created_at": self.created_at,
        }


# ---------------------------------------------------------------------------
# Evaluator
# ---------------------------------------------------------------------------


class ManualEvaluator:
    """Evaluates pasteback submissions against the prompt pack checklist."""

    def evaluate(
        self,
        submission: PastebackSubmission,
        prompt_pack: PromptPack,
    ) -> ManualEvalResult:
        checks = self._run_checks(submission, prompt_pack)
        failed = [c for c in checks if c.status == "fail"]
        warnings = [c for c in checks if c.status == "warning"]

        if failed:
            status = "fail"
        elif warnings:
            status = "needs_human_review"
        else:
            status = "pass"

        return ManualEvalResult(
            eval_id=f"meval-{uuid.uuid4().hex[:12]}",
            dispatch_id=submission.dispatch_id,
            submission_id=submission.submission_id,
            status=status,
            checks=tuple(checks),
            created_at=datetime.now(timezone.utc).isoformat(),
        )

    def _run_checks(
        self,
        submission: PastebackSubmission,
        prompt_pack: PromptPack,
    ) -> list[ManualEvalCheck]:
        checks: list[ManualEvalCheck] = []
        checklist = prompt_pack.evaluation_checklist

        if "output_present" in checklist:
            checks.append(self._check_output_present(submission))

        if "schema_validity" in checklist:
            checks.append(self._check_schema_validity(submission))

        if "boundary_compliance" in checklist:
            checks.append(self._check_boundary_compliance(submission, prompt_pack))

        if "error_free" in checklist:
            checks.append(self._check_error_free(submission))

        if "human_review_required" in checklist:
            checks.append(self._check_human_review(submission))

        return checks

    def _check_output_present(self, sub: PastebackSubmission) -> ManualEvalCheck:
        has_output = bool(sub.raw_output and sub.raw_output.strip())
        return ManualEvalCheck(
            check_id=f"mc-{uuid.uuid4().hex[:8]}",
            name="output_present",
            status="pass" if has_output else "fail",
            reason="output present" if has_output else "no output in pasteback",
        )

    def _check_schema_validity(self, sub: PastebackSubmission) -> ManualEvalCheck:
        valid = bool(sub.submission_id and sub.dispatch_id and sub.output_hash)
        return ManualEvalCheck(
            check_id=f"mc-{uuid.uuid4().hex[:8]}",
            name="schema_validity",
            status="pass" if valid else "fail",
            reason="required fields present" if valid else "missing required fields",
        )

    def _check_boundary_compliance(
        self, sub: PastebackSubmission, pack: PromptPack
    ) -> ManualEvalCheck:
        if not pack.forbidden_outputs:
            return ManualEvalCheck(
                check_id=f"mc-{uuid.uuid4().hex[:8]}",
                name="boundary_compliance",
                status="pass",
                reason="no forbidden output constraints",
            )
        output_lower = sub.raw_output.lower()
        violations = []
        for forbidden in pack.forbidden_outputs:
            constraint_type = self._infer_constraint_type(forbidden)
            markers = _BOUNDARY_VIOLATION_MARKERS.get(constraint_type, ())
            for marker in markers:
                if marker in output_lower:
                    violations.append(f"{constraint_type}: '{marker}'")
                    break
        if violations:
            return ManualEvalCheck(
                check_id=f"mc-{uuid.uuid4().hex[:8]}",
                name="boundary_compliance",
                status="fail",
                reason=f"heuristic boundary violations detected: {'; '.join(violations)}",
            )
        return ManualEvalCheck(
            check_id=f"mc-{uuid.uuid4().hex[:8]}",
            name="boundary_compliance",
            status="pass",
            reason="no boundary violation markers detected (heuristic check)",
        )

    def _infer_constraint_type(self, forbidden_text: str) -> str:
        text = forbidden_text.lower()
        if "provider" in text or "api call" in text:
            return "no_provider_call"
        if "target" in text or "repository" in text or "write" in text:
            return "no_target_write"
        if "sandbox" in text or "execute" in text:
            return "no_sandbox_execution"
        return "unknown"

    def _check_error_free(self, sub: PastebackSubmission) -> ManualEvalCheck:
        error_markers = ("traceback", "exception", "error:", "fatal:")
        output_lower = sub.raw_output.lower()
        found = [m for m in error_markers if m in output_lower]
        if found:
            return ManualEvalCheck(
                check_id=f"mc-{uuid.uuid4().hex[:8]}",
                name="error_free",
                status="warning",
                reason=f"possible error markers found: {', '.join(found)}",
            )
        return ManualEvalCheck(
            check_id=f"mc-{uuid.uuid4().hex[:8]}",
            name="error_free",
            status="pass",
            reason="no error markers detected",
        )

    def _check_human_review(self, sub: PastebackSubmission) -> ManualEvalCheck:
        return ManualEvalCheck(
            check_id=f"mc-{uuid.uuid4().hex[:8]}",
            name="human_review_required",
            status="warning",
            reason="manual execution requires human review",
        )
