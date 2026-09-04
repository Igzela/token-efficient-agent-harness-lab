"""Codex Lifecycle Hooks package.

Provides runtime capability probing (H0), session bootstrap & receipts (H1),
path boundaries & permission guards (H2), and completion continuation (H3).
"""

from .config import DEFAULT_HOOK_EVENTS, HookConfigGenerator, compute_bundle_hash
from .continuation import ContinuationDecision, ContinuationHandler
from .dispatcher import HookDispatcher
from .guard import FORBIDDEN_COMMAND_PATTERNS, GuardHandler
from .probe import CAPABILITY_NAMES, CodexHookProbe, CodexHookProbeResult
from .protocol import (
    CapabilityStatus,
    HookEventName,
    HookInput,
    HookOutput,
    HookSpecificOutput,
    PermissionDecision,
)
from .session import SessionHandler
from .telemetry import HookTelemetry, HookTelemetryData

__all__ = [
    "CAPABILITY_NAMES",
    "ContinuationDecision",
    "ContinuationHandler",
    "CodexHookProbe",
    "CodexHookProbeResult",
    "CapabilityStatus",
    "DEFAULT_HOOK_EVENTS",
    "FORBIDDEN_COMMAND_PATTERNS",
    "GuardHandler",
    "HookConfigGenerator",
    "HookDispatcher",
    "HookEventName",
    "HookInput",
    "HookOutput",
    "HookSpecificOutput",
    "HookTelemetry",
    "HookTelemetryData",
    "PermissionDecision",
    "SessionHandler",
    "compute_bundle_hash",
]
