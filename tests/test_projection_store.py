import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core import replay_all, replay_project_state
from harness_core.errors import ReplayPreflightError
from harness_core.event_schema import canonical_event_json


FIXTURES = Path(__file__).resolve().parent / "fixtures"
BAD_FIXTURE = FIXTURES / "stage0_events_with_line17_issue.jsonl"
SANITIZED_FIXTURE = FIXTURES / "stage0_events_sanitized.jsonl"


class ProjectionStoreTests(unittest.TestCase):
    def test_projection_rejects_bad_line17_fixture(self):
        with self.assertRaises(ReplayPreflightError):
            replay_all(BAD_FIXTURE)

    def test_projection_accepts_sanitized_fixture(self):
        bundle = replay_all(SANITIZED_FIXTURE)

        self.assertEqual(5, len(bundle.project.items))

    def test_sanitized_project_items_are_done(self):
        projection = replay_project_state(SANITIZED_FIXTURE)

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

    def test_handoff_records_include_started_items(self):
        bundle = replay_all(SANITIZED_FIXTURE)

        self.assertEqual(
            ["item_003", "item_004", "item_005"],
            [handoff.item_id for handoff in bundle.task_queue.handoffs],
        )

    def test_dependency_projection_includes_resolved_edges(self):
        bundle = replay_all(SANITIZED_FIXTURE)

        self.assertEqual(
            ["edge_001_003", "edge_002_005"],
            [record.edge_id for record in bundle.dependencies.resolved],
        )

    def test_repeated_replay_is_deterministic(self):
        first = replay_all(SANITIZED_FIXTURE)
        second = replay_all(SANITIZED_FIXTURE)

        self.assertEqual(first, second)

    def test_unknown_event_type_is_ignored_with_warning(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            event = {
                "event_id": "evt_20260516_999999",
                "schema_version": "event.v1",
                "event_type": "future_event_type",
                "timestamp": "2026-05-16T10:00:00+08:00",
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
                "payload": {},
                "idempotency_key": "future:v1",
                "parent_event_id": None,
            }
            path.write_text(canonical_event_json(event) + "\n", encoding="utf-8")

            bundle = replay_all(path)

        self.assertEqual({}, bundle.project.items)
        self.assertEqual(1, len(bundle.warnings))
        self.assertEqual("UnknownEventTypeWarning", bundle.warnings[0].error_type)


class SanitizedFixtureTests(unittest.TestCase):
    def test_sanitized_fixture_has_unique_valid_events(self):
        event_ids = []
        with SANITIZED_FIXTURE.open("r", encoding="utf-8") as handle:
            for line in handle:
                event_ids.append(json.loads(line)["event_id"])

        self.assertEqual(18, len(event_ids))
        self.assertEqual(len(event_ids), len(set(event_ids)))


if __name__ == "__main__":
    unittest.main()
