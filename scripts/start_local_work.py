#!/usr/bin/env python3
"""Start the local engine and finish one real folder task.

No API key and no model call. The engine admits a local-folder product task,
the command executor writes one checked file inside an app-owned workspace,
and a confirmed output copies that file into the folder this script created.
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

STATE_DIR_NAME = ".acp-work-state"
PID_FILE = "engine.pid"
META_FILE = "work-meta.json"
INBOX_DIR_NAME = "inbox"
RESULT_RELATIVE = Path("docs") / "product_golden_path_fixture.md"
EXPECTED_NOTE = "product golden path fixture note\n"


def repo_root_from_script() -> Path:
    return Path(__file__).resolve().parents[1]


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def engine_binary(repo_root: Path) -> Path:
    suffix = ".exe" if sys.platform == "win32" else ""
    return repo_root / "target" / "debug" / f"agent-control-plane{suffix}"


def intake_body(folder: Path, idempotency_key: str) -> dict:
    return {
        "objective": "Create the approved bounded local document",
        "target_id": "local-work-start",
        "target_repo_path": str(folder),
        "source_kind": "local_folder",
        "source_revision": "operator-supplied-placeholder",
        "allowed_paths": ["docs/product_golden_path_fixture.md"],
        "verification_commands": [
            {
                "command": "test -f docs/product_golden_path_fixture.md",
                "timeout_ms": 5000,
            }
        ],
        "output_intent": "apply_local_changes",
        "executor_policy": {
            "allowed_executors": ["command"],
            "prefer": "command",
        },
        "risk_class": "low",
        "approval_required": True,
        "confirm_execution": True,
        "confirm_output": False,
        "idempotency_key": idempotency_key,
        "tenant_id": "local",
        "workspace_id": "default",
        "workspace_mode": "local_folder",
    }


def request_json(url: str, method: str, body: dict | None, timeout: float) -> dict:
    data = None if body is None else json.dumps(body).encode("utf-8")
    headers = {"content-type": "application/json"} if body is not None else {}
    request = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as exc:
        detail = exc.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"{method} {url} -> HTTP {exc.code}: {detail}") from exc


def wait_for_health(base_url: str, process: subprocess.Popen[str], deadline: float) -> dict:
    last_error: Exception | None = None
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"engine exited early with code {process.returncode}")
        try:
            return request_json(f"{base_url}/api/v1/health", "GET", None, 3)
        except (urllib.error.URLError, TimeoutError, OSError, json.JSONDecodeError, RuntimeError) as exc:
            last_error = exc
            time.sleep(0.2)
    raise RuntimeError(f"engine did not become healthy: {last_error}")


def task_record(payload: dict) -> dict:
    task = payload.get("task")
    if isinstance(task, dict):
        return task
    result = payload.get("result")
    if isinstance(result, dict):
        nested = result.get("task")
        if isinstance(nested, dict):
            return nested
    approval = payload.get("approval")
    if isinstance(approval, dict):
        nested = approval.get("task")
        if isinstance(nested, dict):
            return nested
    raise RuntimeError(f"response has no task: {json.dumps(payload)[:800]}")


def nested_get(payload: dict, *path: str):
    current: object = payload
    for key in path:
        if not isinstance(current, dict) or key not in current:
            return None
        current = current[key]
    return current


def prepare_inbox(root: Path) -> Path:
    inbox = root / INBOX_DIR_NAME
    if inbox.exists():
        shutil.rmtree(inbox)
    (inbox / "docs").mkdir(parents=True)
    (inbox / "README.md").write_text("original\n", encoding="utf-8")
    return inbox.resolve()


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


def run_local_work(*, repo_root: Path, engine_bin: Path, dashboard_dir: Path | None, timeout: float) -> int:
    if not engine_bin.exists():
        raise SystemExit(f"engine binary not found: {engine_bin}\nBuild first: cargo build -p engine")

    state = repo_root / STATE_DIR_NAME
    if state.exists():
        stop_engine(state)
    state.mkdir(parents=True)
    inbox = prepare_inbox(state)
    port = free_port()
    base_url = f"http://127.0.0.1:{port}"
    env = {
        **os.environ,
        "HOST": "127.0.0.1",
        "PORT": str(port),
        "ACP_DB_PATH": str(state / "local-team.db"),
        "ACP_BACKUP_DIR": str(state / "backups"),
        "ACP_PRODUCT_GOLDEN_PATH": "1",
        "ACP_ENABLE_SCHEDULER": "1",
        "ACP_ENABLE_SUPERVISED_WORKERS": "1",
        "ACP_SCHEDULER_EXECUTOR": "command",
        "ACP_SCHEDULER_INTERVAL_MS": "200",
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
    if dashboard_dir is not None and (dashboard_dir / "index.html").exists():
        env["ACP_DASHBOARD_DIR"] = str(dashboard_dir)

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
        print("PASS  local runtime healthy")

        created = request_json(
            f"{base_url}/api/v1/product/tasks",
            "POST",
            intake_body(inbox, f"local-work-{time.time_ns()}"),
            20,
        )
        task = task_record(created)
        task_id = task.get("task_id")
        if not isinstance(task_id, str) or not task_id:
            raise RuntimeError(f"task id missing: {created}")
        print(f"PASS  task admitted ({task_id})")

        request_json(
            f"{base_url}/api/v1/product/tasks/{task_id}/compile-and-schedule",
            "POST",
            None,
            30,
        )
        print("PASS  command graph scheduled")

        deadline = time.monotonic() + timeout
        status = ""
        version = 0
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError(f"engine exited with code {process.returncode}")
            current = request_json(f"{base_url}/api/v1/product/tasks/{task_id}", "GET", None, 10)
            task = task_record(current)
            status = str(task.get("status") or "")
            version = int(task.get("version") or 0)
            if status in {"failed", "killed", "blocked", "budget_exhausted", "outcome_unknown"}:
                raise RuntimeError(f"task stopped at {status}: {json.dumps(task)[:1200]}")
            if status == "awaiting_approval":
                break
            finalized = request_json(
                f"{base_url}/api/v1/product/tasks/{task_id}/finalize",
                "POST",
                None,
                30,
            )
            task = task_record(finalized)
            status = str(task.get("status") or status)
            version = int(task.get("version") or version)
            phase = nested_get(finalized, "result", "phase") or finalized.get("phase")
            if status == "awaiting_approval":
                break
            if phase == "execution_failed":
                raise RuntimeError(f"execution failed: {json.dumps(finalized)[:1200]}")
            time.sleep(0.4)
        else:
            raise RuntimeError(f"task did not reach approval; last status={status}")
        print("PASS  local command finished and verification passed")

        approval = request_json(
            f"{base_url}/api/v1/product/tasks/{task_id}/approve",
            "POST",
            {"expected_task_version": version},
            20,
        )
        approval_body = approval.get("approval") if isinstance(approval.get("approval"), dict) else approval
        approval_id = None
        if isinstance(approval_body, dict):
            approval_id = approval_body.get("approval_id") or nested_get(approval_body, "approval", "approval_id")
        if not isinstance(approval_id, str) or not approval_id:
            approval_id = nested_get(approval, "result", "approval_id")
        if not isinstance(approval_id, str) or not approval_id:
            raise RuntimeError(f"approval id missing: {json.dumps(approval)[:1200]}")
        refreshed = task_record(request_json(f"{base_url}/api/v1/product/tasks/{task_id}", "GET", None, 10))
        output_version = int(refreshed.get("version") or 0)
        request_json(
            f"{base_url}/api/v1/product/tasks/{task_id}/output",
            "POST",
            {
                "expected_task_version": output_version,
                "approval_id": approval_id,
                "confirm_output": True,
            },
            30,
        )
        written = inbox / RESULT_RELATIVE
        if not written.is_file():
            raise RuntimeError(f"expected file was not applied: {written}")
        text = written.read_text(encoding="utf-8")
        if text != EXPECTED_NOTE:
            raise RuntimeError(f"applied file has unexpected contents: {text!r}")
        print(f"PASS  wrote {written.relative_to(repo_root)}")
        print(f"OPEN  {base_url}")
        print(f"FOLDER  {inbox}")
        print("This job used the local command executor. It did not call a model or write this repository.")
        print("Stop with: ./scripts/start_local_work.sh --cleanup")
        (state / PID_FILE).write_text(str(process.pid), encoding="utf-8")
        (state / META_FILE).write_text(
            json.dumps(
                {
                    "pid": process.pid,
                    "base_url": base_url,
                    "task_id": task_id,
                    "folder": str(inbox),
                    "written_file": str(written),
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


def self_test() -> int:
    body = intake_body(Path("/tmp/acp-local-work-inbox"), "local-work-1")
    if body["output_intent"] != "apply_local_changes":
        raise SystemExit("self-test expected apply_local_changes")
    if body["executor_policy"]["prefer"] != "command":
        raise SystemExit("self-test expected the command executor")
    if body["confirm_execution"] is not True or body["confirm_output"] is not False:
        raise SystemExit("self-test expected execution confirmation only at intake")
    if body["allowed_paths"] != ["docs/product_golden_path_fixture.md"]:
        raise SystemExit("self-test expected the fixture document path")
    print("self-test ok")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Start the engine and complete one local folder task.")
    parser.add_argument("--repo-root", type=Path, default=repo_root_from_script())
    parser.add_argument("--engine-bin", type=Path)
    parser.add_argument("--dashboard-dir", type=Path)
    parser.add_argument("--timeout", type=float, default=90.0)
    parser.add_argument("--cleanup", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    repo_root = args.repo_root.resolve()
    if args.cleanup:
        stop_engine(repo_root / STATE_DIR_NAME)
        return 0
    if args.self_test:
        return self_test()
    dashboard = args.dashboard_dir or repo_root / "dashboard" / "out"
    return run_local_work(
        repo_root=repo_root,
        engine_bin=(args.engine_bin or engine_binary(repo_root)).resolve(),
        dashboard_dir=dashboard.resolve(),
        timeout=args.timeout,
    )


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except BrokenPipeError:
        raise SystemExit(0)
