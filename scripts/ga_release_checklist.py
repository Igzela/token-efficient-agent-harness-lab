#!/usr/bin/env python3
"""GA release checklist — validates all pre-release gates against a running ACP API.

A release manager runs this before cutting a tarball.  Exit 0 means READY.
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import urllib.error
import urllib.request
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any


@dataclass
class Check:
    name: str
    status: str  # "pass", "fail", "warn"
    detail: str


def request_json(
    base_url: str,
    path: str,
    token: str | None,
    method: str = "GET",
    payload: dict[str, Any] | None = None,
    timeout: float = 10.0,
) -> tuple[int, dict[str, Any] | None, str | None]:
    url = f"{base_url.rstrip('/')}{path}"
    data = None
    headers: dict[str, str] = {"Accept": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
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


# ---------------------------------------------------------------------------
# Individual checks
# ---------------------------------------------------------------------------


def check_secret_scan(repo_root: Path) -> Check:
    """Check 1: Run the secret scanner as a subprocess."""
    script = repo_root / "scripts" / "acp_secret_scan.py"
    if not script.exists():
        return Check("secret_scan", "warn", f"scanner not found at {script}")
    try:
        result = subprocess.run(
            [sys.executable, str(script), "--json"],
            cwd=str(repo_root),
            capture_output=True,
            text=True,
            timeout=30,
        )
    except subprocess.TimeoutExpired:
        return Check("secret_scan", "fail", "scanner timed out (30s)")
    except OSError as exc:
        return Check("secret_scan", "fail", f"failed to run scanner: {exc}")

    if result.returncode == 0:
        return Check("secret_scan", "pass", "no secrets found")
    # Parse JSON to get count
    try:
        data = json.loads(result.stdout)
        count = len(data.get("findings", []))
    except (json.JSONDecodeError, KeyError):
        count = "?"
    return Check("secret_scan", "fail", f"{count} secret(s) found")


def check_health(base_url: str, token: str | None) -> Check:
    """Check 2a: GET /api/v1/health."""
    status, body, error = request_json(base_url, "/api/v1/health", token)
    if 200 <= status < 300 and body:
        health_status = body.get("status", "unknown")
        if health_status == "healthy":
            return Check("health", "pass", f"status={health_status}")
        return Check("health", "warn", f"status={health_status}")
    return Check("health", "fail", error or f"HTTP {status}")


def check_ready(base_url: str, token: str | None) -> Check:
    """Check 2b: GET /api/v1/ready."""
    status, body, error = request_json(base_url, "/api/v1/ready", token)
    if 200 <= status < 300 and body:
        ready_status = body.get("status", "unknown")
        if ready_status == "ready":
            return Check("readiness", "pass", f"status={ready_status}")
        return Check("readiness", "warn", f"status={ready_status}")
    return Check("readiness", "fail", error or f"HTTP {status}")


def check_storage_integrity(base_url: str, token: str | None) -> Check:
    """Check 3: GET /api/v1/storage/integrity."""
    status, body, error = request_json(base_url, "/api/v1/storage/integrity", token)
    if 200 <= status < 300 and body:
        integrity = body.get("integrity", {})
        int_status = integrity.get("status", "unknown")
        if int_status == "ok":
            table_count = len(integrity.get("tables", []))
            return Check("storage_integrity", "pass", f"status={int_status} ({table_count} tables)")
        return Check("storage_integrity", "fail", f"status={int_status}")
    return Check("storage_integrity", "fail", error or f"HTTP {status}")


def check_backup_roundtrip(base_url: str, token: str | None) -> Check:
    """Check 4: Create a backup, then verify it."""
    # Create backup
    status, body, error = request_json(
        base_url,
        "/api/v1/backups",
        token,
        method="POST",
        payload={"confirm_local_backup": True, "label": "ga-checklist"},
    )
    if not (200 <= status < 300) or not body:
        return Check("backup_roundtrip", "fail", f"create failed: {error or f'HTTP {status}'}")

    backup = body.get("backup", {})
    backup_id = backup.get("backup_id")
    if not backup_id:
        return Check("backup_roundtrip", "fail", "no backup_id in response")

    # Verify backup
    status, body, error = request_json(
        base_url,
        f"/api/v1/backups/{backup_id}/verify",
        token,
    )
    if 200 <= status < 300 and body:
        verification = body.get("verification", {})
        if verification.get("valid", False) or verification.get("success", False):
            return Check("backup_roundtrip", "pass", f"backup {backup_id} created and verified")
        return Check("backup_roundtrip", "fail", f"backup {backup_id} verification failed: {verification}")
    return Check("backup_roundtrip", "fail", f"verify failed: {error or f'HTTP {status}'}")


def check_restore_dry_run(base_url: str, token: str | None) -> Check:
    """Check 5: List backups, run restore dry-run on the first one."""
    # List backups to get an ID
    status, body, error = request_json(base_url, "/api/v1/backups", token)
    if not (200 <= status < 300) or not body:
        return Check("restore_dry_run", "fail", f"list failed: {error or f'HTTP {status}'}")

    backups = body.get("backups") or []
    if not backups:
        return Check("restore_dry_run", "warn", "no backups available for dry-run")

    backup_id = backups[0].get("backup_id")
    if not backup_id:
        return Check("restore_dry_run", "fail", "first backup has no backup_id")

    status, body, error = request_json(
        base_url,
        f"/api/v1/backups/{backup_id}/restore/dry-run",
        token,
        method="POST",
        payload={"confirm_restore_dry_run": True},
    )
    if 200 <= status < 300 and body:
        dry_run = body.get("restore_dry_run", {})
        if dry_run.get("valid", False) or dry_run.get("success", False):
            return Check("restore_dry_run", "pass", f"dry-run passed for {backup_id}")
        return Check("restore_dry_run", "fail", f"dry-run invalid: {dry_run}")
    return Check("restore_dry_run", "fail", f"dry-run failed: {error or f'HTTP {status}'}")


def check_metrics(base_url: str, token: str | None) -> Check:
    """Check 6: GET /api/v1/metrics — verify dispatch_count >= 0, audit_event_count >= 0."""
    status, body, error = request_json(base_url, "/api/v1/metrics", token)
    if not (200 <= status < 300) or not body:
        return Check("metrics", "fail", error or f"HTTP {status}")

    dispatch_count = body.get("dispatch_count")
    audit_count = body.get("audit_event_count")

    if dispatch_count is None or audit_count is None:
        return Check("metrics", "warn", f"missing fields: dispatch_count={dispatch_count} audit_event_count={audit_count}")

    if dispatch_count < 0 or audit_count < 0:
        return Check("metrics", "fail", f"negative counts: dispatch={dispatch_count} audit={audit_count}")

    return Check("metrics", "pass", f"dispatch_count={dispatch_count} audit_event_count={audit_count}")


def check_dashboard_build(repo_root: Path) -> Check:
    """Check 7: Verify dashboard/out/index.html exists."""
    index = repo_root / "dashboard" / "out" / "index.html"
    if index.exists():
        size = index.stat().st_size
        return Check("dashboard_build", "pass", f"index.html exists ({size} bytes)")
    return Check("dashboard_build", "fail", f"{index} not found — run dashboard build first")


def check_require_auth(base_url: str, token: str | None) -> Check:
    """Check 8: Verify ACP_REQUIRE_AUTH=1 is set.

    Probe /api/v1/config without a token.  If auth is enforced the server
    returns 401/403; if auth is off it returns 200.
    """
    # First verify config endpoint is reachable with token
    status, body, error = request_json(base_url, "/api/v1/config", token)
    if status in (401, 403) and token is None:
        # No token and got rejected — auth is enforced (good), but we can't
        # inspect config.  Treat as warn since we can't verify the env var.
        return Check("require_auth", "warn", "endpoint rejects unauthenticated requests but no token to inspect config")

    if not (200 <= status < 300):
        return Check("require_auth", "warn", f"config endpoint: {error or f'HTTP {status}'}")

    # Config is reachable with token.  Now probe without token.
    unauth_status, _, _ = request_json(base_url, "/api/v1/config", None)
    if unauth_status in (401, 403):
        return Check("require_auth", "pass", "unauthenticated config request rejected (auth enforced)")
    if 200 <= unauth_status < 300:
        return Check("require_auth", "fail", "config endpoint accessible without token — ACP_REQUIRE_AUTH not enforced")
    return Check("require_auth", "warn", f"unauthenticated probe returned HTTP {unauth_status}")


# ---------------------------------------------------------------------------
# Orchestration
# ---------------------------------------------------------------------------

def collect_checks(base_url: str, token: str | None, repo_root: Path) -> list[Check]:
    checks: list[Check] = []

    # Check 1: Secret scan (offline, no API needed)
    checks.append(check_secret_scan(repo_root))

    # Check 2: Health + readiness
    checks.append(check_health(base_url, token))
    checks.append(check_ready(base_url, token))

    # Check 3: Storage integrity
    checks.append(check_storage_integrity(base_url, token))

    # Check 4: Backup create + verify
    checks.append(check_backup_roundtrip(base_url, token))

    # Check 5: Restore dry-run
    checks.append(check_restore_dry_run(base_url, token))

    # Check 6: Metrics health
    checks.append(check_metrics(base_url, token))

    # Check 7: Dashboard build (offline)
    checks.append(check_dashboard_build(repo_root))

    # Check 8: Config validation — ACP_REQUIRE_AUTH enforced
    checks.append(check_require_auth(base_url, token))

    return checks


def main() -> int:
    parser = argparse.ArgumentParser(
        description="GA release checklist — validate all pre-release gates against a running ACP API.",
    )
    parser.add_argument(
        "--base-url",
        default=os.environ.get("ACP_BASE_URL", "http://127.0.0.1:8080"),
        help="ACP API base URL (default: http://127.0.0.1:8080 or $ACP_BASE_URL).",
    )
    parser.add_argument(
        "--token",
        default=os.environ.get("ACP_ADMIN_API_KEY"),
        help="Admin bearer token (default: $ACP_ADMIN_API_KEY).",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        dest="json_output",
        help="Print machine-readable JSON.",
    )
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[1]
    checks = collect_checks(args.base_url, args.token, repo_root)

    has_fail = any(c.status == "fail" for c in checks)
    has_warn = any(c.status == "warn" for c in checks)
    verdict = "NOT READY" if has_fail else ("READY (with warnings)" if has_warn else "READY")

    if args.json_output:
        output = {
            "base_url": args.base_url,
            "checks": [asdict(c) for c in checks],
            "verdict": verdict,
            "ready": not has_fail,
        }
        print(json.dumps(output, indent=2))
    else:
        print(f"GA Release Checklist: {args.base_url}")
        print("=" * 60)
        symbols = {"pass": "[PASS]", "fail": "[FAIL]", "warn": "[WARN]"}
        for check in checks:
            sym = symbols.get(check.status, "[????]")
            print(f"  {sym:6}  {check.name}: {check.detail}")
        print("=" * 60)
        print(f"  Verdict: {verdict}")
        print()

    return 0 if not has_fail else 1


if __name__ == "__main__":
    raise SystemExit(main())
