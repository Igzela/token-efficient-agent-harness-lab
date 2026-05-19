"""Usage Ledger and Cost-of-Pass schema and validation helpers.

Records per-eval-row token/cost/retry/tool-call data and aggregates
by cost_of_pass_group for cost-efficiency comparison.
"""

from __future__ import annotations

import json
import re
from collections import defaultdict
from dataclasses import dataclass, asdict
from pathlib import Path
from typing import Any, Dict, List, Optional, Sequence, Tuple


# ---------------------------------------------------------------------------
# Schema version
# ---------------------------------------------------------------------------

SCHEMA_VERSION = "usage_ledger.v1"

REQUIRED_FIELDS: Sequence[str] = (
    "schema_version",
    "run_id",
    "case_id",
    "input_tokens",
    "output_tokens",
    "cached_tokens",
    "request_count",
    "tool_call_count",
    "retry_count",
    "wall_clock_ms",
    "estimated_cost",
    "pass",
    "cost_of_pass_group",
    "model_profile_id",
    "context_pack_id",
)

# cost_of_pass_group format: <eval_suite>/<task_family>/<variant_family>/<success_criterion>
COST_OF_PASS_GROUP_PATTERN = re.compile(
    r"^[a-zA-Z0-9_\-]+/[a-zA-Z0-9_\-]+/[a-zA-Z0-9_\-]+/[a-zA-Z0-9_\-]+$"
)


# ---------------------------------------------------------------------------
# Data class
# ---------------------------------------------------------------------------

@dataclass
class UsageLedgerRow:
    """A single usage ledger record."""

    run_id: str
    case_id: str
    input_tokens: int
    output_tokens: int
    cached_tokens: int
    request_count: int
    tool_call_count: int
    retry_count: int
    wall_clock_ms: int
    estimated_cost: float
    pass_: bool
    cost_of_pass_group: str
    model_profile_id: str
    context_pack_id: str
    schema_version: str = SCHEMA_VERSION

    def to_dict(self) -> Dict[str, Any]:
        d = asdict(self)
        d["pass"] = d.pop("pass_")
        return d

    def to_json(self, **kw: Any) -> str:
        return json.dumps(self.to_dict(), **kw)


@dataclass
class CostOfPassAggregate:
    """Aggregated cost-of-pass for a single cost_of_pass_group."""

    cost_of_pass_group: str
    total_estimated_cost: float
    success_count: int
    failure_count: int
    total_count: int
    cost_of_pass: Optional[float]  # None when success_count == 0

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)


@dataclass
class ComparisonResult:
    """Result of comparing two cost-of-pass groups."""

    group_a: str
    group_b: str
    aggregate_a: CostOfPassAggregate
    aggregate_b: CostOfPassAggregate
    valid: bool
    reason: str
    cost_delta: Optional[float] = None
    relative_change_pct: Optional[float] = None

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)


# ---------------------------------------------------------------------------
# Validation
# ---------------------------------------------------------------------------

def validate_usage_ledger_row(data: Dict[str, Any]) -> List[str]:
    """Validate a usage_ledger row dict. Returns list of violations."""
    violations: List[str] = []

    # 1. Required fields
    for f in REQUIRED_FIELDS:
        if f not in data:
            violations.append(f"missing required field: {f}")
    if violations:
        return violations

    # 2. schema_version
    if data["schema_version"] != SCHEMA_VERSION:
        violations.append(f"schema_version must be {SCHEMA_VERSION}, got {data['schema_version']!r}")

    # 3. pass must be bool
    if not isinstance(data["pass"], bool):
        violations.append(f"pass must be a bool, got {type(data['pass']).__name__}")

    # 4. Non-negative integer fields
    int_fields = ("input_tokens", "output_tokens", "cached_tokens",
                  "request_count", "tool_call_count", "retry_count", "wall_clock_ms")
    for f in int_fields:
        if not isinstance(data[f], int) or data[f] < 0:
            violations.append(f"{f} must be a non-negative integer, got {data[f]!r}")

    # 5. estimated_cost non-negative
    if not isinstance(data["estimated_cost"], (int, float)) or data["estimated_cost"] < 0:
        violations.append(f"estimated_cost must be a non-negative number, got {data['estimated_cost']!r}")

    # 6. cached_tokens <= input_tokens
    if isinstance(data["cached_tokens"], int) and isinstance(data["input_tokens"], int):
        if data["cached_tokens"] > data["input_tokens"]:
            violations.append(
                f"cached_tokens ({data['cached_tokens']}) must not exceed "
                f"input_tokens ({data['input_tokens']})"
            )

    # 7. cost_of_pass_group format
    if not COST_OF_PASS_GROUP_PATTERN.match(data["cost_of_pass_group"]):
        violations.append(
            f"cost_of_pass_group {data['cost_of_pass_group']!r} does not match "
            "format <eval_suite>/<task_family>/<variant_family>/<success_criterion>"
        )

    # 8. model_profile_id and context_pack_id: allow empty string or null
    for f in ("model_profile_id", "context_pack_id"):
        val = data[f]
        if val is not None and val != "" and not isinstance(val, str):
            violations.append(f"{f} must be a string, null, or empty string")

    return violations


def is_valid_cost_of_pass_group(group: str) -> bool:
    """Check if a cost_of_pass_group matches the required four-segment format."""
    return bool(COST_OF_PASS_GROUP_PATTERN.match(group))


def parse_cost_of_pass_group(group: str) -> Tuple[str, str, str, str]:
    """Parse a cost_of_pass_group into its four segments.

    Returns (eval_suite, task_family, variant_family, success_criterion).
    Raises ValueError if format is invalid.
    """
    if not COST_OF_PASS_GROUP_PATTERN.match(group):
        raise ValueError(
            f"cost_of_pass_group {group!r} does not match "
            "<eval_suite>/<task_family>/<variant_family>/<success_criterion>"
        )
    parts = group.split("/")
    return parts[0], parts[1], parts[2], parts[3]


# ---------------------------------------------------------------------------
# Aggregation
# ---------------------------------------------------------------------------

def aggregate_cost_of_pass(rows: List[Dict[str, Any]]) -> CostOfPassAggregate:
    """Aggregate usage ledger rows into a single cost-of-pass metric.

    cost_of_pass = total_estimated_cost / success_count
    If success_count == 0, cost_of_pass is None and this is a failure.
    """
    if not rows:
        return CostOfPassAggregate(
            cost_of_pass_group="",
            total_estimated_cost=0.0,
            success_count=0,
            failure_count=0,
            total_count=0,
            cost_of_pass=None,
        )

    group = rows[0].get("cost_of_pass_group", "")
    total_cost = 0.0
    success = 0
    failure = 0

    for row in rows:
        total_cost += row.get("estimated_cost", 0)
        if row.get("pass") is True:
            success += 1
        else:
            failure += 1

    total = success + failure
    cop = total_cost / success if success > 0 else None

    return CostOfPassAggregate(
        cost_of_pass_group=group,
        total_estimated_cost=total_cost,
        success_count=success,
        failure_count=failure,
        total_count=total,
        cost_of_pass=cop,
    )


def group_usage_rows(rows: List[Dict[str, Any]]) -> Dict[str, List[Dict[str, Any]]]:
    """Group usage ledger rows by cost_of_pass_group."""
    groups: Dict[str, List[Dict[str, Any]]] = defaultdict(list)
    for row in rows:
        groups[row["cost_of_pass_group"]].append(row)
    return dict(groups)


def compare_cost_groups(
    before_rows: List[Dict[str, Any]],
    after_rows: List[Dict[str, Any]],
) -> ComparisonResult:
    """Compare cost-of-pass between two sets of rows (before/after).

    Both must be in the same cost_of_pass_group for a valid comparison.
    """
    before_agg = aggregate_cost_of_pass(before_rows)
    after_agg = aggregate_cost_of_pass(after_rows)

    # Check same group
    if before_agg.cost_of_pass_group != after_agg.cost_of_pass_group:
        return ComparisonResult(
            group_a=before_agg.cost_of_pass_group,
            group_b=after_agg.cost_of_pass_group,
            aggregate_a=before_agg,
            aggregate_b=after_agg,
            valid=False,
            reason="cannot compare different cost_of_pass_groups directly",
        )

    # Check both have defined cost_of_pass
    if before_agg.cost_of_pass is None or after_agg.cost_of_pass is None:
        return ComparisonResult(
            group_a=before_agg.cost_of_pass_group,
            group_b=after_agg.cost_of_pass_group,
            aggregate_a=before_agg,
            aggregate_b=after_agg,
            valid=False,
            reason="cost_of_pass undefined for one or both groups (success_count=0)",
        )

    delta = after_agg.cost_of_pass - before_agg.cost_of_pass
    pct = (delta / before_agg.cost_of_pass * 100) if before_agg.cost_of_pass != 0 else None

    return ComparisonResult(
        group_a=before_agg.cost_of_pass_group,
        group_b=after_agg.cost_of_pass_group,
        aggregate_a=before_agg,
        aggregate_b=after_agg,
        valid=True,
        reason="same group, both have defined cost_of_pass",
        cost_delta=delta,
        relative_change_pct=pct,
    )


def detect_invalid_cost_comparison(
    group_a_rows: List[Dict[str, Any]],
    group_b_rows: List[Dict[str, Any]],
) -> Tuple[bool, str]:
    """Detect if comparing two groups would be an invalid cost comparison.

    Returns (is_invalid, reason).
    """
    agg_a = aggregate_cost_of_pass(group_a_rows)
    agg_b = aggregate_cost_of_pass(group_b_rows)

    if agg_a.cost_of_pass_group != agg_b.cost_of_pass_group:
        return True, (
            f"groups {agg_a.cost_of_pass_group!r} and {agg_b.cost_of_pass_group!r} "
            "are different; direct cost comparison is invalid"
        )

    if agg_a.cost_of_pass is None or agg_b.cost_of_pass is None:
        return True, "one or both groups have undefined cost_of_pass (success_count=0)"

    return False, "comparison is valid"


# ---------------------------------------------------------------------------
# Fixture loading
# ---------------------------------------------------------------------------

def load_fixture(path: Path) -> Dict[str, Any]:
    with open(path, "r") as f:
        return json.load(f)


def load_usage_ledger_fixture(path: Path) -> Tuple[Dict[str, Any], List[str]]:
    """Load a usage ledger fixture and validate it."""
    data = load_fixture(path)
    return data, validate_usage_ledger_row(data)


def load_all_fixtures(fixture_dir: Path) -> List[Tuple[str, Dict[str, Any], List[str]]]:
    """Load and validate every .json fixture in a directory."""
    results = []
    for p in sorted(fixture_dir.glob("*.json")):
        data = load_fixture(p)
        violations = validate_usage_ledger_row(data)
        results.append((p.name, data, violations))
    return results
