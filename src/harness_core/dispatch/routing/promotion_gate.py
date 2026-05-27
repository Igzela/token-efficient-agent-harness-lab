"""Promotion gate: shadow→active promotion with evidence thresholds."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from .schemas import PROMOTION_GATE_DEFAULTS, PROMOTION_VERDICTS, PROMOTION_VERDICT_SCHEMA_VERSION, RoutingObservation


class RoutingObservationStore:
    """In-memory observation storage keyed by arm_id."""

    def __init__(self) -> None:
        self._observations: list[RoutingObservation] = []
        self._by_arm: dict[str, list[RoutingObservation]] = {}

    def add_observation(self, obs: RoutingObservation) -> None:
        self._observations.append(obs)
        self._by_arm.setdefault(obs.arm_id, []).append(obs)

    def observations_for_arm(self, arm_id: str) -> list[RoutingObservation]:
        return list(self._by_arm.get(arm_id, []))

    def observations_for_tier_and_group(
        self, tier: str, task_domain: str, task_intent: str
    ) -> list[RoutingObservation]:
        return [
            o for o in self._observations
            if o.selected_tier == tier
            and o.task_domain == task_domain
            and o.task_intent == task_intent
        ]

    def count_by_arm(self, arm_id: str) -> int:
        return len(self._by_arm.get(arm_id, []))

    def count_for_tier_and_group(self, tier: str, task_domain: str, task_intent: str) -> int:
        return len(self.observations_for_tier_and_group(tier, task_domain, task_intent))

    def all_observations(self) -> list[RoutingObservation]:
        return list(self._observations)

    def total_count(self) -> int:
        return len(self._observations)


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


class PromotionGate:
    """Gate shadow→active promotion with evidence thresholds."""

    def __init__(
        self,
        observation_store: RoutingObservationStore,
        min_sample_count: int | None = None,
        max_failure_rate_delta: float | None = None,
        min_cost_reduction_pct: float | None = None,
        require_human_review: bool = False,
    ) -> None:
        self._store = observation_store
        self._min_samples = min_sample_count if min_sample_count is not None else PROMOTION_GATE_DEFAULTS["min_sample_count"]
        self._max_failure_delta = max_failure_rate_delta if max_failure_rate_delta is not None else PROMOTION_GATE_DEFAULTS["max_failure_rate_delta"]
        self._min_cost_reduction = min_cost_reduction_pct if min_cost_reduction_pct is not None else PROMOTION_GATE_DEFAULTS["min_cost_reduction_pct"]
        self._require_human = require_human_review

    def evaluate(
        self,
        task_group: str,
        candidate_tier: str,
        baseline_tier: str = "balanced_worker",
    ) -> PromotionVerdict:
        parts = task_group.split("_", 1)
        domain = parts[0] if parts else ""
        intent = parts[1] if len(parts) > 1 else ""

        sample_count = self._store.count_for_tier_and_group(candidate_tier, domain, intent)
        quality_delta = self._quality_delta(candidate_tier, baseline_tier, domain, intent)
        cost_reduction = self._cost_reduction(candidate_tier, baseline_tier, domain, intent)
        failure_delta = self._failure_rate_delta(candidate_tier, baseline_tier, domain, intent)

        reasons: list[str] = []

        if sample_count < self._min_samples:
            reasons.append(f"insufficient_samples:{sample_count}<{self._min_samples}")
            return PromotionVerdict(
                verdict="insufficient_data",
                task_group=task_group,
                candidate_tier=candidate_tier,
                baseline_tier=baseline_tier,
                sample_count=sample_count,
                quality_delta=quality_delta,
                cost_reduction_pct=cost_reduction,
                failure_rate_delta=failure_delta,
                reasons=tuple(reasons),
            )

        if quality_delta < 0:
            reasons.append(f"quality_regression:{quality_delta:.4f}")
            return PromotionVerdict(
                verdict="hold",
                task_group=task_group,
                candidate_tier=candidate_tier,
                baseline_tier=baseline_tier,
                sample_count=sample_count,
                quality_delta=quality_delta,
                cost_reduction_pct=cost_reduction,
                failure_rate_delta=failure_delta,
                reasons=tuple(reasons),
            )

        if cost_reduction < self._min_cost_reduction:
            reasons.append(f"cost_reduction_below_threshold:{cost_reduction:.2f}<{self._min_cost_reduction}")
            return PromotionVerdict(
                verdict="hold",
                task_group=task_group,
                candidate_tier=candidate_tier,
                baseline_tier=baseline_tier,
                sample_count=sample_count,
                quality_delta=quality_delta,
                cost_reduction_pct=cost_reduction,
                failure_rate_delta=failure_delta,
                reasons=tuple(reasons),
            )

        if failure_delta > self._max_failure_delta:
            reasons.append(f"failure_rate_worse:{failure_delta:.4f}>{self._max_failure_delta}")
            return PromotionVerdict(
                verdict="hold",
                task_group=task_group,
                candidate_tier=candidate_tier,
                baseline_tier=baseline_tier,
                sample_count=sample_count,
                quality_delta=quality_delta,
                cost_reduction_pct=cost_reduction,
                failure_rate_delta=failure_delta,
                reasons=tuple(reasons),
            )

        if self._require_human:
            reasons.append("human_review_required")
            return PromotionVerdict(
                verdict="hold",
                task_group=task_group,
                candidate_tier=candidate_tier,
                baseline_tier=baseline_tier,
                sample_count=sample_count,
                quality_delta=quality_delta,
                cost_reduction_pct=cost_reduction,
                failure_rate_delta=failure_delta,
                reasons=tuple(reasons),
                requires_human_review=True,
            )

        reasons.append("all_gate_conditions_met")
        return PromotionVerdict(
            verdict="promote",
            task_group=task_group,
            candidate_tier=candidate_tier,
            baseline_tier=baseline_tier,
            sample_count=sample_count,
            quality_delta=quality_delta,
            cost_reduction_pct=cost_reduction,
            failure_rate_delta=failure_delta,
            reasons=tuple(reasons),
        )

    def check_sample_count(self, task_group: str, tier: str) -> tuple[bool, int]:
        parts = task_group.split("_", 1)
        domain = parts[0] if parts else ""
        intent = parts[1] if len(parts) > 1 else ""
        count = self._store.count_for_tier_and_group(tier, domain, intent)
        return count >= self._min_samples, count

    def _quality_delta(self, candidate: str, baseline: str, domain: str, intent: str) -> float:
        c_obs = self._store.observations_for_tier_and_group(candidate, domain, intent)
        b_obs = self._store.observations_for_tier_and_group(baseline, domain, intent)
        c_avg = sum(o.quality_score for o in c_obs) / len(c_obs) if c_obs else 0.0
        b_avg = sum(o.quality_score for o in b_obs) / len(b_obs) if b_obs else 0.0
        return c_avg - b_avg

    def _cost_reduction(self, candidate: str, baseline: str, domain: str, intent: str) -> float:
        c_obs = self._store.observations_for_tier_and_group(candidate, domain, intent)
        b_obs = self._store.observations_for_tier_and_group(baseline, domain, intent)
        c_avg = sum(o.cost for o in c_obs) / len(c_obs) if c_obs else 0.0
        b_avg = sum(o.cost for o in b_obs) / len(b_obs) if b_obs else 0.0
        if b_avg == 0:
            return 0.0
        return ((b_avg - c_avg) / b_avg) * 100.0

    def _failure_rate_delta(self, candidate: str, baseline: str, domain: str, intent: str) -> float:
        c_obs = self._store.observations_for_tier_and_group(candidate, domain, intent)
        b_obs = self._store.observations_for_tier_and_group(baseline, domain, intent)
        c_fail = sum(1 for o in c_obs if not o.success) / len(c_obs) if c_obs else 0.0
        b_fail = sum(1 for o in b_obs if not o.success) / len(b_obs) if b_obs else 0.0
        return c_fail - b_fail
