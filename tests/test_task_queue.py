import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core import (
    ProjectBoardItem,
    map_task_status_to_project_board,
    receive_handoff,
    transition_task,
)


class TaskQueueHandoffTests(unittest.TestCase):
    def test_receive_handoff_accepts_ready_item(self):
        item = ProjectBoardItem(item_id="item_003", status="ready")

        result = receive_handoff(item, handoff_id="handoff_003")

        self.assertTrue(result.accepted)
        self.assertEqual("QUEUED", result.task.status)
        self.assertEqual("item_003", result.task.item_id)
        self.assertEqual("sequential", result.task.scheduling_policy)

    def test_receive_handoff_rejects_non_ready_item(self):
        item = ProjectBoardItem(item_id="item_003", status="todo")

        with self.assertRaises(ValueError):
            receive_handoff(item, handoff_id="handoff_003")

    def test_receive_handoff_rejects_non_sequential_scheduling(self):
        item = ProjectBoardItem(item_id="item_003", status="ready")

        with self.assertRaises(ValueError):
            receive_handoff(item, handoff_id="handoff_003", scheduling_policy="parallel")


class TaskQueueTransitionTests(unittest.TestCase):
    def test_legal_task_transition_maps_to_project_board(self):
        task = receive_handoff(
            ProjectBoardItem(item_id="item_003", status="ready"),
            handoff_id="handoff_003",
        ).task

        result = transition_task(task, "RUNNING")

        self.assertEqual("RUNNING", result.task.status)
        self.assertEqual("running", result.project_board_status)
        self.assertIsNone(result.blocked_reason)

    def test_completed_maps_to_review(self):
        task = receive_handoff(
            ProjectBoardItem(item_id="item_003", status="ready"),
            handoff_id="handoff_003",
        ).task
        running = transition_task(task, "RUNNING").task

        result = transition_task(running, "COMPLETED")

        self.assertEqual("review", result.project_board_status)

    def test_illegal_task_transition_is_rejected(self):
        task = receive_handoff(
            ProjectBoardItem(item_id="item_003", status="ready"),
            handoff_id="handoff_003",
        ).task

        with self.assertRaises(ValueError):
            transition_task(task, "COMPLETED")


class TaskQueueMappingTests(unittest.TestCase):
    def test_waiting_approval_maps_to_blocked_approval(self):
        self.assertEqual(("blocked", "approval"), map_task_status_to_project_board("WAITING_APPROVAL"))

    def test_paused_budget_maps_to_blocked_budget(self):
        self.assertEqual(("blocked", "budget"), map_task_status_to_project_board("PAUSED_BUDGET"))

    def test_blocked_upstream_failed_maps_to_blocked_upstream_failed(self):
        self.assertEqual(
            ("blocked", "upstream_failed"),
            map_task_status_to_project_board("BLOCKED_UPSTREAM_FAILED"),
        )

    def test_cancelled_by_dependency_maps_to_failed(self):
        self.assertEqual(("failed", None), map_task_status_to_project_board("CANCELLED_BY_DEPENDENCY"))


if __name__ == "__main__":
    unittest.main()
