"""Human approval gate: checkpoints for human review during workflow execution."""

from __future__ import annotations

from .schemas import WorkflowGraph, WorkflowNode


class HumanApprovalGate:
    """Determines when human approval is needed and tracks approval state."""

    def __init__(self, risk_threshold: float = 0.7) -> None:
        self._risk_threshold = risk_threshold
        self._approved: set[str] = set()
        self._rejected: dict[str, str] = {}  # node_id -> reason

    def requires_approval(self, graph: WorkflowGraph, node: WorkflowNode) -> bool:
        if node.node_id in self._approved:
            return False
        if node.node_id in self._rejected:
            return False

        if node.budget > 0 and node.cost_incurred > node.budget * self._risk_threshold:
            return True

        if node.status == "failed":
            return True

        return False

    def approve(self, node_id: str) -> bool:
        if node_id in self._rejected:
            return False
        self._approved.add(node_id)
        return True

    def reject(self, node_id: str, reason: str) -> bool:
        if node_id in self._approved:
            return False
        self._rejected[node_id] = reason
        return True

    def is_approved(self, node_id: str) -> bool:
        return node_id in self._approved

    def is_rejected(self, node_id: str) -> bool:
        return node_id in self._rejected

    def rejection_reason(self, node_id: str) -> str | None:
        return self._rejected.get(node_id)
