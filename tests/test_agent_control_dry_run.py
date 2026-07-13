"""Static regression checks for disabled-by-default orchestration workflows."""

from __future__ import annotations

import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
WORKFLOWS = ROOT / ".github" / "workflows"
CONTROL = ROOT / "scripts" / "agent-control"
AGENT_WORKFLOWS = tuple(sorted(WORKFLOWS.glob("agent-*.yml")))


class TestWorkflowSyntaxAndPins(unittest.TestCase):
    def test_agent_workflow_yaml_has_a_dedicated_parser_check(self):
        source = (WORKFLOWS / "tests.yml").read_text()
        self.assertIn("check_agent_workflow_yaml.py", source)
        self.assertIn("--with pyyaml", source)

    def test_all_agent_workflows_have_top_level_names(self):
        for path in AGENT_WORKFLOWS:
            with self.subTest(path=path.name):
                self.assertTrue(path.read_text().startswith("name: "))

    def test_all_actions_are_sha_pinned(self):
        for path in AGENT_WORKFLOWS:
            for line in path.read_text().splitlines():
                if "uses:" in line:
                    with self.subTest(path=path.name, line=line):
                        self.assertRegex(line, r"@[0-9a-f]{40}")


class TestControlIssueOnly(unittest.TestCase):
    def test_no_actions_variables_or_variable_context_remain(self):
        source = "\n".join(path.read_text() for path in AGENT_WORKFLOWS)
        self.assertNotIn("actions/variables", source)
        self.assertNotIn("vars.AGENT_", source)
        self.assertNotIn("AGENT_ORCHESTRATOR_ENABLED", source)
        self.assertNotIn("AGENT_AUTO_MERGE_ENABLED", source)
        self.assertNotIn("AGENT_EMERGENCY_STOP", source)

    def test_sensitive_workflows_recheck_live_control(self):
        for name in ("agent-worker.yml", "agent-ci-repair.yml", "agent-review.yml", "agent-merge.yml", "agent-ci-monitor.yml", "agent-intake.yml"):
            with self.subTest(name=name):
                self.assertIn("control_state.py require-live", (WORKFLOWS / name).read_text())

    def test_manual_dispatches_are_control_gated(self):
        source = (WORKFLOWS / "agent-controller.yml").read_text()
        self.assertIn("setup-controls", source)
        for command in ("dispatch-ready", "dispatch-repair", "dispatch-review", "dispatch-merge", "dispatch-next"):
            self.assertIn(command, source)
        self.assertIn("require-auto-merge", source)


class TestDryRunAndVaderTrustBoundary(unittest.TestCase):
    def test_dry_run_performs_no_mutations_or_codex_execution(self):
        source = (WORKFLOWS / "agent-worker.yml").read_text()
        dry_run = source.split("  dry-run:", 1)[1].split("\n  validate:", 1)[0]
        self.assertIn('"mutations": []', dry_run)
        self.assertIn('"codex": False', dry_run)
        self.assertNotIn("gh workflow run", dry_run)
        self.assertNotIn("codex_wrapper.sh", dry_run)
        self.assertNotIn("git push", dry_run)

    def test_vader_workers_have_no_write_permissions_or_push_token(self):
        for name, job in (("agent-worker.yml", "vader-implementation"), ("agent-ci-repair.yml", "vader-repair"), ("agent-review.yml", "vader-review")):
            source = (WORKFLOWS / name).read_text()
            match = re.search(rf"^  {re.escape(job)}:\n(.*?)(?=^  [A-Za-z][A-Za-z-]*:|\Z)", source, re.MULTILINE | re.DOTALL)
            self.assertIsNotNone(match)
            definition = match.group(1)
            with self.subTest(name=name):
                self.assertIn("runs-on: [self-hosted, vader, agent-worker]", definition)
                self.assertIn("contents: read", definition)
                self.assertIn("issues: read", definition)
                self.assertNotIn("pull-requests: write", definition)
                self.assertNotIn("AGENT_PUSH_TOKEN", definition)
                self.assertNotIn("git commit", definition)
                self.assertNotIn("git push", definition)

    def test_codex_wrapper_strips_github_credentials_and_uses_read_only_review(self):
        source = (CONTROL / "codex_wrapper.sh").read_text()
        self.assertIn("env -u GH_TOKEN -u GITHUB_TOKEN -u AGENT_PUSH_TOKEN", source)
        self.assertIn('SANDBOX_MODE="read-only"', source)
        self.assertIn('SANDBOX_MODE="workspace-write"', source)


class TestCredentialIsolation(unittest.TestCase):
    def test_push_token_is_isolated_to_finalizer_push_steps(self):
        for name in ("agent-worker.yml", "agent-ci-repair.yml"):
            source = (WORKFLOWS / name).read_text()
            self.assertIn("Push branch with isolated temporary credentials", source)
            self.assertNotIn("gh auth setup-git", source)
            self.assertNotIn("git config --global --unset-all credential.helper", source)
            self.assertIn("AGENT_PUSH_TOKEN: ${{ secrets.AGENT_PUSH_TOKEN }}", source)
            self.assertIn("GIT_ASKPASS", source)
            self.assertIn("GH_TOKEN: ${{ github.token }}", source)

    def test_branch_protection_api_is_not_a_merge_precondition(self):
        source = (CONTROL / "state_manager.py").read_text() + (WORKFLOWS / "agent-merge.yml").read_text()
        self.assertNotIn("/branches/main/protection", source)
        self.assertIn("pulls/${{ inputs.pr_number }}/merge", source)


if __name__ == "__main__":
    unittest.main(verbosity=2)
