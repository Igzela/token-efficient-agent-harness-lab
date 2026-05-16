"""Advisor Broker for Stage 3 — stub-first advisor protocol integration."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Protocol

from .validators import CANONICAL_FAILURE_CODES, ValidationResult


@dataclass(frozen=True)
class AdvisorContextPack:
    task_id: str
    call_type: str  # preflight | correction | arbitration | risk_scan
    task_spec: dict[str, Any]
    completion: dict[str, Any] | None = None
    handoff_pack: dict[str, Any] | None = None
    run_log_text: str | None = None
    failure_code: str | None = None
    project_context: dict[str, Any] | None = None


@dataclass(frozen=True)
class AdvisorResponse:
    call_type: str
    diagnosis: str
    recommended_action: str
    do_not_do: str
    confidence: float  # 0.0 - 1.0
    token_usage: int
    provider: str  # "stub" | provider name
    raw_response: dict[str, Any] | None = None


@dataclass(frozen=True)
class AdvisorBudget:
    max_tokens: int = 2000
    max_calls_per_task: int = 3
    current_calls: int = 0
    current_tokens: int = 0


class AdvisorProvider(Protocol):
    def invoke(
        self, context: AdvisorContextPack, budget: AdvisorBudget
    ) -> AdvisorResponse: ...


_VALID_CALL_TYPES = frozenset({"preflight", "correction", "arbitration", "risk_scan"})


def _clamp(value: float, lo: float = 0.0, hi: float = 1.0) -> float:
    return max(lo, min(hi, value))


class StubAdvisorProvider:
    """Deterministic stub. Returns fixed responses based on context fields."""

    def invoke(
        self, context: AdvisorContextPack, budget: AdvisorBudget
    ) -> AdvisorResponse:
        if context.call_type == "preflight":
            return self._preflight(context)
        if context.call_type == "correction":
            return self._correction(context)
        if context.call_type == "arbitration":
            return self._arbitration(context)
        if context.call_type == "risk_scan":
            return self._risk_scan(context)
        return AdvisorResponse(
            call_type=context.call_type,
            diagnosis=f"unknown call_type: {context.call_type}",
            recommended_action="none",
            do_not_do="do not proceed with unknown call type",
            confidence=0.0,
            token_usage=50,
            provider="stub",
        )

    def _preflight(self, context: AdvisorContextPack) -> AdvisorResponse:
        return AdvisorResponse(
            call_type="preflight",
            diagnosis="Task scope is clear, dependencies satisfied",
            recommended_action="Proceed with execution",
            do_not_do="Do not modify files outside allowed_files",
            confidence=0.9,
            token_usage=120,
            provider="stub",
        )

    def _correction(self, context: AdvisorContextPack) -> AdvisorResponse:
        failure_code = context.failure_code or "UNKNOWN"
        guidance_map = {
            "F001_TIMEOUT": "Increase timeout or reduce task scope",
            "F002_BUDGET_EXCEEDED": "Reduce token usage or split task",
            "F003_DEPENDENCY_FAILED": "Check upstream task completion",
            "F004_APPROVAL_REJECTED": "Review approval request and revise",
            "F005_PROVIDER_UNAVAILABLE": "Retry with backoff or switch provider",
            "F006_SCOPE_VIOLATION": "Restrict file access to allowed_files",
            "F007_TEST_FAILURE": "Fix failing tests before retry",
            "F008_FORMAT_ERROR": "Validate output schema before submission",
            "F009_POLICY_VIOLATION": "Review policy constraints",
            "F010_CANCELLED": "Task was cancelled; do not retry",
        }
        guidance = guidance_map.get(failure_code, f"Investigate failure code {failure_code}")
        return AdvisorResponse(
            call_type="correction",
            diagnosis=f"Failure code {failure_code} detected",
            recommended_action=guidance,
            do_not_do="Do not retry without addressing root cause",
            confidence=0.8,
            token_usage=150,
            provider="stub",
        )

    def _arbitration(self, context: AdvisorContextPack) -> AdvisorResponse:
        score = 0.0
        if context.completion:
            score = context.completion.get("score", 0.0)
        if score >= 0.6:
            return AdvisorResponse(
                call_type="arbitration",
                diagnosis=f"Score {score:.2f} meets threshold",
                recommended_action="pass",
                do_not_do="Do not override passing score",
                confidence=0.85,
                token_usage=100,
                provider="stub",
            )
        return AdvisorResponse(
            call_type="arbitration",
            diagnosis=f"Score {score:.2f} below threshold",
            recommended_action="fail; escalate to human review",
            do_not_do="Do not auto-retry without human approval",
            confidence=0.75,
            token_usage=100,
            provider="stub",
        )

    def _risk_scan(self, context: AdvisorContextPack) -> AdvisorResponse:
        risk_level = "low"
        if context.completion:
            actions = context.completion.get("requested_action", [])
            if isinstance(actions, list):
                high_risk = {"delete_files", "submit_pr", "run_command"}
                if any(a in high_risk for a in actions):
                    risk_level = "high"
        return AdvisorResponse(
            call_type="risk_scan",
            diagnosis=f"Risk level: {risk_level}",
            recommended_action=f"Proceed with {risk_level} risk mitigation",
            do_not_do="Do not skip approval for high-risk actions",
            confidence=0.85,
            token_usage=80,
            provider="stub",
        )


class AdvisorProtocolValidator:
    """Validate advisor protocol objects against expected schemas."""

    def validate_response(self, response: AdvisorResponse) -> ValidationResult:
        errors: list[str] = []
        if response.call_type not in _VALID_CALL_TYPES:
            errors.append(
                f"invalid call_type: {response.call_type!r}; expected one of {_VALID_CALL_TYPES}"
            )
        if not (0.0 <= response.confidence <= 1.0):
            errors.append(
                f"confidence {response.confidence} out of range [0.0, 1.0]"
            )
        if response.token_usage < 0:
            errors.append(f"token_usage {response.token_usage} must be >= 0")
        if not response.provider:
            errors.append("provider must not be empty")
        return ValidationResult(ok=len(errors) == 0, errors=tuple(errors))

    def validate_context_pack(self, pack: AdvisorContextPack) -> ValidationResult:
        errors: list[str] = []
        if not pack.task_id:
            errors.append("task_id must not be empty")
        if pack.call_type not in _VALID_CALL_TYPES:
            errors.append(
                f"invalid call_type: {pack.call_type!r}; expected one of {_VALID_CALL_TYPES}"
            )
        if not pack.task_spec:
            errors.append("task_spec must not be empty")
        return ValidationResult(ok=len(errors) == 0, errors=tuple(errors))

    def validate_budget(self, budget: AdvisorBudget) -> ValidationResult:
        errors: list[str] = []
        if budget.max_tokens <= 0:
            errors.append(f"max_tokens {budget.max_tokens} must be > 0")
        if budget.max_calls_per_task <= 0:
            errors.append(
                f"max_calls_per_task {budget.max_calls_per_task} must be > 0"
            )
        if budget.current_calls < 0:
            errors.append(f"current_calls {budget.current_calls} must be >= 0")
        if budget.current_tokens < 0:
            errors.append(f"current_tokens {budget.current_tokens} must be >= 0")
        return ValidationResult(ok=len(errors) == 0, errors=tuple(errors))


class AdvisorBrokerBudgetExceeded(Exception):
    """Raised when advisor budget is exceeded."""

    def __init__(self, message: str, budget: AdvisorBudget):
        super().__init__(message)
        self.budget = budget


class AdvisorBroker:
    """Evaluate quality through structured advisor calls."""

    def __init__(self, provider: AdvisorProvider, budget: AdvisorBudget):
        self._provider = provider
        self._budget = budget
        self._validator = AdvisorProtocolValidator()

    @property
    def budget(self) -> AdvisorBudget:
        return self._budget

    def _check_budget(self) -> None:
        if self._budget.current_calls >= self._budget.max_calls_per_task:
            raise AdvisorBrokerBudgetExceeded(
                f"max calls ({self._budget.max_calls_per_task}) exceeded",
                self._budget,
            )
        if self._budget.current_tokens >= self._budget.max_tokens:
            raise AdvisorBrokerBudgetExceeded(
                f"max tokens ({self._budget.max_tokens}) exceeded",
                self._budget,
            )

    def _invoke(self, context: AdvisorContextPack) -> AdvisorResponse:
        self._check_budget()
        ctx_validation = self._validator.validate_context_pack(context)
        if not ctx_validation.ok:
            return AdvisorResponse(
                call_type=context.call_type,
                diagnosis=f"invalid context: {'; '.join(ctx_validation.errors)}",
                recommended_action="fix context before retrying",
                do_not_do="do not proceed with invalid context",
                confidence=0.0,
                token_usage=0,
                provider="stub",
            )
        response = self._provider.invoke(context, self._budget)
        self._budget = AdvisorBudget(
            max_tokens=self._budget.max_tokens,
            max_calls_per_task=self._budget.max_calls_per_task,
            current_calls=self._budget.current_calls + 1,
            current_tokens=self._budget.current_tokens + response.token_usage,
        )
        return response

    def preflight(self, context: AdvisorContextPack) -> AdvisorResponse:
        return self._invoke(
            AdvisorContextPack(
                task_id=context.task_id,
                call_type="preflight",
                task_spec=context.task_spec,
                completion=context.completion,
                handoff_pack=context.handoff_pack,
                run_log_text=context.run_log_text,
                failure_code=context.failure_code,
                project_context=context.project_context,
            )
        )

    def correction(self, context: AdvisorContextPack) -> AdvisorResponse:
        return self._invoke(
            AdvisorContextPack(
                task_id=context.task_id,
                call_type="correction",
                task_spec=context.task_spec,
                completion=context.completion,
                handoff_pack=context.handoff_pack,
                run_log_text=context.run_log_text,
                failure_code=context.failure_code,
                project_context=context.project_context,
            )
        )

    def arbitration(self, context: AdvisorContextPack) -> AdvisorResponse:
        return self._invoke(
            AdvisorContextPack(
                task_id=context.task_id,
                call_type="arbitration",
                task_spec=context.task_spec,
                completion=context.completion,
                handoff_pack=context.handoff_pack,
                run_log_text=context.run_log_text,
                failure_code=context.failure_code,
                project_context=context.project_context,
            )
        )

    def risk_scan(self, context: AdvisorContextPack) -> AdvisorResponse:
        return self._invoke(
            AdvisorContextPack(
                task_id=context.task_id,
                call_type="risk_scan",
                task_spec=context.task_spec,
                completion=context.completion,
                handoff_pack=context.handoff_pack,
                run_log_text=context.run_log_text,
                failure_code=context.failure_code,
                project_context=context.project_context,
            )
        )
