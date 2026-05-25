"""Tests for MVP7 read-only app diagnostics."""

from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest

from harness_core.app_diagnostics import (
    APP_DIAGNOSTICS_SCHEMA_VERSION,
    APP_STATUS_SCHEMA_VERSION,
    DIAGNOSTICS_BOUNDARY_NOTICE,
    build_app_diagnostics,
    build_app_status,
    derive_recent_errors,
)
from harness_core.plan_store import APP_PLANS_SCHEMA_VERSION


def write_registry(path: Path) -> None:
    path.write_text(
        json.dumps(
            {
                "schema_version": "app_registry.v1",
                "repos": [
                    {
                        "id": "target",
                        "name": "Target",
                        "kind": "local",
                        "path": str(path.parent),
                    }
                ],
            },
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )


def write_plans(path: Path) -> None:
    path.write_text(
        json.dumps(
            {
                "schema_version": APP_PLANS_SCHEMA_VERSION,
                "plans": [
                    {
                        "plan_id": "plan",
                        "status": "ready_for_review",
                        "executable": False,
                        "total_token_budget": 100,
                        "context_budget": 50,
                        "execution_budget": 50,
                        "steps": [],
                        "approval_gates": [],
                        "blockers": [],
                        "token_efficiency_notes": [],
                    }
                ],
            },
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )


class AppDiagnosticsTests(unittest.TestCase):
    def test_app_status_reports_component_matrix(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            registry_path = root / "registry.json"
            plans_path = root / "plans.json"
            write_registry(registry_path)
            write_plans(plans_path)

            status = build_app_status(registry_path, plans_path)

        self.assertEqual(APP_STATUS_SCHEMA_VERSION, status["schema_version"])
        self.assertEqual("ok", status["status"])
        self.assertEqual(DIAGNOSTICS_BOUNDARY_NOTICE, status["boundary_notice"])
        components = {component["component"]: component for component in status["components"]}
        self.assertIn("app_server", components)
        self.assertIn("app_registry", components)
        self.assertIn("plan_store", components)
        self.assertIn("dashboard_frontend", components)
        self.assertIn("security_boundary", components)
        self.assertEqual("ok", components["plan_store"]["status"])

    def test_missing_state_files_are_warnings_not_errors(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)

            status = build_app_status(root / "missing-registry.json", root / "missing-plans.json")

        self.assertEqual("warning", status["status"])
        components = {component["component"]: component for component in status["components"]}
        self.assertEqual("warning", components["app_registry"]["status"])
        self.assertEqual("warning", components["plan_store"]["status"])

    def test_corrupt_plan_store_surfaces_recent_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            registry_path = root / "registry.json"
            plans_path = root / "plans.json"
            write_registry(registry_path)
            plans_path.write_text("{not-json", encoding="utf-8")

            diagnostics = build_app_diagnostics(registry_path, plans_path)

        self.assertEqual(APP_DIAGNOSTICS_SCHEMA_VERSION, diagnostics["schema_version"])
        self.assertEqual("blocked", diagnostics["status"])
        self.assertTrue(diagnostics["recent_errors"])
        self.assertEqual("plan_store", diagnostics["recent_errors"][0]["component"])
        self.assertTrue(diagnostics["recommended_debug_actions"])

    def test_corrupt_registry_surfaces_recent_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            registry_path = root / "registry.json"
            plans_path = root / "plans.json"
            registry_path.write_text("{not-json", encoding="utf-8")
            write_plans(plans_path)

            status = build_app_status(registry_path, plans_path)
            recent_errors = derive_recent_errors(status)

        self.assertEqual("blocked", status["status"])
        self.assertEqual("app_registry", recent_errors["errors"][0]["component"])
        self.assertFalse(recent_errors["persistent"])

    def test_diagnostics_do_not_mutate_input_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            registry_path = root / "registry.json"
            plans_path = root / "plans.json"
            write_registry(registry_path)
            write_plans(plans_path)
            registry_before = registry_path.read_text(encoding="utf-8")
            plans_before = plans_path.read_text(encoding="utf-8")

            build_app_diagnostics(registry_path, plans_path)
            registry_after = registry_path.read_text(encoding="utf-8")
            plans_after = plans_path.read_text(encoding="utf-8")

        self.assertEqual(registry_before, registry_after)
        self.assertEqual(plans_before, plans_after)


if __name__ == "__main__":
    unittest.main()
