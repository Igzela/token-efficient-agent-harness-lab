"""Optional Codex hooks for repository path and command safety."""

from .config import (
    DEFAULT_HOOK_EVENTS,
    EVENT_NAME_NORMALIZATION,
    HookConfigGenerator,
    compute_file_sha256,
    discover_hooks,
    hook_key,
    normalize_event_name,
    provision_trust,
)
from .dispatcher import HookDispatcher
from .guard import FORBIDDEN_COMMAND_PATTERNS, LOW_RISK_COMMAND_PREFIXES, TEST_RUNNER_PREFIXES, GuardHandler
from .official_schemas import EVENT_TO_SCHEMA_ID, extract_official_output_schemas, validate_hook_output
from .protocol import (
    HookEventName,
    HookInput,
    HookOutput,
    HookSpecificOutput,
    PermissionDecision,
    PermissionRequestDecisionWire,
)

__all__ = [
    "DEFAULT_HOOK_EVENTS",
    "EVENT_NAME_NORMALIZATION",
    "EVENT_TO_SCHEMA_ID",
    "FORBIDDEN_COMMAND_PATTERNS",
    "GuardHandler",
    "HookConfigGenerator",
    "HookDispatcher",
    "HookEventName",
    "HookInput",
    "HookOutput",
    "HookSpecificOutput",
    "LOW_RISK_COMMAND_PREFIXES",
    "PermissionDecision",
    "PermissionRequestDecisionWire",
    "TEST_RUNNER_PREFIXES",
    "compute_file_sha256",
    "discover_hooks",
    "extract_official_output_schemas",
    "hook_key",
    "normalize_event_name",
    "provision_trust",
    "validate_hook_output",
]
