"""Secret redaction for ephemeral hook receipts (H1).

PostToolUse receipts must never persist raw ``tool_input``: API keys, tokens,
credentials, and sensitive assignments are masked before anything reaches
disk. The patterns below are derived from the repository-canonical secret
scanner (``scripts/acp_secret_scan.py``); ``test_redaction_parity_with_scanner``
pins every canonical pattern so scanner/receipt drift fails loudly instead of
leaking.

This module is worker-local (copied with the ``codex_hooks`` package into the
isolated worker root) and therefore carries its own copy of the patterns
rather than importing the scanner at runtime.
"""

from __future__ import annotations

import re
from typing import Any

# Mirrors scripts/acp_secret_scan.py::SECRET_PATTERNS (canonical source).
SECRET_PATTERNS: list[tuple[str, re.Pattern[str]]] = [
    ("anthropic_token", re.compile(r"\btp-[A-Za-z0-9]{24,}\b")),
    ("openrouter_key", re.compile(r"\bsk-or-v1-[A-Za-z0-9]{24,}\b")),
    ("openai_key", re.compile(r"\bsk-[A-Za-z0-9]{32,}\b")),
    ("google_key", re.compile(r"\bAIza[A-Za-z0-9_-]{20,}\b")),
    ("aws_access_key", re.compile(r"\bAKIA[0-9A-Z]{16}\b")),
    ("local_admin_key", re.compile(r"\bharness_[0-9a-fA-F]{64}\b")),
]

# Mirrors scripts/acp_secret_scan.py::SENSITIVE_ASSIGNMENT (canonical source).
SENSITIVE_ASSIGNMENT = re.compile(
    r"(?i)\b(api[_-]?key|auth[_-]?token|access[_-]?token|secret|password|credential)\b\s*=\s*([^#\s].*)"
)

# Structured keys whose values are always sensitive regardless of content.
SENSITIVE_KEYS = frozenset({
    "api_key",
    "apikey",
    "api-key",
    "auth_token",
    "authtoken",
    "auth-token",
    "access_token",
    "accesstoken",
    "access-token",
    "secret",
    "client_secret",
    "password",
    "passwd",
    "pwd",
    "credential",
    "credentials",
    "authorization",
    "proxy-authorization",
    "token",
    "id_token",
    "refresh_token",
    "private_key",
    "session_token",
})

REDACTED = "***"


def redact_text(value: str) -> str:
    """Mask secrets inside a free-form string."""
    redacted = value
    for _name, pattern in SECRET_PATTERNS:
        redacted = pattern.sub(REDACTED, redacted)
    match = SENSITIVE_ASSIGNMENT.search(redacted)
    if match:
        redacted = f"{redacted[:match.start(2)]}{REDACTED}"
    return redacted


def _is_sensitive_key(key: str) -> bool:
    normalized = key.strip().lower().replace("_", "").replace("-", "")
    compact = {k.replace("_", "").replace("-", "") for k in SENSITIVE_KEYS}
    return normalized in compact


def redact_value(value: Any) -> Any:
    """Recursively redact secrets inside structured tool input.

    Dict values under sensitive key names are replaced wholesale; every
    string is additionally scanned for embedded token patterns and sensitive
    assignments. Non-string scalars pass through unchanged.
    """
    if isinstance(value, dict):
        return {
            k: (REDACTED if isinstance(k, str) and _is_sensitive_key(k) else redact_value(v))
            for k, v in value.items()
        }
    if isinstance(value, (list, tuple)):
        redacted = [redact_value(v) for v in value]
        return type(value)(redacted) if isinstance(value, tuple) else redacted
    if isinstance(value, str):
        return redact_text(value)
    return value


def redact_tool_input(tool_input: Any) -> Any:
    """Redact a hook tool_input payload before persistence. Never returns raw input."""
    if tool_input is None:
        return None
    return redact_value(tool_input)
