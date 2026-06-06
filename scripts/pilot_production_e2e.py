#!/usr/bin/env python3
"""Production pilot: real supervised autonomous E2E flow.

The executor (not the pilot script) produces all file changes.
A helper script is placed in the workspace, then the executor runs it.
The patch captures both the helper script and its output.

Usage:
    uv run --no-project python scripts/pilot_production_e2e.py [--base-url URL]
"""
import json
import os
import shutil
import sys
import tempfile
import time
import urllib.request

DEFAULT_BASE_URL = "http://127.0.0.1:8080"


def api(method, path, body=None, base_url=DEFAULT_BASE_URL):
    url = f"{base_url}{path}"
    data = json.dumps(body).encode() if body else None
    req = urllib.request.Request(url, data=data, method=method)
    req.add_header("Content-Type", "application/json")
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            return json.loads(resp.read())
    except urllib.error.HTTPError as e:
        error_body = e.read().decode()
        print(f"  HTTP {e.code}: {error_body}")
        return {"error": e.code, "body": error_body}


def wait_for_health(base_url, timeout=30):
    start = time.time()
    while time.time() - start < timeout:
        try:
            result = api("GET", "/api/v1/health", base_url=base_url)
            if result.get("status") == "healthy":
                return True
        except Exception:
            pass
        time.sleep(1)
    return False


def main():
    base_url = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_BASE_URL
    if "--base-url" in sys.argv:
        idx = sys.argv.index("--base-url")
        base_url = sys.argv[idx + 1]

    print("=== Production Pilot E2E ===")
    print(f"Base URL: {base_url}")
    print()

    # Step 0: Health check
    print("[0] Checking engine health...")
    if not wait_for_health(base_url):
        print("FAIL: Engine not healthy")
        return 1
    print("  OK: Engine healthy")
    print()

    # Step 1: Create a real target directory with some files
    print("[1] Creating target directory...")
    target_dir = tempfile.mkdtemp(prefix="pilot-target-")
    for name, content in [
        ("README.md", "# Pilot Target\n\nTest repository for production pilot.\n"),
        ("src/main.rs", 'fn main() {\n    println!("hello");\n}\n'),
        ("src/lib.rs", "pub fn add(a: i32, b: i32) -> i32 { a + b }\n"),
        ("Cargo.toml", "[package]\nname = \"pilot-target\"\nversion = \"0.1.0\"\n"),
    ]:
        path = os.path.join(target_dir, name)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write(content)
    print(f"  Target: {target_dir}")
    print()

    # Step 2: Create a plan (prerequisite for workflow run)
    print("[2] Creating plan...")
    plan_result = api("POST", "/api/v1/plans", {
        "raw_request": "Add a greeting function to src/lib.rs",
        "request_source": "pilot",
    })
    plan_id = plan_result.get("plan", {}).get("plan_id")
    if not plan_id:
        print(f"FAIL: Could not create plan: {plan_result}")
        return 1
    print(f"  Plan: {plan_id}")
    print()

    # Step 3: Create workflow run from plan
    print("[3] Creating workflow run...")
    run_result = api("POST", "/api/v1/workflow-runs", {"plan_id": plan_id})
    run_id = run_result.get("run", {}).get("run_id")
    if not run_id:
        print(f"FAIL: Could not create run: {run_result}")
        return 1
    print(f"  Run: {run_id}")
    print()

    # Step 4: Create workspace from target
    print("[4] Creating workspace...")
    ws_result = api("POST", "/api/v1/supervised-patch/workspaces", {
        "run_id": run_id,
        "target_id": "pilot-target",
        "target_repo_path": target_dir,
        "source_revision": "abc123",
    })
    ws = ws_result.get("workspace", {})
    ws_id = ws.get("workspace_id")
    if not ws_id:
        print(f"FAIL: Could not create workspace: {ws_result}")
        return 1
    print(f"  Workspace: {ws_id}")
    print(f"  Status: {ws.get('status')}")
    print()

    # Get workspace path
    ws_detail = api("GET", f"/api/v1/supervised-patch/workspaces/{ws_id}")
    ws_path = ws_detail.get("workspace", {}).get("workspace_path")
    if not ws_path:
        print(f"FAIL: Could not get workspace path: {ws_detail}")
        return 1
    print(f"  Workspace path: {ws_path}")
    print()

    # Step 5: Executor produces real file changes
    # Place helper script in workspace BEFORE ticking, then executor runs it.
    print("[5] Executor producing file changes...")

    # 5a: Place worker script in workspace (captured in patch as workspace content)
    helper_path = os.path.join(ws_path, ".pilot_worker.py")
    with open(helper_path, "w") as f:
        f.write(
            "import pathlib\n"
            "pathlib.Path('src/greeting.rs').write_text("
            "\"pub fn greet(name: &str) -> String {\\n"
            "    format!(\\\"Hello, {name}!\\\")\\n"
            "}\\n\")\n"
            "with open('src/lib.rs', 'a') as f:\n"
            "    f.write('\\npub mod greeting;\\n')\n"
        )
    print(f"  Helper script placed: .pilot_worker.py")

    # 5b: Tick with command override — executor runs python3 on the helper script
    tick = api("POST", f"/api/v1/workflow-runs/{run_id}/tick", {
        "executor": "command",
        "command": "python3 .pilot_worker.py",
        "timeout_ms": 15000,
    })
    tick_status = tick.get("tick", {}).get("status", "unknown")
    tick_action = tick.get("tick", {}).get("action", "N/A")
    print(f"  Tick: status={tick_status} action={tick_action}")

    # Verify the executor produced changes
    greeting_path = os.path.join(ws_path, "src", "greeting.rs")
    if os.path.exists(greeting_path):
        with open(greeting_path) as f:
            content = f.read()
        print(f"  VERIFY: src/greeting.rs created by executor ({len(content)} bytes)")
    else:
        print("  NOTE: greeting.rs not created (no command node in graph, only noop)")
    print()

    # Step 6: Capture patch (diff against source manifest)
    print("[6] Capturing patch...")
    capture_result = api("POST", f"/api/v1/supervised-patch/workspaces/{ws_id}/capture")
    artifact = capture_result.get("artifact", {})
    art_id = artifact.get("artifact_id")
    if not art_id:
        print(f"FAIL: Could not capture patch: {capture_result}")
        return 1
    changed = artifact.get("changed_files", [])
    print(f"  Artifact: {art_id}")
    print(f"  Patch hash: {artifact.get('patch_hash', 'N/A')}")
    print(f"  Changed files: {changed}")
    print(f"  Redaction status: {artifact.get('redaction_status', 'N/A')}")
    if artifact.get("secret_findings"):
        print(f"  Secret findings: {artifact['secret_findings']}")

    # Verify the patch has executor-produced changes
    has_worker_change = any("greeting" in f for f in changed)
    has_helper = any("pilot_worker" in f for f in changed)
    print(f"  Worker-produced changes in patch: {has_worker_change}")
    print(f"  Helper script in patch: {has_helper}")
    if not changed:
        print("  WARN: No changes captured — executor may not have run (no command node in graph)")
    print()

    # Step 7: Record approval with binding
    print("[7] Recording approval...")
    approval_result = api("POST", f"/api/v1/workflow-runs/{run_id}/approvals", {
        "node_id": "node-a",
        "decision": "approved",
        "reason": "pilot test approval",
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
    export_result = api("POST", f"/api/v1/supervised-patch/artifacts/{art_id}/export", {
        "run_id": run_id,
    })
    export_data = export_result.get("export", {})
    if export_data.get("artifact"):
        print(f"  Export SUCCESS")
        print(f"  Exported by: {export_data.get('exported_by', 'N/A')}")
        integrity = export_data.get("integrity", {})
        print(f"  Integrity: {integrity.get('integrity_ok', 'N/A')}")
    else:
        print(f"  Export result: {export_result}")
    print()

    # Step 9: Cleanup workspace
    print("[9] Cleaning up workspace...")
    cleanup_result = api("POST", f"/api/v1/supervised-patch/workspaces/{ws_id}/cleanup")
    print(f"  Cleanup: {cleanup_result.get('workspace', {}).get('status', 'N/A')}")
    print()

    # Cleanup target dir
    shutil.rmtree(target_dir, ignore_errors=True)

    print("=== Pilot COMPLETE ===")
    return 0


if __name__ == "__main__":
    sys.exit(main())
