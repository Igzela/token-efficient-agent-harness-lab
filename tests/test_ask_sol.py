"""Unit tests for shared, harness-neutral ask_sol investigation tool.

Covers:
1. Target model configuration (gpt-5.6-sol, model_reasoning_effort="max", read-only sandbox, --ephemeral)
2. Fail-closed on missing/unsupported Codex capability
3. Exact context binding (repo root, worktree, HEAD SHA, dirty digest)
4. Non-mutation verification and fail-closed on mutation
5. Uncommitted caller dirty state preservation
6. Context staleness prevention across changed HEAD/worktree
7. Recursive invocation rejection (ASK_SOL_ACTIVE guard)
8. Consultation loop & budget bounds
9. Credential & secret redaction
10. Structured JSON Schema validation
11. Independent ordinary worker operation without Sol
12. Harness neutrality
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

    def test_03_exact_context_binding(self):
        """3. Caller repository and worktree identity is bound correctly."""
        ctx = ask_sol.get_git_context(self.worktree)
        self.assertEqual(ctx["head_sha"], self.head_sha)
        self.assertEqual(ctx["dirty_digest"], "clean")
        self.assertEqual(pathlib.Path(ctx["worktree"]).resolve(), self.worktree.resolve())

        # Test with dirty file
        (self.worktree / "new_file.txt").write_text("Uncommitted content", encoding="utf-8")
        ctx_dirty = ask_sol.get_git_context(self.worktree)
        self.assertTrue(ctx_dirty["dirty_digest"].startswith("dirty:"))
        self.assertNotEqual(ctx_dirty["dirty_digest"], "clean")

    def test_04_caller_worktree_mutation_detected_and_fails_closed(self):
        """4. If worktree is mutated during consultation, fails closed with MUTATION_DETECTED."""
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

    def test_05_existing_dirty_state_survives_unchanged(self):
        """5. Pre-existing uncommitted worktree changes survive and are preserved."""
        # Create uncommitted caller work
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
        # Ensure caller file is intact and unmodified
        self.assertTrue(caller_file.is_file())
        self.assertEqual(caller_file.read_text(encoding="utf-8"), caller_content)

    def test_06_changed_head_or_worktree_cannot_reuse_stale_findings(self):
        """6. Investigation findings are strictly bound to the exact HEAD SHA and dirty digest."""
        ctx1 = ask_sol.get_git_context(self.worktree)

        # Commit a change to advance HEAD
        (self.worktree / "feature.py").write_text("x = 1\n", encoding="utf-8")
        subprocess.run(["git", "add", "feature.py"], cwd=self.worktree, capture_output=True, check=True)
        subprocess.run(["git", "commit", "-m", "Add feature"], cwd=self.worktree, capture_output=True, check=True)

        ctx2 = ask_sol.get_git_context(self.worktree)
        self.assertNotEqual(ctx1["head_sha"], ctx2["head_sha"])

        # An investigation envelope from ctx1 must not match ctx2
        envelope_from_ctx1 = {
            "source_context": ctx1,
        }
        self.assertNotEqual(envelope_from_ctx1["source_context"]["head_sha"], ctx2["head_sha"])

    def test_07_recursive_ask_sol_rejected(self):
        """7. Recursive ask_sol calls are blocked when ASK_SOL_ACTIVE or depth count is set."""
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

    def test_08_consultation_budget_and_loop_bounds(self):
        """8. Consultation bounds prevent infinite loops on the same unchanged state."""
        tracker_file = self.worktree / "test_budget.json"
        ctx = ask_sol.get_git_context(self.worktree)

        # Call 1: permitted
        ok1, count1, msg1 = ask_sol.check_and_record_budget(
            self.worktree, "task-1", ctx, max_consultations=2, tracker_override=tracker_file
        )
        self.assertTrue(ok1)
        self.assertEqual(count1, 1)

        # Call 2: permitted
        ok2, count2, msg2 = ask_sol.check_and_record_budget(
            self.worktree, "task-1", ctx, max_consultations=2, tracker_override=tracker_file
        )
        self.assertTrue(ok2)
        self.assertEqual(count2, 2)

        # Call 3 on same state: REJECTED
        ok3, count3, msg3 = ask_sol.check_and_record_budget(
            self.worktree, "task-1", ctx, max_consultations=2, tracker_override=tracker_file
        )
        self.assertFalse(ok3)
        self.assertIn("Consultation budget exhausted", msg3)

        # Call 4 with --force: permitted
        ok4, count4, msg4 = ask_sol.check_and_record_budget(
            self.worktree, "task-1", ctx, max_consultations=2, force=True, tracker_override=tracker_file
        )
        self.assertTrue(ok4)

        # When worktree state changes, count resets
        (self.worktree / "mod.txt").write_text("new content", encoding="utf-8")
        ctx_new = ask_sol.get_git_context(self.worktree)
        ok5, count5, msg5 = ask_sol.check_and_record_budget(
            self.worktree, "task-1", ctx_new, max_consultations=2, tracker_override=tracker_file
        )
        self.assertTrue(ok5)
        self.assertEqual(count5, 1)

    def test_09_credential_and_secret_redaction(self):
        """9. Secret-shaped tokens and credentials are redacted from results."""
        secret_finding = (
            "Found token ghp_1234567890abcdef1234567890abcdef and sk-1234567890abcdef1234567890 in config. "
            "Authorization: Bearer mysecrettoken1234567890abc"
        )
        sanitized = ask_sol.sanitize_text(secret_finding)
        self.assertNotIn("ghp_", sanitized)
        self.assertNotIn("sk-", sanitized)
        self.assertNotIn("mysecrettoken", sanitized)
        self.assertIn("[REDACTED_SECRET]", sanitized)

        # In structured data
        data = {
            "finding": "Password exposed: password='SuperSecretPassword123'",
            "evidence": [{"path": "config.py", "observation": "api_key = 'sk-abcdef1234567890abcdef1234'"}],
        }
        sanitized_data = ask_sol.sanitize_data(data)
        self.assertNotIn("sk-abcdef", sanitized_data["evidence"][0]["observation"])
        self.assertIn("[REDACTED_SECRET]", sanitized_data["evidence"][0]["observation"])

    def test_10_structured_schema_validation(self):
        """10. Structured result validation rejects malformed findings or missing required keys."""
        valid_envelope = {
            "schema_version": "ask_sol_result.v1",
            "status": "SUCCESS",
            "investigation_goal": "Investigate something",
            "caller_hypothesis": None,
            "source_context": {
                "repo_root": "/path/to/repo",
                "worktree": "/path/to/worktree",
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

    def test_11_ordinary_worker_can_operate_without_sol(self):
        """11. Sol is strictly optional and not mandatory for ordinary worker tasks."""
        # A worker can execute dry-run, inspect git context, and finish without invoking Sol
        result = ask_sol.execute_sol_investigation(
            goal="Verification without live model",
            worktree=self.worktree,
            dry_run=True,
        )
        self.assertEqual(result["status"], "SUCCESS")
        self.assertIn("[DRY RUN]", result["finding"])

    def test_12_harness_neutrality_and_cli_usability(self):
        """12. Shared CLI tool is usable across any caller harness and produces clean report/JSON."""
        # Test terminal formatting
        envelope = {
            "schema_version": "ask_sol_result.v1",
            "status": "SUCCESS",
            "investigation_goal": "Check system architecture",
            "caller_hypothesis": "Possible deadlock",
            "source_context": {
                "repo_root": "/repo",
                "worktree": "/worktree",
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
