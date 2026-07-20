#!/usr/bin/env python3
"""Five-minute no-provider local demo for Token-Efficient Agent Harness Lab.

Starts the native engine with a temporary database on a free loopback port,
records a fixture dispatch (noop executor — no provider), binds evidence to
the current source revision, and proves stale-head evidence is rejected.

No API keys, no real providers, no target-repository writes.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

STATE_DIR_NAME = ".acp-demo-state"
PID_FILE = "engine.pid"
META_FILE = "demo-meta.json"
PROOF_FILE = "demo-proof.json"


def default_engine_bin(repo_root: Path) -> Path:
    suffix = ".exe" if sys.platform == "win32" else ""
    new_name = repo_root / "target" / "debug" / f"agent-control-plane{suffix}"
    old_name = repo_root / "target" / "debug" / f"engine{suffix}"
    return new_name if new_name.exists() else old_name


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def fetch_text(url: str, timeout: float = 3.0) -> str:
    with urlopen(url, timeout=timeout) as response:
        return response.read().decode("utf-8")


def fetch_json(url: str, timeout: float = 3.0) -> dict:
    return json.loads(fetch_text(url, timeout=timeout))


def post_json(url: str, body: dict, timeout: float = 5.0) -> dict:
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
        except (HTTPError, URLError, TimeoutError, OSError, json.JSONDecodeError) as exc:
            last_error = exc
            time.sleep(0.2)
    raise RuntimeError(f"engine did not become healthy: {last_error}")


def git_head_sha(repo_root: Path) -> str:
    result = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=repo_root,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(f"git rev-parse HEAD failed: {result.stderr.strip()}")
    sha = result.stdout.strip()
    if len(sha) != 40 or any(ch not in "0123456789abcdef" for ch in sha):
        raise RuntimeError(f"unexpected HEAD sha: {sha!r}")
    return sha


def build_proof(
    *,
    source_revision: str,
    base_url: str,
    dispatch_id: str,
    executor_type: str,
    health_status: str,
) -> dict:
    return {
        "kind": "acp-no-provider-demo-proof.v1",
        "source_revision": source_revision,
        "base_url": base_url,
        "dispatch_id": dispatch_id,
        "executor_type": executor_type,
        "health_status": health_status,
        "provider_calls": False,
        "target_repo_writes": False,
    }


def verify_proof_against_revision(proof: dict, expected_revision: str) -> None:
    """Fail closed when evidence is bound to a different source revision."""
    if not isinstance(proof, dict):
        raise RuntimeError("proof must be an object")
    if proof.get("kind") != "acp-no-provider-demo-proof.v1":
        raise RuntimeError("proof kind mismatch")
    bound = proof.get("source_revision")
    if not bound:
        raise RuntimeError("proof missing source_revision")
    if bound != expected_revision:
        raise RuntimeError(
            f"stale-head rejected: proof bound to {bound}, expected {expected_revision}"
        )
    if proof.get("provider_calls") is not False:
        raise RuntimeError("proof must attest no provider calls")
    if proof.get("target_repo_writes") is not False:
        raise RuntimeError("proof must attest no target-repo writes")
    if not proof.get("dispatch_id"):
        raise RuntimeError("proof missing dispatch_id")


def pass_line(message: str) -> None:
    print(f"PASS  {message}")


def open_line(message: str) -> None:
    print(f"OPEN  {message}")


def cleanup_line(message: str) -> None:
    print(f"CLEANUP  {message}")


def state_dir(repo_root: Path) -> Path:
    return repo_root / STATE_DIR_NAME


def stop_kept_engine(repo_root: Path) -> None:
    root = state_dir(repo_root)
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
    cleanup_line(f"removed {root}")


def run_demo(
    *,
    repo_root: Path,
    engine_bin: Path,
    dashboard_dir: Path,
    timeout: float,
    keep: bool,
) -> int:
    if not engine_bin.exists():
        raise SystemExit(
            f"engine binary not found: {engine_bin}\n"
            "Build first: cargo build -p engine\n"
            "And static dashboard: cd dashboard && bun install --frozen-lockfile && bun run build:static"
        )
    if not (dashboard_dir / "index.html").exists():
        raise SystemExit(
            f"dashboard static index not found: {dashboard_dir / 'index.html'}\n"
            "Build: cd dashboard && bun install --frozen-lockfile && bun run build:static"
        )

    source_revision = git_head_sha(repo_root)
    port = free_port()
    base_url = f"http://127.0.0.1:{port}"

    if keep:
        data_root = state_dir(repo_root)
        if data_root.exists():
            stop_kept_engine(repo_root)
        data_root.mkdir(parents=True, exist_ok=True)
        temp_cm = None
    else:
        temp_cm = tempfile.TemporaryDirectory(prefix="acp-demo-")
        data_root = Path(temp_cm.name)

    try:
        env = {
            **os.environ,
            "HOST": "127.0.0.1",
            "PORT": str(port),
            "ACP_DASHBOARD_DIR": str(dashboard_dir),
            "ACP_DB_PATH": str(data_root / "local-team.db"),
            "ACP_BACKUP_DIR": str(data_root / "backups"),
        }
        # Explicitly avoid provider/trusted-local gates for the public demo.
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

        log_path = data_root / "engine.log"
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
            try:
                health = wait_for_health(base_url, process, time.monotonic() + timeout)
                if health.get("status") != "healthy":
                    raise RuntimeError(f"unexpected health payload: {health}")
                pass_line("local runtime healthy")

                ready = fetch_json(f"{base_url}/api/v1/ready")
                if ready.get("status") != "ready":
                    raise RuntimeError(f"unexpected ready payload: {ready}")

                dispatch = post_json(
                    f"{base_url}/api/v1/dispatch",
                    {
                        "raw_request": "Demo: summarize docs without provider calls",
                        "request_source": "api",
                    },
                )
                executor_type = dispatch.get("execution_result", {}).get("executor_type")
                if executor_type != "noop":
                    raise RuntimeError(
                        f"expected noop executor (no provider); got {executor_type!r}"
                    )
                record = dispatch.get("record") or {}
                dispatch_id = record.get("dispatch_id") or dispatch.get(
                    "execution_result", {}
                ).get("dispatch_id")
                if not dispatch_id:
                    raise RuntimeError(f"dispatch missing id: {dispatch}")
                pass_line("fixture dispatch recorded")

                dispatches = fetch_json(f"{base_url}/api/v1/dispatches?limit=5&search=Demo")
                if not dispatches.get("dispatches"):
                    raise RuntimeError("dispatch search returned no records")

                dashboard_state = fetch_json(f"{base_url}/api/v1/dashboard")
                if int(dashboard_state.get("counts", {}).get("dispatches") or 0) < 1:
                    raise RuntimeError("dashboard did not show persisted dispatch evidence")

                audit = fetch_json(f"{base_url}/api/v1/audit?limit=5&search=dispatch")
                if not audit.get("events"):
                    raise RuntimeError("audit did not return dispatch events")

                proof = build_proof(
                    source_revision=source_revision,
                    base_url=base_url,
                    dispatch_id=str(dispatch_id),
                    executor_type=str(executor_type),
                    health_status=str(health.get("status")),
                )
                verify_proof_against_revision(proof, source_revision)
                pass_line(
                    f"evidence bound to run and source revision ({source_revision[:12]}…)"
                )

                # Stale-head scenario: same proof must fail against a different revision.
                fake_head = "0" * 40
                try:
                    verify_proof_against_revision(proof, fake_head)
                except RuntimeError as exc:
                    if "stale-head rejected" not in str(exc):
                        raise
                    pass_line("stale-head scenario rejected")
                else:
                    raise RuntimeError("stale-head scenario unexpectedly accepted")

                root_html = fetch_text(f"{base_url}/")
                if "Agent Control Plane" not in root_html and "Agent" not in root_html:
                    raise RuntimeError("dashboard root did not serve expected content")
                open_line(base_url)

                proof_path = data_root / PROOF_FILE
                proof_path.write_text(
                    json.dumps(proof, indent=2, sort_keys=True) + "\n", encoding="utf-8"
                )
                print(f"PROOF {proof_path}")

                if keep:
                    (data_root / PID_FILE).write_text(str(process.pid), encoding="utf-8")
                    meta = {
                        "pid": process.pid,
                        "port": port,
                        "base_url": base_url,
                        "source_revision": source_revision,
                        "dispatch_id": dispatch_id,
                    }
                    (data_root / META_FILE).write_text(
                        json.dumps(meta, indent=2, sort_keys=True) + "\n", encoding="utf-8"
                    )
                    print(
                        "KEEP  engine left running; stop with: ./scripts/demo.sh --cleanup"
                    )
                    process = None  # type: ignore[assignment]
                else:
                    cleanup_line("temporary data directory removed on exit")

                print("Demo complete (no provider, no target-repo write).")
                return 0
            except Exception:
                # Surface the last engine log lines before temp cleanup so CI can diagnose.
                try:
                    log_handle.flush()
                    log_tail = log_path.read_text(encoding="utf-8", errors="replace")[-2000:]
                    if log_tail.strip():
                        print("ENGINE_LOG_TAIL_BEGIN", flush=True)
                        print(log_tail, flush=True)
                        print("ENGINE_LOG_TAIL_END", flush=True)
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
    finally:
        if temp_cm is not None:
            temp_cm.cleanup()


def main() -> int:
    parser = argparse.ArgumentParser(
        description="No-provider local demo: health, fixture dispatch, exact-revision proof, stale-head reject."
    )
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--engine-bin", type=Path)
    parser.add_argument("--dashboard-dir", type=Path)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument(
        "--keep",
        action="store_true",
        help=f"Leave the engine running under {STATE_DIR_NAME}/ (use --cleanup later).",
    )
    parser.add_argument(
        "--cleanup",
        action="store_true",
        help=f"Stop a --keep demo and remove {STATE_DIR_NAME}/.",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="Run pure proof verification checks without starting the engine.",
    )
    args = parser.parse_args()
    repo_root = args.repo_root.resolve()

    if args.cleanup:
        stop_kept_engine(repo_root)
        return 0

    if args.self_test:
        good = build_proof(
            source_revision="a" * 40,
            base_url="http://127.0.0.1:9",
            dispatch_id="disp-demo",
            executor_type="noop",
            health_status="healthy",
        )
        verify_proof_against_revision(good, "a" * 40)
        try:
            verify_proof_against_revision(good, "b" * 40)
        except RuntimeError as exc:
            if "stale-head rejected" not in str(exc):
                raise
        else:
            raise SystemExit("self-test expected stale-head rejection")
        print("self-test ok")
        return 0

    engine_bin = (args.engine_bin or default_engine_bin(repo_root)).resolve()
    dashboard_dir = (args.dashboard_dir or repo_root / "dashboard" / "out").resolve()
    return run_demo(
        repo_root=repo_root,
        engine_bin=engine_bin,
        dashboard_dir=dashboard_dir,
        timeout=args.timeout,
        keep=args.keep,
    )


if __name__ == "__main__":
    raise SystemExit(main())
