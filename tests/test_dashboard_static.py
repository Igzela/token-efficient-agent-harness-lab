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
