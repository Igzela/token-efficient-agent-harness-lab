"""Tier-aware history store for routing decisions over UsageLedgerRows."""

from __future__ import annotations

from typing import Any

from ...usage_ledger import CostOfPassAggregate, UsageLedgerRow, aggregate_cost_of_pass, parse_cost_of_pass_group
from ..cost_of_pass import CostOfPassAccumulator


def _task_group_from_row(row: UsageLedgerRow) -> str | None:
    try:
        _, task_family, variant, _criterion = parse_cost_of_pass_group(row.cost_of_pass_group)
        return f"{task_family}_{variant}"
    except (ValueError, IndexError):
        return None


class RoutingHistoryStore:
    """Tier-aware indexing over UsageLedgerRows for routing decisions."""

    def __init__(
        self,
        cost_of_pass_accumulator: CostOfPassAccumulator | None = None,
        tier_profile_map: dict[str, str] | None = None,
    ) -> None:
        self._accumulator = cost_of_pass_accumulator or CostOfPassAccumulator()
        self._tier_map: dict[str, str] = dict(tier_profile_map) if tier_profile_map else {}
        self._group_cache: dict[str, list[UsageLedgerRow]] = {}
        self._tier_cache: dict[str, list[UsageLedgerRow]] = {}
        self._dirty = True

    def _invalidate(self) -> None:
        self._dirty = True

    def _rebuild_caches(self) -> None:
        if not self._dirty:
            return
        self._group_cache.clear()
        self._tier_cache.clear()
        for row in self._accumulator._rows:
            tg = _task_group_from_row(row)
            if tg:
                self._group_cache.setdefault(tg, []).append(row)
            tier = self._tier_map.get(row.model_profile_id, "")
            if tier:
                self._tier_cache.setdefault(tier, []).append(row)
        self._dirty = False

    def add_row(self, row: UsageLedgerRow) -> None:
        self._accumulator.add(row)
        self._invalidate()

    def set_tier_map(self, mapping: dict[str, str]) -> None:
        self._tier_map = dict(mapping)
        self._invalidate()

    def tier_for_profile(self, profile_id: str) -> str | None:
        return self._tier_map.get(profile_id)

    def rows_by_tier(self, tier: str) -> list[UsageLedgerRow]:
        self._rebuild_caches()
        return list(self._tier_cache.get(tier, []))

    def rows_by_task_group(self, task_group: str) -> list[UsageLedgerRow]:
        self._rebuild_caches()
        return list(self._group_cache.get(task_group, []))

    def rows_by_tier_and_task_group(self, tier: str, task_group: str) -> list[UsageLedgerRow]:
        self._rebuild_caches()
        tier_rows = self._tier_cache.get(tier, [])
        group_rows = self._group_cache.get(task_group, [])
        group_set = id  # use identity for fast membership
        group_ids = {id(r) for r in group_rows}
        return [r for r in tier_rows if id(r) in group_ids]

    def aggregate_by_tier(self, tier: str) -> CostOfPassAggregate | None:
        rows = self.rows_by_tier(tier)
        if not rows:
            return None
        return aggregate_cost_of_pass([r.to_dict() for r in rows])

    def aggregate_by_tier_and_task_group(self, tier: str, task_group: str) -> CostOfPassAggregate | None:
        rows = self.rows_by_tier_and_task_group(tier, task_group)
        if not rows:
            return None
        return aggregate_cost_of_pass([r.to_dict() for r in rows])

    def sample_count(self, task_group: str) -> int:
        return len(self.rows_by_task_group(task_group))

    def sample_count_for_tier(self, task_group: str, tier: str) -> int:
        return len(self.rows_by_tier_and_task_group(tier, task_group))

    def tiers_observed(self, task_group: str) -> tuple[str, ...]:
        self._rebuild_caches()
        group_rows = self._group_cache.get(task_group, [])
        tiers: set[str] = set()
        for row in group_rows:
            tier = self._tier_map.get(row.model_profile_id, "")
            if tier:
                tiers.add(tier)
        return tuple(sorted(tiers))

    def total_rows(self) -> int:
        return self._accumulator.total_rows()

    def all_rows(self) -> list[UsageLedgerRow]:
        self._rebuild_caches()
        return list(self._accumulator._rows)
