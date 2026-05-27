"""ProviderAuditEvent schema and ProviderAuditRecorder — in-memory audit store."""

from __future__ import annotations

import uuid
from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Any

from .provider_config import PROVIDER_AUDIT_EVENT_TYPES, REDACTION_STATUSES

# ---------------------------------------------------------------------------
# Schema version
# ---------------------------------------------------------------------------

PROVIDER_AUDIT_EVENT_SCHEMA_VERSION = "provider_audit_event.v1"


# ---------------------------------------------------------------------------
# Schema
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class ProviderAuditEvent:
    event_id: str
    dispatch_id: str
    provider_id: str
    event_type: str  # from PROVIDER_AUDIT_EVENT_TYPES
    input_token_count: int | None = None
    output_token_count: int | None = None
    cost: float | None = None
    currency: str | None = None
    latency_ms: int | None = None
    error_domain: str | None = None
    redaction_status: str = "not_applicable"  # from REDACTION_STATUSES
    created_at: str = field(default_factory=lambda: datetime.now(timezone.utc).isoformat())
    schema_version: str = PROVIDER_AUDIT_EVENT_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "event_id": self.event_id,
            "dispatch_id": self.dispatch_id,
            "provider_id": self.provider_id,
            "event_type": self.event_type,
            "input_token_count": self.input_token_count,
            "output_token_count": self.output_token_count,
            "cost": self.cost,
            "currency": self.currency,
            "latency_ms": self.latency_ms,
            "error_domain": self.error_domain,
            "redaction_status": self.redaction_status,
            "created_at": self.created_at,
        }


# ---------------------------------------------------------------------------
# Recorder
# ---------------------------------------------------------------------------


class ProviderAuditRecorder:
    """In-memory store for provider audit events."""

    def __init__(self) -> None:
        self._events: list[ProviderAuditEvent] = []

    def record(self, event: ProviderAuditEvent) -> None:
        self._events.append(event)

    def create_and_record(
        self,
        dispatch_id: str,
        provider_id: str,
        event_type: str,
        **kwargs: Any,
    ) -> ProviderAuditEvent:
        event = ProviderAuditEvent(
            event_id=f"paudit-{uuid.uuid4().hex[:12]}",
            dispatch_id=dispatch_id,
            provider_id=provider_id,
            event_type=event_type,
            **kwargs,
        )
        self.record(event)
        return event

    def list_events(self, dispatch_id: str) -> list[ProviderAuditEvent]:
        return [e for e in self._events if e.dispatch_id == dispatch_id]

    def list_all(self) -> list[ProviderAuditEvent]:
        return list(self._events)

    def count(self) -> int:
        return len(self._events)
