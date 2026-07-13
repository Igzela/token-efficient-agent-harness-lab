"""Regression tests for the event-driven orchestrator repair contract."""

from __future__ import annotations

import contextlib
import io
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

import yaml


ROOT = pathlib.Path(__file__).resolve().parents[1]
CONTROL = ROOT / "scripts" / "agent-control"
sys.path.insert(0, str(CONTROL))

import lock_manager  # noqa: E402
import ci_verifier  # noqa: E402
import control_state  # noqa: E402
import dispatcher  # noqa: E402
import state_manager  # noqa: E402
import worktree_manager  # noqa: E402


class TestLockManagerCLI(unittest.TestCase):
    def run_cli(self, *args):
        return subprocess.run(
            [sys.executable, str(CONTROL / "lock_manager.py"), *args],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )

    def test_valid_command_arities(self):
        with mock.patch.object(lock_manager, "acquire_lock", return_value=True):
            with mock.patch.object(lock_manager, "release_lock"):
                with mock.patch.object(lock_manager, "_lock_path") as lock_path:
                    lock_path.return_value.exists.return_value = False
                    for args in (("acquire", "key", "1"), ("release", "key"), ("check", "key"), ("count",), ("count", "prefix"), ("capacity",)):
                        with self.subTest(args=args):
                            result = self.run_cli(*args)
                            self.assertEqual(result.returncode, 0, result.stderr)

    def test_invalid_command_arities_fail(self):
        invalid = (
            ("acquire",),
            ("acquire", "key", "1", "extra"),
            ("release",),
            ("release", "key", "extra"),
            ("check",),
            ("check", "key", "extra"),
            ("count", "a", "b"),
            ("capacity", "extra"),
        )
        for args in invalid:
            with self.subTest(args=args):
                result = self.run_cli(*args)
                self.assertNotEqual(result.returncode, 0)


class TestStructuredWorktreeResults(unittest.TestCase):
    def capture_main(self, args):
        output = io.StringIO()
        with mock.patch.object(sys, "argv", ["worktree_manager.py", *args]):
            with contextlib.redirect_stdout(output):
                worktree_manager.main()
        return json.loads(output.getvalue())

    def test_create_emits_structured_json_once(self):
        result = ("/tmp/agent-worktrees/issue-123", "agent/issue-123", "base", None)
        with mock.patch.object(worktree_manager, "create_worktree", return_value=result):
            payload = self.capture_main(("create", "123", "agent/issue-123"))
        self.assertEqual(payload["worktree_path"], result[0])
        self.assertEqual(payload["branch"], result[1])
        self.assertEqual(payload["base_sha"], "base")
        self.assertIsNone(payload["previous_remote_sha"])

    def test_create_pr_emits_structured_json(self):
        pr = {"number": 123, "url": "https://example.invalid/pr/123", "headRefOid": "head"}
        with mock.patch.object(worktree_manager, "create_pr", return_value=pr):
            payload = self.capture_main(("create-pr", "123", "agent/issue-123", "title", "body"))
        self.assertEqual(payload, {"pr_number": 123, "url": pr["url"], "head_sha": "head"})

    def test_registered_worktree_validation_requires_expected_branch(self):
        path = pathlib.Path("/tmp/agent-worktrees/issue-123")
        porcelain = "\n".join(
            [
                f"worktree {path}",
                "HEAD abc",
                "branch refs/heads/agent/issue-999",
                "",
            ]
        )
        with mock.patch.object(worktree_manager, "_git", return_value=porcelain):
            with mock.patch.object(pathlib.Path, "is_dir", return_value=True):
                self.assertFalse(worktree_manager.verify_worktree(path, "agent/issue-123", str(ROOT)))

    def test_cleanup_never_raw_rmtree(self):
        source = (CONTROL / "worktree_manager.py").read_text()
        self.assertNotIn("shutil.rmtree", source)
        cleanup = (CONTROL / "cleanup.py").read_text()
        self.assertNotIn("git\", \"branch\", \"-D", cleanup)
        self.assertNotIn("shutil.rmtree", cleanup)


class TestWorkflowStaticContracts(unittest.TestCase):
    def read(self, name):
        return (ROOT / ".github" / "workflows" / name).read_text()

    def test_worker_creates_worktree_once_and_parses_json(self):
        source = self.read("agent-worker.yml")
        self.assertEqual(source.count("worktree_manager.py create"), 1)
        self.assertNotIn("cut -d=", source)
        self.assertIn(".worktree_path", source)
        self.assertIn("git worktree list --porcelain", source)

    def test_pr_output_is_structured(self):
        source = self.read("agent-worker.yml")
        self.assertNotIn("grep \"pr_number=\"", source)
        self.assertIn(".pr_number", source)

    def test_dispatching_workflows_have_actions_write(self):
        for path in sorted((ROOT / ".github" / "workflows").glob("*.yml")):
            source = path.read_text()
            if "gh workflow run" not in source:
                continue
            data = yaml.safe_load(source)
            self.assertEqual(
                data.get("permissions", {}).get("actions"),
                "write",
                f"{path} dispatches a workflow without actions: write",
            )

    def test_global_dispatcher_is_the_capacity_authority(self):
        controller = self.read("agent-controller.yml")
        dispatcher_source = (CONTROL / "dispatcher.py").read_text()
        self.assertIn("group: agent-dispatch-global", controller)
        self.assertIn("MAX_ACTIVE = 2", dispatcher_source)
        self.assertIn("get_active_issue_numbers", dispatcher_source)
        self.assertNotIn("lock_manager.py capacity", "\n".join(
            path.read_text() for path in (ROOT / ".github" / "workflows").glob("*.yml")
        ))

    def test_dry_run_jobs_have_no_write_or_worker_commands(self):
        forbidden = (
            "worktree_manager.py", "codex_wrapper.sh", "lock_manager.py acquire",
            "gh workflow run", "git commit", "git push", "gh pr create",
            "gh issue edit", "gh issue comment", "gh pr merge",
        )
        for name in ("agent-intake.yml", "agent-worker.yml"):
            source = self.read(name)
            dry_run = source.split("  dry-run:", 1)[1].split("\n  ", 1)[0]
            for command in forbidden:
                self.assertNotIn(command, dry_run, f"{name} dry-run contains {command}")

    def test_canonical_runtime_controls_and_rechecks(self):
        workflows = "\n".join(
            path.read_text() for path in (ROOT / ".github" / "workflows").glob("agent-*.yml")
        )
        for name in ("AGENT_ORCHESTRATOR_ENABLED", "AGENT_AUTO_MERGE_ENABLED", "AGENT_EMERGENCY_STOP"):
            self.assertIn(name, workflows)
        for bad in ("Agent_EMERGENCY_STOP", "AGENT_Orchestrator_ENABLED", "Agent_AUTO_MERGE_ENABLED"):
            self.assertNotIn(bad, workflows)
        worker = self.read("agent-worker.yml")
        self.assertGreaterEqual(worker.count("require-live"), 6)
        self.assertIn("Runtime gate after Codex", worker)
        self.assertIn("git add -A", worker)
        self.assertIn("gh auth setup-git", worker)

    def test_push_credential_is_explicit_and_scoped(self):
        for name in ("agent-worker.yml", "agent-ci-repair.yml"):
            source = self.read(name)
            self.assertIn("secrets.AGENT_PUSH_TOKEN", source)
            self.assertIn("gh auth setup-git", source)
            self.assertIn("unset GH_TOKEN AGENT_PUSH_TOKEN", source)
            self.assertIn("AGENT_PUSH_TOKEN is missing", source)

    def test_exact_head_ci_contract_is_canonical(self):
        requirements = json.loads((CONTROL / "ci_requirements.json").read_text())
        self.assertEqual(requirements["workflow_name"], "tests")
        self.assertEqual(len(requirements["required_jobs"]), 7)
        tests_workflow = self.read("tests.yml")
        self.assertEqual(tests_workflow.count("Verify exact requested head"), 7)
        verifier = (CONTROL / "ci_verifier.py").read_text()
        for field in ("workflow_run_id", "required_jobs", "successful_jobs", "head_sha"):
            self.assertIn(field, verifier)

    def test_merge_is_expected_head_bound_and_fail_closed(self):
        source = self.read("agent-merge.yml")
        self.assertIn('pulls/${{ inputs.pr_number }}/merge', source)
        self.assertIn('-f sha="${{ inputs.head_sha }}"', source)
        self.assertNotIn("gh pr merge", source)
        self.assertIn("state_manager.py verify-merge", source)
        self.assertIn("state_manager.py record-merge", source)

    def test_repair_uses_run_id_and_persists_attempt(self):
        repair = self.read("agent-ci-repair.yml")
        self.assertIn("ci_run_id", repair)
        self.assertNotIn("failed_logs:", repair)
        self.assertNotIn("inputs.failed_logs", repair)
        self.assertIn("AGENT_REPAIR_COUNT", repair)
        self.assertIn("repair_count", repair)
        self.assertIn("fetch_bounded_failed_logs", (CONTROL / "prompt_builder.py").read_text())

    def test_review_uses_canonical_schema_and_binding(self):
        source = self.read("agent-review.yml")
        self.assertIn("validate_review.py", source)
        self.assertIn("--with jsonschema", source)
        self.assertIn("verify-binding", source)
        self.assertIn("review-passed", source)
        self.assertIn("dispatch-merge", source)

    def test_workflow_dispatch_input_names_match_callers(self):
        expected = {
            "agent-controller.yml": {"command", "issue", "pr_number", "head_sha", "ci_run_id", "repair_count", "failed_jobs", "source_issue", "dispatch_id"},
            "agent-worker.yml": {"issue", "dry_run"},
            "agent-ci-repair.yml": {"pr_number", "issue_number", "head_sha", "failed_jobs", "repair_count", "ci_run_id"},
            "agent-review.yml": {"pr_number", "issue_number", "head_sha"},
            "agent-merge.yml": {"pr_number", "issue_number", "head_sha"},
            "tests.yml": {"expected_sha"},
        }
        for name, names in expected.items():
            data = yaml.safe_load(self.read(name))
            trigger = data.get("on") or data.get(True)
            inputs = trigger["workflow_dispatch"]["inputs"]
            self.assertTrue(names <= set(inputs), name)

    def test_wrapper_disabled_state_fails_closed(self):
        source = (CONTROL / "codex_wrapper.sh").read_text()
        self.assertIn('fail_closed "control_disabled"', source)
        self.assertIn('fail_closed "control_state_unavailable"', source)
        self.assertNotIn("exit 0", source)


class TestGlobalDispatcher(unittest.TestCase):
    def test_simultaneous_intake_claims_dispatch_at_most_once(self):
        labels = {state_manager.LABEL_READY}
        recorded = {}
        workflow_calls = []

        def read_state(_issue, dispatch_id, _repo):
            return recorded.get(dispatch_id)

        def set_labels(_issue, *new_labels, repo=""):
            labels.clear()
            labels.update(new_labels)
            return True

        def record_state(_issue, dispatch_id, action, status, details=None, repo=""):
            recorded[dispatch_id] = {"dispatch_id": dispatch_id, "action": action, "status": status}
            return True

        with mock.patch.object(dispatcher, "_repo", return_value="Igzela/token-efficient-agent-harness-lab"), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", side_effect=read_state), \
             mock.patch.object(dispatcher.sm, "get_issue_labels", side_effect=lambda *_args, **_kwargs: set(labels)), \
             mock.patch.object(dispatcher.sm, "has_open_issue_pr", return_value=False), \
             mock.patch.object(dispatcher.sm, "get_active_issue_numbers", return_value=set()), \
             mock.patch.object(dispatcher.sm, "set_labels", side_effect=set_labels), \
             mock.patch.object(dispatcher.sm, "record_dispatch_state", side_effect=record_state), \
             mock.patch.object(dispatcher, "_run_workflow", side_effect=lambda *args: workflow_calls.append(args) or True):
            first = dispatcher.dispatch_ready(12, "worker:12")
            second = dispatcher.dispatch_ready(12, "worker:12")

        self.assertTrue(first["dispatched"])
        self.assertTrue(second["dispatched"])
        self.assertEqual(len(workflow_calls), 1)

    def test_failed_dispatch_rolls_back_claim(self):
        labels = {state_manager.LABEL_READY}
        state_calls = []

        def set_labels(_issue, *new_labels, repo=""):
            labels.clear()
            labels.update(new_labels)
            return True

        with mock.patch.object(dispatcher, "_repo", return_value="repo"), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", return_value=None), \
             mock.patch.object(dispatcher.sm, "get_issue_labels", side_effect=lambda *_args, **_kwargs: set(labels)), \
             mock.patch.object(dispatcher.sm, "has_open_issue_pr", return_value=False), \
             mock.patch.object(dispatcher.sm, "get_active_issue_numbers", return_value=set()), \
             mock.patch.object(dispatcher.sm, "set_labels", side_effect=set_labels), \
             mock.patch.object(dispatcher.sm, "record_dispatch_state", side_effect=lambda *args, **kwargs: state_calls.append(args) or True), \
             mock.patch.object(dispatcher, "_run_workflow", return_value=False):
            result = dispatcher.dispatch_ready(12, "worker:12")

        self.assertFalse(result["dispatched"])
        self.assertEqual(labels, {state_manager.LABEL_READY})
        self.assertEqual(state_calls[-1][3], "failed")


class TestExactHeadCIVerifier(unittest.TestCase):
    def _run(self, **overrides):
        required = ci_verifier.load_requirements()["required_jobs"]
        jobs = [{"name": name, "status": "completed", "conclusion": "success"} for name in required]
        value = {
            "databaseId": 456,
            "workflowName": "tests",
            "headSha": "sha-good",
            "status": "completed",
            "conclusion": "success",
            "jobs": jobs,
            "createdAt": "2026-07-13T00:00:00Z",
            "updatedAt": "2026-07-13T01:00:00Z",
        }
        value.update(overrides)
        return value

    def test_accepts_all_seven_successful_jobs(self):
        with mock.patch.object(ci_verifier, "run_info", return_value=self._run()):
            evidence = ci_verifier.verify_exact_head_ci(207, "sha-good", 456, {"headRefOid": "sha-good"})
        self.assertEqual(evidence["status"], "success")
        self.assertEqual(len(evidence["successful_jobs"]), 7)

    def test_rejects_different_sha(self):
        with mock.patch.object(ci_verifier, "run_info", return_value=self._run(headSha="old")):
            with self.assertRaises(ci_verifier.CIVerificationError):
                ci_verifier.verify_exact_head_ci(207, "sha-good", 456)

    def test_rejects_missing_required_job(self):
        jobs = self._run()["jobs"][:-1]
        with mock.patch.object(ci_verifier, "run_info", return_value=self._run(jobs=jobs)):
            with self.assertRaises(ci_verifier.CIVerificationError):
                ci_verifier.verify_exact_head_ci(207, "sha-good", 456)

    def test_rejects_pending_cancelled_or_failed_job(self):
        for status, conclusion in (("queued", None), ("completed", "cancelled"), ("completed", "failure"), ("completed", "skipped")):
            jobs = self._run()["jobs"]
            jobs[0] = {"name": jobs[0]["name"], "status": status, "conclusion": conclusion}
            with self.subTest(status=status, conclusion=conclusion):
                with mock.patch.object(ci_verifier, "run_info", return_value=self._run(jobs=jobs)):
                    with self.assertRaises(ci_verifier.CIVerificationError):
                        ci_verifier.verify_exact_head_ci(207, "sha-good", 456)

    def test_rejects_moved_pr_head(self):
        with mock.patch.object(ci_verifier, "run_info", return_value=self._run()):
            with self.assertRaises(ci_verifier.CIVerificationError):
                ci_verifier.verify_exact_head_ci(207, "sha-good", 456, {"headRefOid": "moved"})


class TestBindingAndMergeObjections(unittest.TestCase):
    def test_binding_rejects_mismatched_issue_pr(self):
        pr = {
            "headRefName": "agent/issue-12",
            "headRefOid": "sha",
            "body": "Closes #12\n<!-- agent-orchestrator-binding: {\"issue_number\": 12, \"branch\": \"agent/issue-12\"} -->",
        }
        with mock.patch.object(state_manager, "get_pr_info", return_value=pr), \
             mock.patch.object(state_manager, "read_worker_state", return_value={"pr_number": 99, "head_sha": "sha", "extra": {"branch": "agent/issue-12"}}):
            ok, reason = state_manager.verify_issue_pr_binding(12, 207, "sha")
        self.assertFalse(ok)
        self.assertEqual(reason, "worker_state_mismatch")

    def _merge_patches(self, labels=None, pr=None, review=None, threads=None, ci_state=None):
        labels = labels or {state_manager.LABEL_REVIEW_PASSED}
        pr = pr or {
            "state": "OPEN", "baseRefName": "main", "headRefOid": "sha",
            "mergeable": "MERGEABLE", "mergeStateStatus": "CLEAN", "reviews": [],
        }
        patches = [
            mock.patch.object(control_state, "require_auto_merge", return_value={}),
            mock.patch.object(state_manager, "get_issue_labels", return_value=labels),
            mock.patch.object(state_manager, "get_pr_info", return_value=pr),
            mock.patch.object(state_manager, "verify_issue_pr_binding", return_value=(True, "ok")),
            mock.patch.object(state_manager, "read_review_state", return_value=review or {"pr_number": 207, "head_sha": "sha", "verdict": "PASS"}),
            mock.patch.object(state_manager, "unresolved_review_threads", return_value=threads or []),
            mock.patch.object(state_manager, "read_ci_state", return_value={"pr_number": 207, "head_sha": "sha", "workflow_run_id": 456} if ci_state is None else ci_state),
            mock.patch.object(ci_verifier, "verify_exact_head_ci", return_value={"status": "success"}),
            mock.patch.object(state_manager, "_gh", return_value="{}"),
        ]
        return patches

    def test_merge_rejects_unresolved_thread(self):
        patches = self._merge_patches(threads=[{"isResolved": False}])
        with contextlib.ExitStack() as stack:
            for patcher in patches:
                stack.enter_context(patcher)
            with self.assertRaisesRegex(RuntimeError, "unresolved review thread"):
                state_manager.verify_merge_requirements(207, 12, "sha")

    def test_merge_rejects_active_requested_changes(self):
        patches = self._merge_patches(pr={
            "state": "OPEN", "baseRefName": "main", "headRefOid": "sha",
            "mergeable": "MERGEABLE", "mergeStateStatus": "CLEAN",
            "reviews": [{"state": "CHANGES_REQUESTED"}],
        })
        with contextlib.ExitStack() as stack:
            for patcher in patches:
                stack.enter_context(patcher)
            with self.assertRaisesRegex(RuntimeError, "requested-changes"):
                state_manager.verify_merge_requirements(207, 12, "sha")

    def test_merge_rejects_missing_ci_state(self):
        patches = self._merge_patches(ci_state={})
        with contextlib.ExitStack() as stack:
            for patcher in patches:
                stack.enter_context(patcher)
            with self.assertRaisesRegex(RuntimeError, "stored CI state"):
                state_manager.verify_merge_requirements(207, 12, "sha")

    def test_only_canonical_control_variables_are_used(self):
        text = "\n".join(
            path.read_text()
            for path in (ROOT / ".github" / "workflows").glob("agent-*.yml")
        )
        self.assertNotIn("Agent_EMERGENCY_STOP", text)
        self.assertNotIn("AGENT_Orchestrator_ENABLED", text)
        self.assertNotIn("Agent_AUTO_MERGE_ENABLED", text)


if __name__ == "__main__":
    unittest.main(verbosity=2)
