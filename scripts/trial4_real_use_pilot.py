#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import textwrap
import time
from pathlib import Path
from urllib.error import HTTPError
from urllib.parse import urlencode
from urllib.request import Request, urlopen


ADMIN_KEY = "harness_" + ("1" * 64)


class PilotError(RuntimeError):
    pass


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def headers(api_key: str | None = ADMIN_KEY) -> dict[str, str]:
    if api_key is None:
        return {}
    return {"authorization": f"Bearer {api_key}"}


def request_json(
    method: str,
    base_url: str,
    path: str,
    body: dict | None = None,
    api_key: str | None = ADMIN_KEY,
    timeout: float = 5.0,
) -> dict:
    data = None if body is None else json.dumps(body).encode("utf-8")
    req_headers = headers(api_key)
    if body is not None:
        req_headers["content-type"] = "application/json"
    request = Request(
        f"{base_url}{path}",
        data=data,
        headers=req_headers,
        method=method,
    )
    try:
        with urlopen(request, timeout=timeout) as response:
            payload = response.read().decode("utf-8")
    except HTTPError as exc:
        payload = exc.read().decode("utf-8")
        try:
            detail = json.loads(payload)
        except json.JSONDecodeError:
            detail = payload
        raise PilotError(f"{method} {path} failed with {exc.code}: {detail}") from exc
    return json.loads(payload) if payload else {}


def request_text(base_url: str, path: str, api_key: str | None = ADMIN_KEY) -> str:
    request = Request(f"{base_url}{path}", headers=headers(api_key), method="GET")
    with urlopen(request, timeout=5.0) as response:
        return response.read().decode("utf-8")


def wait_for_health(base_url: str, process: subprocess.Popen[str], timeout: float) -> dict:
    deadline = time.monotonic() + timeout
    last_error: Exception | None = None
    while time.monotonic() < deadline:
        if process.poll() is not None:
            output = process.stdout.read() if process.stdout else ""
            raise PilotError(f"engine exited early with code {process.returncode}\n{output}")
        try:
            return request_json("GET", base_url, "/api/v1/health")
        except Exception as exc:  # noqa: BLE001 - surface the last startup error.
            last_error = exc
            time.sleep(0.2)
    raise PilotError(f"engine did not become healthy: {last_error}")


def start_engine(
    repo_root: Path,
    engine_bin: Path,
    data_dir: Path,
    dashboard_dir: Path,
    extra_env: dict[str, str] | None = None,
) -> tuple[subprocess.Popen[str], str]:
    data_dir.mkdir(parents=True, exist_ok=True)
    port = free_port()
    env = {
        **os.environ,
        "HOST": "127.0.0.1",
        "PORT": str(port),
        "ACP_REQUIRE_AUTH": "1",
        "ACP_ADMIN_API_KEY": ADMIN_KEY,
        "ACP_DB_PATH": str(data_dir / "local-team.db"),
        "ACP_BACKUP_DIR": str(data_dir / "backups"),
        "ACP_DASHBOARD_DIR": str(dashboard_dir),
    }
    for key in [
        "ACP_PROVIDER_TYPE",
        "ACP_ENABLE_PROVIDER_EXECUTION",
        "ACP_API_KEY",
        "ACP_MODEL",
        "ACP_BASE_URL",
    ]:
        env.pop(key, None)
    if extra_env:
        env.update(extra_env)

    process = subprocess.Popen(
        [str(engine_bin)],
        cwd=repo_root,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    base_url = f"http://127.0.0.1:{port}"
    wait_for_health(base_url, process, 20.0)
    return process, base_url


def stop_engine(process: subprocess.Popen[str]) -> str:
    process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)
    return process.stdout.read() if process.stdout else ""


def assert_eq(actual: object, expected: object, label: str) -> None:
    if actual != expected:
        raise PilotError(f"{label}: expected {expected!r}, got {actual!r}")


def assert_true(value: object, label: str) -> None:
    if not value:
        raise PilotError(f"{label}: expected truthy value, got {value!r}")


def query_path(path: str, params: dict[str, object]) -> str:
    return f"{path}?{urlencode(params)}"


def run_typescript_sdk_smoke(repo_root: Path, base_url: str) -> dict:
    node = shutil.which("node")
    if node is None:
        return {"status": "skipped", "reason": "node unavailable"}
    sdk_root = repo_root / "sdk/typescript"
    sdk_entry = sdk_root / "dist/index.js"
    if not sdk_entry.exists():
        tsc = sdk_root / "node_modules/.bin/tsc"
        if not tsc.exists():
            return {"status": "skipped", "reason": "TypeScript SDK dist and local tsc unavailable"}
        build = subprocess.run(
            [str(tsc), "-p", "tsconfig.json"],
            cwd=sdk_root,
            text=True,
            capture_output=True,
            check=False,
            timeout=30,
        )
        if build.returncode != 0:
            raise PilotError(f"TypeScript SDK build failed:\nstdout={build.stdout}\nstderr={build.stderr}")
    script = f"""
        import {{ AgentControlPlaneClient }} from {json.dumps(str(sdk_entry))};
        const client = new AgentControlPlaneClient({{ baseUrl: {json.dumps(base_url)}, apiKey: {json.dumps(ADMIN_KEY)} }});
        const health = await client.health();
        const dispatches = await client.dispatches({{ limit: 2, search: "Trial 4" }});
        const audit = await client.audit({{ limit: 2 }});
        const provider = await client.providerHealth();
        if (health.status !== "healthy") throw new Error(`unexpected health ${{JSON.stringify(health)}}`);
        if (!Array.isArray(dispatches.dispatches)) throw new Error("dispatches missing array");
        if (!Array.isArray(audit.events)) throw new Error("audit missing events array");
        if (provider.status !== "noop") throw new Error(`provider should be noop/default-off: ${{JSON.stringify(provider)}}`);
        console.log(JSON.stringify({{
          health: health.status,
          dispatch_count: dispatches.dispatches.length,
          audit_count: audit.events.length,
          provider_status: provider.status
        }}));
    """
    result = subprocess.run(
        [node, "--input-type=module", "-e", script],
        cwd=repo_root,
        text=True,
        capture_output=True,
        check=False,
        timeout=20,
    )
    if result.returncode != 0:
        raise PilotError(f"TypeScript SDK smoke failed:\nstdout={result.stdout}\nstderr={result.stderr}")
    return {"status": "passed", **json.loads(result.stdout)}


def run_python_sdk_smoke(repo_root: Path, base_url: str) -> dict:
    sys.path.insert(0, str(repo_root / "sdk/python/src"))
    from agent_control_plane_sdk import AgentControlPlaneClient  # type: ignore

    client = AgentControlPlaneClient(base_url, api_key=ADMIN_KEY)
    health = client.health()
    dispatches = client.dispatches(limit=2, search="Trial 4")
    audit = client.audit(limit=2)
    provider = client.provider_health()
    assert_eq(health["status"], "healthy", "Python SDK health")
    assert_true(isinstance(dispatches["dispatches"], list), "Python SDK dispatch list")
    assert_true(isinstance(audit["events"], list), "Python SDK audit list")
    assert_eq(provider["status"], "noop", "Python SDK provider health default-off")
    return {
        "status": "passed",
        "health": health["status"],
        "dispatch_count": len(dispatches["dispatches"]),
        "audit_count": len(audit["events"]),
        "provider_status": provider["status"],
    }


def write_fake_cli(path: Path, output_field: str) -> None:
    path.write_text(
        textwrap.dedent(
            f"""\
            #!/usr/bin/env sh
            printf '%s\\n' '{{"{output_field}":"trial4 cli smoke","usage":{{"input_tokens":1,"output_tokens":1}}}}'
            """
        ),
        encoding="utf-8",
    )
    path.chmod(0o755)


def run_cli_smoke(repo_root: Path, engine_bin: Path, dashboard_dir: Path, data_dir: Path) -> dict:
    codex_bin = shutil.which("codex")
    claude_bin = shutil.which("claude")
    if codex_bin is None and claude_bin is None:
        return {"status": "skipped", "reason": "no local codex or claude binary found"}

    data_dir.mkdir(parents=True, exist_ok=True)
    fake_codex = data_dir / "fake-codex"
    fake_claude = data_dir / "fake-claude"
    write_fake_cli(fake_codex, "output")
    write_fake_cli(fake_claude, "result")

    process, base_url = start_engine(
        repo_root,
        engine_bin,
        data_dir,
        dashboard_dir,
        {
            "ACP_ENABLE_CLI_EXECUTION": "1",
            "ACP_CODEX_BIN": str(fake_codex),
            "ACP_CLAUDE_CODE_BIN": str(fake_claude),
            "ACP_CLI_TIMEOUT_MS": "5000",
        },
    )
    try:
        dispatch = request_json(
            "POST",
            base_url,
            "/api/v1/dispatch",
            {
                "raw_request": "Trial 4 code generate a tiny helper function",
                "request_source": "api",
            },
        )
        executor_type = dispatch["execution_result"]["executor_type"]
        status = dispatch["execution_result"]["status"]
        assert_true(
            executor_type in {"codex_cli", "claude_code_cli", "noop"},
            f"CLI smoke executor type {executor_type}",
        )
        if executor_type != "noop":
            assert_eq(status, "cli_completed", "CLI smoke status")
        return {
            "status": "passed",
            "detected_codex": codex_bin,
            "detected_claude": claude_bin,
            "used_stub_bins": True,
            "executor_type": executor_type,
            "execution_status": status,
        }
    finally:
        stop_engine(process)


def run_api_pilot(repo_root: Path, engine_bin: Path, dashboard_dir: Path, data_dir: Path) -> dict:
    process, base_url = start_engine(repo_root, engine_bin, data_dir, dashboard_dir)
    try:
        results: dict[str, object] = {"base_url": base_url}

        health = request_json("GET", base_url, "/api/v1/health")
        ready = request_json("GET", base_url, "/api/v1/ready")
        html = request_text(base_url, "/")
        dashboard = request_json("GET", base_url, "/api/v1/dashboard")
        assert_eq(health["status"], "healthy", "health status")
        assert_eq(ready["status"], "ready", "ready status")
        assert_true("Agent Control Plane" in html, "static dashboard title")
        assert_eq(dashboard["schema_version"], "local_dashboard.v1", "dashboard schema")
        results["startup_dashboard"] = "passed"

        created_key = request_json(
            "POST",
            base_url,
            "/api/v1/keys",
            {
                "user_id": "trial4-user",
                "role": "readonly",
                "scopes": ["health:read", "dispatch:read"],
            },
        )
        key_id = created_key["key_id"]
        assert_true(created_key.get("raw_key"), "created API key raw key")
        keys_before_revoke = request_json("GET", base_url, "/api/v1/keys")
        assert_true(
            any(row["key_id"] == key_id for row in keys_before_revoke["keys"]),
            "created key appears in list",
        )
        revoked = request_json("POST", base_url, f"/api/v1/keys/{key_id}/revoke", {})
        assert_eq(revoked["ok"], True, "revoke API key")
        keys_after_revoke = request_json("GET", base_url, "/api/v1/keys")
        revoked_row = next(row for row in keys_after_revoke["keys"] if row["key_id"] == key_id)
        assert_true(revoked_row.get("revoked_at") is not None, "revoked key metadata")
        results["api_keys"] = {"created": key_id, "revoked": True}

        requests = [
            "Trial 4 alpha dispatch default noop docs review",
            "Trial 4 beta dispatch default noop routing review",
            "Trial 4 gamma dispatch default noop audit review",
        ]
        dispatch_ids: list[str] = []
        for raw_request in requests:
            bundle = request_json(
                "POST",
                base_url,
                "/api/v1/dispatch",
                {"raw_request": raw_request, "request_source": "api"},
            )
            assert_eq(bundle["execution_result"]["executor_type"], "noop", "default dispatch executor")
            assert_eq(bundle["execution_result"]["status"], "not_executed", "default dispatch status")
            dispatch_ids.append(bundle["record"]["dispatch_id"])
        results["noop_dispatches"] = dispatch_ids

        page_one = request_json("GET", base_url, query_path("/api/v1/dispatches", {"limit": 2, "offset": 0}))
        page_two = request_json("GET", base_url, query_path("/api/v1/dispatches", {"limit": 2, "offset": 2}))
        search = request_json("GET", base_url, query_path("/api/v1/dispatches", {"limit": 10, "search": "beta"}))
        detail = request_json("GET", base_url, f"/api/v1/dispatches/{dispatch_ids[1]}")
        assert_eq(len(page_one["dispatches"]), 2, "dispatch page one length")
        assert_eq(len(page_two["dispatches"]), 1, "dispatch page two length")
        assert_eq(len(search["dispatches"]), 1, "dispatch search length")
        assert_true("beta" in search["dispatches"][0]["raw_request"].lower(), "dispatch search match")
        assert_eq(detail["dispatch"]["dispatch_id"], dispatch_ids[1], "dispatch detail")
        assert_eq(
            detail["dispatch"]["bundle"]["record"]["dispatch_id"],
            dispatch_ids[1],
            "dispatch detail bundle",
        )
        results["dispatch_list_detail_search_pagination"] = "passed"

        audit_page = request_json("GET", base_url, query_path("/api/v1/audit", {"limit": 100, "offset": 0}))
        assert_true(len(audit_page["events"]) >= 5, "audit event count")
        assert_true(
            any(event["action"] == "team.key.revoked" for event in audit_page["events"]),
            "audit contains API key revoke",
        )
        results["audit"] = {"events_checked": len(audit_page["events"])}

        backup = request_json(
            "POST",
            base_url,
            "/api/v1/backups",
            {"label": "trial4", "confirm_local_backup": True},
        )
        backup_id = backup["backup"]["backup_id"]
        backups = request_json("GET", base_url, "/api/v1/backups")
        assert_true(any(row["backup_id"] == backup_id for row in backups["backups"]), "backup list")
        request_json(
            "POST",
            base_url,
            "/api/v1/dispatch",
            {"raw_request": "Trial 4 post-backup dispatch before restore", "request_source": "api"},
        )
        restore = request_json(
            "POST",
            base_url,
            f"/api/v1/backups/{backup_id}/restore",
            {"confirm_restore": True},
        )
        assert_eq(restore["restore"]["success"], True, "backup restore success")
        deleted = request_json("DELETE", base_url, f"/api/v1/backups/{backup_id}")
        assert_eq(deleted["ok"], True, "backup delete")
        results["backup"] = {"backup_id": backup_id, "restored": True, "deleted": True}

        export_state = request_json("GET", base_url, "/api/v1/export")
        assert_eq(export_state["schema_version"], "local_team_export.v1", "export schema")
        imported = request_json(
            "POST",
            base_url,
            "/api/v1/import",
            {"snapshot": export_state, "confirm_import": True},
        )
        assert_true("imported" in imported, "import result")
        results["export_import"] = imported["imported"]

        provider = request_json("GET", base_url, "/api/v1/provider/health")
        assert_eq(provider["status"], "noop", "provider health default-off")
        results["provider_health"] = provider

        results["typescript_sdk"] = run_typescript_sdk_smoke(repo_root, base_url)
        results["python_sdk"] = run_python_sdk_smoke(repo_root, base_url)
        return results
    finally:
        stop_engine(process)


def main() -> int:
    parser = argparse.ArgumentParser(description="Run Trial 4 real-use pilot flows.")
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--engine-bin", type=Path)
    parser.add_argument("--dashboard-dir", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()

    repo_root = args.repo_root.resolve()
    engine_bin = (args.engine_bin or repo_root / "target/debug/engine").resolve()
    dashboard_dir = (args.dashboard_dir or repo_root / "dashboard/out").resolve()
    output = args.output

    if not engine_bin.exists():
        raise SystemExit(f"engine binary not found: {engine_bin}; run cargo build -p engine first")
    if not (dashboard_dir / "index.html").exists():
        raise SystemExit(f"dashboard static index not found: {dashboard_dir / 'index.html'}")

    with tempfile.TemporaryDirectory(prefix="acp-trial4-") as tmp:
        data_root = Path(tmp)
        api_results = run_api_pilot(repo_root, engine_bin, dashboard_dir, data_root / "api")
        cli_results = run_cli_smoke(repo_root, engine_bin, dashboard_dir, data_root / "cli")
        results = {
            "schema_version": "trial4_real_use_pilot.v1",
            "api_results": api_results,
            "cli_results": cli_results,
        }
        rendered = json.dumps(results, indent=2, sort_keys=True)
        if output:
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_text(rendered + "\n", encoding="utf-8")
        print(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
