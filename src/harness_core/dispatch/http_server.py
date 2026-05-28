"""Phase 6A: HTTPServer — stdlib-based local API server."""

from __future__ import annotations

import json
import threading
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


class HarnessHTTPHandler(BaseHTTPRequestHandler):
    """HTTP request handler that delegates to registered route handlers."""

    routes: dict[tuple[str, str], RequestHandler] = {}
    store: Any = None
    config: ServerConfig = ServerConfig()

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
        prefix = self.config.api_prefix
        if path.startswith(prefix):
            path = path[len(prefix):]
        if not path.startswith("/"):
            path = "/" + path

        for (route_method, route_path), handler in self.routes.items():
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

    def _handle_request(self, method: str) -> None:
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
        except Exception as e:
            self._send_error_json(500, str(e))


def register_route(method: str, path: str, handler: RequestHandler) -> None:
    """Register a route handler on the HarnessHTTPHandler class."""
    HarnessHTTPHandler.routes[(method, path)] = handler


def clear_routes() -> None:
    """Clear all registered routes."""
    HarnessHTTPHandler.routes.clear()


def create_server(config: ServerConfig | None = None,
                  store: Any = None) -> HTTPServer:
    """Create an HTTPServer with HarnessHTTPHandler configured."""
    cfg = config or ServerConfig()
    HarnessHTTPHandler.config = cfg
    HarnessHTTPHandler.store = store
    server = HTTPServer((cfg.host, cfg.port), HarnessHTTPHandler)
    return server


def start_server_in_thread(config: ServerConfig | None = None,
                           store: Any = None) -> tuple[HTTPServer, threading.Thread]:
    """Start the server in a daemon thread. Returns (server, thread)."""
    server = create_server(config, store)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server, thread
