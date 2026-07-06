#!/usr/bin/env python3
"""Export native harness summaries as token-efficiency scorecard artifacts.

This is a read-only bridge for bounded native dispatch/workflow/run/evidence
JSON. It does not call providers, execute workflows, read target repositories,
or persist raw traces. The normalized scorecard is always produced by
scripts/token_efficiency_scorecard.py so this script cannot drift into a second
schema.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import sys
from datetime import UTC, datetime
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
VALIDATOR_PATH = ROOT / "scripts" / "token_efficiency_scorecard.py"
VALIDATOR_SPEC = importlib.util.spec_from_file_location("token_efficiency_scorecard", VALIDATOR_PATH)
assert VALIDATOR_SPEC is not None
VALIDATOR = importlib.util.module_from_spec(VALIDATOR_SPEC)
sys.modules[VALIDATOR_SPEC.name] = VALIDATOR
assert VALIDATOR_SPEC.loader is not None
VALIDATOR_SPEC.loader.exec_module(VALIDATOR)


EXPORT_SCHEMA_VERSION = "native_scorecard_artifact.v1"
NATIVE_RUNTIME_KIND = "native_harness"
NATIVE_MODE = "native_control_plane"
NATIVE_STATE_STRATEGY = "mixed"
READ_ONLY_STORAGE = "app_owned_artifact_json_export"


class NativeScorecardExportError(ValueError):
    """Raised when native bounded evidence cannot be exported."""


def _require_mapping(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise NativeScorecardExportError(f"{label} must be an object")
    return value


def _optional_mapping(data: dict[str, Any], field: str) -> dict[str, Any]:
    value = data.get(field)
    if value is None:
        return {}
    return _require_mapping(value, field)


def _optional_list(data: dict[str, Any], field: str) -> list[Any]:
    value = data.get(field)
    if value is None:
        return []
    if not isinstance(value, list):
        raise NativeScorecardExportError(f"{field} must be a list when present")
    return value


def _string_from(*values: Any, default: str | None = None) -> str:
    for value in values:
        if isinstance(value, str) and value.strip():
            return value
    if default is not None:
        return default
    raise NativeScorecardExportError("missing required string")


def _number_from(*values: Any, default: float | int = 0) -> float | int:
    for value in values:
        if isinstance(value, bool):
            continue
        if isinstance(value, (int, float)) and value >= 0:
            return value
    return default


def _status_from(value: Any) -> str:
    status = str(value or "").strip().lower()
    if status in {"completed", "success", "succeeded", "passed", "pass"}:
        return "pass"
    if status in {"failed", "failure", "fail"}:
        return "fail"
    if status in {"error", "errored"}:
        return "error"
    if status in {"blocked", "cancelled", "canceled", "timeout", "timed_out"}:
        return "blocked"
    return "blocked"


def _quality_method_from(value: Any, status: str) -> str:
    method = str(value or "").strip().lower()
    if method in VALIDATOR.ALLOWED_QUALITY_METHODS:
        return method
    return "test" if status == "pass" else "none"


def _role_from(value: Any) -> str:
    role = str(value or "").strip().lower()
    return role if role in VALIDATOR.ALLOWED_AGENT_ROLES else "unknown"


def _operation_kind_from(step: dict[str, Any]) -> str:
    value = str(step.get("operation_kind") or step.get("kind") or "").strip().lower()
    if value in VALIDATOR.ALLOWED_OPERATION_KINDS:
        return value
    if step.get("tool_name") or step.get("tool_call_id"):
        return "tool_call"
    if step.get("state_read_bytes") or step.get("state_write_bytes"):
        return "state_read"
    return "control"


def _derive_raw_trace_artifact_id(native: dict[str, Any], evidence: dict[str, Any], run_id: str) -> str:
    artifact_id = evidence.get("raw_trace_artifact_id") or evidence.get("artifact_id")
    if isinstance(artifact_id, str) and artifact_id.strip():
        return artifact_id
    digest = hashlib.sha256(json.dumps(native, sort_keys=True, separators=(",", ":")).encode("utf-8")).hexdigest()
    return f"native-scorecard-source-{run_id}-{digest[:12]}"


def _normalize_step(step: dict[str, Any], run_id: str, index: int) -> dict[str, Any]:
    status = _status_from(step.get("status"))
    context_tokens = _number_from(step.get("context_tokens"), step.get("context_token_total"))
    return {
        "adapter_step_id": _string_from(step.get("adapter_step_id"), step.get("step_id"), step.get("node_id"), default=f"{run_id}-step-{index}"),
        "adapter_run_id": run_id,
        "step_index": index,
        "node_name": _string_from(step.get("node_name"), step.get("name"), step.get("node_id"), default=f"step-{index}"),
        "agent_role": _role_from(step.get("agent_role") or step.get("role")),
        "operation_kind": _operation_kind_from(step),
        "input_tokens": _number_from(step.get("input_tokens"), step.get("input_token_total")),
        "output_tokens": _number_from(step.get("output_tokens"), step.get("output_token_total")),
        "context_tokens": context_tokens,
        "repeated_context_tokens": _number_from(step.get("repeated_context_tokens"), step.get("repeated_context_token_total")),
        "retrieved_refs_count": _number_from(step.get("retrieved_refs_count"), step.get("retrieved_ref_count")),
        "retrieved_ref_tokens": _number_from(step.get("retrieved_ref_tokens"), step.get("retrieved_ref_token_total")),
        "tool_name": step.get("tool_name"),
        "tool_call_id": step.get("tool_call_id"),
        "status": status,
        "error_kind": _string_from(step.get("error_kind"), default="none" if status == "pass" else status),
        "state_read_bytes": _number_from(step.get("state_read_bytes")),
        "state_write_bytes": _number_from(step.get("state_write_bytes")),
    }


def native_summary_to_trace_summary(native_summary: dict[str, Any]) -> dict[str, Any]:
    """Convert bounded native evidence into the validator's trace summary shape."""

    native = _require_mapping(native_summary, "native_summary")
    VALIDATOR._reject_raw_trace_keys(native)
    dispatch = _optional_mapping(native, "dispatch")
    workflow_run = _optional_mapping(native, "workflow_run")
    run = _optional_mapping(native, "run")
    evidence = _optional_mapping(native, "evidence")
    metrics = _optional_mapping(native, "metrics")
    run_id = _string_from(
        native.get("adapter_run_id"),
        run.get("adapter_run_id"),
        run.get("run_id"),
        workflow_run.get("run_id"),
        dispatch.get("dispatch_id"),
    )
    steps = [
        _normalize_step(_require_mapping(step, f"steps[{index}]"), run_id, index)
        for index, step in enumerate(_optional_list(native, "steps"))
    ]
    status = _status_from(native.get("status") or run.get("status") or workflow_run.get("status") or evidence.get("status"))
    pass_fail_reason = _string_from(
        native.get("pass_fail_reason"),
        evidence.get("pass_fail_reason"),
        evidence.get("reason"),
        run.get("reason"),
        workflow_run.get("status_reason"),
        default="bounded native summary exported",
    )
    input_tokens = _number_from(metrics.get("input_token_total"), native.get("input_token_total"), dispatch.get("input_token_total"))
    output_tokens = _number_from(metrics.get("output_token_total"), native.get("output_token_total"), dispatch.get("output_token_total"))
    context_tokens = _number_from(metrics.get("context_token_total"), native.get("context_token_total"), dispatch.get("context_token_total"))

    if steps:
        input_tokens = input_tokens or sum(step["input_tokens"] for step in steps)
        output_tokens = output_tokens or sum(step["output_tokens"] for step in steps)
        context_tokens = context_tokens or sum(step["context_tokens"] for step in steps)

    return {
        "adapter_run_id": run_id,
        "runtime_kind": NATIVE_RUNTIME_KIND,
        "runtime_version": _string_from(native.get("runtime_version"), dispatch.get("runtime_version"), default="native-harness"),
        "scenario_id": _string_from(native.get("scenario_id"), run.get("scenario_id"), workflow_run.get("scenario_id"), dispatch.get("scenario_id"), default="native-run"),
        "mode": _string_from(native.get("mode"), default=NATIVE_MODE),
        "state_strategy": _string_from(native.get("state_strategy"), default=NATIVE_STATE_STRATEGY),
        "status": status,
        "pass_fail_reason": pass_fail_reason,
        "quality_score": native.get("quality_score", evidence.get("quality_score")),
        "quality_method": _quality_method_from(native.get("quality_method") or evidence.get("quality_method"), status),
        "input_token_total": input_tokens,
        "output_token_total": output_tokens,
        "context_token_total": context_tokens,
        "repeated_context_token_total": _number_from(metrics.get("repeated_context_token_total"), native.get("repeated_context_token_total")),
        "retrieved_ref_token_total": _number_from(metrics.get("retrieved_ref_token_total"), native.get("retrieved_ref_token_total")),
        "tool_call_count": _number_from(metrics.get("tool_call_count"), native.get("tool_call_count")),
        "redundant_tool_call_count": _number_from(metrics.get("redundant_tool_call_count"), native.get("redundant_tool_call_count")),
        "retry_count": _number_from(metrics.get("retry_count"), native.get("retry_count"), workflow_run.get("retry_count")),
        "step_count": _number_from(native.get("step_count"), len(steps), default=len(steps)),
        "duration_ms": _number_from(metrics.get("duration_ms"), native.get("duration_ms"), run.get("duration_ms"), workflow_run.get("duration_ms")),
        "estimated_cost_usd": native.get("estimated_cost_usd", metrics.get("estimated_cost_usd")),
        "raw_trace_artifact_id": _derive_raw_trace_artifact_id(native, evidence, run_id),
        "redaction_status": _string_from(native.get("redaction_status"), evidence.get("redaction_status"), default="redacted"),
        "steps": steps,
    }


def build_artifact(scorecard: dict[str, Any]) -> dict[str, Any]:
    payload_hash = hashlib.sha256(json.dumps(scorecard, sort_keys=True, separators=(",", ":")).encode("utf-8")).hexdigest()
    return {
        "schema_version": EXPORT_SCHEMA_VERSION,
        "artifact_kind": "token_efficiency_scorecard",
        "storage": READ_ONLY_STORAGE,
        "read_only": True,
        "created_at": datetime.now(UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "artifact_id": f"scorecard-{scorecard['adapter_run_id']}-{payload_hash[:12]}",
        "content_sha256": payload_hash,
        "scorecard_schema_version": scorecard["schema_version"],
        "scorecard": scorecard,
        "next_storage_integration": "Persist this envelope through LocalProductStore app-owned artifacts and expose read-only scorecard reads; do not add a second storage layer.",
    }


def load_json(path: Path) -> dict[str, Any]:
    try:
        return _require_mapping(json.loads(path.read_text(encoding="utf-8")), str(path))
    except OSError as exc:
        raise NativeScorecardExportError(f"cannot read {path}: {exc}") from exc
    except json.JSONDecodeError as exc:
        raise NativeScorecardExportError(f"invalid JSON in {path}: {exc}") from exc


def render_json(value: dict[str, Any], compact: bool) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":") if compact else None, indent=None if compact else 2)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Export bounded native harness evidence as a validated token-efficiency scorecard artifact."
    )
    parser.add_argument("input", type=Path, help="Bounded native dispatch/workflow/run/evidence JSON")
    parser.add_argument("--output", type=Path, help="Write artifact JSON to this path; stdout is used by default")
    parser.add_argument("--scorecard-only", action="store_true", help="Emit only token_efficiency_scorecard.v1 JSON")
    parser.add_argument("--compact", action="store_true", help="Emit compact JSON")
    args = parser.parse_args(argv)

    try:
        trace_summary = native_summary_to_trace_summary(load_json(args.input))
        scorecard = VALIDATOR.import_scorecard(trace_summary)
        output = scorecard if args.scorecard_only else build_artifact(scorecard)
    except (NativeScorecardExportError, VALIDATOR.ScorecardError) as exc:
        print(f"native scorecard export failed: {exc}", file=sys.stderr)
        return 1

    rendered = render_json(output, args.compact)
    if args.output:
        args.output.write_text(rendered + "\n", encoding="utf-8")
    else:
        print(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
