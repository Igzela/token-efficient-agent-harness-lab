"""Dynamic DAG Manager for Stage 4 — mutation protocol with approval gates and rollback."""

from __future__ import annotations

import hashlib
from dataclasses import dataclass, field
from typing import Any


@dataclass(frozen=True)
class DAGNode:
    node_id: str
    task_id: str | None
    node_type: str  # "task" | "gate" | "decision" | "merge"
    status: str  # "pending" | "running" | "completed" | "failed" | "skipped" | "cancelled"
    tier: str
    metadata: dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class DAGEdge:
    edge_id: str
    from_node: str
    to_node: str
    dependency_type: str  # "hard" | "soft" | "artifact" | "approval"
    status: str = "pending"  # "pending" | "satisfied" | "violated"


@dataclass(frozen=True)
class DAGState:
    dag_id: str
    version: int
    nodes: tuple[DAGNode, ...]
    edges: tuple[DAGEdge, ...]
    created_at: str
    updated_at: str


@dataclass(frozen=True)
class DAGMutationProposal:
    proposal_id: str
    dag_id: str
    mutation_type: str  # "add_node" | "remove_node" | "add_edge" | "remove_edge" | "rewire_edge" | "update_node"
    target_node_id: str | None = None
    target_edge_id: str | None = None
    payload: dict[str, Any] = field(default_factory=dict)
    reason: str = ""
    requires_approval: bool = False
    status: str = "pending"  # "pending" | "approved" | "rejected" | "applied" | "rolled_back"


@dataclass(frozen=True)
class DAGMutationResult:
    proposal_id: str
    applied: bool
    new_dag_version: int
    rolled_back: bool
    errors: tuple[str, ...] = ()


def _has_cycle(nodes: tuple[DAGNode, ...], edges: tuple[DAGEdge, ...]) -> bool:
    adj: dict[str, list[str]] = {n.node_id: [] for n in nodes}
    for e in edges:
        if e.from_node in adj:
            adj[e.from_node].append(e.to_node)

    WHITE, GRAY, BLACK = 0, 1, 2
    color: dict[str, int] = {n.node_id: WHITE for n in nodes}

    def dfs(node: str) -> bool:
        color[node] = GRAY
        for neighbor in adj.get(node, []):
            if color.get(neighbor) == GRAY:
                return True
            if color.get(neighbor) == WHITE and dfs(neighbor):
                return True
        color[node] = BLACK
        return False

    for n in sorted(nodes, key=lambda x: x.node_id):
        if color[n.node_id] == WHITE and dfs(n.node_id):
            return True
    return False


def _find_node(nodes: tuple[DAGNode, ...], node_id: str) -> DAGNode | None:
    for n in nodes:
        if n.node_id == node_id:
            return n
    return None


def _find_edge(edges: tuple[DAGEdge, ...], edge_id: str) -> DAGEdge | None:
    for e in edges:
        if e.edge_id == edge_id:
            return e
    return None


def _requires_approval(proposal: DAGMutationProposal, state: DAGState) -> bool:
    if proposal.requires_approval:
        return True
    if proposal.mutation_type == "remove_node":
        node = _find_node(state.nodes, proposal.target_node_id or "")
        if node and node.status in ("running", "completed"):
            return True
    if proposal.mutation_type == "rewire_edge":
        edge = _find_edge(state.edges, proposal.target_edge_id or "")
        if edge:
            from_node = _find_node(state.nodes, edge.from_node)
            if from_node and from_node.status == "completed":
                return True
    return False


def _apply_add_node(state: DAGState, proposal: DAGMutationProposal) -> DAGState:
    payload = proposal.payload
    new_node = DAGNode(
        node_id=payload["node_id"],
        task_id=payload.get("task_id"),
        node_type=payload.get("node_type", "task"),
        status=payload.get("status", "pending"),
        tier=payload.get("tier", "cheap_executor"),
        metadata=payload.get("metadata", {}),
    )
    if _find_node(state.nodes, new_node.node_id):
        raise ValueError(f"node {new_node.node_id} already exists")
    return DAGState(
        dag_id=state.dag_id,
        version=state.version + 1,
        nodes=state.nodes + (new_node,),
        edges=state.edges,
        created_at=state.created_at,
        updated_at=proposal.proposal_id,
    )


def _apply_remove_node(state: DAGState, proposal: DAGMutationProposal) -> DAGState:
    node_id = proposal.target_node_id
    if not _find_node(state.nodes, node_id):
        raise ValueError(f"node {node_id} not found")
    connected_edges = [e for e in state.edges if e.from_node == node_id or e.to_node == node_id]
    if connected_edges:
        raise ValueError(f"node {node_id} has {len(connected_edges)} connected edges; remove them first")
    return DAGState(
        dag_id=state.dag_id,
        version=state.version + 1,
        nodes=tuple(n for n in state.nodes if n.node_id != node_id),
        edges=state.edges,
        created_at=state.created_at,
        updated_at=proposal.proposal_id,
    )


def _apply_add_edge(state: DAGState, proposal: DAGMutationProposal) -> DAGState:
    payload = proposal.payload
    new_edge = DAGEdge(
        edge_id=payload["edge_id"],
        from_node=payload["from_node"],
        to_node=payload["to_node"],
        dependency_type=payload.get("dependency_type", "hard"),
        status=payload.get("status", "pending"),
    )
    if not _find_node(state.nodes, new_edge.from_node):
        raise ValueError(f"from_node {new_edge.from_node} not found")
    if not _find_node(state.nodes, new_edge.to_node):
        raise ValueError(f"to_node {new_edge.to_node} not found")
    if _find_edge(state.edges, new_edge.edge_id):
        raise ValueError(f"edge {new_edge.edge_id} already exists")
    test_edges = state.edges + (new_edge,)
    if _has_cycle(state.nodes, test_edges):
        raise ValueError(f"adding edge {new_edge.edge_id} would create a cycle")
    return DAGState(
        dag_id=state.dag_id,
        version=state.version + 1,
        nodes=state.nodes,
        edges=test_edges,
        created_at=state.created_at,
        updated_at=proposal.proposal_id,
    )


def _apply_remove_edge(state: DAGState, proposal: DAGMutationProposal) -> DAGState:
    edge_id = proposal.target_edge_id
    if not _find_edge(state.edges, edge_id):
        raise ValueError(f"edge {edge_id} not found")
    return DAGState(
        dag_id=state.dag_id,
        version=state.version + 1,
        nodes=state.nodes,
        edges=tuple(e for e in state.edges if e.edge_id != edge_id),
        created_at=state.created_at,
        updated_at=proposal.proposal_id,
    )


def _apply_rewire_edge(state: DAGState, proposal: DAGMutationProposal) -> DAGState:
    payload = proposal.payload
    edge = _find_edge(state.edges, proposal.target_edge_id)
    if not edge:
        raise ValueError(f"edge {proposal.target_edge_id} not found")
    new_from = payload.get("from_node", edge.from_node)
    new_to = payload.get("to_node", edge.to_node)
    if not _find_node(state.nodes, new_from):
        raise ValueError(f"from_node {new_from} not found")
    if not _find_node(state.nodes, new_to):
        raise ValueError(f"to_node {new_to} not found")
    rewired = DAGEdge(
        edge_id=edge.edge_id,
        from_node=new_from,
        to_node=new_to,
        dependency_type=payload.get("dependency_type", edge.dependency_type),
        status=edge.status,
    )
    other_edges = tuple(e for e in state.edges if e.edge_id != edge.edge_id)
    test_edges = other_edges + (rewired,)
    if _has_cycle(state.nodes, test_edges):
        raise ValueError(f"rewiring edge {edge.edge_id} would create a cycle")
    return DAGState(
        dag_id=state.dag_id,
        version=state.version + 1,
        nodes=state.nodes,
        edges=test_edges,
        created_at=state.created_at,
        updated_at=proposal.proposal_id,
    )


def _apply_update_node(state: DAGState, proposal: DAGMutationProposal) -> DAGState:
    payload = proposal.payload
    node = _find_node(state.nodes, proposal.target_node_id)
    if not node:
        raise ValueError(f"node {proposal.target_node_id} not found")
    updated = DAGNode(
        node_id=node.node_id,
        task_id=payload.get("task_id", node.task_id),
        node_type=payload.get("node_type", node.node_type),
        status=payload.get("status", node.status),
        tier=payload.get("tier", node.tier),
        metadata=payload.get("metadata", node.metadata),
    )
    return DAGState(
        dag_id=state.dag_id,
        version=state.version + 1,
        nodes=tuple(updated if n.node_id == node.node_id else n for n in state.nodes),
        edges=state.edges,
        created_at=state.created_at,
        updated_at=proposal.proposal_id,
    )


_APPLY_MAP = {
    "add_node": _apply_add_node,
    "remove_node": _apply_remove_node,
    "add_edge": _apply_add_edge,
    "remove_edge": _apply_remove_edge,
    "rewire_edge": _apply_rewire_edge,
    "update_node": _apply_update_node,
}


def _compensate(proposal: DAGMutationProposal) -> DAGMutationProposal:
    inv = {
        "add_node": "remove_node",
        "remove_node": "add_node",
        "add_edge": "remove_edge",
        "remove_edge": "add_edge",
        "rewire_edge": "rewire_edge",
        "update_node": "update_node",
    }
    return DAGMutationProposal(
        proposal_id=f"comp_{proposal.proposal_id}",
        dag_id=proposal.dag_id,
        mutation_type=inv[proposal.mutation_type],
        target_node_id=proposal.target_node_id,
        target_edge_id=proposal.target_edge_id,
        payload=proposal.payload,
        reason=f"compensate {proposal.proposal_id}",
    )


class DAGManager:
    """Manage a DAG with mutations, approval gates, and rollback."""

    def __init__(self, dag_id: str, timestamp: str = "2026-01-01T00:00:00Z"):
        self._dag_id = dag_id
        self._state = DAGState(
            dag_id=dag_id,
            version=0,
            nodes=(),
            edges=(),
            created_at=timestamp,
            updated_at=timestamp,
        )
        self._history: list[tuple[DAGState, DAGMutationProposal]] = []

    @property
    def state(self) -> DAGState:
        return self._state

    def current_state(self) -> DAGState:
        return self._state

    def apply_mutation(self, proposal: DAGMutationProposal) -> DAGMutationResult:
        if proposal.dag_id != self._dag_id:
            return DAGMutationResult(
                proposal_id=proposal.proposal_id,
                applied=False,
                new_dag_version=self._state.version,
                rolled_back=False,
                errors=(f"dag_id mismatch: expected {self._dag_id}, got {proposal.dag_id}",),
            )

        if proposal.mutation_type not in _APPLY_MAP:
            return DAGMutationResult(
                proposal_id=proposal.proposal_id,
                applied=False,
                new_dag_version=self._state.version,
                rolled_back=False,
                errors=(f"unknown mutation_type: {proposal.mutation_type}",),
            )

        if _requires_approval(proposal, self._state):
            return DAGMutationResult(
                proposal_id=proposal.proposal_id,
                applied=False,
                new_dag_version=self._state.version,
                rolled_back=False,
                errors=("mutation requires approval; set status=approved to proceed",),
            )

        try:
            new_state = _APPLY_MAP[proposal.mutation_type](self._state, proposal)
        except (ValueError, KeyError) as exc:
            return DAGMutationResult(
                proposal_id=proposal.proposal_id,
                applied=False,
                new_dag_version=self._state.version,
                rolled_back=False,
                errors=(str(exc),),
            )

        self._history.append((self._state, proposal))
        self._state = new_state
        return DAGMutationResult(
            proposal_id=proposal.proposal_id,
            applied=True,
            new_dag_version=new_state.version,
            rolled_back=False,
        )

    def rollback(self, to_version: int) -> DAGState:
        while self._state.version > to_version and self._history:
            prev_state, _prev_proposal = self._history.pop()
            self._state = prev_state
        return self._state

    def validate_dag(self) -> list[str]:
        errors: list[str] = []
        node_ids = {n.node_id for n in self._state.nodes}
        for e in self._state.edges:
            if e.from_node not in node_ids:
                errors.append(f"edge {e.edge_id}: from_node {e.from_node} not found")
            if e.to_node not in node_ids:
                errors.append(f"edge {e.edge_id}: to_node {e.to_node} not found")
        if _has_cycle(self._state.nodes, self._state.edges):
            errors.append("DAG contains a cycle")
        return errors

    def topological_order(self) -> tuple[str, ...]:
        adj: dict[str, list[str]] = {n.node_id: [] for n in self._state.nodes}
        in_degree: dict[str, int] = {n.node_id: 0 for n in self._state.nodes}
        for e in self._state.edges:
            adj[e.from_node].append(e.to_node)
            in_degree[e.to_node] = in_degree.get(e.to_node, 0) + 1

        queue = sorted(n.node_id for n in self._state.nodes if in_degree[n.node_id] == 0)
        result: list[str] = []
        while queue:
            node = queue.pop(0)
            result.append(node)
            for neighbor in sorted(adj[node]):
                in_degree[neighbor] -= 1
                if in_degree[neighbor] == 0:
                    queue.append(neighbor)
        return tuple(result)

    def nodes_ready(self, completed: frozenset[str]) -> tuple[DAGNode, ...]:
        ready: list[DAGNode] = []
        for node in self._state.nodes:
            if node.node_id in completed or node.status != "pending":
                continue
            predecessors = [
                e.from_node for e in self._state.edges if e.to_node == node.node_id
            ]
            if all(p in completed for p in predecessors):
                ready.append(node)
        return tuple(sorted(ready, key=lambda n: n.node_id))

    def path_between(self, from_node: str, to_node: str) -> tuple[str, ...] | None:
        adj: dict[str, list[str]] = {n.node_id: [] for n in self._state.nodes}
        for e in self._state.edges:
            adj[e.from_node].append(e.to_node)

        visited: set[str] = set()
        path: list[str] = []

        def dfs(current: str) -> bool:
            if current == to_node:
                path.append(current)
                return True
            visited.add(current)
            for neighbor in sorted(adj.get(current, [])):
                if neighbor not in visited:
                    path.append(current)
                    if dfs(neighbor):
                        return True
                    path.pop()
            return False

        if dfs(from_node):
            return tuple(path)
        return None
