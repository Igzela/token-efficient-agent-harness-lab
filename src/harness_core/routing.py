"""Routing Experiment Manager for Stage 3 — observational routing experiments."""

from __future__ import annotations

import logging
from dataclasses import dataclass
from typing import Any

_log = logging.getLogger(__name__)

from .evaluation import EvaluationReport, EvaluationRunner, EvalSpec
from .scoring import RunScore, ScoringEngine


@dataclass(frozen=True)
class RoutingPolicy:
    policy_id: str
    tier_map: dict[str, str]  # task_type -> model_tier
    description: str


@dataclass(frozen=True)
class RoutingExperimentSpec:
    experiment_id: str
    policies: tuple[RoutingPolicy, ...]
    eval_cases: tuple[EvalSpec, ...]
    description: str


@dataclass(frozen=True)
class RoutingExperimentResult:
    policy_id: str
    run_score: RunScore
    eval_report: EvaluationReport


@dataclass(frozen=True)
class RoutingExperimentReport:
    experiment_id: str
    results: tuple[RoutingExperimentResult, ...]
    best_policy_id: str
    score_delta: float
    recommendation: str  # "adopt" | "no_change" | "needs_more_data"


class RoutingExperimentManager:
    """Run controlled routing experiments. Observational only."""

    def __init__(
        self,
        scoring: ScoringEngine | None = None,
        evaluator: EvaluationRunner | None = None,
    ):
        self._scoring = scoring or ScoringEngine()
        self._evaluator = evaluator or EvaluationRunner(self._scoring)

    def run_experiment(
        self, spec: RoutingExperimentSpec
    ) -> RoutingExperimentReport:
        results: list[RoutingExperimentResult] = []
        scores: list[tuple[str, float]] = []

        for policy in spec.policies:
            try:
                report = self._evaluator.run_suite(
                    suite_id=f"{spec.experiment_id}:{policy.policy_id}",
                    cases=spec.eval_cases,
                )
                score = self._compute_policy_score(policy, report)
                scores.append((policy.policy_id, score))
                run_score = self._make_run_score(report, policy.policy_id)
                results.append(
                    RoutingExperimentResult(
                        policy_id=policy.policy_id,
                        run_score=run_score,
                        eval_report=report,
                    )
                )
            except Exception:
                _log.exception("routing experiment failed for policy %s", policy.policy_id)
                run_score = RunScore(
                    run_id=f"{spec.experiment_id}:{policy.policy_id}",
                    task_scores=(),
                    aggregate_score=0.0,
                    grade="F",
                    item_count=0,
                    passed_count=0,
                    failed_count=0,
                )
                scores.append((policy.policy_id, 0.0))
                results.append(
                    RoutingExperimentResult(
                        policy_id=policy.policy_id,
                        run_score=run_score,
                        eval_report=EvaluationReport(
                            suite_id=policy.policy_id,
                            cases=(),
                            total=0,
                            passed=0,
                            failed=0,
                        ),
                    )
                )

        best_policy_id, score_delta, recommendation = self._analyze_results(
            scores, len(spec.eval_cases)
        )

        return RoutingExperimentReport(
            experiment_id=spec.experiment_id,
            results=tuple(results),
            best_policy_id=best_policy_id,
            score_delta=score_delta,
            recommendation=recommendation,
        )

    def _compute_policy_score(
        self, policy: RoutingPolicy, report: EvaluationReport
    ) -> float:
        if report.score is not None:
            return report.score.aggregate_score
        if report.total == 0:
            return 0.0
        return report.passed / report.total

    def _make_run_score(
        self, report: EvaluationReport, policy_id: str
    ) -> RunScore:
        if report.score is not None:
            return report.score
        return RunScore(
            run_id=policy_id,
            task_scores=(),
            aggregate_score=report.passed / report.total if report.total > 0 else 0.0,
            grade="F",
            item_count=report.total,
            passed_count=report.passed,
            failed_count=report.failed,
        )

    def _analyze_results(
        self, scores: list[tuple[str, float]], case_count: int
    ) -> tuple[str, float, str]:
        if not scores:
            return "", 0.0, "needs_more_data"

        sorted_scores = sorted(scores, key=lambda x: x[1], reverse=True)
        best_id, best_score = sorted_scores[0]

        if len(scores) < 2:
            return best_id, 0.0, "needs_more_data"

        second_id, second_score = sorted_scores[1]
        delta = best_score - second_score

        if case_count < 3:
            return best_id, delta, "needs_more_data"

        if delta > 0.10:
            return best_id, delta, "adopt"

        return best_id, delta, "no_change"
