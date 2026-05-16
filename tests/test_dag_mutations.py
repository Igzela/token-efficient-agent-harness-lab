import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core.dag_mutations import (
    DAGMutation,
    DAGMutationLimits,
    SUPPORTED_DAG_MUTATIONS,
    create_compensating_mutation,
    dag_mutation_requires_approval,
    mutation_to_audit_event,
    validate_dag_mutation,
)


class DAGMutationValidationTests(unittest.TestCase):
    def test_supported_mutation_types_are_canonical(self):
        self.assertEqual(
            (
                "add_node",
                "remove_node",
                "split_node",
                "retry_node",
                "pause_node",
                "resume_node",
                "replace_edge",
                "rollback",
            ),
            SUPPORTED_DAG_MUTATIONS,
        )

    def test_validate_rejects_unknown_mutation(self):
        mutation = DAGMutation(
            mutation_id="mut_1",
            dag_id="dag_1",
            mutation_type="execute_node",
        )

        result = validate_dag_mutation(
            mutation,
            current_node_count=0,
            current_edge_count=0,
        )

        self.assertFalse(result.ok)
        self.assertIn("unsupported mutation_type", result.errors[0])

    def test_validate_rejects_max_node_limit(self):
        mutation = DAGMutation(
            mutation_id="mut_1",
            dag_id="dag_1",
            mutation_type="add_node",
        )

        result = validate_dag_mutation(
            mutation,
            current_node_count=1,
            current_edge_count=0,
            limits=DAGMutationLimits(max_nodes=1, max_edges=10),
        )

        self.assertFalse(result.ok)
        self.assertIn("max_nodes", result.errors[0])

    def test_validate_rejects_max_edge_limit(self):
        mutation = DAGMutation(
            mutation_id="mut_1",
            dag_id="dag_1",
            mutation_type="split_node",
            payload={"replacement_node_count": 2, "added_edge_count": 3},
        )

        result = validate_dag_mutation(
            mutation,
            current_node_count=1,
            current_edge_count=2,
            limits=DAGMutationLimits(max_nodes=10, max_edges=4),
        )

        self.assertFalse(result.ok)
        self.assertIn("max_edges", result.errors[0])


class DAGMutationApprovalTests(unittest.TestCase):
    def test_explicit_approval_flag_requires_approval(self):
        mutation = DAGMutation(
            mutation_id="mut_1",
            dag_id="dag_1",
            mutation_type="pause_node",
            requires_approval=True,
        )

        self.assertTrue(dag_mutation_requires_approval(mutation))

    def test_running_or_completed_node_mutations_require_approval(self):
        mutation = DAGMutation(
            mutation_id="mut_1",
            dag_id="dag_1",
            mutation_type="remove_node",
            target_node_id="node_1",
        )

        self.assertTrue(
            dag_mutation_requires_approval(
                mutation,
                target_node_status="running",
            )
        )

    def test_replace_edge_from_completed_source_requires_approval(self):
        mutation = DAGMutation(
            mutation_id="mut_1",
            dag_id="dag_1",
            mutation_type="replace_edge",
            target_edge_id="edge_1",
        )

        self.assertTrue(
            dag_mutation_requires_approval(
                mutation,
                source_node_status="completed",
            )
        )


class DAGMutationAuditTests(unittest.TestCase):
    def test_compensating_mutation_is_forward_only_record(self):
        mutation = DAGMutation(
            mutation_id="mut_1",
            dag_id="dag_1",
            mutation_type="pause_node",
            target_node_id="node_1",
            reason="operator pause",
        )

        compensating = create_compensating_mutation(mutation)

        self.assertEqual("comp_mut_1", compensating.mutation_id)
        self.assertEqual("resume_node", compensating.mutation_type)
        self.assertEqual("mut_1", compensating.payload["compensates"])
        self.assertEqual("pending", compensating.status)

    def test_replace_edge_compensation_uses_previous_payload(self):
        mutation = DAGMutation(
            mutation_id="mut_1",
            dag_id="dag_1",
            mutation_type="replace_edge",
            target_edge_id="edge_1",
            payload={"from_node": "node_2", "to_node": "node_3"},
        )

        compensating = create_compensating_mutation(
            mutation,
            previous_payload={"from_node": "node_1", "to_node": "node_3"},
        )

        self.assertEqual("replace_edge", compensating.mutation_type)
        self.assertEqual("node_1", compensating.payload["from_node"])
        self.assertEqual("node_3", compensating.payload["to_node"])

    def test_mutation_to_audit_event_is_descriptive(self):
        mutation = DAGMutation(
            mutation_id="mut_1",
            dag_id="dag_1",
            mutation_type="rollback",
            reason="return to version 1",
        )

        event = mutation_to_audit_event(mutation)

        self.assertEqual("dag_mutation_recorded", event.event_type)
        self.assertEqual("mut_1", event.mutation_id)
        self.assertEqual("rollback", event.mutation_type)
        self.assertEqual("return to version 1", event.reason)


if __name__ == "__main__":
    unittest.main()
