"""Deterministic scheduling-only concurrency controller for Stage 4."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from .dag_manager import DAGEdge, DAGNode, DAGState


@dataclass(frozen=True)
class FileOverlap:
    item_a_id: str
    item_b_id: str
    files: tuple[str, ...]


@dataclass(frozen=True)
class ScheduleBatch:
    scheduled_items: tuple[Any, ...]
    blocked_items: tuple[Any, ...] = ()
    file_overlaps: tuple[FileOverlap, ...] = ()
    warnings: tuple[str, ...] = ()

    @property
    def item_ids(self) -> tuple[str, ...]:
        return tuple(_item_id(item) for item in self.scheduled_items)


class ConcurrencyController:
    """Select a conflict-free batch without spawning concurrent workers."""

    def __init__(self, max_concurrent: int = 4):
        if max_concurrent < 1:
            raise ValueError("max_concurrent must be at least 1")
        self.max_concurrent = max_concurrent

    def schedule(
        self,
        ready_items: tuple[Any, ...],
        dag: DAGState | Any,
        active_claims: tuple[Any, ...] = (),
    ) -> ScheduleBatch:
        if not ready_items:
            return ScheduleBatch(scheduled_items=())

        state = _dag_state(dag)
        overlaps = self.detect_file_overlaps(ready_items)
        blocked: list[Any] = []
        eligible: list[Any] = []
        warnings: list[str] = []

        for item in sorted(ready_items, key=_item_id):
            reason = _blocking_reason(item, state, active_claims)
            if reason:
                blocked.append(item)
                warnings.append(reason)
            else:
                eligible.append(item)

        scheduled: list[Any] = []
        for item in eligible:
            if len(scheduled) >= self.max_concurrent:
                blocked.append(item)
                warnings.append(f"{_item_id(item)} exceeds max_concurrent")
                continue
            if all(self.can_run_parallel(existing, item, overlaps) for existing in scheduled):
                scheduled.append(item)
            else:
                blocked.append(item)
                warnings.append(f"{_item_id(item)} conflicts with scheduled file claims")

        return ScheduleBatch(
            scheduled_items=tuple(scheduled),
            blocked_items=tuple(blocked),
            file_overlaps=overlaps,
            warnings=tuple(warnings),
        )

    def detect_file_overlaps(self, items: tuple[Any, ...]) -> tuple[FileOverlap, ...]:
        overlaps: list[FileOverlap] = []
        sorted_items = tuple(sorted(items, key=_item_id))
        for index, item_a in enumerate(sorted_items):
            for item_b in sorted_items[index + 1 :]:
                files = _conflicting_files(item_a, item_b)
                if files:
                    overlaps.append(
                        FileOverlap(
                            item_a_id=_item_id(item_a),
                            item_b_id=_item_id(item_b),
                            files=files,
                        )
                    )
        return tuple(overlaps)

    def can_run_parallel(
        self,
        item_a: Any,
        item_b: Any,
        overlaps: tuple[FileOverlap, ...],
    ) -> bool:
        pair = tuple(sorted((_item_id(item_a), _item_id(item_b))))
        for overlap in overlaps:
            if tuple(sorted((overlap.item_a_id, overlap.item_b_id))) == pair:
                return False
        return True


def _dag_state(dag: DAGState | Any) -> DAGState:
    if isinstance(dag, DAGState):
        return dag
    if hasattr(dag, "current_state"):
        return dag.current_state()
    if hasattr(dag, "state"):
        return dag.state
    raise TypeError("dag must be a DAGState or expose state/current_state")


def _item_id(item: Any) -> str:
    if isinstance(item, dict):
        return str(item.get("node_id") or item.get("task_id") or item.get("item_id"))
    for attr in ("node_id", "task_id", "item_id"):
        value = getattr(item, attr, None)
        if value:
            return str(value)
    raise ValueError("item must expose node_id, task_id, or item_id")


def _metadata(item: Any) -> dict[str, Any]:
    if isinstance(item, dict):
        return dict(item.get("metadata", item))
    return dict(getattr(item, "metadata", {}))


def _read_files(item: Any) -> tuple[str, ...]:
    metadata = _metadata(item)
    return tuple(sorted(metadata.get("read_files", ())))


def _write_files(item: Any) -> tuple[str, ...]:
    metadata = _metadata(item)
    return tuple(sorted(metadata.get("write_files", ())))


def _conflicting_files(item_a: Any, item_b: Any) -> tuple[str, ...]:
    a_writes = set(_write_files(item_a))
    b_writes = set(_write_files(item_b))
    a_reads = set(_read_files(item_a))
    b_reads = set(_read_files(item_b))
    conflicts = (a_writes & (b_writes | b_reads)) | (b_writes & a_reads)
    return tuple(sorted(conflicts))


def _blocking_reason(
    item: Any, state: DAGState, active_claims: tuple[Any, ...]
) -> str | None:
    item_id = _item_id(item)
    node_status = {node.node_id: node.status for node in state.nodes}
    incoming = tuple(edge for edge in state.edges if edge.to_node == item_id)
    for edge in sorted(incoming, key=lambda e: e.edge_id):
        if _edge_blocks(edge, item, node_status):
            return f"{item_id} blocked by {edge.dependency_type} dependency {edge.edge_id}"

    active_files = {
        getattr(claim, "file_path", None)
        for claim in active_claims
        if not getattr(claim, "released", False)
    }
    active_files.discard(None)
    conflict = sorted(active_files & set(_write_files(item)))
    if conflict:
        return f"{item_id} blocked by active write claim on {conflict[0]}"
    return None


def _edge_blocks(edge: DAGEdge, item: Any, node_status: dict[str, str]) -> bool:
    if edge.dependency_type == "soft":
        return False
    if edge.status == "satisfied":
        return False
    if edge.dependency_type == "artifact":
        verified = set(_metadata(item).get("verified_artifacts", ()))
        return edge.edge_id not in verified and edge.from_node not in verified
    return node_status.get(edge.from_node) != "completed"
