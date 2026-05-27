"""Tests for routing/auto_policies.py — downgrade and upgrade policies."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.routing.auto_policies import AutoDowngradePolicy, AutoUpgradePolicy
from harness_core.dispatch.routing.history_store import RoutingHistoryStore
from harness_core.usage_ledger import UsageLedgerRow


def _make_row(profile_id: str, cost_group: str, passed: bool) -> UsageLedgerRow:
    return UsageLedgerRow(
        run_id="r", case_id="c",
        input_tokens=100, output_tokens=50, cached_tokens=0,
        request_count=1, tool_call_count=0, retry_count=0,
        wall_clock_ms=100, estimated_cost=0.01, pass_=passed,
        cost_of_pass_group=cost_group,
        model_profile_id=profile_id, context_pack_id="",
    )


class AutoDowngradePolicyTests(unittest.TestCase):
    def setUp(self):
        self.policy = AutoDowngradePolicy(policy_id="down-v1", min_sample_count=30)
        self.store = RoutingHistoryStore(tier_profile_map={"p1": "cheap_executor", "p2": "balanced_worker"})
        for i in range(35):
            self.store.add_row(_make_row("p1", "s/c/r/q", True))
            self.store.add_row(_make_row("p2", "s/c/r/q", True))

    def test_downgrades_when_quality_high_and_cheaper(self):
        should, reason = self.policy.should_downgrade(
            "c_r", "balanced_worker", "cheap_executor",
            quality_score=0.85, cost_of_pass=0.005, history_store=self.store,
        )
        self.assertTrue(should)
        self.assertEqual(reason, "cost_optimization")

    def test_rejects_when_quality_low(self):
        should, reason = self.policy.should_downgrade(
            "c_r", "balanced_worker", "cheap_executor",
            quality_score=0.5, cost_of_pass=0.005, history_store=self.store,
        )
        self.assertFalse(should)
        self.assertEqual(reason, "quality_score_below_threshold")

    def test_rejects_when_candidate_not_cheaper(self):
        should, reason = self.policy.should_downgrade(
            "c_r", "cheap_executor", "balanced_worker",
            quality_score=0.85, cost_of_pass=0.015, history_store=self.store,
        )
        self.assertFalse(should)
        self.assertEqual(reason, "candidate_not_cheaper")

    def test_rejects_when_insufficient_samples(self):
        empty_store = RoutingHistoryStore()
        should, reason = self.policy.should_downgrade(
            "c_r", "balanced_worker", "cheap_executor",
            quality_score=0.85, cost_of_pass=0.005, history_store=empty_store,
        )
        self.assertFalse(should)
        self.assertEqual(reason, "insufficient_samples")


class AutoUpgradePolicyTests(unittest.TestCase):
    def setUp(self):
        self.policy = AutoUpgradePolicy(policy_id="up-v1")
        self.store = RoutingHistoryStore()

    def test_upgrades_on_critical_task(self):
        should, reason = self.policy.should_upgrade(
            "c_r", "cheap_executor", "strong_planner",
            quality_score=0.9, failure_rate=0.0, risk_level="critical",
            history_store=self.store,
        )
        self.assertTrue(should)
        self.assertEqual(reason, "critical_task")

    def test_upgrades_on_high_failure_rate(self):
        should, reason = self.policy.should_upgrade(
            "c_r", "cheap_executor", "balanced_worker",
            quality_score=0.9, failure_rate=0.3, risk_level="low",
            history_store=self.store,
        )
        self.assertTrue(should)
        self.assertEqual(reason, "failure_rate")

    def test_upgrades_on_high_uncertainty(self):
        should, reason = self.policy.should_upgrade(
            "c_r", "cheap_executor", "balanced_worker",
            quality_score=0.3, failure_rate=0.0, risk_level="low",
            history_store=self.store,
        )
        self.assertTrue(should)
        self.assertEqual(reason, "high_uncertainty")

    def test_no_upgrade_when_quality_fine(self):
        should, reason = self.policy.should_upgrade(
            "c_r", "cheap_executor", "balanced_worker",
            quality_score=0.8, failure_rate=0.05, risk_level="low",
            history_store=self.store,
        )
        self.assertFalse(should)
        self.assertEqual(reason, "no_upgrade_needed")

    def test_no_upgrade_when_candidate_not_stronger(self):
        should, reason = self.policy.should_upgrade(
            "c_r", "balanced_worker", "cheap_executor",
            quality_score=0.3, failure_rate=0.5, risk_level="low",
            history_store=self.store,
        )
        self.assertFalse(should)
        self.assertEqual(reason, "candidate_not_stronger")


if __name__ == "__main__":
    unittest.main()
