"""Tests for the deterministic MVP3 resource planner."""

from __future__ import annotations

import unittest

from harness_core.app_registry import RepoRef
from harness_core.instance_audit import InstanceAuditReport
from harness_core.resource_planner import DeterministicResourcePlanner, PlanningTask


def local_repo() -> RepoRef:
    return RepoRef(id="local", name="Local", kind="local", path="/tmp/local")


def remote_repo() -> RepoRef:
    return RepoRef(id="remote", name="Remote", kind="remote", url="https://github.com/example/repo.git")


def audit(verdict: str = "PASS", blockers: list[str] | None = None, warnings: list[str] | None = None) -> InstanceAuditReport:
    return InstanceAuditReport(
        target_repo="/tmp/local",
        verdict=verdict,
        checks=[],
        blockers=blockers or [],
        warnings=warnings or [],
        recommended_next_actions=[],
    )


class DeterministicResourcePlannerTests(unittest.TestCase):
    def test_remote_repo_returns_blocked_metadata_only_plan(self):
        task = PlanningTask(task_id="docs", repo_id="remote", objective="Review docs", risk_level="low")

        plan = DeterministicResourcePlanner().plan(task, remote_repo(), None)

        self.assertEqual("blocked", plan.status)
        self.assertEqual(("remote_metadata_only",), plan.blockers)
        self.assertFalse(plan.executable)
        self.assertEqual((), plan.steps)

    def test_blocked_audit_blocks_plan_and_raises_effective_risk(self):
        task = PlanningTask(task_id="audit", repo_id="local", objective="Review docs", risk_level="low")

        plan = DeterministicResourcePlanner().plan(task, local_repo(), audit("BLOCKED", blockers=["missing policy"]))

        self.assertEqual("blocked", plan.status)
        self.assertEqual("critical", plan.effective_risk)
        self.assertIn("audit_blocked", plan.blockers)
        self.assertFalse(plan.executable)

    def test_low_risk_docs_task_is_ready_for_review(self):
        task = PlanningTask(task_id="docs", repo_id="local", objective="Review docs", task_type="docs", risk_level="low")

        plan = DeterministicResourcePlanner().plan(task, local_repo(), audit())

        self.assertEqual("ready_for_review", plan.status)
        self.assertEqual("low", plan.effective_risk)
        self.assertEqual((), plan.approval_gates)
        self.assertFalse(plan.executable)
        self.assertEqual(("planner", "executor", "verifier"), tuple(step.role for step in plan.steps))

    def test_high_risk_keywords_force_approval_gates(self):
        task = PlanningTask(
            task_id="deploy",
            repo_id="local",
            objective="Modify provider config and deploy autonomous worker",
            task_type="docs",
            risk_level="low",
        )

        plan = DeterministicResourcePlanner().plan(task, local_repo(), audit())

        self.assertEqual("needs_approval", plan.status)
        self.assertEqual("high", plan.effective_risk)
        self.assertIn("human_approval_required", plan.approval_gates)
        self.assertIn("provider_integration_gate", plan.approval_gates)
        self.assertIn("deployment_gate", plan.approval_gates)
        self.assertIn("execution_boundary_gate", plan.approval_gates)
        self.assertFalse(plan.executable)

    def test_negated_no_target_repo_writes_does_not_add_mutation_gate(self):
        task = PlanningTask(
            task_id="docs",
            repo_id="local",
            objective="Review governance docs",
            task_type="docs_review",
            risk_level="low",
            constraints=["read-only validation", "no target repo writes", "no source changes"],
        )

        plan = DeterministicResourcePlanner().plan(task, local_repo(), audit())

        self.assertEqual("ready_for_review", plan.status)
        self.assertNotIn("target_repo_mutation_gate", plan.approval_gates)

    def test_positive_target_repo_write_still_adds_mutation_gate(self):
        task = PlanningTask(
            task_id="write",
            repo_id="local",
            objective="Write target repo docs and commit changes",
            task_type="docs",
            risk_level="low",
        )

        plan = DeterministicResourcePlanner().plan(task, local_repo(), audit())

        self.assertEqual("needs_approval", plan.status)
        self.assertIn("target_repo_mutation_gate", plan.approval_gates)

    def test_read_only_docs_task_with_no_write_constraints_not_over_gated(self):
        task = PlanningTask(
            task_id="audit",
            repo_id="local",
            objective="Audit only: inspect project board for stale wording",
            task_type="audit_review",
            risk_level="medium",
            constraints=["target repo remains read-only", "does not modify target repo"],
        )

        plan = DeterministicResourcePlanner().plan(task, local_repo(), audit())

        self.assertEqual("ready_for_review", plan.status)
        self.assertEqual((), plan.approval_gates)

    def test_read_only_task_with_negated_provider_and_sandbox_constraints_not_over_gated(self):
        task = PlanningTask(
            task_id="boundary",
            repo_id="local",
            objective="Review boundary policy",
            task_type="docs_review",
            risk_level="low",
            constraints=["no provider calls", "no sandbox execution", "no autonomous workers"],
        )

        plan = DeterministicResourcePlanner().plan(task, local_repo(), audit())

        self.assertEqual("ready_for_review", plan.status)
        self.assertEqual((), plan.approval_gates)

    def test_provider_or_sandbox_positive_task_still_requires_gate(self):
        task = PlanningTask(
            task_id="provider",
            repo_id="local",
            objective="Evaluate provider API key and sandbox execution",
            task_type="provider",
            risk_level="low",
        )

        plan = DeterministicResourcePlanner().plan(task, local_repo(), audit())

        self.assertEqual("needs_approval", plan.status)
        self.assertIn("provider_integration_gate", plan.approval_gates)
        self.assertIn("execution_boundary_gate", plan.approval_gates)

    def test_high_risk_task_still_requires_gate(self):
        task = PlanningTask(task_id="risk", repo_id="local", objective="Review docs", risk_level="high")

        plan = DeterministicResourcePlanner().plan(task, local_repo(), audit())

        self.assertEqual("needs_approval", plan.status)
        self.assertIn("human_approval_required", plan.approval_gates)

    def test_budget_pressure_reduces_context_mode_to_none(self):
        task = PlanningTask(
            task_id="tight",
            repo_id="local",
            objective="Review docs",
            task_type="docs",
            risk_level="low",
            max_context_tokens=700,
            max_execution_tokens=900,
        )

        plan = DeterministicResourcePlanner().plan(task, local_repo(), audit())

        self.assertEqual(0, plan.context_budget)
        self.assertEqual("none", plan.steps[0].context_mode)
        self.assertTrue(any("context omitted" in note for note in plan.token_efficiency_notes))
        self.assertLessEqual(sum(step.token_budget for step in plan.steps), plan.total_token_budget)

    def test_same_input_produces_same_plan_content(self):
        task = PlanningTask(task_id="same", repo_id="local", objective="Review docs", task_type="docs", risk_level="low")
        planner = DeterministicResourcePlanner()

        first = planner.plan(task, local_repo(), audit()).to_dict()
        second = planner.plan(task, local_repo(), audit()).to_dict()

        self.assertEqual(first, second)


if __name__ == "__main__":
    unittest.main()
