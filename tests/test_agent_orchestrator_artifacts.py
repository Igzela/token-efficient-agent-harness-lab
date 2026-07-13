"""Regression tests for the control-Issue and patch-artifact boundaries."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CONTROL = ROOT / "scripts" / "agent-control"
sys.path.insert(0, str(CONTROL))

import artifact_contract  # type: ignore[import-not-found]
import control_state  # type: ignore[import-not-found]


def control_issue(*, labels=(), state="open", body=None, number=1):
    return {
        "number": number,
        "title": control_state.CONTROL_ISSUE_TITLE,
        "state": state,
        "body": body if body is not None else control_state.CONTROL_MARKER,
        "labels": [{"name": control_state.CONTROL_LABEL}, *({"name": label} for label in labels)],
    }


class TestControlIssueResolution(unittest.TestCase):
    def test_exactly_one_well_formed_open_control_issue_is_required(self):
        resolved = control_state.resolve_control_issue([control_issue(labels=(control_state.ORCHESTRATOR_ENABLED_LABEL,))])
        self.assertEqual(resolved["number"], 1)
        self.assertTrue(resolved["orchestrator_enabled"])
        self.assertFalse(resolved["auto_merge_enabled"])
        self.assertFalse(resolved["emergency_stop"])

    def test_zero_multiple_malformed_and_closed_control_issues_fail_closed(self):
        cases = [
            [],
            [control_issue(number=1), control_issue(number=2)],
            [control_issue(body="missing marker")],
            [control_issue(state="closed")],
        ]
        for issues in cases:
            with self.subTest(issues=issues), self.assertRaises(control_state.ControlStateError):
                control_state.resolve_control_issue(issues)

    def test_label_semantics_and_emergency_stop_precedence(self):
        enabled = control_state.resolve_control_issue(
            [control_issue(labels=(control_state.ORCHESTRATOR_ENABLED_LABEL, control_state.AUTO_MERGE_ENABLED_LABEL))]
        )
        self.assertTrue(enabled["orchestrator_enabled"])
        self.assertTrue(enabled["auto_merge_enabled"])

        stopped = control_state.resolve_control_issue(
            [control_issue(labels=(
                control_state.ORCHESTRATOR_ENABLED_LABEL,
                control_state.AUTO_MERGE_ENABLED_LABEL,
                control_state.EMERGENCY_STOP_LABEL,
            ))]
        )
        self.assertFalse(stopped["orchestrator_enabled"])
        self.assertFalse(stopped["auto_merge_enabled"])
        self.assertTrue(stopped["emergency_stop"])

    def test_runtime_uses_issue_read_api_not_actions_variables(self):
        source = (CONTROL / "control_state.py").read_text()
        self.assertIn("/issues", source)
        self.assertNotIn("actions/variables", source)
        self.assertNotIn("AGENT_ORCHESTRATOR_ENABLED", source)
        self.assertNotIn("AGENT_AUTO_MERGE_ENABLED", source)
        self.assertNotIn("AGENT_EMERGENCY_STOP", source)


class TestPatchArtifactContract(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.repo = Path(self.directory.name) / "repo"
        self.repo.mkdir()
        self.git("init", "-b", "main")
        self.git("config", "user.name", "Test")
        self.git("config", "user.email", "test@example.invalid")
        (self.repo / "tracked.txt").write_text("before\n")
        self.git("add", "tracked.txt")
        self.git("commit", "-m", "initial")
        self.base_sha = self.git("rev-parse", "HEAD").stdout.strip()

    def tearDown(self):
        self.directory.cleanup()

    def git(self, *args):
        return subprocess.run(("git", *args), cwd=self.repo, check=True, text=True, capture_output=True)

    def test_binary_patch_and_manifest_are_recomputed_and_validated(self):
        (self.repo / "tracked.txt").write_text("after\n")
        (self.repo / "new.bin").write_bytes(b"\x00binary\xff\n")
        artifact_dir = self.repo / "artifact"
        manifest = artifact_contract.create_artifact(
            repo=self.repo,
            artifact_dir=artifact_dir,
            worker_type="implementation",
            issue_number=12,
            pr_number=0,
            base_sha=self.base_sha,
            expected_remote_sha=None,
            branch="agent/issue-12",
            codex_exit_code=0,
            local_checks=[{"command": "git diff --check", "exit_code": 0}],
        )
        validated = artifact_contract.validate_artifact(
            artifact_dir=artifact_dir,
            expected_worker_type="implementation",
            issue_number=12,
            pr_number=0,
            base_sha=self.base_sha,
            expected_remote_sha=None,
            branch="agent/issue-12",
        )
        self.assertEqual(manifest["patch_sha256"], validated["patch_sha256"])
        self.assertEqual(validated["changed_files"], ["new.bin", "tracked.txt"])
        self.assertGreater(validated["patch_size_bytes"], 0)

    def test_manifest_tampering_or_forbidden_paths_are_rejected(self):
        (self.repo / "tracked.txt").write_text("after\n")
        artifact_dir = self.repo / "artifact"
        artifact_contract.create_artifact(
            repo=self.repo,
            artifact_dir=artifact_dir,
            worker_type="implementation",
            issue_number=12,
            pr_number=0,
            base_sha=self.base_sha,
            expected_remote_sha=None,
            branch="agent/issue-12",
            codex_exit_code=0,
            local_checks=[],
        )
        manifest_path = artifact_dir / "agent-result.json"
        manifest = json.loads(manifest_path.read_text())
        manifest["changed_files"] = [".github/workflows/steal.yml"]
        manifest_path.write_text(json.dumps(manifest))
        with self.assertRaises(artifact_contract.ArtifactContractError):
            artifact_contract.validate_artifact(
                artifact_dir=artifact_dir,
                expected_worker_type="implementation",
                issue_number=12,
                pr_number=0,
                base_sha=self.base_sha,
                expected_remote_sha=None,
                branch="agent/issue-12",
            )

    def test_local_commit_after_codex_is_rejected_before_artifact_upload(self):
        (self.repo / "tracked.txt").write_text("after\n")
        self.git("add", "tracked.txt")
        self.git("commit", "-m", "untrusted local commit")
        (self.repo / "second.txt").write_text("staged after untrusted commit\n")
        with self.assertRaises(artifact_contract.ArtifactContractError):
            artifact_contract.create_artifact(
                repo=self.repo,
                artifact_dir=self.repo / "artifact",
                worker_type="implementation",
                issue_number=12,
                pr_number=0,
                base_sha=self.base_sha,
                expected_remote_sha=None,
                branch="agent/issue-12",
                codex_exit_code=0,
                local_checks=[],
            )

    def test_task_issue_scope_marker_allows_only_declared_paths(self):
        (self.repo / "tracked.txt").write_text("after\n")
        artifact_dir = self.repo / "artifact"
        manifest = artifact_contract.create_artifact(
            repo=self.repo,
            artifact_dir=artifact_dir,
            worker_type="implementation",
            issue_number=12,
            pr_number=0,
            base_sha=self.base_sha,
            expected_remote_sha=None,
            branch="agent/issue-12",
            codex_exit_code=0,
            local_checks=[],
        )
        artifact_contract.validate_issue_scope(
            '<!-- agent-orchestrator-scope:v1 {"allowed_paths":["tracked.txt"]} -->',
            manifest,
        )
        with self.assertRaises(artifact_contract.ArtifactContractError):
            artifact_contract.validate_issue_scope("task has no machine-readable scope", manifest)

    def test_issue_template_scope_marker_is_editable_and_parser_rejects_broad_paths(self):
        template = (ROOT / ".github" / "ISSUE_TEMPLATE" / "agent_task.md").read_text()
        self.assertEqual(
            artifact_contract.parse_issue_scope(template),
            ["src/", "tests/"],
        )
        for body in (
            '<!-- agent-orchestrator-scope:v1 {"allowed_paths":[]} -->',
            '<!-- agent-orchestrator-scope:v1 {"allowed_paths":["."]} -->',
            '<!-- agent-orchestrator-scope:v1 {"allowed_paths":["*"]} -->',
            '<!-- agent-orchestrator-scope:v1 {"allowed_paths":["src/","src/"]} -->',
        ):
            with self.subTest(body=body), self.assertRaises(artifact_contract.ArtifactContractError):
                artifact_contract.parse_issue_scope(body)


class TestWorkflowTrustBoundaries(unittest.TestCase):
    def workflow(self, name: str) -> str:
        return (ROOT / ".github" / "workflows" / name).read_text()

    def test_worker_and_repair_vader_jobs_only_upload_artifacts(self):
        for workflow_name in ("agent-worker.yml", "agent-ci-repair.yml"):
            source = self.workflow(workflow_name)
            self.assertIn("agent.patch", source)
            self.assertIn("agent-result.json", source)
            self.assertIn("retention-days: 1", source)
            self.assertNotIn("AGENT_PUSH_TOKEN", source.split("runs-on: [self-hosted, vader, agent-worker]", 1)[1].split("runs-on: ubuntu-latest", 1)[0])
            self.assertNotIn("git commit", source.split("runs-on: [self-hosted, vader, agent-worker]", 1)[1].split("runs-on: ubuntu-latest", 1)[0])
            self.assertNotIn("git push", source.split("runs-on: [self-hosted, vader, agent-worker]", 1)[1].split("runs-on: ubuntu-latest", 1)[0])

    def test_only_finalizer_push_step_receives_push_token_without_global_credential_mutation(self):
        for workflow_name in ("agent-worker.yml", "agent-ci-repair.yml"):
            source = self.workflow(workflow_name)
            self.assertIn("Push branch with isolated temporary credentials", source)
            self.assertNotIn("gh auth setup-git", source)
            self.assertNotIn("git config --global --unset-all credential.helper", source)
            self.assertIn("AGENT_PUSH_TOKEN: ${{ secrets.AGENT_PUSH_TOKEN }}", source)
            self.assertIn("GH_TOKEN: ${{ github.token }}", source)

    def test_merge_has_no_branch_protection_api_dependency(self):
        source = (CONTROL / "state_manager.py").read_text()
        self.assertNotIn("/branches/main/protection", source)

    def test_canonical_python_job_executes_every_orchestrator_regression_file(self):
        source = self.workflow("tests.yml")
        expected = [
            "tests/test_agent_control_ci.py",
            "tests/test_agent_control_dry_run.py",
            "tests/test_agent_control_lock.py",
            "tests/test_agent_control_state.py",
            "tests/test_agent_control_worktree.py",
            "tests/test_agent_orchestrator_repairs.py",
            "tests/test_agent_orchestrator_artifacts.py",
        ]
        for test_file in expected:
            self.assertIn(test_file, source)
        self.assertIn("verify_orchestrator_test_suite.py", source)

    def test_controller_setup_alias_and_review_waiting_labels_are_declared(self):
        controller = self.workflow("agent-controller.yml")
        labels = (CONTROL / "setup_labels.py").read_text()
        self.assertIn("setup-controls", controller)
        self.assertIn("agent-review-blocked", labels)
        self.assertIn("agent-merge-ready", labels)
        self.assertIn("retry-review", controller)


if __name__ == "__main__":
    unittest.main(verbosity=2)
