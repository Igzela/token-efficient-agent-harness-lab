#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import secrets
from pathlib import Path


def repo_root_from_script() -> Path:
    return Path(__file__).resolve().parents[1]


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate a local admin API key and print protected-mode startup commands.",
    )
    parser.add_argument("--repo-root", type=Path, default=repo_root_from_script())
    parser.add_argument("--json", action="store_true", help="Print machine-readable JSON.")
    args = parser.parse_args()

    repo_root = args.repo_root.resolve()
    raw_key = f"harness_{secrets.token_hex(32)}"
    env = {
        "ACP_REQUIRE_AUTH": "1",
        "ACP_ADMIN_API_KEY": raw_key,
        "ACP_DASHBOARD_DIR": str(repo_root / "dashboard" / "out"),
    }
    command = (
        f"ACP_REQUIRE_AUTH=1 ACP_ADMIN_API_KEY={raw_key} "
        f"ACP_DASHBOARD_DIR={env['ACP_DASHBOARD_DIR']} cargo run -p engine"
    )

    if args.json:
        print(json.dumps({"env": env, "command": command}, indent=2))
        return 0

    print("Generated local admin key. It is not stored by this script.")
    print()
    for key, value in env.items():
        print(f'export {key}="{value}"')
    print()
    print("Start protected local runtime:")
    print(command)
    print()
    print("Paste the generated key into the dashboard auth panel when prompted.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
