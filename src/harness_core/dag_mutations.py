"""Auditable DAG mutation records for Stage 4."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


SUPPORTED_DAG_MUTATIONS = (
    "add_node",
    "remove_node",
    "split_node",
    "retry_node",
    "pause_node",
    "resume_node",
    "replace_edge",
    "rollback",
)


@dataclass(frozen=True)
class DAGMutationLimits:
    max_nodes: int = 1000
    max_edges: int = 5000


@dataclass(frozen=True)
class DAGMutation:
    mutation_id: str
    dag_id: str
    mutation_type: str
    target_node_id: str | None = None
    target_edge_id: str | None = None
    payload: dict[str, Any] = field(default_factory=dict)
    reason: str = ""
    requires_approval: bool = False
    status: str = "pending"


@dataclass(frozen=True)
class DAGMutationAuditEvent:
    event_type: str
    mutation_id: str
    dag_id: str
    mutation_type: str
    payload: dict[str, Any]
    reason: str


@dataclass(frozen=True)
class DAGMutationValidation:
    ok: bool
    errors: tuple[str, ...] = ()
    warnings: tuple[str, ...] = ()


def validate_dag_mutation(
    mutation: DAGMutation,
    *,
    current_node_count: int,
    current_edge_count: int,
    limits: DAGMutationLimits = DAGMutationLimits(),
) -> DAGMutationValidation:
    """Validate mutation shape and configured size limits."""
    errors: list[str] = []
    if mutation.mutation_type not in SUPPORTED_DAG_MUTATIONS:
        errors.append(f"unsupported mutation_type: {mutation.mutation_type}")

    next_node_count = current_node_count + _node_delta(mutation)
    next_edge_count = current_edge_count + _edge_delta(mutation)
    if next_node_count > limits.max_nodes:
        errors.append(
            f"mutation would exceed max_nodes: {next_node_count} > {limits.max_nodes}"
        )
    if next_edge_count > limits.max_edges:
        errors.append(
            f"mutation would exceed max_edges: {next_edge_count} > {limits.max_edges}"
        )
    return DAGMutationValidation(ok=not errors, errors=tuple(errors))


def dag_mutation_requires_approval(
    mutation: DAGMutation,
    *,
    target_node_status: str | None = None,
    source_node_status: str | None = None,
    affects_artifacts: bool = False,
) -> bool:
    """Return whether a mutation needs human approval under Stage 4 rules."""
    if mutation.requires_approval:
        return True
    if affects_artifacts:
        return True
    if mutation.mutation_type in {
        "remove_node",
        "split_node",
        "retry_node",
        "pause_node",
        "replace_edge",
    } and target_node_status in {"running", "completed"}:
        return True
    if mutation.mutation_type == "replace_edge" and source_node_status == "completed":
        return True
    return False


def create_compensating_mutation(
    mutation: DAGMutation,
    *,
    previous_payload: dict[str, Any] | None = None,
) -> DAGMutation:
    """Create a forward-only compensating mutation record."""
    inverse_types = {
        "add_node": "remove_node",
        "remove_node": "add_node",
        "split_node": "rollback",
        "retry_node": "rollback",
        "pause_node": "resume_node",
        "resume_node": "pause_node",
        "replace_edge": "replace_edge",
        "rollback": "rollback",
    }
    payload = dict(previous_payload if previous_payload is not None else mutation.payload)
    payload["compensates"] = mutation.mutation_id
    return DAGMutation(
        mutation_id=f"comp_{mutation.mutation_id}",
        dag_id=mutation.dag_id,
        mutation_type=inverse_types[mutation.mutation_type],
        target_node_id=mutation.target_node_id,
        target_edge_id=mutation.target_edge_id,
        payload=payload,
        reason=f"compensate {mutation.mutation_id}: {mutation.reason}".strip(),
        requires_approval=False,
        status="pending",
    )


def mutation_to_audit_event(mutation: DAGMutation) -> DAGMutationAuditEvent:
    """Represent a mutation as an append-only audit event payload."""
    return DAGMutationAuditEvent(
        event_type="dag_mutation_recorded",
        mutation_id=mutation.mutation_id,
        dag_id=mutation.dag_id,
        mutation_type=mutation.mutation_type,
        payload={
            "payload": dict(mutation.payload),
            "requires_approval": mutation.requires_approval,
            "status": mutation.status,
            "target_edge_id": mutation.target_edge_id,
            "target_node_id": mutation.target_node_id,
        },
        reason=mutation.reason,
    )


def _node_delta(mutation: DAGMutation) -> int:
    if mutation.mutation_type == "add_node":
        return 1
    if mutation.mutation_type == "remove_node":
        return -1
    if mutation.mutation_type == "split_node":
        replacement_count = int(mutation.payload.get("replacement_node_count", 0))
        return replacement_count - 1
    return 0


def _edge_delta(mutation: DAGMutation) -> int:
    if mutation.mutation_type == "split_node":
        return int(mutation.payload.get("added_edge_count", 0))
    if mutation.mutation_type == "replace_edge":
        return 0
    return int(mutation.payload.get("edge_delta", 0))
