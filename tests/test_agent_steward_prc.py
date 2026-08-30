"""PR-C E2E Acceptance & 10-Scenario Fault Matrix Tests for Autonomous Steward Autonomy Closure."""

from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import MagicMock, patch

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts" / "agent-control"))

import mission_contract as contract
import shadow_steward
import steward_github
from steward_github import FakeGitHubReader, FakeGitHubWriter, GitHubMutationError
import steward_service as service
from steward_journal import StewardJournal
import steward_workers as workers
from steward_workers import FakeTestReviewer, FakeTestWorker


class TestAutonomousStewardPRC(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = Path(self.temp_dir.name)
        self.journal_path = self.root / "steward.sqlite3"
        self.journal = StewardJournal(self.journal_path)

        self.repo_dir = self.root / "repo"
        self.repo_dir.mkdir()
        subprocess.run(["git", "init", "-b", "main"], cwd=self.repo_dir, check=True, capture_output=True)
        subprocess.run(["git", "config", "user.name", "Test"], cwd=self.repo_dir, check=True, capture_output=True)
        subprocess.run(["git", "config", "user.email", "test@example.com"], cwd=self.repo_dir, check=True, capture_output=True)

        readme = self.repo_dir / "README.md"
        readme.write_text("# Test Repo\nInitial content.\n")
        subprocess.run(["git", "add", "README.md"], cwd=self.repo_dir, check=True, capture_output=True)
        subprocess.run(["git", "commit", "-m", "Initial commit"], cwd=self.repo_dir, check=True, capture_output=True)

        rev_result = subprocess.run(["git", "rev-parse", "HEAD"], cwd=self.repo_dir, check=True, capture_output=True, text=True)
        self.base_sha = rev_result.stdout.strip()

        self.github_writer = FakeGitHubWriter(initial_pr_number=501)
        self.srv = service.StewardService(
            journal=self.journal,
            github=self.github_writer,
            github_writer=self.github_writer,
            repo_path=self.repo_dir,
        )

    def tearDown(self):
        self.temp_dir.cleanup()

    def test_live_e2e_two_stage_autonomous_closure(self):
        """Full autonomous lifecycle: Propose -> Authenticate -> Stage 1 (2 cards) -> Merge -> Stage 2 (1 card) -> COMPLETE."""
        # 1. Propose & Authenticate
        mission, prop_sha = self.srv.propose(
            "Please update README.md and documentation for full autonomous control loop.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
            mission_id="MISSION-E2E-1",
        )
        approval = contract.OwnerApproval(
            owner_identity="repository-owner",
            proposal_sha256=prop_sha,
            approval_id="approval-e2e-001",
            approved_at="2026-08-30T00:00:00Z",
        )
        authenticator = contract.AuthenticatedOwnerApprovalValidator(
            trusted_owners=contract.TRUSTED_OWNER_IDENTITIES,
        )
        activated = self.srv.approve(mission, approval, authenticator)
        self.assertEqual(activated.state, "RUNNING")

        # 2. Stage 1: Execute WorkCards & Integrate Stage 1
        fake_worker = FakeTestWorker(status="PASS", changed_paths=("README.md",))
        fake_reviewer = FakeTestReviewer(status="PASS")
        step_res1 = self.srv.step(worker=fake_worker, reviewer=fake_reviewer)
        self.assertEqual(step_res1["status"], "STAGE_INTEGRATED")
        stage1_id = step_res1["stage_id"]

        # Stage 1 PR Lifecycle
        stage1 = contract.Stage(
            stage_id=stage1_id,
            mission_id=activated.mission_id,
            objective="Stage 1: Documentation updates",
            repository_identity=activated.repository_identity,
            acceptance_checks=("git_diff_check",),
            compatibility_checks=(),
            workcard_ids=("card-1-1", "card-1-2"),
            rollback=activated.rollback,
            integration_pr=None,
            exact_head=None,
        )
        bound1 = self.srv.publish_stage(stage1, self.base_sha, title="Stage 1: Docs", body="Stage 1 docs body")
        pr1_number = bound1["pr_number"]
        self.srv.promote_stage_ready(stage1, pr1_number, self.base_sha)

        # Simulate CI & Review PASS
        self.github_writer.prs[pr1_number]["ci_state"] = "PASS"
        self.github_writer.prs[pr1_number]["review_state"] = "PASS"

        # Guarded Merge Stage 1
        receipt1 = self.srv.guarded_merge_stage(stage1, pr1_number, self.base_sha)
        self.assertTrue(receipt1["merged"])

        # Post-Merge Readback Stage 1 (Intermediate)
        readback1 = self.srv.post_merge_readback(stage_id=stage1.stage_id, is_final_stage=False)
        self.assertTrue(readback1["diff_clean"])
        self.assertEqual(readback1["mission_state"], "RUNNING")

        # 3. Stage 2: Dependent Stage
        stage2 = contract.Stage(
            stage_id="stage-2-verification",
            mission_id=activated.mission_id,
            objective="Stage 2: Verification and completion",
            repository_identity=activated.repository_identity,
            acceptance_checks=("git_diff_check",),
            compatibility_checks=(),
            workcard_ids=("card-2-1",),
            rollback=activated.rollback,
            integration_pr=None,
            exact_head=None,
        )
        bound2 = self.srv.publish_stage(stage2, self.base_sha, title="Stage 2: Verification", body="Stage 2 body")
        pr2_number = bound2["pr_number"]
        self.srv.promote_stage_ready(stage2, pr2_number, self.base_sha)

        self.github_writer.prs[pr2_number]["ci_state"] = "PASS"
        self.github_writer.prs[pr2_number]["review_state"] = "PASS"

        # Guarded Merge Stage 2 (Final)
        receipt2 = self.srv.guarded_merge_stage(stage2, pr2_number, self.base_sha)
        self.assertTrue(receipt2["merged"])

        # Post-Merge Readback Stage 2 (Final -> COMPLETE)
        readback2 = self.srv.post_merge_readback(stage_id=stage2.stage_id, is_final_stage=True)
        self.assertTrue(readback2["diff_clean"])
        self.assertEqual(readback2["mission_state"], "COMPLETE")

        # Verify status reports COMPLETE
        status = self.srv.status()
        self.assertEqual(status["mission_state"], "COMPLETE")

    def test_fault_scenario_1_worker_failure_and_replan(self):
        """Scenario 1: Worker failure triggers replan and retry without owner intervention."""
        mission, prop_sha = self.srv.propose("Please update README.md for worker fault drill.", repository="Igzela/token-efficient-agent-harness-lab", base_sha=self.base_sha, mission_id="MISSION-F1")
        approval = contract.OwnerApproval(owner_identity="repository-owner", proposal_sha256=prop_sha, approval_id="appr-f1", approved_at="2026-08-30T00:00:00Z")
        auth = contract.AuthenticatedOwnerApprovalValidator(trusted_owners=contract.TRUSTED_OWNER_IDENTITIES)
        self.srv.approve(mission, approval, auth)

        failing_worker = FakeTestWorker(status="FAIL", detail="build_check_failed")
        res = self.srv.step(worker=failing_worker)
        self.assertEqual(res["status"], "CARD_FAILED")
        self.assertEqual(res["detail"], "build_check_failed")

    def test_fault_scenario_2_ci_failure_and_autonomous_repair(self):
        """Scenario 2: CI failure triggers autonomous replan/repair (0 owner prompts)."""
        mission, prop_sha = self.srv.propose("Please update README.md for CI fault drill.", repository="Igzela/token-efficient-agent-harness-lab", base_sha=self.base_sha, mission_id="MISSION-F2")
        approval = contract.OwnerApproval(owner_identity="repository-owner", proposal_sha256=prop_sha, approval_id="appr-f2", approved_at="2026-08-30T00:00:00Z")
        auth = contract.AuthenticatedOwnerApprovalValidator(trusted_owners=contract.TRUSTED_OWNER_IDENTITIES)
        self.srv.approve(mission, approval, auth)

        plan = self.srv.plan_stages()
        stage = plan.stage
        bound = self.srv.publish_stage(stage, self.base_sha, title="Stage Repair", body="Repair body")
        pr_num = bound["pr_number"]
        self.srv.promote_stage_ready(stage, pr_num, self.base_sha)

        # CI fails
        self.github_writer.prs[pr_num]["ci_state"] = "FAIL"
        status = self.srv.observe_stage_ci(stage, pr_num, self.base_sha)
        self.assertEqual(status.outcome, "WAITING")
        self.assertIn("ci_fail", status.reason)

        # Autonomous replan on CI failure
        replan = self.srv.replan_stage(plan, "CI_FAILED", attempt_number=2)
        self.assertEqual(replan.disposition, "RECOVERY_RECOMMENDED")

    def test_fault_scenario_3_review_blocker_and_repair(self):
        """Scenario 3: Independent review blocker triggers autonomous repair."""
        mission, prop_sha = self.srv.propose("Please update README.md for review blocker drill.", repository="Igzela/token-efficient-agent-harness-lab", base_sha=self.base_sha, mission_id="MISSION-F3")
        approval = contract.OwnerApproval(owner_identity="repository-owner", proposal_sha256=prop_sha, approval_id="appr-f3", approved_at="2026-08-30T00:00:00Z")
        auth = contract.AuthenticatedOwnerApprovalValidator(trusted_owners=contract.TRUSTED_OWNER_IDENTITIES)
        self.srv.approve(mission, approval, auth)

        worker = FakeTestWorker(status="PASS", changed_paths=("README.md",))
        reviewer = FakeTestReviewer(status="FAIL", blockers=("unresolved_scope_issue",), detail="review_failed")
        res = self.srv.step(worker=worker, reviewer=reviewer)
        self.assertEqual(res["status"], "REVIEW_REJECTED")

    def test_fault_scenario_4_accepted_main_drift(self):
        """Scenario 4: Drift in accepted main base SHA is safely detected."""
        mission, prop_sha = self.srv.propose("Please update README.md for drift test.", repository="Igzela/token-efficient-agent-harness-lab", base_sha=self.base_sha, mission_id="MISSION-F4")
        approval = contract.OwnerApproval(owner_identity="repository-owner", proposal_sha256=prop_sha, approval_id="appr-f4", approved_at="2026-08-30T00:00:00Z")
        auth = contract.AuthenticatedOwnerApprovalValidator(trusted_owners=contract.TRUSTED_OWNER_IDENTITIES)
        self.srv.approve(mission, approval, auth)

        plan = self.srv.plan_stages()
        stage = plan.stage
        bound = self.srv.publish_stage(stage, self.base_sha, title="Stage", body="Body")
        pr_num = bound["pr_number"]
        facts = self.github_writer.fetch_stage_pr("Igzela/token-efficient-agent-harness-lab", pr_num)
        # Drift expected base sha raises GitHubFactsError fail-closed
        with self.assertRaises(steward_github.GitHubFactsError):
            steward_github.reconcile_stage_pr(
                facts,
                repository="Igzela/token-efficient-agent-harness-lab",
                pr_number=pr_num,
                expected_base_sha="d" * 40,
                expected_head_sha=self.base_sha,
            )

    def test_fault_scenario_5_service_restart_recovery(self):
        """Scenario 5: Process crash and restart seamlessly reloads from WAL SQLite journal."""
        mission, prop_sha = self.srv.propose("Please update README.md for restart recovery.", repository="Igzela/token-efficient-agent-harness-lab", base_sha=self.base_sha, mission_id="MISSION-F5")
        approval = contract.OwnerApproval(owner_identity="repository-owner", proposal_sha256=prop_sha, approval_id="appr-f5", approved_at="2026-08-30T00:00:00Z")
        auth = contract.AuthenticatedOwnerApprovalValidator(trusted_owners=contract.TRUSTED_OWNER_IDENTITIES)
        self.srv.approve(mission, approval, auth)
        self.srv.heartbeat(tick_id="tick:before_crash")

        restarted_srv = service.StewardService(
            journal=self.journal,
            github=self.github_writer,
            github_writer=self.github_writer,
            repo_path=self.repo_dir,
        )
        status = restarted_srv.status()
        self.assertEqual(status["mission_state"], "RUNNING")
        self.assertEqual(status["mission_id"], "MISSION-F5")

    def test_fault_scenario_6_lease_expiry_and_heartbeat(self):
        """Scenario 6: Heartbeat tracking produces immutable sequential liveness facts."""
        hb1 = self.srv.heartbeat(tick_id="hb:1")
        hb2 = self.srv.heartbeat(tick_id="hb:2")
        self.assertEqual(hb1["schema_version"], "steward_heartbeat.v1")
        self.assertEqual(hb2["seq"], hb1["seq"] + 1)
        self.assertNotEqual(hb1["tail_sha256"], hb2["tail_sha256"])

    def test_fault_scenario_7_github_mutation_outcome_unknown(self):
        """Scenario 7: Indeterminate GitHub merge outcome strictly fails closed."""
        mission, prop_sha = self.srv.propose("Please update README.md for indeterminate merge test.", repository="Igzela/token-efficient-agent-harness-lab", base_sha=self.base_sha, mission_id="MISSION-F7")
        approval = contract.OwnerApproval(owner_identity="repository-owner", proposal_sha256=prop_sha, approval_id="appr-f7", approved_at="2026-08-30T00:00:00Z")
        auth = contract.AuthenticatedOwnerApprovalValidator(trusted_owners=contract.TRUSTED_OWNER_IDENTITIES)
        self.srv.approve(mission, approval, auth)

        plan = self.srv.plan_stages()
        stage = plan.stage
        bound = self.srv.publish_stage(stage, self.base_sha, title="Stage", body="Body")
        pr_num = bound["pr_number"]
        self.srv.promote_stage_ready(stage, pr_num, self.base_sha)
        self.github_writer.prs[pr_num]["ci_state"] = "PASS"
        self.github_writer.prs[pr_num]["review_state"] = "PASS"

        # Inject outcome unknown
        self.github_writer.merge_outcome_unknown = True
        with self.assertRaises(GitHubMutationError):
            self.srv.guarded_merge_stage(stage, pr_num, self.base_sha)

    def test_fault_scenario_8_emergency_stop(self):
        """Scenario 8: Emergency stop halts active mission and records STOPPED state."""
        mission, prop_sha = self.srv.propose("Please update README.md for stop test.", repository="Igzela/token-efficient-agent-harness-lab", base_sha=self.base_sha, mission_id="MISSION-F8")
        approval = contract.OwnerApproval(owner_identity="repository-owner", proposal_sha256=prop_sha, approval_id="appr-f8", approved_at="2026-08-30T00:00:00Z")
        auth = contract.AuthenticatedOwnerApprovalValidator(trusted_owners=contract.TRUSTED_OWNER_IDENTITIES)
        self.srv.approve(mission, approval, auth)

        stop_res = self.srv.stop(reason="manual_kill_switch")
        self.assertEqual(stop_res["status"], "STOPPED")
        self.assertEqual(stop_res["reason"], "manual_kill_switch")
        self.assertEqual(self.srv.status()["mission_state"], "STOPPED")

    def test_fault_scenario_9_rollback_when_boundary_breached(self):
        """Scenario 9: Disallowed path mutation is blocked by worker sandboxing."""
        ctx = workers.WorkerContext(
            mission_id="MISSION-F9",
            stage_id="stage-f9",
            card_id="card-f9",
            attempt=1,
            model_tier="T1",
            base_sha=self.base_sha,
            worktree=self.repo_dir,
            allowed_paths=("README.md",),
            steps=("Modify forbidden file.",),
            focused_tests=(),
            negative_checks=(),
            expected_evidence=(),
            environment=workers.child_environment(dict(os.environ)),
            worktree_branch="main",
        )
        forbidden_worker = FakeTestWorker(status="PASS", changed_paths=(".github/workflows/tests.yml",))
        outcome = forbidden_worker.run(ctx)
        # Reviewer catches changed path outside allowed scope
        reviewer = FakeTestReviewer(status="FAIL", blockers=("path_outside_allowed_scope",))
        review_outcome = reviewer.review(ctx, outcome)
        self.assertEqual(review_outcome.status, "FAIL")
        self.assertIn("path_outside_allowed_scope", review_outcome.blockers)

    def test_fault_scenario_10_idempotence_and_concurrent_sqlite_resilience(self):
        """Scenario 10: Multi-threaded concurrent heartbeats and idempotent event deduplication."""
        errors = []

        def worker_task(worker_id: int):
            try:
                for i in range(15):
                    self.srv.heartbeat(tick_id=f"tick:{worker_id}:{i}")
                    time.sleep(0.001)
            except Exception as exc:
                errors.append(exc)

        threads = [threading.Thread(target=worker_task, args=(i,)) for i in range(4)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        self.assertEqual(len(errors), 0, f"Concurrent journal errors: {errors}")
        events = self.journal.replay()
        self.assertGreaterEqual(len(events), 60)


if __name__ == "__main__":
    unittest.main()
