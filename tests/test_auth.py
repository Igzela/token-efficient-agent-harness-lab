"""Tests for dispatch/auth.py — local API key hashing, tenant resolution, request context."""

import sys
import time
import uuid
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.auth import (
    AUTH_SCHEMA_VERSION,
    APIKey,
    AuthDecision,
    RequestContext,
    Tenant,
    TenantResolver,
    generate_api_key,
    generate_salt,
    hash_api_key,
)


class SchemaVersionTests(unittest.TestCase):
    def test_auth_schema_version(self):
        self.assertEqual(AUTH_SCHEMA_VERSION, "auth.v1")


class GenerateApiKeyTests(unittest.TestCase):
    def test_returns_prefixed_string(self):
        key = generate_api_key()
        self.assertTrue(key.startswith("harness_"))

    def test_unique(self):
        keys = {generate_api_key() for _ in range(100)}
        self.assertEqual(len(keys), 100)


class GenerateSaltTests(unittest.TestCase):
    def test_returns_hex_string(self):
        salt = generate_salt()
        self.assertEqual(len(salt), 32)
        int(salt, 16)  # should not raise

    def test_unique(self):
        salts = {generate_salt() for _ in range(100)}
        self.assertEqual(len(salts), 100)


class HashApiKeyTests(unittest.TestCase):
    def test_deterministic(self):
        h1 = hash_api_key("key", "salt")
        h2 = hash_api_key("key", "salt")
        self.assertEqual(h1, h2)

    def test_different_salt_different_hash(self):
        h1 = hash_api_key("key", "salt1")
        h2 = hash_api_key("key", "salt2")
        self.assertNotEqual(h1, h2)

    def test_different_key_different_hash(self):
        h1 = hash_api_key("key1", "salt")
        h2 = hash_api_key("key2", "salt")
        self.assertNotEqual(h1, h2)

    def test_sha256_output_length(self):
        h = hash_api_key("test", "test")
        self.assertEqual(len(h), 64)


class RequestContextTests(unittest.TestCase):
    def test_fields(self):
        ctx = RequestContext(
            tenant_id="t1",
            api_key_id="k1",
            scopes=frozenset({"read"}),
            request_id="r1",
        )
        self.assertEqual(ctx.tenant_id, "t1")
        self.assertEqual(ctx.api_key_id, "k1")
        self.assertEqual(ctx.scopes, frozenset({"read"}))

    def test_immutable(self):
        ctx = RequestContext(
            tenant_id="t1",
            api_key_id="k1",
            scopes=frozenset(),
            request_id="r1",
        )
        with self.assertRaises(AttributeError):
            ctx.tenant_id = "t2"  # type: ignore[misc]


class TenantTests(unittest.TestCase):
    def test_defaults(self):
        t = Tenant(tenant_id="t1", name="Test")
        self.assertEqual(t.scopes, frozenset())
        self.assertIsNone(t.rate_limit)

    def test_custom_scopes(self):
        t = Tenant(tenant_id="t1", name="T", scopes=frozenset({"admin"}))
        self.assertEqual(t.scopes, frozenset({"admin"}))


class APIKeyTests(unittest.TestCase):
    def test_fields(self):
        k = APIKey(
            key_id="k1",
            tenant_id="t1",
            key_hash="abc",
            key_salt="def",
            scopes=frozenset({"read"}),
            created_at=1.0,
            expires_at=2.0,
        )
        self.assertEqual(k.key_id, "k1")
        self.assertEqual(k.key_salt, "def")

    def test_defaults(self):
        k = APIKey(key_id="k1", tenant_id="t1", key_hash="h", key_salt="s")
        self.assertEqual(k.scopes, frozenset())
        self.assertIsNone(k.expires_at)
        self.assertGreater(k.created_at, 0)


class AuthDecisionTests(unittest.TestCase):
    def test_denied_by_default(self):
        d = AuthDecision(allowed=False, reason="nope")
        self.assertFalse(d.allowed)
        self.assertIsNone(d.tenant_id)

    def test_allowed(self):
        d = AuthDecision(
            allowed=True, tenant_id="t1", api_key_id="k1", scopes=frozenset({"r"})
        )
        self.assertTrue(d.allowed)
        self.assertEqual(d.tenant_id, "t1")


class TenantResolverTests(unittest.TestCase):
    def _make_resolver(self) -> tuple[TenantResolver, Tenant, str]:
        resolver = TenantResolver()
        tenant = Tenant(tenant_id="t1", name="Test Tenant", scopes=frozenset({"read"}))
        resolver.add_tenant(tenant)
        _, raw_key = resolver.create_api_key("t1")
        return resolver, tenant, raw_key

    def test_create_and_resolve(self):
        resolver, _, raw_key = self._make_resolver()
        decision = resolver.resolve(f"Bearer {raw_key}")
        self.assertTrue(decision.allowed)
        self.assertEqual(decision.tenant_id, "t1")

    def test_resolve_none_header(self):
        resolver = TenantResolver()
        d = resolver.resolve(None)
        self.assertFalse(d.allowed)
        self.assertEqual(d.reason, "missing authorization header")

    def test_resolve_empty_header(self):
        resolver = TenantResolver()
        d = resolver.resolve("")
        self.assertFalse(d.allowed)

    def test_resolve_invalid_format(self):
        resolver = TenantResolver()
        d = resolver.resolve("Basic abc123")
        self.assertFalse(d.allowed)
        self.assertEqual(d.reason, "invalid authorization format")

    def test_resolve_wrong_key(self):
        resolver = TenantResolver()
        resolver.add_tenant(Tenant(tenant_id="t1", name="T"))
        d = resolver.resolve("Bearer harness_wrongkey")
        self.assertFalse(d.allowed)
        self.assertEqual(d.reason, "invalid api key")

    def test_resolve_expired_key(self):
        resolver = TenantResolver()
        resolver.add_tenant(Tenant(tenant_id="t1", name="T"))
        _, raw_key = resolver.create_api_key("t1", expires_at=time.time() - 1)
        d = resolver.resolve(f"Bearer {raw_key}")
        self.assertFalse(d.allowed)
        self.assertEqual(d.reason, "api key expired")

    def test_resolve_unknown_tenant(self):
        resolver = TenantResolver()
        # Add key with nonexistent tenant directly
        k = APIKey(
            key_id="k1",
            tenant_id="missing",
            key_hash="x",
            key_salt="x",
        )
        resolver.add_api_key(k)
        d = resolver.resolve(None)
        self.assertFalse(d.allowed)

    def test_create_api_key_unknown_tenant(self):
        resolver = TenantResolver()
        with self.assertRaises(ValueError):
            resolver.create_api_key("nonexistent")

    def test_scopes_propagated(self):
        resolver = TenantResolver()
        resolver.add_tenant(Tenant(tenant_id="t1", name="T"))
        _, raw_key = resolver.create_api_key("t1", scopes=frozenset({"write"}))
        d = resolver.resolve(f"Bearer {raw_key}")
        self.assertEqual(d.scopes, frozenset({"write"}))

    def test_tenant_scopes_used_when_no_key_scopes(self):
        resolver = TenantResolver()
        resolver.add_tenant(Tenant(tenant_id="t1", name="T", scopes=frozenset({"admin"})))
        _, raw_key = resolver.create_api_key("t1")
        d = resolver.resolve(f"Bearer {raw_key}")
        self.assertEqual(d.scopes, frozenset({"admin"}))

    def test_multiple_tenants_independent(self):
        resolver = TenantResolver()
        resolver.add_tenant(Tenant(tenant_id="t1", name="T1"))
        resolver.add_tenant(Tenant(tenant_id="t2", name="T2"))
        _, raw1 = resolver.create_api_key("t1")
        _, raw2 = resolver.create_api_key("t2")
        d1 = resolver.resolve(f"Bearer {raw1}")
        d2 = resolver.resolve(f"Bearer {raw2}")
        self.assertEqual(d1.tenant_id, "t1")
        self.assertEqual(d2.tenant_id, "t2")

    def test_key_id_populated(self):
        resolver = TenantResolver()
        resolver.add_tenant(Tenant(tenant_id="t1", name="T"))
        key, raw_key = resolver.create_api_key("t1")
        d = resolver.resolve(f"Bearer {raw_key}")
        self.assertEqual(d.api_key_id, key.key_id)

    def test_unauthorized_has_empty_key_id(self):
        resolver = TenantResolver()
        d = resolver.resolve(None)
        self.assertIsNone(d.api_key_id)


if __name__ == "__main__":
    unittest.main()
