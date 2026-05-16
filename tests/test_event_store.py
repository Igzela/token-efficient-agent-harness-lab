import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core import EventStore, replay_preflight, validate_jsonl_file
from harness_core.errors import (
    DuplicateEventIdError,
    DuplicateIdempotencyConflictError,
    MissingNewlineError,
    SchemaViolationError,
)
from harness_core.event_schema import canonical_event_json, validate_event


def make_event(**overrides):
    event = {
        "event_id": "evt_20260516_000001",
        "schema_version": "event.v1",
        "event_type": "project_item_state_changed",
        "timestamp": "2026-05-16T10:00:00+08:00",
        "producer": {
            "component_id": "test_kernel",
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
            "item_id": "item_001",
            "previous_status": "ready",
            "new_status": "running",
            "reason": "unit test",
        },
        "idempotency_key": "item_001:ready:running:v1",
        "parent_event_id": None,
    }
    event.update(overrides)
    return event


class EventSchemaTests(unittest.TestCase):
    def test_valid_event_passes(self):
        validate_event(make_event())

    def test_wrong_schema_version_rejected(self):
        with self.assertRaises(SchemaViolationError):
            validate_event(make_event(schema_version="event.v2"))

    def test_missing_required_field_rejected(self):
        event = make_event()
        del event["payload"]

        with self.assertRaises(SchemaViolationError):
            validate_event(event)


class JsonlValidationTests(unittest.TestCase):
    def test_valid_jsonl_file_passes(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            path.write_text(canonical_event_json(make_event()) + "\n", encoding="utf-8")

            report = validate_jsonl_file(path)

        self.assertTrue(report.ok)
        self.assertEqual([], report.errors)

    def test_missing_newline_rejected(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            path.write_text(canonical_event_json(make_event()), encoding="utf-8")

            report = validate_jsonl_file(path)

        self.assertFalse(report.ok)
        self.assertEqual(MissingNewlineError.__name__, report.errors[0].error_type)

    def test_concatenated_json_objects_rejected(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            event = canonical_event_json(make_event())
            path.write_text(f"{event}{event}\n", encoding="utf-8")

            report = validate_jsonl_file(path)

        self.assertFalse(report.ok)
        self.assertTrue(
            any(error.error_type == "InvalidJsonLineError" for error in report.errors)
        )

    def test_stage0_line17_fixture_issue_detected(self):
        fixture = (
            Path(__file__).resolve().parent
            / "fixtures"
            / "stage0_events_with_line17_issue.jsonl"
        )

        report = replay_preflight(fixture)

        self.assertFalse(report.ok)
        self.assertTrue(
            any(
                error.line_number == 17
                and error.error_type == "InvalidJsonLineError"
                for error in report.errors
            )
        )


class EventStoreAppendTests(unittest.TestCase):
    def test_append_writes_one_canonical_json_line(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            event = make_event()

            EventStore(path).append_event(event)

            content = path.read_text(encoding="utf-8")

        self.assertEqual(canonical_event_json(event) + "\n", content)

    def test_duplicate_event_id_rejected(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            store = EventStore(path)
            store.append_event(make_event())

            with self.assertRaises(DuplicateEventIdError):
                store.append_event(
                    make_event(
                        idempotency_key="item_001:ready:running:different-key:v1"
                    )
                )

    def test_same_idempotency_key_same_semantic_hash_is_no_op(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            store = EventStore(path)
            store.append_event(make_event())

            store.append_event(
                make_event(
                    event_id="evt_20260516_000002",
                    timestamp="2026-05-16T10:01:00+08:00",
                )
            )

            lines = path.read_text(encoding="utf-8").splitlines()

        self.assertEqual(1, len(lines))
        self.assertEqual("evt_20260516_000001", json.loads(lines[0])["event_id"])

    def test_same_idempotency_key_different_semantic_hash_rejected(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            store = EventStore(path)
            store.append_event(make_event())

            with self.assertRaises(DuplicateIdempotencyConflictError):
                store.append_event(
                    make_event(
                        event_id="evt_20260516_000002",
                        payload={
                            "project_id": "proj_test",
                            "item_id": "item_001",
                            "previous_status": "ready",
                            "new_status": "blocked",
                            "reason": "different semantic payload",
                        },
                    )
                )


if __name__ == "__main__":
    unittest.main()
