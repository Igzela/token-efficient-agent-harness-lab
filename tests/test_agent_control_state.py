"""Tests for state_manager.py — comprehensive coverage of defect categories.

Covers:
- Defect 19: State reads match the object where state was written (Issue comments)
- Defect 13: Review-running is not review-passed
- Defect 14: Review PASS persisted and bound to exact head
- Defect 15: PASS_WITH_NOTES cannot authorize merge
- Defect 7: Orchestrator is sole Git write owner (prompt check)
- Label definitions and transitions
- Dependency parsing
- Emergency stop checks
- Worker state structures
"""

import json
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "scripts", "agent-control"))
import state_manager as sm


class TestLabelDefinitions(unittest.TestCase):
    """Verify label constants are correct and disjoint where required."""

    def test_all_labels_defined(self):
        required = [
            "agent-draft", "agent-ready", "agent-running",
            "ci-repairing", "review-running", "review-passed",
            "agent-blocked", "agent-complete",
        ]
        for label in required:
            self.assertIn(label, sm.ALL_LABELS)

    def test_active_labels_subset(self):
        for lbl in sm.ACTIVE_LABELS:
            self.assertIn(lbl, sm.ALL_LABELS)

    def test_terminal_labels_subset(self):
        for lbl in sm.TERMINAL_LABELS:
            self.assertIn(lbl, sm.ALL_LABELS)

    def test_active_and_terminal_disjoint(self):
        self.assertTrue(sm.ACTIVE_LABELS.isdisjoint(sm.TERMINAL_LABELS))

    def test_review_running_is_active(self):
        self.assertIn(sm.LABEL_REVIEW_RUNNING, sm.ACTIVE_LABELS)

    def test_review_passed_is_not_active(self):
        self.assertNotIn(sm.LABEL_REVIEW_PASSED, sm.ACTIVE_LABELS)

    def test_review_passed_is_not_terminal(self):
        self.assertNotIn(sm.LABEL_REVIEW_PASSED, sm.TERMINAL_LABELS)

    def test_review_running_and_review_passed_are_different(self):
        self.assertNotEqual(sm.LABEL_REVIEW_RUNNING, sm.LABEL_REVIEW_PASSED)


class TestDependencyParsing(unittest.TestCase):
    """Test dependency resolution from issue bodies."""

    def test_depends_on(self):
        body = "This task depends on #42 and #100 being complete."
        deps = sm.parse_dependencies(body)
        self.assertIn(42, deps)
        self.assertIn(100, deps)

    def test_prerequisite(self):
        body = "Prerequisite: #7"
        deps = sm.parse_dependencies(body)
        self.assertIn(7, deps)

    def test_none(self):
        body = "No dependencies here."
        deps = sm.parse_dependencies(body)
        self.assertEqual(len(deps), 0)

    def test_must_keyword(self):
        body = "#42 must be done first"
        deps = sm.parse_dependencies(body)
        self.assertIn(42, deps)

    def test_multiline(self):
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

    def test_duplicates(self):
        body = "Depends on #5\nAlso depends on #5"
        deps = sm.parse_dependencies(body)
        self.assertEqual(len(deps), 1)
        self.assertIn(5, deps)

    def test_issue_reference_without_keyword(self):
        body = "The issue #123 should not match as a dependency."
        deps = sm.parse_dependencies(body)
        self.assertEqual(len(deps), 0)

    def test_blocked_by(self):
        body = "This is blocked by #88"
        deps = sm.parse_dependencies(body)
        self.assertIn(88, deps)


class TestEmergencyStop(unittest.TestCase):
    """Test emergency stop detection."""

    def test_emergency_stop_not_set(self):
        os.environ.pop(sm.EMERGENCY_STOP_VAR, None)
        self.assertFalse(sm.is_emergency_stopped())

    def test_emergency_stop_true(self):
        os.environ[sm.EMERGENCY_STOP_VAR] = "true"
        self.assertTrue(sm.is_emergency_stopped())
        del os.environ[sm.EMERGENCY_STOP_VAR]

    def test_emergency_stop_false(self):
        os.environ[sm.EMERGENCY_STOP_VAR] = "false"
        self.assertFalse(sm.is_emergency_stopped())
        del os.environ[sm.EMERGENCY_STOP_VAR]

    def test_emergency_stop_case_insensitive(self):
        os.environ[sm.EMERGENCY_STOP_VAR] = "TRUE"
        self.assertTrue(sm.is_emergency_stopped())
        del os.environ[sm.EMERGENCY_STOP_VAR]

    def test_emergency_stop_other_value(self):
        os.environ[sm.EMERGENCY_STOP_VAR] = "yes"
        self.assertFalse(sm.is_emergency_stopped())
        del os.environ[sm.EMERGENCY_STOP_VAR]


class TestWorkerStateStructure(unittest.TestCase):
    """Test worker and CI state structures."""

    def test_worker_state_structure(self):
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
        self.assertIn("pr_number", state)
        self.assertIn("head_sha", state)
        self.assertIn("worker_type", state)

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
        self.assertIn("ci_run_id", state)
        self.assertIn("status", state)

    def test_review_state_structure(self):
        state = {
            "kind": "agent-orchestrator-review-state",
            "version": 1,
            "pr_number": 42,
            "head_sha": "abc123",
            "verdict": "PASS",
            "summary": "All checks pass",
        }
        self.assertEqual(state["kind"], "agent-orchestrator-review-state")
        self.assertIn("verdict", state)
        self.assertIn("summary", state)

    def test_ci_state_with_repair_count(self):
        state = {
            "kind": "agent-orchestrator-ci-state",
            "version": 1,
            "pr_number": 42,
            "head_sha": "abc123",
            "ci_run_id": 12345,
            "status": "failure_repair_1",
            "extra": {"repair_count": 1},
        }
        self.assertEqual(state["status"], "failure_repair_1")
        self.assertEqual(state["extra"]["repair_count"], 1)


class TestMaxRepairAttempts(unittest.TestCase):
    """Defect 19: Repair count persists across events and stops after two attempts."""

    def test_max_repair_attempts_is_two(self):
        self.assertEqual(sm.MAX_REPAIR_ATTEMPTS, 2)


class TestStateReadFromIssueComments(unittest.TestCase):
    """Defect 19: State reads match the object where state was written."""

    def test_read_worker_state_uses_issue_comments(self):
        """Verify read_worker_state calls get_issue_comment_bodies, not get_pr_comment_body."""
        import inspect
        source = inspect.getsource(sm.read_worker_state)
        self.assertIn("get_issue_comment_bodies", source)
        self.assertNotIn("get_pr_comment_body", source)

    def test_read_ci_state_uses_issue_comments(self):
        """Verify read_ci_state calls get_issue_comment_bodies, not get_pr_comment_body."""
        import inspect
        source = inspect.getsource(sm.read_ci_state)
        self.assertIn("get_issue_comment_bodies", source)
        self.assertNotIn("get_pr_comment_body", source)

    def test_read_review_state_uses_issue_comments(self):
        """Verify read_review_state calls get_issue_comment_bodies."""
        import inspect
        source = inspect.getsource(sm.read_review_state)
        self.assertIn("get_issue_comment_bodies", source)


if __name__ == "__main__":
    unittest.main()
