"""Provider-free tests for repository-owned local focused checks."""

from __future__ import annotations

import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "scripts", "agent-control"))
import local_verification  # noqa: E402


class TestLocalVerification(unittest.TestCase):
    def test_select_issue_checks_adds_agent_python_suite_for_control_plane_paths(self):
        selected = local_verification.select_issue_checks(
            ["scripts/agent-control/local_run_once.py", "docs/NEXT_DECISION.md"]
        )
        self.assertEqual(
            selected,
            [
                "git diff --check",
                "python -m unittest discover -s tests -p test_agent_*.py",
                "python scripts/check_agent_handoff.py",
            ],
        )

    def test_docs_only_paths_keep_diff_check_and_handoff_when_canonical(self):
        selected = local_verification.select_issue_checks(["docs/NEXT_DECISION.md"])
        self.assertEqual(
            selected,
            ["git diff --check", "python scripts/check_agent_handoff.py"],
        )

    def test_prose_non_handoff_doc_is_diff_check_only(self):
        selected = local_verification.select_issue_checks(["docs/stage0/readme.md"])
        self.assertEqual(selected, ["git diff --check"])

    def test_unmapped_source_path_fails_closed(self):
        with self.assertRaises(local_verification.LocalVerificationError) as ctx:
            local_verification.select_issue_checks(["src/unknown_module.py"])
        self.assertIn("path_unsupported", ctx.exception.reason)

    def test_workflow_and_rust_and_dashboard_select_their_checks(self):
        selected = local_verification.select_issue_checks(
            [
                "engine/src/lib.rs",
                "dashboard/src/App.tsx",
                ".github/workflows/tests.yml",
            ]
        )
        self.assertIn("cargo fmt --all -- --check", selected)
        self.assertIn("cargo clippy -p engine --all-targets --all-features -- -D warnings", selected)
        self.assertIn("cargo test -p engine", selected)
        self.assertIn("bun run typecheck", selected)
        self.assertIn("bun test", selected)
        self.assertIn("python tools/check_security_baseline.py", selected)

    def test_rejected_lockfile_and_secret_shaped_paths(self):
        for path in (
            "dashboard/package-lock.json",
            "engine.pid",
            ".codegraph/index.db",
            "secrets.env",
        ):
            with self.subTest(path=path):
                with self.assertRaises(local_verification.LocalVerificationError) as ctx:
                    local_verification.select_issue_checks([path])
                self.assertTrue(
                    ctx.exception.reason.startswith("path_rejected:")
                    or ctx.exception.reason.startswith("path_unsupported:"),
                    ctx.exception.reason,
                )

    def test_plan_verification_must_be_allowlisted(self):
        with self.assertRaises(local_verification.LocalVerificationError) as ctx:
            local_verification.select_plan_checks(
                ["rm -rf /"],
                ["docs/stage0/readme.md"],
            )
        self.assertIn("plan_verification_not_allowlisted", ctx.exception.reason)

    def test_run_focused_checks_records_exit_codes_and_fails_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            worktree = Path(tmp)
            completed = mock.Mock(returncode=1)
            with mock.patch.object(
                local_verification.subprocess, "run", return_value=completed
            ) as run:
                with self.assertRaises(local_verification.LocalVerificationError) as ctx:
                    local_verification.run_focused_checks(
                        worktree, ["git diff --check"]
                    )
            self.assertIn("focused_check_failed", ctx.exception.reason)
            run.assert_called_once()
            self.assertEqual(run.call_args.args[0][:3], ["git", "diff", "--check"])

    def test_failed_check_returns_no_partial_success_list_to_caller(self):
        """Callers must not treat a raised error as having passed checks."""

        with tempfile.TemporaryDirectory() as tmp:
            worktree = Path(tmp)
            results = []

            def runner(argv, **kwargs):
                del kwargs
                if argv[:3] == ["git", "diff", "--check"]:
                    return mock.Mock(returncode=0)
                return mock.Mock(returncode=7)

            with self.assertRaises(local_verification.LocalVerificationError):
                results = local_verification.run_focused_checks(
                    worktree,
                    [
                        "git diff --check",
                        "python -m unittest discover -s tests -p test_agent_*.py",
                    ],
                    runner=runner,
                )
            self.assertEqual(results, [])


if __name__ == "__main__":
    unittest.main()
