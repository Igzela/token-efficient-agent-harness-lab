import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core.checkpoint import CheckpointManager
from harness_core.supervisor import RuntimeSupervisor, WorkerHealth


class RuntimeSupervisorCheckpointTests(unittest.TestCase):
    def test_task_checkpoint_flow(self):
        with tempfile.TemporaryDirectory() as tmp:
            manager = CheckpointManager(tmp)
            supervisor = RuntimeSupervisor(manager, dag_version=3)

            started = supervisor.start_task(
                "task_1",
                "node_1",
                ("plan", "write"),
                input_hash="hash_1",
                timestamp="2026-01-01T00:00:01Z",
            )
            stepped = supervisor.checkpoint_step(
                "task_1",
                "plan",
                timestamp="2026-01-01T00:00:02Z",
            )
            completed = supervisor.complete_task(
                "task_1",
                timestamp="2026-01-01T00:00:03Z",
            )

            self.assertEqual("running", started.status)
            self.assertEqual(("plan",), stepped.completed_steps)
            self.assertEqual("write", stepped.current_step)
            self.assertEqual("completed", completed.status)
            self.assertEqual("completed", supervisor.get_status("task_1"))

    def test_crashed_worker_recovery_action_description(self):
        with tempfile.TemporaryDirectory() as tmp:
            manager = CheckpointManager(tmp)
            supervisor = RuntimeSupervisor(manager, heartbeat_timeout=10)
            supervisor.start_task(
                "task_1",
                "node_1",
                ("plan", "write"),
                timestamp="2026-01-01T00:00:01Z",
            )
            supervisor.fail_task(
                "task_1",
                "worker crashed",
                timestamp="2026-01-01T00:00:02Z",
            )
            workers = (
                WorkerHealth(
                    worker_id="worker_1",
                    task_id="task_1",
                    status="crashed",
                    last_heartbeat=10,
                    started_at=1,
                    error="exit",
                ),
            )

            report = supervisor.assess_workers(workers, now=12)

            self.assertFalse(report.healthy)
            self.assertEqual(("worker_1",), tuple(w.worker_id for w in report.crashed_workers))
            self.assertEqual("failed", report.component_health[0].status)
            self.assertEqual("compensate", report.recovery_plans[0].strategy)
            self.assertEqual(
                "task_cancelled",
                report.recovery_plans[0].compensating_events[0].event_type,
            )


class RuntimeSupervisorHealthTests(unittest.TestCase):
    def test_stuck_worker_detection_uses_supplied_timestamp(self):
        with tempfile.TemporaryDirectory() as tmp:
            supervisor = RuntimeSupervisor(
                CheckpointManager(tmp),
                heartbeat_timeout=10,
            )
            workers = (
                WorkerHealth(
                    worker_id="worker_1",
                    task_id="task_1",
                    status="running",
                    last_heartbeat=89,
                    started_at=50,
                ),
                WorkerHealth(
                    worker_id="worker_2",
                    task_id="task_2",
                    status="running",
                    last_heartbeat=95,
                    started_at=90,
                ),
            )

            report = supervisor.assess_workers(workers, now=100)

            self.assertFalse(report.healthy)
            self.assertEqual(("worker_1",), tuple(w.worker_id for w in report.stuck_workers))
            self.assertEqual("degraded", report.component_health[0].status)

    def test_healthy_supplied_workers(self):
        with tempfile.TemporaryDirectory() as tmp:
            supervisor = RuntimeSupervisor(
                CheckpointManager(tmp),
                heartbeat_timeout=10,
            )
            workers = (
                WorkerHealth(
                    worker_id="worker_1",
                    task_id="task_1",
                    status="running",
                    last_heartbeat=99,
                    started_at=90,
                ),
            )

            report = supervisor.assess_workers(workers, now=100)

            self.assertTrue(report.healthy)
            self.assertEqual((), report.stuck_workers)
            self.assertEqual((), report.crashed_workers)
            self.assertEqual("healthy", report.component_health[0].status)

    def test_unknown_status_without_checkpoint(self):
        with tempfile.TemporaryDirectory() as tmp:
            supervisor = RuntimeSupervisor(CheckpointManager(tmp))

            self.assertEqual("unknown", supervisor.get_status("missing_task"))
            self.assertEqual("skip", supervisor.recover_task("missing_task").strategy)


if __name__ == "__main__":
    unittest.main()
