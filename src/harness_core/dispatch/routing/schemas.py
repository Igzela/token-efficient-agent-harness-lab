"""Phase 4 routing schemas: experiments, arms, observations, promotion verdicts."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

# ---------------------------------------------------------------------------
# Schema versions
# ---------------------------------------------------------------------------

ROUTING_EXPERIMENT_SCHEMA_VERSION = "routing_experiment.v1"
ROUTING_ARM_SCHEMA_VERSION = "routing_arm.v1"
ROUTING_OBSERVATION_SCHEMA_VERSION = "routing_observation.v1"
PROMOTION_VERDICT_SCHEMA_VERSION = "promotion_verdict.v1"

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

EXPERIMENT_STATUSES: tuple[str, ...] = ("created", "running", "concluded", "rolled_back")
EXPERIMENT_CONCLUSIONS: tuple[str, ...] = ("adopt_candidate", "keep_baseline", "inconclusive", "rolled_back")
ROUTING_MODES: tuple[str, ...] = ("static", "adaptive", "shadow")
PROMOTION_VERDICTS: tuple[str, ...] = ("promote", "hold", "reject", "insufficient_data")
DOWNGRADE_REASONS: tuple[str, ...] = ("cost_optimization", "quality_sufficient", "budget_pressure")
UPGRADE_REASONS: tuple[str, ...] = ("high_uncertainty", "failure_rate", "critical_task", "quality_risk")

PROMOTION_GATE_DEFAULTS: dict[str, Any] = {
    "min_sample_count": 30,
    "max_failure_rate_delta": 0.05,
    "min_cost_reduction_pct": 5.0,
}

# ---------------------------------------------------------------------------
# Task group helpers — "/" delimiter to avoid collision with underscored domains
# ---------------------------------------------------------------------------

_TASK_GROUP_SEP = "/"


def make_task_group(domain: str, intent: str) -> str:
    return f"{domain}{_TASK_GROUP_SEP}{intent}"


def parse_task_group(task_group: str) -> tuple[str, str]:
    parts = task_group.split(_TASK_GROUP_SEP, 1)
    domain = parts[0] if parts else ""
    intent = parts[1] if len(parts) > 1 else ""
    return domain, intent

# ---------------------------------------------------------------------------
# Schemas
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class RoutingObservation:
    observation_id: str
    arm_id: str
    dispatch_id: str
    task_domain: str
    task_intent: str
    selected_tier: str
    baseline_tier: str
    quality_score: float
    cost: float
    latency_ms: int
    success: bool
    failure_domain: str | None = None
    budget_violation: bool = False
    observed_at: str = ""
    schema_version: str = ROUTING_OBSERVATION_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "observation_id": self.observation_id,
            "arm_id": self.arm_id,
            "dispatch_id": self.dispatch_id,
            "task_domain": self.task_domain,
            "task_intent": self.task_intent,
            "selected_tier": self.selected_tier,
            "baseline_tier": self.baseline_tier,
            "quality_score": self.quality_score,
            "cost": self.cost,
            "latency_ms": self.latency_ms,
            "success": self.success,
            "failure_domain": self.failure_domain,
            "budget_violation": self.budget_violation,
            "observed_at": self.observed_at,
        }


@dataclass(frozen=True)
class RoutingArm:
    arm_id: str
    experiment_id: str
    tier: str
    profile_id: str | None = None
    traffic_weight: float = 1.0
    observations: tuple[RoutingObservation, ...] = ()
    schema_version: str = ROUTING_ARM_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "arm_id": self.arm_id,
            "experiment_id": self.experiment_id,
            "tier": self.tier,
            "profile_id": self.profile_id,
            "traffic_weight": self.traffic_weight,
            "observations": [o.to_dict() for o in self.observations],
        }


@dataclass(frozen=True)
class RoutingExperiment:
    experiment_id: str
    name: str
    task_group: str
    arms: tuple[RoutingArm, ...] = ()
    status: str = "created"
    start_time: str | None = None
    end_time: str | None = None
    conclusion: str | None = None
    schema_version: str = ROUTING_EXPERIMENT_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "experiment_id": self.experiment_id,
            "name": self.name,
            "task_group": self.task_group,
            "arms": [a.to_dict() for a in self.arms],
            "status": self.status,
            "start_time": self.start_time,
            "end_time": self.end_time,
            "conclusion": self.conclusion,
        }


@dataclass(frozen=True)
class PromotionVerdict:
    verdict: str
    task_group: str
    candidate_tier: str
    baseline_tier: str
    sample_count: int
    quality_delta: float
    cost_reduction_pct: float
    failure_rate_delta: float
    reasons: tuple[str, ...] = ()
    requires_human_review: bool = False
    schema_version: str = PROMOTION_VERDICT_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "verdict": self.verdict,
            "task_group": self.task_group,
            "candidate_tier": self.candidate_tier,
            "baseline_tier": self.baseline_tier,
            "sample_count": self.sample_count,
            "quality_delta": self.quality_delta,
            "cost_reduction_pct": self.cost_reduction_pct,
            "failure_rate_delta": self.failure_rate_delta,
            "reasons": list(self.reasons),
            "requires_human_review": self.requires_human_review,
        }


@dataclass
class RoutingSelection:
    """Return type from adaptive tier selectors — carries routing metadata."""

    selected_tier: str
    selected_profile_id: str | None
    fallback_tier: str
    fallback_profile_id: str | None
    shadow_routes: list  # list[ShadowRoute]
    rejected_candidates: list  # list[RejectedCandidate]
    routing_reason: str
    routing_mode: str  # "adaptive" or "static"
    routing_experiment_id: str | None = None

    def as_tuple_7(self) -> tuple:
        return (
            self.selected_tier,
            self.selected_profile_id,
            self.fallback_tier,
            self.fallback_profile_id,
            self.shadow_routes,
            self.rejected_candidates,
            self.routing_reason,
        )
