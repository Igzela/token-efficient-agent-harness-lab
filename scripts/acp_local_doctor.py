#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import socket
import subprocess
import sys
from dataclasses import asdict, dataclass
from pathlib import Path


ADMIN_KEY_RE = re.compile(r"^harness_[0-9a-fA-F]{64}$")


@dataclass
class Check:
    name: str
    status: str
    message: str
    action: str | None = None


def repo_root_from_script() -> Path:
    return Path(__file__).resolve().parents[1]


def command_version(path: str, args: list[str]) -> str:
    try:
        result = subprocess.run(
            [path, *args],
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            timeout=5,
        )
    except Exception as exc:  # noqa: BLE001 - doctor reports actionable diagnostics.
        return f"version check failed: {exc}"
    return result.stdout.strip().splitlines()[0] if result.stdout.strip() else "version unknown"


def command_check(name: str, version_args: list[str], extra_paths: list[Path] | None = None) -> Check:
    found = shutil.which(name)
    if found:
        return Check(
            name=name,
            status="ok",
            message=f"{found} ({command_version(found, version_args)})",
        )

    for candidate_dir in extra_paths or []:
        candidate = candidate_dir / name
        if candidate.exists() and os.access(candidate, os.X_OK):
            return Check(
                name=name,
                status="warn",
                message=f"installed at {candidate}, but not on PATH",
                action=f'export PATH="{candidate_dir}:$PATH"',
            )

    return Check(
        name=name,
        status="error",
        message=f"{name} not found",
        action=f"install {name} and add it to PATH",
    )


def file_check(name: str, path: Path, action: str) -> Check:
    if path.exists():
        return Check(name=name, status="ok", message=str(path))
    return Check(name=name, status="warn", message=f"missing: {path}", action=action)


def port_check(port: int) -> Check:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.settimeout(0.25)
        result = sock.connect_ex(("127.0.0.1", port))
    if result == 0:
        return Check(
            name="port",
            status="warn",
            message=f"127.0.0.1:{port} is already in use",
            action=f"set PORT to another value or stop the process using {port}",
        )
    return Check(name="port", status="ok", message=f"127.0.0.1:{port} is available")


def auth_check() -> Check:
    require_auth = os.environ.get("ACP_REQUIRE_AUTH", "")
    admin_key = os.environ.get("ACP_ADMIN_API_KEY", "")
    enabled = require_auth == "1" or require_auth.lower() == "true"
    if not enabled and not admin_key:
        return Check(
            name="auth",
            status="warn",
            message="protected mode is off (fixture desktop/work still OK)",
            action=(
                "optional for provider-free use; for protected/dashboard auth run "
                "./scripts/harness.sh bootstrap-auth"
            ),
        )
    if enabled and not admin_key:
        return Check(
            name="auth",
            status="error",
            message="ACP_REQUIRE_AUTH is on but ACP_ADMIN_API_KEY is missing",
            action="run ./scripts/harness.sh bootstrap-auth and export the generated key",
        )
    if admin_key and not ADMIN_KEY_RE.match(admin_key):
        return Check(
            name="auth",
            status="error",
            message="ACP_ADMIN_API_KEY has invalid shape",
            action="expected harness_<64 hex characters> from ./scripts/harness.sh bootstrap-auth",
        )
    return Check(name="auth", status="ok", message="protected mode key shape is valid")


LIVE_CONFIRMATION = "I_CONFIRM_BOUNDED_LIVE_PROVIDER_COSTS"


def live_self_test_gate_check() -> Check:
    """Report live-model self-test readiness without reading credential values."""
    credential_env = os.environ.get("ACP_SELF_TEST_CREDENTIAL_ENV", "OPENAI_API_KEY").strip() or "OPENAI_API_KEY"
    confirm = os.environ.get("ACP_SELF_TEST_LIVE_CONFIRM", "").strip()
    credential_present = bool(os.environ.get(credential_env))
    confirm_ok = confirm == LIVE_CONFIRMATION
    if credential_present and confirm_ok:
        return Check(
            name="live_self_test",
            status="ok",
            message=(
                f"gates present (credential env {credential_env!r} set; "
                "confirmation matches; values never printed)"
            ),
        )
    missing: list[str] = []
    if not credential_present:
        missing.append(f"set symbolic credential env {credential_env}")
    if not confirm_ok:
        missing.append(f"export ACP_SELF_TEST_LIVE_CONFIRM={LIVE_CONFIRMATION}")
    return Check(
        name="live_self_test",
        status="warn",
        message="live model/perf self-test gates incomplete (fixture path still OK)",
        action="; ".join(missing) + "; then ./scripts/harness.sh self-test --mode live",
    )


def gh_check() -> Check:
    found = shutil.which("gh")
    if not found:
        return Check(
            name="gh",
            status="warn",
            message="gh CLI not found",
            action="install GitHub CLI for attestation and exact-head workflows: https://cli.github.com/",
        )
    version = command_version(found, ["--version"])
    # Attestation support is reported only from local capability, not inventing network success.
    help_result = subprocess.run(
        [found, "attestation", "--help"],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        timeout=5,
    )
    if help_result.returncode == 0:
        attest = "attestation subcommand available (local CLI)"
        status = "ok"
    else:
        attest = "attestation subcommand not available in this gh build"
        status = "warn"
    return Check(
        name="gh",
        status=status,
        message=f"{found} ({version}); {attest}",
        action=None if status == "ok" else "upgrade gh for `gh attestation verify` support",
    )


def collect_checks(repo_root: Path, port: int) -> list[Check]:
    home = Path.home()
    dashboard_out = repo_root / "dashboard" / "out" / "index.html"
    engine_bin = repo_root / "target" / "debug" / ("agent-control-plane.exe" if sys.platform == "win32" else "agent-control-plane")
    return [
        command_check("node", ["--version"]),
        command_check("npm", ["--version"]),
        command_check("bun", ["--version"], [home / ".bun" / "bin"]),
        command_check("cargo", ["--version"]),
        command_check("uv", ["--version"], [home / ".local" / "bin"]),
        gh_check(),
        file_check("dashboard_export", dashboard_out, "run: cd dashboard && node scripts/build-static.mjs"),
        file_check("engine_binary", engine_bin, "run: cargo build -p engine"),
        port_check(port),
        auth_check(),
        live_self_test_gate_check(),
    ]


def print_text(checks: list[Check]) -> None:
    width = max(len(check.name) for check in checks)
    for check in checks:
        print(f"{check.status.upper():5} {check.name.ljust(width)}  {check.message}")
        if check.action:
            print(f"      action: {check.action}")


def main() -> int:
    parser = argparse.ArgumentParser(description="Check local Agent Control Plane setup readiness.")
    parser.add_argument("--repo-root", type=Path, default=repo_root_from_script())
    parser.add_argument("--port", type=int, default=int(os.environ.get("PORT", "8080")))
    parser.add_argument("--json", action="store_true", help="Print machine-readable JSON.")
    args = parser.parse_args()

    checks = collect_checks(args.repo_root.resolve(), args.port)
    if args.json:
        print(json.dumps({"checks": [asdict(check) for check in checks]}, indent=2))
    else:
        print_text(checks)
    return 1 if any(check.status == "error" for check in checks) else 0


if __name__ == "__main__":
    raise SystemExit(main())
