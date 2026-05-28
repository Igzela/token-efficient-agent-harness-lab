"""Task decomposer: breaks a TaskAnalysis into a WorkflowGraph."""

from __future__ import annotations

import uuid
from datetime import datetime, timezone

from ..task_analyzer import TaskAnalysis
from .agent_role_registry import AgentRoleRegistry
from .schemas import WorkflowEdge, WorkflowGraph, WorkflowNode


class TaskDecomposer:
    """Rule-based task decomposer. Produces a WorkflowGraph from a TaskAnalysis."""

    def __init__(self, role_registry: AgentRoleRegistry | None = None) -> None:
        self._registry = role_registry

    def decompose(self, analysis: TaskAnalysis, dispatch_id: str | None = None) -> WorkflowGraph:
        workflow_id = f"wf-{uuid.uuid4().hex[:12]}"
        now = datetime.now(timezone.utc).isoformat()

        nodes, edges = self._build_graph(workflow_id, analysis)

        return WorkflowGraph(
            workflow_id=workflow_id,
            dispatch_id=dispatch_id or analysis.analysis_id,
            nodes=tuple(nodes),
            edges=tuple(edges),
            status="decomposed",
            created_at=now,
            updated_at=now,
        )

    def _build_graph(
        self, workflow_id: str, analysis: TaskAnalysis
    ) -> tuple[list[WorkflowNode], list[WorkflowEdge]]:
        complexity = analysis.complexity_score

        if complexity < 0.3 and len(analysis.risk_flags) == 0:
            return self._simple_graph(workflow_id, analysis)

        if complexity >= 0.6 or len(analysis.risk_flags) >= 2:
            return self._complex_graph(workflow_id, analysis)

        return self._medium_graph(workflow_id, analysis)

    def _simple_graph(
        self, workflow_id: str, analysis: TaskAnalysis
    ) -> tuple[list[WorkflowNode], list[WorkflowEdge]]:
        node = self._make_node(workflow_id, analysis.task_domain, analysis)
        return [node], []

    def _medium_graph(
        self, workflow_id: str, analysis: TaskAnalysis
    ) -> tuple[list[WorkflowNode], list[WorkflowEdge]]:
        analyze = self._make_node(workflow_id, f"{analysis.task_domain}_analyze", analysis)
        execute = self._make_node(workflow_id, f"{analysis.task_domain}_execute", analysis, input_refs=(analyze.node_id,))
        edges = [WorkflowEdge(
            edge_id=f"edge-{uuid.uuid4().hex[:8]}",
            from_node_id=analyze.node_id,
            to_node_id=execute.node_id,
        )]
        return [analyze, execute], edges

    def _complex_graph(
        self, workflow_id: str, analysis: TaskAnalysis
    ) -> tuple[list[WorkflowNode], list[WorkflowEdge]]:
        analyze = self._make_node(workflow_id, f"{analysis.task_domain}_analyze", analysis)
        plan = self._make_node(workflow_id, f"{analysis.task_domain}_plan", analysis, input_refs=(analyze.node_id,))
        execute = self._make_node(workflow_id, f"{analysis.task_domain}_execute", analysis, input_refs=(plan.node_id,))
        review = self._make_node(workflow_id, f"{analysis.task_domain}_review", analysis, input_refs=(execute.node_id,))

        edges = [
            WorkflowEdge(edge_id=f"edge-{uuid.uuid4().hex[:8]}", from_node_id=analyze.node_id, to_node_id=plan.node_id),
            WorkflowEdge(edge_id=f"edge-{uuid.uuid4().hex[:8]}", from_node_id=plan.node_id, to_node_id=execute.node_id),
            WorkflowEdge(edge_id=f"edge-{uuid.uuid4().hex[:8]}", from_node_id=execute.node_id, to_node_id=review.node_id),
        ]
        return [analyze, plan, execute, review], edges

    def _make_node(
        self,
        workflow_id: str,
        task_type: str,
        analysis: TaskAnalysis,
        input_refs: tuple[str, ...] = (),
    ) -> WorkflowNode:
        node_id = f"node-{uuid.uuid4().hex[:8]}"

        agent_id = None
        if self._registry:
            agent_id = self._registry.assign_agent(workflow_id, node_id, task_type)

        budget = analysis.execution_budget_estimate / max(1, len(analysis.risk_flags) + 1)

        return WorkflowNode(
            node_id=node_id,
            workflow_id=workflow_id,
            task_type=task_type,
            assigned_agent_id=agent_id,
            input_refs=input_refs,
            budget=round(budget, 2),
            created_at=datetime.now(timezone.utc).isoformat(),
        )
