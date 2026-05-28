"""Anthropic-compatible provider adapter for mimo and other Anthropic-format APIs."""

from __future__ import annotations

import json
import logging
import time
import uuid
from datetime import datetime, timezone

_log = logging.getLogger(__name__)
from typing import Any

from ..dispatch_decision import DispatchDecision
from ..executor_adapter import ExecutionResult
from .audit_recorder import ProviderAuditRecorder
from .credential_boundary import CredentialBoundary
from .provider_config import CredentialRef, ProviderConfig
from .provider_executor import ProviderExecutor

_ERROR_DOMAIN_MAP: dict[int, str] = {
    401: "provider_auth",
    403: "provider_auth",
    429: "provider_rate_limit",
    500: "provider_capacity",
    502: "provider_capacity",
    503: "provider_capacity",
    504: "provider_timeout",
}


class AnthropicProviderRequest:
    def __init__(
        self,
        url: str,
        data: bytes | None = None,
        headers: dict[str, str] | None = None,
        method: str = "GET",
    ) -> None:
        self.url = url
        self.data = data
        self.headers = headers or {}
        self.method = method


class AnthropicHTTPError(Exception):
    def __init__(self, code: int, reason: str) -> None:
        super().__init__(reason)
        self.code = code
        self.reason = reason


class AnthropicConnectionError(Exception):
    def __init__(self, reason: str) -> None:
        super().__init__(reason)
        self.reason = reason


def anthropic_urlopen(req: AnthropicProviderRequest, timeout: float | None = None) -> Any:
    raise AnthropicConnectionError("network transport not configured")


class AnthropicProvider(ProviderExecutor):
    """Anthropic-compatible provider (mimo, Claude, etc.)."""

    def __init__(
        self,
        config: ProviderConfig,
        credential_boundary: CredentialBoundary,
        credential_ref: CredentialRef,
        audit_recorder: ProviderAuditRecorder | None = None,
    ) -> None:
        self._config = config
        self._boundary = credential_boundary
        self._cred_ref = credential_ref
        self._audit = audit_recorder

    def execute(
        self, decision: DispatchDecision, raw_request: str, dispatch_id: str
    ) -> ExecutionResult:
        if not self._config.enabled:
            self._record_event(dispatch_id, "error", error_domain="provider_disabled")
            return ExecutionResult(
                result_id=f"exec-{uuid.uuid4().hex[:12]}",
                dispatch_id=dispatch_id,
                decision_id=decision.decision_id,
                executor_type="provider",
                status="not_executed",
                error_domain="provider_disabled",
                error_message="provider is disabled in config",
                attempt_number=0,
                created_at=datetime.now(timezone.utc).isoformat(),
            )

        try:
            api_key = self._boundary.resolve(self._cred_ref)
        except ValueError as e:
            self._record_event(dispatch_id, "error", error_domain="provider_auth")
            return ExecutionResult(
                result_id=f"exec-{uuid.uuid4().hex[:12]}",
                dispatch_id=dispatch_id,
                decision_id=decision.decision_id,
                executor_type="provider",
                status="failed",
                error_domain="provider_auth",
                error_message=str(e),
                attempt_number=1,
                created_at=datetime.now(timezone.utc).isoformat(),
            )

        start = time.monotonic()
        self._record_event(dispatch_id, "request_sent")

        try:
            max_tokens = decision.max_output_tokens or 1024
            response_data = self._call_api(api_key, raw_request, max_tokens)
            latency_ms = int((time.monotonic() - start) * 1000)

            content_blocks = response_data.get("content", [])
            content = "".join(
                block.get("text", "") for block in content_blocks if block.get("type") == "text"
            )

            usage = response_data.get("usage", {})
            input_tok = usage.get("input_tokens", 0) or 0
            output_tok = usage.get("output_tokens", 0) or 0
            estimated_cost = None
            if self._config.input_cost_per_1k is not None and self._config.output_cost_per_1k is not None:
                estimated_cost = (input_tok / 1000) * self._config.input_cost_per_1k + (output_tok / 1000) * self._config.output_cost_per_1k

            result = ExecutionResult(
                result_id=f"exec-{uuid.uuid4().hex[:12]}",
                dispatch_id=dispatch_id,
                decision_id=decision.decision_id,
                executor_type="provider",
                status="provider_completed",
                output=content,
                input_tokens=input_tok,
                output_tokens=output_tok,
                estimated_cost=estimated_cost,
                latency_ms=latency_ms,
                provider_request_id=response_data.get("id"),
                attempt_number=1,
                finish_reason=response_data.get("stop_reason"),
                usage_source="provider_reported",
                created_at=datetime.now(timezone.utc).isoformat(),
            )

            self._record_event(
                dispatch_id, "response_received",
                input_token_count=input_tok,
                output_token_count=output_tok,
                latency_ms=latency_ms,
            )
            return result

        except (json.JSONDecodeError, KeyError, IndexError, TypeError) as e:
            latency_ms = int((time.monotonic() - start) * 1000)
            self._record_event(
                dispatch_id, "error",
                error_domain="provider_error",
                latency_ms=latency_ms,
            )
            return ExecutionResult(
                result_id=f"exec-{uuid.uuid4().hex[:12]}",
                dispatch_id=dispatch_id,
                decision_id=decision.decision_id,
                executor_type="provider",
                status="failed",
                error_domain="provider_error",
                error_message=f"Response parse error: {e}",
                latency_ms=latency_ms,
                attempt_number=1,
                created_at=datetime.now(timezone.utc).isoformat(),
            )

        except AnthropicHTTPError as e:
            latency_ms = int((time.monotonic() - start) * 1000)
            error_domain = _ERROR_DOMAIN_MAP.get(e.code, "provider_error")
            self._record_event(
                dispatch_id, "error",
                error_domain=error_domain,
                latency_ms=latency_ms,
            )
            return ExecutionResult(
                result_id=f"exec-{uuid.uuid4().hex[:12]}",
                dispatch_id=dispatch_id,
                decision_id=decision.decision_id,
                executor_type="provider",
                status="failed",
                error_domain=error_domain,
                error_message=f"HTTP {e.code}: {e.reason}",
                latency_ms=latency_ms,
                attempt_number=1,
                created_at=datetime.now(timezone.utc).isoformat(),
            )

        except AnthropicConnectionError as e:
            latency_ms = int((time.monotonic() - start) * 1000)
            self._record_event(
                dispatch_id, "error",
                error_domain="provider_timeout",
                latency_ms=latency_ms,
            )
            return ExecutionResult(
                result_id=f"exec-{uuid.uuid4().hex[:12]}",
                dispatch_id=dispatch_id,
                decision_id=decision.decision_id,
                executor_type="provider",
                status="failed",
                error_domain="provider_timeout",
                error_message=str(e.reason),
                latency_ms=latency_ms,
                attempt_number=1,
                created_at=datetime.now(timezone.utc).isoformat(),
            )

    def health_check(self) -> bool:
        try:
            api_key = self._boundary.resolve(self._cred_ref)
            url = f"{self._config.base_url}/v1/messages"
            req = AnthropicProviderRequest(
                url,
                data=json.dumps({
                    "model": self._config.model_id,
                    "max_tokens": 1,
                    "messages": [{"role": "user", "content": "ping"}],
                }).encode(),
                headers={
                    "Content-Type": "application/json",
                    "x-api-key": api_key,
                    "anthropic-version": "2023-06-01",
                },
                method="POST",
            )
            with anthropic_urlopen(req, timeout=5) as resp:
                return resp.status == 200
        except Exception:
            _log.exception("anthropic health_check failed")
            return False

    def _call_api(self, api_key: str, prompt: str, max_tokens: int = 1024) -> dict[str, Any]:
        url = f"{self._config.base_url}/v1/messages"
        payload = json.dumps({
            "model": self._config.model_id,
            "max_tokens": max_tokens,
            "messages": [{"role": "user", "content": prompt}],
        }).encode()

        req = AnthropicProviderRequest(
            url,
            data=payload,
            headers={
                "Content-Type": "application/json",
                "x-api-key": api_key,
                "anthropic-version": "2023-06-01",
            },
            method="POST",
        )

        timeout_s = self._config.timeout_ms / 1000
        with anthropic_urlopen(req, timeout=timeout_s) as resp:
            return json.loads(resp.read().decode())

    def _record_event(self, dispatch_id: str, event_type: str, **kwargs: Any) -> None:
        if self._audit:
            self._audit.create_and_record(
                dispatch_id=dispatch_id,
                provider_id=self._config.provider_id,
                event_type=event_type,
                **kwargs,
            )
