"""OpenAI-compatible provider — real API calls via urllib."""

from __future__ import annotations

import json
import time
import urllib.error
import urllib.request
import uuid
from datetime import datetime, timezone
from typing import Any

from ..dispatch_decision import DispatchDecision
from ..executor_adapter import ExecutionResult
from .audit_recorder import ProviderAuditRecorder
from .credential_boundary import CredentialBoundary
from .provider_config import CredentialRef, ProviderConfig
from .provider_executor import ProviderExecutor

# Provider-specific error domain mapping
_ERROR_DOMAIN_MAP: dict[int, str] = {
    401: "provider_auth",
    403: "provider_auth",
    429: "provider_rate_limit",
    500: "provider_capacity",
    502: "provider_capacity",
    503: "provider_capacity",
    504: "provider_timeout",
}


class OpenAIProvider(ProviderExecutor):
    """OpenAI-compatible API provider using stdlib urllib."""

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
            response_data = self._call_api(api_key, raw_request)
            latency_ms = int((time.monotonic() - start) * 1000)
            usage = response_data.get("usage", {})
            content = response_data["choices"][0]["message"]["content"]

            result = ExecutionResult(
                result_id=f"exec-{uuid.uuid4().hex[:12]}",
                dispatch_id=dispatch_id,
                decision_id=decision.decision_id,
                executor_type="provider",
                status="provider_completed",
                output=content,
                input_tokens=usage.get("prompt_tokens"),
                output_tokens=usage.get("completion_tokens"),
                estimated_cost=None,
                latency_ms=latency_ms,
                provider_request_id=response_data.get("id"),
                attempt_number=1,
                finish_reason=response_data["choices"][0].get("finish_reason"),
                usage_source="provider_reported",
                created_at=datetime.now(timezone.utc).isoformat(),
            )

            self._record_event(
                dispatch_id, "response_received",
                input_token_count=usage.get("prompt_tokens"),
                output_token_count=usage.get("completion_tokens"),
                latency_ms=latency_ms,
            )
            return result

        except urllib.error.HTTPError as e:
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

        except urllib.error.URLError as e:
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
            url = f"{self._config.base_url}/models"
            req = urllib.request.Request(
                url,
                headers={"Authorization": f"Bearer {api_key}"},
                method="GET",
            )
            with urllib.request.urlopen(req, timeout=5) as resp:
                return resp.status == 200
        except Exception:
            return False

    def _call_api(self, api_key: str, prompt: str) -> dict[str, Any]:
        url = f"{self._config.base_url}/chat/completions"
        payload = json.dumps({
            "model": self._config.model_id,
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": 1024,
        }).encode()

        req = urllib.request.Request(
            url,
            data=payload,
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {api_key}",
            },
            method="POST",
        )

        timeout_s = self._config.timeout_ms / 1000
        with urllib.request.urlopen(req, timeout=timeout_s) as resp:
            return json.loads(resp.read().decode())

    def _record_event(self, dispatch_id: str, event_type: str, **kwargs: Any) -> None:
        if self._audit:
            self._audit.create_and_record(
                dispatch_id=dispatch_id,
                provider_id=self._config.provider_id,
                event_type=event_type,
                **kwargs,
            )
