"""Contract tests for actions/exact-head-check without network."""

from __future__ import annotations

import json
import os
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ACTION = ROOT / "actions" / "exact-head-check"
VERIFY = ACTION / "verify.sh"
LOCAL_TEST = ACTION / "test_verify_local.sh"


class TestExactHeadAction(unittest.TestCase):
    BASE = "b" * 40
    HEAD = "a" * 40

    def test_action_yml_exists_and_is_composite(self):
        text = (ACTION / "action.yml").read_text(encoding="utf-8")
        self.assertIn("using: composite", text)
        self.assertIn("expected-head", text)
        self.assertIn("pull-request", text)
        self.assertIn("github-token", text)
        self.assertIn("require-review-receipt", text)
        self.assertIn("INPUT_REQUIRE_REVIEW_RECEIPT", text)
        self.assertIn("verify.sh", text)

    def test_verify_script_is_executable_contract(self):
        text = VERIFY.read_text(encoding="utf-8")
        self.assertIn("exact-head-check-proof.v1", text)
        self.assertIn("head_moved", text)
        self.assertIn("INPUT_ALLOW_FORK_HEAD", text)
        self.assertIn("INPUT_REQUIRE_REVIEW_RECEIPT", text)
        self.assertIn("trusted exact-head review receipt confirmed", text)
        self.assertIn("_build_review_observation", text)
        self.assertIn("reviewer_authenticated_identity", text)
        self.assertIn("reviewer_author_identity", text)
        self.assertIn("GITHUB_STEP_SUMMARY", text)
        self.assertIn("merges: false", text)
        self.assertIn("model_calls: false", text)

    def test_local_validation_script_passes(self):
        result = subprocess.run(
            ["bash", str(LOCAL_TEST)],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertIn("passed", result.stdout)

    def test_repository_workflow_allows_forks_only_from_trusted_base_code(self):
        workflow = ROOT / ".github" / "workflows" / "exact-head-check.yml"
        text = workflow.read_text(encoding="utf-8")
        self.assertIn("ref: ${{ github.event.pull_request.base.sha }}", text)
        self.assertIn("path: trusted-base", text)
        self.assertIn("uses: ./trusted-base/actions/exact-head-check", text)
        self.assertIn('allow-fork-head: "true"', text)
        self.assertIn("issues: read", text)
        self.assertIn("require-review-receipt:", text)
        self.assertIn("github.event.action == 'ready_for_review'", text)
        self.assertNotIn("uses: ./actions/exact-head-check", text)

    def test_example_workflow_present(self):
        example = ROOT / "examples" / "github-actions" / "exact-head-check.yml"
        text = example.read_text(encoding="utf-8")
        self.assertIn("exact-head-check", text)
        self.assertIn("expected-head", text)
        self.assertIn("upload-artifact", text)

    def _receipt(self, *, reviewed_sha=None, author="Igzela"):
        return {
            "body": (
                "EXACT-HEAD REVIEW RECEIPT\n"
                f"Reviewed SHA: {reviewed_sha or self.HEAD}\n"
                f"Reviewed range: {self.BASE}...{reviewed_sha or self.HEAD}\n"
                "Reviewer session identity: 019ff89b-eb32-7232-beeb-150fd146f582\n"
                f"Reviewer authenticated identity: {author}\n"
                "Review transport: parent-posted-on-behalf-of-independent-session\n"
                "Implementation session identity: 019ff89b-eb32-7232-beeb-150fd146f583\n"
                "Observed at: 2026-08-25T00:00:00Z\n"
                "Axes: architecture, authority, compatibility, security, audit, rollback, scope/path binding\n"
                "Outcome: PASS\n"
                "Unresolved objections: none\n"
            ),
            "user": {"login": "Igzela"},
        }

    def _run_verify(self, *, issue_comments, reviews=None, fail_api=False):
        with tempfile.TemporaryDirectory() as tmp:
            gh = Path(tmp) / "gh"
            gh.write_text(
                "#!/usr/bin/env python3\n"
                "import json, os, sys\n"
                "args = sys.argv[1:]\n"
                "if os.environ.get('FAKE_GH_FAIL') == '1':\n"
                "    raise SystemExit(7)\n"
                "path = next((a for a in args if a.startswith('repos/')), '')\n"
                "if '/pulls/7' in path and '/comments' not in path and '/reviews' not in path:\n"
                "    print(json.dumps({'number': 7, 'state': 'open', 'head_sha': os.environ['FAKE_HEAD'], 'head_repo': 'o/r', 'base_repo': 'o/r', 'base_sha': os.environ['FAKE_BASE'], 'pr_author': 'Igzela'}))\n"
                "elif '/issues/7/comments' in path:\n"
                "    print(json.dumps([json.loads(os.environ['FAKE_ISSUE_COMMENTS'])]))\n"
                "elif '/pulls/7/comments' in path:\n"
                "    print(json.dumps([[]]))\n"
                "elif '/pulls/7/reviews' in path:\n"
                "    print(json.dumps([json.loads(os.environ.get('FAKE_REVIEWS', '[]'))]))\n"
                "else:\n"
                "    raise SystemExit('unexpected gh api path: ' + path)\n",
                encoding="utf-8",
            )
            gh.chmod(gh.stat().st_mode | stat.S_IXUSR)
            env = os.environ.copy()
            env.update(
                {
                    "PATH": f"{tmp}:{env['PATH']}",
                    "GITHUB_ACTION_PATH": str(ACTION),
                    "GITHUB_REPOSITORY": "o/r",
                    "INPUT_REPOSITORY": "o/r",
                    "INPUT_PULL_REQUEST": "7",
                    "INPUT_EXPECTED_HEAD": self.HEAD,
                    "INPUT_ALLOW_FORK_HEAD": "false",
                    "INPUT_REQUIRE_REVIEW_RECEIPT": "true",
                    "FAKE_HEAD": self.HEAD,
                    "FAKE_BASE": self.BASE,
                    "FAKE_ISSUE_COMMENTS": json.dumps(issue_comments),
                    "FAKE_REVIEWS": json.dumps(reviews or []),
                }
            )
            if fail_api:
                env["FAKE_GH_FAIL"] = "1"
            return subprocess.run(
                ["bash", str(VERIFY)],
                cwd=ROOT,
                env=env,
                capture_output=True,
                text=True,
                check=False,
            )

    def test_mocked_valid_receipt_passes(self):
        result = self._run_verify(issue_comments=[self._receipt()])
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertIn("review=confirmed", result.stdout)

    def test_mocked_current_receipt_with_stale_history_passes(self):
        stale = self._receipt(reviewed_sha="c" * 40)
        result = self._run_verify(issue_comments=[stale, self._receipt()])
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertIn("review=confirmed", result.stdout)

    def test_mocked_receipt_missing_governance_axis_fails_closed(self):
        incomplete = self._receipt()
        incomplete["body"] = incomplete["body"].replace(
            ", rollback", ""
        )
        result = self._run_verify(issue_comments=[incomplete])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("review_axes_missing:rollback", result.stderr)

    def test_mocked_stale_receipt_fails_closed(self):
        stale = self._receipt(reviewed_sha="c" * 40)
        result = self._run_verify(issue_comments=[stale])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("review_receipt_not_for_current_head", result.stderr)

    def test_mocked_multiple_current_receipts_fail_closed(self):
        result = self._run_verify(
            issue_comments=[self._receipt(), self._receipt()]
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("multiple_current_head_review_receipts", result.stderr)

    def test_mocked_malformed_current_receipt_fails_closed(self):
        malformed = self._receipt()
        malformed["body"] = malformed["body"].replace(
            "Outcome: PASS", "Outcome: MAYBE"
        )
        result = self._run_verify(issue_comments=[malformed])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("review_outcome_is_not_exact_pass", result.stderr)

    def test_mocked_authenticated_identity_mismatch_fails_closed(self):
        result = self._run_verify(issue_comments=[self._receipt(author="OtherUser")])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("reviewer_authenticated_identity_mismatch", result.stderr)

    def test_mocked_github_api_failure_fails_closed(self):
        result = self._run_verify(issue_comments=[], fail_api=True)
        self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
