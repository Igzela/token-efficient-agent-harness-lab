import shutil
import sys
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core import (
    ArtifactGate,
    FinalGateRunner,
    QualityGateManager,
    ScoringEngine,
    TaskRecordStore,
    TrajectoryReport,
)


ROOT = Path(__file__).resolve().parents[1]
TASK_005 = ROOT / "docs" / "stage0" / "tasks" / "task-005-failure-fix-loop"


def load_bundle_and_gate(temp_dir: str):
    task_dir = Path(temp_dir) / TASK_005.name
    shutil.copytree(TASK_005, task_dir)
    store = TaskRecordStore(Path(temp_dir))
    bundle = store.load_task_bundle(task_dir)
    # Override artifact_refs to use task-dir-relative paths
    completion = dict(bundle.completion)
    completion["artifact_refs"] = [
        {"artifact_id": "run_log", "path": "run_log.md"}
    ]
    bundle = replace(bundle, completion=completion)
    final_gate = FinalGateRunner()
    decision = final_gate.evaluate(bundle, current_item_status="review")
    artifact_result = ArtifactGate().evaluate(bundle)
    score = ScoringEngine().score_task_bundle(bundle, decision)
    return bundle, decision, artifact_result, score


class QualityGatePassTests(unittest.TestCase):
    def test_pass_with_notes_for_valid_bundle(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle, decision, artifact_result, score = load_bundle_and_gate(temp_dir)
            qg = QualityGateManager().evaluate(bundle, decision, artifact_result, task_score=score)
        # task-005 has failure_code penalty, so score ~0.60 -> pass_with_notes
        self.assertIn(qg.result, ("pass", "pass_with_notes"))
        self.assertEqual("done", qg.next_project_status)

    def test_pass_with_notes_low_score(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle, decision, artifact_result, score = load_bundle_and_gate(temp_dir)
            score = replace(score, weighted_score=0.65, grade="C")
            qg = QualityGateManager().evaluate(bundle, decision, artifact_result, task_score=score)
        self.assertEqual("pass_with_notes", qg.result)
        self.assertEqual("done", qg.next_project_status)

    def test_fail_retryable(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle, _, artifact_result, score = load_bundle_and_gate(temp_dir)
            score = replace(score, weighted_score=0.50, grade="D")
            fail_decision = replace(
                FinalGateRunner().evaluate(bundle, current_item_status="review"),
                result="fail",
                next_project_status="review",
            )
            qg = QualityGateManager().evaluate(bundle, fail_decision, artifact_result, task_score=score)
        self.assertEqual("fail_retryable", qg.result)
        self.assertEqual("ready", qg.next_project_status)

    def test_fail_terminal_low_score(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle, _, artifact_result, score = load_bundle_and_gate(temp_dir)
            score = replace(score, weighted_score=0.30, grade="F")
            fail_decision = replace(
                FinalGateRunner().evaluate(bundle, current_item_status="review"),
                result="fail",
                next_project_status="review",
            )
            qg = QualityGateManager().evaluate(bundle, fail_decision, artifact_result, task_score=score)
        self.assertEqual("fail_terminal", qg.result)
        self.assertEqual("failed", qg.next_project_status)

    def test_fail_terminal_retry_count_exceeded(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle, _, artifact_result, score = load_bundle_and_gate(temp_dir)
            bundle = replace(bundle, completion={**bundle.completion, "retry_count": 3})
            score = replace(score, weighted_score=0.50, grade="D")
            fail_decision = replace(
                FinalGateRunner().evaluate(bundle, current_item_status="review"),
                result="fail",
                next_project_status="review",
            )
            qg = QualityGateManager().evaluate(bundle, fail_decision, artifact_result, task_score=score)
        self.assertEqual("fail_terminal", qg.result)
        self.assertTrue(any("retry_count" in r for r in qg.reasons))

    def test_requires_human_review_pending_approval(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle, decision, artifact_result, score = load_bundle_and_gate(temp_dir)
            handoff_pack = dict(bundle.handoff_pack)
            handoff_pack["approval_request"] = {
                "approval_id": "approval_test",
                "task_id": "stage0_task_005",
                "risk_level": "low",
                "requested_action": "modify_files",
                "summary": "test approval",
                "reason": "test",
                "affected_files": [],
                "options": ["approve", "reject"],
                "timeout_policy": "no_timeout",
                "decision": "pending",
            }
            bundle = replace(bundle, handoff_pack=handoff_pack)
            qg = QualityGateManager().evaluate(bundle, decision, artifact_result, task_score=score)
        self.assertEqual("requires_human_review", qg.result)
        self.assertEqual("blocked", qg.next_project_status)

    def test_status_mapping(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle, decision, artifact_result, score = load_bundle_and_gate(temp_dir)
            qg = QualityGateManager().evaluate(bundle, decision, artifact_result, task_score=score)
        mapping = {
            "pass": "done",
            "pass_with_notes": "done",
            "fail_retryable": "ready",
            "fail_terminal": "failed",
            "requires_human_review": "blocked",
        }
        self.assertEqual(mapping[qg.result], qg.next_project_status)


if __name__ == "__main__":
    unittest.main()
