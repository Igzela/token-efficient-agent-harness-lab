"""Wire protocol and models for Codex Lifecycle Hooks.

This module defines the JSON wire structures consumed and produced by
Codex hook events, as well as strict enums and serialization helpers.
Hooks are worker-local event adapters and never own repository lifecycle
or durable persistence.
"""

from __future__ import annotations

from dataclasses import asdict, dataclass, field
from enum import Enum
import json
from typing import Any, Mapping


class HookEventName(str, Enum):
    """Supported Codex hook event names."""

    SESSION_START = "SessionStart"
    SESSION_END = "SessionEnd"
    PRE_TOOL_USE = "PreToolUse"
    POST_TOOL_USE = "PostToolUse"
    PERMISSION_REQUEST = "PermissionRequest"
    PRE_COMPACT = "PreCompact"
    POST_COMPACT = "PostCompact"
    STOP = "Stop"
    INTERRUPT = "Interrupt"
    SUBAGENT_START = "SubagentStart"
    SUBAGENT_STOP = "SubagentStop"
    USER_PROMPT_SUBMIT = "UserPromptSubmit"


class PermissionDecision(str, Enum):
    """Permission decisions for PreToolUse and PermissionRequest hooks."""

    ALLOW = "allow"
    BLOCK = "block"
    ASK = "ask"


class CapabilityStatus(str, Enum):
    """Evaluation status for runtime hook capabilities (H0 probe)."""

    VERIFIED = "VERIFIED"
    UNSUPPORTED = "UNSUPPORTED"
    BLOCKED = "BLOCKED"
    UNVERIFIED = "UNVERIFIED"


@dataclass(frozen=True)
class HookInput:
    """Incoming payload delivered by Codex to a hook script via stdin."""

    hook_event_name: str
    session_id: str = ""
    turn_id: str | None = None
    cwd: str | None = None
    model: str | None = None
    permission_mode: str | None = None
    transcript_path: str | None = None
    tool_name: str | None = None
    tool_input: dict[str, Any] | None = None
    tool_response: Any | None = None
    tool_use_id: str | None = None
    trigger: str | None = None
    raw_payload: dict[str, Any] = field(default_factory=dict, hash=False, compare=False)

    @classmethod
    def from_dict(cls, data: Mapping[str, Any], event_override: str | None = None) -> HookInput:
        """Parse raw dictionary payload into HookInput."""
        event = event_override or str(data.get("hook_event_name") or data.get("hookEventName") or "")
        tool_input = data.get("tool_input") if isinstance(data.get("tool_input"), dict) else None
        return cls(
            hook_event_name=event,
            session_id=str(data.get("session_id") or ""),
            turn_id=str(data.get("turn_id")) if data.get("turn_id") is not None else None,
            cwd=str(data.get("cwd")) if data.get("cwd") is not None else None,
            model=str(data.get("model")) if data.get("model") is not None else None,
            permission_mode=str(data.get("permission_mode")) if data.get("permission_mode") is not None else None,
            transcript_path=str(data.get("transcript_path")) if data.get("transcript_path") is not None else None,
            tool_name=str(data.get("tool_name")) if data.get("tool_name") is not None else None,
            tool_input=tool_input,
            tool_response=data.get("tool_response"),
            tool_use_id=str(data.get("tool_use_id")) if data.get("tool_use_id") is not None else None,
            trigger=str(data.get("trigger")) if data.get("trigger") is not None else None,
            raw_payload=dict(data),
        )

    @classmethod
    def from_json(cls, raw: str, event_override: str | None = None) -> HookInput:
        """Parse JSON text into HookInput."""
        if not raw or not raw.strip():
            return cls(hook_event_name=event_override or "")
        parsed = json.loads(raw)
        if not isinstance(parsed, dict):
            raise ValueError("hook_input_must_be_json_object")
        return cls.from_dict(parsed, event_override=event_override)


@dataclass(frozen=True)
class HookSpecificOutput:
    """Wire representation for hookSpecificOutput in Codex response."""

    permissionDecision: str | None = None
    permissionDecisionReason: str | None = None
    additionalContext: str | None = None
    updatedInput: dict[str, Any] | None = None
    updatedPermissions: dict[str, Any] | None = None
    stopReason: str | None = None
    suppressOutput: bool | None = None

    def to_dict(self) -> dict[str, Any]:
        """Serialize non-None fields to wire dictionary."""
        out: dict[str, Any] = {}
        if self.permissionDecision is not None:
            out["permissionDecision"] = self.permissionDecision
        if self.permissionDecisionReason is not None:
            out["permissionDecisionReason"] = self.permissionDecisionReason
        if self.additionalContext is not None:
            out["additionalContext"] = self.additionalContext
        if self.updatedInput is not None:
            out["updatedInput"] = self.updatedInput
        if self.updatedPermissions is not None:
            out["updatedPermissions"] = self.updatedPermissions
        if self.stopReason is not None:
            out["stopReason"] = self.stopReason
        if self.suppressOutput is not None:
            out["suppressOutput"] = self.suppressOutput
        return out


@dataclass(frozen=True)
class HookOutput:
    """Complete response returned by hook script on stdout."""

    hookSpecificOutput: HookSpecificOutput | None = None
    systemMessage: str | None = None

    def to_dict(self) -> dict[str, Any]:
        """Serialize to wire dictionary."""
        out: dict[str, Any] = {}
        if self.hookSpecificOutput is not None:
            specific = self.hookSpecificOutput.to_dict()
            if specific:
                out["hookSpecificOutput"] = specific
        if self.systemMessage is not None:
            out["systemMessage"] = self.systemMessage
        return out

    def to_json(self) -> str:
        """Serialize to JSON text."""
        return json.dumps(self.to_dict(), separators=(",", ":"))
