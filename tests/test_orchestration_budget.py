"""Tests for orchestration/multi_agent_budget.py — multi-level budget enforcement."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.orchestration.multi_agent_budget import MultiAgentBudgetManager


class MultiAgentBudgetManagerTests(unittest.TestCase):
    def setUp(self):
        self.budget = MultiAgentBudgetManager()

    def test_create_workflow_budget(self):
        wf_id = self.budget.create_workflow_budget("w1", 10.0)
        self.assertEqual(wf_id, "w1")

    def test_reserve_node_budget_within_limit(self):
        self.budget.create_workflow_budget("w1", 10.0)
        result = self.budget.reserve_node_budget("w1", "n1", "agent-1", 5.0)
        self.assertTrue(result)

    def test_reserve_node_budget_exceeds_workflow_limit(self):
        self.budget.create_workflow_budget("w1", 5.0)
        result = self.budget.reserve_node_budget("w1", "n1", "agent-1", 10.0)
        self.assertFalse(result)

    def test_reserve_node_budget_exceeds_agent_limit(self):
        self.budget.create_workflow_budget("w1", 100.0)
        self.budget.set_agent_limit("w1", "agent-1", 3.0)
        result = self.budget.reserve_node_budget("w1", "n1", "agent-1", 5.0)
        self.assertFalse(result)

    def test_reserve_node_budget_unknown_workflow(self):
        result = self.budget.reserve_node_budget("unknown", "n1", "agent-1", 1.0)
        self.assertFalse(result)

    def test_check_workflow_budget_within(self):
        self.budget.create_workflow_budget("w1", 10.0)
        ok, msg = self.budget.check_workflow_budget("w1")
        self.assertTrue(ok)
        self.assertIsNone(msg)

    def test_check_workflow_budget_exceeded(self):
        self.budget.create_workflow_budget("w1", 5.0)
        self.budget.record_cost("w1", "n1", "agent-1", 6.0)
        ok, msg = self.budget.check_workflow_budget("w1")
        self.assertFalse(ok)
        self.assertIn("workflow_budget_exceeded", msg)

    def test_check_agent_budget_within(self):
        self.budget.create_workflow_budget("w1", 100.0)
        self.budget.set_agent_limit("w1", "agent-1", 10.0)
        self.budget.record_cost("w1", "n1", "agent-1", 5.0)
        ok, msg = self.budget.check_agent_budget("w1", "agent-1")
        self.assertTrue(ok)

    def test_check_agent_budget_exceeded(self):
        self.budget.create_workflow_budget("w1", 100.0)
        self.budget.set_agent_limit("w1", "agent-1", 3.0)
        self.budget.record_cost("w1", "n1", "agent-1", 5.0)
        ok, msg = self.budget.check_agent_budget("w1", "agent-1")
        self.assertFalse(ok)
        self.assertIn("agent_budget_exceeded", msg)

    def test_get_workflow_cost(self):
        self.budget.create_workflow_budget("w1", 10.0)
        self.budget.record_cost("w1", "n1", "agent-1", 3.0)
        self.budget.record_cost("w1", "n2", "agent-2", 2.0)
        self.assertAlmostEqual(self.budget.get_workflow_cost("w1"), 5.0)

    def test_get_agent_cost(self):
        self.budget.create_workflow_budget("w1", 100.0)
        self.budget.record_cost("w1", "n1", "agent-1", 3.0)
        self.budget.record_cost("w1", "n2", "agent-1", 2.0)
        self.assertAlmostEqual(self.budget.get_agent_cost("w1", "agent-1"), 5.0)

    def test_get_workflow_cost_unknown(self):
        self.assertEqual(self.budget.get_workflow_cost("unknown"), 0.0)

    def test_get_agent_cost_unknown(self):
        self.assertEqual(self.budget.get_agent_cost("unknown", "agent-1"), 0.0)

    def test_record_cost_unknown_workflow_no_crash(self):
        self.budget.record_cost("unknown", "n1", "agent-1", 1.0)

    def test_overrun_strategy_default(self):
        self.assertEqual(self.budget.overrun_strategy, "cancel")


if __name__ == "__main__":
    unittest.main()
