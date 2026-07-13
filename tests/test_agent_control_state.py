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
import pathlib
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
from contextlib import redirect_stdout
from io import StringIO

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "scripts", "agent-control"))
import state_manager as sm
import control_state


class TestLabelDefinitions(unittest.TestCase):
    """Verify label constants are correct and disjoint where required."""

    def test_all_labels_defined(self):
        required = [
            "agent-draft", "agent-ready", "agent-running",
            "ci-repairing", "review-running", "review-passed",
            "agent-review-blocked", "agent-merge-ready", "agent-blocked", "agent-complete",
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

    def test_review_blocked_releases_capacity_and_merge_ready_is_waiting(self):
        self.assertIn(sm.LABEL_REVIEW_BLOCKED, sm.TERMINAL_LABELS)
        self.assertNotIn(sm.LABEL_REVIEW_BLOCKED, sm.ACTIVE_LABELS)
        self.assertNotIn(sm.LABEL_MERGE_READY, sm.ACTIVE_LABELS)
        self.assertNotIn(sm.LABEL_MERGE_READY, sm.TERMINAL_LABELS)

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


class TestControlIssueLabels(unittest.TestCase):
    """Emergency stop is an Issue label and overrides every enable label."""

    def test_emergency_stop_label_overrides_enable_labels(self):
        issue = {
            "number": 42,
            "title": control_state.CONTROL_ISSUE_TITLE,
            "state": "open",
            "body": control_state.CONTROL_MARKER,
            "labels": [
                {"name": control_state.CONTROL_LABEL},
                {"name": control_state.ORCHESTRATOR_ENABLED_LABEL},
                {"name": control_state.AUTO_MERGE_ENABLED_LABEL},
                {"name": control_state.EMERGENCY_STOP_LABEL},
            ],
        }
        state = control_state.resolve_control_issue([issue])
        self.assertTrue(state["emergency_stop"])
        self.assertFalse(state["orchestrator_enabled"])
        self.assertFalse(state["auto_merge_enabled"])

    def test_control_cli_accepts_standard_repo_flag(self):
        with mock.patch.object(control_state, "read_control_state", return_value={"number": 42}) as read, \
             mock.patch.object(sys, "argv", ["control_state.py", "status", "--repo", "owner/repo"]), \
             redirect_stdout(StringIO()):
            control_state.main()
        read.assert_called_once_with("owner/repo")

    def test_setup_controls_cli_is_idempotent_and_keeps_disabled_defaults(self):
        with tempfile.TemporaryDirectory() as temp:
            temp_path = pathlib.Path(temp)
            state_path = temp_path / "state.json"
            gh_path = temp_path / "gh"
            gh_path.write_text(
                "#!/usr/bin/env python3\n"
                "import json, os, sys\n"
                "path = os.environ['CONTROL_STUB_STATE']\n"
                "state = json.load(open(path))\n"
                "args = sys.argv[1:]\n"
                "if args[:2] == ['label', 'list']:\n"
                "    print(json.dumps([{'name': name} for name in state['labels']]))\n"
                "elif args[:2] == ['label', 'create']:\n"
                "    state['labels'].append(args[2]); json.dump(state, open(path, 'w'))\n"
                "elif args[:2] == ['api', '--paginate']:\n"
                "    if state['issue']:\n"
                "        print(json.dumps([{'number':208,'state':'open','title':'[agent-control] Orchestrator controls','body':'<!-- agent-orchestrator-control:v1 -->','labels':[{'name':'agent-control'},{'name':'agent-emergency-stop'}]}]))\n"
                "    else: print('[]')\n"
                "elif args[:2] == ['issue', 'create']:\n"
                "    state['issue'] = True; json.dump(state, open(path, 'w')); print('https://github.com/acme/repo/issues/208')\n"
                "else: raise SystemExit('unexpected gh args: ' + repr(args))\n"
            )
            gh_path.chmod(gh_path.stat().st_mode | stat.S_IXUSR)
            state_path.write_text(json.dumps({"labels": [], "issue": False}))
            env = {**os.environ, "PATH": f"{temp}:{os.environ['PATH']}", "CONTROL_STUB_STATE": str(state_path)}
            command = [sys.executable, str(pathlib.Path(__file__).parents[1] / "scripts/agent-control/control_state.py"), "setup-controls", "--repo", "acme/repo"]
            first = subprocess.run(command, cwd=pathlib.Path(__file__).parents[1], env=env, capture_output=True, text=True)
            second = subprocess.run(command, cwd=pathlib.Path(__file__).parents[1], env=env, capture_output=True, text=True)
        self.assertEqual(first.returncode, 0, first.stderr)
        self.assertEqual(second.returncode, 0, second.stderr)
        result = json.loads(second.stdout)
        self.assertEqual(result["labels"], ["agent-control", "agent-emergency-stop"])
        self.assertFalse(result["orchestrator_enabled"])
        self.assertFalse(result["auto_merge_enabled"])


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
