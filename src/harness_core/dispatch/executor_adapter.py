"""Executor adapters: NoopExecutor, MockExecutor, ManualExecutor.

Phase 1: noop/mock/manual only. Provider executor is reserved but disabled.
"""

from __future__ import annotations

import uuid
from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Any

from .dispatch_decision import DispatchDecision

# ---------------------------------------------------------------------------
# Schema version
# ---------------------------------------------------------------------------

EXECUTION_RESULT_SCHEMA_VERSION = "execution_result.v1"

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

EXECUTOR_TYPES: tuple[str, ...] = ("noop", "mock", "manual", "provider")

EXECUTION_STATUSES: tuple[str, ...] = (
    "not_executed", "preview_generated", "mock_completed",
    "manual_pending", "manual_completed", "failed",
)


# ---------------------------------------------------------------------------
# Schema
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class ExecutionResult:
    result_id: str
    dispatch_id: str
    decision_id: str
    executor_type: str  # from EXECUTOR_TYPES
    status: str  # from EXECUTION_STATUSES
    created_at: str
    output: str | None = None
    prompt_pack: dict[str, Any] | None = None
    input_tokens: int | None = None
    output_tokens: int | None = None
    estimated_cost: float | None = None
    latency_ms: int | None = None
    error_domain: str | None = None
    error_message: str | None = None
    provider_request_id: str | None = None
    attempt_number: int | None = None
    finish_reason: str | None = None
    usage_source: str | None = None  # "estimated" | "provider_reported" | "tokenizer_estimated"
    schema_version: str = EXECUTION_RESULT_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "result_id": self.result_id,
            "dispatch_id": self.dispatch_id,
            "decision_id": self.decision_id,
            "executor_type": self.executor_type,
            "status": self.status,
            "output": self.output,
            "prompt_pack": self.prompt_pack,
            "input_tokens": self.input_tokens,
            "output_tokens": self.output_tokens,
            "estimated_cost": self.estimated_cost,
            "latency_ms": self.latency_ms,
            "error_domain": self.error_domain,
            "error_message": self.error_message,
            "provider_request_id": self.provider_request_id,
            "attempt_number": self.attempt_number,
            "finish_reason": self.finish_reason,
            "usage_source": self.usage_source,
            "created_at": self.created_at,
        }


# ---------------------------------------------------------------------------
# Executors
# ---------------------------------------------------------------------------


class NoopExecutor:
    """Returns planned/not_executed. Default executor for Phase 1."""

    def execute(
        self, decision: DispatchDecision, raw_request: str, dispatch_id: str
    ) -> ExecutionResult:
        return ExecutionResult(
            result_id=f"exec-{uuid.uuid4().hex[:12]}",
            dispatch_id=dispatch_id,
            decision_id=decision.decision_id,
            executor_type="noop",
            status="not_executed",
            created_at=datetime.now(timezone.utc).isoformat(),
        )


class MockExecutor:
    """Returns deterministic fake output based on the request."""

    def execute(
        self, decision: DispatchDecision, raw_request: str, dispatch_id: str
    ) -> ExecutionResult:
        mock_output = self._generate_mock_output(raw_request, decision)
        return ExecutionResult(
            result_id=f"exec-{uuid.uuid4().hex[:12]}",
            dispatch_id=dispatch_id,
            decision_id=decision.decision_id,
            executor_type="mock",
            status="mock_completed",
            output=mock_output,
            input_tokens=len(raw_request.split()) * 2,
            output_tokens=len(mock_output.split()) * 2,
            estimated_cost=0.0,
            latency_ms=0,
            created_at=datetime.now(timezone.utc).isoformat(),
        )

    def _generate_mock_output(self, raw_request: str, decision: DispatchDecision) -> str:
        tier = decision.selected_tier
        return f"[mock:{tier}] Analysis complete for: {raw_request[:80]}..."


class ManualExecutor:
    """Generates prompt pack for human execution. Phase 2 bridge."""

    def execute(
        self, decision: DispatchDecision, raw_request: str, dispatch_id: str
    ) -> ExecutionResult:
        prompt_pack = {
            "raw_request": raw_request,
            "recommended_model_tier": decision.selected_tier,
            "budget_limit": decision.max_input_tokens + decision.max_output_tokens,
            "expected_output_schema": "free_text",
            "evaluation_checklist": [
                "schema_validity",
                "boundary_compliance",
                "output_present",
                "error_free",
            ],
            "pasteback_instructions": (
                "Execute the task using the recommended model tier, "
                "then paste the result back for evaluation."
            ),
        }
        return ExecutionResult(
            result_id=f"exec-{uuid.uuid4().hex[:12]}",
            dispatch_id=dispatch_id,
            decision_id=decision.decision_id,
            executor_type="manual",
            status="manual_pending",
            prompt_pack=prompt_pack,
            created_at=datetime.now(timezone.utc).isoformat(),
        )
