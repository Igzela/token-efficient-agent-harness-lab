import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core import TaskRecordStore


ROOT = Path(__file__).resolve().parents[1]
STAGE0_TASKS = ROOT / "docs" / "stage0" / "tasks"
TASK_005 = STAGE0_TASKS / "task-005-failure-fix-loop"


def copy_task_fixture(temp_dir: str, source: Path = TASK_005) -> Path:
    target = Path(temp_dir) / source.name
    shutil.copytree(source, target)
    return target


class TaskRecordStoreTests(unittest.TestCase):
    def test_loads_valid_stage0_style_task_directory_copied_to_temp(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            task_dir = copy_task_fixture(temp_dir)
            store = TaskRecordStore(Path(temp_dir))

            bundle = store.load_task_bundle(task_dir)
            report = store.validate_task_bundle(task_dir)

        self.assertEqual("stage0_task_005", bundle.completion["task_id"])
        self.assertTrue(report.ok)
        self.assertEqual((), report.errors)

    def test_find_task_dirs_returns_task_directories(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            task_dir = copy_task_fixture(temp_dir)

            task_dirs = TaskRecordStore(Path(temp_dir)).find_task_dirs()

        self.assertEqual([task_dir], task_dirs)

    def test_detects_missing_task_spec(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            task_dir = copy_task_fixture(temp_dir)
            (task_dir / "task_spec.json").unlink()

            report = TaskRecordStore(Path(temp_dir)).validate_task_bundle(task_dir)

        self.assertFalse(report.ok)
        self.assertIn("missing required file: task_spec.json", report.errors)

    def test_detects_missing_completion(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            task_dir = copy_task_fixture(temp_dir)
            (task_dir / "completion.json").unlink()

            report = TaskRecordStore(Path(temp_dir)).validate_task_bundle(task_dir)

        self.assertFalse(report.ok)
        self.assertIn("missing required file: completion.json", report.errors)

    def test_detects_invalid_completion(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            task_dir = copy_task_fixture(temp_dir)
            completion_path = task_dir / "completion.json"
            completion = json.loads(completion_path.read_text(encoding="utf-8"))
            del completion["exit_code"]
            completion_path.write_text(json.dumps(completion) + "\n", encoding="utf-8")

            report = TaskRecordStore(Path(temp_dir)).validate_task_bundle(task_dir)

        self.assertFalse(report.ok)
        self.assertIn("completion.json: missing required field: exit_code", report.errors)

    def test_detects_invalid_handoff_pack(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            task_dir = copy_task_fixture(temp_dir)
            handoff_path = task_dir / "handoff_pack.json"
            handoff = json.loads(handoff_path.read_text(encoding="utf-8"))
            handoff["evidence_refs"] = []
            handoff_path.write_text(json.dumps(handoff) + "\n", encoding="utf-8")

            report = TaskRecordStore(Path(temp_dir)).validate_task_bundle(task_dir)

        self.assertFalse(report.ok)
        self.assertIn("handoff_pack.json: evidence_refs must be a non-empty list", report.errors)

    def test_does_not_execute_commands_or_task_content(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            task_dir = copy_task_fixture(temp_dir)
            marker = Path(temp_dir) / "command_was_executed"
            run_log_path = task_dir / "run_log.md"
            run_log_path.write_text(
                f"Do not execute: touch {marker}\n",
                encoding="utf-8",
            )

            report = TaskRecordStore(Path(temp_dir)).validate_task_bundle(task_dir)

        self.assertTrue(report.ok)
        self.assertFalse(marker.exists())

    def test_treats_run_log_as_evidence_only(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            task_dir = copy_task_fixture(temp_dir)

            bundle = TaskRecordStore(Path(temp_dir)).load_task_bundle(task_dir)

        self.assertEqual(task_dir / "run_log.md", bundle.run_log_path)
        self.assertIsNotNone(bundle.run_log_text)
        self.assertIn("Advisor", bundle.run_log_text)

    def test_leaves_source_docs_stage0_unchanged(self):
        before = {
            path.relative_to(TASK_005): path.read_bytes()
            for path in sorted(TASK_005.iterdir())
            if path.is_file()
        }

        report = TaskRecordStore(STAGE0_TASKS).validate_task_bundle(TASK_005)

        after = {
            path.relative_to(TASK_005): path.read_bytes()
            for path in sorted(TASK_005.iterdir())
            if path.is_file()
        }
        self.assertTrue(report.ok)
        self.assertEqual(before, after)


if __name__ == "__main__":
    unittest.main()
