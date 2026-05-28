"""SDK: programmatic Python API for the harness dispatch kernel."""

from __future__ import annotations

import json
import time
from pathlib import Path
from typing import Any

from .dispatch.dispatch_engine import DispatchEngine
from .dispatch.durable_store import DurableStore
from .dispatch.health_checker import HealthChecker

SDK_SCHEMA_VERSION = "sdk.v1"


class HarnessSDK:
    """Convenience wrapper around DispatchEngine, DurableStore, and HealthChecker.

    Provides a single entry point for programmatic integration with the
    harness.  No network calls — pure Python API over local components.
    """

    def __init__(self, store_path: str | None = None) -> None:
        if store_path is not None:
            self._store = DurableStore(db_path=store_path)
        else:
            self._store = DurableStore()
        self._engine = DispatchEngine()
        self._health_checker = HealthChecker(store=self._store)

    def create_dispatch(self, request: dict[str, Any]) -> dict[str, Any]:
        """Create a dispatch decision from a raw request dict.

        The request dict should contain at least a 'raw_request' key with the
        task description string.  Returns the DispatchBundle as a dict.
        """
        raw_request = request.get("raw_request", "")
        request_source = request.get("request_source", "api")
        bundle = self._engine.dispatch(
            raw_request=raw_request,
            request_source=request_source,
        )
        return {
            "dispatch_id": bundle.record.dispatch_id,
            "decision": bundle.decision.to_dict(),
            "record": bundle.record.to_dict(),
            "execution_status": bundle.execution_result.status,
            "evaluation_status": bundle.evaluation_result.status,
        }

    def list_plans(self) -> list[dict[str, Any]]:
        """List all plans stored in the durable store."""
        records = self._store.list_plans()
        return [
            {
                "id": r.record_id,
                "created_at": r.created_at,
                "schema_version": r.schema_version,
                "data": r.data,
            }
            for r in records
        ]

    def get_plan(self, plan_id: str) -> dict[str, Any] | None:
        """Retrieve a single plan by ID."""
        record = self._store.get_plan(plan_id)
        if record is None:
            return None
        return {
            "id": record.record_id,
            "created_at": record.created_at,
            "schema_version": record.schema_version,
            "data": record.data,
        }

    def validate_events(self, events_path: str) -> dict[str, Any]:
        """Validate events in a JSONL file against the event.v1 schema.

        Returns a summary dict with ok, total, valid, invalid counts, and
        any error messages.
        """
        try:
            from .validators import validate_events_schema
        except ImportError:
            return {
                "ok": False,
                "total": 0,
                "valid": 0,
                "invalid": 0,
                "errors": ["validators module not available"],
            }

        path = Path(events_path)
        if not path.exists():
            return {
                "ok": False,
                "total": 0,
                "valid": 0,
                "invalid": 0,
                "errors": [f"file not found: {events_path}"],
            }

        total = 0
        valid = 0
        invalid = 0
        errors: list[str] = []
        for line_num, raw_line in enumerate(path.read_text().splitlines(), start=1):
            line = raw_line.strip()
            if not line:
                continue
            total += 1
            try:
                event = json.loads(line)
            except json.JSONDecodeError as exc:
                invalid += 1
                errors.append(f"line {line_num}: JSON parse error: {exc}")
                continue
            result = validate_events_schema(event)
            if result.ok:
                valid += 1
            else:
                invalid += 1
                for err in result.errors:
                    errors.append(f"line {line_num}: {err}")

        return {
            "ok": invalid == 0,
            "total": total,
            "valid": valid,
            "invalid": invalid,
            "errors": errors,
        }

    def health_check(self) -> dict[str, Any]:
        """Run health checks on storage, events, and plans."""
        return self._health_checker.health_dict()

    def get_status(self) -> dict[str, Any]:
        """Return overall status: storage health, stats, and SDK version."""
        health = self._health_checker.health_dict()
        stats = self._store.stats()
        return {
            "schema_version": SDK_SCHEMA_VERSION,
            "health": health,
            "storage": stats,
            "timestamp": time.time(),
        }

    def close(self) -> None:
        """Close the underlying durable store."""
        self._store.close()
