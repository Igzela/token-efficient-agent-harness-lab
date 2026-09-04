#!/usr/bin/env python3
"""Thin event dispatcher for Codex Lifecycle Hooks.

Single entrypoint invoked by the Codex hook execution engine:
`python3 dispatcher.py <event_name>`

Reads event context from stdin (JSON), routes to specialized event handlers
(Session, Guard, Continuation), and emits responses according to the Codex
hook wire protocol (JSON on stdout with exit 0, or blocking message on stderr
with exit 2).
"""

from __future__ import annotations

import json
import os
from pathlib import Path
import sys
from typing import Any

# Ensure parent directory is on sys.path when executed directly as a script
_CURRENT_DIR = Path(__file__).resolve().parent
_PKG_PARENT = _CURRENT_DIR.parent
if str(_PKG_PARENT) not in sys.path:
    sys.path.insert(0, str(_PKG_PARENT))

try:
    from .continuation import ContinuationHandler
    from .guard import GuardHandler
    from .protocol import HookEventName, HookInput, HookOutput, HookSpecificOutput, PermissionDecision
    from .session import SessionHandler
except ImportError:
    from codex_hooks.continuation import ContinuationHandler
    from codex_hooks.guard import GuardHandler
    from codex_hooks.protocol import HookEventName, HookInput, HookOutput, HookSpecificOutput, PermissionDecision
    from codex_hooks.session import SessionHandler


class HookDispatcher:
    """Routes Codex hook events to specialized handlers."""

    def __init__(self, state_dir: Path | str | None = None):
        self.state_dir = Path(state_dir) if state_dir else None
        self.session_handler = SessionHandler(self.state_dir)
        self.guard_handler = GuardHandler(self.state_dir)
        self.continuation_handler = ContinuationHandler(self.state_dir)

    def dispatch(self, event_name: str, raw_input: str) -> tuple[int, str, str]:
        """Dispatch event to handler.

        Returns (exit_code, stdout_str, stderr_str).
        """
        try:
            hook_input = HookInput.from_json(raw_input, event_override=event_name)
        except Exception as exc:
            # Malformed input: return exit 1 or 2 with error
            return 2, "", f"Malformed hook input: {exc}"

        event = hook_input.hook_event_name or event_name

        if event == HookEventName.SESSION_START.value:
            output = self.session_handler.handle_session_start(hook_input)
            return 0, output.to_json(), ""

        elif event == HookEventName.PRE_COMPACT.value:
            output = self.session_handler.handle_pre_compact(hook_input)
            return 0, output.to_json(), ""

        elif event == HookEventName.POST_COMPACT.value:
            output = self.session_handler.handle_post_compact(hook_input)
            return 0, output.to_json(), ""

        elif event == HookEventName.POST_TOOL_USE.value:
            output = self.session_handler.handle_post_tool_use(hook_input)
            return 0, output.to_json(), ""

        elif event == HookEventName.PRE_TOOL_USE.value:
            output = self.guard_handler.handle_pre_tool_use(hook_input)
            # If blocked, also support exit code 2 if reason present
            if output.hookSpecificOutput and output.hookSpecificOutput.permissionDecision == PermissionDecision.BLOCK.value:
                reason = output.hookSpecificOutput.permissionDecisionReason or "Tool use blocked by policy"
                # Emit both structured JSON on stdout and reason on stderr
                return 2, output.to_json(), reason
            return 0, output.to_json(), ""

        elif event == HookEventName.PERMISSION_REQUEST.value:
            output = self.guard_handler.handle_permission_request(hook_input)
            if output.hookSpecificOutput and output.hookSpecificOutput.permissionDecision == PermissionDecision.BLOCK.value:
                reason = output.hookSpecificOutput.permissionDecisionReason or "Permission denied by policy"
                return 2, output.to_json(), reason
            return 0, output.to_json(), ""

        elif event == HookEventName.STOP.value:
            exit_code, output, stderr_msg = self.continuation_handler.handle_stop(hook_input)
            return exit_code, output.to_json(), stderr_msg or ""

        # Pass-through for other events
        return 0, HookOutput().to_json(), ""


def main(argv: list[str] | None = None) -> int:
    """CLI entrypoint for hook execution."""
    args = argv if argv is not None else sys.argv[1:]
    event_name = args[0] if args else os.environ.get("HOOK_EVENT_NAME", "")
    if not event_name:
        sys.stderr.write("Missing hook event name\n")
        return 1

    try:
        raw_input = sys.stdin.read()
    except Exception:
        raw_input = ""

    dispatcher = HookDispatcher()
    exit_code, stdout_msg, stderr_msg = dispatcher.dispatch(event_name, raw_input)

    if stdout_msg:
        sys.stdout.write(stdout_msg)
        sys.stdout.flush()
    if stderr_msg:
        sys.stderr.write(stderr_msg + "\n")
        sys.stderr.flush()

    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
