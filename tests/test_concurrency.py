import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core.concurrency import ConcurrencyController, FileOverlap
from harness_core.dag_manager import DAGEdge, DAGNode, DAGState
from harness_core.sandbox import FileClaim


def _node(node_id, status="pending", **metadata):
    return DAGNode(
        node_id=node_id,
        task_id=node_id.replace("node", "task"),
        node_type="task",
        status=status,
        tier="cheap_executor",
        metadata=metadata,
    )


def _dag(nodes, edges=()):
    return DAGState(
        dag_id="dag_1",
        version=1,
        nodes=tuple(nodes),
        edges=tuple(edges),
        created_at="2026-01-01T00:00:00Z",
        updated_at="2026-01-01T00:00:00Z",
    )


class ConcurrencyControllerTests(unittest.TestCase):
    def test_independent_items_grouped_together(self):
        items = (
            _node("node_1", write_files=("a.py",)),
            _node("node_2", write_files=("b.py",)),
        )
        controller = ConcurrencyController()

        batch = controller.schedule(items, _dag(items), ())

        self.assertEqual(("node_1", "node_2"), batch.item_ids)
        self.assertEqual((), batch.blocked_items)

    def test_overlapping_files_separated(self):
        items = (
            _node("node_1", write_files=("a.py",)),
            _node("node_2", read_files=("a.py",)),
        )
        controller = ConcurrencyController()

        batch = controller.schedule(items, _dag(items), ())

        self.assertEqual(("node_1",), batch.item_ids)
        self.assertEqual(("node_2",), tuple(item.node_id for item in batch.blocked_items))
        self.assertEqual(
            (FileOverlap("node_1", "node_2", ("a.py",)),),
            batch.file_overlaps,
        )
        self.assertFalse(controller.can_run_parallel(items[0], items[1], batch.file_overlaps))

    def test_max_concurrent_respected(self):
        items = (
            _node("node_1", write_files=("a.py",)),
            _node("node_2", write_files=("b.py",)),
            _node("node_3", write_files=("c.py",)),
        )
        controller = ConcurrencyController(max_concurrent=2)

        batch = controller.schedule(items, _dag(items), ())

        self.assertEqual(("node_1", "node_2"), batch.item_ids)
        self.assertEqual(("node_3",), tuple(item.node_id for item in batch.blocked_items))

    def test_hard_dependency_blocks_downstream(self):
        upstream = _node("node_1", status="running")
        downstream = _node("node_2", write_files=("b.py",))
        dag = _dag(
            (upstream, downstream),
            (
                DAGEdge(
                    edge_id="edge_1",
                    from_node="node_1",
                    to_node="node_2",
                    dependency_type="hard",
                ),
            ),
        )
        controller = ConcurrencyController()

        batch = controller.schedule((downstream,), dag, ())

        self.assertEqual((), batch.item_ids)
        self.assertEqual(("node_2",), tuple(item.node_id for item in batch.blocked_items))

    def test_soft_dependency_does_not_block(self):
        upstream = _node("node_1", status="running")
        downstream = _node("node_2", write_files=("b.py",))
        dag = _dag(
            (upstream, downstream),
            (
                DAGEdge(
                    edge_id="edge_1",
                    from_node="node_1",
                    to_node="node_2",
                    dependency_type="soft",
                ),
            ),
        )
        controller = ConcurrencyController()

        batch = controller.schedule((downstream,), dag, ())

        self.assertEqual(("node_2",), batch.item_ids)

    def test_artifact_dependency_requires_verified_condition(self):
        upstream = _node("node_1", status="completed")
        blocked = _node("node_2", write_files=("b.py",))
        unblocked = _node(
            "node_3",
            write_files=("c.py",),
            verified_artifacts=("edge_2",),
        )
        dag = _dag(
            (upstream, blocked, unblocked),
            (
                DAGEdge(
                    edge_id="edge_1",
                    from_node="node_1",
                    to_node="node_2",
                    dependency_type="artifact",
                ),
                DAGEdge(
                    edge_id="edge_2",
                    from_node="node_1",
                    to_node="node_3",
                    dependency_type="artifact",
                ),
            ),
        )
        controller = ConcurrencyController()

        batch = controller.schedule((blocked, unblocked), dag, ())

        self.assertEqual(("node_3",), batch.item_ids)
        self.assertEqual(("node_2",), tuple(item.node_id for item in batch.blocked_items))

    def test_active_write_claims_block_conflicting_items(self):
        item = _node("node_1", write_files=("a.py",))
        claim = FileClaim(
            claim_id="claim_1",
            sandbox_id="sandbox_1",
            file_path="a.py",
            claimed_at="2026-01-01T00:00:00Z",
        )
        controller = ConcurrencyController()

        batch = controller.schedule((item,), _dag((item,)), (claim,))

        self.assertEqual((), batch.item_ids)
        self.assertEqual(("node_1",), tuple(node.node_id for node in batch.blocked_items))

    def test_empty_ready_list_returns_empty_batch(self):
        controller = ConcurrencyController()

        batch = controller.schedule((), _dag(()), ())

        self.assertEqual((), batch.scheduled_items)
        self.assertEqual((), batch.blocked_items)


if __name__ == "__main__":
    unittest.main()
