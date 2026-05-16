"""Deterministic Stage 1 orchestrator connecting existing components."""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from .batch_runner import BatchRunner, _next_event_ids, _base_event
from .digest import BatchDigest, generate_batch_digest
from .event_schema import validate_event
from .event_store import ReplayPreflightReport
from .final_gate import FinalGateDecision, FinalGateRunner
from .kernel import Kernel
from .projection_store import (
    ProjectItemState,
    ProjectStateProjection,
    ProjectionBundle,
    TaskQueueProjection,
)
from .task_records import TaskRecordStore

# Stage 2 quality components — imported lazily to avoid circular imports
from .artifact_gate import ArtifactGate, ArtifactGateResult
from .quality_gate import QualityGateDecision, QualityGateManager
from .scoring import ScoringEngine, TaskScore
from .trajectory import TrajectoryReport


@dataclass(frozen=True)
class OrchestrationResult:
    action: str
    item_id: str | None = None
    appended_event_ids: tuple[str, ...] = ()
    final_gate_result: str | None = None
    next_status: str | None = None
    digest_summary: Any = None
    warnings: tuple[str, ...] = ()


class Stage1Orchestrator:
    """Thin deterministic orchestration layer over existing Stage 1 components."""

    def __init__(self, event_log_path: str | Path, task_root: str | Path | None = None):
        self.event_log_path = Path(event_log_path)
        self.kernel = Kernel(event_log_path)
        self.batch_runner = BatchRunner(self.kernel)
        self.final_gate = FinalGateRunner()
        self.task_root = Path(task_root) if task_root else None

    def validate(self) -> ReplayPreflightReport:
        return self.kernel.validate()

    def projections(self) -> ProjectionBundle:
        return self.kernel.projections()

    def project_state(self) -> ProjectStateProjection:
        return self.kernel.project_state()

    def task_queue_state(self) -> TaskQueueProjection:
        return self.kernel.task_queue_state()

    def digest(self) -> BatchDigest:
        return generate_batch_digest(self.projections())

    def list_ready_items(self) -> list[ProjectItemState]:
        return self.batch_runner.list_ready_items()

    def run_ready_item(self, item_id: str) -> OrchestrationResult:
        result = self.batch_runner.run_one_ready_item(item_id)
        return OrchestrationResult(
            action="run_ready_item",
            item_id=item_id,
            appended_event_ids=result.appended_event_ids,
            next_status="review",
            digest_summary=result.digest,
        )

    def evaluate_final_gate(
        self, item_id: str, task_dir: Path
    ) -> FinalGateDecision:
        store = TaskRecordStore(task_dir.parent if self.task_root is None else self.task_root)
        bundle = store.load_task_bundle(task_dir)
        project = self.project_state()
        current_status = project.items.get(item_id)
        if current_status is None:
            raise ValueError(f"item not found in project state: {item_id}")
        return self.final_gate.evaluate(bundle, current_item_status=current_status.status)

    def apply_final_gate_decision(
        self, item_id: str, decision: FinalGateDecision
    ) -> OrchestrationResult:
        project = self.project_state()
        current = project.items.get(item_id)
        if current is None:
            raise ValueError(f"item not found in project state: {item_id}")
        if current.status != "review":
            raise ValueError(
                f"item must be in review to apply Final Gate decision, got {current.status}"
            )

        if decision.result == "pass":
            new_status = "done"
            idempotency_key = f"{item_id}:final_gate:pass:v1"
            reason = "Final Gate passed"
        elif decision.result == "pass_with_notes":
            new_status = "done"
            idempotency_key = f"{item_id}:final_gate:pass:v1"
            reason = f"Final Gate passed with notes: {'; '.join(decision.reasons)}"
        elif decision.result == "fail":
            new_status = decision.next_project_status
            idempotency_key = f"{item_id}:final_gate:fail:v1"
            reason = f"Final Gate failed: {'; '.join(decision.reasons)}"
        else:
            raise ValueError(f"unexpected Final Gate result: {decision.result}")

        event_ids = _next_event_ids(self.event_log_path, count=1)
        event = _base_event(
            event_id=event_ids[0],
            timestamp="2026-05-16T00:01:00+08:00",
            event_type="project_item_state_changed",
            payload={
                "project_id": "proj_stage1_closeout",
                "board_version": 1,
                "item_id": item_id,
                "previous_status": current.status,
                "new_status": new_status,
                "reason": reason,
            },
            idempotency_key=idempotency_key,
            parent_event_id=current.last_event_id,
        )
        validate_event(event)
        self.kernel.append_project_event(event)

        digest = generate_batch_digest(self.projections())
        return OrchestrationResult(
            action="apply_final_gate_decision",
            item_id=item_id,
            appended_event_ids=(event["event_id"],),
            final_gate_result=decision.result,
            next_status=new_status,
            digest_summary=digest,
            warnings=decision.reasons,
        )

    def run_one_step(
        self,
        item_id: str | None = None,
        task_dir: Path | None = None,
    ) -> OrchestrationResult:
        self.validate()

        if task_dir is not None and item_id is not None:
            project = self.project_state()
            current = project.items.get(item_id)
            if current is not None and current.status == "review":
                decision = self.evaluate_final_gate(item_id, task_dir)
                return self.apply_final_gate_decision(item_id, decision)

        if item_id is not None:
            return self.run_ready_item(item_id)

        ready_items = self.list_ready_items()
        if not ready_items:
            return OrchestrationResult(
                action="no_op",
                item_id=None,
                warnings=("no ready items and no final gate target",),
            )

        return self.run_ready_item(ready_items[0].item_id)

    def evaluate_quality(
        self,
        item_id: str,
        task_dir: Path,
    ) -> QualityGateDecision:
        """Optional Stage 2 quality evaluation. Does not mutate event log."""
        store = TaskRecordStore(task_dir.parent if self.task_root is None else self.task_root)
        bundle = store.load_task_bundle(task_dir)

        project = self.project_state()
        current = project.items.get(item_id)
        if current is None:
            raise ValueError(f"item not found in project state: {item_id}")

        final_gate_decision = self.final_gate.evaluate(bundle, current_item_status=current.status)
        artifact_result = ArtifactGate().evaluate(bundle)
        score = ScoringEngine().score_task_bundle(bundle, final_gate_decision)

        return QualityGateManager().evaluate(
            bundle,
            final_gate_decision,
            artifact_result,
            task_score=score,
        )

    def advisor_preflight(
        self,
        item_id: str,
        task_dir: Path,
        advisor=None,
    ) -> Any:
        """Optional Stage 3 advisor preflight hook. Does not mutate event log."""
        from .advisor import AdvisorBroker, AdvisorBudget, AdvisorContextPack, StubAdvisorProvider

        if advisor is None:
            broker = AdvisorBroker(StubAdvisorProvider(), AdvisorBudget())
        else:
            broker = advisor

        store = TaskRecordStore(task_dir.parent if self.task_root is None else self.task_root)
        bundle = store.load_task_bundle(task_dir)

        context = AdvisorContextPack(
            task_id=item_id,
            call_type="preflight",
            task_spec=bundle.task_spec,
            completion=bundle.completion,
            handoff_pack=bundle.handoff_pack,
            run_log_text=bundle.run_log_text,
            failure_code=bundle.task_spec.get("failure_code"),
            project_context=None,
        )
        return broker.preflight(context)
