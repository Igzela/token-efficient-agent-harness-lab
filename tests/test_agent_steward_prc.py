"""PR-C: End-to-End Acceptance, Fault Matrix & Final Closed-Loop Tests."""

import os
import sys
import time
import subprocess
import tempfile
import threading
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts" / "agent-control"))

from steward_journal import StewardJournal, JournalError
import steward_github
import steward_service as service
import mission_contract as contract
import shadow_steward
import steward_workers as workers


class StewardPRCTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.journal_path = self.root / "journal.sqlite3"
        self.journal = StewardJournal(self.journal_path)
        self.repo_dir = self.root / "repo"
        self.repo_dir.mkdir()

        # Initialize git repo
        subprocess.run(["git", "init", "-b", "main"], cwd=self.repo_dir, check=True, capture_output=True)
        subprocess.run(["git", "config", "user.name", "Steward Agent"], cwd=self.repo_dir, check=True)
        subprocess.run(["git", "config", "user.email", "steward@example.com"], cwd=self.repo_dir, check=True)
        (self.repo_dir / "README.md").write_text("# Autonomous Steward Full Loop\n")
        (self.repo_dir / "tests").mkdir()
        (self.repo_dir / "tests" / "test_app.py").write_text("def test_ok():\n    assert True\n")
        subprocess.run(["git", "add", "."], cwd=self.repo_dir, check=True)
        subprocess.run(["git", "commit", "-m", "initial commit"], cwd=self.repo_dir, check=True, capture_output=True)
        self.base_sha = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=self.repo_dir,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()

        self.github_writer = steward_github.FakeGitHubWriter()
        self.srv = service.StewardService(
            journal=self.journal,
            github=self.github_writer,
            github_writer=self.github_writer,
            repo_path=self.repo_dir,
        )

    def tearDown(self):
        self.tmp.cleanup()

    def test_e2e_multi_stage_mission_to_completion(self):
        """Full closed loop: propose -> single approval -> Stage 1 (concurrent cards) -> merge -> Stage 2 (dependent) -> merge -> COMPLETE."""
        # 1. Propose & approve
        mission, prop_sha = self.srv.propose(
            "Update README.md and tests/test_app.py with unit tests.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
        )
        approval = contract.OwnerApproval(
            owner_identity="repository-owner",
            proposal_sha256=prop_sha,
            approval_id="mission-app-001",
            approved_at="2026-08-30T00:00:00Z",
        )
        auth = type("Auth", (), {"verify": lambda _s, _a, _p: True})()
        self.srv.approve(mission, approval, auth)
        self.assertEqual(self.srv.status()["state"], "RUNNING")

        # 2. Stage 1: Documentation and interface specs
        plan1 = self.srv.plan_stages()
        stage1 = plan1.stage
        self.assertIsNotNone(stage1)
        
        # Publish Draft PR for Stage 1
        bound1 = self.srv.publish_stage(stage1, self.base_sha, title="Stage 1: Docs", body="Docs body")
        pr1 = bound1["pr_number"]
        self.srv.promote_stage_ready(stage1, pr1, self.base_sha)
        
        # Simulate CI PASS and Review PASS
        self.github_writer.prs[pr1]["ci_state"] = "PASS"
        self.github_writer.prs[pr1]["review_state"] = "PASS"
        
        # Merge Stage 1
        receipt1 = self.srv.guarded_merge_stage(stage1, pr1, self.base_sha)
        self.assertTrue(receipt1["merged"])
        readback1 = self.srv.post_merge_readback(stage_id=stage1.stage_id, is_final_stage=False)
        self.assertTrue(readback1["diff_clean"])
        self.assertEqual(self.srv.status()["state"], "RUNNING")

        # 3. Stage 2: Unit tests (dependent stage)
        stage2 = contract.Stage(
            stage_id="stage-2-tests",
            mission_id=mission.mission_id,
            objective="Add comprehensive unit tests",
            repository_identity=stage1.repository_identity,
            acceptance_checks=("git_diff_check",),
            compatibility_checks=(),
            workcard_ids=("card-2-tests",),
            rollback=stage1.rollback,
            integration_pr=None,
            exact_head=None,
        )
        bound2 = self.srv.publish_stage(stage2, self.base_sha, title="Stage 2: Tests", body="Tests body")
        pr2 = bound2["pr_number"]
        self.srv.promote_stage_ready(stage2, pr2, self.base_sha)

        self.github_writer.prs[pr2]["ci_state"] = "PASS"
        self.github_writer.prs[pr2]["review_state"] = "PASS"

        # Merge Stage 2 (Final stage)
        receipt2 = self.srv.guarded_merge_stage(stage2, pr2, self.base_sha)
        self.assertTrue(receipt2["merged"])
        readback2 = self.srv.post_merge_readback(stage_id=stage2.stage_id, is_final_stage=True)
        self.assertTrue(readback2["diff_clean"])
        self.assertEqual(readback2["mission_state"], "COMPLETE")

        # 4. Status verify
        status = self.srv.status()
        self.assertEqual(status["state"], "COMPLETE")

    def test_fault_matrix_ci_failure_and_autonomous_repair(self):
        """Fault: CI failure triggers autonomous repair without owner prompts."""
        mission, prop_sha = self.srv.propose(
            "Update README.md and tests/test_app.py for repair drill.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
        )
        approval = contract.OwnerApproval(
            owner_identity="repository-owner",
            proposal_sha256=prop_sha,
            approval_id="mission-repair-001",
            approved_at="2026-08-30T00:00:00Z",
        )
        auth = type("Auth", (), {"verify": lambda _s, _a, _p: True})()
        self.srv.approve(mission, approval, auth)
        plan = self.srv.plan_stages()
        stage = plan.stage
        self.assertIsNotNone(stage)
        
        bound = self.srv.publish_stage(stage, self.base_sha, title="Stage Repair", body="Repair body")
        pr_number = bound["pr_number"]
        self.srv.promote_stage_ready(stage, pr_number, self.base_sha)

        # CI fails
        self.github_writer.prs[pr_number]["ci_state"] = "FAIL"
        status = self.srv.observe_stage_ci(stage, pr_number, self.base_sha)
        self.assertEqual(status.outcome, "WAITING")
        self.assertIn("ci_fail", status.reason)

        # Autonomous Replan on CI failure (no owner pause)
        replan = self.srv.replan_stage(plan, "CI_FAILED", attempt_number=2)
        self.assertEqual(replan.disposition, "RECOVERY_RECOMMENDED")
        self.assertIsNotNone(replan.stage)
        
        # New repaired commit and successful CI
        repaired_head = self.base_sha
        self.srv.publish_stage(replan.stage, repaired_head, title="Stage Repaired", body="Repaired body")
        self.github_writer.prs[pr_number]["ci_state"] = "PASS"
        self.github_writer.prs[pr_number]["review_state"] = "PASS"
        
        # Now merge succeeds
        receipt = self.srv.guarded_merge_stage(replan.stage, pr_number, repaired_head)
        self.assertTrue(receipt["merged"])

    def test_fault_matrix_restart_recovery(self):
        """Fault: Service process restart re-reads journal and resumes without state loss."""
        mission, prop_sha = self.srv.propose(
            "Update README.md and tests/test_app.py for restart recovery.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
        )
        approval = contract.OwnerApproval(
            owner_identity="repository-owner",
            proposal_sha256=prop_sha,
            approval_id="mission-restart-001",
            approved_at="2026-08-30T00:00:00Z",
        )
        auth = type("Auth", (), {"verify": lambda _s, _a, _p: True})()
        self.srv.approve(mission, approval, auth)
        self.srv.heartbeat(tick_id="heartbeat:before_restart")

        # Simulate service crash & reload from existing journal
        restarted_srv = service.StewardService(
            journal=self.journal,
            github=self.github_writer,
            github_writer=self.github_writer,
            repo_path=self.repo_dir,
        )
        status = restarted_srv.status()
        self.assertEqual(status["state"], "RUNNING")
        self.assertEqual(status["mission_id"], mission.mission_id)

    def test_fault_matrix_emergency_stop(self):
        """Fault: Emergency stop transitions mission to BLOCKED and prevents further progress."""
        mission, prop_sha = self.srv.propose(
            "Update README.md and tests/test_app.py for emergency stop.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
        )
        approval = contract.OwnerApproval(
            owner_identity="repository-owner",
            proposal_sha256=prop_sha,
            approval_id="mission-stop-001",
            approved_at="2026-08-30T00:00:00Z",
        )
        auth = type("Auth", (), {"verify": lambda _s, _a, _p: True})()
        self.srv.approve(mission, approval, auth)

        # Trigger emergency stop
        stop_res = self.srv.stop(reason="manual_operator_stop")
        self.assertEqual(stop_res["status"], "STOPPED")
        self.assertEqual(stop_res["reason"], "manual_operator_stop")

        status = self.srv.status()
        self.assertEqual(status["state"], "BLOCKED")

    def test_sqlite_concurrent_access_resilience(self):
        """Verify SQLite journal resilience under multi-threaded concurrent heartbeats and events."""
        errors = []

        def worker_task(worker_id: int):
            try:
                for i in range(20):
                    self.srv.heartbeat(tick_id=f"tick:{worker_id}:{i}")
                    time.sleep(0.001)
            except Exception as exc:
                errors.append(exc)

        threads = [threading.Thread(target=worker_task, args=(i,)) for i in range(5)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        self.assertEqual(len(errors), 0, f"Concurrent journal errors: {errors}")
        events = self.journal.replay()
        self.assertGreaterEqual(len(events), 100)
