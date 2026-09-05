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
    """Permission decision compatibility enum."""

    ALLOW = "allow"
    BLOCK = "block"
    DENY = "deny"
    ASK = "ask"
    APPROVE = "approve"


class CapabilityStatus(str, Enum):
    """Evaluation status for runtime hook capabilities (H0 probe)."""

    VERIFIED = "VERIFIED"
    UNSUPPORTED = "UNSUPPORTED"
    BLOCKED = "BLOCKED"
    UNVERIFIED = "UNVERIFIED"


@dataclass(frozen=True)
class PermissionRequestDecisionWire:
    """Wire representation for decision in PermissionRequestHookSpecificOutput."""

    behavior: str  # "allow" | "deny"
    interrupt: bool = False
    message: str | None = None

    def to_dict(self) -> dict[str, Any]:
        out: dict[str, Any] = {"behavior": self.behavior}
        if self.interrupt:
            out["interrupt"] = True
        if self.message is not None:
            out["message"] = self.message
        return out


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
        if tool_input is None and isinstance(data.get("toolInput"), dict):
            tool_input = data.get("toolInput")
        return cls(
            hook_event_name=event,
            session_id=str(data.get("session_id") or data.get("sessionId") or ""),
            turn_id=str(data["turn_id"]) if "turn_id" in data and data["turn_id"] is not None else (str(data["turnId"]) if "turnId" in data and data["turnId"] is not None else None),
            cwd=str(data.get("cwd")) if data.get("cwd") is not None else None,
            model=str(data.get("model")) if data.get("model") is not None else None,
            permission_mode=str(data.get("permission_mode") or data.get("permissionMode")) if (data.get("permission_mode") or data.get("permissionMode")) is not None else None,
            transcript_path=str(data.get("transcript_path") or data.get("transcriptPath")) if (data.get("transcript_path") or data.get("transcriptPath")) is not None else None,
            tool_name=str(data.get("tool_name") or data.get("toolName")) if (data.get("tool_name") or data.get("toolName")) is not None else None,
            tool_input=tool_input,
            tool_response=data.get("tool_response") if "tool_response" in data else data.get("toolResponse"),
            tool_use_id=str(data["tool_use_id"]) if "tool_use_id" in data and data["tool_use_id"] is not None else (str(data["toolUseId"]) if "toolUseId" in data and data["toolUseId"] is not None else None),
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

    hookEventName: str | None = None
    permissionDecision: str | None = None  # "allow" | "deny" | "ask" (PreToolUse)
    permissionDecisionReason: str | None = None
    decision: PermissionRequestDecisionWire | None = None  # (PermissionRequest)
    additionalContext: str | None = None
    updatedInput: dict[str, Any] | None = None
    updatedMCPToolOutput: Any | None = None
    stopReason: str | None = None
    suppressOutput: bool | None = None

    def to_dict(self) -> dict[str, Any]:
        """Serialize non-None fields to wire dictionary."""
        out: dict[str, Any] = {}
        if self.hookEventName is not None:
            out["hookEventName"] = self.hookEventName
        if self.permissionDecision is not None:
            out["permissionDecision"] = self.permissionDecision
        if self.permissionDecisionReason is not None:
            out["permissionDecisionReason"] = self.permissionDecisionReason
        if self.decision is not None:
            out["decision"] = self.decision.to_dict()
        if self.additionalContext is not None:
            out["additionalContext"] = self.additionalContext
        if self.updatedInput is not None:
            out["updatedInput"] = self.updatedInput
        if self.updatedMCPToolOutput is not None:
            out["updatedMCPToolOutput"] = self.updatedMCPToolOutput
        if self.stopReason is not None:
            out["stopReason"] = self.stopReason
        if self.suppressOutput is not None:
            out["suppressOutput"] = self.suppressOutput
        return out


@dataclass(frozen=True)
class HookOutput:
    """Complete response returned by hook script on stdout."""

    continue_: bool = True
    decision: str | None = None  # "approve" | "block" for PreToolUse, "block" for Stop
    reason: str | None = None    # Required non-empty string when decision == "block"
    hookSpecificOutput: HookSpecificOutput | None = None
    stopReason: str | None = None
    suppressOutput: bool | None = None
    systemMessage: str | None = None

    def to_dict(self) -> dict[str, Any]:
        """Serialize to wire dictionary strictly matching official schemas."""
        out: dict[str, Any] = {"continue": self.continue_}
        if self.decision is not None:
            out["decision"] = self.decision
        if self.reason is not None:
            out["reason"] = self.reason
        if self.hookSpecificOutput is not None:
            specific = self.hookSpecificOutput.to_dict()
            if specific:
                out["hookSpecificOutput"] = specific
        if self.stopReason is not None:
            out["stopReason"] = self.stopReason
        if self.suppressOutput is not None:
            out["suppressOutput"] = self.suppressOutput
        if self.systemMessage is not None:
            out["systemMessage"] = self.systemMessage
        return out

    def to_json(self) -> str:
        """Serialize to JSON text."""
        return json.dumps(self.to_dict(), separators=(",", ":"))
