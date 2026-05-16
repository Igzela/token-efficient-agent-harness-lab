import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core import (
    validate_advisor_protocol_events,
    validate_allowed_files_completeness,
    validate_approval_request,
    validate_completion_record,
    validate_events_schema,
    validate_failure_code,
    validate_handoff_pack,
    validate_replay_preflight_check,
)


ROOT = Path(__file__).resolve().parents[1]
TASK_005 = ROOT / "docs" / "stage0" / "tasks" / "task-005-failure-fix-loop"
TASK_004 = ROOT / "docs" / "stage0" / "tasks" / "task-004-config-rule"
SANITIZED_FIXTURE = ROOT / "tests" / "fixtures" / "stage0_events_sanitized.jsonl"
BAD_FIXTURE = ROOT / "tests" / "fixtures" / "stage0_events_with_line17_issue.jsonl"


class ValidatorSuiteTests(unittest.TestCase):
    def test_events_schema_validator_accepts_valid_event(self):
        with SANITIZED_FIXTURE.open("r", encoding="utf-8") as handle:
            event = json.loads(handle.readline())

        result = validate_events_schema(event)

        self.assertTrue(result.ok)

    def test_completion_validator_accepts_task005_fixture(self):
        record = json.loads((TASK_005 / "completion.json").read_text(encoding="utf-8"))

        result = validate_completion_record(record)

        self.assertTrue(result.ok)

    def test_completion_validator_rejects_missing_exit_code(self):
        record = json.loads((TASK_005 / "completion.json").read_text(encoding="utf-8"))
        del record["exit_code"]

        result = validate_completion_record(record)

        self.assertFalse(result.ok)
        self.assertIn("missing required field: exit_code", result.errors)

    def test_handoff_pack_validator_accepts_task005_fixture(self):
        pack = json.loads((TASK_005 / "handoff_pack.json").read_text(encoding="utf-8"))

        result = validate_handoff_pack(pack)

        self.assertTrue(result.ok)

    def test_handoff_pack_validator_rejects_template_pack(self):
        pack = json.loads((TASK_004 / "handoff_pack.json").read_text(encoding="utf-8"))

        result = validate_handoff_pack(pack)

        self.assertFalse(result.ok)
        self.assertIn("_template must be false", result.errors)

    def test_approval_request_validator_accepts_pending_request(self):
        request = {
            "approval_id": "appr_20260515_001",
            "task_id": "stage0_task_004",
            "risk_level": "low",
            "requested_action": "modify_files",
            "summary": "fill approval_request template",
            "reason": "validate approval request shape",
            "affected_files": [{"path": "run_log.md", "change_type": "modify"}],
            "options": ["approve", "reject", "defer"],
            "timeout_policy": "no_timeout",
            "decision": "pending",
        }

        result = validate_approval_request(request)

        self.assertTrue(result.ok)

    def test_approval_request_validator_rejects_missing_timeout_policy(self):
        request = {
            "approval_id": "appr_20260515_001",
            "task_id": "stage0_task_004",
            "risk_level": "low",
            "requested_action": "modify_files",
            "summary": "fill approval_request template",
            "reason": "validate approval request shape",
            "affected_files": [],
            "options": [],
            "decision": "pending",
        }

        result = validate_approval_request(request)

        self.assertFalse(result.ok)
        self.assertIn("missing required field: timeout_policy", result.errors)

    def test_advisor_protocol_validator_accepts_task005_minimum_calls(self):
        result = validate_advisor_protocol_events(
            TASK_005 / "events.jsonl", expected_min_advisor_calls=2
        )

        self.assertTrue(result.ok)

    def test_advisor_protocol_validator_uses_task_specific_call_count(self):
        result = validate_advisor_protocol_events(
            TASK_005 / "events.jsonl", expected_min_advisor_calls=3
        )

        self.assertFalse(result.ok)

    def test_failure_code_validator_accepts_canonical_code(self):
        self.assertTrue(validate_failure_code("F008_FORMAT_ERROR").ok)

    def test_failure_code_validator_rejects_unknown_code(self):
        self.assertFalse(validate_failure_code("some_random_string").ok)

    def test_allowed_files_completeness_integration(self):
        result = validate_allowed_files_completeness(
            ["events.jsonl", "completion.json"],
            ["events.jsonl", "completion.json", "handoff_pack.json"],
        )

        self.assertFalse(result.ok)
        self.assertIn("missing allowed file: handoff_pack.json", result.errors)

    def test_replay_preflight_validator_accepts_sanitized_fixture(self):
        self.assertTrue(validate_replay_preflight_check(SANITIZED_FIXTURE).ok)

    def test_replay_preflight_validator_rejects_bad_fixture(self):
        self.assertFalse(validate_replay_preflight_check(BAD_FIXTURE).ok)


if __name__ == "__main__":
    unittest.main()
