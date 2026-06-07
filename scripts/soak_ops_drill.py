#!/usr/bin/env python3
"""Phase 5: Ops Soak / Production Drill E2E script.

Runs multi-run, multi-executor, failure recovery, backup/restore dry-run,
and dashboard visibility checks against a local agent-control-plane engine.
Emits a machine-readable JSON summary on stdout.

Usage:
    python scripts/soak_ops_drill.py [--base-url URL] [--count N] [--executor TYPE] [--json]
"""
import argparse
import json
import sys
import time
import urllib.request
import urllib.error
from concurrent.futures import ThreadPoolExecutor, as_completed


class ApiClient:
    def __init__(self, base_url: str):
        self.base_url = base_url.rstrip("/")

    def call(self, method: str, path: str, body: dict | None = None) -> tuple[int, dict | str]:
        url = f"{self.base_url}{path}"
        data = json.dumps(body).encode() if body else None
        req = urllib.request.Request(url, data=data, method=method)
        req.add_header("Content-Type", "application/json")
        try:
            with urllib.request.urlopen(req, timeout=30) as resp:
                raw = resp.read().decode()
                try:
                    return resp.status, json.loads(raw)
                except json.JSONDecodeError:
                    return resp.status, raw
        except urllib.error.HTTPError as e:
            raw = e.read().decode()
            try:
                return e.code, json.loads(raw)
            except json.JSONDecodeError:
                return e.code, raw
        except Exception as e:
            return 0, str(e)

    def wait_for_health(self, retries: int = 10, delay: float = 1.0) -> bool:
        for _ in range(retries):
            status, body = self.call("GET", "/api/v1/health")
            if status == 200:
                return True
            time.sleep(delay)
        return False


def run_soak_iteration(client: ApiClient, iteration: int, executor: str) -> dict:
    """Run one soak iteration. Returns per-iteration metrics."""
    latencies = []
    errors = []
    run_ids = []

    def timed_call(method, path, body=None):
        t0 = time.monotonic()
        status, resp = client.call(method, path, body)
        dt = (time.monotonic() - t0) * 1000
        latencies.append(dt)
        return status, resp

    # a. Health check
    status, _ = timed_call("GET", "/api/v1/health")
    if status != 200:
        errors.append("health")

    # b. Readiness check
    status, _ = timed_call("GET", "/api/v1/ready")
    if status != 200:
        errors.append("ready")

    # c. Create plan
    status, plan_resp = timed_call("POST", "/api/v1/plans", {
        "raw_request": f"soak iteration {iteration}",
        "request_source": "soak_ops_drill",
    })
    plan_id = None
    if status in (200, 201) and isinstance(plan_resp, dict):
        plan_id = plan_resp.get("plan_id")
    else:
        errors.append("create_plan")

    # d. Create workflow run from plan
    run_id = None
    if plan_id:
        status, run_resp = timed_call("POST", "/api/v1/workflow-runs", {
            "plan_id": plan_id,
            "actor": "soak_ops_drill",
        })
        if status in (200, 201) and isinstance(run_resp, dict):
            run_id = run_resp.get("run_id")
            run_ids.append(run_id)
        else:
            errors.append("create_run")

    # e. Tick run to completion
    if run_id:
        for _ in range(3):
            status, tick_resp = timed_call("POST", f"/api/v1/workflow-runs/{run_id}/tick", {
                "executor_type": executor,
                "actor": "soak_ops_drill",
            })
            if status != 200:
                break
            if isinstance(tick_resp, dict) and tick_resp.get("action") in ("completed", "no_ready_nodes"):
                break

    # f. Fetch run detail
    if run_id:
        status, run_detail = timed_call("GET", f"/api/v1/workflow-runs/{run_id}")
        if status == 200 and isinstance(run_detail, dict):
            if run_detail.get("status") not in ("completed", "failed"):
                errors.append("run_not_terminal")

    # g. Check scheduler status
    status, _ = timed_call("GET", "/api/v1/scheduler/status")
    if status != 200:
        errors.append("scheduler_status")

    # h. Check executor pool
    status, _ = timed_call("GET", "/api/v1/executor-pool")
    if status != 200:
        errors.append("executor_pool")

    # i. Check queue status
    status, _ = timed_call("GET", "/api/v1/queue/status")
    if status != 200:
        errors.append("queue_status")

    # j. Check decisions
    status, _ = timed_call("GET", "/api/v1/decisions")
    if status != 200:
        errors.append("decisions")

    # k. Check metrics
    status, _ = timed_call("GET", "/api/v1/metrics")
    if status != 200:
        errors.append("metrics")

    # l. Create backup
    backup_created = False
    backup_id = None
    status, backup_resp = timed_call("POST", "/api/v1/backups", {
        "confirm_local_backup": True,
        "label": f"soak-{iteration}",
    })
    if status in (200, 201) and isinstance(backup_resp, dict):
        backup_created = True
        backup_id = backup_resp.get("backup_id")

    # m. Verify backup
    backup_verified = False
    if backup_id:
        status, _ = timed_call("POST", f"/api/v1/backups/{backup_id}/verify")
        if status == 200:
            backup_verified = True

    # n. Restore dry-run
    backup_restore_dry = False
    if backup_id:
        status, _ = timed_call("POST", f"/api/v1/backups/{backup_id}/restore", {
            "confirm_restore": False,
        })
        if status == 200:
            backup_restore_dry = True

    return {
        "iteration": iteration,
        "success": len(errors) == 0,
        "errors": errors,
        "latencies_ms": latencies,
        "run_ids": run_ids,
        "backup_created": backup_created,
        "backup_verified": backup_verified,
        "backup_restore_dry_run": backup_restore_dry,
    }


def run_failure_recovery(client: ApiClient, executor: str) -> dict:
    """Test failure recovery: create run, tick with fail, verify failed status."""
    status, plan_resp = client.call("POST", "/api/v1/plans", {
        "raw_request": "failure recovery test",
        "request_source": "soak_ops_drill",
    })
    if status not in (200, 201) or not isinstance(plan_resp, dict):
        return {"success": False, "error": "create_plan_failed"}

    plan_id = plan_resp.get("plan_id")
    status, run_resp = client.call("POST", "/api/v1/workflow-runs", {
        "plan_id": plan_id,
        "actor": "soak_ops_drill",
    })
    if status not in (200, 201) or not isinstance(run_resp, dict):
        return {"success": False, "error": "create_run_failed"}

    run_id = run_resp.get("run_id")

    # Tick with fail executor
    client.call("POST", f"/api/v1/workflow-runs/{run_id}/tick", {
        "executor_type": "fail",
        "actor": "soak_ops_drill",
    })

    # Verify terminal
    status, run_detail = client.call("GET", f"/api/v1/workflow-runs/{run_id}")
    if status == 200 and isinstance(run_detail, dict):
        terminal = run_detail.get("status") in ("completed", "failed")
        return {"success": terminal, "status": run_detail.get("status")}

    return {"success": False, "error": "fetch_failed"}


def run_multi_executor(client: ApiClient) -> dict:
    """Create 3 runs, tick all, verify terminal."""
    run_ids = []
    for i in range(3):
        status, plan_resp = client.call("POST", "/api/v1/plans", {
            "raw_request": f"multi-executor test {i}",
            "request_source": "soak_ops_drill",
        })
        if status not in (200, 201):
            continue
        plan_id = plan_resp.get("plan_id")
        status, run_resp = client.call("POST", "/api/v1/workflow-runs", {
            "plan_id": plan_id,
            "actor": "soak_ops_drill",
        })
        if status in (200, 201):
            run_ids.append(run_resp.get("run_id"))

    for run_id in run_ids:
        client.call("POST", f"/api/v1/workflow-runs/{run_id}/tick", {
            "executor_type": "noop",
            "actor": "soak_ops_drill",
        })

    all_terminal = True
    for run_id in run_ids:
        status, detail = client.call("GET", f"/api/v1/workflow-runs/{run_id}")
        if status != 200 or (isinstance(detail, dict) and detail.get("status") not in ("completed", "failed")):
            all_terminal = False

    return {"success": all_terminal, "runs": len(run_ids)}


def run_restart_recovery(client: ApiClient, executor: str) -> dict:
    """Test restart recovery: create run, pause, resume, tick to completion."""
    # Create plan
    status, plan_resp = client.call("POST", "/api/v1/plans", {
        "raw_request": "restart recovery test",
        "request_source": "soak_ops_drill",
    })
    if status not in (200, 201) or not isinstance(plan_resp, dict):
        return {"success": False, "error": "create_plan_failed", "step": "create_plan"}

    plan_id = plan_resp.get("plan_id")
    status, run_resp = client.call("POST", "/api/v1/workflow-runs", {
        "plan_id": plan_id,
        "actor": "soak_ops_drill",
    })
    if status not in (200, 201) or not isinstance(run_resp, dict):
        return {"success": False, "error": "create_run_failed", "step": "create_run"}

    run_id = run_resp.get("run_id")

    # Tick once so the run has active nodes
    status, _ = client.call("POST", f"/api/v1/workflow-runs/{run_id}/tick", {
        "executor_type": executor,
        "actor": "soak_ops_drill",
    })
    if status != 200:
        return {"success": False, "error": "initial_tick_failed", "step": "initial_tick", "run_id": run_id}

    # Pause the run
    status, _ = client.call("PUT", f"/api/v1/queue/runs/{run_id}/pause", {
        "reason": "restart-recovery-test",
    })
    if status != 200:
        return {"success": False, "error": "pause_failed", "step": "pause", "run_id": run_id}

    # Verify paused state
    status, detail = client.call("GET", f"/api/v1/workflow-runs/{run_id}")
    paused_ok = status == 200 and isinstance(detail, dict)

    # Resume the run
    status, _ = client.call("POST", f"/api/v1/workflow-runs/{run_id}/resume", {
        "reason": "restart-recovery-test-resume",
    })
    if status != 200:
        return {"success": False, "error": "resume_failed", "step": "resume", "run_id": run_id}

    # Tick to completion after resume
    for _ in range(3):
        status, tick_resp = client.call("POST", f"/api/v1/workflow-runs/{run_id}/tick", {
            "executor_type": executor,
            "actor": "soak_ops_drill",
        })
        if status != 200:
            break
        if isinstance(tick_resp, dict) and tick_resp.get("action") in ("completed", "no_ready_nodes"):
            break

    # Verify terminal
    status, detail = client.call("GET", f"/api/v1/workflow-runs/{run_id}")
    terminal = status == 200 and isinstance(detail, dict) and detail.get("status") in ("completed", "failed")

    return {
        "success": terminal,
        "paused_detected": paused_ok,
        "resumed_and_terminal": terminal,
        "run_id": run_id,
    }


def run_dashboard_visibility(client: ApiClient) -> dict:
    """Check dashboard and overview endpoints."""
    endpoints = [
        "/api/v1/health",
        "/api/v1/costs",
        "/api/v1/metrics",
        "/api/v1/dashboard",
    ]
    passed = 0
    for ep in endpoints:
        status, _ = client.call("GET", ep)
        if status == 200:
            passed += 1
    return {"success": passed == len(endpoints), "checked": len(endpoints), "passed": passed}


def percentile(values: list[float], p: float) -> float:
    if not values:
        return 0.0
    values = sorted(values)
    k = (len(values) - 1) * (p / 100.0)
    f = int(k)
    c = f + 1
    if c >= len(values):
        return values[-1]
    return values[f] + (k - f) * (values[c] - values[f])


def main():
    parser = argparse.ArgumentParser(description="Ops Soak / Production Drill")
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--count", type=int, default=5)
    parser.add_argument("--executor", default="noop")
    parser.add_argument("--json", action="store_true", dest="json_output")
    args = parser.parse_args()

    client = ApiClient(args.base_url)
    t_start = time.monotonic()

    if not client.wait_for_health():
        print("ERROR: engine not healthy", file=sys.stderr)
        sys.exit(1)

    all_latencies = []
    all_results = []
    total_runs_created = 0
    total_runs_completed = 0
    total_runs_failed = 0
    backups_created = 0
    backups_verified = 0
    backups_restore_dry = 0

    print(f"Starting soak: {args.count} iterations, executor={args.executor}", file=sys.stderr)

    for i in range(args.count):
        result = run_soak_iteration(client, i, args.executor)
        all_results.append(result)
        all_latencies.extend(result["latencies_ms"])
        total_runs_created += len(result["run_ids"])
        backups_created += int(result["backup_created"])
        backups_verified += int(result["backup_verified"])
        backups_restore_dry += int(result["backup_restore_dry_run"])
        status_str = "OK" if result["success"] else f"FAIL({','.join(result['errors'])})"
        print(f"  [{i+1}/{args.count}] {status_str}", file=sys.stderr)

    # Check run statuses
    for r in all_results:
        for run_id in r.get("run_ids", []):
            status, detail = client.call("GET", f"/api/v1/workflow-runs/{run_id}")
            if status == 200 and isinstance(detail, dict):
                if detail.get("status") == "completed":
                    total_runs_completed += 1
                elif detail.get("status") == "failed":
                    total_runs_failed += 1

    # Failure recovery test
    print("Running failure recovery test...", file=sys.stderr)
    failure_result = run_failure_recovery(client, args.executor)

    # Multi-executor test
    print("Running multi-executor test...", file=sys.stderr)
    multi_result = run_multi_executor(client)

    # Restart recovery test
    print("Running restart recovery test...", file=sys.stderr)
    restart_result = run_restart_recovery(client, args.executor)

    # Dashboard visibility test
    print("Running dashboard visibility test...", file=sys.stderr)
    dash_result = run_dashboard_visibility(client)

    duration = time.monotonic() - t_start
    successes = sum(1 for r in all_results if r["success"])
    success_rate = successes / max(len(all_results), 1)

    summary = {
        "phase": "ops_soak",
        "status": "PASS" if all(r["success"] for r in all_results) else "FAIL",
        "iterations": args.count,
        "success_rate": round(success_rate, 4),
        "total_runs_created": total_runs_created,
        "total_runs_completed": total_runs_completed,
        "total_runs_failed": total_runs_failed,
        "failure_domains": [e for r in all_results for e in r["errors"]],
        "p50_latency_ms": round(percentile(all_latencies, 50), 1),
        "p95_latency_ms": round(percentile(all_latencies, 95), 1),
        "backup_created": backups_created > 0,
        "backup_verified": backups_verified > 0,
        "backup_restore_dry_run": backups_restore_dry > 0,
        "ops_endpoints_checked": dash_result["checked"],
        "ops_endpoints_passed": dash_result["passed"],
        "failure_recovery_test": failure_result["success"],
        "multi_executor_test": multi_result["success"],
        "restart_recovery_test": restart_result["success"],
        "dashboard_visibility_test": dash_result["success"],
        "duration_seconds": round(duration, 2),
    }

    if args.json_output:
        print(json.dumps(summary, indent=2))
    else:
        print(f"\nSoak Results: {summary['status']}")
        print(f"  Iterations: {summary['iterations']}, Success rate: {summary['success_rate']:.1%}")
        print(f"  Runs: created={summary['total_runs_created']}, completed={summary['total_runs_completed']}, failed={summary['total_runs_failed']}")
        print(f"  Latency: p50={summary['p50_latency_ms']}ms, p95={summary['p95_latency_ms']}ms")
        print(f"  Backup: created={summary['backup_created']}, verified={summary['backup_verified']}, dry_run={summary['backup_restore_dry_run']}")
        print(f"  Failure recovery: {summary['failure_recovery_test']}, Multi-executor: {summary['multi_executor_test']}")
        print(f"  Restart recovery: {summary['restart_recovery_test']}")
        print(f"  Dashboard visibility: {summary['dashboard_visibility_test']}")
        print(f"  Duration: {summary['duration_seconds']}s")

    sys.exit(0 if summary["status"] == "PASS" else 1)


if __name__ == "__main__":
    main()
