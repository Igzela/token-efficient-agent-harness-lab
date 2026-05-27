"""Tests for routing/cost_of_pass_router.py — cost-based routing decisions."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.routing.cost_of_pass_router import CostOfPassRouter
from harness_core.dispatch.routing.history_store import RoutingHistoryStore
from harness_core.usage_ledger import UsageLedgerRow


def _make_row(profile_id: str, cost_group: str, cost: float, passed: bool) -> UsageLedgerRow:
    return UsageLedgerRow(
        run_id="run-1", case_id="case-1",
        input_tokens=100, output_tokens=50, cached_tokens=0,
        request_count=1, tool_call_count=0, retry_count=0,
        wall_clock_ms=100, estimated_cost=cost, pass_=passed,
        cost_of_pass_group=cost_group,
        model_profile_id=profile_id, context_pack_id="",
    )


class CostOfPassRouterTests(unittest.TestCase):
    def setUp(self):
        self.store = RoutingHistoryStore(tier_profile_map={
            "cheap-p": "cheap_executor",
            "bal-p": "balanced_worker",
        })
        for i in range(35):
            self.store.add_row(_make_row("cheap-p", "s/c/r/q", 0.005, True))
            self.store.add_row(_make_row("bal-p", "s/c/r/q", 0.015, True))
        self.router = CostOfPassRouter(self.store, min_sample_count=30)

    def test_best_tier_returns_cheapest(self):
        result = self.router.best_tier_for_task_group("c/r")
        self.assertIsNotNone(result)
        tier, cop = result
        self.assertEqual(tier, "cheap_executor")
        self.assertLess(cop, 0.01)

    def test_best_tier_none_when_insufficient_samples(self):
        store = RoutingHistoryStore(tier_profile_map={"p1": "cheap_executor"})
        for i in range(5):
            store.add_row(_make_row("p1", "s/x/y/z", 0.01, True))
        router = CostOfPassRouter(store, min_sample_count=30)
        self.assertIsNone(router.best_tier_for_task_group("x/y"))

    def test_can_route_adaptively(self):
        self.assertTrue(self.router.can_route_adaptively("c/r"))
        self.assertFalse(self.router.can_route_adaptively("nonexistent_group"))

    def test_cost_comparison(self):
        result = self.router.cost_comparison("c/r", "cheap_executor", "balanced_worker")
        self.assertIsNotNone(result)
        cop_a, cop_b, delta_pct = result
        self.assertLess(cop_a, cop_b)
        self.assertLess(delta_pct, 0)

    def test_cost_comparison_none_when_missing(self):
        result = self.router.cost_comparison("c/r", "cheap_executor", "nonexistent")
        self.assertIsNone(result)

    def test_failure_rate(self):
        for i in range(5):
            self.store.add_row(_make_row("cheap-p", "s/f/g/h", 0.01, i < 3))
        rate = self.router.failure_rate("cheap_executor", "f/g")
        self.assertAlmostEqual(rate, 0.4, places=1)

    def test_failure_rate_zero_for_no_data(self):
        rate = self.router.failure_rate("cheap_executor", "nonexistent")
        self.assertEqual(rate, 0.0)

    def test_tier_cost_of_pass(self):
        cop = self.router.tier_cost_of_pass("cheap_executor", "c/r")
        self.assertIsNotNone(cop)
        self.assertGreater(cop, 0)


if __name__ == "__main__":
    unittest.main()
