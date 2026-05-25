"""Tests for MVP7 app diagnostics API handlers."""

from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest

from harness_core.app_api import handle_api_request
from harness_core.plan_store import APP_PLANS_SCHEMA_VERSION


def write_registry(path: Path, target: Path) -> None:
    path.write_text(
        json.dumps(
            {
                "schema_version": "app_registry.v1",
                "repos": [{"id": "target", "name": "Target", "kind": "local", "path": str(target)}],
            },
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )


def write_plans(path: Path) -> None:
    path.write_text(
        json.dumps({"schema_version": APP_PLANS_SCHEMA_VERSION, "plans": []}, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def target_file_set(path: Path) -> set[str]:
    return {str(item.relative_to(path)) for item in path.rglob("*") if item.is_file()}


class AppApiDiagnosticsTests(unittest.TestCase):
    def test_get_app_status_returns_component_status(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / "target"
            target.mkdir()
            registry_path = root / "registry.json"
            plans_path = root / "plans.json"
            write_registry(registry_path, target)
            write_plans(plans_path)

            response = handle_api_request("GET", "/api/app/status", None, registry_path, plans_path)

        self.assertEqual(200, response.status_code)
        self.assertTrue(response.body_json["ok"])
        self.assertEqual("app_status.v1", response.body_json["status"]["schema_version"])
        self.assertIn("components", response.body_json["status"])

    def test_get_app_diagnostics_returns_debug_sections(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            response = handle_api_request(
                "GET",
                "/api/app/diagnostics",
                None,
                root / "missing-registry.json",
                root / "missing-plans.json",
            )

        self.assertEqual(200, response.status_code)
        diagnostics = response.body_json["diagnostics"]
        self.assertEqual("app_diagnostics.v1", diagnostics["schema_version"])
        self.assertIn("system_overview", diagnostics)
        self.assertIn("data_flow", diagnostics)
        self.assertIn("storage", diagnostics)
        self.assertIn("recommended_debug_actions", diagnostics)

    def test_get_app_recent_errors_returns_derived_errors(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            registry_path = root / "registry.json"
            plans_path = root / "plans.json"
            registry_path.write_text("{not-json", encoding="utf-8")
            write_plans(plans_path)

            response = handle_api_request("GET", "/api/app/recent-errors", None, registry_path, plans_path)

        self.assertEqual(200, response.status_code)
        recent_errors = response.body_json["recent_errors"]
        self.assertEqual("app_recent_errors.v1", recent_errors["schema_version"])
        self.assertFalse(recent_errors["persistent"])
        self.assertEqual("app_registry", recent_errors["errors"][0]["component"])
        self.assertNotIn("Traceback", json.dumps(response.body_json))
        self.assertNotIn("{not-json", json.dumps(response.body_json))

    def test_diagnostics_endpoints_are_get_only(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            registry_path = root / "registry.json"
            plans_path = root / "plans.json"
            write_plans(plans_path)
            before = plans_path.read_text(encoding="utf-8")

            response = handle_api_request("POST", "/api/app/status", "{}", registry_path, plans_path)
            after = plans_path.read_text(encoding="utf-8")

        self.assertEqual(404, response.status_code)
        self.assertEqual("not_found", response.body_json["error"]["code"])
        self.assertEqual(before, after)

    def test_diagnostics_do_not_write_target_repo(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / "target"
            target.mkdir()
            (target / "AGENTS.md").write_text("target instructions\n", encoding="utf-8")
            registry_path = root / "registry.json"
            plans_path = root / "plans.json"
            write_registry(registry_path, target)
            write_plans(plans_path)
            before = target_file_set(target)

            response = handle_api_request("GET", "/api/app/diagnostics", None, registry_path, plans_path)
            after = target_file_set(target)

        self.assertEqual(200, response.status_code)
        self.assertEqual(before, after)


if __name__ == "__main__":
    unittest.main()
