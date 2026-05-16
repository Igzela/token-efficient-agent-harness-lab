import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core import (
    FinalGateRunner,
    Kernel,
    Stage1Orchestrator,
    TaskRecordStore,
    validate_jsonl_file,
)
from harness_core.errors import ReplayPreflightError


ROOT = Path(__file__).resolve().parents[1]
SANITIZED_FIXTURE = ROOT / "tests" / "fixtures" / "stage0_events_sanitized.jsonl"
BAD_FIXTURE = ROOT / "tests" / "fixtures" / "stage0_events_with_line17_issue.jsonl"
TASK_005 = ROOT / "docs" / "stage0" / "tasks" / "task-005-failure-fix-loop"


def ready_event(event_id="evt_20260515_000001", item_id="item_001"):
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
        "".join(
            json.dumps(event, sort_keys=True, separators=(",", ":")) + "\n"
            for event in events
        ),
        encoding="utf-8",
    )


class OrchestratorValidationTests(unittest.TestCase):
    def test_rejects_bad_line17_fixture(self):
        with self.assertRaises(ReplayPreflightError):
            Stage1Orchestrator(BAD_FIXTURE).validate()

    def test_accepts_sanitized_fixture(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            shutil.copyfile(SANITIZED_FIXTURE, path)
            report = Stage1Orchestrator(path).validate()
        self.assertTrue(report.ok)

    def test_digest_on_sanitized_fixture(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            shutil.copyfile(SANITIZED_FIXTURE, path)
            digest = Stage1Orchestrator(path).digest()
        self.assertIsInstance(digest.completed_items, tuple)
        self.assertIsInstance(digest.handoff_count, int)


class OrchestratorReadyItemTests(unittest.TestCase):
    def test_lists_ready_item(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            write_events(path, [ready_event()])
            items = Stage1Orchestrator(path).list_ready_items()
        self.assertEqual(["item_001"], [item.item_id for item in items])

    def test_run_ready_item_moves_to_review_not_done(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            write_events(path, [ready_event()])
            orch = Stage1Orchestrator(path)
            result = orch.run_ready_item("item_001")

            self.assertEqual("run_ready_item", result.action)
            self.assertEqual("item_001", result.item_id)
            self.assertEqual("review", result.next_status)
            self.assertEqual(3, len(result.appended_event_ids))

            projection = Kernel(path).project_state()
            self.assertEqual("review", projection.items["item_001"].status)


class OrchestratorFinalGateTests(unittest.TestCase):
    def test_evaluate_final_gate_valid_bundle_passes(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            write_events(path, [ready_event()])

            orch = Stage1Orchestrator(path)
            orch.run_ready_item("item_001")

            task_dir = Path(temp_dir) / TASK_005.name
            shutil.copytree(TASK_005, task_dir)
            orch.task_root = Path(temp_dir)

            decision = orch.evaluate_final_gate("item_001", task_dir)
            self.assertEqual("pass", decision.result)
            self.assertEqual("done", decision.next_project_status)

    def test_apply_final_gate_decision_appends_review_to_done(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            write_events(path, [ready_event()])

            orch = Stage1Orchestrator(path)
            orch.run_ready_item("item_001")

            task_dir = Path(temp_dir) / TASK_005.name
            shutil.copytree(TASK_005, task_dir)
            orch.task_root = Path(temp_dir)

            decision = orch.evaluate_final_gate("item_001", task_dir)
            result = orch.apply_final_gate_decision("item_001", decision)

            self.assertEqual("apply_final_gate_decision", result.action)
            self.assertEqual("pass", result.final_gate_result)
            self.assertEqual("done", result.next_status)
            self.assertEqual(1, len(result.appended_event_ids))

            projection = Kernel(path).project_state()
            self.assertEqual("done", projection.items["item_001"].status)


class OrchestratorFullFlowTests(unittest.TestCase):
    def test_full_deterministic_flow_ready_to_done(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            write_events(path, [ready_event()])

            orch = Stage1Orchestrator(path)

            step1 = orch.run_one_step(item_id="item_001")
            self.assertEqual("run_ready_item", step1.action)
            self.assertEqual("review", step1.next_status)

            task_dir = Path(temp_dir) / TASK_005.name
            shutil.copytree(TASK_005, task_dir)

            step2 = orch.run_one_step(item_id="item_001", task_dir=task_dir)
            self.assertEqual("apply_final_gate_decision", step2.action)
            self.assertEqual("pass", step2.final_gate_result)
            self.assertEqual("done", step2.next_status)

            projection = Kernel(path).project_state()
            self.assertEqual("done", projection.items["item_001"].status)

    def test_no_ready_item_returns_no_op_without_appending(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            shutil.copyfile(SANITIZED_FIXTURE, path)
            lines_before = path.read_text(encoding="utf-8")

            orch = Stage1Orchestrator(path)
            result = orch.run_one_step()

            self.assertEqual("no_op", result.action)
            self.assertIsNone(result.item_id)
            self.assertEqual(0, len(result.appended_event_ids))
            self.assertIn("no ready items", result.warnings[0])

            lines_after = path.read_text(encoding="utf-8")
            self.assertEqual(lines_before, lines_after)

    def test_invalid_task_bundle_causes_fail_not_done(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            write_events(path, [ready_event()])

            orch = Stage1Orchestrator(path)
            orch.run_one_step(item_id="item_001")

            task_dir = Path(temp_dir) / "bad_task"
            task_dir.mkdir()
            (task_dir / "task_spec.json").write_text("{}", encoding="utf-8")
            (task_dir / "completion.json").write_text("{}", encoding="utf-8")
            (task_dir / "handoff_pack.json").write_text("{}", encoding="utf-8")
            (task_dir / "events.jsonl").write_text(
                json.dumps(ready_event(event_id="evt_bad", item_id="item_001"))
                + "\n",
                encoding="utf-8",
            )

            decision = orch.evaluate_final_gate("item_001", task_dir)
            self.assertEqual("fail", decision.result)

            projection = Kernel(path).project_state()
            self.assertEqual("review", projection.items["item_001"].status)

    def test_event_log_remains_valid_jsonl_after_appends(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            write_events(path, [ready_event()])

            orch = Stage1Orchestrator(path)
            orch.run_one_step(item_id="item_001")

            task_dir = Path(temp_dir) / TASK_005.name
            shutil.copytree(TASK_005, task_dir)
            orch.run_one_step(item_id="item_001", task_dir=task_dir)

            report = validate_jsonl_file(path)
        self.assertTrue(report.ok)

    def test_docs_stage0_events_jsonl_never_modified(self):
        stage0_events = ROOT / "docs" / "stage0" / "events.jsonl"
        if not stage0_events.exists():
            self.skipTest("docs/stage0/events.jsonl does not exist")
        before = stage0_events.read_bytes()

        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            shutil.copyfile(SANITIZED_FIXTURE, path)
            orch = Stage1Orchestrator(path)
            orch.validate()

        after = stage0_events.read_bytes()
        self.assertEqual(before, after)


class OrchestratorQualityHookTests(unittest.TestCase):
    def test_evaluate_quality_returns_decision(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            write_events(path, [ready_event()])

            orch = Stage1Orchestrator(path)
            orch.run_one_step(item_id="item_001")

            task_dir = Path(temp_dir) / TASK_005.name
            shutil.copytree(TASK_005, task_dir)

            # Override artifact_refs to use task-dir-relative paths
            import json as _json
            completion_path = task_dir / "completion.json"
            completion = _json.loads(completion_path.read_text(encoding="utf-8"))
            completion["artifact_refs"] = [{"artifact_id": "run_log", "path": "run_log.md"}]
            completion_path.write_text(_json.dumps(completion, indent=2), encoding="utf-8")

            decision = orch.evaluate_quality("item_001", task_dir)
        self.assertIn(decision.result, ("pass", "pass_with_notes"))
        self.assertEqual("done", decision.next_project_status)

    def test_existing_run_one_step_unchanged(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "events.jsonl"
            write_events(path, [ready_event()])
            orch = Stage1Orchestrator(path)
            result = orch.run_one_step(item_id="item_001")
        self.assertEqual("run_ready_item", result.action)
        self.assertEqual("review", result.next_status)


if __name__ == "__main__":
    unittest.main()
