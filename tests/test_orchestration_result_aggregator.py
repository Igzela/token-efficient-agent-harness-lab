"""Tests for orchestration/result_aggregator.py — workflow result aggregation."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.orchestration.result_aggregator import ResultAggregator
from harness_core.dispatch.orchestration.schemas import WorkflowGraph, WorkflowNode


def _make_node(node_id="n1", status="completed", output_ref="out-1", cost=0.0, error=None):
    return WorkflowNode(
        node_id=node_id, workflow_id="w1", task_type="t",
        assigned_agent_id=None, status=status, output_ref=output_ref,
        cost_incurred=cost, error=error,
    )


def _make_graph(nodes, workflow_id="w1", dispatch_id="d1"):
    return WorkflowGraph(workflow_id=workflow_id, dispatch_id=dispatch_id, nodes=tuple(nodes))


class ResultAggregatorIsCompleteTests(unittest.TestCase):
    def setUp(self):
        self.agg = ResultAggregator()

    def test_empty_graph_is_complete(self):
        graph = _make_graph([])
        self.assertTrue(self.agg.is_complete(graph))

    def test_all_completed_is_complete(self):
        graph = _make_graph([_make_node("n1"), _make_node("n2")])
        self.assertTrue(self.agg.is_complete(graph))

    def test_all_failed_is_complete(self):
        graph = _make_graph([_make_node("n1", status="failed"), _make_node("n2", status="failed")])
        self.assertTrue(self.agg.is_complete(graph))

    def test_all_cancelled_is_complete(self):
        graph = _make_graph([_make_node("n1", status="cancelled")])
        self.assertTrue(self.agg.is_complete(graph))

    def test_mixed_terminal_is_complete(self):
        graph = _make_graph([
            _make_node("n1", status="completed"),
            _make_node("n2", status="failed"),
            _make_node("n3", status="cancelled"),
        ])
        self.assertTrue(self.agg.is_complete(graph))

    def test_running_node_not_complete(self):
        graph = _make_graph([_make_node("n1", status="completed"), _make_node("n2", status="running")])
        self.assertFalse(self.agg.is_complete(graph))

    def test_pending_node_not_complete(self):
        graph = _make_graph([_make_node("n1", status="pending")])
        self.assertFalse(self.agg.is_complete(graph))

    def test_ready_node_not_complete(self):
        graph = _make_graph([_make_node("n1", status="ready")])
        self.assertFalse(self.agg.is_complete(graph))

    def test_waiting_human_node_not_complete(self):
        graph = _make_graph([_make_node("n1", status="waiting_human")])
        self.assertFalse(self.agg.is_complete(graph))


class ResultAggregatorAggregateTests(unittest.TestCase):
    def setUp(self):
        self.agg = ResultAggregator()

    def test_aggregate_empty_graph(self):
        graph = _make_graph([])
        result = self.agg.aggregate(graph)
        self.assertEqual(result["total_nodes"], 0)
        self.assertEqual(result["completed_nodes"], 0)
        self.assertEqual(result["failed_nodes"], 0)
        self.assertEqual(result["total_cost"], 0.0)
        self.assertEqual(result["node_results"], {})

    def test_aggregate_single_completed_node(self):
        graph = _make_graph([_make_node("n1", cost=1.5)])
        result = self.agg.aggregate(graph)
        self.assertEqual(result["total_nodes"], 1)
        self.assertEqual(result["completed_nodes"], 1)
        self.assertEqual(result["failed_nodes"], 0)
        self.assertEqual(result["total_cost"], 1.5)
        self.assertIn("n1", result["node_results"])
        self.assertEqual(result["node_results"]["n1"]["status"], "completed")

    def test_aggregate_mixed_statuses(self):
        graph = _make_graph([
            _make_node("n1", status="completed", cost=2.0),
            _make_node("n2", status="failed", error="broke", cost=0.5),
            _make_node("n3", status="completed", cost=1.0),
        ])
        result = self.agg.aggregate(graph)
        self.assertEqual(result["total_nodes"], 3)
        self.assertEqual(result["completed_nodes"], 2)
        self.assertEqual(result["failed_nodes"], 1)
        self.assertEqual(result["total_cost"], 3.5)

    def test_aggregate_preserves_workflow_metadata(self):
        graph = _make_graph([_make_node()], workflow_id="wf-abc", dispatch_id="disp-xyz")
        result = self.agg.aggregate(graph)
        self.assertEqual(result["workflow_id"], "wf-abc")
        self.assertEqual(result["dispatch_id"], "disp-xyz")

    def test_aggregate_includes_error_field(self):
        graph = _make_graph([_make_node("n1", status="failed", error="timeout")])
        result = self.agg.aggregate(graph)
        self.assertEqual(result["node_results"]["n1"]["error"], "timeout")

    def test_aggregate_zero_cost_nodes(self):
        graph = _make_graph([_make_node("n1", cost=0.0), _make_node("n2", cost=0.0)])
        result = self.agg.aggregate(graph)
        self.assertEqual(result["total_cost"], 0.0)

    def test_aggregate_with_cancelled_node(self):
        graph = _make_graph([
            _make_node("n1", status="completed", cost=2.0),
            _make_node("n2", status="failed", cost=0.5),
            _make_node("n3", status="cancelled", cost=1.0),
        ])
        result = self.agg.aggregate(graph)
        self.assertEqual(result["total_nodes"], 3)
        self.assertEqual(result["completed_nodes"], 1)
        self.assertEqual(result["failed_nodes"], 1)
        self.assertEqual(result["total_cost"], 3.5)

    def test_aggregate_includes_agent_id(self):
        node = WorkflowNode(
            node_id="n1", workflow_id="w1", task_type="t",
            assigned_agent_id="agent-42", status="completed",
        )
        graph = _make_graph([node])
        result = self.agg.aggregate(graph)
        self.assertEqual(result["node_results"]["n1"]["agent_id"], "agent-42")


if __name__ == "__main__":
    unittest.main()
