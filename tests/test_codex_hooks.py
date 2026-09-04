"""Comprehensive tests for Codex Lifecycle Hooks (H0 through H3).

Verifies:
- Protocol: JSON wire serialization and deserialization across all hook events.
- H0 Probe: Real binary capability detection and mock matrix (VERIFIED, UNSUPPORTED, BLOCKED).
- H1 Session: Context bootstrap, compaction state snapshot & rehydration, ephemeral receipts, and ROI telemetry.
- H2 Guard: Worktree boundary enforcement, forbidden path rejection, and permission auto-approval.
- H3 Continuation: Stop hook workcard completion checks and bounded continuation retry loop.
- Dispatcher: End-to-end event routing and wire protocol compliance.
- Trust & Config: Strict TOML generation, SHA256 integrity digest, and definition hash invalidation.
- ROI Telemetry: Deterministic measurement of tokens saved, tools guarded, and premature stops intercepted.
"""

from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
CONTROL_DIR = ROOT / "scripts" / "agent-control"
if str(CONTROL_DIR) not in sys.path:
    sys.path.insert(0, str(CONTROL_DIR))

from codex_hooks import (
    CAPABILITY_NAMES,
    CapabilityStatus,
    CodexHookProbe,
    CodexHookProbeResult,
    ContinuationHandler,
    GuardHandler,
    HookConfigGenerator,
    HookDispatcher,
    HookEventName,
    HookInput,
    HookOutput,
    HookSpecificOutput,
    HookTelemetry,
    PermissionDecision,
    SessionHandler,
    compute_bundle_hash,
)


class TestCodexHooksProtocol(unittest.TestCase):
    """Tests for protocol data structures, wire serialization, and enums."""

    def test_hook_input_deserialization(self):
        raw = json.dumps({
            "hook_event_name": "PreToolUse",
            "session_id": "sess-001",
            "turn_id": "turn-002",
            "cwd": "/workspace",
            "model": "gpt-5",
            "tool_name": "write_to_file",
            "tool_input": {"target_file": "foo.py", "content": "print('hello')"},
        })
        hook_input = HookInput.from_json(raw)
        self.assertEqual(hook_input.hook_event_name, "PreToolUse")
        self.assertEqual(hook_input.session_id, "sess-001")
        self.assertEqual(hook_input.tool_name, "write_to_file")
        self.assertEqual(hook_input.tool_input["target_file"], "foo.py")

    def test_hook_input_empty_payload(self):
        hook_input = HookInput.from_json("", event_override="SessionStart")
        self.assertEqual(hook_input.hook_event_name, "SessionStart")
        self.assertEqual(hook_input.session_id, "")

    def test_hook_output_serialization(self):
        specific = HookSpecificOutput(
            permissionDecision="allow",
            additionalContext="context snippet",
            stopReason="complete",
        )
        out = HookOutput(hookSpecificOutput=specific, systemMessage="system notice")
        as_dict = out.to_dict()
        self.assertEqual(as_dict["systemMessage"], "system notice")
        self.assertEqual(as_dict["hookSpecificOutput"]["permissionDecision"], "allow")
        self.assertEqual(as_dict["hookSpecificOutput"]["additionalContext"], "context snippet")
        self.assertNotIn("permissionDecisionReason", as_dict["hookSpecificOutput"])

        as_json = out.to_json()
        parsed = json.loads(as_json)
        self.assertEqual(parsed["hookSpecificOutput"]["permissionDecision"], "allow")


class TestCodexHooksH0Probe(unittest.TestCase):
    """Tests for H0 Capability and Trust Probe."""

    def test_real_binary_probe(self):
        probe = CodexHookProbe()
        if not probe.binary_path.is_file():
            self.skipTest("Codex binary not installed on host")
        result = probe.run_probe()
        self.assertIn(result.overall_status, {"READY", "DEGRADED"})
        self.assertEqual(result.capabilities.get("hooks.basic"), CapabilityStatus.VERIFIED.value)
        self.assertEqual(result.capabilities.get("session_start"), CapabilityStatus.VERIFIED.value)
        self.assertEqual(result.capabilities.get("pre_tool"), CapabilityStatus.VERIFIED.value)
        self.assertEqual(result.capabilities.get("post_tool"), CapabilityStatus.VERIFIED.value)
        self.assertEqual(result.capabilities.get("stop"), CapabilityStatus.VERIFIED.value)
        self.assertTrue(result.is_ready())

    def test_missing_binary_probe_blocked(self):
        probe = CodexHookProbe(codex_binary="/nonexistent/bin/codex")
        result = probe.run_probe()
        self.assertEqual(result.overall_status, "BLOCKED")
        for cap in CAPABILITY_NAMES:
            self.assertEqual(result.capabilities[cap], CapabilityStatus.BLOCKED.value)
        self.assertFalse(result.is_ready())

    def test_mock_runner_unsupported(self):
        def mock_runner(args, **_kwargs):
            if "--version" in args and len(args) == 2:
                return 0, "codex-cli 0.100.0", ""
            if "features" in args:
                return 0, "hooks  under development  false\n", ""
            return 1, "", "unknown field `hooks`"

        probe = CodexHookProbe(codex_binary="/bin/echo", runner=mock_runner)
        result = probe.run_probe()
        self.assertEqual(result.capabilities.get("hooks.basic"), CapabilityStatus.UNSUPPORTED.value)
        self.assertEqual(result.overall_status, "UNSUPPORTED")
        self.assertFalse(result.is_ready())


class TestCodexHooksH1Session(unittest.TestCase):
    """Tests for H1 Context Bootstrap, Compaction Rehydration, and Ephemeral Receipts."""

    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp_dir.cleanup)
        self.state_dir = Path(self.temp_dir.name)

    def test_session_start_bootstrap(self):
        os.environ["STEWARD_WORKCARD_ID"] = "card-test-01"
        os.environ["STEWARD_WORKTREE"] = str(self.state_dir)
        os.environ["STEWARD_ALLOWED_PATHS"] = json.dumps(["scripts/agent-control", "docs/ARCHITECTURE.md"])
        os.environ["STEWARD_FORBIDDEN_PATHS"] = json.dumps(["docs/ROADMAP.md"])
        os.environ["STEWARD_CARD_OBJECTIVE"] = json.dumps(["Implement feature X", "Add tests"])

        try:
            handler = SessionHandler(self.state_dir)
            hook_input = HookInput(hook_event_name="SessionStart", session_id="sess-1")
            output = handler.handle_session_start(hook_input)

            self.assertIsNotNone(output.hookSpecificOutput)
            context = output.hookSpecificOutput.additionalContext
            self.assertIn("card-test-01", context)
            self.assertIn("scripts/agent-control", context)
            self.assertIn("docs/ROADMAP.md", context)
            self.assertIn("Implement feature X", context)

            # Check telemetry
            telemetry = handler.telemetry.metrics
            self.assertGreater(telemetry["bootstrap_bytes_saved"], 0)
        finally:
            for k in ("STEWARD_WORKCARD_ID", "STEWARD_WORKTREE", "STEWARD_ALLOWED_PATHS",
                      "STEWARD_FORBIDDEN_PATHS", "STEWARD_CARD_OBJECTIVE"):
                os.environ.pop(k, None)

    def test_compaction_rehydration_loop(self):
        os.environ["STEWARD_WORKCARD_ID"] = "card-test-compact"
        os.environ["STEWARD_WORKTREE"] = str(self.state_dir)
        os.environ["STEWARD_ALLOWED_PATHS"] = json.dumps(["scripts/"])

        try:
            handler = SessionHandler(self.state_dir)
            pre_input = HookInput(hook_event_name="PreCompact", session_id="sess-c", turn_id="turn-5")
            pre_out = handler.handle_pre_compact(pre_input)
            self.assertIn("PreCompact", pre_out.hookSpecificOutput.additionalContext)
            self.assertTrue(handler.compaction_path.is_file())

            post_input = HookInput(hook_event_name="PostCompact", session_id="sess-c")
            post_out = handler.handle_post_compact(post_input)
            rehydrate = post_out.hookSpecificOutput.additionalContext
            self.assertIn("card-test-compact", rehydrate)
            self.assertIn("scripts/", rehydrate)

            self.assertEqual(handler.telemetry.metrics["compaction_rehydrations"], 1)
        finally:
            for k in ("STEWARD_WORKCARD_ID", "STEWARD_WORKTREE", "STEWARD_ALLOWED_PATHS"):
                os.environ.pop(k, None)

    def test_post_tool_use_receipts_and_compression(self):
        handler = SessionHandler(self.state_dir)
        large_response = "A" * 5000
        hook_input = HookInput(
            hook_event_name="PostToolUse",
            tool_name="bash",
            tool_input={"command": "cat large_file.txt"},
            tool_response=large_response,
            turn_id="turn-tool-1",
        )
        output = handler.handle_post_tool_use(hook_input)
        self.assertIn("Receipt #0001 recorded", output.hookSpecificOutput.additionalContext)

        # Check receipts directory
        receipts = list(handler.receipts_dir.glob("receipt_*.json"))
        self.assertEqual(len(receipts), 1)
        receipt_data = json.loads(receipts[0].read_text(encoding="utf-8"))
        self.assertEqual(receipt_data["tool_name"], "bash")
        self.assertEqual(receipt_data["response_bytes"], 5000)

        # Check telemetry compression
        self.assertGreater(handler.telemetry.metrics["receipt_bytes_saved"], 4000)


class TestCodexHooksH2Guard(unittest.TestCase):
    """Tests for H2 Path Boundary Guard and Permission Auto-Approval."""

    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp_dir.cleanup)
        self.worktree = Path(self.temp_dir.name)
        (self.worktree / "src").mkdir(parents=True)
        (self.worktree / "docs").mkdir(parents=True)
        self.handler = GuardHandler(self.worktree / "hooks_state")

    def test_in_scope_write_allowed(self):
        os.environ["STEWARD_WORKTREE"] = str(self.worktree)
        os.environ["STEWARD_ALLOWED_PATHS"] = json.dumps(["src/"])
        os.environ["STEWARD_FORBIDDEN_PATHS"] = json.dumps(["docs/ROADMAP.md"])

        try:
            hook_input = HookInput(
                hook_event_name="PreToolUse",
                tool_name="write_to_file",
                tool_input={"target_file": str(self.worktree / "src" / "app.py")},
            )
            out = self.handler.handle_pre_tool_use(hook_input)
            self.assertEqual(out.hookSpecificOutput.permissionDecision, PermissionDecision.ALLOW.value)
        finally:
            for k in ("STEWARD_WORKTREE", "STEWARD_ALLOWED_PATHS", "STEWARD_FORBIDDEN_PATHS"):
                os.environ.pop(k, None)

    def test_out_of_scope_write_blocked(self):
        os.environ["STEWARD_WORKTREE"] = str(self.worktree)
        os.environ["STEWARD_ALLOWED_PATHS"] = json.dumps(["src/"])
        os.environ["STEWARD_FORBIDDEN_PATHS"] = json.dumps(["docs/ROADMAP.md"])

        try:
            hook_input = HookInput(
                hook_event_name="PreToolUse",
                tool_name="write_to_file",
                tool_input={"target_file": str(self.worktree / "docs" / "other.md")},
            )
            out = self.handler.handle_pre_tool_use(hook_input)
            self.assertEqual(out.hookSpecificOutput.permissionDecision, PermissionDecision.BLOCK.value)
            self.assertIn("Path outside allowed", out.hookSpecificOutput.permissionDecisionReason)
        finally:
            for k in ("STEWARD_WORKTREE", "STEWARD_ALLOWED_PATHS", "STEWARD_FORBIDDEN_PATHS"):
                os.environ.pop(k, None)

    def test_strictly_forbidden_path_blocked(self):
        os.environ["STEWARD_WORKTREE"] = str(self.worktree)
        os.environ["STEWARD_ALLOWED_PATHS"] = json.dumps(["docs/"])
        os.environ["STEWARD_FORBIDDEN_PATHS"] = json.dumps(["docs/ROADMAP.md"])

        try:
            hook_input = HookInput(
                hook_event_name="PreToolUse",
                tool_name="replace_file_content",
                tool_input={"TargetFile": str(self.worktree / "docs" / "ROADMAP.md")},
            )
            out = self.handler.handle_pre_tool_use(hook_input)
            self.assertEqual(out.hookSpecificOutput.permissionDecision, PermissionDecision.BLOCK.value)
            self.assertIn("strictly forbidden scope", out.hookSpecificOutput.permissionDecisionReason)
        finally:
            for k in ("STEWARD_WORKTREE", "STEWARD_ALLOWED_PATHS", "STEWARD_FORBIDDEN_PATHS"):
                os.environ.pop(k, None)

    def test_forbidden_command_patterns_blocked(self):
        os.environ["STEWARD_WORKTREE"] = str(self.worktree)

        try:
            for bad_cmd in ("git push origin main", "rm -rf /", "sqlite3 /var/lib/agent-steward/steward.sqlite3"):
                hook_input = HookInput(
                    hook_event_name="PreToolUse",
                    tool_name="bash",
                    tool_input={"command": bad_cmd},
                )
                out = self.handler.handle_pre_tool_use(hook_input)
                self.assertEqual(out.hookSpecificOutput.permissionDecision, PermissionDecision.BLOCK.value)
        finally:
            os.environ.pop("STEWARD_WORKTREE", None)

    def test_permission_request_auto_approval(self):
        hook_input = HookInput(
            hook_event_name="PermissionRequest",
            tool_name="bash",
            tool_input={"command": "pytest tests/test_codex_hooks.py"},
        )
        out = self.handler.handle_permission_request(hook_input)
        self.assertEqual(out.hookSpecificOutput.permissionDecision, PermissionDecision.ALLOW.value)


class TestCodexHooksH3Continuation(unittest.TestCase):
    """Tests for H3 Stop hook and autonomous continuation loop."""

    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp_dir.cleanup)
        self.worktree = Path(self.temp_dir.name)
        subprocess.run(["git", "init", str(self.worktree)], check=True, capture_output=True)
        self.state_dir = self.worktree / "hooks_state"
        self.handler = ContinuationHandler(self.state_dir, max_continuations=2)

    def test_stop_prevented_when_no_changes(self):
        os.environ["STEWARD_WORKTREE"] = str(self.worktree)
        os.environ["STEWARD_WORKCARD_ID"] = "card-incomplete"
        os.environ["STEWARD_WORKER_TYPE"] = "implement"

        try:
            hook_input = HookInput(hook_event_name="Stop")
            # Attempt 1: blocked
            exit_code, out, stderr = self.handler.handle_stop(hook_input)
            self.assertEqual(exit_code, 2)
            self.assertIn("incomplete", stderr)
            self.assertEqual(self.handler.telemetry.metrics["premature_stops_intercepted"], 1)

            # Attempt 2: blocked
            exit_code, out, stderr = self.handler.handle_stop(hook_input)
            self.assertEqual(exit_code, 2)
            self.assertEqual(self.handler.telemetry.metrics["premature_stops_intercepted"], 2)

            # Attempt 3: budget exhausted -> allowed
            exit_code, out, stderr = self.handler.handle_stop(hook_input)
            self.assertEqual(exit_code, 0)
            self.assertIn("continuation_budget_exhausted", out.hookSpecificOutput.stopReason)
        finally:
            for k in ("STEWARD_WORKTREE", "STEWARD_WORKCARD_ID", "STEWARD_WORKER_TYPE"):
                os.environ.pop(k, None)

    def test_stop_allowed_when_changes_present(self):
        os.environ["STEWARD_WORKTREE"] = str(self.worktree)
        os.environ["STEWARD_WORKCARD_ID"] = "card-complete"
        os.environ["STEWARD_WORKER_TYPE"] = "implement"

        try:
            (self.worktree / "foo.txt").write_text("modified", encoding="utf-8")
            hook_input = HookInput(hook_event_name="Stop")
            exit_code, out, stderr = self.handler.handle_stop(hook_input)
            self.assertEqual(exit_code, 0)
            self.assertIsNone(stderr)
            self.assertEqual(out.hookSpecificOutput.stopReason, "workcard_changes_present")
        finally:
            for k in ("STEWARD_WORKTREE", "STEWARD_WORKCARD_ID", "STEWARD_WORKER_TYPE"):
                os.environ.pop(k, None)


class TestCodexHooksDispatcher(unittest.TestCase):
    """Tests for HookDispatcher entrypoint routing and exit codes."""

    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp_dir.cleanup)
        self.state_dir = Path(self.temp_dir.name)
        self.dispatcher = HookDispatcher(self.state_dir)

    def test_dispatch_session_start(self):
        code, stdout, stderr = self.dispatcher.dispatch("SessionStart", json.dumps({"session_id": "s1"}))
        self.assertEqual(code, 0)
        self.assertIn("hookSpecificOutput", stdout)

    def test_dispatch_pre_tool_block(self):
        os.environ["STEWARD_WORKTREE"] = str(self.state_dir)
        os.environ["STEWARD_ALLOWED_PATHS"] = json.dumps(["src/"])
        try:
            payload = json.dumps({
                "tool_name": "write_to_file",
                "tool_input": {"target_file": str(self.state_dir / "forbidden.py")},
            })
            code, stdout, stderr = self.dispatcher.dispatch("PreToolUse", payload)
            self.assertEqual(code, 2)
            self.assertIn("Path outside allowed", stderr)
        finally:
            for k in ("STEWARD_WORKTREE", "STEWARD_ALLOWED_PATHS"):
                os.environ.pop(k, None)


class TestCodexHooksConfigAndTrust(unittest.TestCase):
    """Tests for HookConfigGenerator and bundle trust hash invalidation."""

    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp_dir.cleanup)
        self.root = Path(self.temp_dir.name)
        self.dispatcher_path = self.root / "dispatcher.py"
        self.dispatcher_path.write_text("#!/usr/bin/env python3\nprint('dispatcher')\n", encoding="utf-8")

    def test_config_generation_and_hash_invalidation(self):
        generator = HookConfigGenerator(
            dispatcher_path=self.dispatcher_path,
            worktree_path=self.root,
        )
        toml_v1 = generator.generate_toml()
        self.assertIn("[features]", toml_v1)
        self.assertIn("hooks = true", toml_v1)
        self.assertIn(f'[hooks.state."{self.dispatcher_path}"]', toml_v1)
        self.assertIn("trusted_hash =", toml_v1)

        hash_v1 = compute_bundle_hash(self.dispatcher_path)

        # Mutate dispatcher -> hash must change (definition hash invalidation)
        self.dispatcher_path.write_text("#!/usr/bin/env python3\nprint('tampered')\n", encoding="utf-8")
        hash_v2 = compute_bundle_hash(self.dispatcher_path)
        self.assertNotEqual(hash_v1, hash_v2)


class TestCodexHooksROIBenchmark(unittest.TestCase):
    """Deterministic comparison: without Hooks vs with Hooks."""

    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp_dir.cleanup)
        self.root = Path(self.temp_dir.name)

    def test_deterministic_roi_comparison(self):
        telemetry = HookTelemetry(self.root)

        # 1. Baseline context ingestion without hooks:
        # Full documents ingestion (START_HERE, AGENTS, ARCHITECTURE, AUTONOMY, ROADMAP) ~ 120,000 bytes
        baseline_bytes = 120_000

        # With H1 bounded context bootstrap:
        # Injected bootstrap is ~1,500 bytes
        injected_bytes = 1_500
        telemetry.record_bootstrap(injected_bytes, baseline_doc_bytes=baseline_bytes)

        # 2. Tool output without hooks:
        # Raw pytest / build logs ~ 15,000 bytes per tool call
        # With H1 receipts: compressed summary ~ 250 bytes
        telemetry.record_tool_receipt("pytest", 15_000, 250)

        # 3. Path boundaries without hooks:
        # Errant writes to forbidden paths would succeed or cause corrupt state
        # With H2 guard: intercepted and blocked
        telemetry.record_tool_block("write_to_file", "Path touches forbidden docs/ROADMAP.md")

        # 4. Premature stop without hooks:
        # Model stops prematurely without making changes
        # With H3 stop hook: intercepted and continued
        telemetry.record_stop_intercept(1, "no_workspace_changes")

        metrics = telemetry.metrics
        self.assertGreater(metrics["bootstrap_bytes_saved"], 100_000)
        self.assertGreater(metrics["receipt_bytes_saved"], 14_000)
        self.assertEqual(metrics["tools_blocked"], 1)
        self.assertEqual(metrics["premature_stops_intercepted"], 1)

        # Verify telemetry persistence and reload
        reloaded = HookTelemetry(self.root)
        self.assertEqual(reloaded.metrics["tools_blocked"], 1)
        self.assertEqual(reloaded.metrics["premature_stops_intercepted"], 1)


if __name__ == "__main__":
    unittest.main()
