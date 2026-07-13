"""Regression tests for the event-driven orchestrator's deterministic path."""

from __future__ import annotations

import json
import os
import pathlib
import sys
import tempfile
import unittest
from unittest import mock

ROOT = pathlib.Path(__file__).resolve().parents[1]
CONTROL = ROOT / "scripts" / "agent-control"
WORKFLOWS = ROOT / ".github" / "workflows"
sys.path.insert(0, str(CONTROL))

import ci_handler
import ci_verifier
import control_state
import dispatcher
import state_manager


class TestWorkflowContracts(unittest.TestCase):
    def read(self, name: str) -> str:
        return (WORKFLOWS / name).read_text()

    def test_all_workflow_dispatch_callers_and_inputs_match(self):
        expected = {
            "agent-controller.yml": {"command", "issue", "pr_number", "head_sha", "ci_run_id", "repair_count", "source_issue", "dispatch_id"},
            "agent-worker.yml": {"issue", "dry_run"},
            "agent-ci-repair.yml": {"pr_number", "issue_number", "head_sha", "ci_run_id", "repair_count"},
            "agent-review.yml": {"pr_number", "issue_number", "head_sha"},
            "agent-merge.yml": {"pr_number", "issue_number", "head_sha"},
            "tests.yml": {"expected_sha"},
        }
        for name, fields in expected.items():
            source = self.read(name)
            for field in fields:
                with self.subTest(name=name, field=field):
                    self.assertIn(f"      {field}:\n", source, name)

    def test_repair_dispatch_carries_run_id_not_unbounded_logs(self):
        controller = self.read("agent-controller.yml")
        monitor = self.read("agent-ci-monitor.yml")
        repair = self.read("agent-ci-repair.yml")
        self.assertIn("ci_run_id", controller)
        self.assertIn("ci_run_id", monitor)
        self.assertNotIn("failed_jobs:", repair)
        self.assertNotIn("failed_logs", monitor)
        self.assertIn("ci_evidence.py", repair)
        self.assertIn("repair-evidence", repair)
        self.assertIn("fetch bounded failed jobs, steps, and redacted logs by run id", repair.lower())

    def test_artifact_finalizers_recompute_all_untrusted_bindings(self):
        for name in ("agent-worker.yml", "agent-ci-repair.yml"):
            source = self.read(name)
            with self.subTest(name=name):
                self.assertIn("artifact_contract.py validate", source)
                self.assertIn("artifact_contract.py validate-scope", source)
                self.assertIn("git apply --index --binary", source)
                self.assertIn("artifact_contract.py validate-index", source)
                self.assertIn("git diff --cached --check", source)
                self.assertIn("git diff --check", source)
                self.assertIn("control_state.py require-live", source)
                self.assertIn("agent.patch", source)
                self.assertIn("agent-result.json", source)

    def test_review_worker_is_read_only_and_only_pass_authorizes_merge(self):
        source = self.read("agent-review.yml")
        vader = source.split("  vader-review:", 1)[1].split("\n  finalize:", 1)[0]
        self.assertIn("codex_wrapper.sh review", vader)
        self.assertNotIn("git commit", vader)
        self.assertNotIn("git push", vader)
        self.assertIn("validate_review.py", source)
        self.assertIn("steps.verdict.outputs.verdict == 'PASS'", source)
        self.assertIn("steps.verdict.outputs.verdict != 'PASS'", source)
        self.assertIn("require-auto-merge", source)

    def test_merge_requires_exact_head_ci_pass_review_and_objections_without_admin_api(self):
        source = self.read("agent-merge.yml") + (CONTROL / "state_manager.py").read_text()
        for requirement in ("verify-merge", "require-auto-merge", "pulls/${{ inputs.pr_number }}/merge", "mergeCommit", "review-passed"):
            self.assertIn(requirement, source)
        self.assertNotIn("branches/main/protection", source)

    def test_failure_handlers_preserve_nonterminal_outcomes(self):
        source = "\n".join(self.read(name) for name in ("agent-worker.yml", "agent-ci-repair.yml", "agent-review.yml", "agent-merge.yml", "agent-ci-monitor.yml"))
        self.assertIn("disabled or emergency", source)
        self.assertIn("stale", source)
        self.assertIn("agent-blocked", source)
        self.assertIn("max_repairs_exceeded", (CONTROL / "ci_handler.py").read_text())


class TestDispatcher(unittest.TestCase):
    def test_duplicate_delivery_dispatches_once(self):
        labels = {state_manager.LABEL_READY}
        recorded = {}
        workflow_calls = []

        def set_labels(_issue, *new_labels, repo=""):
            labels.clear()
            labels.update(new_labels)
            return True

        def record(_issue, dispatch_id, action, status, details=None, repo=""):
            recorded[dispatch_id] = {"status": status, "action": action}
            return True

        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="repo"), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", side_effect=lambda _i, key, _r: recorded.get(key)), \
             mock.patch.object(dispatcher.sm, "get_issue_labels", return_value=labels), \
             mock.patch.object(dispatcher.sm, "has_open_issue_pr", return_value=False), \
             mock.patch.object(dispatcher.sm, "get_active_issue_numbers", return_value=set()), \
             mock.patch.object(dispatcher.sm, "set_labels", side_effect=set_labels), \
             mock.patch.object(dispatcher.sm, "record_dispatch_state", side_effect=record), \
             mock.patch.object(dispatcher, "_run_workflow", side_effect=lambda *args: workflow_calls.append(args) or True):
            first = dispatcher.dispatch_ready(12, "worker:12")
            second = dispatcher.dispatch_ready(12, "worker:12")
        self.assertTrue(first["dispatched"])
        self.assertTrue(second["dispatched"])
        self.assertEqual(len(workflow_calls), 1)

    def test_capacity_full_is_nonterminal(self):
        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="repo"), \
             mock.patch.object(dispatcher.sm, "get_active_issue_numbers", return_value={1, 2}):
            result = dispatcher.dispatch_next("1")
        self.assertFalse(result["dispatched"])
        self.assertEqual(result["reason"], "capacity-full")

    def test_disabled_dispatch_does_not_claim_or_block(self):
        with mock.patch.object(dispatcher.control_state, "require_live", side_effect=control_state.ControlStateError("disabled")):
            result = dispatcher.dispatch_ready(12, "worker:12")
        self.assertFalse(result["dispatched"])
        self.assertEqual(result["reason"], "disabled_or_emergency_stopped")


class TestCIEventTrust(unittest.TestCase):
    def event_file(self, event: dict) -> str:
        file = tempfile.NamedTemporaryFile(mode="w", delete=False)
        json.dump(event, file)
        file.close()
        return file.name

    def test_foreign_or_fork_workflow_run_is_rejected_before_state_mutation(self):
        path = self.event_file({
            "repository": {"full_name": "trusted/repo"},
            "workflow_run": {"head_repository": {"full_name": "fork/repo"}, "status": "completed", "conclusion": "failure"},
        })
        try:
            with mock.patch.dict(os.environ, {"AGENT_REPO": "trusted/repo"}, clear=False):
                result = ci_handler.process_ci_completion(path)
        finally:
            os.unlink(path)
        self.assertEqual(result["action"], "noop")
        self.assertEqual(result["reason"], "fork_or_foreign_head_repository")


class TestExactHeadCI(unittest.TestCase):
    def test_all_seven_required_jobs_must_succeed_on_exact_head(self):
        required = ci_verifier.load_requirements()["required_jobs"]
        run = {
            "databaseId": 1, "workflowName": "tests", "headSha": "a" * 40,
            "status": "completed", "conclusion": "success",
            "jobs": [{"name": name, "status": "completed", "conclusion": "success"} for name in required],
        }
        with mock.patch.object(ci_verifier, "run_info", return_value=run):
            result = ci_verifier.verify_exact_head_ci(2, "a" * 40, 1, {"headRefOid": "a" * 40})
        self.assertEqual(result["successful_jobs"], required)

    def test_moved_head_or_missing_job_is_rejected(self):
        required = ci_verifier.load_requirements()["required_jobs"]
        run = {
            "databaseId": 1, "workflowName": "tests", "headSha": "a" * 40,
            "status": "completed", "conclusion": "success",
            "jobs": [{"name": name, "status": "completed", "conclusion": "success"} for name in required[:-1]],
        }
        with mock.patch.object(ci_verifier, "run_info", return_value=run):
            with self.assertRaises(ci_verifier.CIVerificationError):
                ci_verifier.verify_exact_head_ci(2, "a" * 40, 1, {"headRefOid": "b" * 40})


if __name__ == "__main__":
    unittest.main(verbosity=2)
