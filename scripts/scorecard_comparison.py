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

CONTRACT_MATCH_FIELDS = [
    "scenario_digest",
    "task_digest",
    "runtime_kind",
    "runtime_version",
    "provider_id",
    "model_id",
    "tokenizer_id",
    "pricing_id",
    "input_cost_per_1k_usd",
    "output_cost_per_1k_usd",
    "quality_method",
    "quality_threshold",
    "evaluator_version",
    "redaction_policy",
    "retry_policy",
    "seed",
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
        return VALIDATOR.import_scorecard(value)
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


def _contract(scorecard: dict[str, Any]) -> dict[str, Any]:
    return _require_mapping(scorecard.get("comparison_contract"), "scorecard.comparison_contract")


def _require_matching_contracts(baseline: dict[str, Any], candidate: dict[str, Any]) -> dict[str, Any]:
    baseline_contract = _contract(baseline)
    candidate_contract = _contract(candidate)
    for field in CONTRACT_MATCH_FIELDS:
        if baseline_contract.get(field) != candidate_contract.get(field):
            raise ScorecardComparisonError(f"comparison_contract.{field} must match")
    return baseline_contract


def _quality_qualified(scorecard: dict[str, Any], threshold: float) -> bool:
    quality_score = scorecard.get("quality_score")
    return (
        scorecard.get("status") == "pass"
        and isinstance(quality_score, (int, float))
        and not isinstance(quality_score, bool)
        and float(quality_score) >= threshold
    )


def _delta(candidate: Any, baseline: Any) -> float | int | None:
    if not isinstance(candidate, (int, float)) or isinstance(candidate, bool):
        return None
    if not isinstance(baseline, (int, float)) or isinstance(baseline, bool):
        return None
    return round(float(candidate) - float(baseline), 6)


def compare_scorecards(scorecards: list[dict[str, Any]]) -> dict[str, Any]:
    """Return a read-only field comparison across normalized scorecards.

    The baseline and candidate are explicit modes, independent of input order.
    Advantage claims require matching contracts and both scores meeting the
    shared quality threshold.
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

    baselines = [card for card in normalized if card.get("mode") == "stateless_reread"]
    candidates = [card for card in normalized if card.get("mode") == "stateful_store"]
    if len(baselines) != 1 or len(candidates) != 1 or len(normalized) != 2:
        raise ScorecardComparisonError(
            "comparison requires exactly one stateless_reread baseline and one stateful_store candidate"
        )
    baseline = baselines[0]
    candidate = candidates[0]
    contract = _require_matching_contracts(baseline, candidate)
    threshold = float(contract["quality_threshold"])
    baseline_qualified = _quality_qualified(baseline, threshold)
    candidate_qualified = _quality_qualified(candidate, threshold)
    both_qualified = baseline_qualified and candidate_qualified
    baseline_total = _comparison_value(baseline, "total_tokens")
    candidate_total = _comparison_value(candidate, "total_tokens")
    baseline_cost = baseline.get("estimated_cost_usd")
    candidate_cost = candidate.get("estimated_cost_usd")
    token_reduction = _safe_token_reduction_ratio(baseline_total, candidate_total)
    cost_reduction = _delta(baseline_cost, candidate_cost)
    token_advantage = both_qualified and token_reduction is not None and token_reduction > 0
    cost_advantage = both_qualified and cost_reduction is not None and cost_reduction > 0
    rows = []
    for role, card in (("baseline", baseline), ("candidate", candidate)):
        row = {
            "comparison_role": role,
            "adapter_run_id": card["adapter_run_id"],
            "runtime_kind": card["runtime_kind"],
            "runtime_version": card["runtime_version"],
            "scenario_id": card["scenario_id"],
            "mode": card["mode"],
            "state_strategy": card["state_strategy"],
        }
        for field in COMPARISON_FIELDS:
            if field == "token_reduction_ratio":
                row[field] = 0.0 if role == "baseline" else token_reduction if both_qualified else None
            else:
                row[field] = _comparison_value(card, field)
        rows.append(row)

    return {
        "comparison_kind": "token_efficiency_scorecard_read_only_comparison",
        "read_only": True,
        "comparison_basis": "token_efficiency_scorecard.v1 bounded summaries",
        "baseline_adapter_run_id": baseline["adapter_run_id"],
        "baseline_mode": baseline["mode"],
        "candidate_adapter_run_id": candidate["adapter_run_id"],
        "candidate_mode": candidate["mode"],
        "scenario_id": normalized[0]["scenario_id"],
        "comparison_contract": contract,
        "baseline": rows[0],
        "candidate": rows[1],
        "quality_gate": {
            "method": contract["quality_method"],
            "threshold": threshold,
            "baseline_qualified": baseline_qualified,
            "candidate_qualified": candidate_qualified,
            "both_qualified": both_qualified,
        },
        "deltas": {
            "total_tokens": _delta(candidate_total, baseline_total),
            "repeated_context_ratio": _delta(
                _comparison_value(candidate, "repeated_context_ratio"),
                _comparison_value(baseline, "repeated_context_ratio"),
            ),
            "estimated_cost_usd": _delta(candidate_cost, baseline_cost),
            "duration_ms": _delta(candidate.get("duration_ms"), baseline.get("duration_ms")),
            "retry_count": _delta(candidate.get("retry_count"), baseline.get("retry_count")),
            "quality_score": _delta(candidate.get("quality_score"), baseline.get("quality_score")),
        },
        "advantages": {
            "token": {
                "reported": token_advantage,
                "reduction_ratio": token_reduction if token_advantage else None,
            },
            "cost": {
                "reported": cost_advantage,
                "reduction_usd": cost_reduction if cost_advantage else None,
            },
        },
        "fields": COMPARISON_FIELDS,
        "rows": rows,
    }
