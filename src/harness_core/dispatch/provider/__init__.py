"""Provider integration for the dispatch kernel — real model API execution."""

from __future__ import annotations

from .audit_recorder import ProviderAuditEvent, ProviderAuditRecorder, PROVIDER_AUDIT_EVENT_SCHEMA_VERSION
from .credential_boundary import CredentialBoundary
from .provider_config import (
    BACKOFF_STRATEGIES,
    CREDENTIAL_STORAGE_BACKENDS,
    CREDENTIAL_REF_SCHEMA_VERSION,
    PROVIDER_AUDIT_EVENT_TYPES,
    PROVIDER_CONFIG_SCHEMA_VERSION,
    PROVIDER_TYPES,
    REDACTION_STATUSES,
    RETRY_POLICY_SCHEMA_VERSION,
    CredentialRef,
    ProviderConfig,
    RetryPolicy,
)
from .provider_executor import ProviderExecutor, StubProvider
from .redaction import redact_audit_fields, redact_secrets
from .retry_manager import RetryFallbackManager

__all__ = [
    "BACKOFF_STRATEGIES",
    "CREDENTIAL_REF_SCHEMA_VERSION",
    "CREDENTIAL_STORAGE_BACKENDS",
    "CredentialBoundary",
    "CredentialRef",
    "ProviderAuditEvent",
    "ProviderAuditRecorder",
    "PROVIDER_AUDIT_EVENT_SCHEMA_VERSION",
    "PROVIDER_AUDIT_EVENT_TYPES",
    "PROVIDER_CONFIG_SCHEMA_VERSION",
    "PROVIDER_TYPES",
    "ProviderConfig",
    "ProviderExecutor",
    "REDACTION_STATUSES",
    "RETRY_POLICY_SCHEMA_VERSION",
    "RetryFallbackManager",
    "RetryPolicy",
    "StubProvider",
    "redact_audit_fields",
    "redact_secrets",
]
