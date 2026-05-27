"""Cost-of-pass router: route based on historical cost-of-pass data."""

from __future__ import annotations

from typing import Any

from ...usage_ledger import CostOfPassAggregate
from .history_store import RoutingHistoryStore


class CostOfPassRouter:
    """Route based on historical cost-of-pass data."""

    def __init__(
        self,
        history_store: RoutingHistoryStore,
        min_sample_count: int = 30,
        min_cost_reduction_pct: float = 5.0,
    ) -> None:
        self._store = history_store
        self._min_samples = min_sample_count
        self._min_cost_reduction = min_cost_reduction_pct

    def best_tier_for_task_group(self, task_group: str) -> tuple[str, float] | None:
        tiers = self._store.tiers_observed(task_group)
        if not tiers:
            return None

        best_tier: str | None = None
        best_cop: float | None = None

        for tier in tiers:
            agg = self._store.aggregate_by_tier_and_task_group(tier, task_group)
            if agg is None or agg.cost_of_pass is None:
                continue
            if agg.total_count < self._min_samples:
                continue
            if best_cop is None or agg.cost_of_pass < best_cop:
                best_cop = agg.cost_of_pass
                best_tier = tier

        if best_tier is None or best_cop is None:
            return None
        return best_tier, best_cop

    def can_route_adaptively(self, task_group: str) -> bool:
        return self.best_tier_for_task_group(task_group) is not None

    def cost_comparison(
        self, task_group: str, tier_a: str, tier_b: str
    ) -> tuple[float, float, float] | None:
        agg_a = self._store.aggregate_by_tier_and_task_group(tier_a, task_group)
        agg_b = self._store.aggregate_by_tier_and_task_group(tier_b, task_group)
        if (
            agg_a is None or agg_b is None
            or agg_a.cost_of_pass is None or agg_b.cost_of_pass is None
        ):
            return None
        if agg_b.cost_of_pass == 0:
            return None
        delta_pct = ((agg_a.cost_of_pass - agg_b.cost_of_pass) / agg_b.cost_of_pass) * 100.0
        return agg_a.cost_of_pass, agg_b.cost_of_pass, delta_pct

    def failure_rate(self, tier: str, task_group: str) -> float:
        agg = self._store.aggregate_by_tier_and_task_group(tier, task_group)
        if agg is None or agg.total_count == 0:
            return 0.0
        return agg.failure_count / agg.total_count

    def tier_cost_of_pass(self, tier: str, task_group: str) -> float | None:
        agg = self._store.aggregate_by_tier_and_task_group(tier, task_group)
        if agg is None:
            return None
        return agg.cost_of_pass
