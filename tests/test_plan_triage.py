"""Tests for MVP6 read-only planning portfolio triage."""

from __future__ import annotations

from copy import deepcopy
import unittest

from harness_core.plan_triage import (
    TRIAGE_BOUNDARY_NOTICE,
    PlanTriageError,
    build_portfolio_triage,
    classify_plan_bottleneck,
    compute_review_priority,
    derive_token_hotspots,
    triage_plan,
    validate_triage_limit,
)


def plan(
    plan_id: str,
    status: str,
    *,
    repo_id: str = "repo",
    blockers: list[str] | None = None,
    notes: list[str] | None = None,
    gates: list[str] | None = None,
    audit_verdict: str = "PASS",
    step_count: int = 3,
    context_budget: int = 1200,
    execution_budget: int = 1200,
    context_mode: str = "summary",
    effective_risk: str = "medium",
    task_type: str = "review",
) -> dict:
    return {
        "plan_id": plan_id,
        "status": status,
        "executable": False,
        "effective_risk": effective_risk,
        "total_token_budget": context_budget + execution_budget,
        "context_budget": context_budget,
        "execution_budget": execution_budget,
        "blockers": blockers or [],
        "approval_gates": gates or [],
        "token_efficiency_notes": notes or [],
        "audit_summary": {"verdict": audit_verdict},
        "repo_snapshot": {"id": repo_id, "kind": "local"},
        "task": {"repo_id": repo_id, "task_type": task_type},
        "steps": [{"role": "planner", "context_mode": context_mode} for _ in range(step_count)],
    }


class PlanTriageTests(unittest.TestCase):
    def test_empty_plan_store_returns_zero_count_triage(self):
        triage = build_portfolio_triage([])

        self.assertEqual("plan_triage.v1", triage["schema_version"])
        self.assertEqual(0, triage["total_plans"])
        self.assertEqual(0, triage["returned_items"])
        self.assertEqual(0, triage["summary"]["blocked"])
        self.assertEqual(TRIAGE_BOUNDARY_NOTICE, triage["boundary_notice"])

    def test_blocked_remote_metadata_is_remote_limited(self):
        item = triage_plan(plan("remote", "blocked", blockers=["remote_metadata_only"]))

        self.assertEqual("remote_limited", item["review_bucket"])
        self.assertEqual("remote_metadata_only", item["bottleneck"])
        self.assertEqual(70, item["review_priority"])

    def test_blocked_audit_is_audit_blocked(self):
        item = triage_plan(plan("audit", "blocked", blockers=["audit_blocked"], audit_verdict="BLOCKED"))

        self.assertEqual("audit_blocked", item["review_bucket"])
        self.assertEqual("audit_failure", item["bottleneck"])
        self.assertEqual(90, compute_review_priority(plan("audit", "blocked", blockers=["audit_blocked"])))

    def test_needs_approval_is_review_gates(self):
        item = triage_plan(plan("gated", "needs_approval", gates=["human_approval_required"]))

        self.assertEqual("review_gates", item["review_bucket"])
        self.assertEqual("approval_gates", classify_plan_bottleneck(plan("gated", "needs_approval")))
        self.assertEqual(80, item["review_priority"])

    def test_ready_with_token_notes_is_token_budget_review(self):
        item = triage_plan(
            plan("budget", "ready_for_review", notes=["Context budget pressure: full context reduced to excerpts."])
        )

        self.assertEqual("token_budget_review", item["review_bucket"])
        self.assertEqual("token_hotspot", item["bottleneck"])
        self.assertIn("budget_pressure_notes_present", item["token_hotspots"])

    def test_clean_ready_is_normal_review(self):
        item = triage_plan(plan("clean", "ready_for_review", notes=[]))

        self.assertEqual("normal_review", item["review_bucket"])
        self.assertEqual("none", item["bottleneck"])

    def test_high_context_ratio_is_token_hotspot(self):
        hotspots = derive_token_hotspots(plan("context", "ready_for_review", context_budget=900, execution_budget=100))

        self.assertIn("high_context_budget", hotspots)

    def test_many_steps_is_high_step_count_hotspot(self):
        item = triage_plan(plan("many", "ready_for_review", step_count=8, notes=[]))

        self.assertEqual("split_or_simplify", item["review_bucket"])
        self.assertIn("high_step_count", item["token_hotspots"])

    def test_stable_sort_order_is_deterministic(self):
        triage = build_portfolio_triage(
            [
                plan("a", "ready_for_review", notes=[]),
                plan("b", "ready_for_review", notes=[]),
                plan("audit", "blocked", blockers=["audit_blocked"]),
            ]
        )

        self.assertEqual(["audit", "b", "a"], [item["plan_id"] for item in triage["items"]])

    def test_triage_distinguishes_budget_pressure_original_from_lower_budget_variant(self):
        triage = build_portfolio_triage(
            [
                plan("low", "ready_for_review", context_budget=800, execution_budget=900),
                plan("original", "ready_for_review", context_budget=6000, execution_budget=1800, context_mode="full"),
            ]
        )

        self.assertEqual("original", triage["items"][0]["plan_id"])
        self.assertGreater(triage["items"][0]["review_priority"], triage["items"][1]["review_priority"])

    def test_semantic_priority_precedes_stored_index(self):
        triage = build_portfolio_triage(
            [
                plan("provider", "needs_approval", gates=["provider_integration_gate"], task_type="provider"),
                plan("low", "needs_approval", gates=["human_approval_required"]),
            ]
        )

        self.assertEqual("provider", triage["items"][0]["plan_id"])

    def test_all_needs_approval_plans_still_get_differentiated_priorities(self):
        triage = build_portfolio_triage(
            [
                plan("human", "needs_approval", gates=["human_approval_required"]),
                plan("provider", "needs_approval", gates=["provider_integration_gate"], task_type="provider"),
                plan("budget", "needs_approval", gates=["human_approval_required"], context_budget=6000, execution_budget=1800, context_mode="full"),
            ]
        )

        priorities = {item["plan_id"]: item["review_priority"] for item in triage["items"]}
        self.assertGreater(len(set(priorities.values())), 1)
        self.assertGreater(priorities["provider"], priorities["human"])

    def test_provider_boundary_plan_and_budget_pressure_plan_have_explainable_bottlenecks(self):
        provider = triage_plan(plan("provider", "needs_approval", gates=["provider_integration_gate"], task_type="provider"))
        budget = triage_plan(
            plan("budget", "ready_for_review", context_budget=6000, execution_budget=1800, context_mode="full")
        )

        self.assertEqual("provider_or_execution_gate", provider["bottleneck"])
        self.assertEqual("token_hotspot", budget["bottleneck"])

    def test_low_risk_ready_plan_ranks_below_true_gated_plan(self):
        triage = build_portfolio_triage(
            [
                plan("ready", "ready_for_review", notes=[], effective_risk="low"),
                plan("provider", "needs_approval", gates=["provider_integration_gate"], task_type="provider"),
            ]
        )

        self.assertEqual("provider", triage["items"][0]["plan_id"])

    def test_triage_output_is_non_executable(self):
        triage = build_portfolio_triage([plan("clean", "ready_for_review")])

        self.assertTrue(triage["generated_from_store_only"])
        self.assertFalse(triage["persistent"])
        self.assertTrue(triage["non_executable"])
        self.assertFalse(triage["items"][0]["executable"])

    def test_triage_does_not_mutate_input_plans(self):
        plans = [plan("clean", "ready_for_review")]
        before = deepcopy(plans)

        build_portfolio_triage(plans)

        self.assertEqual(before, plans)

    def test_limit_validation(self):
        self.assertEqual(1, validate_triage_limit(1))
        with self.assertRaises(PlanTriageError):
            validate_triage_limit(0)
        with self.assertRaises(PlanTriageError):
            validate_triage_limit(101)


if __name__ == "__main__":
    unittest.main()
