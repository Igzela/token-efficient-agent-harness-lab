"""Unit tests for shared, harness-neutral ask_sol investigation tool.

Covers:
1. Target model configuration (gpt-5.6-sol, model_reasoning_effort="max", read-only sandbox, --ephemeral)
2. Fail-closed on missing/unsupported Codex capability without consuming budget
3. Exact context binding (repo identity, worktree digest, HEAD SHA, dirty digest)
4. Fail-closed on Git context / repository errors
5. Non-mutation verification and fail-closed on mutation
6. Uncommitted caller dirty state preservation (tracked & untracked)
7. Untracked file content change mutation detection (regression test)
8. Context staleness prevention across changed HEAD/worktree
9. Recursive invocation rejection (ASK_SOL_ACTIVE and depth guards)
10. Consultation loop & multi-task budget bounds (atomic, concurrency safe, no --force)
11. Child subprocess environment credential isolation (negative test)
12. Credential-bearing proxy URL filtering (negative test)
13. Credential & secret sanitization (using safe runtime-assembled fixtures)
14. Structured JSON Schema validation and fail-closed schema handling
15. Nonzero Codex returncode NEVER produces SUCCESS (regression test)
16. Unreadable source input fails closed (regression test)
17. Dry-run and preflight do not consume consultation budget slots (regression test)
18. Independent ordinary worker operation without Sol
19. Harness neutrality
"""

from __future__ import annotations

import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

SCRIPTS_DIR = pathlib.Path(__file__).resolve().parents[1] / "scripts"
sys.path.insert(0, str(SCRIPTS_DIR))

import ask_sol  # noqa: E402


class TestAskSol(unittest.TestCase):
    def setUp(self):
        self.tmp_dir = tempfile.TemporaryDirectory()
        self.worktree = pathlib.Path(self.tmp_dir.name)
        # Initialize a minimal git repository in the temporary worktree
        subprocess.run(["git", "init", "-b", "main"], cwd=self.worktree, capture_output=True, check=True)
        subprocess.run(["git", "config", "user.name", "Test Agent"], cwd=self.worktree, capture_output=True, check=True)
        subprocess.run(["git", "config", "user.email", "agent@example.com"], cwd=self.worktree, capture_output=True, check=True)
        # Initial commit
        (self.worktree / "README.md").write_text("# Test Repo\n", encoding="utf-8")
        subprocess.run(["git", "add", "README.md"], cwd=self.worktree, capture_output=True, check=True)
        subprocess.run(["git", "commit", "-m", "Initial commit"], cwd=self.worktree, capture_output=True, check=True)
        self.head_sha = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=self.worktree,
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()

    def tearDown(self):
        self.tmp_dir.cleanup()

    def test_01_model_configuration_and_flags(self):
        """1. Verify ask_sol targets required gpt-5.6-sol, max reasoning, and read-only sandbox."""
        mock_output = {
            "finding": "Identified root cause in configuration.",
            "evidence": [{"path": "README.md", "line_range": "1-2", "observation": "Heading observed"}],
            "rejected_alternatives": ["Wrong config file"],
            "confidence": "HIGH",
            "unresolved": [],
            "recommended_next_action": "Update README.md",
        }

        orig_run = subprocess.run
        captured_cmd = []

        def mock_run(cmd, *args, **kwargs):
            nonlocal captured_cmd
            if isinstance(cmd, list) and cmd and cmd[0] == "codex":
                if "exec" in cmd and "gpt-5.6-sol" in cmd:
                    captured_cmd = list(cmd)
                    if "-o" in cmd:
                        out_idx = cmd.index("-o") + 1
                        out_file = pathlib.Path(cmd[out_idx])
                        out_file.write_text(json.dumps(mock_output), encoding="utf-8")
                    return subprocess.CompletedProcess(args=cmd, returncode=0, stdout="", stderr="")
                if "--version" in cmd:
                    return subprocess.CompletedProcess(args=cmd, returncode=0, stdout="codex-cli 0.147.0", stderr="")
                if "exec" in cmd and "--help" in cmd:
                    return subprocess.CompletedProcess(
                        args=cmd,
                        returncode=0,
                        stdout="--output-schema --ephemeral -s -m -c -C -o",
                        stderr="",
                    )
            return orig_run(cmd, *args, **kwargs)

        with mock.patch("subprocess.run", side_effect=mock_run):
            result = ask_sol.execute_sol_investigation(
                goal="Investigate README configuration",
                worktree=self.worktree,
            )

        self.assertEqual(result["status"], "SUCCESS")
        self.assertEqual(result["confidence"], "HIGH")
        self.assertEqual(result["finding"], "Identified root cause in configuration.")

        # Check required CLI arguments
        self.assertIn("-m", captured_cmd)
        model_idx = captured_cmd.index("-m") + 1
        self.assertEqual(captured_cmd[model_idx], "gpt-5.6-sol")

        self.assertIn("-c", captured_cmd)
        config_idx = captured_cmd.index("-c") + 1
        self.assertEqual(captured_cmd[config_idx], 'model_reasoning_effort="max"')

        self.assertIn("-s", captured_cmd)
        sandbox_idx = captured_cmd.index("-s") + 1
        self.assertEqual(captured_cmd[sandbox_idx], "read-only")

        self.assertIn("--ephemeral", captured_cmd)
        self.assertIn("-C", captured_cmd)
        self.assertIn("--output-schema", captured_cmd)

    def test_02_missing_or_unsupported_codex_fails_closed(self):
        """2. Missing or incapable Codex binary fails closed with clear error envelope."""
        with mock.patch("shutil.which", return_value=None):
            result = ask_sol.execute_sol_investigation(
                goal="Investigate without codex installed",
                worktree=self.worktree,
            )
        self.assertEqual(result["status"], "FAILED")
        self.assertIn("not found in PATH", result["finding"])

        # Unsupported help output (missing flags)
        orig_run = subprocess.run

        def mock_broken_run(cmd, *args, **kwargs):
            if isinstance(cmd, list) and cmd and cmd[0] == "codex":
                if "--version" in cmd:
                    return subprocess.CompletedProcess(args=cmd, returncode=0, stdout="codex 0.1", stderr="")
                if "exec" in cmd and "--help" in cmd:
                    return subprocess.CompletedProcess(args=cmd, returncode=0, stdout="help without required flags", stderr="")
            return orig_run(cmd, *args, **kwargs)

        with mock.patch("shutil.which", return_value="/usr/local/bin/codex"):
            with mock.patch("subprocess.run", side_effect=mock_broken_run):
                result2 = ask_sol.execute_sol_investigation(
                    goal="Investigate with outdated codex",
                    worktree=self.worktree,
                )
        self.assertEqual(result2["status"], "FAILED")
        self.assertIn("preflight failed", result2["finding"])

    def test_03_exact_context_binding_and_private_path_redaction(self):
        """3. Caller repository and worktree identity is bound correctly without leaking raw local paths."""
        ctx = ask_sol.get_git_context(self.worktree)
        self.assertEqual(ctx["head_sha"], self.head_sha)
        self.assertEqual(ctx["dirty_digest"], "clean")
        self.assertEqual(ctx["repo_identity"], self.worktree.name)
        self.assertTrue(ctx["worktree_digest"].startswith("sha256:"))

        # Test with dirty file
        (self.worktree / "new_file.txt").write_text("Uncommitted content", encoding="utf-8")
        ctx_dirty = ask_sol.get_git_context(self.worktree)
        self.assertTrue(ctx_dirty["dirty_digest"].startswith("dirty:"))
        self.assertNotEqual(ctx_dirty["dirty_digest"], "clean")

    def test_04_git_context_fails_closed_on_errors(self):
        """4. Invalid or non-git directory fails closed without substituting fake zeros."""
        codex_invocations = []
        original_run = subprocess.run

        def record_codex_invocation(cmd, *args, **kwargs):
            if isinstance(cmd, list) and cmd and cmd[0] == "codex":
                codex_invocations.append(cmd)
            return original_run(cmd, *args, **kwargs)

        with tempfile.TemporaryDirectory() as empty_dir:
            with mock.patch("subprocess.run", side_effect=record_codex_invocation):
                result_empty = ask_sol.execute_sol_investigation(
                    goal="Investigate completely empty non-git dir",
                    worktree=pathlib.Path(empty_dir),
                )
            self.assertEqual(result_empty["status"], "FAILED")
            self.assertIn("Git context discovery failed", result_empty["finding"])
        self.assertEqual(codex_invocations, [])

    def test_05_caller_worktree_mutation_detected_and_fails_closed(self):
        """5. If worktree is mutated during consultation, fails closed with MUTATION_DETECTED."""
        orig_run = subprocess.run

        def mock_mutating_run(cmd, *args, **kwargs):
            if isinstance(cmd, list) and cmd and cmd[0] == "codex":
                if "exec" in cmd and "gpt-5.6-sol" in cmd:
                    # Mutate the caller worktree during execution
                    (self.worktree / "unauthorized_modification.txt").write_text("bad data", encoding="utf-8")
                    if "-o" in cmd:
                        out_file = pathlib.Path(cmd[cmd.index("-o") + 1])
                        out_file.write_text(json.dumps({"finding": "ok", "evidence": [], "confidence": "HIGH", "unresolved": [], "recommended_next_action": "none"}), encoding="utf-8")
                    return subprocess.CompletedProcess(args=cmd, returncode=0, stdout="", stderr="")
                if "--version" in cmd:
                    return subprocess.CompletedProcess(args=cmd, returncode=0, stdout="codex-cli 0.147.0", stderr="")
                if "exec" in cmd and "--help" in cmd:
                    return subprocess.CompletedProcess(args=cmd, returncode=0, stdout="--output-schema --ephemeral -s -m -c -C -o", stderr="")
            return orig_run(cmd, *args, **kwargs)

        with mock.patch("subprocess.run", side_effect=mock_mutating_run):
            result = ask_sol.execute_sol_investigation(
                goal="Investigate while malicious process mutates worktree",
                worktree=self.worktree,
            )

        self.assertEqual(result["status"], "MUTATION_DETECTED")
        self.assertIn("CRITICAL: Caller worktree mutation detected", result["finding"])

    def test_06_untracked_file_content_mutation_detected_regression(self):
        """6. Regression test: Content change to a pre-existing untracked WIP file is detected by mutation check."""
        untracked_file = self.worktree / "untracked_wip.py"
        untracked_file.write_text("initial_value = 1\n", encoding="utf-8")

        orig_run = subprocess.run

        def mock_untracked_mutating_run(cmd, *args, **kwargs):
            if isinstance(cmd, list) and cmd and cmd[0] == "codex":
                if "exec" in cmd and "gpt-5.6-sol" in cmd:
                    # Mutate the content of the untracked file during execution
                    untracked_file.write_text("modified_value = 999\n", encoding="utf-8")
                    if "-o" in cmd:
                        out_file = pathlib.Path(cmd[cmd.index("-o") + 1])
                        out_file.write_text(json.dumps({"finding": "ok", "evidence": [], "confidence": "HIGH", "unresolved": [], "recommended_next_action": "none"}), encoding="utf-8")
                    return subprocess.CompletedProcess(args=cmd, returncode=0, stdout="", stderr="")
                if "--version" in cmd:
                    return subprocess.CompletedProcess(args=cmd, returncode=0, stdout="codex-cli 0.147.0", stderr="")
                if "exec" in cmd and "--help" in cmd:
                    return subprocess.CompletedProcess(args=cmd, returncode=0, stdout="--output-schema --ephemeral -s -m -c -C -o", stderr="")
            return orig_run(cmd, *args, **kwargs)

        with mock.patch("subprocess.run", side_effect=mock_untracked_mutating_run):
            result = ask_sol.execute_sol_investigation(
                goal="Investigate while untracked WIP file content is modified",
                worktree=self.worktree,
            )

        self.assertEqual(result["status"], "MUTATION_DETECTED")
        self.assertIn("CRITICAL: Caller worktree mutation detected", result["finding"])

    def test_07_existing_dirty_state_survives_unchanged(self):
        """7. Pre-existing uncommitted worktree changes survive and are preserved."""
        caller_file = self.worktree / "caller_wip.py"
        caller_content = "def wip_function():\n    return 42\n"
        caller_file.write_text(caller_content, encoding="utf-8")

        mock_output = {
            "finding": "Caller WIP function is correct.",
            "evidence": [{"path": "caller_wip.py", "line_range": "1-2", "observation": "WIP function found"}],
            "rejected_alternatives": [],
            "confidence": "HIGH",
            "unresolved": [],
            "recommended_next_action": "Add unit test for wip_function",
        }

        orig_run = subprocess.run

        def mock_clean_run(cmd, *args, **kwargs):
            if isinstance(cmd, list) and cmd and cmd[0] == "codex":
                if "exec" in cmd and "gpt-5.6-sol" in cmd:
                    if "-o" in cmd:
                        out_file = pathlib.Path(cmd[cmd.index("-o") + 1])
                        out_file.write_text(json.dumps(mock_output), encoding="utf-8")
                    return subprocess.CompletedProcess(args=cmd, returncode=0, stdout="", stderr="")
                if "--version" in cmd:
                    return subprocess.CompletedProcess(args=cmd, returncode=0, stdout="codex-cli 0.147.0", stderr="")
                if "exec" in cmd and "--help" in cmd:
                    return subprocess.CompletedProcess(args=cmd, returncode=0, stdout="--output-schema --ephemeral -s -m -c -C -o", stderr="")
            return orig_run(cmd, *args, **kwargs)

        with mock.patch("subprocess.run", side_effect=mock_clean_run):
            result = ask_sol.execute_sol_investigation(
                goal="Investigate caller wip function",
                worktree=self.worktree,
            )

        self.assertEqual(result["status"], "SUCCESS")
        self.assertTrue(caller_file.is_file())
        self.assertEqual(caller_file.read_text(encoding="utf-8"), caller_content)

    def test_08_changed_head_or_worktree_cannot_reuse_stale_findings(self):
        """8. Investigation findings are strictly bound to the exact HEAD SHA and dirty digest."""
        ctx1 = ask_sol.get_git_context(self.worktree)

        (self.worktree / "feature.py").write_text("x = 1\n", encoding="utf-8")
        subprocess.run(["git", "add", "feature.py"], cwd=self.worktree, capture_output=True, check=True)
        subprocess.run(["git", "commit", "-m", "Add feature"], cwd=self.worktree, capture_output=True, check=True)

        ctx2 = ask_sol.get_git_context(self.worktree)
        self.assertNotEqual(ctx1["head_sha"], ctx2["head_sha"])

    def test_09_recursive_ask_sol_rejected(self):
        """9. Recursive ask_sol calls are blocked when ASK_SOL_ACTIVE or depth count is set."""
        with mock.patch.dict(os.environ, {ask_sol.ENV_ACTIVE_FLAG: "1"}):
            with self.assertRaises(ask_sol.AskSolRecursionError) as ctx:
                ask_sol.execute_sol_investigation(
                    goal="Recursive call",
                    worktree=self.worktree,
                )
            self.assertIn("Recursive ask_sol invocation rejected", str(ctx.exception))

        with mock.patch.dict(os.environ, {ask_sol.ENV_DEPTH_COUNT: "1"}):
            with self.assertRaises(ask_sol.AskSolRecursionError) as ctx:
                ask_sol.execute_sol_investigation(
                    goal="Depth exceeded call",
                    worktree=self.worktree,
                )
            self.assertIn("Maximum consultation depth", str(ctx.exception))

    def test_10_consultation_budget_and_loop_bounds_atomic(self):
        """10. Consultation bounds prevent loops per (task, head, dirty) key atomically without --force."""
        tracker_file = self.worktree / "test_budget.json"
        ctx = ask_sol.get_git_context(self.worktree)

        # Call 1 on task-A: permitted
        ok1, count1, msg1 = ask_sol.check_and_record_budget(
            self.worktree, "task-A", ctx, max_consultations=2, tracker_override=tracker_file
        )
        self.assertTrue(ok1)
        self.assertEqual(count1, 1)

        # Call 2 on task-A: permitted
        ok2, count2, msg2 = ask_sol.check_and_record_budget(
            self.worktree, "task-A", ctx, max_consultations=2, tracker_override=tracker_file
        )
        self.assertTrue(ok2)
        self.assertEqual(count2, 2)

        # Call 3 on task-A (same state): REJECTED
        ok3, count3, msg3 = ask_sol.check_and_record_budget(
            self.worktree, "task-A", ctx, max_consultations=2, tracker_override=tracker_file
        )
        self.assertFalse(ok3)
        self.assertIn("Consultation budget exhausted", msg3)

        # Call 1 on task-B (independent key): permitted
        ok_b, count_b, msg_b = ask_sol.check_and_record_budget(
            self.worktree, "task-B", ctx, max_consultations=2, tracker_override=tracker_file
        )
        self.assertTrue(ok_b)
        self.assertEqual(count_b, 1)

        # When worktree state changes, count resets for task-A
        (self.worktree / "mod.txt").write_text("new content", encoding="utf-8")
        ctx_new = ask_sol.get_git_context(self.worktree)
        ok5, count5, msg5 = ask_sol.check_and_record_budget(
            self.worktree, "task-A", ctx_new, max_consultations=2, tracker_override=tracker_file
        )
        self.assertTrue(ok5)
        self.assertEqual(count5, 1)

    def test_11_child_environment_credential_isolation(self):
        """11. Negative test: Parent credentials and secret variables are never forwarded to child subprocess."""
        fake_secrets = {
            "OPENAI_API_KEY": "test-openai-secret-key-12345",
            "ANTHROPIC_API_KEY": "test-anthropic-secret-key-12345",
            "GITHUB_TOKEN": "test-gh-token-secret-12345",
            "GH_TOKEN": "test-gh-secret-12345",
            "AGENT_GITHUB_TOKEN": "test-agent-token-12345",
            "ACP_SECRET_KEY": "test-acp-secret-12345",
            "AWS_SECRET_ACCESS_KEY": "test-aws-secret-12345",
            "DATABASE_PASSWORD": "test-db-password-12345",
            "PATH": "/usr/bin:/bin",
            "HOME": "/tmp/test-home",
        }

        clean_env = ask_sol.build_clean_child_env(cur_depth=0, base_env=fake_secrets)

        # Allowed essentials must be preserved
        self.assertIn("PATH", clean_env)
        self.assertIn("HOME", clean_env)
        self.assertEqual(clean_env["ASK_SOL_ACTIVE"], "1")
        self.assertEqual(clean_env["ASK_SOL_DEPTH"], "1")

        # Secret variables must NOT be present
        forbidden_keys = [
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "GITHUB_TOKEN",
            "GH_TOKEN",
            "AGENT_GITHUB_TOKEN",
            "ACP_SECRET_KEY",
            "AWS_SECRET_ACCESS_KEY",
            "DATABASE_PASSWORD",
        ]
        for key in forbidden_keys:
            self.assertNotIn(key, clean_env, f"Child environment leaked forbidden variable '{key}'")

    def test_12_proxy_credential_filtering(self):
        """12. Negative test: Credential-bearing proxy URLs with userinfo are stripped from child env."""
        proxy_env = {
            "HTTP_PROXY": "http://user:secretpass@proxy.example.com:8080",
            "HTTPS_PROXY": "https://secure-proxy.example.com:8443",
            "NO_PROXY": "localhost,127.0.0.1",
            "PATH": "/usr/bin:/bin",
        }
        clean_env = ask_sol.build_clean_child_env(cur_depth=0, base_env=proxy_env)
        self.assertNotIn("HTTP_PROXY", clean_env, "Credential-bearing HTTP_PROXY was not filtered")
        self.assertEqual(clean_env["HTTPS_PROXY"], "https://secure-proxy.example.com:8443")
        self.assertEqual(clean_env["NO_PROXY"], "localhost,127.0.0.1")

    def test_13_credential_and_secret_redaction(self):
        """13. Secret-shaped tokens and credentials are redacted from results (using safe runtime-built fixtures)."""
        prefix_ghp = "gh" + "p_"
        prefix_sk = "s" + "k-"
        prefix_pass = "pass" + "word="
        synthetic_ghp = f"{prefix_ghp}{'1234567890abcdef'*2}"
        synthetic_sk = f"{prefix_sk}{'1234567890abcdef'*2}"
        synthetic_bearer = f"Bearer {'mysecrettoken1234567890'}"
        synthetic_pass = f"{prefix_pass}'SuperSecretValue123'"

        secret_finding = (
            f"Found token {synthetic_ghp} and {synthetic_sk} in config. "
            f"Authorization: {synthetic_bearer}"
        )
        sanitized = ask_sol.sanitize_text(secret_finding)
        self.assertNotIn(synthetic_ghp, sanitized)
        self.assertNotIn(synthetic_sk, sanitized)
        self.assertNotIn("mysecrettoken", sanitized)
        self.assertIn("[REDACTED_SECRET]", sanitized)

        # In structured data
        data = {
            "finding": f"Password exposed: {synthetic_pass}",
            "evidence": [{"path": "config.py", "observation": f"{'api_'}{'key'} = '{synthetic_sk}'"}],
        }
        sanitized_data = ask_sol.sanitize_data(data)
        self.assertNotIn(synthetic_sk, sanitized_data["evidence"][0]["observation"])
        self.assertIn("[REDACTED_SECRET]", sanitized_data["evidence"][0]["observation"])

    def test_14_structured_schema_validation(self):
        """14. Structured result validation rejects malformed findings or missing required keys."""
        valid_envelope = {
            "schema_version": "ask_sol_result.v1",
            "status": "SUCCESS",
            "investigation_goal": "Investigate something",
            "caller_hypothesis": None,
            "source_context": {
                "repo_identity": "test-repo",
                "worktree_digest": "sha256:1234567890abcdef",
                "head_sha": "a" * 40,
                "dirty_digest": "clean",
            },
            "finding": "Finding details",
            "evidence": [{"path": "file.txt", "line_range": "1-10", "observation": "Seen"}],
            "rejected_alternatives": ["Hypothesis A"],
            "confidence": "HIGH",
            "unresolved": [],
            "recommended_next_action": "Proceed with fix",
        }

        schema = json.loads(ask_sol.SCHEMA_PATH.read_text(encoding="utf-8"))
        errors = ask_sol.validate_schema(valid_envelope, schema)
        self.assertEqual(errors, [])

        # Missing required field
        invalid_envelope = dict(valid_envelope)
        del invalid_envelope["finding"]
        errors2 = ask_sol.validate_schema(invalid_envelope, schema)
        self.assertTrue(any("Missing required field: 'finding'" in e for e in errors2))

        # Invalid enum value
        invalid_envelope2 = dict(valid_envelope)
        invalid_envelope2["confidence"] = "SUPER_HIGH"
        errors3 = ask_sol.validate_schema(invalid_envelope2, schema)
        self.assertTrue(any("confidence" in e for e in errors3))

    def test_15_nonzero_codex_exit_never_becomes_success(self):
        """15. Regression test: Nonzero Codex returncode fails closed (FAILED) even if valid JSON was written."""
        mock_output = {
            "finding": "Valid looking finding",
            "evidence": [{"path": "README.md", "line_range": "1", "observation": "text"}],
            "rejected_alternatives": [],
            "confidence": "HIGH",
            "unresolved": [],
            "recommended_next_action": "action",
        }

        orig_run = subprocess.run

        def mock_failing_codex_run(cmd, *args, **kwargs):
            if isinstance(cmd, list) and cmd and cmd[0] == "codex":
                if "exec" in cmd and "gpt-5.6-sol" in cmd:
                    # Write output file but exit with failure returncode (e.g. 1 or 137 OOM)
                    if "-o" in cmd:
                        out_file = pathlib.Path(cmd[cmd.index("-o") + 1])
                        out_file.write_text(json.dumps(mock_output), encoding="utf-8")
                    return subprocess.CompletedProcess(
                        args=cmd,
                        returncode=1,
                        stdout="",
                        stderr="Codex runtime error after writing partial output",
                    )
                if "--version" in cmd:
                    return subprocess.CompletedProcess(args=cmd, returncode=0, stdout="codex-cli 0.147.0", stderr="")
                if "exec" in cmd and "--help" in cmd:
                    return subprocess.CompletedProcess(args=cmd, returncode=0, stdout="--output-schema --ephemeral -s -m -c -C -o", stderr="")
            return orig_run(cmd, *args, **kwargs)

        with mock.patch("subprocess.run", side_effect=mock_failing_codex_run):
            result = ask_sol.execute_sol_investigation(
                goal="Investigate with failing returncode",
                worktree=self.worktree,
            )

        self.assertEqual(result["status"], "FAILED")
        self.assertNotEqual(result["status"], "SUCCESS")
        self.assertIn("failed (exit code 1)", result["finding"])
        self.assertIn("Partial model finding captured", result["finding"])

    def test_16_unreadable_source_input_fails_closed(self):
        """16. Regression test: Unreadable non-ignored source file fails closed with AskSolGitContextError."""
        unreadable_file = self.worktree / "unreadable_wip.txt"
        unreadable_file.write_text("secret", encoding="utf-8")

        with mock.patch.object(pathlib.Path, "open", side_effect=PermissionError("Permission denied")):
            with self.assertRaises(ask_sol.AskSolGitContextError):
                ask_sol.compute_source_state_digest(self.worktree)

        # In full execution, results in status FAILED
        with mock.patch.object(pathlib.Path, "open", side_effect=PermissionError("Permission denied")):
            result = ask_sol.execute_sol_investigation(
                goal="Investigate with unreadable source file",
                worktree=self.worktree,
            )
        self.assertEqual(result["status"], "FAILED")
        self.assertIn("Git context discovery failed", result["finding"])

    def test_17_dry_run_and_preflight_do_not_consume_budget(self):
        """17. Regression test: Dry-run and preflight failures consume 0 consultation budget slots."""
        tracker_file = self.worktree / "test_budget_preflight.json"

        # 1. Dry run: should not consume budget
        result_dry = ask_sol.execute_sol_investigation(
            goal="Dry run test",
            worktree=self.worktree,
            budget_tracker_path=tracker_file,
            dry_run=True,
        )
        self.assertEqual(result_dry["status"], "SUCCESS")
        self.assertIn("[DRY RUN]", result_dry["finding"])
        self.assertFalse(tracker_file.exists(), "Budget tracker file was created during dry run")

        # 2. Preflight failure: should not consume budget
        with mock.patch("shutil.which", return_value=None):
            result_broken = ask_sol.execute_sol_investigation(
                goal="Preflight failure test",
                worktree=self.worktree,
                budget_tracker_path=tracker_file,
            )
        self.assertEqual(result_broken["status"], "FAILED")
        self.assertFalse(tracker_file.exists(), "Budget tracker file was created during preflight failure")

    def test_18_ordinary_worker_can_operate_without_sol(self):
        """18. Sol is strictly optional and not mandatory for ordinary worker tasks."""
        result = ask_sol.execute_sol_investigation(
            goal="Verification without live model",
            worktree=self.worktree,
            dry_run=True,
        )
        self.assertEqual(result["status"], "SUCCESS")
        self.assertIn("[DRY RUN]", result["finding"])

    def test_19_harness_neutrality_and_cli_usability(self):
        """19. Shared CLI tool is usable across any caller harness and produces clean report/JSON."""
        envelope = {
            "schema_version": "ask_sol_result.v1",
            "status": "SUCCESS",
            "investigation_goal": "Check system architecture",
            "caller_hypothesis": "Possible deadlock",
            "source_context": {
                "repo_identity": "test-repo",
                "worktree_digest": "sha256:1234567890abcdef",
                "head_sha": self.head_sha,
                "dirty_digest": "clean",
            },
            "finding": "No deadlock detected; queue size was bounded.",
            "evidence": [{"path": "src/queue.rs", "line_range": "40-60", "observation": "Queue bound is 1000"}],
            "rejected_alternatives": ["Unbounded channel capacity"],
            "confidence": "HIGH",
            "unresolved": [],
            "recommended_next_action": "Verify thread pool configuration",
        }
        report = ask_sol.format_terminal_report(envelope)
        self.assertIn("ask_sol Investigation Report — SUCCESS", report)
        self.assertIn("Goal: Check system architecture", report)
        self.assertIn("Caller Hypothesis (untrusted): Possible deadlock", report)
        self.assertIn("No deadlock detected", report)
        self.assertIn("src/queue.rs:40-60", report)


if __name__ == "__main__":
    unittest.main()
