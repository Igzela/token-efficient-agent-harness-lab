"""Provider-free contract and replay tests for the PR2 Shadow Steward."""

from __future__ import annotations

from dataclasses import replace
import ast
from pathlib import Path
import sys
import unittest


ROOT = Path(__file__).resolve().parents[1]
CONTROL = ROOT / "scripts" / "agent-control"
sys.path.insert(0, str(CONTROL))

import mission_contract as contract  # noqa: E402
import shadow_steward as shadow  # noqa: E402


class ShadowStewardTests(unittest.TestCase):
    class Authenticator:
        def verify(self, approval, proposal_sha256):
            return (
                approval.owner_identity == "repository-owner"
                and approval.proposal_sha256 == proposal_sha256
                and approval.approval_id == "shadow-approval-1"
            )

    def setUp(self) -> None:
        self.mission = contract.campaign_mission()
        self.request = (
            "Implement the bounded documentation and tests change in "
            "docs/ARCHITECTURE_BOOK.md and tests/test_mission_contract.py."
        )
        self.proposal = shadow.compile_proposal(self.request)
        self.approval = contract.OwnerApproval(
            "repository-owner",
            self.proposal.proposal_sha256,
            "shadow-approval-1",
            "2026-08-28T00:00:00Z",
        )
        self.authenticator = self.Authenticator()

    def approved_plan(self):
        decision = shadow.evaluate_proposal(
            self.proposal,
            self.mission,
            self.approval,
            owner_authenticator=self.authenticator,
        )
        return shadow.plan_stage(
            self.proposal,
            self.mission,
            self.approval,
            owner_authenticator=self.authenticator,
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
            "Write to GitHub while updating docs/ARCHITECTURE_BOOK.md.",
            "Erase records while updating docs/ARCHITECTURE_BOOK.md.",
            "I cannot determine whether it completed while updating docs/ARCHITECTURE_BOOK.md.",
            "Push changes upstream.",
            "Merge this branch.",
            "Comment on the pull request.",
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
        self.assertEqual(
            shadow.plan_stage(proposal, self.mission).stop.code,
            "SCOPE_EXCEEDED",
        )

    def test_planner_reuses_and_validates_mission_stage_workcard_owners(self):
        plan = self.approved_plan()
        self.assertEqual(plan.disposition, "PLANNED")
        self.assertIsNotNone(plan.stage)
        self.assertEqual(len(plan.workcards), 1)
        self.assertTrue(plan.projection_only)
        contract.validate_stage(plan.stage, self.mission, plan.workcards)
        self.assertTrue(shadow.shadow_only(plan))

        forged_plan = replace(plan, _provenance=None)
        with self.assertRaisesRegex(shadow.ShadowStewardError, "plan_projection_invalid"):
            shadow.replan(forged_plan, "CI_FAILED")

        forged_mission = replace(plan, mission_id="forged-mission")
        with self.assertRaisesRegex(shadow.ShadowStewardError, "plan_projection_invalid"):
            shadow.replan(forged_mission, "CI_FAILED")

        stale = replace(self.mission, state="RUNNING")
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
        intake_wire["risk_flags"] = ["credential=super-secret"]
        with self.assertRaisesRegex(shadow.ShadowStewardError, "risk_flags_invalid"):
            shadow.Intake.from_wire(intake_wire)

        intake_wire = shadow.compile_intake(self.request).to_wire()
        intake_wire["intent"] = "Implement the original prompt contents."
        with self.assertRaisesRegex(shadow.ShadowStewardError, "intent_invalid"):
            shadow.Intake.from_wire(intake_wire)

        proposal_wire = self.proposal.to_wire()
        proposal_wire["risk_flags"] = ["credential=super-secret"]
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


if __name__ == "__main__":
    unittest.main()
