"""Regression tests for the event-driven orchestrator's deterministic path."""

from __future__ import annotations

import json
import os
import pathlib
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = pathlib.Path(__file__).resolve().parents[1]
CONTROL = ROOT / "scripts" / "agent-control"
WORKFLOWS = ROOT / ".github" / "workflows"
sys.path.insert(0, str(CONTROL))

import ci_handler
import ci_verifier
import control_state
import dispatcher
import pr_binding
import prompt_builder
import state_manager


class TestWorkflowContracts(unittest.TestCase):
    def read(self, name: str) -> str:
        return (WORKFLOWS / name).read_text()

    def shell_scripts(self, name: str) -> str:
        lines = self.read(name).splitlines()
        scripts = []
        index = 0
        while index < len(lines):
            line = lines[index]
            stripped = line.lstrip()
            if not stripped.startswith("run:"):
                index += 1
                continue
            indentation = len(line) - len(stripped)
            inline = stripped.removeprefix("run:").strip()
            if inline and inline != "|":
                scripts.append(inline)
            index += 1
            while index < len(lines):
                candidate = lines[index]
                if candidate.strip() and len(candidate) - len(candidate.lstrip()) <= indentation:
                    break
                scripts.append(candidate)
                index += 1
        return "\n".join(scripts)

    def test_all_workflow_dispatch_callers_and_inputs_match(self):
        expected = {
            "agent-controller.yml": {"command", "issue", "pr_number", "head_sha", "ci_run_id", "repair_count", "source_issue", "dispatch_id"},
            "agent-worker.yml": {"issue", "dry_run"},
            "agent-ci-repair.yml": {"pr_number", "issue_number", "head_sha", "ci_run_id", "repair_count"},
            "agent-review.yml": {"pr_number", "issue_number", "head_sha"},
            "agent-merge.yml": {"pr_number", "issue_number", "head_sha"},
            "tests.yml": {"expected_sha"},
        }
        for name, fields in expected.items():
            source = self.read(name)
            for field in fields:
                with self.subTest(name=name, field=field):
                    self.assertIn(f"      {field}:\n", source, name)

    def test_canonical_workflow_has_one_top_level_environment_mapping(self):
        source = self.read("tests.yml")
        self.assertEqual(source.count("\nenv:\n"), 1)
        self.assertIn("  EXPECTED_SHA: ${{ inputs.expected_sha }}", source)

    def test_repair_dispatch_carries_run_id_not_unbounded_logs(self):
        controller = self.read("agent-controller.yml")
        monitor = self.read("agent-ci-monitor.yml")
        repair = self.read("agent-ci-repair.yml")
        self.assertIn("ci_run_id", controller)
        self.assertIn("ci_run_id", monitor)
        self.assertNotIn("failed_jobs:", repair)
        self.assertNotIn("failed_logs", monitor)
        self.assertIn("ci_evidence.py", repair)
        self.assertIn("repair-evidence", repair)
        self.assertIn("fetch bounded failed jobs, steps, and redacted logs by run id", repair.lower())

    def test_artifact_finalizers_recompute_all_untrusted_bindings(self):
        for name in ("agent-worker.yml", "agent-ci-repair.yml"):
            source = self.read(name)
            with self.subTest(name=name):
                self.assertIn("artifact_contract.py validate", source)
                self.assertIn("artifact_contract.py validate-scope", source)
                self.assertIn("git apply --index --binary", source)
                self.assertIn("artifact_contract.py validate-index", source)
                self.assertIn("git diff --cached --check", source)
                self.assertIn("git diff --check", source)
                self.assertIn("control_state.py require-live", source)
                self.assertIn("agent.patch", source)
                self.assertIn("agent-result.json", source)

    def test_finalizers_use_verified_pr_binding_cli_and_supported_ci_acquisition(self):
        worker = self.read("agent-worker.yml")
        repair = self.read("agent-ci-repair.yml")
        self.assertIn("pr_binding.py create-or-update", worker)
        self.assertNotIn("gh pr create", worker)
        self.assertNotIn("--json number --jq", worker)
        self.assertIn("pr_binding.py verify-post-push", repair)
        self.assertIn("ci_verifier.py verify-failed-run", repair)
        self.assertIn("ci_verifier.py acquire", worker)
        self.assertIn("ci_verifier.py acquire", repair)

    def test_review_worker_is_read_only_and_only_pass_authorizes_merge(self):
        source = self.read("agent-review.yml")
        vader = source.split("  vader-review:", 1)[1].split("\n  finalize:", 1)[0]
        self.assertIn("codex_wrapper.sh review", vader)
        self.assertNotIn("git commit", vader)
        self.assertNotIn("git push", vader)
        self.assertIn("validate_review.py", source)
        self.assertIn("steps.verdict.outputs.verdict == 'PASS'", source)
        self.assertIn("steps.verdict.outputs.verdict != 'PASS'", source)
        self.assertIn("require-auto-merge", source)
        self.assertIn("agent-review-blocked", source)
        self.assertIn("agent-merge-ready", source + (CONTROL / "state_manager.py").read_text())
        self.assertNotIn('set-labels "${{ inputs.issue_number }}" agent-running', source)

    def test_review_terminal_states_release_capacity_and_retry_is_explicit(self):
        self.assertEqual(
            state_manager.labels_for_review_verdict("PASS"),
            {state_manager.LABEL_REVIEW_PASSED, state_manager.LABEL_MERGE_READY},
        )
        self.assertEqual(
            state_manager.labels_for_review_verdict("PASS_WITH_NOTES"),
            {state_manager.LABEL_REVIEW_BLOCKED},
        )
        self.assertEqual(
            state_manager.ACTIVE_LABELS & state_manager.labels_for_review_verdict("PASS"),
            set(),
        )
        self.assertIn("retry-review", self.read("agent-controller.yml"))

    def test_review_summary_is_passed_through_environment_not_shell_expression(self):
        source = self.read("agent-review.yml")
        self.assertIn("REVIEW_SUMMARY: ${{ steps.verdict.outputs.summary }}", source)
        self.assertIn('"$REVIEW_SUMMARY"', source)

    def test_string_workflow_inputs_are_not_interpolated_into_shell_scripts(self):
        forbidden = {
            "agent-controller.yml": (
                "${{ inputs.command }}",
                "${{ inputs.dispatch_id }}",
                "${{ inputs.head_sha }}",
            ),
            "agent-ci-repair.yml": ("${{ inputs.head_sha }}",),
            "agent-review.yml": ("${{ inputs.head_sha }}",),
            "agent-merge.yml": ("${{ inputs.head_sha }}",),
            "tests.yml": ("${{ inputs.expected_sha }}",),
        }
        for name, expressions in forbidden.items():
            scripts = self.shell_scripts(name)
            for expression in expressions:
                with self.subTest(name=name, expression=expression):
                    self.assertNotIn(expression, scripts)

    def test_merge_requires_exact_head_ci_pass_review_and_objections_without_admin_api(self):
        source = self.read("agent-merge.yml") + (CONTROL / "state_manager.py").read_text()
        for requirement in ("verify-merge", "require-auto-merge", "pulls/${{ inputs.pr_number }}/merge", "mergeCommit", "review-passed"):
            self.assertIn(requirement, source)
        self.assertNotIn("branches/main/protection", source)

    def test_failure_handlers_preserve_nonterminal_outcomes(self):
        source = "\n".join(self.read(name) for name in ("agent-worker.yml", "agent-ci-repair.yml", "agent-review.yml", "agent-merge.yml", "agent-ci-monitor.yml"))
        self.assertIn("disabled or emergency", source)
        self.assertIn("stale", source)
        self.assertIn("agent-blocked", source)
        self.assertIn("max_repairs_exceeded", (CONTROL / "ci_handler.py").read_text())

    def test_worker_has_post_claim_nonstart_capacity_release(self):
        source = self.read("agent-worker.yml")
        self.assertIn("rejected-before-vader:", source)
        self.assertIn("release-rejected-worker", source)
        self.assertIn("needs: [gate, validate, vader-implementation, finalize]", source)

    def test_legacy_generic_issue_mutators_are_not_exposed(self):
        source = (CONTROL / "state_manager.py").read_text()
        for command in ("select-task", "next-task", "retry-task", "block-task"):
            self.assertNotIn(f'command == "{command}"', source)


class TestDispatcher(unittest.TestCase):
    def test_duplicate_delivery_dispatches_once(self):
        labels = {state_manager.LABEL_READY}
        recorded = {}
        workflow_calls = []

        def set_labels(_issue, *new_labels, repo=""):
            labels.clear()
            labels.update(new_labels)
            return True

        def record(_issue, dispatch_id, action, status, details=None, repo=""):
            recorded[dispatch_id] = {"status": status, "action": action}
            return True

        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="repo"), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", side_effect=lambda _i, key, _r: recorded.get(key)), \
             mock.patch.object(dispatcher.sm, "get_issue_labels_checked", return_value=labels), \
             mock.patch.object(dispatcher.sm, "check_dependencies_complete", return_value=(True, None)), \
             mock.patch.object(dispatcher.sm, "has_open_issue_pr", return_value=False), \
             mock.patch.object(dispatcher.sm, "validate_task_scope", return_value=(True, ["src/"])), \
             mock.patch.object(dispatcher.sm, "get_active_issue_numbers", return_value=set()), \
             mock.patch.object(dispatcher.sm, "set_labels", side_effect=set_labels), \
             mock.patch.object(dispatcher.sm, "record_dispatch_state", side_effect=record), \
             mock.patch.object(dispatcher, "_run_workflow", side_effect=lambda *args: workflow_calls.append(args) or True):
            first = dispatcher.dispatch_ready(12, "worker:12")
            second = dispatcher.dispatch_ready(12, "worker:12")
        self.assertTrue(first["dispatched"])
        self.assertTrue(second["dispatched"])
        self.assertEqual(len(workflow_calls), 1)

    def test_claimed_dispatch_is_not_reissued_when_final_audit_write_failed(self):
        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="repo"), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", return_value={"status": "claimed"}), \
             mock.patch.object(dispatcher, "_run_workflow") as workflow:
            result = dispatcher.dispatch_ready(12, "worker:12")
        self.assertFalse(result["dispatched"])
        self.assertEqual(result["reason"], "dispatch_in_flight")
        workflow.assert_not_called()

    def test_initial_pr_creation_cli_parses_supported_rest_response_and_reuses_pr(self):
        sha = "a" * 40
        with tempfile.TemporaryDirectory() as temp:
            temp_path = pathlib.Path(temp)
            state_path = temp_path / "pr-state.json"
            gh_path = temp_path / "gh"
            gh_path.write_text(
                "#!/usr/bin/env python3\n"
                "import json, os, sys\n"
                "state = os.environ['PR_STATE']\n"
                "args = sys.argv[1:]\n"
                "if args[:2] == ['pr', 'list']:\n"
                "    print(json.dumps([json.load(open(state))]) if os.path.exists(state) else '[]')\n"
                "elif args[:2] == ['api', '--method'] and 'POST' in args:\n"
                "    url = 'https://github.com/acme/repo/pull/123'\n"
                "    value = {'number':123,'html_url':url,'url':url,'state':'OPEN','baseRefName':'main','headRefName':'agent/issue-42','headRefOid':os.environ['PR_SHA'],'body':'Closes #42\\n\\n<!-- agent-orchestrator-binding: {\\\"issue_number\\\": 42, \\\"branch\\\": \\\"agent/issue-42\\\"} -->'}\n"
                "    json.dump(value, open(state, 'w')); print(json.dumps({'number':123,'html_url':url}))\n"
                "elif args[:2] == ['api', '--method'] and 'PATCH' in args:\n"
                "    value = json.load(open(state)); print(json.dumps(value))\n"
                "elif args[:2] == ['pr', 'view']:\n"
                "    print(json.dumps(json.load(open(state))))\n"
                "else: raise SystemExit('unexpected gh args: ' + repr(args))\n"
            )
            gh_path.chmod(gh_path.stat().st_mode | stat.S_IXUSR)
            body = temp_path / "body.md"
            body.write_text(
                'Closes #42\n\n'
                '<!-- agent-orchestrator-binding: {"issue_number": 42, "branch": "agent/issue-42"} -->\n'
            )
            env = {
                **os.environ,
                "AGENT_GH_CMD": str(gh_path),
                "AGENT_REPO": "acme/repo",
                "PR_STATE": str(state_path),
                "PR_SHA": sha,
            }
            command = [
                sys.executable, str(CONTROL / "pr_binding.py"), "create-or-update",
                "42", "agent/issue-42", sha, "agent: implement #42", str(body),
            ]
            first = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True)
            second = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True)
        self.assertEqual(first.returncode, 0, first.stderr)
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertEqual(json.loads(first.stdout)["number"], 123)
        self.assertEqual(json.loads(second.stdout)["number"], 123)

    def test_ci_failed_run_cli_works_from_repository_root_with_stubbed_gh(self):
        sha = "b" * 40
        with tempfile.TemporaryDirectory() as temp:
            gh_path = pathlib.Path(temp) / "gh"
            gh_path.write_text(
                "#!/usr/bin/env python3\n"
                "import json\n"
                "print(json.dumps({'databaseId':456,'workflowName':'tests','status':'completed','conclusion':'failure','headSha':'%s'}))\n"
                % sha
            )
            gh_path.chmod(gh_path.stat().st_mode | stat.S_IXUSR)
            env = {**os.environ, "PATH": f"{temp}:{os.environ['PATH']}"}
            result = subprocess.run(
                [sys.executable, str(CONTROL / "ci_verifier.py"), "verify-failed-run", "456", sha],
                cwd=ROOT, env=env, capture_output=True, text=True,
            )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["workflow_run_id"], 456)

    def test_ci_repair_prompt_cli_works_from_repository_root_with_artifact(self):
        sha = "c" * 40
        with tempfile.TemporaryDirectory() as temp:
            temp_path = pathlib.Path(temp)
            gh_path = temp_path / "gh"
            gh_path.write_text("#!/bin/sh\nexit 1\n")
            gh_path.chmod(gh_path.stat().st_mode | stat.S_IXUSR)
            evidence = temp_path / "evidence.json"
            evidence.write_text(json.dumps({
                "schema_version": 1,
                "failed_jobs": [{"name": "Python", "failed_steps": ["tests"]}],
                "logs": "bounded failure",
            }))
            env = {
                **os.environ,
                "PATH": f"{temp}:{os.environ['PATH']}",
                "AGENT_REPAIR_COUNT": "1",
            }
            result = subprocess.run(
                [sys.executable, str(CONTROL / "prompt_builder.py"), "ci-repair", "207", sha, str(evidence)],
                cwd=ROOT, env=env, capture_output=True, text=True,
            )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(sha, result.stdout)
        self.assertIn("bounded failure", result.stdout)

    def test_review_prompt_rejects_an_incomplete_truncated_diff(self):
        with mock.patch.object(prompt_builder, "build_context", return_value={"AGENTS_md": ""}), \
             mock.patch.object(prompt_builder, "_gh", return_value="x" * (prompt_builder.MAX_REVIEW_DIFF_CHARS + 1)):
            with self.assertRaisesRegex(ValueError, "complete PR diff exceeds"):
                prompt_builder.build_review_prompt(207, "a" * 40)

    def test_capacity_full_is_nonterminal(self):
        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="repo"), \
             mock.patch.object(dispatcher.sm, "get_active_issue_numbers", return_value={1, 2}):
            result = dispatcher.dispatch_next("1")
        self.assertFalse(result["dispatched"])
        self.assertEqual(result["reason"], "capacity-full")

    def test_disabled_dispatch_does_not_claim_or_block(self):
        with mock.patch.object(dispatcher.control_state, "require_live", side_effect=control_state.ControlStateError("disabled")):
            result = dispatcher.dispatch_ready(12, "worker:12")
        self.assertFalse(result["dispatched"])
        self.assertEqual(result["reason"], "disabled_or_emergency_stopped")

    def test_invalid_scope_is_rejected_before_capacity_claim_or_workflow(self):
        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="acme/repo"), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", return_value=None), \
             mock.patch.object(dispatcher.sm, "get_issue_labels_checked", return_value={state_manager.LABEL_READY}), \
             mock.patch.object(dispatcher.sm, "validate_task_scope", return_value=(False, "wildcard")), \
             mock.patch.object(dispatcher.sm, "record_dispatch_state", return_value=True) as record, \
             mock.patch.object(dispatcher, "_run_workflow") as workflow:
            result = dispatcher.dispatch_ready(77, "worker:77")
        self.assertFalse(result["dispatched"])
        self.assertIn("invalid_scope", result["reason"])
        record.assert_called_once()
        workflow.assert_not_called()

    def test_dependency_is_rechecked_by_the_serialized_claim(self):
        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="acme/repo"), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", return_value=None), \
             mock.patch.object(dispatcher.sm, "get_issue_labels_checked", return_value={state_manager.LABEL_READY}), \
             mock.patch.object(dispatcher.sm, "validate_task_scope", return_value=(True, ["src/"])), \
             mock.patch.object(dispatcher.sm, "check_dependencies_complete", return_value=(False, 41)), \
             mock.patch.object(dispatcher, "_run_workflow") as workflow:
            result = dispatcher.dispatch_ready(77, "worker:77")
        self.assertFalse(result["dispatched"])
        self.assertEqual(result["reason"], "dependencies_not_ready:41")
        workflow.assert_not_called()

    def test_failed_dispatch_reports_failed_rollback(self):
        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_claim", return_value=(True, [state_manager.LABEL_READY], "claimed")), \
             mock.patch.object(dispatcher, "_run_workflow", return_value=False), \
             mock.patch.object(dispatcher, "_rollback", return_value=False):
            result = dispatcher.dispatch_ready(77, "worker:77")
        self.assertEqual(result["reason"], "workflow_dispatch_failed_rollback_failed")

    def test_retry_review_derives_and_revalidates_the_current_binding(self):
        worker = {
            "pr_number": 207,
            "head_sha": "a" * 40,
            "extra": {"branch": "agent/issue-42"},
        }
        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="acme/repo"), \
             mock.patch.object(dispatcher.sm, "get_issue_labels_checked", return_value={state_manager.LABEL_REVIEW_BLOCKED}), \
             mock.patch.object(dispatcher.sm, "read_worker_state", return_value=worker), \
             mock.patch.object(dispatcher.sm, "verify_issue_pr_binding", return_value=(True, "ok")), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", return_value=None), \
             mock.patch.object(dispatcher.sm, "get_active_issue_numbers", return_value=set()), \
             mock.patch.object(dispatcher.sm, "set_labels", return_value=True) as set_labels, \
             mock.patch.object(dispatcher.sm, "record_dispatch_state", return_value=True), \
             mock.patch.object(dispatcher, "_run_workflow", return_value=True) as workflow:
            result = dispatcher.retry_review(42)
        self.assertTrue(result["dispatched"])
        set_labels.assert_any_call(42, state_manager.LABEL_REVIEW_RUNNING, repo="acme/repo")
        workflow.assert_called_once_with(
            "agent-review.yml",
            {"pr_number": 207, "issue_number": 42, "head_sha": "a" * 40},
        )


class TestCIEventTrust(unittest.TestCase):
    def event_file(self, event: dict) -> str:
        file = tempfile.NamedTemporaryFile(mode="w", delete=False)
        json.dump(event, file)
        file.close()
        return file.name

    def test_foreign_or_fork_workflow_run_is_rejected_before_state_mutation(self):
        path = self.event_file({
            "repository": {"full_name": "trusted/repo"},
            "workflow_run": {"head_repository": {"full_name": "fork/repo"}, "status": "completed", "conclusion": "failure"},
        })
        try:
            with mock.patch.dict(os.environ, {"AGENT_REPO": "trusted/repo"}, clear=False):
                result = ci_handler.process_ci_completion(path)
        finally:
            os.unlink(path)
        self.assertEqual(result["action"], "noop")
        self.assertEqual(result["reason"], "fork_or_foreign_head_repository")

    def test_exact_head_run_with_a_different_branch_is_stale(self):
        sha = "1" * 40
        body = 'Closes #42\n\n<!-- agent-orchestrator-binding: {"issue_number": 42, "branch": "agent/issue-42"} -->'
        event_path = self.event_file({
            "repository": {"full_name": "trusted/repo"},
            "workflow_run": {
                "name": "tests", "status": "completed", "conclusion": "success",
                "head_branch": "agent/issue-42", "head_sha": sha, "id": 99,
                "head_repository": {"full_name": "trusted/repo"},
                "pull_requests": [{"number": 207, "head": {"sha": sha}}],
            },
        })
        pr = {
            "state": "OPEN", "headRefName": "agent/issue-42", "headRefOid": sha,
            "body": body,
        }
        run = {
            "workflowName": "tests", "status": "completed", "conclusion": "success",
            "headSha": sha, "headBranch": "agent/other", "databaseId": 99,
        }
        try:
            with mock.patch.dict(os.environ, {"AGENT_REPO": "trusted/repo"}, clear=False), \
                 mock.patch.object(ci_handler.ci_verifier, "run_info", return_value=run), \
                 mock.patch.object(ci_handler.sm, "get_pr_info", return_value=pr):
                result = ci_handler.process_ci_completion(event_path)
        finally:
            os.unlink(event_path)
        self.assertEqual(result, {"action": "stale", "reason": "head_sha_mismatch"})


class TestRepairHeadTransition(unittest.TestCase):
    def test_post_push_verification_accepts_h2_before_new_worker_state(self):
        body = 'Closes #42\n\n<!-- agent-orchestrator-binding: {"issue_number": 42, "branch": "agent/issue-42"} -->'
        pr = {
            "number": 207, "state": "OPEN", "baseRefName": "main",
            "headRefName": "agent/issue-42", "headRefOid": "b" * 40,
            "body": body, "url": "https://github.com/acme/repo/pull/207",
        }
        with mock.patch.object(pr_binding, "_open_prs", return_value=[pr]):
            result = pr_binding.verify_post_push_binding(42, 207, "agent/issue-42", "b" * 40, "acme/repo")
        self.assertEqual(result["head_sha"], "b" * 40)

    def test_new_worker_state_binds_h2_and_old_head_is_rejected(self):
        pr = {
            "number": 207, "state": "OPEN", "baseRefName": "main",
            "headRefName": "agent/issue-42", "headRefOid": "b" * 40,
            "body": 'Closes #42\n\n<!-- agent-orchestrator-binding: {"issue_number": 42, "branch": "agent/issue-42"} -->',
        }
        with mock.patch.object(state_manager, "get_pr_info", return_value=pr), \
             mock.patch.object(state_manager, "read_worker_state", return_value={"pr_number": 207, "head_sha": "b" * 40, "extra": {"branch": "agent/issue-42"}}), \
             mock.patch.object(state_manager, "_gh", return_value="[]"):
            ok, reason = state_manager.verify_issue_pr_binding(42, 207, "b" * 40, "acme/repo")
            old_ok, old_reason = state_manager.verify_issue_pr_binding(42, 207, "a" * 40, "acme/repo")
        self.assertTrue(ok, reason)
        self.assertFalse(old_ok)
        self.assertEqual(old_reason, "head_mismatch")

    def test_old_ci_and_review_evidence_cannot_authorize_new_head(self):
        with mock.patch.object(state_manager, "get_issue_labels", return_value={state_manager.LABEL_REVIEW_PASSED, state_manager.LABEL_MERGE_READY}), \
             mock.patch.object(state_manager, "get_pr_info", return_value={"state": "OPEN", "baseRefName": "main", "headRefOid": "b" * 40, "mergeable": "MERGEABLE", "mergeStateStatus": "CLEAN", "reviews": []}), \
             mock.patch.object(state_manager, "verify_issue_pr_binding", return_value=(True, "ok")), \
             mock.patch.object(state_manager, "read_review_state", return_value={"pr_number": 207, "head_sha": "a" * 40, "verdict": "PASS"}), \
             mock.patch.object(state_manager, "unresolved_review_threads", return_value=[]), \
             mock.patch.object(control_state, "require_auto_merge", return_value={}):
            with self.assertRaises(RuntimeError):
                state_manager.verify_merge_requirements(207, 42, "b" * 40, "acme/repo")


class TestExactHeadCI(unittest.TestCase):
    def test_all_seven_required_jobs_must_succeed_on_exact_head(self):
        required = ci_verifier.load_requirements()["required_jobs"]
        run = {
            "databaseId": 1, "workflowName": "tests", "headSha": "a" * 40,
            "status": "completed", "conclusion": "success",
            "jobs": [{"name": name, "status": "completed", "conclusion": "success"} for name in required],
        }
        with mock.patch.object(ci_verifier, "run_info", return_value=run):
            result = ci_verifier.verify_exact_head_ci(2, "a" * 40, 1, {"headRefOid": "a" * 40})
        self.assertEqual(result["successful_jobs"], required)

    def test_moved_head_or_missing_job_is_rejected(self):
        required = ci_verifier.load_requirements()["required_jobs"]
        run = {
            "databaseId": 1, "workflowName": "tests", "headSha": "a" * 40,
            "status": "completed", "conclusion": "success",
            "jobs": [{"name": name, "status": "completed", "conclusion": "success"} for name in required[:-1]],
        }
        with mock.patch.object(ci_verifier, "run_info", return_value=run):
            with self.assertRaises(ci_verifier.CIVerificationError):
                ci_verifier.verify_exact_head_ci(2, "a" * 40, 1, {"headRefOid": "b" * 40})

    def test_existing_natural_exact_head_run_is_reused_without_dispatch(self):
        run = {"databaseId": 11, "event": "pull_request", "status": "queued", "conclusion": "", "headSha": "c" * 40, "headBranch": "agent/issue-7", "workflowName": "tests"}
        with mock.patch.object(ci_verifier, "find_exact_runs", return_value=[run]), \
             mock.patch.object(ci_verifier.subprocess, "run") as dispatch:
            result = ci_verifier.acquire_exact_ci(7, "agent/issue-7", "c" * 40, observe_seconds=1)
        dispatch.assert_not_called()
        self.assertEqual(result["workflow_run_id"], 11)
        self.assertEqual(result["source"], "pull_request")

    def test_missing_exact_head_run_dispatches_once_then_binds_one(self):
        run = {"databaseId": 12, "event": "workflow_dispatch", "status": "queued", "conclusion": "", "headSha": "d" * 40, "headBranch": "agent/issue-8", "workflowName": "tests"}
        with mock.patch.object(ci_verifier, "find_exact_runs", side_effect=[[], [run]]), \
             mock.patch.object(ci_verifier.subprocess, "run", return_value=mock.Mock(returncode=0)) as dispatch:
            result = ci_verifier.acquire_exact_ci(8, "agent/issue-8", "d" * 40, observe_seconds=0)
        dispatch.assert_called_once()
        self.assertEqual(result["workflow_run_id"], 12)
        self.assertEqual(result["source"], "workflow_dispatch")

    def test_two_exact_head_runs_select_one_and_mark_duplicate(self):
        runs = [
            {"databaseId": 21, "event": "pull_request", "status": "queued", "conclusion": "", "headSha": "e" * 40, "headBranch": "agent/issue-9", "workflowName": "tests"},
            {"databaseId": 22, "event": "workflow_dispatch", "status": "queued", "conclusion": "", "headSha": "e" * 40, "headBranch": "agent/issue-9", "workflowName": "tests"},
        ]
        with mock.patch.object(ci_verifier, "find_exact_runs", return_value=runs):
            result = ci_verifier.acquire_exact_ci(9, "agent/issue-9", "e" * 40, observe_seconds=1)
        self.assertEqual(result["workflow_run_id"], 21)
        self.assertEqual(result["duplicate_run_ids"], [22])

    def test_duplicate_success_or_failure_event_is_idempotent(self):
        with mock.patch.object(ci_handler.sm, "read_ci_acquisition", return_value={"workflow_run_id": 31}), \
             mock.patch.object(ci_handler.sm, "read_ci_state", return_value=None), \
             mock.patch.object(ci_handler.ci_verifier, "find_exact_runs", return_value=[]):
            self.assertTrue(ci_handler._is_duplicate_exact_head_run(42, 207, "f" * 40, 32, "agent/issue-42"))
        with mock.patch.object(ci_handler.sm, "read_ci_acquisition", return_value=None), \
             mock.patch.object(ci_handler.sm, "read_ci_state", return_value={"pr_number": 207, "head_sha": "f" * 40, "workflow_run_id": 33, "status": "failure_repair_0"}), \
             mock.patch.object(ci_handler.ci_verifier, "find_exact_runs", return_value=[]):
            self.assertTrue(ci_handler._is_duplicate_exact_head_run(42, 207, "f" * 40, 33, "agent/issue-42"))

    def test_cancelled_run_is_not_acquired_when_fallback_becomes_available(self):
        cancelled = {
            "databaseId": 41, "event": "pull_request", "status": "completed", "conclusion": "cancelled",
            "headSha": "f" * 40, "headBranch": "agent/issue-42", "workflowName": "tests",
        }
        fallback = {
            "databaseId": 42, "event": "workflow_dispatch", "status": "queued", "conclusion": "",
            "headSha": "f" * 40, "headBranch": "agent/issue-42", "workflowName": "tests",
        }
        with mock.patch.object(ci_verifier, "find_exact_runs", side_effect=[[cancelled], [cancelled, fallback]]), \
             mock.patch.object(ci_verifier.subprocess, "run", return_value=mock.Mock(returncode=0)) as dispatch:
            result = ci_verifier.acquire_exact_ci(207, "agent/issue-42", "f" * 40, observe_seconds=0)
        dispatch.assert_called_once()
        self.assertEqual(result["workflow_run_id"], 42)

    def test_bound_fallback_is_not_shadowed_by_older_cancelled_run(self):
        sha = "f" * 40
        runs = [
            {
                "databaseId": 41,
                "status": "completed",
                "conclusion": "cancelled",
            },
            {
                "databaseId": 42,
                "status": "completed",
                "conclusion": "success",
            },
        ]
        with mock.patch.object(
            ci_handler.sm,
            "read_ci_acquisition",
            return_value={"workflow_run_id": 42},
        ), mock.patch.object(
            ci_handler.sm, "read_ci_state", return_value=None
        ), mock.patch.object(
            ci_handler.ci_verifier, "find_exact_runs", return_value=runs
        ):
            duplicate = ci_handler._is_duplicate_exact_head_run(
                42, 207, sha, 42, "agent/issue-42"
            )
        self.assertFalse(duplicate)

    def test_cancelled_authoritative_run_blocks_and_releases_capacity(self):
        sha = "f" * 40
        body = 'Closes #42\n\n<!-- agent-orchestrator-binding: {"issue_number": 42, "branch": "agent/issue-42"} -->'
        event_path = tempfile.NamedTemporaryFile(mode="w", delete=False)
        json.dump({
            "repository": {"full_name": "trusted/repo"},
            "workflow_run": {
                "name": "tests", "status": "completed", "conclusion": "cancelled",
                "head_branch": "agent/issue-42", "head_sha": sha, "id": 41,
                "head_repository": {"full_name": "trusted/repo"},
                "pull_requests": [{"number": 207, "head": {"sha": sha}}],
            },
        }, event_path)
        event_path.close()
        run = {
            "databaseId": 41, "workflowName": "tests", "status": "completed", "conclusion": "cancelled",
            "headSha": sha, "headBranch": "agent/issue-42", "jobs": [],
        }
        pr = {"state": "OPEN", "headRefName": "agent/issue-42", "headRefOid": sha, "body": body}
        try:
            with mock.patch.dict(os.environ, {"AGENT_REPO": "trusted/repo"}, clear=False), \
                 mock.patch.object(ci_handler.ci_verifier, "run_info", return_value=run), \
                 mock.patch.object(ci_handler.sm, "get_pr_info", return_value=pr), \
                 mock.patch.object(ci_handler, "_find_issue_for_pr", return_value=42), \
                 mock.patch.object(ci_handler, "_is_duplicate_exact_head_run", return_value=False), \
                 mock.patch.object(ci_handler.sm, "verify_issue_pr_binding", return_value=(True, "ok")), \
                 mock.patch.object(ci_handler, "_record_ci") as record:
                result = ci_handler.process_ci_completion(event_path.name)
            with mock.patch.dict(os.environ, {"AGENT_REPO": "trusted/repo"}, clear=False), \
                 mock.patch.object(ci_handler.ci_verifier, "run_info", return_value=run), \
                 mock.patch.object(ci_handler.sm, "get_pr_info", return_value=pr), \
                 mock.patch.object(ci_handler, "_find_issue_for_pr", return_value=42), \
                 mock.patch.object(ci_handler, "_is_duplicate_exact_head_run", return_value=False), \
                 mock.patch.object(ci_handler.sm, "verify_issue_pr_binding", return_value=(True, "ok")), \
                 mock.patch.object(ci_handler, "_record_ci", side_effect=RuntimeError("write failed")):
                unavailable = ci_handler.process_ci_completion(event_path.name)
        finally:
            os.unlink(event_path.name)
        self.assertEqual(result["action"], "blocked")
        self.assertEqual(result["reason"], "ci_terminal_cancelled")
        record.assert_called_once()
        self.assertEqual(unavailable["action"], "blocked")
        self.assertIn("ci_state_unavailable", unavailable["reason"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
