"""Budget manager for pre-execution token/cost reservations."""

from __future__ import annotations

import uuid
from datetime import datetime, timezone
from typing import Any

from .dispatch_decision import BudgetReservation
from .task_analyzer import TaskAnalysis

# ---------------------------------------------------------------------------
# Tier cost estimates (tokens per unit)
# ---------------------------------------------------------------------------

_TIER_COST_PER_1K_INPUT: dict[str, float] = {
    "cheap_executor": 0.0005,
    "balanced_worker": 0.003,
    "strong_planner": 0.015,
    "verifier": 0.003,
    "advisor": 0.015,
}

_TIER_COST_PER_1K_OUTPUT: dict[str, float] = {
    "cheap_executor": 0.0015,
    "balanced_worker": 0.015,
    "strong_planner": 0.075,
    "verifier": 0.015,
    "advisor": 0.075,
}


class BudgetManager:
    """Pre-execution budget reservation manager.

    Does NOT track actual spend (that is usage_ledger's job).
    """

    def __init__(self, default_currency: str = "token"):
        self._default_currency = default_currency

    def create_reservation(
        self,
        decision_id: str,
        analysis: TaskAnalysis,
        tier: str,
    ) -> BudgetReservation:
        input_tokens = analysis.context_budget_estimate
        output_tokens = analysis.execution_budget_estimate
        total_tokens = input_tokens + output_tokens
        cost = self.estimate_cost(tier, input_tokens, output_tokens)

        now = datetime.now(timezone.utc).isoformat()
        return BudgetReservation(
            reservation_id=f"res-{uuid.uuid4().hex[:12]}",
            decision_id=decision_id,
            currency=self._default_currency,
            pre_budget=total_tokens,
            reserved_input_tokens=input_tokens,
            reserved_output_tokens=output_tokens,
            reserved_total_tokens=total_tokens,
            reserved_cost=round(cost, 6),
            status="reserved",
            created_at=now,
            updated_at=now,
        )

    def check_violation(
        self, reservation: BudgetReservation, actual_tokens: int
    ) -> tuple[bool, str | None]:
        if actual_tokens > reservation.reserved_total_tokens:
            delta = actual_tokens - reservation.reserved_total_tokens
            return True, f"budget exceeded by {delta} tokens"
        return False, None

    def estimate_cost(self, tier: str, input_tokens: int, output_tokens: int) -> float:
        input_rate = _TIER_COST_PER_1K_INPUT.get(tier, 0.003)
        output_rate = _TIER_COST_PER_1K_OUTPUT.get(tier, 0.015)
        return (input_tokens / 1000.0 * input_rate) + (output_tokens / 1000.0 * output_rate)
