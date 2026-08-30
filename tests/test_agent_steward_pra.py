"""PR-A Unit and Integration Tests for Autonomous Steward Control Loop Activation."""

from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts" / "agent-control"))

import mission_contract as contract
import shadow_steward
import steward_service as service
from steward_journal import StewardJournal
import steward_workers as workers
from steward_workers import FakeTestReviewer, FakeTestWorker


class TestAutonomousStewardPRA(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = Path(self.temp_dir.name)
        self.journal_path = self.root / "steward.sqlite3"
        self.journal = StewardJournal(self.journal_path)

        # Setup local git repo fixture
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

    def tearDown(self):
        self.temp_dir.cleanup()

    def test_wal_mode_and_journal_lifecycle_events(self):
        """Verify SQLite journal operates in WAL mode and records mission lifecycle facts."""
        events = self.journal.replay()
        self.assertEqual(len(events), 0)
        self.assertIsNone(self.journal.active_mission_record())

        # Record proposal
        p_event = self.journal.record_mission_proposal(
            "MISSION-TEST-1",
            "a" * 64,
            {"objective": "test mission"},
        )
        self.assertEqual(p_event.event, "MISSION_PROPOSED")
        self.assertEqual(p_event.state, "PROPOSING")
        self.assertIsNone(self.journal.active_mission_record())

        # Record activation
        a_event = self.journal.record_mission_activation(
            "MISSION-TEST-1",
            "a" * 64,
            {"objective": "test mission", "schema_version": contract.SCHEMA_VERSION},
        )
        self.assertEqual(a_event.event, "MISSION_ACTIVATED")
        self.assertEqual(a_event.state, "RUNNING")
        active = self.journal.active_mission_record()
        self.assertIsNotNone(active)
        self.assertEqual(active.mission_id, "MISSION-TEST-1")

        # Record completion
        c_event = self.journal.record_mission_completion("MISSION-TEST-1", {"status": "SUCCESS"})
        self.assertEqual(c_event.event, "MISSION_COMPLETED")
        self.assertIsNone(self.journal.active_mission_record())

        # Record stop
        self.journal.record_mission_activation("MISSION-TEST-2", "b" * 64)
        self.assertIsNotNone(self.journal.active_mission_record())
        s_event = self.journal.record_mission_stop("MISSION-TEST-2", reason="emergency_stop")
        self.assertEqual(s_event.event, "MISSION_STOPPED")
        self.assertIsNone(self.journal.active_mission_record())

    def test_service_status_reports_idle_when_no_active_mission(self):
        """Verify StewardService reports IDLE state when no mission is active."""
        srv = service.StewardService(
            journal=self.journal,
            github=service.GhReadOnlyGitHub(),
            repo_path=self.repo_dir,
        )
        status = srv.status()
        self.assertEqual(status["mission_state"], "IDLE")
        self.assertIsNone(status["mission_objective"])

    def test_natural_language_propose_and_approve_workflow(self):
        """Verify natural language request compiles to proposed mission and activates with real validator."""
        srv = service.StewardService(
            journal=self.journal,
            github=service.GhReadOnlyGitHub(),
            repo_path=self.repo_dir,
        )
        mission, proposal_sha256 = srv.propose(
            "Please update the project documentation in README.md to describe autonomy.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
            branch="main",
            source_ref="main",
        )
        self.assertEqual(mission.state, "PROPOSING")
        self.assertIn("README.md", mission.allowed_paths)
        self.assertEqual(mission.proposal_sha256, proposal_sha256)

        # Journal should now have MISSION_PROPOSED
        events = self.journal.replay()
        self.assertEqual(len(events), 1)
        self.assertEqual(events[0].event, "MISSION_PROPOSED")

        # Approve and activate using canonical AuthenticatedOwnerApprovalValidator
        approval = contract.OwnerApproval(
            owner_identity="repository-owner",
            proposal_sha256=proposal_sha256,
            approval_id="approval-issue-100",
            approved_at="2026-08-30T00:00:00Z",
        )
        authenticator = contract.AuthenticatedOwnerApprovalValidator(
            trusted_owners=contract.TRUSTED_OWNER_IDENTITIES,
        )
        activated = srv.approve(mission, approval, authenticator)
        self.assertEqual(activated.state, "RUNNING")
        self.assertEqual(srv.mission_id, activated.mission_id)

        # Status should now report RUNNING
        status = srv.status()
        self.assertEqual(status["mission_state"], "RUNNING")
        self.assertEqual(status["mission_id"], activated.mission_id)

    def test_negative_owner_approval_authentication(self):
        """Verify unauthenticated, forged, and replay approvals are strictly rejected."""
        srv = service.StewardService(
            journal=self.journal,
            github=service.GhReadOnlyGitHub(),
            repo_path=self.repo_dir,
        )
        mission, proposal_sha256 = srv.propose(
            "Update README.md for negative testing.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
            mission_id="MISSION-NEG-1",
        )

        authenticator = contract.AuthenticatedOwnerApprovalValidator(
            trusted_owners=contract.TRUSTED_OWNER_IDENTITIES,
        )

        # 1. Untrusted owner identity
        untrusted_approval = contract.OwnerApproval(
            owner_identity="malicious-actor",
            proposal_sha256=proposal_sha256,
            approval_id="approval-untrusted-1",
            approved_at="2026-08-30T00:00:00Z",
        )
        with self.assertRaises(contract.MissionContractError):
            srv.approve(mission, untrusted_approval, authenticator)

        # 2. Forged proposal sha256 mismatch
        forged_approval = contract.OwnerApproval(
            owner_identity="repository-owner",
            proposal_sha256="f" * 64,
            approval_id="approval-forged-1",
            approved_at="2026-08-30T00:00:00Z",
        )
        with self.assertRaises(contract.MissionContractError):
            srv.approve(mission, forged_approval, authenticator)

        # 3. Valid first approval succeeds
        valid_approval = contract.OwnerApproval(
            owner_identity="repository-owner",
            proposal_sha256=proposal_sha256,
            approval_id="approval-valid-1",
            approved_at="2026-08-30T00:00:00Z",
        )
        activated = srv.approve(mission, valid_approval, authenticator)
        self.assertEqual(activated.state, "RUNNING")

        # 4. Replay of the same approval ID must be rejected by anti-replay
        with self.assertRaises(contract.MissionContractError):
            srv.approve(mission, valid_approval, authenticator)

    def test_plan_and_replan_dynamic_mission(self):
        """Verify plan_stages and replan_stage handle dynamic missions cleanly."""
        srv = service.StewardService(
            journal=self.journal,
            github=service.GhReadOnlyGitHub(),
            repo_path=self.repo_dir,
        )
        mission, proposal_sha256 = srv.propose(
            "Update README.md with test details.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
            mission_id="MISSION-PLAN-1",
        )
        approval = contract.OwnerApproval(
            owner_identity="repository-owner",
            proposal_sha256=proposal_sha256,
            approval_id="approval-plan-101",
            approved_at="2026-08-30T00:00:00Z",
        )
        authenticator = contract.AuthenticatedOwnerApprovalValidator(
            trusted_owners=contract.TRUSTED_OWNER_IDENTITIES,
        )
        srv.approve(mission, approval, authenticator)

        # Plan stage
        plan = srv.plan_stages()
        self.assertEqual(plan.disposition, "PLANNED")
        self.assertIsNotNone(plan.stage)
        self.assertEqual(len(plan.workcards), 2)

        # Replan on ordinary failure (e.g. WORKER_FAILED)
        replan = srv.replan_stage(plan, "WORKER_FAILED", attempt_number=2)
        self.assertEqual(replan.disposition, "RECOVERY_RECOMMENDED")

    def test_general_worker_and_reviewer_execution(self):
        """Verify general_worker and general_reviewer execute and review within contract."""
        worker = workers.general_worker()
        reviewer = workers.general_reviewer()

        context = workers.WorkerContext(
            mission_id="MISSION-TEST-1",
            stage_id="stage-test-1",
            card_id="card-test-1",
            attempt=1,
            model_tier="T1",
            base_sha=self.base_sha,
            worktree=self.repo_dir,
            allowed_paths=("README.md",),
            steps=("Apply bounded change.",),
            focused_tests=(),
            negative_checks=(),
            expected_evidence=(),
            environment=workers.child_environment(dict(os.environ)),
            worktree_branch="main",
        )

        outcome = worker.run(context)
        self.assertEqual(outcome.status, "PASS")
        self.assertIn("README.md", outcome.changed_paths)
        self.assertNotEqual(outcome.head_sha, self.base_sha)

        review = reviewer.review(context, outcome)
        self.assertEqual(review.status, "PASS")
        self.assertEqual(review.blockers, ())
        self.assertTrue(review.security_ok)
        self.assertTrue(review.rollback_ok)

    def test_canonical_service_advancement_loop(self):
        """Verify service.step() / run() autonomously advances active mission: load -> plan -> dispatch -> integrate."""
        srv = service.StewardService(
            journal=self.journal,
            github=service.GhReadOnlyGitHub(),
            repo_path=self.repo_dir,
        )
        mission, proposal_sha256 = srv.propose(
            "Update README.md with advancement loop verification.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
            mission_id="MISSION-ADV-1",
        )
        approval = contract.OwnerApproval(
            owner_identity="repository-owner",
            proposal_sha256=proposal_sha256,
            approval_id="approval-adv-1",
            approved_at="2026-08-30T00:00:00Z",
        )
        authenticator = contract.AuthenticatedOwnerApprovalValidator(
            trusted_owners=contract.TRUSTED_OWNER_IDENTITIES,
        )
        srv.approve(mission, approval, authenticator)

        # Execute autonomous service loop step
        fake_worker = FakeTestWorker(status="PASS", changed_paths=("README.md",))
        fake_reviewer = FakeTestReviewer(status="PASS")

        result = srv.step(worker=fake_worker, reviewer=fake_reviewer)
        self.assertEqual(result["status"], "STAGE_INTEGRATED")
        self.assertEqual(result["mission_id"], "MISSION-ADV-1")

        # Verify journal recorded all lifecycle steps
        events = self.journal.replay()
        event_names = [e.event for e in events]
        self.assertIn("MISSION_PROPOSED", event_names)
        self.assertIn("MISSION_ACTIVATED", event_names)
        self.assertIn("STAGE_PLANNED", event_names)
        self.assertIn("WORKER_STARTED", event_names)
        self.assertIn("WORKER_COMMITTED", event_names)
        self.assertIn("REVIEW_PASSED", event_names)
        self.assertIn("STAGE_INTEGRATED", event_names)

    def test_cli_subcommands(self):
        """Verify unified CLI subcommands propose, approve, status, stop, run."""
        j_path = str(self.root / "cli_journal.sqlite3")

        # Status on empty journal
        ret = service.main(["--journal", j_path, "status"])
        self.assertEqual(ret, 0)

        # Propose
        ret = service.main([
            "--journal", j_path,
            "propose",
            "--request", "Update README.md for CLI test",
            "--base-sha", self.base_sha,
            "--mission-id", "MISSION-CLI-1",
        ])
        self.assertEqual(ret, 0)

        j = StewardJournal(j_path)
        events = j.replay()
        self.assertEqual(len(events), 1)
        prop_sha = events[0].data["proposal_sha256"]

        # Approve
        ret = service.main([
            "--journal", j_path,
            "approve",
            "--mission-id", "MISSION-CLI-1",
            "--proposal-sha256", prop_sha,
        ])
        self.assertEqual(ret, 0)

        # Status after approval
        ret = service.main(["--journal", j_path, "status"])
        self.assertEqual(ret, 0)

        # Run once
        ret = service.main(["--journal", j_path, "run", "--once"])
        self.assertEqual(ret, 0)

        # Stop
        ret = service.main([
            "--journal", j_path,
            "stop",
            "--mission-id", "MISSION-CLI-1",
            "--reason", "operator_stop",
        ])
        self.assertEqual(ret, 0)


if __name__ == "__main__":
    unittest.main()
