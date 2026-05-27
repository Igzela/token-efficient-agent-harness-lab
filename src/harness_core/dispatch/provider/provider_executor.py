"""Provider executor interface and StubProvider — deterministic fake for testing."""

from __future__ import annotations

import hashlib
import uuid
from datetime import datetime, timezone

from ..dispatch_decision import DispatchDecision
from ..executor_adapter import ExecutionResult


class ProviderExecutor:
    """Base class for provider executors. All providers implement this interface."""

    def execute(
        self, decision: DispatchDecision, raw_request: str, dispatch_id: str
    ) -> ExecutionResult:
        raise NotImplementedError

    def health_check(self) -> bool:
        raise NotImplementedError


class StubProvider(ProviderExecutor):
    """Deterministic fake provider — always succeeds, returns hash-based output."""

    def __init__(self, provider_id: str = "stub") -> None:
        self._provider_id = provider_id

    def execute(
        self, decision: DispatchDecision, raw_request: str, dispatch_id: str
    ) -> ExecutionResult:
        content_hash = hashlib.sha256(raw_request.encode()).hexdigest()[:16]
        output = f"[stub:{self._provider_id}] Response for hash {content_hash}"
        input_tokens = max(1, len(raw_request) // 4)
        output_tokens = max(1, len(output) // 4)

        return ExecutionResult(
            result_id=f"exec-{uuid.uuid4().hex[:12]}",
            dispatch_id=dispatch_id,
            decision_id=decision.decision_id,
            executor_type="provider",
            status="provider_completed",
            output=output,
            input_tokens=input_tokens,
            output_tokens=output_tokens,
            estimated_cost=round((input_tokens + output_tokens) / 1000 * 0.002, 6),
            latency_ms=0,
            provider_request_id=f"stub-{content_hash}",
            attempt_number=1,
            finish_reason="stop",
            usage_source="estimated",
            created_at=datetime.now(timezone.utc).isoformat(),
        )

    def health_check(self) -> bool:
        return True
