#!/usr/bin/env python3
"""Run the engine suite in parallel only after the env-lock contract is present."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import re


LOCK_NAMES = (
    "auto_adjustment_env_lock",
    "provider_cli_env_lock",
    "adaptive_operator_env_lock",
    "target_repo_output_env_lock",
)
AUTH_TESTS = (
    "axum_local_store_persists_dispatch_history_and_dashboard_summary",
    "axum_local_store_exposes_team_config_costs_and_export",
)


def _read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError:
        return ""


def parallel_contract_present(root: Path) -> bool:
    harness = _read(root / "engine/tests/test_http_server.rs")
    auth = _read(root / "engine/tests/http_server/auth.rs")
    common = _read(root / "engine/tests/http_server/common.rs")
    if not harness or not auth or not common:
        return False
    for name in LOCK_NAMES:
        delegated = re.search(
            rf"fn\s+{name}\(\)\s*->[^{{]+\{{\s*common::{name}\(\)\s*\}}",
            harness,
            re.DOTALL,
        )
        canonical = re.search(
            rf"pub\(crate\)\s+fn\s+{name}\(\)\s*->",
            common,
        )
        if not delegated or not canonical:
            return False
    for test_name in AUTH_TESTS:
        function = re.search(
            rf"async\s+fn\s+{test_name}\(\)\s*\{{(?P<body>.{{0,800}})",
            auth,
            re.DOTALL,
        )
        if not function or not re.search(
            r"provider_cli_env_lock\(\)\.lock\(\)\.await",
            function.group("body"),
        ):
            return False
    return True


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--repository-root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
    )
    parser.add_argument("--print-mode", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    parallel = parallel_contract_present(args.repository_root.resolve())
    mode = "parallel" if parallel else "serial"
    print(f"engine test mode: {mode}", flush=True)
    if args.print_mode:
        return 0
    command = ["cargo", "test", "-p", "engine"]
    if not parallel:
        command.extend(("--", "--test-threads=1"))
    os.execvp(command[0], command)
    return 127


if __name__ == "__main__":
    raise SystemExit(main())
