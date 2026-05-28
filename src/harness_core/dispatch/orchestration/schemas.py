"""Phase 5 orchestration schemas: workflows, nodes, edges, messages, conflicts, roles."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

# ---------------------------------------------------------------------------
# Schema versions
# ---------------------------------------------------------------------------

WORKFLOW_SCHEMA_VERSION = "workflow_graph.v1"
WORKFLOW_NODE_SCHEMA_VERSION = "workflow_node.v1"
WORKFLOW_EDGE_SCHEMA_VERSION = "workflow_edge.v1"
AGENT_MESSAGE_SCHEMA_VERSION = "agent_message.v1"
CONFLICT_RECORD_SCHEMA_VERSION = "conflict_record.v1"
AGENT_ROLE_SCHEMA_VERSION = "agent_role.v1"

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

WORKFLOW_STATUSES: tuple[str, ...] = (
    "created", "decomposed", "running", "waiting_human",
    "aggregating", "completed", "failed", "cancelled",
)

NODE_STATUSES: tuple[str, ...] = (
    "pending", "ready", "running", "completed", "failed", "cancelled", "waiting_human",
)

EDGE_TYPES: tuple[str, ...] = ("dependency", "data_flow")

MESSAGE_TYPES: tuple[str, ...] = ("task_assign", "result", "conflict", "approval_request", "status_update")

CONFLICT_TYPES: tuple[str, ...] = ("output_conflict", "resource_conflict", "dependency_violation", "budget_overrun")

RESOLUTION_STRATEGIES: tuple[str, ...] = ("latest_wins", "priority_wins", "merge", "human_decides", "skip")

# ---------------------------------------------------------------------------
# Schemas
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class WorkflowNode:
    node_id: str
    workflow_id: str
    task_type: str
    assigned_agent_id: str | None
    status: str = "pending"
    input_refs: tuple[str, ...] = ()
    output_ref: str | None = None
    budget: float = 0.0
    cost_incurred: float = 0.0
    error: str | None = None
    created_at: str = ""
    started_at: str | None = None
    completed_at: str | None = None
    schema_version: str = WORKFLOW_NODE_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "node_id": self.node_id,
            "workflow_id": self.workflow_id,
            "task_type": self.task_type,
            "assigned_agent_id": self.assigned_agent_id,
            "status": self.status,
            "input_refs": list(self.input_refs),
            "output_ref": self.output_ref,
            "budget": self.budget,
            "cost_incurred": self.cost_incurred,
            "error": self.error,
            "created_at": self.created_at,
            "started_at": self.started_at,
            "completed_at": self.completed_at,
        }


@dataclass(frozen=True)
class WorkflowEdge:
    edge_id: str
    from_node_id: str
    to_node_id: str
    edge_type: str = "dependency"
    schema_version: str = WORKFLOW_EDGE_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "edge_id": self.edge_id,
            "from_node_id": self.from_node_id,
            "to_node_id": self.to_node_id,
            "edge_type": self.edge_type,
        }


@dataclass(frozen=True)
class WorkflowGraph:
    workflow_id: str
    dispatch_id: str
    nodes: tuple[WorkflowNode, ...] = ()
    edges: tuple[WorkflowEdge, ...] = ()
    status: str = "created"
    created_at: str = ""
    started_at: str | None = None
    completed_at: str | None = None
    result: dict[str, Any] | None = None
    schema_version: str = WORKFLOW_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "workflow_id": self.workflow_id,
            "dispatch_id": self.dispatch_id,
            "nodes": [n.to_dict() for n in self.nodes],
            "edges": [e.to_dict() for e in self.edges],
            "status": self.status,
            "created_at": self.created_at,
            "started_at": self.started_at,
            "completed_at": self.completed_at,
            "result": self.result,
        }


@dataclass(frozen=True)
class AgentMessage:
    message_id: str
    from_agent_id: str
    to_agent_id: str
    workflow_id: str
    message_type: str
    payload: dict[str, Any] = field(default_factory=dict)
    timestamp: str = ""
    schema_version: str = AGENT_MESSAGE_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "message_id": self.message_id,
            "from_agent_id": self.from_agent_id,
            "to_agent_id": self.to_agent_id,
            "workflow_id": self.workflow_id,
            "message_type": self.message_type,
            "payload": self.payload,
            "timestamp": self.timestamp,
        }


@dataclass(frozen=True)
class ConflictRecord:
    conflict_id: str
    workflow_id: str
    conflict_type: str
    involved_nodes: tuple[str, ...]
    resolution_strategy: str | None = None
    resolution_result: str | None = None
    resolved_at: str | None = None
    schema_version: str = CONFLICT_RECORD_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "conflict_id": self.conflict_id,
            "workflow_id": self.workflow_id,
            "conflict_type": self.conflict_type,
            "involved_nodes": list(self.involved_nodes),
            "resolution_strategy": self.resolution_strategy,
            "resolution_result": self.resolution_result,
            "resolved_at": self.resolved_at,
        }


@dataclass(frozen=True)
class AgentRole:
    role_id: str
    role_name: str
    capabilities: tuple[str, ...] = ()
    max_concurrent_nodes: int = 1
    budget_limit: float = 0.0
    schema_version: str = AGENT_ROLE_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "role_id": self.role_id,
            "role_name": self.role_name,
            "capabilities": list(self.capabilities),
            "max_concurrent_nodes": self.max_concurrent_nodes,
            "budget_limit": self.budget_limit,
        }
