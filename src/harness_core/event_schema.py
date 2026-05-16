"""Minimal event.v1 schema validation for Stage 1 Day 1."""

from __future__ import annotations

import hashlib
import json
from typing import Any

from .errors import SchemaViolationError

REQUIRED_FIELDS = {
    "event_id",
    "schema_version",
    "event_type",
    "timestamp",
    "producer",
    "correlation",
    "severity",
    "payload",
    "idempotency_key",
    "parent_event_id",
}

REQUIRED_PRODUCER_FIELDS = {"component_id", "component_type"}
VALID_SEVERITIES = {"info", "warn", "error"}
IDEMPOTENCY_HASH_EXCLUDED_FIELDS = {"event_id", "timestamp"}


def validate_event(event: dict[str, Any]) -> None:
    """Validate the minimal event.v1 contract.

    This intentionally avoids implementing the full future validator suite.
    """
    if not isinstance(event, dict):
        raise SchemaViolationError("event must be a JSON object")

    missing = sorted(REQUIRED_FIELDS - event.keys())
    if missing:
        raise SchemaViolationError(f"missing required field(s): {', '.join(missing)}")

    _require_string(event, "event_id")
    _require_string(event, "event_type")
    _require_string(event, "timestamp")
    _require_string(event, "idempotency_key")

    if event["schema_version"] != "event.v1":
        raise SchemaViolationError("schema_version must be event.v1")

    if event["severity"] not in VALID_SEVERITIES:
        raise SchemaViolationError("severity must be one of: error, info, warn")

    if not isinstance(event["producer"], dict):
        raise SchemaViolationError("producer must be an object")
    missing_producer = sorted(REQUIRED_PRODUCER_FIELDS - event["producer"].keys())
    if missing_producer:
        raise SchemaViolationError(
            f"producer missing required field(s): {', '.join(missing_producer)}"
        )
    _require_string(event["producer"], "component_id", prefix="producer.")
    _require_string(event["producer"], "component_type", prefix="producer.")

    if not isinstance(event["correlation"], dict):
        raise SchemaViolationError("correlation must be an object")

    if not isinstance(event["payload"], dict):
        raise SchemaViolationError("payload must be an object")

    parent_event_id = event["parent_event_id"]
    if parent_event_id is not None and not isinstance(parent_event_id, str):
        raise SchemaViolationError("parent_event_id must be a string or null")


def stable_idempotency_hash(event: dict[str, Any]) -> str:
    """Hash semantic event content, excluding volatile identity fields."""
    semantic_event = {
        key: value
        for key, value in event.items()
        if key not in IDEMPOTENCY_HASH_EXCLUDED_FIELDS
    }
    canonical = json.dumps(semantic_event, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


def canonical_event_json(event: dict[str, Any]) -> str:
    """Serialize an event as one canonical JSON object."""
    return json.dumps(event, sort_keys=True, separators=(",", ":"))


def _require_string(mapping: dict[str, Any], key: str, prefix: str = "") -> None:
    if not isinstance(mapping[key], str) or not mapping[key]:
        raise SchemaViolationError(f"{prefix}{key} must be a non-empty string")
