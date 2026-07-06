#!/usr/bin/env python3
"""Validate bounded token-efficiency trace summaries and emit scorecards.

This script is intentionally importer-first. It does not run external runtimes,
call providers, read repositories, or persist anything. It validates a bounded
trace summary and emits a normalized token_efficiency_scorecard.v1 JSON object
that a later storage/API layer can persist through LocalProductStore artifacts.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any


SCHEMA_VERSION = "token_efficiency_scorecard.v1"

ALLOWED_RUNTIME_KINDS = {
    "native_harness",
    "langgraph",
    "crewai",
    "microsoft_agent_framework",
    "other",
}

ALLOWED_MODES = {
    "native_control_plane",
    "stateless_reread",
    "stateful_store",
    "pruned_context",
    "external_runtime",
}

ALLOWED_STATE_STRATEGIES = {
    "none",
    "full_history",
    "durable_state",
    "memory_digest",
    "retrieval_refs",
    "mixed",
}

ALLOWED_STATUS = {"pass", "fail", "error", "blocked"}
ALLOWED_QUALITY_METHODS = {"rule", "test", "human_review", "model_judge", "mixed", "none"}
ALLOWED_REDACTION_STATUS = {"not_needed", "redacted", "rejected"}
ALLOWED_AGENT_ROLES = {"planner", "executor", "reviewer", "evaluator", "unknown"}
ALLOWED_OPERATION_KINDS = {
    "model_call",
    "tool_call",
    "state_read",
    "state_write",
    "retrieval",
    "evaluation",
    "control",
}

RUN_REQUIRED_STRINGS = {
    "adapter_run_id",
    "runtime_kind",
    "runtime_version",
    "scenario_id",
    "mode",
    "state_strategy",
    "status",
    "pass_fail_reason",
    "quality_method",
    "raw_trace_artifact_id",
    "redaction_status",
}

RUN_REQUIRED_NONNEGATIVE_NUMBERS = {
    "input_token_total",
    "output_token_total",
    "context_token_total",
    "repeated_context_token_total",
    "retrieved_ref_token_total",
    "tool_call_count",
    "redundant_tool_call_count",
    "retry_count",
    "step_count",
    "duration_ms",
}

RUN_OPTIONAL_NONNEGATIVE_NUMBERS = {"estimated_cost_usd"}

STEP_REQUIRED_STRINGS = {
    "adapter_step_id",
    "adapter_run_id",
    "node_name",
    "agent_role",
    "operation_kind",
    "status",
    "error_kind",
}

STEP_REQUIRED_NONNEGATIVE_NUMBERS = {
    "step_index",
    "input_tokens",
    "output_tokens",
    "context_tokens",
    "repeated_context_tokens",
    "retrieved_refs_count",
    "retrieved_ref_tokens",
    "state_read_bytes",
    "state_write_bytes",
}

RAW_TRACE_KEY_FRAGMENTS = {
    "raw_trace",
    "raw_prompt",
    "raw_output",
    "transcript",
    "conversation",
    "message_history",
    "credential",
    "repository_text",
    "repo_full_text",
    "repo_content",
    "private_path",
    "secret",
    "password",
}

SECRET_VALUE_PATTERNS = (
    re.compile(r"\bsk-[A-Za-z0-9_-]{16,}\b"),
    re.compile(r"\bgh[pousr]_[A-Za-z0-9_]{20,}\b"),
    re.compile(r"\b(?:api[_-]?key|auth[_-]?token|password)\s*[:=]\s*\S+", re.IGNORECASE),
)
PRIVATE_PATH_PATTERNS = (
    re.compile(r"(^|\s)/(?:home|Users)/[^\s]+"),
    re.compile(r"\b[A-Za-z]:\\Users\\[^\s]+"),
)


class ScorecardError(ValueError):
    """Raised when a trace summary cannot become a valid scorecard."""


def _require_mapping(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ScorecardError(f"{label} must be an object")
    return value


def _require_string(data: dict[str, Any], field: str, label: str) -> str:
    value = data.get(field)
    if not isinstance(value, str) or not value.strip():
        raise ScorecardError(f"{label}.{field} must be a non-empty string")
    return value


def _require_nonnegative_number(data: dict[str, Any], field: str, label: str) -> float | int:
    value = data.get(field)
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ScorecardError(f"{label}.{field} must be a non-negative number")
    if value < 0:
        raise ScorecardError(f"{label}.{field} must be non-negative")
    return value


def _reject_raw_trace_keys(value: Any, path: str = "$") -> None:
    if isinstance(value, dict):
        for key, child in value.items():
            lowered = str(key).lower()
            if lowered != "raw_trace_artifact_id" and any(fragment in lowered for fragment in RAW_TRACE_KEY_FRAGMENTS):
                raise ScorecardError(f"raw or sensitive trace field is not allowed: {path}.{key}")
            _reject_raw_trace_keys(child, f"{path}.{key}")
    elif isinstance(value, list):
        for index, child in enumerate(value):
            _reject_raw_trace_keys(child, f"{path}[{index}]")
    elif isinstance(value, str):
        if any(pattern.search(value) for pattern in SECRET_VALUE_PATTERNS):
            raise ScorecardError(f"secret-shaped trace value is not allowed: {path}")
        if any(pattern.search(value) for pattern in PRIVATE_PATH_PATTERNS):
            raise ScorecardError(f"private path trace value is not allowed: {path}")


def _validate_allowed(value: str, allowed: set[str], field: str) -> None:
    if value not in allowed:
        allowed_values = ", ".join(sorted(allowed))
        raise ScorecardError(f"{field} must be one of: {allowed_values}")


def _ratio(numerator: float | int, denominator: float | int) -> float:
    return round(float(numerator) / float(max(denominator, 1)), 6)


def _derive_metrics(scorecard: dict[str, Any]) -> dict[str, Any]:
    total_tokens = scorecard["input_token_total"] + scorecard["output_token_total"]
    status = scorecard["status"]
    return {
        "total_tokens": total_tokens,
        "context_share": _ratio(scorecard["context_token_total"], total_tokens),
        "repeated_context_ratio": _ratio(
            scorecard["repeated_context_token_total"],
            scorecard["context_token_total"],
        ),
        "tool_redundancy_ratio": _ratio(
            scorecard["redundant_tool_call_count"],
            scorecard["tool_call_count"],
        ),
        "tokens_per_passing_run": total_tokens if status == "pass" else None,
        "cost_per_passing_run": scorecard.get("estimated_cost_usd") if status == "pass" else None,
        "step_retry_ratio": _ratio(scorecard["retry_count"], scorecard["step_count"]),
    }


def _validate_step(step: dict[str, Any], expected_run_id: str, index: int) -> dict[str, Any]:
    label = f"steps[{index}]"
    for field in STEP_REQUIRED_STRINGS:
        _require_string(step, field, label)
    for field in STEP_REQUIRED_NONNEGATIVE_NUMBERS:
        _require_nonnegative_number(step, field, label)

    if step["adapter_run_id"] != expected_run_id:
        raise ScorecardError(f"{label}.adapter_run_id must match adapter_run_id")
    if step["step_index"] != index:
        raise ScorecardError(f"{label}.step_index must equal its zero-based order")
    _validate_allowed(step["agent_role"], ALLOWED_AGENT_ROLES, f"{label}.agent_role")
    _validate_allowed(step["operation_kind"], ALLOWED_OPERATION_KINDS, f"{label}.operation_kind")
    _validate_allowed(step["status"], ALLOWED_STATUS, f"{label}.status")
    if step["repeated_context_tokens"] > step["context_tokens"]:
        raise ScorecardError(f"{label}.repeated_context_tokens cannot exceed context_tokens")
    if step["retrieved_ref_tokens"] > step["context_tokens"]:
        raise ScorecardError(f"{label}.retrieved_ref_tokens cannot exceed context_tokens")

    normalized = {field: step[field] for field in sorted(STEP_REQUIRED_STRINGS | STEP_REQUIRED_NONNEGATIVE_NUMBERS)}
    for optional_field in ("tool_name", "tool_call_id", "started_at", "finished_at"):
        if optional_field in step:
            normalized[optional_field] = step[optional_field]
    return normalized


def import_scorecard(summary: dict[str, Any]) -> dict[str, Any]:
    """Return a normalized scorecard or raise ScorecardError."""

    data = _require_mapping(summary, "summary")
    _reject_raw_trace_keys(data)

    for field in RUN_REQUIRED_STRINGS:
        _require_string(data, field, "summary")
    for field in RUN_REQUIRED_NONNEGATIVE_NUMBERS:
        _require_nonnegative_number(data, field, "summary")
    for field in RUN_OPTIONAL_NONNEGATIVE_NUMBERS:
        if field in data and data[field] is not None:
            _require_nonnegative_number(data, field, "summary")

    _validate_allowed(data["runtime_kind"], ALLOWED_RUNTIME_KINDS, "runtime_kind")
    _validate_allowed(data["mode"], ALLOWED_MODES, "mode")
    _validate_allowed(data["state_strategy"], ALLOWED_STATE_STRATEGIES, "state_strategy")
    _validate_allowed(data["status"], ALLOWED_STATUS, "status")
    _validate_allowed(data["quality_method"], ALLOWED_QUALITY_METHODS, "quality_method")
    _validate_allowed(data["redaction_status"], ALLOWED_REDACTION_STATUS, "redaction_status")

    if data["redundant_tool_call_count"] > data["tool_call_count"]:
        raise ScorecardError("redundant_tool_call_count cannot exceed tool_call_count")
    if data["repeated_context_token_total"] > data["context_token_total"]:
        raise ScorecardError("repeated_context_token_total cannot exceed context_token_total")
    if data["retrieved_ref_token_total"] > data["context_token_total"]:
        raise ScorecardError("retrieved_ref_token_total cannot exceed context_token_total")
    if data["status"] == "pass" and data["quality_method"] == "none":
        raise ScorecardError("passing runs require a non-none quality_method")

    quality_score = data.get("quality_score")
    if quality_score is not None:
        if isinstance(quality_score, bool) or not isinstance(quality_score, (int, float)):
            raise ScorecardError("quality_score must be a number between 0.0 and 1.0")
        if not 0.0 <= quality_score <= 1.0:
            raise ScorecardError("quality_score must be between 0.0 and 1.0")

    normalized = {"schema_version": SCHEMA_VERSION}
    for field in sorted(RUN_REQUIRED_STRINGS | RUN_REQUIRED_NONNEGATIVE_NUMBERS):
        normalized[field] = data[field]
    for optional_field in ("quality_score", "estimated_cost_usd"):
        if optional_field in data:
            normalized[optional_field] = data[optional_field]

    steps = data.get("steps", [])
    if steps is None:
        steps = []
    if not isinstance(steps, list):
        raise ScorecardError("summary.steps must be a list when present")
    if steps and len(steps) != data["step_count"]:
        raise ScorecardError("step_count must match the number of supplied steps")
    normalized_steps = [
        _validate_step(_require_mapping(step, f"steps[{index}]"), data["adapter_run_id"], index)
        for index, step in enumerate(steps)
    ]
    if normalized_steps:
        normalized["steps"] = normalized_steps

    normalized["derived_metrics"] = _derive_metrics(normalized)
    return normalized


def load_json(path: Path) -> dict[str, Any]:
    try:
        return _require_mapping(json.loads(path.read_text(encoding="utf-8")), str(path))
    except OSError as exc:
        raise ScorecardError(f"cannot read {path}: {exc}") from exc
    except json.JSONDecodeError as exc:
        raise ScorecardError(f"invalid JSON in {path}: {exc}") from exc


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Validate a bounded trace summary and emit token_efficiency_scorecard.v1 JSON."
    )
    parser.add_argument("input", type=Path, help="Trace summary JSON file")
    parser.add_argument("--output", type=Path, help="Optional output path; stdout is used by default")
    parser.add_argument("--compact", action="store_true", help="Emit compact JSON")
    args = parser.parse_args(argv)

    try:
        scorecard = import_scorecard(load_json(args.input))
    except ScorecardError as exc:
        print(f"scorecard import failed: {exc}", file=sys.stderr)
        return 1

    rendered = json.dumps(scorecard, sort_keys=True, separators=(",", ":") if args.compact else None, indent=None if args.compact else 2)
    if args.output:
        args.output.write_text(rendered + "\n", encoding="utf-8")
    else:
        print(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
