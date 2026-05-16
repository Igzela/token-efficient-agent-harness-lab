import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core import BatchRunner, Kernel, validate_jsonl_file
from harness_core.errors import ReplayPreflightError


ROOT = Path(__file__).resolve().parents[1]
SANITIZED_FIXTURE = ROOT / "tests" / "fixtures" / "stage0_events_sanitized.jsonl"
BAD_FIXTURE = ROOT / "tests" / "fixtures" / "stage0_events_with_line17_issue.jsonl"


def ready_event(event_id="evt_20260515_000001", item_id="item_ready"):
    return {
        "event_id": event_id,
        "schema_version": "event.v1",
        "event_type": "project_item_state_changed",
        "timestamp": "2026-05-15T20:00:00+08:00",
        "producer": {
            "component_id": "test",
            "component_type": "unit_test",
        },
        "correlation": {
            "batch_id": "batch_test",
            "project_id": "proj_test",
            "run_id": "run_test",
        },
        "severity": "info",
        "payload": {
            "project_id": "proj_test",
            "board_version": 1,
            "item_id": item_id,
            "previous_status": "todo",
            "new_status": "ready",
            "reason": "ready fixture",
        },
        "idempotency_key": f"{item_id}:todo:ready:v1",
        "parent_event_id": None,
    }


def write_events(path: Path, events: list[dict]):
    path.write_text(
        "".join(json.dumps(event, sort_keys=True, separators=(",", ":")) + "\n" for event in events),
        encoding="utf-8",
    )


class BatchRunnerTests(unittest.TestCase):
    def test_lists_ready_item_from_custom_fixture(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            write_events(path, [ready_event()])

            ready_items = BatchRunner(Kernel(path)).list_ready_items()

        self.assertEqual(["item_ready"], [item.item_id for item in ready_items])

    def test_refuses_when_no_ready_items_exist(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            shutil.copyfile(SANITIZED_FIXTURE, path)

            with self.assertRaises(ValueError):
                BatchRunner(Kernel(path)).run_one_ready_item("item_001")

    def test_refuses_invalid_event_log(self):
        with self.assertRaises(ReplayPreflightError):
            BatchRunner(Kernel(BAD_FIXTURE)).list_ready_items()

    def test_refuses_item_that_is_not_ready(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            shutil.copyfile(SANITIZED_FIXTURE, path)

            with self.assertRaises(ValueError):
                BatchRunner(Kernel(path)).run_one_ready_item("item_005")

    def test_refuses_ready_item_that_already_has_handoff(self):
        event = ready_event(item_id="item_ready")
        handoff = {
            **ready_event(event_id="evt_20260515_000002", item_id="item_other"),
            "event_type": "project_to_queue_handoff_created",
            "payload": {
                "project_id": "proj_test",
                "item_id": "item_ready",
                "handoff_id": "handoff_item_ready",
                "scheduling_policy": "sequential",
            },
            "idempotency_key": "item_ready:handoff:v1",
            "parent_event_id": "evt_20260515_000001",
        }
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            write_events(path, [event, handoff])

            with self.assertRaises(ValueError):
                BatchRunner(Kernel(path)).run_one_ready_item("item_ready")

    def test_appends_running_handoff_and_review_events(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            write_events(path, [ready_event()])

            result = BatchRunner(Kernel(path)).run_one_ready_item("item_ready")
            events = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()]

        self.assertEqual(
            ("evt_20260515_000002", "evt_20260515_000003", "evt_20260515_000004"),
            result.appended_event_ids,
        )
        self.assertEqual(
            [
                "project_item_state_changed",
                "project_to_queue_handoff_created",
                "project_item_state_changed",
            ],
            [event["event_type"] for event in events[-3:]],
        )
        self.assertEqual("running", events[-3]["payload"]["new_status"])
        self.assertEqual("review", events[-1]["payload"]["new_status"])

    def test_event_appends_preserve_jsonl_validity_and_digest_is_generated(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            write_events(path, [ready_event()])

            result = BatchRunner(Kernel(path)).run_one_ready_item("item_ready")
            report = validate_jsonl_file(path)

        self.assertTrue(report.ok)
        self.assertEqual(1, result.digest.handoff_count)

    def test_run_one_ready_item_projects_item_to_review(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            write_events(path, [ready_event()])

            BatchRunner(Kernel(path)).run_one_ready_item("item_ready")
            projection = Kernel(path).project_state()

        self.assertEqual("review", projection.items["item_ready"].status)

    def test_planned_events_validate_before_append(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            write_events(path, [ready_event()])
            runner = BatchRunner(Kernel(path))

            planned = runner._plan_events("item_ready")

        self.assertEqual(3, len(planned))
        for event in planned:
            self.assertIn("event_id", event)
            self.assertEqual("event.v1", event["schema_version"])

    def test_invalid_planned_event_prevents_partial_append(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            write_events(path, [ready_event()])
            runner = BatchRunner(Kernel(path))
            invalid_event = runner._plan_events("item_ready")[0]
            del invalid_event["payload"]
            runner._plan_events = lambda item_id: [invalid_event]

            with self.assertRaises(Exception):
                runner.run_one_ready_item("item_ready")

            lines = path.read_text(encoding="utf-8").splitlines()

        self.assertEqual(1, len(lines))

    def test_event_id_generation_falls_back_when_existing_ids_have_no_suffix(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            write_events(path, [ready_event(event_id="custom", item_id="item_ready")])

            result = BatchRunner(Kernel(path)).run_one_ready_item("item_ready")

        self.assertEqual(("evt_000001", "evt_000002", "evt_000003"), result.appended_event_ids)


if __name__ == "__main__":
    unittest.main()
