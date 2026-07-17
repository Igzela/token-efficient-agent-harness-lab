"""Regression tests for the event-driven orchestrator's deterministic path."""

from __future__ import annotations

import json
import ast
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
# Synthetic GitHub-run fixtures must opt out of production identity enrichment.
os.environ.setdefault("AGENT_CI_FIXTURE_MODE", "true")
sys.path.insert(0, str(CONTROL))

import ci_handler
import ci_verifier
import control_state
import dispatcher
import pr_binding
import prompt_builder
import runner_readiness
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
        self.assertIn("ci_verifier.py acquire-run", worker)
        self.assertIn("ci_verifier.py acquire-run", repair)
        self.assertNotIn("ci_verifier.py acquire ", worker)
        self.assertNotIn("ci_verifier.py acquire ", repair)

    def test_review_worker_is_read_only_and_only_pass_authorizes_merge(self):
        source = self.read("agent-review.yml")
        vader = source.split("  vader-review:", 1)[1].split("\n  finalize:", 1)[0]
        self.assertIn("codex_wrapper.sh review", vader)
        self.assertNotIn("git commit", vader)
        self.assertNotIn("git push", vader)
        self.assertIn("validate_review.py", source)
        self.assertIn("steps.verdict.outputs.verdict == 'PASS'", source)
        self.assertIn("codex-last-message.txt", vader)
        self.assertNotIn("codex-last-message.json", vader)
        self.assertIn("review-result.txt", vader)
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

    def test_review_evidence_uses_a_sidecar_not_shell_or_workflow_output_text(self):
        source = self.read("agent-review.yml")
        self.assertIn("--evidence-file \"$RUNNER_TEMP/review-validation.json\"", source)
        self.assertIn("record-review-failure", source)
        self.assertNotIn("steps.verdict.outputs.summary", source)

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

    def test_ci_monitor_consumes_every_terminal_dispatch_outcome(self):
        monitor = self.read("agent-ci-monitor.yml")
        handler = ast.parse((CONTROL / "ci_handler.py").read_text())
        actions = set()
        for node in ast.walk(handler):
            if isinstance(node, ast.Dict):
                for key, value in zip(node.keys, node.values):
                    if isinstance(key, ast.Constant) and key.value == "action":
                        if isinstance(value, ast.Constant) and isinstance(value.value, str):
                            actions.add(value.value)
            if isinstance(node, ast.Call) and isinstance(node.func, ast.Name):
                if node.func.id == "_record_ci_terminal":
                    for keyword in node.keywords:
                        if keyword.arg == "action" and isinstance(keyword.value, ast.Constant):
                            actions.add(keyword.value.value)
                if node.func.id == "_record_ci_noop":
                    actions.add("noop")
            if isinstance(node, ast.Assign) and any(
                isinstance(target, ast.Name) and target.id == "action"
                for target in node.targets
            ) and isinstance(node.value, ast.IfExp):
                for branch in (node.value.body, node.value.orelse):
                    if isinstance(branch, ast.Constant) and isinstance(branch.value, str):
                        actions.add(branch.value)
        self.assertEqual(actions, {"trigger_repair", "trigger_review", "merge_ready", "blocked", "stale", "noop"})
        for action in sorted(actions):
            with self.subTest(action=action):
                self.assertIn(f"action == '{action}'", monitor)
        self.assertIn("terminal_status != ''", monitor)
        self.assertIn("release-ci-terminal", monitor)
        self.assertIn("agent-blocked", monitor)
        self.assertIn("terminal_status == '' && steps.decision.outputs.action == 'trigger_review'", monitor)
        self.assertIn("terminal_status == '' && steps.decision.outputs.action == 'trigger_repair'", monitor)
        self.assertIn("terminal_status == '' && steps.decision.outputs.action == 'merge_ready'", monitor)
        self.assertIn("terminal_status == '' && steps.decision.outputs.action == 'blocked'", monitor)
        self.assertIn('steps.decision.outputs.action == \'noop\'', monitor)
        self.assertIn('"$TERMINAL_STATUS" "$NOOP_REASON" "$OBSERVED_STATUS"', monitor)
        self.assertIn("steps.decision.outputs.terminal_status != ''", monitor)
        self.assertIn("steps.decision.outputs.terminal_status == ''", monitor)
        self.assertIn("_record_ci_noop", (CONTROL / "ci_handler.py").read_text())
        self.assertIn("Record control stop during follow-up handoff", monitor)
        self.assertIn("continue-on-error: true", monitor)
        self.assertIn('terminal_ci_control_stopped ci_control_stopped:before_followup_dispatch "$OBSERVED_STATUS"', monitor)
        for action, required in {
            "trigger_repair": "dispatcher.py dispatch-repair",
            "trigger_review": "dispatcher.py dispatch-review",
            "merge_ready": "require-auto-merge",
            "blocked": "release-failed",
            "stale": "release-ci-terminal",
            "noop": "release-failed",
        }.items():
            with self.subTest(action=action):
                self.assertIn(required, monitor)
        worker = self.read("agent-worker.yml")
        self.assertIn("id: acquire_ci", worker)
        self.assertIn("ci_control_stopped=true", worker)
        self.assertIn("steps.acquire_ci.outputs.ci_control_stopped != 'true'", worker)
        self.assertIn("record-ci", worker)
        self.assertIn("cancel-in-progress: ${{ inputs.command == 'emergency-stop' }}", self.read("agent-controller.yml"))
        for workflow in ("agent-ci-monitor.yml", "agent-review.yml", "agent-ci-repair.yml", "agent-merge.yml", "agent-worker.yml", "agent-intake.yml"):
            self.assertIn("group: agent-orchestrator-state", self.read(workflow))
        self.assertIn("agent-orchestrator-state", self.read("agent-controller.yml"))

    def test_ci_monitor_action_contract_executes_a_consumer_for_each_handler_action(self):
        blocks = self.read("agent-ci-monitor.yml").split("      - name: ")[1:]
        handler = ast.parse((CONTROL / "ci_handler.py").read_text())
        process = next(
            node for node in ast.walk(handler)
            if isinstance(node, ast.FunctionDef) and node.name == "process_ci_dispatch"
        )
        handler_actions = {
            node.value
            for node in ast.walk(process)
            if isinstance(node, ast.Constant) and isinstance(node.value, str)
            and node.value in {"trigger_repair", "trigger_review", "merge_ready", "blocked", "stale", "noop"}
        }
        names = {node.id for node in ast.walk(process) if isinstance(node, ast.Name)}
        if "_record_ci_terminal" in names:
            handler_actions.add("blocked")
        if "_record_ci_noop" in names or "_unbound_noop_result" in names:
            handler_actions.add("noop")
        # The source walk above is intentionally restricted to the action
        # vocabulary; this assertion protects the contract when a new action
        # is introduced in the handler.
        self.assertEqual(
            handler_actions,
            {"trigger_repair", "trigger_review", "merge_ready", "blocked", "stale", "noop"},
        )
        action_contract = {
            "trigger_repair": (
                "steps.decision.outputs.terminal_status == '' && steps.decision.outputs.action == 'trigger_repair'",
                "dispatcher.py dispatch-repair",
            ),
            "trigger_review": (
                "steps.decision.outputs.terminal_status == '' && steps.decision.outputs.action == 'trigger_review'",
                "dispatcher.py dispatch-review",
            ),
            "merge_ready": (
                "steps.decision.outputs.terminal_status == '' && steps.decision.outputs.action == 'merge_ready'",
                "require-auto-merge",
            ),
            "blocked": (
                "steps.decision.outputs.terminal_status == '' && steps.decision.outputs.action == 'blocked'",
                "release-failed",
            ),
            "stale": (
                "steps.decision.outputs.action == 'stale'",
                "release-ci-terminal",
            ),
            "noop": (
                "steps.decision.outputs.action == 'noop'",
                "release-failed",
            ),
        }
        for action, (condition, command) in action_contract.items():
            candidates = [
                block for block in blocks
                if condition in block and "run:" in block
            ]
            with self.subTest(action=action):
                self.assertTrue(candidates)
                if action == "noop":
                    self.assertTrue(any("terminal_status != ''" in block for block in candidates))
                    self.assertTrue(any("terminal_status == ''" in block for block in candidates))
                self.assertTrue(any(command in block for block in candidates))

        for block in blocks:
            if "state_manager.py release-ci-terminal" in block:
                for field in ("$ISSUE_NUMBER", "$PR_NUMBER", "$HEAD_SHA", "$CI_RUN_ID"):
                    self.assertIn(field, block)

        self.assertIn("dispatcher.py dispatch-repair", self.read("agent-ci-monitor.yml"))
        self.assertIn("dispatcher.py dispatch-review", self.read("agent-ci-monitor.yml"))
        self.assertIn("dispatcher.py dispatch-merge", self.read("agent-ci-monitor.yml"))
        self.assertIn("ci_handler.py validate-decision", self.read("agent-ci-monitor.yml"))

        for action, consumer in ci_handler.DISPATCH_ACTION_CONSUMERS.items():
            result = {
                "action": action,
                "issue_number": 42,
                "pr_number": 207,
                "head_sha": "a" * 40,
                "ci_run_id": 9001,
            }
            if action in {"blocked", "stale", "noop"}:
                result["terminal_status"] = f"terminal_{action}"
            if action == "trigger_repair":
                result["repair_count"] = 1
            self.assertEqual(ci_handler.validate_ci_dispatch_decision(result), consumer)

        monitor = self.read("agent-ci-monitor.yml")
        self.assertIn("terminal_ci_followup_dispatch_failed", monitor)
        self.assertIn('"ci_followup_dispatch_failed:$ACTION" "$OBSERVED_STATUS"', monitor)
        self.assertIn("terminal_ci_merge_dispatch_failed", monitor)
        self.assertIn("ci_merge_dispatch_failed:merge_ready", monitor)
        self.assertIn("capacity_retained", monitor)
        self.assertIn("dispatch_in_flight", monitor)
        self.assertIn("capacity_already_claimed", monitor)
        self.assertIn("REPAIR_CAPACITY_RETAINED", monitor)
        self.assertIn("MERGE_CAPACITY_RETAINED", monitor)
        self.assertIn("has_inflight_ci_dispatch", (CONTROL / "state_manager.py").read_text())

    def test_worker_has_post_claim_nonstart_capacity_release(self):
        source = self.read("agent-worker.yml")
        self.assertIn("rejected-before-vader:", source)
        self.assertIn("release-rejected-worker", source)
        self.assertIn("needs: [gate, validate, vader-implementation, finalize]", source)


class TestCITerminalState(unittest.TestCase):
    def test_explicit_dispatch_unbound_noop_is_terminal_and_exactly_bound(self):
        result = ci_handler._unbound_noop_result(
            42, 207, "a" * 40, 9001, "pr_unavailable",
        )
        self.assertEqual(result["action"], "noop")
        self.assertEqual(result["terminal_status"], "terminal_ci_unbound_noop")
        self.assertEqual(
            {result[key] for key in ("issue_number", "pr_number", "head_sha", "ci_run_id")},
            {42, 207, "a" * 40, 9001},
        )

    def test_terminal_resolution_is_idempotent_and_records_release_result(self):
        comments = []

        def read_comments(*args, **kwargs):
            return list(comments)

        def write_comment(issue, body, repo=""):
            comments.append({
                "author": {"login": "github-actions[bot]"},
                "body": body,
            })
            return True

        with mock.patch.object(
            state_manager, "release_failed_capacity",
            side_effect=[(True, "released"), (True, "already_released")],
        ), mock.patch.object(
            state_manager, "get_issue_comments", side_effect=read_comments,
        ), mock.patch.object(
            state_manager, "comment_on_issue", side_effect=write_comment,
        ) as comment:
            first = state_manager.release_and_record_ci_terminal(
                42, 207, "a" * 40, 9001, "terminal_ci_stale_binding",
                "ci_stale_binding:pr_head_moved", "in_progress",
            )
            second = state_manager.release_and_record_ci_terminal(
                42, 207, "a" * 40, 9001, "terminal_ci_stale_binding",
                "ci_stale_binding:pr_head_moved", "in_progress",
            )

        self.assertEqual(first, (True, "released"))
        self.assertEqual(second, (True, "already_recorded"))
        self.assertEqual(comment.call_count, 2)
        evidence = json.loads(comments[-1]["body"])
        self.assertEqual(evidence["issue_number"], 42)
        self.assertEqual(evidence["pr_number"], 207)
        self.assertEqual(evidence["head_sha"], "a" * 40)
        self.assertEqual(evidence["ci_run_id"], 9001)
        self.assertEqual(evidence["capacity_release_outcome"], "released")

    def test_unbound_noop_terminal_release_uses_worker_binding_without_ci_state(self):
        comments = []

        def write_comment(issue, body, repo=""):
            comments.append({"author": {"login": "github-actions[bot]"}, "body": body})
            return True

        with mock.patch.object(
            state_manager, "get_issue_comments", return_value=[]
        ), mock.patch.object(
            state_manager, "comment_on_issue", side_effect=write_comment
        ), mock.patch.object(
            state_manager, "get_issue_labels_checked", return_value={state_manager.LABEL_RUNNING}
        ), mock.patch.object(
            state_manager, "read_worker_state", return_value={
                "pr_number": 207, "head_sha": "a" * 40,
                "extra": {"branch": "agent/issue-42"},
            }
        ), mock.patch.object(
            state_manager, "get_pr_info", return_value=None
        ), mock.patch.object(
            state_manager, "set_labels", return_value=True
        ) as transition:
            result = state_manager.release_and_record_ci_terminal(
                42, 207, "a" * 40, 9001, "terminal_ci_unbound_noop",
                "ci_unbound_noop:pr_unavailable", "unknown",
            )
        self.assertEqual(result, (True, "released"))
        transition.assert_called_once_with(42, state_manager.LABEL_BLOCKED, repo="")
        self.assertEqual(json.loads(comments[-1]["body"])["ci_run_id"], 9001)

    def test_unbound_noop_cannot_demote_review_capacity(self):
        comments = []

        def write_comment(issue, body, repo=""):
            comments.append({"author": {"login": "github-actions[bot]"}, "body": body})
            return True

        with mock.patch.object(
            state_manager, "get_issue_comments", return_value=[]
        ), mock.patch.object(
            state_manager, "comment_on_issue", side_effect=write_comment
        ), mock.patch.object(
            state_manager, "get_issue_labels_checked",
            return_value={state_manager.LABEL_REVIEW_RUNNING},
        ), mock.patch.object(
            state_manager, "set_labels", return_value=True
        ) as transition:
            result = state_manager.release_and_record_ci_terminal(
                42, 207, "a" * 40, 9001, "terminal_ci_unbound_noop",
                "ci_unbound_noop:pr_unavailable", "unknown",
            )

        self.assertEqual(result, (False, "ci_active_phase_mismatch"))
        transition.assert_not_called()
        evidence = json.loads(comments[-1]["body"])
        self.assertEqual(evidence["capacity_release_outcome"], "failed")
        self.assertEqual(evidence["capacity_release_reason"], "ci_active_phase_mismatch")

    def test_terminal_audit_failure_precedes_capacity_release(self):
        with mock.patch.object(
            state_manager, "get_issue_comments", return_value=[]
        ), mock.patch.object(
            state_manager, "comment_on_issue", return_value=False
        ), mock.patch.object(
            state_manager, "release_failed_capacity"
        ) as release:
            result = state_manager.release_and_record_ci_terminal(
                42, 207, "a" * 40, 9001, "terminal_ci_control_stopped",
                "ci_control_stopped:emergency_stop", "in_progress",
            )
        self.assertEqual(result, (False, "terminal_resolution_write_failed"))
        release.assert_not_called()

    def test_duplicate_noop_preserves_accepted_repair_claim(self):
        comments = [{
            "author": {"login": "github-actions[bot]"},
            "body": json.dumps({
                "kind": "agent-orchestrator-dispatch-state",
                "status": "claimed",
                "action": "repair",
                "details": {
                    "pr_number": 207,
                    "issue_number": 42,
                    "head_sha": "a" * 40,
                    "ci_run_id": "9001",
                },
            }),
        }]

        def write_comment(issue, body, repo=""):
            comments.append({"author": {"login": "github-actions[bot]"}, "body": body})
            return True

        with mock.patch.object(
            state_manager, "get_issue_comments", return_value=comments
        ), mock.patch.object(
            state_manager, "comment_on_issue", side_effect=write_comment
        ), mock.patch.object(
            state_manager, "release_failed_capacity"
        ) as release:
            result = state_manager.release_and_record_ci_terminal(
                42, 207, "a" * 40, 9001, "terminal_noop",
                "ci_noop:duplicate_exact_head_run", "completed",
            )

        self.assertEqual(result, (True, "dispatch_in_flight"))
        release.assert_not_called()
        evidence = json.loads(comments[-1]["body"])
        self.assertEqual(evidence["capacity_release_outcome"], "preserved")
        self.assertEqual(evidence["capacity_release_reason"], "dispatch_in_flight")

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
                "print(json.dumps({'databaseId':456,'workflowName':'tests','status':'completed','conclusion':'failure','headSha':'%s','headBranch':'agent/issue-42'}))\n"
                % sha
            )
            gh_path.chmod(gh_path.stat().st_mode | stat.S_IXUSR)
            env = {**os.environ, "PATH": f"{temp}:{os.environ['PATH']}"}
            env.update({"AGENT_REPO": "", "GITHUB_REPOSITORY": ""})
            result = subprocess.run(
                [sys.executable, str(CONTROL / "ci_verifier.py"), "verify-failed-run", "456", sha, "agent/issue-42", "207"],
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

    def test_final_dispatch_audit_failure_retains_claim_and_blocks_duplicate(self):
        labels = {state_manager.LABEL_RUNNING}
        dispatch_states = {}
        workflow_calls = []

        def read_dispatch(_issue, dispatch_id, _repo):
            return dispatch_states.get(dispatch_id)

        def set_labels(_issue, *new_labels, repo=""):
            labels.clear()
            labels.update(new_labels)
            return True

        def record_dispatch(_issue, dispatch_id, action, status, details=None, repo=""):
            if status == "claimed":
                dispatch_states[dispatch_id] = {"action": action, "status": status}
                return True
            return False

        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="repo"), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", side_effect=read_dispatch), \
             mock.patch.object(dispatcher.sm, "get_issue_labels_checked", return_value=labels), \
             mock.patch.object(dispatcher.sm, "get_active_issue_numbers", return_value={42}), \
             mock.patch.object(dispatcher.sm, "set_labels", side_effect=set_labels), \
             mock.patch.object(dispatcher.sm, "record_dispatch_state", side_effect=record_dispatch), \
             mock.patch.object(dispatcher, "_run_workflow", side_effect=lambda *args: workflow_calls.append(args) or True):
            first = dispatcher.dispatch_review(207, 42, "a" * 40)
            second = dispatcher.dispatch_review(207, 42, "a" * 40)

        self.assertFalse(first["dispatched"])
        self.assertEqual(first["reason"], "dispatch_state_failed_capacity_retained")
        self.assertFalse(second["dispatched"])
        self.assertEqual(second["reason"], "dispatch_in_flight")
        self.assertEqual(len(workflow_calls), 1)
        self.assertEqual(labels, {state_manager.LABEL_REVIEW_RUNNING})

    def test_repair_dispatch_audit_failure_retains_claim_and_blocks_duplicate(self):
        labels = {state_manager.LABEL_RUNNING}
        dispatch_states = {}
        workflow_calls = []

        def read_dispatch(_issue, dispatch_id, _repo):
            return dispatch_states.get(dispatch_id)

        def set_labels(_issue, *new_labels, repo=""):
            labels.clear()
            labels.update(new_labels)
            return True

        def record_dispatch(_issue, dispatch_id, action, status, details=None, repo=""):
            if status == "claimed":
                dispatch_states[dispatch_id] = {"action": action, "status": status}
                return True
            return False

        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="repo"), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", side_effect=read_dispatch), \
             mock.patch.object(dispatcher.sm, "get_issue_labels_checked", return_value=labels), \
             mock.patch.object(dispatcher.sm, "get_active_issue_numbers", return_value={42}), \
             mock.patch.object(dispatcher.sm, "set_labels", side_effect=set_labels), \
             mock.patch.object(dispatcher.sm, "record_dispatch_state", side_effect=record_dispatch), \
             mock.patch.object(dispatcher, "_run_workflow", side_effect=lambda *args: workflow_calls.append(args) or True):
            first = dispatcher.dispatch_repair(207, 42, "a" * 40, "9001", "1")
            second = dispatcher.dispatch_repair(207, 42, "a" * 40, "9001", "1")

        self.assertFalse(first["dispatched"])
        self.assertEqual(first["reason"], "dispatch_state_failed_capacity_retained")
        self.assertFalse(second["dispatched"])
        self.assertEqual(second["reason"], "dispatch_in_flight")
        self.assertEqual(len(workflow_calls), 1)
        self.assertEqual(labels, {state_manager.LABEL_CI_REPAIRING})

    def test_merge_dispatch_audit_failure_retains_claim_and_blocks_duplicate(self):
        dispatch_states = {}
        workflow_calls = []

        def read_dispatch(_issue, dispatch_id, _repo):
            return dispatch_states.get(dispatch_id)

        def record_dispatch(_issue, dispatch_id, action, status, details=None, repo=""):
            if status == "claimed":
                dispatch_states[dispatch_id] = {"action": action, "status": status}
                return True
            return False

        with mock.patch.object(dispatcher.control_state, "require_auto_merge", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="repo"), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", side_effect=read_dispatch), \
             mock.patch.object(dispatcher.sm, "get_issue_labels_checked", return_value={state_manager.LABEL_MERGE_READY, state_manager.LABEL_REVIEW_PASSED}), \
             mock.patch.object(dispatcher.sm, "read_review_state", return_value={"pr_number": 207, "head_sha": "a" * 40, "verdict": "PASS"}), \
             mock.patch.object(dispatcher.sm, "record_dispatch_state", side_effect=record_dispatch), \
             mock.patch.object(dispatcher, "_run_workflow", side_effect=lambda *args: workflow_calls.append(args) or True):
            first = dispatcher.dispatch_merge(207, 42, "a" * 40)
            second = dispatcher.dispatch_merge(207, 42, "a" * 40)

        self.assertFalse(first["dispatched"])
        self.assertEqual(first["reason"], "dispatch_state_failed_capacity_retained")
        self.assertFalse(second["dispatched"])
        self.assertEqual(second["reason"], "dispatch_in_flight")
        self.assertEqual(len(workflow_calls), 1)

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
                 mock.patch.object(ci_handler.sm, "get_pr_info", return_value=pr), \
                 mock.patch.object(ci_handler.sm, "verify_issue_pr_binding", return_value=(True, "ok")), \
                 mock.patch.object(ci_handler.sm, "record_ci_terminal_state", return_value=True):
                result = ci_handler.process_ci_completion(event_path)
        finally:
            os.unlink(event_path)
        self.assertEqual(result["action"], "stale")
        self.assertEqual(result["terminal_status"], "terminal_ci_stale_binding")
        self.assertEqual(result["reason"], "ci_stale_binding:branch_moved")

    def test_moved_head_event_records_stale_old_binding_for_capacity_release(self):
        old_sha = "2" * 40
        new_sha = "3" * 40
        body = 'Closes #42\n\n<!-- agent-orchestrator-binding: {"issue_number": 42, "branch": "agent/issue-42"} -->'
        event_path = self.event_file({
            "repository": {"full_name": "trusted/repo"},
            "workflow_run": {
                "name": "tests", "status": "completed", "conclusion": "success",
                "head_branch": "agent/issue-42", "head_sha": old_sha, "id": 100,
                "head_repository": {"full_name": "trusted/repo"},
                "pull_requests": [{"number": 207, "head": {"sha": old_sha}}],
            },
        })
        run = {
            "databaseId": 100, "workflowName": "tests", "status": "completed",
            "conclusion": "success", "headSha": old_sha, "headBranch": "agent/issue-42",
        }
        moved_pr = {
            "state": "OPEN", "headRefName": "agent/issue-42", "headRefOid": new_sha,
            "body": body,
        }
        try:
            with mock.patch.dict(os.environ, {"AGENT_REPO": "trusted/repo"}, clear=False), \
                 mock.patch.object(ci_handler.ci_verifier, "run_info", return_value=run), \
                 mock.patch.object(ci_handler.sm, "get_pr_info", return_value=moved_pr), \
                 mock.patch.object(ci_handler, "_find_issue_for_pr", return_value=42), \
                 mock.patch.object(ci_handler.sm, "read_worker_state", return_value={
                     "pr_number": 207, "head_sha": old_sha,
                     "extra": {"branch": "agent/issue-42"},
                 }), \
                 mock.patch.object(ci_handler.sm, "record_ci_terminal_state", return_value=True):
                result = ci_handler.process_ci_completion(event_path)
        finally:
            os.unlink(event_path)
        self.assertEqual(result["action"], "stale")
        self.assertEqual(result["head_sha"], old_sha)
        self.assertEqual(result["reason"], "ci_stale_binding:pr_head_moved:current_head_changed")


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

    def test_terminal_release_accepts_only_same_issue_pr_branch_after_head_move(self):
        pr = {
            "number": 207, "state": "OPEN", "headRefName": "agent/issue-42",
            "headRefOid": "b" * 40,
            "body": 'Closes #42\n\n<!-- agent-orchestrator-binding: {"issue_number": 42, "branch": "agent/issue-42"} -->',
        }
        worker = {
            "pr_number": 207, "head_sha": "a" * 40,
            "extra": {"branch": "agent/issue-42"},
        }
        with mock.patch.object(state_manager, "get_pr_info", return_value=pr), \
             mock.patch.object(state_manager, "read_worker_state", return_value=worker), \
             mock.patch.object(state_manager, "_gh", return_value="[]"):
            ok, reason = state_manager.verify_ci_terminal_binding(42, 207, "a" * 40, "acme/repo")
        self.assertTrue(ok, reason)
        self.assertEqual(reason, "stale_head_replaced")

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
    def setUp(self):
        # Synthetic run fixtures intentionally omit provider API identity
        # fields.  Production calls populate and require these fields; keep
        # the unit fixtures independent of the runner's repository environment.
        self._clear_repo_env = mock.patch.dict(
            os.environ,
            {
                "AGENT_REPO": "",
                "GITHUB_REPOSITORY": "",
                "AGENT_CI_FIXTURE_MODE": "true",
            },
            clear=False,
        )
        self._clear_repo_env.start()
        self._control_live = mock.patch.object(
            ci_verifier, "control_is_live", return_value=True
        )
        self._control_live.start()

    def tearDown(self):
        self._control_live.stop()
        self._clear_repo_env.stop()

    def test_acquisition_stops_before_fallback_when_control_changes(self):
        sha = "c" * 40
        with mock.patch.object(ci_verifier, "control_is_live", side_effect=[True, False]), \
             mock.patch.object(ci_verifier, "find_exact_runs", return_value=[]), \
             mock.patch.object(ci_verifier.subprocess, "run") as dispatch:
            with self.assertRaises(ci_verifier.CIControlStopped) as raised:
                ci_verifier.acquire_exact_run(
                    1, "agent/issue-c", sha, observe_seconds=0,
                )
        self.assertIn("before_fallback_dispatch", str(raised.exception))
        dispatch.assert_not_called()

    def test_acquisition_rechecks_control_after_fallback_dispatch(self):
        sha = "d" * 40
        fallback = {
            "databaseId": 501, "event": "workflow_dispatch", "status": "queued",
            "conclusion": "", "headSha": sha, "headBranch": "agent/issue-d",
            "workflowName": "tests",
        }
        with mock.patch.object(ci_verifier, "control_is_live", side_effect=[True, True, True, False]), \
             mock.patch.object(ci_verifier, "find_exact_runs", side_effect=[[], [fallback]]), \
             mock.patch.object(ci_verifier, "run_info", return_value=fallback), \
             mock.patch.object(ci_verifier.subprocess, "run", return_value=mock.Mock(
                 returncode=0,
                 stdout="https://github.com/trusted/repo/actions/runs/501",
             )) as dispatch:
            with self.assertRaises(ci_verifier.CIControlStopped) as raised:
                ci_verifier.acquire_exact_run(
                    1, "agent/issue-d", sha, observe_seconds=0,
                )
        self.assertIn("after_fallback_dispatch", str(raised.exception))
        self.assertEqual(raised.exception.ci_run_id, 501)
        self.assertEqual(raised.exception.head_sha, sha)
        self.assertEqual(raised.exception.observed_run["status"], "queued")
        dispatch.assert_called_once()

    def test_delayed_fallback_visibility_rechecks_stop_and_preserves_exact_run(self):
        sha = "d" * 40
        fallback = {
            "databaseId": 778, "event": "workflow_dispatch", "status": "queued",
            "conclusion": "", "headSha": sha, "headBranch": "agent/issue-d",
            "workflowName": "tests",
        }
        dispatch = mock.Mock(returncode=0, stdout="")
        listing = mock.Mock(
            returncode=0,
            stdout=json.dumps([{
                "databaseId": 778, "name": "tests-deadbeef",
                "headSha": sha, "headBranch": "agent/issue-d",
                "event": "workflow_dispatch",
            }]),
        )
        with mock.patch.object(
            ci_verifier, "control_is_live", side_effect=[True, True, True, False, False]
        ), mock.patch.object(ci_verifier, "find_exact_runs", return_value=[]), \
             mock.patch.object(ci_verifier, "run_info", return_value=fallback), \
             mock.patch.object(ci_verifier.uuid, "uuid4", return_value=mock.Mock(hex="deadbeef")), \
             mock.patch.object(ci_verifier.subprocess, "run", side_effect=[dispatch, listing]):
            with self.assertRaises(ci_verifier.CIControlStopped) as raised:
                ci_verifier.acquire_exact_run(1, "agent/issue-d", sha, observe_seconds=0)
        self.assertEqual(raised.exception.ci_run_id, 778)
        self.assertEqual(raised.exception.observed_run["databaseId"], 778)

    def test_stopped_fallback_lookup_is_bounded_when_run_identity_never_appears(self):
        sha = "d" * 40
        dispatch = mock.Mock(returncode=0, stdout="")
        listing = mock.Mock(returncode=0, stdout="[]")
        with mock.patch.object(
            ci_verifier, "control_is_live", side_effect=[True, True, True, False]
        ), mock.patch.object(
            ci_verifier, "find_exact_runs", return_value=[]
        ), mock.patch.object(
            ci_verifier.uuid, "uuid4", return_value=mock.Mock(hex="deadbeef")
        ), mock.patch.object(
            ci_verifier.subprocess, "run", side_effect=[dispatch, listing]
        ), mock.patch.object(
            ci_verifier, "FALLBACK_STOP_RECONCILIATION_SECONDS", 0
        ), mock.patch.object(
            ci_verifier.time, "sleep", return_value=None
        ):
            with self.assertRaises(ci_verifier.CIVerificationError) as raised:
                ci_verifier.acquire_exact_run(
                    1, "agent/issue-d", sha, observe_seconds=0,
                )
        self.assertEqual(
            str(raised.exception),
            "ci_control_stopped:fallback_run_identity_missing",
        )

    def test_dispatched_fallback_id_cannot_be_replaced_by_newer_same_head_run(self):
        sha = "d" * 40
        fallback = {
            "databaseId": 779, "event": "workflow_dispatch", "status": "queued",
            "conclusion": "", "headSha": sha, "headBranch": "agent/issue-d",
            "workflowName": "tests",
        }
        newer = {
            "databaseId": 780, "event": "pull_request", "status": "completed",
            "conclusion": "success", "headSha": sha, "headBranch": "agent/issue-d",
            "workflowName": "tests",
        }
        with mock.patch.object(ci_verifier, "find_exact_runs", side_effect=[[], [newer]]), \
             mock.patch.object(ci_verifier, "run_info", return_value=fallback), \
             mock.patch.object(ci_verifier.subprocess, "run", return_value=mock.Mock(
                 returncode=0, stdout="https://github.com/trusted/repo/actions/runs/779"
             )):
            result = ci_verifier.acquire_exact_run(1, "agent/issue-d", sha, observe_seconds=0)
        self.assertEqual(result["workflow_run_id"], 779)

    def test_stop_after_dispatch_binds_run_id_before_provider_visibility(self):
        sha = "d" * 40
        completed_dispatch = mock.Mock(
            returncode=0,
            stdout="https://github.com/trusted/repo/actions/runs/777",
        )
        with mock.patch.object(ci_verifier, "control_is_live", side_effect=[True, True, True, False]), \
             mock.patch.object(ci_verifier, "find_exact_runs", return_value=[]), \
             mock.patch.object(ci_verifier, "run_info", return_value=None), \
             mock.patch.object(ci_verifier.subprocess, "run", return_value=completed_dispatch):
            with self.assertRaises(ci_verifier.CIControlStopped) as raised:
                ci_verifier.acquire_exact_run(
                    1, "agent/issue-d", sha, observe_seconds=0,
                )
        self.assertEqual(raised.exception.ci_run_id, 777)
        self.assertEqual(raised.exception.observed_run["databaseId"], 777)
        self.assertEqual(raised.exception.observed_run["status"], "dispatched")

    def test_production_identity_rejects_each_missing_identity_field(self):
        sha = "e" * 40
        complete = {
            "databaseId": 502,
            "event": "pull_request",
            "status": "completed",
            "conclusion": "success",
            "headSha": sha,
            "headBranch": "agent/issue-e",
            "workflowName": "tests",
            "workflowDatabaseId": 278094148,
            "path": ".github/workflows/tests.yml",
            "repository": "trusted/repo",
            "headRepository": "trusted/repo",
            "pullRequestNumbers": [207],
        }
        fields = {
            "repository": "repository_identity_missing",
            "headRepository": "head_repository_identity_missing",
            "workflowName": "workflow_name_identity_missing",
            "workflowDatabaseId": "workflow_id_identity_missing",
            "path": "workflow_path_identity_missing",
            "headSha": "head_identity_missing",
            "headBranch": "branch_identity_missing",
            "pullRequestNumbers": "pr_binding_identity_missing",
        }
        for field, expected_reason in fields.items():
            with self.subTest(field=field), mock.patch.dict(
                os.environ,
                {"AGENT_REPO": "trusted/repo", "AGENT_CI_FIXTURE_MODE": "false"},
                clear=False,
            ):
                run = dict(complete)
                run.pop(field)
                self.assertEqual(
                    ci_verifier._validate_run_identity(
                        run, sha, "agent/issue-e", 207,
                    ),
                    expected_reason,
                )

    def test_production_exact_head_verification_rejects_missing_identity(self):
        sha = "h" * 40
        required = ci_verifier.load_requirements()["required_jobs"]
        run = {
            "databaseId": 505,
            "workflowName": "tests",
            "status": "completed",
            "conclusion": "success",
            "headSha": sha,
            "headBranch": "agent/issue-h",
            "jobs": [
                {"name": name, "status": "completed", "conclusion": "success"}
                for name in required
            ],
        }
        with mock.patch.dict(
            os.environ,
            {"AGENT_REPO": "trusted/repo", "AGENT_CI_FIXTURE_MODE": "false"},
            clear=False,
        ), mock.patch.object(ci_verifier, "run_info", return_value=run):
            with self.assertRaisesRegex(
                ci_verifier.CIVerificationError, "repository_identity_missing"
            ):
                ci_verifier.verify_exact_head_ci(
                    207, sha, 505,
                    {"headRefOid": sha, "headRefName": "agent/issue-h"},
                )

    def test_terminal_processing_refresh_rejects_head_changed_after_poll_validator(self):
        sha = "f" * 40
        run = {
            "databaseId": 503, "event": "workflow_dispatch", "status": "queued",
            "conclusion": "", "headSha": sha, "headBranch": "agent/issue-f",
            "workflowName": "tests", "workflowDatabaseId": 278094148,
            "path": ".github/workflows/tests.yml", "repository": "trusted/repo",
            "headRepository": "trusted/repo",
        }
        initial_pr = {
            "number": 207, "state": "OPEN", "headRefOid": sha,
            "headRefName": "agent/issue-f", "body": "Closes #42",
        }
        moved_pr = dict(initial_pr, headRefOid="0" * 40)

        def complete_after_validator(*args, **kwargs):
            self.assertIsNone(kwargs["validator"]())
            return {
                "status": "success", "conclusion": "success", "ci_run_id": 503,
                "head_sha": sha, "branch": "agent/issue-f", "run": dict(
                    run, status="completed", conclusion="success",
                ),
            }

        with mock.patch.dict(os.environ, {"AGENT_REPO": "trusted/repo"}, clear=False), \
             mock.patch.object(ci_handler.ci_verifier, "run_info", return_value=run), \
             mock.patch.object(ci_handler.sm, "get_pr_info", side_effect=[initial_pr, initial_pr, moved_pr]), \
             mock.patch.object(ci_handler, "_find_issue_for_pr", return_value=42), \
             mock.patch.object(ci_handler.sm, "verify_issue_pr_binding", return_value=(True, "ok")), \
             mock.patch.object(ci_handler.ci_verifier, "wait_for_run_completion", side_effect=complete_after_validator), \
             mock.patch.object(ci_handler.sm, "record_ci_terminal_state", return_value=True), \
             mock.patch.object(ci_handler.ci_verifier, "verify_exact_head_ci") as verify:
            result = ci_handler.process_ci_dispatch(42, 207, sha, 503)
        self.assertEqual(result["action"], "stale")
        self.assertIn("pr_head_moved", result["reason"])
        verify.assert_not_called()

    def test_control_stop_is_recorded_with_observed_run_and_terminal_binding(self):
        sha = "g" * 40
        run = {
            "databaseId": 504, "event": "workflow_dispatch", "status": "in_progress",
            "conclusion": "", "headSha": sha, "headBranch": "agent/issue-g",
            "workflowName": "tests", "workflowDatabaseId": 278094148,
            "path": ".github/workflows/tests.yml", "repository": "trusted/repo",
            "headRepository": "trusted/repo",
        }
        pr = {"number": 207, "state": "OPEN", "headRefOid": sha,
              "headRefName": "agent/issue-g", "body": "Closes #42"}
        with mock.patch.dict(os.environ, {"AGENT_REPO": "trusted/repo"}, clear=False), \
             mock.patch.object(ci_handler.ci_verifier, "run_info", return_value=run), \
             mock.patch.object(ci_handler.sm, "get_pr_info", side_effect=[pr, pr]), \
             mock.patch.object(ci_handler, "_find_issue_for_pr", return_value=42), \
             mock.patch.object(ci_handler.sm, "verify_issue_pr_binding", return_value=(True, "ok")), \
             mock.patch.object(ci_handler.ci_verifier, "wait_for_run_completion", return_value={
                 "status": "ci_control_stopped", "reason": "control_emergency_stop_activated",
                 "ci_run_id": 504, "head_sha": sha,
             }), \
             mock.patch.object(ci_handler.sm, "record_ci_terminal_state", return_value=True) as record:
            result = ci_handler.process_ci_dispatch(42, 207, sha, 504)
        self.assertEqual(result["action"], "blocked")
        self.assertEqual(result["terminal_status"], "terminal_ci_control_stopped")
        record.assert_called_once()
        args = record.call_args.args
        self.assertEqual(args[:6], (42, 207, sha, 504, "terminal_ci_control_stopped", "in_progress"))
        self.assertEqual(args[6], "control_emergency_stop_activated")

    def test_stale_run_identity_cannot_terminalize_before_binding(self):
        sha = "h" * 40
        pr = {
            "number": 207, "state": "OPEN", "headRefOid": sha,
            "headRefName": "agent/issue-h", "body": "Closes #42",
        }
        with mock.patch.dict(os.environ, {"AGENT_REPO": "trusted/repo"}, clear=False), \
             mock.patch.object(ci_handler.ci_verifier, "run_info", return_value={
                 "status": "completed", "conclusion": "success",
             }), \
             mock.patch.object(ci_handler.sm, "get_pr_info", return_value=pr), \
             mock.patch.object(ci_handler, "_find_issue_for_pr", return_value=42), \
             mock.patch.object(ci_handler.sm, "verify_issue_pr_binding", return_value=(False, "issue_pr_mismatch")), \
             mock.patch.object(ci_handler.sm, "record_ci_terminal_state", return_value=True) as record:
            result = ci_handler.process_ci_dispatch(42, 207, sha, 505)
        self.assertEqual(result["action"], "noop")
        self.assertIn("binding_rejected", result["reason"])
        record.assert_not_called()

    def test_refresh_terminal_binding_reads_authority_before_stop_check(self):
        sha = "i" * 40
        pr = {
            "number": 207, "state": "OPEN", "headRefOid": sha,
            "headRefName": "agent/issue-i", "body": "Closes #42",
        }
        with mock.patch.object(ci_handler.sm, "get_pr_info", return_value=pr) as get_pr, \
             mock.patch.object(ci_handler.sm, "verify_issue_pr_binding", return_value=(True, "ok")) as verify, \
             mock.patch.object(ci_handler.ci_verifier, "control_is_live", return_value=False):
            refreshed, reason = ci_handler._refresh_terminal_binding(
                42, 207, sha, "agent/issue-i",
            )
        self.assertEqual(refreshed, pr)
        self.assertEqual(reason, "ci_control_stopped:control_emergency_stop_activated")
        get_pr.assert_called_once_with(207)
        verify.assert_called_once_with(42, 207, sha)

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
        run = {"databaseId": 11, "event": "pull_request", "status": "completed", "conclusion": "success", "headSha": "c" * 40, "headBranch": "agent/issue-7", "workflowName": "tests"}
        with mock.patch.object(ci_verifier, "find_exact_runs", return_value=[run]), \
             mock.patch.object(ci_verifier.subprocess, "run") as dispatch:
            result = ci_verifier.acquire_exact_ci(7, "agent/issue-7", "c" * 40, observe_seconds=1)
        dispatch.assert_not_called()
        self.assertEqual(result["workflow_run_id"], 11)
        self.assertEqual(result["source"], "pull_request")

    def test_missing_exact_head_run_dispatches_once_then_binds_one(self):
        run = {"databaseId": 12, "event": "workflow_dispatch", "status": "completed", "conclusion": "success", "headSha": "d" * 40, "headBranch": "agent/issue-8", "workflowName": "tests"}
        with mock.patch.object(ci_verifier, "find_exact_runs", side_effect=[[], [run]]), \
             mock.patch.object(ci_verifier.subprocess, "run", return_value=mock.Mock(returncode=0)) as dispatch:
            result = ci_verifier.acquire_exact_ci(8, "agent/issue-8", "d" * 40, observe_seconds=0)
        dispatch.assert_called_once()
        self.assertEqual(result["workflow_run_id"], 12)
        self.assertEqual(result["source"], "workflow_dispatch")

    def test_two_exact_head_runs_select_one_and_mark_duplicate(self):
        runs = [
            {"databaseId": 21, "event": "pull_request", "status": "completed", "conclusion": "success", "headSha": "e" * 40, "headBranch": "agent/issue-9", "workflowName": "tests"},
            {"databaseId": 22, "event": "workflow_dispatch", "status": "completed", "conclusion": "success", "headSha": "e" * 40, "headBranch": "agent/issue-9", "workflowName": "tests"},
        ]
        with mock.patch.object(ci_verifier, "find_exact_runs", return_value=runs):
            result = ci_verifier.acquire_exact_ci(9, "agent/issue-9", "e" * 40, observe_seconds=1)
        self.assertEqual(result["workflow_run_id"], 21)
        self.assertEqual(result["duplicate_run_ids"], [22])

    def test_acquire_run_natural_queued_binds_without_fallback(self):
        sha = "z" * 40
        run = {"databaseId": 101, "event": "pull_request", "status": "queued", "conclusion": "",
               "headSha": sha, "headBranch": "agent/issue-x", "workflowName": "tests",
               "updatedAt": "2026-07-14T00:00:00Z"}
        with mock.patch.object(ci_verifier, "find_exact_runs", return_value=[run]), \
             mock.patch.object(ci_verifier.subprocess, "run") as dispatch:
            result = ci_verifier.acquire_exact_run(1, "agent/issue-x", sha, observe_seconds=0)
        dispatch.assert_not_called()
        self.assertEqual(result["workflow_run_id"], 101)
        self.assertEqual(result["bound_status"], "queued")
        self.assertEqual(result["selection_reason"], "natural_active_observed")
        self.assertFalse(result["fallback_dispatched"])

    def test_acquire_run_natural_in_progress_binds_without_fallback(self):
        sha = "y" * 40
        run = {"databaseId": 102, "event": "pull_request", "status": "in_progress", "conclusion": "",
               "headSha": sha, "headBranch": "agent/issue-y", "workflowName": "tests",
               "updatedAt": "2026-07-14T00:00:00Z"}
        with mock.patch.object(ci_verifier, "find_exact_runs", return_value=[run]), \
             mock.patch.object(ci_verifier.subprocess, "run") as dispatch:
            result = ci_verifier.acquire_exact_run(1, "agent/issue-y", sha, observe_seconds=0)
        dispatch.assert_not_called()
        self.assertEqual(result["workflow_run_id"], 102)
        self.assertEqual(result["bound_status"], "in_progress")
        self.assertEqual(result["selection_reason"], "natural_active_observed")

    def test_acquire_run_completed_binds_without_dispatch(self):
        sha = "w" * 40
        run = {"databaseId": 103, "event": "pull_request", "status": "completed", "conclusion": "success",
               "headSha": sha, "headBranch": "agent/issue-w", "workflowName": "tests",
               "updatedAt": "2026-07-14T00:00:00Z"}
        with mock.patch.object(ci_verifier, "find_exact_runs", return_value=[run]), \
             mock.patch.object(ci_verifier.subprocess, "run") as dispatch:
            result = ci_verifier.acquire_exact_run(1, "agent/issue-w", sha, observe_seconds=0)
        dispatch.assert_not_called()
        self.assertEqual(result["workflow_run_id"], 103)
        self.assertEqual(result["bound_status"], "completed")
        self.assertEqual(result["selection_reason"], "natural_completed_observed")

    def test_acquire_run_no_runs_dispatches_exactly_one_fallback(self):
        sha = "v" * 40
        fallback = {"databaseId": 104, "event": "workflow_dispatch", "status": "completed", "conclusion": "success",
                    "headSha": sha, "headBranch": "agent/issue-v", "workflowName": "tests",
                    "updatedAt": "2026-07-14T00:00:00Z"}
        with mock.patch.object(ci_verifier, "find_exact_runs", side_effect=[[], [fallback]]), \
             mock.patch.object(ci_verifier.subprocess, "run", return_value=mock.Mock(returncode=0)) as dispatch:
            result = ci_verifier.acquire_exact_run(1, "agent/issue-v", sha, observe_seconds=0)
        dispatch.assert_called_once()
        self.assertEqual(result["workflow_run_id"], 104)
        self.assertEqual(result["source"], "workflow_dispatch")
        self.assertTrue(result["fallback_dispatched"])

    def test_acquire_run_fallback_never_observable_raises(self):
        sha = "u" * 40
        with mock.patch.object(ci_verifier, "find_exact_runs", return_value=[]), \
             mock.patch.object(ci_verifier.subprocess, "run", return_value=mock.Mock(returncode=0)) as dispatch:
            with self.assertRaises(ci_verifier.CIRunObservationTimeout):
                ci_verifier.acquire_exact_run(1, "agent/issue-u", sha, observe_seconds=0, dispatch_timeout_seconds=0)
        dispatch.assert_called_once()

    def test_acquire_run_superseded_candidates(self):
        sha = "t" * 40
        older = {"databaseId": 105, "event": "pull_request", "status": "completed", "conclusion": "failure",
                 "headSha": sha, "headBranch": "agent/issue-t", "workflowName": "tests",
                 "updatedAt": "2026-07-14T00:01:00Z"}
        newer = {"databaseId": 106, "event": "pull_request", "status": "completed", "conclusion": "success",
                 "headSha": sha, "headBranch": "agent/issue-t", "workflowName": "tests",
                 "updatedAt": "2026-07-14T00:02:00Z"}
        with mock.patch.object(ci_verifier, "find_exact_runs", return_value=[older, newer]):
            result = ci_verifier.acquire_exact_run(1, "agent/issue-t", sha, observe_seconds=0)
        self.assertEqual(result["workflow_run_id"], 106)
        self.assertEqual(result["superseded_run_ids"], [105])

    def test_wait_for_run_queued_to_success(self):
        sha = "s" * 40
        run = {"databaseId": 110, "status": "completed", "conclusion": "success",
               "headSha": sha, "headBranch": "agent/issue-s", "workflowName": "tests",
               "updatedAt": "2026-07-14T00:01:00Z"}
        with mock.patch.object(ci_verifier, "control_is_live", return_value=True), \
             mock.patch.object(ci_verifier, "run_info", return_value=run), \
             mock.patch.object(ci_verifier, "find_exact_runs", return_value=[run]):
            result = ci_verifier.wait_for_run_completion(
                110, expected_head=sha, expected_branch="agent/issue-s",
                completion_timeout_seconds=30, poll_seconds=60,
            )
        self.assertEqual(result["status"], "success")
        self.assertEqual(result["ci_run_id"], 110)

    def test_wait_for_run_queued_to_failure(self):
        sha = "r" * 40
        run = {"databaseId": 111, "status": "completed", "conclusion": "failure",
               "headSha": sha, "headBranch": "agent/issue-r", "workflowName": "tests",
               "updatedAt": "2026-07-14T00:01:00Z"}
        with mock.patch.object(ci_verifier, "control_is_live", return_value=True), \
             mock.patch.object(ci_verifier, "run_info", return_value=run), \
             mock.patch.object(ci_verifier, "find_exact_runs", return_value=[run]):
            result = ci_verifier.wait_for_run_completion(
                111, expected_head=sha, expected_branch="agent/issue-r",
                completion_timeout_seconds=30, poll_seconds=60,
            )
        self.assertEqual(result["status"], "failure")
        self.assertEqual(result["ci_run_id"], 111)

    def test_wait_for_run_control_stopped(self):
        sha = "q" * 40
        run = {"databaseId": 112, "status": "queued", "conclusion": "",
               "headSha": sha, "headBranch": "agent/issue-q", "workflowName": "tests",
               "updatedAt": "2026-07-14T00:00:00Z"}
        with mock.patch.object(ci_verifier, "control_is_live", return_value=False), \
             mock.patch.object(ci_verifier, "run_info", return_value=run):
            result = ci_verifier.wait_for_run_completion(
                112, expected_head=sha, expected_branch="agent/issue-q",
                completion_timeout_seconds=30, poll_seconds=60,
            )
        self.assertEqual(result["status"], "ci_control_stopped")
        self.assertEqual(result["observed_status"], "queued")

    def test_exact_head_verification_rejects_run_id_mismatch_with_typed_reason(self):
        sha = "i" * 40
        run = {
            "databaseId": 507, "workflowName": "tests", "status": "completed",
            "conclusion": "success", "headSha": sha,
            "jobs": [{"name": name, "status": "completed", "conclusion": "success"}
                      for name in ci_verifier.load_requirements()["required_jobs"]],
        }
        with mock.patch.object(ci_verifier, "run_info", return_value=run):
            with self.assertRaisesRegex(ci_verifier.CIVerificationError, "run_id_identity_mismatch"):
                ci_verifier.verify_exact_head_ci(207, sha, 508, {"headRefOid": sha})

    def test_exact_head_verification_reports_typed_missing_workflow_and_head_identity(self):
        required = ci_verifier.load_requirements()["required_jobs"]
        for field, expected in (("workflowName", "workflow_name_identity_missing"),
                                ("headSha", "head_identity_missing")):
            with self.subTest(field=field):
                sha = "j" * 40
                run = {
                    "databaseId": 509, "workflowName": "tests", "status": "completed",
                    "conclusion": "success", "headSha": sha,
                    "headBranch": "agent/issue-j",
                    "jobs": [{"name": name, "status": "completed", "conclusion": "success"}
                              for name in required],
                }
                run.pop(field)
                with mock.patch.dict(os.environ, {"AGENT_REPO": "trusted/repo", "AGENT_CI_FIXTURE_MODE": "false"}, clear=False), \
                     mock.patch.object(ci_verifier, "run_info", return_value=run):
                    with self.assertRaisesRegex(ci_verifier.CIVerificationError, expected):
                        ci_verifier.verify_exact_head_ci(
                            207, sha, 509,
                            {"headRefOid": sha, "headRefName": "agent/issue-j"},
                        )

    def test_exact_head_verification_rejects_missing_pr_branch_identity(self):
        sha = "k" * 40
        run = {
            "databaseId": 510, "workflowName": "tests", "status": "completed",
            "conclusion": "success", "headSha": sha, "headBranch": "agent/issue-k",
            "repository": "trusted/repo", "headRepository": "trusted/repo",
            "workflowDatabaseId": 278094148, "path": ".github/workflows/tests.yml",
            "pullRequestNumbers": [207],
            "jobs": [{"name": name, "status": "completed", "conclusion": "success"}
                      for name in ci_verifier.load_requirements()["required_jobs"]],
        }
        with mock.patch.dict(os.environ, {"AGENT_REPO": "trusted/repo", "AGENT_CI_FIXTURE_MODE": "false"}, clear=False), \
             mock.patch.object(ci_verifier, "run_info", return_value=run):
            with self.assertRaisesRegex(ci_verifier.CIVerificationError, "pr_branch_identity_missing"):
                ci_verifier.verify_exact_head_ci(207, sha, 510, {"headRefOid": sha})

    def test_wait_for_run_head_moved(self):
        sha = "p" * 40
        moved = {"databaseId": 113, "status": "in_progress", "conclusion": "",
                 "headSha": "x" * 40, "headBranch": "agent/issue-p", "workflowName": "tests",
                 "updatedAt": "2026-07-14T00:00:00Z"}
        with mock.patch.object(ci_verifier, "control_is_live", return_value=True), \
             mock.patch.object(ci_verifier, "run_info", return_value=moved):
            result = ci_verifier.wait_for_run_completion(
                113, expected_head=sha, expected_branch="agent/issue-p",
                completion_timeout_seconds=30, poll_seconds=60,
            )
        self.assertEqual(result["status"], "ci_stale_binding")
        self.assertEqual(result["reason"], "head_moved")

    def test_wait_for_run_completion_timeout(self):
        sha = "n" * 40
        run = {"databaseId": 114, "status": "queued", "conclusion": "",
               "headSha": sha, "headBranch": "agent/issue-n", "workflowName": "tests",
               "updatedAt": "2026-07-14T00:00:00Z"}
        with mock.patch.object(ci_verifier, "control_is_live", return_value=True), \
             mock.patch.object(ci_verifier, "run_info", return_value=run), \
             mock.patch.object(ci_verifier, "find_exact_runs", return_value=[run]):
            result = ci_verifier.wait_for_run_completion(
                114, expected_head=sha, expected_branch="agent/issue-n",
                completion_timeout_seconds=0, poll_seconds=60,
            )
        self.assertEqual(result["status"], "ci_completion_timeout")

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
            "databaseId": 42, "event": "workflow_dispatch", "status": "completed", "conclusion": "success",
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
                 mock.patch.object(ci_handler.sm, "read_ci_acquisition", return_value=None), \
                 mock.patch.object(ci_handler.sm, "record_ci_acquisition", return_value=True), \
                 mock.patch.object(ci_handler.sm, "record_ci_terminal_state", return_value=True), \
                 mock.patch.object(ci_handler, "_record_ci") as record:
                result = ci_handler.process_ci_completion(event_path.name)
            with mock.patch.dict(os.environ, {"AGENT_REPO": "trusted/repo"}, clear=False), \
                 mock.patch.object(ci_handler.ci_verifier, "run_info", return_value=run), \
                 mock.patch.object(ci_handler.sm, "get_pr_info", return_value=pr), \
                 mock.patch.object(ci_handler, "_find_issue_for_pr", return_value=42), \
                 mock.patch.object(ci_handler, "_is_duplicate_exact_head_run", return_value=False), \
                 mock.patch.object(ci_handler.sm, "verify_issue_pr_binding", return_value=(True, "ok")), \
                 mock.patch.object(ci_handler.sm, "read_ci_acquisition", return_value=None), \
                 mock.patch.object(ci_handler.sm, "record_ci_acquisition", return_value=True), \
                 mock.patch.object(ci_handler, "_record_ci", side_effect=RuntimeError("write failed")):
                unavailable = ci_handler.process_ci_completion(event_path.name)
        finally:
            os.unlink(event_path.name)
        self.assertEqual(result["action"], "blocked")
        self.assertEqual(result["reason"], "ci_terminal_cancelled")
        record.assert_called_once()
        self.assertEqual(unavailable["action"], "blocked")
        self.assertEqual(unavailable["terminal_status"], "terminal_ci_state_unavailable")
        self.assertIn("ci_state_unavailable", unavailable["reason"])

    def test_canonical_selection_prefers_newer_completed_result_over_run_id(self):
        sha = "a" * 40
        runs = [
            {"databaseId": 900, "event": "pull_request", "status": "completed", "conclusion": "failure",
             "headSha": sha, "headBranch": "agent/x", "workflowName": "tests",
             "createdAt": "2026-07-14T00:00:00Z", "updatedAt": "2026-07-14T00:01:00Z"},
            {"databaseId": 901, "event": "pull_request", "status": "completed", "conclusion": "success",
             "headSha": sha, "headBranch": "agent/x", "workflowName": "tests",
             "createdAt": "2026-07-14T00:02:00Z", "updatedAt": "2026-07-14T00:03:00Z"},
        ]
        with mock.patch.object(ci_verifier, "find_exact_runs", return_value=runs):
            result = ci_verifier.acquire_exact_ci(1, "agent/x", sha, observe_seconds=0)
        self.assertEqual(result["workflow_run_id"], 901)
        self.assertEqual(result["selection_reason"], "natural_completed_observed")
        self.assertEqual(result["superseded_run_ids"], [900])

    def test_newer_failure_supersedes_older_success(self):
        sha = "b" * 40
        runs = [
            {"databaseId": 910, "event": "pull_request", "status": "completed", "conclusion": "success",
             "headSha": sha, "headBranch": "agent/x", "workflowName": "tests",
             "updatedAt": "2026-07-14T00:01:00Z"},
            {"databaseId": 911, "event": "pull_request", "status": "completed", "conclusion": "failure",
             "headSha": sha, "headBranch": "agent/x", "workflowName": "tests",
             "updatedAt": "2026-07-14T00:02:00Z"},
        ]
        with mock.patch.object(ci_verifier, "find_exact_runs", return_value=runs):
            result = ci_verifier.acquire_exact_ci(1, "agent/x", sha, observe_seconds=0)
        self.assertEqual(result["workflow_run_id"], 911)

    def test_completed_fallback_success_supersedes_pending_natural_run(self):
        sha = "c" * 40
        runs = [
            {"databaseId": 920, "event": "pull_request", "status": "in_progress", "conclusion": "",
             "headSha": sha, "headBranch": "agent/x", "workflowName": "tests",
             "updatedAt": "2026-07-14T00:03:00Z"},
            {"databaseId": 921, "event": "workflow_dispatch", "status": "completed", "conclusion": "success",
             "headSha": sha, "headBranch": "agent/x", "workflowName": "tests",
             "updatedAt": "2026-07-14T00:02:00Z"},
        ]
        with mock.patch.object(ci_verifier, "find_exact_runs", return_value=runs), \
             mock.patch.object(ci_verifier.subprocess, "run") as dispatch:
            result = ci_verifier.acquire_exact_ci(1, "agent/x", sha, observe_seconds=0)
        dispatch.assert_not_called()
        self.assertEqual(result["workflow_run_id"], 921)
        self.assertEqual(result["source"], "workflow_dispatch")

    def test_natural_pending_completes_through_combined_function(self):
        sha = "d" * 40
        required = ci_verifier.load_requirements()["required_jobs"]
        natural = {"databaseId": 930, "event": "pull_request", "status": "queued", "conclusion": "",
                   "headSha": sha, "headBranch": "agent/x", "workflowName": "tests",
                   "updatedAt": "2026-07-14T00:00:00Z"}
        completed = {"databaseId": 930, "event": "pull_request", "status": "completed", "conclusion": "success",
                     "headSha": sha, "headBranch": "agent/x", "workflowName": "tests",
                     "updatedAt": "2026-07-14T00:01:00Z",
                     "jobs": [{"name": name, "status": "completed", "conclusion": "success"} for name in required]}
        with mock.patch.object(ci_verifier, "find_exact_runs", side_effect=[[natural], [completed]]), \
             mock.patch.object(ci_verifier, "run_info", return_value=completed), \
             mock.patch.object(ci_verifier, "control_is_live", return_value=True), \
             mock.patch.object(ci_verifier.subprocess, "run", return_value=mock.Mock(returncode=0)) as dispatch:
            result = ci_verifier.acquire_exact_ci(1, "agent/x", sha, observe_seconds=0)
        dispatch.assert_not_called()
        self.assertEqual(result["workflow_run_id"], 930)
        self.assertEqual(result["status"], "completed")
        self.assertEqual(result["source"], "pull_request")

    def test_selection_records_unsupported_and_observed_candidates(self):
        sha = "e" * 40
        unsupported = {"databaseId": 940, "event": "pull_request", "status": "completed", "conclusion": "cancelled",
                       "headSha": sha, "headBranch": "agent/x", "workflowName": "tests", "updatedAt": "2026-07-14T00:00:00Z"}
        with mock.patch.object(ci_verifier, "find_exact_runs", side_effect=[[unsupported], [unsupported]]), \
             mock.patch.object(ci_verifier.subprocess, "run", return_value=mock.Mock(returncode=0)) as dispatch:
            with self.assertRaises(ci_verifier.CIVerificationError):
                ci_verifier.acquire_exact_ci(1, "agent/x", sha, observe_seconds=0, dispatch_timeout_seconds=0)
        dispatch.assert_called_once()

    def test_completion_reselects_after_unsupported_run_before_blocking(self):
        event_run = {"databaseId": 940, "event": "pull_request"}
        replacement = {
            "workflow_run_id": 941,
            "source": "workflow_dispatch",
            "status": "bound",
            "selection_reason": "newest_completed_supported",
            "observed_run_ids": [940, 941],
            "superseded_run_ids": [940],
            "unsupported_run_ids": [940],
            "fallback_dispatched": True,
            "duplicate_run_ids": [940],
        }
        with mock.patch.object(ci_handler.ci_verifier, "acquire_exact_ci", return_value=replacement), \
             mock.patch.object(ci_handler.sm, "read_ci_acquisition", return_value=None), \
             mock.patch.object(ci_handler.sm, "get_pr_info", return_value={"state": "OPEN", "headRefOid": "a" * 40, "headRefName": "agent/x"}), \
             mock.patch.object(ci_handler.sm, "verify_issue_pr_binding", return_value=(True, "ok")), \
             mock.patch.object(ci_handler.ci_verifier, "control_is_live", return_value=True), \
             mock.patch.object(ci_handler.sm, "record_ci_acquisition", return_value=True) as record:
            selected = ci_handler._reselect_unsupported(42, 207, "a" * 40, "agent/x", event_run)
        self.assertEqual(selected, replacement)
        record.assert_called_once()
        self.assertTrue(record.call_args.kwargs["metadata"]["fallback_dispatched"])

    def test_timed_out_natural_run_is_reselected_when_later_natural_success_exists(self):
        sha = "2" * 40
        runs = [
            {"databaseId": 970, "event": "pull_request", "status": "completed", "conclusion": "timed_out",
             "headSha": sha, "headBranch": "agent/x", "workflowName": "tests",
             "updatedAt": "2026-07-14T00:01:00Z"},
            {"databaseId": 971, "event": "pull_request", "status": "completed", "conclusion": "success",
             "headSha": sha, "headBranch": "agent/x", "workflowName": "tests",
             "updatedAt": "2026-07-14T00:02:00Z"},
        ]
        with mock.patch.object(ci_verifier, "find_exact_runs", return_value=runs), \
             mock.patch.object(ci_verifier.subprocess, "run") as dispatch:
            result = ci_verifier.acquire_exact_ci(1, "agent/x", sha, observe_seconds=0)
        dispatch.assert_not_called()
        self.assertEqual(result["workflow_run_id"], 971)
        self.assertEqual(result["unsupported_run_ids"], [970])

    def test_equal_completed_natural_and_fallback_prefers_natural(self):
        sha = "3" * 40
        runs = [
            {"databaseId": 980, "event": "pull_request", "status": "completed", "conclusion": "success",
             "headSha": sha, "headBranch": "agent/x", "workflowName": "tests",
             "updatedAt": "2026-07-14T00:04:00Z"},
            {"databaseId": 981, "event": "workflow_dispatch", "status": "completed", "conclusion": "success",
             "headSha": sha, "headBranch": "agent/x", "workflowName": "tests",
             "updatedAt": "2026-07-14T00:04:00Z"},
        ]
        with mock.patch.object(ci_verifier, "find_exact_runs", return_value=runs):
            result = ci_verifier.acquire_exact_ci(1, "agent/x", sha, observe_seconds=0)
        self.assertEqual(result["workflow_run_id"], 980)
        self.assertEqual(result["source"], "pull_request")

    def test_two_completed_failures_select_newest_and_preserve_repair_metadata(self):
        sha = "4" * 40
        runs = [
            {"databaseId": 990, "event": "pull_request", "status": "completed", "conclusion": "failure",
             "headSha": sha, "headBranch": "agent/x", "workflowName": "tests",
             "updatedAt": "2026-07-14T00:05:00Z"},
            {"databaseId": 991, "event": "workflow_dispatch", "status": "completed", "conclusion": "failure",
             "headSha": sha, "headBranch": "agent/x", "workflowName": "tests",
             "updatedAt": "2026-07-14T00:06:00Z"},
        ]
        with mock.patch.object(ci_verifier, "find_exact_runs", return_value=runs):
            result = ci_verifier.acquire_exact_ci(1, "agent/x", sha, observe_seconds=0)
        self.assertEqual(result["workflow_run_id"], 991)
        self.assertEqual(result["superseded_run_ids"], [990])
        self.assertEqual(result["observed_run_ids"], [990, 991])

    def test_state_store_outage_during_reselection_fails_closed(self):
        with mock.patch.object(ci_handler.sm, "read_ci_acquisition", side_effect=ci_handler.sm.StateUnavailableError("outage")):
            with self.assertRaises(ci_handler.sm.StateUnavailableError):
                ci_handler._is_duplicate_exact_head_run(42, 207, "5" * 40, 1000, "agent/x")

    def test_candidate_identity_rejects_wrong_branch_workflow_or_head_repository(self):
        sha = "f" * 40
        candidates = [
            {"databaseId": 950, "event": "pull_request", "status": "completed", "conclusion": "success",
             "headSha": sha, "headBranch": "other", "workflowName": "tests", "headRepository": "trusted/repo"},
            {"databaseId": 951, "event": "pull_request", "status": "completed", "conclusion": "success",
             "headSha": sha, "headBranch": "agent/x", "workflowName": "other", "headRepository": "trusted/repo"},
            {"databaseId": 952, "event": "pull_request", "status": "completed", "conclusion": "success",
             "headSha": sha, "headBranch": "agent/x", "workflowName": "tests", "headRepository": "fork/repo"},
        ]
        with mock.patch.dict(
            os.environ,
            {"AGENT_REPO": "trusted/repo", "AGENT_CI_FIXTURE_MODE": "false"},
            clear=False,
        ), \
             mock.patch.object(ci_verifier, "find_exact_runs", return_value=candidates), \
             mock.patch.object(ci_verifier.subprocess, "run", return_value=mock.Mock(returncode=1)) as dispatch:
            with self.assertRaises(ci_verifier.CIVerificationError):
                ci_verifier.acquire_exact_ci(1, "agent/x", sha, observe_seconds=0, dispatch_timeout_seconds=0)
        dispatch.assert_not_called()

    def test_candidate_identity_rejects_wrong_pull_request_when_provider_binds_one(self):
        sha = "7" * 40
        candidate = {
            "databaseId": 955, "event": "pull_request", "status": "completed", "conclusion": "success",
            "headSha": sha, "headBranch": "agent/x", "workflowName": "tests",
            "headRepository": "trusted/repo", "pullRequestNumbers": [208],
        }
        with mock.patch.dict(
            os.environ,
            {"AGENT_REPO": "trusted/repo", "AGENT_CI_FIXTURE_MODE": "false"},
            clear=False,
        ), \
             mock.patch.object(ci_verifier, "find_exact_runs", return_value=[candidate]), \
             mock.patch.object(ci_verifier.subprocess, "run", return_value=mock.Mock(returncode=1)) as dispatch:
            with self.assertRaises(ci_verifier.CIVerificationError):
                ci_verifier.acquire_exact_ci(207, "agent/x", sha, observe_seconds=0, dispatch_timeout_seconds=0)
        dispatch.assert_not_called()

    def test_reselection_keeps_repair_count_and_ignores_stale_failure_after_success(self):
        sha = "1" * 40
        body = 'Closes #42\n\n<!-- agent-orchestrator-binding: {"issue_number": 42, "branch": "agent/x"} -->'
        event_path = tempfile.NamedTemporaryFile(mode="w", delete=False)
        json.dump({
            "repository": {"full_name": "trusted/repo"},
            "workflow_run": {
                "name": "tests", "status": "completed", "conclusion": "failure",
                "head_branch": "agent/x", "head_sha": sha, "id": 960,
                "head_repository": {"full_name": "trusted/repo"},
                "pull_requests": [{"number": 207, "head": {"sha": sha}}],
            },
        }, event_path)
        event_path.close()
        run = {"databaseId": 960, "workflowName": "tests", "status": "completed", "conclusion": "failure",
               "headSha": sha, "headBranch": "agent/x", "jobs": []}
        newer_success = {"databaseId": 961, "event": "pull_request", "status": "completed", "conclusion": "success",
                         "headSha": sha, "headBranch": "agent/x", "workflowName": "tests",
                         "updatedAt": "2026-07-14T00:02:00Z"}
        try:
            with mock.patch.dict(os.environ, {"AGENT_REPO": "trusted/repo"}, clear=False), \
                 mock.patch.object(ci_handler.ci_verifier, "run_info", return_value=run), \
                 mock.patch.object(ci_handler.ci_verifier, "find_exact_runs", return_value=[run, newer_success]), \
                 mock.patch.object(ci_handler.sm, "get_pr_info", return_value={"state": "OPEN", "headRefName": "agent/x", "headRefOid": sha, "body": body}), \
                 mock.patch.object(ci_handler, "_find_issue_for_pr", return_value=42), \
                 mock.patch.object(ci_handler.sm, "verify_issue_pr_binding", return_value=(True, "ok")), \
                 mock.patch.object(ci_handler.sm, "record_ci_terminal_state", return_value=True), \
                 mock.patch.object(ci_handler.sm, "read_ci_acquisition", return_value={"workflow_run_id": 960}), \
                 mock.patch.object(ci_handler.sm, "read_ci_state", return_value={"pr_number": 207, "head_sha": sha, "workflow_run_id": 961, "status": "success", "extra": {"repair_count": 1}}):
                result = ci_handler.process_ci_completion(event_path.name)
        finally:
            os.unlink(event_path.name)
        self.assertEqual(result["action"], "noop")
        self.assertEqual(result["reason"], "ci_noop:duplicate_exact_head_run")

    def test_completion_persists_canonical_supersession_metadata(self):
        sha = "6" * 40
        older = {
            "databaseId": 1000, "event": "pull_request", "status": "completed",
            "conclusion": "failure", "headSha": sha, "headBranch": "agent/x",
            "workflowName": "tests", "updatedAt": "2026-07-14T00:10:00Z",
        }
        newer = {
            "databaseId": 1001, "event": "workflow_dispatch", "status": "completed",
            "conclusion": "success", "headSha": sha, "headBranch": "agent/x",
            "workflowName": "tests", "updatedAt": "2026-07-14T00:11:00Z",
        }
        with mock.patch.object(ci_handler.ci_verifier, "find_exact_runs", return_value=[older, newer]), \
             mock.patch.object(ci_handler.sm, "read_ci_acquisition", return_value=None), \
             mock.patch.object(ci_handler.sm, "record_ci_acquisition", return_value=True) as record:
            self.assertTrue(
                ci_handler._persist_canonical_acquisition(42, 207, sha, "agent/x", newer)
            )
        record.assert_called_once()
        args = record.call_args.args
        self.assertEqual(args[:6], (42, 207, sha, 1001, "workflow_dispatch", [1000]))
        metadata = record.call_args.kwargs["metadata"]
        self.assertEqual(metadata["observed_run_ids"], [1000, 1001])
        self.assertEqual(metadata["superseded_run_ids"], [1000])
        self.assertEqual(metadata["unsupported_run_ids"], [])
        self.assertEqual(metadata["status"], "bound")

    def test_wait_for_run_queued_in_progress_success_sequence(self):
        sha = "t" * 40
        required = ci_verifier.load_requirements()["required_jobs"]
        transitions = [
            {"databaseId": 120, "status": "queued", "conclusion": "",
             "headSha": sha, "headBranch": "agent/issue-t", "workflowName": "tests",
             "updatedAt": "2026-07-14T00:00:00Z"},
            {"databaseId": 120, "status": "in_progress", "conclusion": "",
             "headSha": sha, "headBranch": "agent/issue-t", "workflowName": "tests",
             "updatedAt": "2026-07-14T00:00:30Z"},
            {"databaseId": 120, "status": "completed", "conclusion": "success",
             "headSha": sha, "headBranch": "agent/issue-t", "workflowName": "tests",
             "updatedAt": "2026-07-14T00:01:00Z",
             "jobs": [{"name": name, "status": "completed", "conclusion": "success"} for name in required]},
        ]
        with mock.patch.object(ci_verifier, "control_is_live", return_value=True), \
             mock.patch.object(ci_verifier, "run_info", side_effect=transitions), \
             mock.patch.object(ci_verifier, "find_exact_runs", return_value=[transitions[-1]]):
            result = ci_verifier.wait_for_run_completion(
                120, expected_head=sha, expected_branch="agent/issue-t",
                completion_timeout_seconds=30, poll_seconds=60,
                sleep=lambda _: None,
            )
        self.assertEqual(result["status"], "success")
        self.assertEqual(result["ci_run_id"], 120)
        self.assertEqual(result["conclusion"], "success")

    def test_wait_for_run_queued_in_progress_failure_sequence(self):
        sha = "u" * 40
        transitions = [
            {"databaseId": 121, "status": "queued", "conclusion": "",
             "headSha": sha, "headBranch": "agent/issue-u", "workflowName": "tests",
             "updatedAt": "2026-07-14T00:00:00Z"},
            {"databaseId": 121, "status": "in_progress", "conclusion": "",
             "headSha": sha, "headBranch": "agent/issue-u", "workflowName": "tests",
             "updatedAt": "2026-07-14T00:00:30Z"},
            {"databaseId": 121, "status": "completed", "conclusion": "failure",
             "headSha": sha, "headBranch": "agent/issue-u", "workflowName": "tests",
             "updatedAt": "2026-07-14T00:01:00Z"},
        ]
        with mock.patch.object(ci_verifier, "control_is_live", return_value=True), \
             mock.patch.object(ci_verifier, "run_info", side_effect=transitions), \
             mock.patch.object(ci_verifier, "find_exact_runs", return_value=[transitions[-1]]):
            result = ci_verifier.wait_for_run_completion(
                121, expected_head=sha, expected_branch="agent/issue-u",
                completion_timeout_seconds=30, poll_seconds=60,
                sleep=lambda _: None,
            )
        self.assertEqual(result["status"], "failure")
        self.assertEqual(result["ci_run_id"], 121)
        self.assertEqual(result["conclusion"], "failure")

    def test_wait_for_run_with_validator_pr_closed(self):
        sha = "v" * 40
        run = {"databaseId": 122, "status": "in_progress", "conclusion": "",
               "headSha": sha, "headBranch": "agent/issue-v", "workflowName": "tests",
               "updatedAt": "2026-07-14T00:00:00Z"}
        def validator():
            return "pr_closed"
        with mock.patch.object(ci_verifier, "control_is_live", return_value=True), \
             mock.patch.object(ci_verifier, "run_info", return_value=run):
            result = ci_verifier.wait_for_run_completion(
                122, expected_head=sha, expected_branch="agent/issue-v",
                completion_timeout_seconds=30, poll_seconds=60,
                validator=validator, sleep=lambda _: None,
            )
        self.assertEqual(result["status"], "ci_stale_binding")
        self.assertEqual(result["reason"], "pr_closed")

    def test_wait_for_run_with_validator_pr_head_moved(self):
        sha = "w" * 40
        run = {"databaseId": 123, "status": "in_progress", "conclusion": "",
               "headSha": sha, "headBranch": "agent/issue-w", "workflowName": "tests",
               "updatedAt": "2026-07-14T00:00:00Z"}
        def validator():
            return "pr_head_moved"
        with mock.patch.object(ci_verifier, "control_is_live", return_value=True), \
             mock.patch.object(ci_verifier, "run_info", return_value=run):
            result = ci_verifier.wait_for_run_completion(
                123, expected_head=sha, expected_branch="agent/issue-w",
                completion_timeout_seconds=30, poll_seconds=60,
                validator=validator, sleep=lambda _: None,
            )
        self.assertEqual(result["status"], "ci_stale_binding")
        self.assertEqual(result["reason"], "pr_head_moved")

    def test_wait_for_run_with_validator_binding_rejected(self):
        sha = "x" * 40
        run = {"databaseId": 124, "status": "in_progress", "conclusion": "",
               "headSha": sha, "headBranch": "agent/issue-x", "workflowName": "tests",
               "updatedAt": "2026-07-14T00:00:00Z"}
        def validator():
            return "binding_rejected:issue_pr_mismatch"
        with mock.patch.object(ci_verifier, "control_is_live", return_value=True), \
             mock.patch.object(ci_verifier, "run_info", return_value=run):
            result = ci_verifier.wait_for_run_completion(
                124, expected_head=sha, expected_branch="agent/issue-x",
                completion_timeout_seconds=30, poll_seconds=60,
                validator=validator, sleep=lambda _: None,
            )
        self.assertEqual(result["status"], "ci_stale_binding")
        self.assertEqual(result["reason"], "binding_rejected:issue_pr_mismatch")

    @mock.patch.object(ci_handler.ci_verifier, "verify_exact_head_ci", side_effect=ci_verifier.CIVerificationError("mock failure"))
    @mock.patch.object(ci_handler, "_record_ci_terminal", return_value={
        "action": "stale", "reason": "ci_stale_binding:exact_head_ci_rejected",
    })
    @mock.patch.object(ci_handler, "_record_ci")
    @mock.patch.object(ci_handler, "_persist_canonical_acquisition")
    @mock.patch.object(ci_handler, "_is_duplicate_exact_head_run", return_value=False)
    @mock.patch.object(ci_handler.sm, "verify_issue_pr_binding", return_value=(True, "ok"))
    @mock.patch.object(ci_handler, "_find_issue_for_pr", return_value=42)
    @mock.patch.object(ci_handler.sm, "get_pr_info", return_value={"number": 207, "headRefOid": "abc123", "headRefName": "agent/issue-42", "body": "fixes #42", "state": "OPEN"})
    @mock.patch.object(ci_handler.ci_verifier, "run_info", return_value={
        "databaseId": 99999, "status": "completed", "conclusion": "success",
        "headSha": "abc123", "headBranch": "agent/issue-42",
        "workflowName": "tests", "workflowId": 278094148, "workflowDatabaseId": 278094148, "path": ".github/workflows/tests.yml",
        "repository": "test/repo", "headRepository": "test/repo",
    })
    @mock.patch.dict(os.environ, {"AGENT_REPO": "test/repo"})
    def test_process_dispatch_exact_head_ci_fail_propagates(self, *mocks):
        result = ci_handler.process_ci_dispatch(42, 207, "abc123", 99999)
        self.assertEqual(result["action"], "stale")
        self.assertIn("exact_head_ci_rejected", result["reason"])

    def test_typed_verifier_identity_reason_is_preserved_for_terminal_evidence(self):
        self.assertEqual(
            ci_handler._typed_ci_verification_reason(
                ci_verifier.CIVerificationError(
                    "CI run identity rejected: workflow_path_identity_missing"
                )
            ),
            "workflow_path_identity_missing",
        )
        self.assertEqual(
            ci_handler._typed_ci_verification_reason(
                ci_verifier.CIVerificationError("required CI jobs are absent: tests")
            ),
            "exact_head_ci_rejected",
        )

    @mock.patch.object(ci_handler, "_record_ci_terminal", return_value={
        "action": "blocked", "pr_number": 207, "issue_number": 42,
        "head_sha": "abc123", "ci_run_id": 99999, "reason": "ci_completion_timeout",
    })
    @mock.patch.object(ci_handler.ci_verifier, "wait_for_run_completion", return_value={
        "status": "ci_completion_timeout", "reason": "run_still_active",
        "ci_run_id": 99999, "head_sha": "abc123",
    })
    @mock.patch.object(ci_handler, "_persist_canonical_acquisition")
    @mock.patch.object(ci_handler, "_is_duplicate_exact_head_run", return_value=False)
    @mock.patch.object(ci_handler.sm, "verify_issue_pr_binding", return_value=(True, "ok"))
    @mock.patch.object(ci_handler, "_find_issue_for_pr", return_value=42)
    @mock.patch.object(ci_handler.sm, "get_pr_info", return_value={"number": 207, "headRefOid": "abc123", "headRefName": "agent/issue-42", "body": "fixes #42", "state": "OPEN"})
    @mock.patch.object(ci_handler.ci_verifier, "run_info", return_value={
        "databaseId": 99999, "status": "queued", "conclusion": "",
        "headSha": "abc123", "headBranch": "agent/issue-42",
        "workflowName": "tests", "workflowId": 278094148, "workflowDatabaseId": 278094148, "path": ".github/workflows/tests.yml",
        "repository": "test/repo", "headRepository": "test/repo",
    })
    @mock.patch.dict(os.environ, {"AGENT_REPO": "test/repo"})
    def test_process_dispatch_completion_timeout_persists_terminal(self, *mocks):
        result = ci_handler.process_ci_dispatch(42, 207, "abc123", 99999)
        self.assertEqual(result["action"], "blocked")
        self.assertEqual(result["reason"], "ci_completion_timeout")

    @mock.patch.object(ci_handler, "_record_ci_terminal", return_value={
        "action": "blocked", "reason": "control_emergency_stop_activated",
    })
    @mock.patch.object(ci_handler.ci_verifier, "wait_for_run_completion", return_value={
        "status": "ci_control_stopped", "reason": "control_emergency_stop_activated",
        "ci_run_id": 99999, "head_sha": "abc123",
    })
    @mock.patch.object(ci_handler, "_persist_canonical_acquisition")
    @mock.patch.object(ci_handler, "_is_duplicate_exact_head_run", return_value=False)
    @mock.patch.object(ci_handler.sm, "verify_issue_pr_binding", return_value=(True, "ok"))
    @mock.patch.object(ci_handler, "_find_issue_for_pr", return_value=42)
    @mock.patch.object(ci_handler.sm, "get_pr_info", return_value={"number": 207, "headRefOid": "abc123", "headRefName": "agent/issue-42", "body": "fixes #42", "state": "OPEN"})
    @mock.patch.object(ci_handler.ci_verifier, "run_info", return_value={
        "databaseId": 99999, "status": "queued", "conclusion": "",
        "headSha": "abc123", "headBranch": "agent/issue-42",
        "workflowName": "tests", "workflowId": 278094148, "workflowDatabaseId": 278094148, "path": ".github/workflows/tests.yml",
        "repository": "test/repo", "headRepository": "test/repo",
    })
    @mock.patch.dict(os.environ, {"AGENT_REPO": "test/repo"})
    def test_process_dispatch_control_stopped_returns_blocked(self, *mocks):
        result = ci_handler.process_ci_dispatch(42, 207, "abc123", 99999)
        self.assertEqual(result["action"], "blocked")
        self.assertEqual(result["reason"], "control_emergency_stop_activated")


class TestRunnerReadiness(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.directory.name) / "runner"
        (self.root / "bin").mkdir(parents=True)
        for name in (".runner", ".credentials", ".credentials_rsaparams"):
            (self.root / name).write_bytes(b"credential-content-must-not-escape")
        listener = self.root / "bin" / "Runner.Listener"
        listener.write_text("placeholder")
        listener.chmod(listener.stat().st_mode | stat.S_IXUSR)

    def tearDown(self):
        self.directory.cleanup()

    def command(self, *, runners, service_scope="user", service_active=True, api_error=None,
                pr_creation=True):
        def fake(argv, **kwargs):
            if argv[:2] == ["gh", "api"]:
                if api_error == "failed":
                    raise runner_readiness.ReadinessError("command_failed")
                if api_error == "malformed":
                    return "not-json"
                if argv[-1].endswith("/actions/permissions/workflow"):
                    return json.dumps({
                        "default_workflow_permissions": "read",
                        "can_approve_pull_request_reviews": pr_creation,
                    })
                return json.dumps(runners)
            if argv and argv[0] == str(self.root / "bin" / "Runner.Listener"):
                return "Runner.Listener 3.0\n"
            if argv and argv[0] == "systemctl":
                scope = "user" if "--user" in argv else "system"
                if scope != service_scope:
                    return "LoadState=not-found\nActiveState=inactive\nSubState=dead\n"
                active = "active" if service_active else "inactive"
                substate = "running" if service_active else "dead"
                return f"LoadState=loaded\nActiveState={active}\nSubState={substate}\n"
            raise AssertionError(argv)

        return fake

    def runner(self, *, busy=False, labels=None, status="online", runners=None,
               service_scope="user", service_active=True, api_error=None, allow_busy=False,
               pr_creation=True):
        entry = {
            "id": 17,
            "name": "Vader",
            "status": status,
            "busy": busy,
            "labels": [{"name": value} for value in (labels or ["self-hosted", "vader", "agent-worker"])],
        }
        pages = runners if runners is not None else [{"total_count": 1, "runners": [entry]}]
        with mock.patch.object(
            runner_readiness, "_run_command",
            side_effect=self.command(
                runners=pages,
                service_scope=service_scope,
                service_active=service_active,
                api_error=api_error,
                pr_creation=pr_creation,
            ),
        ):
            return runner_readiness.check_readiness(
                repo="Igzela/token-efficient-agent-harness-lab",
                runner_root=self.root,
                runner_name="Vader",
                allow_busy=allow_busy,
            )

    def test_online_idle_runner_passes_and_paginates_all_pages(self):
        first = {"total_count": 2, "runners": [{
            "id": 4, "name": "Other", "status": "online", "busy": False,
            "labels": [{"name": "self-hosted"}],
        }]}
        second = {"total_count": 2, "runners": [{
            "id": 17, "name": "Vader", "status": "online", "busy": False,
            "labels": [{"name": "self-hosted"}, {"name": "vader"}, {"name": "agent-worker"}],
        }]}
        result = self.runner(runners=[first, second])
        self.assertTrue(result["ready"])
        self.assertEqual(result["service_layout"], "user")
        self.assertFalse(result["busy"])
        self.assertTrue(result["actions_pr_creation"])

    def test_repository_pr_creation_capability_fails_closed(self):
        result = self.runner(pr_creation=False)
        self.assertFalse(result["ready"])
        self.assertEqual(result["reason"], "github_actions_pr_creation_disabled")

    def test_repository_only_preflight_has_no_runner_dependency(self):
        with mock.patch.object(
            runner_readiness,
            "_run_command",
            return_value=json.dumps({"can_approve_pull_request_reviews": True}),
        ):
            result = runner_readiness.check_repository_capabilities(
                "Igzela/token-efficient-agent-harness-lab"
            )
        self.assertEqual(result, {
            "actions_pr_creation": True,
            "ready": True,
            "repo": "Igzela/token-efficient-agent-harness-lab",
        })

    def test_allowed_busy_runner_passes_only_with_explicit_option(self):
        self.assertFalse(self.runner(busy=True)["ready"])
        self.assertTrue(self.runner(busy=True, allow_busy=True)["ready"])

    def test_offline_runner_fails_closed(self):
        result = self.runner(status="offline")
        self.assertFalse(result["ready"])
        self.assertEqual(result["reason"], "runner_offline")

    def test_missing_labels_fails_closed(self):
        result = self.runner(labels=["self-hosted", "vader"])
        self.assertFalse(result["ready"])
        self.assertEqual(result["reason"], "runner_labels_invalid")

    def test_duplicate_identity_fails_closed(self):
        duplicate = [{"total_count": 2, "runners": [
            {"id": 17, "name": "Vader", "status": "online", "busy": False,
             "labels": [{"name": "self-hosted"}, {"name": "vader"}, {"name": "agent-worker"}]},
            {"id": 18, "name": "Vader", "status": "online", "busy": False,
             "labels": [{"name": "self-hosted"}, {"name": "vader"}, {"name": "agent-worker"}]},
        ]}]
        result = self.runner(runners=duplicate)
        self.assertFalse(result["ready"])
        self.assertEqual(result["reason"], "runner_identity_ambiguous")

    def test_inactive_service_fails_closed(self):
        result = self.runner(service_active=False)
        self.assertFalse(result["ready"])
        self.assertEqual(result["reason"], "service_not_active")

    def test_system_service_layout_is_supported(self):
        result = self.runner(service_scope="system")
        self.assertTrue(result["ready"])
        self.assertEqual(result["service_layout"], "system")

    def test_missing_local_configuration_fails_without_reading_contents(self):
        (self.root / ".credentials").unlink()
        result = self.runner()
        self.assertFalse(result["ready"])
        self.assertEqual(result["reason"], "runner_configuration_missing")

    def test_malformed_or_failed_api_response_fails_closed(self):
        for api_error in ("malformed", "failed"):
            with self.subTest(api_error=api_error):
                result = self.runner(api_error=api_error)
                self.assertFalse(result["ready"])
                self.assertEqual(result["reason"], "github_workflow_permissions_unavailable")

    def test_credential_contents_never_appear_in_bounded_status(self):
        result = self.runner()
        serialized = json.dumps(result, sort_keys=True)
        self.assertNotIn("credential-content-must-not-escape", serialized)
        self.assertLess(len(serialized.encode("utf-8")), 4096)


if __name__ == "__main__":
    unittest.main(verbosity=2)
