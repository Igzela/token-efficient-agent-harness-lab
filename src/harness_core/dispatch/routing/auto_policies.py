"""Auto-downgrade and auto-upgrade routing policies."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from ..dispatch_decision import MODEL_TIERS
from .history_store import RoutingHistoryStore

_TIER_COST_ORDER: dict[str, int] = {t: i for i, t in enumerate(MODEL_TIERS)}


@dataclass(frozen=True)
class AutoDowngradePolicy:
    policy_id: str
    quality_risk_threshold: float = 0.1
    min_quality_score: float = 0.7
    min_sample_count: int = 30
    description: str = ""

    def should_downgrade(
        self,
        task_group: str,
        current_tier: str,
        candidate_tier: str,
        quality_score: float,
        cost_of_pass: float | None,
        history_store: RoutingHistoryStore,
    ) -> tuple[bool, str]:
        if quality_score < self.min_quality_score:
            return False, "quality_score_below_threshold"

        current_idx = _TIER_COST_ORDER.get(current_tier, 1)
        candidate_idx = _TIER_COST_ORDER.get(candidate_tier, 0)
        if candidate_idx >= current_idx:
            return False, "candidate_not_cheaper"

        sample_count = history_store.sample_count(task_group)
        if sample_count < self.min_sample_count:
            return False, "insufficient_samples"

        return True, "cost_optimization"


@dataclass(frozen=True)
class AutoUpgradePolicy:
    policy_id: str
    uncertainty_threshold: float = 0.4
    failure_rate_threshold: float = 0.2
    description: str = ""

    def should_upgrade(
        self,
        task_group: str,
        current_tier: str,
        candidate_tier: str,
        quality_score: float,
        failure_rate: float,
        risk_level: str,
        history_store: RoutingHistoryStore,
    ) -> tuple[bool, str]:
        current_idx = _TIER_COST_ORDER.get(current_tier, 1)
        candidate_idx = _TIER_COST_ORDER.get(candidate_tier, 2)
        if candidate_idx <= current_idx:
            return False, "candidate_not_stronger"

        if risk_level == "critical":
            return True, "critical_task"

        if failure_rate > self.failure_rate_threshold:
            return True, "failure_rate"

        if quality_score < self.uncertainty_threshold:
            return True, "high_uncertainty"

        return False, "no_upgrade_needed"
