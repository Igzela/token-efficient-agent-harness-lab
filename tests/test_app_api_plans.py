"""Tests for MVP3 planning API handlers."""

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


def register_remote(registry_path: Path, repo_id: str = "remote") -> None:
    response = handle_api_request(
        "POST",
        "/api/repos",
        json.dumps({"id": repo_id, "name": "Remote", "kind": "remote", "url": "https://github.com/example/repo.git"}),
        registry_path,
    )
    if response.status_code != 201:
        raise AssertionError(response.body_json)


def plan_payload(repo_id: str = "target", **overrides) -> dict:
    payload = {
        "task_id": "docs",
        "repo_id": repo_id,
        "objective": "Review docs and propose a safe next slice",
        "task_type": "docs",
        "risk_level": "low",
        "max_context_tokens": 4000,
        "max_execution_tokens": 3000,
    }
    payload.update(overrides)
    return payload


class AppApiPlansTests(unittest.TestCase):
    def test_post_plan_for_local_repo_returns_ready_for_review(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / "target"
            target.mkdir()
            write_instance(target)
            registry_path = root / "registry.json"
            plans_path = root / "plans.json"
            register_local(registry_path, target)

            response = handle_api_request(
                "POST",
                "/api/plans",
                json.dumps(plan_payload()),
                registry_path,
                plans_path,
            )

        self.assertEqual(201, response.status_code)
        plan = response.body_json["plan"]
        self.assertEqual("ready_for_review", plan["status"])
        self.assertFalse(plan["executable"])
        self.assertEqual(plan["context_budget"] + plan["execution_budget"], plan["total_token_budget"])
        self.assertLessEqual(sum(step["token_budget"] for step in plan["steps"]), plan["total_token_budget"])

    def test_high_risk_plan_needs_approval(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / "target"
            target.mkdir()
            write_instance(target)
            registry_path = root / "registry.json"
            plans_path = root / "plans.json"
            register_local(registry_path, target)

            response = handle_api_request(
                "POST",
                "/api/plans",
                json.dumps(plan_payload(objective="Write provider config and deploy worker")),
                registry_path,
                plans_path,
            )

        self.assertEqual(201, response.status_code)
        plan = response.body_json["plan"]
        self.assertEqual("needs_approval", plan["status"])
        self.assertIn("human_approval_required", plan["approval_gates"])
        self.assertFalse(plan["executable"])

    def test_remote_repo_plan_is_blocked_without_audit_conflict(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            registry_path = root / "registry.json"
            plans_path = root / "plans.json"
            register_remote(registry_path)

            response = handle_api_request(
                "POST",
                "/api/plans",
                json.dumps(plan_payload("remote")),
                registry_path,
                plans_path,
            )

        self.assertEqual(200, response.status_code)
        plan = response.body_json["plan"]
        self.assertEqual("blocked", plan["status"])
        self.assertEqual(["remote_metadata_only"], plan["blockers"])
        self.assertFalse(plan["executable"])

    def test_blocked_audit_blocks_plan(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / "target"
            target.mkdir()
            registry_path = root / "registry.json"
            plans_path = root / "plans.json"
            register_local(registry_path, target)

            response = handle_api_request(
                "POST",
                "/api/plans",
                json.dumps(plan_payload()),
                registry_path,
                plans_path,
            )

        self.assertEqual(200, response.status_code)
        self.assertEqual("blocked", response.body_json["plan"]["status"])
        self.assertIn("audit_blocked", response.body_json["plan"]["blockers"])

    def test_post_plan_rejects_path_input(self):
        with tempfile.TemporaryDirectory() as tmp:
            response = handle_api_request(
                "POST",
                "/api/plans",
                json.dumps({"repo_id": "target", "path": "/tmp", "objective": "Review docs"}),
                Path(tmp) / "registry.json",
                Path(tmp) / "plans.json",
            )

        self.assertEqual(400, response.status_code)
        self.assertEqual("path_not_allowed", response.body_json["error"]["code"])

    def test_post_plan_rejects_invalid_budget(self):
        with tempfile.TemporaryDirectory() as tmp:
            response = handle_api_request(
                "POST",
                "/api/plans",
                json.dumps(plan_payload(max_context_tokens=-1)),
                Path(tmp) / "registry.json",
                Path(tmp) / "plans.json",
            )

        self.assertEqual(400, response.status_code)
        self.assertEqual("invalid_budget", response.body_json["error"]["code"])

    def test_post_plan_rejects_unknown_repo_id(self):
        with tempfile.TemporaryDirectory() as tmp:
            response = handle_api_request(
                "POST",
                "/api/plans",
                json.dumps(plan_payload("missing")),
                Path(tmp) / "registry.json",
                Path(tmp) / "plans.json",
            )

        self.assertEqual(404, response.status_code)
        self.assertEqual("invalid_repo_id", response.body_json["error"]["code"])

    def test_post_plan_rejects_malformed_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            response = handle_api_request(
                "POST",
                "/api/plans",
                "{not-json",
                Path(tmp) / "registry.json",
                Path(tmp) / "plans.json",
            )

        self.assertEqual(400, response.status_code)
        self.assertEqual("invalid_json", response.body_json["error"]["code"])

    def test_get_plan_returns_stored_plan(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / "target"
            target.mkdir()
            write_instance(target)
            registry_path = root / "registry.json"
            plans_path = root / "plans.json"
            register_local(registry_path, target)
            create_response = handle_api_request(
                "POST",
                "/api/plans",
                json.dumps(plan_payload()),
                registry_path,
                plans_path,
            )
            plan_id = create_response.body_json["plan"]["plan_id"]

            get_response = handle_api_request("GET", f"/api/plans/{plan_id}", None, registry_path, plans_path)

        self.assertEqual(200, get_response.status_code)
        self.assertEqual(plan_id, get_response.body_json["plan"]["plan_id"])

    def test_get_unknown_plan_returns_structured_404(self):
        with tempfile.TemporaryDirectory() as tmp:
            response = handle_api_request(
                "GET",
                "/api/plans/plan-missing",
                None,
                Path(tmp) / "registry.json",
                Path(tmp) / "plans.json",
            )

        self.assertEqual(404, response.status_code)
        self.assertEqual("invalid_plan_id", response.body_json["error"]["code"])

    def test_corrupt_plan_store_returns_structured_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            plans_path = root / "plans.json"
            plans_path.write_text("{not-json", encoding="utf-8")

            response = handle_api_request("GET", "/api/plans/plan-any", None, root / "registry.json", plans_path)

        self.assertEqual(500, response.status_code)
        self.assertEqual("plan_store_error", response.body_json["error"]["code"])
        self.assertNotIn("Traceback", json.dumps(response.body_json))

    def test_plan_store_inside_target_repo_is_rejected_without_writing(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / "target"
            target.mkdir()
            write_instance(target)
            registry_path = root / "registry.json"
            plans_path = target / ".harness_app_plans.json"
            register_local(registry_path, target)

            response = handle_api_request(
                "POST",
                "/api/plans",
                json.dumps(plan_payload()),
                registry_path,
                plans_path,
            )

        self.assertEqual(400, response.status_code)
        self.assertEqual("plan_store_inside_target_repo", response.body_json["error"]["code"])
        self.assertFalse(plans_path.exists())


if __name__ == "__main__":
    unittest.main()
