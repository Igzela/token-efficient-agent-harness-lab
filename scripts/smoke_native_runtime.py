#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from urllib.error import HTTPError
from urllib.request import Request, urlopen


def default_engine_bin(repo_root: Path) -> Path:
    suffix = ".exe" if sys.platform == "win32" else ""
    # New binary name; fall back to legacy name for compatibility
    new_name = repo_root / "target" / "debug" / f"agent-control-plane{suffix}"
    old_name = repo_root / "target" / "debug" / f"engine{suffix}"
    return new_name if new_name.exists() else old_name


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def fetch_text(url: str, timeout: float = 2.0) -> str:
    with urlopen(url, timeout=timeout) as response:
        return response.read().decode("utf-8")


def fetch_json(url: str, timeout: float = 2.0) -> dict:
    return json.loads(fetch_text(url, timeout=timeout))


def fetch_error_json(url: str, timeout: float = 2.0) -> tuple[int, dict]:
    try:
        fetch_text(url, timeout=timeout)
    except HTTPError as error:
        return error.code, json.loads(error.read().decode("utf-8"))
    raise RuntimeError(f"expected HTTP error from {url}")


def post_json(url: str, body: dict, timeout: float = 2.0) -> dict:
    request = Request(
        url,
        data=json.dumps(body).encode("utf-8"),
        headers={"content-type": "application/json"},
        method="POST",
    )
    with urlopen(request, timeout=timeout) as response:
        return json.loads(response.read().decode("utf-8"))


def wait_for_health(base_url: str, process: subprocess.Popen[str], deadline: float) -> dict:
    last_error: Exception | None = None
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"engine exited early with code {process.returncode}")
        try:
            return fetch_json(f"{base_url}/api/v1/health")
        except Exception as exc:  # noqa: BLE001 - smoke script reports the last transient error.
            last_error = exc
            time.sleep(0.2)
    raise RuntimeError(f"engine did not become healthy: {last_error}")


def main() -> int:
    parser = argparse.ArgumentParser(description="Smoke test native engine + static dashboard runtime.")
    parser.add_argument("--repo-root", default=Path(__file__).resolve().parents[1], type=Path)
    parser.add_argument("--engine-bin", type=Path)
    parser.add_argument("--dashboard-dir", type=Path)
    parser.add_argument("--timeout", default=15.0, type=float)
    args = parser.parse_args()

    repo_root = args.repo_root.resolve()
    engine_bin = (args.engine_bin or default_engine_bin(repo_root)).resolve()
    dashboard_dir = (args.dashboard_dir or repo_root / "dashboard" / "out").resolve()

    if not engine_bin.exists():
        raise SystemExit(f"engine binary not found: {engine_bin}")
    if not (dashboard_dir / "index.html").exists():
        raise SystemExit(f"dashboard static index not found: {dashboard_dir / 'index.html'}")

    port = free_port()
    base_url = f"http://127.0.0.1:{port}"
    with tempfile.TemporaryDirectory(prefix="acp-native-smoke-") as data_dir:
        data_root = Path(data_dir)
        env = {
            **os.environ,
            "HOST": "127.0.0.1",
            "PORT": str(port),
            "ACP_DASHBOARD_DIR": str(dashboard_dir),
            "ACP_DB_PATH": str(data_root / "local-team.db"),
            "ACP_BACKUP_DIR": str(data_root / "backups"),
        }
        process = subprocess.Popen(
            [str(engine_bin)],
            cwd=repo_root,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
        try:
            health = wait_for_health(base_url, process, time.monotonic() + args.timeout)
            if health.get("status") != "healthy":
                raise RuntimeError(f"unexpected health payload: {health}")

            ready = fetch_json(f"{base_url}/api/v1/ready")
            if ready.get("status") != "ready":
                raise RuntimeError(f"unexpected ready payload: {ready}")

            dispatch = post_json(
                f"{base_url}/api/v1/dispatch",
                {"raw_request": "Summarize docs without provider calls", "request_source": "api"},
            )
            if dispatch.get("execution_result", {}).get("executor_type") != "noop":
                raise RuntimeError("dispatch did not use noop executor")

            dispatches = fetch_json(f"{base_url}/api/v1/dispatches?limit=2&search=docs")
            if len(dispatches.get("dispatches", [])) != 1:
                raise RuntimeError(f"dispatch search did not return the persisted record: {dispatches}")

            dashboard_state = fetch_json(f"{base_url}/api/v1/dashboard")
            if dashboard_state.get("counts", {}).get("dispatches") != 1:
                raise RuntimeError(f"dashboard did not read persisted dispatch state: {dashboard_state}")
            if dashboard_state.get("dispatches", [{}])[0].get("request_source") != "api":
                raise RuntimeError(f"dashboard dispatch history mismatch: {dashboard_state}")

            export_state = fetch_json(f"{base_url}/api/v1/export")
            if export_state.get("schema_version") != "local_team_export.v1":
                raise RuntimeError(f"unexpected export payload: {export_state}")

            audit = fetch_json(f"{base_url}/api/v1/audit?limit=2&search=dispatch")
            if not audit.get("events"):
                raise RuntimeError(f"audit search did not return dispatch audit events: {audit}")

            status, backup_error = fetch_error_json(f"{base_url}/api/v1/backups")
            if status != 401 or backup_error.get("code") != "backup_admin_required":
                raise RuntimeError(f"backup boundary error was not structured: {backup_error}")

            dashboard = fetch_text(f"{base_url}/")
            if "Agent Control Plane" not in dashboard:
                raise RuntimeError("dashboard root did not contain expected title")

            print(f"native runtime smoke passed at {base_url}")
            return 0
        finally:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)


if __name__ == "__main__":
    raise SystemExit(main())
