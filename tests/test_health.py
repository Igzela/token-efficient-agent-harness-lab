import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core.health import HealthMonitor
from harness_core.supervisor import ComponentHealth, SupervisorReport, WorkerHealth


class HealthMonitorTests(unittest.TestCase):
    def test_health_report_aggregation(self):
        monitor = HealthMonitor()
        components = (
            ComponentHealth(
                component_id="checkpoint",
                status="healthy",
                message="ok",
                checked_at=100,
            ),
            ComponentHealth(
                component_id="supervisor",
                status="degraded",
                message="1 stuck worker",
                checked_at=100,
            ),
        )

        report = monitor.aggregate(components, checked_at=100)

        self.assertEqual("degraded", report.overall_status)
        self.assertFalse(report.healthy)
        self.assertEqual(("checkpoint", "supervisor"), tuple(c.component_id for c in report.components))

    def test_supervisor_report_aggregation(self):
        monitor = HealthMonitor()
        worker = WorkerHealth(
            worker_id="worker_1",
            task_id="task_1",
            status="running",
            last_heartbeat=1,
            started_at=1,
        )
        supervisor_report = SupervisorReport(
            checked_at=100,
            healthy=False,
            stuck_workers=(worker,),
            crashed_workers=(),
            component_health=(
                ComponentHealth(
                    component_id="runtime_supervisor",
                    status="degraded",
                    message="1 stuck worker(s)",
                    checked_at=100,
                ),
            ),
        )

        health = monitor.from_supervisor_report(supervisor_report)

        self.assertEqual("degraded", health.overall_status)
        self.assertEqual(("1 stuck worker(s)",), health.warnings)


if __name__ == "__main__":
    unittest.main()
