"""Provider-free tests for repository-owned local focused checks."""

from __future__ import annotations

import os
import subprocess
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

            def runner(argv, *, cwd, timeout_seconds):
                del cwd, timeout_seconds
                self.assertEqual(argv[:3], ["git", "diff", "--check"])
                return 1, "", "failed"

            with mock.patch.object(
                local_verification, "_candidate_patch_sha256", return_value="a" * 64
            ), self.assertRaises(local_verification.LocalVerificationError) as ctx:
                local_verification.run_focused_checks(
                    worktree, ["git diff --check"], runner=runner
                )
            self.assertIn("focused_check_failed", ctx.exception.reason)

    def test_failed_check_returns_no_partial_success_list_to_caller(self):
        """Callers must not treat a raised error as having passed checks."""

        with tempfile.TemporaryDirectory() as tmp:
            worktree = Path(tmp)
            results = []

            def runner(argv, *, cwd, timeout_seconds):
                del cwd, timeout_seconds
                if argv[:3] == ["git", "diff", "--check"]:
                    return 0, "", ""
                return 7, "", "fail"

            with mock.patch.object(
                local_verification, "_candidate_patch_sha256", return_value="a" * 64
            ), self.assertRaises(local_verification.LocalVerificationError):
                results = local_verification.run_focused_checks(
                    worktree,
                    [
                        "git diff --check",
                        "python -m unittest discover -s tests -p test_agent_*.py",
                    ],
                    runner=runner,
                )
            self.assertEqual(results, [])

    def test_default_runner_uses_sanitized_bounded_process(self):
        import local_run_once as lro

        with tempfile.TemporaryDirectory() as tmp:
            worktree = Path(tmp)
            with mock.patch.object(
                local_verification, "_candidate_patch_sha256", return_value="a" * 64
            ), mock.patch.object(
                lro, "_bounded_process", return_value=(0, "", "")
            ) as bounded:
                results = local_verification.run_focused_checks(
                    worktree, ["git diff --check"]
                )
            self.assertEqual(results, [{"command": "git diff --check", "exit_code": 0}])
            bounded.assert_called_once()

    def test_diff_check_is_bound_to_head_and_includes_staged_changes(self):
        self.assertEqual(
            local_verification.allowlisted_command("git diff --check"),
            ["git", "diff", "--check", "HEAD"],
        )

    def test_focused_check_mutation_fails_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            worktree = Path(tmp)
            subprocess.run(["git", "init", "-q"], cwd=worktree, check=True)
            subprocess.run(["git", "config", "user.name", "Test"], cwd=worktree, check=True)
            subprocess.run(["git", "config", "user.email", "test@example.invalid"], cwd=worktree, check=True)
            target = worktree / "docs" / "CURRENT_STATUS.md"
            target.parent.mkdir()
            target.write_text("base\n", encoding="utf-8")
            subprocess.run(["git", "add", "docs/CURRENT_STATUS.md"], cwd=worktree, check=True)
            subprocess.run(["git", "commit", "-qm", "base"], cwd=worktree, check=True)
            target.write_text("candidate\n", encoding="utf-8")

            def mutating_runner(argv, *, cwd, timeout_seconds):
                del argv, timeout_seconds
                (Path(cwd) / "docs" / "CURRENT_STATUS.md").write_text(
                    "mutated by check\n", encoding="utf-8"
                )
                return 0, "", ""

            with self.assertRaises(local_verification.LocalVerificationError) as ctx:
                local_verification.run_focused_checks(
                    worktree, ["git diff --check"], runner=mutating_runner
                )
            self.assertEqual(ctx.exception.reason, "focused_check_mutated_candidate")

    def test_candidate_identity_git_uses_sanitized_env_and_temporary_index(self):
        import local_run_once as lro

        observed = []

        def fake_run(command, **kwargs):
            observed.append((command, kwargs["env"]))
            return mock.Mock(returncode=0, stdout=b"candidate-patch")

        with tempfile.TemporaryDirectory() as tmp, \
             mock.patch.dict(os.environ, {
                 "GH_TOKEN": "must-not-pass",
                 "GITHUB_TOKEN": "must-not-pass",
             }), \
             mock.patch.object(lro, "child_env", wraps=lro.child_env), \
             mock.patch.object(local_verification.subprocess, "run", side_effect=fake_run):
            digest = local_verification._candidate_patch_sha256(Path(tmp))
        self.assertRegex(digest, r"^[0-9a-f]{64}$")
        self.assertEqual(len(observed), 3)
        for _command, env in observed:
            self.assertNotIn("GH_TOKEN", env)
            self.assertNotIn("GITHUB_TOKEN", env)
            self.assertIn("GIT_INDEX_FILE", env)


if __name__ == "__main__":
    unittest.main()
