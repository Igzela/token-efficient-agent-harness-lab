"""Tests for orchestration/human_approval_gate.py — approval gate logic."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.orchestration.human_approval_gate import HumanApprovalGate
from harness_core.dispatch.orchestration.schemas import WorkflowGraph, WorkflowNode


def _make_node(node_id="n1", status="completed", budget=100.0, cost=0.0):
    return WorkflowNode(
        node_id=node_id, workflow_id="w1", task_type="t",
        assigned_agent_id=None, status=status, budget=budget, cost_incurred=cost,
    )


def _make_graph(nodes):
    return WorkflowGraph(workflow_id="w1", dispatch_id="d1", nodes=tuple(nodes))


class RequiresApprovalTests(unittest.TestCase):
    def test_failed_node_requires_approval(self):
        gate = HumanApprovalGate()
        node = _make_node(status="failed")
        graph = _make_graph([node])
        self.assertTrue(gate.requires_approval(graph, node))

    def test_completed_node_under_threshold_no_approval(self):
        gate = HumanApprovalGate(risk_threshold=0.7)
        node = _make_node(budget=100.0, cost=50.0)
        graph = _make_graph([node])
        self.assertFalse(gate.requires_approval(graph, node))

    def test_completed_node_over_threshold_requires_approval(self):
        gate = HumanApprovalGate(risk_threshold=0.7)
        node = _make_node(budget=100.0, cost=80.0)
        graph = _make_graph([node])
        self.assertTrue(gate.requires_approval(graph, node))

    def test_cost_exactly_at_threshold_no_approval(self):
        gate = HumanApprovalGate(risk_threshold=0.7)
        node = _make_node(budget=100.0, cost=70.0)
        graph = _make_graph([node])
        self.assertFalse(gate.requires_approval(graph, node))

    def test_cost_just_above_threshold_requires_approval(self):
        gate = HumanApprovalGate(risk_threshold=0.7)
        node = _make_node(budget=100.0, cost=70.01)
        graph = _make_graph([node])
        self.assertTrue(gate.requires_approval(graph, node))

    def test_zero_budget_node_no_approval(self):
        gate = HumanApprovalGate()
        node = _make_node(budget=0.0, cost=999.0, status="completed")
        graph = _make_graph([node])
        self.assertFalse(gate.requires_approval(graph, node))

    def test_running_node_no_approval(self):
        gate = HumanApprovalGate()
        node = _make_node(status="running")
        graph = _make_graph([node])
        self.assertFalse(gate.requires_approval(graph, node))

    def test_waiting_human_node_no_approval(self):
        gate = HumanApprovalGate()
        node = _make_node(status="waiting_human")
        graph = _make_graph([node])
        self.assertFalse(gate.requires_approval(graph, node))

    def test_zero_threshold_triggers_on_any_cost(self):
        gate = HumanApprovalGate(risk_threshold=0.0)
        node = _make_node(budget=100.0, cost=0.01)
        graph = _make_graph([node])
        self.assertTrue(gate.requires_approval(graph, node))

    def test_negative_cost_no_approval(self):
        gate = HumanApprovalGate(risk_threshold=0.7)
        node = _make_node(budget=100.0, cost=-10.0)
        graph = _make_graph([node])
        self.assertFalse(gate.requires_approval(graph, node))


class ApproveRejectConflictTests(unittest.TestCase):
    def test_approve_then_reject_returns_false(self):
        gate = HumanApprovalGate()
        self.assertTrue(gate.approve("n1"))
        self.assertFalse(gate.reject("n1", "changed mind"))

    def test_reject_then_approve_returns_false(self):
        gate = HumanApprovalGate()
        self.assertTrue(gate.reject("n1", "bad output"))
        self.assertFalse(gate.approve("n1"))

    def test_approve_twice_returns_true(self):
        gate = HumanApprovalGate()
        self.assertTrue(gate.approve("n1"))
        self.assertTrue(gate.approve("n1"))

    def test_reject_twice_returns_true(self):
        gate = HumanApprovalGate()
        self.assertTrue(gate.reject("n1", "r1"))
        self.assertTrue(gate.reject("n1", "r2"))

    def test_reject_then_approve_does_not_change_state(self):
        gate = HumanApprovalGate()
        gate.reject("n1", "reason")
        result = gate.approve("n1")
        self.assertFalse(result)
        self.assertFalse(gate.is_approved("n1"))
        self.assertTrue(gate.is_rejected("n1"))


class ApprovalStateQueryTests(unittest.TestCase):
    def test_is_approved_after_approve(self):
        gate = HumanApprovalGate()
        gate.approve("n1")
        self.assertTrue(gate.is_approved("n1"))
        self.assertFalse(gate.is_rejected("n1"))

    def test_is_rejected_after_reject(self):
        gate = HumanApprovalGate()
        gate.reject("n1", "reason")
        self.assertTrue(gate.is_rejected("n1"))
        self.assertFalse(gate.is_approved("n1"))

    def test_unknown_node_not_approved(self):
        gate = HumanApprovalGate()
        self.assertFalse(gate.is_approved("unknown"))
        self.assertFalse(gate.is_rejected("unknown"))

    def test_rejection_reason_after_reject(self):
        gate = HumanApprovalGate()
        gate.reject("n1", "quality too low")
        self.assertEqual(gate.rejection_reason("n1"), "quality too low")

    def test_rejection_reason_for_unknown_node(self):
        gate = HumanApprovalGate()
        self.assertIsNone(gate.rejection_reason("unknown"))

    def test_rejection_reason_for_approved_node(self):
        gate = HumanApprovalGate()
        gate.approve("n1")
        self.assertIsNone(gate.rejection_reason("n1"))


class ApprovedNodeSkipsApprovalTests(unittest.TestCase):
    def test_approved_node_does_not_require_approval(self):
        gate = HumanApprovalGate()
        node = _make_node(status="failed")
        graph = _make_graph([node])
        gate.approve("n1")
        self.assertFalse(gate.requires_approval(graph, node))

    def test_rejected_node_does_not_require_approval(self):
        gate = HumanApprovalGate()
        node = _make_node(status="failed")
        graph = _make_graph([node])
        gate.reject("n1", "rejected")
        self.assertFalse(gate.requires_approval(graph, node))

    def test_approved_over_budget_node_no_approval(self):
        gate = HumanApprovalGate(risk_threshold=0.7)
        node = _make_node(budget=100.0, cost=90.0)
        graph = _make_graph([node])
        gate.approve("n1")
        self.assertFalse(gate.requires_approval(graph, node))


if __name__ == "__main__":
    unittest.main()
