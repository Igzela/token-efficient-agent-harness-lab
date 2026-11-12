#!/usr/bin/env python3
"""Start the local operator desktop: engine + dashboard UI.

No provider call. Serves the built dashboard from the engine on a loopback
URL and opens it with the desktop browser when available (xdg-open / open).
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

STATE_DIR_NAME = ".acp-operator-state"
PID_FILE = "engine.pid"
META_FILE = "operator-meta.json"


def repo_root_from_script() -> Path:
    return Path(__file__).resolve().parents[1]


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def engine_binary(repo_root: Path) -> Path:
    suffix = ".exe" if sys.platform == "win32" else ""
    debug = repo_root / "target" / "debug" / f"agent-control-plane{suffix}"
    release = repo_root / "target" / "release" / f"agent-control-plane{suffix}"
    if release.exists():
        return release
    return debug


def request_json(url: str, timeout: float = 3.0) -> dict:
    with urllib.request.urlopen(url, timeout=timeout) as response:
        return json.loads(response.read().decode("utf-8"))


def wait_for_health(base_url: str, process: subprocess.Popen[str], deadline: float) -> dict:
    last_error: Exception | None = None
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"engine exited early with code {process.returncode}")
        try:
            return request_json(f"{base_url}/api/v1/health")
        except (urllib.error.URLError, TimeoutError, OSError, json.JSONDecodeError) as exc:
            last_error = exc
            time.sleep(0.2)
    raise RuntimeError(f"engine did not become healthy: {last_error}")


def open_desktop(url: str) -> None:
    for opener in (
        ["xdg-open", url],
        ["gio", "open", url],
        ["open", url],
    ):
        if shutil.which(opener[0]):
            subprocess.Popen(
                opener,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            print(f"OPEN  desktop browser -> {url}")
            return
    print(f"OPEN  {url}")
    print("HINT  no xdg-open/open found; paste the URL into a browser")


def stop_engine(root: Path) -> None:
    pid_path = root / PID_FILE
    if pid_path.exists():
        try:
            pid = int(pid_path.read_text(encoding="utf-8").strip())
        except ValueError:
            pid = 0
        if pid > 0:
            try:
                os.kill(pid, 15)
            except ProcessLookupError:
                pass
            for _ in range(20):
                try:
                    os.kill(pid, 0)
                except ProcessLookupError:
                    break
                time.sleep(0.1)
            else:
                try:
                    os.kill(pid, 9)
                except ProcessLookupError:
                    pass
    if root.exists():
        shutil.rmtree(root, ignore_errors=True)
    print(f"CLEANUP  removed {root}")


def run_operator(
    *,
    repo_root: Path,
    engine_bin: Path,
    dashboard_dir: Path,
    timeout: float,
    port: int | None,
) -> int:
    if not engine_bin.exists():
        raise SystemExit(
            f"engine binary not found: {engine_bin}\n"
            "Build first: cargo build -p engine"
        )
    if not (dashboard_dir / "index.html").exists():
        raise SystemExit(
            f"dashboard static index not found: {dashboard_dir / 'index.html'}\n"
            "Build: cd dashboard && bun install --frozen-lockfile && bun run build:static"
        )

    state = repo_root / STATE_DIR_NAME
    if state.exists():
        stop_engine(state)
    state.mkdir(parents=True)

    chosen_port = port or free_port()
    base_url = f"http://127.0.0.1:{chosen_port}"
    env = {
        **os.environ,
        "HOST": "127.0.0.1",
        "PORT": str(chosen_port),
        "ACP_DASHBOARD_DIR": str(dashboard_dir),
        "ACP_DB_PATH": str(state / "local-team.db"),
        "ACP_BACKUP_DIR": str(state / "backups"),
        "ACP_PRODUCT_GOLDEN_PATH": "1",
        "ACP_ENABLE_SCHEDULER": "1",
        "ACP_ENABLE_SUPERVISED_WORKERS": "1",
        "ACP_SCHEDULER_EXECUTOR": "command",
        "ACP_SCHEDULER_INTERVAL_MS": "500",
    }
    for key in (
        "ACP_REQUIRE_AUTH",
        "ACP_ADMIN_API_KEY",
        "ACP_TRUSTED_LOCAL_PROFILE",
        "ACP_ENABLE_PROVIDER_EXECUTION",
        "ACP_ENABLE_CLI_EXECUTION",
        "ACP_ENABLE_TARGET_REPO_OUTPUT",
        "ACP_PROVIDER_TYPE",
        "ACP_API_KEY",
    ):
        env.pop(key, None)

    log_path = state / "engine.log"
    log_handle = log_path.open("w", encoding="utf-8")
    process = subprocess.Popen(
        [str(engine_bin)],
        cwd=repo_root,
        env=env,
        stdout=log_handle,
        stderr=subprocess.STDOUT,
        text=True,
    )
    try:
        health = wait_for_health(base_url, process, time.monotonic() + timeout)
        if health.get("status") != "healthy":
            raise RuntimeError(f"unexpected health payload: {health}")
        ready = request_json(f"{base_url}/api/v1/ready")
        if ready.get("status") != "ready":
            raise RuntimeError(f"unexpected ready payload: {ready}")
        print("PASS  local runtime healthy")
        print(f"PASS  dashboard served from {dashboard_dir}")
        open_desktop(base_url)
        print("UI tabs: Mission Control, Operator, Runs, Scheduler, Settings, Health, Audit")
        print("CLI companion: ./scripts/harness.sh work   # finish one local folder job")
        print("Stop with: ./scripts/harness.sh stop")
        (state / PID_FILE).write_text(str(process.pid), encoding="utf-8")
        (state / META_FILE).write_text(
            json.dumps(
                {
                    "pid": process.pid,
                    "port": chosen_port,
                    "base_url": base_url,
                    "dashboard_dir": str(dashboard_dir),
                },
                indent=2,
            )
            + "\n",
            encoding="utf-8",
        )
        process = None  # type: ignore[assignment]
        return 0
    except Exception:
        try:
            log_handle.flush()
            tail = log_path.read_text(encoding="utf-8", errors="replace")[-2000:]
            if tail.strip():
                print("ENGINE_LOG_TAIL_BEGIN")
                print(tail)
                print("ENGINE_LOG_TAIL_END")
        except OSError:
            pass
        raise
    finally:
        log_handle.close()
        if process is not None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Start the local operator desktop (engine + dashboard UI)."
    )
    parser.add_argument("--repo-root", type=Path, default=repo_root_from_script())
    parser.add_argument("--engine-bin", type=Path)
    parser.add_argument("--dashboard-dir", type=Path)
    parser.add_argument("--timeout", type=float, default=60.0)
    parser.add_argument("--port", type=int, default=0, help="0 = pick a free loopback port")
    parser.add_argument("--cleanup", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    repo_root = args.repo_root.resolve()

    if args.cleanup:
        stop_engine(repo_root / STATE_DIR_NAME)
        work = repo_root / ".acp-work-state"
        demo = repo_root / ".acp-demo-state"
        if work.exists():
            stop_engine(work)
        if demo.exists():
            stop_engine(demo)
        return 0

    if args.self_test:
        assert STATE_DIR_NAME == ".acp-operator-state"
        print("self-test ok")
        return 0

    return run_operator(
        repo_root=repo_root,
        engine_bin=(args.engine_bin or engine_binary(repo_root)).resolve(),
        dashboard_dir=(args.dashboard_dir or repo_root / "dashboard" / "out").resolve(),
        timeout=args.timeout,
        port=args.port or None,
    )


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except BrokenPipeError:
        raise SystemExit(0)
