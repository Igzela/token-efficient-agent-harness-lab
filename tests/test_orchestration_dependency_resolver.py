"""Tests for orchestration/dependency_resolver.py — graph validation and topological sort."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.orchestration.dependency_resolver import DependencyResolver
from harness_core.dispatch.orchestration.schemas import WorkflowEdge, WorkflowGraph, WorkflowNode


def _make_graph(nodes, edges):
    return WorkflowGraph(
        workflow_id="w1", dispatch_id="d1",
        nodes=tuple(nodes), edges=tuple(edges),
    )


class DependencyResolverValidateTests(unittest.TestCase):
    def test_valid_graph(self):
        n1 = WorkflowNode(node_id="n1", workflow_id="w1", task_type="a", assigned_agent_id=None)
        n2 = WorkflowNode(node_id="n2", workflow_id="w1", task_type="b", assigned_agent_id=None)
        e = WorkflowEdge(edge_id="e1", from_node_id="n1", to_node_id="n2")
        resolver = DependencyResolver()
        valid, errors = resolver.validate(_make_graph([n1, n2], [e]))
        self.assertTrue(valid)
        self.assertEqual(errors, [])

    def test_missing_source(self):
        n2 = WorkflowNode(node_id="n2", workflow_id="w1", task_type="b", assigned_agent_id=None)
        e = WorkflowEdge(edge_id="e1", from_node_id="missing", to_node_id="n2")
        resolver = DependencyResolver()
        valid, errors = resolver.validate(_make_graph([n2], [e]))
        self.assertFalse(valid)
        self.assertIn("missing_source:missing", errors)

    def test_missing_target(self):
        n1 = WorkflowNode(node_id="n1", workflow_id="w1", task_type="a", assigned_agent_id=None)
        e = WorkflowEdge(edge_id="e1", from_node_id="n1", to_node_id="missing")
        resolver = DependencyResolver()
        valid, errors = resolver.validate(_make_graph([n1], [e]))
        self.assertFalse(valid)
        self.assertIn("missing_target:missing", errors)

    def test_cycle_detection(self):
        n1 = WorkflowNode(node_id="n1", workflow_id="w1", task_type="a", assigned_agent_id=None)
        n2 = WorkflowNode(node_id="n2", workflow_id="w1", task_type="b", assigned_agent_id=None)
        e1 = WorkflowEdge(edge_id="e1", from_node_id="n1", to_node_id="n2")
        e2 = WorkflowEdge(edge_id="e2", from_node_id="n2", to_node_id="n1")
        resolver = DependencyResolver()
        valid, errors = resolver.validate(_make_graph([n1, n2], [e1, e2]))
        self.assertFalse(valid)
        self.assertIn("cycle_detected", errors)


class DependencyResolverExecutionOrderTests(unittest.TestCase):
    def test_linear_chain(self):
        n1 = WorkflowNode(node_id="n1", workflow_id="w1", task_type="a", assigned_agent_id=None)
        n2 = WorkflowNode(node_id="n2", workflow_id="w1", task_type="b", assigned_agent_id=None)
        n3 = WorkflowNode(node_id="n3", workflow_id="w1", task_type="c", assigned_agent_id=None)
        e1 = WorkflowEdge(edge_id="e1", from_node_id="n1", to_node_id="n2")
        e2 = WorkflowEdge(edge_id="e2", from_node_id="n2", to_node_id="n3")
        resolver = DependencyResolver()
        waves = resolver.execution_order(_make_graph([n1, n2, n3], [e1, e2]))
        self.assertEqual(waves, [["n1"], ["n2"], ["n3"]])

    def test_parallel_tasks(self):
        n1 = WorkflowNode(node_id="n1", workflow_id="w1", task_type="a", assigned_agent_id=None)
        n2 = WorkflowNode(node_id="n2", workflow_id="w1", task_type="b", assigned_agent_id=None)
        n3 = WorkflowNode(node_id="n3", workflow_id="w1", task_type="c", assigned_agent_id=None)
        e = WorkflowEdge(edge_id="e1", from_node_id="n1", to_node_id="n3")
        e2 = WorkflowEdge(edge_id="e2", from_node_id="n2", to_node_id="n3")
        resolver = DependencyResolver()
        waves = resolver.execution_order(_make_graph([n1, n2, n3], [e, e2]))
        self.assertEqual(waves[0], ["n1", "n2"])
        self.assertEqual(waves[1], ["n3"])

    def test_no_edges(self):
        n1 = WorkflowNode(node_id="n1", workflow_id="w1", task_type="a", assigned_agent_id=None)
        n2 = WorkflowNode(node_id="n2", workflow_id="w1", task_type="b", assigned_agent_id=None)
        resolver = DependencyResolver()
        waves = resolver.execution_order(_make_graph([n1, n2], []))
        self.assertEqual(len(waves), 1)
        self.assertEqual(sorted(waves[0]), ["n1", "n2"])


class DependencyResolverReadyNodesTests(unittest.TestCase):
    def test_ready_with_no_deps(self):
        n1 = WorkflowNode(node_id="n1", workflow_id="w1", task_type="a", assigned_agent_id=None, status="pending")
        resolver = DependencyResolver()
        ready = resolver.ready_nodes(_make_graph([n1], []))
        self.assertEqual(ready, ["n1"])

    def test_ready_after_dependency_completed(self):
        n1 = WorkflowNode(node_id="n1", workflow_id="w1", task_type="a", assigned_agent_id=None, status="completed")
        n2 = WorkflowNode(node_id="n2", workflow_id="w1", task_type="b", assigned_agent_id=None, status="pending")
        e = WorkflowEdge(edge_id="e1", from_node_id="n1", to_node_id="n2")
        resolver = DependencyResolver()
        ready = resolver.ready_nodes(_make_graph([n1, n2], [e]))
        self.assertEqual(ready, ["n2"])

    def test_not_ready_when_dependency_pending(self):
        n1 = WorkflowNode(node_id="n1", workflow_id="w1", task_type="a", assigned_agent_id=None, status="pending")
        n2 = WorkflowNode(node_id="n2", workflow_id="w1", task_type="b", assigned_agent_id=None, status="pending")
        e = WorkflowEdge(edge_id="e1", from_node_id="n1", to_node_id="n2")
        resolver = DependencyResolver()
        ready = resolver.ready_nodes(_make_graph([n1, n2], [e]))
        self.assertEqual(ready, ["n1"])

    def test_ready_empty_when_all_completed(self):
        n1 = WorkflowNode(node_id="n1", workflow_id="w1", task_type="a", assigned_agent_id=None, status="completed")
        resolver = DependencyResolver()
        ready = resolver.ready_nodes(_make_graph([n1], []))
        self.assertEqual(ready, [])


if __name__ == "__main__":
    unittest.main()
