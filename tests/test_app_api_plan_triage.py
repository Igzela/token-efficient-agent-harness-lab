"""Tests for MVP6 planning portfolio triage API handlers."""

from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest

from harness_core.app_api import handle_api_request
from harness_core.plan_store import APP_PLANS_SCHEMA_VERSION


def plan(
    plan_id: str,
    status: str,
    *,
    repo_id: str = "repo",
    blockers: list[str] | None = None,
    notes: list[str] | None = None,
    audit_verdict: str = "PASS",
) -> dict:
    return {
        "plan_id": plan_id,
        "status": status,
        "executable": False,
        "effective_risk": "medium",
        "total_token_budget": 2400,
        "context_budget": 1200,
        "execution_budget": 1200,
        "blockers": blockers or [],
        "approval_gates": [],
        "token_efficiency_notes": notes or [],
        "audit_summary": {"verdict": audit_verdict},
        "repo_snapshot": {"id": repo_id, "kind": "local"},
        "task": {"repo_id": repo_id, "task_type": "review"},
        "steps": [{"role": "planner", "context_mode": "summary"}],
    }


def write_plans(path: Path, plans: list[dict]) -> None:
    path.write_text(
        json.dumps({"schema_version": APP_PLANS_SCHEMA_VERSION, "plans": plans}, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def target_file_set(path: Path) -> set[str]:
    return {str(item.relative_to(path)) for item in path.rglob("*") if item.is_file()}


class AppApiPlanTriageTests(unittest.TestCase):
    def test_get_plan_triage_returns_summary_and_items(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            plans_path = root / "plans.json"
            write_plans(
                plans_path,
                [
                    plan("ready", "ready_for_review"),
                    plan("blocked", "blocked", blockers=["audit_blocked"], audit_verdict="BLOCKED"),
                ],
            )

            response = handle_api_request("GET", "/api/plans/triage", None, root / "registry.json", plans_path)

        self.assertEqual(200, response.status_code)
        triage = response.body_json["triage"]
        self.assertTrue(triage["generated_from_store_only"])
        self.assertFalse(triage["persistent"])
        self.assertTrue(triage["non_executable"])
        self.assertEqual(2, triage["total_plans"])
        self.assertEqual(["blocked", "ready"], [item["plan_id"] for item in triage["items"]])

    def test_get_plan_triage_repo_filter(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            plans_path = root / "plans.json"
            write_plans(
                plans_path,
                [
                    plan("one", "ready_for_review", repo_id="one"),
                    plan("two", "ready_for_review", repo_id="two"),
                ],
            )

            response = handle_api_request(
                "GET",
                "/api/plans/triage?repo_id=two",
                None,
                root / "registry.json",
                plans_path,
            )

        self.assertEqual(200, response.status_code)
        self.assertEqual("two", response.body_json["triage"]["repo_id"])
        self.assertEqual(["two"], [item["plan_id"] for item in response.body_json["triage"]["items"]])

    def test_get_plan_triage_invalid_limit_returns_structured_400(self):
        with tempfile.TemporaryDirectory() as tmp:
            response = handle_api_request(
                "GET",
                "/api/plans/triage?limit=abc",
                None,
                Path(tmp) / "registry.json",
                Path(tmp) / "plans.json",
            )

        self.assertEqual(400, response.status_code)
        self.assertEqual("invalid_plan_triage_request", response.body_json["error"]["code"])

    def test_get_plan_triage_limit_above_max_returns_structured_400(self):
        with tempfile.TemporaryDirectory() as tmp:
            response = handle_api_request(
                "GET",
                "/api/plans/triage?limit=101",
                None,
                Path(tmp) / "registry.json",
                Path(tmp) / "plans.json",
            )

        self.assertEqual(400, response.status_code)
        self.assertEqual("invalid_plan_triage_request", response.body_json["error"]["code"])

    def test_plan_triage_route_is_not_treated_as_plan_id(self):
        with tempfile.TemporaryDirectory() as tmp:
            response = handle_api_request(
                "GET",
                "/api/plans/triage",
                None,
                Path(tmp) / "registry.json",
                Path(tmp) / "plans.json",
            )

        self.assertEqual(200, response.status_code)
        self.assertEqual("plan_triage.v1", response.body_json["triage"]["schema_version"])

    def test_plan_triage_endpoint_is_get_only(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            plans_path = root / "plans.json"
            write_plans(plans_path, [plan("ready", "ready_for_review")])
            before = plans_path.read_text(encoding="utf-8")

            response = handle_api_request(
                "POST",
                "/api/plans/triage",
                "{}",
                root / "registry.json",
                plans_path,
            )
            after = plans_path.read_text(encoding="utf-8")

        self.assertEqual(404, response.status_code)
        self.assertEqual("not_found", response.body_json["error"]["code"])
        self.assertEqual(before, after)

    def test_corrupt_plan_store_returns_structured_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            plans_path = root / "plans.json"
            plans_path.write_text("{not-json", encoding="utf-8")

            response = handle_api_request("GET", "/api/plans/triage", None, root / "registry.json", plans_path)

        self.assertEqual(500, response.status_code)
        self.assertEqual("plan_store_error", response.body_json["error"]["code"])
        self.assertNotIn("Traceback", json.dumps(response.body_json))

    def test_plan_triage_does_not_change_plan_store_contents(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            plans_path = root / "plans.json"
            write_plans(plans_path, [plan("ready", "ready_for_review")])
            before = plans_path.read_text(encoding="utf-8")

            response = handle_api_request("GET", "/api/plans/triage", None, root / "registry.json", plans_path)
            after = plans_path.read_text(encoding="utf-8")

        self.assertEqual(200, response.status_code)
        self.assertEqual(before, after)

    def test_plan_triage_does_not_write_target_repo(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / "target"
            target.mkdir()
            (target / "AGENTS.md").write_text("target instructions\n", encoding="utf-8")
            plans_path = root / "plans.json"
            item = plan("ready", "ready_for_review")
            item["repo_snapshot"]["canonical_path"] = str(target)
            write_plans(plans_path, [item])
            before = target_file_set(target)

            response = handle_api_request("GET", "/api/plans/triage", None, root / "registry.json", plans_path)
            after = target_file_set(target)

        self.assertEqual(200, response.status_code)
        self.assertEqual(before, after)


if __name__ == "__main__":
    unittest.main()
