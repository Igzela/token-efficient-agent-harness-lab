import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core.artifact_lifecycle import ArtifactLifecycleManager


class ArtifactLifecycleTests(unittest.TestCase):
    def test_produce_artifact(self):
        manager = ArtifactLifecycleManager()

        artifact = manager.produce_artifact(
            artifact_id="artifact_1",
            task_id="task_1",
            artifact_type="patch",
            path="tmp/patch.diff",
            sha256="abc123",
        )

        self.assertEqual("produced", artifact.status)
        self.assertEqual(("draft", "produced"), (
            manager.list_transitions()[0].from_status,
            manager.list_transitions()[0].to_status,
        ))

    def test_verify_artifact(self):
        manager = ArtifactLifecycleManager()
        manager.produce_artifact(
            artifact_id="artifact_1",
            task_id="task_1",
            artifact_type="patch",
            path="tmp/patch.diff",
            sha256="abc123",
        )

        artifact = manager.verify_artifact("artifact_1")

        self.assertEqual("verified", artifact.status)

    def test_reject_artifact(self):
        manager = ArtifactLifecycleManager()
        manager.produce_artifact(
            artifact_id="artifact_1",
            task_id="task_1",
            artifact_type="patch",
            path="tmp/patch.diff",
            sha256="abc123",
        )

        artifact = manager.reject_artifact("artifact_1", reason="failed validation")

        self.assertEqual("rejected", artifact.status)
        self.assertEqual("failed validation", manager.list_transitions()[-1].reason)

    def test_promote_artifact(self):
        manager = ArtifactLifecycleManager()
        manager.produce_artifact(
            artifact_id="artifact_1",
            task_id="task_1",
            artifact_type="patch",
            path="tmp/patch.diff",
            sha256="abc123",
        )
        manager.verify_artifact("artifact_1")

        artifact = manager.promote_artifact("artifact_1")

        self.assertEqual("promoted", artifact.status)

    def test_archive_artifact(self):
        manager = ArtifactLifecycleManager()
        manager.produce_artifact(
            artifact_id="artifact_1",
            task_id="task_1",
            artifact_type="patch",
            path="tmp/patch.diff",
            sha256="abc123",
        )
        manager.verify_artifact("artifact_1")
        manager.promote_artifact("artifact_1")

        artifact = manager.archive_artifact("artifact_1")

        self.assertEqual("archived", artifact.status)

    def test_invalid_transition_rejected(self):
        manager = ArtifactLifecycleManager()
        manager.produce_artifact(
            artifact_id="artifact_1",
            task_id="task_1",
            artifact_type="patch",
            path="tmp/patch.diff",
            sha256="abc123",
        )

        with self.assertRaises(ValueError):
            manager.promote_artifact("artifact_1")

    def test_dependency_unlock_on_verified_artifact(self):
        manager = ArtifactLifecycleManager()
        manager.produce_artifact(
            artifact_id="artifact_1",
            task_id="task_1",
            artifact_type="patch",
            path="tmp/patch.diff",
            sha256="abc123",
        )
        locked = manager.dependency_unlock("artifact_1", "dep_1")
        manager.verify_artifact("artifact_1")

        unlocked = manager.dependency_unlock("artifact_1", "dep_1")

        self.assertFalse(locked.unlocked)
        self.assertTrue(unlocked.unlocked)


if __name__ == "__main__":
    unittest.main()
