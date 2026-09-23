"""Optional repository path and command safety checks for Codex hooks.

Implements:
- PreToolUse: Checks tool calls and rejects out-of-checkout paths, forbidden
  commands, or executions lacking a valid repository scope.
- PermissionRequest: Auto-approves only provably scoped, low-risk workspace actions;
  strictly fails closed on missing/malformed context or unknown/risky operations
  (no auto-allow on blacklist-miss).
"""

from __future__ import annotations

from pathlib import Path
import re
import shlex
import subprocess
from typing import Any

from .protocol import (
    HookInput,
    HookOutput,
    HookSpecificOutput,
    PermissionDecision,
    PermissionRequestDecisionWire,
)

FORBIDDEN_COMMAND_PATTERNS = (
    re.compile(r"\bgit\s+(?:push|fetch|pull|merge|remote|clone)\b"),
    re.compile(r"\brm\s+-[a-zA-Z]*[rf][a-zA-Z]*\s+(?:/|\.\.)(?:[\s;&|]|$)"),
    re.compile(r"\.git/config\b"),
    re.compile(r"\b(?:sudo|su|passwd|chown)\b"),
    re.compile(r"\b(?:curl|wget|ssh|nc|ncat|telnet|ftp|scp|rsync)\b"),
)

# Shell constructs whose effects cannot be statically proven. Any command
# containing them is blocked: the regex analysis below is explicitly NOT a
# complete shell parser, so unprovable scope fails closed.
UNPROVABLE_SHELL_CONSTRUCTS = (
    re.compile(r"\$\("),      # command substitution $(...)
    re.compile(r"`"),         # legacy command substitution
    re.compile(r"<\("),       # process substitution <(...)
    re.compile(r">\("),       # process substitution >(...)
)

# Redirection operators whose targets must be scope-checked. `2>&1`-style fd
# duplications, /dev/null, and /dev/std{out,err} are not file writes and are
# exempted explicitly in _is_command_low_risk_and_scoped.
REDIRECTION_PATTERN = re.compile(r"(?:\d*&?>>?\|?)\s*([^\s;&|]+)")

REDIRECTION_EXEMPT_TARGETS = ("/dev/null", "/dev/stdout", "/dev/stderr")

# Chain operators: each segment must independently prove scope. Split is
# quote-aware (single/double quotes); anything else unparseable fails closed.
# NOTE: `;;` is intentionally NOT a split operator (it is only valid inside
# `case` statements; there the whole command fails closed as unprovable).
# A trailing `;` yields an empty final segment and is therefore blocked:
# fail-closed, by design. `VAR=val cmd` / `env VAR=val cmd` prefixes are not
# unwrapped and are blocked as unprovable (known limitation, documented).
CHAIN_SPLIT_PATTERN = re.compile(r"&&|\|\||;|\|")

LOW_RISK_COMMAND_PREFIXES = (
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

# Verification-runner heads. These execute repository code, so every
# path-like argument must remain inside the current Git checkout.
TEST_RUNNER_PREFIXES = (
    "pytest",
    "python3 -m unittest",
    "python -m unittest",
    "python3 -m py_compile",
    "python -m py_compile",
    "cargo test",
    "cargo check",
    "cargo build",
    "uv run",
)


class GuardHandler:
    """Enforce checkout boundaries and reject unprovable or dangerous commands."""

    def __init__(self, state_dir: Path | str | None = None):
        # Kept for uniform construction; the policy handler is stateless.
        self.state_dir = Path(state_dir) if state_dir is not None else None

    def _get_context(self) -> tuple[bool, str, Path, list[str], list[str]]:
        """Resolve the actual Git checkout and its mandatory protected paths."""
        current_dir = Path.cwd().resolve()
        try:
            repository_result = subprocess.run(
                ["git", "rev-parse", "--show-toplevel"],
                cwd=current_dir,
                capture_output=True,
                text=True,
                timeout=5,
                check=False,
            )
            if repository_result.returncode != 0 or not repository_result.stdout.strip():
                return False, "repository_root_unavailable", current_dir, [], []
            worktree = Path(repository_result.stdout.strip()).resolve()
        except (OSError, RuntimeError, ValueError, subprocess.TimeoutExpired):
            return False, "repository_root_unavailable", current_dir, [], []

        return True, "", worktree, ["."], [".git/", ".github/"]

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
        """Check if candidate path is allowed under the current repository scope."""
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
            if allow_clean == ".":
                allowed_match = True
                break
            if rel_str == allow_clean or rel_str.startswith(f"{allow_clean}/"):
                allowed_match = True
                break
            # Also allow parent directories of allowed target
            if allow_clean.startswith(f"{rel_str}/"):
                allowed_match = True
                break

        if not allowed_match:
            return False, f"Path outside allowed repository paths: {rel_str} (allowed: {allowed_paths})"

        return True, ""

    def _split_command_chain(self, cmd_str: str) -> list[str] | None:
        """Quote-aware split of a shell command on chain operators.

        Returns None when quotes are unbalanced (unparseable -> fail closed).
        """
        segments: list[str] = []
        current: list[str] = []
        quote: str | None = None
        i = 0
        n = len(cmd_str)
        while i < n:
            ch = cmd_str[i]
            if quote is not None:
                current.append(ch)
                if ch == quote:
                    quote = None
                i += 1
                continue
            if ch in ("'", '"'):
                quote = ch
                current.append(ch)
                i += 1
                continue
            if ch == "\\" and i + 1 < n:
                current.append(ch)
                current.append(cmd_str[i + 1])
                i += 2
                continue
            m = CHAIN_SPLIT_PATTERN.match(cmd_str, i)
            if m:
                segments.append("".join(current))
                current = []
                i = m.end()
                continue
            current.append(ch)
            i += 1
        if quote is not None:
            return None
        segments.append("".join(current))
        return segments

    def _is_path_like_arg(self, token: str) -> bool:
        """Heuristic: does this argv token look like a filesystem path?"""
        if not token or token.startswith("-"):
            return False
        if "/" in token or "\\" in token:
            return True
        if re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_.\-]*\.[A-Za-z0-9]{1,5}", token):
            return True
        return False

    def _is_script_invocation_scoped(
        self,
        parts: list[str],
        worktree_root: Path,
        allowed_paths: list[str],
        forbidden_paths: list[str],
    ) -> tuple[bool, str]:
        """Scope-check `python <script> [args...]` / `./script [args...]`."""
        head = parts[0]
        if head.startswith("./"):
            script_args = [head[2:]] + parts[1:]
        elif head == "python" or head == "python3":
            if len(parts) < 2:
                return False, "python invocation without target"
            if parts[1].startswith("-"):
                # Covers `python -c ...` and any other flag-led execution:
                # arbitrary code execution can never prove scope.
                return False, f"python flag-led execution is not provably scoped: {parts[1]}"
            script_args = parts[1:]
        else:
            return False, "not a script invocation"

        for arg in script_args:
            if self._is_path_like_arg(arg):
                ok, reason = self._is_path_allowed(arg, worktree_root, allowed_paths, forbidden_paths)
                if not ok:
                    return False, f"script argument out of scope: {reason}"
        # The entry script itself must always be in scope.
        ok, reason = self._is_path_allowed(
            script_args[0], worktree_root, allowed_paths, forbidden_paths
        )
        if not ok:
            return False, f"script target out of scope: {reason}"
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

        # 2. Unprovable shell constructs fail closed: this analysis is
        # explicitly not a complete shell parser.
        for pat in UNPROVABLE_SHELL_CONSTRUCTS:
            if pat.search(stripped):
                return False, f"unprovable_shell_construct: {pat.pattern}"

        # 3. Check shell redirection targets (whole command, all segments)
        for m in REDIRECTION_PATTERN.finditer(stripped):
            target = m.group(1).strip()
            if not target:
                continue
            if target in REDIRECTION_EXEMPT_TARGETS:
                continue
            if re.fullmatch(r"&\d+", target):
                continue  # fd duplication (e.g. 2>&1), not a file write
            ok, reason = self._is_path_allowed(target, worktree_root, allowed_paths, forbidden_paths)
            if not ok:
                return False, f"command_redirection_out_of_scope: {reason}"

        # 4. Every chain segment must independently prove scope.
        segments = self._split_command_chain(stripped)
        if segments is None:
            return False, "unbalanced_quotes_unparseable_command"
        for segment in segments:
            seg = segment.strip()
            if not seg:
                return False, "empty_chain_segment"
            ok, reason = self._is_segment_low_risk_and_scoped(
                seg, worktree_root, allowed_paths, forbidden_paths
            )
            if not ok:
                return False, reason

        return True, ""

    def _is_test_runner_segment_allowed(
        self,
        seg: str,
        worktree_root: Path,
        allowed_paths: list[str],
        forbidden_paths: list[str],
    ) -> tuple[bool, str]:
        """Allow verification commands whose path arguments stay in the checkout."""
        try:
            parts = shlex.split(seg)
        except Exception:
            return False, "command_not_provably_scoped_or_low_risk: unparseable segment"
        for arg in parts[1:]:
            if not self._is_path_like_arg(arg):
                continue
            ok, _reason = self._is_path_allowed(arg, worktree_root, allowed_paths, forbidden_paths)
            if ok:
                continue
            return False, (
                f"test_runner_target_out_of_scope: {arg} (allowed: {allowed_paths})"
            )
        return True, ""

    def _is_segment_low_risk_and_scoped(
        self,
        seg: str,
        worktree_root: Path,
        allowed_paths: list[str],
        forbidden_paths: list[str],
    ) -> tuple[bool, str]:
        """Verify a single chain segment is provably safe.

        Read-only inspection tools are allowed unconditionally. Verification
        runners are allowed only for checkout-scoped targets. Scoped script
        execution (python/<.->) is allowed only when every path-like argument
        is in scope. Everything else fails closed.
        """
        # 1. Read-only whitelist: workspace inspection without write capability.
        for prefix in LOW_RISK_COMMAND_PREFIXES:
            if seg == prefix or seg.startswith(f"{prefix} ") or seg.startswith(f"{prefix}\t"):
                return True, ""

        # 2. Verification runners: scoped or explicitly declared targets only.
        for prefix in TEST_RUNNER_PREFIXES:
            if seg == prefix or seg.startswith(f"{prefix} ") or seg.startswith(f"{prefix}\t"):
                return self._is_test_runner_segment_allowed(
                    seg, worktree_root, allowed_paths, forbidden_paths
                )

        # 3. Scoped script execution (python <in-scope script>, ./<in-scope script>).
        try:
            parts = shlex.split(seg)
        except Exception:
            return False, "command_not_provably_scoped_or_low_risk: unparseable segment"
        if parts and (parts[0] in ("python", "python3") or parts[0].startswith("./")):
            ok, reason = self._is_script_invocation_scoped(parts, worktree_root, allowed_paths, forbidden_paths)
            if ok:
                return True, ""
            return False, reason

        return False, f"command_not_provably_scoped_or_low_risk: {seg[:80]}"

    def handle_pre_tool_use(self, hook_input: HookInput) -> HookOutput:
        """Evaluate PreToolUse against repository scope and command safety.

        Shell/exec commands are approved only when provably scoped and
        low-risk; anything else (touch/cp/mv/tee/sed/python -c and friends)
        is blocked. The static analysis is explicitly not a complete shell
        parser, so unprovable scope fails closed.
        """
        tool_input = hook_input.tool_input or {}

        # 1. Context validation (fail-closed)
        is_valid, ctx_err, worktree, allowed, forbidden = self._get_context()
        if not is_valid:
            reason = f"repository_root_unavailable: {ctx_err}"
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

        # 4. Shell/exec commands must prove scope: provably low-risk and
        # in-scope, otherwise block (no auto-allow after path extraction).
        # A tool call with neither an observable command nor extractable
        # paths has no provable surface at all and fails closed as well.
        if not cmd_str and not paths:
            reason = "unprovable_scope: no observable command or paths in tool input"
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
        if cmd_str:
            ok, reason = self._is_command_low_risk_and_scoped(cmd_str, worktree, allowed, forbidden)
            if not ok:
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
                        message=f"repository_root_unavailable: {ctx_err}",
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

        # If no command or paths were present the action has no provable
        # surface at all: do NOT auto-allow (a bare tool name proves nothing).
        if not paths and not cmd_str:
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
