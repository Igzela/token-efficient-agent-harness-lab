"""Artifact lifecycle records for Stage 4."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


VALID_TRANSITIONS = {
    "draft": {"produced"},
    "produced": {"verified", "rejected"},
    "verified": {"promoted"},
    "promoted": {"archived"},
    "archived": set(),
    "rejected": set(),
}


@dataclass(frozen=True)
class ArtifactRecord:
    artifact_id: str
    task_id: str
    artifact_type: str
    path: str
    sha256: str
    status: str
    created_at: str
    updated_at: str
    metadata: dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class ArtifactTransition:
    artifact_id: str
    from_status: str
    to_status: str
    timestamp: str
    reason: str


@dataclass(frozen=True)
class DependencyUnlock:
    artifact_id: str
    dependency_id: str
    unlocked: bool
    reason: str


class ArtifactLifecycleManager:
    """Track artifact state transitions without moving files."""

    def __init__(self) -> None:
        self._records: dict[str, ArtifactRecord] = {}
        self._transitions: list[ArtifactTransition] = []

    def produce_artifact(
        self,
        *,
        artifact_id: str,
        task_id: str,
        artifact_type: str,
        path: str,
        sha256: str,
        timestamp: str = "2026-01-01T00:00:00Z",
        metadata: dict[str, Any] | None = None,
    ) -> ArtifactRecord:
        if artifact_id in self._records:
            existing = self._records[artifact_id]
            if (
                existing.task_id,
                existing.artifact_type,
                existing.path,
                existing.sha256,
            ) == (task_id, artifact_type, path, sha256):
                return existing
            raise ValueError(f"artifact {artifact_id} already exists with different content")
        record = ArtifactRecord(
            artifact_id=artifact_id,
            task_id=task_id,
            artifact_type=artifact_type,
            path=path,
            sha256=sha256,
            status="produced",
            created_at=timestamp,
            updated_at=timestamp,
            metadata=dict(metadata or {}),
        )
        self._records[artifact_id] = record
        self._transitions.append(
            ArtifactTransition(
                artifact_id=artifact_id,
                from_status="draft",
                to_status="produced",
                timestamp=timestamp,
                reason="artifact produced",
            )
        )
        return record

    def verify_artifact(
        self,
        artifact_id: str,
        *,
        timestamp: str = "2026-01-01T00:00:00Z",
        reason: str = "artifact verified",
    ) -> ArtifactRecord:
        return self._transition(artifact_id, "verified", timestamp, reason)

    def reject_artifact(
        self,
        artifact_id: str,
        *,
        timestamp: str = "2026-01-01T00:00:00Z",
        reason: str = "artifact rejected",
    ) -> ArtifactRecord:
        return self._transition(artifact_id, "rejected", timestamp, reason)

    def promote_artifact(
        self,
        artifact_id: str,
        *,
        timestamp: str = "2026-01-01T00:00:00Z",
        reason: str = "artifact promoted",
    ) -> ArtifactRecord:
        return self._transition(artifact_id, "promoted", timestamp, reason)

    def archive_artifact(
        self,
        artifact_id: str,
        *,
        timestamp: str = "2026-01-01T00:00:00Z",
        reason: str = "artifact archived",
    ) -> ArtifactRecord:
        return self._transition(artifact_id, "archived", timestamp, reason)

    def dependency_unlock(
        self, artifact_id: str, dependency_id: str
    ) -> DependencyUnlock:
        record = self._require(artifact_id)
        unlocked = record.status in ("verified", "promoted")
        reason = (
            f"artifact {artifact_id} is {record.status}"
            if unlocked
            else f"artifact {artifact_id} is not verified or promoted"
        )
        return DependencyUnlock(
            artifact_id=artifact_id,
            dependency_id=dependency_id,
            unlocked=unlocked,
            reason=reason,
        )

    def get_artifact(self, artifact_id: str) -> ArtifactRecord | None:
        return self._records.get(artifact_id)

    def list_artifacts(self) -> tuple[ArtifactRecord, ...]:
        return tuple(self._records[key] for key in sorted(self._records))

    def list_transitions(self, artifact_id: str | None = None) -> tuple[ArtifactTransition, ...]:
        transitions = self._transitions
        if artifact_id is not None:
            transitions = [t for t in transitions if t.artifact_id == artifact_id]
        return tuple(transitions)

    def _transition(
        self, artifact_id: str, to_status: str, timestamp: str, reason: str
    ) -> ArtifactRecord:
        current = self._require(artifact_id)
        allowed = VALID_TRANSITIONS.get(current.status, set())
        if to_status not in allowed:
            raise ValueError(
                f"invalid artifact transition {current.status} -> {to_status}"
            )
        updated = ArtifactRecord(
            artifact_id=current.artifact_id,
            task_id=current.task_id,
            artifact_type=current.artifact_type,
            path=current.path,
            sha256=current.sha256,
            status=to_status,
            created_at=current.created_at,
            updated_at=timestamp,
            metadata=dict(current.metadata),
        )
        self._records[artifact_id] = updated
        self._transitions.append(
            ArtifactTransition(
                artifact_id=artifact_id,
                from_status=current.status,
                to_status=to_status,
                timestamp=timestamp,
                reason=reason,
            )
        )
        return updated

    def _require(self, artifact_id: str) -> ArtifactRecord:
        record = self._records.get(artifact_id)
        if record is None:
            raise ValueError(f"unknown artifact: {artifact_id}")
        return record
