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


ADMIN_KEY = "harness_" + ("5" * 64)


class PilotError(RuntimeError):
    pass


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def headers(api_key: str | None = ADMIN_KEY) -> dict[str, str]:
    return {} if api_key is None else {"authorization": f"Bearer {api_key}"}


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
    request = Request(f"{base_url}{path}", data=data, headers=req_headers, method=method)
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


def query_path(path: str, params: dict[str, object]) -> str:
    return f"{path}?{urlencode(params)}"


def wait_for_health(base_url: str, process: subprocess.Popen[str], timeout: float) -> None:
    deadline = time.monotonic() + timeout
    last_error: Exception | None = None
    while time.monotonic() < deadline:
        if process.poll() is not None:
            output = process.stdout.read() if process.stdout else ""
            raise PilotError(f"engine exited early with code {process.returncode}\n{output}")
        try:
            request_json("GET", base_url, "/api/v1/health")
            return
        except Exception as exc:  # noqa: BLE001 - report final startup failure.
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
        "ACP_ENABLE_CLI_EXECUTION",
        "ACP_CODEX_BIN",
        "ACP_CLAUDE_CODE_BIN",
        "ACP_CLI_TIMEOUT_MS",
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


def dispatch(base_url: str, raw_request: str) -> dict:
    return request_json(
        "POST",
        base_url,
        "/api/v1/dispatch",
        {"raw_request": raw_request, "request_source": "trial5"},
        timeout=10.0,
    )


def write_stub(path: Path, mode: str) -> None:
    if mode == "codex_ok":
        body = "printf '%s\\n' '{\"id\":\"codex-stub-1\",\"output\":\"codex ok\",\"usage\":{\"input_tokens\":11,\"output_tokens\":7}}'\n"
    elif mode == "claude_ok":
        body = "printf '%s\\n' '{\"id\":\"claude-stub-1\",\"result\":\"claude ok\",\"usage\":{\"input_tokens\":13,\"output_tokens\":5}}'\n"
    elif mode == "nonzero":
        body = "printf '%s\\n' 'trial5 boom' >&2\nexit 42\n"
    elif mode == "malformed":
        body = "printf '%s\\n' 'not-json'\n"
    elif mode == "timeout":
        body = "sleep 2\nprintf '%s\\n' '{\"output\":\"late\"}'\n"
    else:
        raise PilotError(f"unknown stub mode: {mode}")
    path.write_text("#!/usr/bin/env sh\n" + body, encoding="utf-8")
    path.chmod(0o755)


def executable_stub_dir(repo_root: Path, name: str) -> Path:
    root = repo_root / "target" / "script-fake-cli"
    root.mkdir(parents=True, exist_ok=True)
    return Path(tempfile.mkdtemp(prefix=f"{name}-", dir=root))


def assert_cli_gate(bundle: dict, executor_type: str) -> None:
    gates = bundle["decision"].get("execution_gates", [])
    assert_true(
        any(gate.get("gate_type") == "cli_execution" for gate in gates),
        f"{executor_type} cli gate",
    )


def assert_noop_bundle(bundle: dict, label: str) -> None:
    assert_eq(bundle["execution_result"]["executor_type"], "noop", f"{label} executor")
    assert_eq(bundle["execution_result"]["status"], "not_executed", f"{label} status")


def assert_cli_success(bundle: dict, executor_type: str, output: str, input_tokens: int) -> None:
    result = bundle["execution_result"]
    assert_eq(bundle["decision"]["selected_tier"], executor_type, f"{executor_type} selected tier")
    assert_eq(result["executor_type"], executor_type, f"{executor_type} result executor")
    assert_eq(result["status"], "cli_completed", f"{executor_type} status")
    assert_eq(result["output"], output, f"{executor_type} output")
    assert_eq(result["input_tokens"], input_tokens, f"{executor_type} input tokens")
    assert_true(result["estimated_cost"] > 0, f"{executor_type} estimated cost")
    assert_eq(bundle["record"]["final_status"], "completed", f"{executor_type} final status")
    assert_cli_gate(bundle, executor_type)


def assert_cli_failure(bundle: dict, executor_type: str, error_domain: str, label: str) -> None:
    result = bundle["execution_result"]
    assert_eq(result["executor_type"], executor_type, f"{label} executor")
    assert_eq(result["status"], "failed", f"{label} status")
    assert_eq(result["error_domain"], error_domain, f"{label} error domain")
    assert_eq(bundle["record"]["final_status"], "failed", f"{label} final status")
    assert_cli_gate(bundle, executor_type)


def run_typescript_sdk_smoke(repo_root: Path, base_url: str, dispatch_id: str) -> dict:
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
        const detail = await client.dispatchDetail({json.dumps(dispatch_id)});
        const costs = await client.costDetails({{ limit: 10 }});
        const provider = await client.providerHealth();
        const result = detail.dispatch.bundle.execution_result;
        if (result.executor_type !== "codex_cli") throw new Error(JSON.stringify(result));
        if (result.status !== "cli_completed") throw new Error(JSON.stringify(result));
        if (result.input_tokens !== 11) throw new Error(JSON.stringify(result));
        if (provider.status !== "noop") throw new Error(JSON.stringify(provider));
        if (!Array.isArray(costs.dispatches)) throw new Error("cost details missing array");
        console.log(JSON.stringify({{
          executor_type: result.executor_type,
          execution_status: result.status,
          input_tokens: result.input_tokens,
          provider_status: provider.status,
          cost_rows: costs.dispatches.length
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


def run_python_sdk_smoke(repo_root: Path, base_url: str, dispatch_id: str) -> dict:
    sys.path.insert(0, str(repo_root / "sdk/python/src"))
    from agent_control_plane_sdk import AgentControlPlaneClient  # type: ignore

    client = AgentControlPlaneClient(base_url, api_key=ADMIN_KEY)
    detail = client.dispatch_detail(dispatch_id)
    costs = client.cost_details(limit=10)
    provider = client.provider_health()
    result = detail["dispatch"]["bundle"]["execution_result"]
    assert_eq(result["executor_type"], "codex_cli", "Python SDK CLI executor")
    assert_eq(result["status"], "cli_completed", "Python SDK CLI status")
    assert_eq(result["input_tokens"], 11, "Python SDK CLI tokens")
    assert_eq(provider["status"], "noop", "Python SDK provider unconfigured")
    assert_true(isinstance(costs["dispatches"], list), "Python SDK cost detail rows")
    return {
        "status": "passed",
        "executor_type": result["executor_type"],
        "execution_status": result["status"],
        "input_tokens": result["input_tokens"],
        "provider_status": provider["status"],
        "cost_rows": len(costs["dispatches"]),
    }


def run_default_cli_discovery_flow(repo_root: Path, engine_bin: Path, dashboard_dir: Path, data_dir: Path) -> dict:
    bin_dir = executable_stub_dir(repo_root, "trial5-default-cli")
    write_stub(bin_dir / "codex", "codex_ok")
    write_stub(bin_dir / "claude", "claude_ok")
    process, base_url = start_engine(
        repo_root,
        engine_bin,
        data_dir,
        dashboard_dir,
        {"PATH": f"{bin_dir}:{os.environ.get('PATH', '')}"},
    )
    try:
        html = request_text(base_url, "/")
        assert_true("Agent Control Plane" in html, "static dashboard served")
        bundle = dispatch(base_url, "Trial 5 generate Rust function for default CLI discovery")
        assert_cli_success(bundle, "codex_cli", "codex ok", 11)
        provider = request_json("GET", base_url, "/api/v1/provider/health")
        assert_eq(provider["status"], "noop", "default provider health")
        return {
            "status": "passed",
            "dashboard_static": True,
            "executor_type": bundle["execution_result"]["executor_type"],
            "execution_status": bundle["execution_result"]["status"],
            "provider_status": provider["status"],
        }
    finally:
        stop_engine(process)


def run_missing_autodetect_flow(repo_root: Path, engine_bin: Path, dashboard_dir: Path, data_dir: Path) -> dict:
    empty_path = data_dir / "empty-path"
    empty_path.mkdir(parents=True, exist_ok=True)
    process, base_url = start_engine(
        repo_root,
        engine_bin,
        data_dir,
        dashboard_dir,
        {"ACP_ENABLE_CLI_EXECUTION": "1", "PATH": str(empty_path)},
    )
    try:
        bundle = dispatch(base_url, "Trial 5 generate Rust function with missing autodetect")
        assert_noop_bundle(bundle, "missing-autodetect dispatch")
    finally:
        output = stop_engine(process)
    assert_true("codex binary not found" in output, "missing autodetect codex diagnostic")
    assert_true("claude binary not found" in output, "missing autodetect claude diagnostic")
    return {"status": "passed", "diagnostics": "codex/claude binary not found", "executor_type": "noop"}


def run_stub_success_flow(repo_root: Path, engine_bin: Path, dashboard_dir: Path, data_dir: Path) -> dict:
    bin_dir = executable_stub_dir(repo_root, "trial5-success")
    codex = bin_dir / "codex"
    claude = bin_dir / "claude"
    write_stub(codex, "codex_ok")
    write_stub(claude, "claude_ok")
    process, base_url = start_engine(
        repo_root,
        engine_bin,
        data_dir,
        dashboard_dir,
        {
            "ACP_ENABLE_CLI_EXECUTION": "1",
            "ACP_CODEX_BIN": str(codex),
            "ACP_CLAUDE_CODE_BIN": str(claude),
            "ACP_CLI_TIMEOUT_MS": "5000",
        },
    )
    try:
        codex_bundle = dispatch(base_url, "Trial 5 generate a Rust function in module beta.rs")
        claude_bundle = dispatch(base_url, "Trial 5 architecture plan for system design boundaries")
        assert_cli_success(codex_bundle, "codex_cli", "codex ok", 11)
        assert_cli_success(claude_bundle, "claude_code_cli", "claude ok", 13)

        codex_id = codex_bundle["record"]["dispatch_id"]
        claude_id = claude_bundle["record"]["dispatch_id"]
        list_page = request_json("GET", base_url, query_path("/api/v1/dispatches", {"limit": 1, "offset": 0}))
        second_page = request_json("GET", base_url, query_path("/api/v1/dispatches", {"limit": 1, "offset": 1}))
        search = request_json("GET", base_url, query_path("/api/v1/dispatches", {"limit": 10, "search": "module beta"}))
        detail = request_json("GET", base_url, f"/api/v1/dispatches/{codex_id}")
        costs = request_json("GET", base_url, "/api/v1/costs")
        cost_details = request_json("GET", base_url, query_path("/api/v1/costs/dispatches", {"limit": 10}))
        audit = request_json("GET", base_url, query_path("/api/v1/audit", {"limit": 100}))
        provider_audit = request_json("GET", base_url, query_path("/api/v1/provider/audit", {"limit": 100}))
        dashboard = request_json("GET", base_url, "/api/v1/dashboard")

        assert_eq(len(list_page["dispatches"]), 1, "dispatch list first page")
        assert_eq(len(second_page["dispatches"]), 1, "dispatch list second page")
        assert_true(len(search["dispatches"]) >= 1, "dispatch search by CLI output")
        assert_eq(
            detail["dispatch"]["bundle"]["execution_result"]["executor_type"],
            "codex_cli",
            "dispatch detail CLI executor",
        )
        assert_true(costs["total_estimated_cost_usd"] > 0, "cost summary includes CLI estimate")
        assert_true(
            any(row["executor_type"] == "codex_cli" and row["input_tokens"] == 11 for row in cost_details["dispatches"]),
            "cost detail CLI token row",
        )
        assert_true(any(event["action"] == "dispatch.record" for event in audit["events"]), "dispatch audit event")
        assert_eq(provider_audit["events"], [], "provider audit remains empty")
        assert_eq(
            dashboard["boundaries"]["provider_transport"],
            "noop",
            "dashboard provider transport unconfigured",
        )

        ts = run_typescript_sdk_smoke(repo_root, base_url, codex_id)
        py = run_python_sdk_smoke(repo_root, base_url, codex_id)
        return {
            "status": "passed",
            "base_url": base_url,
            "codex_dispatch_id": codex_id,
            "claude_dispatch_id": claude_id,
            "search_matches": len(search["dispatches"]),
            "provider_audit_events": len(provider_audit["events"]),
            "dashboard_provider_transport": dashboard["boundaries"]["provider_transport"],
            "typescript_sdk": ts,
            "python_sdk": py,
        }
    finally:
        stop_engine(process)


def run_failure_case(
    repo_root: Path,
    engine_bin: Path,
    dashboard_dir: Path,
    data_dir: Path,
    stub_mode: str | None,
    expected_domain: str,
    timeout_ms: str = "5000",
) -> dict:
    bin_dir = executable_stub_dir(repo_root, f"trial5-{expected_domain}")
    codex = bin_dir / "codex"
    if stub_mode is None:
        codex = bin_dir / "missing-codex"
    else:
        write_stub(codex, stub_mode)
    process, base_url = start_engine(
        repo_root,
        engine_bin,
        data_dir,
        dashboard_dir,
        {
            "ACP_ENABLE_CLI_EXECUTION": "1",
            "ACP_CODEX_BIN": str(codex),
            "ACP_CLI_TIMEOUT_MS": timeout_ms,
        },
    )
    try:
        bundle = dispatch(base_url, f"Trial 5 generate Rust function failure path {expected_domain}")
        assert_cli_failure(bundle, "codex_cli", expected_domain, expected_domain)
        costs = request_json("GET", base_url, query_path("/api/v1/costs/dispatches", {"limit": 5}))
        row = next(row for row in costs["dispatches"] if row["dispatch_id"] == bundle["record"]["dispatch_id"])
        assert_eq(row["input_tokens"], 0, f"{expected_domain} cost input tokens safe")
        assert_eq(row["output_tokens"], 0, f"{expected_domain} cost output tokens safe")
        return {
            "status": "passed",
            "dispatch_id": bundle["record"]["dispatch_id"],
            "error_domain": bundle["execution_result"]["error_domain"],
            "final_status": bundle["record"]["final_status"],
        }
    finally:
        stop_engine(process)


def run_failure_flows(repo_root: Path, engine_bin: Path, dashboard_dir: Path, data_dir: Path) -> dict:
    return {
        "missing_binary": run_failure_case(
            repo_root,
            engine_bin,
            dashboard_dir,
            data_dir / "missing",
            None,
            "cli_not_found",
        ),
        "nonzero_exit": run_failure_case(
            repo_root,
            engine_bin,
            dashboard_dir,
            data_dir / "nonzero",
            "nonzero",
            "cli_execution_error",
        ),
        "malformed_output": run_failure_case(
            repo_root,
            engine_bin,
            dashboard_dir,
            data_dir / "malformed",
            "malformed",
            "cli_output_parse_error",
        ),
        "timeout": run_failure_case(
            repo_root,
            engine_bin,
            dashboard_dir,
            data_dir / "timeout",
            "timeout",
            "cli_timeout",
            timeout_ms="200",
        ),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Run Trial 5 CLI execution beta pilot flows.")
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--engine-bin", type=Path)
    parser.add_argument("--dashboard-dir", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()

    repo_root = args.repo_root.resolve()
    engine_bin = (args.engine_bin or repo_root / "target/debug/agent-control-plane").resolve()
    dashboard_dir = (args.dashboard_dir or repo_root / "dashboard/out").resolve()

    if not engine_bin.exists():
        raise SystemExit(f"engine binary not found: {engine_bin}; run cargo build -p engine first")
    if not (dashboard_dir / "index.html").exists():
        raise SystemExit(f"dashboard static index not found: {dashboard_dir / 'index.html'}")

    with tempfile.TemporaryDirectory(prefix="acp-trial5-") as tmp:
        data_root = Path(tmp)
        results = {
            "schema_version": "trial5_cli_execution_beta.v1",
            "real_local_discovery": {
                "codex": shutil.which("codex"),
                "claude": shutil.which("claude"),
                "note": "discovery only; deterministic beta uses stubs",
            },
            "default_cli_discovery": run_default_cli_discovery_flow(
                repo_root, engine_bin, dashboard_dir, data_root / "default-cli"
            ),
            "missing_autodetect": run_missing_autodetect_flow(
                repo_root, engine_bin, dashboard_dir, data_root / "missing-autodetect"
            ),
            "stub_success": run_stub_success_flow(repo_root, engine_bin, dashboard_dir, data_root / "success"),
            "failure_paths": run_failure_flows(repo_root, engine_bin, dashboard_dir, data_root / "failures"),
        }
        rendered = json.dumps(results, indent=2, sort_keys=True)
        if args.output:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(rendered + "\n", encoding="utf-8")
        print(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
