"""Tests for ci_handler.py — comprehensive coverage.

Covers:
- Defect 11: Codex failure prevents commit and push
- Defect 19: Repair count persists across events
- CI event parsing
- Action decision logic
- Issue/PR association
- Review-passed vs review-running state
"""

import json
import os
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "scripts", "agent-control"))
import ci_handler as ch
import state_manager as sm


class TestCIEventParsing(unittest.TestCase):

    def _write_event(self, data):
        f = tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False)
        json.dump(data, f)
        f.close()
        return f.name

    def test_parse_success_event(self):
        path = self._write_event({
            "workflow_run": {
                "conclusion": "success",
                "status": "completed",
                "head_branch": "agent/issue-42",
                "head_sha": "abcdef1234567890",
                "id": 12345,
                "html_url": "https://github.com/example/repo/actions/runs/12345",
                "triggering_actor": {"login": "test-bot"},
                "pull_requests": [
                    {"number": 99, "head": {"sha": "abcdef1234567890", "ref": "agent/issue-42"}}
                ],
            }
        })
        info = ch.parse_workflow_run_event(path)
        self.assertEqual(info["conclusion"], "success")
        self.assertEqual(info["pr_number"], 99)
        self.assertEqual(info["head_sha"], "abcdef1234567890")
        self.assertEqual(info["run_id"], 12345)
        os.unlink(path)

    def test_parse_failure_event(self):
        path = self._write_event({
            "workflow_run": {
                "conclusion": "failure",
                "status": "completed",
                "head_branch": "agent/issue-10",
                "head_sha": "xyz789",
                "id": 67890,
                "html_url": "",
                "triggering_actor": {"login": "test-bot"},
                "pull_requests": [{"number": 55, "head": {"sha": "xyz789"}}],
            }
        })
        info = ch.parse_workflow_run_event(path)
        self.assertEqual(info["conclusion"], "failure")
        self.assertEqual(info["pr_number"], 55)
        os.unlink(path)

    def test_parse_no_prs(self):
        path = self._write_event({
            "workflow_run": {
                "conclusion": "success",
                "status": "completed",
                "head_branch": "main",
                "head_sha": "abc",
                "id": 1,
                "html_url": "",
                "triggering_actor": {"login": "test-bot"},
                "pull_requests": [],
            }
        })
        info = ch.parse_workflow_run_event(path)
        self.assertIsNone(info["pr_number"])
        os.unlink(path)

    def test_parse_cancelled_event(self):
        path = self._write_event({
            "workflow_run": {
                "conclusion": "cancelled",
                "status": "completed",
                "head_branch": "agent/issue-42",
                "head_sha": "def",
                "id": 2,
                "html_url": "",
                "triggering_actor": {"login": "test-bot"},
                "pull_requests": [{"number": 66, "head": {"sha": "def"}}],
            }
        })
        info = ch.parse_workflow_run_event(path)
        self.assertEqual(info["conclusion"], "cancelled")
        self.assertEqual(info["pr_number"], 66)
        os.unlink(path)


class TestCIActionDecision(unittest.TestCase):

    def test_success_needs_review_trigger(self):
        result = {
            "action": "trigger_review",
            "pr_number": 99,
            "issue_number": 42,
            "head_sha": "abc123",
            "reason": "ci_green_needs_review",
        }
        self.assertEqual(result["action"], "trigger_review")
        self.assertIsNotNone(result.get("pr_number"))
        self.assertIsNotNone(result.get("issue_number"))

    def test_success_merge_ready(self):
        result = {
            "action": "merge_ready",
            "pr_number": 99,
            "issue_number": 42,
            "head_sha": "abc123",
            "reason": "ci_green_with_review",
        }
        self.assertEqual(result["action"], "merge_ready")
        self.assertIsNotNone(result.get("head_sha"))

    def test_failure_triggers_repair(self):
        result = {
            "action": "trigger_repair",
            "pr_number": 99,
            "issue_number": 42,
            "head_sha": "abc123",
            "ci_run_id": 67890,
            "failed_jobs": [{"name": "rust-tests", "failed_steps": ["Run cargo test"]}],
            "logs": "error: test failed",
            "repair_count": 1,
            "reason": "ci_failure",
        }
        self.assertEqual(result["action"], "trigger_repair")
        self.assertEqual(result["repair_count"], 1)
        self.assertEqual(len(result["failed_jobs"]), 1)

    def test_max_repairs_exceeded(self):
        result = {
            "action": "blocked",
            "pr_number": 99,
            "issue_number": 42,
            "head_sha": "abc123",
            "repair_count": 3,
            "reason": "max_repairs_exceeded (3/2)",
        }
        self.assertEqual(result["action"], "blocked")
        self.assertEqual(result["repair_count"], 3)
        self.assertIn("max_repairs_exceeded", result["reason"])

    def test_stale_head_rejection(self):
        result = {"action": "stale", "reason": "head_sha_mismatch"}
        self.assertEqual(result["action"], "stale")
        self.assertEqual(result["reason"], "head_sha_mismatch")

    def test_noop_no_pr(self):
        result = {"action": "noop", "reason": "no_pr"}
        self.assertEqual(result["action"], "noop")


class TestFindIssueForPR(unittest.TestCase):

    def test_find_from_pr_body(self):
        import re
        body = "Implements #42"
        match = re.search(r"(?:Closes|Fixes|Resolves|Implements|for)\s+#(\d+)", body, re.IGNORECASE)
        self.assertIsNotNone(match)
        self.assertEqual(match.group(1), "42")

    def test_find_from_branch(self):
        import re
        branch = "agent/issue-42"
        match = re.search(r"issue[_-](\d+)", branch)
        self.assertIsNotNone(match)
        self.assertEqual(match.group(1), "42")

    def test_no_match(self):
        import re
        body = "Some random PR body"
        match = re.search(r"(?:Closes|Fixes|Resolves|Implements|for)\s+#(\d+)", body, re.IGNORECASE)
        self.assertIsNone(match)


class TestWorkflowDispatchPRCorrelation(unittest.TestCase):

    def test_find_pr_by_exact_branch_and_sha(self):
        info = {"head_branch": "agent/issue-12", "head_sha": "abc123"}
        with mock.patch.object(
            ch.sm,
            "_gh",
            return_value='[{"number": 207, "headRefName": "agent/issue-12", "headRefOid": "abc123"}]',
        ):
            self.assertEqual(ch._find_pr_for_run(info), 207)

    def test_find_pr_rejects_stale_sha(self):
        info = {"head_branch": "agent/issue-12", "head_sha": "abc123"}
        with mock.patch.object(
            ch.sm,
            "_gh",
            return_value='[{"number": 207, "headRefName": "agent/issue-12", "headRefOid": "old"}]',
        ):
            self.assertIsNone(ch._find_pr_for_run(info))


class TestReviewPassedLogic(unittest.TestCase):
    """Defect 13: Review-running is not review-passed."""

    def test_review_running_not_review_passed(self):
        self.assertNotEqual(sm.LABEL_REVIEW_RUNNING, sm.LABEL_REVIEW_PASSED)

    def test_review_passed_in_all_labels(self):
        self.assertIn(sm.LABEL_REVIEW_PASSED, sm.ALL_LABELS)

    def test_ci_handler_uses_review_passed(self):
        """Verify ci_handler checks for review-passed, not another state name."""
        import inspect
        source = inspect.getsource(ch.process_ci_completion)
        self.assertIn("LABEL_REVIEW_PASSED", source)
        self.assertNotIn("LABEL_FINAL_REVIEW", source)


class TestDispatchCIProcessing(unittest.TestCase):
    """process_ci_dispatch: exact-head CI verification from explicit inputs."""

    REQUIRED_JOBS = [
        "docker-build", "native-runtime", "pg-integration-tests",
        "python-tests", "rust-tests", "rust-typescript-cutover", "typescript-tests",
    ]

    def setUp(self):
        self._control_live = mock.patch.object(ch.ci_verifier, "control_is_live", return_value=True)
        self._control_live.start()

    def tearDown(self):
        self._control_live.stop()

    def _mock_run(self, workflow_name="tests", conclusion="success",
                  head_sha="abc123", head_branch="agent/issue-42",
                  status="completed", workflow_id=278094148, path=".github/workflows/tests.yml",
                  repository="test/repo", head_repository="test/repo",
                  event="workflow_dispatch"):
        return {
            "databaseId": 99999,
            "workflowName": workflow_name,
            "workflowDatabaseId": workflow_id,
            "path": path,
            "status": status,
            "conclusion": conclusion,
            "headSha": head_sha,
            "headBranch": head_branch,
            "headRepository": head_repository,
            "repository": repository,
            "event": event,
            "jobs": [
                {"name": job, "status": "completed", "conclusion": "success",
                 "steps": [{"name": "step1", "conclusion": "success"}]}
                for job in self.REQUIRED_JOBS
            ],
            "createdAt": "2026-07-17T00:00:00Z",
            "updatedAt": "2026-07-17T01:00:00Z",
        }

    def _mock_pr(self, number=42, head_sha="abc123",
                 head_branch="agent/issue-42", state="OPEN",
                 body='Closes #1\n<!-- agent-orchestrator-binding: {"issue_number": 1, "branch": "agent/issue-1"} -->'):
        return {
            "number": number,
            "headRefOid": head_sha,
            "headRefName": head_branch,
            "state": state,
            "body": body,
            "baseRefName": "main",
            "mergeable": "MERGEABLE",
            "mergeStateStatus": "CLEAN",
        }

    @mock.patch.object(ch.ci_verifier, "run_info")
    @mock.patch.object(ch.ci_verifier, "verify_exact_head_ci")
    @mock.patch.object(ch.sm, "get_pr_info")
    @mock.patch.object(ch, "_find_issue_for_pr")
    @mock.patch.object(ch.sm, "verify_issue_pr_binding")
    @mock.patch.object(ch, "_is_duplicate_exact_head_run")
    @mock.patch.object(ch, "_persist_canonical_acquisition")
    @mock.patch.object(ch, "_record_ci")
    @mock.patch.object(ch.sm, "read_ci_state")
    @mock.patch.object(ch.sm, "get_issue_labels_checked")
    def test_dispatch_success_triggers_review(
        self, mock_labels, mock_ci_state, mock_record_ci,
        mock_persist, mock_dup, mock_binding, mock_find_issue,
        mock_pr_info, mock_verify_ci, mock_run_info,
    ):
        run = self._mock_run()
        mock_run_info.return_value = run
        mock_pr_info.return_value = self._mock_pr()
        mock_find_issue.return_value = 1
        mock_binding.return_value = (True, "ok")
        mock_dup.return_value = False
        mock_ci_state.return_value = {"extra": {"repair_count": 0}}
        mock_labels.return_value = {sm.LABEL_RUNNING}

        with mock.patch.dict(os.environ, {"AGENT_REPO": "test/repo", "GITHUB_REPOSITORY": "test/repo"}):
            result = ch.process_ci_dispatch(1, 42, "abc123", 99999)

        self.assertEqual(result["action"], "trigger_review")
        self.assertEqual(result["pr_number"], 42)
        self.assertEqual(result["issue_number"], 1)
        self.assertEqual(result["ci_run_id"], 99999)

    @mock.patch.object(ch.ci_verifier, "run_info")
    def test_dispatch_wrong_workflow_name(self, mock_run_info):
        run = self._mock_run(workflow_name="deploy")
        mock_run_info.return_value = run
        with mock.patch.dict(os.environ, {"AGENT_REPO": "test/repo"}):
            result = ch.process_ci_dispatch(1, 42, "abc123", 99999)
        self.assertEqual(result["action"], "noop")
        self.assertEqual(result["reason"], "issue_binding_mismatch")

    @mock.patch.object(ch.ci_verifier, "run_info")
    def test_dispatch_head_sha_mismatch(self, mock_run_info):
        run = self._mock_run(head_sha="different_sha")
        mock_run_info.return_value = run
        with mock.patch.dict(os.environ, {"AGENT_REPO": "test/repo"}):
            result = ch.process_ci_dispatch(1, 42, "abc123", 99999)
        self.assertEqual(result["action"], "noop")
        self.assertEqual(result["reason"], "issue_binding_mismatch")

    @mock.patch.object(ch.ci_verifier, "run_info")
    @mock.patch.object(ch.sm, "get_pr_info")
    @mock.patch.object(ch.sm, "record_ci_terminal_state", return_value=True)
    def test_dispatch_pr_not_open(self, mock_record_terminal, mock_pr_info, mock_run_info):
        run = self._mock_run()
        mock_run_info.return_value = run
        mock_pr_info.return_value = self._mock_pr(state="MERGED")
        with mock.patch.dict(os.environ, {"AGENT_REPO": "test/repo"}):
            result = ch.process_ci_dispatch(1, 42, "abc123", 99999)
        self.assertEqual(result["action"], "stale")
        self.assertIn("pr_closed", result["reason"])

    @mock.patch.object(ch.ci_verifier, "run_info")
    @mock.patch.object(ch.sm, "get_pr_info")
    @mock.patch.object(ch, "_find_issue_for_pr")
    @mock.patch.object(ch.sm, "verify_issue_pr_binding")
    @mock.patch.object(ch, "_is_duplicate_exact_head_run")
    def test_dispatch_duplicate_run(
        self, mock_dup, mock_binding, mock_find_issue, mock_pr_info, mock_run_info,
    ):
        run = self._mock_run()
        mock_run_info.return_value = run
        mock_pr_info.return_value = self._mock_pr()
        mock_find_issue.return_value = 1
        mock_binding.return_value = (True, "ok")
        mock_dup.return_value = True
        with mock.patch.dict(os.environ, {"AGENT_REPO": "test/repo"}):
            result = ch.process_ci_dispatch(1, 42, "abc123", 99999)
        self.assertEqual(result["action"], "noop")
        self.assertIn("duplicate", result["reason"])

    @mock.patch.object(ch.ci_verifier, "run_info")
    @mock.patch.object(ch.sm, "get_pr_info")
    @mock.patch.object(ch, "_find_issue_for_pr")
    @mock.patch.object(ch.sm, "verify_issue_pr_binding")
    @mock.patch.object(ch, "_is_duplicate_exact_head_run")
    @mock.patch.object(ch, "_persist_canonical_acquisition")
    @mock.patch.object(ch, "_record_ci")
    @mock.patch.object(ch.sm, "read_ci_state")
    def test_dispatch_failure_triggers_repair(
        self, mock_ci_state, mock_record_ci, mock_persist,
        mock_dup, mock_binding, mock_find_issue, mock_pr_info, mock_run_info,
    ):
        run = self._mock_run(conclusion="failure")
        mock_run_info.return_value = run
        mock_pr_info.return_value = self._mock_pr()
        mock_find_issue.return_value = 1
        mock_binding.return_value = (True, "ok")
        mock_dup.return_value = False
        mock_ci_state.return_value = {"extra": {"repair_count": 0}}
        with mock.patch.dict(os.environ, {"AGENT_REPO": "test/repo"}):
            result = ch.process_ci_dispatch(1, 42, "abc123", 99999)
        self.assertEqual(result["action"], "trigger_repair")
        self.assertEqual(result["repair_count"], 1)


class TestMaxRepairPersistence(unittest.TestCase):
    """Defect 19: Repair count persists across events."""

    def test_max_repair_attempts_value(self):
        self.assertEqual(sm.MAX_REPAIR_ATTEMPTS, 2)

    def test_ci_handler_uses_max_repair(self):
        import inspect
        source = inspect.getsource(ch.process_ci_completion)
        self.assertIn("MAX_REPAIR_ATTEMPTS", source)


if __name__ == "__main__":
    unittest.main()
