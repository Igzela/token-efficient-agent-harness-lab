#!/usr/bin/env python3
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
    headers = {"Accept": "application/json", "Authorization": f"Bearer {token}"}
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


def run_smoke(
    base_url: str,
    token: str,
    label: str,
    execute_restore: bool,
    confirm_execute_restore: bool,
) -> tuple[list[Step], int]:
    steps: list[Step] = []

    status, body, error = request_json(
        base_url,
        "/api/v1/backups",
        token,
        method="POST",
        payload={"label": label, "confirm_local_backup": True},
    )
    if not (200 <= status < 300 and body and body.get("backup")):
        steps.append(fail_step("create_backup", status, error))
        return steps, 1
    backup_id = body["backup"]["backup_id"]
    steps.append(Step("create_backup", "ok", f"backup_id={backup_id}"))

    status, body, error = request_json(base_url, f"/api/v1/backups/{backup_id}/verify", token)
    verification = (body or {}).get("verification") or {}
    if not (200 <= status < 300 and verification.get("success") is True):
        steps.append(fail_step("verify_backup", status, error or json.dumps(verification)))
        return steps, 1
    steps.append(
        Step(
            "verify_backup",
            "ok",
            f"checksum_ok={verification.get('checksum_ok')} integrity_ok={verification.get('integrity_ok')}",
        )
    )

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
        return steps, 1
    steps.append(
        Step(
            "restore_dry_run",
            "ok",
            f"would_overwrite={dry_run.get('restore_would_overwrite')} records_checked={dry_run.get('records_checked')}",
        )
    )

    if execute_restore:
        if not confirm_execute_restore:
            steps.append(Step("execute_restore", "error", "--confirm-execute-restore is required"))
            return steps, 1
        status, body, error = request_json(
            base_url,
            f"/api/v1/backups/{backup_id}/restore",
            token,
            method="POST",
            payload={"confirm_restore": True},
        )
        restore = (body or {}).get("restore") or {}
        if not (200 <= status < 300 and restore.get("success") is True):
            steps.append(fail_step("execute_restore", status, error or json.dumps(restore)))
            return steps, 1
        steps.append(Step("execute_restore", "ok", f"records_restored={restore.get('records_restored')}"))
    else:
        steps.append(Step("execute_restore", "skipped", "default smoke is non-destructive"))

    return steps, 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Run backup verify and restore dry-run smoke against a local ACP API.")
    parser.add_argument("--base-url", default=base_url_from_env(), help="API base URL.")
    parser.add_argument("--token", default=os.environ.get("ACP_ADMIN_API_KEY"), help="Bearer token with backup:admin.")
    parser.add_argument("--label", default="restore-smoke", help="Backup label to create.")
    parser.add_argument("--execute-restore", action="store_true", help="Also execute real app-owned SQLite restore.")
    parser.add_argument(
        "--confirm-execute-restore",
        action="store_true",
        help="Required with --execute-restore because it overwrites the app-owned SQLite DB.",
    )
    parser.add_argument("--json", action="store_true", help="Print machine-readable JSON.")
    args = parser.parse_args()

    if not args.token:
        print("Missing token. Set ACP_ADMIN_API_KEY or pass --token.", file=sys.stderr)
        return 2

    steps, exit_code = run_smoke(
        args.base_url,
        args.token,
        args.label,
        args.execute_restore,
        args.confirm_execute_restore,
    )

    if args.json:
        print(json.dumps({"base_url": args.base_url, "steps": [asdict(step) for step in steps]}, indent=2))
    else:
        print(f"Agent Control Plane restore smoke: {args.base_url}")
        for step in steps:
            print(f"- {step.status.upper():7} {step.name}: {step.detail}")

    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
