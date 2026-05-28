"""Multi-agent budget manager: coordinates budgets across workflow nodes and agents."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass
class _WorkflowBudget:
    total_limit: float
    consumed: float = 0.0
    agent_limits: dict[str, float] = field(default_factory=dict)
    agent_consumed: dict[str, float] = field(default_factory=dict)
    node_reservations: dict[str, float] = field(default_factory=dict)


class MultiAgentBudgetManager:
    """Tracks and enforces budgets at workflow, agent, and node levels."""

    def __init__(self, overrun_strategy: str = "cancel") -> None:
        self._budgets: dict[str, _WorkflowBudget] = {}
        self.overrun_strategy = overrun_strategy

    def create_workflow_budget(self, workflow_id: str, total_limit: float) -> str:
        self._budgets[workflow_id] = _WorkflowBudget(total_limit=total_limit)
        return workflow_id

    def reserve_node_budget(
        self, workflow_id: str, node_id: str, agent_id: str, estimated_cost: float
    ) -> bool:
        budget = self._budgets.get(workflow_id)
        if budget is None:
            return False

        if budget.consumed + estimated_cost > budget.total_limit:
            return False

        agent_limit = budget.agent_limits.get(agent_id, float("inf"))
        agent_consumed = budget.agent_consumed.get(agent_id, 0.0)
        if agent_consumed + estimated_cost > agent_limit:
            return False

        budget.node_reservations[node_id] = estimated_cost
        return True

    def record_cost(self, workflow_id: str, node_id: str, agent_id: str, cost: float) -> None:
        budget = self._budgets.get(workflow_id)
        if budget is None:
            return
        budget.consumed += cost
        budget.agent_consumed[agent_id] = budget.agent_consumed.get(agent_id, 0.0) + cost

    def check_workflow_budget(self, workflow_id: str) -> tuple[bool, str | None]:
        budget = self._budgets.get(workflow_id)
        if budget is None:
            return True, None
        if budget.consumed > budget.total_limit:
            return False, f"workflow_budget_exceeded:{budget.consumed:.4f}/{budget.total_limit:.4f}"
        return True, None

    def check_agent_budget(self, workflow_id: str, agent_id: str) -> tuple[bool, str | None]:
        budget = self._budgets.get(workflow_id)
        if budget is None:
            return True, None
        limit = budget.agent_limits.get(agent_id, float("inf"))
        consumed = budget.agent_consumed.get(agent_id, 0.0)
        if consumed > limit:
            return False, f"agent_budget_exceeded:{agent_id}:{consumed:.4f}/{limit:.4f}"
        return True, None

    def set_agent_limit(self, workflow_id: str, agent_id: str, limit: float) -> None:
        budget = self._budgets.get(workflow_id)
        if budget is not None:
            budget.agent_limits[agent_id] = limit

    def get_workflow_cost(self, workflow_id: str) -> float:
        budget = self._budgets.get(workflow_id)
        return budget.consumed if budget else 0.0

    def get_agent_cost(self, workflow_id: str, agent_id: str) -> float:
        budget = self._budgets.get(workflow_id)
        if budget is None:
            return 0.0
        return budget.agent_consumed.get(agent_id, 0.0)
