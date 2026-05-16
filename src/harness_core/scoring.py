"""Deterministic rule-based scoring engine for Stage 2."""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from .final_gate import FinalGateDecision
from .task_records import TaskRecordBundle
from .validators import CANONICAL_FAILURE_CODES


@dataclass(frozen=True)
class ScoreComponent:
    name: str
    weight: float
    raw_score: float
    weighted_score: float
    penalties: tuple[str, ...] = ()


@dataclass(frozen=True)
class ArtifactScore:
    artifact_id: str
    existence_ok: bool
    schema_ok: bool
    evidence_refs_ok: bool
    score: float
    penalties: tuple[str, ...] = ()


@dataclass(frozen=True)
class TaskScore:
    task_id: str
    completion_score: float
    handoff_score: float
    artifact_score: float
    run_log_score: float
    failure_code_penalty: float
    weighted_score: float
    grade: str
    penalties: tuple[str, ...] = ()


@dataclass(frozen=True)
class RunScore:
    run_id: str
    task_scores: tuple[TaskScore, ...]
    aggregate_score: float
    grade: str
    item_count: int
    passed_count: int
    failed_count: int


def _clamp(value: float, lo: float = 0.0, hi: float = 1.0) -> float:
    return max(lo, min(hi, value))


def _grade(score: float) -> str:
    if score >= 0.90:
        return "A"
    if score >= 0.75:
        return "B"
    if score >= 0.60:
        return "C"
    if score >= 0.40:
        return "D"
    return "F"


class ScoringEngine:
    """Deterministic rule-based scoring. No model judge. No external calls."""

    def score_task_bundle(
        self,
        bundle: TaskRecordBundle,
        decision: FinalGateDecision,
    ) -> TaskScore:
        penalties: list[str] = []

        completion_score = self._score_completion(bundle, penalties)
        handoff_score = self._score_handoff(bundle, penalties)
        artifact_score = self._score_artifacts(bundle, penalties)
        run_log_score = self._score_run_log(bundle, penalties)
        failure_penalty = self._score_failure_code(bundle, penalties)

        raw = (
            0.25 * completion_score
            + 0.20 * handoff_score
            + 0.25 * artifact_score
            + 0.10 * run_log_score
        )
        weighted = _clamp(raw + failure_penalty)

        return TaskScore(
            task_id=bundle.task_spec.get("task_id", "<unknown>"),
            completion_score=completion_score,
            handoff_score=handoff_score,
            artifact_score=artifact_score,
            run_log_score=run_log_score,
            failure_code_penalty=failure_penalty,
            weighted_score=weighted,
            grade=_grade(weighted),
            penalties=tuple(penalties),
        )

    def score_artifact(
        self,
        artifact_ref: dict[str, Any],
        bundle: TaskRecordBundle,
    ) -> ArtifactScore:
        penalties: list[str] = []
        artifact_path = artifact_ref.get("path", "")
        artifact_id = artifact_ref.get("artifact_id", artifact_path)

        existence_ok = False
        if artifact_path:
            full_path = bundle.task_dir / artifact_path
            existence_ok = full_path.exists()
        if not existence_ok:
            penalties.append(f"artifact not found: {artifact_path}")

        schema_ok = bool(bundle.completion and bundle.handoff_pack)
        if not schema_ok:
            penalties.append("completion or handoff_pack missing")

        evidence_refs_ok = False
        evidence_refs = bundle.handoff_pack.get("evidence_refs", [])
        if evidence_refs and isinstance(evidence_refs, list):
            evidence_refs_ok = all(
                isinstance(ref, dict) and ref.get("path")
                for ref in evidence_refs
            )
        if not evidence_refs_ok:
            penalties.append("evidence_refs invalid or empty")

        sub_scores = [
            0.40 if existence_ok else 0.0,
            0.30 if schema_ok else 0.0,
            0.30 if evidence_refs_ok else 0.0,
        ]
        score = _clamp(sum(sub_scores))

        return ArtifactScore(
            artifact_id=artifact_id,
            existence_ok=existence_ok,
            schema_ok=schema_ok,
            evidence_refs_ok=evidence_refs_ok,
            score=score,
            penalties=tuple(penalties),
        )

    def score_run(self, task_scores: tuple[TaskScore, ...]) -> RunScore:
        if not task_scores:
            return RunScore(
                run_id="<empty>",
                task_scores=(),
                aggregate_score=0.0,
                grade="F",
                item_count=0,
                passed_count=0,
                failed_count=0,
            )

        aggregate = _clamp(
            sum(ts.weighted_score for ts in task_scores) / len(task_scores)
        )
        passed = sum(1 for ts in task_scores if ts.weighted_score >= 0.60)
        failed = len(task_scores) - passed

        return RunScore(
            run_id="<run>",
            task_scores=task_scores,
            aggregate_score=aggregate,
            grade=_grade(aggregate),
            item_count=len(task_scores),
            passed_count=passed,
            failed_count=failed,
        )

    def _score_completion(self, bundle: TaskRecordBundle, penalties: list[str]) -> float:
        completion = bundle.completion
        if not completion:
            penalties.append("completion.json missing or empty")
            return 0.0

        status = completion.get("status")
        exit_code = completion.get("exit_code")

        if status == "completed" and exit_code == 0:
            return 1.0

        if status == "completed" and exit_code != 0:
            penalties.append(f"completion exit_code={exit_code}, expected 0")
            return 0.3

        penalties.append(f"completion status={status}, expected completed")
        return 0.0

    def _score_handoff(self, bundle: TaskRecordBundle, penalties: list[str]) -> float:
        pack = bundle.handoff_pack
        if not pack:
            penalties.append("handoff_pack.json missing or empty")
            return 0.0

        score = 1.0
        for field_name in ("structured_fields", "summary", "evidence_refs"):
            if not pack.get(field_name):
                penalties.append(f"handoff_pack.{field_name} missing")
                score -= 0.34

        return _clamp(score)

    def _score_artifacts(self, bundle: TaskRecordBundle, penalties: list[str]) -> float:
        artifact_refs = bundle.completion.get("artifact_refs", [])
        if not artifact_refs:
            penalties.append("no artifact_refs in completion")
            return 0.0

        scores = [self.score_artifact(ref, bundle) for ref in artifact_refs]
        avg = sum(a.score for a in scores) / len(scores) if scores else 0.0
        return _clamp(avg)

    def _score_run_log(self, bundle: TaskRecordBundle, penalties: list[str]) -> float:
        if bundle.run_log_path is None:
            penalties.append("run_log.md missing")
            return 0.0
        if bundle.run_log_text and len(bundle.run_log_text.strip()) < 20:
            penalties.append("run_log.md very short")
            return 0.5
        return 1.0

    def _score_failure_code(self, bundle: TaskRecordBundle, penalties: list[str]) -> float:
        failure_code = bundle.task_spec.get("failure_code") or bundle.completion.get("failure_code")
        if failure_code and failure_code in CANONICAL_FAILURE_CODES:
            penalties.append(f"canonical failure_code: {failure_code}")
            return -0.20
        return 0.0
