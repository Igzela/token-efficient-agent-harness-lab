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


def canonical_bytes(request_text: str) -> bytes:
    """The single immutable canonical byte sequence for this request.

    The envelope hash, the thread marker, and the delivered message are all
    derived from these exact bytes; no later strip/rejoin is allowed.
    """
    return canonicalize(request_text).strip().encode("utf-8") + b"\n"


def request_sha256(request_text: str) -> str:
    return hashlib.sha256(canonical_bytes(request_text)).hexdigest()


def marker_line(sha: str) -> str:
    return f"{MARKER_PREFIX} {sha}"


def build_message(request_text: str) -> tuple[str, str]:
    """Envelope: marker first, then blank line, then the canonical bytes.

    The SHA is derived from the same canonical bytes that are delivered, so
    the envelope hash, the marker, and the thread body can never diverge.
    """
    body = canonical_bytes(request_text).decode("utf-8")
    sha = request_sha256(request_text)
    return f"{marker_line(sha)}\n\n{body}", sha


def canonical_body(request_text: str) -> str:
    """The exact delivered body text (marker-excluded)."""
    return canonical_bytes(request_text).decode("utf-8")


def extract_marker(text: str | None) -> str | None:
    match = MARKER_RE.search(text or "")
    return match.group(1) if match else None


def assert_canonical(text: str) -> None:
    """Raise if text is not already canonical (for strict delivery checks)."""
    if text != canonicalize(text):
        raise ValueError("request text is not canonical (NFC/LF/no-BOM)")
