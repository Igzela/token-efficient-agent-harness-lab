"""PR-B: GitHub Integration and Guarded Merge Loop tests."""

import os
import sys
from pathlib import Path
import subprocess
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts" / "agent-control"))

from steward_journal import StewardJournal
import steward_github
import steward_service as service
import mission_contract as contract
import shadow_steward
import steward_workers as workers


class StewardPRBTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.journal_path = self.root / "journal.sqlite3"
        self.journal = StewardJournal(self.journal_path)
        self.repo_dir = self.root / "repo"
        self.repo_dir.mkdir()

        # Initialize git repo
        subprocess.run(["git", "init", "-b", "main"], cwd=self.repo_dir, check=True, capture_output=True)
        subprocess.run(["git", "config", "user.name", "Test Agent"], cwd=self.repo_dir, check=True)
        subprocess.run(["git", "config", "user.email", "agent@example.com"], cwd=self.repo_dir, check=True)
        (self.repo_dir / "README.md").write_text("# Autonomous Steward Test\n")
        subprocess.run(["git", "add", "."], cwd=self.repo_dir, check=True)
        subprocess.run(["git", "commit", "-m", "init"], cwd=self.repo_dir, check=True, capture_output=True)
        self.base_sha = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=self.repo_dir,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()

        # Setup Fake GitHub Writer
        self.github_writer = steward_github.FakeGitHubWriter()
        self.srv = service.StewardService(
            journal=self.journal,
            github=self.github_writer,
            github_writer=self.github_writer,
            repo_path=self.repo_dir,
        )

        # Propose and approve a mission
        self.mission, self.proposal_sha256 = self.srv.propose(
            "Update README.md with test details.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
        )
        self.approval = contract.OwnerApproval(
            owner_identity="repository-owner",
            proposal_sha256=self.proposal_sha256,
            approval_id="issue-201",
            approved_at="2026-08-30T00:00:00Z",
        )
        authenticator = type("Auth", (), {"verify": lambda _s, _a, _p: True})()
        self.activated_mission = self.srv.approve(self.mission, self.approval, authenticator)
        self.plan = self.srv.plan_stages()

    def tearDown(self):
        self.tmp.cleanup()

    def test_publish_stage_draft_pr(self):
        """Verify publish_stage creates a Draft PR and records STAGE_PR_BOUND."""
        stage = self.plan.stage
        self.assertIsNotNone(stage)
        integration_head = self.base_sha

        bound = self.srv.publish_stage(
            stage,
            integration_head,
            title="Stage PR Title",
            body="Stage PR Description",
        )
        pr_number = bound["pr_number"]
        self.assertEqual(pr_number, 101)
        self.assertTrue(bound["draft"])

        # Check journal event
        events = self.journal.replay()
        bound_event = next((e for e in events if e.event == "STAGE_PR_BOUND" and e.mission_id == self.mission.mission_id), None)
        self.assertIsNotNone(bound_event)
        self.assertEqual(bound_event.data["pr_number"], 101)
        self.assertEqual(bound_event.data["head_sha"], integration_head)

    def test_promote_stage_ready(self):
        """Verify promote_stage_ready marks the Draft PR ready and records STAGE_PR_READY."""
        stage = self.plan.stage
        bound = self.srv.publish_stage(
            stage,
            self.base_sha,
            title="Stage PR Title",
            body="Stage PR Description",
        )
        pr_number = bound["pr_number"]

        promoted = self.srv.promote_stage_ready(stage, pr_number, self.base_sha)
        self.assertTrue(promoted)

        # Check writer state
        facts = self.github_writer.fetch_stage_pr(self.mission.repository_identity.repository, pr_number)
        self.assertFalse(facts["draft"])

        # Check journal event
        events = self.journal.replay()
        ready_event = next((e for e in events if e.event == "STAGE_PR_READY" and e.mission_id == self.mission.mission_id), None)
        self.assertIsNotNone(ready_event)
        self.assertEqual(ready_event.data["pr_number"], pr_number)

    def test_observe_stage_ci_lifecycle(self):
        """Verify observe_stage_ci handles PENDING, FAIL, and PASS transitions."""
        stage = self.plan.stage
        bound = self.srv.publish_stage(
            stage,
            self.base_sha,
            title="Stage PR Title",
            body="Stage PR Description",
        )
        pr_number = bound["pr_number"]
        self.srv.promote_stage_ready(stage, pr_number, self.base_sha)

        # Initial state: CI PENDING
        status = self.srv.observe_stage_ci(stage, pr_number, self.base_sha)
        self.assertEqual(status.outcome, "WAITING")
        self.assertIn("ci_pending", status.reason)

        # CI FAIL
        self.github_writer.prs[pr_number]["ci_state"] = "FAIL"
        status_fail = self.srv.observe_stage_ci(stage, pr_number, self.base_sha)
        self.assertEqual(status_fail.outcome, "WAITING")
        self.assertIn("ci_fail", status_fail.reason)

        # CI PASS and Review PASS -> WAITING_FOR_MERGE
        self.github_writer.prs[pr_number]["ci_state"] = "PASS"
        self.github_writer.prs[pr_number]["review_state"] = "PASS"
        status_pass = self.srv.observe_stage_ci(stage, pr_number, self.base_sha)
        self.assertEqual(status_pass.outcome, "WAITING_FOR_MERGE")

        # Check journal recorded STAGE_WAITING_FOR_MERGE
        events = self.journal.replay()
        waiting_event = next((e for e in events if e.event == "STAGE_WAITING_FOR_MERGE" and e.mission_id == self.mission.mission_id), None)
        self.assertIsNotNone(waiting_event)

    def test_guarded_merge_and_post_merge_readback(self):
        """Verify guarded merge executes on eligible head and post-merge readback completes mission."""
        stage = self.plan.stage
        bound = self.srv.publish_stage(
            stage,
            self.base_sha,
            title="Stage PR Title",
            body="Stage PR Description",
        )
        pr_number = bound["pr_number"]
        self.srv.promote_stage_ready(stage, pr_number, self.base_sha)

        self.github_writer.prs[pr_number]["ci_state"] = "PASS"
        self.github_writer.prs[pr_number]["review_state"] = "PASS"

        # Execute guarded merge
        receipt = self.srv.guarded_merge_stage(stage, pr_number, self.base_sha)
        self.assertTrue(receipt["merged"])
        self.assertEqual(receipt["pr_number"], pr_number)

        # Check writer state
        facts = self.github_writer.fetch_stage_pr(self.mission.repository_identity.repository, pr_number)
        self.assertTrue(facts["merged"])

        # Post merge readback for final stage
        readback = self.srv.post_merge_readback(is_final_stage=True)
        self.assertTrue(readback["diff_clean"])
        self.assertEqual(readback["mission_state"], "COMPLETE")

        # Verify journal status
        status = self.srv.status()
        self.assertEqual(status["state"], "COMPLETE")

    def test_guarded_merge_rejects_ineligible_pr(self):
        """Verify guarded_merge_stage raises GitHubMutationError when PR is not eligible."""
        stage = self.plan.stage
        bound = self.srv.publish_stage(
            stage,
            self.base_sha,
            title="Stage PR Title",
            body="Stage PR Description",
        )
        pr_number = bound["pr_number"]
        # PR is still draft and CI is PENDING
        with self.assertRaises(steward_github.GitHubMutationError):
            self.srv.guarded_merge_stage(stage, pr_number, self.base_sha)

    def test_guarded_merge_fail_closed_on_unknown_outcome(self):
        """Verify guarded merge fails closed when external mutation returns unknown outcome."""
        stage = self.plan.stage
        bound = self.srv.publish_stage(
            stage,
            self.base_sha,
            title="Stage PR Title",
            body="Stage PR Description",
        )
        pr_number = bound["pr_number"]
        self.srv.promote_stage_ready(stage, pr_number, self.base_sha)
        self.github_writer.prs[pr_number]["ci_state"] = "PASS"
        self.github_writer.prs[pr_number]["review_state"] = "PASS"

        # Inject unknown outcome fault
        self.github_writer.merge_outcome_unknown = True
        with self.assertRaises(steward_github.GitHubMutationError) as ctx:
            self.srv.guarded_merge_stage(stage, pr_number, self.base_sha)
        self.assertIn("merge_outcome_unknown", str(ctx.exception))
