import json
import shutil
import sys
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core import FinalGateRunner, ScoringEngine, TaskRecordStore
from harness_core.scoring import _grade


ROOT = Path(__file__).resolve().parents[1]
TASK_005 = ROOT / "docs" / "stage0" / "tasks" / "task-005-failure-fix-loop"


def load_bundle_and_decision(temp_dir: str):
    task_dir = Path(temp_dir) / TASK_005.name
    shutil.copytree(TASK_005, task_dir)
    store = TaskRecordStore(Path(temp_dir))
    bundle = store.load_task_bundle(task_dir)
    decision = FinalGateRunner().evaluate(bundle, current_item_status="review")
    return bundle, decision


class ScoringEngineTests(unittest.TestCase):
    def test_valid_bundle_produces_score(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle, decision = load_bundle_and_decision(temp_dir)
            score = ScoringEngine().score_task_bundle(bundle, decision)
        self.assertGreater(score.weighted_score, 0.0)
        self.assertIn(score.grade, ("A", "B", "C", "D", "F"))
        self.assertEqual("stage0_task_005", score.task_id)

    def test_missing_run_log_creates_penalty(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            task_dir = Path(temp_dir) / TASK_005.name
            shutil.copytree(TASK_005, task_dir)
            (task_dir / "run_log.md").unlink()
            store = TaskRecordStore(Path(temp_dir))
            bundle = store.load_task_bundle(task_dir)
            decision = FinalGateRunner().evaluate(bundle, current_item_status="review")

            score = ScoringEngine().score_task_bundle(bundle, decision)
        self.assertEqual(0.0, score.run_log_score)
        self.assertTrue(any("run_log" in p for p in score.penalties))

    def test_missing_artifact_creates_penalty(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle, decision = load_bundle_and_decision(temp_dir)
            bundle = replace(bundle, completion={**bundle.completion, "artifact_refs": []})
            score = ScoringEngine().score_task_bundle(bundle, decision)
        self.assertEqual(0.0, score.artifact_score)
        self.assertTrue(any("artifact_refs" in p for p in score.penalties))

    def test_failure_code_creates_penalty(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle, decision = load_bundle_and_decision(temp_dir)
            bundle = replace(
                bundle,
                task_spec={**bundle.task_spec, "failure_code": "F008_FORMAT_ERROR"},
            )
            score = ScoringEngine().score_task_bundle(bundle, decision)
        self.assertEqual(-0.20, score.failure_code_penalty)
        self.assertTrue(any("failure_code" in p for p in score.penalties))

    def test_score_clamped_to_valid_range(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle, decision = load_bundle_and_decision(temp_dir)
            bundle = replace(
                bundle,
                task_spec={**bundle.task_spec, "failure_code": "F008_FORMAT_ERROR"},
                completion={},
                handoff_pack={},
            )
            score = ScoringEngine().score_task_bundle(bundle, decision)
        self.assertGreaterEqual(score.weighted_score, 0.0)
        self.assertLessEqual(score.weighted_score, 1.0)

    def test_grade_mapping(self):
        self.assertEqual("A", _grade(0.95))
        self.assertEqual("A", _grade(0.90))
        self.assertEqual("B", _grade(0.89))
        self.assertEqual("B", _grade(0.75))
        self.assertEqual("C", _grade(0.74))
        self.assertEqual("C", _grade(0.60))
        self.assertEqual("D", _grade(0.59))
        self.assertEqual("D", _grade(0.40))
        self.assertEqual("F", _grade(0.39))
        self.assertEqual("F", _grade(0.0))

    def test_score_run_aggregates(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle, decision = load_bundle_and_decision(temp_dir)
            engine = ScoringEngine()
            ts1 = engine.score_task_bundle(bundle, decision)
            ts2 = replace(ts1, task_id="task_2", weighted_score=0.80, grade="B")

            run = engine.score_run((ts1, ts2))
        self.assertEqual(2, run.item_count)
        self.assertAlmostEqual(
            (ts1.weighted_score + 0.80) / 2.0,
            run.aggregate_score,
            places=2,
        )
        self.assertIn(run.grade, ("A", "B", "C", "D", "F"))

    def test_score_run_empty(self):
        run = ScoringEngine().score_run(())
        self.assertEqual(0, run.item_count)
        self.assertEqual(0.0, run.aggregate_score)
        self.assertEqual("F", run.grade)

    def test_score_artifact_valid(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle, _ = load_bundle_and_decision(temp_dir)
            artifact_ref = {"artifact_id": "run_log", "path": "run_log.md"}
            score = ScoringEngine().score_artifact(artifact_ref, bundle)
        self.assertTrue(score.existence_ok)
        self.assertTrue(score.schema_ok)
        self.assertGreater(score.score, 0.0)


if __name__ == "__main__":
    unittest.main()
