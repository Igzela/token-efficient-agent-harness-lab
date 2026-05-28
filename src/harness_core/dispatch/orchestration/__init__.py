"""Phase 5: Multi-agent orchestration subpackage."""

from __future__ import annotations

from .agent_role_registry import AgentRoleRegistry
from .conflict_resolver import ConflictResolver
from .dependency_resolver import DependencyResolver
from .human_approval_gate import HumanApprovalGate
from .multi_agent_budget import MultiAgentBudgetManager
from .result_aggregator import ResultAggregator
from .schemas import (
    AGENT_MESSAGE_SCHEMA_VERSION,
    AGENT_ROLE_SCHEMA_VERSION,
    CONFLICT_RECORD_SCHEMA_VERSION,
    CONFLICT_TYPES,
    EDGE_TYPES,
    MESSAGE_TYPES,
    RESOLUTION_STRATEGIES,
    WORKFLOW_EDGE_SCHEMA_VERSION,
    WORKFLOW_NODE_SCHEMA_VERSION,
    WORKFLOW_SCHEMA_VERSION,
    WORKFLOW_STATUSES,
    NODE_STATUSES,
    AgentMessage,
    AgentRole,
    ConflictRecord,
    WorkflowEdge,
    WorkflowGraph,
    WorkflowNode,
)
from .task_decomposer import TaskDecomposer
from .work_queue import WorkQueue
from .workflow_engine import WorkflowEngine

__all__ = [
    "AGENT_MESSAGE_SCHEMA_VERSION",
    "AGENT_ROLE_SCHEMA_VERSION",
    "AgentMessage",
    "AgentRole",
    "AgentRoleRegistry",
    "CONFLICT_RECORD_SCHEMA_VERSION",
    "CONFLICT_TYPES",
    "ConflictRecord",
    "ConflictResolver",
    "DependencyResolver",
    "EDGE_TYPES",
    "HumanApprovalGate",
    "MESSAGE_TYPES",
    "MultiAgentBudgetManager",
    "NODE_STATUSES",
    "RESOLUTION_STRATEGIES",
    "ResultAggregator",
    "TaskDecomposer",
    "WORKFLOW_EDGE_SCHEMA_VERSION",
    "WORKFLOW_NODE_SCHEMA_VERSION",
    "WORKFLOW_SCHEMA_VERSION",
    "WORKFLOW_STATUSES",
    "WorkflowEdge",
    "WorkflowEngine",
    "WorkflowGraph",
    "WorkflowNode",
    "WorkQueue",
]
