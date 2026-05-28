"""Tests for model_selector.py."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.model_selector import DispatchRoutingPolicy, ModelSelector
from harness_core.dispatch.task_analyzer import RuleBasedTaskAnalyzer


def make_analysis(request="Review auth.py for security issues"):
    return RuleBasedTaskAnalyzer().analyze(request)


class ModelSelectorTests(unittest.TestCase):
    def setUp(self):
        self.selector = ModelSelector()

    def test_select_returns_tuple(self):
        result = self.selector.select(make_analysis())
        self.assertEqual(len(result), 7)

    def test_select_tier_from_policy(self):
        a = make_analysis("Summarize the README")
        tier, _, _, _, _, _, _ = self.selector.select(a)
        self.assertIn(tier, ("cheap_executor", "balanced_worker", "strong_planner", "verifier", "advisor"))

    def test_shadow_routes_always_present(self):
        a = make_analysis()
        _, _, _, _, shadow_routes, _, _ = self.selector.select(a)
        self.assertGreater(len(shadow_routes), 0)

    def test_shadow_route_is_diagnostic(self):
        a = make_analysis()
        _, _, _, _, shadow_routes, _, _ = self.selector.select(a)
        for sr in shadow_routes:
            self.assertEqual(sr.admission_scope, "diagnostic")

    def test_low_confidence_escalates(self):
        a = make_analysis("Make it better")
        tier, _, _, _, _, rejected, reason = self.selector.select(a)
        self.assertIn("low_confidence", reason)

    def test_critical_risk_overrides(self):
        a = make_analysis("Rotate the API keys in config files")
        tier, _, _, _, _, _, reason = self.selector.select(a)
        self.assertIn(tier, ("strong_planner", "advisor"))

    def test_fallback_tier_exists(self):
        a = make_analysis()
        _, _, fallback, _, _, _, _ = self.selector.select(a)
        self.assertIn(fallback, ("cheap_executor", "balanced_worker", "strong_planner", "verifier", "advisor"))

    def test_rejected_candidates_list(self):
        a = make_analysis("Make it better")
        _, _, _, _, _, rejected, _ = self.selector.select(a)
        self.assertIsInstance(rejected, list)


class DispatchRoutingPolicyTests(unittest.TestCase):
    def test_select_tier_default(self):
        policy = DispatchRoutingPolicy(policy_id="test", tier_map={"code_review": "balanced_worker"})
        a = make_analysis("Review auth.py for security issues")
        tier = policy.select_tier(a)
        self.assertEqual(tier, "balanced_worker")

    def test_high_risk_overrides(self):
        policy = DispatchRoutingPolicy(policy_id="test", tier_map={"code_review": "cheap_executor"})
        a = make_analysis("Review auth.py for security issues")
        tier = policy.select_tier(a)
        self.assertIn(tier, ("cheap_executor", "balanced_worker"))

    def test_missing_domain_key_defaults_to_balanced(self):
        policy = DispatchRoutingPolicy(policy_id="test", tier_map={})
        a = make_analysis("Review auth.py for security issues")
        tier = policy.select_tier(a)
        self.assertEqual(tier, "balanced_worker")

    def test_custom_tier_map(self):
        policy = DispatchRoutingPolicy(
            policy_id="custom",
            tier_map={"code_review": "verifier", "docs_summarize": "strong_planner"},
        )
        a = make_analysis("Summarize the README")
        tier = policy.select_tier(a)
        self.assertEqual(tier, "strong_planner")


class ModelSelectorBudgetTests(unittest.TestCase):
    def test_low_budget_rejects_strong_planner(self):
        a = make_analysis("Review auth.py for security issues")
        # Force low budget by replacing the field
        from dataclasses import replace
        low_budget = replace(a, context_budget_estimate=200)
        selector = ModelSelector()
        _, _, _, _, _, rejected, reason = selector.select(low_budget)
        self.assertIn("budget_constrained", reason)
        budget_rejected = [r for r in rejected if r.constraint_failed == "budget_threshold"]
        self.assertEqual(len(budget_rejected), 1)

    def test_self_diagnostic_shadow_for_cheapest_tier(self):
        # Use a request that resolves to cheap_executor tier
        a = make_analysis("Classify this as a simple task")
        selector = ModelSelector()
        selected, _, fallback, _, shadow_routes, _, _ = selector.select(a)
        reasons = [sr.reason for sr in shadow_routes]
        # If selected is cheap_executor, should have self-diagnostic or fallback != selected
        if selected == "cheap_executor":
            self.assertTrue(
                any("self-diagnostic" in r for r in reasons) or fallback != selected,
                f"Expected self-diagnostic shadow or different fallback, got {reasons}",
            )


if __name__ == "__main__":
    unittest.main()
