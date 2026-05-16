"""Append-only JSONL Event Store and replay preflight validation."""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from json import JSONDecodeError
from pathlib import Path
from typing import Any

from .errors import (
    DuplicateEventIdError,
    DuplicateIdempotencyConflictError,
    InvalidJsonLineError,
    MissingNewlineError,
    ReplayPreflightError,
    SchemaViolationError,
)
from .event_schema import canonical_event_json, stable_idempotency_hash, validate_event


@dataclass(frozen=True)
class ValidationIssue:
    line_number: int | None
    error_type: str
    message: str


@dataclass
class ValidationReport:
    path: Path
    errors: list[ValidationIssue] = field(default_factory=list)
    warnings: list[ValidationIssue] = field(default_factory=list)

    @property
    def ok(self) -> bool:
        return not self.errors


@dataclass
class ReplayPreflightReport(ValidationReport):
    event_count: int = 0
    event_ids: set[str] = field(default_factory=set)


class EventStore:
    """Append-only JSONL Event Store for Stage 1 Day 1."""

    def __init__(self, path: str | Path):
        self.path = Path(path)

    def append_event(self, event: dict[str, Any]) -> None:
        """Validate and append one canonical JSON object plus newline.

        A duplicate idempotency key with the same stable hash is treated as a
        successful no-op. Callers do not provide or manage JSONL newlines.
        """
        validate_event(event)
        event_hash = stable_idempotency_hash(event)
        event_ids, idempotency_hashes = self._load_indexes()

        idempotency_key = event["idempotency_key"]
        existing_hash = idempotency_hashes.get(idempotency_key)
        if existing_hash is not None:
            if existing_hash == event_hash:
                return
            raise DuplicateIdempotencyConflictError(
                f"idempotency_key already exists with different semantic hash: {idempotency_key}"
            )

        event_id = event["event_id"]
        if event_id in event_ids:
            raise DuplicateEventIdError(f"duplicate event_id: {event_id}")

        self.path.parent.mkdir(parents=True, exist_ok=True)
        with self.path.open("a", encoding="utf-8") as handle:
            handle.write(canonical_event_json(event) + "\n")

    def _load_indexes(self) -> tuple[set[str], dict[str, str]]:
        event_ids: set[str] = set()
        idempotency_hashes: dict[str, str] = {}

        if not self.path.exists():
            return event_ids, idempotency_hashes

        report = validate_jsonl_file(self.path)
        if not report.ok:
            raise ReplayPreflightError("existing event store failed validation")

        with self.path.open("r", encoding="utf-8") as handle:
            for line in handle:
                event = json.loads(line)
                event_ids.add(event["event_id"])
                idempotency_hashes[event["idempotency_key"]] = stable_idempotency_hash(event)

        return event_ids, idempotency_hashes


def validate_jsonl_file(path: str | Path) -> ValidationReport:
    """Validate one JSON object per newline-terminated JSONL line."""
    report = ValidationReport(path=Path(path))
    _validate_jsonl_into_report(Path(path), report)
    return report


def replay_preflight(path: str | Path) -> ReplayPreflightReport:
    """Run blocking checks before replaying an event stream."""
    report = ReplayPreflightReport(path=Path(path))
    events = _validate_jsonl_into_report(Path(path), report)

    seen_event_ids: dict[str, int] = {}
    available_event_ids = {event["event_id"] for _, event in events}

    for line_number, event in events:
        event_id = event["event_id"]
        if event_id in seen_event_ids:
            report.errors.append(
                ValidationIssue(
                    line_number=line_number,
                    error_type=DuplicateEventIdError.__name__,
                    message=(
                        f"duplicate event_id {event_id}; first seen on line "
                        f"{seen_event_ids[event_id]}"
                    ),
                )
            )
        else:
            seen_event_ids[event_id] = line_number

        parent_event_id = event.get("parent_event_id")
        if parent_event_id is not None and parent_event_id not in available_event_ids:
            report.warnings.append(
                ValidationIssue(
                    line_number=line_number,
                    error_type="MissingParentEventWarning",
                    message=f"parent_event_id does not exist in stream: {parent_event_id}",
                )
            )

    report.event_count = len(events)
    report.event_ids = set(seen_event_ids)
    return report


def load_event_ids(path: str | Path) -> set[str]:
    """Load event IDs from a valid JSONL event stream."""
    report = replay_preflight(path)
    if not report.ok:
        raise ReplayPreflightError("cannot load event IDs from invalid event stream")
    return report.event_ids


def _validate_jsonl_into_report(
    path: Path, report: ValidationReport
) -> list[tuple[int, dict[str, Any]]]:
    events: list[tuple[int, dict[str, Any]]] = []
    if not path.exists():
        return events

    with path.open("rb") as handle:
        for line_number, raw_line in enumerate(handle, start=1):
            if not raw_line.endswith(b"\n"):
                report.errors.append(
                    ValidationIssue(
                        line_number=line_number,
                        error_type=MissingNewlineError.__name__,
                        message="line is not newline-terminated",
                    )
                )

            line = raw_line.rstrip(b"\n")
            if line.endswith(b"\r"):
                line = line[:-1]

            try:
                decoded = line.decode("utf-8")
                decoder = json.JSONDecoder()
                event, end_index = decoder.raw_decode(decoded)
                if decoded[end_index:].strip():
                    raise InvalidJsonLineError(
                        "line contains trailing content after one JSON object"
                    )
            except (UnicodeDecodeError, JSONDecodeError, InvalidJsonLineError) as exc:
                report.errors.append(
                    ValidationIssue(
                        line_number=line_number,
                        error_type=InvalidJsonLineError.__name__,
                        message=str(exc),
                    )
                )
                continue

            if not isinstance(event, dict):
                report.errors.append(
                    ValidationIssue(
                        line_number=line_number,
                        error_type=InvalidJsonLineError.__name__,
                        message="line must contain a JSON object",
                    )
                )
                continue

            try:
                validate_event(event)
            except SchemaViolationError as exc:
                report.errors.append(
                    ValidationIssue(
                        line_number=line_number,
                        error_type=SchemaViolationError.__name__,
                        message=str(exc),
                    )
                )
                continue

            events.append((line_number, event))

    return events
