"""Fail CI if canonical tests stop executing orchestrator regressions."""

from __future__ import annotations

import sys
from pathlib import Path


REQUIRED = (
    "tests/test_agent_shadow_steward.py",
    "tests/test_agent_steward.py",
    "tests/test_agent_steward_faults.py",
    "tests/test_agent_steward_journal.py",
    "tests/test_mission_contract.py",
    "tests/test_review_convergence.py",
    "tests/test_review_loop.py",
    "tests/test_steward_deferred_acceptance.py",
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
