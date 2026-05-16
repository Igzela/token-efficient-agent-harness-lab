"""Runtime supervisor health summaries for Stage 4."""

from __future__ import annotations

from dataclasses import dataclass

from .checkpoint import Checkpoint, CheckpointManager, RecoveryPlan


@dataclass(frozen=True)
class WorkerHealth:
    worker_id: str
    task_id: str
    status: str  # "idle" | "running" | "completed" | "failed" | "crashed"
    last_heartbeat: int
    started_at: int
    error: str | None = None


@dataclass(frozen=True)
class ComponentHealth:
    component_id: str
    status: str  # "healthy" | "degraded" | "failed"
    message: str
    checked_at: int


@dataclass(frozen=True)
class SupervisorReport:
    checked_at: int
    healthy: bool
    stuck_workers: tuple[WorkerHealth, ...]
    crashed_workers: tuple[WorkerHealth, ...]
    component_health: tuple[ComponentHealth, ...]
    recovery_plans: tuple[RecoveryPlan, ...] = ()


class RuntimeSupervisor:
    """Coordinate deterministic checkpoint state and supplied worker health."""

    def __init__(
        self,
        checkpoint_manager: CheckpointManager,
        *,
        heartbeat_timeout: int = 300,
        dag_version: int = 0,
    ):
        self.checkpoint_manager = checkpoint_manager
        self.heartbeat_timeout = heartbeat_timeout
        self.dag_version = dag_version
        self._task_nodes: dict[str, str] = {}
        self._task_steps: dict[str, tuple[str, ...]] = {}
        self._task_input_hashes: dict[str, str] = {}

    def start_task(
        self,
        task_id: str,
        node_id: str,
        steps: tuple[str, ...],
        *,
        input_hash: str = "",
        timestamp: str = "2026-01-01T00:00:00Z",
    ) -> Checkpoint:
        self._task_nodes[task_id] = node_id
        self._task_steps[task_id] = tuple(steps)
        self._task_input_hashes[task_id] = input_hash
        current_step = steps[0] if steps else "start"
        pending_steps = tuple(steps[1:]) if steps else ()
        return self.checkpoint_manager.create_checkpoint(
            task_id=task_id,
            node_id=node_id,
            dag_version=self.dag_version,
            status="running",
            current_step=current_step,
            completed_steps=(),
            pending_steps=pending_steps,
            input_hash=input_hash,
            created_at=timestamp,
        )

    def checkpoint_step(
        self,
        task_id: str,
        step: str,
        *,
        timestamp: str = "2026-01-01T00:00:00Z",
    ) -> Checkpoint:
        latest = self.checkpoint_manager.latest_checkpoint(task_id)
        if latest is None:
            raise ValueError(f"task {task_id} has not started")
        completed = tuple(dict.fromkeys(latest.completed_steps + (step,)))
        pending = tuple(s for s in self._task_steps.get(task_id, ()) if s not in completed)
        current_step = pending[0] if pending else step
        return self.checkpoint_manager.create_checkpoint(
            task_id=task_id,
            node_id=latest.node_id,
            dag_version=latest.dag_version,
            status="running",
            current_step=current_step,
            completed_steps=completed,
            pending_steps=pending[1:] if pending else (),
            input_hash=latest.input_hash,
            created_at=timestamp,
        )

    def complete_task(
        self,
        task_id: str,
        *,
        timestamp: str = "2026-01-01T00:00:00Z",
    ) -> Checkpoint:
        latest = self._latest_required(task_id)
        steps = self._task_steps.get(task_id, latest.completed_steps)
        completed = tuple(dict.fromkeys(latest.completed_steps + tuple(steps)))
        return self.checkpoint_manager.create_checkpoint(
            task_id=task_id,
            node_id=latest.node_id,
            dag_version=latest.dag_version,
            status="completed",
            current_step=latest.current_step,
            completed_steps=completed,
            pending_steps=(),
            input_hash=latest.input_hash,
            created_at=timestamp,
            resumable=False,
            resume_strategy="skip",
        )

    def fail_task(
        self,
        task_id: str,
        reason: str,
        *,
        timestamp: str = "2026-01-01T00:00:00Z",
    ) -> Checkpoint:
        latest = self._latest_required(task_id)
        return self.checkpoint_manager.create_checkpoint(
            task_id=task_id,
            node_id=latest.node_id,
            dag_version=latest.dag_version,
            status="failed",
            current_step=latest.current_step,
            completed_steps=latest.completed_steps,
            pending_steps=latest.pending_steps,
            input_hash=latest.input_hash,
            created_at=timestamp,
            resumable=False,
            resume_strategy="restart_in_new_sandbox",
            reason=reason,
        )

    def recover_task(self, task_id: str) -> RecoveryPlan:
        return self.checkpoint_manager.create_recovery_plan(task_id)

    def get_status(self, task_id: str) -> str:
        checkpoint = self.checkpoint_manager.latest_checkpoint(task_id)
        if checkpoint is None:
            return "unknown"
        return checkpoint.status

    def assess_workers(
        self, workers: tuple[WorkerHealth, ...], *, now: int
    ) -> SupervisorReport:
        stuck = tuple(
            worker
            for worker in workers
            if worker.status == "running"
            and now - worker.last_heartbeat > self.heartbeat_timeout
        )
        crashed = tuple(
            worker for worker in workers if worker.status in ("crashed", "failed")
        )
        components = (
            ComponentHealth(
                component_id="runtime_supervisor",
                status="failed" if crashed else "degraded" if stuck else "healthy",
                message=_component_message(stuck, crashed),
                checked_at=now,
            ),
        )
        plans = tuple(self.recover_task(worker.task_id) for worker in crashed)
        return SupervisorReport(
            checked_at=now,
            healthy=not stuck and not crashed,
            stuck_workers=stuck,
            crashed_workers=crashed,
            component_health=components,
            recovery_plans=plans,
        )

    def _latest_required(self, task_id: str) -> Checkpoint:
        latest = self.checkpoint_manager.latest_checkpoint(task_id)
        if latest is None:
            raise ValueError(f"task {task_id} has not started")
        return latest


def _component_message(
    stuck: tuple[WorkerHealth, ...], crashed: tuple[WorkerHealth, ...]
) -> str:
    if crashed:
        return f"{len(crashed)} crashed worker(s)"
    if stuck:
        return f"{len(stuck)} stuck worker(s)"
    return "all supplied workers healthy"
