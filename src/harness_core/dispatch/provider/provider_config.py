"""Provider configuration schemas — ProviderConfig, CredentialRef, RetryPolicy."""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Any

# ---------------------------------------------------------------------------
# Schema version
# ---------------------------------------------------------------------------

PROVIDER_CONFIG_SCHEMA_VERSION = "provider_config.v1"
CREDENTIAL_REF_SCHEMA_VERSION = "credential_ref.v1"
RETRY_POLICY_SCHEMA_VERSION = "retry_policy.v1"

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

PROVIDER_TYPES: tuple[str, ...] = ("openai_compatible", "anthropic", "local")
CREDENTIAL_STORAGE_BACKENDS: tuple[str, ...] = ("env", "file", "keyring", "vault")
BACKOFF_STRATEGIES: tuple[str, ...] = ("linear", "exponential", "none")
PROVIDER_AUDIT_EVENT_TYPES: tuple[str, ...] = (
    "request_sent", "response_received", "error", "timeout", "retry", "fallback",
)
REDACTION_STATUSES: tuple[str, ...] = ("redacted", "not_applicable")


# ---------------------------------------------------------------------------
# Schemas
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class ProviderConfig:
    provider_id: str
    provider_type: str  # from PROVIDER_TYPES
    base_url: str
    model_id: str
    credential_ref: str  # reference ID, never the secret itself
    timeout_ms: int = 30_000
    max_retries: int = 3
    rate_limit_policy_id: str | None = None
    enabled: bool = True
    input_cost_per_1k: float | None = None  # cost per 1k input tokens
    output_cost_per_1k: float | None = None  # cost per 1k output tokens
    currency: str = "USD"
    created_at: str = field(default_factory=lambda: datetime.now(timezone.utc).isoformat())
    schema_version: str = PROVIDER_CONFIG_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "provider_id": self.provider_id,
            "provider_type": self.provider_type,
            "base_url": self.base_url,
            "model_id": self.model_id,
            "credential_ref": self.credential_ref,
            "timeout_ms": self.timeout_ms,
            "max_retries": self.max_retries,
            "rate_limit_policy_id": self.rate_limit_policy_id,
            "enabled": self.enabled,
            "input_cost_per_1k": self.input_cost_per_1k,
            "output_cost_per_1k": self.output_cost_per_1k,
            "currency": self.currency,
            "created_at": self.created_at,
        }


@dataclass(frozen=True)
class CredentialRef:
    credential_ref_id: str  # env var name or file path
    storage_backend: str  # from CREDENTIAL_STORAGE_BACKENDS
    redacted_display: str  # e.g. "sk-***abc"
    scope: str  # e.g. "provider:openai"
    created_at: str = field(default_factory=lambda: datetime.now(timezone.utc).isoformat())
    schema_version: str = CREDENTIAL_REF_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "credential_ref_id": self.credential_ref_id,
            "storage_backend": self.storage_backend,
            "redacted_display": self.redacted_display,
            "scope": self.scope,
            "created_at": self.created_at,
        }


@dataclass(frozen=True)
class RetryPolicy:
    policy_id: str
    max_retries: int = 3  # retries after first attempt (total attempts = 1 + max_retries)
    backoff_strategy: str = "exponential"  # from BACKOFF_STRATEGIES
    base_delay_ms: int = 1000
    max_delay_ms: int = 30_000
    retryable_error_domains: tuple[str, ...] = ("provider_rate_limit", "provider_timeout", "provider_capacity")
    budget_check_per_retry: bool = True
    schema_version: str = RETRY_POLICY_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "policy_id": self.policy_id,
            "max_retries": self.max_retries,
            "backoff_strategy": self.backoff_strategy,
            "base_delay_ms": self.base_delay_ms,
            "max_delay_ms": self.max_delay_ms,
            "retryable_error_domains": list(self.retryable_error_domains),
            "budget_check_per_retry": self.budget_check_per_retry,
        }
