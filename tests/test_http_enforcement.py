"""Tests for Phase 6B-3 enforcement: scope checks, rate limiting, 403/429."""

from __future__ import annotations

import json
import unittest
from http.server import HTTPServer
from typing import Any
from unittest.mock import MagicMock

from harness_core.dispatch.auth import (
    AuthorizationDecision,
    RequestContext,
    Tenant,
    TenantResolver,
)
from harness_core.dispatch.http_server import (
    RouteMatch,
    ServerConfig,
    ServerContext,
    create_server,
    register_route,
    clear_routes,
)
from harness_core.dispatch.rate_limiter import RateLimiter


def _dummy_handler(match: RouteMatch, body: dict[str, Any] | None) -> dict[str, Any]:
    return {"ok": True, "route_pattern": match.route_pattern}


def _make_server(
    tenant_resolver: Any = None,
    rate_limiter: Any = None,
) -> tuple[HTTPServer, ServerContext]:
    config = ServerConfig(host="127.0.0.1", port=0)
    server = create_server(config, tenant_resolver=tenant_resolver,
                           rate_limiter=rate_limiter)
    ctx = server._harness_context
    return server, ctx


class TestAuthorizationDecision(unittest.TestCase):
    def test_allowed(self) -> None:
        d = AuthorizationDecision(allowed=True)
        self.assertTrue(d.allowed)

    def test_denied_with_reason(self) -> None:
        d = AuthorizationDecision(allowed=False, reason="missing scopes: dispatch:write")
        self.assertFalse(d.allowed)
        self.assertIn("dispatch:write", d.reason)

    def test_scopes(self) -> None:
        d = AuthorizationDecision(
            allowed=True,
            required_scopes=frozenset({"dispatch:read"}),
            granted_scopes=frozenset({"dispatch:read", "dispatch:write"}),
        )
        self.assertEqual(d.required_scopes, frozenset({"dispatch:read"}))


class TestRouteMatch(unittest.TestCase):
    def test_route_pattern_field(self) -> None:
        m = RouteMatch(method="GET", path="/api/v1/plans", route_pattern="/plans")
        self.assertEqual(m.route_pattern, "/plans")

    def test_default_route_pattern_empty(self) -> None:
        m = RouteMatch(method="GET", path="/api/v1/plans")
        self.assertEqual(m.route_pattern, "")


class TestScopeEnforcement(unittest.TestCase):
    def test_no_scopes_required_passes(self) -> None:
        server, ctx = _make_server()
        register_route("GET", "/open", _dummy_handler, server=server)
        handler, params, pattern = server._harness_context.routes[("GET", "/open")], {}, "/open"
        # No scopes configured → should pass
        self.assertEqual(ctx.route_scopes.get(("GET", "/open")), None)
        server.server_close()

    def test_scopes_required_no_context_denies(self) -> None:
        server, ctx = _make_server()
        register_route("GET", "/protected", _dummy_handler,
                       required_scopes=frozenset({"dispatch:read"}),
                       server=server)
        # Create a handler instance to test _check_scopes
        from harness_core.dispatch.http_server import HarnessHTTPHandler
        handler_instance = HarnessHTTPHandler.__new__(HarnessHTTPHandler)
        handler_instance.server = server
        # No request_context → should deny
        decision = handler_instance._check_scopes(None, ("GET", "/protected"))
        self.assertFalse(decision.allowed)
        self.assertIn("authentication required", decision.reason)
        server.server_close()

    def test_scopes_required_inadequate_scopes_denies(self) -> None:
        server, ctx = _make_server()
        register_route("POST", "/write", _dummy_handler,
                       required_scopes=frozenset({"dispatch:write"}),
                       server=server)
        from harness_core.dispatch.http_server import HarnessHTTPHandler
        handler_instance = HarnessHTTPHandler.__new__(HarnessHTTPHandler)
        handler_instance.server = server
        ctx = server._harness_context
        ctx.route_scopes[("POST", "/write")] = frozenset({"dispatch:write"})
        request_ctx = RequestContext(
            tenant_id="t1",
            api_key_id="k1",
            scopes=frozenset({"dispatch:read"}),
            request_id="req-1",
        )
        decision = handler_instance._check_scopes(request_ctx, ("POST", "/write"))
        self.assertFalse(decision.allowed)
        self.assertIn("dispatch:write", decision.reason)
        server.server_close()

    def test_scopes_required_adequate_scopes_passes(self) -> None:
        server, ctx = _make_server()
        register_route("POST", "/write", _dummy_handler,
                       required_scopes=frozenset({"dispatch:write"}),
                       server=server)
        from harness_core.dispatch.http_server import HarnessHTTPHandler
        handler_instance = HarnessHTTPHandler.__new__(HarnessHTTPHandler)
        handler_instance.server = server
        ctx = server._harness_context
        ctx.route_scopes[("POST", "/write")] = frozenset({"dispatch:write"})
        request_ctx = RequestContext(
            tenant_id="t1",
            api_key_id="k1",
            scopes=frozenset({"dispatch:read", "dispatch:write"}),
            request_id="req-1",
        )
        decision = handler_instance._check_scopes(request_ctx, ("POST", "/write"))
        self.assertTrue(decision.allowed)
        server.server_close()


class TestRateLimitEnforcement(unittest.TestCase):
    def test_no_rate_limiter_passes(self) -> None:
        server, ctx = _make_server()
        from harness_core.dispatch.http_server import HarnessHTTPHandler
        handler_instance = HarnessHTTPHandler.__new__(HarnessHTTPHandler)
        handler_instance.server = server
        request_ctx = RequestContext(
            tenant_id="t1", api_key_id="k1",
            scopes=frozenset(), request_id="req-1",
        )
        allowed, retry = handler_instance._check_rate_limit(request_ctx)
        self.assertTrue(allowed)
        self.assertIsNone(retry)
        server.server_close()

    def test_rate_limiter_blocks_after_limit(self) -> None:
        limiter = RateLimiter(window_seconds=60)
        server, ctx = _make_server(rate_limiter=limiter)
        from harness_core.dispatch.http_server import HarnessHTTPHandler
        handler_instance = HarnessHTTPHandler.__new__(HarnessHTTPHandler)
        handler_instance.server = server
        request_ctx = RequestContext(
            tenant_id="t1", api_key_id="k1",
            scopes=frozenset(), request_id="req-1",
        )
        # Use up the limit
        for _ in range(60):
            limiter.check("t1", "k1", rate_limit=60)
        allowed, retry = handler_instance._check_rate_limit(request_ctx)
        self.assertFalse(allowed)
        self.assertIsNotNone(retry)
        server.server_close()

    def test_rate_limiter_different_tenants_independent(self) -> None:
        limiter = RateLimiter(window_seconds=60)
        server, ctx = _make_server(rate_limiter=limiter)
        from harness_core.dispatch.http_server import HarnessHTTPHandler
        handler_instance = HarnessHTTPHandler.__new__(HarnessHTTPHandler)
        handler_instance.server = server

        # Exhaust tenant-a
        for _ in range(60):
            limiter.check("t-a", "k1", rate_limit=60)
        ctx_a = RequestContext(tenant_id="t-a", api_key_id="k1",
                              scopes=frozenset(), request_id="r1")
        allowed_a, _ = handler_instance._check_rate_limit(ctx_a)
        self.assertFalse(allowed_a)

        # Tenant-b should still be allowed
        ctx_b = RequestContext(tenant_id="t-b", api_key_id="k2",
                              scopes=frozenset(), request_id="r2")
        allowed_b, _ = handler_instance._check_rate_limit(ctx_b)
        self.assertTrue(allowed_b)
        server.server_close()


class TestClearRoutes(unittest.TestCase):
    def test_clear_routes_removes_scopes(self) -> None:
        server, ctx = _make_server()
        register_route("GET", "/test", _dummy_handler,
                       required_scopes=frozenset({"dispatch:read"}),
                       server=server)
        self.assertIn(("GET", "/test"), ctx.route_scopes)
        clear_routes(server)
        self.assertNotIn(("GET", "/test"), ctx.route_scopes)
        self.assertEqual(len(ctx.routes), 0)
        server.server_close()


if __name__ == "__main__":
    unittest.main()
