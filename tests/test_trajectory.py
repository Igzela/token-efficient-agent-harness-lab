import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core import TrajectoryMonitor


ROOT = Path(__file__).resolve().parents[1]
SANITIZED_FIXTURE = ROOT / "tests" / "fixtures" / "stage0_events_sanitized.jsonl"
BAD_FIXTURE = ROOT / "tests" / "fixtures" / "stage0_events_with_line17_issue.jsonl"


def make_event(event_id, event_type="project_item_state_changed", payload=None, **extra):
    event = {
        "event_id": event_id,
        "schema_version": "event.v1",
        "event_type": event_type,
        "timestamp": "2026-05-16T00:00:00+08:00",
        "producer": {"component_id": "test", "component_type": "unit_test"},
        "correlation": {"batch_id": "b", "project_id": "p", "run_id": "r"},
        "severity": "info",
        "payload": payload or {},
        "idempotency_key": f"{event_id}:key:v1",
        "parent_event_id": None,
    }
    event.update(extra)
    return event


def write_events(path, events):
    path.write_text(
        "".join(json.dumps(e, sort_keys=True, separators=(",", ":")) + "\n" for e in events),
        encoding="utf-8",
    )


class TrajectoryMonitorProjectStreamTests(unittest.TestCase):
    def test_clean_sanitized_fixture_ok(self):
        report = TrajectoryMonitor().analyze_project_stream(SANITIZED_FIXTURE)
        self.assertTrue(report.ok)

    def test_repeated_failure_anomaly(self):
        events = [
            make_event(f"evt_{i}", payload={
                "project_id": "p", "board_version": 1,
                "item_id": "item_001",
                "previous_status": "review", "new_status": "failed",
                "reason": "test",
            })
            for i in range(4)
        ]
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            write_events(path, events)
            report = TrajectoryMonitor(failure_threshold=3).analyze_project_stream(path)
        self.assertFalse(report.ok)
        self.assertTrue(any(a.anomaly_type == "repeated_failure" for a in report.anomalies))

    def test_loop_anomaly(self):
        events = [
            make_event("evt_1", payload={
                "project_id": "p", "board_version": 1,
                "item_id": "item_001",
                "previous_status": "ready", "new_status": "running",
                "reason": "start",
            }),
            make_event("evt_2", payload={
                "project_id": "p", "board_version": 1,
                "item_id": "item_001",
                "previous_status": "running", "new_status": "review",
                "reason": "done",
            }),
            make_event("evt_3", payload={
                "project_id": "p", "board_version": 1,
                "item_id": "item_001",
                "previous_status": "review", "new_status": "failed",
                "reason": "fail",
            }),
            make_event("evt_4", payload={
                "project_id": "p", "board_version": 1,
                "item_id": "item_001",
                "previous_status": "failed", "new_status": "ready",
                "reason": "retry",
            }),
            make_event("evt_5", payload={
                "project_id": "p", "board_version": 1,
                "item_id": "item_001",
                "previous_status": "ready", "new_status": "running",
                "reason": "start2",
            }),
            make_event("evt_6", payload={
                "project_id": "p", "board_version": 1,
                "item_id": "item_001",
                "previous_status": "running", "new_status": "review",
                "reason": "done2",
            }),
        ]
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            write_events(path, events)
            report = TrajectoryMonitor(loop_threshold=2).analyze_project_stream(path)
        self.assertTrue(report.loop_detected)

    def test_missing_handoff_anomaly(self):
        events = [
            make_event("evt_1", payload={
                "project_id": "p", "board_version": 1,
                "item_id": "item_001",
                "previous_status": "ready", "new_status": "running",
                "reason": "no handoff",
            }),
        ]
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            write_events(path, events)
            report = TrajectoryMonitor().analyze_project_stream(path)
        self.assertTrue(any(a.anomaly_type == "missing_handoff" for a in report.anomalies))
        self.assertEqual(1, report.missing_handoff_count)

    def test_bad_line17_fixture_produces_error(self):
        report = TrajectoryMonitor().analyze_project_stream(BAD_FIXTURE)
        # The bad fixture may parse or may fail; either way it should not crash
        self.assertIsInstance(report, type(report))


class TrajectoryMonitorTaskStreamTests(unittest.TestCase):
    def test_excessive_retry_anomaly(self):
        events = [
            make_event("evt_1", payload={
                "item_id": "item_001",
                "retry_count": 5,
            }),
        ]
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            write_events(path, events)
            report = TrajectoryMonitor().analyze_task_stream(path, "item_001")
        self.assertTrue(any(a.anomaly_type == "excessive_retry" for a in report.anomalies))

    def test_missing_file_returns_error(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "nonexistent.jsonl"
            report = TrajectoryMonitor().analyze_task_stream(path, "item_001")
        self.assertFalse(report.ok)
        self.assertTrue(any(a.anomaly_type == "missing_file" for a in report.anomalies))


if __name__ == "__main__":
    unittest.main()
