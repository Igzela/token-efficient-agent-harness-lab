"""Minimal Validator Suite for Stage 1 Day 5."""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from .event_schema import validate_event
from .event_store import replay_preflight
from .project_board import check_allowed_files

CANONICAL_FAILURE_CODES = {
    "F001_TIMEOUT",
    "F002_BUDGET_EXCEEDED",
    "F003_DEPENDENCY_FAILED",
    "F004_APPROVAL_REJECTED",
    "F005_PROVIDER_UNAVAILABLE",
    "F006_SCOPE_VIOLATION",
    "F007_TEST_FAILURE",
    "F008_FORMAT_ERROR",
    "F009_POLICY_VIOLATION",
    "F010_CANCELLED",
}


@dataclass(frozen=True)
class ValidationResult:
    ok: bool
    errors: tuple[str, ...] = field(default_factory=tuple)
    warnings: tuple[str, ...] = field(default_factory=tuple)


def validate_events_schema(event: dict[str, Any]) -> ValidationResult:
    try:
        validate_event(event)
    except Exception as exc:
        return ValidationResult(ok=False, errors=(str(exc),))
    return ValidationResult(ok=True)


def validate_completion_record(record: dict[str, Any]) -> ValidationResult:
    errors: list[str] = []
    _require_false_template(record, errors)
    for field_name in ("status", "exit_code", "artifact_refs"):
        if field_name not in record:
            errors.append(f"missing required field: {field_name}")
    if record.get("status") not in {"completed", "failed"}:
        errors.append("status must be completed or failed")
    if "exit_code" in record and not isinstance(record["exit_code"], int):
        errors.append("exit_code must be an integer")
    if "artifact_refs" in record and not isinstance(record["artifact_refs"], list):
        errors.append("artifact_refs must be a list")
    return _result(errors)


def validate_handoff_pack(pack: dict[str, Any]) -> ValidationResult:
    errors: list[str] = []
    _require_false_template(pack, errors)
    for field_name in ("structured_fields", "summary", "evidence_refs"):
        if field_name not in pack:
            errors.append(f"missing required field: {field_name}")
    if not isinstance(pack.get("structured_fields"), dict):
        errors.append("structured_fields must be an object")
    if not pack.get("summary"):
        errors.append("summary must be non-empty")
    if not isinstance(pack.get("evidence_refs"), list) or not pack.get("evidence_refs"):
        errors.append("evidence_refs must be a non-empty list")
    return _result(errors)


def validate_approval_request(request: dict[str, Any]) -> ValidationResult:
    errors: list[str] = []
    required = (
        "approval_id",
        "task_id",
        "risk_level",
        "requested_action",
        "summary",
        "reason",
        "affected_files",
        "options",
        "timeout_policy",
        "decision",
    )
    for field_name in required:
        if field_name not in request:
            errors.append(f"missing required field: {field_name}")
    if "options" in request and not isinstance(request["options"], list):
        errors.append("options must be a list")
    if request.get("decision") not in {"pending", "approved", "rejected", "deferred"}:
        errors.append("decision must be pending, approved, rejected, or deferred")
    return _result(errors)


def validate_advisor_protocol_events(
    events_path: str | Path, expected_min_advisor_calls: int
) -> ValidationResult:
    errors: list[str] = []
    response_count = 0
    with Path(events_path).open("r", encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, start=1):
            event = json.loads(line)
            if event.get("event_type") != "advisor_response_received":
                continue
            response_count += 1
            payload = event.get("payload", {})
            for field_name in ("diagnosis", "recommended_action", "do_not_do", "confidence"):
                if field_name not in payload:
                    errors.append(
                        f"advisor response on line {line_number} missing {field_name}"
                    )

    if response_count < expected_min_advisor_calls:
        errors.append(
            f"expected at least {expected_min_advisor_calls} advisor response events, "
            f"found {response_count}"
        )
    return _result(errors)


def validate_failure_code(failure_code: str, failure_subcode: str | None = None) -> ValidationResult:
    if failure_code not in CANONICAL_FAILURE_CODES:
        return ValidationResult(ok=False, errors=(f"non-canonical failure_code: {failure_code}",))
    return ValidationResult(ok=True)


def validate_allowed_files_completeness(
    allowed_files: list[str] | tuple[str, ...], required_files: list[str] | tuple[str, ...]
) -> ValidationResult:
    check = check_allowed_files(allowed_files, required_files)
    if check.ok:
        return ValidationResult(ok=True)
    return ValidationResult(
        ok=False,
        errors=tuple(f"missing allowed file: {file_path}" for file_path in check.missing_files),
    )


def validate_replay_preflight_check(path: str | Path) -> ValidationResult:
    report = replay_preflight(path)
    if report.ok:
        return ValidationResult(
            ok=True,
            warnings=tuple(issue.message for issue in report.warnings),
        )
    return ValidationResult(
        ok=False,
        errors=tuple(issue.message for issue in report.errors),
        warnings=tuple(issue.message for issue in report.warnings),
    )


def _require_false_template(record: dict[str, Any], errors: list[str]) -> None:
    if record.get("_template") is not False:
        errors.append("_template must be false")


def _result(errors: list[str]) -> ValidationResult:
    return ValidationResult(ok=not errors, errors=tuple(errors))
