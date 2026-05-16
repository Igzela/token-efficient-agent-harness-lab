import shutil
import sys
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core import (
    ArtifactGate,
    BaselineComparison,
    FinalGateRunner,
    QualityDigestGenerator,
    QualityGateManager,
    ScoringEngine,
    TaskRecordStore,
    TrajectoryReport,
    replay_all,
)
from harness_core.scoring import RunScore


ROOT = Path(__file__).resolve().parents[1]
TASK_005 = ROOT / "docs" / "stage0" / "tasks" / "task-005-failure-fix-loop"


def load_context(temp_dir: str):
    task_dir = Path(temp_dir) / TASK_005.name
    shutil.copytree(TASK_005, task_dir)
    store = TaskRecordStore(Path(temp_dir))
    bundle = store.load_task_bundle(task_dir)
    completion = dict(bundle.completion)
    completion["artifact_refs"] = [{"artifact_id": "run_log", "path": "run_log.md"}]
    bundle = replace(bundle, completion=completion)
    return bundle


class QualityDigestGeneratorTests(unittest.TestCase):
    def test_generates_digest_for_clean_run(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle = load_context(temp_dir)
            fg = FinalGateRunner().evaluate(bundle, current_item_status="review")
            ag = ArtifactGate().evaluate(bundle)
            se = ScoringEngine()
            score = se.score_task_bundle(bundle, fg)
            qg = QualityGateManager().evaluate(bundle, fg, ag, task_score=score)
            run_score = se.score_run((score,))
            trajectory = TrajectoryReport()

            # Create a minimal projection bundle
            from harness_core.projection_store import (
                ProjectItemState,
                ProjectStateProjection,
                ProjectionBundle,
                TaskQueueProjection,
                DependencyProjection,
            )
            proj = ProjectStateProjection(items={
                "item_001": ProjectItemState(
                    item_id="item_001", status="done",
                    previous_status="review", reason="test",
                    last_event_id="evt_001", last_updated="2026-05-16",
                ),
            })
            projections = ProjectionBundle(
                project=proj,
                task_queue=TaskQueueProjection(),
                dependencies=DependencyProjection(),
            )

            digest = QualityDigestGenerator().generate(
                projections=projections,
                run_score=run_score,
                gate_decisions={"item_001": qg},
                trajectory=trajectory,
            )
        self.assertEqual(1, len(digest.items))
        self.assertIn(digest.items[0].quality_gate_result, ("pass", "pass_with_notes"))
        self.assertGreater(digest.aggregate_score, 0.0)

    def test_includes_trajectory_anomalies(self):
        from harness_core.trajectory import TrajectoryAnomaly

        trajectory = TrajectoryReport(
            ok=False,
            anomalies=(
                TrajectoryAnomaly(
                    anomaly_type="repeated_failure",
                    item_id="item_001",
                    message="failed 3 times",
                    severity="error",
                ),
            ),
        )
        from harness_core.projection_store import (
            ProjectItemState,
            ProjectStateProjection,
            ProjectionBundle,
            TaskQueueProjection,
            DependencyProjection,
        )
        proj = ProjectStateProjection(items={
            "item_001": ProjectItemState(
                item_id="item_001", status="failed",
                previous_status="review", reason="test",
                last_event_id="evt_001", last_updated="2026-05-16",
            ),
        })
        projections = ProjectionBundle(
            project=proj,
            task_queue=TaskQueueProjection(),
            dependencies=DependencyProjection(),
        )
        run_score = RunScore("run_1", (), 0.50, "D", 1, 0, 1)

        digest = QualityDigestGenerator().generate(
            projections=projections,
            run_score=run_score,
            gate_decisions={},
            trajectory=trajectory,
        )
        self.assertFalse(digest.trajectory_ok)
        self.assertTrue(len(digest.items[0].anomalies) > 0)

    def test_includes_baseline_delta(self):
        baseline = BaselineComparison(
            baseline_id="baseline_1",
            current_run_score=RunScore("run_1", (), 0.85, "B", 1, 1, 0),
            score_delta=0.05,
            regression_detected=False,
            improved_cases=(),
            regressed_cases=(),
        )
        from harness_core.projection_store import (
            ProjectItemState,
            ProjectStateProjection,
            ProjectionBundle,
            TaskQueueProjection,
            DependencyProjection,
        )
        proj = ProjectStateProjection(items={
            "item_001": ProjectItemState(
                item_id="item_001", status="done",
                previous_status="review", reason="test",
                last_event_id="evt_001", last_updated="2026-05-16",
            ),
        })
        projections = ProjectionBundle(
            project=proj,
            task_queue=TaskQueueProjection(),
            dependencies=DependencyProjection(),
        )
        run_score = RunScore("run_1", (), 0.85, "B", 1, 1, 0)

        digest = QualityDigestGenerator().generate(
            projections=projections,
            run_score=run_score,
            gate_decisions={},
            trajectory=TrajectoryReport(),
            baseline=baseline,
        )
        self.assertIsNotNone(digest.baseline_delta)
        self.assertAlmostEqual(0.05, digest.baseline_delta, places=2)

    def test_recommends_action_for_fail_retryable(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle = load_context(temp_dir)
            fg = FinalGateRunner().evaluate(bundle, current_item_status="review")
            ag = ArtifactGate().evaluate(bundle)
            se = ScoringEngine()
            score = se.score_task_bundle(bundle, fg)
            score = replace(score, weighted_score=0.50, grade="D")
            fail_decision = replace(fg, result="fail", next_project_status="review")
            qg = QualityGateManager().evaluate(bundle, fail_decision, ag, task_score=score)

            from harness_core.projection_store import (
                ProjectItemState,
                ProjectStateProjection,
                ProjectionBundle,
                TaskQueueProjection,
                DependencyProjection,
            )
            proj = ProjectStateProjection(items={
                "item_001": ProjectItemState(
                    item_id="item_001", status="review",
                    previous_status="running", reason="test",
                    last_event_id="evt_001", last_updated="2026-05-16",
                ),
            })
            projections = ProjectionBundle(
                project=proj,
                task_queue=TaskQueueProjection(),
                dependencies=DependencyProjection(),
            )
            run_score = se.score_run((score,))

            digest = QualityDigestGenerator().generate(
                projections=projections,
                run_score=run_score,
                gate_decisions={"item_001": qg},
                trajectory=TrajectoryReport(),
            )
        self.assertTrue(any("Retry" in a for a in digest.recommended_actions))

    def test_recommends_human_review(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle = load_context(temp_dir)
            handoff_pack = dict(bundle.handoff_pack)
            handoff_pack["approval_request"] = {
                "approval_id": "ap_1", "task_id": "t1", "risk_level": "low",
                "requested_action": "modify_files", "summary": "test",
                "reason": "test", "affected_files": [], "options": ["approve"],
                "timeout_policy": "no_timeout", "decision": "pending",
            }
            bundle = replace(bundle, handoff_pack=handoff_pack)
            fg = FinalGateRunner().evaluate(bundle, current_item_status="review")
            ag = ArtifactGate().evaluate(bundle)
            se = ScoringEngine()
            score = se.score_task_bundle(bundle, fg)
            qg = QualityGateManager().evaluate(bundle, fg, ag, task_score=score)

            from harness_core.projection_store import (
                ProjectItemState,
                ProjectStateProjection,
                ProjectionBundle,
                TaskQueueProjection,
                DependencyProjection,
            )
            proj = ProjectStateProjection(items={
                "item_001": ProjectItemState(
                    item_id="item_001", status="blocked",
                    previous_status="review", reason="test",
                    last_event_id="evt_001", last_updated="2026-05-16",
                ),
            })
            projections = ProjectionBundle(
                project=proj,
                task_queue=TaskQueueProjection(),
                dependencies=DependencyProjection(),
            )
            run_score = se.score_run((score,))

            digest = QualityDigestGenerator().generate(
                projections=projections,
                run_score=run_score,
                gate_decisions={"item_001": qg},
                trajectory=TrajectoryReport(),
            )
        self.assertTrue(any("Human review" in a for a in digest.recommended_actions))


if __name__ == "__main__":
    unittest.main()
