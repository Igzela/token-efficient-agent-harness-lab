"""Fail CI if canonical tests stop executing orchestrator regressions."""

from __future__ import annotations

import sys
from pathlib import Path


REQUIRED = (
    "tests/test_agent_control_ci.py",
    "tests/test_agent_control_dry_run.py",
    "tests/test_agent_control_state.py",
    "tests/test_agent_control_worktree.py",
    "tests/test_agent_orchestrator_repairs.py",
    "tests/test_agent_orchestrator_artifacts.py",
)


def main() -> None:
    workflow = Path(".github/workflows/tests.yml")
    source = workflow.read_text(encoding="utf-8")
    missing = [path for path in REQUIRED if path not in source]
    absent = [path for path in REQUIRED if not Path(path).is_file()]
    if missing or absent:
        raise SystemExit(f"canonical orchestrator test suite is incomplete; missing workflow={missing}, files={absent}")


if __name__ == "__main__":
    main()
