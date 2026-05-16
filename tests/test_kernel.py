import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core import Kernel, validate_jsonl_file
from harness_core.errors import ReplayPreflightError


ROOT = Path(__file__).resolve().parents[1]
SANITIZED_FIXTURE = ROOT / "tests" / "fixtures" / "stage0_events_sanitized.jsonl"
BAD_FIXTURE = ROOT / "tests" / "fixtures" / "stage0_events_with_line17_issue.jsonl"


def make_project_event(event_id="evt_20260515_000032"):
    return {
        "event_id": event_id,
        "schema_version": "event.v1",
        "event_type": "project_item_state_changed",
        "timestamp": "2026-05-15T22:10:00+08:00",
        "producer": {
            "component_id": "kernel_test",
            "component_type": "unit_test",
        },
        "correlation": {
            "batch_id": "batch_test",
            "project_id": "proj_test",
            "run_id": "kernel_test_run",
        },
        "severity": "info",
        "payload": {
            "project_id": "proj_test",
            "board_version": 1,
            "item_id": "item_999",
            "previous_status": "todo",
            "new_status": "ready",
            "reason": "kernel append test",
        },
        "idempotency_key": "item_999:todo:ready:v1",
        "parent_event_id": None,
    }


class KernelTests(unittest.TestCase):
    def test_validate_rejects_bad_line17_fixture(self):
        kernel = Kernel(BAD_FIXTURE)

        with self.assertRaises(ReplayPreflightError):
            kernel.validate()

    def test_validate_accepts_sanitized_fixture(self):
        report = Kernel(SANITIZED_FIXTURE).validate()

        self.assertTrue(report.ok)
        self.assertEqual(18, report.event_count)

    def test_project_state_projects_five_done_items(self):
        projection = Kernel(SANITIZED_FIXTURE).project_state()

        self.assertEqual(
            {
                "item_001": "done",
                "item_002": "done",
                "item_003": "done",
                "item_004": "done",
                "item_005": "done",
            },
            {item_id: item.status for item_id, item in projection.items.items()},
        )

    def test_append_project_event_to_temp_event_log(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            shutil.copyfile(SANITIZED_FIXTURE, path)
            kernel = Kernel(path)

            kernel.append_project_event(make_project_event())

            events = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()]

        self.assertEqual("evt_20260515_000032", events[-1]["event_id"])

    def test_append_preserves_jsonl_validity(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            shutil.copyfile(SANITIZED_FIXTURE, path)
            kernel = Kernel(path)

            kernel.append_project_event(make_project_event())
            report = validate_jsonl_file(path)

        self.assertTrue(report.ok)

    def test_append_to_protected_stage0_source_is_rejected(self):
        kernel = Kernel(ROOT / "docs" / "stage0" / "events.jsonl")

        with self.assertRaises(PermissionError):
            kernel.append_project_event(make_project_event())

    def test_append_to_protected_fixture_is_rejected(self):
        kernel = Kernel(SANITIZED_FIXTURE)

        with self.assertRaises(PermissionError):
            kernel.append_project_event(make_project_event())


if __name__ == "__main__":
    unittest.main()
