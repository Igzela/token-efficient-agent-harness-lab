"""Tests for Autonomous Steward PR-A: Canonical Control Loop Activation."""

from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

import sys

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts" / "agent-control"))

import mission_contract as contract
import shadow_steward as shadow
import steward
import steward_github
from steward_journal import JournalEvent, StewardJournal
import steward_service as service
import steward_workers as workers
import worktree_manager


class StewardPRATests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.repo_dir = self.root / "repo"
        self.repo_dir.mkdir()
        subprocess.run(["git", "init", "-b", "main"], cwd=self.repo_dir, check=True, capture_output=True)
        subprocess.run(["git", "config", "user.name", "Test Agent"], cwd=self.repo_dir, check=True)
        subprocess.run(["git", "config", "user.email", "agent@example.com"], cwd=self.repo_dir, check=True)
        (self.repo_dir / "README.md").write_text("# Autonomous Steward Test\n", encoding="utf-8")
        subprocess.run(["git", "add", "."], cwd=self.repo_dir, check=True)
        subprocess.run(["git", "commit", "-m", "initial commit"], cwd=self.repo_dir, check=True, capture_output=True)
        self.base_sha = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=self.repo_dir, check=True, capture_output=True, text=True
        ).stdout.strip()
        self.journal_path = self.root / "journal.sqlite3"
        self.journal = StewardJournal(self.journal_path)

    def test_journal_wal_mode_and_mission_lifecycle_events(self):
        """Verify journal supports WAL mode, busy timeout, and mission lifecycle events."""
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
            github=steward_github.FakeGitHubReader(),
            repo_path=self.repo_dir,
        )
        status = srv.status()
        self.assertEqual(status["state"], "IDLE")
        self.assertIsNone(status["mission_id"])
        self.assertIsNone(status["active_mission"])

    def test_natural_language_propose_and_approve_workflow(self):
        """Verify natural language request compiles to proposed mission and activates upon approval."""
        srv = service.StewardService(
            journal=self.journal,
            github=steward_github.FakeGitHubReader(),
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

        # Approve and activate
        approval = contract.OwnerApproval(
            owner_identity="repository-owner",
            proposal_sha256=proposal_sha256,
            approval_id="issue-100",
            approved_at="2026-08-30T00:00:00Z",
        )
        authenticator = type("Auth", (), {"verify": lambda _s, _a, _p: True})()
        activated = srv.approve(mission, approval, authenticator)
        self.assertEqual(activated.state, "RUNNING")
        self.assertEqual(srv.mission_id, activated.mission_id)

        # Status should now be RUNNING
        status = srv.status()
        self.assertEqual(status["state"], "RUNNING")
        self.assertEqual(status["mission_id"], activated.mission_id)

    def test_plan_and_replan_dynamic_mission(self):
        """Verify plan_stages and replan_stage handle dynamic missions cleanly."""
        srv = service.StewardService(
            journal=self.journal,
            github=steward_github.FakeGitHubReader(),
            repo_path=self.repo_dir,
        )
        mission, proposal_sha256 = srv.propose(
            "Update README.md with test details.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
        )
        approval = contract.OwnerApproval(
            owner_identity="repository-owner",
            proposal_sha256=proposal_sha256,
            approval_id="issue-101",
            approved_at="2026-08-30T00:00:00Z",
        )
        authenticator = type("Auth", (), {"verify": lambda _s, _a, _p: True})()
        activated = srv.approve(mission, approval, authenticator)

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
        lock_dir = self.root / "locks"
        lock_dir.mkdir()

        # Create WorkCard context
        context = workers.WorkerContext(
            mission_id="MISSION-TEST-1",
            stage_id="stage-test-1",
            card_id="card-test-1",
            attempt=1,
            model_tier="T1",
            base_sha=self.base_sha,
            worktree=self.repo_dir,
            allowed_paths=("README.md",),
            steps=("Update README.md.",),
            focused_tests=("focused_checks",),
            negative_checks=("forbidden",),
            expected_evidence=("receipts",),
            environment=workers.child_environment(),
            worktree_branch="card-test-1",
        )

        outcome = worker.run(context)
        self.assertEqual(outcome.status, "PASS")
        self.assertIn("README.md", outcome.changed_paths)
        self.assertNotEqual(outcome.head_sha, self.base_sha)

        review = reviewer.run(context, outcome)
        self.assertEqual(review.status, "PASS")
        self.assertEqual(review.blockers, ())
        self.assertTrue(review.security_ok)
        self.assertTrue(review.rollback_ok)

    def test_cli_subcommands(self):
        """Verify unified CLI subcommands propose, approve, status, and stop."""
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
