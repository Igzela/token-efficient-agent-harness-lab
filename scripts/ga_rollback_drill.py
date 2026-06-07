#!/usr/bin/env python3
"""Non-destructive rollback drill against a running ACP API.

Exercises: health -> backup -> verify -> restore dry-run -> storage integrity
-> metrics snapshot -> second health check.  All steps are read-only or
create-only; nothing is overwritten or deleted.
"""
from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.error
import urllib.request
from dataclasses import asdict, dataclass
from typing import Any


@dataclass
class Step:
    name: str
    status: str
    detail: str


def base_url_from_env() -> str:
    host = os.environ.get("HOST", "127.0.0.1")
    port = os.environ.get("PORT", "8080")
    return f"http://{host}:{port}"


def request_json(
    base_url: str,
    path: str,
    token: str,
    method: str = "GET",
    payload: dict[str, Any] | None = None,
    timeout: float = 10.0,
) -> tuple[int, dict[str, Any] | None, str | None]:
    url = f"{base_url.rstrip('/')}{path}"
    data = None
    headers: dict[str, str] = {"Accept": "application/json", "Authorization": f"Bearer {token}"}
    if payload is not None:
        data = json.dumps(payload).encode("utf-8")
        headers["Content-Type"] = "application/json"
    request = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            body = response.read().decode("utf-8", errors="replace")
            return response.status, json.loads(body) if body else {}, None
    except urllib.error.HTTPError as exc:
        body = exc.read().decode("utf-8", errors="replace")
        try:
            parsed = json.loads(body) if body else {}
        except json.JSONDecodeError:
            parsed = {"error": body}
        return exc.code, parsed, parsed.get("error") or str(exc)
    except urllib.error.URLError as exc:
        return 0, None, str(exc.reason)
    except TimeoutError:
        return 0, None, "request timed out"
    except json.JSONDecodeError as exc:
        return 0, None, f"invalid JSON response: {exc}"


def fail_step(name: str, status: int, error: str | None) -> Step:
    return Step(name, "error", error or f"HTTP {status}")


def run_drill(base_url: str, token: str) -> tuple[list[Step], dict[str, Any]]:
    steps: list[Step] = []
    metrics_snapshot: dict[str, Any] = {}

    # --- Step 1: Health check ---
    status, body, error = request_json(base_url, "/api/v1/health", token)
    if 200 <= status < 300 and body:
        steps.append(Step("health_check", "ok", f"status={body.get('status', 'unknown')}"))
    else:
        steps.append(fail_step("health_check", status, error))
        return steps, metrics_snapshot

    # --- Step 2: Create backup ---
    status, body, error = request_json(
        base_url,
        "/api/v1/backups",
        token,
        method="POST",
        payload={"label": "rollback-drill", "confirm_local_backup": True},
    )
    if not (200 <= status < 300 and body and body.get("backup")):
        steps.append(fail_step("create_backup", status, error))
        return steps, metrics_snapshot
    backup_id = body["backup"]["backup_id"]
    steps.append(Step("create_backup", "ok", f"backup_id={backup_id}"))

    # --- Step 3: Verify backup ---
    status, body, error = request_json(base_url, f"/api/v1/backups/{backup_id}/verify", token)
    verification = (body or {}).get("verification") or {}
    if not (200 <= status < 300 and verification.get("success") is True):
        steps.append(fail_step("verify_backup", status, error or json.dumps(verification)))
        return steps, metrics_snapshot
    steps.append(
        Step(
            "verify_backup",
            "ok",
            f"checksum_ok={verification.get('checksum_ok')} integrity_ok={verification.get('integrity_ok')}",
        )
    )

    # --- Step 4: Restore dry-run ---
    status, body, error = request_json(
        base_url,
        f"/api/v1/backups/{backup_id}/restore/dry-run",
        token,
        method="POST",
        payload={"confirm_restore_dry_run": True},
    )
    dry_run = (body or {}).get("restore_dry_run") or {}
    if not (200 <= status < 300 and dry_run.get("success") is True and dry_run.get("dry_run") is True):
        steps.append(fail_step("restore_dry_run", status, error or json.dumps(dry_run)))
        return steps, metrics_snapshot
    steps.append(
        Step(
            "restore_dry_run",
            "ok",
            f"would_overwrite={dry_run.get('restore_would_overwrite')} records_checked={dry_run.get('records_checked')}",
        )
    )

    # --- Step 5: Storage integrity ---
    status, body, error = request_json(base_url, "/api/v1/storage/integrity", token)
    if 200 <= status < 300 and body:
        integrity = body.get("integrity") or {}
        steps.append(
            Step("storage_integrity", "ok", f"status={integrity.get('status', 'unknown')}")
        )
    else:
        steps.append(fail_step("storage_integrity", status, error))
        return steps, metrics_snapshot

    # --- Step 6: Metrics snapshot ---
    status, body, error = request_json(base_url, "/api/v1/metrics", token)
    if 200 <= status < 300 and body:
        metrics_snapshot = {
            "dispatch_count": body.get("dispatch_count", 0),
            "audit_count": body.get("audit_event_count", 0),
            "backup_count": body.get("backup_count", 0),
        }
        steps.append(
            Step(
                "metrics_snapshot",
                "ok",
                (
                    f"dispatch_count={metrics_snapshot['dispatch_count']} "
                    f"audit_count={metrics_snapshot['audit_count']} "
                    f"backup_count={metrics_snapshot['backup_count']}"
                ),
            )
        )
    else:
        steps.append(fail_step("metrics_snapshot", status, error))
        return steps, metrics_snapshot

    # --- Step 7: Second health check ---
    status, body, error = request_json(base_url, "/api/v1/health", token)
    if 200 <= status < 300 and body:
        steps.append(
            Step("health_check_post", "ok", f"status={body.get('status', 'unknown')}")
        )
    else:
        steps.append(fail_step("health_check_post", status, error))

    return steps, metrics_snapshot


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Non-destructive rollback drill against a local ACP API."
    )
    parser.add_argument("--base-url", default=base_url_from_env(), help="API base URL.")
    parser.add_argument(
        "--token",
        default=os.environ.get("ACP_ADMIN_API_KEY"),
        help="Bearer token with backup:admin scope.",
    )
    parser.add_argument("--json", action="store_true", help="Print machine-readable JSON.")
    args = parser.parse_args()

    if not args.token:
        print("Missing token. Set ACP_ADMIN_API_KEY or pass --token.", file=sys.stderr)
        return 2

    steps, metrics_snapshot = run_drill(args.base_url, args.token)
    has_error = any(step.status == "error" for step in steps)
    verdict = "PASS" if not has_error else "FAIL"

    if args.json:
        print(
            json.dumps(
                {
                    "base_url": args.base_url,
                    "steps": [asdict(step) for step in steps],
                    "metrics_snapshot": metrics_snapshot,
                    "verdict": verdict,
                },
                indent=2,
            )
        )
    else:
        print(f"Agent Control Plane rollback drill: {args.base_url}")
        for step in steps:
            print(f"- {step.status.upper():7} {step.name}: {step.detail}")
        print(f"\nVerdict: {verdict}")

    return 1 if has_error else 0


if __name__ == "__main__":
    raise SystemExit(main())
