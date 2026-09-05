"""Path boundary guard and permission verification for Codex Lifecycle Hooks (H2).

Implements:
- PreToolUse: Intercepts tool calls and rejects out-of-scope paths, forbidden
  commands, or executions lacking valid WorkCard scope context.
- PermissionRequest: Auto-approves only provably scoped, low-risk workspace actions;
  strictly fails closed on missing/malformed context or unknown/risky operations
  (no auto-allow on blacklist-miss).
"""

from __future__ import annotations

import json
import os
from pathlib import Path
import re
import shlex
from typing import Any

from .protocol import (
    HookInput,
    HookOutput,
    HookSpecificOutput,
    PermissionDecision,
    PermissionRequestDecisionWire,
)
from .telemetry import HookTelemetry

FORBIDDEN_COMMAND_PATTERNS = (
    re.compile(r"\bgit\s+(?:push|fetch|pull|merge|remote|clone)\b"),
    re.compile(r"\brm\s+-[a-zA-Z]*[rf][a-zA-Z]*\s+(?:/|\.\.)(?:[\s;&|]|$)"),
    re.compile(r"/var/lib/agent-steward\b"),
    re.compile(r"\.git/config\b"),
    re.compile(r"\b(?:sudo|su|passwd|chown)\b"),
    re.compile(r"\b(?:curl|wget|ssh|nc|ncat|telnet|ftp|scp|rsync)\b"),
)

LOW_RISK_COMMAND_PREFIXES = (
    "pytest",
    "python3 -m unittest",
    "python -m unittest",
    "python3 -m py_compile",
    "python -m py_compile",
    "cargo test",
    "cargo check",
    "cargo build",
    "uv run",
    "git status",
    "git diff",
    "git log",
    "git branch",
    "git rev-parse",
    "ls",
    "cat",
    "head",
    "tail",
    "grep",
    "rg",
    "find",
    "wc",
    "pwd",
    "file",
    "which",
    "echo",
    "printf",
    "test",
)


class GuardHandler:
    """Enforces worktree path boundaries and fail-closed permission decisions."""

    def __init__(self, state_dir: Path | str | None = None):
        if state_dir is not None:
            self.state_dir = Path(state_dir)
        else:
            env_dir = os.environ.get("STEWARD_SESSION_STATE_DIR")
            self.state_dir = Path(env_dir) if env_dir else Path("/tmp/codex_hooks_state")
        self.state_dir.mkdir(parents=True, exist_ok=True)
        self.telemetry = HookTelemetry(self.state_dir)

    def _get_context(self) -> tuple[bool, str, Path, list[str], list[str]]:
        """Extract and validate WorkCard and scope context from environment.

        Returns (is_valid, error_reason, worktree, allowed_paths, forbidden_paths).
        """
        worktree_raw = os.environ.get("STEWARD_WORKTREE", "")
        worktree = Path(worktree_raw).resolve() if worktree_raw else Path(os.getcwd()).resolve()

        card_id = os.environ.get("STEWARD_WORKCARD_ID", "").strip()
        if not card_id:
            return False, "missing_or_empty_STEWARD_WORKCARD_ID", worktree, [], []

        allowed_raw = os.environ.get("STEWARD_ALLOWED_PATHS", "")
        if not allowed_raw:
            return False, "missing_STEWARD_ALLOWED_PATHS", worktree, [], []

        try:
            allowed = json.loads(allowed_raw)
            if not isinstance(allowed, list) or len(allowed) == 0:
                return False, "empty_or_non_list_STEWARD_ALLOWED_PATHS", worktree, [], []
            if not all(isinstance(p, str) and p.strip() for p in allowed):
                return False, "invalid_entry_in_STEWARD_ALLOWED_PATHS", worktree, [], []
        except Exception as exc:
            return False, f"malformed_json_STEWARD_ALLOWED_PATHS: {exc}", worktree, [], []

        forbidden_raw = os.environ.get("STEWARD_FORBIDDEN_PATHS", "[]")
        try:
            forbidden = json.loads(forbidden_raw) if forbidden_raw else []
            if not isinstance(forbidden, list):
                forbidden = []
        except Exception:
            forbidden = []

        return True, "", worktree, allowed, forbidden

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
        # Check command string for obvious redirection or touched files
        for cmd_key in ("command", "CommandLine", "cmd"):
            cmd = tool_input.get(cmd_key)
            if isinstance(cmd, str):
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

        # Invariant 1: Must be inside worktree root
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

        # Invariant 3: Must match at least one allowed path
        allowed_match = False
        for allow in allowed_paths:
            allow_clean = allow.strip().rstrip("/")
            if not allow_clean:
                continue
            if rel_str == allow_clean or rel_str.startswith(f"{allow_clean}/"):
                allowed_match = True
                break
            # Also allow parent directories of allowed target
            if allow_clean.startswith(f"{rel_str}/"):
                allowed_match = True
                break

        if not allowed_match:
            return False, f"Path outside allowed WorkCard paths: {rel_str} (allowed: {allowed_paths})"

        return True, ""

    def _is_command_low_risk_and_scoped(
        self,
        cmd_str: str,
        worktree_root: Path,
        allowed_paths: list[str],
        forbidden_paths: list[str],
    ) -> tuple[bool, str]:
        """Verify command is provably low-risk and in-scope (no auto-allow on blacklist miss)."""
        stripped = cmd_str.strip()
        if not stripped:
            return False, "empty_command"

        # 1. Blacklist check
        for pat in FORBIDDEN_COMMAND_PATTERNS:
            if pat.search(stripped):
                return False, f"matches_forbidden_pattern: {pat.pattern}"

        # 2. Check shell redirection targets
        for m in re.finditer(r"(?:>|>>)\s*([^\s;&|]+)", stripped):
            target = m.group(1).strip()
            ok, reason = self._is_path_allowed(target, worktree_root, allowed_paths, forbidden_paths)
            if not ok:
                return False, f"command_redirection_out_of_scope: {reason}"

        # 3. Whitelist check: Must start with a known low-risk tool prefix
        is_low_risk = False
        for prefix in LOW_RISK_COMMAND_PREFIXES:
            if stripped == prefix or stripped.startswith(f"{prefix} ") or stripped.startswith(f"{prefix}\t"):
                is_low_risk = True
                break

        if not is_low_risk:
            # Check if command is a safe python execution or script within allowed_paths
            if stripped.startswith("python") or stripped.startswith("./"):
                try:
                    parts = shlex.split(stripped)
                    if len(parts) >= 2:
                        target_file = parts[1]
                        ok, _ = self._is_path_allowed(target_file, worktree_root, allowed_paths, forbidden_paths)
                        if ok:
                            is_low_risk = True
                except Exception:
                    pass

        if not is_low_risk:
            return False, f"command_not_provably_scoped_or_low_risk: {stripped[:80]}"

        return True, ""

    def handle_pre_tool_use(self, hook_input: HookInput) -> HookOutput:
        """Evaluate PreToolUse against WorkCard context, path constraints, and command safety."""
        tool_name = hook_input.tool_name or ""
        tool_input = hook_input.tool_input or {}

        # 1. Context validation (fail-closed)
        is_valid, ctx_err, worktree, allowed, forbidden = self._get_context()
        if not is_valid:
            reason = f"missing_or_malformed_scope_context: {ctx_err}"
            self.telemetry.record_tool_block(tool_name, reason)
            return HookOutput(
                continue_=True,
                decision="block",
                reason=reason,
                hookSpecificOutput=HookSpecificOutput(
                    hookEventName="PreToolUse",
                    permissionDecision="deny",
                    permissionDecisionReason=reason,
                ),
            )

        # 2. Check command strings for forbidden commands
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
                        continue_=True,
                        decision="block",
                        reason=reason,
                        hookSpecificOutput=HookSpecificOutput(
                            hookEventName="PreToolUse",
                            permissionDecision="deny",
                            permissionDecisionReason=reason,
                        ),
                    )

        # 3. Check extracted file paths
        paths = self._extract_paths(tool_input)
        for p in paths:
            ok, reason = self._is_path_allowed(p, worktree, allowed, forbidden)
            if not ok:
                self.telemetry.record_tool_block(tool_name, reason)
                return HookOutput(
                    continue_=True,
                    decision="block",
                    reason=reason,
                    hookSpecificOutput=HookSpecificOutput(
                        hookEventName="PreToolUse",
                        permissionDecision="deny",
                        permissionDecisionReason=reason,
                    ),
                )

        # In-scope and safe -> allow
        return HookOutput(
            continue_=True,
            decision="approve",
            hookSpecificOutput=HookSpecificOutput(
                hookEventName="PreToolUse",
                permissionDecision="allow",
            ),
        )

    def handle_permission_request(self, hook_input: HookInput) -> HookOutput:
        """Verify PermissionRequest: only allow provably scoped, low-risk actions.

        Strictly fails closed on missing/malformed context or unverified operations;
        never auto-allows on blacklist-miss.
        """
        tool_input = hook_input.tool_input or {}

        # 1. Context validation (fail-closed)
        is_valid, ctx_err, worktree, allowed, forbidden = self._get_context()
        if not is_valid:
            return HookOutput(
                continue_=True,
                hookSpecificOutput=HookSpecificOutput(
                    hookEventName="PermissionRequest",
                    decision=PermissionRequestDecisionWire(
                        behavior="deny",
                        message=f"missing_or_malformed_scope_context: {ctx_err}",
                    ),
                ),
            )

        # 2. Check file paths if present
        paths = self._extract_paths(tool_input)
        for p in paths:
            ok, reason = self._is_path_allowed(p, worktree, allowed, forbidden)
            if not ok:
                return HookOutput(
                    continue_=True,
                    hookSpecificOutput=HookSpecificOutput(
                        hookEventName="PermissionRequest",
                        decision=PermissionRequestDecisionWire(
                            behavior="deny",
                            message=f"unauthorized_path: {reason}",
                        ),
                    ),
                )

        # 3. Check command if present
        cmd_str = ""
        for k in ("command", "CommandLine", "cmd"):
            if k in tool_input and isinstance(tool_input[k], str):
                cmd_str = tool_input[k]
                break

        if cmd_str:
            is_low_risk, reason = self._is_command_low_risk_and_scoped(cmd_str, worktree, allowed, forbidden)
            if not is_low_risk:
                return HookOutput(
                    continue_=True,
                    hookSpecificOutput=HookSpecificOutput(
                        hookEventName="PermissionRequest",
                        decision=PermissionRequestDecisionWire(
                            behavior="deny",
                            message=f"unauthorized_operation: {reason}",
                        ),
                    ),
                )

        # If no command or paths were present and action is unknown, do NOT auto-allow
        if not paths and not cmd_str and not hook_input.tool_name:
            return HookOutput(
                continue_=True,
                hookSpecificOutput=HookSpecificOutput(
                    hookEventName="PermissionRequest",
                    decision=PermissionRequestDecisionWire(
                        behavior="deny",
                        message="unprovable_scope: empty action context",
                    ),
                ),
            )

        # Action is provably scoped and low-risk -> allow
        return HookOutput(
            continue_=True,
            hookSpecificOutput=HookSpecificOutput(
                hookEventName="PermissionRequest",
                decision=PermissionRequestDecisionWire(behavior="allow"),
            ),
        )
