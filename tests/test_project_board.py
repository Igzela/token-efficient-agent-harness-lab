import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core import (
    ProjectBoardItem,
    check_allowed_files,
    complete_task_to_review,
    final_gate,
    transition_item,
)


class ProjectBoardTransitionTests(unittest.TestCase):
    def test_legal_transition_is_applied(self):
        item = ProjectBoardItem(item_id="item_001", status="todo")

        result = transition_item(item, "ready", "dependencies satisfied")

        self.assertEqual("todo", result.previous_status)
        self.assertEqual("ready", result.new_status)
        self.assertEqual("ready", result.item.status)

    def test_illegal_transition_is_rejected(self):
        item = ProjectBoardItem(item_id="item_001", status="todo")

        with self.assertRaises(ValueError):
            transition_item(item, "done", "skip to done")

    def test_task_completion_moves_running_item_to_review_not_done(self):
        item = ProjectBoardItem(item_id="item_001", status="running")

        result = complete_task_to_review(item, "task completed")

        self.assertEqual("review", result.item.status)

    def test_task_completion_requires_running_item(self):
        item = ProjectBoardItem(item_id="item_001", status="ready")

        with self.assertRaises(ValueError):
            complete_task_to_review(item, "task completed")


class FinalGateTests(unittest.TestCase):
    def test_final_gate_pass_moves_review_to_done(self):
        item = ProjectBoardItem(item_id="item_001", status="review")

        result = final_gate(item, "pass", "verified")

        self.assertEqual("done", result.item.status)

    def test_final_gate_pass_with_notes_stays_in_review(self):
        item = ProjectBoardItem(item_id="item_001", status="review")

        result = final_gate(item, "pass_with_notes", "needs follow-up")

        self.assertEqual("review", result.item.status)

    def test_final_gate_fail_moves_review_to_failed(self):
        item = ProjectBoardItem(item_id="item_001", status="review")

        result = final_gate(item, "fail", "verification failed")

        self.assertEqual("failed", result.item.status)

    def test_final_gate_requires_review(self):
        item = ProjectBoardItem(item_id="item_001", status="running")

        with self.assertRaises(ValueError):
            final_gate(item, "pass", "too early")


class AllowedFilesTests(unittest.TestCase):
    def test_allowed_files_complete(self):
        result = check_allowed_files(
            ["events.jsonl", "completion.json", "handoff_pack.json"],
            ["events.jsonl", "completion.json"],
        )

        self.assertTrue(result.ok)
        self.assertEqual((), result.missing_files)

    def test_allowed_files_missing_required_files(self):
        result = check_allowed_files(
            ["events.jsonl"],
            ["events.jsonl", "completion.json", "handoff_pack.json"],
        )

        self.assertFalse(result.ok)
        self.assertEqual(("completion.json", "handoff_pack.json"), result.missing_files)


if __name__ == "__main__":
    unittest.main()
