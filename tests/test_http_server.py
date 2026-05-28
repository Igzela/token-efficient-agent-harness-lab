"""Tests for dispatch/http_server.py — stdlib HTTP server."""

import json
import sys
import threading
import urllib.request
from http.server import HTTPServer
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.http_server import (
    HTTP_SERVER_SCHEMA_VERSION,
    HarnessHTTPHandler,
    RouteMatch,
    ServerConfig,
    clear_routes,
    create_server,
    register_route,
    start_server_in_thread,
)


def _find_free_port() -> int:
    import socket
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


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


class PathMatchingTests(unittest.TestCase):
    def setUp(self):
        clear_routes()

    def tearDown(self):
        clear_routes()

    def test_exact_match_via_integration(self):
        def handler(match: RouteMatch, body: dict | None) -> dict:
            return {"matched": True}

        register_route("GET", "/plans", handler)
        server = create_server(ServerConfig(port=_find_free_port()))
        thread = threading.Thread(target=server.serve_forever)
        thread.daemon = True
        thread.start()
        try:
            url = f"http://127.0.0.1:{server.server_address[1]}/api/v1/plans"
            with urllib.request.urlopen(url) as resp:
                data = json.loads(resp.read())
            self.assertTrue(data["matched"])
        finally:
            server.shutdown()

    def test_wildcard_match_via_integration(self):
        def echo(match: RouteMatch, body: dict | None) -> dict:
            return {"id": match.params.get("plan_id")}

        register_route("GET", "/plans/{plan_id}", echo)
        port = _find_free_port()
        server = create_server(ServerConfig(port=port))
        thread = threading.Thread(target=server.serve_forever)
        thread.daemon = True
        thread.start()
        try:
            url = f"http://127.0.0.1:{port}/api/v1/plans/p123"
            with urllib.request.urlopen(url) as resp:
                data = json.loads(resp.read())
            self.assertEqual(data["id"], "p123")
        finally:
            server.shutdown()

    def test_no_match_via_integration(self):
        port = _find_free_port()
        server = create_server(ServerConfig(port=port))
        thread = threading.Thread(target=server.serve_forever)
        thread.daemon = True
        thread.start()
        try:
            url = f"http://127.0.0.1:{port}/api/v1/nonexistent"
            try:
                urllib.request.urlopen(url)
                self.fail("Should have raised")
            except urllib.error.HTTPError as e:
                self.assertEqual(e.code, 404)
        finally:
            server.shutdown()

    def test_method_mismatch_via_integration(self):
        def echo(match: RouteMatch, body: dict | None) -> dict:
            return {}

        register_route("POST", "/plans", echo)
        port = _find_free_port()
        server = create_server(ServerConfig(port=port))
        thread = threading.Thread(target=server.serve_forever)
        thread.daemon = True
        thread.start()
        try:
            url = f"http://127.0.0.1:{port}/api/v1/plans"
            try:
                urllib.request.urlopen(url)
                self.fail("Should have raised")
            except urllib.error.HTTPError as e:
                self.assertEqual(e.code, 404)
        finally:
            server.shutdown()


class RegisterRouteTests(unittest.TestCase):
    def setUp(self):
        clear_routes()

    def tearDown(self):
        clear_routes()

    def test_register_and_lookup(self):
        def handler(match: RouteMatch, body: dict | None) -> dict:
            return {"ok": True}
        register_route("GET", "/test", handler)
        self.assertIn(("GET", "/test"), HarnessHTTPHandler.routes)

    def test_clear_routes(self):
        register_route("GET", "/test", lambda m, b: {})
        clear_routes()
        self.assertEqual(len(HarnessHTTPHandler.routes), 0)


class HTTPIntegrationTests(unittest.TestCase):
    def setUp(self):
        clear_routes()
        self.port = _find_free_port()
        self.config = ServerConfig(port=self.port)

    def tearDown(self):
        clear_routes()

    def test_get_returns_json(self):
        def handler(match: RouteMatch, body: dict | None) -> dict:
            return {"status": "ok"}

        register_route("GET", "/ping", handler)
        server = create_server(self.config)
        thread = threading.Thread(target=server.serve_forever)
        thread.daemon = True
        thread.start()
        try:
            url = f"http://127.0.0.1:{self.port}/api/v1/ping"
            with urllib.request.urlopen(url) as resp:
                data = json.loads(resp.read())
            self.assertEqual(data["status"], "ok")
            self.assertEqual(resp.status, 200)
        finally:
            server.shutdown()

    def test_post_with_body(self):
        received: dict = {}

        def handler(match: RouteMatch, body: dict | None) -> dict:
            received["body"] = body
            return {"received": True}

        register_route("POST", "/plans", handler)
        server = create_server(self.config)
        thread = threading.Thread(target=server.serve_forever)
        thread.daemon = True
        thread.start()
        try:
            url = f"http://127.0.0.1:{self.port}/api/v1/plans"
            payload = json.dumps({"task": "build"}).encode()
            req = urllib.request.Request(url, data=payload, method="POST",
                                        headers={"Content-Type": "application/json"})
            with urllib.request.urlopen(req) as resp:
                data = json.loads(resp.read())
            self.assertTrue(data["received"])
            self.assertEqual(received["body"]["task"], "build")
        finally:
            server.shutdown()

    def test_404_for_unknown_route(self):
        server = create_server(self.config)
        thread = threading.Thread(target=server.serve_forever)
        thread.daemon = True
        thread.start()
        try:
            url = f"http://127.0.0.1:{self.port}/api/v1/nonexistent"
            try:
                urllib.request.urlopen(url)
                self.fail("Should have raised")
            except urllib.error.HTTPError as e:
                self.assertEqual(e.code, 404)
        finally:
            server.shutdown()

    def test_custom_api_prefix(self):
        def handler(match: RouteMatch, body: dict | None) -> dict:
            return {"prefix": "v2"}

        config = ServerConfig(port=self.port, api_prefix="/v2")
        register_route("GET", "/data", handler)
        server = create_server(config)
        thread = threading.Thread(target=server.serve_forever)
        thread.daemon = True
        thread.start()
        try:
            url = f"http://127.0.0.1:{self.port}/v2/data"
            with urllib.request.urlopen(url) as resp:
                data = json.loads(resp.read())
            self.assertEqual(data["prefix"], "v2")
        finally:
            server.shutdown()

    def test_handler_exception_returns_500(self):
        def handler(match: RouteMatch, body: dict | None) -> dict:
            raise RuntimeError("boom")

        register_route("GET", "/error", handler)
        server = create_server(self.config)
        thread = threading.Thread(target=server.serve_forever)
        thread.daemon = True
        thread.start()
        try:
            url = f"http://127.0.0.1:{self.port}/api/v1/error"
            try:
                urllib.request.urlopen(url)
                self.fail("Should have raised")
            except urllib.error.HTTPError as e:
                self.assertEqual(e.code, 500)
        finally:
            server.shutdown()

    def test_path_params_extraction(self):
        def handler(match: RouteMatch, body: dict | None) -> dict:
            return {"repo_id": match.params.get("repo_id")}

        register_route("GET", "/repos/{repo_id}", handler)
        server = create_server(self.config)
        thread = threading.Thread(target=server.serve_forever)
        thread.daemon = True
        thread.start()
        try:
            url = f"http://127.0.0.1:{self.port}/api/v1/repos/my-repo"
            with urllib.request.urlopen(url) as resp:
                data = json.loads(resp.read())
            self.assertEqual(data["repo_id"], "my-repo")
        finally:
            server.shutdown()


class StartServerInThreadTests(unittest.TestCase):
    def setUp(self):
        clear_routes()
        self.port = _find_free_port()

    def tearDown(self):
        clear_routes()

    def test_starts_and_stops(self):
        def handler(match: RouteMatch, body: dict | None) -> dict:
            return {"alive": True}

        register_route("GET", "/health", handler)
        server, thread = start_server_in_thread(ServerConfig(port=self.port))
        try:
            url = f"http://127.0.0.1:{self.port}/api/v1/health"
            with urllib.request.urlopen(url) as resp:
                data = json.loads(resp.read())
            self.assertTrue(data["alive"])
        finally:
            server.shutdown()
            thread.join(timeout=2)

    def test_daemon_thread(self):
        server, thread = start_server_in_thread(ServerConfig(port=self.port))
        self.assertTrue(thread.daemon)
        server.shutdown()


class SchemaVersionTests(unittest.TestCase):
    def test_schema_version_defined(self):
        self.assertEqual(HTTP_SERVER_SCHEMA_VERSION, "http_server.v1")


if __name__ == "__main__":
    unittest.main()
