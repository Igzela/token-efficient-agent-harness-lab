"""Workflow engine: orchestrates multi-agent workflow lifecycle."""

from __future__ import annotations

from datetime import datetime, timezone
from typing import Any

from ..task_analyzer import TaskAnalysis
from .conflict_resolver import ConflictResolver
from .dependency_resolver import DependencyResolver
from .human_approval_gate import HumanApprovalGate
from .multi_agent_budget import MultiAgentBudgetManager
from .result_aggregator import ResultAggregator
from .schemas import WorkflowGraph, WorkflowNode
from .task_decomposer import TaskDecomposer
from .work_queue import WorkQueue


class WorkflowEngine:
    """Orchestrates the full workflow lifecycle: decompose, execute, resolve, aggregate."""

    def __init__(
        self,
        decomposer: TaskDecomposer | None = None,
        resolver: DependencyResolver | None = None,
        queue: WorkQueue | None = None,
        conflict_resolver: ConflictResolver | None = None,
        aggregator: ResultAggregator | None = None,
        approval_gate: HumanApprovalGate | None = None,
        budget_manager: MultiAgentBudgetManager | None = None,
    ) -> None:
        self._decomposer = decomposer or TaskDecomposer()
        self._resolver = resolver or DependencyResolver()
        self._queue = queue or WorkQueue()
        self._conflict_resolver = conflict_resolver or ConflictResolver()
        self._aggregator = aggregator or ResultAggregator()
        self._approval_gate = approval_gate or HumanApprovalGate()
        self._budget_manager = budget_manager or MultiAgentBudgetManager()

    def create_workflow(self, analysis: TaskAnalysis, budget_limit: float = 100.0) -> WorkflowGraph:
        graph = self._decomposer.decompose(analysis)

        valid, errors = self._resolver.validate(graph)
        if not valid:
            graph = graph._replace(status="failed") if hasattr(graph, '_replace') else WorkflowGraph(
                workflow_id=graph.workflow_id,
                dispatch_id=graph.dispatch_id,
                nodes=graph.nodes,
                edges=graph.edges,
                status="failed",
                created_at=graph.created_at,
                schema_version=graph.schema_version,
            )

        self._budget_manager.create_workflow_budget(graph.workflow_id, budget_limit)

        for node in graph.nodes:
            self._queue.enqueue(node)

        return graph

    def tick(self, graph: WorkflowGraph) -> WorkflowGraph:
        if graph.status in ("completed", "failed", "cancelled"):
            return graph

        graph = self._start_ready_nodes(graph)
        graph = self._check_completions(graph)

        if self._aggregator.is_complete(graph):
            conflicts = self._conflict_resolver.detect_conflicts(graph)
            unresolved = [c for c in conflicts if c.resolution_strategy is None]
            if unresolved:
                graph = self._resolve_conflicts(graph, unresolved)

            needs_approval = self._check_approval_needed(graph)
            if needs_approval:
                return self._set_status(graph, "waiting_human")

            result = self._aggregator.aggregate(graph)
            return WorkflowGraph(
                workflow_id=graph.workflow_id,
                dispatch_id=graph.dispatch_id,
                nodes=graph.nodes,
                edges=graph.edges,
                status="completed",
                created_at=graph.created_at,
                started_at=graph.started_at,
                completed_at=datetime.now(timezone.utc).isoformat(),
                result=result,
                schema_version=graph.schema_version,
            )

        if graph.status in ("created", "decomposed"):
            return self._set_status(graph, "running")

        return graph

    def resume_after_approval(self, graph: WorkflowGraph, node_id: str) -> WorkflowGraph:
        if graph.status != "waiting_human":
            return graph

        self._approval_gate.approve(node_id)
        return self._set_status(graph, "running")

    def reject_approval(self, graph: WorkflowGraph, node_id: str, reason: str) -> WorkflowGraph:
        self._approval_gate.reject(node_id, reason)
        return self._set_status(graph, "cancelled")

    def cancel(self, graph: WorkflowGraph) -> WorkflowGraph:
        for node in graph.nodes:
            if node.status in ("pending", "ready"):
                self._queue.cancel(node.node_id)
        return self._set_status(graph, "cancelled")

    def complete_node(self, graph: WorkflowGraph, node_id: str, output_ref: str, cost: float = 0.0) -> WorkflowGraph:
        self._queue.complete(node_id, output_ref)
        self._budget_manager.record_cost(graph.workflow_id, node_id, "", cost)
        return self._update_node_status(graph, node_id, "completed", output_ref=output_ref, cost_incurred=cost)

    def fail_node(self, graph: WorkflowGraph, node_id: str, error: str) -> WorkflowGraph:
        self._queue.fail(node_id, error)
        return self._update_node_status(graph, node_id, "failed", error=error)

    def _start_ready_nodes(self, graph: WorkflowGraph) -> WorkflowGraph:
        ready_ids = self._resolver.ready_nodes(graph)
        for node_id in ready_ids:
            self._queue.start(node_id)
            graph = self._update_node_status(graph, node_id, "ready")
        return graph

    def _check_completions(self, graph: WorkflowGraph) -> WorkflowGraph:
        return graph

    def _check_approval_needed(self, graph: WorkflowGraph) -> bool:
        for node in graph.nodes:
            if node.status == "completed" and self._approval_gate.requires_approval(graph, node):
                return True
        return False

    def _resolve_conflicts(self, graph: WorkflowGraph, conflicts: list) -> WorkflowGraph:
        for conflict in conflicts:
            self._conflict_resolver.resolve(conflict)
        return graph

    def _set_status(self, graph: WorkflowGraph, status: str) -> WorkflowGraph:
        now = datetime.now(timezone.utc).isoformat()
        started = graph.started_at if graph.started_at else (now if status == "running" else None)
        return WorkflowGraph(
            workflow_id=graph.workflow_id,
            dispatch_id=graph.dispatch_id,
            nodes=graph.nodes,
            edges=graph.edges,
            status=status,
            created_at=graph.created_at,
            started_at=started,
            completed_at=now if status in ("completed", "failed", "cancelled") else None,
            result=graph.result,
            schema_version=graph.schema_version,
        )

    def _update_node_status(
        self, graph: WorkflowGraph, node_id: str, status: str,
        output_ref: str | None = None, error: str | None = None, cost_incurred: float = 0.0,
    ) -> WorkflowGraph:
        now = datetime.now(timezone.utc).isoformat()
        updated_nodes: list[WorkflowNode] = []
        for node in graph.nodes:
            if node.node_id == node_id:
                updated_nodes.append(WorkflowNode(
                    node_id=node.node_id,
                    workflow_id=node.workflow_id,
                    task_type=node.task_type,
                    assigned_agent_id=node.assigned_agent_id,
                    status=status,
                    input_refs=node.input_refs,
                    output_ref=output_ref if output_ref is not None else node.output_ref,
                    budget=node.budget,
                    cost_incurred=cost_incurred if cost_incurred > 0 else node.cost_incurred,
                    error=error if error is not None else node.error,
                    created_at=node.created_at,
                    started_at=node.started_at or (now if status == "running" else None),
                    completed_at=now if status in ("completed", "failed") else None,
                    schema_version=node.schema_version,
                ))
            else:
                updated_nodes.append(node)
        return WorkflowGraph(
            workflow_id=graph.workflow_id,
            dispatch_id=graph.dispatch_id,
            nodes=tuple(updated_nodes),
            edges=graph.edges,
            status=graph.status,
            created_at=graph.created_at,
            started_at=graph.started_at,
            completed_at=graph.completed_at,
            result=graph.result,
            schema_version=graph.schema_version,
        )
