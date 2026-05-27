"""Dispatch engine: orchestrator for the dispatch kernel."""

from __future__ import annotations

import uuid
from datetime import datetime, timezone
from typing import Any

from .budget_manager import BudgetManager
from .dispatch_decision import (
    EXECUTION_GATE_TYPES,
    BudgetReservation,
    DispatchDecision,
    ExecutionGate,
)
from .dispatch_ledger import DispatchBundle, DispatchLedger, DispatchRecord
from .evaluation_stub import EvaluationStub
from .executor_adapter import ExecutionResult
from .model_selector import ModelSelector
from .task_analyzer import RuleBasedTaskAnalyzer, TaskAnalysis


class DispatchEngine:
    """Orchestrator combining all dispatch components.

    Constructor injection for full testability.
    """

    def __init__(
        self,
        analyzer: RuleBasedTaskAnalyzer | None = None,
        selector: ModelSelector | None = None,
        budget_manager: BudgetManager | None = None,
        executor: Any | None = None,
        evaluator: EvaluationStub | None = None,
        ledger: DispatchLedger | None = None,
    ):
        from .executor_adapter import NoopExecutor
        self._analyzer = analyzer or RuleBasedTaskAnalyzer()
        self._selector = selector or ModelSelector()
        self._budget = budget_manager or BudgetManager()
        self._executor = executor or NoopExecutor()
        self._evaluator = evaluator or EvaluationStub()
        self._ledger = ledger or DispatchLedger()

    def dispatch(
        self,
        raw_request: str,
        request_source: str = "test_fixture",
    ) -> DispatchBundle:
        dispatch_id = f"disp-{uuid.uuid4().hex[:12]}"
        decision_id = f"dec-{uuid.uuid4().hex[:12]}"

        # Step 1: Analyze
        analysis = self._analyzer.analyze(raw_request, request_source=request_source)

        # Step 2: Select model tier
        (
            selected_tier, selected_profile_id,
            fallback_tier, fallback_profile_id,
            shadow_routes, rejected_candidates, routing_reason,
        ) = self._selector.select(analysis)

        # Step 3: Reserve budget
        reservation = self._budget.create_reservation(decision_id, analysis, selected_tier)

        # Step 4: Build execution policy and gates
        execution_policy = self._build_execution_policy(selected_tier, analysis)
        execution_gates = self._build_execution_gates(analysis, reservation, execution_policy)

        # Step 5: Build dispatch decision
        decision = DispatchDecision(
            decision_id=decision_id,
            analysis_id=analysis.analysis_id,
            analysis_snapshot=analysis.to_dict(),
            selected_tier=selected_tier,
            selected_profile_id=selected_profile_id,
            fallback_tier=fallback_tier,
            fallback_profile_id=fallback_profile_id,
            shadow_routes=tuple(shadow_routes),
            hard_constraints=self._derive_hard_constraints(analysis, execution_policy),
            rejected_candidates=tuple(rejected_candidates),
            no_shadow_route_reason=None if shadow_routes else "no_alternatives_available",
            max_input_tokens=analysis.context_budget_estimate,
            max_output_tokens=analysis.execution_budget_estimate,
            routing_reason=routing_reason,
            quality_requirement=analysis.quality_requirement,
            expected_quality_band=self._quality_band(selected_tier),
            confidence=analysis.confidence,
            confidence_label=analysis.confidence_label,
            budget_reservation=reservation,
            execution_policy=execution_policy,
            execution_gates=tuple(execution_gates),
            decision_status=self._determine_decision_status(execution_gates),
            created_at=datetime.now(timezone.utc).isoformat(),
        )

        # Step 6: Create dispatch record
        record = self._ledger.create_record(
            dispatch_id=dispatch_id,
            request_snapshot=raw_request,
            task_analysis_id=analysis.analysis_id,
            decision_id=decision_id,
            budget_reservation_id=reservation.reservation_id,
        )

        # Step 7: Execute (use injected executor)
        exec_result = self._executor.execute(decision, raw_request, dispatch_id)

        # Step 8: Evaluate
        eval_result = self._evaluator.evaluate(exec_result, decision)

        # Step 9: Update ledger
        final_status = self._derive_final_status(exec_result, eval_result)
        record = self._ledger.update_record(
            record,
            final_status=final_status,
            execution_result_id=exec_result.result_id,
            evaluation_result_id=eval_result.evaluation_id,
        )

        # Step 10: Store full chain bundle
        bundle = self._ledger.store_bundle(
            record=record,
            analysis=analysis,
            decision=decision,
            execution_result=exec_result,
            evaluation_result=eval_result,
        )

        return bundle

    def _build_execution_policy(
        self, tier: str, analysis: TaskAnalysis,
    ) -> dict[str, Any]:
        type_name = type(self._executor).__name__
        is_provider = "Provider" in type_name or "provider" in type_name.lower()

        if is_provider:
            executor_type = "provider"
            execution_allowed = True
        else:
            executor_type = type_name.replace("Executor", "").lower()
            if executor_type not in ("noop", "mock", "manual"):
                executor_type = "noop"
            execution_allowed = True

        requires_review = (
            analysis.risk_level in ("critical", "high")
            or analysis.confidence_label == "low"
        )

        return {
            "executor_type": executor_type,
            "execution_allowed": execution_allowed,
            "requires_human_review": requires_review,
            "max_retries": 0,
        }

    def _build_execution_gates(
        self,
        analysis: TaskAnalysis,
        reservation: BudgetReservation,
        execution_policy: dict[str, Any],
    ) -> list[ExecutionGate]:
        gates: list[ExecutionGate] = []

        is_provider = execution_policy.get("executor_type") == "provider"

        if not is_provider:
            gates.append(ExecutionGate(
                gate_id=f"gate-{uuid.uuid4().hex[:8]}",
                gate_type="provider_disabled",
                severity="info",
                reason="real provider calls disabled — non-provider executor",
                clearance_required="policy",
        ))

        gates.append(ExecutionGate(
            gate_id=f"gate-{uuid.uuid4().hex[:8]}",
            gate_type="sandbox_disabled",
            severity="info",
            reason="sandbox execution disabled in Phase 1",
            clearance_required="policy",
        ))

        # Risk gate
        if analysis.risk_level in ("critical", "high"):
            gates.append(ExecutionGate(
                gate_id=f"gate-{uuid.uuid4().hex[:8]}",
                gate_type="risk",
                severity="block",
                reason=f"risk_level={analysis.risk_level}",
                clearance_required="human",
            ))

        # Target write gate
        if "target_write" in analysis.risk_flags:
            gates.append(ExecutionGate(
                gate_id=f"gate-{uuid.uuid4().hex[:8]}",
                gate_type="target_write",
                severity="block",
                reason="target_write risk flag detected",
                clearance_required="human",
            ))

        # Confidence gate
        if analysis.confidence_label == "low":
            gates.append(ExecutionGate(
                gate_id=f"gate-{uuid.uuid4().hex[:8]}",
                gate_type="confidence",
                severity="warning",
                reason=f"confidence={analysis.confidence:.2f} below threshold",
                clearance_required="none",
            ))

        # Budget gate
        if reservation.budget_violation:
            gates.append(ExecutionGate(
                gate_id=f"gate-{uuid.uuid4().hex[:8]}",
                gate_type="budget",
                severity="block",
                reason="budget reservation violated",
                clearance_required="human",
            ))

        # Boundary gate
        if "provider_call" in analysis.risk_flags or "sandbox_execution" in analysis.risk_flags:
            gates.append(ExecutionGate(
                gate_id=f"gate-{uuid.uuid4().hex[:8]}",
                gate_type="boundary",
                severity="block",
                reason="boundary violation detected (provider/sandbox)",
                clearance_required="human",
            ))

        # Manual review gate
        if execution_policy.get("requires_human_review"):
            gates.append(ExecutionGate(
                gate_id=f"gate-{uuid.uuid4().hex[:8]}",
                gate_type="manual_review",
                severity="block",
                reason="high risk or low confidence requires human review",
                clearance_required="human",
            ))

        return gates

    def _derive_hard_constraints(
        self, analysis: TaskAnalysis, execution_policy: dict[str, Any],
    ) -> tuple[str, ...]:
        constraints: list[str] = ["no_target_write"]
        if execution_policy.get("executor_type") != "provider":
            constraints.append("no_provider_call")
        if analysis.risk_level == "critical":
            constraints.append("requires_human_approval")
        return tuple(constraints)

    def _quality_band(self, tier: str) -> str:
        bands = {
            "cheap_executor": "low",
            "balanced_worker": "medium",
            "strong_planner": "high",
            "verifier": "high",
            "advisor": "high",
        }
        return bands.get(tier, "unknown")

    def _determine_decision_status(self, gates: list[ExecutionGate]) -> str:
        blocking = [g for g in gates if g.severity in ("block", "critical")]
        if blocking:
            return "needs_approval"
        return "decided"

    def _derive_final_status(
        self, exec_result: ExecutionResult, eval_result: Any
    ) -> str:
        if exec_result.status == "not_executed":
            return "not_executed"
        if exec_result.status == "failed":
            return "failed"
        if exec_result.status == "manual_pending":
            return "manual_pending"
        if eval_result.status == "fail":
            return "failed"
        if eval_result.status == "needs_human_review":
            return "escalated"
        return "completed"
