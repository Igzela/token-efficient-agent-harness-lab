"""Tests for orchestration/work_queue.py — node queue management."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.orchestration.work_queue import WorkQueue
from harness_core.dispatch.orchestration.schemas import WorkflowGraph, WorkflowNode


def _make_node(node_id="n1", status="pending"):
    return WorkflowNode(node_id=node_id, workflow_id="w1", task_type="t", assigned_agent_id=None, status=status)


def _make_graph(nodes):
    return WorkflowGraph(workflow_id="w1", dispatch_id="d1", nodes=tuple(nodes))


class WorkQueueTests(unittest.TestCase):
    def setUp(self):
        self.queue = WorkQueue()

    def test_enqueue(self):
        node = _make_node()
        self.queue.enqueue(node)
        self.assertEqual(self.queue.status_of("n1"), "ready")

    def test_dequeue_ready(self):
        node = _make_node()
        self.queue.enqueue(node)
        graph = _make_graph([node])
        ready = self.queue.dequeue_ready(graph)
        self.assertEqual(len(ready), 1)
        self.assertEqual(ready[0].node_id, "n1")

    def test_dequeue_ready_excludes_non_ready(self):
        node = _make_node(status="completed")
        self.queue.enqueue(node)
        graph = _make_graph([node])
        ready = self.queue.dequeue_ready(graph)
        self.assertEqual(len(ready), 0)

    def test_start(self):
        node = _make_node()
        self.queue.enqueue(node)
        self.queue.start("n1")
        self.assertEqual(self.queue.status_of("n1"), "running")

    def test_start_only_transitions_from_ready(self):
        node = _make_node()
        self.queue.enqueue(node)
        self.queue.start("n1")
        self.queue.start("n1")
        self.assertEqual(self.queue.status_of("n1"), "running")

    def test_complete(self):
        node = _make_node()
        self.queue.enqueue(node)
        self.queue.complete("n1", "output-1")
        self.assertEqual(self.queue.status_of("n1"), "completed")

    def test_fail(self):
        node = _make_node()
        self.queue.enqueue(node)
        self.queue.fail("n1", "error occurred")
        self.assertEqual(self.queue.status_of("n1"), "failed")

    def test_cancel_pending(self):
        node = _make_node(status="pending")
        self.queue.cancel("n1")
        self.assertEqual(self.queue.status_of("n1"), "cancelled")

    def test_cancel_ready(self):
        node = _make_node()
        self.queue.enqueue(node)
        self.queue.cancel("n1")
        self.assertEqual(self.queue.status_of("n1"), "cancelled")

    def test_cancel_running_not_cancelled(self):
        node = _make_node()
        self.queue.enqueue(node)
        self.queue.start("n1")
        self.queue.cancel("n1")
        self.assertEqual(self.queue.status_of("n1"), "running")

    def test_status_of_unknown(self):
        self.assertEqual(self.queue.status_of("unknown"), "pending")

    def test_reset(self):
        node = _make_node()
        self.queue.enqueue(node)
        self.queue.reset()
        self.assertEqual(self.queue.status_of("n1"), "pending")


if __name__ == "__main__":
    unittest.main()
