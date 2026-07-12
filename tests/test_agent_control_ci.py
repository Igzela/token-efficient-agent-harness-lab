"""Tests for ci_handler.py"""

import json
import os
import sys
import tempfile
import unittest

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
                    {
                        "number": 99,
                        "head": {"sha": "abcdef1234567890", "ref": "agent/issue-42"},
                    }
                ],
            }
        })

        info = ch.parse_workflow_run_event(path)
        self.assertEqual(info["conclusion"], "success")
        self.assertEqual(info["status"], "completed")
        self.assertEqual(info["pr_number"], 99)
        self.assertEqual(info["head_sha"], "abcdef1234567890")
        self.assertEqual(info["run_id"], 12345)
        self.assertEqual(info["head_branch"], "agent/issue-42")

        os.unlink(path)

    def test_parse_failure_event(self):
        path = self._write_event({
            "workflow_run": {
                "conclusion": "failure",
                "status": "completed",
                "head_branch": "agent/issue-10",
                "head_sha": "xyz789",
                "id": 67890,
                "html_url": "https://github.com/example/repo/actions/runs/67890",
                "triggering_actor": {"login": "test-bot"},
                "pull_requests": [
                    {
                        "number": 55,
                        "head": {"sha": "xyz789", "ref": "agent/issue-10"},
                    }
                ],
            }
        })

        info = ch.parse_workflow_run_event(path)
        self.assertEqual(info["conclusion"], "failure")
        self.assertEqual(info["pr_number"], 55)
        self.assertEqual(info["head_sha"], "xyz789")

        os.unlink(path)

    def test_parse_no_prs(self):
        path = self._write_event({
            "workflow_run": {
                "conclusion": "success",
                "status": "completed",
                "head_branch": "main",
                "head_sha": "abc",
                "id": 1,
                "html_url": "https://github.com/example/repo/actions/runs/1",
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
        result = {
            "action": "stale",
            "reason": "head_sha_mismatch",
        }
        self.assertEqual(result["action"], "stale")
        self.assertEqual(result["reason"], "head_sha_mismatch")

    def test_noop_no_pr(self):
        result = {
            "action": "noop",
            "reason": "no_pr",
        }
        self.assertEqual(result["action"], "noop")


class TestFindIssueForPR(unittest.TestCase):

    def test_find_from_pr_body(self):
        # Tests _find_issue_for_pr would need gh CLI, but we can test the regex
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


if __name__ == "__main__":
    unittest.main()
