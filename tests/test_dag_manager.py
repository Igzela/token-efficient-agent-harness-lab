import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core.dag_manager import (
    DAGEdge,
    DAGManager,
    DAGMutationProposal,
    DAGNode,
    DAGState,
    _has_cycle,
)


def _make_node(node_id: str, **kwargs) -> DAGNode:
    defaults = dict(node_id=node_id, task_id=node_id, node_type="task", status="pending", tier="cheap_executor")
    defaults.update(kwargs)
    return DAGNode(**defaults)


def _make_edge(edge_id: str, from_node: str, to_node: str, **kwargs) -> DAGEdge:
    defaults = dict(edge_id=edge_id, from_node=from_node, to_node=to_node, dependency_type="hard", status="pending")
    defaults.update(kwargs)
    return DAGEdge(**defaults)


def _add_node_prop(dag_id: str, node_id: str, **kwargs) -> DAGMutationProposal:
    payload = {"node_id": node_id, "task_id": node_id, "node_type": "task", "status": "pending", "tier": "cheap_executor"}
    payload.update(kwargs)
    return DAGMutationProposal(
        proposal_id=f"add_{node_id}",
        dag_id=dag_id,
        mutation_type="add_node",
        payload=payload,
    )


def _remove_node_prop(dag_id: str, node_id: str) -> DAGMutationProposal:
    return DAGMutationProposal(
        proposal_id=f"remove_{node_id}",
        dag_id=dag_id,
        mutation_type="remove_node",
        target_node_id=node_id,
    )


def _add_edge_prop(dag_id: str, edge_id: str, from_node: str, to_node: str) -> DAGMutationProposal:
    return DAGMutationProposal(
        proposal_id=f"add_edge_{edge_id}",
        dag_id=dag_id,
        mutation_type="add_edge",
        payload={"edge_id": edge_id, "from_node": from_node, "to_node": to_node, "dependency_type": "hard"},
    )


def _remove_edge_prop(dag_id: str, edge_id: str) -> DAGMutationProposal:
    return DAGMutationProposal(
        proposal_id=f"remove_edge_{edge_id}",
        dag_id=dag_id,
        mutation_type="remove_edge",
        target_edge_id=edge_id,
    )


class DAGCycleDetectionTests(unittest.TestCase):
    def test_empty_dag_no_cycle(self):
        self.assertFalse(_has_cycle((), ()))

    def test_single_node_no_cycle(self):
        self.assertFalse(_has_cycle((_make_node("a"),), ()))

    def test_acyclic_chain(self):
        nodes = (_make_node("a"), _make_node("b"), _make_node("c"))
        edges = (_make_edge("e1", "a", "b"), _make_edge("e2", "b", "c"))
        self.assertFalse(_has_cycle(nodes, edges))

    def test_simple_cycle(self):
        nodes = (_make_node("a"), _make_node("b"))
        edges = (_make_edge("e1", "a", "b"), _make_edge("e2", "b", "a"))
        self.assertTrue(_has_cycle(nodes, edges))

    def test_self_loop(self):
        nodes = (_make_node("a"),)
        edges = (_make_edge("e1", "a", "a"),)
        self.assertTrue(_has_cycle(nodes, edges))


class DAGManagerBasicTests(unittest.TestCase):
    def test_initial_state(self):
        mgr = DAGManager("dag_1")
        state = mgr.current_state()
        self.assertEqual("dag_1", state.dag_id)
        self.assertEqual(0, state.version)
        self.assertEqual((), state.nodes)
        self.assertEqual((), state.edges)

    def test_add_node(self):
        mgr = DAGManager("dag_1")
        result = mgr.apply_mutation(_add_node_prop("dag_1", "n1"))
        self.assertTrue(result.applied)
        self.assertEqual(1, result.new_dag_version)
        self.assertEqual(1, len(mgr.state.nodes))
        self.assertEqual("n1", mgr.state.nodes[0].node_id)

    def test_add_node_duplicate_fails(self):
        mgr = DAGManager("dag_1")
        mgr.apply_mutation(_add_node_prop("dag_1", "n1"))
        result = mgr.apply_mutation(_add_node_prop("dag_1", "n1"))
        self.assertFalse(result.applied)
        self.assertIn("already exists", result.errors[0])

    def test_remove_node(self):
        mgr = DAGManager("dag_1")
        mgr.apply_mutation(_add_node_prop("dag_1", "n1"))
        result = mgr.apply_mutation(_remove_node_prop("dag_1", "n1"))
        self.assertTrue(result.applied)
        self.assertEqual(0, len(mgr.state.nodes))

    def test_remove_node_not_found(self):
        mgr = DAGManager("dag_1")
        result = mgr.apply_mutation(_remove_node_prop("dag_1", "n1"))
        self.assertFalse(result.applied)
        self.assertIn("not found", result.errors[0])

    def test_remove_node_with_edges_fails(self):
        mgr = DAGManager("dag_1")
        mgr.apply_mutation(_add_node_prop("dag_1", "n1"))
        mgr.apply_mutation(_add_node_prop("dag_1", "n2"))
        mgr.apply_mutation(_add_edge_prop("dag_1", "e1", "n1", "n2"))
        result = mgr.apply_mutation(_remove_node_prop("dag_1", "n1"))
        self.assertFalse(result.applied)
        self.assertIn("connected edges", result.errors[0])


class DAGManagerEdgeTests(unittest.TestCase):
    def test_add_edge(self):
        mgr = DAGManager("dag_1")
        mgr.apply_mutation(_add_node_prop("dag_1", "n1"))
        mgr.apply_mutation(_add_node_prop("dag_1", "n2"))
        result = mgr.apply_mutation(_add_edge_prop("dag_1", "e1", "n1", "n2"))
        self.assertTrue(result.applied)
        self.assertEqual(1, len(mgr.state.edges))

    def test_add_edge_missing_node(self):
        mgr = DAGManager("dag_1")
        mgr.apply_mutation(_add_node_prop("dag_1", "n1"))
        result = mgr.apply_mutation(_add_edge_prop("dag_1", "e1", "n1", "n2"))
        self.assertFalse(result.applied)
        self.assertIn("not found", result.errors[0])

    def test_add_edge_creates_cycle(self):
        mgr = DAGManager("dag_1")
        mgr.apply_mutation(_add_node_prop("dag_1", "n1"))
        mgr.apply_mutation(_add_node_prop("dag_1", "n2"))
        mgr.apply_mutation(_add_edge_prop("dag_1", "e1", "n1", "n2"))
        result = mgr.apply_mutation(_add_edge_prop("dag_1", "e2", "n2", "n1"))
        self.assertFalse(result.applied)
        self.assertIn("cycle", result.errors[0])

    def test_remove_edge(self):
        mgr = DAGManager("dag_1")
        mgr.apply_mutation(_add_node_prop("dag_1", "n1"))
        mgr.apply_mutation(_add_node_prop("dag_1", "n2"))
        mgr.apply_mutation(_add_edge_prop("dag_1", "e1", "n1", "n2"))
        result = mgr.apply_mutation(_remove_edge_prop("dag_1", "e1"))
        self.assertTrue(result.applied)
        self.assertEqual(0, len(mgr.state.edges))

    def test_rewire_edge(self):
        mgr = DAGManager("dag_1")
        mgr.apply_mutation(_add_node_prop("dag_1", "n1"))
        mgr.apply_mutation(_add_node_prop("dag_1", "n2"))
        mgr.apply_mutation(_add_node_prop("dag_1", "n3"))
        mgr.apply_mutation(_add_edge_prop("dag_1", "e1", "n1", "n2"))
        result = mgr.apply_mutation(DAGMutationProposal(
            proposal_id="rewire_e1",
            dag_id="dag_1",
            mutation_type="rewire_edge",
            target_edge_id="e1",
            payload={"from_node": "n1", "to_node": "n3"},
        ))
        self.assertTrue(result.applied)
        edge = [e for e in mgr.state.edges if e.edge_id == "e1"][0]
        self.assertEqual("n3", edge.to_node)


class DAGManagerApprovalTests(unittest.TestCase):
    def test_remove_running_node_requires_approval(self):
        mgr = DAGManager("dag_1")
        mgr.apply_mutation(_add_node_prop("dag_1", "n1", status="running"))
        result = mgr.apply_mutation(_remove_node_prop("dag_1", "n1"))
        self.assertFalse(result.applied)
        self.assertIn("requires approval", result.errors[0])

    def test_explicit_approval_required(self):
        mgr = DAGManager("dag_1")
        proposal = DAGMutationProposal(
            proposal_id="add_n1",
            dag_id="dag_1",
            mutation_type="add_node",
            payload={"node_id": "n1", "task_id": "n1", "node_type": "task", "status": "pending", "tier": "cheap_executor"},
            requires_approval=True,
        )
        result = mgr.apply_mutation(proposal)
        self.assertFalse(result.applied)
        self.assertIn("requires approval", result.errors[0])


class DAGManagerDagIdMismatchTests(unittest.TestCase):
    def test_wrong_dag_id_rejected(self):
        mgr = DAGManager("dag_1")
        result = mgr.apply_mutation(_add_node_prop("dag_2", "n1"))
        self.assertFalse(result.applied)
        self.assertIn("dag_id mismatch", result.errors[0])


class DAGManagerUnknownMutationTests(unittest.TestCase):
    def test_unknown_mutation_type(self):
        mgr = DAGManager("dag_1")
        proposal = DAGMutationProposal(
            proposal_id="bad",
            dag_id="dag_1",
            mutation_type="unknown_op",
        )
        result = mgr.apply_mutation(proposal)
        self.assertFalse(result.applied)
        self.assertIn("unknown mutation_type", result.errors[0])


class DAGManagerRollbackTests(unittest.TestCase):
    def test_rollback_restores_previous_state(self):
        mgr = DAGManager("dag_1")
        mgr.apply_mutation(_add_node_prop("dag_1", "n1"))
        mgr.apply_mutation(_add_node_prop("dag_1", "n2"))
        self.assertEqual(2, mgr.state.version)
        self.assertEqual(2, len(mgr.state.nodes))

        state = mgr.rollback(0)
        self.assertEqual(0, state.version)
        self.assertEqual(0, len(state.nodes))

    def test_rollback_to_current_version_noop(self):
        mgr = DAGManager("dag_1")
        mgr.apply_mutation(_add_node_prop("dag_1", "n1"))
        state = mgr.rollback(1)
        self.assertEqual(1, state.version)
        self.assertEqual(1, len(state.nodes))


class DAGManagerTopologicalOrderTests(unittest.TestCase):
    def test_empty_dag(self):
        mgr = DAGManager("dag_1")
        self.assertEqual((), mgr.topological_order())

    def test_linear_chain(self):
        mgr = DAGManager("dag_1")
        mgr.apply_mutation(_add_node_prop("dag_1", "a"))
        mgr.apply_mutation(_add_node_prop("dag_1", "b"))
        mgr.apply_mutation(_add_node_prop("dag_1", "c"))
        mgr.apply_mutation(_add_edge_prop("dag_1", "e1", "a", "b"))
        mgr.apply_mutation(_add_edge_prop("dag_1", "e2", "b", "c"))
        order = mgr.topological_order()
        self.assertEqual(("a", "b", "c"), order)

    def test_diamond(self):
        mgr = DAGManager("dag_1")
        for n in ("a", "b", "c", "d"):
            mgr.apply_mutation(_add_node_prop("dag_1", n))
        mgr.apply_mutation(_add_edge_prop("dag_1", "e1", "a", "b"))
        mgr.apply_mutation(_add_edge_prop("dag_1", "e2", "a", "c"))
        mgr.apply_mutation(_add_edge_prop("dag_1", "e3", "b", "d"))
        mgr.apply_mutation(_add_edge_prop("dag_1", "e4", "c", "d"))
        order = mgr.topological_order()
        self.assertEqual("a", order[0])
        self.assertEqual("d", order[-1])


class DAGManagerNodesReadyTests(unittest.TestCase):
    def test_no_predecessors_all_ready(self):
        mgr = DAGManager("dag_1")
        mgr.apply_mutation(_add_node_prop("dag_1", "a"))
        mgr.apply_mutation(_add_node_prop("dag_1", "b"))
        ready = mgr.nodes_ready(frozenset())
        self.assertEqual(("a", "b"), tuple(n.node_id for n in ready))

    def test_root_nodes_ready_when_none_completed(self):
        mgr = DAGManager("dag_1")
        mgr.apply_mutation(_add_node_prop("dag_1", "a"))
        mgr.apply_mutation(_add_node_prop("dag_1", "b"))
        mgr.apply_mutation(_add_edge_prop("dag_1", "e1", "a", "b"))
        ready = mgr.nodes_ready(frozenset())
        self.assertEqual(("a",), tuple(n.node_id for n in ready))

    def test_dependent_node_ready_after_predecessor(self):
        mgr = DAGManager("dag_1")
        mgr.apply_mutation(_add_node_prop("dag_1", "a"))
        mgr.apply_mutation(_add_node_prop("dag_1", "b"))
        mgr.apply_mutation(_add_edge_prop("dag_1", "e1", "a", "b"))
        ready = mgr.nodes_ready(frozenset({"a"}))
        self.assertEqual(("b",), tuple(n.node_id for n in ready))


class DAGManagerPathBetweenTests(unittest.TestCase):
    def test_path_exists(self):
        mgr = DAGManager("dag_1")
        mgr.apply_mutation(_add_node_prop("dag_1", "a"))
        mgr.apply_mutation(_add_node_prop("dag_1", "b"))
        mgr.apply_mutation(_add_node_prop("dag_1", "c"))
        mgr.apply_mutation(_add_edge_prop("dag_1", "e1", "a", "b"))
        mgr.apply_mutation(_add_edge_prop("dag_1", "e2", "b", "c"))
        path = mgr.path_between("a", "c")
        self.assertEqual(("a", "b", "c"), path)

    def test_path_not_exists(self):
        mgr = DAGManager("dag_1")
        mgr.apply_mutation(_add_node_prop("dag_1", "a"))
        mgr.apply_mutation(_add_node_prop("dag_1", "b"))
        self.assertIsNone(mgr.path_between("a", "b"))


class DAGManagerValidateTests(unittest.TestCase):
    def test_valid_dag(self):
        mgr = DAGManager("dag_1")
        mgr.apply_mutation(_add_node_prop("dag_1", "a"))
        mgr.apply_mutation(_add_node_prop("dag_1", "b"))
        mgr.apply_mutation(_add_edge_prop("dag_1", "e1", "a", "b"))
        errors = mgr.validate_dag()
        self.assertEqual([], errors)


if __name__ == "__main__":
    unittest.main()
