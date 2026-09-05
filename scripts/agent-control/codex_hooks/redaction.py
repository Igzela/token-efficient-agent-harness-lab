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

# Hooks-side supplementary patterns (extensions beyond the canonical scanner,
# pinned by hooks tests, for secret shapes observed in worker tool input):
SUPPLEMENTARY_PATTERNS: list[tuple[str, re.Pattern[str]]] = [
    # Authorization headers, e.g. "Authorization: Bearer <token>".
    (
        "authorization_header",
        re.compile(r"(?i)\b(authorization\s*:\s*(?:bearer\s+)?)([^\s;,]+)"),
    ),
]

# Prefixed env-style assignments the canonical \b-anchored pattern misses,
# e.g. OPENAI_API_KEY=sk-..., MY_TOKEN=.... The sensitive word must form a
# whole underscore-delimited segment so innocent names (e.g. "monkey") fail.
_ASSIGNMENT_SHAPE = re.compile(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(\"[^\"]*\"|'[^']*'|[^\s#;,]+)")
_SENSITIVE_NAME_SEGMENT = re.compile(
    r"(?i)(?:^|_)(?:API[_-]?KEY|AUTH[_-]?TOKEN|ACCESS[_-]?TOKEN|SECRET|PASSWORD|PASSWD|CREDENTIALS?|TOKEN|APIKEY|AUTHTOKEN|ACCESSTOKEN)(?:_|$)"
)


def _mask_prefixed_assignments(text: str) -> str:
    """Mask VAR=value assignments whose name carries a sensitive segment."""

    def repl(match: re.Match[str]) -> str:
        if _SENSITIVE_NAME_SEGMENT.search(match.group(1)):
            return f"{match.group(1)}={REDACTED}"
        return match.group(0)

    out = text
    for _ in range(4):
        masked = _ASSIGNMENT_SHAPE.sub(repl, out)
        if masked == out:
            break
        out = masked
    return out

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
    # Loop: a single pass masks only the first sensitive assignment.
    for _ in range(4):
        match = SENSITIVE_ASSIGNMENT.search(redacted)
        if not match:
            break
        redacted = f"{redacted[:match.start(2)]}{REDACTED}"
    for _name, pattern in SUPPLEMENTARY_PATTERNS:
        redacted = pattern.sub(lambda m: f"{m.group(1)}{REDACTED}", redacted)
    redacted = _mask_prefixed_assignments(redacted)
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
