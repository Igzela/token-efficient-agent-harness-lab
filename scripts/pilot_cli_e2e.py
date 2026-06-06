#!/usr/bin/env python3
"""Real CLI pilot: supervised autonomous E2E with Claude Code CLI executor.

The engine spawns a real claude CLI process to produce file changes.
Verifies the full lifecycle: plan → run → workspace → CLI tick → capture → approval → export.

Usage:
    uv run --no-project python scripts/pilot_cli_e2e.py [--base-url URL] [--executor claude_code_cli|codex_cli]
"""
import json
import os
import shutil
import sys
import tempfile
import time
import urllib.request

DEFAULT_BASE_URL = "http://127.0.0.1:8080"


class ApiClient:
    def __init__(self, base_url: str):
        self.base_url = base_url

    def call(self, method, path, body=None):
        url = f"{self.base_url}{path}"
        data = json.dumps(body).encode() if body else None
        req = urllib.request.Request(url, data=data, method=method)
        req.add_header("Content-Type", "application/json")
        try:
            with urllib.request.urlopen(req, timeout=120) as resp:
                return json.loads(resp.read())
        except urllib.error.HTTPError as e:
            error_body = e.read().decode()
            print(f"  HTTP {e.code}: {error_body}")
            return {"error": e.code, "body": error_body}

    def wait_for_health(self, timeout=30):
        start = time.time()
        while time.time() - start < timeout:
            try:
                result = self.call("GET", "/api/v1/health")
                if result.get("status") == "healthy":
                    return True
            except Exception:
                pass
            time.sleep(1)
        return False


def main():
    base_url = DEFAULT_BASE_URL
    executor_type = "claude_code_cli"

    args = sys.argv[1:]
    i = 0
    while i < len(args):
        if args[i] == "--base-url" and i + 1 < len(args):
            base_url = args[i + 1]
            i += 2
        elif args[i] == "--executor" and i + 1 < len(args):
            executor_type = args[i + 1]
            i += 2
        else:
            i += 1

    api = ApiClient(base_url)

    print(f"=== Real CLI Pilot E2E ({executor_type}) ===")
    print(f"Base URL: {base_url}")
    print()

    # Step 0: Health check
    print("[0] Checking engine health...")
    if not api.wait_for_health():
        print("FAIL: Engine not healthy")
        return 1
    print("  OK: Engine healthy")
    print()

    # Step 1: Create a real target directory
    print("[1] Creating target directory...")
    target_dir = tempfile.mkdtemp(prefix="cli-pilot-target-")
    for name, content in [
        ("README.md", "# CLI Pilot Target\n"),
        ("src/main.rs", 'fn main() {\n    println!("hello");\n}\n'),
        ("src/lib.rs", "pub fn add(a: i32, b: i32) -> i32 { a + b }\n"),
        ("Cargo.toml", "[package]\nname = \"cli-pilot-target\"\nversion = \"0.1.0\"\n"),
    ]:
        path = os.path.join(target_dir, name)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write(content)
    print(f"  Target: {target_dir}")
    print()

    # Step 2: Create a plan
    print("[2] Creating plan...")
    plan_result = api.call("POST", "/api/v1/plans", {
        "raw_request": "Add a greeting module to this Rust project. Create src/greeting.rs with a greet function that takes a name and returns a greeting string. Also add `pub mod greeting;` to src/lib.rs.",
        "request_source": "cli-pilot",
    })
    plan_id = plan_result.get("plan", {}).get("plan_id")
    if not plan_id:
        print(f"FAIL: Could not create plan: {plan_result}")
        return 1
    print(f"  Plan: {plan_id}")
    print()

    # Step 3: Create workflow run
    print("[3] Creating workflow run...")
    run_result = api.call("POST", "/api/v1/workflow-runs", {"plan_id": plan_id})
    run_id = run_result.get("run", {}).get("run_id")
    if not run_id:
        print(f"FAIL: Could not create run: {run_result}")
        return 1
    print(f"  Run: {run_id}")
    print()

    # Step 4: Create workspace
    print("[4] Creating workspace...")
    ws_result = api.call("POST", "/api/v1/supervised-patch/workspaces", {
        "run_id": run_id,
        "target_id": "cli-pilot-target",
        "target_repo_path": target_dir,
        "source_revision": "abc123",
    })
    ws = ws_result.get("workspace", {})
    ws_id = ws.get("workspace_id")
    if not ws_id:
        print(f"FAIL: Could not create workspace: {ws_result}")
        return 1
    print(f"  Workspace: {ws_id}")
    print()

    # Get workspace path
    ws_detail = api.call("GET", f"/api/v1/supervised-patch/workspaces/{ws_id}")
    ws_path = ws_detail.get("workspace", {}).get("workspace_path")
    if not ws_path:
        print(f"FAIL: Could not get workspace path: {ws_detail}")
        return 1
    print(f"  Workspace path: {ws_path}")
    print()

    # Step 5: Tick with real CLI executor
    print(f"[5] Running real {executor_type} executor...")
    print("  (this spawns a real CLI process — may take 30-90s)")

    tick = api.call("POST", f"/api/v1/workflow-runs/{run_id}/tick", {
        "executor": executor_type,
        "command": "In the current working directory, create a file src/greeting.rs with a pub fn greet(name: &str) -> String function. Then append `pub mod greeting;` to the existing src/lib.rs in the current directory. Do NOT look for or modify any files outside the current directory.",
        "timeout_ms": 120000,
    })
    tick_result = tick.get("tick", {}).get("result", {})
    tick_status = tick_result.get("status", "unknown")
    tick_action = tick.get("tick", {}).get("action", "N/A")
    tick_output = tick_result.get("output", "")
    print(f"  Tick: status={tick_status} action={tick_action}")
    if tick_output:
        preview = str(tick_output)[:200]
        print(f"  Output preview: {preview}...")
    print()

    # Hard assert: tick must have completed
    if tick_status not in ("completed", "cli_completed", "noop_completed"):
        print(f"FAIL: Tick did not complete. Status: {tick_status}")
        print(f"  Full response: {json.dumps(tick, indent=2)}")
        return 1

    # Hard assert: greeting.rs must exist in workspace
    greeting_path = os.path.join(ws_path, "src", "greeting.rs")
    if not os.path.exists(greeting_path):
        print("FAIL: src/greeting.rs was NOT created by CLI executor")
        print(f"  Workspace contents: {os.listdir(ws_path)}")
        src_dir = os.path.join(ws_path, "src")
        if os.path.isdir(src_dir):
            print(f"  src/ contents: {os.listdir(src_dir)}")
        return 1
    with open(greeting_path) as f:
        greeting_content = f.read()
    print(f"  VERIFY: src/greeting.rs created ({len(greeting_content)} bytes)")
    print(f"  Content preview: {greeting_content[:150]}...")
    print()

    # Step 6: Capture patch
    print("[6] Capturing patch...")
    capture_result = api.call("POST", f"/api/v1/supervised-patch/workspaces/{ws_id}/capture")
    artifact = capture_result.get("artifact", {})
    art_id = artifact.get("artifact_id")
    if not art_id:
        print(f"FAIL: Could not capture patch: {capture_result}")
        return 1
    changed = artifact.get("changed_files", [])
    print(f"  Artifact: {art_id}")
    print(f"  Patch hash: {artifact.get('patch_hash', 'N/A')}")
    print(f"  Changed files: {changed}")

    # Hard assert: patch must contain greeting changes
    has_greeting = any("greeting" in f for f in changed)
    if not has_greeting:
        print(f"FAIL: Patch does not contain greeting changes. Files: {changed}")
        return 1
    print(f"  OK: greeting changes confirmed in patch")
    print()

    # Step 7: Record approval
    print("[7] Recording approval...")
    approval_result = api.call("POST", f"/api/v1/workflow-runs/{run_id}/approvals", {
        "node_id": "node-a",
        "decision": "approved",
        "reason": "cli pilot test approval",
        "bound_patch_hash": artifact.get("patch_hash"),
        "bound_source_revision": "abc123",
        "bound_changed_files": changed,
        "expires_at": "2099-12-31T23:59:59Z",
    })
    approval = approval_result.get("approval", {})
    print(f"  Approval: {approval.get('decision', 'N/A')}")
    print()

    # Step 8: Export artifact
    print("[8] Exporting artifact...")
    export_result = api.call("POST", f"/api/v1/supervised-patch/artifacts/{art_id}/export", {
        "run_id": run_id,
    })
    export_data = export_result.get("export", {})
    if export_data.get("artifact"):
        print(f"  Export SUCCESS")
        integrity = export_data.get("integrity", {})
        print(f"  Integrity: {integrity.get('integrity_ok', 'N/A')}")
    else:
        print(f"  Export result: {export_result}")
    print()

    # Step 9: Cleanup
    print("[9] Cleaning up...")
    cleanup_result = api.call("POST", f"/api/v1/supervised-patch/workspaces/{ws_id}/cleanup")
    print(f"  Cleanup: {cleanup_result.get('workspace', {}).get('status', 'N/A')}")
    shutil.rmtree(target_dir, ignore_errors=True)
    print()

    print("=== CLI Pilot COMPLETE ===")
    return 0


if __name__ == "__main__":
    sys.exit(main())
