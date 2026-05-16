import json
import shutil
import sys
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core import ArtifactGate, TaskRecordStore


ROOT = Path(__file__).resolve().parents[1]
TASK_005 = ROOT / "docs" / "stage0" / "tasks" / "task-005-failure-fix-loop"


def load_bundle(temp_dir: str):
    task_dir = Path(temp_dir) / TASK_005.name
    shutil.copytree(TASK_005, task_dir)
    return TaskRecordStore(Path(temp_dir)).load_task_bundle(task_dir)


class ArtifactGateTests(unittest.TestCase):
    def test_valid_bundle_passes(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            task_dir = Path(temp_dir) / TASK_005.name
            shutil.copytree(TASK_005, task_dir)
            store = TaskRecordStore(Path(temp_dir))
            bundle = store.load_task_bundle(task_dir)
            # Override artifact_refs to use task-dir-relative paths
            completion = dict(bundle.completion)
            completion["artifact_refs"] = [
                {"artifact_id": "run_log", "path": "run_log.md"}
            ]
            bundle = replace(bundle, completion=completion)
            result = ArtifactGate().evaluate(bundle)
        self.assertTrue(result.ok)
        self.assertTrue(all(c.passed for c in result.checks))

    def test_missing_artifact_fails(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle = load_bundle(temp_dir)
            completion = dict(bundle.completion)
            completion["artifact_refs"] = [
                {"artifact_id": "missing", "path": "nonexistent_file.txt"}
            ]
            bundle = replace(bundle, completion=completion)
            result = ArtifactGate().evaluate(bundle)
        self.assertFalse(result.ok)
        self.assertIn("nonexistent_file.txt", result.missing_artifacts)

    def test_missing_evidence_refs_fails(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle = load_bundle(temp_dir)
            handoff_pack = dict(bundle.handoff_pack)
            handoff_pack["evidence_refs"] = []
            bundle = replace(bundle, handoff_pack=handoff_pack)
            result = ArtifactGate().evaluate(bundle)
        self.assertFalse(result.ok)
        self.assertIn("evidence_refs", result.missing_artifacts)

    def test_forbidden_artifact_path_fails(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            task_dir = Path(temp_dir) / TASK_005.name
            shutil.copytree(TASK_005, task_dir)
            store = TaskRecordStore(Path(temp_dir))
            bundle = store.load_task_bundle(task_dir)
            completion = dict(bundle.completion)
            completion["artifact_refs"] = [
                {"artifact_id": "run_log", "path": "run_log.md"}
            ]
            bundle = replace(bundle, completion=completion)
            result = ArtifactGate().evaluate(
                bundle, forbidden_files=("run_log.md",)
            )
        self.assertFalse(result.ok)
        self.assertTrue(len(result.forbidden_violations) > 0)

    def test_allowed_files_missing_coverage_fails(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle = load_bundle(temp_dir)
            result = ArtifactGate().evaluate(
                bundle, allowed_files=("some_other_file.py",)
            )
        self.assertFalse(result.ok)
        self.assertTrue(any("not in allowed_files" in m for m in result.missing_artifacts))

    def test_invalid_completion_fails(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle = load_bundle(temp_dir)
            bundle = replace(bundle, completion={})
            result = ArtifactGate().evaluate(bundle)
        self.assertFalse(result.ok)
        self.assertTrue(len(result.schema_violations) > 0)

    def test_invalid_handoff_pack_fails(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            bundle = load_bundle(temp_dir)
            handoff_pack = dict(bundle.handoff_pack)
            handoff_pack["summary"] = ""
            bundle = replace(bundle, handoff_pack=handoff_pack)
            result = ArtifactGate().evaluate(bundle)
        self.assertFalse(result.ok)
        self.assertTrue(len(result.schema_violations) > 0)


if __name__ == "__main__":
    unittest.main()
