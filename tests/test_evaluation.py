import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core import EvalSpec, EvaluationRunner


ROOT = Path(__file__).resolve().parents[1]
SANITIZED_FIXTURE = ROOT / "tests" / "fixtures" / "stage0_events_sanitized.jsonl"
BAD_FIXTURE = ROOT / "tests" / "fixtures" / "stage0_events_with_line17_issue.jsonl"


def ready_event(event_id="evt_20260515_000001", item_id="item_001"):
    return {
        "event_id": event_id,
        "schema_version": "event.v1",
        "event_type": "project_item_state_changed",
        "timestamp": "2026-05-15T20:00:00+08:00",
        "producer": {"component_id": "test", "component_type": "unit_test"},
        "correlation": {"batch_id": "b", "project_id": "p", "run_id": "r"},
        "severity": "info",
        "payload": {
            "project_id": "p",
            "board_version": 1,
            "item_id": item_id,
            "previous_status": "todo",
            "new_status": "ready",
            "reason": "test",
        },
        "idempotency_key": f"{item_id}:todo:ready:v1",
        "parent_event_id": None,
    }


def write_events(path, events):
    path.write_text(
        "".join(json.dumps(e, sort_keys=True, separators=(",", ":")) + "\n" for e in events),
        encoding="utf-8",
    )


class EvaluationRunnerTests(unittest.TestCase):
    def test_sanitized_fixture_passes(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            shutil.copyfile(SANITIZED_FIXTURE, path)
            spec = EvalSpec(case_id="sanitized", fixture_path=path, expected_outcome="pass")
            case = EvaluationRunner().run_single(spec)
        self.assertTrue(case.passed)
        self.assertEqual("pass", case.actual_outcome)

    def test_bad_line17_fixture_fails(self):
        spec = EvalSpec(case_id="bad_line17", fixture_path=BAD_FIXTURE, expected_outcome="fail")
        case = EvaluationRunner().run_single(spec)
        self.assertTrue(case.passed)
        self.assertEqual("fail", case.actual_outcome)

    def test_suite_aggregates(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            shutil.copyfile(SANITIZED_FIXTURE, path)
            cases = (
                EvalSpec(case_id="good", fixture_path=path, expected_outcome="pass"),
                EvalSpec(case_id="bad", fixture_path=BAD_FIXTURE, expected_outcome="fail"),
            )
            report = EvaluationRunner().run_suite("test_suite", cases)
        self.assertEqual(2, report.total)
        self.assertEqual(2, report.passed)
        self.assertEqual(0, report.failed)

    def test_one_failing_case_does_not_abort_suite(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            nonexistent = Path(temp_dir) / "nonexistent.jsonl"
            path = Path(temp_dir) / "events.jsonl"
            shutil.copyfile(SANITIZED_FIXTURE, path)
            cases = (
                EvalSpec(case_id="missing", fixture_path=nonexistent, expected_outcome="pass"),
                EvalSpec(case_id="good", fixture_path=path, expected_outcome="pass"),
            )
            report = EvaluationRunner().run_suite("test_suite", cases)
        self.assertEqual(2, report.total)
        self.assertEqual(1, report.passed)
        self.assertEqual(1, report.failed)

    def test_orchestrator_flow_case(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            write_events(path, [ready_event()])
            spec = EvalSpec(
                case_id="orch_flow",
                fixture_path=path,
                expected_outcome="pass",
                item_id="item_001",
                description="ready item exists",
            )
            case = EvaluationRunner().run_single(spec)
        self.assertTrue(case.passed)
        self.assertEqual("pass", case.actual_outcome)


if __name__ == "__main__":
    unittest.main()
