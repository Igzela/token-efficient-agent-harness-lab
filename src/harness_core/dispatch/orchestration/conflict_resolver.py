"""Conflict resolver: detects and resolves workflow conflicts."""

from __future__ import annotations

import uuid
from datetime import datetime, timezone

from .schemas import CONFLICT_TYPES, RESOLUTION_STRATEGIES, ConflictRecord, WorkflowGraph


class ConflictResolver:
    """Rule-based conflict detection and resolution."""

    def detect_conflicts(self, graph: WorkflowGraph) -> list[ConflictRecord]:
        conflicts: list[ConflictRecord] = []
        now = datetime.now(timezone.utc).isoformat()

        completed = [n for n in graph.nodes if n.status == "completed"]
        failed = [n for n in graph.nodes if n.status == "failed"]

        if failed:
            conflicts.append(ConflictRecord(
                conflict_id=f"conflict-{uuid.uuid4().hex[:8]}",
                workflow_id=graph.workflow_id,
                conflict_type="dependency_violation",
                involved_nodes=tuple(n.node_id for n in failed),
                resolved_at=now,
            ))

        output_groups: dict[str, list[str]] = {}
        for node in completed:
            if node.output_ref:
                output_groups.setdefault(node.output_ref, []).append(node.node_id)
        for output_ref, node_ids in output_groups.items():
            if len(node_ids) > 1:
                conflicts.append(ConflictRecord(
                    conflict_id=f"conflict-{uuid.uuid4().hex[:8]}",
                    workflow_id=graph.workflow_id,
                    conflict_type="output_conflict",
                    involved_nodes=tuple(node_ids),
                ))

        agent_nodes: dict[str, list[str]] = {}
        for node in completed:
            if node.assigned_agent_id:
                agent_nodes.setdefault(node.assigned_agent_id, []).append(node.node_id)
        for agent_id, node_ids in agent_nodes.items():
            if len(node_ids) > 1:
                conflicts.append(ConflictRecord(
                    conflict_id=f"conflict-{uuid.uuid4().hex[:8]}",
                    workflow_id=graph.workflow_id,
                    conflict_type="resource_conflict",
                    involved_nodes=tuple(node_ids),
                ))

        total_cost = sum(n.cost_incurred for n in graph.nodes)
        total_budget = sum(n.budget for n in graph.nodes)
        if total_budget > 0 and total_cost > total_budget:
            conflicts.append(ConflictRecord(
                conflict_id=f"conflict-{uuid.uuid4().hex[:8]}",
                workflow_id=graph.workflow_id,
                conflict_type="budget_overrun",
                involved_nodes=tuple(n.node_id for n in graph.nodes),
            ))

        return conflicts

    def resolve(self, conflict: ConflictRecord) -> ConflictRecord:
        strategy = self._pick_strategy(conflict)
        now = datetime.now(timezone.utc).isoformat()

        if conflict.conflict_type == "output_conflict":
            result = "latest_output_wins"
        elif conflict.conflict_type == "resource_conflict":
            result = "serialized_execution"
        elif conflict.conflict_type == "dependency_violation":
            result = "failed_node_skipped"
        elif conflict.conflict_type == "budget_overrun":
            result = "workflow_cancelled"
        else:
            result = "unresolved"

        return ConflictRecord(
            conflict_id=conflict.conflict_id,
            workflow_id=conflict.workflow_id,
            conflict_type=conflict.conflict_type,
            involved_nodes=conflict.involved_nodes,
            resolution_strategy=strategy,
            resolution_result=result,
            resolved_at=now,
            schema_version=conflict.schema_version,
        )

    def _pick_strategy(self, conflict: ConflictRecord) -> str:
        if conflict.conflict_type == "output_conflict":
            return "latest_wins"
        if conflict.conflict_type == "resource_conflict":
            return "priority_wins"
        if conflict.conflict_type == "budget_overrun":
            return "human_decides"
        return "skip"
