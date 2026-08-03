"""Repository-owned focused checks for the local run-once path.

Model output is untrusted.  After a worker mutates a worktree, only commands
from this module may run, and only after every changed path is classified by
the fail-closed supported-path contract below.  Free-text verification from
Issues, plan markers, or model transcripts is never executed as a shell.
"""

from __future__ import annotations

from pathlib import Path
import re
import subprocess
import sys
from typing import Any, Callable


class LocalVerificationError(RuntimeError):
    """A repository-owned focused check failed or a path is unsupported."""

    def __init__(self, reason: str):
        super().__init__(reason)
        self.reason = reason


# Display string → argv.  The display is what the artifact records; argv is
# what the process actually executes.  Keep displays stable for evidence.
_ALLOWLIST: dict[str, list[str]] = {
    "git diff --check": ["git", "diff", "--check"],
    "python -m unittest discover -s tests -p test_agent_*.py": [
        sys.executable,
        "-m",
        "unittest",
        "discover",
        "-s",
        "tests",
        "-p",
        "test_agent_*.py",
    ],
    "cargo fmt --all -- --check": ["cargo", "fmt", "--all", "--", "--check"],
    "cargo clippy -p engine --all-targets --all-features -- -D warnings": [
        "cargo",
        "clippy",
        "-p",
        "engine",
        "--all-targets",
        "--all-features",
        "--",
        "-D",
        "warnings",
    ],
    "cargo test -p engine": ["cargo", "test", "-p", "engine"],
    "bun run typecheck": ["bun", "run", "typecheck"],
    "bun test": ["bun", "test"],
    "python tools/check_security_baseline.py": [
        sys.executable,
        "tools/check_security_baseline.py",
    ],
    "python scripts/check_agent_handoff.py": [
        sys.executable,
        "scripts/check_agent_handoff.py",
    ],
    "bash scripts/check_wire_codegen_drift.sh": [
        "bash",
        "scripts/check_wire_codegen_drift.sh",
    ],
}

# Path classes that this local-loop v1 may accept.  Any changed path that does
# not match a classifier is rejected before artifact/commit/push.
_PROSE_PATH = re.compile(
    r"^(?:"
    r"docs/.+\.md"
    r"|README\.md"
    r"|START_HERE\.md"
    r"|AGENTS\.md"
    r"|CLAUDE\.md"
    r"|LICENSE"
    r"|CHANGELOG\.md"
    r")$"
)
_AGENT_PYTHON_PATH = re.compile(
    r"^(?:"
    r"scripts/agent-control/.+\.py"
    r"|tests/test_agent_[^/]+\.py"
    r"|tools/check_security_baseline\.py"
    r"|tools/test_[^/]+\.py"
    r")$"
)
_RUST_PATH = re.compile(
    r"^(?:"
    r"engine/.+"
    r"|Cargo\.toml"
    r"|Cargo\.lock"
    r"|rust-toolchain(?:\.toml)?"
    r")$"
)
_DASHBOARD_PATH = re.compile(
    r"^(?:"
    r"dashboard/(?!package-lock\.json).+"
    r")$"
)
_WORKFLOW_OR_SHELL_PATH = re.compile(
    r"^(?:"
    r"\.github/workflows/.+\.ya?ml"
    r"|scripts/agent-control/.+\.sh"
    r"|scripts/[^/]+\.sh"
    r")$"
)
_WIRE_PATH = re.compile(
    r"^(?:"
    r"engine/src/wire/.+"
    r"|scripts/generate_.+\.py"
    r"|scripts/check_wire_codegen_drift\.sh"
    r")$"
)
_HANDOFF_DOC_PATH = re.compile(
    r"^(?:"
    r"docs/(?:CURRENT_STATUS|NEXT_DECISION|MODULE_MAP|ARCHITECTURE_BOOK|"
    r"REAL_WORLD_TESTING_PLAYBOOK|RUNBOOK)\.md"
    r"|START_HERE\.md"
    r"|AGENTS\.md"
    r"|CLAUDE\.md"
    r"|README\.md"
    r")$"
)

# Explicitly rejected dependency/lock/config islands that must never ride on
# prose-only checks even if a broader prefix might match later.
_REJECTED_PATH = re.compile(
    r"^(?:"
    r"dashboard/package-lock\.json"
    r"|package-lock\.json"
    r"|pnpm-lock\.yaml"
    r"|yarn\.lock"
    r"|engine\.pid"
    r"|\.codegraph(/.*)?"
    r"|.*\.(?:sqlite|db|pem|key|env)$"
    r"|.*credentials.*"
    r")$"
)


def allowlisted_command(display: str) -> list[str] | None:
    if not isinstance(display, str):
        return None
    return _ALLOWLIST.get(display.strip())


def _classify_path(path: str) -> set[str]:
    """Return the set of check classes required by one relative path."""

    if not path or path.startswith("/") or ".." in Path(path).parts:
        raise LocalVerificationError(f"path_unsafe:{path[:120]}")
    if _REJECTED_PATH.match(path):
        raise LocalVerificationError(f"path_rejected:{path[:120]}")
    classes: set[str] = set()
    matched = False
    if _PROSE_PATH.match(path):
        classes.add("prose")
        matched = True
    if _AGENT_PYTHON_PATH.match(path):
        classes.add("agent_python")
        matched = True
    if _RUST_PATH.match(path):
        classes.add("rust")
        matched = True
    if _DASHBOARD_PATH.match(path):
        classes.add("dashboard")
        matched = True
    if _WORKFLOW_OR_SHELL_PATH.match(path):
        classes.add("automation")
        matched = True
    if _WIRE_PATH.match(path):
        classes.add("wire")
        matched = True
    if _HANDOFF_DOC_PATH.match(path):
        classes.add("handoff_docs")
        matched = True
    if not matched:
        raise LocalVerificationError(f"path_unsupported:{path[:120]}")
    return classes


def classify_changed_paths(changed: list[str]) -> set[str]:
    if not isinstance(changed, list):
        raise LocalVerificationError("changed_paths_invalid")
    if not changed:
        # No mutation: still require whitespace/conflict hygiene.
        return {"prose"}
    classes: set[str] = set()
    for path in changed:
        if not isinstance(path, str) or not path.strip():
            raise LocalVerificationError("changed_path_invalid")
        classes |= _classify_path(path.strip())
    return classes


def checks_for_classes(classes: set[str]) -> list[str]:
    """Map path classes to an ordered, deduplicated allowlist of displays."""

    selected: list[str] = ["git diff --check"]
    if "agent_python" in classes:
        selected.append("python -m unittest discover -s tests -p test_agent_*.py")
    if "rust" in classes:
        selected.extend(
            [
                "cargo fmt --all -- --check",
                "cargo clippy -p engine --all-targets --all-features -- -D warnings",
                "cargo test -p engine",
            ]
        )
    if "dashboard" in classes:
        selected.extend(["bun run typecheck", "bun test"])
    if "automation" in classes:
        selected.append("python tools/check_security_baseline.py")
    if "wire" in classes:
        selected.append("bash scripts/check_wire_codegen_drift.sh")
    if "handoff_docs" in classes:
        selected.append("python scripts/check_agent_handoff.py")
    # prose-only is covered by git diff --check alone.
    for display in selected:
        if allowlisted_command(display) is None:
            raise LocalVerificationError(f"focused_check_not_allowlisted:{display[:80]}")
    return selected


def select_issue_checks(changed: list[str]) -> list[str]:
    """Select repository-owned checks from the observed path set."""

    return checks_for_classes(classify_changed_paths(changed))


def select_plan_checks(verification: list[str], changed: list[str]) -> list[str]:
    """Plan lane remains deferred; keep selector for non-admitted tests only."""

    selected = select_issue_checks(changed)
    if not isinstance(verification, list):
        raise LocalVerificationError("plan_verification_invalid")
    for entry in verification:
        if not isinstance(entry, str) or not entry.strip():
            raise LocalVerificationError("plan_verification_invalid")
        display = entry.strip()
        if allowlisted_command(display) is None:
            raise LocalVerificationError(
                f"plan_verification_not_allowlisted:{display[:80]}"
            )
        if display not in selected:
            selected.append(display)
    return selected


def changed_paths(worktree: Path) -> list[str]:
    """Return tracked+untracked paths changed relative to HEAD."""

    try:
        tracked = subprocess.run(
            ["git", "diff", "--name-only", "HEAD"],
            cwd=worktree,
            capture_output=True,
            text=True,
            timeout=60,
            check=False,
        )
        untracked = subprocess.run(
            ["git", "ls-files", "--others", "--exclude-standard"],
            cwd=worktree,
            capture_output=True,
            text=True,
            timeout=60,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise LocalVerificationError("focused_check_git_unavailable") from exc
    if tracked.returncode != 0 or untracked.returncode != 0:
        raise LocalVerificationError("focused_check_git_failed")
    paths: list[str] = []
    for block in (tracked.stdout, untracked.stdout):
        for line in block.splitlines():
            value = line.strip()
            if value and value not in paths:
                paths.append(value)
    return paths


def run_focused_checks(
    worktree: Path,
    displays: list[str],
    *,
    timeout_seconds: int = 1800,
    runner: Callable[..., tuple[int, str, str]] | None = None,
) -> list[dict[str, Any]]:
    """Execute allowlisted checks and return the artifact ``local_checks`` list.

    Checks run through the same sanitized-env, isolated-session, tree-kill
    adapter as the model child so a hung cargo/bun descendant cannot leak
    after timeout and cannot inherit GitHub/provider credentials.
    """

    if not displays:
        raise LocalVerificationError("focused_checks_empty")
    # Lazy import avoids a circular dependency with local_run_once.
    import local_loop
    import local_run_once

    run = runner or (
        lambda argv, cwd, timeout_seconds: local_run_once._bounded_process(
            argv, cwd=cwd, timeout_seconds=timeout_seconds
        )
    )
    results: list[dict[str, Any]] = []
    for display in displays:
        argv = allowlisted_command(display)
        if argv is None:
            raise LocalVerificationError(f"focused_check_not_allowlisted:{display[:80]}")
        try:
            exit_code, _stdout, _stderr = run(
                argv, cwd=worktree, timeout_seconds=timeout_seconds
            )
        except (OSError, subprocess.TimeoutExpired, local_loop.LoopUnavailable) as exc:
            raise LocalVerificationError(
                f"focused_check_unavailable:{display[:80]}"
            ) from exc
        # Record the exact display (stable) which is 1:1 with allowlisted argv.
        results.append({"command": display, "exit_code": int(exit_code)})
        if exit_code == 124:
            raise LocalVerificationError(
                f"focused_check_timeout:{display}"
            )
        if exit_code != 0:
            raise LocalVerificationError(
                f"focused_check_failed:{display}:{exit_code}"
            )
    return results


def run_issue_focused_checks(worktree: Path) -> list[dict[str, Any]]:
    paths = changed_paths(worktree)
    return run_focused_checks(worktree, select_issue_checks(paths))


def run_plan_focused_checks(
    worktree: Path, verification: list[str]
) -> list[dict[str, Any]]:
    paths = changed_paths(worktree)
    return run_focused_checks(
        worktree, select_plan_checks(verification, paths)
    )
