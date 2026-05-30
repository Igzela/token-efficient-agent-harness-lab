"""Static dashboard boundary checks."""

from __future__ import annotations

from pathlib import Path
import re
import unittest


REPO_ROOT = Path(__file__).resolve().parent.parent


class DashboardStaticTests(unittest.TestCase):
    def test_mvp4_workbench_non_executable_copy_is_visible(self):
        html = (REPO_ROOT / "web" / "dashboard" / "index.html").read_text(encoding="utf-8")

        self.assertIn("Plans are non-executable. Review actions are advisory only.", html)
        self.assertIn("Plan Review Workbench", html)

    def test_mvp5_guidance_preview_copy_is_visible(self):
        html = (REPO_ROOT / "web" / "dashboard" / "index.html").read_text(encoding="utf-8")

        self.assertIn("Review Guidance Preview", html)
        self.assertIn("Guidance is advisory only. It does not approve, execute, or mutate plans.", html)
        self.assertIn("Generate review guidance", html)

    def test_mvp6_portfolio_triage_copy_is_visible(self):
        html = (REPO_ROOT / "web" / "dashboard" / "index.html").read_text(encoding="utf-8")

        self.assertIn("Planning Portfolio Triage", html)
        self.assertIn(
            "Portfolio triage is advisory only. It does not approve, execute, mutate, assign, or write target repositories.",
            html,
        )
        self.assertIn("Refresh triage", html)

    def test_mvp8_operations_console_copy_is_visible(self):
        html = (REPO_ROOT / "web" / "dashboard" / "index.html").read_text(encoding="utf-8")

        self.assertNotIn("Harness App MVP5", html)
        self.assertNotIn("Plan Review Control Plane", html)
        self.assertIn("Harness App MVP8", html)
        self.assertIn("Operations Console", html)
        self.assertIn("Refresh status", html)
        self.assertIn("Audit selected repo", html)
        self.assertIn("Component Status Matrix", html)
        self.assertIn("Data Flow Status", html)
        self.assertIn("Storage Health", html)
        self.assertIn("Recent API Errors", html)
        self.assertIn("Recommended Debug Actions", html)
        self.assertIn("Tools", html)
        self.assertIn("Repository Audit", html)
        self.assertIn("Planning", html)
        self.assertIn("Plan Review", html)
        self.assertIn("Portfolio Triage", html)
        self.assertIn("Review Guidance", html)
        self.assertIn(
            "Operations diagnostics are read-only. They do not approve, execute, mutate, assign, call providers, launch workers or sandboxes, or write target repositories.",
            html,
        )

    def test_mvp8_first_screen_primary_actions_stay_minimal(self):
        html = (REPO_ROOT / "web" / "dashboard" / "index.html").read_text(encoding="utf-8")
        primary_actions = re.findall(r"<button[^>]*class=\"[^\"]*primary-action[^\"]*\"[^>]*>", html)

        self.assertLessEqual(len(primary_actions), 3, primary_actions)
        self.assertEqual(len(primary_actions), 2, primary_actions)

    def test_dashboard_button_labels_do_not_offer_execution_controls(self):
        html = (REPO_ROOT / "web" / "dashboard" / "index.html").read_text(encoding="utf-8")
        labels = [re.sub(r"\s+", " ", label).strip().lower() for label in re.findall(r"<button[^>]*>(.*?)</button>", html, re.DOTALL)]
        forbidden = {
            "approve",
            "assign",
            "apply",
            "run",
            "execute",
            "dispatch",
            "launch",
            "worker",
            "sandbox",
            "assign worker",
            "start task",
            "apply plan",
            "merge",
            "deploy",
        }

        self.assertTrue(labels)
        self.assertTrue(forbidden.isdisjoint(labels), labels)


if __name__ == "__main__":
    unittest.main()
