"""CostOfPassAccumulator: aggregates cost-of-pass from manual execution runs."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from ..usage_ledger import CostOfPassAggregate, UsageLedgerRow, aggregate_cost_of_pass


# ---------------------------------------------------------------------------
# Accumulator
# ---------------------------------------------------------------------------


class CostOfPassAccumulator:
    """Accumulates UsageLedgerRows and produces CostOfPassAggregates."""

    def __init__(self) -> None:
        self._rows: list[UsageLedgerRow] = []

    def add(self, row: UsageLedgerRow) -> None:
        self._rows.append(row)

    def aggregate_all(self) -> list[CostOfPassAggregate]:
        groups: dict[str, list] = {}
        for row in self._rows:
            groups.setdefault(row.cost_of_pass_group, []).append(row.to_dict())
        return [aggregate_cost_of_pass(rows) for rows in groups.values()]

    def aggregate_group(self, group: str) -> CostOfPassAggregate | None:
        for agg in self.aggregate_all():
            if agg.cost_of_pass_group == group:
                return agg
        return None

    def rows_for_group(self, group: str) -> list[UsageLedgerRow]:
        return [r for r in self._rows if r.cost_of_pass_group == group]

    def total_cost(self) -> float:
        return sum(r.estimated_cost for r in self._rows)

    def total_rows(self) -> int:
        return len(self._rows)

    def success_rate(self) -> float:
        if not self._rows:
            return 0.0
        successes = sum(1 for r in self._rows if r.pass_)
        return round(successes / len(self._rows), 4)
