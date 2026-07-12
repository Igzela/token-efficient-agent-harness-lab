"""Tests for state_manager.py"""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "scripts", "agent-control"))
import state_manager as sm


class TestStateManager(unittest.TestCase):

    def test_all_labels_defined(self):
        self.assertIn(sm.LABEL_DRAFT, sm.ALL_LABELS)
        self.assertIn(sm.LABEL_READY, sm.ALL_LABELS)
        self.assertIn(sm.LABEL_RUNNING, sm.ALL_LABELS)
        self.assertIn(sm.LABEL_CI_REPAIRING, sm.ALL_LABELS)
        self.assertIn(sm.LABEL_FINAL_REVIEW, sm.ALL_LABELS)
        self.assertIn(sm.LABEL_BLOCKED, sm.ALL_LABELS)
        self.assertIn(sm.LABEL_COMPLETE, sm.ALL_LABELS)

    def test_active_labels_subset(self):
        for lbl in sm.ACTIVE_LABELS:
            self.assertIn(lbl, sm.ALL_LABELS)

    def test_terminal_labels_subset(self):
        for lbl in sm.TERMINAL_LABELS:
            self.assertIn(lbl, sm.ALL_LABELS)

    def test_active_and_terminal_disjoint(self):
        self.assertTrue(sm.ACTIVE_LABELS.isdisjoint(sm.TERMINAL_LABELS))

    def test_parse_dependencies_depends_on(self):
        body = "This task depends on #42 and #100 being complete."
        deps = sm.parse_dependencies(body)
        self.assertIn(42, deps)
        self.assertIn(100, deps)

    def test_parse_dependencies_prerequisite(self):
        body = "Prerequisite: #7"
        deps = sm.parse_dependencies(body)
        self.assertIn(7, deps)

    def test_parse_dependencies_none(self):
        body = "No dependencies here."
        deps = sm.parse_dependencies(body)
        self.assertEqual(len(deps), 0)

    def test_parse_dependencies_must_keyword(self):
        body = "#42 must be done first"
        deps = sm.parse_dependencies(body)
        self.assertIn(42, deps)

    def test_parse_dependencies_multiline(self):
        body = """Goal: implement feature

        depends on:
        #1
        depends on #2
        #3 must be done
        """
        deps = sm.parse_dependencies(body)
        self.assertIn(1, deps)
        self.assertIn(2, deps)
        self.assertIn(3, deps)

    def test_parse_dependencies_duplicates(self):
        body = "Depends on #5\nAlso depends on #5"
        deps = sm.parse_dependencies(body)
        self.assertEqual(len(deps), 1)
        self.assertIn(5, deps)

    def test_max_repair_attempts(self):
        self.assertEqual(sm.MAX_REPAIR_ATTEMPTS, 2)


class TestWorkerState(unittest.TestCase):

    def test_record_state_structure(self):
        state = {
            "kind": "agent-orchestrator-state",
            "version": 1,
            "pr_number": 42,
            "head_sha": "abc123",
            "worker_type": "implementation",
            "extra": {"repair_count": 0},
        }
        self.assertEqual(state["kind"], "agent-orchestrator-state")
        self.assertEqual(state["version"], 1)
        self.assertEqual(state["pr_number"], 42)
        self.assertEqual(state["head_sha"], "abc123")
        self.assertEqual(state["worker_type"], "implementation")

    def test_ci_state_structure(self):
        state = {
            "kind": "agent-orchestrator-ci-state",
            "version": 1,
            "pr_number": 42,
            "head_sha": "abc123",
            "ci_run_id": 12345,
            "status": "success",
        }
        self.assertEqual(state["kind"], "agent-orchestrator-ci-state")
        self.assertEqual(state["ci_run_id"], 12345)
        self.assertEqual(state["status"], "success")


if __name__ == "__main__":
    unittest.main()
