"""Regression tests for the event-driven orchestrator's deterministic path."""

from __future__ import annotations

import json
import ast
import hashlib
import os
import pathlib
import stat
import subprocess
import sys
import tempfile
import threading
import unittest
from unittest import mock


ROOT = pathlib.Path(__file__).resolve().parents[1]
CONTROL = ROOT / "scripts" / "agent-control"
WORKFLOWS = ROOT / ".github" / "workflows"
# Synthetic GitHub-run fixtures must opt out of production identity enrichment.
os.environ.setdefault("AGENT_CI_FIXTURE_MODE", "true")
sys.path.insert(0, str(CONTROL))

import artifact_contract
import ci_handler
import ci_verifier
import control_state
import dispatcher
import pr_binding
import prompt_builder
import runner_readiness
import state_manager


def _successful_required_jobs():
    """Jobs that satisfy exact-head checkout evidence for all required jobs."""
    step = {
        "name": ci_verifier.EXACT_HEAD_VERIFY_STEP,
        "status": "completed",
        "conclusion": "success",
    }
    return [
        {
            "name": name,
            "status": "completed",
            "conclusion": "success",
            "steps": [step],
        }
        for name in ci_verifier.load_requirements()["required_jobs"]
    ]


def _pr_binding_fixture(number=207, sha=None, issue_number=42, branch="agent/issue-42", is_draft=True):
    """A valid Issue-bound PR fixture that also carries the Draft flag."""
    if sha is None:
        sha = "b" * 40
    body = (
        f"Closes #{issue_number}\n\n"
        f'<!-- agent-orchestrator-binding: {{"issue_number": {issue_number}, "branch": "{branch}"}} -->'
    )
    return {
        "number": number,
        "state": "OPEN",
        "baseRefName": "main",
        "baseRefOid": "c" * 40,
        "headRefName": branch,
        "headRefOid": sha,
        "body": body,
        "url": f"https://github.com/acme/repo/pull/{number}",
        "isDraft": is_draft,
    }


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

    def test_review_workflow_has_bounded_retrospective_correction_lane(self):
        workflow = self.read("agent-review.yml")
        self.assertIn("retrospective-correction:", workflow)
        self.assertIn("record-review-correction", workflow)
        self.assertIn("Read back effective corrected review state", workflow)
        self.assertIn("historical_merge_compliant == false", workflow)
        self.assertIn("if: inputs.operation == 'review'", workflow)

    def test_all_workflow_dispatch_callers_and_inputs_match(self):
        expected = {
            "agent-controller.yml": {"command", "issue", "pr_number", "head_sha", "ci_run_id", "repair_count", "source_issue", "dispatch_id", "attempt_id", "client_token"},
            "agent-worker.yml": {"issue", "dry_run", "dispatch_id", "claim_nonce"},
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

    def test_new_implementation_claims_route_through_agent_dispatch_global(self):
        controller = self.read("agent-controller.yml")
        intake = self.read("agent-intake.yml")
        merge = self.read("agent-merge.yml")
        self.assertIn("group: agent-dispatch-global", controller)
        self.assertIn("gh workflow run agent-controller.yml", intake)
        self.assertIn("-f command=dispatch-ready", intake)
        self.assertNotIn("dispatcher.py dispatch-ready", intake)
        self.assertIn("gh workflow run agent-controller.yml", merge)
        self.assertIn("-f command=dispatch-next", merge)
        self.assertNotIn("dispatcher.py dispatch-next", merge)

    def test_canonical_workflow_has_one_top_level_environment_mapping(self):
        source = self.read("tests.yml")
        self.assertEqual(source.count("\nenv:\n"), 1)
        self.assertIn(
            "  EXPECTED_SHA: ${{ inputs.expected_sha || github.event.pull_request.head.sha || github.sha }}",
            source,
        )

    def test_canonical_workflow_binds_checkout_and_fail_closed_exact_head(self):
        source = self.read("tests.yml")
        # workflow_dispatch must require expected_sha (orchestrator fallback).
        self.assertIn("      expected_sha:\n", source)
        self.assertRegex(
            source,
            r"expected_sha:\n(?:.*\n){0,4}\s+required:\s*true",
        )
        # Every checkout must pin the resolved expected commit (seven test jobs + context-capsule publisher).
        self.assertEqual(source.count("ref: ${{ env.EXPECTED_SHA }}"), 8)
        # Exact-head verification must not be skippable via inputs.expected_sha.
        self.assertNotIn("if: inputs.expected_sha", source)
        self.assertEqual(source.count("name: Verify exact requested head"), 8)
        # Shell verification uses env (not raw input interpolation) and fails closed.
        scripts = self.shell_scripts("tests.yml")
        self.assertIn('if [ -z "${EXPECTED_SHA}" ]', scripts)
        self.assertIn('if [ "${actual}" != "${EXPECTED_SHA}" ]', scripts)
        self.assertIn("exit 1", scripts)

    def test_context_capsule_publisher_binds_pr_and_reuses_one_snapshot(self):
        source = self.read("tests.yml")
        self.assertIn("GITHUB_PR_NUMBER: ${{ github.event.pull_request.number || '' }}", source)
        self.assertIn("ref: ${{ github.event.pull_request.base.sha }}", source)
        self.assertIn("uses: ./trusted-base/actions/exact-head-check", source)
        self.assertIn("--exact-head-proof trusted-exact-head-proof.json", source)
        self.assertNotIn('checks["exact-head-check"] = {"result": "success"}', source)
        self.assertIn("--capsule-json context-capsule/context-capsule.json", source)
        repair = self.read("agent-ci-repair.yml")
        prompt_step = repair.split("Build repair prompt from trusted checkout and bounded evidence", 1)[1].split("Recheck control immediately before Codex repair", 1)[0]
        self.assertNotIn("GITHUB_SHA: ${{ inputs.head_sha }}", prompt_step)
        self.assertIn("AGENT_CONTEXT_EXPECTED_HEAD_SHA: ${{ inputs.head_sha }}", prompt_step)
        self.assertIn('actual="$(git rev-parse HEAD)"', prompt_step)
        self.assertIn('test "$actual" = "$INPUT_HEAD_SHA"', prompt_step)
        self.assertNotIn("GH_TOKEN:", prompt_step)

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
                self.assertIn("artifact_contract.py validate-scope-binding", source)
                self.assertIn("git apply --index --binary", source)
                self.assertIn("artifact_contract.py validate-index", source)
                self.assertIn("git diff --cached --check", source)
                self.assertIn("git diff --check", source)
                self.assertIn("control_state.py require-live", source)
                self.assertIn("agent.patch", source)
                self.assertIn("agent-result.json", source)

    def test_finalizers_and_workers_use_claim_bound_task_scope(self):
        for name in ("agent-worker.yml", "agent-ci-repair.yml"):
            source = self.read(name)
            with self.subTest(name=name):
                self.assertIn("verify-task-scope-binding", source)
                self.assertIn("artifact_contract.py validate-scope-binding", source)
                self.assertIn("task-scope-binding.json", source)
                self.assertNotIn("gh issue view", source)
                self.assertNotIn("task-issue-body.md", source)
                self.assertNotIn("artifact_contract.py validate-scope ", source)

    def test_worker_rechecks_claim_bound_scope_immediately_before_codex(self):
        source = self.read("agent-worker.yml")
        step = source.split("Recheck control and claim-bound task scope immediately before Codex", 1)[1]
        step = step.split("\n  - name:", 1)[0]
        self.assertIn("verify-task-scope-binding", step)
        self.assertIn("control_state.py require-live", step)

    def test_repair_prepare_rechecks_claim_bound_scope_before_codex(self):
        source = self.read("agent-ci-repair.yml")
        prepare = source.split("  prepare:", 1)[1].split("\n  vader-repair:", 1)[0]
        self.assertIn("verify-task-scope-binding", prepare)
        self.assertIn("verify-binding", prepare)

    def test_repair_rechecks_claim_bound_scope_immediately_before_codex_repair(self):
        source = self.read("agent-ci-repair.yml")
        step = source.split("Recheck control immediately before Codex repair", 1)[1]
        step = step.split("\n  - name:", 1)[0]
        self.assertIn("control_state.py require-live", step)
        self.assertIn('verify-task-scope-binding "${{ inputs.issue_number }}"', step)

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

    def test_pr_binding_failure_precedes_worker_state_and_ci_acquisition(self):
        worker = self.read("agent-worker.yml")
        repair = self.read("agent-ci-repair.yml")
        worker_create = worker.index("pr_binding.py create-or-update")
        worker_record = worker.index("state_manager.py record-worker")
        worker_acquire = worker.index("ci_verifier.py acquire-run")
        self.assertLess(worker_create, worker_record)
        self.assertLess(worker_record, worker_acquire)
        repair_verify = repair.index("pr_binding.py verify-post-push")
        repair_update = repair.index("pr_binding.py create-or-update")
        repair_record = repair.index("state_manager.py record-worker")
        repair_acquire = repair.index("ci_verifier.py acquire-run")
        self.assertLess(repair_verify, repair_update)
        self.assertLess(repair_update, repair_record)
        self.assertLess(repair_record, repair_acquire)

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
                "${{ inputs.attempt_id }}",
                "${{ inputs.client_token }}",
            ),
            "agent-ci-repair.yml": ("${{ inputs.head_sha }}",),
            "agent-review.yml": ("${{ inputs.head_sha }}",),
            "agent-merge.yml": ("${{ inputs.head_sha }}",),
            "tests.yml": ("${{ inputs.expected_sha }}",),
            "agent-worker.yml": ("${{ inputs.dispatch_id }}", "${{ inputs.claim_nonce }}"),
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
            "noop": "release-ci-terminal",
        }.items():
            with self.subTest(action=action):
                self.assertIn(required, monitor)
        worker = self.read("agent-worker.yml")
        self.assertIn("id: acquire_ci", worker)
        self.assertIn("ci_control_stopped=true", worker)
        self.assertIn("ci_control_stopped:after_acquisition", worker)
        self.assertIn("if: always() &&", worker)
        self.assertIn("ci_control_stopped:after_acquisition", self.read("agent-ci-repair.yml"))
        for workflow in ("agent-ci-repair.yml", "agent-review.yml", "agent-merge.yml"):
            self.assertIn("if: always() &&", self.read(workflow))
        self.assertIn("steps.acquire_ci.outputs.ci_control_stopped != 'true'", worker)
        self.assertIn("record-ci", worker)
        controller = self.read("agent-controller.yml")
        self.assertIn("group: agent-dispatch-global", controller)
        self.assertIn("cancel-in-progress: false", controller)
        self.assertNotIn("cancel-in-progress: ${{ inputs.command == 'emergency-stop' }}", controller)
        per_resource_groups = {
            "agent-ci-monitor.yml": "agent-ci-monitor-${{ github.event.workflow_run.id || github.run_id }}",
            "agent-intake.yml": "agent-intake-${{ github.event.issue.number }}",
            "agent-worker.yml": "agent-worker-${{ inputs.issue }}",
            "agent-ci-repair.yml": "agent-ci-repair-${{ inputs.pr_number }}",
            "agent-review.yml": "agent-review-${{ inputs.pr_number }}-${{ inputs.head_sha }}",
            "agent-merge.yml": "agent-merge-${{ inputs.pr_number }}-${{ inputs.head_sha }}",
        }
        for workflow, group in per_resource_groups.items():
            source = self.read(workflow)
            self.assertIn(f"group: {group}", source)
            self.assertIn("cancel-in-progress: false", source)
        for workflow in list(per_resource_groups) + ["agent-controller.yml"]:
            self.assertNotIn("agent-orchestrator-state", self.read(workflow))

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
                "steps.decision.outputs.terminal_status != ''",
                "release-ci-terminal",
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

        terminal_consumers = {
            action: consumer for action, consumer in ci_handler.DISPATCH_ACTION_CONSUMERS.items()
            if action in {"blocked", "stale", "noop"}
        }
        self.assertEqual(
            terminal_consumers,
            {"blocked": "release_terminal_capacity", "stale": "release_terminal_capacity", "noop": "release_terminal_capacity"},
        )
        for action in terminal_consumers:
            result = {
                "action": action,
                "issue_number": 42,
                "pr_number": 207,
                "head_sha": "a" * 40,
                "ci_run_id": 9001,
                "terminal_status": f"terminal_{action}",
            }
            active_labels = {state_manager.LABEL_RUNNING}
            comments = []

            def release_capacity(issue, *new_labels, repo=""):
                active_labels.difference_update(state_manager.ACTIVE_LABELS)
                active_labels.update(new_labels)
                return True

            def write_comment(issue, body, repo=""):
                comments.append({"author": {"login": "github-actions[bot]"}, "body": body})
                return True

            with self.subTest(action=action), \
                 mock.patch.object(state_manager, "get_issue_comments", return_value=comments), \
                 mock.patch.object(state_manager, "comment_on_issue", side_effect=write_comment), \
                 mock.patch.object(state_manager, "get_issue_labels_checked", return_value=active_labels), \
                 mock.patch.object(state_manager, "read_worker_state", return_value={"pr_number": 207, "head_sha": "a" * 40}), \
                 mock.patch.object(state_manager, "read_ci_state", return_value={"workflow_run_id": 9001}), \
                 mock.patch.object(state_manager, "has_inflight_ci_dispatch", return_value=False), \
                 mock.patch.object(state_manager, "verify_ci_terminal_binding", return_value=(True, "ok")), \
                 mock.patch.object(state_manager, "set_labels", side_effect=release_capacity):
                self.assertEqual(ci_handler.validate_ci_dispatch_decision(result), "release_terminal_capacity")
                released = state_manager.release_and_record_ci_terminal(
                    42, 207, "a" * 40, 9001,
                    result["terminal_status"], "test", "completed",
                )
                self.assertEqual(released, (True, "released"))
                self.assertFalse(active_labels & state_manager.ACTIVE_LABELS)

        monitor = self.read("agent-ci-monitor.yml")
        self.assertIn("terminal_ci_followup_dispatch_failed", monitor)
        self.assertIn('"ci_followup_dispatch_failed:$ACTION" "$OBSERVED_STATUS"', monitor)
        self.assertIn("Reconcile a cancelled follow-up dispatch claim", monitor)
        self.assertIn("state_manager.py reconcile-dispatch", monitor)
        self.assertIn("if: always() && steps.decision.outputs.terminal_status == ''", monitor)
        self.assertIn("reconcile_claimed_dispatch", (CONTROL / "state_manager.py").read_text())
        self.assertIn("terminal_ci_merge_dispatch_failed", monitor)
        self.assertIn("ci_merge_dispatch_failed:merge_ready", monitor)
        self.assertIn("capacity_retained", monitor)
        self.assertIn("dispatch_in_flight", monitor)
        self.assertIn("capacity_already_claimed", monitor)
        self.assertIn("capacity_already_claimed_unverified", monitor)
        self.assertIn("REPAIR_CAPACITY_RETAINED", monitor)
        self.assertIn("MERGE_CAPACITY_RETAINED", monitor)
        self.assertIn("has_inflight_ci_dispatch", (CONTROL / "state_manager.py").read_text())
        self.assertIn('"${{ steps.decision.outputs.ci_run_id }}"', monitor)

    def test_worker_has_post_claim_nonstart_capacity_release(self):
        source = self.read("agent-worker.yml")
        self.assertIn("rejected-before-vader:", source)
        self.assertIn("release-rejected-worker", source)
        self.assertIn("needs: [gate, validate, vader-implementation, finalize]", source)
        step = source.split("Release a successfully rejected dispatcher claim", 1)[1]
        step = step.split("\n  - name:", 1)[0]
        self.assertIn("INPUT_DISPATCH_ID: ${{ inputs.dispatch_id }}", source)
        self.assertIn('"$INPUT_DISPATCH_ID"', step)
        self.assertIn("INPUT_CLAIM_NONCE: ${{ inputs.claim_nonce }}", source)
        self.assertIn('"$INPUT_CLAIM_NONCE"', step)
        self.assertEqual(source.count("${{ inputs.dispatch_id }}"), 1)
        self.assertEqual(source.count("${{ inputs.claim_nonce }}"), 1)
        self.assertIn("      dispatch_id:\n", source)
        self.assertIn("      claim_nonce:\n", source)


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

    def test_duplicate_noop_preserves_normally_audited_repair_claim(self):
        comments = [{
            "author": {"login": "github-actions[bot]"},
            "body": json.dumps({
                "kind": "agent-orchestrator-dispatch-state",
                "status": "dispatched",
                "action": "repair",
                "details": {
                    "pr_number": 207,
                    "issue_number": 42,
                    "head_sha": "a" * 40,
                    "ci_run_id": "9001",
                },
            }),
        }]

        with mock.patch.object(
            state_manager, "get_issue_comments", return_value=comments
        ), mock.patch.object(
            state_manager, "comment_on_issue", return_value=True
        ), mock.patch.object(
            state_manager, "release_failed_capacity"
        ) as release:
            result = state_manager.release_and_record_ci_terminal(
                42, 207, "a" * 40, 9001, "terminal_noop",
                "ci_noop:duplicate_exact_head_run", "completed",
            )

        self.assertEqual(result, (True, "dispatch_in_flight"))
        release.assert_not_called()

    def test_cancelled_claim_reconciliation_releases_only_unfinished_claim(self):
        claim = {
            "kind": "agent-orchestrator-dispatch-state",
            "status": "claimed",
            "action": "repair",
            "details": {
                "target_label": state_manager.LABEL_CI_REPAIRING,
                "pr_number": 207,
                "head_sha": "a" * 40,
                "ci_run_id": "9001",
            },
        }
        with mock.patch.object(state_manager, "read_dispatch_state", return_value=claim), \
             mock.patch.object(state_manager, "release_failed_capacity", return_value=(True, "released")) as release, \
             mock.patch.object(state_manager, "record_dispatch_state", return_value=True) as record:
            result = state_manager.reconcile_claimed_dispatch(
                42, "repair:207:" + "a" * 40 + ":9001:0",
                "workflow_cancelled:followup_dispatch",
            )
        self.assertEqual(result, (True, "released"))
        release.assert_called_once_with(
            42,
            state_manager.LABEL_CI_REPAIRING,
            state_manager.LABEL_BLOCKED,
            expected_sha="a" * 40,
            repo="",
            expected_pr=207,
            expected_run_id="9001",
        )
        self.assertEqual(record.call_args.args[3], "failed")

    def test_cancelled_dispatched_claim_remains_owned_by_child_workflow(self):
        claim = {
            "kind": "agent-orchestrator-dispatch-state",
            "status": "dispatched",
            "action": "review",
            "details": {"target_label": state_manager.LABEL_REVIEW_RUNNING},
        }
        with mock.patch.object(state_manager, "read_dispatch_state", return_value=claim), \
             mock.patch.object(state_manager, "release_failed_capacity") as release:
            result = state_manager.reconcile_claimed_dispatch(
                42, "review:207:" + "a" * 40,
                "workflow_cancelled:followup_dispatch",
            )
        self.assertEqual(result, (True, "dispatched_in_flight"))
        release.assert_not_called()

    def test_repeated_cancellation_reconciliation_does_not_double_release(self):
        claim = {
            "kind": "agent-orchestrator-dispatch-state",
            "status": "claimed",
            "action": "repair",
            "details": {
                "target_label": state_manager.LABEL_CI_REPAIRING,
                "pr_number": 207,
                "head_sha": "a" * 40,
                "ci_run_id": "9001",
            },
        }
        failed_claim = {**claim, "status": "failed"}
        states = [claim, failed_claim]

        def read_dispatch(_issue, _dispatch_id, _repo):
            return states[0] if states else None

        with mock.patch.object(state_manager, "read_dispatch_state", side_effect=read_dispatch), \
             mock.patch.object(state_manager, "release_failed_capacity", return_value=(True, "released")) as release, \
             mock.patch.object(state_manager, "record_dispatch_state", side_effect=lambda *a, **kw: states.pop(0) or True):
            first = state_manager.reconcile_claimed_dispatch(
                42, "repair:207:" + "a" * 40 + ":9001:0",
                "workflow_cancelled:followup_dispatch",
            )
            second = state_manager.reconcile_claimed_dispatch(
                42, "repair:207:" + "a" * 40 + ":9001:0",
                "workflow_cancelled:followup_dispatch",
            )
        self.assertEqual(first, (True, "released"))
        self.assertEqual(second, (True, "already_terminal"))
        release.assert_called_once()

    def test_release_rejects_unrelated_pr_ownership(self):
        labels = {state_manager.LABEL_CI_REPAIRING}

        def set_labels(_issue, *new_labels, repo=""):
            labels.clear()
            labels.update(new_labels)
            return True

        with mock.patch.object(
            state_manager, "get_issue_labels_checked",
            side_effect=[set(labels), set(labels)],
        ), mock.patch.object(
            state_manager, "read_worker_state", return_value={"pr_number": 208, "head_sha": "a" * 40},
        ), mock.patch.object(
            state_manager, "read_ci_state",
        ), mock.patch.object(
            state_manager, "set_labels", side_effect=set_labels,
        ) as transition:
            ok, reason = state_manager.release_failed_capacity(
                42, "ci-repairing", state_manager.LABEL_BLOCKED,
                expected_sha="a" * 40, expected_pr=207, expected_run_id=9001,
            )
        self.assertFalse(ok)
        self.assertEqual(reason, "worker_pr_mismatch")
        transition.assert_not_called()

    def test_release_rejects_unrelated_ci_run_ownership(self):
        labels = {state_manager.LABEL_CI_REPAIRING}

        def set_labels(_issue, *new_labels, repo=""):
            labels.clear()
            labels.update(new_labels)
            return True

        with mock.patch.object(
            state_manager, "get_issue_labels_checked",
            side_effect=[set(labels), set(labels)],
        ), mock.patch.object(
            state_manager, "read_worker_state", return_value={"pr_number": 207, "head_sha": "a" * 40},
        ), mock.patch.object(
            state_manager, "read_ci_state", return_value={"workflow_run_id": 9002},
        ), mock.patch.object(
            state_manager, "set_labels", side_effect=set_labels,
        ) as transition:
            ok, reason = state_manager.release_failed_capacity(
                42, "ci-repairing", state_manager.LABEL_BLOCKED,
                expected_sha="a" * 40, expected_pr=207, expected_run_id=9001,
            )
        self.assertFalse(ok)
        self.assertEqual(reason, "ci_run_mismatch")
        transition.assert_not_called()

    def test_release_rejects_capacity_state_changed_during_authorization_window(self):
        first_read = {state_manager.LABEL_CI_REPAIRING}
        second_read = {state_manager.LABEL_RUNNING}

        with mock.patch.object(
            state_manager, "get_issue_labels_checked",
            side_effect=[first_read, second_read],
        ), mock.patch.object(
            state_manager, "read_worker_state", return_value={"pr_number": 207, "head_sha": "a" * 40},
        ), mock.patch.object(
            state_manager, "read_ci_state", return_value={"workflow_run_id": 9001},
        ), mock.patch.object(
            state_manager, "set_labels",
        ) as transition:
            ok, reason = state_manager.release_failed_capacity(
                42, "ci-repairing", state_manager.LABEL_BLOCKED,
                expected_sha="a" * 40, expected_pr=207, expected_run_id=9001,
            )
        self.assertFalse(ok)
        self.assertEqual(reason, "capacity_state_changed")
        transition.assert_not_called()

    def test_stale_head_during_cancellation_is_rejected_by_terminal_binding(self):
        with mock.patch.object(
            state_manager, "read_worker_state",
            return_value={"pr_number": 207, "head_sha": "b" * 40, "extra": {"branch": "agent/issue-42"}},
        ):
            ok, reason = state_manager.verify_ci_terminal_binding(
                42, 207, "a" * 40, "",
            )
        self.assertFalse(ok)
        self.assertEqual(reason, "worker_binding_mismatch")

    def test_no_active_capacity_leak_after_terminal_release(self):
        labels = {state_manager.LABEL_CI_REPAIRING}

        def set_labels(_issue, *new_labels, repo=""):
            labels.clear()
            labels.update(new_labels)
            return True

        with mock.patch.object(
            state_manager, "get_issue_labels_checked",
            side_effect=[set(labels), set(labels)],
        ), mock.patch.object(
            state_manager, "read_worker_state", return_value={"pr_number": 207, "head_sha": "a" * 40},
        ), mock.patch.object(
            state_manager, "read_ci_state", return_value={"workflow_run_id": 9001},
        ), mock.patch.object(
            state_manager, "verify_ci_terminal_binding", return_value=(True, "ok"),
        ), mock.patch.object(
            state_manager, "set_labels", side_effect=set_labels,
        ):
            ok, reason = state_manager.release_failed_capacity(
                42, "ci-repairing", state_manager.LABEL_BLOCKED,
                expected_sha="a" * 40, expected_pr=207, expected_run_id=9001,
            )
        self.assertTrue(ok)
        self.assertEqual(reason, "released")
        self.assertFalse(labels & state_manager.ACTIVE_LABELS)

    def test_legacy_generic_issue_mutators_are_not_exposed(self):
        source = (CONTROL / "state_manager.py").read_text()
        for command in ("select-task", "next-task", "retry-task", "block-task"):
            self.assertNotIn(f'command == "{command}"', source)


class TestDispatcher(unittest.TestCase):
    def test_active_scope_comes_from_trusted_claim_not_mutable_issue_body(self):
        claim = {
            "kind": "agent-orchestrator-dispatch-state",
            "version": 1,
            "issue_number": 41,
            "dispatch_id": "worker:41",
            "action": "worker",
            "status": "dispatched",
            "details": {
                "allowed_paths": ["scripts/"],
                "task_body_sha256": "a" * 64,
            },
        }
        comments = [{
            "author": {"login": "github-actions[bot]"},
            "body": json.dumps(claim),
        }]
        with mock.patch.object(state_manager, "get_issue_comments", return_value=comments), \
             mock.patch.object(state_manager, "validate_task_scope", side_effect=AssertionError("mutable body read")):
            scopes = state_manager.get_active_issue_scopes({41}, "acme/repo")

        self.assertEqual(scopes, {41: ["scripts/"]})

    def test_current_task_body_must_match_claim_bound_digest(self):
        original = '<!-- agent-orchestrator-scope:v1 {"allowed_paths":["src/"]} -->'
        changed = '<!-- agent-orchestrator-scope:v1 {"allowed_paths":["docs/"]} -->'
        binding = artifact_contract.build_issue_scope_binding(original)
        claim = {
            "kind": "agent-orchestrator-dispatch-state",
            "version": 1,
            "issue_number": 41,
            "dispatch_id": "worker:41",
            "action": "worker",
            "status": "dispatched",
            "details": binding,
        }
        comments = [{
            "author": {"login": "github-actions[bot]"},
            "body": json.dumps(claim),
        }]
        with mock.patch.object(state_manager, "get_issue_comments", return_value=comments), \
             mock.patch.object(state_manager, "get_issue_body", return_value=changed):
            ok, reason, observed = state_manager.verify_task_scope_binding(41, "acme/repo")

        self.assertFalse(ok)
        self.assertEqual(reason, "task_body_changed")
        self.assertIsNone(observed)

    def worker_claim(self, **overrides):
        claim = {
            "kind": "agent-orchestrator-dispatch-state",
            "version": 1,
            "issue_number": 41,
            "dispatch_id": "worker:41",
            "action": "worker",
            "status": "dispatched",
            "details": {
                "allowed_paths": ["src/"],
                "task_body_sha256": "a" * 64,
                "claim_nonce": "a" * 32,
            },
        }
        claim.update(overrides)
        return claim

    def trusted_comment(self, state):
        return {"author": {"login": "github-actions[bot]"}, "body": json.dumps(state)}

    def test_read_dispatch_state_fails_closed_on_malformed_marker_comment(self):
        malformed = {
            "author": {"login": "github-actions[bot]"},
            "body": '{"kind": "agent-orchestrator-dispatch-state", "status": "dispatched",',
        }
        older = self.worker_claim()
        comments = [malformed, self.trusted_comment(older)]
        with mock.patch.object(state_manager, "get_issue_comments", return_value=comments):
            with self.assertRaises(state_manager.StateUnavailableError):
                state_manager.read_dispatch_state(41, "worker:41", "acme/repo")

    def test_read_dispatch_state_fails_closed_on_marker_quoting_prose(self):
        prose = {
            "author": {"login": "github-actions[bot]"},
            "body": "plain text mentioning agent-orchestrator-dispatch-state",
        }
        older = self.worker_claim()
        comments = [prose, self.trusted_comment(older)]
        with mock.patch.object(state_manager, "get_issue_comments", return_value=comments):
            with self.assertRaises(state_manager.StateUnavailableError):
                state_manager.read_dispatch_state(41, "worker:41", "acme/repo")

    def test_read_dispatch_state_fails_closed_on_wrong_version(self):
        newer = self.worker_claim(version=2)
        older = self.worker_claim()
        comments = [self.trusted_comment(newer), self.trusted_comment(older)]
        with mock.patch.object(state_manager, "get_issue_comments", return_value=comments):
            with self.assertRaises(state_manager.StateUnavailableError):
                state_manager.read_dispatch_state(41, "worker:41", "acme/repo")

    def test_read_dispatch_state_skips_unrelated_and_wrong_issue_documents(self):
        wrong_issue = self.worker_claim(issue_number=42, dispatch_id="worker:42")
        review = {
            "kind": "agent-orchestrator-dispatch-state", "version": 1, "issue_number": 41,
            "dispatch_id": "review:207:" + "a" * 40, "action": "review", "status": "dispatched",
            "details": {"pr_number": 207, "issue_number": 41, "head_sha": "a" * 40},
        }
        repair = {
            "kind": "agent-orchestrator-dispatch-state", "version": 1, "issue_number": 41,
            "dispatch_id": "repair:207:" + "a" * 40 + ":9001:0", "action": "repair",
            "status": "dispatched",
            "details": {"pr_number": 207, "issue_number": 41, "head_sha": "a" * 40},
        }
        quoting = {
            "kind": "agent-orchestrator-review-state", "version": 2, "issue_number": 41,
            "pr_number": 207, "head_sha": "a" * 40, "verdict": "BLOCKED",
            "summary": 'quotes "agent-orchestrator-dispatch-state" in prose',
            "blockers": [], "major_notes": [], "minor_notes": [], "artifact_sha256": "",
        }
        claim = self.worker_claim()
        comments = [
            self.trusted_comment(wrong_issue),
            self.trusted_comment(review),
            self.trusted_comment(repair),
            self.trusted_comment(quoting),
            self.trusted_comment(claim),
        ]
        with mock.patch.object(state_manager, "get_issue_comments", return_value=comments):
            observed = state_manager.read_dispatch_state(41, "worker:41", "acme/repo")
        self.assertEqual(observed, claim)

    def test_newer_wrong_version_review_repair_and_merge_do_not_block_exact_worker_state(self):
        review = {
            "kind": "agent-orchestrator-dispatch-state", "version": 2, "issue_number": 41,
            "dispatch_id": "review:207:" + "a" * 40, "action": "review", "status": "dispatched",
            "details": {"pr_number": 207, "issue_number": 41, "head_sha": "a" * 40},
        }
        repair = {
            "kind": "agent-orchestrator-dispatch-state", "version": 2, "issue_number": 41,
            "dispatch_id": "repair:207:" + "a" * 40 + ":9001:0", "action": "repair",
            "status": "dispatched",
            "details": {"pr_number": 207, "issue_number": 41, "head_sha": "a" * 40},
        }
        merge = {
            "kind": "agent-orchestrator-dispatch-state", "version": 2, "issue_number": 41,
            "dispatch_id": "merge:207:" + "a" * 40, "action": "merge", "status": "dispatched",
            "details": {"pr_number": 207, "issue_number": 41, "head_sha": "a" * 40},
        }
        claim = self.worker_claim()
        comments = [
            self.trusted_comment(review),
            self.trusted_comment(repair),
            self.trusted_comment(merge),
            self.trusted_comment(claim),
        ]
        with mock.patch.object(state_manager, "get_issue_comments", return_value=comments):
            observed = state_manager.read_dispatch_state(41, "worker:41", "acme/repo")
        self.assertEqual(observed, claim)

    def test_capacity_recheck_compensates_when_authoritative_active_exceeds_k(self):
        labels = {state_manager.LABEL_READY}
        persisted = {}

        def read_dispatch(_issue, dispatch_id, _repo):
            return persisted.get(dispatch_id)

        def set_labels(_issue, *new_labels, repo=""):
            labels.clear()
            labels.update(new_labels)
            return True

        def record(_issue, dispatch_id, action, status, details=None, repo=""):
            persisted[dispatch_id] = {
                "kind": "agent-orchestrator-dispatch-state",
                "version": 1,
                "issue_number": _issue,
                "dispatch_id": dispatch_id,
                "action": action,
                "status": status,
                "details": dict(details or {}),
            }
            return True

        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="acme/repo"), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", side_effect=read_dispatch), \
             mock.patch.object(dispatcher.sm, "get_issue_labels_checked", return_value=labels), \
             mock.patch.object(dispatcher.sm, "read_task_scope_binding", return_value=(True, {
                 "allowed_paths": ["src/"], "task_body_sha256": "a" * 64,
             })), \
             mock.patch.object(dispatcher.sm, "check_dependencies_complete", return_value=(True, None)), \
             mock.patch.object(dispatcher.sm, "has_open_issue_pr", return_value=False), \
             mock.patch.object(dispatcher.sm, "get_active_capacity", side_effect=[
                 {"issues": set(), "plans": []},
                 {"issues": {41, 42, 77}, "plans": []},
             ]), \
             mock.patch.object(dispatcher.sm, "get_active_issue_scopes", return_value={}, create=True), \
             mock.patch.object(dispatcher.sm, "set_labels", side_effect=set_labels), \
             mock.patch.object(dispatcher.sm, "record_dispatch_state", side_effect=record), \
             mock.patch.object(dispatcher, "_run_workflow") as workflow:
            result = dispatcher.dispatch_ready(77, "worker:77")
        self.assertFalse(result["dispatched"])
        self.assertEqual(result["reason"], "capacity_recheck_exceeded")
        workflow.assert_not_called()
        rollback = persisted["worker:77"]
        self.assertEqual(rollback["status"], "failed")
        self.assertEqual(rollback["action"], "rollback")
        self.assertEqual(rollback["details"]["reason"], "capacity_recheck_exceeded")
        self.assertEqual(rollback["details"]["allowed_paths"], ["src/"])
        self.assertEqual(rollback["details"]["task_body_sha256"], "a" * 64)
        self.assertRegex(rollback["details"]["claim_nonce"], r"^[0-9a-f]{32}$")
        self.assertEqual(labels, {state_manager.LABEL_READY})

    def test_capacity_recheck_unavailable_fails_closed_and_compensates(self):
        labels = {state_manager.LABEL_READY}
        persisted = {}

        def read_dispatch(_issue, dispatch_id, _repo):
            return persisted.get(dispatch_id)

        def set_labels(_issue, *new_labels, repo=""):
            labels.clear()
            labels.update(new_labels)
            return True

        def record(_issue, dispatch_id, action, status, details=None, repo=""):
            persisted[dispatch_id] = {
                "kind": "agent-orchestrator-dispatch-state",
                "version": 1,
                "issue_number": _issue,
                "dispatch_id": dispatch_id,
                "action": action,
                "status": status,
                "details": dict(details or {}),
            }
            return True

        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="acme/repo"), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", side_effect=read_dispatch), \
             mock.patch.object(dispatcher.sm, "get_issue_labels_checked", return_value=labels), \
             mock.patch.object(dispatcher.sm, "read_task_scope_binding", return_value=(True, {
                 "allowed_paths": ["src/"], "task_body_sha256": "a" * 64,
             })), \
             mock.patch.object(dispatcher.sm, "check_dependencies_complete", return_value=(True, None)), \
             mock.patch.object(dispatcher.sm, "has_open_issue_pr", return_value=False), \
             mock.patch.object(dispatcher.sm, "get_active_capacity", side_effect=[
                 {"issues": set(), "plans": []}, None,
             ]), \
             mock.patch.object(dispatcher.sm, "get_active_issue_scopes", return_value={}, create=True), \
             mock.patch.object(dispatcher.sm, "set_labels", side_effect=set_labels), \
             mock.patch.object(dispatcher.sm, "record_dispatch_state", side_effect=record), \
             mock.patch.object(dispatcher, "_run_workflow") as workflow:
            result = dispatcher.dispatch_ready(77, "worker:77")
        self.assertFalse(result["dispatched"])
        self.assertEqual(result["reason"], "capacity_recheck_unavailable")
        workflow.assert_not_called()
        rollback = persisted["worker:77"]
        self.assertEqual(rollback["status"], "failed")
        self.assertEqual(rollback["action"], "rollback")
        self.assertEqual(rollback["details"]["reason"], "capacity_recheck_unavailable")
        self.assertRegex(rollback["details"]["claim_nonce"], r"^[0-9a-f]{32}$")
        self.assertEqual(labels, {state_manager.LABEL_READY})

    def test_overlapping_claims_never_exceed_capacity_and_leave_no_claim_effects(self):
        preset = 41
        first = 77
        second = 88
        labels = {
            preset: {state_manager.LABEL_RUNNING},
            first: {state_manager.LABEL_READY},
            second: {state_manager.LABEL_READY},
        }
        bodies = {
            preset: '<!-- agent-orchestrator-scope:v1 {"allowed_paths":["docs/"]} -->',
            first: '<!-- agent-orchestrator-scope:v1 {"allowed_paths":["src/"]} -->',
            second: '<!-- agent-orchestrator-scope:v1 {"allowed_paths":["scripts/"]} -->',
        }
        comments = {}
        state_lock = threading.Lock()
        precheck_barrier = threading.Barrier(2)
        active_read_count = 0
        precheck_snapshots = []
        recheck_snapshots = []
        recheck_snapshot = None
        workflow_calls = []
        results = {}
        failures = []

        preset_claim = {
            "kind": "agent-orchestrator-dispatch-state",
            "version": 1,
            "issue_number": preset,
            "dispatch_id": f"worker:{preset}",
            "action": "worker",
            "status": "dispatched",
            "details": {"allowed_paths": ["docs/"], "task_body_sha256": "a" * 64},
        }
        comments[preset] = [self.trusted_comment(preset_claim)]

        def issue_comments(issue, repo=""):
            with state_lock:
                return list(reversed(comments.get(issue, [])))

        def post_comment(issue, body, repo=""):
            with state_lock:
                comments.setdefault(issue, []).append(
                    {"author": {"login": "github-actions[bot]"}, "body": body}
                )
            return True

        def get_body(issue, repo=""):
            return bodies[issue]

        def get_labels(issue, repo=""):
            with state_lock:
                return set(labels.get(issue, set()))

        def capture_recheck_snapshot():
            # Runs once inside the running-label barrier, at the release
            # moment: both running labels are durably written and neither
            # thread has rolled back yet, so both rechecks observe the same
            # consistent breach snapshot.
            nonlocal recheck_snapshot
            with state_lock:
                recheck_snapshot = frozenset(
                    issue for issue, issue_labels in labels.items()
                    if issue_labels & state_manager.ACTIVE_LABELS
                )

        running_barrier = threading.Barrier(2, action=capture_recheck_snapshot)

        def set_labels(issue, *new_labels, repo=""):
            is_running = state_manager.LABEL_RUNNING in new_labels
            with state_lock:
                labels[issue] = set(new_labels)
            if is_running:
                # Both running labels must be durably written before either
                # thread may continue into the post-label capacity recheck.
                running_barrier.wait(timeout=60)
            return True

        def active_numbers(repo=""):
            nonlocal active_read_count
            with state_lock:
                active_read_count += 1
                call = active_read_count
            if call <= 2:
                # Both threads' first authoritative precheck reads the same
                # K-1 snapshot: neither thread can write its running label
                # before both prechecks have returned.
                with state_lock:
                    snapshot = frozenset(
                        issue for issue, issue_labels in labels.items()
                        if issue_labels & state_manager.ACTIVE_LABELS
                    )
                precheck_barrier.wait(timeout=60)
                with state_lock:
                    precheck_snapshots.append(snapshot)
                return set(snapshot)
            with state_lock:
                snapshot = recheck_snapshot if recheck_snapshot is not None else frozenset(
                    issue for issue, issue_labels in labels.items()
                    if issue_labels & state_manager.ACTIVE_LABELS
                )
            with state_lock:
                recheck_snapshots.append(snapshot)
            return set(snapshot)

        def run_dispatch(issue):
            try:
                results[issue] = dispatcher.dispatch_ready(issue, f"worker:{issue}")
            except Exception as exc:  # noqa: BLE001 - surface thread failures
                failures.append(exc)

        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="acme/repo"), \
             mock.patch.object(dispatcher.sm, "get_issue_comments", side_effect=issue_comments), \
             mock.patch.object(dispatcher.sm, "comment_on_issue", side_effect=post_comment), \
             mock.patch.object(dispatcher.sm, "get_issue_body", side_effect=get_body), \
             mock.patch.object(dispatcher.sm, "get_issue_labels_checked", side_effect=get_labels), \
             mock.patch.object(dispatcher.sm, "set_labels", side_effect=set_labels), \
             mock.patch.object(
                 dispatcher.sm,
                 "get_active_capacity",
                 side_effect=lambda repo="": {"issues": active_numbers(repo), "plans": []},
             ), \
             mock.patch.object(dispatcher.sm, "has_open_issue_pr", return_value=False), \
             mock.patch.object(dispatcher, "_run_workflow", side_effect=lambda *args: workflow_calls.append(args) or True):
            threads = [
                threading.Thread(target=run_dispatch, args=(first,)),
                threading.Thread(target=run_dispatch, args=(second,)),
            ]
            for thread in threads:
                thread.start()
            for thread in threads:
                thread.join(timeout=90)
        self.assertFalse(failures)
        self.assertTrue(all(not thread.is_alive() for thread in threads))

        self.assertEqual(results[first]["dispatched"], False)
        self.assertEqual(results[second]["dispatched"], False)
        self.assertEqual(results[first]["reason"], "capacity_recheck_exceeded")
        self.assertEqual(results[second]["reason"], "capacity_recheck_exceeded")
        self.assertEqual(workflow_calls, [])
        self.assertEqual(precheck_snapshots, [frozenset({preset}), frozenset({preset})])
        self.assertTrue(
            all(len(snapshot) <= state_manager.MAX_ACTIVE for snapshot in precheck_snapshots)
        )
        self.assertEqual(
            recheck_snapshots,
            [frozenset({preset, first, second}), frozenset({preset, first, second})],
        )
        self.assertTrue(
            any(len(snapshot) > state_manager.MAX_ACTIVE for snapshot in recheck_snapshots)
        )
        with state_lock:
            final_active = {
                issue for issue, issue_labels in labels.items()
                if issue_labels & state_manager.ACTIVE_LABELS
            }
        self.assertEqual(final_active, {preset})
        self.assertLessEqual(len(final_active), state_manager.MAX_ACTIVE)
        for issue in (first, second):
            with state_lock:
                persisted = [json.loads(item["body"]) for item in comments[issue]]
            self.assertEqual(len(persisted), 2)
            claimed, rollback = persisted
            self.assertEqual(claimed["status"], "claimed")
            self.assertEqual(claimed["action"], "worker")
            self.assertEqual(rollback["status"], "failed")
            self.assertEqual(rollback["action"], "rollback")
            self.assertEqual(rollback["details"]["reason"], "capacity_recheck_exceeded")
            self.assertEqual(
                rollback["details"]["allowed_paths"],
                artifact_contract.parse_issue_scope(bodies[issue]),
            )
            self.assertEqual(
                rollback["details"]["task_body_sha256"],
                hashlib.sha256(bodies[issue].encode("utf-8")).hexdigest(),
            )
            self.assertRegex(rollback["details"]["claim_nonce"], r"^[0-9a-f]{32}$")
            self.assertEqual(rollback["details"]["target_label"], state_manager.LABEL_RUNNING)
            self.assertEqual(rollback["details"]["previous_labels"], [state_manager.LABEL_READY])
            self.assertEqual(
                [state["status"] for state in persisted],
                ["claimed", "failed"],
            )

    def test_verify_task_scope_binding_accepts_unchanged_body(self):
        body = '<!-- agent-orchestrator-scope:v1 {"allowed_paths":["src/"]} -->'
        claim = self.worker_claim(details=artifact_contract.build_issue_scope_binding(body))
        comments = [self.trusted_comment(claim)]
        with mock.patch.object(state_manager, "get_issue_comments", return_value=comments), \
             mock.patch.object(state_manager, "get_issue_body", return_value=body):
            ok, reason, observed = state_manager.verify_task_scope_binding(41, "acme/repo")
        self.assertTrue(ok)
        self.assertEqual(reason, "ok")
        self.assertEqual(observed["allowed_paths"], ["src/"])

    def test_verify_task_scope_binding_fails_closed_without_a_claim(self):
        with mock.patch.object(state_manager, "get_issue_comments", return_value=[]), \
             mock.patch.object(state_manager, "get_issue_body", side_effect=AssertionError("claim required")):
            ok, reason, observed = state_manager.verify_task_scope_binding(41, "acme/repo")
        self.assertFalse(ok)
        self.assertEqual(reason, "claim_scope_unavailable")
        self.assertIsNone(observed)

    def test_read_task_scope_binding_reads_the_body_exactly_once(self):
        body = '<!-- agent-orchestrator-scope:v1 {"allowed_paths":["src/"]} -->'
        calls = []

        def get_body(issue, repo=""):
            calls.append(issue)
            return body

        with mock.patch.object(state_manager, "get_issue_body", side_effect=get_body):
            ok, binding = state_manager.read_task_scope_binding(41, "acme/repo")
        self.assertTrue(ok)
        self.assertEqual(binding["allowed_paths"], ["src/"])
        self.assertEqual(binding["task_body_sha256"], hashlib.sha256(body.encode("utf-8")).hexdigest())
        self.assertEqual(calls, [41])

    def test_read_task_scope_binding_fails_closed_on_unavailable_body(self):
        with mock.patch.object(state_manager, "get_issue_body", return_value=None):
            ok, reason = state_manager.read_task_scope_binding(41, "acme/repo")
        self.assertFalse(ok)
        self.assertEqual(reason, "task_body_unavailable")

    def test_validate_task_scope_reuses_read_task_scope_binding(self):
        body = '<!-- agent-orchestrator-scope:v1 {"allowed_paths":["src/"]} -->'
        with mock.patch.object(state_manager, "get_issue_body", return_value=body):
            valid, scope = state_manager.validate_task_scope(41, "acme/repo")
        self.assertEqual((valid, scope), (True, ["src/"]))

    def test_claim_scope_ignores_untrusted_authors(self):
        claim = self.worker_claim()
        comments = [{"author": {"login": "some-user"}, "body": json.dumps(claim)}]
        with mock.patch.object(state_manager, "get_issue_comments", return_value=comments):
            self.assertIsNone(state_manager.read_worker_claim_scope(41, "acme/repo"))
            self.assertIsNone(state_manager.get_active_issue_scopes({41}, "acme/repo"))

    def test_claim_scope_rejects_wrong_identity_fields(self):
        variants = [
            {"action": "review"},
            {"action": "repair"},
            {"status": "failed"},
            {"status": "rejected"},
            {"status": "rollback"},
            {"version": 2},
            {"issue_number": 42},
        ]
        for mutation in variants:
            claim = self.worker_claim(**mutation)
            comments = [self.trusted_comment(claim)]
            with self.subTest(mutation=mutation):
                with mock.patch.object(state_manager, "get_issue_comments", return_value=comments):
                    self.assertIsNone(state_manager.read_worker_claim_scope(41, "acme/repo"))

    def test_claim_scope_rejects_invalid_binding_fields(self):
        invalid_digests = ["A" * 64, "a" * 63, "", "g" * 64, None, 5]
        invalid_paths = [[], ["../src/"], ["src/", "src/"], ["src/*"]]
        for digest in invalid_digests:
            claim = self.worker_claim(details={"allowed_paths": ["src/"], "task_body_sha256": digest})
            comments = [self.trusted_comment(claim)]
            with self.subTest(digest=digest):
                with mock.patch.object(state_manager, "get_issue_comments", return_value=comments):
                    self.assertIsNone(state_manager.read_worker_claim_scope(41, "acme/repo"))
        for paths in invalid_paths:
            claim = self.worker_claim(details={"allowed_paths": paths, "task_body_sha256": "a" * 64})
            comments = [self.trusted_comment(claim)]
            with self.subTest(paths=paths):
                with mock.patch.object(state_manager, "get_issue_comments", return_value=comments):
                    self.assertIsNone(state_manager.read_worker_claim_scope(41, "acme/repo"))

    def test_claim_scope_accepts_details_with_extra_dispatch_fields(self):
        claim = self.worker_claim(details={
            "previous_labels": [state_manager.LABEL_READY],
            "target_label": state_manager.LABEL_RUNNING,
            "issue_number": 41,
            "allowed_paths": ["scripts/"],
            "task_body_sha256": "a" * 64,
        })
        comments = [self.trusted_comment(claim)]
        with mock.patch.object(state_manager, "get_issue_comments", return_value=comments):
            scopes = state_manager.get_active_issue_scopes({41}, "acme/repo")
        self.assertEqual(scopes, {41: ["scripts/"]})

    def test_newer_terminal_worker_state_blocks_older_dispatched_scope(self):
        older = self.worker_claim()
        newer = self.worker_claim(status="failed", details={"reason": "reconciled"})
        comments = [self.trusted_comment(newer), self.trusted_comment(older)]
        with mock.patch.object(state_manager, "get_issue_comments", return_value=comments):
            self.assertIsNone(state_manager.read_worker_claim_scope(41, "acme/repo"))
            self.assertIsNone(state_manager.get_active_issue_scopes({41}, "acme/repo"))

    def test_newer_claimed_scope_supersedes_older_terminal_worker_state(self):
        older = self.worker_claim(status="failed", details={"reason": "reconciled"})
        newer = self.worker_claim(details={"allowed_paths": ["docs/"], "task_body_sha256": "b" * 64})
        comments = [self.trusted_comment(newer), self.trusted_comment(older)]
        with mock.patch.object(state_manager, "get_issue_comments", return_value=comments):
            scopes = state_manager.get_active_issue_scopes({41}, "acme/repo")
        self.assertEqual(scopes, {41: ["docs/"]})

    def test_newer_review_and_repair_states_still_find_worker_binding(self):
        repair = {
            "kind": "agent-orchestrator-dispatch-state", "version": 1, "issue_number": 41,
            "dispatch_id": "repair:207", "action": "repair", "status": "dispatched",
            "details": {"pr_number": 207, "issue_number": 41, "head_sha": "a" * 40},
        }
        review = {
            "kind": "agent-orchestrator-dispatch-state", "version": 1, "issue_number": 41,
            "dispatch_id": "review:207", "action": "review", "status": "dispatched",
            "details": {"pr_number": 207, "issue_number": 41, "head_sha": "a" * 40},
        }
        worker = self.worker_claim()
        comments = [self.trusted_comment(repair), self.trusted_comment(review), self.trusted_comment(worker)]
        with mock.patch.object(state_manager, "get_issue_comments", return_value=comments):
            claim = state_manager.read_worker_claim_scope(41, "acme/repo")
        self.assertEqual(claim, {"allowed_paths": ["src/"], "task_body_sha256": "a" * 64})

    def test_newer_malformed_worker_state_blocks_older_dispatched_scope(self):
        older = self.worker_claim()
        truncated = {
            "author": {"login": "github-actions[bot]"},
            "body": '{"kind": "agent-orchestrator-dispatch-state", "status": "dispatched", "details": {"allowed_paths": ["src/"]',
        }
        comments = [truncated, self.trusted_comment(older)]
        with mock.patch.object(state_manager, "get_issue_comments", return_value=comments):
            self.assertIsNone(state_manager.read_worker_claim_scope(41, "acme/repo"))
            self.assertIsNone(state_manager.get_active_issue_scopes({41}, "acme/repo"))

    def test_newer_wrong_version_worker_state_blocks_older_dispatched_scope(self):
        older = self.worker_claim()
        newer = self.worker_claim(version=2)
        comments = [self.trusted_comment(newer), self.trusted_comment(older)]
        with mock.patch.object(state_manager, "get_issue_comments", return_value=comments):
            self.assertIsNone(state_manager.read_worker_claim_scope(41, "acme/repo"))
            self.assertIsNone(state_manager.get_active_issue_scopes({41}, "acme/repo"))

    def test_newer_unrelated_issue_state_does_not_shadow_claim(self):
        unrelated = self.worker_claim(issue_number=42, dispatch_id="worker:42")
        claim = self.worker_claim()
        comments = [self.trusted_comment(unrelated), self.trusted_comment(claim)]
        with mock.patch.object(state_manager, "get_issue_comments", return_value=comments):
            observed = state_manager.read_worker_claim_scope(41, "acme/repo")
        self.assertEqual(observed, {"allowed_paths": ["src/"], "task_body_sha256": "a" * 64})

    def test_other_state_documents_quoting_the_marker_do_not_shadow_claim(self):
        review = {
            "kind": "agent-orchestrator-review-state", "version": 2, "issue_number": 41,
            "pr_number": 207, "head_sha": "a" * 40, "verdict": "BLOCKED",
            "summary": 'quotes issue text "agent-orchestrator-dispatch-state"',
            "blockers": [], "major_notes": [], "minor_notes": [], "artifact_sha256": "",
        }
        claim = self.worker_claim()
        comments = [self.trusted_comment(review), self.trusted_comment(claim)]
        with mock.patch.object(state_manager, "get_issue_comments", return_value=comments):
            observed = state_manager.read_worker_claim_scope(41, "acme/repo")
        self.assertEqual(observed, {"allowed_paths": ["src/"], "task_body_sha256": "a" * 64})

    def test_active_scope_conflict_uses_claim_bound_scope_after_body_change(self):
        claim = self.worker_claim(details={
            "allowed_paths": ["scripts/agent-control/"],
            "task_body_sha256": "a" * 64,
        })
        comments = [self.trusted_comment(claim)]
        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="acme/repo"), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", return_value=None), \
             mock.patch.object(dispatcher.sm, "get_issue_labels_checked", return_value={state_manager.LABEL_READY}), \
             mock.patch.object(dispatcher.sm, "read_task_scope_binding", return_value=(True, {
                 "allowed_paths": ["scripts/"], "task_body_sha256": "b" * 64,
             })), \
             mock.patch.object(dispatcher.sm, "check_dependencies_complete", return_value=(True, None)), \
             mock.patch.object(dispatcher.sm, "has_open_issue_pr", return_value=False), \
             mock.patch.object(dispatcher.sm, "get_active_capacity", return_value={"issues": {41}, "plans": []}), \
             mock.patch.object(dispatcher.sm, "get_issue_comments", return_value=comments), \
             mock.patch.object(dispatcher.sm, "get_issue_body", side_effect=AssertionError("mutable body read")), \
             mock.patch.object(dispatcher, "_run_workflow") as workflow:
            result = dispatcher.dispatch_ready(77, "worker:77")
        self.assertFalse(result["dispatched"])
        self.assertEqual(result["reason"], "scope_conflict:41")
        workflow.assert_not_called()

    def test_rollback_preserves_the_original_claim_binding(self):
        claim = self.worker_claim(status="claimed", details={
            "previous_labels": [state_manager.LABEL_READY],
            "target_label": state_manager.LABEL_RUNNING,
            "issue_number": 41,
            "allowed_paths": ["src/"],
            "task_body_sha256": "a" * 64,
        })
        with mock.patch.object(dispatcher.sm, "set_labels", return_value=True), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", return_value=claim), \
             mock.patch.object(dispatcher.sm, "record_dispatch_state", return_value=True) as record:
            result = dispatcher._rollback(41, "worker:41", [state_manager.LABEL_READY], "workflow_dispatch_failed")
        self.assertTrue(result)
        details = record.call_args[0][4]
        self.assertEqual(details["allowed_paths"], ["src/"])
        self.assertEqual(details["task_body_sha256"], "a" * 64)
        self.assertEqual(details["reason"], "workflow_dispatch_failed")
        self.assertEqual(record.call_args[0][1:3], ("worker:41", "rollback"))
        self.assertEqual(record.call_args[0][3], "failed")

    def test_rollback_fails_closed_on_malformed_previous_details(self):
        malformed = self.worker_claim(status="claimed", details="not-an-object")
        with mock.patch.object(dispatcher.sm, "set_labels", return_value=True), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", return_value=malformed), \
             mock.patch.object(dispatcher.sm, "record_dispatch_state", return_value=True) as record:
            result = dispatcher._rollback(41, "worker:41", [state_manager.LABEL_READY], "workflow_dispatch_failed")
        self.assertTrue(result)
        details = record.call_args[0][4]
        self.assertEqual(details, {"reason": "workflow_dispatch_failed"})

    def test_rollback_label_restore_failure_returns_false_without_terminal_record(self):
        with mock.patch.object(dispatcher.sm, "set_labels", return_value=False), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", return_value=self.worker_claim(status="claimed")), \
             mock.patch.object(dispatcher.sm, "record_dispatch_state", return_value=True) as record:
            result = dispatcher._rollback(41, "worker:41", [state_manager.LABEL_READY], "workflow_dispatch_failed")
        self.assertFalse(result)
        record.assert_not_called()

    def test_rollback_restore_failure_keeps_claim_for_reconcile_and_never_redispatch(self):
        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="acme/repo"), \
             mock.patch.object(dispatcher, "_claim", return_value=(True, [state_manager.LABEL_READY], "claimed")), \
              mock.patch.object(dispatcher.sm, "get_active_capacity", return_value={"issues": set(), "plans": []}), \
              mock.patch.object(dispatcher.sm, "set_labels", return_value=False), \
             mock.patch.object(dispatcher.sm, "record_dispatch_state", return_value=True) as record, \
             mock.patch.object(dispatcher, "_run_workflow", return_value=False) as workflow:
            result = dispatcher.dispatch_ready(77, "worker:77")
        self.assertFalse(result["dispatched"])
        self.assertEqual(result["reason"], "workflow_dispatch_failed_rollback_failed")
        record.assert_not_called()
        workflow.assert_called_once()

    def test_record_dispatched_fails_closed_on_malformed_previous_details(self):
        malformed = self.worker_claim(status="claimed", details=["not", "a", "dict"])
        with mock.patch.object(dispatcher.sm, "read_dispatch_state", return_value=malformed), \
             mock.patch.object(dispatcher.sm, "record_dispatch_state", return_value=True) as record:
            result = dispatcher._record_dispatched(41, "worker:41", "worker", {"workflow": "agent-worker.yml"})
        self.assertTrue(result)
        details = record.call_args[0][4]
        self.assertEqual(details, {"workflow": "agent-worker.yml"})

    def test_reconcile_preserves_the_original_claim_binding(self):
        claim = self.worker_claim(status="claimed", details={
            "previous_labels": [state_manager.LABEL_READY],
            "target_label": state_manager.LABEL_RUNNING,
            "issue_number": 41,
            "allowed_paths": ["src/"],
            "task_body_sha256": "a" * 64,
        })
        with mock.patch.object(state_manager, "read_dispatch_state", return_value=claim), \
             mock.patch.object(state_manager, "release_failed_capacity", return_value=(True, "released")), \
             mock.patch.object(state_manager, "record_dispatch_state", return_value=True) as record:
            result = state_manager.reconcile_claimed_dispatch(41, "worker:41", "workflow_cancelled")
        self.assertEqual(result, (True, "released"))
        details = record.call_args[0][4]
        self.assertEqual(details["allowed_paths"], ["src/"])
        self.assertEqual(details["task_body_sha256"], "a" * 64)
        self.assertEqual(details["reason"], "workflow_cancelled")

    def test_duplicate_delivery_dispatches_once(self):
        labels = {state_manager.LABEL_READY}
        recorded = {}
        workflow_calls = []

        def set_labels(_issue, *new_labels, repo=""):
            labels.clear()
            labels.update(new_labels)
            return True

        def record(_issue, dispatch_id, action, status, details=None, repo=""):
            recorded[dispatch_id] = {
                "kind": "agent-orchestrator-dispatch-state",
                "issue_number": _issue, "status": status, "action": action,
                "details": details or {},
            }
            return True

        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="repo"), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", side_effect=lambda _i, key, _r: recorded.get(key)), \
             mock.patch.object(dispatcher.sm, "get_issue_labels_checked", return_value=labels), \
             mock.patch.object(dispatcher.sm, "check_dependencies_complete", return_value=(True, None)), \
             mock.patch.object(dispatcher.sm, "has_open_issue_pr", return_value=False), \
             mock.patch.object(dispatcher.sm, "read_task_scope_binding", return_value=(True, {
                 "allowed_paths": ["src/"], "task_body_sha256": "a" * 64,
             })), \
              mock.patch.object(dispatcher.sm, "get_active_capacity", return_value={"issues": set(), "plans": []}), \
              mock.patch.object(dispatcher.sm, "set_labels", side_effect=set_labels), \
             mock.patch.object(dispatcher.sm, "record_dispatch_state", side_effect=record), \
             mock.patch.object(dispatcher, "_run_workflow", side_effect=lambda *args: workflow_calls.append(args) or True):
            first = dispatcher.dispatch_ready(12, "worker:12")
            second = dispatcher.dispatch_ready(12, "worker:12")
        self.assertTrue(first["dispatched"])
        self.assertTrue(second["dispatched"])
        self.assertEqual(len(workflow_calls), 1)
        self.assertEqual(recorded["worker:12"]["details"]["allowed_paths"], ["src/"])
        self.assertEqual(recorded["worker:12"]["details"]["task_body_sha256"], "a" * 64)

    def test_claimed_dispatch_is_not_reissued_when_final_audit_write_failed(self):
        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="repo"), \
             mock.patch.object(
                 dispatcher.sm,
                 "read_dispatch_state",
                 return_value={
                     "kind": "agent-orchestrator-dispatch-state",
                     "issue_number": 12,
                     "action": "worker",
                     "status": "claimed",
                     "details": {
                         "issue_number": 12,
                         "target_label": state_manager.LABEL_RUNNING,
                     },
                 },
             ), \
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
                "    if 'draft=true' not in args:\n"
                "        raise SystemExit('POST missing draft flag: ' + repr(args))\n"
                "    url = 'https://github.com/acme/repo/pull/123'\n"
                "    value = {'number':123,'html_url':url,'url':url,'state':'OPEN','baseRefName':'main','headRefName':'agent/issue-42','headRefOid':os.environ['PR_SHA'],'isDraft':True,'body':'Closes #42\\n\\n<!-- agent-orchestrator-binding: {\\\"issue_number\\\": 42, \\\"branch\\\": \\\"agent/issue-42\\\"} -->'}\n"
                "    json.dump(value, open(state, 'w')); print(json.dumps({'number':123,'html_url':url}))\n"
                "elif args[:2] == ['api', '--method'] and 'PATCH' in args:\n"
                "    if 'draft=true' in args:\n"
                "        raise SystemExit('PATCH must not convert draft state: ' + repr(args))\n"
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
        sha = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
        ).strip()
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
                # This fixture does not model a dispatch-bound workflow head.
                # An inherited pull-request merge SHA must not impersonate one.
                "GITHUB_SHA": "",
                "GITHUB_RUN_ID": "",
                "AGENT_CONTEXT_EXPECTED_HEAD_SHA": sha,
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
             mock.patch.object(state_manager, "resolve_live_review_binding", return_value=(True, "ok", {"head_sha": "a" * 40, "base_sha": "c" * 40, "reviewed_range": f"{'c' * 40}...{'a' * 40}"})), \
             mock.patch.object(prompt_builder, "_gh", return_value="x" * (prompt_builder.MAX_REVIEW_DIFF_CHARS + 1)):
            with self.assertRaisesRegex(ValueError, "complete PR diff exceeds"):
                prompt_builder.build_review_prompt(207, "a" * 40)

    def test_capacity_full_is_nonterminal(self):
        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="repo"), \
             mock.patch.object(dispatcher.sm, "verify_review_issue_pr_binding", return_value=(True, "ok", {"head_sha": "a" * 40})), \
             mock.patch.object(dispatcher.sm, "get_active_capacity", return_value={"issues": {1, 2}, "plans": []}):
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
             mock.patch.object(dispatcher.sm, "read_task_scope_binding", return_value=(False, "wildcard")), \
             mock.patch.object(dispatcher.sm, "record_dispatch_state", return_value=True) as record, \
             mock.patch.object(dispatcher, "_run_workflow") as workflow:
            result = dispatcher.dispatch_ready(77, "worker:77")
        self.assertFalse(result["dispatched"])
        self.assertIn("invalid_scope", result["reason"])
        record.assert_called_once()
        workflow.assert_not_called()

    def test_missing_binding_blocks_claim_label_and_dispatch(self):
        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="acme/repo"), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", return_value=None), \
             mock.patch.object(dispatcher.sm, "get_issue_labels_checked", return_value={state_manager.LABEL_READY}), \
             mock.patch.object(dispatcher.sm, "read_task_scope_binding", return_value=(False, "task_body_unavailable")), \
             mock.patch.object(dispatcher.sm, "record_dispatch_state", return_value=True) as record, \
             mock.patch.object(dispatcher.sm, "set_labels") as set_labels, \
             mock.patch.object(dispatcher, "_run_workflow") as workflow:
            result = dispatcher.dispatch_ready(77, "worker:77")
        self.assertFalse(result["dispatched"])
        self.assertEqual(result["reason"], "invalid_scope:task_body_unavailable")
        self.assertEqual(record.call_args.args[3], "rejected")
        set_labels.assert_not_called()
        workflow.assert_not_called()

    def test_dependency_is_rechecked_by_the_serialized_claim(self):
        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="acme/repo"), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", return_value=None), \
             mock.patch.object(dispatcher.sm, "get_issue_labels_checked", return_value={state_manager.LABEL_READY}), \
             mock.patch.object(dispatcher.sm, "read_task_scope_binding", return_value=(True, {
                 "allowed_paths": ["src/"], "task_body_sha256": "a" * 64,
             })), \
             mock.patch.object(dispatcher.sm, "check_dependencies_complete", return_value=(False, 41)), \
             mock.patch.object(dispatcher, "_run_workflow") as workflow:
            result = dispatcher.dispatch_ready(77, "worker:77")
        self.assertFalse(result["dispatched"])
        self.assertEqual(result["reason"], "dependencies_not_ready:41")
        workflow.assert_not_called()

    def test_serialized_claim_rejects_scope_conflict_with_active_issue(self):
        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="acme/repo"), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", return_value=None), \
             mock.patch.object(dispatcher.sm, "get_issue_labels_checked", return_value={state_manager.LABEL_READY}), \
             mock.patch.object(dispatcher.sm, "read_task_scope_binding", return_value=(True, {
                 "allowed_paths": ["scripts/"], "task_body_sha256": "a" * 64,
             })), \
             mock.patch.object(dispatcher.sm, "check_dependencies_complete", return_value=(True, None)), \
             mock.patch.object(dispatcher.sm, "has_open_issue_pr", return_value=False), \
              mock.patch.object(dispatcher.sm, "get_active_capacity", return_value={"issues": {41}, "plans": []}), \
              mock.patch.object(dispatcher.sm, "get_active_issue_scopes", return_value={41: ["scripts/agent-control/"]}, create=True), \
             mock.patch.object(dispatcher, "_run_workflow") as workflow:
            result = dispatcher.dispatch_ready(77, "worker:77")

        self.assertFalse(result["dispatched"])
        self.assertEqual(result["reason"], "scope_conflict:41")
        workflow.assert_not_called()

    def test_failed_dispatch_reports_failed_rollback(self):
        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_claim", return_value=(True, [state_manager.LABEL_READY], "claimed")), \
             mock.patch.object(dispatcher.sm, "get_active_capacity", return_value={"issues": {77}, "plans": []}), \
             mock.patch.object(dispatcher, "_run_workflow", return_value=False), \
             mock.patch.object(dispatcher, "_rollback", return_value=False):
            result = dispatcher.dispatch_ready(77, "worker:77")
        self.assertEqual(result["reason"], "workflow_dispatch_failed_rollback_failed")

    def test_dispatch_claim_is_persisted_before_label_mutation(self):
        calls = []
        recorded_details = []
        labels = {state_manager.LABEL_READY}

        def record(*args, **kwargs):
            calls.append("state")
            recorded_details.append(args[4])
            return True

        def set_labels(*args, **kwargs):
            calls.append("label")
            labels.clear()
            labels.add(state_manager.LABEL_RUNNING)
            return True

        with mock.patch.object(dispatcher, "_repo", return_value="repo"), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", return_value=None), \
             mock.patch.object(dispatcher.sm, "get_issue_labels_checked", return_value=labels), \
             mock.patch.object(dispatcher.sm, "check_dependencies_complete", return_value=(True, None)), \
             mock.patch.object(dispatcher.sm, "has_open_issue_pr", return_value=False), \
             mock.patch.object(dispatcher.sm, "read_task_scope_binding", return_value=(True, {
                 "allowed_paths": ["src/"], "task_body_sha256": "a" * 64,
             }), create=True), \
              mock.patch.object(dispatcher.sm, "get_active_capacity", return_value={"issues": set(), "plans": []}), \
              mock.patch.object(dispatcher.sm, "record_dispatch_state", side_effect=record), \
             mock.patch.object(dispatcher.sm, "set_labels", side_effect=set_labels):
            claimed = dispatcher._claim(12, state_manager.LABEL_RUNNING, "worker:12", "worker", {"issue_number": 12})
        self.assertTrue(claimed[0])
        self.assertEqual(calls[:2], ["state", "label"])
        self.assertEqual(recorded_details[0]["allowed_paths"], ["src/"])
        self.assertEqual(recorded_details[0]["task_body_sha256"], "a" * 64)

    def test_reselection_stop_preserves_identity_unavailable_audit_fields(self):
        replacement = {
            "status": "ci_control_stopped",
            "workflow_run_id": 0,
            "observed_run": {"status": "unknown", "dispatch_nonce": "nonce-1"},
            "reason": "ci_control_stopped:fallback_run_identity_missing",
            "run_identity": "unavailable",
            "dispatch_nonce": "nonce-1",
        }
        with mock.patch.object(ci_handler, "_record_ci_terminal", return_value={}) as record:
            ci_handler._process_reselection_result(42, 207, "a" * 40, replacement)
        self.assertEqual(record.call_args.kwargs["extra"], {
            "run_identity": "unavailable",
            "dispatch_nonce": "nonce-1",
        })

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
                dispatch_states[dispatch_id] = {
                    "kind": "agent-orchestrator-dispatch-state",
                    "issue_number": _issue,
                    "action": action, "status": status, "details": details,
                }
                return True
            return False

        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="repo"), \
             mock.patch.object(dispatcher.sm, "verify_review_issue_pr_binding", return_value=(True, "ok", {"head_sha": "a" * 40})), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", side_effect=read_dispatch), \
             mock.patch.object(dispatcher.sm, "get_issue_labels_checked", return_value=labels), \
              mock.patch.object(dispatcher.sm, "get_active_capacity", return_value={"issues": {42}, "plans": []}), \
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

    def test_review_binding_failure_prevents_claim_and_workflow_dispatch(self):
        for reason in (
            "expected_head_invalid",
            "head_mismatch",
            "live_metadata_unavailable",
            "live_base_unavailable",
        ):
            with self.subTest(reason=reason), \
                 mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
                 mock.patch.object(dispatcher, "_repo", return_value="acme/repo"), \
                 mock.patch.object(dispatcher.sm, "verify_review_issue_pr_binding", return_value=(False, reason, None)), \
                 mock.patch.object(dispatcher, "_claim") as claim, \
                 mock.patch.object(dispatcher, "_run_workflow") as workflow:
                result = dispatcher.dispatch_review(207, 42, "a" * 40)
            self.assertFalse(result["dispatched"])
            self.assertEqual(result["reason"], f"binding_rejected:{reason}")
            claim.assert_not_called()
            workflow.assert_not_called()

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
                dispatch_states[dispatch_id] = {
                    "kind": "agent-orchestrator-dispatch-state",
                    "issue_number": _issue,
                    "action": action, "status": status, "details": details,
                }
                return True
            return False

        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="repo"), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", side_effect=read_dispatch), \
             mock.patch.object(dispatcher.sm, "get_issue_labels_checked", return_value=labels), \
              mock.patch.object(dispatcher.sm, "get_active_capacity", return_value={"issues": {42}, "plans": []}), \
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
        details = dispatch_states["repair:207:" + "a" * 40 + ":9001:1"]["details"]
        self.assertEqual(
            {key: details[key] for key in ("pr_number", "issue_number", "head_sha", "repair_count", "ci_run_id")},
            {
                "pr_number": 207,
                "issue_number": 42,
                "head_sha": "a" * 40,
                "repair_count": "1",
                "ci_run_id": "9001",
            },
        )

    def test_merge_dispatch_audit_failure_retains_claim_and_blocks_duplicate(self):
        dispatch_states = {}
        workflow_calls = []

        def read_dispatch(_issue, dispatch_id, _repo):
            return dispatch_states.get(dispatch_id)

        def record_dispatch(_issue, dispatch_id, action, status, details=None, repo=""):
            if status == "claimed":
                dispatch_states[dispatch_id] = {
                    "kind": "agent-orchestrator-dispatch-state",
                    "issue_number": _issue,
                    "action": action, "status": status, "details": details,
                }
                return True
            return False

        with mock.patch.object(dispatcher.control_state, "require_auto_merge", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="repo"), \
             mock.patch.object(dispatcher.sm, "verify_review_issue_pr_binding", return_value=(True, "ok", {"head_sha": "a" * 40})), \
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
        prior_review = {
            "kind": "agent-orchestrator-review-state",
            "version": 3,
            "issue_number": 42,
            "pr_number": 207,
            "head_sha": "a" * 40,
            "verdict": "BLOCKED",
            "summary": "blocked",
            "base_sha": "c" * 40,
            "reviewed_range": f"{'c' * 40}...{'a' * 40}",
            "review_mode": "full",
            "review_round": 1,
            "findings": [{
                "id": "B-1",
                "axis": "correctness",
                "evidence": "x",
                "severity": "blocker",
                "disposition": "block_current_head",
                "scope_relation": "in_packet",
                "origin_head": "a" * 40,
                "acceptance_condition": "fixed",
                "status": "open",
            }],
            "open_blocker_ids": ["B-1"],
            "deferred_note_ids": [],
            "decision_required_ids": [],
            "autonomous_repairs_remaining": 1,
            "stop_reason": "",
        }
        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="acme/repo"), \
             mock.patch.object(dispatcher.sm, "get_issue_labels_checked", return_value={state_manager.LABEL_REVIEW_BLOCKED}), \
             mock.patch.object(dispatcher.sm, "read_worker_state", return_value=worker), \
             mock.patch.object(dispatcher.sm, "verify_review_issue_pr_binding", return_value=(True, "ok", {"head_sha": "a" * 40})), \
             mock.patch.object(dispatcher.sm, "read_review_state", return_value=prior_review), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", return_value=None), \
              mock.patch.object(dispatcher.sm, "get_active_capacity", return_value={"issues": set(), "plans": []}), \
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

    def test_retry_review_audit_failure_retains_claim_and_blocks_duplicate(self):
        worker = {
            "pr_number": 207,
            "head_sha": "a" * 40,
            "extra": {"branch": "agent/issue-42"},
        }
        prior_review = {
            "kind": "agent-orchestrator-review-state",
            "version": 3,
            "issue_number": 42,
            "pr_number": 207,
            "head_sha": "a" * 40,
            "verdict": "BLOCKED",
            "summary": "blocked",
            "base_sha": "c" * 40,
            "reviewed_range": f"{'c' * 40}...{'a' * 40}",
            "review_mode": "full",
            "review_round": 1,
            "findings": [],
            "open_blocker_ids": [],
            "deferred_note_ids": [],
            "decision_required_ids": [],
            "autonomous_repairs_remaining": 1,
            "stop_reason": "",
        }
        dispatch_state = {}
        workflow_calls = []

        def read_dispatch(_issue, _dispatch_id, _repo):
            return dispatch_state or None

        def record_dispatch(_issue, _dispatch_id, action, status, details=None, repo=""):
            if status == "claimed":
                dispatch_state.update({
                    "kind": "agent-orchestrator-dispatch-state",
                    "issue_number": 42,
                    "action": action,
                    "status": status,
                    "details": details,
                })
                return True
            return False

        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="acme/repo"), \
             mock.patch.object(dispatcher.sm, "get_issue_labels_checked", return_value={state_manager.LABEL_REVIEW_BLOCKED}), \
             mock.patch.object(dispatcher.sm, "read_worker_state", return_value=worker), \
             mock.patch.object(dispatcher.sm, "verify_review_issue_pr_binding", return_value=(True, "ok", {"head_sha": "a" * 40})), \
             mock.patch.object(dispatcher.sm, "read_review_state", return_value=prior_review), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", side_effect=read_dispatch), \
              mock.patch.object(dispatcher.sm, "get_active_capacity", return_value={"issues": set(), "plans": []}), \
              mock.patch.object(dispatcher.sm, "set_labels", return_value=True), \
             mock.patch.object(dispatcher.sm, "record_dispatch_state", side_effect=record_dispatch), \
             mock.patch.object(dispatcher, "_run_workflow", side_effect=lambda *args: workflow_calls.append(args) or True):
            first = dispatcher.retry_review(42)
            second = dispatcher.retry_review(42)

        self.assertEqual(first["reason"], "dispatch_state_failed_capacity_retained")
        self.assertEqual(second["reason"], "dispatch_in_flight")
        self.assertEqual(len(workflow_calls), 1)

    def test_worker_audit_failure_retains_claim_and_blocks_duplicate(self):
        labels = {state_manager.LABEL_READY}
        dispatch_state = {}
        workflow_calls = []

        def read_dispatch(_issue, _dispatch_id, _repo):
            return dispatch_state or None

        def set_labels(_issue, *new_labels, repo=""):
            labels.clear()
            labels.update(new_labels)
            return True

        def record_dispatch(_issue, _dispatch_id, action, status, details=None, repo=""):
            if status == "claimed":
                dispatch_state.update({
                    "kind": "agent-orchestrator-dispatch-state",
                    "issue_number": _issue,
                    "action": action,
                    "status": status,
                    "details": details,
                })
                return True
            return False

        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="repo"), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", side_effect=read_dispatch), \
             mock.patch.object(dispatcher.sm, "get_issue_labels_checked", return_value=labels), \
             mock.patch.object(dispatcher.sm, "check_dependencies_complete", return_value=(True, None)), \
             mock.patch.object(dispatcher.sm, "has_open_issue_pr", return_value=False), \
             mock.patch.object(dispatcher.sm, "read_task_scope_binding", return_value=(True, {
                 "allowed_paths": ["src/"], "task_body_sha256": "a" * 64,
             })), \
              mock.patch.object(dispatcher.sm, "get_active_capacity", return_value={"issues": set(), "plans": []}), \
              mock.patch.object(dispatcher.sm, "set_labels", side_effect=set_labels), \
             mock.patch.object(dispatcher.sm, "record_dispatch_state", side_effect=record_dispatch), \
             mock.patch.object(dispatcher, "_run_workflow", side_effect=lambda *args: workflow_calls.append(args) or True):
            first = dispatcher.dispatch_ready(42, "worker:42")
            second = dispatcher.dispatch_ready(42, "worker:42")

        self.assertEqual(first["reason"], "dispatch_state_failed_capacity_retained")
        self.assertEqual(second["reason"], "dispatch_in_flight")
        self.assertEqual(len(workflow_calls), 1)

    def test_rejected_worker_terminates_claim_and_preserves_binding(self):
        claim = self.worker_claim(details={
            "previous_labels": [state_manager.LABEL_READY],
            "target_label": state_manager.LABEL_RUNNING,
            "issue_number": 41,
            "allowed_paths": ["src/"],
            "task_body_sha256": "a" * 64,
            "claim_nonce": "a" * 32,
        })
        recorded = []

        def record_dispatch(_issue, _dispatch_id, action, status, details=None, repo=""):
            recorded.append((_dispatch_id, action, status, details))
            return True

        comments = [self.trusted_comment(claim)]
        with mock.patch.object(state_manager, "release_failed_capacity", return_value=(True, "released")), \
             mock.patch.object(state_manager, "get_issue_comments", return_value=comments), \
             mock.patch.object(state_manager, "comment_on_issue", return_value=True), \
             mock.patch.object(state_manager, "record_dispatch_state", side_effect=record_dispatch):
            ok, reason = state_manager.release_rejected_worker(
                41, "true", "success", "false", "acme/repo", 123, "invalid_issue_scope",
                "worker:41", "a" * 32,
            )
        self.assertTrue(ok, reason)
        self.assertEqual(reason, "released")
        self.assertEqual(len(recorded), 1)
        dispatch_id, action, status, details = recorded[-1]
        self.assertEqual(dispatch_id, "worker:41")
        self.assertEqual((action, status), ("worker", "failed"))
        self.assertEqual(details["allowed_paths"], ["src/"])
        self.assertEqual(details["task_body_sha256"], "a" * 64)
        self.assertEqual(details["claim_nonce"], "a" * 32)
        self.assertEqual(details["reason"], "invalid_issue_scope")
        self.assertNotIn("capacity_release", details)
        terminal = self.trusted_comment({
            "kind": "agent-orchestrator-dispatch-state", "version": 1,
            "issue_number": 41, "dispatch_id": "worker:41", "action": "worker",
            "status": "failed", "details": details,
        })
        with mock.patch.object(state_manager, "get_issue_comments", return_value=[terminal, self.trusted_comment(claim)]):
            self.assertIsNone(state_manager.read_worker_claim_scope(41, "acme/repo"))

    def test_rejected_worker_release_failure_leaves_terminal_durable_for_retry(self):
        claim = self.worker_claim()
        comments = [self.trusted_comment(claim)]
        recorded = []

        def record_dispatch(_issue, _dispatch_id, action, status, details=None, repo=""):
            recorded.append((_dispatch_id, action, status))
            return True

        with mock.patch.object(state_manager, "release_failed_capacity", return_value=(False, "active_state_mismatch")), \
             mock.patch.object(state_manager, "get_issue_comments", return_value=comments), \
             mock.patch.object(state_manager, "record_dispatch_state", side_effect=record_dispatch):
            ok, reason = state_manager.release_rejected_worker(
                41, "true", "success", "false", "acme/repo",
                rejection_reason="invalid_issue_scope", dispatch_id="worker:41",
                claim_nonce="a" * 32,
            )
        self.assertFalse(ok)
        self.assertEqual(reason, "active_state_mismatch")
        self.assertEqual(recorded, [("worker:41", "worker", "failed")])

    def test_rejected_worker_retry_after_terminal_before_release_crash_is_idempotent(self):
        claim = self.worker_claim()
        terminal = self.worker_claim(status="failed", details={
            "allowed_paths": ["src/"],
            "task_body_sha256": "a" * 64,
            "claim_nonce": "a" * 32,
            "reason": "invalid_issue_scope",
        })
        comments = [self.trusted_comment(terminal), self.trusted_comment(claim)]
        with mock.patch.object(state_manager, "release_failed_capacity", return_value=(True, "released")) as release, \
             mock.patch.object(state_manager, "get_issue_comments", return_value=comments), \
             mock.patch.object(state_manager, "record_dispatch_state") as record:
            ok, reason = state_manager.release_rejected_worker(
                41, "true", "success", "false", "acme/repo",
                rejection_reason="invalid_issue_scope", dispatch_id="worker:41",
                claim_nonce="a" * 32,
            )
        self.assertTrue(ok, reason)
        self.assertEqual(reason, "released")
        record.assert_not_called()
        release.assert_called_once_with(
            41, state_manager.LABEL_RUNNING, state_manager.LABEL_BLOCKED, repo="acme/repo"
        )

    def test_rejected_worker_fails_closed_on_comments_api_failure(self):
        with mock.patch.object(state_manager, "release_failed_capacity", return_value=(True, "released")), \
             mock.patch.object(state_manager, "get_issue_comments", side_effect=state_manager.StateUnavailableError("api")), \
             mock.patch.object(state_manager, "record_dispatch_state") as record:
            ok, reason = state_manager.release_rejected_worker(
                41, "true", "success", "false", "acme/repo",
                rejection_reason="invalid_issue_scope", dispatch_id="worker:41",
                claim_nonce="a" * 32,
            )
        self.assertFalse(ok)
        self.assertEqual(reason, "claim_state_unavailable")
        record.assert_not_called()

    def test_rejected_worker_superseded_by_newer_generation_has_no_side_effects(self):
        newer = self.worker_claim(dispatch_id="next:5:41", details={
            "allowed_paths": ["docs/"],
            "task_body_sha256": "b" * 64,
            "claim_nonce": "b" * 32,
        })
        older = self.worker_claim(details={
            "allowed_paths": ["src/"],
            "task_body_sha256": "a" * 64,
            "claim_nonce": "a" * 32,
        })
        comments = [self.trusted_comment(newer), self.trusted_comment(older)]
        with mock.patch.object(state_manager, "release_failed_capacity") as release, \
             mock.patch.object(state_manager, "get_issue_comments", return_value=comments), \
             mock.patch.object(state_manager, "comment_on_issue") as comment, \
             mock.patch.object(state_manager, "record_dispatch_state") as record:
            ok, reason = state_manager.release_rejected_worker(
                41, "true", "success", "false", "acme/repo",
                rejection_reason="invalid_issue_scope", dispatch_id="worker:41",
                claim_nonce="a" * 32,
            )
        self.assertTrue(ok)
        self.assertEqual(reason, "superseded")
        record.assert_not_called()
        comment.assert_not_called()
        release.assert_not_called()
        with mock.patch.object(state_manager, "get_issue_comments", return_value=comments):
            claim = state_manager.read_worker_claim_scope(41, "acme/repo")
        self.assertEqual(claim, {"allowed_paths": ["docs/"], "task_body_sha256": "b" * 64})

    def test_rejected_worker_fails_closed_when_no_worker_claim_state_exists(self):
        unrelated = {
            "kind": "agent-orchestrator-review-state", "version": 2,
            "issue_number": 41, "pr_number": 7, "head_sha": "a" * 40,
            "verdict": "BLOCKED", "summary": "unrelated",
        }
        comments = [self.trusted_comment(unrelated)]
        with mock.patch.object(state_manager, "release_failed_capacity") as release, \
             mock.patch.object(state_manager, "get_issue_comments", return_value=comments), \
             mock.patch.object(state_manager, "record_dispatch_state") as record:
            ok, reason = state_manager.release_rejected_worker(
                41, "true", "success", "false", "acme/repo",
                rejection_reason="invalid_issue_scope", dispatch_id="worker:41",
                claim_nonce="a" * 32,
            )
        self.assertFalse(ok)
        self.assertEqual(reason, "claim_not_found")
        record.assert_not_called()
        release.assert_not_called()

    def test_rejected_worker_fails_closed_on_terminal_write_failure(self):
        claim = self.worker_claim()
        comments = [self.trusted_comment(claim)]
        with mock.patch.object(state_manager, "release_failed_capacity", return_value=(True, "released")) as release, \
             mock.patch.object(state_manager, "get_issue_comments", return_value=comments), \
             mock.patch.object(state_manager, "record_dispatch_state", return_value=False):
            ok, reason = state_manager.release_rejected_worker(
                41, "true", "success", "false", "acme/repo",
                rejection_reason="invalid_issue_scope", dispatch_id="worker:41",
                claim_nonce="a" * 32,
            )
        self.assertFalse(ok)
        self.assertEqual(reason, "claim_state_failed_write")
        release.assert_not_called()

    def test_rejected_worker_fails_closed_on_unparseable_latest_state(self):
        comments = [{
            "author": {"login": "github-actions[bot]"},
            "body": '{"kind": "agent-orchestrator-dispatch-state", "status": "dispatched"',
        }]
        with mock.patch.object(state_manager, "release_failed_capacity", return_value=(True, "released")), \
             mock.patch.object(state_manager, "get_issue_comments", return_value=comments), \
             mock.patch.object(state_manager, "record_dispatch_state") as record:
            ok, reason = state_manager.release_rejected_worker(
                41, "true", "success", "false", "acme/repo",
                rejection_reason="invalid_issue_scope", dispatch_id="worker:41",
                claim_nonce="a" * 32,
            )
        self.assertFalse(ok)
        self.assertEqual(reason, "claim_state_unverifiable")
        record.assert_not_called()

    def test_rejected_worker_fails_closed_on_wrong_version(self):
        comments = [self.trusted_comment(self.worker_claim(version=2))]
        with mock.patch.object(state_manager, "release_failed_capacity", return_value=(True, "released")), \
             mock.patch.object(state_manager, "get_issue_comments", return_value=comments), \
             mock.patch.object(state_manager, "record_dispatch_state") as record:
            ok, reason = state_manager.release_rejected_worker(
                41, "true", "success", "false", "acme/repo",
                rejection_reason="invalid_issue_scope", dispatch_id="worker:41",
                claim_nonce="a" * 32,
            )
        self.assertFalse(ok)
        self.assertEqual(reason, "claim_state_unverifiable")
        record.assert_not_called()

    def test_rejected_worker_fails_closed_on_invalid_claim_binding(self):
        comments = [self.trusted_comment(self.worker_claim(details={
            "allowed_paths": ["src/"], "claim_nonce": "a" * 32,
        }))]
        with mock.patch.object(state_manager, "release_failed_capacity", return_value=(True, "released")), \
             mock.patch.object(state_manager, "get_issue_comments", return_value=comments), \
             mock.patch.object(state_manager, "record_dispatch_state") as record:
            ok, reason = state_manager.release_rejected_worker(
                41, "true", "success", "false", "acme/repo",
                rejection_reason="invalid_issue_scope", dispatch_id="worker:41",
                claim_nonce="a" * 32,
            )
        self.assertFalse(ok)
        self.assertEqual(reason, "claim_binding_unverifiable")
        record.assert_not_called()

    def test_rejected_worker_fails_closed_without_an_explicit_dispatch_id(self):
        claim = self.worker_claim()
        comments = [self.trusted_comment(claim)]
        with mock.patch.object(state_manager, "release_failed_capacity", return_value=(True, "already_released")), \
             mock.patch.object(state_manager, "get_issue_comments", return_value=comments), \
             mock.patch.object(state_manager, "record_dispatch_state") as record:
            ok, reason = state_manager.release_rejected_worker(
                41, "true", "success", "false", "acme/repo",
            )
        self.assertFalse(ok)
        self.assertEqual(reason, "claim_dispatch_id_unavailable")
        record.assert_not_called()

    def test_rejected_worker_fails_closed_without_a_claim_nonce(self):
        claim = self.worker_claim()
        comments = [self.trusted_comment(claim)]
        with mock.patch.object(state_manager, "release_failed_capacity", return_value=(True, "already_released")), \
             mock.patch.object(state_manager, "get_issue_comments", return_value=comments), \
             mock.patch.object(state_manager, "record_dispatch_state") as record:
            ok, reason = state_manager.release_rejected_worker(
                41, "true", "success", "false", "acme/repo",
                dispatch_id="worker:41",
            )
        self.assertFalse(ok)
        self.assertEqual(reason, "claim_nonce_unavailable")
        record.assert_not_called()

    def test_rejected_worker_retry_completes_after_capacity_already_released(self):
        claim = self.worker_claim()
        comments = [self.trusted_comment(claim)]
        with mock.patch.object(state_manager, "release_failed_capacity", return_value=(True, "already_released")), \
             mock.patch.object(state_manager, "get_issue_comments", return_value=comments), \
             mock.patch.object(state_manager, "record_dispatch_state", return_value=True):
            ok, reason = state_manager.release_rejected_worker(
                41, "true", "success", "false", "acme/repo",
                rejection_reason="invalid_issue_scope", dispatch_id="worker:41",
                claim_nonce="a" * 32,
            )
        self.assertTrue(ok)
        self.assertEqual(reason, "already_released")

    def test_rejected_worker_is_idempotent_when_the_exact_claim_is_already_terminal(self):
        comments = [self.trusted_comment(self.worker_claim(
            status="failed", details={
                "allowed_paths": ["src/"],
                "task_body_sha256": "a" * 64,
                "reason": "reconciled", "claim_nonce": "a" * 32,
            },
        ))]
        with mock.patch.object(state_manager, "release_failed_capacity", return_value=(True, "released")), \
             mock.patch.object(state_manager, "get_issue_comments", return_value=comments), \
             mock.patch.object(state_manager, "record_dispatch_state") as record:
            ok, reason = state_manager.release_rejected_worker(
                41, "true", "success", "false", "acme/repo",
                rejection_reason="invalid_issue_scope", dispatch_id="worker:41",
                claim_nonce="a" * 32,
            )
        self.assertTrue(ok)
        record.assert_not_called()

    def test_rejected_worker_fails_closed_on_terminal_claim_without_a_digest(self):
        terminal = self.worker_claim(status="failed", details={
            "allowed_paths": ["src/"],
            "claim_nonce": "a" * 32,
            "reason": "invalid_issue_scope",
        })
        comments = [self.trusted_comment(terminal)]
        with mock.patch.object(state_manager, "release_failed_capacity") as release, \
             mock.patch.object(state_manager, "get_issue_comments", return_value=comments), \
             mock.patch.object(state_manager, "comment_on_issue") as comment, \
             mock.patch.object(state_manager, "record_dispatch_state") as record:
            ok, reason = state_manager.release_rejected_worker(
                41, "true", "success", "false", "acme/repo",
                rejection_reason="invalid_issue_scope", dispatch_id="worker:41",
                claim_nonce="a" * 32,
            )
        self.assertFalse(ok)
        self.assertEqual(reason, "claim_binding_unverifiable")
        record.assert_not_called()
        comment.assert_not_called()
        release.assert_not_called()

    def test_rejected_worker_fails_closed_on_terminal_claim_with_an_invalid_path(self):
        terminal = self.worker_claim(status="failed", details={
            "allowed_paths": ["src/*"],
            "task_body_sha256": "a" * 64,
            "claim_nonce": "a" * 32,
            "reason": "invalid_issue_scope",
        })
        comments = [self.trusted_comment(terminal)]
        with mock.patch.object(state_manager, "release_failed_capacity") as release, \
             mock.patch.object(state_manager, "get_issue_comments", return_value=comments), \
             mock.patch.object(state_manager, "comment_on_issue") as comment, \
             mock.patch.object(state_manager, "record_dispatch_state") as record:
            ok, reason = state_manager.release_rejected_worker(
                41, "true", "success", "false", "acme/repo",
                rejection_reason="invalid_issue_scope", dispatch_id="worker:41",
                claim_nonce="a" * 32,
            )
        self.assertFalse(ok)
        self.assertEqual(reason, "claim_binding_unverifiable")
        record.assert_not_called()
        comment.assert_not_called()
        release.assert_not_called()

    def test_new_run_nonce_terminates_only_its_own_claim_generation(self):
        newer = self.worker_claim(details={
            "allowed_paths": ["docs/"],
            "task_body_sha256": "b" * 64,
            "claim_nonce": "b" * 32,
        })
        older = self.worker_claim(details={
            "allowed_paths": ["src/"],
            "task_body_sha256": "a" * 64,
            "claim_nonce": "a" * 32,
        })
        recorded = []

        def record_dispatch(_issue, _dispatch_id, action, status, details=None, repo=""):
            recorded.append((_dispatch_id, action, status, details))
            return True

        comments = [self.trusted_comment(newer), self.trusted_comment(older)]
        with mock.patch.object(state_manager, "release_failed_capacity", return_value=(True, "released")) as release, \
             mock.patch.object(state_manager, "get_issue_comments", return_value=comments), \
             mock.patch.object(state_manager, "record_dispatch_state", side_effect=record_dispatch):
            ok, reason = state_manager.release_rejected_worker(
                41, "true", "success", "false", "acme/repo",
                rejection_reason="invalid_issue_scope", dispatch_id="worker:41",
                claim_nonce="b" * 32,
            )
        self.assertTrue(ok, reason)
        self.assertEqual(len(recorded), 1)
        self.assertEqual(recorded[0][:3], ("worker:41", "worker", "failed"))
        self.assertEqual(recorded[0][3]["claim_nonce"], "b" * 32)
        release.assert_called_once_with(
            41, state_manager.LABEL_RUNNING, state_manager.LABEL_BLOCKED, repo="acme/repo"
        )
        terminal = self.trusted_comment({
            "kind": "agent-orchestrator-dispatch-state", "version": 1,
            "issue_number": 41, "dispatch_id": "worker:41", "action": "worker",
            "status": "failed", "details": recorded[0][3],
        })
        with mock.patch.object(state_manager, "get_issue_comments", return_value=[terminal] + comments):
            self.assertIsNone(state_manager.read_worker_claim_scope(41, "acme/repo"))

    def test_old_run_nonce_cannot_terminalize_or_release_a_newer_claim_reusing_the_dispatch_id(self):
        newer = self.worker_claim(details={
            "allowed_paths": ["docs/"],
            "task_body_sha256": "b" * 64,
            "claim_nonce": "b" * 32,
        })
        older = self.worker_claim(details={
            "allowed_paths": ["src/"],
            "task_body_sha256": "a" * 64,
            "claim_nonce": "a" * 32,
        })
        comments = [self.trusted_comment(newer), self.trusted_comment(older)]
        with mock.patch.object(state_manager, "release_failed_capacity") as release, \
             mock.patch.object(state_manager, "get_issue_comments", return_value=comments), \
             mock.patch.object(state_manager, "comment_on_issue") as comment, \
             mock.patch.object(state_manager, "record_dispatch_state") as record:
            ok, reason = state_manager.release_rejected_worker(
                41, "true", "success", "false", "acme/repo",
                rejection_reason="invalid_issue_scope", dispatch_id="worker:41",
                claim_nonce="a" * 32,
            )
        self.assertTrue(ok)
        self.assertEqual(reason, "superseded")
        record.assert_not_called()
        comment.assert_not_called()
        release.assert_not_called()
        with mock.patch.object(state_manager, "get_issue_comments", return_value=comments):
            claim = state_manager.read_worker_claim_scope(41, "acme/repo")
        self.assertEqual(claim, {"allowed_paths": ["docs/"], "task_body_sha256": "b" * 64})

    def test_old_run_nonce_superseded_when_only_a_newer_claim_generation_exists(self):
        newer = self.worker_claim(details={
            "allowed_paths": ["docs/"],
            "task_body_sha256": "b" * 64,
            "claim_nonce": "b" * 32,
        })
        comments = [self.trusted_comment(newer)]
        with mock.patch.object(state_manager, "release_failed_capacity") as release, \
             mock.patch.object(state_manager, "get_issue_comments", return_value=comments), \
             mock.patch.object(state_manager, "record_dispatch_state") as record:
            ok, reason = state_manager.release_rejected_worker(
                41, "true", "success", "false", "acme/repo",
                rejection_reason="invalid_issue_scope", dispatch_id="worker:41",
                claim_nonce="a" * 32,
            )
        self.assertTrue(ok)
        self.assertEqual(reason, "superseded")
        record.assert_not_called()
        release.assert_not_called()

    def test_old_run_nonce_cannot_claim_idempotency_of_a_newer_terminal_claim(self):
        newer = self.worker_claim(status="failed", details={
            "reason": "reconciled", "claim_nonce": "b" * 32,
        })
        comments = [self.trusted_comment(newer)]
        with mock.patch.object(state_manager, "release_failed_capacity") as release, \
             mock.patch.object(state_manager, "get_issue_comments", return_value=comments), \
             mock.patch.object(state_manager, "comment_on_issue") as comment, \
             mock.patch.object(state_manager, "record_dispatch_state") as record:
            ok, reason = state_manager.release_rejected_worker(
                41, "true", "success", "false", "acme/repo",
                rejection_reason="invalid_issue_scope", dispatch_id="worker:41",
                claim_nonce="a" * 32,
            )
        self.assertTrue(ok)
        self.assertEqual(reason, "superseded")
        record.assert_not_called()
        comment.assert_not_called()
        release.assert_not_called()

    def test_terminal_comment_never_shadows_a_later_created_new_claim(self):
        new_claim = self.worker_claim(dispatch_id="next:6:41", details={
            "allowed_paths": ["docs/"],
            "task_body_sha256": "c" * 64,
            "claim_nonce": "c" * 32,
        })
        terminal = self.worker_claim(status="failed", details={
            "allowed_paths": ["src/"],
            "task_body_sha256": "a" * 64,
            "claim_nonce": "a" * 32,
            "reason": "invalid_issue_scope",
        })
        old_claim = self.worker_claim()
        comments = [
            self.trusted_comment(new_claim),
            self.trusted_comment(terminal),
            self.trusted_comment(old_claim),
        ]
        with mock.patch.object(state_manager, "get_issue_comments", return_value=comments):
            claim = state_manager.read_worker_claim_scope(41, "acme/repo")
        self.assertEqual(claim, {"allowed_paths": ["docs/"], "task_body_sha256": "c" * 64})

    def test_dispatch_retry_keeps_the_original_claim_nonce(self):
        labels = {state_manager.LABEL_READY}
        recorded = {}
        workflow_calls = []

        def set_labels(_issue, *new_labels, repo=""):
            labels.clear()
            labels.update(new_labels)
            return True

        def record(_issue, dispatch_id, action, status, details=None, repo=""):
            recorded[dispatch_id] = {
                "kind": "agent-orchestrator-dispatch-state",
                "issue_number": _issue, "status": status, "action": action,
                "details": details or {},
            }
            return True

        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="repo"), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", side_effect=lambda _i, key, _r: recorded.get(key)), \
             mock.patch.object(dispatcher.sm, "get_issue_labels_checked", return_value=labels), \
             mock.patch.object(dispatcher.sm, "check_dependencies_complete", return_value=(True, None)), \
             mock.patch.object(dispatcher.sm, "has_open_issue_pr", return_value=False), \
             mock.patch.object(dispatcher.sm, "read_task_scope_binding", return_value=(True, {
                 "allowed_paths": ["src/"], "task_body_sha256": "a" * 64,
             })), \
              mock.patch.object(dispatcher.sm, "get_active_capacity", return_value={"issues": set(), "plans": []}), \
              mock.patch.object(dispatcher.sm, "set_labels", side_effect=set_labels), \
             mock.patch.object(dispatcher.sm, "record_dispatch_state", side_effect=record), \
             mock.patch.object(dispatcher, "_run_workflow", side_effect=lambda *args: workflow_calls.append(args) or True):
            first = dispatcher.dispatch_ready(12, "worker:12")
            second = dispatcher.dispatch_ready(12, "worker:12")
        self.assertTrue(first["dispatched"])
        self.assertTrue(second["dispatched"])
        self.assertEqual(second["reason"], "already_dispatched")
        self.assertEqual(len(workflow_calls), 1)
        original_nonce = recorded["worker:12"]["details"]["claim_nonce"]
        self.assertRegex(original_nonce, r"^[0-9a-f]{32}$")
        self.assertEqual(workflow_calls[0][1]["claim_nonce"], original_nonce)
        self.assertEqual(recorded["worker:12"]["details"]["allowed_paths"], ["src/"])
        self.assertEqual(recorded["worker:12"]["details"]["task_body_sha256"], "a" * 64)

    def test_worker_dispatch_carries_the_exact_claim_identity(self):
        workflow_calls = []
        labels = {state_manager.LABEL_READY}
        persisted = {}

        def record_dispatch(_issue, _dispatch_id, action, status, details=None, repo=""):
            if status == "claimed":
                persisted.update(details or {})
            return True

        with mock.patch.object(dispatcher.control_state, "require_live", return_value={}), \
             mock.patch.object(dispatcher, "_repo", return_value="repo"), \
             mock.patch.object(dispatcher.sm, "read_dispatch_state", return_value=None), \
             mock.patch.object(dispatcher.sm, "get_issue_labels_checked", return_value=labels), \
             mock.patch.object(dispatcher.sm, "read_task_scope_binding", return_value=(True, {
                 "allowed_paths": ["src/"], "task_body_sha256": "a" * 64,
             })), \
             mock.patch.object(dispatcher.sm, "check_dependencies_complete", return_value=(True, None)), \
             mock.patch.object(dispatcher.sm, "has_open_issue_pr", return_value=False), \
              mock.patch.object(dispatcher.sm, "get_active_capacity", return_value={"issues": set(), "plans": []}), \
              mock.patch.object(dispatcher.sm, "set_labels", return_value=True), \
             mock.patch.object(dispatcher.sm, "record_dispatch_state", side_effect=record_dispatch), \
             mock.patch.object(dispatcher, "_run_workflow", side_effect=lambda *args: workflow_calls.append(args) or True):
            result = dispatcher.dispatch_ready(12, "next:5:12")
        self.assertTrue(result["dispatched"])
        nonce = persisted["claim_nonce"]
        self.assertRegex(nonce, r"^[0-9a-f]{32}$")
        self.assertEqual(
            workflow_calls[0],
            ("agent-worker.yml", {
                "issue": 12, "dry_run": "false", "dispatch_id": "next:5:12",
                "claim_nonce": nonce,
            }),
        )

    def test_new_scope_binding_clis_validate_arity(self):
        for command in ("read-task-scope-binding", "verify-task-scope-binding"):
            for args in ([command], [command, "41", "extra"]):
                with self.subTest(args=args):
                    result = subprocess.run(
                        [sys.executable, str(CONTROL / "state_manager.py"), *args],
                        cwd=ROOT, capture_output=True, text=True,
                    )
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("Usage:", result.stderr)

    def test_release_rejected_worker_cli_validates_arity(self):
        for args in (
            ["release-rejected-worker"],
            ["release-rejected-worker", "41"],
            ["release-rejected-worker", "41", "true", "success", "false", "123", "x", "y", "z", "w", "v"],
        ):
            with self.subTest(args=args):
                result = subprocess.run(
                    [sys.executable, str(CONTROL / "state_manager.py"), *args],
                    cwd=ROOT, capture_output=True, text=True,
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Usage:", result.stderr)


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


class TestPRBindingDraftGate(unittest.TestCase):
    def test_create_post_sends_draft_true_gh_field_and_accepts_draft_pr(self):
        sha = "b" * 40
        pr = _pr_binding_fixture(number=123, sha=sha)
        with mock.patch.object(pr_binding, "_open_prs", side_effect=[[], [pr]]), \
             mock.patch.object(pr_binding, "_gh_json", return_value={
                 "number": 123, "html_url": "https://github.com/acme/repo/pull/123",
             }) as gh_json, \
             mock.patch.object(pr_binding, "_view_pr", return_value=pr):
            result = pr_binding.create_or_update_pr(
                42, "agent/issue-42", sha, "agent: implement #42",
                "Closes #42\n\n<!-- agent-orchestrator-binding: {\"issue_number\": 42, \"branch\": \"agent/issue-42\"} -->",
                "acme/repo",
            )
        post = gh_json.call_args_list[0]
        self.assertEqual(post.args[0:3], ("api", "--method", "POST"))
        self.assertIn("repos/acme/repo/pulls", post.args)
        self.assertIn("draft=true", post.args)
        self.assertEqual(result["number"], 123)
        self.assertEqual(result["head_sha"], sha)

    def test_reuse_patches_existing_draft_without_touching_draft_state(self):
        sha = "b" * 40
        pr = _pr_binding_fixture(number=207, sha=sha)
        with mock.patch.object(pr_binding, "_open_prs", side_effect=[[pr], [pr]]), \
             mock.patch.object(pr_binding, "_gh") as gh, \
             mock.patch.object(pr_binding, "_view_pr", return_value=pr):
            result = pr_binding.create_or_update_pr(
                42, "agent/issue-42", sha, "agent: implement #42",
                "Closes #42\n\n<!-- agent-orchestrator-binding: {\"issue_number\": 42, \"branch\": \"agent/issue-42\"} -->",
                "acme/repo",
            )
        patch = gh.call_args_list[0]
        self.assertEqual(patch.args[0:3], ("api", "--method", "PATCH"))
        self.assertIn("repos/acme/repo/pulls/207", patch.args)
        self.assertNotIn("draft", patch.args)
        self.assertEqual(result["number"], 207)

    def test_reuse_rejects_ready_pr_without_converting_it(self):
        sha = "b" * 40
        ready = _pr_binding_fixture(number=207, sha=sha, is_draft=False)
        with mock.patch.object(pr_binding, "_open_prs", return_value=[ready]), \
             mock.patch.object(pr_binding, "_gh") as gh, \
             mock.patch.object(pr_binding, "_view_pr") as view:
            with self.assertRaisesRegex(pr_binding.PRBindingError, "not a Draft"):
                pr_binding.create_or_update_pr(
                    42, "agent/issue-42", sha, "agent: implement #42",
                    "Closes #42\n\n<!-- agent-orchestrator-binding: {\"issue_number\": 42, \"branch\": \"agent/issue-42\"} -->",
                    "acme/repo",
                )
        gh.assert_not_called()
        view.assert_not_called()

    def test_reuse_rejects_missing_draft_candidate_without_patching(self):
        sha = "b" * 40
        missing = _pr_binding_fixture(number=207, sha=sha)
        del missing["isDraft"]
        with mock.patch.object(pr_binding, "_open_prs", return_value=[missing]), \
             mock.patch.object(pr_binding, "_gh") as gh, \
             mock.patch.object(pr_binding, "_view_pr") as view:
            with self.assertRaisesRegex(pr_binding.PRBindingError, "not a Draft"):
                pr_binding.create_or_update_pr(
                    42, "agent/issue-42", sha, "agent: implement #42",
                    "Closes #42\n\n<!-- agent-orchestrator-binding: {\"issue_number\": 42, \"branch\": \"agent/issue-42\"} -->",
                    "acme/repo",
                )
        gh.assert_not_called()
        view.assert_not_called()

    def test_verify_pr_rejects_missing_or_false_draft_field(self):
        sha = "b" * 40
        missing = _pr_binding_fixture(number=207, sha=sha)
        del missing["isDraft"]
        with self.assertRaisesRegex(pr_binding.PRBindingError, "not a Draft"):
            pr_binding._verify_pr(missing, 42, "agent/issue-42", sha, [missing])
        ready = _pr_binding_fixture(number=207, sha=sha, is_draft=False)
        with self.assertRaisesRegex(pr_binding.PRBindingError, "not a Draft"):
            pr_binding._verify_pr(ready, 42, "agent/issue-42", sha, [ready])

    def test_draft_gate_preserves_exact_head_marker_and_closing_link_checks(self):
        sha = "b" * 40
        wrong_head = _pr_binding_fixture(number=207, sha=sha)
        with self.assertRaisesRegex(pr_binding.PRBindingError, "branch or head does not match"):
            pr_binding._verify_pr(wrong_head, 42, "agent/issue-42", "c" * 40, [wrong_head])
        no_marker = _pr_binding_fixture(number=207, sha=sha)
        no_marker["body"] = "Closes #42"
        with self.assertRaisesRegex(pr_binding.PRBindingError, "Issue marker is invalid"):
            pr_binding._verify_pr(no_marker, 42, "agent/issue-42", sha, [no_marker])
        ok = _pr_binding_fixture(number=207, sha=sha)
        self.assertEqual(pr_binding._verify_pr(ok, 42, "agent/issue-42", sha, [ok])["number"], 207)


class TestRepairHeadTransition(unittest.TestCase):
    def test_post_push_verification_accepts_h2_before_new_worker_state(self):
        pr = _pr_binding_fixture(number=207, sha="b" * 40)
        with mock.patch.object(pr_binding, "_open_prs", return_value=[pr]):
            result = pr_binding.verify_post_push_binding(42, 207, "agent/issue-42", "b" * 40, "acme/repo")
        self.assertEqual(result["head_sha"], "b" * 40)

    def test_post_push_verification_rejects_ready_pr(self):
        sha = "b" * 40
        pr = _pr_binding_fixture(number=207, sha=sha, is_draft=False)
        with mock.patch.object(pr_binding.time, "sleep"), \
             mock.patch.object(pr_binding.time, "monotonic", side_effect=[0.0, 0.0, 30.0]), \
             mock.patch.object(pr_binding, "_open_prs", return_value=[pr]):
            with self.assertRaisesRegex(pr_binding.PRBindingError, "not a Draft"):
                pr_binding.verify_post_push_binding(42, 207, "agent/issue-42", sha, "acme/repo")

    def test_post_push_verification_rejects_missing_draft_field(self):
        sha = "b" * 40
        pr = _pr_binding_fixture(number=207, sha=sha)
        del pr["isDraft"]
        with mock.patch.object(pr_binding.time, "sleep"), \
             mock.patch.object(pr_binding.time, "monotonic", side_effect=[0.0, 0.0, 30.0]), \
             mock.patch.object(pr_binding, "_open_prs", return_value=[pr]):
            with self.assertRaisesRegex(pr_binding.PRBindingError, "not a Draft"):
                pr_binding.verify_post_push_binding(42, 207, "agent/issue-42", sha, "acme/repo")

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
        self.assertEqual(str(raised.exception), "ci_control_stopped:fallback_run_identity_missing")
        self.assertIsNone(raised.exception.ci_run_id)
        self.assertEqual(raised.exception.dispatch_nonce, "deadbeef")
        self.assertEqual(raised.exception.observed_run["dispatch_nonce"], "deadbeef")

    def test_control_stopped_identity_missing_serializes_typed_non_authorizing_result(self):
        exc = ci_verifier.CIControlStopped(
            "ci_control_stopped:fallback_run_identity_missing",
            head_sha="a" * 40,
            dispatch_nonce="deadbeef",
            observed_run={"status": "unknown"},
        )
        with mock.patch.object(ci_verifier, "dispatch_exact_ci", side_effect=exc), \
             mock.patch.object(ci_verifier.sys, "argv", ["ci_verifier.py", "dispatch", "agent/issue-a", "a" * 40]), \
             mock.patch("builtins.print") as printed:
            ci_verifier.main()
        payload = json.loads(printed.call_args.args[0])
        self.assertEqual(payload["status"], "ci_control_stopped")
        self.assertEqual(payload["ci_run_id"], 0)
        self.assertEqual(payload["run_identity"], "unavailable")
        self.assertEqual(payload["dispatch_nonce"], "deadbeef")

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
            "attempt": 1,
            "artifacts": [{
                "name": "context-capsule-1-1-" + "a" * 40,
                "expired": False,
                "workflow_run": {"id": 1, "run_attempt": 1},
            }],
            "jobs": _successful_required_jobs(),
        }
        with mock.patch.object(ci_verifier, "run_info", return_value=run):
            result = ci_verifier.verify_exact_head_ci(2, "a" * 40, 1, {"headRefOid": "a" * 40})
        self.assertEqual(result["successful_jobs"], required)
        self.assertEqual(result["checked_out_sha"], "a" * 40)
        self.assertEqual(result["exact_head_verify_step"], ci_verifier.EXACT_HEAD_VERIFY_STEP)

    def test_context_capsule_artifact_must_be_published_on_exact_head(self):
        sha = "a" * 40
        run = {
            "databaseId": 3, "attempt": 1, "workflowName": "tests", "headSha": sha,
            "status": "completed", "conclusion": "success",
            "jobs": _successful_required_jobs(),
            "artifacts": [],
        }
        with mock.patch.object(ci_verifier, "run_info", return_value=run):
            with self.assertRaisesRegex(
                ci_verifier.CIVerificationError,
                "context-capsule artifact publication evidence",
            ):
                ci_verifier.verify_exact_head_ci(2, sha, 3, {"headRefOid": sha})

    def test_context_capsule_artifact_must_bind_nested_workflow_run(self):
        sha = "a" * 40
        run = {
            "databaseId": 4,
            "attempt": 1,
            "workflowName": "tests",
            "headSha": sha,
            "status": "completed",
            "conclusion": "success",
            "jobs": _successful_required_jobs(),
            "artifacts": [{
                "name": f"context-capsule-4-1-{sha}",
                "expired": False,
                "workflow_run": {"id": 3, "run_attempt": 1},
            }],
        }
        with mock.patch.object(ci_verifier, "run_info", return_value=run):
            with self.assertRaisesRegex(
                ci_verifier.CIVerificationError,
                "context-capsule artifact publication evidence",
            ):
                ci_verifier.verify_exact_head_ci(2, sha, 4, {"headRefOid": sha})

    def test_context_capsule_artifact_requires_explicit_unexpired_status(self):
        sha = "a" * 40
        run = {
            "databaseId": 5,
            "attempt": 1,
            "workflowName": "tests",
            "headSha": sha,
            "status": "completed",
            "conclusion": "success",
            "jobs": _successful_required_jobs(),
            "artifacts": [{
                "name": f"context-capsule-5-1-{sha}",
                "workflow_run": {"id": 5},
            }],
        }
        with mock.patch.object(ci_verifier, "run_info", return_value=run):
            with self.assertRaisesRegex(
                ci_verifier.CIVerificationError,
                "context-capsule artifact publication evidence",
            ):
                ci_verifier.verify_exact_head_ci(2, sha, 5, {"headRefOid": sha})

    def test_context_capsule_artifact_rejects_conflicting_workflow_run_id(self):
        sha = "a" * 40
        run = {
            "databaseId": 6,
            "attempt": 1,
            "workflowName": "tests",
            "headSha": sha,
            "status": "completed",
            "conclusion": "success",
            "jobs": _successful_required_jobs(),
            "artifacts": [{
                "name": f"context-capsule-6-1-{sha}",
                "expired": False,
                "workflow_run_id": 6,
                "workflow_run": {"id": 7},
            }],
        }
        with mock.patch.object(ci_verifier, "run_info", return_value=run):
            with self.assertRaisesRegex(
                ci_verifier.CIVerificationError,
                "context-capsule artifact publication evidence",
            ):
                ci_verifier.verify_exact_head_ci(2, sha, 6, {"headRefOid": sha})

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

    def test_exact_head_evidence_rejects_skipped_or_absent_verify_step(self):
        required = ci_verifier.load_requirements()["required_jobs"]
        sha = "s" * 40

        # Missing step payloads are not exact-head proof.
        missing_steps = {
            "databaseId": 801, "workflowName": "tests", "headSha": sha,
            "status": "completed", "conclusion": "success",
            "jobs": [
                {"name": name, "status": "completed", "conclusion": "success"}
                for name in required
            ],
        }
        with mock.patch.object(ci_verifier, "run_info", return_value=missing_steps):
            with self.assertRaisesRegex(
                ci_verifier.CIVerificationError,
                "exact-head verification step evidence is absent",
            ):
                ci_verifier.verify_exact_head_ci(2, sha, 801, {"headRefOid": sha})

        # A natural pull_request run that skipped the step is rejected.
        skipped_jobs = []
        for name in required:
            skipped_jobs.append({
                "name": name,
                "status": "completed",
                "conclusion": "success",
                "steps": [{
                    "name": ci_verifier.EXACT_HEAD_VERIFY_STEP,
                    "status": "completed",
                    "conclusion": "skipped",
                }],
            })
        skipped_run = {
            "databaseId": 802, "workflowName": "tests", "headSha": sha,
            "status": "completed", "conclusion": "success", "event": "pull_request",
            "jobs": skipped_jobs,
        }
        with mock.patch.object(ci_verifier, "run_info", return_value=skipped_run):
            with self.assertRaisesRegex(
                ci_verifier.CIVerificationError,
                "exact-head verification step was skipped",
            ):
                ci_verifier.verify_exact_head_ci(2, sha, 802, {"headRefOid": sha})

    def test_existing_natural_exact_head_run_is_reused_without_dispatch(self):
        run = {"databaseId": 11, "event": "pull_request", "status": "completed", "conclusion": "success", "headSha": "c" * 40, "headBranch": "agent/issue-7", "workflowName": "tests"}
        with mock.patch.object(ci_verifier, "find_exact_runs", return_value=[run]), \
             mock.patch.object(ci_verifier.subprocess, "run") as dispatch, \
             mock.patch.object(ci_verifier, "verify_exact_head_ci"):
            result = ci_verifier.acquire_exact_ci(7, "agent/issue-7", "c" * 40, observe_seconds=1)
        dispatch.assert_not_called()
        self.assertEqual(result["workflow_run_id"], 11)
        self.assertEqual(result["source"], "pull_request")

    def test_missing_exact_head_run_dispatches_once_then_binds_one(self):
        run = {"databaseId": 12, "event": "workflow_dispatch", "status": "completed", "conclusion": "success", "headSha": "d" * 40, "headBranch": "agent/issue-8", "workflowName": "tests"}
        with mock.patch.object(ci_verifier, "find_exact_runs", side_effect=[[], [run]]), \
             mock.patch.object(ci_verifier.subprocess, "run", return_value=mock.Mock(returncode=0)) as dispatch, \
             mock.patch.object(ci_verifier, "verify_exact_head_ci"):
            result = ci_verifier.acquire_exact_ci(8, "agent/issue-8", "d" * 40, observe_seconds=0)
        dispatch.assert_called_once()
        self.assertEqual(result["workflow_run_id"], 12)
        self.assertEqual(result["source"], "workflow_dispatch")

    def test_legacy_completion_refreshes_final_binding_before_ci_verification(self):
        sha = "v" * 40
        refreshed = {"state": "OPEN", "headRefOid": sha, "headRefName": "agent/issue-v"}
        acquisition = {
            "workflow_run_id": 120,
            "source": "pull_request",
            "duplicate_run_ids": [],
            "observed_run_ids": [120],
            "selection_reason": "natural_completed_observed",
            "superseded_run_ids": [],
            "unsupported_run_ids": [],
            "fallback_dispatched": False,
        }
        completion = {
            "status": "success", "conclusion": "success", "ci_run_id": 120,
            "head_sha": sha, "run": {"databaseId": 120, "status": "completed"},
        }
        with mock.patch.object(ci_verifier, "wait_for_run_completion", return_value=completion), \
             mock.patch.object(ci_verifier, "verify_exact_head_ci") as verify:
            result = ci_verifier._finalize_acquisition_with_wait(
                acquisition,
                pr_number=207,
                branch="agent/issue-v",
                head_sha=sha,
                completion_timeout_seconds=30,
                poll_seconds=1,
                final_validator=lambda: (refreshed, None),
            )
        self.assertEqual(result["status"], "completed")
        verify.assert_called_once_with(207, sha, 120, pr_snapshot=refreshed)

    def test_legacy_completion_refreshes_before_control_stop_terminalization(self):
        sha = "w" * 40
        refreshed = {"state": "OPEN", "headRefOid": sha, "headRefName": "agent/issue-w"}
        acquisition = {"workflow_run_id": 121}
        completion = {
            "status": "ci_control_stopped",
            "reason": "control_emergency_stop_activated",
            "ci_run_id": 121,
            "head_sha": sha,
            "run": {"databaseId": 121, "status": "in_progress"},
        }
        validator = mock.Mock(return_value=(refreshed, None))
        with mock.patch.object(ci_verifier, "wait_for_run_completion", return_value=completion):
            with self.assertRaises(ci_verifier.CIControlStopped):
                ci_verifier._finalize_acquisition_with_wait(
                    acquisition,
                    pr_number=207,
                    branch="agent/issue-w",
                    head_sha=sha,
                    completion_timeout_seconds=30,
                    poll_seconds=1,
                    final_validator=validator,
                )
        validator.assert_called_once_with()

    def test_completed_acquisition_still_runs_final_binding_validator(self):
        sha = "x" * 40
        acquisition = {
            "workflow_run_id": 122,
            "bound_status": "completed",
            "bound_conclusion": "success",
            "source": "pull_request",
        }
        validator = mock.Mock(return_value=(None, "ci_stale_binding:pr_closed"))
        with mock.patch.object(ci_verifier, "acquire_exact_run", return_value=acquisition):
            with self.assertRaisesRegex(ci_verifier.CIStaleBinding, "pr_closed"):
                ci_verifier.acquire_exact_ci(
                    207,
                    "agent/issue-x",
                    sha,
                    observe_seconds=0,
                    final_validator=validator,
                )
        validator.assert_called_once_with()

    def test_completed_success_acquisition_runs_exact_head_verifier(self):
        sha = "y" * 40
        acquisition = {
            "workflow_run_id": 123,
            "bound_status": "completed",
            "bound_conclusion": "success",
            "source": "pull_request",
        }
        with mock.patch.object(ci_verifier, "acquire_exact_run", return_value=acquisition), \
             mock.patch.object(ci_verifier, "verify_exact_head_ci") as verify:
            result = ci_verifier.acquire_exact_ci(
                207, "agent/issue-y", sha, observe_seconds=0
            )
        self.assertEqual(result["status"], "completed")
        verify.assert_called_once_with(
            207,
            sha,
            123,
            pr_snapshot={"headRefOid": sha, "headRefName": "agent/issue-y"},
        )

    def test_two_exact_head_runs_select_one_and_mark_duplicate(self):
        runs = [
            {"databaseId": 21, "event": "pull_request", "status": "completed", "conclusion": "success", "headSha": "e" * 40, "headBranch": "agent/issue-9", "workflowName": "tests"},
            {"databaseId": 22, "event": "workflow_dispatch", "status": "completed", "conclusion": "success", "headSha": "e" * 40, "headBranch": "agent/issue-9", "workflowName": "tests"},
        ]
        with mock.patch.object(ci_verifier, "find_exact_runs", return_value=runs):
            with mock.patch.object(ci_verifier, "verify_exact_head_ci"):
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
             mock.patch.object(ci_verifier.subprocess, "run", return_value=mock.Mock(returncode=0)) as dispatch, \
             mock.patch.object(ci_verifier, "verify_exact_head_ci"):
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
        with mock.patch.object(ci_verifier, "find_exact_runs", return_value=runs), \
             mock.patch.object(ci_verifier, "verify_exact_head_ci"):
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
             mock.patch.object(ci_verifier.subprocess, "run") as dispatch, \
             mock.patch.object(ci_verifier, "verify_exact_head_ci"):
            result = ci_verifier.acquire_exact_ci(1, "agent/x", sha, observe_seconds=0)
        dispatch.assert_not_called()
        self.assertEqual(result["workflow_run_id"], 921)
        self.assertEqual(result["source"], "workflow_dispatch")

    def test_natural_pending_completes_through_combined_function(self):
        sha = "d" * 40
        natural = {"databaseId": 930, "event": "pull_request", "status": "queued", "conclusion": "",
                   "headSha": sha, "headBranch": "agent/x", "workflowName": "tests",
                   "updatedAt": "2026-07-14T00:00:00Z"}
        completed = {"databaseId": 930, "event": "pull_request", "status": "completed", "conclusion": "success",
                     "headSha": sha, "headBranch": "agent/x", "workflowName": "tests",
                     "updatedAt": "2026-07-14T00:01:00Z",
                     "attempt": 1,
                     "artifacts": [{
                         "name": "context-capsule-930-1-" + sha,
                         "expired": False,
                         "workflow_run": {"id": 930, "run_attempt": 1},
                     }],
                     "jobs": _successful_required_jobs()}
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
             mock.patch.object(ci_verifier.subprocess, "run") as dispatch, \
             mock.patch.object(ci_verifier, "verify_exact_head_ci"):
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
        with mock.patch.object(ci_verifier, "find_exact_runs", return_value=runs), \
             mock.patch.object(ci_verifier, "verify_exact_head_ci"):
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
