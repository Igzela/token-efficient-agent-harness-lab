"""Fail-closed runtime controls stored on one dedicated GitHub Issue."""

from __future__ import annotations

import json
import os
import subprocess
import sys
from typing import Any, Iterable


CONTROL_ISSUE_TITLE = "[agent-control] Orchestrator controls"
CONTROL_MARKER = "<!-- agent-orchestrator-control:v1 -->"
CONTROL_LABEL = "agent-control"
ORCHESTRATOR_ENABLED_LABEL = "agent-orchestrator-enabled"
AUTO_MERGE_ENABLED_LABEL = "agent-auto-merge-enabled"
EMERGENCY_STOP_LABEL = "agent-emergency-stop"
CONTROL_LABELS = frozenset(
    {
        CONTROL_LABEL,
        ORCHESTRATOR_ENABLED_LABEL,
        AUTO_MERGE_ENABLED_LABEL,
        EMERGENCY_STOP_LABEL,
    }
)


class ControlStateError(RuntimeError):
    """Raised when control authority is unavailable or ambiguous."""


def _repo() -> str:
    repo = os.environ.get("AGENT_REPO") or os.environ.get("GITHUB_REPOSITORY")
    if not repo:
        raise ControlStateError("GITHUB_REPOSITORY is unavailable")
    return repo


def _run_gh(*args: str, input_text: str | None = None) -> str:
    result = subprocess.run(
        ["gh", *args],
        input=input_text,
        capture_output=True,
        text=True,
        timeout=30,
    )
    if result.returncode != 0:
        raise ControlStateError(result.stderr.strip() or "GitHub control operation failed")
    return result.stdout.strip()


def _issue_labels(issue: dict[str, Any]) -> set[str]:
    labels = issue.get("labels", [])
    if not isinstance(labels, list):
        return set()
    return {
        label["name"]
        for label in labels
        if isinstance(label, dict) and isinstance(label.get("name"), str)
    }


def resolve_control_issue(issues: Iterable[dict[str, Any]]) -> dict[str, Any]:
    """Return the one valid open control Issue, otherwise fail closed.

    The caller supplies the complete set of open Issues bearing the identity
    label.  Any malformed or ambiguous candidate is an authority failure.
    """

    candidates = list(issues)
    open_candidates = [issue for issue in candidates if issue.get("state") == "open"]
    if len(open_candidates) != 1:
        raise ControlStateError("exactly one open agent-control Issue is required")
    issue = open_candidates[0]
    labels = _issue_labels(issue)
    if (
        issue.get("title") != CONTROL_ISSUE_TITLE
        or CONTROL_MARKER not in str(issue.get("body") or "")
        or CONTROL_LABEL not in labels
        or not isinstance(issue.get("number"), int)
    ):
        raise ControlStateError("agent-control Issue is malformed")

    emergency_stop = EMERGENCY_STOP_LABEL in labels
    orchestrator_enabled = ORCHESTRATOR_ENABLED_LABEL in labels and not emergency_stop
    auto_merge_enabled = orchestrator_enabled and AUTO_MERGE_ENABLED_LABEL in labels
    return {
        "number": issue["number"],
        "title": CONTROL_ISSUE_TITLE,
        "labels": sorted(labels),
        "orchestrator_enabled": orchestrator_enabled,
        "auto_merge_enabled": auto_merge_enabled,
        "emergency_stop": emergency_stop,
    }


def read_control_state(repo: str | None = None) -> dict[str, Any]:
    target = repo or _repo()
    raw = _run_gh(
        "api",
        "--paginate",
        f"repos/{target}/issues?state=open&labels={CONTROL_LABEL}&per_page=100",
    )
    try:
        issues = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise ControlStateError("agent-control Issue response was invalid") from exc
    if not isinstance(issues, list):
        raise ControlStateError("agent-control Issue response was not a list")
    return resolve_control_issue(issues)


def require_live(repo: str | None = None) -> dict[str, Any]:
    state = read_control_state(repo)
    if state["emergency_stop"]:
        raise ControlStateError("agent control emergency stop is active")
    if not state["orchestrator_enabled"]:
        raise ControlStateError("agent orchestrator is disabled")
    return state


def require_auto_merge(repo: str | None = None) -> dict[str, Any]:
    state = require_live(repo)
    if not state["auto_merge_enabled"]:
        raise ControlStateError("agent auto-merge is disabled")
    return state


def _ensure_label(repo: str, name: str, description: str, color: str) -> None:
    raw = _run_gh("label", "list", "--repo", repo, "--limit", "100", "--json", "name")
    try:
        existing = {item["name"] for item in json.loads(raw)}
    except (json.JSONDecodeError, KeyError, TypeError) as exc:
        raise ControlStateError("unable to inspect repository labels") from exc
    if name not in existing:
        _run_gh("label", "create", name, "--repo", repo, "--color", color, "--description", description)


def setup(repo: str | None = None) -> dict[str, Any]:
    """Idempotently create the control labels and one disabled control Issue."""

    target = repo or _repo()
    for name, description, color in (
        (CONTROL_LABEL, "Identity label for the orchestrator control Issue", "5319e7"),
        (ORCHESTRATOR_ENABLED_LABEL, "Allows orchestrator work when emergency stop is absent", "0e8a16"),
        (AUTO_MERGE_ENABLED_LABEL, "Allows merge after all independent gates pass", "0e8a16"),
        (EMERGENCY_STOP_LABEL, "Stops all orchestrator transitions immediately", "e11d48"),
    ):
        _ensure_label(target, name, description, color)

    raw = _run_gh(
        "api",
        "--paginate",
        f"repos/{target}/issues?state=open&labels={CONTROL_LABEL}&per_page=100",
    )
    try:
        issues = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise ControlStateError("agent-control Issue response was invalid") from exc
    if not issues:
        _run_gh(
            "issue",
            "create",
            "--repo",
            target,
            "--title",
            CONTROL_ISSUE_TITLE,
            "--body",
            f"{CONTROL_MARKER}\n\nLabels on this Issue are the live orchestrator controls.",
            "--label",
            f"{CONTROL_LABEL},{EMERGENCY_STOP_LABEL}",
        )
    return read_control_state(target)


def mutate_control_labels(command: str, repo: str | None = None) -> dict[str, Any]:
    target = repo or _repo()
    state = read_control_state(target)
    number = str(state["number"])
    if command == "enable-orchestrator":
        add, remove = [ORCHESTRATOR_ENABLED_LABEL], [EMERGENCY_STOP_LABEL]
    elif command == "disable-orchestrator":
        add, remove = [], [ORCHESTRATOR_ENABLED_LABEL, AUTO_MERGE_ENABLED_LABEL]
    elif command == "enable-auto-merge":
        add, remove = [AUTO_MERGE_ENABLED_LABEL], []
    elif command == "disable-auto-merge":
        add, remove = [], [AUTO_MERGE_ENABLED_LABEL]
    elif command == "emergency-stop":
        add, remove = [EMERGENCY_STOP_LABEL], []
    elif command == "emergency-resume":
        add, remove = [], [EMERGENCY_STOP_LABEL]
    else:
        raise ControlStateError(f"unknown control command: {command}")
    args = ["issue", "edit", number, "--repo", target]
    if add:
        args.extend(["--add-label", ",".join(add)])
    if remove:
        args.extend(["--remove-label", ",".join(remove)])
    _run_gh(*args)
    return read_control_state(target)


def main() -> None:
    if len(sys.argv) < 2:
        print(
            "Usage: control_state.py <setup|setup-controls|status|read|require-live|require-auto-merge|"
            "enable-orchestrator|disable-orchestrator|enable-auto-merge|disable-auto-merge|"
            "emergency-stop|emergency-resume> [--repo OWNER/REPO]",
            file=sys.stderr,
        )
        raise SystemExit(2)
    command, arguments = sys.argv[1], sys.argv[2:]
    if len(arguments) == 0:
        repo = None
    elif len(arguments) == 1 and not arguments[0].startswith("-"):
        repo = arguments[0]
    elif len(arguments) == 2 and arguments[0] == "--repo":
        repo = arguments[1]
    else:
        print("invalid repository argument", file=sys.stderr)
        raise SystemExit(2)
    try:
        if command in {"setup", "setup-controls"}:
            state = setup(repo)
        elif command in {"status", "read"}:
            state = read_control_state(repo)
        elif command == "require-live":
            state = require_live(repo)
        elif command == "require-auto-merge":
            state = require_auto_merge(repo)
        else:
            state = mutate_control_labels(command, repo)
        print(json.dumps(state, sort_keys=True))
    except ControlStateError as exc:
        print(f"CONTROL_STATE_ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc


if __name__ == "__main__":
    main()
