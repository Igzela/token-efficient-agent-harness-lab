"""RetryFallbackManager — retry with budget check, fallback routing."""

from __future__ import annotations

import time
from typing import Callable

from ..dispatch_decision import DispatchDecision
from ..executor_adapter import ExecutionResult
from .provider_config import RetryPolicy
from .provider_executor import ProviderExecutor


class RetryFallbackManager:
    """Wraps a primary provider with retry logic and optional fallback."""

    def __init__(
        self,
        primary: ProviderExecutor,
        fallback: ProviderExecutor | None,
        retry_policy: RetryPolicy,
        budget_check: Callable[[], bool],
    ) -> None:
        self._primary = primary
        self._fallback = fallback
        self._policy = retry_policy
        self._budget_check = budget_check

    def execute(
        self, decision: DispatchDecision, raw_request: str, dispatch_id: str
    ) -> ExecutionResult:
        result = self._try_execute(self._primary, decision, raw_request, dispatch_id, attempt=1)

        if result.status == "failed" and self._is_retryable(result):
            for attempt in range(2, self._policy.max_retries + 1):
                if self._policy.budget_check_per_retry and not self._budget_check():
                    result = self._with_error(result, "budget_exhausted", "Budget exhausted before retry")
                    break
                self._wait_backoff(attempt)
                result = self._try_execute(self._primary, decision, raw_request, dispatch_id, attempt=attempt)
                if result.status != "failed" or not self._is_retryable(result):
                    break

        if result.error_domain == "budget_exhausted":
            return result

        if result.status == "failed" and self._fallback is not None:
            result = self._try_execute(self._fallback, decision, raw_request, dispatch_id, attempt=1)

        return result

    def _try_execute(
        self,
        provider: ProviderExecutor,
        decision: DispatchDecision,
        raw_request: str,
        dispatch_id: str,
        attempt: int,
    ) -> ExecutionResult:
        try:
            result = provider.execute(decision, raw_request, dispatch_id)
            return ExecutionResult(
                result_id=result.result_id,
                dispatch_id=result.dispatch_id,
                decision_id=result.decision_id,
                executor_type=result.executor_type,
                status=result.status,
                output=result.output,
                prompt_pack=result.prompt_pack,
                input_tokens=result.input_tokens,
                output_tokens=result.output_tokens,
                estimated_cost=result.estimated_cost,
                latency_ms=result.latency_ms,
                error_domain=result.error_domain,
                error_message=result.error_message,
                provider_request_id=result.provider_request_id,
                attempt_number=attempt,
                finish_reason=result.finish_reason,
                usage_source=result.usage_source,
                created_at=result.created_at,
            )
        except Exception as e:
            return ExecutionResult(
                result_id=f"exec-{__import__('uuid').uuid4().hex[:12]}",
                dispatch_id=dispatch_id,
                decision_id=decision.decision_id,
                executor_type="provider",
                status="failed",
                error_domain="provider_error",
                error_message=str(e),
                attempt_number=attempt,
                created_at=__import__("datetime").datetime.now(__import__("datetime").timezone.utc).isoformat(),
            )

    def _is_retryable(self, result: ExecutionResult) -> bool:
        return result.error_domain in self._policy.retryable_error_domains

    def _wait_backoff(self, attempt: int) -> None:
        if self._policy.backoff_strategy == "none":
            return
        delay_ms = self._policy.base_delay_ms
        if self._policy.backoff_strategy == "exponential":
            delay_ms = min(
                self._policy.base_delay_ms * (2 ** (attempt - 1)),
                self._policy.max_delay_ms,
            )
        elif self._policy.backoff_strategy == "linear":
            delay_ms = min(
                self._policy.base_delay_ms * attempt,
                self._policy.max_delay_ms,
            )
        time.sleep(delay_ms / 1000)

    def _with_error(self, result: ExecutionResult, domain: str, message: str) -> ExecutionResult:
        return ExecutionResult(
            result_id=result.result_id,
            dispatch_id=result.dispatch_id,
            decision_id=result.decision_id,
            executor_type=result.executor_type,
            status="failed",
            error_domain=domain,
            error_message=message,
            attempt_number=result.attempt_number,
            created_at=result.created_at,
        )
