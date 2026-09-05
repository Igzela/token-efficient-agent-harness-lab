"""Session context bootstrap, compaction checkpointing, and receipts (H1).

Implements:
- SessionStart: Injects bounded WorkCard context, allowed paths, and execution invariants
  with strict official wire hookEventName. When the runtime reports
  ``source == "compact"``, rehydrates critical constraints and progress from the
  PreCompact checkpoint instead of the startup bootstrap.
- PreCompact: Persists the active WorkCard checkpoint (WorkCard id, session,
  git status) to local state. Emits no hook body beyond the official
  top-level fields: the official pre-compact schema forbids hookSpecificOutput.
- PostCompact: Pass-through acknowledgement. Context re-injection after
  compaction is owned by SessionStart(source="compact"), never by a fabricated
  PostCompact additionalContext (also forbidden by the official schema).
- PostToolUse: Writes ephemeral redacted tool receipts, tracks verification
  evidence, and compresses oversized tool outputs.
"""

from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
from typing import Any

from .evidence import build_evidence_record, extract_tool_success, read_focused_tests
from .protocol import HookInput, HookOutput, HookSpecificOutput
from .redaction import redact_text, redact_tool_input
from .telemetry import HookTelemetry


class SessionHandler:
    """Manages session lifecycle events: SessionStart, PreCompact, PostCompact, PostToolUse."""

    def __init__(self, state_dir: Path | str | None = None):
        if state_dir is not None:
            self.state_dir = Path(state_dir)
        else:
            env_dir = os.environ.get("STEWARD_SESSION_STATE_DIR")
            self.state_dir = Path(env_dir) if env_dir else Path("/tmp/codex_hooks_state")
        self.state_dir.mkdir(parents=True, exist_ok=True)
        self.receipts_dir = self.state_dir / "receipts"
        self.receipts_dir.mkdir(parents=True, exist_ok=True)
        self.compaction_path = self.state_dir / "compaction_state.json"
        self.evidence_file = self.state_dir / "verification_evidence.json"
        self.telemetry = HookTelemetry(self.state_dir)

    def handle_session_start(self, hook_input: HookInput) -> HookOutput:
        """Construct bounded context bootstrap for SessionStart.

        When the runtime reports source="compact", rehydrate from the
        PreCompact checkpoint instead of emitting the startup bootstrap.
        """
        if (hook_input.source or "") == "compact":
            return self._handle_compact_rehydration(hook_input)

        card_id = os.environ.get("STEWARD_WORKCARD_ID", "")
        worktree = os.environ.get("STEWARD_WORKTREE", "")
        allowed_raw = os.environ.get("STEWARD_ALLOWED_PATHS", "[]")
        forbidden_raw = os.environ.get("STEWARD_FORBIDDEN_PATHS", "[]")
        steps_raw = os.environ.get("STEWARD_CARD_OBJECTIVE", "[]")

        try:
            allowed = json.loads(allowed_raw) if allowed_raw else []
        except Exception:
            allowed = []
        try:
            forbidden = json.loads(forbidden_raw) if forbidden_raw else []
        except Exception:
            forbidden = []
        try:
            steps = json.loads(steps_raw) if steps_raw else []
        except Exception:
            steps = []

        context_lines = [
            "### Autonomous WorkCard Execution Context",
            f"- WorkCard ID: {card_id or 'unknown'}",
            f"- Workspace Root: {worktree or os.getcwd()}",
            f"- Allowed Target Paths: {', '.join(allowed) if allowed else 'all repository files'}",
        ]
        if forbidden:
            context_lines.append(f"- Strictly Forbidden Paths: {', '.join(forbidden)}")
        if steps:
            context_lines.append("- WorkCard Steps:")
            for s in steps:
                context_lines.append(f"  * {s}")

        context_lines.extend([
            "- Execution Rules:",
            "  * Steward is the sole lifecycle and durable persistence authority.",
            "  * Never modify files outside Allowed Target Paths.",
            "  * Verify all edits locally before finishing.",
            "  * Stop only when implementation and verification are complete.",
        ])

        injected_context = "\n".join(context_lines)
        self.telemetry.record_bootstrap(len(injected_context.encode("utf-8")))

        return HookOutput(
            continue_=True,
            hookSpecificOutput=HookSpecificOutput(
                hookEventName="SessionStart",
                additionalContext=injected_context,
            ),
        )

    def _handle_compact_rehydration(self, hook_input: HookInput) -> HookOutput:
        """Rehydrate constraints and progress after compaction.

        This is the sole post-compaction context injection path, driven by the
        real SessionStart(source="compact") contract. It reads the checkpoint
        persisted by handle_pre_compact; a missing checkpoint fails closed to
        a minimal constraint reminder rather than fabricated progress.
        """
        card_id = os.environ.get("STEWARD_WORKCARD_ID", "")
        allowed_raw = os.environ.get("STEWARD_ALLOWED_PATHS", "[]")
        try:
            allowed = json.loads(allowed_raw) if allowed_raw else []
        except Exception:
            allowed = []

        checkpoint: dict[str, Any] = {}
        if self.compaction_path.is_file() and not self.compaction_path.is_symlink():
            try:
                checkpoint = json.loads(self.compaction_path.read_text(encoding="utf-8"))
            except Exception:
                checkpoint = {}

        rehydrate_lines = [
            "### Compaction Rehydration Notice",
            f"- Continuing WorkCard: {card_id or 'unknown'}",
            f"- Allowed Scope: {', '.join(allowed) if allowed else 'workspace'}",
        ]
        diff_summary = checkpoint.get("modified_files", "") if isinstance(checkpoint, dict) else ""
        if diff_summary:
            rehydrate_lines.append(f"- Workspace changes already in progress:\n{diff_summary}")
        else:
            rehydrate_lines.append("- No checkpointed workspace changes; verify scope before editing.")
        rehydrate_lines.append("- Steward remains the sole lifecycle and persistence authority.")

        rehydrate_text = "\n".join(rehydrate_lines)
        self.telemetry.record_compaction(len(rehydrate_text.encode("utf-8")))

        return HookOutput(
            continue_=True,
            hookSpecificOutput=HookSpecificOutput(
                hookEventName="SessionStart",
                additionalContext=rehydrate_text,
            ),
        )

    def handle_pre_compact(self, hook_input: HookInput) -> HookOutput:
        """Snapshot active state and git status before compaction.

        Emits only the official top-level fields: the official pre-compact
        output schema forbids hookSpecificOutput.
        """
        card_id = os.environ.get("STEWARD_WORKCARD_ID", "")
        worktree = os.environ.get("STEWARD_WORKTREE", os.getcwd())

        # Collect current git status
        git_summary = ""
        try:
            proc = subprocess.run(
                ["git", "-C", worktree, "status", "--porcelain"],
                capture_output=True,
                text=True,
                timeout=5,
                check=False,
            )
            git_summary = proc.stdout.strip()
        except Exception:
            pass

        state = {
            "workcard_id": card_id,
            "session_id": hook_input.session_id,
            "modified_files": git_summary,
            "turn_id": hook_input.turn_id,
        }
        self.compaction_path.write_text(json.dumps(state, indent=2), encoding="utf-8")

        return HookOutput(continue_=True)

    def handle_post_compact(self, hook_input: HookInput) -> HookOutput:
        """Acknowledge compaction without injecting context.

        Post-compaction re-injection is owned exclusively by
        SessionStart(source="compact"); the official post-compact output
        schema forbids hookSpecificOutput, so nothing is emitted here.
        """
        return HookOutput(continue_=True)

    def handle_post_tool_use(self, hook_input: HookInput) -> HookOutput:
        """Record ephemeral redacted tool receipts and bound verification evidence.

        The receipt persists only redacted tool input (never raw secrets).
        PASS evidence is recorded only when the tool response carries a real
        machine-readable success signal; string output without structure never
        counts as success.
        """
        tool_name = hook_input.tool_name or "unknown_tool"
        raw_output = str(hook_input.tool_response or "")
        raw_len = len(raw_output)

        # Write ephemeral receipt with redacted input only
        receipt_count = len(list(self.receipts_dir.glob("receipt_*.json"))) + 1
        receipt_file = self.receipts_dir / f"receipt_{receipt_count:04d}_{tool_name}.json"
        receipt_data = {
            "receipt_id": receipt_count,
            "tool_name": tool_name,
            "tool_input": redact_tool_input(hook_input.tool_input),
            "response_bytes": raw_len,
            "turn_id": hook_input.turn_id,
        }
        receipt_file.write_text(json.dumps(receipt_data, indent=2), encoding="utf-8")

        # Telemetry compression
        summary_len = min(raw_len, 256)
        self.telemetry.record_tool_receipt(tool_name, raw_len, summary_len)

        # If tool execution was a test runner, record bound PASS evidence only
        # on a proven structured success signal.
        cmd_str = ""
        if isinstance(hook_input.tool_input, dict):
            for k in ("command", "CommandLine", "cmd"):
                if k in hook_input.tool_input and isinstance(hook_input.tool_input[k], str):
                    cmd_str = hook_input.tool_input[k]
                    break
        if cmd_str and any(t in cmd_str for t in ("pytest", "cargo test", "unittest")):
            success = extract_tool_success(hook_input.tool_response)
            if success is True:
                card_id = os.environ.get("STEWARD_WORKCARD_ID", "")
                worktree = os.environ.get("STEWARD_WORKTREE", os.getcwd())
                focused = self._focused_tests()
                record = build_evidence_record(
                    workcard_id=card_id,
                    focused_tests=focused,
                    command=redact_text(cmd_str),
                    success=True,
                    worktree=worktree,
                    receipt_id=receipt_count,
                )
                self.evidence_file.write_text(json.dumps(record, indent=2), encoding="utf-8")

        additional = None
        if raw_len > 4096:
            additional = f"[Receipt #{receipt_count:04d} recorded for {tool_name} ({raw_len} bytes)]"

        return HookOutput(
            continue_=True,
            hookSpecificOutput=HookSpecificOutput(
                hookEventName="PostToolUse",
                additionalContext=additional,
            ),
        )

    def _focused_tests(self) -> list[str]:
        return read_focused_tests()
