#!/usr/bin/env python3
"""SG-1 real dynamic CLI pilot matrix.

Requires a local engine with workflow/supervised-patch APIs. Real CLI executors
remain explicit opt-in via engine env (`ACP_ENABLE_CLI_EXECUTION=1`).
"""
from __future__ import annotations

import argparse
import json
import os
import shutil
import sys
import tempfile
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path


DEFAULT_BASE_URL = "http://127.0.0.1:8080"
EXECUTOR_BINS = {
    "claude_code_cli": ("ACP_CLAUDE_CODE_BIN", "claude"),
    "codex_cli": ("ACP_CODEX_BIN", "codex"),
}


@dataclass(frozen=True)
class TaskClass:
    name: str
    request: str
    files: dict[str, str]
    fix_command: str
    verify_command: str
    expected: dict[str, str]


TASK_CLASSES = [
    TaskClass(
        name="rust_module_fix",
        request="Add Rust greeting module and expose it from lib.rs.",
        files={
            "Cargo.toml": '[package]\nname = "sg1-rust"\nversion = "0.1.0"\nedition = "2021"\n',
            "src/lib.rs": "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
        },
        fix_command=(
            "Create src/greeting.rs containing `pub fn greet(name: &str) -> String` "
            "returning `format!(\"Hello, {}!\", name)`. Add `pub mod greeting;` to src/lib.rs. "
            "Modify only files under current directory."
        ),
        verify_command="Verify src/greeting.rs and src/lib.rs contain the requested Rust module. Do not modify files outside current directory.",
        expected={"src/greeting.rs": "pub fn greet", "src/lib.rs": "pub mod greeting"},
    ),
    TaskClass(
        name="docs_repair",
        request="Create an operations note with readiness checklist.",
        files={"README.md": "# SG1 Docs Target\n"},
        fix_command=(
            "Create docs/ops-note.md with three markdown bullets about backup, restore dry-run, "
            "and audit evidence. Modify only files under current directory."
        ),
        verify_command="Verify docs/ops-note.md exists and mentions backup, restore dry-run, and audit evidence. Do not modify files outside current directory.",
        expected={"docs/ops-note.md": "restore dry-run"},
    ),
    TaskClass(
        name="config_repair",
        request="Add local operations config defaults.",
        files={"config/local.toml": "[runtime]\nprofile = \"local\"\n"},
        fix_command=(
            "Update config/local.toml so it contains [runtime] profile local, [backup] enabled true, "
            "and [audit] redaction true. Modify only files under current directory."
        ),
        verify_command="Verify config/local.toml includes backup enabled true and audit redaction true. Do not modify files outside current directory.",
        expected={"config/local.toml": "redaction"},
    ),
]


class ApiClient:
    def __init__(self, base_url: str, token: str | None = None):
        self.base_url = base_url.rstrip("/")
        self.token = token

    def call(self, method: str, path: str, body: dict | None = None) -> tuple[int, dict | str]:
        req = urllib.request.Request(
            f"{self.base_url}{path}",
            data=json.dumps(body).encode() if body is not None else None,
            method=method,
        )
        req.add_header("Content-Type", "application/json")
        if self.token:
            req.add_header("Authorization", f"Bearer {self.token}")
        try:
            with urllib.request.urlopen(req, timeout=180) as resp:
                raw = resp.read().decode()
                return resp.status, json.loads(raw) if raw else {}
        except urllib.error.HTTPError as exc:
            raw = exc.read().decode()
            try:
                return exc.code, json.loads(raw)
            except json.JSONDecodeError:
                return exc.code, raw
        except Exception as exc:
            return 0, str(exc)

    def wait_for_health(self, timeout: float) -> bool:
        start = time.monotonic()
        while time.monotonic() - start < timeout:
            status, body = self.call("GET", "/api/v1/health")
            if status == 200 and isinstance(body, dict):
                return True
            time.sleep(0.5)
        return False


def executor_probe(executor: str) -> dict:
    env_key, default_bin = EXECUTOR_BINS[executor]
    configured = os.environ.get(env_key)
    resolved = configured or shutil.which(default_bin)
    enabled = os.environ.get("ACP_ENABLE_CLI_EXECUTION", "").lower() in {"1", "true", "yes", "on"}
    return {
        "executor": executor,
        "env_key": env_key,
        "default_binary": default_bin,
        "configured_binary": configured,
        "resolved_binary": resolved,
        "cli_execution_env_enabled": enabled,
        "available": bool(resolved) and enabled,
    }


def write_target(task: TaskClass) -> str:
    target_dir = tempfile.mkdtemp(prefix=f"sg1-{task.name}-")
    for rel, content in task.files.items():
        path = Path(target_dir) / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
    return target_dir


def require_id(body: dict | str, wrapper: str, key: str) -> str | None:
    if not isinstance(body, dict):
        return None
    value = body.get(wrapper)
    if not isinstance(value, dict):
        return None
    got = value.get(key)
    return got if isinstance(got, str) and got else None


def run_task(client: ApiClient, executor: str, task: TaskClass, timeout_ms: int) -> dict:
    started = time.monotonic()
    target_dir = write_target(task)
    workspace_id = None
    run_id = None
    artifact_id = None
    evidence: dict = {"target_dir": target_dir}
    try:
        status, plan_body = client.call(
            "POST",
            "/api/v1/plans",
            {"raw_request": task.request, "request_source": "sg1_dynamic_cli_matrix"},
        )
        plan_id = require_id(plan_body, "plan", "plan_id")
        if status not in (200, 201) or not plan_id:
            return fail_result(executor, task, "create_plan", status, plan_body, evidence, started)

        status, run_body = client.call("POST", "/api/v1/workflow-runs", {"plan_id": plan_id, "actor": "sg1"})
        run_id = require_id(run_body, "run", "run_id")
        if status not in (200, 201) or not run_id:
            return fail_result(executor, task, "create_run", status, run_body, evidence, started)
        evidence["run_id"] = run_id

        status, ws_body = client.call(
            "POST",
            "/api/v1/supervised-patch/workspaces",
            {
                "run_id": run_id,
                "target_id": f"sg1-{task.name}",
                "target_repo_path": target_dir,
                "source_revision": "sg1-source",
            },
        )
        workspace_id = require_id(ws_body, "workspace", "workspace_id")
        if status not in (200, 201) or not workspace_id:
            return fail_result(executor, task, "create_workspace", status, ws_body, evidence, started)
        evidence["workspace_id"] = workspace_id

        status, fail_body = client.call("POST", f"/api/v1/workflow-runs/{run_id}/tick", {"executor": "fail", "actor": "sg1"})
        if status != 200 or not tick_result_status(fail_body, "failed"):
            return fail_result(executor, task, "seed_failure", status, fail_body, evidence, started)
        evidence["failure_seeded"] = True

        status, dynamic_body = client.call(
            "POST",
            f"/api/v1/workflow-runs/{run_id}/tick",
            {"executor": "dynamic", "actor": "sg1"},
        )
        graph_mutated = status == 200 and dynamic_graph_mutated(dynamic_body)
        evidence["dynamic_tick"] = dynamic_body if isinstance(dynamic_body, dict) else {"raw": dynamic_body}
        if not graph_mutated:
            return fail_result(executor, task, "graph_mutation", status, dynamic_body, evidence, started)

        fix = client.call(
            "POST",
            f"/api/v1/workflow-runs/{run_id}/tick",
            {"executor": executor, "actor": "sg1", "command": task.fix_command, "timeout_ms": timeout_ms},
        )
        verify = client.call(
            "POST",
            f"/api/v1/workflow-runs/{run_id}/tick",
            {"executor": executor, "actor": "sg1", "command": task.verify_command, "timeout_ms": timeout_ms},
        )
        evidence["fix_tick_status"] = fix[0]
        evidence["verify_tick_status"] = verify[0]

        status, ws_detail = client.call("GET", f"/api/v1/supervised-patch/workspaces/{workspace_id}")
        workspace_path = workspace_path_from(ws_detail)
        evidence["workspace_path"] = workspace_path
        missing = verify_expected_files(workspace_path, task.expected)
        if missing:
            return fail_result(executor, task, "workspace_verification", status, {"missing": missing}, evidence, started)

        status, capture_body = client.call("POST", f"/api/v1/supervised-patch/workspaces/{workspace_id}/capture")
        artifact_id = require_id(capture_body, "artifact", "artifact_id")
        artifact = capture_body.get("artifact", {}) if isinstance(capture_body, dict) else {}
        changed_files = artifact.get("changed_files", []) if isinstance(artifact, dict) else []
        patch_hash = artifact.get("patch_hash") if isinstance(artifact, dict) else None
        if status != 200 or not artifact_id or not patch_hash:
            return fail_result(executor, task, "capture_patch", status, capture_body, evidence, started)
        evidence.update({"artifact_id": artifact_id, "changed_files": changed_files, "patch_hash": patch_hash})

        status, approval_body = client.call(
            "POST",
            f"/api/v1/workflow-runs/{run_id}/approvals",
            {
                "node_id": "node-a",
                "decision": "approved",
                "reason": "SG-1 evidence-bound export approval",
                "bound_patch_hash": patch_hash,
                "bound_source_revision": "sg1-source",
                "bound_changed_files": changed_files,
                "expires_at": "2099-12-31T23:59:59Z",
            },
        )
        if status != 200:
            return fail_result(executor, task, "approval_binding", status, approval_body, evidence, started)

        status, export_body = client.call("POST", f"/api/v1/supervised-patch/artifacts/{artifact_id}/export", {"run_id": run_id})
        export_ok = status == 200 and isinstance(export_body, dict) and bool(export_body.get("export"))
        evidence["export"] = export_body if isinstance(export_body, dict) else {"raw": export_body}
        if not export_ok:
            return fail_result(executor, task, "evidence_export", status, export_body, evidence, started)

        return {
            "executor": executor,
            "task_class": task.name,
            "status": "PASS",
            "duration_seconds": round(time.monotonic() - started, 2),
            "evidence": evidence,
        }
    finally:
        if workspace_id:
            client.call("POST", f"/api/v1/supervised-patch/workspaces/{workspace_id}/cleanup")
        shutil.rmtree(target_dir, ignore_errors=True)


def fail_result(executor: str, task: TaskClass, step: str, status: int, body: dict | str, evidence: dict, started: float) -> dict:
    return {
        "executor": executor,
        "task_class": task.name,
        "status": "FAIL",
        "failed_step": step,
        "http_status": status,
        "body": body,
        "duration_seconds": round(time.monotonic() - started, 2),
        "evidence": evidence,
    }


def tick_result_status(body: dict | str, expected: str) -> bool:
    if not isinstance(body, dict):
        return False
    tick = body.get("tick") if isinstance(body.get("tick"), dict) else body
    result = tick.get("result") if isinstance(tick.get("result"), dict) else {}
    return result.get("status") == expected


def dynamic_graph_mutated(body: dict | str) -> bool:
    if not isinstance(body, dict):
        return False
    tick = body.get("tick") if isinstance(body.get("tick"), dict) else {}
    actions = tick.get("actions") if isinstance(tick.get("actions"), list) else []
    return tick.get("mutations_applied", 0) > 0 and any(a.get("type") == "graph_mutated" for a in actions if isinstance(a, dict))


def workspace_path_from(body: dict | str) -> str | None:
    if not isinstance(body, dict):
        return None
    workspace = body.get("workspace") if isinstance(body.get("workspace"), dict) else {}
    value = workspace.get("workspace_path")
    return value if isinstance(value, str) else None


def verify_expected_files(workspace_path: str | None, expected: dict[str, str]) -> list[str]:
    if not workspace_path:
        return list(expected)
    missing = []
    for rel, needle in expected.items():
        path = Path(workspace_path) / rel
        if not path.exists() or needle not in path.read_text(errors="ignore"):
            missing.append(rel)
    return missing


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run SG-1 dynamic CLI pilot matrix.")
    parser.add_argument("--base-url", default=DEFAULT_BASE_URL)
    parser.add_argument("--executor", action="append", choices=sorted(EXECUTOR_BINS), dest="executors")
    parser.add_argument("--task-class", action="append", choices=[t.name for t in TASK_CLASSES], dest="task_classes")
    parser.add_argument("--token", default=os.environ.get("ACP_ADMIN_API_KEY"))
    parser.add_argument("--timeout-ms", type=int, default=120_000)
    parser.add_argument("--health-timeout", type=float, default=30.0)
    parser.add_argument("--force-unavailable", action="store_true")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    client = ApiClient(args.base_url, args.token)
    executors = args.executors or list(EXECUTOR_BINS)
    selected_tasks = [t for t in TASK_CLASSES if not args.task_classes or t.name in args.task_classes]
    probes = [executor_probe(e) for e in executors]
    report = {
        "phase": "SG-1",
        "name": "real_dynamic_cli_pilot_matrix",
        "base_url": args.base_url,
        "task_classes": [t.name for t in selected_tasks],
        "executor_probes": probes,
        "results": [],
        "skipped": [],
    }

    if not client.wait_for_health(args.health_timeout):
        report["status"] = "FAIL"
        report["failure"] = "engine_not_healthy"
        print(json.dumps(report, indent=2))
        return 1

    for probe in probes:
        executor = probe["executor"]
        if not probe["available"] and not args.force_unavailable:
            reason = (
                "cli_execution_gate_disabled"
                if probe["resolved_binary"]
                else "executor_binary_not_found"
            )
            report["skipped"].append({"executor": executor, "reason": reason, "evidence": probe})
            continue
        for task in selected_tasks:
            report["results"].append(run_task(client, executor, task, args.timeout_ms))

    passes = [r for r in report["results"] if r["status"] == "PASS"]
    failures = [r for r in report["results"] if r["status"] == "FAIL"]
    if failures:
        report["status"] = "FAIL"
    elif passes:
        report["status"] = "PASS"
    else:
        report["status"] = "SKIP"
    report["pass_count"] = len(passes)
    report["fail_count"] = len(failures)
    report["skip_count"] = len(report["skipped"])
    print(json.dumps(report, indent=2))
    return 0 if report["status"] in {"PASS", "SKIP"} else 1


if __name__ == "__main__":
    sys.exit(main())
