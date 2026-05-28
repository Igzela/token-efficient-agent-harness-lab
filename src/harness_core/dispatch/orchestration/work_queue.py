"""Work queue: manages pending and in-progress workflow nodes."""

from __future__ import annotations

from datetime import datetime, timezone

from .schemas import WorkflowGraph, WorkflowNode


class WorkQueue:
    """In-memory queue for tracking node execution state."""

    def __init__(self) -> None:
        self._node_status: dict[str, str] = {}  # node_id -> status

    def enqueue(self, node: WorkflowNode) -> None:
        self._node_status[node.node_id] = "ready"

    def dequeue_ready(self, graph: WorkflowGraph) -> list[WorkflowNode]:
        ready = []
        for node in graph.nodes:
            if self._node_status.get(node.node_id) == "ready" and node.status == "pending":
                ready.append(node)
        return ready

    def start(self, node_id: str) -> None:
        if self._node_status.get(node_id) == "ready":
            self._node_status[node_id] = "running"

    def complete(self, node_id: str, output_ref: str) -> None:
        self._node_status[node_id] = "completed"

    def fail(self, node_id: str, error: str) -> None:
        self._node_status[node_id] = "failed"

    def cancel(self, node_id: str) -> None:
        status = self._node_status.get(node_id, "pending")
        if status in ("pending", "ready"):
            self._node_status[node_id] = "cancelled"

    def status_of(self, node_id: str) -> str:
        return self._node_status.get(node_id, "pending")

    def reset(self) -> None:
        self._node_status.clear()
