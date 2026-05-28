"""Controlled Evaluation Runner for Stage 2."""

from __future__ import annotations

import json
import logging
from dataclasses import dataclass

_log = logging.getLogger(__name__)
from pathlib import Path
from typing import Any

from .event_store import validate_jsonl_file
from .kernel import Kernel
from .orchestrator import Stage1Orchestrator
from .scoring import RunScore, ScoringEngine, TaskScore


@dataclass(frozen=True)
class EvalSpec:
    case_id: str
    fixture_path: Path
    expected_outcome: str  # pass | fail | no_op
    task_dir: Path | None = None
    item_id: str | None = None
    description: str = ""


@dataclass(frozen=True)
class EvalCase:
    case_id: str
    fixture_path: Path
    expected_outcome: str
    actual_outcome: str
    passed: bool
    score: TaskScore | None = None


@dataclass(frozen=True)
class EvaluationReport:
    suite_id: str
    cases: tuple[EvalCase, ...]
    total: int
    passed: int
    failed: int
    score: RunScore | None = None


class EvaluationRunner:
    """Run controlled evaluation over known fixtures."""

    def __init__(self, scoring_engine: ScoringEngine | None = None):
        self.scoring = scoring_engine or ScoringEngine()

    def run_single(self, spec: EvalSpec) -> EvalCase:
        try:
            actual = self._evaluate(spec)
        except Exception:
            _log.exception("evaluation failed for case %s", spec.case_id)
            actual = "error"

        passed = actual == spec.expected_outcome
        return EvalCase(
            case_id=spec.case_id,
            fixture_path=spec.fixture_path,
            expected_outcome=spec.expected_outcome,
            actual_outcome=actual,
            passed=passed,
        )

    def run_suite(self, suite_id: str, cases: tuple[EvalSpec, ...]) -> EvaluationReport:
        results = [self.run_single(case) for case in cases]
        passed = sum(1 for r in results if r.passed)
        failed = len(results) - passed

        task_scores = tuple(r.score for r in results if r.score is not None)
        run_score = self.scoring.score_run(task_scores) if task_scores else None

        return EvaluationReport(
            suite_id=suite_id,
            cases=tuple(results),
            total=len(results),
            passed=passed,
            failed=failed,
            score=run_score,
        )

    def _evaluate(self, spec: EvalSpec) -> str:
        path = spec.fixture_path
        if not path.exists():
            return "fail"

        if path.suffix == ".jsonl":
            return self._evaluate_jsonl(path)

        if path.is_dir():
            return self._evaluate_orchestrator(spec)

        return "fail"

    def _evaluate_jsonl(self, path: Path) -> str:
        report = validate_jsonl_file(path)
        return "pass" if report.ok else "fail"

    def _evaluate_orchestrator(self, spec: EvalSpec) -> str:
        orch = Stage1Orchestrator(spec.fixture_path)
        try:
            orch.validate()
        except Exception:
            _log.exception("orchestrator validation failed for %s", spec.fixture_path)
            return "fail"

        if spec.item_id:
            ready = orch.list_ready_items()
            if any(item.item_id == spec.item_id for item in ready):
                return "pass"
            return "no_op"

        return "pass"
