"""Baseline Run Manager for Stage 2 — store and compare evaluation results."""

from __future__ import annotations

import json
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from .evaluation import EvaluationReport
from .scoring import RunScore


@dataclass(frozen=True)
class BaselineRecord:
    baseline_id: str
    timestamp: str
    run_score: RunScore
    evaluation_report: EvaluationReport
    metadata: dict[str, Any]


@dataclass(frozen=True)
class BaselineComparison:
    baseline_id: str
    current_run_score: RunScore
    score_delta: float
    regression_detected: bool
    improved_cases: tuple[str, ...]
    regressed_cases: tuple[str, ...]


class BaselineManager:
    """Store evaluation baselines as JSON files and compare against them."""

    def __init__(self, baseline_dir: str | Path):
        self.baseline_dir = Path(baseline_dir)

    def save_baseline(
        self,
        report: EvaluationReport,
        score: RunScore,
        metadata: dict[str, Any] | None = None,
    ) -> BaselineRecord:
        self.baseline_dir.mkdir(parents=True, exist_ok=True)
        timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        baseline_id = f"baseline_{timestamp}"

        record = BaselineRecord(
            baseline_id=baseline_id,
            timestamp=timestamp,
            run_score=score,
            evaluation_report=report,
            metadata=metadata or {},
        )

        path = self.baseline_dir / f"{baseline_id}.json"
        path.write_text(
            json.dumps(_serialize_baseline(record), indent=2, sort_keys=True),
            encoding="utf-8",
        )
        return record

    def load_latest_baseline(self) -> BaselineRecord | None:
        if not self.baseline_dir.exists():
            return None

        files = sorted(self.baseline_dir.glob("baseline_*.json"))
        if not files:
            return None

        return _deserialize_baseline(json.loads(files[-1].read_text(encoding="utf-8")))

    def compare(
        self,
        current: EvaluationReport,
        current_score: RunScore,
    ) -> BaselineComparison | None:
        baseline = self.load_latest_baseline()
        if baseline is None:
            return None

        score_delta = current_score.aggregate_score - baseline.run_score.aggregate_score

        baseline_outcomes = {c.case_id: c.actual_outcome for c in baseline.evaluation_report.cases}
        current_outcomes = {c.case_id: c.actual_outcome for c in current.cases}

        improved: list[str] = []
        regressed: list[str] = []

        for case_id in set(list(baseline_outcomes.keys()) + list(current_outcomes.keys())):
            base = baseline_outcomes.get(case_id)
            curr = current_outcomes.get(case_id)
            if base == "fail" and curr == "pass":
                improved.append(case_id)
            elif base == "pass" and curr == "fail":
                regressed.append(case_id)

        return BaselineComparison(
            baseline_id=baseline.baseline_id,
            current_run_score=current_score,
            score_delta=round(score_delta, 4),
            regression_detected=len(regressed) > 0,
            improved_cases=tuple(improved),
            regressed_cases=tuple(regressed),
        )


def _serialize_baseline(record: BaselineRecord) -> dict[str, Any]:
    return {
        "baseline_id": record.baseline_id,
        "timestamp": record.timestamp,
        "run_score": _serialize_run_score(record.run_score),
        "evaluation_report": _serialize_eval_report(record.evaluation_report),
        "metadata": record.metadata,
    }


def _serialize_run_score(score: RunScore) -> dict[str, Any]:
    return {
        "run_id": score.run_id,
        "aggregate_score": score.aggregate_score,
        "grade": score.grade,
        "item_count": score.item_count,
        "passed_count": score.passed_count,
        "failed_count": score.failed_count,
    }


def _serialize_eval_report(report: EvaluationReport) -> dict[str, Any]:
    return {
        "suite_id": report.suite_id,
        "total": report.total,
        "passed": report.passed,
        "failed": report.failed,
        "cases": [
            {
                "case_id": c.case_id,
                "expected_outcome": c.expected_outcome,
                "actual_outcome": c.actual_outcome,
                "passed": c.passed,
            }
            for c in report.cases
        ],
    }


def _deserialize_baseline(data: dict[str, Any]) -> BaselineRecord:
    score_data = data["run_score"]
    run_score = RunScore(
        run_id=score_data["run_id"],
        task_scores=(),
        aggregate_score=score_data["aggregate_score"],
        grade=score_data["grade"],
        item_count=score_data["item_count"],
        passed_count=score_data["passed_count"],
        failed_count=score_data["failed_count"],
    )

    report_data = data["evaluation_report"]
    from .evaluation import EvalCase

    cases = tuple(
        EvalCase(
            case_id=c["case_id"],
            fixture_path=Path(""),  # not stored
            expected_outcome=c["expected_outcome"],
            actual_outcome=c["actual_outcome"],
            passed=c["passed"],
        )
        for c in report_data["cases"]
    )
    eval_report = EvaluationReport(
        suite_id=report_data["suite_id"],
        cases=cases,
        total=report_data["total"],
        passed=report_data["passed"],
        failed=report_data["failed"],
    )

    return BaselineRecord(
        baseline_id=data["baseline_id"],
        timestamp=data["timestamp"],
        run_score=run_score,
        evaluation_report=eval_report,
        metadata=data.get("metadata", {}),
    )
