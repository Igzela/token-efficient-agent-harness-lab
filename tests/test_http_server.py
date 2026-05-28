"""Tests for dispatch/http_server.py — stdlib HTTP server with per-server isolation."""

import json
import sys
import threading
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.http_server import (
    HTTP_SERVER_SCHEMA_VERSION,
    HarnessHTTPHandler,
    RouteMatch,
    ServerConfig,
    ServerContext,
    clear_routes,
    create_server,
    register_route,
    start_server_in_thread,
)
from harness_core.dispatch.auth import (
    Tenant,
    TenantResolver,
)


def _find_free_port() -> int:
    import socket
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def _start_server(server):
    t = threading.Thread(target=server.serve_forever, daemon=True)
    t.start()
    return t


class ServerConfigTests(unittest.TestCase):
    def test_defaults(self):
        cfg = ServerConfig()
        self.assertEqual(cfg.host, "127.0.0.1")
        self.assertEqual(cfg.port, 8080)
        self.assertEqual(cfg.api_prefix, "/api/v1")

    def test_custom(self):
        cfg = ServerConfig(host="0.0.0.0", port=9000, api_prefix="/v2")
        self.assertEqual(cfg.host, "0.0.0.0")
        self.assertEqual(cfg.port, 9000)

    def test_immutable(self):
        cfg = ServerConfig()
        with self.assertRaises(AttributeError):
            cfg.port = 9999  # type: ignore[misc]


class RouteMatchTests(unittest.TestCase):
    def test_fields(self):
        m = RouteMatch(method="GET", path="/plans", params={"id": "p1"})
        self.assertEqual(m.method, "GET")
        self.assertEqual(m.params["id"], "p1")

    def test_default_params(self):
        m = RouteMatch(method="GET", path="/plans")
        self.assertEqual(m.params, {})


class ServerContextTests(unittest.TestCase):
    def test_fields(self):
        cfg = ServerConfig(port=9000)
        ctx = ServerContext(config=cfg)
        self.assertEqual(ctx.config.port, 9000)
        self.assertEqual(ctx.routes, {})
        self.assertIsNone(ctx.store)

    def test_store(self):
        ctx = ServerContext(config=ServerConfig(), store={"db": True})
        self.assertEqual(ctx.store["db"], True)


class PathMatchingTests(unittest.TestCase):
    def test_exact_match_via_integration(self):
        server = create_server(ServerConfig(port=_find_free_port()))
        register_route("GET", "/plans", lambda m, b: {"matched": True}, server=server)
        _start_server(server)
        try:
            url = f"http://127.0.0.1:{server.server_address[1]}/api/v1/plans"
            with urllib.request.urlopen(url) as resp:
                data = json.loads(resp.read())
            self.assertTrue(data["matched"])
        finally:
            server.shutdown()
            server.server_close()

    def test_wildcard_match_via_integration(self):
        server = create_server(ServerConfig(port=_find_free_port()))
        register_route("GET", "/plans/{plan_id}",
                        lambda m, b: {"id": m.params.get("plan_id")}, server=server)
        _start_server(server)
        try:
            port = server.server_address[1]
            with urllib.request.urlopen(f"http://127.0.0.1:{port}/api/v1/plans/p123") as resp:
                data = json.loads(resp.read())
            self.assertEqual(data["id"], "p123")
        finally:
            server.shutdown()
            server.server_close()

    def test_no_match_via_integration(self):
        server = create_server(ServerConfig(port=_find_free_port()))
        _start_server(server)
        try:
            port = server.server_address[1]
            with self.assertRaises(urllib.error.HTTPError) as ctx:
                urllib.request.urlopen(f"http://127.0.0.1:{port}/api/v1/nonexistent")
            self.assertEqual(ctx.exception.code, 404)
        finally:
            server.shutdown()
            server.server_close()

    def test_method_mismatch_via_integration(self):
        server = create_server(ServerConfig(port=_find_free_port()))
        register_route("POST", "/plans", lambda m, b: {}, server=server)
        _start_server(server)
        try:
            port = server.server_address[1]
            with self.assertRaises(urllib.error.HTTPError) as ctx:
                urllib.request.urlopen(f"http://127.0.0.1:{port}/api/v1/plans")
            self.assertEqual(ctx.exception.code, 404)
        finally:
            server.shutdown()
            server.server_close()


class RegisterRouteTests(unittest.TestCase):
    def test_register_and_lookup(self):
        server = create_server(ServerConfig(port=_find_free_port()))
        register_route("GET", "/test", lambda m, b: {"ok": True}, server=server)
        self.assertIn(("GET", "/test"), server._harness_context.routes)
        server.server_close()

    def test_clear_routes(self):
        server = create_server(ServerConfig(port=_find_free_port()))
        register_route("GET", "/test", lambda m, b: {}, server=server)
        clear_routes(server=server)
        self.assertEqual(len(server._harness_context.routes), 0)
        server.server_close()

    def test_register_via_last_context_fallback(self):
        server = create_server(ServerConfig(port=_find_free_port()))
        register_route("GET", "/fallback", lambda m, b: {})
        self.assertIn(("GET", "/fallback"), server._harness_context.routes)
        server.server_close()


class HTTPIntegrationTests(unittest.TestCase):
    def test_get_returns_json(self):
        server = create_server(ServerConfig(port=_find_free_port()))
        register_route("GET", "/ping", lambda m, b: {"status": "ok"}, server=server)
        _start_server(server)
        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{server.server_address[1]}/api/v1/ping") as resp:
                data = json.loads(resp.read())
            self.assertEqual(data["status"], "ok")
            self.assertEqual(resp.status, 200)
        finally:
            server.shutdown()
            server.server_close()

    def test_post_with_body(self):
        received: dict = {}
        server = create_server(ServerConfig(port=_find_free_port()))
        register_route("POST", "/plans",
                        lambda m, b: (received.update({"body": b}), {"received": True})[1],
                        server=server)
        _start_server(server)
        try:
            url = f"http://127.0.0.1:{server.server_address[1]}/api/v1/plans"
            payload = json.dumps({"task": "build"}).encode()
            req = urllib.request.Request(url, data=payload, method="POST",
                                        headers={"Content-Type": "application/json"})
            with urllib.request.urlopen(req) as resp:
                data = json.loads(resp.read())
            self.assertTrue(data["received"])
            self.assertEqual(received["body"]["task"], "build")
        finally:
            server.shutdown()
            server.server_close()

    def test_404_for_unknown_route(self):
        server = create_server(ServerConfig(port=_find_free_port()))
        _start_server(server)
        try:
            with self.assertRaises(urllib.error.HTTPError) as ctx:
                urllib.request.urlopen(f"http://127.0.0.1:{server.server_address[1]}/api/v1/nonexistent")
            self.assertEqual(ctx.exception.code, 404)
        finally:
            server.shutdown()
            server.server_close()

    def test_custom_api_prefix(self):
        port = _find_free_port()
        server = create_server(ServerConfig(port=port, api_prefix="/v2"))
        register_route("GET", "/data", lambda m, b: {"prefix": "v2"}, server=server)
        _start_server(server)
        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{port}/v2/data") as resp:
                data = json.loads(resp.read())
            self.assertEqual(data["prefix"], "v2")
        finally:
            server.shutdown()
            server.server_close()

    def test_handler_exception_returns_500_generic_message(self):
        server = create_server(ServerConfig(port=_find_free_port()))
        register_route("GET", "/error",
                        lambda m, b: (_ for _ in ()).throw(RuntimeError("secret details")),
                        server=server)
        _start_server(server)
        try:
            with self.assertRaises(urllib.error.HTTPError) as ctx:
                urllib.request.urlopen(f"http://127.0.0.1:{server.server_address[1]}/api/v1/error")
            self.assertEqual(ctx.exception.code, 500)
            body = json.loads(ctx.exception.read())
            self.assertEqual(body["error"], "internal server error")
            self.assertNotIn("secret details", body["error"])
        finally:
            server.shutdown()
            server.server_close()

    def test_path_params_extraction(self):
        server = create_server(ServerConfig(port=_find_free_port()))
        register_route("GET", "/repos/{repo_id}",
                        lambda m, b: {"repo_id": m.params.get("repo_id")}, server=server)
        _start_server(server)
        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{server.server_address[1]}/api/v1/repos/my-repo") as resp:
                data = json.loads(resp.read())
            self.assertEqual(data["repo_id"], "my-repo")
        finally:
            server.shutdown()
            server.server_close()


class StartServerInThreadTests(unittest.TestCase):
    def test_starts_and_stops(self):
        port = _find_free_port()
        server, thread = start_server_in_thread(ServerConfig(port=port))
        register_route("GET", "/health", lambda m, b: {"alive": True}, server=server)
        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{port}/api/v1/health") as resp:
                data = json.loads(resp.read())
            self.assertTrue(data["alive"])
        finally:
            server.shutdown()
            thread.join(timeout=2)

    def test_daemon_thread(self):
        port = _find_free_port()
        server, thread = start_server_in_thread(ServerConfig(port=port))
        self.assertTrue(thread.daemon)
        server.shutdown()


class ServerIsolationTests(unittest.TestCase):
    """Phase 6B-1: verify two servers in one process are fully isolated."""

    def test_two_servers_have_independent_routes(self):
        server_a = create_server(ServerConfig(port=_find_free_port()))
        server_b = create_server(ServerConfig(port=_find_free_port()))

        register_route("GET", "/only-a", lambda m, b: {"from": "a"}, server=server_a)
        register_route("GET", "/only-b", lambda m, b: {"from": "b"}, server=server_b)

        ctx_a = server_a._harness_context
        ctx_b = server_b._harness_context

        self.assertIn(("GET", "/only-a"), ctx_a.routes)
        self.assertNotIn(("GET", "/only-a"), ctx_b.routes)
        self.assertIn(("GET", "/only-b"), ctx_b.routes)
        self.assertNotIn(("GET", "/only-b"), ctx_a.routes)

        server_a.server_close()
        server_b.server_close()

    def test_different_api_prefix(self):
        port_a = _find_free_port()
        port_b = _find_free_port()
        server_a = create_server(ServerConfig(port=port_a, api_prefix="/api/v1"))
        server_b = create_server(ServerConfig(port=port_b, api_prefix="/v2"))

        register_route("GET", "/data", lambda m, b: {"prefix": "v1"}, server=server_a)
        register_route("GET", "/data", lambda m, b: {"prefix": "v2"}, server=server_b)

        _start_server(server_a)
        _start_server(server_b)

        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{port_a}/api/v1/data") as resp:
                self.assertEqual(json.loads(resp.read())["prefix"], "v1")
            with urllib.request.urlopen(f"http://127.0.0.1:{port_b}/v2/data") as resp:
                self.assertEqual(json.loads(resp.read())["prefix"], "v2")
        finally:
            server_a.shutdown()
            server_b.shutdown()

    def test_different_stores(self):
        store_a = {"name": "store_a"}
        store_b = {"name": "store_b"}
        server_a = create_server(ServerConfig(port=_find_free_port()), store=store_a)
        server_b = create_server(ServerConfig(port=_find_free_port()), store=store_b)

        self.assertIs(server_a._harness_context.store, store_a)
        self.assertIs(server_b._harness_context.store, store_b)

        server_a.server_close()
        server_b.server_close()

    def test_clear_routes_isolation(self):
        server_a = create_server(ServerConfig(port=_find_free_port()))
        server_b = create_server(ServerConfig(port=_find_free_port()))

        register_route("GET", "/shared-path", lambda m, b: {}, server=server_a)
        register_route("GET", "/shared-path", lambda m, b: {}, server=server_b)

        clear_routes(server=server_a)

        self.assertEqual(len(server_a._harness_context.routes), 0)
        self.assertEqual(len(server_b._harness_context.routes), 1)

        server_a.server_close()
        server_b.server_close()


class SchemaVersionTests(unittest.TestCase):
    def test_schema_version_defined(self):
        self.assertEqual(HTTP_SERVER_SCHEMA_VERSION, "http_server.v1")


class AuthIntegrationTests(unittest.TestCase):
    """Phase 6B-2: auth middleware integration with HTTP server."""

    def _make_auth_server(self):
        from harness_core.dispatch.auth import Tenant, TenantResolver
        resolver = TenantResolver()
        resolver.add_tenant(Tenant(tenant_id="t1", name="Test"))
        _, raw_key = resolver.create_api_key("t1", scopes=frozenset({"read"}))
        server = create_server(
            ServerConfig(port=_find_free_port()),
            tenant_resolver=resolver,
        )
        return server, raw_key

    def test_unauthenticated_request_returns_401(self):
        server = create_server(
            ServerConfig(port=_find_free_port()),
            tenant_resolver=TenantResolver(),
        )
        register_route("GET", "/plans", lambda m, b: {"ok": True}, server=server)
        _start_server(server)
        try:
            with self.assertRaises(urllib.error.HTTPError) as ctx:
                urllib.request.urlopen(f"http://127.0.0.1:{server.server_address[1]}/api/v1/plans")
            self.assertEqual(ctx.exception.code, 401)
        finally:
            server.shutdown()
            server.server_close()

    def test_valid_key_allows_request(self):
        server, raw_key = self._make_auth_server()
        register_route("GET", "/plans", lambda m, b: {"ok": True}, server=server)
        _start_server(server)
        try:
            url = f"http://127.0.0.1:{server.server_address[1]}/api/v1/plans"
            req = urllib.request.Request(url, headers={"Authorization": f"Bearer {raw_key}"})
            with urllib.request.urlopen(req) as resp:
                data = json.loads(resp.read())
            self.assertTrue(data["ok"])
        finally:
            server.shutdown()
            server.server_close()

    def test_invalid_key_returns_401(self):
        server, _ = self._make_auth_server()
        register_route("GET", "/plans", lambda m, b: {"ok": True}, server=server)
        _start_server(server)
        try:
            url = f"http://127.0.0.1:{server.server_address[1]}/api/v1/plans"
            req = urllib.request.Request(url, headers={"Authorization": "Bearer harness_badkey"})
            with self.assertRaises(urllib.error.HTTPError) as ctx:
                urllib.request.urlopen(req)
            self.assertEqual(ctx.exception.code, 401)
        finally:
            server.shutdown()
            server.server_close()

    def test_no_auth_header_returns_401(self):
        server, _ = self._make_auth_server()
        register_route("GET", "/plans", lambda m, b: {"ok": True}, server=server)
        _start_server(server)
        try:
            with self.assertRaises(urllib.error.HTTPError) as ctx:
                urllib.request.urlopen(f"http://127.0.0.1:{server.server_address[1]}/api/v1/plans")
            self.assertEqual(ctx.exception.code, 401)
        finally:
            server.shutdown()
            server.server_close()

    def test_no_tenant_resolver_skips_auth(self):
        server = create_server(ServerConfig(port=_find_free_port()))
        register_route("GET", "/open", lambda m, b: {"open": True}, server=server)
        _start_server(server)
        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{server.server_address[1]}/api/v1/open") as resp:
                data = json.loads(resp.read())
            self.assertTrue(data["open"])
        finally:
            server.shutdown()
            server.server_close()

    def test_two_servers_different_auth(self):
        resolver_a = TenantResolver()
        resolver_a.add_tenant(Tenant(tenant_id="a", name="A"))
        _, key_a = resolver_a.create_api_key("a")

        resolver_b = TenantResolver()
        resolver_b.add_tenant(Tenant(tenant_id="b", name="B"))
        _, key_b = resolver_b.create_api_key("b")

        server_a = create_server(ServerConfig(port=_find_free_port()), tenant_resolver=resolver_a)
        server_b = create_server(ServerConfig(port=_find_free_port()), tenant_resolver=resolver_b)

        register_route("GET", "/data", lambda m, b: {"server": m.params.get("x", "a")}, server=server_a)
        register_route("GET", "/data", lambda m, b: {"server": "b"}, server=server_b)

        _start_server(server_a)
        _start_server(server_b)

        try:
            port_a = server_a.server_address[1]
            port_b = server_b.server_address[1]

            req_a = urllib.request.Request(
                f"http://127.0.0.1:{port_a}/api/v1/data",
                headers={"Authorization": f"Bearer {key_a}"},
            )
            with urllib.request.urlopen(req_a) as resp:
                self.assertEqual(json.loads(resp.read())["server"], "a")

            req_b = urllib.request.Request(
                f"http://127.0.0.1:{port_b}/api/v1/data",
                headers={"Authorization": f"Bearer {key_b}"},
            )
            with urllib.request.urlopen(req_b) as resp:
                self.assertEqual(json.loads(resp.read()), {"server": "b"})

            # key_a should be rejected on server_b
            req_wrong = urllib.request.Request(
                f"http://127.0.0.1:{port_b}/api/v1/data",
                headers={"Authorization": f"Bearer {key_a}"},
            )
            with self.assertRaises(urllib.error.HTTPError) as ctx:
                urllib.request.urlopen(req_wrong)
            self.assertEqual(ctx.exception.code, 401)
        finally:
            server_a.shutdown()
            server_b.shutdown()


if __name__ == "__main__":
    unittest.main()
