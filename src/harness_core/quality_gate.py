"""Quality Gate Manager for Stage 2 — deterministic quality decisions."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from .artifact_gate import ArtifactGateResult
from .final_gate import FinalGateDecision
from .scoring import TaskScore
from .task_records import TaskRecordBundle
from .trajectory import TrajectoryReport


@dataclass(frozen=True)
class QualityGateDecision:
    result: str  # pass | pass_with_notes | fail_retryable | fail_terminal | requires_human_review
    reasons: tuple[str, ...]
    next_project_status: str  # done | ready | failed | blocked
    score: TaskScore | None = None
    artifact_result: ArtifactGateResult | None = None
    trajectory_result: TrajectoryReport | None = None


class QualityGateManager:
    """Evaluate quality beyond binary pass/fail."""

    def evaluate(
        self,
        bundle: TaskRecordBundle,
        final_gate: FinalGateDecision,
        artifact_result: ArtifactGateResult,
        trajectory_report: TrajectoryReport | None = None,
        task_score: TaskScore | None = None,
    ) -> QualityGateDecision:
        reasons: list[str] = []

        # Check for pending approval — requires human review
        if self._has_pending_approval(bundle):
            reasons.append("approval_request is pending; requires human review")
            return QualityGateDecision(
                result="requires_human_review",
                reasons=tuple(reasons),
                next_project_status="blocked",
                score=task_score,
                artifact_result=artifact_result,
                trajectory_result=trajectory_report,
            )

        # Check trajectory for human-review anomalies
        if trajectory_report and not trajectory_report.ok:
            error_anomalies = [a for a in trajectory_report.anomalies if a.severity == "error"]
            if error_anomalies:
                reasons.append(f"trajectory anomalies detected: {len(error_anomalies)} error(s)")
                for a in error_anomalies[:3]:
                    reasons.append(f"  {a.anomaly_type}: {a.message}")

        # Final Gate pass path
        if final_gate.result in ("pass", "pass_with_notes"):
            score_value = task_score.weighted_score if task_score else 1.0

            if not artifact_result.ok:
                blocking = [c for c in artifact_result.checks if not c.passed]
                reasons.append(f"artifact gate failed: {len(blocking)} check(s)")

            if score_value >= 0.75 and artifact_result.ok and not reasons:
                reasons.extend(final_gate.reasons)
                return QualityGateDecision(
                    result="pass",
                    reasons=tuple(reasons),
                    next_project_status="done",
                    score=task_score,
                    artifact_result=artifact_result,
                    trajectory_result=trajectory_report,
                )

            if score_value >= 0.60:
                reasons.extend(final_gate.reasons)
                if not artifact_result.ok:
                    reasons.append("artifact gate has warnings but score is acceptable")
                return QualityGateDecision(
                    result="pass_with_notes",
                    reasons=tuple(reasons),
                    next_project_status="done",
                    score=task_score,
                    artifact_result=artifact_result,
                    trajectory_result=trajectory_report,
                )

            # Low score despite Final Gate pass
            reasons.append(f"score {score_value:.2f} below 0.60 threshold")
            return QualityGateDecision(
                result="fail_terminal",
                reasons=tuple(reasons),
                next_project_status="failed",
                score=task_score,
                artifact_result=artifact_result,
                trajectory_result=trajectory_report,
            )

        # Final Gate fail path
        retry_count = bundle.completion.get("retry_count", 0)
        score_value = task_score.weighted_score if task_score else 0.0

        if score_value >= 0.40 and retry_count < 3:
            reasons.extend(final_gate.reasons)
            reasons.append(f"score {score_value:.2f} >= 0.40 and retry_count={retry_count} < 3")
            return QualityGateDecision(
                result="fail_retryable",
                reasons=tuple(reasons),
                next_project_status="ready",
                score=task_score,
                artifact_result=artifact_result,
                trajectory_result=trajectory_report,
            )

        reasons.extend(final_gate.reasons)
        if retry_count >= 3:
            reasons.append(f"retry_count={retry_count} >= 3")
        if score_value < 0.40:
            reasons.append(f"score {score_value:.2f} < 0.40")
        return QualityGateDecision(
            result="fail_terminal",
            reasons=tuple(reasons),
            next_project_status="failed",
            score=task_score,
            artifact_result=artifact_result,
            trajectory_result=trajectory_report,
        )

    def _has_pending_approval(self, bundle: TaskRecordBundle) -> bool:
        return self._walk_for_pending_approval(bundle.handoff_pack)

    def _walk_for_pending_approval(self, value: Any) -> bool:
        if isinstance(value, dict):
            if value.get("approval_id") and value.get("decision") == "pending":
                return True
            return any(self._walk_for_pending_approval(v) for v in value.values())
        if isinstance(value, list):
            return any(self._walk_for_pending_approval(v) for v in value)
        return False
