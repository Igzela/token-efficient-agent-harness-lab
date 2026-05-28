"""Checkpoint and recovery planning primitives for Stage 4."""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from .event_store import replay_preflight
from .projection_store import replay_all


@dataclass(frozen=True)
class ArtifactRef:
    artifact_type: str
    path: str
    sha256: str


@dataclass(frozen=True)
class CompensatingEvent:
    event_type: str
    payload: dict[str, Any]
    reason: str


@dataclass(frozen=True)
class Checkpoint:
    checkpoint_id: str
    task_id: str
    node_id: str
    dag_version: int
    status: str  # "running" | "completed" | "failed"
    current_step: str
    completed_steps: tuple[str, ...]
    pending_steps: tuple[str, ...]
    input_hash: str
    artifact_refs: tuple[ArtifactRef, ...] = ()
    model_call_refs: tuple[str, ...] = ()
    tool_call_refs: tuple[str, ...] = ()
    resumable: bool = True
    resume_strategy: str = "resume_in_same_sandbox"
    created_at: str = "2026-01-01T00:00:00Z"
    reason: str | None = None


@dataclass(frozen=True)
class RecoveryPlan:
    task_id: str
    checkpoint_id: str | None
    strategy: str  # "resume" | "restart" | "skip" | "compensate"
    compensating_events: tuple[CompensatingEvent, ...] = ()
    resumed_from_step: str | None = None
    warnings: tuple[str, ...] = ()


@dataclass(frozen=True)
class IntegrityCheck:
    ok: bool
    errors: tuple[str, ...] = ()
    warnings: tuple[str, ...] = ()


class CheckpointManager:
    """Persist JSON checkpoints and produce deterministic recovery plans."""

    def __init__(self, store_dir: str | Path):
        self.store_dir = Path(store_dir)

    def save_checkpoint(self, checkpoint: Checkpoint) -> None:
        self.store_dir.mkdir(parents=True, exist_ok=True)
        path = self._path_for(checkpoint.checkpoint_id)
        path.write_text(
            json.dumps(_checkpoint_to_dict(checkpoint), sort_keys=True, indent=2)
            + "\n",
            encoding="utf-8",
        )

    def load_checkpoint(self, checkpoint_id: str) -> Checkpoint | None:
        path = self._path_for(checkpoint_id)
        if not path.exists():
            return None
        return _checkpoint_from_dict(json.loads(path.read_text(encoding="utf-8")))

    def list_checkpoints(self, task_id: str) -> tuple[Checkpoint, ...]:
        if not self.store_dir.exists():
            return ()
        checkpoints = []
        for path in sorted(self.store_dir.glob("*.json")):
            checkpoint = _checkpoint_from_dict(json.loads(path.read_text(encoding="utf-8")))
            if checkpoint.task_id == task_id:
                checkpoints.append(checkpoint)
        return tuple(sorted(checkpoints, key=lambda c: (c.created_at, c.checkpoint_id)))

    def latest_checkpoint(self, task_id: str) -> Checkpoint | None:
        checkpoints = self.list_checkpoints(task_id)
        if not checkpoints:
            return None
        return checkpoints[-1]

    def checkpoint_id_for(
        self,
        task_id: str,
        node_id: str,
        dag_version: int,
        current_step: str,
        created_at: str,
    ) -> str:
        payload = {
            "created_at": created_at,
            "current_step": current_step,
            "dag_version": dag_version,
            "node_id": node_id,
            "task_id": task_id,
        }
        digest = hashlib.sha256(
            json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
        ).hexdigest()[:16]
        return f"ckpt_{task_id}_{digest}"

    def create_checkpoint(
        self,
        *,
        task_id: str,
        node_id: str,
        dag_version: int,
        status: str,
        current_step: str,
        completed_steps: tuple[str, ...],
        pending_steps: tuple[str, ...],
        input_hash: str,
        created_at: str,
        artifact_refs: tuple[ArtifactRef, ...] = (),
        model_call_refs: tuple[str, ...] = (),
        tool_call_refs: tuple[str, ...] = (),
        resumable: bool = True,
        resume_strategy: str = "resume_in_same_sandbox",
        reason: str | None = None,
    ) -> Checkpoint:
        checkpoint = Checkpoint(
            checkpoint_id=self.checkpoint_id_for(
                task_id, node_id, dag_version, current_step, created_at
            ),
            task_id=task_id,
            node_id=node_id,
            dag_version=dag_version,
            status=status,
            current_step=current_step,
            completed_steps=tuple(completed_steps),
            pending_steps=tuple(pending_steps),
            input_hash=input_hash,
            artifact_refs=tuple(artifact_refs),
            model_call_refs=tuple(model_call_refs),
            tool_call_refs=tuple(tool_call_refs),
            resumable=resumable,
            resume_strategy=resume_strategy,
            created_at=created_at,
            reason=reason,
        )
        self.save_checkpoint(checkpoint)
        return checkpoint

    def create_recovery_plan(self, task_id: str) -> RecoveryPlan:
        checkpoint = self.latest_checkpoint(task_id)
        if checkpoint is None:
            return RecoveryPlan(
                task_id=task_id,
                checkpoint_id=None,
                strategy="skip",
                warnings=("no checkpoint exists",),
            )
        if checkpoint.status == "running" and checkpoint.resumable:
            return RecoveryPlan(
                task_id=task_id,
                checkpoint_id=checkpoint.checkpoint_id,
                strategy="resume",
                resumed_from_step=checkpoint.current_step,
            )
        if checkpoint.status == "running":
            return RecoveryPlan(
                task_id=task_id,
                checkpoint_id=checkpoint.checkpoint_id,
                strategy="restart",
                warnings=("checkpoint is not resumable",),
            )
        if checkpoint.status == "failed":
            return RecoveryPlan(
                task_id=task_id,
                checkpoint_id=checkpoint.checkpoint_id,
                strategy="compensate",
                compensating_events=self.generate_compensating_events(checkpoint),
            )
        return RecoveryPlan(
            task_id=task_id,
            checkpoint_id=checkpoint.checkpoint_id,
            strategy="skip",
            warnings=(f"checkpoint status is {checkpoint.status}",),
        )

    def generate_compensating_events(
        self, checkpoint: Checkpoint
    ) -> tuple[CompensatingEvent, ...]:
        events = [
            CompensatingEvent(
                event_type="task_cancelled",
                payload={
                    "checkpoint_id": checkpoint.checkpoint_id,
                    "reason": checkpoint.reason or "recovery",
                    "task_id": checkpoint.task_id,
                },
                reason="Cancel failed task by appending a compensating event",
            ),
            CompensatingEvent(
                event_type="claim_released",
                payload={"task_id": checkpoint.task_id},
                reason="Release file claims on recovery",
            ),
        ]
        return tuple(events)

    def check_event_log_integrity(self, event_log_path: str | Path) -> IntegrityCheck:
        report = replay_preflight(event_log_path)
        return IntegrityCheck(
            ok=report.ok,
            errors=tuple(issue.message for issue in report.errors),
            warnings=tuple(issue.message for issue in report.warnings),
        )

    def check_projection_consistency(self, event_log_path: str | Path) -> IntegrityCheck:
        try:
            bundle = replay_all(event_log_path)
        except Exception as exc:
            return IntegrityCheck(ok=False, errors=(str(exc),))
        warnings = tuple(issue.message for issue in bundle.warnings)
        return IntegrityCheck(ok=True, warnings=warnings)

    def _path_for(self, checkpoint_id: str) -> Path:
        path = (self.store_dir / f"{checkpoint_id}.json").resolve()
        if not str(path).startswith(str(self.store_dir.resolve())):
            raise ValueError(f"checkpoint_id contains path traversal: {checkpoint_id!r}")
        return path


def _checkpoint_to_dict(checkpoint: Checkpoint) -> dict[str, Any]:
    return {
        "artifact_refs": [
            {
                "artifact_type": ref.artifact_type,
                "path": ref.path,
                "sha256": ref.sha256,
            }
            for ref in checkpoint.artifact_refs
        ],
        "checkpoint_id": checkpoint.checkpoint_id,
        "completed_steps": list(checkpoint.completed_steps),
        "created_at": checkpoint.created_at,
        "current_step": checkpoint.current_step,
        "dag_version": checkpoint.dag_version,
        "input_hash": checkpoint.input_hash,
        "model_call_refs": list(checkpoint.model_call_refs),
        "node_id": checkpoint.node_id,
        "pending_steps": list(checkpoint.pending_steps),
        "reason": checkpoint.reason,
        "resumable": checkpoint.resumable,
        "resume_strategy": checkpoint.resume_strategy,
        "status": checkpoint.status,
        "task_id": checkpoint.task_id,
        "tool_call_refs": list(checkpoint.tool_call_refs),
    }


def _checkpoint_from_dict(data: dict[str, Any]) -> Checkpoint:
    return Checkpoint(
        checkpoint_id=data["checkpoint_id"],
        task_id=data["task_id"],
        node_id=data["node_id"],
        dag_version=int(data["dag_version"]),
        status=data["status"],
        current_step=data["current_step"],
        completed_steps=tuple(data["completed_steps"]),
        pending_steps=tuple(data["pending_steps"]),
        input_hash=data["input_hash"],
        artifact_refs=tuple(
            ArtifactRef(
                artifact_type=ref["artifact_type"],
                path=ref["path"],
                sha256=ref["sha256"],
            )
            for ref in data.get("artifact_refs", [])
        ),
        model_call_refs=tuple(data.get("model_call_refs", [])),
        tool_call_refs=tuple(data.get("tool_call_refs", [])),
        resumable=bool(data.get("resumable", True)),
        resume_strategy=data.get("resume_strategy", "resume_in_same_sandbox"),
        created_at=data["created_at"],
        reason=data.get("reason"),
    )
