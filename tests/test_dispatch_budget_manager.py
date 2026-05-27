"""Tests for budget_manager.py."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.budget_manager import BudgetManager
from harness_core.dispatch.task_analyzer import RuleBasedTaskAnalyzer


def make_analysis(request="Summarize the README"):
    return RuleBasedTaskAnalyzer().analyze(request)


class BudgetManagerTests(unittest.TestCase):
    def setUp(self):
        self.bm = BudgetManager()

    def test_create_reservation(self):
        analysis = make_analysis()
        r = self.bm.create_reservation("dec-001", analysis, "balanced_worker")
        self.assertEqual(r.status, "reserved")
        self.assertEqual(r.decision_id, "dec-001")
        self.assertGreater(r.reserved_total_tokens, 0)

    def test_reservation_has_token_breakdown(self):
        analysis = make_analysis()
        r = self.bm.create_reservation("dec-001", analysis, "balanced_worker")
        self.assertEqual(r.reserved_total_tokens, r.reserved_input_tokens + r.reserved_output_tokens)

    def test_check_violation_within_budget(self):
        analysis = make_analysis()
        r = self.bm.create_reservation("dec-001", analysis, "balanced_worker")
        violated, reason = self.bm.check_violation(r, r.reserved_total_tokens - 1)
        self.assertFalse(violated)

    def test_check_violation_exceeds_budget(self):
        analysis = make_analysis()
        r = self.bm.create_reservation("dec-001", analysis, "balanced_worker")
        violated, reason = self.bm.check_violation(r, r.reserved_total_tokens + 100)
        self.assertTrue(violated)
        self.assertIn("exceeded", reason)

    def test_estimate_cost_positive(self):
        cost = self.bm.estimate_cost("balanced_worker", 1000, 500)
        self.assertGreater(cost, 0)

    def test_estimate_cost_cheaper_tier(self):
        cheap = self.bm.estimate_cost("cheap_executor", 1000, 500)
        expensive = self.bm.estimate_cost("strong_planner", 1000, 500)
        self.assertLess(cheap, expensive)

    def test_default_currency(self):
        analysis = make_analysis()
        r = self.bm.create_reservation("dec-001", analysis, "balanced_worker")
        self.assertEqual(r.currency, "token")

    def test_reservation_cost_rounded(self):
        analysis = make_analysis()
        r = self.bm.create_reservation("dec-001", analysis, "balanced_worker")
        # Should be rounded to 6 decimal places
        self.assertEqual(r.reserved_cost, round(r.reserved_cost, 6))


if __name__ == "__main__":
    unittest.main()
