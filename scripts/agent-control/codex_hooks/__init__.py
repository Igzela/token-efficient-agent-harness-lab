"""Codex Lifecycle Hooks package.

Provides runtime capability probing (H0), session bootstrap & receipts (H1),
path boundaries & permission guards (H2), and completion continuation (H3).
"""

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
from .continuation import ContinuationDecision, ContinuationHandler
from .dispatcher import HookDispatcher
from .evidence import (
    EVIDENCE_SCHEMA_VERSION,
    build_evidence_record,
    evidence_binding_matches,
    extract_tool_success,
    focused_tests_digest,
    workspace_state,
)
from .guard import FORBIDDEN_COMMAND_PATTERNS, LOW_RISK_COMMAND_PREFIXES, TEST_RUNNER_PREFIXES, GuardHandler
from .official_schemas import EVENT_TO_SCHEMA_ID, extract_official_output_schemas, validate_hook_output
from .probe import CAPABILITY_NAMES, CodexHookProbe, CodexHookProbeResult
from .protocol import (
    CapabilityStatus,
    HookEventName,
    HookInput,
    HookOutput,
    HookSpecificOutput,
    PermissionDecision,
    PermissionRequestDecisionWire,
)
from .redaction import REDACTED, SENSITIVE_KEYS, redact_text, redact_tool_input, redact_value
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
    "EVIDENCE_SCHEMA_VERSION",
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
    "HookTelemetry",
    "HookTelemetryData",
    "LOW_RISK_COMMAND_PREFIXES",
    "PermissionDecision",
    "PermissionRequestDecisionWire",
    "REDACTED",
    "SENSITIVE_KEYS",
    "SessionHandler",
    "TEST_RUNNER_PREFIXES",
    "build_evidence_record",
    "compute_file_sha256",
    "discover_hooks",
    "evidence_binding_matches",
    "extract_official_output_schemas",
    "extract_tool_success",
    "focused_tests_digest",
    "hook_key",
    "normalize_event_name",
    "provision_trust",
    "redact_text",
    "redact_tool_input",
    "redact_value",
    "validate_hook_output",
    "workspace_state",
]
