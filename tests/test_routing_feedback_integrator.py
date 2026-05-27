"""Tests for routing/feedback_integrator.py — quality→routing feedback."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.routing.auto_policies import AutoDowngradePolicy, AutoUpgradePolicy
from harness_core.dispatch.routing.feedback_integrator import FeedbackIntegrator
from harness_core.dispatch.routing.history_store import RoutingHistoryStore
from harness_core.dispatch.routing.promotion_gate import RoutingObservationStore
from harness_core.usage_ledger import UsageLedgerRow


def _make_row(profile_id: str, cost_group: str, cost: float, passed: bool) -> UsageLedgerRow:
    return UsageLedgerRow(
        run_id="r", case_id="c",
        input_tokens=100, output_tokens=50, cached_tokens=0,
        request_count=1, tool_call_count=0, retry_count=0,
        wall_clock_ms=100, estimated_cost=cost, pass_=passed,
        cost_of_pass_group=cost_group,
        model_profile_id=profile_id, context_pack_id="",
    )


class FeedbackIntegratorTests(unittest.TestCase):
    def setUp(self):
        self.history = RoutingHistoryStore(tier_profile_map={"p1": "cheap_executor", "p2": "balanced_worker"})
        self.obs_store = RoutingObservationStore()
        self.integrator = FeedbackIntegrator(self.history, self.obs_store)

    def test_record_outcome_creates_observation(self):
        obs = self.integrator.record_outcome(
            dispatch_id="d1", task_domain="code", task_intent="review",
            selected_tier="cheap_executor", baseline_tier="balanced_worker",
            quality_score=0.85, cost=0.005, latency_ms=100, success=True,
        )
        self.assertEqual(obs.selected_tier, "cheap_executor")
        self.assertEqual(self.obs_store.total_count(), 1)

    def test_should_adapt_no_observations(self):
        should, reason = self.integrator.should_adapt("code_review", "cheap_executor")
        self.assertFalse(should)
        self.assertEqual(reason, "no_observations")

    def test_should_adapt_no_change_needed(self):
        for i in range(5):
            self.integrator.record_outcome(
                dispatch_id=f"d{i}", task_domain="code", task_intent="review",
                selected_tier="cheap_executor", baseline_tier="balanced_worker",
                quality_score=0.9, cost=0.005, latency_ms=100, success=True,
            )
        should, reason = self.integrator.should_adapt("code_review", "cheap_executor")
        self.assertFalse(should)

    def test_should_adapt_triggers_upgrade_on_failure(self):
        for i in range(10):
            self.integrator.record_outcome(
                dispatch_id=f"d{i}", task_domain="code", task_intent="review",
                selected_tier="cheap_executor", baseline_tier="balanced_worker",
                quality_score=0.3, cost=0.005, latency_ms=100, success=i < 3,
            )
        integrator = FeedbackIntegrator(
            self.history, self.obs_store,
            auto_upgrade=AutoUpgradePolicy(policy_id="up-v1"),
        )
        should, reason = integrator.should_adapt("code_review", "cheap_executor")
        self.assertTrue(should)
        self.assertIn("upgrade", reason)

    def test_task_group_for(self):
        tg = FeedbackIntegrator.task_group_for("code", "review")
        self.assertEqual(tg, "code_review")

    def test_summary_empty(self):
        s = self.integrator.summary("code_review")
        self.assertEqual(s["sample_count"], 0)
        self.assertEqual(s["tiers"], [])

    def test_summary_with_data(self):
        for i in range(5):
            self.integrator.record_outcome(
                dispatch_id=f"d{i}", task_domain="code", task_intent="review",
                selected_tier="cheap_executor", baseline_tier="balanced_worker",
                quality_score=0.9, cost=0.005, latency_ms=100, success=True,
            )
        s = self.integrator.summary("code_review")
        self.assertEqual(s["sample_count"], 5)
        self.assertIn("cheap_executor", s["tiers"])
        self.assertEqual(s["best_tier"], "cheap_executor")


if __name__ == "__main__":
    unittest.main()
