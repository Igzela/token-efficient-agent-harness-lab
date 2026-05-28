"""Phase 6B-2: Local auth — API key hashing, tenant resolution, request context."""

from __future__ import annotations

import hashlib
import hmac
import secrets
import time
from dataclasses import dataclass, field


AUTH_SCHEMA_VERSION = "auth.v1"


@dataclass(frozen=True)
class RequestContext:
    tenant_id: str
    api_key_id: str
    scopes: frozenset[str]
    request_id: str


@dataclass(frozen=True)
class Tenant:
    tenant_id: str
    name: str
    scopes: frozenset[str] = field(default_factory=frozenset)
    rate_limit: int | None = None


@dataclass(frozen=True)
class APIKey:
    key_id: str
    tenant_id: str
    key_hash: str
    key_salt: str
    scopes: frozenset[str] = field(default_factory=frozenset)
    created_at: float = field(default_factory=time.time)
    expires_at: float | None = None


@dataclass(frozen=True)
class AuthDecision:
    allowed: bool
    tenant_id: str | None = None
    api_key_id: str | None = None
    scopes: frozenset[str] = field(default_factory=frozenset)
    reason: str = ""


_API_KEY_PREFIX = "harness_"
_API_KEY_SUFFIX_LEN = 64  # 32 bytes hex = 64 chars


def generate_api_key() -> str:
    return f"{_API_KEY_PREFIX}{secrets.token_hex(32)}"


def generate_salt() -> str:
    return secrets.token_hex(16)


def hash_api_key(raw_key: str, salt: str) -> str:
    return hashlib.sha256((salt + raw_key).encode()).hexdigest()


def _validate_token_shape(token: str) -> bool:
    if not token.startswith(_API_KEY_PREFIX):
        return False
    suffix = token[len(_API_KEY_PREFIX):]
    if len(suffix) != _API_KEY_SUFFIX_LEN:
        return False
    try:
        int(suffix, 16)
    except ValueError:
        return False
    return True


class TenantResolver:
    def __init__(self) -> None:
        self._api_keys: dict[str, APIKey] = {}
        self._tenants: dict[str, Tenant] = {}

    def add_tenant(self, tenant: Tenant) -> None:
        self._tenants[tenant.tenant_id] = tenant

    def add_api_key(self, key: APIKey) -> None:
        self._api_keys[key.key_id] = key

    def create_api_key(
        self,
        tenant_id: str,
        scopes: frozenset[str] | None = None,
        expires_at: float | None = None,
    ) -> tuple[APIKey, str]:
        if tenant_id not in self._tenants:
            raise ValueError(f"unknown tenant: {tenant_id}")
        raw_key = generate_api_key()
        salt = generate_salt()
        key_hash = hash_api_key(raw_key, salt)
        tenant = self._tenants[tenant_id]
        key_scopes = scopes if scopes is not None else tenant.scopes
        if tenant.scopes and not key_scopes.issubset(tenant.scopes):
            raise ValueError(
                f"key scopes {key_scopes} exceed tenant scopes {tenant.scopes}"
            )
        key = APIKey(
            key_id=f"key_{secrets.token_hex(8)}",
            tenant_id=tenant_id,
            key_hash=key_hash,
            key_salt=salt,
            scopes=key_scopes,
            expires_at=expires_at,
        )
        self._api_keys[key.key_id] = key
        return key, raw_key

    def resolve(self, auth_header: str | None) -> AuthDecision:
        if not auth_header:
            return AuthDecision(allowed=False, reason="missing authorization header")
        parts = auth_header.split(" ", 1)
        if len(parts) != 2 or parts[0].lower() != "bearer":
            return AuthDecision(allowed=False, reason="invalid authorization format")
        raw_token = parts[1]
        if not _validate_token_shape(raw_token):
            return AuthDecision(allowed=False, reason="invalid api key")
        matched_key: APIKey | None = None
        for key in self._api_keys.values():
            if hmac.compare_digest(
                hash_api_key(raw_token, key.key_salt), key.key_hash
            ):
                matched_key = key
                break
        if matched_key is None:
            return AuthDecision(allowed=False, reason="invalid api key")
        if matched_key.expires_at is not None and time.time() > matched_key.expires_at:
            return AuthDecision(allowed=False, reason="api key expired")
        tenant = self._tenants.get(matched_key.tenant_id)
        if tenant is None:
            return AuthDecision(allowed=False, reason="unknown tenant")
        return AuthDecision(
            allowed=True,
            tenant_id=matched_key.tenant_id,
            api_key_id=matched_key.key_id,
            scopes=matched_key.scopes,
        )
