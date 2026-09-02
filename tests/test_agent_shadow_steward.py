"""Provider-free contract and replay tests for the PR2 Shadow Steward."""

from __future__ import annotations

from dataclasses import replace
import ast
import hashlib
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
CONTROL = ROOT / "scripts" / "agent-control"
sys.path.insert(0, str(CONTROL))

import mission_contract as contract  # noqa: E402
import shadow_steward as shadow  # noqa: E402
import steward  # noqa: E402
import steward_github  # noqa: E402
from steward_journal import StewardJournal  # noqa: E402
import steward_workers as workers  # noqa: E402
import worktree_manager  # noqa: E402


class ShadowStewardTests(unittest.TestCase):
    class Authenticator:
        def verify(self, approval, proposal_sha256):
            return (
                approval.owner_identity == "github:Igzela"
                and approval.proposal_sha256 == proposal_sha256
            )

    def setUp(self) -> None:
        self.mission = contract.campaign_mission()
        self.request = (
            "Implement the bounded documentation and tests change in "
            "docs/ARCHITECTURE_BOOK.md and tests/test_mission_contract.py."
        )
        self.proposal = shadow.compile_proposal(self.request)
        self.approval = contract.OwnerApproval(
            "github:Igzela",
            self.proposal.proposal_sha256,
            "shadow-approval-1",
            "2026-08-28T00:00:00Z",
        )
        self.authenticator = self.Authenticator()
        # Stage planning derives authority from one already-activated Mission;
        # it must never turn a second synthetic stage approval into authority.
        activation_approval = contract.OwnerApproval(
            "github:Igzela",
            self.mission.proposal_sha256,
            "shadow-mission-approval",
            "2026-08-30T00:00:00Z",
        )
        self.running_mission = contract.activate_current_mission(
            repository=self.mission.repository_identity.repository,
            base_sha=self.mission.repository_identity.base_sha,
            branch=self.mission.repository_identity.branch,
            source_ref=self.mission.repository_identity.source_ref,
            source_sha256=self.mission.repository_identity.source_sha256,
            proposal_sha256=self.mission.proposal_sha256,
            owner_approval=activation_approval,
            owner_authenticator=self.authenticator,
            mission=self.mission,
        )

    def approved_plan(self):
        decision = shadow.evaluate_proposal(
            self.proposal,
            self.mission,
            self.approval,
            owner_authenticator=self.authenticator,
        )
        return shadow.plan_stage(
            self.proposal,
            self.running_mission,
        )

    def test_intake_is_bounded_digested_and_does_not_retain_raw_request(self):
        intake = shadow.compile_intake(self.request)
        self.assertEqual(intake.requested_paths, (
            "docs/ARCHITECTURE_BOOK.md",
            "tests/test_mission_contract.py",
        ))
        self.assertNotIn("Implement", repr(intake))
        self.assertNotIn("Implement", str(intake.to_wire()))
        self.assertEqual(len(intake.request_sha256), 64)

    def test_proposal_round_trip_and_digest_binding(self):
        self.assertEqual(
            shadow.MissionProposal.from_wire(self.proposal.to_wire()).to_wire(),
            self.proposal.to_wire(),
        )
        forged = self.proposal.to_wire()
        forged["requested_paths"] = ["engine/foo.py"]
        with self.assertRaisesRegex(shadow.ShadowStewardError, "proposal_digest_mismatch"):
            shadow.MissionProposal.from_wire(forged)

    def test_owner_approval_requires_exact_digest_and_never_consumes_authority(self):
        approved = shadow.evaluate_proposal(
            self.proposal,
            self.mission,
            self.approval,
            owner_authenticator=self.authenticator,
        )
        self.assertEqual(approved.status, "SHADOW_RECOMMENDATION")
        self.assertTrue(approved.owner_authenticated)
        self.assertTrue(approved.recommendation_active)
        self.assertFalse(approved.authority_consumed)
        self.assertFalse(approved.mutation_allowed)

        forged = replace(self.approval, proposal_sha256="f" * 64)
        rejected = shadow.evaluate_proposal(
            self.proposal,
            self.mission,
            forged,
            owner_authenticator=self.authenticator,
        )
        self.assertEqual(rejected.status, "REJECTED")
        self.assertFalse(rejected.recommendation_active)
        self.assertFalse(rejected.owner_authenticated)

        unauthenticated = shadow.evaluate_proposal(
            self.proposal, self.mission, self.approval
        )
        self.assertEqual(unauthenticated.status, "WAITING_AUTHENTICATION")

        malformed = replace(self.approval, approved_at="not-a-timestamp")
        rejected = shadow.evaluate_proposal(
            self.proposal,
            self.mission,
            malformed,
            owner_authenticator=self.authenticator,
        )
        self.assertEqual(rejected.status, "REJECTED")

    def test_unauthorized_comment_shaped_input_cannot_activate(self):
        forged_comment = {
            "author": "repository-owner",
            "body": "approve",
            "proposal_sha256": self.proposal.proposal_sha256,
        }
        result = shadow.evaluate_proposal(
            self.proposal,
            self.mission,
            forged_comment,
            owner_authenticator=self.authenticator,
        )
        self.assertEqual(result.status, "REJECTED")
        self.assertFalse(result.owner_authenticated)

        attacker = replace(self.approval, owner_identity="attacker")
        result = shadow.evaluate_proposal(
            self.proposal,
            self.mission,
            attacker,
            owner_authenticator=self.authenticator,
        )
        self.assertEqual(result.status, "REJECTED")

    def test_scope_and_high_risk_requests_pause(self):
        for request, expected in (
            (
                "Please deploy this to production from docs/ARCHITECTURE_BOOK.md.",
                "AUTHORITY_REQUIRED",
            ),
            ("Delete the production data; outcome unknown after the send.", "SAFETY_CONFLICT"),
            ("Broaden scope to all files and increase budget.", "AUTHORITY_REQUIRED"),
        ):
            with self.subTest(request=request):
                proposal = shadow.compile_proposal(request)
                self.assertIn(expected, proposal.stop_codes)
                result = shadow.evaluate_proposal(proposal, self.mission, None)
                self.assertEqual(result.status, "WAITING_APPROVAL")
                plan = shadow.plan_stage(proposal, self.mission)
                self.assertEqual(plan.disposition, "PAUSED_FOR_OWNER")
                self.assertTrue(plan.stop.pause_owner)

        for request in (
            "Ship this to prod from docs/ARCHITECTURE_BOOK.md.",
            "Expand the scope to docs/ARCHITECTURE_BOOK.md.",
            "I do not know whether it was sent.",
            "The operation may have succeeded while updating docs/ARCHITECTURE_BOOK.md.",
            "The operation could have succeeded while updating docs/ARCHITECTURE_BOOK.md.",
            "The operation may have completed while updating docs/ARCHITECTURE_BOOK.md.",
            "We have no confirmation it was sent while updating docs/ARCHITECTURE_BOOK.md.",
            "We lack confirmation of delivery while updating docs/ARCHITECTURE_BOOK.md.",
            "The response was ambiguous while updating docs/ARCHITECTURE_BOOK.md.",
            "It might have been transmitted while updating docs/ARCHITECTURE_BOOK.md.",
            "Write to GitHub while updating docs/ARCHITECTURE_BOOK.md.",
            "Erase records while updating docs/ARCHITECTURE_BOOK.md.",
            "I cannot determine whether it completed while updating docs/ARCHITECTURE_BOOK.md.",
            "Push changes upstream.",
            "Merge this branch.",
            "Comment on the pull request.",
            "Send the report to Slack while updating docs/ARCHITECTURE_BOOK.md.",
            "Post the result to the external service while updating docs/ARCHITECTURE_BOOK.md.",
            "Open a pull request for docs/ARCHITECTURE_BOOK.md.",
            "Send the report to Discord.",
            "Notify the team in Microsoft Teams.",
            "Call the external API.",
            "Upload the artifact to S3.",
            "Push the image to the container registry.",
            "Trigger the webhook.",
            "Email the report.",
            "Invoke the API while updating docs/ARCHITECTURE_BOOK.md.",
            "Post the result to Notion while updating docs/ARCHITECTURE_BOOK.md.",
            "Push the changes to origin while updating docs/ARCHITECTURE_BOOK.md.",
            "Send the report to an arbitrary endpoint.",
            "Post the result to a URL.",
            "Call an endpoint.",
            "Invoke an unknown service.",
            "Connect to an external system.",
            "Send an HTTP request.",
            "Write to a socket.",
            "Make a network request.",
            "Transmit the report to an arbitrary destination.",
            "Run curl against an unknown endpoint.",
            "Open a connection to an arbitrary host.",
            "Make a request to an arbitrary destination.",
            "Send the report via SMTP.",
            "Connect to an unknown host.",
            "Execute a webhook.",
            "curl https://example.invalid while updating docs/ARCHITECTURE_BOOK.md.",
            "Sending the report to Discord while updating docs/ARCHITECTURE_BOOK.md.",
            "git push while updating docs/ARCHITECTURE_BOOK.md.",
            "Hit an external endpoint while updating docs/ARCHITECTURE_BOOK.md.",
            "Use curl against a remote host while updating docs/ARCHITECTURE_BOOK.md.",
            "Message a Slack channel while updating docs/ARCHITECTURE_BOOK.md.",
            "Create a ticket in Jira while updating docs/ARCHITECTURE_BOOK.md.",
            "Make an outbound request while updating docs/ARCHITECTURE_BOOK.md.",
            "Store data in the external database while updating docs/ARCHITECTURE_BOOK.md.",
            "Call the remote service while updating docs/ARCHITECTURE_BOOK.md.",
            "Connect over LAN while updating docs/ARCHITECTURE_BOOK.md.",
            "Invoke RPC while updating docs/ARCHITECTURE_BOOK.md.",
            "Run gh issue comment while updating docs/ARCHITECTURE_BOOK.md.",
            "Use nc to contact 10.0.0.1 while updating docs/ARCHITECTURE_BOOK.md.",
            "Send data over TCP while updating docs/ARCHITECTURE_BOOK.md.",
            "Transfer with rsync while updating docs/ARCHITECTURE_BOOK.md.",
            "Send over WebSocket while updating docs/ARCHITECTURE_BOOK.md.",
            "Push a Docker image while updating docs/ARCHITECTURE_BOOK.md.",
            "Commit and push while updating docs/ARCHITECTURE_BOOK.md.",
            "Submit a merge request while updating docs/ARCHITECTURE_BOOK.md.",
            "Send the report via QUIC while updating docs/ARCHITECTURE_BOOK.md.",
            "Connect to localhost while updating docs/ARCHITECTURE_BOOK.md.",
            "Invoke an MCP server while updating docs/ARCHITECTURE_BOOK.md.",
            "Invoke a CLI while updating docs/ARCHITECTURE_BOOK.md.",
            "Upload using SFTP while updating docs/ARCHITECTURE_BOOK.md.",
            "git pushing changes while updating docs/ARCHITECTURE_BOOK.md.",
            "Send data over IP while updating docs/ARCHITECTURE_BOOK.md.",
            "Run a shell command while updating docs/ARCHITECTURE_BOOK.md.",
            "Execute a shell command while updating docs/ARCHITECTURE_BOOK.md.",
            "Use the terminal while updating docs/ARCHITECTURE_BOOK.md.",
            "Run bash while updating docs/ARCHITECTURE_BOOK.md.",
            "Execute Python while updating docs/ARCHITECTURE_BOOK.md.",
            "Invoke bash while updating docs/ARCHITECTURE_BOOK.md.",
            "Run an executable while updating docs/ARCHITECTURE_BOOK.md.",
            "Call a binary while updating docs/ARCHITECTURE_BOOK.md.",
            "Create a ticket in Salesforce while updating docs/ARCHITECTURE_BOOK.md.",
            "Use an integration while updating docs/ARCHITECTURE_BOOK.md.",
            "Send the report outside while updating docs/ARCHITECTURE_BOOK.md.",
            "Transmit the report offsite while updating docs/ARCHITECTURE_BOOK.md.",
            "Share the report with a colleague while updating docs/ARCHITECTURE_BOOK.md.",
            "Make a request while updating docs/ARCHITECTURE_BOOK.md.",
            "Launch a shell to review docs/ARCHITECTURE_BOOK.md.",
            "Start a process to inspect docs/ARCHITECTURE_BOOK.md.",
            "Spawn an executable to inspect docs/ARCHITECTURE_BOOK.md.",
            "Remove records while updating docs/ARCHITECTURE_BOOK.md.",
            "Clear the data while updating docs/ARCHITECTURE_BOOK.md.",
            "Increase the scope to docs/ARCHITECTURE_BOOK.md.",
            "Override the owner approval for docs/ARCHITECTURE_BOOK.md.",
            "Roll out this change while updating docs/ARCHITECTURE_BOOK.md.",
            "Promote this change live while updating docs/ARCHITECTURE_BOOK.md.",
            "更新 docs/ARCHITECTURE_BOOK.md 并发送到外部服务。",
            "更新 docs/ARCHITECTURE_BOOK.md 并删除生产数据。",
            "更新 docs/ARCHITECTURE_BOOK.md，结果未知。",
            "更新 docs/ARCHITECTURE_BOOK.md 并部署到生产。",
            "更新 docs/ARCHITECTURE_BOOK.md 并扩大权限。",
            "更新 docs/ARCHITECTURE_BOOK.md 并运行命令。",
        ):
            with self.subTest(request=request):
                proposal = shadow.compile_proposal(request)
                self.assertTrue(proposal.stop_codes)
                self.assertTrue(shadow.plan_stage(proposal, self.mission).stop.pause_owner)

    def test_change_type_cannot_widen_the_registered_mission(self):
        proposal = shadow.compile_proposal(
            "Update the workflow in docs/ARCHITECTURE_BOOK.md."
        )
        self.assertIn("workflow", proposal.change_types)
        self.assertEqual(shadow.plan_stage(proposal, self.mission).disposition, "WAITING_APPROVAL")

    def test_planner_reuses_and_validates_mission_stage_workcard_owners(self):
        plan = self.approved_plan()
        self.assertEqual(plan.disposition, "PLANNED")
        self.assertIsNotNone(plan.stage)
        self.assertEqual(len(plan.workcards), 2)
        self.assertEqual(
            {card.dependencies for card in plan.workcards},
            {()},
        )
        self.assertTrue(plan.projection_only)
        contract.validate_stage(plan.stage, self.running_mission, plan.workcards)
        self.assertTrue(shadow.shadow_only(plan))

        forged_plan = replace(plan, _provenance=None)
        with self.assertRaisesRegex(shadow.ShadowStewardError, "plan_projection_invalid"):
            shadow.replan(forged_plan, "CI_FAILED")

        forged_mission = replace(plan, mission_id="forged-mission")
        with self.assertRaisesRegex(shadow.ShadowStewardError, "plan_projection_invalid"):
            shadow.replan(forged_mission, "CI_FAILED")

        stale = replace(self.mission, objective="forged objective")
        with self.assertRaisesRegex(shadow.ShadowStewardError, "mission_registration_invalid"):
            shadow.plan_stage(self.proposal, stale)

    def test_routine_replan_does_not_pause_but_unknown_outcome_does(self):
        plan = self.approved_plan()
        recovered = shadow.replan(plan, "CI_FAILED")
        self.assertEqual(recovered.disposition, "RECOVERY_RECOMMENDED")
        self.assertFalse(recovered.stop.pause_owner)
        self.assertTrue(recovered.stop.retry_allowed)
        self.assertEqual(recovered.workcards[0].result_state, "REPLAN_REQUIRED")

        unknown = shadow.replan(plan, "EXTERNAL_OUTCOME_UNKNOWN")
        self.assertEqual(unknown.disposition, "PAUSED_FOR_OWNER")
        self.assertTrue(unknown.stop.pause_owner)
        self.assertFalse(unknown.stop.retry_allowed)
        self.assertEqual(unknown.workcards, ())

        budget = shadow.replan(
            plan,
            "TEST_FAILED",
            attempt_number=self.mission.budget.max_retries + 1,
        )
        self.assertEqual(budget.stop.code, "BUDGET_EXCEEDED")
        self.assertFalse(budget.stop.retry_allowed)

    def test_historical_issue_pr_ci_review_replay_has_no_false_pause(self):
        cases = shadow.historical_failure_fixtures()
        result = shadow.replay_historical_failures(cases)
        self.assertTrue(result.passed)
        self.assertEqual(result.case_count, 5)
        self.assertEqual(result.ordinary_failure_count, 3)
        self.assertEqual(result.false_pause_count, 0)
        self.assertEqual(result.mismatch_count, 0)
        self.assertEqual(len(result.comparison_sha256), 64)

    def test_compact_status_contains_only_projection_facts(self):
        plan = self.approved_plan()
        replay = shadow.replay_historical_failures(
            [shadow.historical_failure_fixtures()[2]]
        )
        status = shadow.compact_status(plan, replay)
        wire = status.to_wire()
        self.assertTrue(wire["projection_only"])
        self.assertFalse(wire["authority_consumed"])
        self.assertFalse(wire["mutation_allowed"])
        self.assertEqual(wire["replay_case_count"], 1)
        self.assertNotIn("requested_paths", wire)
        self.assertNotIn("Implement", str(wire))

        forged = replace(replay, case_count=999)
        with self.assertRaisesRegex(
            shadow.ShadowStewardError, "replay_projection_invalid"
        ):
            shadow.compact_status(plan, forged)

        unsealed = replace(replay, _provenance=None)
        with self.assertRaisesRegex(
            shadow.ShadowStewardError, "replay_projection_invalid"
        ):
            shadow.compact_status(plan, unsealed)

    def test_planner_waits_for_approval_instead_of_bypassing_it(self):
        waiting = shadow.plan_stage(self.proposal, self.mission)
        self.assertEqual(waiting.disposition, "WAITING_APPROVAL")
        self.assertIsNone(waiting.stage)

    def test_replay_evidence_digest_cannot_be_forged(self):
        case = shadow.historical_failure_fixtures()[0].to_wire()
        case["evidence_ref"] = "github:issue:77:tampered"
        with self.assertRaisesRegex(
            shadow.ShadowStewardError, "replay_evidence_digest_mismatch"
        ):
            shadow.ReplayCase.from_wire(case)

        forged_status = shadow.historical_failure_fixtures()[0].to_wire()
        forged_status["legacy_status"] = "outcome_unknown"
        with self.assertRaisesRegex(
            shadow.ShadowStewardError, "replay_case_fields_invalid"
        ):
            shadow.ReplayCase.from_wire(forged_status)

        divergent = shadow.historical_failure_fixtures()[0].to_wire()
        divergent["failure_code"] = "CI_FAILED"
        divergent["evidence_sha256"] = shadow.ReplayCase._evidence_digest(
            divergent["case_id"],
            divergent["source"],
            divergent["failure_code"],
            divergent["evidence_ref"],
        )
        with self.assertRaisesRegex(
            shadow.ShadowStewardError, "historical_evidence_binding_invalid"
        ):
            shadow.ReplayCase.from_wire(divergent)

    def test_untrusted_bounds_and_unknown_failure_fail_closed(self):
        with self.assertRaisesRegex(shadow.ShadowStewardError, "raw_request_invalid"):
            shadow.compile_intake("x\ncommand")
        with self.assertRaisesRegex(shadow.ShadowStewardError, "raw_request_invalid"):
            shadow.compile_intake("x" * (shadow.MAX_INTAKE_CHARS + 1))
        stop = shadow.classify_stop("some_unrecognized_failure")
        self.assertEqual(stop.category, "PAUSED_FOR_OWNER")
        self.assertFalse(stop.retry_allowed)

        private = shadow.compile_intake("Inspect docs/.env and scripts/.git/config.")
        self.assertEqual(private.requested_paths, ())
        self.assertIn("SAFETY_CONFLICT", private.stop_codes)

    def test_wire_ingress_rejects_private_paths_even_with_a_recomputed_digest(self):
        for path, reason in (
            ("docs/.env.local", "private_path_forbidden"),
            ("docs/id_rsa", "private_path_forbidden"),
            ("docs/private_key", "private_path_forbidden"),
            ("docs/private-key", "private_path_forbidden"),
            ("docs/privatekey", "private_path_forbidden"),
            ("docs/id_rsa_backup", "private_path_forbidden"),
            ("docs/authorized_keys", "private_path_forbidden"),
            ("docs/access_token", "private_path_forbidden"),
            ("docs/../engine/foo.py", "path_syntax_forbidden"),
            ("docs/../../etc/passwd", "path_syntax_forbidden"),
            ("/evil/../docs/ARCHITECTURE_BOOK.md", "path_syntax_forbidden"),
            ("/evil/docs/ARCHITECTURE_BOOK.md", "path_syntax_forbidden"),
            ("evil/docs/ARCHITECTURE_BOOK.md", "path_syntax_forbidden"),
            ("C:docs/ARCHITECTURE_BOOK.md", "path_syntax_forbidden"),
            ("prefix:docs/ARCHITECTURE_BOOK.md", "path_syntax_forbidden"),
            (r"\\server\\docs\\ARCHITECTURE_BOOK.md", "path_syntax_forbidden"),
            (r"evil\\docs\\ARCHITECTURE_BOOK.md", "path_syntax_forbidden"),
            (r"docs\\..\\engine\\foo.py", "path_syntax_forbidden"),
            (r"C:\\evil\\docs\\ARCHITECTURE_BOOK.md", "path_syntax_forbidden"),
        ):
            with self.subTest(path=path):
                intake = shadow.compile_intake(f"Update {path}.")
                self.assertEqual(intake.requested_paths, ())
                self.assertIn("SAFETY_CONFLICT", intake.stop_codes)

                intake_wire = shadow.compile_intake(self.request).to_wire()
                intake_wire["requested_paths"] = [path]
                with self.assertRaisesRegex(shadow.ShadowStewardError, reason):
                    shadow.Intake.from_wire(intake_wire)

                proposal_wire = self.proposal.to_wire()
                proposal_wire["requested_paths"] = [path]
                proposal_wire["proposal_sha256"] = contract.json_sha256(
                    {
                        key: value
                        for key, value in proposal_wire.items()
                        if key != "proposal_sha256"
                    }
                )
                with self.assertRaisesRegex(shadow.ShadowStewardError, reason):
                    shadow.MissionProposal.from_wire(proposal_wire)

    def test_wire_ingress_accepts_only_controlled_redacted_fields(self):
        intake_wire = shadow.compile_intake(self.request).to_wire()
        intake_wire["risk_flags"] = ["credential:untrusted"]
        with self.assertRaisesRegex(shadow.ShadowStewardError, "risk_flags_invalid"):
            shadow.Intake.from_wire(intake_wire)

        intake_wire = shadow.compile_intake(self.request).to_wire()
        intake_wire["intent"] = "Implement the original prompt contents."
        with self.assertRaisesRegex(shadow.ShadowStewardError, "intent_invalid"):
            shadow.Intake.from_wire(intake_wire)

        proposal_wire = self.proposal.to_wire()
        proposal_wire["risk_flags"] = ["credential:untrusted"]
        with self.assertRaisesRegex(shadow.ShadowStewardError, "risk_flags_invalid"):
            shadow.MissionProposal.from_wire(proposal_wire)

        proposal_wire = self.proposal.to_wire()
        proposal_wire["objective_kind"] = "Implement the original prompt contents."
        with self.assertRaisesRegex(
            shadow.ShadowStewardError, "objective_kind_invalid"
        ):
            shadow.MissionProposal.from_wire(proposal_wire)

    def test_shadow_module_has_no_effect_transport_or_persistence_imports(self):
        source = (CONTROL / "shadow_steward.py").read_text(encoding="utf-8")
        tree = ast.parse(source)
        imported = {
            node.names[0].name.split(".", 1)[0]
            for node in ast.walk(tree)
            if isinstance(node, ast.Import)
        }
        imported.update(
            node.module.split(".", 1)[0]
            for node in ast.walk(tree)
            if isinstance(node, ast.ImportFrom) and node.module
        )
        self.assertEqual(
            imported & {"subprocess", "urllib", "requests", "sqlite3", "pathlib", "socket"},
            set(),
        )
        self.assertNotIn("GitHubReader", source)


class StewardCheckpointRestartTests(unittest.TestCase):
    """Public-lifecycle regressions for durable WorkCard checkpoint recovery."""

    class Authenticator:
        def verify(self, approval, proposal_sha256):
            return (
                approval.owner_identity == "github:Igzela"
                and approval.proposal_sha256 == proposal_sha256
            )

    def git(self, *args: str, cwd: Path | None = None) -> str:
        result = subprocess.run(
            ["git", *args],
            cwd=cwd or self.repo,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout.strip()

    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.repo = self.root / "repo"
        self.repo.mkdir()
        self.git("init", "-b", "main")
        self.git("config", "user.name", "Checkpoint Fixture")
        self.git("config", "user.email", "checkpoint@example.invalid")
        (self.repo / "docs").mkdir()
        (self.repo / "docs" / "AUTONOMY.md").write_text("accepted\n", encoding="utf-8")
        self.git("add", "docs/AUTONOMY.md")
        self.git("commit", "-m", "accepted baseline")
        self.git(
            "remote",
            "add",
            "origin",
            "https://github.com/Igzela/token-efficient-agent-harness-lab.git",
        )
        self.base = self.git("rev-parse", "HEAD")

        proposed, proposal_sha = contract.compile_proposal_mission(
            "Update docs/AUTONOMY.md.",
            repository=contract.CAMPAIGN_REPOSITORY,
            base_sha=self.base,
            mission_id="MISSION-CHECKPOINT-RESTART",
        )
        approval = contract.OwnerApproval(
            "github:Igzela",
            proposal_sha,
            "checkpoint-restart-approval",
            "2026-09-03T00:00:00Z",
        )
        self.mission = contract.activate_current_mission(
            repository=proposed.repository_identity.repository,
            base_sha=self.base,
            branch=proposed.repository_identity.branch,
            source_ref=proposed.repository_identity.source_ref,
            source_sha256=proposed.repository_identity.source_sha256,
            proposal_sha256=proposal_sha,
            owner_approval=approval,
            owner_authenticator=self.Authenticator(),
            mission=proposed,
        )
        self.card = contract.WorkCard(
            "stage-restart:card-1",
            "stage-restart",
            ("docs/AUTONOMY.md",),
            ("outside-approved/",),
            ("Apply one bounded change.",),
            ("focused_checks_required",),
            ("reject scope expansion",),
            ("durable checkpoint",),
            (),
            ("docs/AUTONOMY.md",),
            2,
            "T1",
            self.mission.rollback,
            "PENDING",
        )
        self.stage = contract.Stage(
            "stage-restart",
            self.mission.mission_id,
            "Recover one exact reviewed WorkCard checkpoint.",
            self.mission.repository_identity,
            ("focused", "independent review"),
            ("no external effects",),
            (self.card.card_id,),
            self.mission.rollback,
            None,
            None,
        )
        self.journal = StewardJournal(self.root / "steward.sqlite3")
        self.worktree_base_patch = mock.patch.object(
            worktree_manager, "WORKTREE_BASE", self.root / "worktrees"
        )
        self.worktree_base_patch.start()
        self.addCleanup(self.worktree_base_patch.stop)

    def append(self, event: str, state: str, *, data=None, attempt: int = 1) -> None:
        self.journal.append(
            event=event,
            idempotency_key=f"fixture:{event.lower()}:{attempt}",
            mission_id=self.mission.mission_id,
            stage_id=self.stage.stage_id,
            card_id=self.card.card_id,
            attempt=attempt,
            state=state,
            detail="fixture",
            data=data,
        )

    def create_reviewed_checkpoint(
        self, *, include_focused: bool = True, include_review: bool = True
    ) -> tuple[Path, str, str, workers.ReviewOutcome]:
        worktree, branch = worktree_manager.steward_worktree_location(
            self.mission.mission_id,
            self.stage.stage_id,
            self.card.card_id,
            self.base,
        )
        worktree.parent.mkdir(parents=True)
        self.git("branch", branch, self.base)
        self.git("worktree", "add", str(worktree), branch)
        (worktree / "docs" / "AUTONOMY.md").write_text(
            "accepted\nreviewed checkpoint\n", encoding="utf-8"
        )
        self.git("add", "docs/AUTONOMY.md", cwd=worktree)
        self.git("commit", "-m", "bounded checkpoint", cwd=worktree)
        head = self.git("rev-parse", "HEAD", cwd=worktree)
        implementation_session = "steward-process:stage-restart:card-1:1"
        reviewer_session = "review-process:stage-restart:card-1:1"
        binding = worktree_manager.steward_binding_digest(
            self.mission.mission_id,
            self.stage.stage_id,
            self.card.card_id,
            self.base,
        )
        changed_paths_digest = hashlib.sha256(
            b"docs/AUTONOMY.md"
        ).hexdigest()[:24]
        reviewed_range = workers.review_range_digest(
            self.base, head, worktree=worktree
        )
        review = workers.ReviewOutcome.from_wire(
            workers.seal_review_outcome_wire(
                {
                    "schema_version": "steward_review_outcome.v1",
                    "status": "PASS",
                    "reviewer_session_id": reviewer_session,
                    "implementation_session_id": implementation_session,
                    "reviewed_head_sha": head,
                    "blockers": [],
                    "detail": "",
                    "reviewed_base_sha": self.base,
                    "reviewed_range_sha256": reviewed_range,
                    "review_axes": ["standards", "spec"],
                    "review_round": 1,
                    "review_mode": "full",
                    "review_receipt_sha256": "",
                    "summary": "bounded independent review",
                    "findings": None,
                    "security_ok": True,
                    "rollback_ok": True,
                    "observed_ci_status": "unknown",
                    "finding_ledger_digest": "",
                }
            )
        )
        decision = workers.canonical_review_decision(review)
        self.append("CARD_QUEUED", "QUEUED")
        self.append(
            "WORKER_STARTED",
            "RUNNING",
            data={
                "base_sha": self.base,
                "worktree_binding_sha256": binding,
                "branch": branch,
            },
        )
        self.append(
            "WORKER_CHECKPOINT",
            "VERIFYING",
            data={
                "base_sha": self.base,
                "head_sha": head,
                "changed_paths_digest": changed_paths_digest,
                "worktree_binding_sha256": binding,
                "implementation_session_id": implementation_session,
            },
        )
        if include_focused:
            self.append(
                "FOCUSED_CHECKS_PASSED",
                "REVIEWING",
                data={"check_count": 1},
            )
        if include_review:
            self.assertTrue(include_focused)
            self.append(
                "LOCAL_REVIEW_OBSERVED",
                "REVIEWING",
                data={
                    "base_sha": self.base,
                    "head_sha": head,
                    "review_round": 1,
                    "review_mode": "full",
                    "verdict": "PASS",
                    "open_blocker_ids": [],
                    "deferred_note_ids": [],
                    "finding_ledger_digest": decision.finding_ledger_digest,
                    "security_ok": True,
                    "rollback_ok": True,
                    "observed_ci_status": "unknown",
                    "implementation_session_id": implementation_session,
                    "reviewer_session_id": reviewer_session,
                    "reviewed_range_sha256": reviewed_range,
                    "review_axes": ["standards", "spec"],
                    "review_receipt_sha256": review.review_receipt_sha256,
                },
            )
        return worktree, branch, head, review

    def test_restart_restores_reviewed_checkpoint_without_replaying_children(self):
        worktree, branch, head, _review = self.create_reviewed_checkpoint()
        shutil.rmtree(worktree)
        worker = workers.BoundedProcessWorker(
            lambda _context: ["/usr/bin/python3", "-c", "raise SystemExit(99)"]
        )
        reviewer = workers.BoundedProcessReviewer(
            lambda _context, _outcome: [
                "/usr/bin/python3",
                "-c",
                "raise SystemExit(99)",
            ]
        )
        instance = steward.Steward(
            repository=contract.CAMPAIGN_REPOSITORY,
            repo_path=self.repo,
            journal=self.journal,
            github=steward_github.FakeGitHubReader(),
            worker=worker,
            reviewer=reviewer,
            lock_dir=self.root / "locks",
        )
        with (
            mock.patch.object(
                worker, "run", side_effect=AssertionError("worker replayed")
            ) as worker_run,
            mock.patch.object(
                reviewer, "review", side_effect=AssertionError("reviewer replayed")
            ) as reviewer_run,
        ):
            results = instance.execute_stage(
                self.mission,
                self.stage,
                (self.card,),
                base_sha=self.base,
            )
            repeated = instance.execute_stage(
                self.mission,
                self.stage,
                (self.card,),
                base_sha=self.base,
            )

        result = results[self.card.card_id]
        repeated_result = repeated[self.card.card_id]
        self.assertEqual(result.status, "WAITING_FOR_PR")
        self.assertEqual(repeated_result.status, "WAITING_FOR_PR")
        self.assertEqual(result.head_sha, head)
        self.assertEqual(repeated_result.head_sha, head)
        self.assertTrue(
            worktree_manager.verify_worktree(
                worktree, branch, self.repo, expected_sha=head
            )
        )
        worker_run.assert_not_called()
        reviewer_run.assert_not_called()

    def test_restart_reviews_focused_checkpoint_once_without_replaying_worker(self):
        worktree, branch, head, review = self.create_reviewed_checkpoint(
            include_review=False
        )
        shutil.rmtree(worktree)
        worker = workers.BoundedProcessWorker(
            lambda _context: ["/usr/bin/python3", "-c", "raise SystemExit(99)"]
        )
        reviewer = workers.BoundedProcessReviewer(
            lambda _context, _outcome: [
                "/usr/bin/python3",
                "-c",
                "raise SystemExit(99)",
            ]
        )
        instance = steward.Steward(
            repository=contract.CAMPAIGN_REPOSITORY,
            repo_path=self.repo,
            journal=self.journal,
            github=steward_github.FakeGitHubReader(),
            worker=worker,
            reviewer=reviewer,
            lock_dir=self.root / "locks",
        )
        with (
            mock.patch.object(
                worker, "run", side_effect=AssertionError("worker replayed")
            ) as worker_run,
            mock.patch.object(reviewer, "review", return_value=review) as reviewer_run,
        ):
            results = instance.execute_stage(
                self.mission,
                self.stage,
                (self.card,),
                base_sha=self.base,
            )

        result = results[self.card.card_id]
        self.assertEqual(result.status, "WAITING_FOR_PR")
        self.assertEqual(result.head_sha, head)
        self.assertTrue(
            worktree_manager.verify_worktree(
                worktree, branch, self.repo, expected_sha=head
            )
        )
        worker_run.assert_not_called()
        reviewer_run.assert_called_once()
        self.assertEqual(
            [event.event for event in self.journal.replay()].count(
                "LOCAL_REVIEW_OBSERVED"
            ),
            1,
        )

    def test_restart_verifies_checkpoint_once_without_replaying_worker(self):
        worktree, branch, head, review = self.create_reviewed_checkpoint(
            include_focused=False,
            include_review=False,
        )
        shutil.rmtree(worktree)
        worker = workers.BoundedProcessWorker(
            lambda _context: ["/usr/bin/python3", "-c", "raise SystemExit(99)"]
        )
        reviewer = workers.BoundedProcessReviewer(
            lambda _context, _outcome: [
                "/usr/bin/python3",
                "-c",
                "raise SystemExit(99)",
            ]
        )
        with mock.patch.object(
            workers,
            "run_allowlisted_checks",
            return_value=[{"command": "git diff --check", "exit_code": 0}],
        ) as verifier_run:
            instance = steward.Steward(
                repository=contract.CAMPAIGN_REPOSITORY,
                repo_path=self.repo,
                journal=self.journal,
                github=steward_github.FakeGitHubReader(),
                worker=worker,
                reviewer=reviewer,
                lock_dir=self.root / "locks",
            )
            with (
                mock.patch.object(
                    worker, "run", side_effect=AssertionError("worker replayed")
                ) as worker_run,
                mock.patch.object(
                    reviewer, "review", return_value=review
                ) as reviewer_run,
            ):
                results = instance.execute_stage(
                    self.mission,
                    self.stage,
                    (self.card,),
                    base_sha=self.base,
                )
                repeated = instance.execute_stage(
                    self.mission,
                    self.stage,
                    (self.card,),
                    base_sha=self.base,
                )

        result = results[self.card.card_id]
        repeated_result = repeated[self.card.card_id]
        self.assertEqual(result.status, "WAITING_FOR_PR")
        self.assertEqual(repeated_result.status, "WAITING_FOR_PR")
        self.assertEqual(result.head_sha, head)
        self.assertEqual(repeated_result.head_sha, head)
        self.assertTrue(
            worktree_manager.verify_worktree(
                worktree, branch, self.repo, expected_sha=head
            )
        )
        worker_run.assert_not_called()
        verifier_run.assert_called_once()
        reviewer_run.assert_called_once()
        events = [event.event for event in self.journal.replay()]
        self.assertEqual(events.count("FOCUSED_CHECKS_PASSED"), 1)
        self.assertEqual(events.count("LOCAL_REVIEW_OBSERVED"), 1)

    def test_checkpoint_verifier_fault_does_not_admit_worker_retry(self):
        worktree, _branch, _head, _review = self.create_reviewed_checkpoint(
            include_focused=False,
            include_review=False,
        )
        shutil.rmtree(worktree)
        worker = workers.BoundedProcessWorker(
            lambda _context: ["/usr/bin/python3", "-c", "raise SystemExit(99)"]
        )
        reviewer = workers.BoundedProcessReviewer(
            lambda _context, _outcome: ["/usr/bin/python3", "-c", "raise SystemExit(99)"]
        )
        with mock.patch.object(
            workers,
            "run_allowlisted_checks",
            side_effect=RuntimeError("fixture verifier fault"),
        ) as verifier_run:
            instance = steward.Steward(
                repository=contract.CAMPAIGN_REPOSITORY,
                repo_path=self.repo,
                journal=self.journal,
                github=steward_github.FakeGitHubReader(),
                worker=worker,
                reviewer=reviewer,
                lock_dir=self.root / "locks",
            )
            with mock.patch.object(
                worker, "run", side_effect=AssertionError("worker replayed")
            ) as worker_run:
                first = instance.execute_stage(
                    self.mission,
                    self.stage,
                    (self.card,),
                    base_sha=self.base,
                )
                second = instance.execute_stage(
                    self.mission,
                    self.stage,
                    (self.card,),
                    base_sha=self.base,
                )

        self.assertEqual(first[self.card.card_id].status, "RECOVERY_REQUIRED")
        self.assertEqual(second[self.card.card_id].status, "RECOVERY_REQUIRED")
        self.assertEqual(verifier_run.call_count, 2)
        worker_run.assert_not_called()

    def test_checkpoint_restore_cleans_only_verified_new_registration(self):
        worktree, branch = worktree_manager.steward_worktree_location(
            self.mission.mission_id,
            self.stage.stage_id,
            self.card.card_id,
            self.base,
        )
        self.git("branch", branch, self.base)
        with mock.patch.object(worktree_manager, "verify_worktree", return_value=False):
            restored = worktree_manager.restore_steward_checkpoint_worktree(
                self.mission.mission_id,
                self.stage.stage_id,
                self.card.card_id,
                str(self.repo),
                self.base,
                self.base,
            )

        self.assertIsNone(restored)
        records = self.git("worktree", "list", "--porcelain")
        self.assertNotIn(branch, records)
        self.assertFalse(worktree.exists())
        self.assertEqual(self.git("rev-parse", branch), self.base)

    def test_focused_checkpoint_recovery_preserves_r2_review_convergence(self):
        worktree, branch, head1, _review1 = self.create_reviewed_checkpoint(
            include_focused=False,
            include_review=False,
        )
        prior = {
            "base_sha": self.base,
            "head_sha": head1,
            "review_round": 1,
            "review_mode": "full",
            "verdict": "FAIL",
            "open_blocker_ids": ["finding-1"],
            "deferred_note_ids": [],
            "decision_required_ids": [],
            "finding_ledger_digest": "2" * 64,
            "security_ok": True,
            "rollback_ok": True,
            "autonomous_repairs_remaining": 1,
            "stop_reason": "",
        }
        self.append("REVIEW_FAILED", "REVIEWING", data=prior)
        self.append("RETRYING", "RETRYING")
        self.append("CARD_QUEUED", "QUEUED", attempt=2)
        (worktree / "docs" / "AUTONOMY.md").write_text(
            "accepted\nreviewed checkpoint\nR2 checkpoint\n", encoding="utf-8"
        )
        self.git("add", "docs/AUTONOMY.md", cwd=worktree)
        self.git("commit", "-m", "bounded R2 checkpoint", cwd=worktree)
        head2 = self.git("rev-parse", "HEAD", cwd=worktree)
        binding = worktree_manager.steward_binding_digest(
            self.mission.mission_id,
            self.stage.stage_id,
            self.card.card_id,
            self.base,
        )
        self.append(
            "WORKER_STARTED",
            "RUNNING",
            attempt=2,
            data={
                "base_sha": self.base,
                "worktree_binding_sha256": binding,
                "branch": branch,
            },
        )
        self.append(
            "WORKER_CHECKPOINT",
            "VERIFYING",
            attempt=2,
            data={
                "base_sha": self.base,
                "head_sha": head2,
                "changed_paths_digest": hashlib.sha256(b"docs/AUTONOMY.md").hexdigest()[:24],
                "worktree_binding_sha256": binding,
                "implementation_session_id": "steward-process:stage-restart:card-1:2",
            },
        )
        reviewed_range = workers.review_range_digest(self.base, head2, worktree=worktree)
        shutil.rmtree(worktree)
        review2 = workers.ReviewOutcome.from_wire(
            workers.seal_review_outcome_wire(
                {
                    "schema_version": "steward_review_outcome.v1",
                    "status": "PASS",
                    "reviewer_session_id": "review-process:stage-restart:card-1:2",
                    "implementation_session_id": "steward-process:stage-restart:card-1:2",
                    "reviewed_head_sha": head2,
                    "blockers": [],
                    "detail": "",
                    "reviewed_base_sha": self.base,
                    "reviewed_range_sha256": reviewed_range,
                    "review_axes": ["standards", "spec"],
                    "review_round": 2,
                    "review_mode": "repair_verification",
                    "review_receipt_sha256": "",
                    "summary": "bounded R2 independent review",
                    "findings": None,
                    "security_ok": True,
                    "rollback_ok": True,
                    "observed_ci_status": "unknown",
                    "finding_ledger_digest": "",
                }
            )
        )
        worker = workers.BoundedProcessWorker(
            lambda _context: ["/usr/bin/python3", "-c", "raise SystemExit(99)"]
        )
        reviewer = workers.BoundedProcessReviewer(
            lambda _context, _outcome: ["/usr/bin/python3", "-c", "raise SystemExit(99)"]
        )
        with (
            mock.patch.object(
                workers,
                "run_allowlisted_checks",
                return_value=[{"command": "git diff --check", "exit_code": 0}],
            ) as verifier_run,
        ):
            instance = steward.Steward(
                repository=contract.CAMPAIGN_REPOSITORY,
                repo_path=self.repo,
                journal=self.journal,
                github=steward_github.FakeGitHubReader(),
                worker=worker,
                reviewer=reviewer,
                lock_dir=self.root / "locks",
            )
            with (
                mock.patch.object(worker, "run", side_effect=AssertionError("worker replayed")) as worker_run,
                mock.patch.object(reviewer, "review", return_value=review2) as reviewer_run,
            ):
                result = instance.execute_stage(
                    self.mission,
                    self.stage,
                    (self.card,),
                    base_sha=self.base,
                )

        self.assertEqual(
            result[self.card.card_id].status,
            "WAITING_FOR_PR",
            result[self.card.card_id].reason,
        )
        self.assertEqual(result[self.card.card_id].head_sha, head2)
        reviewer_run.assert_called_once()
        verifier_run.assert_called_once()
        worker_run.assert_not_called()
        self.assertEqual(
            [event.event for event in self.journal.replay()].count(
                "REVIEW_REPAIR_BATCH_CONSUMED"
            ),
            1,
        )

    def test_restart_refuses_checkpoint_when_derived_branch_head_drifted(self):
        worktree, branch, _head, _review = self.create_reviewed_checkpoint()
        shutil.rmtree(worktree)
        self.git("worktree", "remove", "--force", str(worktree))
        self.git("branch", "-f", branch, self.base)
        worker = workers.BoundedProcessWorker(
            lambda _context: ["/usr/bin/python3", "-c", "raise SystemExit(99)"]
        )
        reviewer = workers.BoundedProcessReviewer(
            lambda _context, _outcome: [
                "/usr/bin/python3",
                "-c",
                "raise SystemExit(99)",
            ]
        )
        instance = steward.Steward(
            repository=contract.CAMPAIGN_REPOSITORY,
            repo_path=self.repo,
            journal=self.journal,
            github=steward_github.FakeGitHubReader(),
            worker=worker,
            reviewer=reviewer,
            lock_dir=self.root / "locks",
        )
        with (
            mock.patch.object(
                worker, "run", side_effect=AssertionError("worker replayed")
            ) as worker_run,
            mock.patch.object(
                reviewer, "review", side_effect=AssertionError("reviewer replayed")
            ) as reviewer_run,
        ):
            results = instance.execute_stage(
                self.mission,
                self.stage,
                (self.card,),
                base_sha=self.base,
            )

        self.assertEqual(results[self.card.card_id].status, "RECOVERY_REQUIRED")
        self.assertFalse(worktree.exists())
        self.assertEqual(self.git("rev-parse", branch), self.base)
        worker_run.assert_not_called()
        reviewer_run.assert_not_called()

    def test_restart_refuses_stale_checkpoint_when_a_later_tail_fact_exists(self):
        worktree, _branch, _head, _review = self.create_reviewed_checkpoint()
        self.append("REVIEW_REPAIR_BATCH_CONSUMED", "REVIEWING")
        worker = workers.BoundedProcessWorker(
            lambda _context: ["/usr/bin/python3", "-c", "raise SystemExit(99)"]
        )
        reviewer = workers.BoundedProcessReviewer(
            lambda _context, _outcome: [
                "/usr/bin/python3",
                "-c",
                "raise SystemExit(99)",
            ]
        )
        instance = steward.Steward(
            repository=contract.CAMPAIGN_REPOSITORY,
            repo_path=self.repo,
            journal=self.journal,
            github=steward_github.FakeGitHubReader(),
            worker=worker,
            reviewer=reviewer,
            lock_dir=self.root / "locks",
        )
        with (
            mock.patch.object(
                worker, "run", side_effect=AssertionError("worker replayed")
            ) as worker_run,
            mock.patch.object(
                reviewer, "review", side_effect=AssertionError("reviewer replayed")
            ) as reviewer_run,
        ):
            results = instance.execute_stage(
                self.mission,
                self.stage,
                (self.card,),
                base_sha=self.base,
            )

        self.assertEqual(results[self.card.card_id].status, "RECOVERY_REQUIRED")
        self.assertTrue(worktree.exists())
        worker_run.assert_not_called()
        reviewer_run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
