"""PR-B Unit and Integration Tests for Single Canonical Merge Owner & Mutation Guards."""

from __future__ import annotations

import json
from pathlib import Path
import sys
import unittest
from unittest.mock import MagicMock, patch

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts" / "agent-control"))

import steward_github as gh
from steward_github import (
    FakeGitHubReader,
    FakeGitHubWriter,
    GhGitHubWriter,
    GitHubMutationError,
    GitHubPreflightError,
    GitHubFactsError,
)


class TestAutonomousStewardPRB(unittest.TestCase):
    def setUp(self):
        self.repo = "Igzela/token-efficient-agent-harness-lab"
        self.head_sha = "a" * 40
        self.base_sha = "b" * 40

    def test_fake_writer_contract_parity(self):
        """Verify FakeGitHubWriter provides complete fidelity to GitHubWriter protocol."""
        writer = FakeGitHubWriter(initial_pr_number=201)

        # 1. Create Stage Draft PR
        pr = writer.create_or_update_stage_pr(
            stage_id="stage-1",
            mission_id="mission-1",
            branch="stage/stage-1",
            expected_sha=self.head_sha,
            base_sha=self.base_sha,
            title="Stage 1 PR",
            body="Stage 1 PR description",
            repository=self.repo,
        )
        self.assertEqual(pr["pr_number"], 201)
        self.assertTrue(pr["draft"])
        self.assertFalse(pr["merged"])
        self.assertEqual(pr["head_sha"], self.head_sha)

        # 2. Mark Ready fails on head mismatch
        with self.assertRaises(GitHubMutationError):
            writer.mark_ready(self.repo, 201, "f" * 40)

        # 3. Mark Ready succeeds on exact head match
        ready = writer.mark_ready(self.repo, 201, self.head_sha)
        self.assertTrue(ready)
        facts = writer.fetch_stage_pr(self.repo, 201)
        self.assertFalse(facts["draft"])

        # 4. Guarded merge fails on head mismatch
        with self.assertRaises(GitHubMutationError):
            writer.guarded_merge(self.repo, 201, "f" * 40)

        # 5. Guarded merge succeeds on exact head match
        merge_res = writer.guarded_merge(self.repo, 201, self.head_sha)
        self.assertTrue(merge_res["merged"])
        self.assertEqual(merge_res["head_sha"], self.head_sha)

        # 6. Post-merge readback returns authoritative SHA
        readback = writer.post_merge_readback(self.repo, 201, self.head_sha)
        self.assertEqual(readback["status"], "VERIFIED")
        self.assertEqual(readback["accepted_main_sha"], self.head_sha)

    def test_mark_ready_exact_head_guard(self):
        """Verify GhGitHubWriter.mark_ready re-reads live head before mutating PR state."""
        writer = GhGitHubWriter(timeout_seconds=10)

        # Mock reader to return mismatched head
        mock_facts = {
            "repository": self.repo,
            "pr_number": 301,
            "state": "OPEN",
            "draft": True,
            "merged": False,
            "base_sha": self.base_sha,
            "head_sha": "9" * 40,  # Different from expected head
            "ci_state": "PASS",
            "review_state": "PASS",
            "base_branch": "main",
            "head_branch": "stage-branch",
        }

        with patch.object(writer.reader, "fetch_stage_pr", return_value=mock_facts):
            with self.assertRaisesRegex(GitHubMutationError, "exact_head_mismatch_before_mark_ready"):
                writer.mark_ready(self.repo, 301, self.head_sha)

    def test_review_receipt_binding_drift_fails_before_any_mutation(self):
        writer = GhGitHubWriter(timeout_seconds=10)
        facts = {
            "repository": self.repo,
            "pr_number": 305,
            "state": "OPEN",
            "draft": True,
            "merged": False,
            "base_sha": "c" * 40,
            "head_sha": self.head_sha,
            "ci_state": "PENDING",
            "review_state": "PENDING",
            "base_branch": "main",
            "head_branch": "stage-branch",
        }
        with (
            patch.object(writer, "fetch_stage_pr", return_value=facts),
            patch("subprocess.run") as mutation,
        ):
            with self.assertRaisesRegex(
                GitHubPreflightError, "review_receipt_exact_binding_mismatch"
            ):
                writer.publish_exact_head_review(
                    self.repo,
                    305,
                    self.head_sha,
                    base_sha=self.base_sha,
                    reviewer_session_id="review-session",
                    implementation_session_id="implementation-session",
                    reviewed_range_sha256="d" * 64,
                    review_receipt_sha256="e" * 64,
                )
        mutation.assert_not_called()

    def test_guarded_merge_dispatches_workflow_and_verifies_result(self):
        """Verify guarded_merge delegates strictly to agent-merge.yml and reads back merge proof."""
        writer = GhGitHubWriter(timeout_seconds=5)

        mock_facts_before = {
            "repository": self.repo,
            "pr_number": 302,
            "state": "OPEN",
            "draft": False,
            "merged": False,
            "base_sha": self.base_sha,
            "head_sha": self.head_sha,
            "ci_state": "PASS",
            "review_state": "PASS",
            "base_branch": "main",
            "head_branch": "stage-branch",
        }

        mock_facts_after = dict(mock_facts_before)
        mock_facts_after["merged"] = True
        mock_facts_after["state"] = "MERGED"

        # Dispatch command succeeds, readback returns merged
        with patch("subprocess.run") as mock_run:
            mock_run.return_value = MagicMock(returncode=0, stdout="", stderr="")
            with patch.object(writer, "fetch_stage_pr", side_effect=[mock_facts_before, mock_facts_after]):
                result = writer.guarded_merge(self.repo, 302, self.head_sha, workflow_file="agent-merge.yml")
                self.assertTrue(result["merged"])
                self.assertEqual(result["head_sha"], self.head_sha)

                # Verify subprocess ran workflow run dispatch, NOT gh pr merge
                call_args = mock_run.call_args[0][0]
                self.assertIn("workflow", call_args)
                self.assertIn("run", call_args)
                self.assertIn("agent-merge.yml", call_args)
                self.assertNotIn("merge", call_args[:3])

    def test_guarded_merge_fail_closed_on_indeterminate_outcome(self):
        """Verify guarded_merge raises merge_outcome_unknown when timeout occurs and merged status cannot be proven."""
        writer = GhGitHubWriter(timeout_seconds=1)

        mock_facts_unmerged = {
            "repository": self.repo,
            "pr_number": 303,
            "state": "OPEN",
            "draft": False,
            "merged": False,
            "base_sha": self.base_sha,
            "head_sha": self.head_sha,
            "ci_state": "PASS",
            "review_state": "PASS",
            "base_branch": "main",
            "head_branch": "stage-branch",
        }

        with patch("subprocess.run") as mock_run:
            mock_run.return_value = MagicMock(returncode=0, stdout="", stderr="")
            with patch.object(writer, "fetch_stage_pr", return_value=mock_facts_unmerged):
                with self.assertRaisesRegex(GitHubMutationError, "merge_outcome_unknown"):
                    writer.guarded_merge(self.repo, 303, self.head_sha, timeout_seconds=1)

    def test_post_merge_readback_authoritative_sha(self):
        """Verify post_merge_readback queries GitHub main branch SHA and validates integrity."""
        writer = GhGitHubWriter(timeout_seconds=5)

        merged_pr = {
            "number": 304,
            "state": "closed",
            "merged": True,
            "head": {"sha": self.head_sha},
            "merge_commit_sha": "d" * 40,
        }
        branch = {"commit": {"sha": "d" * 40}}
        with patch("subprocess.run") as mock_run:
            mock_run.side_effect = [
                MagicMock(returncode=0, stdout=json.dumps(merged_pr), stderr=""),
                MagicMock(returncode=0, stdout=json.dumps(branch), stderr=""),
            ]
            readback = writer.post_merge_readback(self.repo, 304, self.head_sha)
            self.assertEqual(readback["schema_version"], "post_merge_readback.v2")
            self.assertEqual(readback["accepted_main_sha"], "d" * 40)
            self.assertEqual(readback["status"], "VERIFIED")

        # Reject a main tip that cannot be proved to be the PR merge commit.
        with patch("subprocess.run") as mock_run:
            mock_run.side_effect = [
                MagicMock(returncode=0, stdout=json.dumps(merged_pr), stderr=""),
                MagicMock(returncode=0, stdout=json.dumps({"commit": {"sha": "e" * 40}}), stderr=""),
            ]
            with self.assertRaises(GitHubFactsError):
                writer.post_merge_readback(self.repo, 304, self.head_sha)


if __name__ == "__main__":
    unittest.main()
