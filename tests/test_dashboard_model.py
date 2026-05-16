import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core.artifact_lifecycle import ArtifactLifecycleManager
from harness_core.dashboard_model import DashboardSnapshot
from harness_core.dag_manager import DAGNode, DAGState
from harness_core.health import HealthReport
from harness_core.supervisor import ComponentHealth, SupervisorReport


class DashboardSnapshotTests(unittest.TestCase):
    def test_dashboard_snapshot_construction(self):
        node = DAGNode(
            node_id="node_1",
            task_id="task_1",
            node_type="task",
            status="pending",
            tier="cheap_executor",
        )
        dag = DAGState(
            dag_id="dag_1",
            version=4,
            nodes=(node,),
            edges=(),
            created_at="2026-01-01T00:00:00Z",
            updated_at="2026-01-01T00:00:00Z",
        )
        component = ComponentHealth(
            component_id="runtime_supervisor",
            status="healthy",
            message="ok",
            checked_at=100,
        )
        supervisor_report = SupervisorReport(
            checked_at=100,
            healthy=True,
            stuck_workers=(),
            crashed_workers=(),
            component_health=(component,),
        )
        health_report = HealthReport(
            checked_at=100,
            overall_status="healthy",
            components=(component,),
        )
        artifacts = ArtifactLifecycleManager()
        artifact = artifacts.produce_artifact(
            artifact_id="artifact_1",
            task_id="task_1",
            artifact_type="patch",
            path="tmp/patch.diff",
            sha256="abc123",
        )

        snapshot = DashboardSnapshot.build(
            generated_at="2026-01-01T00:00:01Z",
            dag=dag,
            supervisor_report=supervisor_report,
            health_report=health_report,
            artifacts=(artifact,),
        )

        self.assertEqual("dag_1", snapshot.dag_id)
        self.assertEqual(4, snapshot.dag_version)
        self.assertEqual(1, snapshot.node_count)
        self.assertEqual(0, snapshot.edge_count)
        self.assertEqual((artifact,), snapshot.artifacts)
        self.assertTrue(snapshot.health_report.healthy)


if __name__ == "__main__":
    unittest.main()
