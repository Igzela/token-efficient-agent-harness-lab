"""CredentialBoundary — secret resolution, validation, and redaction display."""

from __future__ import annotations

import os
from typing import Any

from .provider_config import CredentialRef


class CredentialBoundary:
    """Manages credential resolution from environment variables.

    Phase 3 scope: env backend only. File/keyring/vault deferred.
    """

    def __init__(self, backend: str = "env") -> None:
        if backend != "env":
            raise ValueError(f"Only 'env' backend supported in Phase 3, got '{backend}'")
        self._backend = backend

    def resolve(self, ref: CredentialRef) -> str:
        """Read credential value from environment variable."""
        if ref.storage_backend != "env":
            raise ValueError(
                f"Credential {ref.credential_ref_id} uses backend '{ref.storage_backend}', "
                f"only 'env' is supported in Phase 3"
            )
        value = os.environ.get(ref.credential_ref_id)
        if value is None:
            raise ValueError(
                f"Credential environment variable '{ref.credential_ref_id}' is not set"
            )
        return value

    def validate(self, ref: CredentialRef) -> bool:
        """Check if credential is available without raising."""
        try:
            self.resolve(ref)
            return True
        except ValueError:
            return False

    @staticmethod
    def redact_display(secret: str) -> str:
        """Generate a redacted display string like 'sk-***abc'."""
        if len(secret) <= 4:
            return "***"
        prefix = secret[:3]
        suffix = secret[-3:]
        return f"{prefix}***{suffix}"
