"""Dispatch decision schemas: DispatchDecision, BudgetReservation, ExecutionGate, and supporting types."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

# ---------------------------------------------------------------------------
# Schema versions
# ---------------------------------------------------------------------------

BUDGET_RESERVATION_SCHEMA_VERSION = "budget_reservation.v1"
DISPATCH_DECISION_SCHEMA_VERSION = "dispatch_decision.v1"

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

TASK_DOMAINS: tuple[str, ...] = (
    "code", "docs", "config", "infra", "math",
    "architecture", "repo_ops", "governance", "other",
)

TASK_INTENTS: tuple[str, ...] = (
    "generate", "review", "debug", "summarize", "audit",
    "plan", "refactor", "compare", "explain", "classify",
)

RISK_FLAGS: tuple[str, ...] = (
    "target_write", "provider_call", "sandbox_execution", "deployment",
    "secret_handling", "destructive_operation", "long_context", "high_uncertainty",
)

RISK_LEVELS: tuple[str, ...] = ("low", "medium", "high", "critical")

QUALITY_REQUIREMENTS: tuple[str, ...] = ("draft", "standard", "high", "critical")

MODEL_TIERS: tuple[str, ...] = (
    "cheap_executor", "balanced_worker", "strong_planner", "verifier", "advisor",
)

COMPLEXITY_WEIGHTS: dict[str, float] = {
    "cognitive": 0.35,
    "context": 0.25,
    "execution_risk": 0.25,
    "ambiguity": 0.15,
}

EXECUTION_GATE_TYPES: tuple[str, ...] = (
    "budget", "risk", "boundary", "confidence",
    "manual_review", "provider_disabled", "sandbox_disabled", "target_write",
)

GATE_SEVERITIES: tuple[str, ...] = ("info", "warning", "block", "critical")

CLEARANCE_VALUES: tuple[str, ...] = ("none", "human", "governance", "policy")

DECISION_STATUSES: tuple[str, ...] = (
    "decided", "needs_approval", "blocked", "diagnostic_only",
)

EXECUTOR_TYPES: tuple[str, ...] = ("noop", "mock", "manual", "provider")

REQUEST_SOURCES: tuple[str, ...] = (
    "cli", "api", "dashboard", "agent", "workflow", "test_fixture",
)


# ---------------------------------------------------------------------------
# Schemas
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class Evidence:
    feature: str
    text: str
    span: tuple[int, int]  # [start, end]
    polarity: str  # "positive" | "negative"
    source: str  # "raw_request" | "repo_context" | "user_constraints" | "target_metadata"
    rule_id: str | None = None
    confidence: float = 1.0
    negation_scope: str | None = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "feature": self.feature,
            "text": self.text,
            "span": list(self.span),
            "polarity": self.polarity,
            "source": self.source,
            "rule_id": self.rule_id,
            "confidence": self.confidence,
            "negation_scope": self.negation_scope,
        }


@dataclass(frozen=True)
class ShadowRoute:
    tier: str
    profile_id: str | None
    reason: str
    admission_scope: str  # literal "diagnostic"
    estimated_cost: float | None = None
    expected_tradeoff: str = ""

    def to_dict(self) -> dict[str, Any]:
        return {
            "tier": self.tier,
            "profile_id": self.profile_id,
            "reason": self.reason,
            "admission_scope": self.admission_scope,
            "estimated_cost": self.estimated_cost,
            "expected_tradeoff": self.expected_tradeoff,
        }


@dataclass(frozen=True)
class BudgetReservation:
    reservation_id: str
    decision_id: str
    currency: str  # "USD" | "token"
    pre_budget: int
    reserved_input_tokens: int
    reserved_output_tokens: int
    reserved_total_tokens: int
    reserved_cost: float
    status: str  # "reserved" | "consumed" | "released" | "violated" | "expired"
    created_at: str
    updated_at: str
    pricing_snapshot_id: str | None = None
    budget_policy_id: str | None = None
    budget_gate: str | None = None
    actual_usage_ref: str | None = None
    budget_delta: int | None = None
    budget_violation: bool = False
    expires_at: str | None = None
    schema_version: str = BUDGET_RESERVATION_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "reservation_id": self.reservation_id,
            "decision_id": self.decision_id,
            "currency": self.currency,
            "pricing_snapshot_id": self.pricing_snapshot_id,
            "pre_budget": self.pre_budget,
            "reserved_input_tokens": self.reserved_input_tokens,
            "reserved_output_tokens": self.reserved_output_tokens,
            "reserved_total_tokens": self.reserved_total_tokens,
            "reserved_cost": self.reserved_cost,
            "budget_policy_id": self.budget_policy_id,
            "budget_gate": self.budget_gate,
            "status": self.status,
            "actual_usage_ref": self.actual_usage_ref,
            "budget_delta": self.budget_delta,
            "budget_violation": self.budget_violation,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
            "expires_at": self.expires_at,
        }


@dataclass(frozen=True)
class ExecutionGate:
    gate_id: str
    gate_type: str  # from EXECUTION_GATE_TYPES
    severity: str  # from GATE_SEVERITIES
    reason: str
    evidence_refs: tuple[str, ...] = ()
    clearance_required: str = "none"  # from CLEARANCE_VALUES
    cleared: bool = False
    cleared_by: str | None = None
    cleared_at: str | None = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "gate_id": self.gate_id,
            "gate_type": self.gate_type,
            "severity": self.severity,
            "reason": self.reason,
            "evidence_refs": list(self.evidence_refs),
            "clearance_required": self.clearance_required,
            "cleared": self.cleared,
            "cleared_by": self.cleared_by,
            "cleared_at": self.cleared_at,
        }


@dataclass(frozen=True)
class RejectedCandidate:
    tier: str
    profile_id: str | None
    reason: str
    constraint_failed: str | None = None
    estimated_cost: float | None = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "tier": self.tier,
            "profile_id": self.profile_id,
            "reason": self.reason,
            "constraint_failed": self.constraint_failed,
            "estimated_cost": self.estimated_cost,
        }


@dataclass(frozen=True)
class DispatchDecision:
    decision_id: str
    analysis_id: str
    analysis_snapshot: dict[str, Any]
    selected_tier: str
    fallback_tier: str
    routing_reason: str
    quality_requirement: str  # from QUALITY_REQUIREMENTS
    expected_quality_band: str  # "low" | "medium" | "high" | "unknown"
    confidence: float
    confidence_label: str  # "low" | "medium" | "high"
    budget_reservation: BudgetReservation
    execution_policy: dict[str, Any]  # executor_type, execution_allowed, requires_human_review, max_retries
    decision_status: str  # from DECISION_STATUSES
    created_at: str
    selected_profile_id: str | None = None
    fallback_profile_id: str | None = None
    shadow_routes: tuple[ShadowRoute, ...] = ()
    hard_constraints: tuple[str, ...] = ()
    rejected_candidates: tuple[RejectedCandidate, ...] = ()
    no_shadow_route_reason: str | None = None
    max_input_tokens: int = 4000
    max_output_tokens: int = 3000
    execution_gates: tuple[ExecutionGate, ...] = ()
    schema_version: str = DISPATCH_DECISION_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "decision_id": self.decision_id,
            "analysis_id": self.analysis_id,
            "analysis_snapshot": self.analysis_snapshot,
            "selected_tier": self.selected_tier,
            "selected_profile_id": self.selected_profile_id,
            "fallback_tier": self.fallback_tier,
            "fallback_profile_id": self.fallback_profile_id,
            "shadow_routes": [sr.to_dict() for sr in self.shadow_routes],
            "hard_constraints": list(self.hard_constraints),
            "rejected_candidates": [rc.to_dict() for rc in self.rejected_candidates],
            "no_shadow_route_reason": self.no_shadow_route_reason,
            "max_input_tokens": self.max_input_tokens,
            "max_output_tokens": self.max_output_tokens,
            "routing_reason": self.routing_reason,
            "quality_requirement": self.quality_requirement,
            "expected_quality_band": self.expected_quality_band,
            "confidence": self.confidence,
            "confidence_label": self.confidence_label,
            "budget_reservation": self.budget_reservation.to_dict(),
            "execution_policy": self.execution_policy,
            "execution_gates": [eg.to_dict() for eg in self.execution_gates],
            "decision_status": self.decision_status,
            "created_at": self.created_at,
        }
