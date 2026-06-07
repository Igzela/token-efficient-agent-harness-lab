#!/usr/bin/env python3
"""Phase 5: Ops Soak / Production Drill E2E script.

Runs multi-run, multi-executor, failure recovery, backup/restore dry-run,
and dashboard visibility checks against a local agent-control-plane engine.
Emits a machine-readable JSON summary on stdout.

Usage:
    python scripts/soak_ops_drill.py [--base-url URL] [--count N] [--duration SEC] [--concurrency N] [--executor TYPE] [--dynamic] [--restart-command CMD] [--token TOKEN] [--json]
"""
import argparse
import json
import os
import subprocess
import sys
import time
import urllib.request
import urllib.error
from concurrent.futures import ThreadPoolExecutor, as_completed


class ApiClient:
    def __init__(self, base_url: str, token: str | None = None):
        self.base_url = base_url.rstrip("/")
        self.token = token

    def call(self, method: str, path: str, body: dict | None = None) -> tuple[int, dict | str]:
        url = f"{self.base_url}{path}"
        data = json.dumps(body).encode() if body else None
        req = urllib.request.Request(url, data=data, method=method)
        req.add_header("Content-Type", "application/json")
        if self.token:
            req.add_header("Authorization", f"Bearer {self.token}")
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
        plan_id = (plan_resp.get("plan", {}).get("plan_id") if isinstance(plan_resp.get("plan"), dict) else None)
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
            run_id = (run_resp.get("run", {}).get("run_id") if isinstance(run_resp.get("run"), dict) else None)
            run_ids.append(run_id)
        else:
            errors.append("create_run")

    # e. Tick run to completion
    if run_id:
        for _ in range(3):
            status, tick_resp = timed_call("POST", f"/api/v1/workflow-runs/{run_id}/tick", {
                "executor": executor,
                "actor": "soak_ops_drill",
            })
            if status != 200:
                break
            tick_action = None
            if isinstance(tick_resp, dict):
                tick_data = tick_resp.get("tick") if isinstance(tick_resp.get("tick"), dict) else tick_resp
                tick_action = tick_data.get("action")
            if tick_action in ("completed", "no_ready_nodes"):
                break

    # f. Fetch run detail
    if run_id:
        status, run_detail = timed_call("GET", f"/api/v1/workflow-runs/{run_id}")
        if status == 200 and isinstance(run_detail, dict):
            if run_status(run_detail) not in ("completed", "failed"):
                errors.append("run_not_terminal")

    # g. Check scheduler status
    status, _ = timed_call("GET", "/api/v1/scheduler/status")
    if status != 200:
        errors.append("scheduler_status")

    # h. Check executor pool
    executor_pool_evidence = False
    status, pool_resp = timed_call("GET", "/api/v1/executor-pool")
    if status != 200:
        errors.append("executor_pool")
    elif isinstance(pool_resp, dict):
        pool = pool_resp.get("executor_pool") if isinstance(pool_resp.get("executor_pool"), dict) else {}
        executors = pool.get("executors") if isinstance(pool, dict) else []
        executor_pool_evidence = bool(executors)
        if not executor_pool_evidence:
            errors.append("executor_pool_empty")

    # i. Check queue status
    queue_evidence = False
    status, queue_resp = timed_call("GET", "/api/v1/queue/status")
    if status != 200:
        errors.append("queue_status")
    elif isinstance(queue_resp, dict):
        queue = queue_resp.get("queue") if isinstance(queue_resp.get("queue"), dict) else {}
        queue_evidence = all(k in queue for k in ("total_queued", "total_running", "backpressure_active"))
        if not queue_evidence:
            errors.append("queue_status_incomplete")

    # j. Check decisions
    decision_count = 0
    status, decision_resp = timed_call("GET", "/api/v1/decisions")
    if status != 200:
        errors.append("decisions")
    elif isinstance(decision_resp, dict):
        decisions = decision_resp.get("decisions")
        if isinstance(decisions, list):
            decision_count = len(decisions)
        if decision_count == 0:
            errors.append("decisions_empty")

    # k. Check metrics
    status, _ = timed_call("GET", "/api/v1/metrics")
    if status != 200:
        errors.append("metrics")

    # l. Create backup
    backup_created = False
    backup_id = None
    backup_skipped = False
    status, backup_resp = timed_call("POST", "/api/v1/backups", {
        "confirm_local_backup": True,
        "label": f"soak-{iteration}",
    })
    if status in (200, 201) and isinstance(backup_resp, dict):
        bp = backup_resp.get("backup") if isinstance(backup_resp.get("backup"), dict) else backup_resp
        backup_created = True
        backup_id = bp.get("backup_id") if bp else None
    elif status == 401:
        backup_skipped = True

    # m. Verify backup
    backup_verified = False
    if backup_id:
        status, _ = timed_call("GET", f"/api/v1/backups/{backup_id}/verify")
        if status == 200:
            backup_verified = True

    # n. Restore dry-run
    backup_restore_dry = False
    if backup_id:
        status, _ = timed_call("POST", f"/api/v1/backups/{backup_id}/restore/dry-run", {
            "confirm_restore_dry_run": True,
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
        "backup_skipped": backup_skipped,
        "decision_count": decision_count,
        "executor_pool_evidence": executor_pool_evidence,
        "queue_evidence": queue_evidence,
    }


def run_failure_recovery(client: ApiClient, executor: str) -> dict:
    """Test failure recovery: create run, tick with fail, verify failed status."""
    status, plan_resp = client.call("POST", "/api/v1/plans", {
        "raw_request": "failure recovery test",
        "request_source": "soak_ops_drill",
    })
    if status not in (200, 201) or not isinstance(plan_resp, dict):
        return {"success": False, "error": "create_plan_failed"}

    plan_id = (plan_resp.get("plan", {}).get("plan_id") if isinstance(plan_resp.get("plan"), dict) else None)
    status, run_resp = client.call("POST", "/api/v1/workflow-runs", {
        "plan_id": plan_id,
        "actor": "soak_ops_drill",
    })
    if status not in (200, 201) or not isinstance(run_resp, dict):
        return {"success": False, "error": "create_run_failed"}

    run_id = (run_resp.get("run", {}).get("run_id") if isinstance(run_resp.get("run"), dict) else None)

    # Tick with fail executor
    client.call("POST", f"/api/v1/workflow-runs/{run_id}/tick", {
        "executor": "fail",
        "actor": "soak_ops_drill",
    })

    # Verify failed terminal status; completed would mean the fail executor was not used.
    status, run_detail = client.call("GET", f"/api/v1/workflow-runs/{run_id}")
    if status == 200 and isinstance(run_detail, dict):
        st = run_status(run_detail)
        return {"success": st == "failed", "status": st}

    return {"success": False, "error": "fetch_failed"}


def run_multi_executor(client: ApiClient) -> dict:
    """Create runs with distinguishable executors and verify terminal outcomes."""
    run_ids = []
    expected = {}
    executors = [("noop", "completed"), ("fail", "failed")]
    for i, (executor, expected_status) in enumerate(executors):
        status, plan_resp = client.call("POST", "/api/v1/plans", {
            "raw_request": f"multi-executor test {i}",
            "request_source": "soak_ops_drill",
        })
        if status not in (200, 201):
            continue
        plan_id = (plan_resp.get("plan", {}).get("plan_id") if isinstance(plan_resp.get("plan"), dict) else None)
        status, run_resp = client.call("POST", "/api/v1/workflow-runs", {
            "plan_id": plan_id,
            "actor": "soak_ops_drill",
        })
        if status in (200, 201):
            run_id = (run_resp.get("run", {}).get("run_id") if isinstance(run_resp.get("run"), dict) else None)
            if run_id:
                run_ids.append((run_id, executor))
                expected[run_id] = expected_status

    for run_id, executor in run_ids:
        client.call("POST", f"/api/v1/workflow-runs/{run_id}/tick", {
            "executor": executor,
            "actor": "soak_ops_drill",
        })

    all_expected = len(run_ids) == len(executors)
    statuses = {}
    for run_id, _ in run_ids:
        status, detail = client.call("GET", f"/api/v1/workflow-runs/{run_id}")
        st = run_status(detail) if status == 200 and isinstance(detail, dict) else None
        statuses[run_id] = st
        if st != expected.get(run_id):
            all_expected = False

    return {"success": all_expected, "runs": len(run_ids), "statuses": statuses}


def run_restart_recovery(
    client: ApiClient,
    executor: str,
    restart_command: str | None,
    restart_timeout: float = 60.0,
) -> dict:
    """Test recovery across an operator-supplied engine restart command."""
    if not restart_command:
        return {
            "success": False,
            "error": "restart_command_required",
            "detail": "Pass --restart-command to prove process restart recovery.",
        }

    # Create plan
    status, plan_resp = client.call("POST", "/api/v1/plans", {
        "raw_request": "restart recovery test",
        "request_source": "soak_ops_drill",
    })
    if status not in (200, 201) or not isinstance(plan_resp, dict):
        return {"success": False, "error": "create_plan_failed", "step": "create_plan"}

    plan_id = (plan_resp.get("plan", {}).get("plan_id") if isinstance(plan_resp.get("plan"), dict) else None)
    status, run_resp = client.call("POST", "/api/v1/workflow-runs", {
        "plan_id": plan_id,
        "actor": "soak_ops_drill",
    })
    if status not in (200, 201) or not isinstance(run_resp, dict):
        return {"success": False, "error": "create_run_failed", "step": "create_run"}

    run_id = (run_resp.get("run", {}).get("run_id") if isinstance(run_resp.get("run"), dict) else None)

    # Pause before restart so the recovery proof is tied to persisted run state.
    status, pause_body = client.call("PUT", f"/api/v1/queue/runs/{run_id}/pause", {
        "reason": "restart-recovery-test",
    })
    if status != 200:
        return {
            "success": False,
            "error": "pause_failed",
            "step": "pause",
            "run_id": run_id,
            "body": pause_body,
        }

    try:
        completed = subprocess.run(
            restart_command,
            shell=True,
            capture_output=True,
            text=True,
            timeout=restart_timeout,
        )
    except subprocess.TimeoutExpired:
        return {
            "success": False,
            "error": "restart_command_timeout",
            "run_id": run_id,
        }
    except OSError as exc:
        return {
            "success": False,
            "error": "restart_command_failed_to_start",
            "detail": str(exc),
            "run_id": run_id,
        }

    restart_evidence = {
        "returncode": completed.returncode,
        "stdout_tail": completed.stdout[-500:],
        "stderr_tail": completed.stderr[-500:],
    }
    if completed.returncode != 0:
        return {
            "success": False,
            "error": "restart_command_failed",
            "run_id": run_id,
            "restart": restart_evidence,
        }

    if not client.wait_for_health(retries=max(int(restart_timeout), 1), delay=1.0):
        return {
            "success": False,
            "error": "health_not_restored",
            "run_id": run_id,
            "restart": restart_evidence,
        }

    status, detail = client.call("GET", f"/api/v1/workflow-runs/{run_id}")
    if status != 200 or not isinstance(detail, dict):
        return {
            "success": False,
            "error": "run_not_readable_after_restart",
            "run_id": run_id,
            "restart": restart_evidence,
        }
    after_restart_status = run_status(detail)

    # Resume the run
    status, resume_body = client.call("POST", f"/api/v1/workflow-runs/{run_id}/resume", {
        "reason": "restart-recovery-test-resume",
    })
    if status != 200:
        return {
            "success": False,
            "error": "resume_failed",
            "step": "resume",
            "run_id": run_id,
            "body": resume_body,
            "after_restart_status": after_restart_status,
            "restart": restart_evidence,
        }

    terminal = False
    final_status = None
    poll_deadline = time.monotonic() + restart_timeout
    while time.monotonic() < poll_deadline:
        status, detail = client.call("GET", f"/api/v1/workflow-runs/{run_id}")
        if status != 200:
            break
        final_status = run_status(detail) if isinstance(detail, dict) else None
        if final_status in ("completed", "failed"):
            terminal = True
            break
        time.sleep(1.0)

    return {
        "success": terminal,
        "after_restart_status": after_restart_status,
        "final_status": final_status,
        "run_id": run_id,
        "executor": executor,
        "restart": restart_evidence,
    }


def create_plan_run(client: ApiClient, raw_request: str) -> tuple[str | None, str | None, dict | str]:
    status, plan_resp = client.call("POST", "/api/v1/plans", {
        "raw_request": raw_request,
        "request_source": "soak_ops_drill",
    })
    if status not in (200, 201) or not isinstance(plan_resp, dict):
        return None, None, plan_resp
    plan_id = plan_resp.get("plan", {}).get("plan_id") if isinstance(plan_resp.get("plan"), dict) else None
    status, run_resp = client.call("POST", "/api/v1/workflow-runs", {
        "plan_id": plan_id,
        "actor": "soak_ops_drill",
    })
    if status not in (200, 201) or not isinstance(run_resp, dict):
        return plan_id, None, run_resp
    run_id = run_resp.get("run", {}).get("run_id") if isinstance(run_resp.get("run"), dict) else None
    return plan_id, run_id, run_resp


def run_dynamic_recovery(client: ApiClient) -> dict:
    """Seed a failed run, then use DynamicWorkflowController HTTP tick to mutate graph."""
    _, run_id, body = create_plan_run(client, "dynamic recovery failure injection")
    if not run_id:
        return {"success": False, "error": "create_run_failed", "body": body}

    status, fail_body = client.call("POST", f"/api/v1/workflow-runs/{run_id}/tick", {
        "executor": "fail",
        "actor": "soak_ops_drill",
    })
    if status != 200:
        return {"success": False, "error": "seed_failure_failed", "run_id": run_id, "body": fail_body}

    status, dynamic_body = client.call("POST", f"/api/v1/workflow-runs/{run_id}/tick", {
        "executor": "dynamic",
        "actor": "soak_ops_drill",
    })
    tick = dynamic_body.get("tick", {}) if isinstance(dynamic_body, dict) else {}
    actions = tick.get("actions", []) if isinstance(tick, dict) else []
    graph_mutated = any(isinstance(a, dict) and a.get("type") == "graph_mutated" for a in actions)
    mutations = tick.get("mutations_applied", 0) if isinstance(tick, dict) else 0
    return {
        "success": status == 200 and graph_mutated and mutations > 0,
        "run_id": run_id,
        "graph_mutated": graph_mutated,
        "mutations_applied": mutations,
        "actions": actions,
    }


def run_timeout_probe(client: ApiClient) -> dict:
    """Run command executor against a workspace script that exceeds timeout."""
    import tempfile
    from pathlib import Path
    import shutil

    target = tempfile.mkdtemp(prefix="soak-timeout-target-")
    workspace_id = None
    try:
        Path(target, "slow.py").write_text("import time\ntime.sleep(3)\n")
        _, run_id, body = create_plan_run(client, "timeout failure injection")
        if not run_id:
            return {"success": False, "error": "create_run_failed", "body": body}
        status, ws_resp = client.call("POST", "/api/v1/supervised-patch/workspaces", {
            "run_id": run_id,
            "target_id": "soak-timeout",
            "target_repo_path": target,
            "source_revision": "soak-timeout",
        })
        workspace_id = ws_resp.get("workspace", {}).get("workspace_id") if isinstance(ws_resp, dict) else None
        if status not in (200, 201) or not workspace_id:
            return {"success": False, "error": "workspace_failed", "body": ws_resp}
        status, tick_resp = client.call("POST", f"/api/v1/workflow-runs/{run_id}/tick", {
            "executor": "command",
            "actor": "soak_ops_drill",
            "command": "python3 slow.py",
            "timeout_ms": 1000,
        })
        tick = tick_resp.get("tick", {}) if isinstance(tick_resp, dict) else {}
        result = tick.get("result", {}) if isinstance(tick, dict) else {}
        return {
            "success": status == 200 and result.get("error_domain") == "command_timeout",
            "run_id": run_id,
            "error_domain": result.get("error_domain"),
            "status": result.get("status"),
        }
    finally:
        if workspace_id:
            client.call("POST", f"/api/v1/supervised-patch/workspaces/{workspace_id}/cleanup")
        shutil.rmtree(target, ignore_errors=True)


def run_retry_exhaustion(client: ApiClient) -> dict:
    _, run_id, body = create_plan_run(client, "retry exhaustion failure injection")
    if not run_id:
        return {"success": False, "error": "create_run_failed", "body": body}
    statuses = []
    for _ in range(3):
        status, tick_resp = client.call("POST", f"/api/v1/workflow-runs/{run_id}/tick", {
            "executor": "fail",
            "actor": "soak_ops_drill",
            "max_retries": 1,
        })
        tick = tick_resp.get("tick", {}) if isinstance(tick_resp, dict) else {}
        statuses.append({"http_status": status, "action": tick.get("action"), "result": tick.get("result")})
    status, detail = client.call("GET", f"/api/v1/workflow-runs/{run_id}")
    final_status = run_status(detail) if status == 200 and isinstance(detail, dict) else None
    return {"success": final_status == "failed", "run_id": run_id, "final_status": final_status, "ticks": statuses}


def run_queue_pressure(client: ApiClient, concurrency: int) -> dict:
    created = []
    for i in range(max(concurrency, 2)):
        _, run_id, _ = create_plan_run(client, f"queue pressure run {i}")
        if run_id:
            created.append(run_id)
    status, queue_resp = client.call("GET", "/api/v1/queue/status")
    queue = queue_resp.get("queue", {}) if isinstance(queue_resp, dict) else {}
    required_keys = ("total_queued", "total_running", "backpressure_active")
    has_shape = isinstance(queue, dict) and all(k in queue for k in required_keys)
    total_queued = queue.get("total_queued", 0) if isinstance(queue, dict) else 0
    total_running = queue.get("total_running", 0) if isinstance(queue, dict) else 0
    backpressure_active = queue.get("backpressure_active") if isinstance(queue, dict) else None
    observed_load = int(total_queued or 0) + int(total_running or 0)
    success = status == 200 and len(created) >= 2 and has_shape and (
        observed_load > 0 or backpressure_active is True
    )
    result = {
        "success": success,
        "runs_created": len(created),
        "queue_status": queue,
        "observed_load": observed_load,
    }
    if not success:
        result["error"] = "queue_pressure_not_observed"
    return {
        **result,
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


def run_status(detail):
    """Extract status from run detail response, handling nested 'run' wrapper."""
    if not isinstance(detail, dict):
        return None
    if "status" in detail:
        return detail["status"]
    run = detail.get("run")
    if isinstance(run, dict):
        return run.get("status")
    return None


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
    parser.add_argument("--duration", type=float, default=0.0, help="Minimum soak duration in seconds.")
    parser.add_argument("--concurrency", type=int, default=2)
    parser.add_argument("--executor", default="noop")
    parser.add_argument("--dynamic", action="store_true", help="Run dynamic recovery failure-injection probe.")
    parser.add_argument("--restart-command", help="Command that restarts the running engine for recovery proof.")
    parser.add_argument("--restart-timeout", type=float, default=60.0)
    parser.add_argument("--token", default=os.environ.get("ACP_ADMIN_API_KEY"))
    parser.add_argument("--json", action="store_true", dest="json_output")
    args = parser.parse_args()
    args.concurrency = max(args.concurrency, 1)

    client = ApiClient(args.base_url, args.token)
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
    total_decisions_seen = 0
    executor_pool_checks = 0
    queue_checks = 0

    print(
        f"Starting soak: count={args.count}, duration={args.duration}s, concurrency={args.concurrency}, executor={args.executor}",
        file=sys.stderr,
    )

    submitted = 0
    deadline = t_start + args.duration if args.duration > 0 else None
    while submitted < args.count or (deadline is not None and time.monotonic() < deadline):
        batch_size = min(args.concurrency, max(args.count - submitted, args.concurrency if deadline else 1))
        with ThreadPoolExecutor(max_workers=batch_size) as pool:
            futures = [
                pool.submit(run_soak_iteration, client, submitted + offset, args.executor)
                for offset in range(batch_size)
            ]
            submitted += batch_size
            for future in as_completed(futures):
                result = future.result()
                all_results.append(result)
                all_latencies.extend(result["latencies_ms"])
                total_runs_created += len(result["run_ids"])
                backups_created += int(result["backup_created"])
                backups_verified += int(result["backup_verified"])
                backups_restore_dry += int(result["backup_restore_dry_run"])
                total_decisions_seen += int(result.get("decision_count", 0))
                executor_pool_checks += int(result.get("executor_pool_evidence", False))
                queue_checks += int(result.get("queue_evidence", False))
                status_str = "OK" if result["success"] else f"FAIL({','.join(result['errors'])})"
                print(f"  [{len(all_results)}] {status_str}", file=sys.stderr)

    # Check run statuses
    for r in all_results:
        for run_id in r.get("run_ids", []):
            status, detail = client.call("GET", f"/api/v1/workflow-runs/{run_id}")
            if status == 200 and isinstance(detail, dict):
                st = run_status(detail)
                if st == "completed":
                    total_runs_completed += 1
                elif st == "failed":
                    total_runs_failed += 1

    # Failure recovery test
    print("Running failure recovery test...", file=sys.stderr)
    failure_result = run_failure_recovery(client, args.executor)

    # Multi-executor test
    print("Running multi-executor test...", file=sys.stderr)
    multi_result = run_multi_executor(client)

    # Restart recovery test
    print("Running restart recovery test...", file=sys.stderr)
    restart_result = run_restart_recovery(
        client,
        args.executor,
        args.restart_command,
        args.restart_timeout,
    )

    print("Running timeout probe...", file=sys.stderr)
    timeout_result = run_timeout_probe(client)

    print("Running retry exhaustion probe...", file=sys.stderr)
    retry_result = run_retry_exhaustion(client)

    print("Running queue pressure probe...", file=sys.stderr)
    queue_pressure_result = run_queue_pressure(client, args.concurrency)

    dynamic_result = {"success": True, "skipped": True}
    if args.dynamic:
        print("Running dynamic recovery probe...", file=sys.stderr)
        dynamic_result = run_dynamic_recovery(client)

    # Dashboard visibility test
    print("Running dashboard visibility test...", file=sys.stderr)
    dash_result = run_dashboard_visibility(client)

    backups_skipped = sum(1 for r in all_results if r.get("backup_skipped"))
    backup_auth_configured = backups_skipped == 0

    duration = time.monotonic() - t_start
    successes = sum(1 for r in all_results if r["success"])
    success_rate = successes / max(len(all_results), 1)

    required_checks = [
        total_runs_created > 0,
        total_runs_completed > 0,
        failure_result["success"],
        multi_result["success"],
        restart_result["success"],
        timeout_result["success"],
        retry_result["success"],
        queue_pressure_result["success"],
        dynamic_result["success"],
        dash_result["success"],
        total_decisions_seen > 0,
        executor_pool_checks == len(all_results),
        queue_checks == len(all_results),
        args.concurrency > 1 and len(all_results) >= args.concurrency,
    ]
    if backup_auth_configured:
        required_checks.extend([
            backups_created > 0,
            backups_verified > 0,
            backups_restore_dry > 0,
        ])

    all_required_pass = all(required_checks)

    summary = {
        "phase": "ops_soak",
        "track_phase": "SG-2",
        "status": "PASS" if all_required_pass else "FAIL",
        "iterations": len(all_results),
        "requested_count": args.count,
        "requested_duration_seconds": args.duration,
        "concurrency": args.concurrency,
        "executor": args.executor,
        "dynamic": args.dynamic,
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
        "backup_auth_configured": backup_auth_configured,
        "decision_records_seen": total_decisions_seen,
        "executor_pool_evidence": executor_pool_checks == len(all_results),
        "queue_evidence": queue_checks == len(all_results),
        "timeout_probe": timeout_result,
        "retry_exhaustion_probe": retry_result,
        "queue_pressure_probe": queue_pressure_result,
        "dynamic_recovery_probe": dynamic_result,
        "sqlite_contention_evidence": args.concurrency > 1 and len(all_results) >= args.concurrency,
        "ops_endpoints_checked": dash_result["checked"],
        "ops_endpoints_passed": dash_result["passed"],
        "failure_recovery_test": failure_result["success"],
        "multi_executor_test": multi_result["success"],
        "restart_recovery_test": restart_result["success"],
        "timeout_test": timeout_result["success"],
        "retry_exhaustion_test": retry_result["success"],
        "queue_pressure_test": queue_pressure_result["success"],
        "dynamic_recovery_test": dynamic_result["success"],
        "dashboard_visibility_test": dash_result["success"],
        "duration_seconds": round(duration, 2),
    }

    if total_runs_created == 0:
        print("ERROR: zero runs created — engine did not produce any runs", file=sys.stderr)
        sys.exit(1)

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
