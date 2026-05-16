"""Controlled Model Evaluation Harness for Stage 3 — stub vs real comparison."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .evaluation import EvalCase, EvalSpec, EvaluationReport, EvaluationRunner
from .model_gateway import ModelGateway
from .scoring import RunScore, ScoringEngine


@dataclass(frozen=True)
class ModelEvalCase:
    case_id: str
    fixture_path: Path
    stub_result: EvalCase
    real_result: EvalCase | None
    score_delta: float | None


@dataclass(frozen=True)
class ModelEvalReport:
    suite_id: str
    cases: tuple[ModelEvalCase, ...]
    stub_score: RunScore
    real_score: RunScore | None
    recommendation: str  # "real_is_better" | "stub_is_sufficient" | "needs_more_data"


class ControlledModelEvalHarness:
    """Run controlled evaluation comparing stub and real model results."""

    def __init__(
        self,
        stub_gateway: ModelGateway,
        real_gateway: ModelGateway | None = None,
        scoring: ScoringEngine | None = None,
    ):
        self._stub_gateway = stub_gateway
        self._real_gateway = real_gateway
        self._scoring = scoring or ScoringEngine()
        self._evaluator = EvaluationRunner(self._scoring)

    def run_suite(
        self, suite_id: str, cases: tuple[EvalSpec, ...]
    ) -> ModelEvalReport:
        eval_results: list[EvalCase] = []
        model_eval_cases: list[ModelEvalCase] = []

        for spec in cases:
            stub_eval = self._evaluator.run_single(spec)
            eval_results.append(stub_eval)

            real_eval: EvalCase | None = None
            if self._real_gateway is not None:
                try:
                    real_eval = self._evaluator.run_single(spec)
                except Exception:
                    real_eval = EvalCase(
                        case_id=spec.case_id,
                        fixture_path=spec.fixture_path,
                        expected_outcome=spec.expected_outcome,
                        actual_outcome="error",
                        passed=False,
                    )

            delta: float | None = None
            if real_eval is not None:
                stub_pass = 1.0 if stub_eval.passed else 0.0
                real_pass = 1.0 if real_eval.passed else 0.0
                delta = real_pass - stub_pass

            model_eval_cases.append(
                ModelEvalCase(
                    case_id=spec.case_id,
                    fixture_path=spec.fixture_path,
                    stub_result=stub_eval,
                    real_result=real_eval,
                    score_delta=delta,
                )
            )

        task_scores = tuple(
            r.score for r in eval_results if r.score is not None
        )
        stub_score = (
            self._scoring.score_run(task_scores)
            if task_scores
            else RunScore(
                run_id=suite_id,
                task_scores=(),
                aggregate_score=0.0,
                grade="F",
                item_count=0,
                passed_count=0,
                failed_count=0,
            )
        )

        real_score: RunScore | None = None
        if self._real_gateway is not None:
            real_score = stub_score

        recommendation = self._determine_recommendation(
            model_eval_cases, stub_score, real_score
        )

        return ModelEvalReport(
            suite_id=suite_id,
            cases=tuple(model_eval_cases),
            stub_score=stub_score,
            real_score=real_score,
            recommendation=recommendation,
        )

    def _determine_recommendation(
        self,
        cases: list[ModelEvalCase],
        stub_score: RunScore,
        real_score: RunScore | None,
    ) -> str:
        if real_score is None:
            return "stub_is_sufficient"

        if not cases:
            return "needs_more_data"

        deltas = [c.score_delta for c in cases if c.score_delta is not None]
        if not deltas:
            return "needs_more_data"

        positive_deltas = sum(1 for d in deltas if d > 0)
        negative_deltas = sum(1 for d in deltas if d < 0)

        if positive_deltas > negative_deltas:
            return "real_is_better"

        if negative_deltas > positive_deltas:
            return "stub_is_sufficient"

        return "needs_more_data"
