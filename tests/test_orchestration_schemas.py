"""Tests for orchestration/schemas.py — Phase 5 orchestration dataclasses."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.orchestration.schemas import (
    WORKFLOW_STATUSES,
    NODE_STATUSES,
    EDGE_TYPES,
    MESSAGE_TYPES,
    CONFLICT_TYPES,
    RESOLUTION_STRATEGIES,
    WORKFLOW_SCHEMA_VERSION,
    WORKFLOW_NODE_SCHEMA_VERSION,
    WORKFLOW_EDGE_SCHEMA_VERSION,
    AGENT_MESSAGE_SCHEMA_VERSION,
    CONFLICT_RECORD_SCHEMA_VERSION,
    AGENT_ROLE_SCHEMA_VERSION,
    WorkflowNode,
    WorkflowEdge,
    WorkflowGraph,
    AgentMessage,
    ConflictRecord,
    AgentRole,
)


class WorkflowNodeTests(unittest.TestCase):
    def test_construction(self):
        node = WorkflowNode(
            node_id="n1", workflow_id="w1", task_type="code_review",
            assigned_agent_id="agent-1",
        )
        self.assertEqual(node.node_id, "n1")
        self.assertEqual(node.status, "pending")
        self.assertEqual(node.budget, 0.0)
        self.assertEqual(node.schema_version, WORKFLOW_NODE_SCHEMA_VERSION)

    def test_to_dict(self):
        node = WorkflowNode(
            node_id="n1", workflow_id="w1", task_type="code_review",
            assigned_agent_id="agent-1", status="completed", output_ref="out-1",
        )
        d = node.to_dict()
        self.assertEqual(d["node_id"], "n1")
        self.assertEqual(d["status"], "completed")
        self.assertEqual(d["output_ref"], "out-1")

    def test_frozen(self):
        node = WorkflowNode(node_id="n1", workflow_id="w1", task_type="t", assigned_agent_id=None)
        with self.assertRaises(AttributeError):
            node.node_id = "x"


class WorkflowEdgeTests(unittest.TestCase):
    def test_construction(self):
        edge = WorkflowEdge(edge_id="e1", from_node_id="n1", to_node_id="n2")
        self.assertEqual(edge.edge_type, "dependency")
        self.assertEqual(edge.schema_version, WORKFLOW_EDGE_SCHEMA_VERSION)

    def test_to_dict(self):
        edge = WorkflowEdge(edge_id="e1", from_node_id="n1", to_node_id="n2", edge_type="data_flow")
        d = edge.to_dict()
        self.assertEqual(d["edge_type"], "data_flow")

    def test_frozen(self):
        edge = WorkflowEdge(edge_id="e1", from_node_id="n1", to_node_id="n2")
        with self.assertRaises(AttributeError):
            edge.edge_id = "x"


class WorkflowGraphTests(unittest.TestCase):
    def test_construction(self):
        graph = WorkflowGraph(workflow_id="w1", dispatch_id="d1")
        self.assertEqual(graph.status, "created")
        self.assertEqual(graph.nodes, ())
        self.assertEqual(graph.edges, ())
        self.assertEqual(graph.schema_version, WORKFLOW_SCHEMA_VERSION)

    def test_to_dict(self):
        node = WorkflowNode(node_id="n1", workflow_id="w1", task_type="t", assigned_agent_id=None)
        edge = WorkflowEdge(edge_id="e1", from_node_id="n1", to_node_id="n2")
        graph = WorkflowGraph(workflow_id="w1", dispatch_id="d1", nodes=(node,), edges=(edge,))
        d = graph.to_dict()
        self.assertEqual(len(d["nodes"]), 1)
        self.assertEqual(len(d["edges"]), 1)

    def test_frozen(self):
        graph = WorkflowGraph(workflow_id="w1", dispatch_id="d1")
        with self.assertRaises(AttributeError):
            graph.workflow_id = "x"


class AgentMessageTests(unittest.TestCase):
    def test_construction(self):
        msg = AgentMessage(
            message_id="m1", from_agent_id="a1", to_agent_id="a2",
            workflow_id="w1", message_type="task_assign",
        )
        self.assertEqual(msg.message_type, "task_assign")
        self.assertEqual(msg.schema_version, AGENT_MESSAGE_SCHEMA_VERSION)

    def test_to_dict(self):
        msg = AgentMessage(
            message_id="m1", from_agent_id="a1", to_agent_id="a2",
            workflow_id="w1", message_type="result", payload={"key": "val"},
        )
        d = msg.to_dict()
        self.assertEqual(d["payload"]["key"], "val")

    def test_frozen(self):
        msg = AgentMessage(
            message_id="m1", from_agent_id="a1", to_agent_id="a2",
            workflow_id="w1", message_type="t",
        )
        with self.assertRaises(AttributeError):
            msg.message_id = "x"


class ConflictRecordTests(unittest.TestCase):
    def test_construction(self):
        cr = ConflictRecord(
            conflict_id="c1", workflow_id="w1", conflict_type="output_conflict",
            involved_nodes=("n1", "n2"),
        )
        self.assertEqual(cr.conflict_type, "output_conflict")
        self.assertIsNone(cr.resolution_strategy)
        self.assertEqual(cr.schema_version, CONFLICT_RECORD_SCHEMA_VERSION)

    def test_to_dict(self):
        cr = ConflictRecord(
            conflict_id="c1", workflow_id="w1", conflict_type="output_conflict",
            involved_nodes=("n1",), resolution_strategy="latest_wins",
        )
        d = cr.to_dict()
        self.assertEqual(d["resolution_strategy"], "latest_wins")

    def test_frozen(self):
        cr = ConflictRecord(conflict_id="c1", workflow_id="w1", conflict_type="t", involved_nodes=())
        with self.assertRaises(AttributeError):
            cr.conflict_id = "x"


class AgentRoleTests(unittest.TestCase):
    def test_construction(self):
        role = AgentRole(role_id="r1", role_name="code_agent", capabilities=("code_review",))
        self.assertEqual(role.max_concurrent_nodes, 1)
        self.assertEqual(role.schema_version, AGENT_ROLE_SCHEMA_VERSION)

    def test_to_dict(self):
        role = AgentRole(role_id="r1", role_name="code_agent", capabilities=("code_review",), budget_limit=10.0)
        d = role.to_dict()
        self.assertEqual(d["budget_limit"], 10.0)

    def test_frozen(self):
        role = AgentRole(role_id="r1", role_name="r", capabilities=())
        with self.assertRaises(AttributeError):
            role.role_id = "x"


class ConstantsTests(unittest.TestCase):
    def test_workflow_statuses(self):
        self.assertIn("created", WORKFLOW_STATUSES)
        self.assertIn("running", WORKFLOW_STATUSES)
        self.assertIn("completed", WORKFLOW_STATUSES)
        self.assertIn("failed", WORKFLOW_STATUSES)
        self.assertIn("cancelled", WORKFLOW_STATUSES)

    def test_node_statuses(self):
        self.assertIn("pending", NODE_STATUSES)
        self.assertIn("ready", NODE_STATUSES)
        self.assertIn("running", NODE_STATUSES)
        self.assertIn("completed", NODE_STATUSES)
        self.assertIn("waiting_human", NODE_STATUSES)

    def test_edge_types(self):
        self.assertIn("dependency", EDGE_TYPES)
        self.assertIn("data_flow", EDGE_TYPES)

    def test_message_types(self):
        self.assertIn("task_assign", MESSAGE_TYPES)
        self.assertIn("result", MESSAGE_TYPES)

    def test_conflict_types(self):
        self.assertIn("output_conflict", CONFLICT_TYPES)
        self.assertIn("budget_overrun", CONFLICT_TYPES)

    def test_resolution_strategies(self):
        self.assertIn("latest_wins", RESOLUTION_STRATEGIES)
        self.assertIn("human_decides", RESOLUTION_STRATEGIES)


if __name__ == "__main__":
    unittest.main()
