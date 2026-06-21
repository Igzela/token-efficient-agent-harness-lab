#!/usr/bin/env python3
"""Run three real CLI-to-branch production pilots against disposable git repos."""

from __future__ import annotations

import json
import os
import shutil
import signal
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass(frozen=True)
class Scenario:
    name: str
    files: dict[str, str]
    prompt: str
    verification: str


SCENARIOS = (
    Scenario(
        name="python-calculator",
        files={
            ".gitignore": "__pycache__/\n",
            "calculator.py": "def add(a, b):\n    return a + b\n",
            "test_calculator.py": (
                "import unittest\n\n"
                "import calculator\n\n\n"
                "class CalculatorTests(unittest.TestCase):\n"
                "    def test_add(self):\n"
                "        self.assertEqual(calculator.add(2, 3), 5)\n\n\n"
                "if __name__ == '__main__':\n"
                "    unittest.main()\n"
            ),
        },
        prompt=(
            "在当前仓库实现 calculator.multiply(a, b)，并补充 unittest，覆盖正数、负数和零。"
            "运行测试确认通过。只修改当前仓库，不要 commit 或 push。"
        ),
        verification="python3 -m unittest",
    ),
    Scenario(
        name="rust-slugify",
        files={
            ".gitignore": "target/\n",
            "Cargo.toml": (
                "[package]\nname = \"pilot_slugify\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"
            ),
            "src/lib.rs": (
                "pub fn identity(value: &str) -> &str {\n    value\n}\n\n"
                "#[cfg(test)]\nmod tests {\n"
                "    use super::*;\n\n"
                "    #[test]\n    fn identity_keeps_input() {\n"
                "        assert_eq!(identity(\"hello\"), \"hello\");\n"
                "    }\n}\n"
            ),
        },
        prompt=(
            "Implement a public slugify(&str) -> String function in this Rust crate. "
            "Lowercase ASCII words, replace runs of non-alphanumeric characters with one hyphen, "
            "trim edge hyphens, and add focused unit tests. Run cargo test. "
            "Only modify this repository; do not commit or push."
        ),
        verification="cargo test",
    ),
    Scenario(
        name="node-title-case",
        files={
            "package.json": (
                "{\n  \"name\": \"pilot-title-case\",\n  \"type\": \"module\",\n"
                "  \"scripts\": {\"test\": \"node --test\"}\n}\n"
            ),
            "src/format.js": "export function lower(value) {\n  return value.toLowerCase();\n}\n",
            "test/format.test.js": (
                "import test from 'node:test';\n"
                "import assert from 'node:assert/strict';\n"
                "import { lower } from '../src/format.js';\n\n"
                "test('lower', () => assert.equal(lower('Hello'), 'hello'));\n"
            ),
        },
        prompt=(
            "Add and export a titleCase(value) function in src/format.js. It should collapse "
            "whitespace and capitalize each word while lowercasing the remaining letters. "
            "Add node:test coverage for mixed case and repeated whitespace, then run npm test. "
            "Only modify this repository; do not commit or push."
        ),
        verification="npm test",
    ),
)


class PilotError(RuntimeError):
    pass


class Api:
    def __init__(self, base_url: str) -> None:
        self.base_url = base_url

    def call(self, method: str, path: str, body: dict[str, Any] | None = None) -> dict[str, Any]:
        data = json.dumps(body).encode() if body is not None else None
        request = urllib.request.Request(
            f"{self.base_url}{path}",
            data=data,
            method=method,
            headers={"Content-Type": "application/json"},
        )
        try:
            with urllib.request.urlopen(request, timeout=900) as response:
                return json.loads(response.read())
        except urllib.error.HTTPError as error:
            detail = error.read().decode(errors="replace")
            raise PilotError(f"{method} {path} failed: HTTP {error.code}: {detail}") from error


def run(command: list[str], cwd: Path | None = None) -> str:
    result = subprocess.run(command, cwd=cwd, text=True, capture_output=True, check=False)
    if result.returncode != 0:
        raise PilotError(
            f"command failed: {' '.join(command)}\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result.stdout.strip()


def free_port() -> int:
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return int(listener.getsockname()[1])


def create_repo(root: Path, scenario: Scenario) -> tuple[Path, Path, str]:
    target = root / scenario.name
    remote = root / f"{scenario.name}.git"
    target.mkdir()
    for relative, content in scenario.files.items():
        path = target / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
    run(["git", "init", "-b", "main"], target)
    run(["git", "config", "user.name", "ACP Real Pilot"], target)
    run(["git", "config", "user.email", "pilot@example.invalid"], target)
    run(["git", "add", "-A"], target)
    run(["git", "commit", "-m", "base"], target)
    run(["git", "init", "--bare", str(remote)])
    run(["git", "symbolic-ref", "HEAD", "refs/heads/main"], remote)
    run(["git", "remote", "add", "origin", str(remote)], target)
    run(["git", "push", "-u", "origin", "main"], target)
    return target, remote, run(["git", "rev-parse", "HEAD"], target)


def wait_for_health(api: Api, process: subprocess.Popen[str]) -> None:
    for _ in range(180):
        if process.poll() is not None:
            raise PilotError("engine exited before becoming healthy")
        try:
            if api.call("GET", "/api/v1/health").get("status") == "healthy":
                return
        except (PilotError, urllib.error.URLError):
            pass
        time.sleep(1)
    raise PilotError("engine health timeout")


def required(payload: dict[str, Any], *path: str) -> Any:
    current: Any = payload
    for key in path:
        if not isinstance(current, dict) or key not in current:
            raise PilotError(f"missing {'.'.join(path)} in response")
        current = current[key]
    return current


def tick_to_terminal(api: Api, run_id: str) -> dict[str, Any]:
    for _ in range(12):
        tick = api.call(
            "POST",
            f"/api/v1/workflow-runs/{run_id}/tick",
            {"actor": "real-output-pilot", "executor": "claude_code_cli", "timeout_ms": 600000},
        )
        result = required(tick, "tick", "result")
        if result.get("status") == "failed":
            raise PilotError(f"CLI tick failed: {json.dumps(result, indent=2)}")
        run_detail = api.call("GET", f"/api/v1/workflow-runs/{run_id}")
        workflow_run = required(run_detail, "run")
        if workflow_run.get("status") == "completed":
            return workflow_run
    raise PilotError("workflow did not complete within 12 ticks")


def run_scenario(api: Api, root: Path, scenario: Scenario) -> dict[str, Any]:
    target, remote, source_revision = create_repo(root, scenario)
    main_before = run(["git", "rev-parse", "main"], target)
    plan = api.call(
        "POST",
        "/api/v1/plans",
        {"raw_request": scenario.prompt, "request_source": "real-output-pilot"},
    )
    plan_id = required(plan, "plan", "plan_id")
    workflow_run = api.call("POST", "/api/v1/workflow-runs", {"plan_id": plan_id})
    run_id = required(workflow_run, "run", "run_id")
    workspace_response = api.call(
        "POST",
        "/api/v1/supervised-patch/workspaces",
        {
            "run_id": run_id,
            "plan_id": plan_id,
            "target_id": scenario.name,
            "target_repo_path": str(target),
            "source_revision": source_revision,
            "workspace_mode": "git_worktree",
        },
    )
    workspace = required(workspace_response, "workspace")
    workspace_id = workspace["workspace_id"]
    tick_to_terminal(api, run_id)
    verification_response = api.call(
        "POST",
        f"/api/v1/supervised-patch/workspaces/{workspace_id}/verify",
        {
            "command": scenario.verification,
            "confirm_verification": True,
            "repair_executor": "claude_code_cli",
            "max_repair_attempts": 1,
            "timeout_ms": 600000,
        },
    )
    verification = required(verification_response, "verification")
    if verification.get("status") != "evidence_recorded":
        raise PilotError(f"{scenario.name} verification failed: {json.dumps(verification, indent=2)}")
    capture = api.call(
        "POST", f"/api/v1/supervised-patch/workspaces/{workspace_id}/capture"
    )
    artifact = required(capture, "artifact")
    artifact_id = artifact["artifact_id"]
    approval = api.call(
        "POST",
        f"/api/v1/workflow-runs/{run_id}/approvals",
        {
            "node_id": f"{scenario.name}-approval",
            "decision": "approved",
            "reason": "real output pilot verification passed",
            "bound_patch_hash": artifact["patch_hash"],
            "bound_source_revision": workspace["source_revision"],
            "bound_changed_files": artifact["changed_files"],
            "expires_at": "2099-12-31T23:59:59Z",
        },
    )
    branch_name = f"acp/pilot-{scenario.name}"
    pushed = api.call(
        "POST",
        f"/api/v1/supervised-patch/artifacts/{artifact_id}/output",
        {
            "run_id": run_id,
            "mode": "push_branch",
            "confirm_target_output": True,
            "branch_name": branch_name,
            "remote": "origin",
            "commit_message": f"feat: complete {scenario.name} pilot",
            "pr_title": f"Complete {scenario.name} pilot",
        },
    )
    output = required(pushed, "output")
    main_after = run(["git", "rev-parse", "main"], target)
    remote_commit = run(["git", "rev-parse", f"refs/heads/{branch_name}"], remote)
    if main_before != main_after:
        raise PilotError(f"{scenario.name} mutated main")
    if remote_commit != output["commit_sha"]:
        raise PilotError(f"{scenario.name} remote branch mismatch")
    return {
        "scenario": scenario.name,
        "target_repo": str(target),
        "remote": str(remote),
        "plan_id": plan_id,
        "run_id": run_id,
        "workspace_id": workspace_id,
        "artifact_id": artifact_id,
        "approval_id": required(approval, "approval", "approval_id"),
        "verification": verification,
        "changed_files": artifact["changed_files"],
        "patch_hash": artifact["patch_hash"],
        "branch": branch_name,
        "branch_commit": output["commit_sha"],
        "main_unchanged": True,
    }


def main() -> int:
    repo_root = Path(__file__).resolve().parents[1]
    run_root = Path(tempfile.mkdtemp(prefix="acp-real-output-pilots-"))
    port = free_port()
    base_url = f"http://127.0.0.1:{port}"
    claude = shutil.which("claude")
    if not claude:
        raise PilotError("claude CLI is not installed")
    env = os.environ.copy()
    env.update(
        {
            "HOST": "127.0.0.1",
            "PORT": str(port),
            "ACP_DB_PATH": str(run_root / "control-plane.db"),
            "ACP_DASHBOARD_DIR": str(repo_root / "dashboard" / "out"),
            "ACP_ENABLE_CLI_EXECUTION": "1",
            "ACP_CLAUDE_CODE_BIN": claude,
            "ACP_CLI_TIMEOUT_MS": "600000",
            "ACP_CLI_ENV_ALLOWLIST": "HOME,USER,LOGNAME,CLAUDE_CONFIG_DIR",
            "ACP_ENABLE_TARGET_REPO_OUTPUT": "1",
            "ACP_TARGET_REPO_ALLOW_LOCAL_REMOTE": "1",
            "ACP_TARGET_REPO_REMOTE_ALLOWLIST": "origin",
        }
    )
    log_path = run_root / "engine.log"
    with log_path.open("w", encoding="utf-8") as log:
        process = subprocess.Popen(
            [str(repo_root / "target" / "debug" / "agent-control-plane")],
            cwd=repo_root,
            env=env,
            stdout=log,
            stderr=subprocess.STDOUT,
            text=True,
            start_new_session=True,
        )
        try:
            api = Api(base_url)
            wait_for_health(api, process)
            results = []
            for scenario in SCENARIOS:
                print(f"=== {scenario.name} ===", flush=True)
                result = run_scenario(api, run_root, scenario)
                results.append(result)
                print(
                    f"PASS branch={result['branch']} files={','.join(result['changed_files'])}",
                    flush=True,
                )
            summary = {
                "schema_version": "real_output_pilots.v1",
                "base_url": base_url,
                "run_root": str(run_root),
                "engine_log": str(log_path),
                "executor": "claude_code_cli",
                "results": results,
            }
            summary_path = run_root / "summary.json"
            summary_path.write_text(
                json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
            )
            print(f"ALL PILOTS PASSED summary={summary_path}", flush=True)
            return 0
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait(timeout=10)


if __name__ == "__main__":
    raise SystemExit(main())
