"""Result aggregator: combines sub-task outputs into a final workflow result."""

from __future__ import annotations

from typing import Any

from .schemas import WorkflowGraph


class ResultAggregator:
    """Merges completed node outputs into a single result dict."""

    def is_complete(self, graph: WorkflowGraph) -> bool:
        return all(n.status in ("completed", "failed", "cancelled") for n in graph.nodes)

    def aggregate(self, graph: WorkflowGraph) -> dict[str, Any]:
        node_results: dict[str, Any] = {}
        for node in graph.nodes:
            node_results[node.node_id] = {
                "task_type": node.task_type,
                "status": node.status,
                "output_ref": node.output_ref,
                "error": node.error,
                "agent_id": node.assigned_agent_id,
            }

        total_cost = sum(n.cost_incurred for n in graph.nodes)
        completed_count = sum(1 for n in graph.nodes if n.status == "completed")
        failed_count = sum(1 for n in graph.nodes if n.status == "failed")

        return {
            "workflow_id": graph.workflow_id,
            "dispatch_id": graph.dispatch_id,
            "total_nodes": len(graph.nodes),
            "completed_nodes": completed_count,
            "failed_nodes": failed_count,
            "total_cost": total_cost,
            "node_results": node_results,
        }
