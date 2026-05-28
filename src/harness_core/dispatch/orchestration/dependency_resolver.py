"""Dependency resolver: validates and orders the workflow dependency graph."""

from __future__ import annotations

from .schemas import WorkflowGraph


class DependencyResolver:
    """Validates workflow graphs and computes execution order via topological sort."""

    def validate(self, graph: WorkflowGraph) -> tuple[bool, list[str]]:
        errors: list[str] = []
        node_ids = {n.node_id for n in graph.nodes}

        for edge in graph.edges:
            if edge.from_node_id not in node_ids:
                errors.append(f"missing_source:{edge.from_node_id}")
            if edge.to_node_id not in node_ids:
                errors.append(f"missing_target:{edge.to_node_id}")

        if self._has_cycle(graph):
            errors.append("cycle_detected")

        return (len(errors) == 0, errors)

    def execution_order(self, graph: WorkflowGraph) -> list[list[str]]:
        in_degree: dict[str, int] = {n.node_id: 0 for n in graph.nodes}
        dependents: dict[str, list[str]] = {n.node_id: [] for n in graph.nodes}

        for edge in graph.edges:
            in_degree[edge.to_node_id] = in_degree.get(edge.to_node_id, 0) + 1
            dependents.setdefault(edge.from_node_id, []).append(edge.to_node_id)

        waves: list[list[str]] = []
        ready = [nid for nid, deg in in_degree.items() if deg == 0]

        while ready:
            waves.append(sorted(ready))
            next_ready: list[str] = []
            for nid in ready:
                for dep in dependents.get(nid, []):
                    in_degree[dep] -= 1
                    if in_degree[dep] == 0:
                        next_ready.append(dep)
            ready = next_ready

        return waves

    def ready_nodes(self, graph: WorkflowGraph) -> list[str]:
        completed = {n.node_id for n in graph.nodes if n.status == "completed"}
        ready: list[str] = []

        for node in graph.nodes:
            if node.status != "pending":
                continue
            deps = self._dependencies_of(graph, node.node_id)
            if all(d in completed for d in deps):
                ready.append(node.node_id)

        return sorted(ready)

    def _dependencies_of(self, graph: WorkflowGraph, node_id: str) -> list[str]:
        return [
            edge.from_node_id
            for edge in graph.edges
            if edge.to_node_id == node_id and edge.edge_type == "dependency"
        ]

    def _has_cycle(self, graph: WorkflowGraph) -> bool:
        WHITE, GRAY, BLACK = 0, 1, 2
        color: dict[str, int] = {n.node_id: WHITE for n in graph.nodes}
        adj: dict[str, list[str]] = {n.node_id: [] for n in graph.nodes}
        for edge in graph.edges:
            if edge.from_node_id in adj:
                adj[edge.from_node_id].append(edge.to_node_id)

        def dfs(nid: str) -> bool:
            color[nid] = GRAY
            for dep in adj[nid]:
                if dep not in color:
                    continue
                if color[dep] == GRAY:
                    return True
                if color[dep] == WHITE and dfs(dep):
                    return True
            color[nid] = BLACK
            return False

        return any(dfs(nid) for nid, c in color.items() if c == WHITE)
