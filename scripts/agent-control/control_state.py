"""Read authoritative GitHub Actions control variables at operation time."""

from __future__ import annotations

import json
import os
import subprocess
import sys
from typing import Any


CONTROL_VARIABLES = frozenset(
    {
        "AGENT_ORCHESTRATOR_ENABLED",
        "AGENT_AUTO_MERGE_ENABLED",
        "AGENT_EMERGENCY_STOP",
    }
)


def _repo() -> str:
    repo = os.environ.get("AGENT_REPO") or os.environ.get("GITHUB_REPOSITORY")
    if not repo:
        raise RuntimeError("GITHUB_REPOSITORY is unavailable")
    return repo


def read_variable(name: str, repo: str | None = None) -> str:
    if name not in CONTROL_VARIABLES:
        raise ValueError(f"unknown control variable: {name}")
    target = repo or _repo()
    result = subprocess.run(
        [
            "gh",
            "api",
            f"repos/{target}/actions/variables/{name}",
            "--jq",
            ".value",
        ],
        capture_output=True,
        text=True,
        timeout=30,
    )
    if result.returncode != 0 or not result.stdout.strip():
        raise RuntimeError(f"control variable query failed: {name}")
    return result.stdout.strip()


def read_control_state(repo: str | None = None) -> dict[str, Any]:
    values = {name: read_variable(name, repo) for name in sorted(CONTROL_VARIABLES)}
    return {
        "orchestrator_enabled": values["AGENT_ORCHESTRATOR_ENABLED"].lower() == "true",
        "auto_merge_enabled": values["AGENT_AUTO_MERGE_ENABLED"].lower() == "true",
        "emergency_stop": values["AGENT_EMERGENCY_STOP"].lower() == "true",
        "variables": values,
    }


def require_live(repo: str | None = None) -> dict[str, Any]:
    state = read_control_state(repo)
    if not state["orchestrator_enabled"]:
        raise RuntimeError("AGENT_ORCHESTRATOR_ENABLED is not true")
    if state["emergency_stop"]:
        raise RuntimeError("AGENT_EMERGENCY_STOP is true")
    return state


def require_auto_merge(repo: str | None = None) -> dict[str, Any]:
    state = require_live(repo)
    if not state["auto_merge_enabled"]:
        raise RuntimeError("AGENT_AUTO_MERGE_ENABLED is not true")
    return state


def main() -> None:
    if len(sys.argv) not in {2, 3}:
        print(
            "Usage: control_state.py <read|require-live|require-auto-merge|check-emergency-stop> [repo]",
            file=sys.stderr,
        )
        sys.exit(2)
    command = sys.argv[1]
    repo = sys.argv[2] if len(sys.argv) == 3 else None
    try:
        if command == "read":
            result = read_control_state(repo)
        elif command == "require-live":
            result = require_live(repo)
        elif command == "require-auto-merge":
            result = require_auto_merge(repo)
        elif command == "check-emergency-stop":
            state = read_control_state(repo)
            result = {"emergency_stop": state["emergency_stop"]}
        else:
            raise ValueError(f"unknown command: {command}")
        print(json.dumps(result, sort_keys=True))
    except (RuntimeError, ValueError) as exc:
        print(f"CONTROL_STATE_ERROR: {exc}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
