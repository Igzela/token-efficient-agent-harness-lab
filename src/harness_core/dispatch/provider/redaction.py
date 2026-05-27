"""Secret redaction pipeline — strips secrets from text and audit fields."""

from __future__ import annotations

from typing import Any

from .credential_boundary import CredentialBoundary
from .provider_config import CredentialRef


def redact_secrets(
    text: str,
    refs: list[CredentialRef],
    boundary: CredentialBoundary,
) -> str:
    """Replace known secret values with *** in text."""
    result = text
    for ref in refs:
        try:
            secret = boundary.resolve(ref)
            if secret and secret in result:
                result = result.replace(secret, "***")
        except ValueError:
            pass
    return result


def redact_audit_fields(data: dict[str, Any]) -> dict[str, Any]:
    """Recursively redact fields that look like secrets in audit data."""
    sensitive_keys = {"api_key", "secret", "token", "password", "credential", "private_key", "access_key", "auth_token"}
    result: dict[str, Any] = {}
    for key, value in data.items():
        if key.lower() in sensitive_keys:
            result[key] = "***"
        elif isinstance(value, dict):
            result[key] = redact_audit_fields(value)
        elif isinstance(value, list):
            result[key] = [
                redact_audit_fields(item) if isinstance(item, dict) else item
                for item in value
            ]
        else:
            result[key] = value
    return result
