"""Phase 6A/6B: HTTPServer — stdlib-based local API server with per-server isolation."""

from __future__ import annotations

import json
import threading
import uuid
from dataclasses import dataclass, field
from http.server import BaseHTTPRequestHandler, HTTPServer
from typing import Any, Callable


HTTP_SERVER_SCHEMA_VERSION = "http_server.v1"


@dataclass(frozen=True)
class ServerConfig:
    host: str = "127.0.0.1"
    port: int = 8080
    api_prefix: str = "/api/v1"


@dataclass(frozen=True)
class RouteMatch:
    method: str
    path: str
    params: dict[str, str] = field(default_factory=dict)


RequestHandler = Callable[[RouteMatch, dict[str, Any] | None], dict[str, Any]]


@dataclass
class ServerContext:
    """Per-server state: routes, config, and store."""
    config: ServerConfig
    routes: dict[tuple[str, str], RequestHandler] = field(default_factory=dict)
    store: Any = None
    tenant_resolver: Any = None


class HarnessHTTPHandler(BaseHTTPRequestHandler):
    """HTTP request handler that delegates to registered route handlers."""

    @property
    def _context(self) -> ServerContext:
        return self.server._harness_context  # type: ignore[attr-defined]

    @property
    def _config(self) -> ServerConfig:
        return self._context.config

    def log_message(self, format: str, *args: Any) -> None:
        pass

    def _read_body(self) -> dict[str, Any] | None:
        content_length = int(self.headers.get("Content-Length", 0))
        if content_length == 0:
            return None
        body = self.rfile.read(content_length)
        try:
            return json.loads(body)
        except json.JSONDecodeError:
            return None

    def _send_json(self, data: dict[str, Any], status: int = 200) -> None:
        body = json.dumps(data, default=str).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _send_error_json(self, status: int, message: str) -> None:
        self._send_json({"error": message}, status=status)

    def _match_route(self, method: str) -> tuple[RequestHandler, dict[str, str]] | None:
        path = self.path
        query_idx = path.find("?")
        if query_idx != -1:
            path = path[:query_idx]
        prefix = self._config.api_prefix
        if path.startswith(prefix):
            path = path[len(prefix):]
        if not path.startswith("/"):
            path = "/" + path

        for (route_method, route_path), handler in self._context.routes.items():
            if route_method != method:
                continue
            params = self._match_path(route_path, path)
            if params is not None:
                return handler, params
        return None

    def _match_path(self, pattern: str, path: str) -> dict[str, str] | None:
        pattern_parts = pattern.strip("/").split("/")
        path_parts = path.strip("/").split("/")
        if len(pattern_parts) != len(path_parts):
            return None
        params: dict[str, str] = {}
        for pp, rp in zip(pattern_parts, path_parts):
            if pp.startswith("{") and pp.endswith("}"):
                params[pp[1:-1]] = rp
            elif pp != rp:
                return None
        return params

    def do_GET(self) -> None:
        self._handle_request("GET")

    def do_POST(self) -> None:
        self._handle_request("POST")

    def do_PUT(self) -> None:
        self._handle_request("PUT")

    def do_DELETE(self) -> None:
        self._handle_request("DELETE")

    def _authenticate_request(self) -> Any:
        """Run auth middleware if tenant_resolver is configured.

        Returns RequestContext if allowed, or None if 401 already sent.
        Returns None silently when no tenant_resolver is configured (unauthenticated mode).
        """
        resolver = self._context.tenant_resolver
        if resolver is None:
            return None
        auth_header = self.headers.get("Authorization")
        decision = resolver.resolve(auth_header)
        if not decision.allowed:
            self._send_error_json(401, decision.reason)
            return None
        return {
            "tenant_id": decision.tenant_id,
            "api_key_id": decision.api_key_id,
            "scopes": decision.scopes,
            "request_id": str(uuid.uuid4()),
        }

    def _handle_request(self, method: str) -> None:
        ctx = self._authenticate_request()
        if ctx is None and self._context.tenant_resolver is not None:
            return
        result = self._match_route(method)
        if result is None:
            self._send_error_json(404, f"no route for {method} {self.path}")
            return
        handler, params = result
        body = self._read_body()
        match = RouteMatch(method=method, path=self.path, params=params)
        try:
            response = handler(match, body)
            self._send_json(response)
        except Exception:
            self._send_error_json(500, "internal server error")


_last_context: ServerContext | None = None


def register_route(method: str, path: str, handler: RequestHandler,
                   server: HTTPServer | None = None) -> None:
    """Register a route handler. If server is given, register on that server's context.

    If server is None, registers on the most recently created server context.
    """
    ctx = _get_context(server)
    ctx.routes[(method, path)] = handler


def clear_routes(server: HTTPServer | None = None) -> None:
    """Clear registered routes. If server is given, clear that server's routes.

    If server is None, clears the most recently created server context's routes.
    """
    ctx = _get_context(server)
    ctx.routes.clear()


def _get_context(server: HTTPServer | None = None) -> ServerContext:
    if server is not None:
        return server._harness_context  # type: ignore[attr-defined]
    if _last_context is None:
        raise RuntimeError("No server created yet. Call create_server() first.")
    return _last_context


def create_server(config: ServerConfig | None = None,
                  store: Any = None,
                  tenant_resolver: Any = None) -> HTTPServer:
    """Create an HTTPServer with per-server isolated state."""
    global _last_context
    cfg = config or ServerConfig()
    ctx = ServerContext(config=cfg, store=store, tenant_resolver=tenant_resolver)
    _last_context = ctx
    server = HTTPServer((cfg.host, cfg.port), HarnessHTTPHandler)
    server._harness_context = ctx  # type: ignore[attr-defined]
    return server


def start_server_in_thread(config: ServerConfig | None = None,
                           store: Any = None,
                           tenant_resolver: Any = None) -> tuple[HTTPServer, threading.Thread]:
    """Start the server in a daemon thread. Returns (server, thread)."""
    server = create_server(config, store, tenant_resolver=tenant_resolver)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server, thread
