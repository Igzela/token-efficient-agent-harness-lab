"""Work queue: stateless helper operating on WorkflowGraph as source of truth."""

from __future__ import annotations

from datetime import datetime, timezone

from .schemas import WorkflowGraph, WorkflowNode


class WorkQueue:
    """Stateless helper that reads/writes node status on WorkflowGraph."""

    def enqueue(self, graph: WorkflowGraph, node: WorkflowNode) -> WorkflowGraph:
        return graph

    def dequeue_ready(self, graph: WorkflowGraph) -> list[WorkflowNode]:
        return [n for n in graph.nodes if n.status == "ready"]

    def start(self, graph: WorkflowGraph, node_id: str) -> WorkflowGraph:
        return self._update_node(graph, node_id, "running")

    def complete(self, graph: WorkflowGraph, node_id: str, output_ref: str) -> WorkflowGraph:
        node = self._find_node(graph, node_id)
        if node is None:
            return graph
        now = datetime.now(timezone.utc).isoformat()
        updated = WorkflowNode(
            node_id=node.node_id,
            workflow_id=node.workflow_id,
            task_type=node.task_type,
            assigned_agent_id=node.assigned_agent_id,
            status="completed",
            input_refs=node.input_refs,
            output_ref=output_ref,
            budget=node.budget,
            cost_incurred=node.cost_incurred,
            error=None,
            created_at=node.created_at,
            started_at=node.started_at,
            completed_at=now,
            schema_version=node.schema_version,
        )
        return self._replace_node(graph, node_id, updated)

    def fail(self, graph: WorkflowGraph, node_id: str, error: str) -> WorkflowGraph:
        node = self._find_node(graph, node_id)
        if node is None:
            return graph
        now = datetime.now(timezone.utc).isoformat()
        updated = WorkflowNode(
            node_id=node.node_id,
            workflow_id=node.workflow_id,
            task_type=node.task_type,
            assigned_agent_id=node.assigned_agent_id,
            status="failed",
            input_refs=node.input_refs,
            output_ref=node.output_ref,
            budget=node.budget,
            cost_incurred=node.cost_incurred,
            error=error,
            created_at=node.created_at,
            started_at=node.started_at,
            completed_at=now,
            schema_version=node.schema_version,
        )
        return self._replace_node(graph, node_id, updated)

    def cancel(self, graph: WorkflowGraph, node_id: str) -> WorkflowGraph:
        node = self._find_node(graph, node_id)
        if node is None or node.status not in ("pending", "ready"):
            return graph
        return self._update_node(graph, node_id, "cancelled")

    def status_of(self, graph: WorkflowGraph, node_id: str) -> str:
        node = self._find_node(graph, node_id)
        return node.status if node else "pending"

    def reset(self) -> None:
        pass

    def _find_node(self, graph: WorkflowGraph, node_id: str) -> WorkflowNode | None:
        for node in graph.nodes:
            if node.node_id == node_id:
                return node
        return None

    def _update_node(self, graph: WorkflowGraph, node_id: str, status: str) -> WorkflowGraph:
        node = self._find_node(graph, node_id)
        if node is None:
            return graph
        now = datetime.now(timezone.utc).isoformat()
        updated = WorkflowNode(
            node_id=node.node_id,
            workflow_id=node.workflow_id,
            task_type=node.task_type,
            assigned_agent_id=node.assigned_agent_id,
            status=status,
            input_refs=node.input_refs,
            output_ref=node.output_ref,
            budget=node.budget,
            cost_incurred=node.cost_incurred,
            error=node.error,
            created_at=node.created_at,
            started_at=node.started_at or (now if status == "running" else None),
            completed_at=now if status in ("completed", "failed") else None,
            schema_version=node.schema_version,
        )
        return self._replace_node(graph, node_id, updated)

    def _replace_node(self, graph: WorkflowGraph, node_id: str, replacement: WorkflowNode) -> WorkflowGraph:
        updated_nodes = tuple(replacement if n.node_id == node_id else n for n in graph.nodes)
        return WorkflowGraph(
            workflow_id=graph.workflow_id,
            dispatch_id=graph.dispatch_id,
            nodes=updated_nodes,
            edges=graph.edges,
            status=graph.status,
            created_at=graph.created_at,
            updated_at=graph.updated_at,
            started_at=graph.started_at,
            completed_at=graph.completed_at,
            result=graph.result,
            schema_version=graph.schema_version,
        )
