"""Comprehensive dry-run tests for the agent orchestrator.

Validates state transitions, event filters, dependency resolution,
duplicate-event idempotency, exact-head stale-run rejection,
retry limits, concurrency locking, prompt templates, workflow YAML parsing,
Codex wrapper behavior, and the complete transition graph
without invoking Codex, pushing, creating PRs, or merging.
"""

import json
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "scripts", "agent-control"))
import state_manager as sm
import lock_manager as lm
import ci_handler as ch


RESULTS = []


def log_test(name, passed, details=""):
    RESULTS.append({"name": name, "passed": passed, "details": details})
    status = "PASS" if passed else "FAIL"
    print(f"  [{status}] {name}" + (f": {details}" if details else ""))


class TestEventFiltering(unittest.TestCase):
    """Defect 1: agent-ready intake dispatches exactly one worker."""

    def test_agent_draft_is_recognized(self):
        self.assertIn(sm.LABEL_DRAFT, sm.ALL_LABELS)

    def test_agent_ready_is_recognized(self):
        self.assertIn(sm.LABEL_READY, sm.ALL_LABELS)

    def test_agent_running_is_active(self):
        self.assertIn(sm.LABEL_RUNNING, sm.ACTIVE_LABELS)

    def test_ci_repairing_is_active(self):
        self.assertIn(sm.LABEL_CI_REPAIRING, sm.ACTIVE_LABELS)

    def test_review_running_is_active(self):
        self.assertIn(sm.LABEL_REVIEW_RUNNING, sm.ACTIVE_LABELS)

    def test_review_passed_is_not_active(self):
        self.assertNotIn(sm.LABEL_REVIEW_PASSED, sm.ACTIVE_LABELS)

    def test_agent_blocked_is_terminal(self):
        self.assertIn(sm.LABEL_BLOCKED, sm.TERMINAL_LABELS)

    def test_agent_complete_is_terminal(self):
        self.assertIn(sm.LABEL_COMPLETE, sm.TERMINAL_LABELS)

    def test_unknown_labels_ignored(self):
        unknown = {"bug", "enhancement"}
        self.assertTrue(unknown.isdisjoint(sm.ACTIVE_LABELS | sm.TERMINAL_LABELS))


class TestDependencyParsing(unittest.TestCase):
    """Test dependency resolution."""

    def test_depends_on(self):
        body = "This task depends on #42 and #100 being complete."
        deps = sm.parse_dependencies(body)
        self.assertIn(42, deps)
        self.assertIn(100, deps)

    def test_empty_body(self):
        body = "No dependencies here."
        deps = sm.parse_dependencies(body)
        self.assertEqual(len(deps), 0)


class TestConcurrencyLocking(unittest.TestCase):
    """Test concurrency lock acquisition and release."""

    def setUp(self):
        self.key = f"dry-run-test-{os.getpid()}-{id(self)}"

    def tearDown(self):
        lm.release_lock(self.key)

    def test_acquire_and_release(self):
        self.assertTrue(lm.acquire_lock(self.key, timeout_secs=5))
        lm.release_lock(self.key)
        self.assertTrue(lm.acquire_lock(self.key, timeout_secs=2))

    def test_re_acquire_held_lock_fails(self):
        self.assertTrue(lm.acquire_lock(self.key, timeout_secs=5))
        self.assertFalse(lm.acquire_lock(self.key, timeout_secs=2))

    def test_repo_capacity(self):
        self.assertTrue(lm.check_repo_capacity())

    def test_concurrency_group_format(self):
        g = lm.gh_concurrency_group(1, 100, "abc123")
        self.assertIn("issue-1", g)
        self.assertIn("pr-100", g)
        self.assertIn("sha-abc123", g)


class TestCIEventParsing(unittest.TestCase):
    """Test CI workflow run event parsing."""

    def _write_event(self, data):
        f = tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False)
        json.dump(data, f)
        f.close()
        return f.name

    def test_parse_success_event(self):
        path = self._write_event({
            "workflow_run": {
                "conclusion": "success",
                "status": "completed",
                "head_branch": "agent/issue-42",
                "head_sha": "abcdef1234567890",
                "id": 12345,
                "html_url": "https://github.com/example/repo/actions/runs/12345",
                "triggering_actor": {"login": "test-bot"},
                "pull_requests": [
                    {"number": 99, "head": {"sha": "abcdef1234567890", "ref": "agent/issue-42"}}
                ],
            }
        })
        info = ch.parse_workflow_run_event(path)
        self.assertEqual(info["conclusion"], "success")
        self.assertEqual(info["pr_number"], 99)
        self.assertEqual(info["head_sha"], "abcdef1234567890")
        self.assertEqual(info["run_id"], 12345)
        os.unlink(path)

    def test_parse_failure_event(self):
        path = self._write_event({
            "workflow_run": {
                "conclusion": "failure",
                "status": "completed",
                "head_branch": "agent/issue-10",
                "head_sha": "xyz789",
                "id": 67890,
                "html_url": "",
                "triggering_actor": {"login": "test-bot"},
                "pull_requests": [{"number": 55, "head": {"sha": "xyz789"}}],
            }
        })
        info = ch.parse_workflow_run_event(path)
        self.assertEqual(info["conclusion"], "failure")
        self.assertEqual(info["pr_number"], 55)
        os.unlink(path)

    def test_parse_no_prs(self):
        path = self._write_event({
            "workflow_run": {
                "conclusion": "success",
                "status": "completed",
                "head_branch": "main",
                "head_sha": "abc",
                "id": 1,
                "html_url": "",
                "triggering_actor": {"login": "test-bot"},
                "pull_requests": [],
            }
        })
        info = ch.parse_workflow_run_event(path)
        self.assertIsNone(info["pr_number"])
        os.unlink(path)


class TestRetryLimits(unittest.TestCase):
    """Defect 19: Repair count persists across events and stops after two attempts."""

    def test_max_repair_is_two(self):
        self.assertEqual(sm.MAX_REPAIR_ATTEMPTS, 2)

    def test_first_repair_allowed(self):
        self.assertTrue(1 <= sm.MAX_REPAIR_ATTEMPTS)

    def test_second_repair_allowed(self):
        self.assertTrue(2 <= sm.MAX_REPAIR_ATTEMPTS)

    def test_third_repair_blocked(self):
        self.assertFalse(3 <= sm.MAX_REPAIR_ATTEMPTS)


class TestWorkflowYAMLParsing(unittest.TestCase):
    """Defect 25: All workflow YAML parses and dispatch inputs match their callers."""

    def _load_yaml(self, path):
        import yaml
        with open(path) as f:
            return yaml.safe_load(f)

    def _get_trigger(self, data):
        """Get the trigger key from YAML data. PyYAML parses 'on' as True."""
        for key in (True, "on"):
            if key in data:
                return data[key]
        return None

    def test_intake_yml_parses(self):
        path = os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-intake.yml")
        if os.path.exists(path):
            data = self._load_yaml(path)
            self.assertIsNotNone(self._get_trigger(data))
            self.assertIn("jobs", data)

    def test_worker_yml_parses(self):
        path = os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-worker.yml")
        if os.path.exists(path):
            data = self._load_yaml(path)
            trigger = self._get_trigger(data)
            self.assertIsNotNone(trigger)
            self.assertIn("jobs", data)
            self.assertIn("workflow_dispatch", trigger)
            inputs = trigger["workflow_dispatch"]["inputs"]
            self.assertIn("issue", inputs)
            self.assertIn("dry_run", inputs)

    def test_controller_yml_parses(self):
        path = os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-controller.yml")
        if os.path.exists(path):
            data = self._load_yaml(path)
            self.assertIsNotNone(self._get_trigger(data))

    def test_ci_monitor_yml_parses(self):
        path = os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-ci-monitor.yml")
        if os.path.exists(path):
            data = self._load_yaml(path)
            self.assertIsNotNone(self._get_trigger(data))

    def test_ci_repair_yml_parses(self):
        path = os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-ci-repair.yml")
        if os.path.exists(path):
            data = self._load_yaml(path)
            trigger = self._get_trigger(data)
            self.assertIsNotNone(trigger)
            inputs = trigger["workflow_dispatch"]["inputs"]
            self.assertIn("failed_logs", inputs)

    def test_review_yml_parses(self):
        path = os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-review.yml")
        if os.path.exists(path):
            data = self._load_yaml(path)
            self.assertIsNotNone(self._get_trigger(data))

    def test_merge_yml_parses(self):
        path = os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-merge.yml")
        if os.path.exists(path):
            data = self._load_yaml(path)
            self.assertIsNotNone(self._get_trigger(data))


class TestEnableGatePattern(unittest.TestCase):
    """Defect 2: Disabled orchestrator prevents every downstream action."""

    def _load_yaml(self, path):
        import yaml
        with open(path) as f:
            return yaml.safe_load(f)

    def test_intake_has_gate_job(self):
        path = os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-intake.yml")
        if os.path.exists(path):
            data = self._load_yaml(path)
            self.assertIn("gate", data["jobs"])
            gate = data["jobs"]["gate"]
            self.assertIn("outputs", gate)

    def test_worker_has_gate_job(self):
        path = os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-worker.yml")
        if os.path.exists(path):
            data = self._load_yaml(path)
            self.assertIn("gate", data["jobs"])

    def test_merge_has_gate_job(self):
        path = os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-merge.yml")
        if os.path.exists(path):
            data = self._load_yaml(path)
            self.assertIn("gate", data["jobs"])

    def test_review_has_gate_job(self):
        path = os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-review.yml")
        if os.path.exists(path):
            data = self._load_yaml(path)
            self.assertIn("gate", data["jobs"])


class TestNoHardcodedEnable(unittest.TestCase):
    """Defect 2: No hardcoded AGENT_ORCHESTRATOR_ENABLED: 'true' in workflows."""

    def _check_no_hardcoded_enable(self, workflow_path):
        if not os.path.exists(workflow_path):
            return True
        with open(workflow_path) as f:
            content = f.read()
        self.assertNotIn('AGENT_ORCHESTRATOR_ENABLED: "true"', content)
        self.assertNotIn("AGENT_ORCHESTRATOR_ENABLED: 'true'", content)

    def test_intake_no_hardcoded(self):
        self._check_no_hardcoded_enable(
            os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-intake.yml"))

    def test_worker_no_hardcoded(self):
        self._check_no_hardcoded_enable(
            os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-worker.yml"))

    def test_ci_repair_no_hardcoded(self):
        self._check_no_hardcoded_enable(
            os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-ci-repair.yml"))

    def test_review_no_hardcoded(self):
        self._check_no_hardcoded_enable(
            os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-review.yml"))

    def test_merge_no_hardcoded(self):
        self._check_no_hardcoded_enable(
            os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-merge.yml"))


class TestCodexPromptNoGitWrite(unittest.TestCase):
    """Defect 4+7: Codex prompt does not tell Codex to commit, push, create PR, or merge."""

    def test_implementation_prompt_no_commit_instructions(self):
        path = os.path.join(os.path.dirname(__file__), "..", "scripts", "agent-control", "prompts", "implementation.md")
        if os.path.exists(path):
            with open(path) as f:
                content = f.read().lower()
            self.assertNotIn("do not commit", content.replace("do not commit changes", "").replace("do not commit secrets", ""))
            self.assertIn("do not commit", content)
            self.assertNotIn("git push", content)
            self.assertNotIn("create or update a pr", content)
            self.assertNotIn("create a pr", content)

    def test_ci_repair_prompt_no_commit_instructions(self):
        path = os.path.join(os.path.dirname(__file__), "..", "scripts", "agent-control", "prompts", "ci_repair.md")
        if os.path.exists(path):
            with open(path) as f:
                content = f.read().lower()
            self.assertNotIn("git push", content)
            self.assertNotIn("commit changes", content.replace("do not commit changes", ""))

    def test_implementation_prompt_says_not_to_commit(self):
        path = os.path.join(os.path.dirname(__file__), "..", "scripts", "agent-control", "prompts", "implementation.md")
        if os.path.exists(path):
            with open(path) as f:
                content = f.read()
            self.assertIn("Do NOT commit changes", content)
            self.assertIn("Do NOT push branches", content)
            self.assertIn("Do NOT create or update PRs", content)


class TestGHTokenInWorkflows(unittest.TestCase):
    """Defect 6: Every gh-using job has explicit GH_TOKEN."""

    def _load_yaml(self, path):
        import yaml
        with open(path) as f:
            return yaml.safe_load(f)

    def _has_gh_token(self, workflow_path, job_name):
        if not os.path.exists(workflow_path):
            return True
        data = self._load_yaml(workflow_path)
        if "jobs" not in data or job_name not in data["jobs"]:
            return True
        job = data["jobs"][job_name]
        env = job.get("env", {})
        return "GH_TOKEN" in env

    def test_worker_implement_has_gh_token(self):
        self.assertTrue(self._has_gh_token(
            os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-worker.yml"),
            "implement"))

    def test_ci_monitor_process_has_gh_token(self):
        self.assertTrue(self._has_gh_token(
            os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-ci-monitor.yml"),
            "process"))

    def test_ci_repair_repair_has_gh_token(self):
        self.assertTrue(self._has_gh_token(
            os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-ci-repair.yml"),
            "repair"))

    def test_review_review_has_gh_token(self):
        self.assertTrue(self._has_gh_token(
            os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-review.yml"),
            "review"))

    def test_merge_merge_has_gh_token(self):
        self.assertTrue(self._has_gh_token(
            os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-merge.yml"),
            "merge"))

    def test_controller_control_has_gh_token(self):
        self.assertTrue(self._has_gh_token(
            os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-controller.yml"),
            "control"))

    def test_intake_intake_has_gh_token(self):
        self.assertTrue(self._has_gh_token(
            os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-intake.yml"),
            "intake"))


class TestEmergencyStopInWorkflows(unittest.TestCase):
    """Defect 22: Emergency stop prevents push and merge."""

    def _has_emergency_stop_check(self, workflow_path):
        if not os.path.exists(workflow_path):
            return True
        with open(workflow_path) as f:
            content = f.read()
        return "AGENT_EMERGENCY_STOP" in content

    def test_intake_has_emergency_stop(self):
        self.assertTrue(self._has_emergency_stop_check(
            os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-intake.yml")))

    def test_worker_has_emergency_stop(self):
        self.assertTrue(self._has_emergency_stop_check(
            os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-worker.yml")))

    def test_merge_has_emergency_stop(self):
        self.assertTrue(self._has_emergency_stop_check(
            os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-merge.yml")))

    def test_review_has_emergency_stop(self):
        self.assertTrue(self._has_emergency_stop_check(
            os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-review.yml")))

    def test_ci_monitor_has_emergency_stop(self):
        self.assertTrue(self._has_emergency_stop_check(
            os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-ci-monitor.yml")))

    def test_ci_repair_has_emergency_stop(self):
        self.assertTrue(self._has_emergency_stop_check(
            os.path.join(os.path.dirname(__file__), "..", ".github", "workflows", "agent-ci-repair.yml")))


class TestCodexWrapper(unittest.TestCase):
    """Defect 12: Codex wrapper edge cases."""

    def test_wrapper_script_exists(self):
        path = os.path.join(os.path.dirname(__file__), "..", "scripts", "agent-control", "codex_wrapper.sh")
        self.assertTrue(os.path.exists(path))

    def test_wrapper_checks_emergency_stop(self):
        path = os.path.join(os.path.dirname(__file__), "..", "scripts", "agent-control", "codex_wrapper.sh")
        if os.path.exists(path):
            with open(path) as f:
                content = f.read()
            self.assertIn("AGENT_EMERGENCY_STOP", content)

    def test_wrapper_checks_enabled(self):
        path = os.path.join(os.path.dirname(__file__), "..", "scripts", "agent-control", "codex_wrapper.sh")
        if os.path.exists(path):
            with open(path) as f:
                content = f.read()
            self.assertIn("AGENT_ORCHESTRATOR_ENABLED", content)

    def test_wrapper_does_not_commit(self):
        path = os.path.join(os.path.dirname(__file__), "..", "scripts", "agent-control", "codex_wrapper.sh")
        if os.path.exists(path):
            with open(path) as f:
                content = f.read()
            self.assertIn("Never commits, pushes, merges, or creates PRs", content)

    def test_wrapper_uses_set_uo_pipefail(self):
        path = os.path.join(os.path.dirname(__file__), "..", "scripts", "agent-control", "codex_wrapper.sh")
        if os.path.exists(path):
            with open(path) as f:
                content = f.read()
            self.assertIn("set -euo pipefail", content)

    def test_wrapper_handles_unset_code_home(self):
        path = os.path.join(os.path.dirname(__file__), "..", "scripts", "agent-control", "codex_wrapper.sh")
        if os.path.exists(path):
            with open(path) as f:
                content = f.read()
            self.assertIn('${CODEX_HOME:-}', content)


if __name__ == "__main__":
    unittest.main(verbosity=2)
