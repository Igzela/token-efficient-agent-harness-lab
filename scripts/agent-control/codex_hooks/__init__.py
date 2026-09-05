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
    HOOK_EPHEMERAL_PATH_MARKERS,
    build_evidence_record,
    evidence_binding_matches,
    extract_tool_success,
    focused_tests_digest,
    is_hooks_ephemeral_status_path,
    porcelain_work_product_lines,
    read_allowed_paths,
    read_expected_evidence,
    read_focused_tests,
    read_negative_checks,
    workcard_acceptance_digest,
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
    "HOOK_EPHEMERAL_PATH_MARKERS",
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
    "is_hooks_ephemeral_status_path",
    "normalize_event_name",
    "porcelain_work_product_lines",
    "provision_trust",
    "read_allowed_paths",
    "read_expected_evidence",
    "read_focused_tests",
    "read_negative_checks",
    "redact_text",
    "redact_tool_input",
    "redact_value",
    "validate_hook_output",
    "workcard_acceptance_digest",
    "workspace_state",
]
