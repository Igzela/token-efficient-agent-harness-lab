"""Request canonicalization and envelope/marker protocol (pure logic).

The visible `Review-Request-SHA256:` marker makes a delivered request
recognizable in the thread so a resend can be refused without guessing.
Canonicalization is deterministic: NFC-normalized, LF-only, no BOM.
"""

from __future__ import annotations

import hashlib
import re
import unicodedata

MARKER_PREFIX = "Review-Request-SHA256:"
MARKER_RE = re.compile(r"Review-Request-SHA256:\s*([0-9a-f]{64})")


def canonicalize(text: str) -> str:
    """Deterministic request text: NFC-normalized, LF-only, no BOM."""
    text = text.replace("\r\n", "\n").replace("\r", "\n")
    text = text.replace("\ufeff", "")
    return unicodedata.normalize("NFC", text)


def request_sha256(request_text: str) -> str:
    return hashlib.sha256(canonicalize(request_text).encode("utf-8")).hexdigest()


def marker_line(sha: str) -> str:
    return f"{MARKER_PREFIX} {sha}"


def build_message(request_text: str) -> tuple[str, str]:
    """Envelope: marker first, then blank line, then canonical request text."""
    body = canonicalize(request_text).strip() + "\n"
    sha = request_sha256(body)
    return f"{marker_line(sha)}\n\n{body}", sha


def extract_marker(text: str | None) -> str | None:
    match = MARKER_RE.search(text or "")
    return match.group(1) if match else None


def assert_canonical(text: str) -> None:
    """Raise if text is not already canonical (for strict delivery checks)."""
    if text != canonicalize(text):
        raise ValueError("request text is not canonical (NFC/LF/no-BOM)")
