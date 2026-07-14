#!/usr/bin/env python3
"""Fail-closed readiness checks for the repository's self-hosted runner.

This checker deliberately does not use the Actions runner's ``config.sh``.
It verifies only bounded local metadata and the public runner status API; it
never reads or prints runner credential files.
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import re
import subprocess
import sys
from typing import Any, Sequence


REQUIRED_FILES = (".runner", ".credentials", ".credentials_rsaparams")
REQUIRED_LABELS = frozenset({"self-hosted", "vader", "agent-worker"})
SERVICE_PREFIX = "actions.runner."
SERVICE_SUFFIX = ".service"
COMMAND_TIMEOUT_SECONDS = 15
MAX_STATUS_BYTES = 4096


class ReadinessError(Exception):
    """A bounded, non-secret readiness failure."""


def _run_command(
    argv: Sequence[str], *, cwd: pathlib.Path | None = None, env: dict[str, str] | None = None
) -> str:
    command_env = None if env is None else dict(env)
    try:
        completed = subprocess.run(
            list(argv),
            cwd=cwd,
            env=command_env,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            timeout=COMMAND_TIMEOUT_SECONDS,
        )
    except (OSError, subprocess.SubprocessError):
        raise ReadinessError("command_unavailable") from None
    if completed.returncode != 0:
        raise ReadinessError("command_failed")
    return completed.stdout


def _service_name(repo: str, runner_name: str) -> str:
    return f"{SERVICE_PREFIX}{repo.replace('/', '-')}.{runner_name}{SERVICE_SUFFIX}"


def _validate_arguments(repo: str, runner_name: str) -> None:
    if re.fullmatch(r"[^/\s]+/[^/\s]+", repo) is None:
        raise ReadinessError("repository_invalid")
    if not runner_name or len(runner_name) > 128 or any(char in runner_name for char in "\r\n\x00"):
        raise ReadinessError("runner_name_invalid")


def _check_local_configuration(root: pathlib.Path) -> None:
    if not root.is_dir():
        raise ReadinessError("runner_root_missing")
    for filename in REQUIRED_FILES:
        path = root / filename
        try:
            stat = path.stat()
        except OSError:
            raise ReadinessError("runner_configuration_missing") from None
        if not path.is_file() or stat.st_size <= 0:
            raise ReadinessError("runner_configuration_missing")


def _check_listener(root: pathlib.Path) -> None:
    listener = root / "bin" / "Runner.Listener"
    if not listener.is_file() or not os.access(listener, os.X_OK):
        raise ReadinessError("runner_listener_missing")
    try:
        _run_command([str(listener), "--version"], cwd=root)
    except ReadinessError:
        raise ReadinessError("runner_listener_unavailable") from None


def _systemctl_environment(scope: str) -> dict[str, str] | None:
    if scope != "user":
        return None
    environment = dict(os.environ)
    runtime_dir = environment.get("XDG_RUNTIME_DIR") or f"/run/user/{os.getuid()}"
    environment.setdefault("XDG_RUNTIME_DIR", runtime_dir)
    environment.setdefault("DBUS_SESSION_BUS_ADDRESS", f"unix:path={runtime_dir}/bus")
    return environment


def _service_state(scope: str, service_name: str) -> dict[str, str] | None:
    command = ["systemctl"]
    if scope == "user":
        command.append("--user")
    command.extend(
        [
            "show",
            service_name,
            "--property=LoadState,ActiveState,SubState",
            "--no-pager",
        ]
    )
    try:
        output = _run_command(command, env=_systemctl_environment(scope))
    except ReadinessError:
        raise ReadinessError("service_status_unavailable") from None
    fields: dict[str, str] = {}
    for line in output.splitlines():
        key, separator, value = line.partition("=")
        if separator and key in {"LoadState", "ActiveState", "SubState"} and value:
            fields[key] = value
    if set(fields) != {"LoadState", "ActiveState", "SubState"}:
        return None
    return fields


def _check_service(service_name: str) -> str:
    states = {scope: _service_state(scope, service_name) for scope in ("user", "system")}
    loaded = [scope for scope, state in states.items() if state and state["LoadState"] == "loaded"]
    if len(loaded) != 1:
        raise ReadinessError("service_layout_invalid")
    state = states[loaded[0]]
    if state["ActiveState"] != "active" or state["SubState"] != "running":
        raise ReadinessError("service_not_active")
    return loaded[0]


def _read_runners(repo: str) -> list[dict[str, Any]]:
    endpoint = f"repos/{repo}/actions/runners"
    try:
        output = _run_command(["gh", "api", "--paginate", "--slurp", endpoint])
        pages = json.loads(output)
    except (ReadinessError, json.JSONDecodeError, TypeError, ValueError):
        raise ReadinessError("github_api_unavailable") from None
    if not isinstance(pages, list) or not pages:
        raise ReadinessError("github_api_unavailable")
    runners: list[dict[str, Any]] = []
    expected_total: int | None = None
    for page in pages:
        if (
            not isinstance(page, dict)
            or type(page.get("total_count")) is not int
            or page["total_count"] < 0
            or not isinstance(page.get("runners"), list)
        ):
            raise ReadinessError("github_api_malformed")
        if expected_total is None:
            expected_total = page["total_count"]
        elif page["total_count"] != expected_total:
            raise ReadinessError("github_api_contradictory")
        for runner in page["runners"]:
            if not isinstance(runner, dict):
                raise ReadinessError("github_api_malformed")
            if type(runner.get("id")) is not int or runner["id"] <= 0:
                raise ReadinessError("github_api_malformed")
            if not isinstance(runner.get("name"), str) or not runner["name"]:
                raise ReadinessError("github_api_malformed")
            if runner.get("status") not in {"online", "offline"}:
                raise ReadinessError("github_api_malformed")
            if type(runner.get("busy")) is not bool:
                raise ReadinessError("github_api_malformed")
            labels = runner.get("labels")
            if not isinstance(labels, list):
                raise ReadinessError("github_api_malformed")
            for label in labels:
                if not isinstance(label, dict) or not isinstance(label.get("name"), str):
                    raise ReadinessError("github_api_malformed")
            runners.append(runner)
    if expected_total != len(runners):
        raise ReadinessError("github_api_partial")
    runner_ids = [runner["id"] for runner in runners]
    if len(set(runner_ids)) != len(runner_ids):
        raise ReadinessError("github_api_contradictory")
    return runners


def _check_github_runner(repo: str, runner_name: str, allow_busy: bool) -> dict[str, Any]:
    matches = [runner for runner in _read_runners(repo) if runner["name"] == runner_name]
    if len(matches) != 1:
        raise ReadinessError("runner_identity_ambiguous" if len(matches) > 1 else "runner_missing")
    runner = matches[0]
    if runner["status"] != "online":
        raise ReadinessError("runner_offline")
    if runner["busy"] and not allow_busy:
        raise ReadinessError("runner_busy")
    label_names = {label["name"] for label in runner["labels"]}
    if not REQUIRED_LABELS.issubset(label_names):
        raise ReadinessError("runner_labels_invalid")
    return {
        "runner_id": runner.get("id") if isinstance(runner.get("id"), int) else None,
        "status": runner["status"],
        "busy": runner["busy"],
        "labels": sorted(label_names & (REQUIRED_LABELS | {"Linux", "X64"})),
    }


def check_readiness(
    *, repo: str, runner_root: pathlib.Path | str, runner_name: str, allow_busy: bool = False
) -> dict[str, Any]:
    """Return bounded status JSON data; never return credential contents."""

    root = pathlib.Path(runner_root).expanduser()
    base: dict[str, Any] = {
        "ready": False,
        "repo": repo[:256],
        "runner_name": runner_name[:128],
    }
    try:
        _validate_arguments(repo, runner_name)
        _check_local_configuration(root)
        _check_listener(root)
        service_name = _service_name(repo, runner_name)
        service_layout = _check_service(service_name)
        github = _check_github_runner(repo, runner_name, allow_busy)
        base.update(
            {
                "ready": True,
                "service_layout": service_layout,
                "service_name": service_name,
                **github,
            }
        )
    except ReadinessError as error:
        base["reason"] = str(error)
    serialized = json.dumps(base, sort_keys=True, separators=(",", ":"))
    if len(serialized.encode("utf-8")) > MAX_STATUS_BYTES:
        return {
            "ready": False,
            "repo": base["repo"],
            "runner_name": base["runner_name"],
            "reason": "status_too_large",
        }
    return base


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--runner-root", required=True)
    parser.add_argument("--runner-name", required=True)
    parser.add_argument("--allow-busy", action="store_true")
    args = parser.parse_args(argv)
    status = check_readiness(
        repo=args.repo,
        runner_root=args.runner_root,
        runner_name=args.runner_name,
        allow_busy=args.allow_busy,
    )
    print(json.dumps(status, sort_keys=True, separators=(",", ":")))
    return 0 if status.get("ready") else 1


if __name__ == "__main__":
    sys.exit(main())
