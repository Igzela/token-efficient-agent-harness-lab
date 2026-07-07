#!/usr/bin/env python3
"""Read-only comparison helpers for token-efficiency scorecards.

The helpers in this module compare already-bounded scorecard summaries. They do
not call providers, run runtimes, read repositories, or persist artifacts.
"""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
VALIDATOR_PATH = ROOT / "scripts" / "token_efficiency_scorecard.py"
VALIDATOR_SPEC = importlib.util.spec_from_file_location("token_efficiency_scorecard", VALIDATOR_PATH)
assert VALIDATOR_SPEC is not None
VALIDATOR = importlib.util.module_from_spec(VALIDATOR_SPEC)
sys.modules.setdefault(VALIDATOR_SPEC.name, VALIDATOR)
assert VALIDATOR_SPEC.loader is not None
VALIDATOR_SPEC.loader.exec_module(VALIDATOR)

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
    "token_reduction_ratio",
]


class ScorecardComparisonError(ValueError):
    """Raised when scorecards cannot be compared safely."""


def _require_mapping(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ScorecardComparisonError(f"{label} must be an object")
    return value


def _normalize_scorecard(value: dict[str, Any]) -> dict[str, Any]:
    try:
        VALIDATOR._reject_raw_trace_keys(value)
        if value.get("schema_version") != VALIDATOR.SCHEMA_VERSION or "derived_metrics" not in value:
            return VALIDATOR.import_scorecard(value)
        return value
    except VALIDATOR.ScorecardError as exc:
        raise ScorecardComparisonError(str(exc)) from exc


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


def _safe_token_reduction_ratio(baseline_total: Any, total_tokens: Any) -> float | None:
    if not isinstance(baseline_total, (int, float)) or baseline_total <= 0:
        return None
    if not isinstance(total_tokens, (int, float)) or total_tokens < 0:
        return None
    return round((float(baseline_total) - float(total_tokens)) / float(baseline_total), 6)


def compare_scorecards(scorecards: list[dict[str, Any]]) -> dict[str, Any]:
    """Return a read-only field comparison across normalized scorecards.

    The first supplied scorecard is the token-reduction baseline. The function
    requires every compared scorecard to share the same scenario_id so token
    reduction is never reported across unrelated tasks.
    """

    if len(scorecards) < 2:
        raise ScorecardComparisonError("comparison requires at least two scorecards")

    normalized = [
        _normalize_scorecard(_require_mapping(scorecard, f"scorecards[{index}]"))
        for index, scorecard in enumerate(scorecards)
    ]
    scenario_ids = {card.get("scenario_id") for card in normalized}
    if len(scenario_ids) != 1:
        raise ScorecardComparisonError("all compared scorecards must share scenario_id")

    baseline = normalized[0]
    baseline_total = _comparison_value(baseline, "total_tokens")
    rows = []
    for card in normalized:
        row = {
            "adapter_run_id": card["adapter_run_id"],
            "runtime_kind": card["runtime_kind"],
            "runtime_version": card["runtime_version"],
            "scenario_id": card["scenario_id"],
            "mode": card["mode"],
            "state_strategy": card["state_strategy"],
        }
        for field in COMPARISON_FIELDS:
            if field == "token_reduction_ratio":
                row[field] = _safe_token_reduction_ratio(
                    baseline_total,
                    _comparison_value(card, "total_tokens"),
                )
            else:
                row[field] = _comparison_value(card, field)
        rows.append(row)

    return {
        "comparison_kind": "token_efficiency_scorecard_read_only_comparison",
        "read_only": True,
        "comparison_basis": "token_efficiency_scorecard.v1 bounded summaries",
        "baseline_adapter_run_id": baseline["adapter_run_id"],
        "baseline_mode": baseline["mode"],
        "scenario_id": normalized[0]["scenario_id"],
        "fields": COMPARISON_FIELDS,
        "rows": rows,
    }
