"""Session context bootstrap, compaction rehydration, and receipts (H1).

Implements:
- SessionStart: Injects bounded WorkCard context, allowed paths, and execution invariants.
- PreCompact: Preserves active WorkCard state and git status before compaction.
- PostCompact: Rehydrates critical constraints and progress into the post-compaction context.
- PostToolUse: Writes ephemeral tool receipts and compresses oversized tool outputs.
"""

from __future__ import annotations

from dataclasses import asdict, dataclass
import json
import os
from pathlib import Path
import subprocess
from typing import Any

from .protocol import HookInput, HookOutput, HookSpecificOutput
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
        self.telemetry = HookTelemetry(self.state_dir)

    def handle_session_start(self, hook_input: HookInput) -> HookOutput:
        """Construct bounded context bootstrap for SessionStart."""
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
            hookSpecificOutput=HookSpecificOutput(
                additionalContext=injected_context,
            )
        )

    def handle_pre_compact(self, hook_input: HookInput) -> HookOutput:
        """Snapshot active state and git status before compaction."""
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

        return HookOutput(
            hookSpecificOutput=HookSpecificOutput(
                additionalContext="PreCompact: Active workcard state checkpointed.",
            )
        )

    def handle_post_compact(self, hook_input: HookInput) -> HookOutput:
        """Rehydrate active constraints and progress after compaction."""
        card_id = os.environ.get("STEWARD_WORKCARD_ID", "")
        allowed_raw = os.environ.get("STEWARD_ALLOWED_PATHS", "[]")

        try:
            allowed = json.loads(allowed_raw) if allowed_raw else []
        except Exception:
            allowed = []

        diff_summary = ""
        if self.compaction_path.is_file():
            try:
                data = json.loads(self.compaction_path.read_text(encoding="utf-8"))
                diff_summary = data.get("modified_files", "")
            except Exception:
                pass

        rehydrate_lines = [
            "### Compaction Rehydration Notice",
            f"- Continuing WorkCard: {card_id}",
            f"- Allowed Scope: {', '.join(allowed) if allowed else 'workspace'}",
        ]
        if diff_summary:
            rehydrate_lines.append(f"- Workspace changes already in progress:\n{diff_summary}")
        else:
            rehydrate_lines.append("- No uncommitted workspace changes detected yet.")

        rehydrate_text = "\n".join(rehydrate_lines)
        self.telemetry.record_compaction(len(rehydrate_text.encode("utf-8")))

        return HookOutput(
            hookSpecificOutput=HookSpecificOutput(
                additionalContext=rehydrate_text,
            )
        )

    def handle_post_tool_use(self, hook_input: HookInput) -> HookOutput:
        """Record ephemeral tool receipts and compress oversized outputs."""
        tool_name = hook_input.tool_name or "unknown_tool"
        raw_output = str(hook_input.tool_response or "")
        raw_len = len(raw_output)

        # Write ephemeral receipt
        receipt_count = len(list(self.receipts_dir.glob("receipt_*.json"))) + 1
        receipt_file = self.receipts_dir / f"receipt_{receipt_count:04d}_{tool_name}.json"
        receipt_data = {
            "receipt_id": receipt_count,
            "tool_name": tool_name,
            "tool_input": hook_input.tool_input,
            "response_bytes": raw_len,
            "turn_id": hook_input.turn_id,
        }
        receipt_file.write_text(json.dumps(receipt_data, indent=2), encoding="utf-8")

        # Telemetry compression
        summary_len = min(raw_len, 256)
        self.telemetry.record_tool_receipt(tool_name, raw_len, summary_len)

        additional = None
        if raw_len > 4096:
            additional = f"[Receipt #{receipt_count:04d} recorded for {tool_name} ({raw_len} bytes)]"

        return HookOutput(
            hookSpecificOutput=HookSpecificOutput(
                additionalContext=additional,
            )
        )
