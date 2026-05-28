"""Workflow engine: orchestrates multi-agent workflow lifecycle."""

from __future__ import annotations

from datetime import datetime, timezone
from typing import Any

from ..dispatch_ledger import DispatchBundle
from ..task_analyzer import TaskAnalysis
from .agent_role_registry import AgentRoleRegistry
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
        role_registry: AgentRoleRegistry | None = None,
    ) -> None:
        self._decomposer = decomposer or TaskDecomposer()
        self._resolver = resolver or DependencyResolver()
        self._queue = queue or WorkQueue()
        self._conflict_resolver = conflict_resolver or ConflictResolver()
        self._aggregator = aggregator or ResultAggregator()
        self._approval_gate = approval_gate or HumanApprovalGate()
        self._budget_manager = budget_manager or MultiAgentBudgetManager()
        self._role_registry = role_registry or getattr(self._decomposer, '_registry', None)

    def create_workflow(
        self,
        analysis: TaskAnalysis,
        budget_limit: float = 100.0,
        dispatch_bundle: DispatchBundle | None = None,
        dispatch_id: str | None = None,
        decision_status: str = "decided",
    ) -> WorkflowGraph:
        dispatch_id, decision_status = self._extract_dispatch_context(
            dispatch_bundle, dispatch_id, decision_status,
        )
        if decision_status != "decided":
            raise ValueError(
                f"Cannot create workflow: decision_status={decision_status!r}, requires 'decided'"
            )

        graph = self._decomposer.decompose(analysis, dispatch_id=dispatch_id)

        valid, errors = self._resolver.validate(graph)
        if not valid:
            graph = self._set_status(graph, "failed")

        self._budget_manager.create_workflow_budget(graph.workflow_id, budget_limit)

        for node in graph.nodes:
            self._queue.enqueue(graph, node)

        return graph

    def tick(self, graph: WorkflowGraph) -> WorkflowGraph:
        if graph.status in ("completed", "failed", "cancelled"):
            return graph

        graph = self._start_ready_nodes(graph)

        if self._aggregator.is_complete(graph):
            has_failed = any(n.status in ("failed", "cancelled") for n in graph.nodes)
            if has_failed:
                graph = self._handle_failed_nodes(graph)

            conflicts = self._conflict_resolver.detect_conflicts(graph)
            unresolved = [c for c in conflicts if c.resolution_strategy is None]
            if unresolved:
                graph = self._resolve_conflicts(graph, unresolved)
                if graph.status == "cancelled":
                    return graph

            needs_approval = self._check_approval_needed(graph)
            if needs_approval:
                return self._set_status(graph, "waiting_human")

            if has_failed and graph.status not in ("failed", "cancelled"):
                return self._set_status(graph, "failed")

            result = self._aggregator.aggregate(graph)
            return WorkflowGraph(
                workflow_id=graph.workflow_id,
                dispatch_id=graph.dispatch_id,
                nodes=graph.nodes,
                edges=graph.edges,
                status="completed",
                created_at=graph.created_at,
                updated_at=graph.updated_at,
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
                graph = self._queue.cancel(graph, node.node_id)
                self._release_agent(node)
        return self._set_status(graph, "cancelled")

    def complete_node(self, graph: WorkflowGraph, node_id: str, output_ref: str, cost: float = 0.0) -> WorkflowGraph:
        graph = self._queue.complete(graph, node_id, output_ref)

        node = self._find_node(graph, node_id)
        if node and cost > 0:
            agent_id = node.assigned_agent_id or ""
            self._budget_manager.record_cost(graph.workflow_id, node_id, agent_id, cost)

            ok, msg = self._budget_manager.check_workflow_budget(graph.workflow_id)
            if not ok and self._budget_manager.overrun_strategy == "cancel":
                return self._set_status(graph, "failed")

        graph = self._update_node_fields(graph, node_id, cost_incurred=cost)

        if node:
            self._release_agent(node)

        return graph

    def fail_node(self, graph: WorkflowGraph, node_id: str, error: str) -> WorkflowGraph:
        graph = self._queue.fail(graph, node_id, error)

        node = self._find_node(graph, node_id)
        if node:
            self._release_agent(node)

        return graph

    def _start_ready_nodes(self, graph: WorkflowGraph) -> WorkflowGraph:
        ready_ids = self._resolver.ready_nodes(graph)
        for node_id in ready_ids:
            graph = self._queue.start(graph, node_id)
        return graph

    def _check_approval_needed(self, graph: WorkflowGraph) -> bool:
        for node in graph.nodes:
            if node.status in ("completed", "failed", "waiting_human"):
                if self._approval_gate.requires_approval(graph, node):
                    return True
        return False

    def _handle_failed_nodes(self, graph: WorkflowGraph) -> WorkflowGraph:
        failed_or_cancelled = [n for n in graph.nodes if n.status in ("failed", "cancelled")]
        if not failed_or_cancelled:
            return graph
        for node in failed_or_cancelled:
            if self._approval_gate.requires_approval(graph, node):
                return self._set_status(graph, "waiting_human")
        return graph

    def _resolve_conflicts(self, graph: WorkflowGraph, conflicts: list) -> WorkflowGraph:
        for conflict in conflicts:
            resolved = self._conflict_resolver.resolve(conflict)
            if resolved.resolution_result == "workflow_cancelled":
                return self._set_status(graph, "cancelled")
        return graph

    def _release_agent(self, node: WorkflowNode) -> None:
        if self._role_registry and node.assigned_agent_id:
            self._role_registry.release_node(node.workflow_id, node.node_id)

    def _extract_dispatch_context(
        self,
        bundle: DispatchBundle | None,
        dispatch_id: str | None,
        decision_status: str,
    ) -> tuple[str | None, str]:
        if bundle is not None:
            return bundle.record.dispatch_id, bundle.decision.decision_status
        return dispatch_id, decision_status

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
            updated_at=now,
            started_at=started,
            completed_at=now if status in ("completed", "failed", "cancelled") else None,
            result=graph.result,
            schema_version=graph.schema_version,
        )

    def _find_node(self, graph: WorkflowGraph, node_id: str) -> WorkflowNode | None:
        for node in graph.nodes:
            if node.node_id == node_id:
                return node
        return None

    def _update_node_fields(
        self, graph: WorkflowGraph, node_id: str,
        cost_incurred: float = 0.0,
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
                    status=node.status,
                    input_refs=node.input_refs,
                    output_ref=node.output_ref,
                    budget=node.budget,
                    cost_incurred=cost_incurred if cost_incurred > 0 else node.cost_incurred,
                    error=node.error,
                    created_at=node.created_at,
                    started_at=node.started_at,
                    completed_at=node.completed_at,
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
            updated_at=graph.updated_at,
            started_at=graph.started_at,
            completed_at=graph.completed_at,
            result=graph.result,
            schema_version=graph.schema_version,
        )
