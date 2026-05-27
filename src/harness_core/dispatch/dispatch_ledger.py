"""Dispatch ledger: in-memory store for DispatchRecords and full-chain bundles."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any

from .dispatch_decision import DispatchDecision
from .evaluation_stub import EvaluationResult
from .executor_adapter import ExecutionResult
from .task_analyzer import TaskAnalysis

# ---------------------------------------------------------------------------
# Schema version
# ---------------------------------------------------------------------------

DISPATCH_RECORD_SCHEMA_VERSION = "dispatch_record.v1"

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

DISPATCH_STATUSES: tuple[str, ...] = (
    "dispatched", "executing", "completed", "failed", "escalated",
    "cancelled", "not_executed", "manual_pending",
)


# ---------------------------------------------------------------------------
# Schema
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class DispatchRecord:
    dispatch_id: str
    request_snapshot: str
    task_analysis_id: str
    decision_id: str
    final_status: str  # from DISPATCH_STATUSES
    created_at: str
    updated_at: str
    execution_result_id: str | None = None
    evaluation_result_id: str | None = None
    usage_ledger_row_id: str | None = None
    budget_reservation_id: str | None = None
    schema_version: str = DISPATCH_RECORD_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "dispatch_id": self.dispatch_id,
            "request_snapshot": self.request_snapshot,
            "task_analysis_id": self.task_analysis_id,
            "decision_id": self.decision_id,
            "execution_result_id": self.execution_result_id,
            "evaluation_result_id": self.evaluation_result_id,
            "usage_ledger_row_id": self.usage_ledger_row_id,
            "budget_reservation_id": self.budget_reservation_id,
            "final_status": self.final_status,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
        }


@dataclass(frozen=True)
class DispatchBundle:
    """Full evidence chain for a single dispatch."""
    record: DispatchRecord
    analysis: TaskAnalysis
    decision: DispatchDecision
    execution_result: ExecutionResult
    evaluation_result: EvaluationResult

    def to_dict(self) -> dict[str, Any]:
        return {
            "record": self.record.to_dict(),
            "analysis": self.analysis.to_dict(),
            "decision": self.decision.to_dict(),
            "execution_result": self.execution_result.to_dict(),
            "evaluation_result": self.evaluation_result.to_dict(),
        }


# ---------------------------------------------------------------------------
# Ledger
# ---------------------------------------------------------------------------


class DispatchLedger:
    """In-memory dispatch record store with full-chain bundle support."""

    def __init__(self) -> None:
        self._records: dict[str, DispatchRecord] = {}
        self._bundles: dict[str, DispatchBundle] = {}

    def create_record(
        self,
        dispatch_id: str,
        request_snapshot: str,
        task_analysis_id: str,
        decision_id: str,
        budget_reservation_id: str | None = None,
    ) -> DispatchRecord:
        now = datetime.now(timezone.utc).isoformat()
        record = DispatchRecord(
            dispatch_id=dispatch_id,
            request_snapshot=request_snapshot,
            task_analysis_id=task_analysis_id,
            decision_id=decision_id,
            budget_reservation_id=budget_reservation_id,
            final_status="dispatched",
            created_at=now,
            updated_at=now,
        )
        self._records[dispatch_id] = record
        return record

    def update_record(
        self,
        record: DispatchRecord,
        final_status: str | None = None,
        execution_result_id: str | None = None,
        evaluation_result_id: str | None = None,
        usage_ledger_row_id: str | None = None,
    ) -> DispatchRecord:
        updated = DispatchRecord(
            dispatch_id=record.dispatch_id,
            request_snapshot=record.request_snapshot,
            task_analysis_id=record.task_analysis_id,
            decision_id=record.decision_id,
            final_status=final_status or record.final_status,
            created_at=record.created_at,
            updated_at=datetime.now(timezone.utc).isoformat(),
            execution_result_id=execution_result_id or record.execution_result_id,
            evaluation_result_id=evaluation_result_id or record.evaluation_result_id,
            usage_ledger_row_id=usage_ledger_row_id or record.usage_ledger_row_id,
            budget_reservation_id=record.budget_reservation_id,
        )
        self._records[record.dispatch_id] = updated
        return updated

    def store_bundle(
        self,
        record: DispatchRecord,
        analysis: TaskAnalysis,
        decision: DispatchDecision,
        execution_result: ExecutionResult,
        evaluation_result: EvaluationResult,
    ) -> DispatchBundle:
        bundle = DispatchBundle(
            record=record,
            analysis=analysis,
            decision=decision,
            execution_result=execution_result,
            evaluation_result=evaluation_result,
        )
        self._bundles[record.dispatch_id] = bundle
        return bundle

    def get_record(self, dispatch_id: str) -> DispatchRecord | None:
        return self._records.get(dispatch_id)

    def get_bundle(self, dispatch_id: str) -> DispatchBundle | None:
        return self._bundles.get(dispatch_id)

    def list_records(self) -> list[DispatchRecord]:
        return list(self._records.values())

    def list_bundles(self) -> list[DispatchBundle]:
        return list(self._bundles.values())

    def replay(self, dispatch_id: str) -> DispatchBundle | DispatchRecord | None:
        bundle = self._bundles.get(dispatch_id)
        if bundle:
            return bundle
        return self._records.get(dispatch_id)
