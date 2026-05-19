"""Model Harness Profile and Shadow Routing schema and validation helpers.

Defines model_harness_profile for describing model capabilities (not credentials)
and shadow_routing_recommendation for diagnostic-only routing suggestions.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, asdict
from pathlib import Path
from typing import Any, Dict, List, Optional, Sequence, Tuple


# ---------------------------------------------------------------------------
# Schema versions
# ---------------------------------------------------------------------------

MODEL_PROFILE_SCHEMA_VERSION = "model_harness_profile.v1"
SHADOW_ROUTING_SCHEMA_VERSION = "shadow_routing_recommendation.v1"


# ---------------------------------------------------------------------------
# Enums
# ---------------------------------------------------------------------------

TIERS: Tuple[str, ...] = (
    "cheap_executor",
    "balanced_worker",
    "strong_planner",
    "verifier",
    "advisor",
)

TOOL_STRICTNESS: Tuple[str, ...] = ("strict", "tolerant", "unsupported")

JSON_TOLERANCE: Tuple[str, ...] = ("strict_json", "tolerant_json", "text_only")

REASONING_EFFORT: Tuple[str, ...] = ("low", "medium", "high")

PARALLEL_TOOL_PREFERENCE: Tuple[str, ...] = ("none", "allowed", "preferred", "forbidden")

CACHE_STRATEGY: Tuple[str, ...] = ("no_cache", "read_cache", "write_cache", "read_write_cache")

FALLBACK_POLICY: Tuple[str, ...] = (
    "no_fallback", "same_tier_only", "lower_cost_allowed",
    "higher_quality_allowed", "human_required",
)

ENFORCEMENT_SCOPES: Tuple[str, ...] = (
    "prompt_assembly", "gateway_validation", "context_broker", "all",
)

RECOMMENDATION_VALUES: Tuple[str, ...] = (
    "keep_baseline", "try_candidate", "reject_candidate", "needs_more_evidence",
)

RISK_LEVELS: Tuple[str, ...] = ("low", "medium", "high", "critical")

CREDENTIAL_KEYWORDS: Tuple[str, ...] = (
    "api_key", "secret", "token", "password", "credential",
    "private_key", "access_key", "auth_token",
)


# ---------------------------------------------------------------------------
# Required fields
# ---------------------------------------------------------------------------

PROFILE_REQUIRED: Sequence[str] = (
    "schema_version", "profile_id", "provider", "model_id", "tier",
    "tool_strictness", "json_tolerance", "reasoning_effort",
    "output_format_expectation", "parallel_tool_preference",
    "escaping_quirks", "cache_strategy", "fallback_policy",
    "context_window", "cost_metadata", "allowed_tools",
    "forbidden_previous_tools",
)

SHADOW_ROUTING_REQUIRED: Sequence[str] = (
    "recommendation_id", "task_family", "variant_family",
    "success_criterion", "candidate_profile_id", "baseline_profile_id",
    "rationale", "evidence_refs", "expected_quality_delta",
    "expected_cost_delta", "risk_level", "recommendation",
    "admission_scope", "active_routing_allowed",
)


# ---------------------------------------------------------------------------
# Validation
# ---------------------------------------------------------------------------

def validate_model_harness_profile(data: Dict[str, Any]) -> List[str]:
    """Validate a model_harness_profile dict. Returns list of violations."""
    violations: List[str] = []

    # 1. Required fields
    for f in PROFILE_REQUIRED:
        if f not in data:
            violations.append(f"missing required field: {f}")
    if violations:
        return violations

    # 2. schema_version
    if data["schema_version"] != MODEL_PROFILE_SCHEMA_VERSION:
        violations.append(f"schema_version must be {MODEL_PROFILE_SCHEMA_VERSION}")

    # 3. Enums
    if data["tier"] not in TIERS:
        violations.append(f"tier {data['tier']!r} not in {TIERS}")
    if data["tool_strictness"] not in TOOL_STRICTNESS:
        violations.append(f"tool_strictness {data['tool_strictness']!r} not in {TOOL_STRICTNESS}")
    if data["json_tolerance"] not in JSON_TOLERANCE:
        violations.append(f"json_tolerance {data['json_tolerance']!r} not in {JSON_TOLERANCE}")
    if data["reasoning_effort"] not in REASONING_EFFORT:
        violations.append(f"reasoning_effort {data['reasoning_effort']!r} not in {REASONING_EFFORT}")
    if data["parallel_tool_preference"] not in PARALLEL_TOOL_PREFERENCE:
        violations.append(f"parallel_tool_preference {data['parallel_tool_preference']!r} not in {PARALLEL_TOOL_PREFERENCE}")
    if data["cache_strategy"] not in CACHE_STRATEGY:
        violations.append(f"cache_strategy {data['cache_strategy']!r} not in {CACHE_STRATEGY}")
    if data["fallback_policy"] not in FALLBACK_POLICY:
        violations.append(f"fallback_policy {data['fallback_policy']!r} not in {FALLBACK_POLICY}")

    # 4. context_window must be positive integer
    if not isinstance(data["context_window"], int) or data["context_window"] <= 0:
        violations.append(f"context_window must be a positive integer, got {data['context_window']!r}")

    # 5. cost_metadata validation
    v_cost = _validate_cost_metadata(data.get("cost_metadata", {}))
    violations.extend(v_cost)

    # 6. allowed_tools must be a list
    if not isinstance(data["allowed_tools"], list):
        violations.append("allowed_tools must be a list")

    # 7. forbidden_previous_tools validation
    v_fpt = _validate_forbidden_previous_tools(data.get("forbidden_previous_tools", []))
    violations.extend(v_fpt)

    # 8. allowed_tools and forbidden_previous_tools conflict
    v_conflict = _check_tool_conflict(data.get("allowed_tools", []), data.get("forbidden_previous_tools", []))
    violations.extend(v_conflict)

    # 9. Credential detection
    v_cred = _detect_credentials(data)
    violations.extend(v_cred)

    # 10. fallback_policy=human_required check
    if data["fallback_policy"] == "human_required":
        # This is informational — human_required means no auto fallback
        pass  # Valid, just noted

    return violations


def validate_shadow_routing_recommendation(data: Dict[str, Any]) -> List[str]:
    """Validate a shadow_routing_recommendation dict."""
    violations: List[str] = []

    # 1. Required fields
    for f in SHADOW_ROUTING_REQUIRED:
        if f not in data:
            violations.append(f"missing required field: {f}")
    if violations:
        return violations

    # 2. schema_version
    if data.get("schema_version") != SHADOW_ROUTING_SCHEMA_VERSION:
        violations.append(f"schema_version must be {SHADOW_ROUTING_SCHEMA_VERSION}")

    # 3. recommendation enum
    if data["recommendation"] not in RECOMMENDATION_VALUES:
        violations.append(f"recommendation {data['recommendation']!r} not in {RECOMMENDATION_VALUES}")

    # 4. risk_level enum
    if data["risk_level"] not in RISK_LEVELS:
        violations.append(f"risk_level {data['risk_level']!r} not in {RISK_LEVELS}")

    # 5. admission_scope must be diagnostic
    if data["admission_scope"] != "diagnostic":
        violations.append(f"admission_scope must be 'diagnostic', got {data['admission_scope']!r}")

    # 6. active_routing_allowed must be false
    if data["active_routing_allowed"] is not False:
        violations.append(f"active_routing_allowed must be false, got {data['active_routing_allowed']!r}")

    # 7. evidence_refs must be a list
    if not isinstance(data["evidence_refs"], list):
        violations.append("evidence_refs must be a list")

    # 8. rationale must be non-empty string
    if not isinstance(data["rationale"], str) or not data["rationale"]:
        violations.append("rationale must be a non-empty string")

    return violations


def _validate_cost_metadata(meta: Dict[str, Any]) -> List[str]:
    violations: List[str] = []
    if not isinstance(meta, dict):
        violations.append("cost_metadata must be a dict")
        return violations
    for field in ("input_cost_per_1k", "output_cost_per_1k"):
        if field in meta:
            if not isinstance(meta[field], (int, float)) or meta[field] < 0:
                violations.append(f"cost_metadata.{field} must be non-negative, got {meta[field]!r}")
    return violations


def _validate_forbidden_previous_tools(items: Any) -> List[str]:
    violations: List[str] = []
    if not isinstance(items, list):
        violations.append("forbidden_previous_tools must be a list")
        return violations
    for i, item in enumerate(items):
        if not isinstance(item, dict):
            violations.append(f"forbidden_previous_tools[{i}] must be a dict")
            continue
        if "tool_id" not in item:
            violations.append(f"forbidden_previous_tools[{i}] missing tool_id")
        if "reason" not in item or not item.get("reason"):
            violations.append(f"forbidden_previous_tools[{i}] missing reason")
        if "enforcement_scope" in item:
            if item["enforcement_scope"] not in ENFORCEMENT_SCOPES:
                violations.append(
                    f"forbidden_previous_tools[{i}].enforcement_scope {item['enforcement_scope']!r} "
                    f"not in {ENFORCEMENT_SCOPES}"
                )
    return violations


def _check_tool_conflict(allowed: List[Any], forbidden: List[Any]) -> List[str]:
    violations: List[str] = []
    allowed_ids = set()
    for t in allowed:
        if isinstance(t, dict):
            allowed_ids.add(t.get("tool_id", ""))
        elif isinstance(t, str):
            allowed_ids.add(t)

    forbidden_ids = set()
    for t in forbidden:
        if isinstance(t, dict):
            forbidden_ids.add(t.get("tool_id", ""))
        elif isinstance(t, str):
            forbidden_ids.add(t)

    conflict = allowed_ids & forbidden_ids
    if conflict:
        violations.append(f"tool_id conflict between allowed_tools and forbidden_previous_tools: {conflict}")
    return violations


def _detect_credentials(data: Dict[str, Any]) -> List[str]:
    """Detect provider credentials in profile data."""
    violations: List[str] = []
    data_str = json.dumps(data).lower()
    for kw in CREDENTIAL_KEYWORDS:
        if kw in data_str:
            violations.append(
                f"provider credential detected in profile: '{kw}' found; "
                "profiles describe capabilities, not credentials"
            )
            break  # One violation is enough
    return violations


# ---------------------------------------------------------------------------
# Shadow routing helpers
# ---------------------------------------------------------------------------

def is_shadow_only(recommendation: Dict[str, Any]) -> bool:
    """Check if a recommendation is shadow-only (diagnostic, not active)."""
    return (
        recommendation.get("admission_scope") == "diagnostic"
        and recommendation.get("active_routing_allowed") is False
    )


def can_compare_with_usage_ledger(
    recommendation: Dict[str, Any],
    usage_ledger_group: str,
) -> Tuple[bool, str]:
    """Check if a shadow recommendation can be compared with a usage_ledger group.

    They must share the same cost_of_pass_group pattern.
    """
    rec_group = f"{recommendation['task_family']}/{recommendation['variant_family']}/{recommendation['success_criterion']}"
    # The usage_ledger group has 4 segments: eval_suite/task_family/variant_family/success_criterion
    # The recommendation has 3 segments: task_family/variant_family/success_criterion
    # We check if the recommendation's 3 segments match the last 3 segments of the usage_ledger group
    ledger_parts = usage_ledger_group.split("/")
    if len(ledger_parts) == 4:
        ledger_tail = "/".join(ledger_parts[1:])
    else:
        ledger_tail = usage_ledger_group

    if rec_group == ledger_tail:
        return True, "recommendation matches usage_ledger group tail"
    return False, f"recommendation {rec_group!r} does not match usage_ledger group {usage_ledger_group!r}"


# ---------------------------------------------------------------------------
# Data classes
# ---------------------------------------------------------------------------

@dataclass
class CostMetadata:
    input_cost_per_1k: float = 0.0
    output_cost_per_1k: float = 0.0
    cache_read_cost_per_1k: Optional[float] = None
    cache_write_cost_per_1k: Optional[float] = None

    def to_dict(self) -> Dict[str, Any]:
        return {k: v for k, v in asdict(self).items() if v is not None}


@dataclass
class ForbiddenPreviousTool:
    tool_id: str
    tool_type: str = ""
    reason: str = ""
    replacement_tool_id: Optional[str] = None
    enforcement_scope: str = "all"

    def to_dict(self) -> Dict[str, Any]:
        return {k: v for k, v in asdict(self).items() if v is not None}


@dataclass
class ModelHarnessProfile:
    profile_id: str
    provider: str
    model_id: str
    tier: str
    tool_strictness: str
    json_tolerance: str
    reasoning_effort: str
    output_format_expectation: str
    parallel_tool_preference: str
    escaping_quirks: str
    cache_strategy: str
    fallback_policy: str
    context_window: int
    cost_metadata: CostMetadata
    allowed_tools: List[Dict[str, Any]]
    forbidden_previous_tools: List[Dict[str, Any]]
    schema_version: str = MODEL_PROFILE_SCHEMA_VERSION

    def to_dict(self) -> Dict[str, Any]:
        d = asdict(self)
        d["cost_metadata"] = self.cost_metadata.to_dict()
        return d

    def to_json(self, **kw: Any) -> str:
        return json.dumps(self.to_dict(), **kw)


@dataclass
class ShadowRoutingRecommendation:
    recommendation_id: str
    task_family: str
    variant_family: str
    success_criterion: str
    candidate_profile_id: str
    baseline_profile_id: str
    rationale: str
    evidence_refs: List[str]
    expected_quality_delta: float
    expected_cost_delta: float
    risk_level: str
    recommendation: str
    admission_scope: str = "diagnostic"
    active_routing_allowed: bool = False
    schema_version: str = SHADOW_ROUTING_SCHEMA_VERSION

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)

    def to_json(self, **kw: Any) -> str:
        return json.dumps(self.to_dict(), **kw)


# ---------------------------------------------------------------------------
# Fixture loading
# ---------------------------------------------------------------------------

def load_fixture(path: Path) -> Dict[str, Any]:
    with open(path, "r") as f:
        return json.load(f)


def load_and_validate_profile(path: Path) -> Tuple[Dict[str, Any], List[str]]:
    data = load_fixture(path)
    return data, validate_model_harness_profile(data)


def load_and_validate_shadow(path: Path) -> Tuple[Dict[str, Any], List[str]]:
    data = load_fixture(path)
    return data, validate_shadow_routing_recommendation(data)


def load_all_fixtures(fixture_dir: Path) -> List[Tuple[str, Dict[str, Any], List[str]]]:
    results = []
    for p in sorted(fixture_dir.glob("*.json")):
        data = load_fixture(p)
        # Auto-detect schema type
        if data.get("schema_version") == SHADOW_ROUTING_SCHEMA_VERSION:
            violations = validate_shadow_routing_recommendation(data)
        else:
            violations = validate_model_harness_profile(data)
        results.append((p.name, data, violations))
    return results
