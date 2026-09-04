"""Path boundary guard and permission auto-approval for Codex Lifecycle Hooks (H2).

Implements:
- PreToolUse: Intercepts tool calls and rejects out-of-scope paths or forbidden commands.
- PermissionRequest: Auto-approves bounded, in-scope execution while blocking dangerous operations.
"""

from __future__ import annotations

import json
import os
from pathlib import Path, PurePosixPath
import re
import shlex
from typing import Any

from .protocol import HookInput, HookOutput, HookSpecificOutput, PermissionDecision
from .telemetry import HookTelemetry


FORBIDDEN_COMMAND_PATTERNS = (
    re.compile(r"\bgit\s+(?:push|fetch|pull|merge|remote)\b"),
    re.compile(r"\brm\s+-[a-zA-Z]*[rf][a-zA-Z]*\s+(?:/|\.\.)(?:[\s;&|]|$)"),
    re.compile(r"/var/lib/agent-steward\b"),
    re.compile(r"\.git/config\b"),
)


class GuardHandler:
    """Enforces worktree path boundaries and permission decisions."""

    def __init__(self, state_dir: Path | str | None = None):
        if state_dir is not None:
            self.state_dir = Path(state_dir)
        else:
            env_dir = os.environ.get("STEWARD_SESSION_STATE_DIR")
            self.state_dir = Path(env_dir) if env_dir else Path("/tmp/codex_hooks_state")
        self.state_dir.mkdir(parents=True, exist_ok=True)
        self.telemetry = HookTelemetry(self.state_dir)

    def _extract_paths(self, tool_input: dict[str, Any] | None) -> list[str]:
        """Extract candidate target file paths from tool input payload."""
        if not tool_input:
            return []
        paths: list[str] = []
        for key in (
            "TargetFile", "target_file", "FilePath", "file_path",
            "path", "Path", "file", "dest", "destination", "target",
        ):
            val = tool_input.get(key)
            if isinstance(val, str) and val.strip():
                paths.append(val.strip())
        # Check command string for obvious redirection or files
        for cmd_key in ("command", "CommandLine", "cmd"):
            cmd = tool_input.get(cmd_key)
            if isinstance(cmd, str):
                # Match common file redirections or touches
                for m in re.finditer(r"(?:>|>>)\s*([^\s;&|]+)", cmd):
                    paths.append(m.group(1).strip())
        return paths

    def _is_path_allowed(
        self,
        candidate_path: str,
        worktree_root: Path,
        allowed_paths: list[str],
        forbidden_paths: list[str],
    ) -> tuple[bool, str]:
        """Check if candidate path is allowed under current WorkCard constraints."""
        cand = Path(candidate_path)
        if not cand.is_absolute():
            cand = (worktree_root / cand).resolve()
        else:
            cand = cand.resolve()

        # Invariant 1: Must be inside worktree
        try:
            rel = cand.relative_to(worktree_root.resolve())
            rel_str = str(rel)
        except ValueError:
            return False, f"Path escapes workspace root: {candidate_path}"

        # Invariant 2: Cannot touch forbidden paths
        for forb in forbidden_paths:
            forb_clean = forb.strip().rstrip("/")
            if not forb_clean:
                continue
            if rel_str == forb_clean or rel_str.startswith(f"{forb_clean}/"):
                return False, f"Path touches strictly forbidden scope: {rel_str} (forbidden: {forb})"

        # Invariant 3: If allowed_paths specified, must match at least one allowed prefix
        if allowed_paths:
            allowed_match = False
            for allow in allowed_paths:
                allow_clean = allow.strip().rstrip("/")
                if not allow_clean:
                    continue
                if rel_str == allow_clean or rel_str.startswith(f"{allow_clean}/"):
                    allowed_match = True
                    break
                # Also allow parent directory leading to allowed path (e.g. if writing into directory)
                if allow_clean.startswith(f"{rel_str}/"):
                    allowed_match = True
                    break
            if not allowed_match:
                return False, f"Path outside allowed WorkCard paths: {rel_str} (allowed: {allowed_paths})"

        return True, ""

    def handle_pre_tool_use(self, hook_input: HookInput) -> HookOutput:
        """Evaluate PreToolUse against path constraints and command safety."""
        tool_name = hook_input.tool_name or ""
        tool_input = hook_input.tool_input or {}
        worktree = Path(os.environ.get("STEWARD_WORKTREE", os.getcwd())).resolve()

        allowed_raw = os.environ.get("STEWARD_ALLOWED_PATHS", "[]")
        forbidden_raw = os.environ.get("STEWARD_FORBIDDEN_PATHS", "[]")
        try:
            allowed = json.loads(allowed_raw) if allowed_raw else []
        except Exception:
            allowed = []
        try:
            forbidden = json.loads(forbidden_raw) if forbidden_raw else []
        except Exception:
            forbidden = []

        # 1. Check command strings for forbidden commands
        cmd_str = ""
        for k in ("command", "CommandLine", "cmd"):
            if k in tool_input and isinstance(tool_input[k], str):
                cmd_str = tool_input[k]
                break

        if cmd_str:
            for pat in FORBIDDEN_COMMAND_PATTERNS:
                if pat.search(cmd_str):
                    reason = f"Command matches forbidden pattern ({pat.pattern}): {cmd_str[:80]}"
                    self.telemetry.record_tool_block(tool_name, reason)
                    return HookOutput(
                        hookSpecificOutput=HookSpecificOutput(
                            permissionDecision=PermissionDecision.BLOCK.value,
                            permissionDecisionReason=reason,
                        )
                    )

        # 2. Check extracted file paths
        paths = self._extract_paths(tool_input)
        for p in paths:
            ok, reason = self._is_path_allowed(p, worktree, allowed, forbidden)
            if not ok:
                self.telemetry.record_tool_block(tool_name, reason)
                return HookOutput(
                    hookSpecificOutput=HookSpecificOutput(
                        permissionDecision=PermissionDecision.BLOCK.value,
                        permissionDecisionReason=reason,
                    )
                )

        # In-scope and safe -> allow
        return HookOutput(
            hookSpecificOutput=HookSpecificOutput(
                permissionDecision=PermissionDecision.ALLOW.value,
            )
        )

    def handle_permission_request(self, hook_input: HookInput) -> HookOutput:
        """Auto-approve bounded workspace actions; block dangerous external operations."""
        # Check command or tool input
        tool_input = hook_input.tool_input or {}
        cmd_str = ""
        for k in ("command", "CommandLine", "cmd"):
            if k in tool_input and isinstance(tool_input[k], str):
                cmd_str = tool_input[k]
                break

        if cmd_str:
            for pat in FORBIDDEN_COMMAND_PATTERNS:
                if pat.search(cmd_str):
                    reason = f"Permission denied for forbidden command pattern: {cmd_str[:80]}"
                    return HookOutput(
                        hookSpecificOutput=HookSpecificOutput(
                            permissionDecision=PermissionDecision.BLOCK.value,
                            permissionDecisionReason=reason,
                        )
                    )

        return HookOutput(
            hookSpecificOutput=HookSpecificOutput(
                permissionDecision=PermissionDecision.ALLOW.value,
            )
        )
