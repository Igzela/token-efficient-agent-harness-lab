"""Tests for MVP4 read-only plan review workbench views."""

from __future__ import annotations

import unittest

from harness_core.plan_workbench import (
    PlanFilters,
    PlanWorkbenchError,
    compare_plans,
    list_plan_summaries,
    recommend_next_review_action,
    summarize_plans,
)


def plan(
    plan_id: str,
    status: str,
    *,
    repo_id: str = "repo",
    repo_kind: str = "local",
    risk: str = "low",
    total: int = 1000,
    context: int = 400,
    execution: int = 600,
    task_type: str = "docs",
    blockers: list[str] | None = None,
    gates: list[str] | None = None,
    notes: list[str] | None = None,
    audit_verdict: str = "PASS",
    step_modes: list[str] | None = None,
) -> dict:
    modes = step_modes or ["excerpt", "summary"]
    return {
        "plan_id": plan_id,
        "status": status,
        "effective_risk": risk,
        "executable": False,
        "total_token_budget": total,
        "context_budget": context,
        "execution_budget": execution,
        "task": {
            "repo_id": repo_id,
            "task_id": plan_id,
            "task_type": task_type,
        },
        "repo_snapshot": {
            "id": repo_id,
            "kind": repo_kind,
        },
        "audit_summary": {
            "verdict": audit_verdict,
        },
        "steps": [
            {"role": "planner", "context_mode": mode, "token_budget": 100}
            for mode in modes
        ],
        "approval_gates": gates or [],
        "blockers": blockers or [],
        "token_efficiency_notes": notes or [],
    }


class PlanWorkbenchTests(unittest.TestCase):
    def test_summarize_empty_plan_store_returns_zero_counts(self):
        summary = summarize_plans([])

        self.assertEqual(0, summary["total_plans"])
        self.assertEqual(0, summary["total_token_budget"])
        self.assertEqual(0, summary["average_token_budget"])
        self.assertEqual(0, summary["by_status"]["ready_for_review"])

    def test_summary_counts_status_repo_kind_and_budget(self):
        plans = [
            plan("a", "ready_for_review", total=1000),
            plan("b", "needs_approval", gates=["human_approval_required"], total=2000),
            plan("c", "blocked", repo_id="remote", repo_kind="remote", blockers=["remote_metadata_only"], total=3000),
        ]

        summary = summarize_plans(plans)

        self.assertEqual(3, summary["total_plans"])
        self.assertEqual(1, summary["by_status"]["ready_for_review"])
        self.assertEqual(1, summary["by_status"]["needs_approval"])
        self.assertEqual(1, summary["by_status"]["blocked"])
        self.assertEqual(2, summary["by_repo_kind"]["local"])
        self.assertEqual(1, summary["by_repo_kind"]["remote"])
        self.assertEqual(6000, summary["total_token_budget"])
        self.assertEqual(2000, summary["average_token_budget"])
        self.assertEqual(1, summary["plans_with_blockers"])
        self.assertEqual(1, summary["plans_with_approval_gates"])

    def test_summary_filters_by_repo_id(self):
        plans = [
            plan("a", "ready_for_review", repo_id="one", total=1000),
            plan("b", "ready_for_review", repo_id="two", total=3000),
        ]

        summary = summarize_plans(plans, repo_id="one")

        self.assertEqual(1, summary["total_plans"])
        self.assertEqual(1000, summary["total_token_budget"])

    def test_recommend_review_actions(self):
        cases = [
            (plan("remote", "blocked", blockers=["remote_metadata_only"]), "review_remote_limit"),
            (plan("audit", "blocked", blockers=["audit_blocked"], audit_verdict="BLOCKED"), "review_audit_failure"),
            (plan("other", "blocked", blockers=["manual_blocker"]), "review_blockers"),
            (plan("approval", "needs_approval", gates=["human_approval_required"]), "review_approval_gates"),
            (
                plan("budget", "ready_for_review", notes=["Context budget pressure: full context reduced."]),
                "review_token_budget",
            ),
            (plan("clean", "ready_for_review"), "review_steps"),
        ]

        for input_plan, expected in cases:
            with self.subTest(expected=expected):
                self.assertEqual(expected, recommend_next_review_action(input_plan))

    def test_high_budget_ready_plan_recommends_token_budget_review(self):
        input_plan = plan("broad", "ready_for_review", total=7800, context=6000, execution=1800, notes=[])

        self.assertEqual("review_token_budget", recommend_next_review_action(input_plan))

    def test_list_plan_summaries_filters_status_and_limit(self):
        plans = [
            plan("a", "ready_for_review", risk="low"),
            plan("b", "blocked", risk="critical"),
            plan("c", "blocked", risk="high"),
        ]

        summaries = list_plan_summaries(plans, PlanFilters(status="blocked", limit=1))

        self.assertEqual(1, len(summaries))
        self.assertEqual("b", summaries[0]["plan_id"])
        self.assertEqual("blocked", summaries[0]["status"])

    def test_compare_two_plans_returns_budget_and_context_deltas(self):
        plans = [
            plan("a", "ready_for_review", total=5000, context=3000, execution=2000, step_modes=["full", "summary"]),
            plan(
                "b",
                "needs_approval",
                total=3000,
                context=1000,
                execution=2000,
                gates=["human_approval_required"],
                step_modes=["summary", "summary", "none"],
            ),
        ]

        comparison = compare_plans(plans, ["a", "b"])

        self.assertTrue(comparison["same_repo"])
        self.assertEqual("ready_for_review->needs_approval", comparison["status_delta"])
        self.assertEqual(-2000, comparison["token_budget_delta"])
        self.assertEqual(-2000, comparison["context_budget_delta"])
        self.assertEqual(0, comparison["execution_budget_delta"])
        self.assertEqual(1, comparison["step_count_delta"])
        self.assertEqual(1, comparison["approval_gate_delta"])
        self.assertEqual([{"step_index": 0, "a": "full", "b": "summary"}, {"step_index": 2, "a": None, "b": "none"}], comparison["context_mode_changes"])

    def test_compare_rejects_missing_or_wrong_count_plan_ids(self):
        plans = [plan("a", "ready_for_review")]

        with self.assertRaises(KeyError):
            compare_plans(plans, ["a", "missing"])
        with self.assertRaises(PlanWorkbenchError):
            compare_plans(plans, ["a"])
        with self.assertRaises(PlanWorkbenchError):
            compare_plans(plans, ["a", "a", "a"])
        with self.assertRaises(PlanWorkbenchError):
            compare_plans(plans, ["a", "a"])


if __name__ == "__main__":
    unittest.main()
