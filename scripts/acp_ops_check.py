#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import urllib.error
import urllib.request
from dataclasses import asdict, dataclass
from typing import Any


@dataclass
class Check:
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
    token: str | None,
    method: str = "GET",
    payload: dict[str, Any] | None = None,
    timeout: float = 5.0,
) -> tuple[int, dict[str, Any] | None, str | None]:
    url = f"{base_url.rstrip('/')}{path}"
    data = None
    headers = {"Accept": "application/json"}
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


def check_ok(name: str, status_code: int, body: dict[str, Any] | None, error: str | None) -> Check:
    if 200 <= status_code < 300:
        return Check(name, "ok", "reachable")
    if status_code in {401, 403}:
        return Check(name, "warn", f"auth blocked ({status_code}); provide a token with required scope")
    return Check(name, "error", error or f"HTTP {status_code}")


def collect_checks(base_url: str, token: str | None) -> list[Check]:
    checks: list[Check] = []

    status, body, error = request_json(base_url, "/api/v1/health", token)
    if 200 <= status < 300 and body:
        checks.append(Check("health", "ok", f"status={body.get('status', 'unknown')}"))
    else:
        checks.append(check_ok("health", status, body, error))

    status, body, error = request_json(base_url, "/api/v1/ready", token)
    if 200 <= status < 300 and body:
        checks.append(Check("ready", "ok", f"status={body.get('status', 'unknown')}"))
    else:
        checks.append(check_ok("ready", status, body, error))

    status, body, error = request_json(base_url, "/api/v1/metrics", token)
    if 200 <= status < 300 and body:
        detail = (
            f"dispatches={body.get('dispatch_count', 0)} "
            f"audit={body.get('audit_event_count', 0)} "
            f"backups={body.get('backup_count', 0)} "
            f"provider_enabled={body.get('provider_enabled', False)}"
        )
        checks.append(Check("metrics", "ok", detail))
    else:
        checks.append(check_ok("metrics", status, body, error))

    status, body, error = request_json(base_url, "/api/v1/storage/integrity", token)
    if 200 <= status < 300 and body:
        integrity = body.get("integrity", {})
        checks.append(Check("storage_integrity", "ok", f"status={integrity.get('status', 'unknown')}"))
    else:
        checks.append(check_ok("storage_integrity", status, body, error))

    status, body, error = request_json(base_url, "/api/v1/provider/health", token)
    if 200 <= status < 300 and body:
        checks.append(Check("provider_health", "ok", f"status={body.get('status', 'unknown')}"))
    else:
        checks.append(check_ok("provider_health", status, body, error))

    status, body, error = request_json(base_url, "/api/v1/backups", token)
    if 200 <= status < 300 and body:
        backups = body.get("backups") or []
        checks.append(Check("backups", "ok", f"count={len(backups)}"))
    else:
        checks.append(check_ok("backups", status, body, error))

    status, body, error = request_json(base_url, "/api/v1/costs", token)
    if 200 <= status < 300 and body:
        checks.append(
            Check(
                "costs",
                "ok",
                f"reserved={body.get('total_reserved_cost', 0)} estimated={body.get('total_estimated_cost_usd', 0)}",
            )
        )
    else:
        checks.append(check_ok("costs", status, body, error))

    return checks


def main() -> int:
    parser = argparse.ArgumentParser(description="Check local Agent Control Plane operational readiness.")
    parser.add_argument("--base-url", default=base_url_from_env(), help="API base URL.")
    parser.add_argument("--token", default=os.environ.get("ACP_ADMIN_API_KEY"), help="Bearer token; never printed.")
    parser.add_argument("--json", action="store_true", help="Print machine-readable JSON.")
    args = parser.parse_args()

    checks = collect_checks(args.base_url, args.token)
    has_error = any(check.status == "error" for check in checks)

    if args.json:
        print(json.dumps({"base_url": args.base_url, "checks": [asdict(check) for check in checks]}, indent=2))
    else:
        print(f"Agent Control Plane ops check: {args.base_url}")
        for check in checks:
            print(f"- {check.status.upper():5} {check.name}: {check.detail}")

    return 1 if has_error else 0


if __name__ == "__main__":
    raise SystemExit(main())
