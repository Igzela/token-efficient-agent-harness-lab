"""Tests for orchestration/conflict_resolver.py — conflict detection and resolution."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.orchestration.conflict_resolver import ConflictResolver
from harness_core.dispatch.orchestration.schemas import (
    CONFLICT_TYPES,
    RESOLUTION_STRATEGIES,
    ConflictRecord,
    WorkflowEdge,
    WorkflowGraph,
    WorkflowNode,
)


def _make_graph(nodes, edges=()):
    return WorkflowGraph(workflow_id="w1", dispatch_id="d1", nodes=tuple(nodes), edges=tuple(edges))


class ConflictResolverDetectTests(unittest.TestCase):
    def test_no_conflicts_simple_graph(self):
        n1 = WorkflowNode(node_id="n1", workflow_id="w1", task_type="a", assigned_agent_id=None, status="completed", output_ref="out-1")
        resolver = ConflictResolver()
        conflicts = resolver.detect_conflicts(_make_graph([n1]))
        self.assertEqual(len(conflicts), 0)

    def test_detect_failed_nodes(self):
        n1 = WorkflowNode(node_id="n1", workflow_id="w1", task_type="a", assigned_agent_id=None, status="failed", error="oops")
        resolver = ConflictResolver()
        conflicts = resolver.detect_conflicts(_make_graph([n1]))
        self.assertEqual(len(conflicts), 1)
        self.assertEqual(conflicts[0].conflict_type, "dependency_violation")

    def test_detect_output_conflict(self):
        n1 = WorkflowNode(node_id="n1", workflow_id="w1", task_type="a", assigned_agent_id=None, status="completed", output_ref="shared")
        n2 = WorkflowNode(node_id="n2", workflow_id="w1", task_type="b", assigned_agent_id=None, status="completed", output_ref="shared")
        resolver = ConflictResolver()
        conflicts = resolver.detect_conflicts(_make_graph([n1, n2]))
        output_conflicts = [c for c in conflicts if c.conflict_type == "output_conflict"]
        self.assertEqual(len(output_conflicts), 1)
        self.assertIn("n1", output_conflicts[0].involved_nodes)
        self.assertIn("n2", output_conflicts[0].involved_nodes)

    def test_detect_resource_conflict(self):
        n1 = WorkflowNode(node_id="n1", workflow_id="w1", task_type="a", assigned_agent_id="agent-1", status="completed")
        n2 = WorkflowNode(node_id="n2", workflow_id="w1", task_type="b", assigned_agent_id="agent-1", status="completed")
        resolver = ConflictResolver()
        conflicts = resolver.detect_conflicts(_make_graph([n1, n2]))
        resource_conflicts = [c for c in conflicts if c.conflict_type == "resource_conflict"]
        self.assertEqual(len(resource_conflicts), 1)

    def test_detect_budget_overrun(self):
        n1 = WorkflowNode(node_id="n1", workflow_id="w1", task_type="a", assigned_agent_id=None, status="completed", budget=1.0, cost_incurred=2.0)
        resolver = ConflictResolver()
        conflicts = resolver.detect_conflicts(_make_graph([n1]))
        budget_conflicts = [c for c in conflicts if c.conflict_type == "budget_overrun"]
        self.assertEqual(len(budget_conflicts), 1)

    def test_no_budget_overrun_within_budget(self):
        n1 = WorkflowNode(node_id="n1", workflow_id="w1", task_type="a", assigned_agent_id=None, status="completed", budget=10.0, cost_incurred=5.0)
        resolver = ConflictResolver()
        conflicts = resolver.detect_conflicts(_make_graph([n1]))
        budget_conflicts = [c for c in conflicts if c.conflict_type == "budget_overrun"]
        self.assertEqual(len(budget_conflicts), 0)


class ConflictResolverResolveTests(unittest.TestCase):
    def test_resolve_output_conflict(self):
        cr = ConflictRecord(
            conflict_id="c1", workflow_id="w1", conflict_type="output_conflict",
            involved_nodes=("n1", "n2"),
        )
        resolver = ConflictResolver()
        resolved = resolver.resolve(cr)
        self.assertEqual(resolved.resolution_strategy, "latest_wins")
        self.assertEqual(resolved.resolution_result, "latest_output_wins")
        self.assertIsNotNone(resolved.resolved_at)

    def test_resolve_resource_conflict(self):
        cr = ConflictRecord(
            conflict_id="c1", workflow_id="w1", conflict_type="resource_conflict",
            involved_nodes=("n1", "n2"),
        )
        resolver = ConflictResolver()
        resolved = resolver.resolve(cr)
        self.assertEqual(resolved.resolution_strategy, "priority_wins")

    def test_resolve_dependency_violation(self):
        cr = ConflictRecord(
            conflict_id="c1", workflow_id="w1", conflict_type="dependency_violation",
            involved_nodes=("n1",),
        )
        resolver = ConflictResolver()
        resolved = resolver.resolve(cr)
        self.assertEqual(resolved.resolution_strategy, "skip")

    def test_resolve_budget_overrun(self):
        cr = ConflictRecord(
            conflict_id="c1", workflow_id="w1", conflict_type="budget_overrun",
            involved_nodes=("n1",),
        )
        resolver = ConflictResolver()
        resolved = resolver.resolve(cr)
        self.assertEqual(resolved.resolution_strategy, "human_decides")


class ConflictConstantsTests(unittest.TestCase):
    def test_all_conflict_types_covered(self):
        self.assertIn("output_conflict", CONFLICT_TYPES)
        self.assertIn("resource_conflict", CONFLICT_TYPES)
        self.assertIn("dependency_violation", CONFLICT_TYPES)
        self.assertIn("budget_overrun", CONFLICT_TYPES)

    def test_all_resolution_strategies(self):
        self.assertIn("latest_wins", RESOLUTION_STRATEGIES)
        self.assertIn("priority_wins", RESOLUTION_STRATEGIES)
        self.assertIn("human_decides", RESOLUTION_STRATEGIES)


if __name__ == "__main__":
    unittest.main()
