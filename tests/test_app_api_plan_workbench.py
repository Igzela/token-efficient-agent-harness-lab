"""Tests for MVP4 read-only plan workbench API handlers."""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from harness_core.app_api import handle_api_request


VALID_AGENTS = """# Agent Execution Policy

Claude Code operates as an execution adapter under the Token-Efficient Agent Harness.
It is not the governance authority.
All governance decisions require human authorisation.
Claude Code must pause before pushing directly to `main` or `master`.
Claude Code must not connect real LLM provider systems without approval.
Claude Code must pause before modifying active YAML or active user/project state.
"""


def write_instance(root: Path) -> None:
    harness = root / "docs" / "harness"
    harness.mkdir(parents=True)
    (root / "AGENTS.md").write_text(VALID_AGENTS, encoding="utf-8")
    (harness / "PROJECT_BOARD.md").write_text(
        "# Project Board\n\n## Task States\n\ntodo ready running done\n\nPhase 1 Final Gate: PASS\n",
        encoding="utf-8",
    )
    (harness / "TASK_QUEUE.md").write_text(
        "### P1-001\n\n**Status**: done\n**Goal**: test\n",
        encoding="utf-8",
    )
    (harness / "QUALITY_GATES.md").write_text(
        "unknown_error requires human review\nprovider boundary\nactive state mutation requires approval\nauto modification forbidden\nread-only boundary\n",
        encoding="utf-8",
    )
    (harness / "DECISION_RECORD.md").write_text("# Decision Record\n", encoding="utf-8")
    (harness / "RISK_REGISTER.md").write_text(
        "active risk\nmitigated risk\nprovider/LLM premature integration\nscope drift\nmutation active state\n",
        encoding="utf-8",
    )


def register_local(registry_path: Path, target: Path, repo_id: str = "target") -> None:
    response = handle_api_request(
        "POST",
        "/api/repos",
        json.dumps({"id": repo_id, "name": "Target", "kind": "local", "path": str(target)}),
        registry_path,
    )
    if response.status_code != 201:
        raise AssertionError(response.body_json)


def create_plan(registry_path: Path, plans_path: Path, repo_id: str, task_id: str, **overrides) -> dict:
    payload = {
        "task_id": task_id,
        "repo_id": repo_id,
        "objective": "Review docs and propose a safe next slice",
        "task_type": "docs",
        "risk_level": "low",
        "max_context_tokens": 4000,
        "max_execution_tokens": 3000,
    }
    payload.update(overrides)
    response = handle_api_request("POST", "/api/plans", json.dumps(payload), registry_path, plans_path)
    if response.status_code not in {200, 201}:
        raise AssertionError(response.body_json)
    return response.body_json["plan"]


def target_file_set(root: Path) -> set[str]:
    return {str(path.relative_to(root)) for path in root.rglob("*")}


class AppApiPlanWorkbenchTests(unittest.TestCase):
    def test_get_plans_returns_list_summaries(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / "target"
            target.mkdir()
            write_instance(target)
            registry_path = root / "registry.json"
            plans_path = root / "plans.json"
            register_local(registry_path, target)
            create_plan(registry_path, plans_path, "target", "docs")

            response = handle_api_request("GET", "/api/plans", None, registry_path, plans_path)

        self.assertEqual(200, response.status_code)
        self.assertTrue(response.body_json["ok"])
        self.assertEqual(1, len(response.body_json["plans"]))
        item = response.body_json["plans"][0]
        self.assertEqual("target", item["repo_id"])
        self.assertEqual("ready_for_review", item["status"])
        self.assertFalse(item["executable"])
        self.assertNotIn("steps", item)

    def test_get_plans_filters_by_repo_and_status(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            one = root / "one"
            two = root / "two"
            one.mkdir()
            two.mkdir()
            write_instance(one)
            write_instance(two)
            registry_path = root / "registry.json"
            plans_path = root / "plans.json"
            register_local(registry_path, one, "one")
            register_local(registry_path, two, "two")
            create_plan(registry_path, plans_path, "one", "one-docs")
            create_plan(
                registry_path,
                plans_path,
                "two",
                "two-risk",
                objective="Write provider config and deploy worker",
            )

            response = handle_api_request(
                "GET",
                "/api/plans?repo_id=two&status=needs_approval",
                None,
                registry_path,
                plans_path,
            )

        self.assertEqual(200, response.status_code)
        self.assertEqual(1, len(response.body_json["plans"]))
        self.assertEqual("two", response.body_json["plans"][0]["repo_id"])
        self.assertEqual("needs_approval", response.body_json["plans"][0]["status"])

    def test_get_plan_summary_returns_counts(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / "target"
            blocked = root / "blocked"
            target.mkdir()
            blocked.mkdir()
            write_instance(target)
            registry_path = root / "registry.json"
            plans_path = root / "plans.json"
            register_local(registry_path, target, "target")
            register_local(registry_path, blocked, "blocked")
            create_plan(registry_path, plans_path, "target", "ready")
            create_plan(
                registry_path,
                plans_path,
                "target",
                "gated",
                objective="Write provider config and deploy worker",
            )
            create_plan(registry_path, plans_path, "blocked", "blocked")

            response = handle_api_request("GET", "/api/plans/summary", None, registry_path, plans_path)

        self.assertEqual(200, response.status_code)
        summary = response.body_json["summary"]
        self.assertEqual(3, summary["total_plans"])
        self.assertEqual(1, summary["by_status"]["ready_for_review"])
        self.assertEqual(1, summary["by_status"]["needs_approval"])
        self.assertEqual(1, summary["by_status"]["blocked"])
        self.assertEqual(2, summary["plans_with_approval_gates"])

    def test_get_plan_compare_returns_deterministic_comparison(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / "target"
            target.mkdir()
            write_instance(target)
            registry_path = root / "registry.json"
            plans_path = root / "plans.json"
            register_local(registry_path, target)
            first = create_plan(registry_path, plans_path, "target", "first", max_context_tokens=4000)
            second = create_plan(registry_path, plans_path, "target", "second", max_context_tokens=700)

            response = handle_api_request(
                "GET",
                f"/api/plans/compare?plan_id={first['plan_id']}&plan_id={second['plan_id']}",
                None,
                registry_path,
                plans_path,
            )

        self.assertEqual(200, response.status_code)
        comparison = response.body_json["comparison"]
        self.assertTrue(comparison["same_repo"])
        self.assertEqual(-2500, comparison["context_budget_delta"])
        self.assertLess(comparison["token_budget_delta"], 0)
        self.assertTrue(comparison["context_mode_changes"])

    def test_get_plan_compare_missing_query_returns_structured_400(self):
        with tempfile.TemporaryDirectory() as tmp:
            response = handle_api_request(
                "GET",
                "/api/plans/compare?plan_id=one",
                None,
                Path(tmp) / "registry.json",
                Path(tmp) / "plans.json",
            )

        self.assertEqual(400, response.status_code)
        self.assertEqual("invalid_plan_workbench_request", response.body_json["error"]["code"])

    def test_get_plan_compare_duplicate_id_returns_structured_400(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / "target"
            target.mkdir()
            write_instance(target)
            registry_path = root / "registry.json"
            plans_path = root / "plans.json"
            register_local(registry_path, target)
            first = create_plan(registry_path, plans_path, "target", "first")

            response = handle_api_request(
                "GET",
                f"/api/plans/compare?plan_id={first['plan_id']}&plan_id={first['plan_id']}",
                None,
                registry_path,
                plans_path,
            )

        self.assertEqual(400, response.status_code)
        self.assertEqual("invalid_plan_workbench_request", response.body_json["error"]["code"])

    def test_get_plan_compare_unknown_plan_returns_structured_404(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / "target"
            target.mkdir()
            write_instance(target)
            registry_path = root / "registry.json"
            plans_path = root / "plans.json"
            register_local(registry_path, target)
            first = create_plan(registry_path, plans_path, "target", "first")

            response = handle_api_request(
                "GET",
                f"/api/plans/compare?plan_id={first['plan_id']}&plan_id=missing",
                None,
                registry_path,
                plans_path,
            )

        self.assertEqual(404, response.status_code)
        self.assertEqual("invalid_plan_id", response.body_json["error"]["code"])

    def test_summary_and_compare_routes_are_not_treated_as_plan_ids(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            summary_response = handle_api_request("GET", "/api/plans/summary", None, root / "registry.json", root / "plans.json")
            compare_response = handle_api_request("GET", "/api/plans/compare", None, root / "registry.json", root / "plans.json")

        self.assertEqual(200, summary_response.status_code)
        self.assertIn("summary", summary_response.body_json)
        self.assertEqual(400, compare_response.status_code)
        self.assertEqual("invalid_plan_workbench_request", compare_response.body_json["error"]["code"])

    def test_get_plans_rejects_invalid_limit(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            zero_response = handle_api_request("GET", "/api/plans?limit=0", None, root / "registry.json", root / "plans.json")
            huge_response = handle_api_request("GET", "/api/plans?limit=101", None, root / "registry.json", root / "plans.json")
            text_response = handle_api_request("GET", "/api/plans?limit=many", None, root / "registry.json", root / "plans.json")

        self.assertEqual(400, zero_response.status_code)
        self.assertEqual(400, huge_response.status_code)
        self.assertEqual(400, text_response.status_code)
        self.assertEqual("invalid_plan_workbench_request", zero_response.body_json["error"]["code"])

    def test_corrupt_plan_store_returns_structured_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            plans_path = root / "plans.json"
            plans_path.write_text("{not-json", encoding="utf-8")

            response = handle_api_request("GET", "/api/plans/summary", None, root / "registry.json", plans_path)

        self.assertEqual(500, response.status_code)
        self.assertEqual("plan_store_error", response.body_json["error"]["code"])
        self.assertNotIn("Traceback", json.dumps(response.body_json))

    def test_workbench_endpoints_do_not_write_target_repo(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / "target"
            target.mkdir()
            write_instance(target)
            registry_path = root / "registry.json"
            plans_path = root / "plans.json"
            register_local(registry_path, target)
            first = create_plan(registry_path, plans_path, "target", "first")
            second = create_plan(registry_path, plans_path, "target", "second", max_context_tokens=700)
            before = target_file_set(target)

            list_response = handle_api_request("GET", "/api/plans", None, registry_path, plans_path)
            summary_response = handle_api_request("GET", "/api/plans/summary", None, registry_path, plans_path)
            compare_response = handle_api_request(
                "GET",
                f"/api/plans/compare?plan_id={first['plan_id']}&plan_id={second['plan_id']}",
                None,
                registry_path,
                plans_path,
            )
            after = target_file_set(target)

        self.assertEqual(200, list_response.status_code)
        self.assertEqual(200, summary_response.status_code)
        self.assertEqual(200, compare_response.status_code)
        self.assertEqual(before, after)


if __name__ == "__main__":
    unittest.main()
