#!/usr/bin/env python3
"""Import bounded LangGraph trace summaries as token-efficiency scorecards.

This is importer-only: it does not import LangGraph, run a graph, call providers,
read repositories, or persist artifacts. Inputs must already be bounded,
redacted, summary-level JSON objects. The output is the existing
`token_efficiency_scorecard.v1` shape from `scripts/token_efficiency_scorecard.py`.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
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


RUNTIME_KIND = "langgraph"
ALLOWED_MODES = {"stateless_reread", "stateful_store"}
COMPARISON_FIELDS = [
    "total_tokens",
    "context_share",
    "repeated_context_ratio",
    "tool_call_count",
    "retry_count",
    "duration_ms",
    "status",
    "quality_method",
    "tokens_per_passing_run",
    "cost_per_passing_run",
]


class LangGraphTraceImportError(ValueError):
    """Raised when a bounded LangGraph summary cannot be imported."""


def _require_mapping(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise LangGraphTraceImportError(f"{label} must be an object")
    return value


def _load_json(path: Path) -> dict[str, Any]:
    try:
        return _require_mapping(json.loads(path.read_text(encoding="utf-8")), str(path))
    except OSError as exc:
        raise LangGraphTraceImportError(f"cannot read {path}: {exc}") from exc
    except json.JSONDecodeError as exc:
        raise LangGraphTraceImportError(f"invalid JSON in {path}: {exc}") from exc


def _render_json(value: Any, compact: bool) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":") if compact else None, indent=None if compact else 2)


def import_langgraph_scorecard(summary: dict[str, Any]) -> dict[str, Any]:
    """Validate a bounded LangGraph summary and return a normalized scorecard."""

    bounded = _require_mapping(summary, "langgraph_summary")

    schema_version = bounded.get("schema_version")
    if schema_version is not None and schema_version != VALIDATOR.SCHEMA_VERSION:
        raise LangGraphTraceImportError("schema_version must be token_efficiency_scorecard.v1 when present")
    if bounded.get("runtime_kind") != RUNTIME_KIND:
        raise LangGraphTraceImportError("runtime_kind must be langgraph")
    if bounded.get("mode") not in ALLOWED_MODES:
        raise LangGraphTraceImportError("mode must be stateless_reread or stateful_store")

    try:
        VALIDATOR._reject_raw_trace_keys(bounded)
        return VALIDATOR.import_scorecard(bounded)
    except VALIDATOR.ScorecardError as exc:
        raise LangGraphTraceImportError(str(exc)) from exc


def _comparison_value(scorecard: dict[str, Any], field: str) -> Any:
    derived = _require_mapping(scorecard.get("derived_metrics", {}), "scorecard.derived_metrics")
    if field == "total_tokens":
        return derived.get("total_tokens")
    if field == "context_share":
        return derived.get("context_share")
    if field == "repeated_context_ratio":
        return derived.get("repeated_context_ratio")
    if field == "tokens_per_passing_run":
        return derived.get("tokens_per_passing_run")
    if field == "cost_per_passing_run":
        return derived.get("cost_per_passing_run")
    return scorecard.get(field)


def compare_scorecards(scorecards: list[dict[str, Any]]) -> dict[str, Any]:
    """Return a read-only field comparison across normalized scorecards."""

    if len(scorecards) < 2:
        raise LangGraphTraceImportError("comparison requires at least two scorecards")

    normalized = []
    for index, scorecard in enumerate(scorecards):
        card = _require_mapping(scorecard, f"scorecards[{index}]")
        if card.get("schema_version") != VALIDATOR.SCHEMA_VERSION:
            card = import_langgraph_scorecard(card)
        VALIDATOR._reject_raw_trace_keys(card)
        normalized.append(card)

    scenario_ids = {card.get("scenario_id") for card in normalized}
    if len(scenario_ids) != 1:
        raise LangGraphTraceImportError("all compared scorecards must share scenario_id")

    rows = []
    for card in normalized:
        row = {
            "adapter_run_id": card["adapter_run_id"],
            "runtime_kind": card["runtime_kind"],
            "runtime_version": card["runtime_version"],
            "scenario_id": card["scenario_id"],
            "mode": card["mode"],
        }
        for field in COMPARISON_FIELDS:
            row[field] = _comparison_value(card, field)
        rows.append(row)

    return {
        "comparison_kind": "token_efficiency_scorecard_read_only_comparison",
        "read_only": True,
        "comparison_basis": "token_efficiency_scorecard.v1 bounded summaries",
        "scenario_id": normalized[0]["scenario_id"],
        "fields": COMPARISON_FIELDS,
        "rows": rows,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Import bounded LangGraph summaries or compare token-efficiency scorecards."
    )
    parser.add_argument("inputs", nargs="+", type=Path, help="Bounded summary/scorecard JSON input(s)")
    parser.add_argument("--compare", action="store_true", help="Emit read-only comparison rows for two or more inputs")
    parser.add_argument("--output", type=Path, help="Write JSON to this path; stdout is used by default")
    parser.add_argument("--compact", action="store_true", help="Emit compact JSON")
    args = parser.parse_args(argv)

    try:
        loaded = [_load_json(path) for path in args.inputs]
        output = compare_scorecards(loaded) if args.compare else import_langgraph_scorecard(loaded[0])
        if not args.compare and len(loaded) != 1:
            raise LangGraphTraceImportError("multiple inputs require --compare")
    except LangGraphTraceImportError as exc:
        print(f"langgraph trace import failed: {exc}", file=sys.stderr)
        return 1

    rendered = _render_json(output, args.compact)
    if args.output:
        args.output.write_text(rendered + "\n", encoding="utf-8")
    else:
        print(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
