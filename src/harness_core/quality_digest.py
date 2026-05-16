"""Quality Digest Generator for Stage 2 — enriched batch digest."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from .baseline import BaselineComparison
from .projection_store import ProjectionBundle
from .quality_gate import QualityGateDecision
from .scoring import RunScore
from .trajectory import TrajectoryReport


@dataclass(frozen=True)
class QualityDigestItem:
    item_id: str
    status: str
    quality_gate_result: str
    score: float
    grade: str
    anomalies: tuple[str, ...]


@dataclass(frozen=True)
class QualityDigest:
    batch_id: str
    items: tuple[QualityDigestItem, ...]
    aggregate_score: float
    aggregate_grade: str
    trajectory_ok: bool
    baseline_delta: float | None
    summary: str
    recommended_actions: tuple[str, ...]


class QualityDigestGenerator:
    """Generate quality-enriched batch digest."""

    def generate(
        self,
        projections: ProjectionBundle,
        run_score: RunScore,
        gate_decisions: dict[str, QualityGateDecision],
        trajectory: TrajectoryReport,
        baseline: BaselineComparison | None = None,
    ) -> QualityDigest:
        items: list[QualityDigestItem] = []
        for item_id, item_state in sorted(projections.project.items.items()):
            gate = gate_decisions.get(item_id)
            score_val = gate.score.weighted_score if gate and gate.score else 0.0
            grade_val = gate.score.grade if gate and gate.score else "F"
            result = gate.result if gate else "not_evaluated"

            anomalies: list[str] = []
            for a in trajectory.anomalies:
                if a.item_id == item_id:
                    anomalies.append(f"{a.anomaly_type}: {a.message}")

            items.append(
                QualityDigestItem(
                    item_id=item_id,
                    status=item_state.status,
                    quality_gate_result=result,
                    score=score_val,
                    grade=grade_val,
                    anomalies=tuple(anomalies),
                )
            )

        actions = self._recommend_actions(items, trajectory, baseline)

        baseline_delta = baseline.score_delta if baseline else None

        return QualityDigest(
            batch_id=projections.project.items[list(projections.project.items.keys())[0]].last_event_id
            if projections.project.items
            else "<empty>",
            items=tuple(items),
            aggregate_score=run_score.aggregate_score,
            aggregate_grade=run_score.grade,
            trajectory_ok=trajectory.ok,
            baseline_delta=baseline_delta,
            summary=self._build_summary(items, run_score, trajectory, baseline),
            recommended_actions=tuple(actions),
        )

    def _build_summary(
        self,
        items: list[QualityDigestItem],
        run_score: RunScore,
        trajectory: TrajectoryReport,
        baseline: BaselineComparison | None,
    ) -> str:
        parts = [
            f"Run: {run_score.item_count} items, score {run_score.aggregate_score:.2f} ({run_score.grade})",
            f"Passed: {run_score.passed_count}, Failed: {run_score.failed_count}",
        ]
        if not trajectory.ok:
            parts.append(f"Trajectory: {len(trajectory.anomalies)} anomaly/anomalies detected")
        if baseline:
            delta_sign = "+" if baseline.score_delta >= 0 else ""
            parts.append(f"Baseline delta: {delta_sign}{baseline.score_delta:.2f}")
            if baseline.regression_detected:
                parts.append(f"REGRESSION: {len(baseline.regressed_cases)} case(s) regressed")
        return "; ".join(parts)

    def _recommend_actions(
        self,
        items: list[QualityDigestItem],
        trajectory: TrajectoryReport,
        baseline: BaselineComparison | None,
    ) -> list[str]:
        actions: list[str] = []

        for item in items:
            if item.quality_gate_result == "fail_retryable":
                actions.append(f"Retry item {item.item_id} (score {item.score:.2f})")
            if item.quality_gate_result == "requires_human_review":
                actions.append(f"Human review required for item {item.item_id}")
            if item.quality_gate_result == "fail_terminal":
                actions.append(f"Item {item.item_id} failed terminally; investigate")

        if baseline and baseline.regression_detected:
            actions.append(
                f"Regression detected in: {', '.join(baseline.regressed_cases)}"
            )

        if not trajectory.ok:
            actions.append("Investigate trajectory anomalies before next run")

        return actions
