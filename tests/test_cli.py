import subprocess
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SANITIZED_FIXTURE = ROOT / "tests" / "fixtures" / "stage0_events_sanitized.jsonl"
BAD_FIXTURE = ROOT / "tests" / "fixtures" / "stage0_events_with_line17_issue.jsonl"


def run_cli(*args):
    return subprocess.run(
        [sys.executable, "-m", "harness_core.cli", *args],
        cwd=ROOT,
        env={"PYTHONPATH": "src"},
        text=True,
        capture_output=True,
        check=False,
    )


class CliTests(unittest.TestCase):
    def test_validate_events_passes_sanitized_fixture(self):
        result = run_cli("validate-events", str(SANITIZED_FIXTURE))

        self.assertEqual(0, result.returncode)
        self.assertIn("JSONL validation: OK", result.stdout)
        self.assertIn("Replay preflight: OK (18 events)", result.stdout)

    def test_validate_events_fails_bad_line17_fixture(self):
        result = run_cli("validate-events", str(BAD_FIXTURE))

        self.assertNotEqual(0, result.returncode)
        self.assertIn("JSONL validation: FAIL", result.stdout)
        self.assertIn("line 17", result.stderr)
        self.assertIn("InvalidJsonLineError", result.stderr)
        self.assertNotIn("Replay preflight", result.stdout)

    def test_validate_events_verbose_runs_preflight_after_jsonl_error(self):
        result = run_cli("validate-events", "--verbose", str(BAD_FIXTURE))

        self.assertNotEqual(0, result.returncode)
        self.assertIn("JSONL validation: FAIL", result.stdout)
        self.assertIn("Replay preflight", result.stdout)

    def test_project_state_shows_five_done_items(self):
        result = run_cli("project-state", str(SANITIZED_FIXTURE))

        self.assertEqual(0, result.returncode)
        for item_id in ("item_001", "item_002", "item_003", "item_004", "item_005"):
            self.assertIn(f"{item_id} done", result.stdout)

    def test_task_queue_shows_expected_handoffs(self):
        result = run_cli("task-queue", str(SANITIZED_FIXTURE))

        self.assertEqual(0, result.returncode)
        self.assertIn("handoff_003 item_003 sequential", result.stdout)
        self.assertIn("handoff_004 item_004 sequential", result.stdout)
        self.assertIn("handoff_005 item_005 sequential", result.stdout)

    def test_digest_prints_expected_summary(self):
        result = run_cli("digest", str(SANITIZED_FIXTURE))

        self.assertEqual(0, result.returncode)
        self.assertIn("completed_items item_001,item_002,item_003,item_004,item_005", result.stdout)
        self.assertIn("handoff_count 3", result.stdout)
        self.assertIn("resolved_dependency_count 2", result.stdout)

    def test_cli_exits_nonzero_on_invalid_input_path(self):
        result = run_cli("validate-events", "does-not-exist.jsonl")

        self.assertNotEqual(0, result.returncode)
        self.assertIn("file not found", result.stderr)


if __name__ == "__main__":
    unittest.main()
