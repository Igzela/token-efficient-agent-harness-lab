"""Tests for Harness Change Evaluation Track.

Deterministic evaluation harness for comparing future harness changes
against fixed fixture suites. Proves that fixture suites produce stable,
repeatable evaluation snapshots.
"""

import hashlib
import json
import sys
import unittest
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core.event_store import replay_preflight
from harness_core.projection_store import replay_all
from harness_core.digest import generate_batch_digest
from harness_core.scoring import ScoringEngine, TaskScore
from harness_core.trajectory import TrajectoryMonitor


ROOT = Path(__file__).resolve().parents[1]
FIXTURE_DIR = ROOT / "tests" / "fixtures" / "harness_change_eval"


@dataclass(frozen=True)
class CaseSnapshot:
    case_id: str
    fixture_path: str
    preflight_ok: bool
    event_count: int
    projection_item_count: int
    digest_completed: tuple[str, ...]
    digest_blocked: tuple[str, ...]
    digest_handoff_count: int
    scoring_aggregate: float
    scoring_grade: str
    trajectory_ok: bool
    trajectory_anomaly_count: int
    quality_gate_result: str
    keep_rate: float | None = None
    user_feedback: float | None = None


@dataclass(frozen=True)
class EvaluationSnapshot:
    snapshot_id: str
    timestamp: str
    harness_commit: str
    suite_id: str
    cases: tuple[CaseSnapshot, ...]
    aggregate_score: float
    aggregate_grade: str
    total_cases: int
    passed_cases: int
    summary: str


@dataclass(frozen=True)
class CaseDelta:
    case_id: str
    before_score: float
    after_score: float
    delta: float
    before_grade: str
    after_grade: str
    status: str


@dataclass(frozen=True)
class BeforeAfterComparison:
    comparison_id: str
    before_snapshot_id: str
    after_snapshot_id: str
    score_delta: float
    grade_changed: bool
    regressed_cases: tuple[str, ...]
    improved_cases: tuple[str, ...]
    unchanged_cases: tuple[str, ...]
    new_cases: tuple[str, ...]
    removed_cases: tuple[str, ...]
    per_case_deltas: tuple[CaseDelta, ...]
    regression_detected: bool
    summary: str


_PASSING_GATES = {"pass", "pass_with_notes"}


def _score_gate_result(preflight_ok, trajectory_ok, event_count, item_count):
    if not preflight_ok:
        return "fail_terminal"
    if not trajectory_ok:
        return "requires_human_review"
    if event_count == 0:
        return "fail_terminal"
    return "pass"


def _grade(score):
    if score >= 0.90:
        return "A"
    if score >= 0.75:
        return "B"
    if score >= 0.60:
        return "C"
    if score >= 0.40:
        return "D"
    return "F"


def build_case_snapshot(case_id, fixture_path):
    preflight = replay_preflight(fixture_path)
    preflight_ok = preflight.ok
    event_count = preflight.event_count

    trajectory = TrajectoryMonitor().analyze_project_stream(fixture_path)

    projection_item_count = 0
    digest_completed = ()
    digest_blocked = ()
    digest_handoff_count = 0
    scoring_aggregate = 0.0
    scoring_grade = "F"

    if preflight_ok:
        projections = replay_all(fixture_path)
        projection_item_count = len(projections.project.items)
        digest = generate_batch_digest(projections)
        digest_completed = digest.completed_items
        digest_blocked = digest.blocked_or_waiting_approval
        digest_handoff_count = digest.handoff_count

        scoring = ScoringEngine()
        task_scores = []
        for item_id, item_state in projections.project.items.items():
            ts = TaskScore(
                task_id=item_id,
                completion_score=1.0 if item_state.status == "done" else 0.0,
                handoff_score=1.0 if any(h.item_id == item_id for h in projections.task_queue.handoffs) else 0.0,
                artifact_score=0.0,
                run_log_score=0.0,
                failure_code_penalty=0.0,
                weighted_score=0.0,
                grade="F",
            )
            task_scores.append(ts)
        if task_scores:
            run_score = scoring.score_run(tuple(task_scores))
            scoring_aggregate = run_score.aggregate_score
            scoring_grade = run_score.grade

    gate_result = _score_gate_result(preflight_ok, trajectory.ok, event_count, projection_item_count)

    return CaseSnapshot(
        case_id=case_id,
        fixture_path=str(fixture_path),
        preflight_ok=preflight_ok,
        event_count=event_count,
        projection_item_count=projection_item_count,
        digest_completed=digest_completed,
        digest_blocked=digest_blocked,
        digest_handoff_count=digest_handoff_count,
        scoring_aggregate=scoring_aggregate,
        scoring_grade=scoring_grade,
        trajectory_ok=trajectory.ok,
        trajectory_anomaly_count=len(trajectory.anomalies),
        quality_gate_result=gate_result,
    )


def build_snapshot(suite_id, fixture_dir):
    cases = []
    for sub in sorted(fixture_dir.iterdir()):
        if sub.is_dir() and (sub / "events.jsonl").exists():
            case = build_case_snapshot(sub.name, sub / "events.jsonl")
            cases.append(case)

    scores = [c.scoring_aggregate for c in cases]
    aggregate = sum(scores) / len(scores) if scores else 0.0
    passed = sum(1 for c in cases if c.quality_gate_result in _PASSING_GATES)

    snapshot_id_input = f"{suite_id}:{json.dumps([c.case_id for c in cases], sort_keys=True)}"
    snapshot_id = hashlib.sha256(snapshot_id_input.encode()).hexdigest()[:16]

    return EvaluationSnapshot(
        snapshot_id=snapshot_id,
        timestamp=datetime.now(timezone.utc).isoformat(),
        harness_commit="placeholder",
        suite_id=suite_id,
        cases=tuple(cases),
        aggregate_score=round(aggregate, 4),
        aggregate_grade=_grade(aggregate),
        total_cases=len(cases),
        passed_cases=passed,
        summary=f"{len(cases)} cases, {passed} passed, aggregate {aggregate:.4f}",
    )


def build_comparison(before: EvaluationSnapshot, after: EvaluationSnapshot) -> BeforeAfterComparison:
    before_by_id = {c.case_id: c for c in before.cases}
    after_by_id = {c.case_id: c for c in after.cases}

    all_ids = sorted(set(before_by_id) | set(after_by_id))
    new_cases = tuple(cid for cid in all_ids if cid not in before_by_id)
    removed_cases = tuple(cid for cid in all_ids if cid not in after_by_id)

    regressed: list[str] = []
    improved: list[str] = []
    unchanged: list[str] = []
    deltas: list[CaseDelta] = []

    for cid in all_ids:
        if cid in before_by_id and cid in after_by_id:
            b = before_by_id[cid]
            a = after_by_id[cid]
            delta = round(a.scoring_aggregate - b.scoring_aggregate, 4)
            b_gate_pass = b.quality_gate_result in _PASSING_GATES
            a_gate_pass = a.quality_gate_result in _PASSING_GATES

            status = "unchanged"
            if a_gate_pass and not b_gate_pass:
                status = "improved"
                improved.append(cid)
            elif not a_gate_pass and b_gate_pass:
                status = "regressed"
                regressed.append(cid)
            elif delta < -0.05:
                status = "regressed"
                regressed.append(cid)
            elif delta > 0.05:
                status = "improved"
                improved.append(cid)
            else:
                unchanged.append(cid)

            deltas.append(CaseDelta(
                case_id=cid,
                before_score=b.scoring_aggregate,
                after_score=a.scoring_aggregate,
                delta=delta,
                before_grade=b.scoring_grade,
                after_grade=a.scoring_grade,
                status=status,
            ))

    score_delta = round(after.aggregate_score - before.aggregate_score, 4)
    grade_changed = before.aggregate_grade != after.aggregate_grade
    regression_detected = len(regressed) > 0 or grade_changed and after.aggregate_grade < before.aggregate_grade

    comp_id_input = f"{before.snapshot_id}:{after.snapshot_id}"
    comp_id = hashlib.sha256(comp_id_input.encode()).hexdigest()[:16]

    parts = [f"Score: {before.aggregate_score:.4f} -> {after.aggregate_score:.4f} (delta {score_delta:+.4f})"]
    if regressed:
        parts.append(f"Regressed: {', '.join(regressed)}")
    if improved:
        parts.append(f"Improved: {', '.join(improved)}")
    if new_cases:
        parts.append(f"New: {', '.join(new_cases)}")
    if removed_cases:
        parts.append(f"Removed: {', '.join(removed_cases)}")

    return BeforeAfterComparison(
        comparison_id=comp_id,
        before_snapshot_id=before.snapshot_id,
        after_snapshot_id=after.snapshot_id,
        score_delta=score_delta,
        grade_changed=grade_changed,
        regressed_cases=tuple(regressed),
        improved_cases=tuple(improved),
        unchanged_cases=tuple(unchanged),
        new_cases=new_cases,
        removed_cases=removed_cases,
        per_case_deltas=tuple(deltas),
        regression_detected=regression_detected,
        summary="; ".join(parts),
    )


class HarnessChangeEvalTests(unittest.TestCase):
    """Prove fixture suites produce stable, comparable evaluation snapshots."""

    def test_fixture_suite_produces_stable_snapshot(self):
        """Run the fixture suite twice and assert identical results (determinism)."""
        snap1 = build_snapshot("hce_stability", FIXTURE_DIR)
        snap2 = build_snapshot("hce_stability", FIXTURE_DIR)

        self.assertEqual(snap1.total_cases, snap2.total_cases)
        self.assertEqual(snap1.passed_cases, snap2.passed_cases)
        self.assertEqual(snap1.aggregate_score, snap2.aggregate_score)
        self.assertEqual(snap1.aggregate_grade, snap2.aggregate_grade)
        self.assertEqual(snap1.snapshot_id, snap2.snapshot_id)

        for c1, c2 in zip(snap1.cases, snap2.cases):
            self.assertEqual(c1.case_id, c2.case_id)
            self.assertEqual(c1.preflight_ok, c2.preflight_ok)
            self.assertEqual(c1.event_count, c2.event_count)
            self.assertEqual(c1.projection_item_count, c2.projection_item_count)
            self.assertEqual(c1.scoring_aggregate, c2.scoring_aggregate)
            self.assertEqual(c1.scoring_grade, c2.scoring_grade)
            self.assertEqual(c1.trajectory_ok, c2.trajectory_ok)
            self.assertEqual(c1.trajectory_anomaly_count, c2.trajectory_anomaly_count)
            self.assertEqual(c1.quality_gate_result, c2.quality_gate_result)
            self.assertIsNone(c1.keep_rate)
            self.assertIsNone(c1.user_feedback)

    def test_comparison_detects_no_regression(self):
        """Baseline vs identical run should report no regressions."""
        before = build_snapshot("hce_baseline", FIXTURE_DIR)
        after = build_snapshot("hce_after", FIXTURE_DIR)

        comp = build_comparison(before, after)

        self.assertFalse(comp.regression_detected)
        self.assertEqual(0.0, comp.score_delta)
        self.assertFalse(comp.grade_changed)
        self.assertEqual((), comp.regressed_cases)
        self.assertEqual((), comp.improved_cases)
        self.assertEqual((), comp.new_cases)
        self.assertEqual((), comp.removed_cases)
        self.assertEqual(before.total_cases, len(comp.unchanged_cases))

    def test_each_fixture_produces_expected_gate_result(self):
        """Each fixture should hit its expected path through the quality gate."""
        snap = build_snapshot("hce_gate_check", FIXTURE_DIR)
        by_id = {c.case_id: c for c in snap.cases}

        self.assertTrue(by_id["good_flow"].preflight_ok)
        self.assertEqual("pass", by_id["good_flow"].quality_gate_result)
        self.assertTrue(by_id["good_flow"].trajectory_ok)

        self.assertFalse(by_id["validation_failure"].preflight_ok)
        self.assertEqual("fail_terminal", by_id["validation_failure"].quality_gate_result)

        self.assertTrue(by_id["trajectory_anomaly"].preflight_ok)
        self.assertFalse(by_id["trajectory_anomaly"].trajectory_ok)
        self.assertEqual("requires_human_review", by_id["trajectory_anomaly"].quality_gate_result)
