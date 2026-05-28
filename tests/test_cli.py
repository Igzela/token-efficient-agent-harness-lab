import json
import subprocess
import sys
import tempfile
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


class ValidateAliasTests(unittest.TestCase):
    def test_validate_alias_passes(self):
        result = run_cli("validate", str(SANITIZED_FIXTURE))

        self.assertEqual(0, result.returncode)
        self.assertIn("JSONL validation: OK", result.stdout)

    def test_validate_alias_fails_bad_file(self):
        result = run_cli("validate", str(BAD_FIXTURE))

        self.assertNotEqual(0, result.returncode)
        self.assertIn("JSONL validation: FAIL", result.stdout)

    def test_validate_alias_verbose(self):
        result = run_cli("validate", "--verbose", str(BAD_FIXTURE))

        self.assertNotEqual(0, result.returncode)
        self.assertIn("Replay preflight", result.stdout)


class DispatchTests(unittest.TestCase):
    def test_dispatch_creates_decision(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            json.dump({"text": "summarize the README file", "source": "cli"}, f)
            f.flush()
            result = run_cli("dispatch", f.name)

        self.assertEqual(0, result.returncode)
        output = json.loads(result.stdout)
        self.assertIn("dispatch_id", output)
        self.assertIn("decision", output)
        self.assertIn("selected_tier", output["decision"])
        self.assertIn("budget_reservation", output["decision"])

    def test_dispatch_missing_text_field(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            json.dump({"source": "cli"}, f)
            f.flush()
            result = run_cli("dispatch", f.name)

        self.assertNotEqual(0, result.returncode)
        self.assertIn("must contain a 'text' field", result.stderr)

    def test_dispatch_nonexistent_file(self):
        result = run_cli("dispatch", "/nonexistent/request.json")

        self.assertNotEqual(0, result.returncode)
        self.assertIn("file not found", result.stderr)

    def test_dispatch_invalid_json(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            f.write("not valid json")
            f.flush()
            result = run_cli("dispatch", f.name)

        self.assertNotEqual(0, result.returncode)
        self.assertIn("failed to read request file", result.stderr)

    def test_dispatch_non_object_json(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            json.dump(["not", "an", "object"], f)
            f.flush()
            result = run_cli("dispatch", f.name)

        self.assertNotEqual(0, result.returncode)
        self.assertIn("must contain a JSON object", result.stderr)


class PlansTests(unittest.TestCase):
    def _store_path(self):
        return tempfile.mktemp(suffix=".db")

    def test_plans_list_empty(self):
        result = run_cli("plans", "--store", self._store_path(), "list")

        self.assertEqual(0, result.returncode)
        self.assertIn("No plans stored.", result.stdout)

    def test_plans_show_not_found(self):
        result = run_cli("plans", "--store", self._store_path(), "show", "nonexistent")

        self.assertNotEqual(0, result.returncode)
        self.assertIn("plan not found", result.stderr)

    def test_plans_subcommand_required(self):
        result = run_cli("plans", "--store", self._store_path())

        self.assertNotEqual(0, result.returncode)


class ReposTests(unittest.TestCase):
    def _store_path(self):
        return tempfile.mktemp(suffix=".db")

    def test_repos_list_empty(self):
        result = run_cli("repos", "--store", self._store_path(), "list")

        self.assertEqual(0, result.returncode)
        self.assertIn("No repos registered.", result.stdout)

    def test_repos_add_valid_path(self):
        store_path = self._store_path()
        result = run_cli("repos", "--store", store_path, "add", str(ROOT))

        self.assertEqual(0, result.returncode)
        self.assertIn("Registered repo:", result.stdout)

        result_list = run_cli("repos", "--store", store_path, "list")
        self.assertEqual(0, result_list.returncode)
        self.assertIn("token-efficient-agent-harness-lab", result_list.stdout)

    def test_repos_add_nonexistent_path(self):
        result = run_cli("repos", "--store", self._store_path(), "add", "/nonexistent/path")

        self.assertNotEqual(0, result.returncode)
        self.assertIn("path not found", result.stderr)

    def test_repos_subcommand_required(self):
        result = run_cli("repos", "--store", self._store_path())

        self.assertNotEqual(0, result.returncode)


class HealthTests(unittest.TestCase):
    def test_health_output_format(self):
        result = run_cli("health")

        self.assertEqual(0, result.returncode)
        self.assertIn("Overall status: healthy", result.stdout)
        self.assertIn("storage:", result.stdout)
        self.assertIn("events:", result.stdout)
        self.assertIn("plans:", result.stdout)

    def test_health_exits_nonzero_when_unhealthy(self):
        result = run_cli("health")

        self.assertEqual(0, result.returncode)
        self.assertIn("healthy", result.stdout)


class StatusTests(unittest.TestCase):
    def test_status_output_format(self):
        result = run_cli("status")

        self.assertEqual(0, result.returncode)
        self.assertIn("System status: healthy", result.stdout)
        self.assertIn("Plans:", result.stdout)
        self.assertIn("Repos:", result.stdout)
        self.assertIn("Events:", result.stdout)
        self.assertIn("Migrations:", result.stdout)

    def test_status_shows_zero_counts(self):
        result = run_cli("status")

        self.assertEqual(0, result.returncode)
        self.assertIn("Plans:         0", result.stdout)
        self.assertIn("Repos:         0", result.stdout)
        self.assertIn("Events:        0", result.stdout)
        self.assertIn("Migrations:    0", result.stdout)


class HelpTests(unittest.TestCase):
    def test_main_help(self):
        result = run_cli("--help")

        self.assertEqual(0, result.returncode)
        self.assertIn("harness-core", result.stdout)

    def test_dispatch_help(self):
        result = run_cli("dispatch", "--help")

        self.assertEqual(0, result.returncode)
        self.assertIn("request_file", result.stdout)

    def test_plans_help(self):
        result = run_cli("plans", "--help")

        self.assertEqual(0, result.returncode)
        self.assertIn("list stored plans", result.stdout)
        self.assertIn("show plan details", result.stdout)

    def test_repos_help(self):
        result = run_cli("repos", "--help")

        self.assertEqual(0, result.returncode)
        self.assertIn("list registered repos", result.stdout)
        self.assertIn("register a repo", result.stdout)


class PlanStoreRoundtripTests(unittest.TestCase):
    def test_plans_show_after_dispatch_with_store(self):
        store_path = tempfile.mktemp(suffix=".db")
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            json.dump({"text": "analyze code quality"}, f)
            f.flush()
            result = run_cli("dispatch", "--store", store_path, f.name)

        self.assertEqual(0, result.returncode)

        plans_result = run_cli("plans", "--store", store_path, "list")
        self.assertEqual(0, plans_result.returncode)
        self.assertNotIn("No plans stored.", plans_result.stdout)


if __name__ == "__main__":
    unittest.main()
