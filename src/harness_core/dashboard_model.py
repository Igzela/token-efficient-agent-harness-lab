"""Read-only dashboard snapshot data model for Stage 4."""

from __future__ import annotations

from dataclasses import dataclass

from .artifact_lifecycle import ArtifactRecord
from .checkpoint import RecoveryPlan
from .concurrency import ScheduleBatch
from .dag_manager import DAGState
from .health import HealthReport
from .supervisor import SupervisorReport


@dataclass(frozen=True)
class DashboardSnapshot:
    generated_at: str
    dag_id: str
    dag_version: int
    node_count: int
    edge_count: int
    supervisor_report: SupervisorReport
    health_report: HealthReport
    artifacts: tuple[ArtifactRecord, ...] = ()
    recovery_plans: tuple[RecoveryPlan, ...] = ()
    schedule_batches: tuple[ScheduleBatch, ...] = ()

    @classmethod
    def build(
        cls,
        *,
        generated_at: str,
        dag: DAGState,
        supervisor_report: SupervisorReport,
        health_report: HealthReport,
        artifacts: tuple[ArtifactRecord, ...] = (),
        recovery_plans: tuple[RecoveryPlan, ...] = (),
        schedule_batches: tuple[ScheduleBatch, ...] = (),
    ) -> "DashboardSnapshot":
        return cls(
            generated_at=generated_at,
            dag_id=dag.dag_id,
            dag_version=dag.version,
            node_count=len(dag.nodes),
            edge_count=len(dag.edges),
            supervisor_report=supervisor_report,
            health_report=health_report,
            artifacts=tuple(sorted(artifacts, key=lambda artifact: artifact.artifact_id)),
            recovery_plans=tuple(recovery_plans),
            schedule_batches=tuple(schedule_batches),
        )
