import json
import shutil
import sys
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core import FinalGateRunner, TaskRecordStore


ROOT = Path(__file__).resolve().parents[1]
TASK_005 = ROOT / "docs" / "stage0" / "tasks" / "task-005-failure-fix-loop"


def load_temp_bundle(temp_dir: str):
    task_dir = Path(temp_dir) / TASK_005.name
    shutil.copytree(TASK_005, task_dir)
    return TaskRecordStore(Path(temp_dir)).load_task_bundle(task_dir)


def pending_approval_request():
    return {
        "approval_id": "approval_final_gate_test",
        "task_id": "stage0_task_005",
        "risk_level": "low",
        "requested_action": "modify_files",
        "summary": "pending approval should remain pending",
        "reason": "verify Final Gate does not execute approval actions",
        "affected_files": [{"path": "completion.json", "change_type": "read"}],
        "options": ["approve", "reject", "defer"],
        "timeout_policy": "no_timeout",
        "decision": "pending",
    }


class FinalGateRunnerTests(unittest.TestCase):
    def test_valid_completed_bundle_in_review_passes_to_done(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle = load_temp_bundle(temp_dir)

            decision = FinalGateRunner().evaluate(bundle, current_item_status="review")

        self.assertEqual("pass", decision.result)
        self.assertEqual("done", decision.next_project_status)

    def test_valid_completed_bundle_not_in_review_fails(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle = load_temp_bundle(temp_dir)

            decision = FinalGateRunner().evaluate(bundle, current_item_status="running")

        self.assertEqual("fail", decision.result)
        self.assertEqual("review", decision.next_project_status)
        self.assertIn("project item must be review", decision.reasons[0])

    def test_missing_completion_fields_fail(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle = load_temp_bundle(temp_dir)
            bundle = replace(bundle, completion={})

            decision = FinalGateRunner().evaluate(bundle, current_item_status="review")

        self.assertEqual("fail", decision.result)
        self.assertEqual("review", decision.next_project_status)
        self.assertTrue(any("completion.json" in reason for reason in decision.reasons))

    def test_invalid_completion_fails(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle = load_temp_bundle(temp_dir)
            completion = dict(bundle.completion)
            completion["exit_code"] = "0"
            bundle = replace(bundle, completion=completion)

            decision = FinalGateRunner().evaluate(bundle, current_item_status="review")

        self.assertEqual("fail", decision.result)
        self.assertTrue(any("exit_code must be an integer" in reason for reason in decision.reasons))

    def test_missing_handoff_pack_fields_fail(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle = load_temp_bundle(temp_dir)
            bundle = replace(bundle, handoff_pack={})

            decision = FinalGateRunner().evaluate(bundle, current_item_status="review")

        self.assertEqual("fail", decision.result)
        self.assertEqual("review", decision.next_project_status)
        self.assertTrue(any("handoff_pack.json" in reason for reason in decision.reasons))

    def test_invalid_handoff_pack_fails(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle = load_temp_bundle(temp_dir)
            handoff_pack = dict(bundle.handoff_pack)
            handoff_pack["summary"] = ""
            bundle = replace(bundle, handoff_pack=handoff_pack)

            decision = FinalGateRunner().evaluate(bundle, current_item_status="review")

        self.assertEqual("fail", decision.result)
        self.assertTrue(any("summary must be non-empty" in reason for reason in decision.reasons))

    def test_pass_with_notes_for_non_blocking_warning(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            task_dir = Path(temp_dir) / TASK_005.name
            shutil.copytree(TASK_005, task_dir)
            (task_dir / "run_log.md").unlink()
            bundle = TaskRecordStore(Path(temp_dir)).load_task_bundle(task_dir)

            decision = FinalGateRunner().evaluate(bundle, current_item_status="review")

        self.assertEqual("pass_with_notes", decision.result)
        self.assertEqual("review", decision.next_project_status)
        self.assertIn("run_log.md not present", decision.reasons[0])

    def test_pending_approval_request_does_not_execute_approval_action(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle = load_temp_bundle(temp_dir)
            handoff_pack = dict(bundle.handoff_pack)
            handoff_pack["approval_request"] = pending_approval_request()
            bundle = replace(bundle, handoff_pack=handoff_pack)

            decision = FinalGateRunner().evaluate(bundle, current_item_status="review")

        self.assertEqual("fail", decision.result)
        self.assertEqual("review", decision.next_project_status)
        self.assertIn("is pending", decision.reasons[0])
        self.assertIn("did not execute approval", decision.reasons[0])

    def test_does_not_mutate_event_logs_or_project_board_directly(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle = load_temp_bundle(temp_dir)
            before_events = bundle.events_path.read_bytes()
            current_status = "review"

            decision = FinalGateRunner().evaluate(bundle, current_item_status=current_status)
            after_events = bundle.events_path.read_bytes()

        self.assertEqual("pass", decision.result)
        self.assertEqual(before_events, after_events)
        self.assertEqual("review", current_status)

    def test_evidence_refs_are_reported(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle = load_temp_bundle(temp_dir)

            decision = FinalGateRunner().evaluate(bundle, current_item_status="review")

        self.assertTrue(any(ref.endswith("run_log.md") for ref in decision.evidence_refs))
        self.assertTrue(any("completion.json" in ref for ref in decision.evidence_refs))


if __name__ == "__main__":
    unittest.main()
