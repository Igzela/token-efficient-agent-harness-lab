#!/usr/bin/env python3
"""Local server for the Harness App read-only control plane."""

from __future__ import annotations

import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import sys

REPO_ROOT = Path(__file__).resolve().parent.parent
SRC_ROOT = REPO_ROOT / "src"
if str(SRC_ROOT) not in sys.path:
    sys.path.insert(0, str(SRC_ROOT))

from harness_core.app_api import default_plan_store_path, handle_api_request  # noqa: E402


ALLOWED_HOSTS = {"127.0.0.1", "localhost"}
DEFAULT_REGISTRY_PATH = REPO_ROOT / ".harness_app_registry.json"
DEFAULT_PLANS_PATH = default_plan_store_path()
DEFAULT_STATIC_ROOT = REPO_ROOT / "web" / "dashboard"


class HarnessAppRequestHandler(SimpleHTTPRequestHandler):
    """Serve dashboard files and local API responses from one origin."""

    registry_path: Path
    plans_path: Path

    def do_GET(self) -> None:
        if self.path.startswith("/api/"):
            self._send_api_response(handle_api_request("GET", self.path, None, self.registry_path, self.plans_path))
            return
        super().do_GET()

    def do_POST(self) -> None:
        if not self.path.startswith("/api/"):
            self._send_api_response(handle_api_request("POST", self.path, None, self.registry_path, self.plans_path))
            return

        try:
            content_length = int(self.headers.get("Content-Length", "0"))
        except ValueError:
            content_length = 0
        body = self.rfile.read(content_length) if content_length > 0 else b""
        self._send_api_response(handle_api_request("POST", self.path, body, self.registry_path, self.plans_path))

    def _send_api_response(self, response) -> None:
        body = response.body_bytes()
        self.send_response(response.status_code)
        for key, value in response.response_headers().items():
            self.send_header(key, value)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run the local Harness app dashboard and API server.")
    parser.add_argument("--host", default="127.0.0.1", help="Bind host. Only 127.0.0.1 or localhost are allowed.")
    parser.add_argument("--port", type=int, default=8765, help="Bind port.")
    parser.add_argument("--registry", default=str(DEFAULT_REGISTRY_PATH), help="Path to the app registry JSON file.")
    parser.add_argument("--plans", default=str(DEFAULT_PLANS_PATH), help="Path to the app plans JSON file.")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    if args.host not in ALLOWED_HOSTS:
        print("Refusing to bind non-local host. Use 127.0.0.1 or localhost.", file=sys.stderr)
        return 2

    registry_path = Path(args.registry).expanduser().resolve()
    plans_path = Path(args.plans).expanduser().resolve()
    class ConfiguredHarnessAppRequestHandler(HarnessAppRequestHandler):
        pass

    ConfiguredHarnessAppRequestHandler.registry_path = registry_path
    ConfiguredHarnessAppRequestHandler.plans_path = plans_path
    handler_class = partial(ConfiguredHarnessAppRequestHandler, directory=str(DEFAULT_STATIC_ROOT))

    server = ThreadingHTTPServer((args.host, args.port), handler_class)
    print(f"Serving Harness App on http://{args.host}:{args.port}/")
    print(f"Registry: {registry_path}")
    print(f"Plans: {plans_path}")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nStopping Harness App server.")
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
