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

COMPARISON_PATH = ROOT / "scripts" / "scorecard_comparison.py"
COMPARISON_SPEC = importlib.util.spec_from_file_location("scorecard_comparison", COMPARISON_PATH)
assert COMPARISON_SPEC is not None
COMPARISON = importlib.util.module_from_spec(COMPARISON_SPEC)
sys.modules[COMPARISON_SPEC.name] = COMPARISON
assert COMPARISON_SPEC.loader is not None
COMPARISON_SPEC.loader.exec_module(COMPARISON)


RUNTIME_KIND = "langgraph"
ALLOWED_MODES = {"stateless_reread", "stateful_store"}
COMPARISON_FIELDS = COMPARISON.COMPARISON_FIELDS


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


def build_langgraph_artifact(scorecard: dict[str, Any]) -> dict[str, Any]:
    """Build the shared v2 artifact envelope without relabeling runtime identity."""

    normalized = import_langgraph_scorecard(scorecard)
    return VALIDATOR.build_scorecard_artifact(normalized)


def compare_scorecards(scorecards: list[dict[str, Any]]) -> dict[str, Any]:
    """Return a read-only field comparison across normalized scorecards."""
    try:
        normalized = [
            import_langgraph_scorecard(_require_mapping(scorecard, f"scorecards[{index}]"))
            for index, scorecard in enumerate(scorecards)
        ]
        return COMPARISON.compare_scorecards(normalized)
    except COMPARISON.ScorecardComparisonError as exc:
        raise LangGraphTraceImportError(str(exc)) from exc


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Import bounded LangGraph summaries or compare token-efficiency scorecards."
    )
    parser.add_argument("inputs", nargs="+", type=Path, help="Bounded summary/scorecard JSON input(s)")
    parser.add_argument("--compare", action="store_true", help="Emit read-only comparison rows for two or more inputs")
    parser.add_argument("--artifact", action="store_true", help="Emit a runtime-neutral scorecard_artifact.v2 envelope")
    parser.add_argument("--output", type=Path, help="Write JSON to this path; stdout is used by default")
    parser.add_argument("--compact", action="store_true", help="Emit compact JSON")
    args = parser.parse_args(argv)

    try:
        loaded = [_load_json(path) for path in args.inputs]
        if args.compare and args.artifact:
            raise LangGraphTraceImportError("--compare and --artifact cannot be combined")
        scorecard = None if args.compare else import_langgraph_scorecard(loaded[0])
        output = compare_scorecards(loaded) if args.compare else build_langgraph_artifact(scorecard) if args.artifact else scorecard
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
