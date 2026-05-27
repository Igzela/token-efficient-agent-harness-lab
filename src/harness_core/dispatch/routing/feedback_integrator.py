"""Feedback integrator: feed quality results back into routing decisions."""

from __future__ import annotations

import uuid
from datetime import datetime, timezone
from typing import Any

from .auto_policies import AutoDowngradePolicy, AutoUpgradePolicy
from .history_store import RoutingHistoryStore
from .promotion_gate import RoutingObservationStore
from .schemas import RoutingObservation, make_task_group, parse_task_group


class FeedbackIntegrator:
    """Feed quality results back into routing decisions."""

    def __init__(
        self,
        history_store: RoutingHistoryStore,
        observation_store: RoutingObservationStore,
        auto_downgrade: AutoDowngradePolicy | None = None,
        auto_upgrade: AutoUpgradePolicy | None = None,
    ) -> None:
        self._history = history_store
        self._observations = observation_store
        self._downgrade = auto_downgrade
        self._upgrade = auto_upgrade

    def record_outcome(
        self,
        dispatch_id: str,
        task_domain: str,
        task_intent: str,
        selected_tier: str,
        baseline_tier: str,
        quality_score: float,
        cost: float,
        latency_ms: int,
        success: bool,
        failure_domain: str | None = None,
        budget_violation: bool = False,
    ) -> RoutingObservation:
        obs = RoutingObservation(
            observation_id=f"obs-{uuid.uuid4().hex[:12]}",
            arm_id=f"arm-{selected_tier}",
            dispatch_id=dispatch_id,
            task_domain=task_domain,
            task_intent=task_intent,
            selected_tier=selected_tier,
            baseline_tier=baseline_tier,
            quality_score=quality_score,
            cost=cost,
            latency_ms=latency_ms,
            success=success,
            failure_domain=failure_domain,
            budget_violation=budget_violation,
            observed_at=datetime.now(timezone.utc).isoformat(),
        )
        self._observations.add_observation(obs)
        return obs

    def should_adapt(self, task_group: str, current_tier: str) -> tuple[bool, str]:
        domain, intent = parse_task_group(task_group)

        obs = self._observations.observations_for_tier_and_group(current_tier, domain, intent)
        if not obs:
            return False, "no_observations"

        quality_score = sum(o.quality_score for o in obs) / len(obs)
        failure_count = sum(1 for o in obs if not o.success)
        failure_rate = failure_count / len(obs)

        if self._upgrade:
            for tier in ("strong_planner", "balanced_worker", "cheap_executor"):
                if tier == current_tier:
                    continue
                should, reason = self._upgrade.should_upgrade(
                    task_group, current_tier, tier,
                    quality_score, failure_rate, "low",
                    self._history,
                )
                if should:
                    return True, f"upgrade_to_{tier}:{reason}"

        if self._downgrade:
            for tier in ("cheap_executor",):
                if tier == current_tier:
                    continue
                agg = self._history.aggregate_by_tier_and_task_group(tier, task_group)
                cop = agg.cost_of_pass if agg else None
                should, reason = self._downgrade.should_downgrade(
                    task_group, current_tier, tier,
                    quality_score, cop, self._history,
                )
                if should:
                    return True, f"downgrade_to_{tier}:{reason}"

        return False, "no_adaptation_needed"

    def summary(self, task_group: str) -> dict[str, Any]:
        domain, intent = parse_task_group(task_group)

        all_obs = [
            o for o in self._observations.all_observations()
            if o.task_domain == domain and o.task_intent == intent
        ]
        if not all_obs:
            return {
                "task_group": task_group,
                "sample_count": 0,
                "tiers": [],
                "best_tier": None,
                "avg_quality": 0.0,
                "avg_cost": 0.0,
            }

        tier_quality: dict[str, list[float]] = {}
        tier_cost: dict[str, list[float]] = {}
        for o in all_obs:
            tier_quality.setdefault(o.selected_tier, []).append(o.quality_score)
            tier_cost.setdefault(o.selected_tier, []).append(o.cost)

        best_tier = max(
            tier_quality.keys(),
            key=lambda t: sum(tier_quality[t]) / len(tier_quality[t]),
        )

        return {
            "task_group": task_group,
            "sample_count": len(all_obs),
            "tiers": sorted(set(o.selected_tier for o in all_obs)),
            "best_tier": best_tier,
            "avg_quality": sum(o.quality_score for o in all_obs) / len(all_obs),
            "avg_cost": sum(o.cost for o in all_obs) / len(all_obs),
        }

    @staticmethod
    def task_group_for(dispatch_domain: str, dispatch_intent: str) -> str:
        return make_task_group(dispatch_domain, dispatch_intent)
