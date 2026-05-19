"""Context Pack v2 schema and validation helpers.

Implements the four canonical wire schemas (advisor_context_pack_v2,
model_context_pack_v2, context_retrieval_request, context_retrieval_result)
and the v1.3.2 five-layer context_layers composition layout.

All schemas are compatible with the v1.2 canonical definitions and
v1.3.1/v1.3.2 architecture extensions.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field, asdict
from pathlib import Path
from typing import Any, Dict, List, Optional, Sequence, Tuple


# ---------------------------------------------------------------------------
# Schema versions
# ---------------------------------------------------------------------------

ADVISOR_CONTEXT_PACK_V2 = "advisor_context_pack.v2"
MODEL_CONTEXT_PACK_V2 = "model_context_pack.v2"
CONTEXT_RETRIEVAL_REQUEST = "context_retrieval_request.v1"
CONTEXT_RETRIEVAL_RESULT = "context_retrieval_result.v1"
CONTEXT_LAYERS_VERSION = "context_layers.v1"


# ---------------------------------------------------------------------------
# Enums
# ---------------------------------------------------------------------------

CALL_TYPES: Tuple[str, ...] = ("preflight", "correction", "arbitration", "risk_scan")

MODEL_ROLES: Tuple[str, ...] = (
    "planner", "executor", "debugger", "verifier", "advisor", "integrator",
)

CONTENT_MODES: Tuple[str, ...] = ("summary", "excerpt", "full")

RETRIEVAL_RESULT_STATUS: Tuple[str, ...] = (
    "fulfilled", "partial", "denied", "not_found", "budget_exceeded",
)

REQUESTER_TYPES: Tuple[str, ...] = (
    "advisor", "model", "verifier", "human", "evaluator",
)

REF_TYPES: Tuple[str, ...] = (
    "run_log", "completion", "handoff_pack", "artifact", "event", "digest", "source_excerpt",
)

FRESHNESS_VALUES: Tuple[str, ...] = ("current", "stale", "unknown")

CACHE_POLICY_VALUES: Tuple[str, ...] = (
    "no_cache", "read_cache_allowed", "write_cache_allowed", "read_write_cache_allowed",
)

PACK_PRUNE_POLICY_VALUES: Tuple[str, ...] = (
    "preserve_invariants",
    "drop_recent_evidence_first",
    "drop_memory_digest_first",
    "deny_if_over_budget",
)

RETRIEVAL_PRIORITY: Tuple[str, ...] = ("low", "normal", "high")


# ---------------------------------------------------------------------------
# Required fields per schema
# ---------------------------------------------------------------------------

ADVISOR_PACK_REQUIRED: Sequence[str] = (
    "schema_version", "pack_id", "task_id", "item_id", "call_type",
    "objective", "current_status", "allowed_files", "forbidden_files",
    "artifact_refs", "evidence_refs", "quality_signals", "budget",
    "retrieval_policy", "created_at",
)

MODEL_PACK_REQUIRED: Sequence[str] = (
    "schema_version", "pack_id", "task_id", "item_id", "model_tier",
    "model_harness_profile_id", "role", "task_summary",
    "allowed_tools", "forbidden_tools", "allowed_files", "forbidden_files",
    "artifact_refs", "evidence_refs", "context_budget", "retrieval_policy",
    "created_at",
)

RETRIEVAL_REQUEST_REQUIRED: Sequence[str] = (
    "request_id", "requester_id", "requester_type", "task_id",
    "reason", "requested_refs", "token_budget", "priority", "created_at",
)

RETRIEVAL_RESULT_REQUIRED: Sequence[str] = (
    "request_id", "result_id", "status", "returned_refs",
    "total_token_estimate", "budget_remaining", "created_at",
)

CONTEXT_LAYERS_REQUIRED: Sequence[str] = (
    "invariants", "task_pack", "dynamic_refs", "memory_digest", "recent_evidence",
)

MEMORY_DIGEST_REQUIRED: Sequence[str] = (
    "source_refs", "expiry_policy", "conflict_resolution",
)


# ---------------------------------------------------------------------------
# Validation helpers
# ---------------------------------------------------------------------------

def _check_fields(data: Dict[str, Any], required: Sequence[str], prefix: str = "") -> List[str]:
    violations: List[str] = []
    for f in required:
        if f not in data:
            label = f"{prefix}{f}" if prefix else f
            violations.append(f"missing required field: {label}")
    return violations


def validate_advisor_context_pack_v2(data: Dict[str, Any]) -> List[str]:
    """Validate an advisor_context_pack_v2 dict."""
    v = _check_fields(data, ADVISOR_PACK_REQUIRED)
    if v:
        return v

    if data["schema_version"] != ADVISOR_CONTEXT_PACK_V2:
        v.append(f"schema_version must be {ADVISOR_CONTEXT_PACK_V2}")
    if data["call_type"] not in CALL_TYPES:
        v.append(f"call_type {data['call_type']!r} not in {CALL_TYPES}")
    if not isinstance(data["allowed_files"], list):
        v.append("allowed_files must be a list")
    if not isinstance(data["forbidden_files"], list):
        v.append("forbidden_files must be a list")
    if not isinstance(data["artifact_refs"], list):
        v.append("artifact_refs must be a list")
    if not isinstance(data["evidence_refs"], list):
        v.append("evidence_refs must be a list")
    v.extend(_validate_budget(data.get("budget", {}), "budget"))
    v.extend(_validate_retrieval_policy(data.get("retrieval_policy", {})))
    return v


def validate_model_context_pack_v2(data: Dict[str, Any]) -> List[str]:
    """Validate a model_context_pack_v2 dict."""
    v = _check_fields(data, MODEL_PACK_REQUIRED)
    if v:
        return v

    if data["schema_version"] != MODEL_CONTEXT_PACK_V2:
        v.append(f"schema_version must be {MODEL_CONTEXT_PACK_V2}")
    if data["role"] not in MODEL_ROLES:
        v.append(f"role {data['role']!r} not in {MODEL_ROLES}")
    if not isinstance(data["allowed_tools"], list):
        v.append("allowed_tools must be a list")
    if not isinstance(data["forbidden_tools"], list):
        v.append("forbidden_tools must be a list")
    if not isinstance(data["allowed_files"], list):
        v.append("allowed_files must be a list")
    if not isinstance(data["forbidden_files"], list):
        v.append("forbidden_files must be a list")
    if not isinstance(data["artifact_refs"], list):
        v.append("artifact_refs must be a list")
    if not isinstance(data["evidence_refs"], list):
        v.append("evidence_refs must be a list")
    v.extend(_validate_context_budget(data.get("context_budget", {})))
    v.extend(_validate_retrieval_policy(data.get("retrieval_policy", {})))
    return v


def validate_context_retrieval_request(data: Dict[str, Any]) -> List[str]:
    """Validate a context_retrieval_request dict."""
    v = _check_fields(data, RETRIEVAL_REQUEST_REQUIRED)
    if v:
        return v

    if data["requester_type"] not in REQUESTER_TYPES:
        v.append(f"requester_type {data['requester_type']!r} not in {REQUESTER_TYPES}")
    if not isinstance(data["reason"], str) or not data["reason"]:
        v.append("reason must be a non-empty string")
    if not isinstance(data["requested_refs"], list):
        v.append("requested_refs must be a list")
    if data["priority"] not in RETRIEVAL_PRIORITY:
        v.append(f"priority {data['priority']!r} not in {RETRIEVAL_PRIORITY}")
    if not isinstance(data["token_budget"], int) or data["token_budget"] <= 0:
        v.append("token_budget must be a positive integer")
    for ref in data.get("requested_refs", []):
        if ref.get("requested_scope") not in CONTENT_MODES:
            v.append(f"requested_scope {ref.get('requested_scope')!r} not in {CONTENT_MODES}")
    return v


def validate_context_retrieval_result(data: Dict[str, Any]) -> List[str]:
    """Validate a context_retrieval_result dict."""
    v = _check_fields(data, RETRIEVAL_RESULT_REQUIRED)
    if v:
        return v

    if data["status"] not in RETRIEVAL_RESULT_STATUS:
        v.append(f"status {data['status']!r} not in {RETRIEVAL_RESULT_STATUS}")
    if not isinstance(data["returned_refs"], list):
        v.append("returned_refs must be a list")
    if not isinstance(data["total_token_estimate"], int) or data["total_token_estimate"] < 0:
        v.append("total_token_estimate must be a non-negative integer")
    if not isinstance(data["budget_remaining"], int) or data["budget_remaining"] < 0:
        v.append("budget_remaining must be a non-negative integer")

    for ref in data.get("returned_refs", []):
        if ref.get("content_mode") not in CONTENT_MODES:
            v.append(f"content_mode {ref.get('content_mode')!r} not in {CONTENT_MODES}")
        if "token_estimate" not in ref:
            v.append(f"returned_ref {ref.get('ref_id')} missing token_estimate")
    return v


def validate_context_layers(data: Dict[str, Any]) -> List[str]:
    """Validate a context_layers dict (five-layer structure)."""
    v = _check_fields(data, CONTEXT_LAYERS_REQUIRED)
    if v:
        return v

    # Validate memory_digest sub-structure
    md = data.get("memory_digest", {})
    if isinstance(md, dict):
        v.extend(_validate_memory_digest(md))
    else:
        v.append("memory_digest must be a dict")

    # Validate freshness if present
    if "freshness" in data:
        if data["freshness"] not in FRESHNESS_VALUES:
            v.append(f"freshness {data['freshness']!r} not in {FRESHNESS_VALUES}")

    # Validate cache_policy if present
    if "cache_policy" in data:
        if data["cache_policy"] not in CACHE_POLICY_VALUES:
            v.append(f"cache_policy {data['cache_policy']!r} not in {CACHE_POLICY_VALUES}")

    # Validate pack_prune_policy if present
    if "pack_prune_policy" in data:
        if data["pack_prune_policy"] not in PACK_PRUNE_POLICY_VALUES:
            v.append(f"pack_prune_policy {data['pack_prune_policy']!r} not in {PACK_PRUNE_POLICY_VALUES}")

    return v


def _validate_budget(budget: Dict[str, Any], prefix: str) -> List[str]:
    v: List[str] = []
    if not isinstance(budget, dict):
        v.append(f"{prefix} must be a dict")
        return v
    if "max_context_tokens" in budget:
        if not isinstance(budget["max_context_tokens"], int) or budget["max_context_tokens"] <= 0:
            v.append(f"{prefix}.max_context_tokens must be a positive integer")
    if "preferred_context_tokens" in budget:
        if not isinstance(budget["preferred_context_tokens"], int) or budget["preferred_context_tokens"] <= 0:
            v.append(f"{prefix}.preferred_context_tokens must be a positive integer")
    return v


def _validate_context_budget(budget: Dict[str, Any]) -> List[str]:
    v: List[str] = []
    if not isinstance(budget, dict):
        v.append("context_budget must be a dict")
        return v
    if "max_context_tokens" in budget:
        if not isinstance(budget["max_context_tokens"], int) or budget["max_context_tokens"] <= 0:
            v.append("context_budget.max_context_tokens must be a positive integer")
    if "preferred_context_tokens" in budget:
        if not isinstance(budget["preferred_context_tokens"], int) or budget["preferred_context_tokens"] <= 0:
            v.append("context_budget.preferred_context_tokens must be a positive integer")
    if "reserved_response_tokens" in budget:
        if not isinstance(budget["reserved_response_tokens"], int) or budget["reserved_response_tokens"] <= 0:
            v.append("context_budget.reserved_response_tokens must be a positive integer")
    return v


def _validate_retrieval_policy(policy: Dict[str, Any]) -> List[str]:
    v: List[str] = []
    if not isinstance(policy, dict):
        v.append("retrieval_policy must be a dict")
        return v
    if "allow_retrieval" in policy:
        if not isinstance(policy["allow_retrieval"], bool):
            v.append("retrieval_policy.allow_retrieval must be a bool")
    if "allowed_ref_types" in policy:
        if not isinstance(policy["allowed_ref_types"], list):
            v.append("retrieval_policy.allowed_ref_types must be a list")
    if "forbidden_paths" in policy:
        if not isinstance(policy["forbidden_paths"], list):
            v.append("retrieval_policy.forbidden_paths must be a list")
    return v


def _validate_memory_digest(md: Dict[str, Any]) -> List[str]:
    v: List[str] = []
    for f in MEMORY_DIGEST_REQUIRED:
        if f not in md:
            v.append(f"memory_digest missing required field: {f}")
    if "source_refs" in md and not isinstance(md["source_refs"], list):
        v.append("memory_digest.source_refs must be a list")
    return v


def validate_full_content_inline_denied(
    pack_data: Dict[str, Any],
    result_data: Dict[str, Any],
) -> List[str]:
    """Verify that full content inline is denied unless retrieval_result allows it.

    Returns violations if full content is inlined without proper retrieval.
    """
    violations: List[str] = []

    # Check artifact_refs for full content
    for ref in pack_data.get("artifact_refs", []):
        if ref.get("content_mode") == "full":
            # Must have a retrieval_result with content_mode=full and policy allows
            allowed = False
            for rref in result_data.get("returned_refs", []):
                if rref.get("ref_id") == ref.get("artifact_id") and rref.get("content_mode") == "full":
                    allowed = True
                    break
            if not allowed:
                violations.append(
                    f"artifact {ref.get('artifact_id')} has full content inline "
                    "without retrieval_result allowing it"
                )

    return violations


def check_budget_compliance(
    pack_data: Dict[str, Any],
    total_tokens_used: int,
) -> Tuple[bool, str]:
    """Check if a pack respects its context budget.

    Returns (compliant, reason).
    """
    budget = pack_data.get("context_budget") or pack_data.get("budget", {})
    max_tokens = budget.get("max_context_tokens", 0)
    if max_tokens <= 0:
        return True, "no budget defined"
    if total_tokens_used <= max_tokens:
        return True, f"within budget ({total_tokens_used}/{max_tokens})"
    return False, f"over budget ({total_tokens_used}/{max_tokens})"


def apply_prune_policy(
    pack_data: Dict[str, Any],
    current_tokens: int,
    max_tokens: int,
) -> Tuple[Dict[str, Any], str]:
    """Apply pack_prune_policy to bring a pack under budget.

    Returns (pruned_pack, action_taken). If pruning is impossible, raises ValueError.
    """
    policy = pack_data.get("pack_prune_policy", "deny_if_over_budget")

    if current_tokens <= max_tokens:
        return pack_data, "no_pruning_needed"

    if policy == "deny_if_over_budget":
        raise ValueError(
            f"pack over budget ({current_tokens}/{max_tokens}) and policy is deny_if_over_budget"
        )

    pruned = dict(pack_data)
    layers = pruned.get("context_layers", {})

    if policy == "drop_recent_evidence_first":
        if layers.get("recent_evidence"):
            layers["recent_evidence"] = []
            pruned["context_layers"] = layers
            return pruned, "dropped_recent_evidence"

    elif policy == "drop_memory_digest_first":
        if layers.get("memory_digest"):
            layers["memory_digest"] = {"source_refs": [], "expiry_policy": "on_prune", "conflict_resolution": "drop"}
            pruned["context_layers"] = layers
            return pruned, "dropped_memory_digest"

    elif policy == "preserve_invariants":
        # Try dropping recent_evidence first, then memory_digest
        if layers.get("recent_evidence"):
            layers["recent_evidence"] = []
            pruned["context_layers"] = layers
            return pruned, "dropped_recent_evidence"
        if layers.get("memory_digest"):
            layers["memory_digest"] = {"source_refs": [], "expiry_policy": "on_prune", "conflict_resolution": "drop"}
            pruned["context_layers"] = layers
            return pruned, "dropped_memory_digest"

    raise ValueError(
        f"cannot prune pack under budget ({current_tokens}/{max_tokens}) "
        f"with policy {policy!r}"
    )


# ---------------------------------------------------------------------------
# Data classes
# ---------------------------------------------------------------------------

@dataclass
class ContextBudget:
    max_context_tokens: int
    preferred_context_tokens: int
    max_response_tokens: Optional[int] = None
    reserved_response_tokens: Optional[int] = None

    def to_dict(self) -> Dict[str, Any]:
        return {k: v for k, v in asdict(self).items() if v is not None}


@dataclass
class RetrievalPolicy:
    allow_retrieval: bool = True
    allowed_ref_types: Optional[List[str]] = None
    forbidden_paths: Optional[List[str]] = None
    max_retrieval_calls: Optional[int] = None

    def to_dict(self) -> Dict[str, Any]:
        d: Dict[str, Any] = {"allow_retrieval": self.allow_retrieval}
        if self.allowed_ref_types is not None:
            d["allowed_ref_types"] = self.allowed_ref_types
        if self.forbidden_paths is not None:
            d["forbidden_paths"] = self.forbidden_paths
        if self.max_retrieval_calls is not None:
            d["max_retrieval_calls"] = self.max_retrieval_calls
        return d


@dataclass
class MemoryDigest:
    source_refs: List[str]
    expiry_policy: str
    conflict_resolution: str
    summary: Optional[str] = None

    def to_dict(self) -> Dict[str, Any]:
        return {k: v for k, v in asdict(self).items() if v is not None}


@dataclass
class ContextLayers:
    invariants: Dict[str, Any]
    task_pack: Dict[str, Any]
    dynamic_refs: List[Dict[str, Any]]
    memory_digest: MemoryDigest
    recent_evidence: List[Dict[str, Any]]
    freshness: str = "current"
    cache_policy: str = "no_cache"
    pack_prune_policy: str = "preserve_invariants"

    def to_dict(self) -> Dict[str, Any]:
        return {
            "invariants": self.invariants,
            "task_pack": self.task_pack,
            "dynamic_refs": self.dynamic_refs,
            "memory_digest": self.memory_digest.to_dict(),
            "recent_evidence": self.recent_evidence,
            "freshness": self.freshness,
            "cache_policy": self.cache_policy,
            "pack_prune_policy": self.pack_prune_policy,
        }


# ---------------------------------------------------------------------------
# Fixture loading
# ---------------------------------------------------------------------------

def load_fixture(path: Path) -> Dict[str, Any]:
    with open(path, "r") as f:
        return json.load(f)


def load_and_validate_fixture(path: Path, schema_type: str) -> Tuple[Dict[str, Any], List[str]]:
    data = load_fixture(path)
    validator = {
        "advisor_context_pack_v2": validate_advisor_context_pack_v2,
        "model_context_pack_v2": validate_model_context_pack_v2,
        "context_retrieval_request": validate_context_retrieval_request,
        "context_retrieval_result": validate_context_retrieval_result,
        "context_layers": validate_context_layers,
    }.get(schema_type)
    if validator is None:
        return data, [f"unknown schema_type: {schema_type!r}"]
    return data, validator(data)


def load_all_fixtures(fixture_dir: Path, schema_type: str) -> List[Tuple[str, Dict[str, Any], List[str]]]:
    results = []
    for p in sorted(fixture_dir.glob("*.json")):
        data, violations = load_and_validate_fixture(p, schema_type)
        results.append((p.name, data, violations))
    return results
