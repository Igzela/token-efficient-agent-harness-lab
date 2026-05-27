"""Evaluation stub: basic boundary checks, no quality judgment."""

from __future__ import annotations

import uuid
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any

from .dispatch_decision import DispatchDecision
from .executor_adapter import ExecutionResult

# ---------------------------------------------------------------------------
# Schema version
# ---------------------------------------------------------------------------

EVALUATION_RESULT_SCHEMA_VERSION = "evaluation_result.v1"

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

EVAL_CHECK_NAMES: tuple[str, ...] = (
    "schema_validity",
    "boundary_compliance",
    "output_present",
    "error_free",
    "human_review_required",
)


# ---------------------------------------------------------------------------
# Schemas
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class EvaluationCheck:
    check_id: str
    name: str
    status: str  # "pass" | "fail" | "warning" | "skipped"
    reason: str

    def to_dict(self) -> dict[str, Any]:
        return {
            "check_id": self.check_id,
            "name": self.name,
            "status": self.status,
            "reason": self.reason,
        }


@dataclass(frozen=True)
class EvaluationResult:
    evaluation_id: str
    dispatch_id: str
    decision_id: str
    execution_result_id: str
    status: str  # "pass" | "fail" | "needs_human_review" | "not_evaluated"
    checks: tuple[EvaluationCheck, ...]
    created_at: str
    quality_score: float | None = None
    requires_retry: bool = False
    retry_reason: str | None = None
    schema_version: str = EVALUATION_RESULT_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "evaluation_id": self.evaluation_id,
            "dispatch_id": self.dispatch_id,
            "decision_id": self.decision_id,
            "execution_result_id": self.execution_result_id,
            "status": self.status,
            "checks": [c.to_dict() for c in self.checks],
            "quality_score": self.quality_score,
            "requires_retry": self.requires_retry,
            "retry_reason": self.retry_reason,
            "created_at": self.created_at,
        }


# ---------------------------------------------------------------------------
# Stub evaluator
# ---------------------------------------------------------------------------


class EvaluationStub:
    """Basic boundary checks. No quality judgment, no LLM judge."""

    def evaluate(
        self, result: ExecutionResult, decision: DispatchDecision
    ) -> EvaluationResult:
        checks = [
            self._check_schema_validity(result),
            self._check_boundary_compliance(result, decision),
            self._check_output_present(result),
            self._check_error_free(result),
            self._check_human_review_required(result, decision),
        ]

        failed = [c for c in checks if c.status == "fail"]
        warnings = [c for c in checks if c.status == "warning"]

        if failed:
            status = "fail"
        elif any(c.name == "human_review_required" and c.status == "warning" for c in checks):
            status = "needs_human_review"
        elif warnings:
            status = "pass"
        else:
            status = "pass"

        return EvaluationResult(
            evaluation_id=f"eval-{uuid.uuid4().hex[:12]}",
            dispatch_id=result.dispatch_id,
            decision_id=result.decision_id,
            execution_result_id=result.result_id,
            status=status,
            checks=tuple(checks),
            created_at=datetime.now(timezone.utc).isoformat(),
        )

    def _check_schema_validity(self, result: ExecutionResult) -> EvaluationCheck:
        valid = bool(result.result_id and result.dispatch_id and result.decision_id)
        return EvaluationCheck(
            check_id=f"chk-{uuid.uuid4().hex[:8]}",
            name="schema_validity",
            status="pass" if valid else "fail",
            reason="required fields present" if valid else "missing required fields",
        )

    def _check_boundary_compliance(
        self, result: ExecutionResult, decision: DispatchDecision
    ) -> EvaluationCheck:
        if result.executor_type == "provider":
            return EvaluationCheck(
                check_id=f"chk-{uuid.uuid4().hex[:8]}",
                name="boundary_compliance",
                status="fail",
                reason="provider executor not allowed in Phase 1",
            )
        return EvaluationCheck(
            check_id=f"chk-{uuid.uuid4().hex[:8]}",
            name="boundary_compliance",
            status="pass",
            reason=f"executor_type={result.executor_type} within Phase 1 boundaries",
        )

    def _check_output_present(self, result: ExecutionResult) -> EvaluationCheck:
        if result.executor_type == "noop":
            return EvaluationCheck(
                check_id=f"chk-{uuid.uuid4().hex[:8]}",
                name="output_present",
                status="warning",
                reason="noop executor produces no output (expected)",
            )
        if result.output or result.prompt_pack:
            return EvaluationCheck(
                check_id=f"chk-{uuid.uuid4().hex[:8]}",
                name="output_present",
                status="pass",
                reason="output or prompt_pack present",
            )
        return EvaluationCheck(
            check_id=f"chk-{uuid.uuid4().hex[:8]}",
            name="output_present",
            status="fail",
            reason="no output and no prompt_pack",
        )

    def _check_error_free(self, result: ExecutionResult) -> EvaluationCheck:
        if result.error_domain:
            return EvaluationCheck(
                check_id=f"chk-{uuid.uuid4().hex[:8]}",
                name="error_free",
                status="fail",
                reason=f"error: {result.error_domain}: {result.error_message}",
            )
        return EvaluationCheck(
            check_id=f"chk-{uuid.uuid4().hex[:8]}",
            name="error_free",
            status="pass",
            reason="no errors",
        )

    def _check_human_review_required(
        self, result: ExecutionResult, decision: DispatchDecision
    ) -> EvaluationCheck:
        if result.status == "manual_pending":
            return EvaluationCheck(
                check_id=f"chk-{uuid.uuid4().hex[:8]}",
                name="human_review_required",
                status="warning",
                reason="manual executor awaiting human pasteback",
            )
        if decision.execution_policy.get("requires_human_review"):
            return EvaluationCheck(
                check_id=f"chk-{uuid.uuid4().hex[:8]}",
                name="human_review_required",
                status="warning",
                reason="execution policy requires human review",
            )
        return EvaluationCheck(
            check_id=f"chk-{uuid.uuid4().hex[:8]}",
            name="human_review_required",
            status="pass",
            reason="no human review required",
        )
