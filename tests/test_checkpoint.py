import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core.checkpoint import (
    ArtifactRef,
    CheckpointManager,
    RecoveryPlan,
)
from harness_core.event_store import EventStore


def _event(event_id, event_type, payload):
    return {
        "event_id": event_id,
        "schema_version": "event.v1",
        "event_type": event_type,
        "timestamp": "2026-01-01T00:00:00Z",
        "producer": {
            "component_id": "test",
            "component_type": "unit_test",
        },
        "correlation": {},
        "severity": "info",
        "payload": payload,
        "idempotency_key": event_id,
        "parent_event_id": None,
    }


class CheckpointPersistenceTests(unittest.TestCase):
    def test_checkpoint_save_load(self):
        with tempfile.TemporaryDirectory() as tmp:
            manager = CheckpointManager(tmp)
            checkpoint = manager.create_checkpoint(
                task_id="task_1",
                node_id="node_1",
                dag_version=2,
                status="running",
                current_step="write",
                completed_steps=("plan",),
                pending_steps=("test",),
                input_hash="hash_1",
                created_at="2026-01-01T00:00:01Z",
                artifact_refs=(
                    ArtifactRef("patch", "tmp/patch.diff", "abc123"),
                ),
            )

            loaded = manager.load_checkpoint(checkpoint.checkpoint_id)

            self.assertEqual(checkpoint, loaded)
            self.assertEqual("task_1", loaded.task_id)
            self.assertEqual("patch", loaded.artifact_refs[0].artifact_type)

    def test_latest_checkpoint_lookup(self):
        with tempfile.TemporaryDirectory() as tmp:
            manager = CheckpointManager(tmp)
            first = manager.create_checkpoint(
                task_id="task_1",
                node_id="node_1",
                dag_version=1,
                status="running",
                current_step="plan",
                completed_steps=(),
                pending_steps=("write",),
                input_hash="hash_1",
                created_at="2026-01-01T00:00:01Z",
            )
            second = manager.create_checkpoint(
                task_id="task_1",
                node_id="node_1",
                dag_version=1,
                status="running",
                current_step="write",
                completed_steps=("plan",),
                pending_steps=(),
                input_hash="hash_1",
                created_at="2026-01-01T00:00:02Z",
            )

            self.assertEqual(second, manager.latest_checkpoint("task_1"))
            self.assertEqual((first, second), manager.list_checkpoints("task_1"))

    def test_checkpoint_overwrite_idempotency(self):
        with tempfile.TemporaryDirectory() as tmp:
            manager = CheckpointManager(tmp)
            checkpoint = manager.create_checkpoint(
                task_id="task_1",
                node_id="node_1",
                dag_version=1,
                status="running",
                current_step="plan",
                completed_steps=(),
                pending_steps=("write",),
                input_hash="hash_1",
                created_at="2026-01-01T00:00:01Z",
            )
            path = Path(tmp) / f"{checkpoint.checkpoint_id}.json"
            first_content = path.read_text(encoding="utf-8")

            manager.save_checkpoint(checkpoint)
            second_content = path.read_text(encoding="utf-8")

            self.assertEqual(first_content, second_content)
            self.assertEqual(checkpoint, manager.load_checkpoint(checkpoint.checkpoint_id))


class RecoveryPlanTests(unittest.TestCase):
    def test_recovery_plan_creation_for_failed_checkpoint(self):
        with tempfile.TemporaryDirectory() as tmp:
            manager = CheckpointManager(tmp)
            checkpoint = manager.create_checkpoint(
                task_id="task_1",
                node_id="node_1",
                dag_version=1,
                status="failed",
                current_step="write",
                completed_steps=("plan",),
                pending_steps=("test",),
                input_hash="hash_1",
                created_at="2026-01-01T00:00:01Z",
                resumable=False,
                reason="worker crashed",
            )

            plan = manager.create_recovery_plan("task_1")

            self.assertIsInstance(plan, RecoveryPlan)
            self.assertEqual("compensate", plan.strategy)
            self.assertEqual(checkpoint.checkpoint_id, plan.checkpoint_id)
            self.assertEqual(
                ("task_cancelled", "claim_released"),
                tuple(event.event_type for event in plan.compensating_events),
            )
            self.assertEqual(
                "worker crashed", plan.compensating_events[0].payload["reason"]
            )

    def test_recovery_plan_skip_without_checkpoint(self):
        with tempfile.TemporaryDirectory() as tmp:
            manager = CheckpointManager(tmp)

            plan = manager.create_recovery_plan("task_missing")

            self.assertEqual("skip", plan.strategy)
            self.assertIsNone(plan.checkpoint_id)


class CheckpointIntegrityTests(unittest.TestCase):
    def test_event_log_integrity_check(self):
        with tempfile.TemporaryDirectory() as tmp:
            event_log = Path(tmp) / "events.jsonl"
            store = EventStore(event_log)
            store.append_event(
                _event(
                    "evt_1",
                    "project_item_state_changed",
                    {"item_id": "item_1", "new_status": "done"},
                )
            )
            manager = CheckpointManager(Path(tmp) / "checkpoints")

            result = manager.check_event_log_integrity(event_log)

            self.assertTrue(result.ok)
            self.assertEqual((), result.errors)

    def test_projection_consistency_check(self):
        with tempfile.TemporaryDirectory() as tmp:
            event_log = Path(tmp) / "events.jsonl"
            store = EventStore(event_log)
            store.append_event(
                _event(
                    "evt_1",
                    "project_item_state_changed",
                    {"item_id": "item_1", "new_status": "in_review"},
                )
            )
            store.append_event(
                _event(
                    "evt_2",
                    "project_dependency_resolved",
                    {
                        "edge_id": "edge_1",
                        "from_node": "node_1",
                        "to_node": "node_2",
                        "dependency_type": "artifact",
                    },
                )
            )
            manager = CheckpointManager(Path(tmp) / "checkpoints")

            result = manager.check_projection_consistency(event_log)

            self.assertTrue(result.ok)
            self.assertEqual((), result.errors)
            self.assertEqual((), result.warnings)

    def test_event_log_integrity_reports_invalid_jsonl(self):
        with tempfile.TemporaryDirectory() as tmp:
            event_log = Path(tmp) / "events.jsonl"
            event_log.write_text(json.dumps({"bad": "event"}) + "\n", encoding="utf-8")
            manager = CheckpointManager(Path(tmp) / "checkpoints")

            result = manager.check_event_log_integrity(event_log)

            self.assertFalse(result.ok)
            self.assertTrue(result.errors)


class PathTraversalTests(unittest.TestCase):
    def test_path_traversal_in_checkpoint_id_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            manager = CheckpointManager(tmp)
            with self.assertRaises(ValueError):
                manager.load_checkpoint("../../etc/passwd")

    def test_path_traversal_in_save_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            manager = CheckpointManager(tmp)
            checkpoint = manager.create_checkpoint(
                task_id="task_1",
                node_id="node_1",
                dag_version=1,
                status="running",
                current_step="plan",
                completed_steps=(),
                pending_steps=("plan",),
                input_hash="hash_1",
                created_at="2026-01-01T00:00:01Z",
            )
            # Direct save_checkpoint with traversal id
            from harness_core.checkpoint import Checkpoint
            evil = Checkpoint(
                checkpoint_id="../../tmp/evil",
                task_id="t", node_id="n", dag_version=1,
                status="running", current_step="s",
                completed_steps=(), pending_steps=(),
                input_hash="h",
            )
            with self.assertRaises(ValueError):
                manager.save_checkpoint(evil)


if __name__ == "__main__":
    unittest.main()
