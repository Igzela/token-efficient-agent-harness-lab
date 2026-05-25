"""Tests for MVP5 non-persistent review guidance."""

from __future__ import annotations

from copy import deepcopy
import unittest

from harness_core.review_guidance import BOUNDARY_NOTICE, REVIEW_OPTION_NAMES, build_review_guidance


def plan(
    status: str,
    *,
    plan_id: str = "plan",
    blockers: list[str] | None = None,
    notes: list[str] | None = None,
    gates: list[str] | None = None,
    audit_verdict: str = "PASS",
    step_count: int = 3,
    context_budget: int = 2500,
    execution_budget: int = 3000,
) -> dict:
    return {
        "plan_id": plan_id,
        "status": status,
        "executable": False,
        "total_token_budget": context_budget + execution_budget,
        "context_budget": context_budget,
        "execution_budget": execution_budget,
        "blockers": blockers or [],
        "approval_gates": gates or [],
        "token_efficiency_notes": notes or [],
        "audit_summary": {"verdict": audit_verdict},
        "steps": [{"role": "planner", "context_mode": "summary"} for _ in range(step_count)],
    }


class ReviewGuidanceTests(unittest.TestCase):
    def test_blocked_remote_metadata_guidance(self):
        guidance = build_review_guidance(plan("blocked", blockers=["remote_metadata_only"]))

        self.assertEqual("register_local_repo", guidance["recommended_option"])
        self.assertEqual("review_remote_limit", guidance["next_review_action"])
        self.assertIn("keep_remote_metadata_only", [option["option"] for option in guidance["options"]])

    def test_blocked_audit_failure_guidance(self):
        guidance = build_review_guidance(plan("blocked", blockers=["audit_blocked"], audit_verdict="BLOCKED"))

        self.assertEqual("inspect_audit_result", guidance["recommended_option"])
        self.assertEqual("review_audit_failure", guidance["next_review_action"])

    def test_blocked_other_guidance(self):
        guidance = build_review_guidance(plan("blocked", blockers=["manual_blocker"]))

        self.assertEqual("inspect_blockers", guidance["recommended_option"])

    def test_needs_review_guidance_does_not_offer_approve(self):
        guidance = build_review_guidance(plan("needs_approval", gates=["human_approval_required"]))
        options = [option["option"] for option in guidance["options"]]

        self.assertEqual("inspect_gates", guidance["recommended_option"])
        self.assertIn("continue_review", options)
        self.assertNotIn("approve", options)
        self.assertNotIn("approve_plan", options)

    def test_ready_with_budget_pressure_guidance(self):
        guidance = build_review_guidance(
            plan("ready_for_review", notes=["Context budget pressure: full context reduced to excerpts."])
        )

        self.assertEqual("reduce_budget", guidance["recommended_option"])
        self.assertEqual("review_token_budget", guidance["next_review_action"])

    def test_clean_ready_guidance(self):
        guidance = build_review_guidance(plan("ready_for_review", notes=[]))

        self.assertEqual("continue_review", guidance["recommended_option"])
        self.assertEqual("review_steps", guidance["next_review_action"])

    def test_many_step_ready_guidance_suggests_split(self):
        guidance = build_review_guidance(plan("ready_for_review", notes=[], step_count=6))

        self.assertEqual("split_plan", guidance["recommended_option"])

    def test_guidance_has_non_executable_preview_boundary(self):
        guidance = build_review_guidance(plan("ready_for_review"))

        self.assertFalse(guidance["executable"])
        self.assertTrue(guidance["preview_only"])
        self.assertEqual(BOUNDARY_NOTICE, guidance["boundary_notice"])
        self.assertTrue(guidance["evidence_requirements"])
        self.assertTrue(guidance["token_efficiency_guidance"])

    def test_guidance_does_not_mutate_input_plan(self):
        original = plan("ready_for_review")
        before = deepcopy(original)

        build_review_guidance(original)

        self.assertEqual(before, original)

    def test_guidance_options_are_known_and_human_review_only(self):
        guidance = build_review_guidance(plan("needs_approval", gates=["human_approval_required"]))

        for option in guidance["options"]:
            self.assertIn(option["option"], REVIEW_OPTION_NAMES)
            self.assertEqual("human_review_only", option["allowed_effect"])


if __name__ == "__main__":
    unittest.main()
