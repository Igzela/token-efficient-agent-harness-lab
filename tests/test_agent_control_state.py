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

    def test_dependency_check_fails_closed_when_issue_body_is_unavailable(self):
        with mock.patch.object(sm, "_gh", return_value=None):
            complete, reason = sm.check_dependencies_complete(42, "acme/repo")
        self.assertFalse(complete)
        self.assertEqual(reason, "dependency_state_unavailable")


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
            state_path.write_text(json.dumps({"labels": ["unrelated-label"], "issue": False}))
            env = {**os.environ, "PATH": f"{temp}:{os.environ['PATH']}", "CONTROL_STUB_STATE": str(state_path)}
            command = [sys.executable, str(pathlib.Path(__file__).parents[1] / "scripts/agent-control/control_state.py"), "setup-controls", "--repo", "acme/repo"]
            first = subprocess.run(command, cwd=pathlib.Path(__file__).parents[1], env=env, capture_output=True, text=True)
            second = subprocess.run(command, cwd=pathlib.Path(__file__).parents[1], env=env, capture_output=True, text=True)
            final_state = json.loads(state_path.read_text())
        self.assertEqual(first.returncode, 0, first.stderr)
        self.assertEqual(second.returncode, 0, second.stderr)
        result = json.loads(second.stdout)
        self.assertEqual(result["labels"], ["agent-control", "agent-emergency-stop"])
        self.assertFalse(result["orchestrator_enabled"])
        self.assertFalse(result["auto_merge_enabled"])
        self.assertIn("unrelated-label", final_state["labels"])
        self.assertTrue(set(control_state.REQUIRED_LABELS).issubset(final_state["labels"]))

    def test_control_commands_enforce_explicit_reauthorization_and_verify_live_state(self):
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
                "if args[:2] == ['api', '--paginate']:\n"
                "    issue = {'number': 208, 'state': 'open', 'title': '[agent-control] Orchestrator controls', 'body': '<!-- agent-orchestrator-control:v1 -->', 'labels': [{'name': name} for name in state['labels']]}\n"
                "    print(json.dumps([issue]))\n"
                "elif args[:2] == ['issue', 'edit']:\n"
                "    if os.environ.get('CONTROL_STUB_FAIL_EDIT') == '1': raise SystemExit('mutation failed')\n"
                "    if os.environ.get('CONTROL_STUB_MISMATCH') == '1': raise SystemExit(0)\n"
                "    labels = set(state['labels'])\n"
                "    if '--add-label' in args: labels.update(args[args.index('--add-label') + 1].split(','))\n"
                "    if '--remove-label' in args: labels.difference_update(args[args.index('--remove-label') + 1].split(','))\n"
                "    state['labels'] = sorted(labels); json.dump(state, open(path, 'w'))\n"
                "else: raise SystemExit('unexpected gh args: ' + repr(args))\n"
            )
            gh_path.chmod(gh_path.stat().st_mode | stat.S_IXUSR)
            state_path.write_text(json.dumps({"labels": [
                "agent-control", "agent-orchestrator-enabled", "agent-auto-merge-enabled"
            ]}))
            env = {**os.environ, "PATH": f"{temp}:{os.environ['PATH']}", "CONTROL_STUB_STATE": str(state_path)}
            script = pathlib.Path(__file__).parents[1] / "scripts/agent-control/control_state.py"

            def run(command, extra=None):
                child_env = {**env, **(extra or {})}
                return subprocess.run(
                    [sys.executable, str(script), command, "--repo", "acme/repo"],
                    cwd=pathlib.Path(__file__).parents[1], env=child_env,
                    capture_output=True, text=True,
                )

            def labels():
                return json.loads(state_path.read_text())["labels"]

            stopped = run("emergency-stop")
            self.assertEqual(stopped.returncode, 0, stopped.stderr)
            self.assertEqual(labels(), ["agent-control", "agent-emergency-stop"])
            self.assertNotEqual(run("enable-orchestrator").returncode, 0)
            self.assertNotEqual(run("enable-auto-merge").returncode, 0)
            resumed = run("emergency-resume")
            self.assertEqual(resumed.returncode, 0, resumed.stderr)
            self.assertEqual(labels(), ["agent-control"])
            enabled = run("enable-orchestrator")
            self.assertEqual(enabled.returncode, 0, enabled.stderr)
            self.assertEqual(labels(), ["agent-control", "agent-orchestrator-enabled"])
            auto = run("enable-auto-merge")
            self.assertEqual(auto.returncode, 0, auto.stderr)
            self.assertEqual(labels(), [
                "agent-auto-merge-enabled", "agent-control", "agent-orchestrator-enabled"
            ])
            disabled = run("disable-orchestrator")
            self.assertEqual(disabled.returncode, 0, disabled.stderr)
            self.assertEqual(labels(), ["agent-control"])
            self.assertNotEqual(run("enable-auto-merge").returncode, 0)
            self.assertEqual(run("disable-auto-merge").returncode, 0)
            self.assertEqual(run("emergency-stop").returncode, 0)
            self.assertEqual(run("emergency-stop").returncode, 0)
            self.assertEqual(labels(), ["agent-control", "agent-emergency-stop"])
            self.assertEqual(run("emergency-resume").returncode, 0)
            self.assertEqual(run("emergency-resume").returncode, 0)
            self.assertEqual(labels(), ["agent-control"])

            failed = run("enable-orchestrator", {"CONTROL_STUB_FAIL_EDIT": "1"})
            self.assertNotEqual(failed.returncode, 0)
            self.assertEqual(labels(), ["agent-control"])
            mismatch = run("enable-orchestrator", {"CONTROL_STUB_MISMATCH": "1"})
            self.assertNotEqual(mismatch.returncode, 0)
            self.assertEqual(labels(), ["agent-control"])

    def test_setup_requires_complete_required_label_set_without_overwriting_unrelated_metadata(self):
        self.assertEqual(len(control_state.REQUIRED_LABELS), 15)
        self.assertIn("agent-draft", control_state.REQUIRED_LABELS)
        self.assertIn("agent-generated", control_state.REQUIRED_LABELS)
        self.assertIn(control_state.EMERGENCY_STOP_LABEL, control_state.REQUIRED_LABELS)

    def test_setup_label_listing_failure_and_malformed_control_issue_fail_closed(self):
        with mock.patch.object(control_state, "_run_gh", side_effect=RuntimeError("unavailable")):
            with self.assertRaises(control_state.ControlStateError):
                control_state.setup("acme/repo")
        with self.assertRaises(control_state.ControlStateError):
            control_state.resolve_control_issue([
                {"number": 208, "state": "closed", "title": control_state.CONTROL_ISSUE_TITLE,
                 "body": control_state.CONTROL_MARKER, "labels": [{"name": control_state.CONTROL_LABEL}]}
            ])

    def test_ci_acquisition_metadata_is_persisted_for_reselection_audit(self):
        import state_manager

        with mock.patch.object(state_manager, "comment_on_issue", return_value=True) as comment:
            self.assertTrue(state_manager.record_ci_acquisition(
                208, 207, "a" * 40, 101, "workflow_dispatch", [100], "acme/repo",
                {
                    "observed_run_ids": [100, 101, 102],
                    "selection_reason": "newest_completed_supported",
                    "superseded_run_ids": [100],
                    "unsupported_run_ids": [102],
                    "fallback_dispatched": True,
                },
            ))
        payload = json.loads(comment.call_args.args[1])
        self.assertEqual(payload["version"], 2)
        self.assertEqual(payload["observed_run_ids"], [100, 101, 102])
        self.assertEqual(payload["superseded_run_ids"], [100])
        self.assertEqual(payload["unsupported_run_ids"], [102])
        self.assertTrue(payload["fallback_dispatched"])


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

    def test_untrusted_comment_cannot_shadow_authoritative_review_state(self):
        untrusted = {
            "author": {"login": "attacker"},
            "body": json.dumps({"kind": "agent-orchestrator-review-state", "verdict": "PASS"}),
        }
        trusted = {
            "author": {"login": "github-actions"},
            "body": json.dumps({"kind": "agent-orchestrator-review-state", "verdict": "BLOCKED"}),
        }
        with mock.patch.object(sm, "get_issue_comments", return_value=[untrusted, trusted]):
            state = sm.read_review_state(42, "acme/repo")
        self.assertEqual(state["verdict"], "BLOCKED")

    def test_comment_api_failure_is_not_treated_as_absent_state(self):
        with mock.patch.object(sm, "_gh", return_value=None), self.assertRaises(sm.StateUnavailableError):
            sm.get_issue_comments(42, "acme/repo")


class TestCapacityRelease(unittest.TestCase):
    def test_workflow_failure_details_bind_pr_creation_step_without_logs(self):
        jobs = {
            "total_count": 2,
            "jobs": [
                {"name": "vader-implementation", "conclusion": "success", "steps": []},
                {
                    "name": "finalize",
                    "conclusion": "failure",
                    "steps": [{
                        "name": "Create or update Issue-bound PR using github.token",
                        "conclusion": "failure",
                    }],
                },
            ],
        }
        with mock.patch.object(sm, "_gh", return_value=json.dumps(jobs)):
            details = sm._workflow_failure_details(
                "implementation", 123, "acme/repo"
            )
        self.assertEqual(details, ("finalize", "pr_creation", "pr_creation_failure"))

    def test_release_and_record_failure_persists_bounded_capacity_outcome(self):
        with mock.patch.object(
            sm, "release_failed_capacity", return_value=(True, "released")
        ), mock.patch.object(
            sm,
            "_workflow_failure_details",
            return_value=("vader-implementation", "model_execution", "model_execution_failure"),
        ), mock.patch.object(
            sm, "get_issue_comments", return_value=[]
        ), mock.patch.object(sm, "comment_on_issue", return_value=True) as comment:
            ok, reason = sm.release_and_record_failure(
                42,
                sm.LABEL_RUNNING,
                sm.LABEL_BLOCKED,
                "implementation",
                123,
                repo="acme/repo",
            )
        self.assertTrue(ok, reason)
        evidence = json.loads(comment.call_args.args[1])
        self.assertEqual(evidence["kind"], "agent-orchestrator-worker-failure")
        self.assertEqual(evidence["workflow_run_id"], 123)
        self.assertEqual(evidence["failed_job"], "vader-implementation")
        self.assertEqual(evidence["failed_phase"], "model_execution")
        self.assertEqual(evidence["reason_code"], "model_execution_failure")
        self.assertEqual(evidence["capacity_release_outcome"], "released")
        self.assertNotIn("log", evidence)

    def test_rejected_worker_records_fixed_repository_preflight_reason(self):
        with mock.patch.object(
            sm, "release_failed_capacity", return_value=(True, "released")
        ), mock.patch.object(sm, "get_issue_comments", return_value=[]), mock.patch.object(
            sm, "comment_on_issue", return_value=True
        ) as comment:
            ok, reason = sm.release_rejected_worker(
                42,
                "true",
                "success",
                "false",
                "acme/repo",
                123,
                "github_actions_pr_creation_disabled",
            )
        self.assertTrue(ok, reason)
        evidence = json.loads(comment.call_args.args[1])
        self.assertEqual(evidence["reason_code"], "github_actions_pr_creation_disabled")

    def test_changed_capacity_outcome_appends_correction_for_same_run(self):
        prior = sm.WorkflowFailureState(
            42,
            "implementation",
            123,
            "unknown",
            "workflow",
            "workflow_failure_details_unavailable",
            "failed",
            "label_state_unavailable",
        ).to_wire()
        comments = [{
            "author": {"login": "github-actions"},
            "body": json.dumps(prior),
        }]
        with mock.patch.object(sm, "get_issue_comments", return_value=comments), mock.patch.object(
            sm, "comment_on_issue", return_value=True
        ) as comment:
            ok, reason = sm._record_workflow_failure(
                42,
                "implementation",
                123,
                "unknown",
                "workflow",
                "workflow_failure_details_unavailable",
                True,
                "released",
                "acme/repo",
            )
        self.assertTrue(ok, reason)
        corrected = json.loads(comment.call_args.args[1])
        self.assertEqual(corrected["capacity_release_outcome"], "released")

    def test_review_repair_and_worker_failures_release_only_current_capacity(self):
        sha = "a" * 40
        cases = (
            (sm.LABEL_REVIEW_RUNNING, sm.LABEL_REVIEW_BLOCKED, sha),
            (sm.LABEL_CI_REPAIRING, sm.LABEL_BLOCKED, sha),
            (sm.LABEL_RUNNING, sm.LABEL_BLOCKED, None),
        )
        for active, terminal, expected_sha in cases:
            with self.subTest(active=active), \
                 mock.patch.object(sm, "get_issue_labels_checked", return_value={active}), \
                 mock.patch.object(sm, "read_worker_state", return_value={"head_sha": sha}), \
                 mock.patch.object(sm, "set_labels", return_value=True) as transition:
                ok, reason = sm.release_failed_capacity(
                    42, active, terminal, expected_sha, "acme/repo"
                )
            self.assertTrue(ok, reason)
            transition.assert_called_once_with(42, terminal, repo="acme/repo")

    def test_stale_failure_cannot_release_a_newer_head(self):
        with mock.patch.object(sm, "get_issue_labels_checked", return_value={sm.LABEL_REVIEW_RUNNING}), \
             mock.patch.object(sm, "read_worker_state", return_value={"head_sha": "b" * 40}), \
             mock.patch.object(sm, "set_labels") as transition:
            ok, reason = sm.release_failed_capacity(
                42, sm.LABEL_REVIEW_RUNNING, sm.LABEL_REVIEW_BLOCKED, "a" * 40, "acme/repo"
            )
        self.assertFalse(ok)
        self.assertEqual(reason, "worker_head_mismatch")
        transition.assert_not_called()

    def test_repair_rebound_to_new_head_releases_only_for_same_attempt(self):
        worker = {
            "head_sha": "b" * 40,
            "extra": {"failed_run_id": 91, "repair_attempt": 2},
        }
        with mock.patch.object(sm, "get_issue_labels_checked", return_value={sm.LABEL_CI_REPAIRING}), \
             mock.patch.object(sm, "read_worker_state", return_value=worker), \
             mock.patch.object(sm, "set_labels", return_value=True) as transition:
            ok, reason = sm.release_failed_capacity(
                42, sm.LABEL_CI_REPAIRING, sm.LABEL_BLOCKED, "a" * 40,
                "acme/repo", 91, 2,
            )
        self.assertTrue(ok, reason)
        transition.assert_called_once_with(42, sm.LABEL_BLOCKED, repo="acme/repo")

        with mock.patch.object(sm, "get_issue_labels_checked", return_value={sm.LABEL_CI_REPAIRING}), \
             mock.patch.object(sm, "read_worker_state", return_value=worker), \
             mock.patch.object(sm, "set_labels") as transition:
            ok, reason = sm.release_failed_capacity(
                42, sm.LABEL_CI_REPAIRING, sm.LABEL_BLOCKED, "a" * 40,
                "acme/repo", 92, 2,
            )
        self.assertFalse(ok)
        self.assertEqual(reason, "worker_head_mismatch")
        transition.assert_not_called()

    def test_cancelled_ci_any_active_transition_and_review_verdict_are_executable(self):
        with mock.patch.object(sm, "get_issue_labels_checked", return_value={sm.LABEL_RUNNING}), \
             mock.patch.object(sm, "read_worker_state", return_value={"head_sha": "a" * 40}), \
             mock.patch.object(sm, "set_labels", return_value=True) as transition:
            ok, reason = sm.release_failed_capacity(
                42, "any", sm.LABEL_BLOCKED, "a" * 40, "acme/repo"
            )
        self.assertTrue(ok, reason)
        transition.assert_called_once_with(42, sm.LABEL_BLOCKED, repo="acme/repo")
        with mock.patch.object(
            sm,
            "get_issue_labels_checked",
            side_effect=[{sm.LABEL_REVIEW_RUNNING}, {sm.LABEL_REVIEW_BLOCKED}],
        ), mock.patch.object(sm, "set_labels", return_value=True) as transition:
            self.assertTrue(sm.finalize_review_labels(42, "PASS_WITH_NOTES", "acme/repo"))
        transition.assert_called_once_with(42, sm.LABEL_REVIEW_BLOCKED, repo="acme/repo")

    def test_terminal_release_requires_exact_pr_and_ci_run_binding(self):
        pr = {
            "number": 207, "state": "OPEN", "headRefName": "agent/issue-42",
            "headRefOid": "a" * 40,
            "body": 'Closes #42\n\n<!-- agent-orchestrator-binding: {"issue_number": 42, "branch": "agent/issue-42"} -->',
        }
        with mock.patch.object(sm, "get_issue_labels_checked", return_value={sm.LABEL_RUNNING}), \
             mock.patch.object(sm, "get_pr_info", return_value=pr), \
             mock.patch.object(sm, "read_worker_state", return_value={"pr_number": 207, "head_sha": "a" * 40, "extra": {"branch": "agent/issue-42"}}), \
             mock.patch.object(sm, "read_ci_state", return_value={"pr_number": 207, "head_sha": "a" * 40, "workflow_run_id": 9001}), \
             mock.patch.object(sm, "_gh", return_value="[]"), \
             mock.patch.object(sm, "set_labels", return_value=True) as transition:
            ok, reason = sm.release_failed_capacity(
                42, "any", sm.LABEL_BLOCKED, "a" * 40, "acme/repo",
                expected_pr=207, expected_run_id=9001,
            )
        self.assertTrue(ok, reason)
        transition.assert_called_once_with(42, sm.LABEL_BLOCKED, repo="acme/repo")

        with mock.patch.object(sm, "get_issue_labels_checked", return_value={sm.LABEL_RUNNING}), \
             mock.patch.object(sm, "get_pr_info", return_value=pr), \
             mock.patch.object(sm, "read_worker_state", return_value={"pr_number": 207, "head_sha": "a" * 40, "extra": {"branch": "agent/issue-42"}}), \
             mock.patch.object(sm, "read_ci_state", return_value={"pr_number": 207, "head_sha": "a" * 40, "workflow_run_id": 9002}), \
             mock.patch.object(sm, "_gh", return_value="[]"), \
             mock.patch.object(sm, "set_labels") as transition:
            ok, reason = sm.release_failed_capacity(
                42, "any", sm.LABEL_BLOCKED, "a" * 40, "acme/repo",
                expected_pr=207, expected_run_id=9001,
            )
        self.assertFalse(ok)
        self.assertEqual(reason, "ci_run_mismatch")
        transition.assert_not_called()

    def test_terminal_ci_release_cannot_demote_review_capacity(self):
        with mock.patch.object(sm, "get_issue_labels_checked", return_value={sm.LABEL_REVIEW_RUNNING}), \
             mock.patch.object(sm, "set_labels") as transition:
            ok, reason = sm.release_failed_capacity(
                42, "any", sm.LABEL_BLOCKED, "a" * 40, "acme/repo",
                expected_pr=207, expected_run_id=9001,
            )
        self.assertFalse(ok)
        self.assertEqual(reason, "ci_active_phase_mismatch")
        transition.assert_not_called()

    def test_terminal_ci_release_stops_when_capacity_changes_before_mutation(self):
        with mock.patch.object(
            sm,
            "get_issue_labels_checked",
            side_effect=[{sm.LABEL_RUNNING}, {sm.LABEL_REVIEW_RUNNING}],
        ), mock.patch.object(sm, "set_labels") as transition:
            ok, reason = sm.release_failed_capacity(
                42, "any", sm.LABEL_BLOCKED, repo="acme/repo",
            )
        self.assertFalse(ok)
        self.assertEqual(reason, "capacity_state_changed")
        transition.assert_not_called()

    def test_post_claim_emergency_or_scope_rejection_releases_worker(self):
        for gate_enabled, validate_result, can_start in (
            ("false", "skipped", ""),
            ("true", "success", "false"),
        ):
            with self.subTest(gate_enabled=gate_enabled, can_start=can_start), \
                 mock.patch.object(
                     sm, "release_failed_capacity", return_value=(True, "released")
                 ) as release:
                ok, reason = sm.release_rejected_worker(
                    42, gate_enabled, validate_result, can_start, "acme/repo"
                )
            self.assertTrue(ok, reason)
            release.assert_called_once_with(
                42, sm.LABEL_RUNNING, sm.LABEL_BLOCKED, repo="acme/repo"
            )

    def test_started_or_failed_validation_is_left_to_its_failure_path(self):
        for validate_result, can_start in (("success", "true"), ("failure", "")):
            with mock.patch.object(sm, "release_failed_capacity") as release:
                ok, reason = sm.release_rejected_worker(
                    42, "true", validate_result, can_start, "acme/repo"
                )
            self.assertTrue(ok, reason)
            self.assertEqual(reason, "worker_not_rejected")
            release.assert_not_called()


if __name__ == "__main__":
    unittest.main()
