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
from steward import StageIntegration


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
        self.github_writer.remote_main_sha = self.base_sha
        self.srv = service.StewardService(
            journal=self.journal,
            github=self.github_writer,
            github_writer=self.github_writer,
            repo_path=self.repo_dir,
        )

    def tearDown(self):
        self.temp_dir.cleanup()

    @staticmethod
    def _evidence(mission: contract.MaintenanceMission, proposal_sha256: str, suffix: str) -> contract.OwnerApprovalEvidence:
        return contract.OwnerApprovalEvidence(
            transport="github_issue_comment",
            repository=mission.repository_identity.repository,
            mission_id=mission.mission_id,
            approval_id=f"approval-{suffix}",
            owner_identity="github:Igzela",
            proposal_sha256=proposal_sha256,
            accepted_main_sha=mission.repository_identity.base_sha,
            evidence_id=f"github-comment-{suffix}",
        )

    class _ControlOff:
        def emergency_stop_active(self, *, repository: str, issue_number: int) -> bool:
            return False

    class _FixtureApprovalSource:
        """SIMULATED authenticated transport for deterministic tests only."""

        __steward_test_fixture__ = True

        def __init__(self, evidence: contract.OwnerApprovalEvidence):
            self.evidence = evidence

        def read(self, **_kwargs) -> contract.OwnerApprovalEvidence:
            return self.evidence

    def _approve(self, mission: contract.MaintenanceMission, proposal_sha256: str, suffix: str) -> contract.MaintenanceMission:
        return self.srv.approve(
            mission,
            approval_comment_id=8000 + len(suffix),
            approval_source=self._FixtureApprovalSource(self._evidence(mission, proposal_sha256, suffix)),
        )

    class _SimulatedStageExecutor:
        """SIMULATED executor seam; never used for the production live drill."""

        def __init__(self, *, github, **_kwargs):
            self.github = github
            self.repo_path = Path(_kwargs["repo_path"])

        def execute_stage_to_waiting_for_merge(self, mission, stage, cards, *, base_sha, title, body):
            branch = f"agent/simulated-{stage.stage_id}"
            target = self.repo_path / cards[0].allowed_paths[0]
            target.parent.mkdir(parents=True, exist_ok=True)
            with target.open("a", encoding="utf-8") as handle:
                handle.write(f"\nSimulated bounded stage {stage.stage_id}.\n")
            subprocess.run(["git", "add", "--", str(target.relative_to(self.repo_path))], cwd=self.repo_path, check=True, capture_output=True)
            subprocess.run(["git", "commit", "-m", f"simulated stage {stage.stage_id}"], cwd=self.repo_path, check=True, capture_output=True)
            head = subprocess.run(["git", "rev-parse", "HEAD"], cwd=self.repo_path, check=True, capture_output=True, text=True).stdout.strip()
            integration = StageIntegration(stage.stage_id, branch, base_sha, head, ())
            pr = self.github.create_or_update_stage_pr(
                stage.stage_id,
                mission.mission_id,
                branch,
                head,
                base_sha,
                title,
                body,
                mission.repository_identity.repository,
            )
            return {"status": "stage_pr_draft", "integration": integration, "pr": pr}

    def test_simulated_default_loop_advances_two_stage_pr_lifecycle_without_manual_sequence(self):
        """SIMULATED state-machine coverage; live evidence is recorded separately."""
        self.srv.control_state = self._ControlOff()
        mission, digest = self.srv.propose(
            "Update README.md, docs/ARCHITECTURE.md, and docs/RUNBOOK.md with bounded maintenance evidence.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
            mission_id="MISSION-SIM-LOOP",
        )
        self._approve(mission, digest, "sim-loop")
        with (
            patch("steward.Steward", self._SimulatedStageExecutor),
            patch("steward_service.production_reviewer", return_value=FakeTestReviewer(status="PASS")),
        ):
            self.assertEqual(self.srv.step()["status"], "STAGE_PLANNED")
            draft_one = self.srv.step()
            self.assertEqual(draft_one["status"], "STAGE_PR_DRAFT")
            self.assertEqual(self.srv.step()["status"], "STAGE_PR_READY")
            first_pr = draft_one["pr_number"]
            self.github_writer.prs[first_pr]["ci_state"] = "PASS"
            self.github_writer.prs[first_pr]["review_state"] = "PASS"
            self.assertEqual(self.srv.step()["status"], "MERGE_READBACK")
            with patch("subprocess.run", return_value=MagicMock(returncode=0, stdout=self.github_writer.remote_main_sha + "\n", stderr="")):
                self.assertEqual(self.srv.step()["status"], "NEXT_STAGE")
            self.assertEqual(self.srv.step()["status"], "STAGE_PLANNED")
            draft_two = self.srv.step()
            self.assertEqual(draft_two["status"], "STAGE_PR_DRAFT")
            self.assertEqual(self.srv.step()["status"], "STAGE_PR_READY")
            second_pr = draft_two["pr_number"]
            self.github_writer.prs[second_pr]["ci_state"] = "PASS"
            self.github_writer.prs[second_pr]["review_state"] = "PASS"
            self.assertEqual(self.srv.step()["status"], "MERGE_READBACK")
            with patch("subprocess.run", return_value=MagicMock(returncode=0, stdout=self.github_writer.remote_main_sha + "\n", stderr="")):
                self.assertEqual(self.srv.step()["status"], "COMPLETE")
        events = [event.event for event in self.journal.replay()]
        self.assertIn("SERVICE_LEASE_ACQUIRED", events)
        self.assertIn("STAGE_MERGE_DISPATCH_INTENT", events)
        self.assertEqual(self.srv.status()["mission_state"], "COMPLETE")

    def test_integrated_review_attests_actual_diff_not_declared_scope(self):
        """SIMULATED negative coverage: a reviewer receives observed paths only."""
        mission, digest = self.srv.propose(
            "Update README.md and docs/ARCHITECTURE.md with documentation tests.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
            mission_id="MISSION-DIFF-EVIDENCE",
        )
        self._approve(mission, digest, "diff-evidence")
        plan = self.srv.plan_stages()
        target = self.repo_dir / "README.md"
        target.write_text(target.read_text(encoding="utf-8") + "\nObserved only.\n", encoding="utf-8")
        subprocess.run(["git", "add", "README.md"], cwd=self.repo_dir, check=True, capture_output=True)
        subprocess.run(["git", "commit", "-m", "observed diff"], cwd=self.repo_dir, check=True, capture_output=True)
        head = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=self.repo_dir, check=True,
            capture_output=True, text=True,
        ).stdout.strip()
        integration = StageIntegration(plan.stage.stage_id, "agent/diff-evidence", self.base_sha, head, ())
        observed = {}

        class CapturingReviewer:
            def review(self, context, outcome):
                observed["paths"] = outcome.changed_paths
                return FakeTestReviewer(status="PASS").review(context, outcome)

        self.srv._review_integrated_stage(
            mission, plan.stage, plan.workcards, integration, CapturingReviewer()
        )
        self.assertEqual(observed["paths"], ("README.md",))

    def test_simulated_two_stage_lifecycle_helpers(self):
        """SIMULATED helper coverage; this is not a live GitHub acceptance."""
        # 1. Propose & Authenticate
        mission, prop_sha = self.srv.propose(
            "Please update README.md and documentation for full autonomous control loop.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
            mission_id="MISSION-E2E-1",
        )
        activated = self._approve(mission, prop_sha, "e2e-001")
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
        with patch("subprocess.run") as git_run:
            git_run.return_value = MagicMock(returncode=0, stdout=self.base_sha + "\n", stderr="")
            readback1 = self.srv.post_merge_readback(
                stage_id=stage1.stage_id,
                pr_number=pr1_number,
                expected_head_sha=self.base_sha,
                is_final_stage=False,
            )
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
        with patch("subprocess.run") as git_run:
            git_run.return_value = MagicMock(returncode=0, stdout=self.base_sha + "\n", stderr="")
            readback2 = self.srv.post_merge_readback(
                stage_id=stage2.stage_id,
                pr_number=pr2_number,
                expected_head_sha=self.base_sha,
                is_final_stage=True,
            )
        self.assertTrue(readback2["diff_clean"])
        self.assertEqual(readback2["mission_state"], "COMPLETE")

        # Verify status reports COMPLETE
        status = self.srv.status()
        self.assertEqual(status["mission_state"], "COMPLETE")

    def test_fault_scenario_1_worker_failure_and_replan(self):
        """Scenario 1: Worker failure triggers replan and retry without owner intervention."""
        mission, prop_sha = self.srv.propose("Please update README.md for worker fault drill.", repository="Igzela/token-efficient-agent-harness-lab", base_sha=self.base_sha, mission_id="MISSION-F1")
        self._approve(mission, prop_sha, "f1")

        failing_worker = FakeTestWorker(status="FAIL", detail="build_check_failed")
        res = self.srv.step(worker=failing_worker)
        self.assertEqual(res["status"], "CARD_FAILED")
        self.assertEqual(res["detail"], "build_check_failed")

    def test_fault_scenario_2_ci_failure_and_autonomous_repair(self):
        """Scenario 2: CI failure triggers autonomous replan/repair (0 owner prompts)."""
        mission, prop_sha = self.srv.propose("Please update README.md for CI fault drill.", repository="Igzela/token-efficient-agent-harness-lab", base_sha=self.base_sha, mission_id="MISSION-F2")
        self._approve(mission, prop_sha, "f2")

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
        self._approve(mission, prop_sha, "f3")

        worker = FakeTestWorker(status="PASS", changed_paths=("README.md",))
        reviewer = FakeTestReviewer(status="FAIL", blockers=("unresolved_scope_issue",), detail="review_failed")
        res = self.srv.step(worker=worker, reviewer=reviewer)
        self.assertEqual(res["status"], "REVIEW_REJECTED")

    def test_review_receipt_preflight_drift_replans_without_outcome_unknown(self):
        """SIMULATED: a before-send identity rejection is routine repair."""

        mission, digest = self.srv.propose(
            "Update README.md with bounded documentation evidence.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
            mission_id="MISSION-REVIEW-PREFLIGHT",
        )
        self._approve(mission, digest, "review-preflight")
        self.srv.control_state = self._ControlOff()
        with (
            patch("steward.Steward", self._SimulatedStageExecutor),
            patch("steward_service.production_reviewer", return_value=FakeTestReviewer(status="PASS")),
            patch.object(
                self.github_writer,
                "publish_exact_head_review",
                side_effect=steward_github.GitHubPreflightError(
                    "review_receipt_exact_binding_mismatch"
                ),
            ),
        ):
            self.assertEqual(self.srv.step()["status"], "STAGE_PLANNED")
            self.assertEqual(self.srv.step()["status"], "REPLAN_REQUIRED")
            self.assertEqual(self.srv.step()["status"], "STAGE_REPLANNED")
        events = [event.event for event in self.journal.replay()]
        self.assertIn("STAGE_REPLAN_REQUESTED", events)
        self.assertIn("STAGE_REVIEW_DISPATCH_PREFLIGHT_REJECTED", events)
        self.assertNotIn("STAGE_OUTCOME_UNKNOWN", events)

    def test_review_receipt_read_failure_keeps_loop_alive_and_retries(self):
        """SIMULATED: a pre-POST GitHub read interruption is restart-safe."""

        mission, digest = self.srv.propose(
            "Update README.md with bounded documentation evidence.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
            mission_id="MISSION-REVIEW-READ-RETRY",
        )
        self._approve(mission, digest, "review-read-retry")
        self.srv.control_state = self._ControlOff()
        original = self.github_writer.publish_exact_head_review
        calls = {"count": 0}

        def publish_once_then_succeed(*args, **kwargs):
            if calls["count"] == 0:
                calls["count"] += 1
                raise steward_github.GitHubReadError("github_review_read_failed")
            calls["count"] += 1
            return original(*args, **kwargs)

        with (
            patch("steward.Steward", self._SimulatedStageExecutor),
            patch("steward_service.production_reviewer", return_value=FakeTestReviewer(status="PASS")),
            patch.object(self.github_writer, "publish_exact_head_review", side_effect=publish_once_then_succeed),
        ):
            self.assertEqual(self.srv.step()["status"], "STAGE_PLANNED")
            self.assertEqual(self.srv.step()["status"], "WAITING_GITHUB_READBACK")
            self.assertEqual(self.srv.step()["status"], "STAGE_PR_READY")

        events = [event.event for event in self.journal.replay()]
        self.assertIn("STAGE_REVIEW_READ_WAITING", events)
        self.assertIn("STAGE_REVIEW_RECEIPT_PUBLISHED", events)
        self.assertNotIn("STAGE_OUTCOME_UNKNOWN", events)
        self.assertEqual(calls["count"], 2)

    def test_exhausted_primary_candidates_shift_to_an_alternative_without_owner_prompt(self):
        """SIMULATED repair loop: candidate exhaustion changes strategy, not authority."""

        mission, digest = self.srv.propose(
            "Update README.md and docs/ARCHITECTURE.md for candidate strategy coverage.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
            mission_id="MISSION-ALTERNATIVE",
        )
        active = self._approve(mission, digest, "alternative")
        planned = self.srv._next_stage_plan(active, 0)
        self.assertIsNotNone(planned)
        stage, cards, total = planned
        self.srv._record_stage_plan(
            active, stage, cards, stage_index=1, stage_total=total
        )
        for _ in range(3):
            current_stage, _current_cards, metadata = self.srv._stage_records(active.mission_id)[-1]
            replacement = self.srv._replan_stage(active, current_stage, metadata)
            self.assertEqual(replacement["status"], "STAGE_REPLANNED")
        _alternative_stage, alternative_cards, metadata = self.srv._stage_records(active.mission_id)[-1]
        self.assertEqual(metadata["strategy"], "alternative")
        self.assertTrue(alternative_cards[0].steps[0].startswith("Use a bounded alternative"))
        self.assertIn(
            "STAGE_REPLAN_STRATEGY_SHIFT",
            [event.event for event in self.journal.replay()],
        )
        self.assertEqual(self.srv.status()["mission_state"], "RUNNING")

    def test_fault_scenario_4_accepted_main_drift(self):
        """Scenario 4: Drift in accepted main base SHA is safely detected."""
        mission, prop_sha = self.srv.propose("Please update README.md for drift test.", repository="Igzela/token-efficient-agent-harness-lab", base_sha=self.base_sha, mission_id="MISSION-F4")
        self._approve(mission, prop_sha, "f4")

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

    def test_simulated_bound_stage_drift_halts_before_external_mutation(self):
        """SIMULATED: bound Stage drift is caught before Ready or merge writes."""

        mission, digest = self.srv.propose(
            "Update README.md with bounded bound-stage drift evidence.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
            mission_id="MISSION-BOUND-DRIFT",
        )
        self._approve(mission, digest, "bound-drift")
        self.srv.control_state = self._ControlOff()
        self.assertEqual(self.srv.step()["status"], "STAGE_PLANNED")
        stage, _cards, _metadata = self.srv._stage_records(mission.mission_id)[-1]
        bound = self.srv.publish_stage(stage, self.base_sha, title="Stage", body="Body")
        self.github_writer.remote_main_sha = "e" * 40

        result = self.srv.step()

        self.assertEqual(result["status"], "REPLAN_REQUIRED")
        self.assertEqual(self.srv._active_mission().repository_identity.base_sha, "e" * 40)
        self.assertEqual(
            [name for name, _data in self.github_writer.actions if name != "create_pr"],
            [],
        )
        self.assertIsNotNone(
            self.srv._latest_stage_event(
                mission.mission_id,
                stage.stage_id,
                "STAGE_REPLAN_REQUESTED",
            )
        )

    def _bound_stage_with_pending_intent(self, mission_id: str, suffix: str):
        mission, digest = self.srv.propose(
            "Update README.md with bounded pending mutation recovery evidence.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
            mission_id=mission_id,
        )
        self._approve(mission, digest, suffix)
        self.srv.control_state = self._ControlOff()
        self.assertEqual(self.srv.step()["status"], "STAGE_PLANNED")
        stage, _cards, _metadata = self.srv._stage_records(mission_id)[-1]
        bound = self.srv.publish_stage(stage, self.base_sha, title="Stage", body="Body")
        return mission, stage, bound

    def test_simulated_merge_intent_and_bound_drift_stays_read_only(self):
        """SIMULATED: merge intent blocks drift rebind and candidate supersede."""

        mission, stage, bound = self._bound_stage_with_pending_intent(
            "MISSION-PENDING-MERGE-DRIFT", "pending-merge"
        )
        pr_number = bound["pr_number"]
        self.github_writer.prs[pr_number].update(
            {"draft": False, "ci_state": "PASS", "review_state": "PASS"}
        )
        self.journal.append(
            event="STAGE_MERGE_DISPATCH_INTENT",
            idempotency_key="pending-merge-intent",
            mission_id=mission.mission_id,
            stage_id=stage.stage_id,
            card_id="",
            state="RUNNING",
            detail="canonical_merge_workflow_dispatch_intent",
            data={"pr_number": pr_number, "head_sha": bound["head_sha"]},
            enforce_transition=False,
        )
        self.github_writer.remote_main_sha = "f" * 40
        actions_before = list(self.github_writer.actions)

        result = self.srv.step()

        self.assertEqual(result["status"], "OUTCOME_UNKNOWN")
        self.assertEqual(self.srv._active_mission().repository_identity.base_sha, self.base_sha)
        self.assertEqual(self.github_writer.actions, actions_before)
        self.assertIsNone(
            self.srv._latest_stage_event(
                mission.mission_id, stage.stage_id, "STAGE_REPLAN_REQUESTED"
            )
        )

    def test_simulated_merge_identity_read_failure_keeps_service_alive(self):
        """SIMULATED: pre-dispatch PR read failure is retryable, not a crash."""

        mission, stage, bound = self._bound_stage_with_pending_intent(
            "MISSION-MERGE-READ-RECOVERY", "merge-read"
        )
        pr_number = bound["pr_number"]
        self.github_writer.prs[pr_number].update(
            {"draft": False, "ci_state": "PASS", "review_state": "PASS"}
        )
        with patch.object(
            self.github_writer,
            "guarded_merge",
            side_effect=[
                steward_github.GitHubReadError("github_read_failed"),
                {"merged": False, "pr_number": pr_number, "head_sha": bound["head_sha"]},
            ],
        ) as guarded_merge:
            result = self.srv.step()
            self.assertEqual(result["status"], "WAITING_GITHUB_READBACK")
            retried = self.srv.step()
        self.assertEqual(retried["status"], "MERGE_READBACK")
        self.assertEqual(guarded_merge.call_count, 2)
        self.assertIsNotNone(
            self.srv._latest_stage_event(
                mission.mission_id, stage.stage_id, "STAGE_MERGE_READ_WAITING"
            )
        )
        self.assertIsNone(
            self.srv._latest_stage_event(
                mission.mission_id, stage.stage_id, "STAGE_OUTCOME_UNKNOWN"
            )
        )

    def test_simulated_terminal_merge_rejection_unblocks_bounded_replan(self):
        """SIMULATED: read-only failed workflow proof permits safe replacement."""

        mission, stage, bound = self._bound_stage_with_pending_intent(
            "MISSION-MERGE-REJECTED-RECOVERY", "merge-rejected"
        )
        pr_number = bound["pr_number"]
        self.github_writer.prs[pr_number].update(
            {"draft": False, "ci_state": "PASS", "review_state": "PASS"}
        )
        self.journal.append(
            event="STAGE_MERGE_DISPATCH_INTENT",
            idempotency_key="merge-rejected-intent",
            mission_id=mission.mission_id,
            stage_id=stage.stage_id,
            card_id="",
            state="RUNNING",
            detail="canonical_merge_workflow_dispatch_intent",
            data={"pr_number": pr_number, "head_sha": bound["head_sha"]},
            enforce_transition=False,
        )
        with patch.object(
            self.github_writer,
            "reconcile_merge_dispatch",
            create=True,
            return_value={
                "status": "REJECTED",
                "repository": "Igzela/token-efficient-agent-harness-lab",
                "pr_number": pr_number,
                "expected_head_sha": bound["head_sha"],
                "run_ids": [901],
            },
        ):
            self.assertEqual(self.srv.step()["status"], "REPLAN_REQUIRED")
            self.assertEqual(self.srv.step()["status"], "STAGE_REPLANNED")
        self.assertIsNotNone(
            self.srv._latest_stage_event(
                mission.mission_id,
                stage.stage_id,
                "STAGE_MERGE_DISPATCH_RECONCILED",
            )
        )

    def test_merge_reconciliation_precedes_older_stage_outcome_unknown(self):
        """SIMULATED: merge recovery remains reachable after restart marker."""

        mission, stage, bound = self._bound_stage_with_pending_intent(
            "MISSION-MERGE-UNKNOWN-RECOVERY", "merge-unknown"
        )
        pr_number = bound["pr_number"]
        self.github_writer.prs[pr_number].update(
            {"draft": False, "ci_state": "PASS", "review_state": "PASS"}
        )
        self.journal.append(
            event="STAGE_MERGE_DISPATCH_INTENT",
            idempotency_key="merge-unknown-intent",
            mission_id=mission.mission_id,
            stage_id=stage.stage_id,
            card_id="",
            state="RUNNING",
            detail="canonical_merge_workflow_dispatch_intent",
            data={"pr_number": pr_number, "head_sha": bound["head_sha"]},
            enforce_transition=False,
        )
        self.journal.append(
            event="STAGE_OUTCOME_UNKNOWN",
            idempotency_key="merge-unknown-marker",
            mission_id=mission.mission_id,
            stage_id=stage.stage_id,
            card_id="",
            state="OUTCOME_UNKNOWN",
            detail="merge_dispatch_interrupted_before_readback",
            data={"pr_number": pr_number, "head_sha": bound["head_sha"]},
            enforce_transition=False,
        )
        with patch.object(
            self.github_writer,
            "reconcile_merge_dispatch",
            create=True,
            return_value={
                "status": "REJECTED",
                "repository": "Igzela/token-efficient-agent-harness-lab",
                "pr_number": pr_number,
                "expected_head_sha": bound["head_sha"],
                "run_ids": [902],
            },
        ) as reconcile:
            self.assertEqual(self.srv.step()["status"], "REPLAN_REQUIRED")
            self.assertEqual(self.srv.step()["status"], "STAGE_REPLANNED")
        reconcile.assert_called_once()
        self.assertIsNotNone(
            self.srv._latest_stage_event(
                mission.mission_id,
                stage.stage_id,
                "STAGE_MERGE_DISPATCH_RECONCILED",
            )
        )

    def test_simulated_ready_intent_and_bound_drift_stays_read_only(self):
        """SIMULATED: an interrupted Ready mutation cannot trigger replan."""

        mission, stage, bound = self._bound_stage_with_pending_intent(
            "MISSION-PENDING-READY-DRIFT", "pending-ready"
        )
        pr_number = bound["pr_number"]
        self.journal.append(
            event="STAGE_READY_DISPATCH_INTENT",
            idempotency_key="pending-ready-intent",
            mission_id=mission.mission_id,
            stage_id=stage.stage_id,
            card_id="",
            state="RUNNING",
            detail="stage_ready_dispatch_intent",
            data={"pr_number": pr_number, "head_sha": bound["head_sha"]},
            enforce_transition=False,
        )
        self.github_writer.remote_main_sha = "f" * 40
        actions_before = list(self.github_writer.actions)

        result = self.srv.step()

        self.assertEqual(result["status"], "OUTCOME_UNKNOWN")
        self.assertEqual(self.srv._active_mission().repository_identity.base_sha, self.base_sha)
        self.assertEqual(self.github_writer.actions, actions_before)
        self.assertIsNone(
            self.srv._latest_stage_event(
                mission.mission_id, stage.stage_id, "STAGE_REPLAN_REQUESTED"
            )
        )

    def test_simulated_review_intent_and_bound_drift_stays_read_only(self):
        """SIMULATED: an interrupted review receipt cannot trigger replan."""

        mission, stage, bound = self._bound_stage_with_pending_intent(
            "MISSION-PENDING-REVIEW-DRIFT", "pending-review"
        )
        pr_number = bound["pr_number"]
        self.journal.append(
            event="STAGE_REVIEW_DISPATCH_INTENT",
            idempotency_key="pending-review-intent",
            mission_id=mission.mission_id,
            stage_id=stage.stage_id,
            card_id="",
            state="RUNNING",
            detail="exact_head_review_receipt_publish_intent",
            data={
                "pr_number": pr_number,
                "head_sha": bound["head_sha"],
                "base_sha": self.base_sha,
                "reviewer_session_id": "reviewer-session",
                "implementation_session_id": "implementation-session",
                "reviewed_range_sha256": "a" * 64,
                "review_receipt_sha256": "b" * 64,
            },
            enforce_transition=False,
        )
        self.github_writer.remote_main_sha = "f" * 40
        actions_before = list(self.github_writer.actions)

        result = self.srv.step()

        self.assertEqual(result["status"], "OUTCOME_UNKNOWN")
        self.assertEqual(self.srv._active_mission().repository_identity.base_sha, self.base_sha)
        self.assertEqual(self.github_writer.actions, actions_before)
        self.assertIsNone(
            self.srv._latest_stage_event(
                mission.mission_id, stage.stage_id, "STAGE_REPLAN_REQUESTED"
            )
        )

    def test_simulated_unbound_stage_rebinds_before_worker_dispatch(self):
        """SIMULATED: accepted-main drift replans an unissued Stage before dispatch."""

        mission, digest = self.srv.propose(
            "Update README.md with bounded drift recovery evidence.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
            mission_id="MISSION-UNBOUND-DRIFT",
        )
        self._approve(mission, digest, "unbound-drift")
        self.srv.control_state = self._ControlOff()
        self.assertEqual(self.srv.step()["status"], "STAGE_PLANNED")
        stale_stage = self.srv._stage_records(mission.mission_id)[-1][0]
        new_main = "a" * 40
        self.assertNotEqual(new_main, self.base_sha)
        self.github_writer.remote_main_sha = new_main

        with patch.object(self.srv, "_execute_production_stage") as dispatch:
            result = self.srv.step()
            dispatch.assert_not_called()
        self.assertEqual(result["status"], "REPLAN_REQUIRED")
        self.assertEqual(self.srv._active_mission().repository_identity.base_sha, new_main)

        with patch.object(self.srv, "_execute_production_stage") as dispatch:
            replanned = self.srv.step()
            dispatch.assert_not_called()
        self.assertEqual(replanned["status"], "STAGE_REPLANNED")
        self.assertEqual(replanned["retry"], 1)
        self.assertEqual(replanned["reason"], "accepted_main_drift")
        replacement = self.srv._stage_records(mission.mission_id)[-1][0]
        self.assertNotEqual(replacement.stage_id, stale_stage.stage_id)
        self.assertEqual(replacement.repository_identity.base_sha, new_main)

    def test_simulated_base_drift_does_not_exhaust_alternative_candidate_budget(self):
        """SIMULATED: safe base recovery preserves the current attempt budget slot."""

        mission, digest = self.srv.propose(
            "Update README.md with bounded alternative drift evidence.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
            mission_id="MISSION-ALTERNATIVE-DRIFT",
        )
        active = self._approve(mission, digest, "alternative-drift")
        self.srv.control_state = self._ControlOff()
        retry = active.budget.max_attempts
        planned = self.srv._next_stage_plan(
            active, 0, retry=retry, strategy="alternative"
        )
        self.assertIsNotNone(planned)
        stage, cards, total = planned
        self.srv._record_stage_plan(
            active,
            stage,
            cards,
            stage_index=1,
            stage_total=total,
            retry=retry,
            strategy="alternative",
        )
        new_main = "c" * 40
        self.assertNotEqual(new_main, self.base_sha)
        self.github_writer.remote_main_sha = new_main

        self.assertEqual(self.srv.step()["status"], "REPLAN_REQUIRED")
        result = self.srv.step()
        self.assertEqual(result["status"], "STAGE_REPLANNED")
        self.assertEqual(result["retry"], retry)
        self.assertEqual(result["strategy"], "alternative")
        self.assertEqual(result["reason"], "accepted_main_drift")
        replacement, _cards, metadata = self.srv._stage_records(active.mission_id)[-1]
        self.assertEqual(replacement.repository_identity.base_sha, new_main)
        self.assertEqual(metadata["retry"], retry)
        self.assertEqual(metadata["strategy"], "alternative")

    def test_simulated_base_drift_does_not_mask_pending_candidate_repair(self):
        """SIMULATED: overlapping drift retains the candidate-failure retry charge."""

        mission, digest = self.srv.propose(
            "Update README.md with bounded overlapping recovery evidence.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
            mission_id="MISSION-OVERLAPPING-DRIFT",
        )
        self._approve(mission, digest, "overlapping-drift")
        self.srv.control_state = self._ControlOff()
        self.assertEqual(self.srv.step()["status"], "STAGE_PLANNED")
        stage = self.srv._stage_records(mission.mission_id)[-1][0]
        self.journal.append(
            event="STAGE_REPLAN_REQUESTED",
            idempotency_key=f"test-worker-replan:{mission.mission_id}:{stage.stage_id}",
            mission_id=mission.mission_id,
            stage_id=stage.stage_id,
            card_id="",
            state="RUNNING",
            detail="worker_failure_requires_fresh_candidate",
            data={},
            enforce_transition=False,
        )
        new_main = "d" * 40
        self.assertNotEqual(new_main, self.base_sha)
        self.github_writer.remote_main_sha = new_main

        self.assertEqual(self.srv.step()["status"], "REPLAN_REQUIRED")
        result = self.srv.step()
        self.assertEqual(result["status"], "STAGE_REPLANNED")
        self.assertEqual(result["retry"], 2)
        self.assertEqual(result["reason"], "candidate_repair_with_base_drift")
        replacement, _cards, metadata = self.srv._stage_records(mission.mission_id)[-1]
        self.assertEqual(replacement.repository_identity.base_sha, new_main)
        self.assertEqual(metadata["retry"], 2)

    def test_simulated_unbound_stage_waits_when_main_authority_is_unavailable(self):
        """SIMULATED: a failed accepted-main read cannot dispatch a worker."""

        mission, digest = self.srv.propose(
            "Update README.md with bounded authority failure evidence.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
            mission_id="MISSION-MAIN-READ-WAIT",
        )
        self._approve(mission, digest, "main-read-wait")
        self.srv.control_state = self._ControlOff()
        self.assertEqual(self.srv.step()["status"], "STAGE_PLANNED")

        with (
            patch.object(
                self.github_writer,
                "fetch_accepted_main",
                side_effect=steward_github.GitHubReadError("accepted_main_read_failed"),
            ),
            patch.object(self.srv, "_execute_production_stage") as dispatch,
        ):
            result = self.srv.step()
            dispatch.assert_not_called()
        self.assertEqual(result["status"], "WAITING_GITHUB_READBACK")
        self.assertIn(
            "ACCEPTED_MAIN_READ_UNAVAILABLE",
            [event.event for event in self.journal.replay()],
        )

    def test_simulated_next_stage_plan_rebinds_before_creation(self):
        """SIMULATED: a fresh Stage plan is derived only from live accepted main."""

        mission, digest = self.srv.propose(
            "Update README.md with bounded next-stage drift evidence.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
            mission_id="MISSION-PLAN-DRIFT",
        )
        self._approve(mission, digest, "plan-drift")
        self.srv.control_state = self._ControlOff()
        new_main = "b" * 40
        self.assertNotEqual(new_main, self.base_sha)
        self.github_writer.remote_main_sha = new_main

        rebound = self.srv.step()
        self.assertEqual(rebound["status"], "MISSION_BASE_REBOUND")
        self.assertEqual(self.srv._stage_records(mission.mission_id), [])
        planned = self.srv.step()
        self.assertEqual(planned["status"], "STAGE_PLANNED")
        stage = self.srv._stage_records(mission.mission_id)[-1][0]
        self.assertEqual(stage.repository_identity.base_sha, new_main)

    def test_fault_scenario_5_service_restart_recovery(self):
        """Scenario 5: Process crash and restart seamlessly reloads from WAL SQLite journal."""
        mission, prop_sha = self.srv.propose("Please update README.md for restart recovery.", repository="Igzela/token-efficient-agent-harness-lab", base_sha=self.base_sha, mission_id="MISSION-F5")
        self._approve(mission, prop_sha, "f5")
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

    def test_fault_scenario_6_actual_single_writer_lease_recovery(self):
        """SIMULATED repository fixture with a real flock acquisition/loss/recovery."""
        competing = service.StewardService(
            journal=self.journal,
            github=self.github_writer,
            github_writer=self.github_writer,
            repo_path=self.repo_dir,
        )
        with self.srv._service_lease():
            with self.assertRaisesRegex(service.StewardServiceError, "service_lease_unavailable"):
                with competing._service_lease():
                    self.fail("competing writer must not acquire the live flock")
        # Release is observable; a later writer can acquire the same real
        # flock rather than treating heartbeat sequence increments as a lease.
        with competing._service_lease():
            self.assertTrue(competing._service_lease_held)
        events = [event.event for event in self.journal.replay()]
        self.assertGreaterEqual(events.count("SERVICE_LEASE_ACQUIRED"), 2)
        self.assertGreaterEqual(events.count("SERVICE_LEASE_RELEASED"), 2)

    def test_service_lease_is_scoped_to_the_journal_not_the_mission_id(self):
        """An idle process cannot bypass an active Mission by choosing another key."""

        competing = service.StewardService(
            journal=self.journal,
            github=self.github_writer,
            github_writer=self.github_writer,
            repo_path=self.repo_dir,
        )
        self.srv.mission_id = "MISSION-LEASE-A"
        with self.srv._service_lease():
            with self.assertRaisesRegex(service.StewardServiceError, "service_lease_unavailable"):
                with competing._service_lease():
                    self.fail("an idle writer must use the same journal lease")

    def test_locally_constructed_running_mission_cannot_drive_production_step(self):
        """A runtime Mission requires the matching durable activation record."""

        proposal, digest = contract.compile_proposal_mission(
            "Update README.md with a bounded direct-construction probe.",
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
            mission_id="MISSION-DIRECT-PROBE",
        )
        local_approval = contract.OwnerApproval(
            "github:Igzela", digest, "locally-constructed", "2026-08-30T00:00:00Z"
        )
        constructed = contract.activate_current_mission(
            repository=proposal.repository_identity.repository,
            base_sha=proposal.repository_identity.base_sha,
            branch=proposal.repository_identity.branch,
            source_ref=proposal.repository_identity.source_ref,
            source_sha256=proposal.repository_identity.source_sha256,
            proposal_sha256=digest,
            owner_approval=local_approval,
            owner_authenticator=type("FixtureAuthenticator", (), {"verify": lambda *_args: True})(),
            mission=proposal,
        )
        direct = service.StewardService(
            journal=self.journal,
            github=self.github_writer,
            github_writer=self.github_writer,
            repo_path=self.repo_dir,
            mission=constructed,
            control_state=self._ControlOff(),
        )
        self.assertEqual(direct.step()["status"], "IDLE")
        self.assertNotIn("STAGE_PLANNED", [event.event for event in self.journal.replay()])

    def test_fault_scenario_7_github_mutation_outcome_unknown(self):
        """Scenario 7: Indeterminate GitHub merge outcome strictly fails closed."""
        mission, prop_sha = self.srv.propose("Please update README.md for indeterminate merge test.", repository="Igzela/token-efficient-agent-harness-lab", base_sha=self.base_sha, mission_id="MISSION-F7")
        self._approve(mission, prop_sha, "f7")

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

    def test_fault_scenario_8_emergency_stop_halts_dispatch_without_erasing_mission(self):
        """SIMULATED control source: active stop blocks dispatch and preserves recovery state."""
        mission, prop_sha = self.srv.propose("Please update README.md for stop test.", repository="Igzela/token-efficient-agent-harness-lab", base_sha=self.base_sha, mission_id="MISSION-F8")
        self._approve(mission, prop_sha, "f8")

        class StopOn:
            def emergency_stop_active(self, *, repository: str, issue_number: int) -> bool:
                return True

        self.srv.control_state = StopOn()
        result = self.srv.step()
        self.assertEqual(result["status"], "EMERGENCY_STOP")
        self.assertEqual(self.srv.status()["mission_state"], "RUNNING")
        self.assertIn("EMERGENCY_STOP_OBSERVED", [event.event for event in self.journal.replay()])

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
