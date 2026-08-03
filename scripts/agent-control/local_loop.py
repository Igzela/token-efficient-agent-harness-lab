"""Read-only front edge for a GitHub-controlled, locally executed agent loop.

The repository and GitHub remain authoritative.  This module performs one
bounded poll and returns a versioned decision; it owns no daemon, lease, task
state, approval, budget, output, review, merge, or retry authority.
"""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile
import time
import uuid
from dataclasses import dataclass, field
from typing import Any, Callable, Protocol

import artifact_contract
import control_state
import pr_binding
import prompt_builder
import state_manager
import worktree_manager


POLL_KIND = "repo-agent-loop-poll.v1"
TASK_MARKER = re.compile(r"<!--\s*repo-agent-task:v1\s*(\{.*?\})\s*-->", re.DOTALL)
REPOSITORY = re.compile(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")
BRANCH = re.compile(r"^[A-Za-z0-9._/-]{1,200}$")
HEX40 = re.compile(r"^[0-9a-f]{40}$")


class LoopUnavailable(RuntimeError):
    """Raised when a required live fact cannot be read unambiguously."""


class GitHubReader(Protocol):
    def read_control_state(self) -> dict[str, Any]: ...
    def repository_metadata(self) -> dict[str, Any]: ...
    def current_user(self) -> str: ...
    def accepted_main_sha(self, branch: str) -> str: ...
    def list_ready_issues(self) -> list[dict[str, Any]]: ...
    def labels_for_issue(self, issue_number: int) -> set[str]: ...
    def has_open_issue_pr(self, issue_number: int) -> bool: ...
    def active_issue_scopes(self) -> dict[int, list[str]]: ...

    def issue_snapshot(self, issue_number: int) -> dict[str, str]: ...

    def dispatch_controller(self, command: str, fields: dict[str, object]) -> None: ...


class GitReader(Protocol):
    def origin_main_sha(self, repo_path: Path, branch: str) -> str: ...


@dataclass(frozen=True)
class LocalRunOnceResult:
    """Bounded result for one local attempt; durable state remains on GitHub."""

    status: str
    issue_number: int
    attempt_id: str
    details: dict[str, Any] = field(default_factory=dict)

    def to_wire(self) -> dict[str, Any]:
        return {
            "kind": "repo-agent-local-run-once.v1",
            "status": self.status,
            "issue_number": self.issue_number,
            "attempt_id": self.attempt_id,
            "details": self.details,
        }


def _decision(status: str, *, action: str = "none", **fields: Any) -> dict[str, Any]:
    return {
        "kind": POLL_KIND,
        "status": status,
        "action": action,
        "selected": [],
        "rejected": [],
        "deferred_issue_numbers": [],
        **fields,
    }


def task_main_sha(body: str) -> str:
    """Parse the task body's accepted-main binding marker.

    This is the canonical parser for the ``repo-agent-task:v1`` marker; the
    trusted controller gateway (``dispatcher.claim_local``) reuses it so the
    local loop and the server-side claim gate can never disagree about which
    marker binds a task to its accepted main SHA.
    """

    matches = TASK_MARKER.findall(body or "")
    if len(matches) != 1:
        raise ValueError("task must contain exactly one repo-agent-task.v1 marker")
    try:
        value = json.loads(matches[0])
    except json.JSONDecodeError as exc:
        raise ValueError("repo-agent-task.v1 marker is invalid JSON") from exc
    if not isinstance(value, dict) or set(value) != {"accepted_main_sha"}:
        raise ValueError("repo-agent-task.v1 must contain only accepted_main_sha")
    sha = value["accepted_main_sha"]
    if not isinstance(sha, str) or not HEX40.fullmatch(sha):
        raise ValueError("repo-agent-task.v1 accepted_main_sha must be 40 lowercase hex")
    return sha


def _normalized_issue(raw: dict[str, Any]) -> dict[str, Any]:
    try:
        number = int(raw["number"])
        title = raw["title"]
        url = raw["url"]
        author = raw["author"]
        body = raw["body"]
        labels = raw["labels"]
    except (KeyError, TypeError, ValueError) as exc:
        raise LoopUnavailable("GitHub returned a malformed ready Issue") from exc
    if (
        number <= 0
        or not isinstance(title, str)
        or not isinstance(url, str)
        or not isinstance(author, str)
        or not isinstance(body, str)
        or not isinstance(labels, list)
        or not all(isinstance(label, str) and label for label in labels)
    ):
        raise LoopUnavailable("GitHub returned a malformed ready Issue")
    return {
        "number": number,
        "title": title,
        "url": url,
        "author": author,
        "body": body,
        "labels": set(labels),
    }


class LoopController:
    """Deep read-only module for one deterministic local-loop poll."""

    def __init__(
        self,
        github: GitHubReader,
        git: GitReader,
        *,
        repository: str,
        repo_path: Path,
        max_active: int = state_manager.MAX_ACTIVE,
    ) -> None:
        if not REPOSITORY.fullmatch(repository):
            raise ValueError("repository must be owner/name")
        if max_active < 1 or max_active > state_manager.MAX_ACTIVE:
            raise ValueError(
                f"max_active must be between 1 and {state_manager.MAX_ACTIVE}"
            )
        self.github = github
        self.git = git
        self.repository = repository
        self.repo_path = Path(repo_path).expanduser().resolve()
        self.max_active = max_active

    def poll(self) -> dict[str, Any]:
        try:
            control = self.github.read_control_state()
            if control.get("emergency_stop") or not control.get("orchestrator_enabled"):
                return _decision("control_stopped", reason="orchestrator_disabled_or_stopped")

            metadata = self.github.repository_metadata()
            if str(metadata.get("name_with_owner", "")).casefold() != self.repository.casefold():
                return _decision("identity_rejected", reason="repository_identity_mismatch")
            owner = metadata.get("owner")
            current_user = self.github.current_user()
            if not isinstance(owner, str) or current_user.casefold() != owner.casefold():
                return _decision("identity_rejected", reason="authenticated_user_is_not_owner")
            branch = metadata.get("default_branch")
            if not isinstance(branch, str) or not BRANCH.fullmatch(branch):
                raise LoopUnavailable("repository default branch is unavailable or invalid")

            accepted_main = self.github.accepted_main_sha(branch)
            local_main = self.git.origin_main_sha(self.repo_path, branch)
            if not HEX40.fullmatch(accepted_main) or not HEX40.fullmatch(local_main):
                raise LoopUnavailable("accepted main identity is unavailable or invalid")
            if accepted_main != local_main:
                return _decision(
                    "stale_checkout",
                    accepted_main_sha=accepted_main,
                    local_origin_main_sha=local_main,
                    reason="fetch_required",
                )

            active_scopes = self.github.active_issue_scopes()
            if (
                not isinstance(active_scopes, dict)
                or not all(
                    isinstance(issue, int)
                    and issue > 0
                    and isinstance(paths, list)
                    and paths
                    and all(isinstance(path, str) and path for path in paths)
                    for issue, paths in active_scopes.items()
                )
            ):
                raise LoopUnavailable("active Issue scope state is unavailable or invalid")
            active = set(active_scopes)
            if len(active) >= self.max_active:
                return _decision(
                    "capacity_full",
                    accepted_main_sha=accepted_main,
                    active_issue_numbers=sorted(active),
                )

            rejected: list[dict[str, Any]] = []
            eligible: list[dict[str, Any]] = []
            for issue in sorted(
                (_normalized_issue(item) for item in self.github.list_ready_issues()),
                key=lambda item: item["number"],
            ):
                rejection = self._evaluate_issue(issue, owner, accepted_main)
                if rejection is not None:
                    rejected.append({"issue_number": issue["number"], **rejection})
                    continue
                allowed_paths = artifact_contract.parse_issue_scope(issue["body"])
                eligible.append(
                    {
                        "issue_number": issue["number"],
                        "title": issue["title"],
                        "url": issue["url"],
                        "author": issue["author"],
                        "allowed_paths": allowed_paths,
                        "task_body_sha256": hashlib.sha256(
                            issue["body"].encode("utf-8")
                        ).hexdigest(),
                    }
                )

            selected: list[dict[str, Any]] = []
            deferred: list[int] = []
            occupied = [(issue, active_scopes[issue]) for issue in sorted(active)]
            available_slots = self.max_active - len(active)
            for candidate in eligible:
                conflict = next(
                    (
                        issue
                        for issue, paths in occupied
                        if artifact_contract.scopes_overlap(
                            candidate["allowed_paths"], paths
                        )
                    ),
                    None,
                )
                if conflict is not None:
                    rejected.append(
                        {
                            "issue_number": candidate["issue_number"],
                            "reason": "scope_conflict",
                            "conflicts_with": conflict,
                        }
                    )
                    continue
                if len(selected) >= available_slots:
                    deferred.append(candidate["issue_number"])
                    continue
                selected.append(candidate)
                occupied.append((candidate["issue_number"], candidate["allowed_paths"]))

            if not selected:
                return _decision(
                    "no_eligible_task",
                    accepted_main_sha=accepted_main,
                    active_issue_numbers=sorted(active),
                    rejected=rejected,
                    deferred_issue_numbers=deferred,
                )
            return _decision(
                "ready",
                action="run_many" if len(selected) > 1 else "run_once",
                accepted_main_sha=accepted_main,
                active_issue_numbers=sorted(active),
                selected=selected,
                rejected=rejected,
                deferred_issue_numbers=deferred,
            )
        except LoopUnavailable as exc:
            return _decision("unavailable", reason=str(exc)[:300])

    def _evaluate_issue(
        self, issue: dict[str, Any], owner: str, accepted_main: str
    ) -> dict[str, Any] | None:
        labels = issue["labels"]
        if (
            state_manager.LABEL_READY not in labels
            or labels & (state_manager.ACTIVE_LABELS | state_manager.TERMINAL_LABELS)
        ):
            return {"reason": "invalid_state_labels"}
        if issue["author"].casefold() != owner.casefold():
            return {"reason": "untrusted_author"}
        try:
            task_main = task_main_sha(issue["body"])
        except ValueError:
            return {"reason": "invalid_task_binding"}
        if task_main != accepted_main:
            return {"reason": "accepted_main_mismatch"}
        try:
            artifact_contract.parse_issue_scope(issue["body"])
        except (artifact_contract.ArtifactContractError, TypeError, ValueError):
            return {"reason": "invalid_scope"}
        for dependency in sorted(state_manager.parse_dependencies(issue["body"])):
            labels = self.github.labels_for_issue(dependency)
            if state_manager.LABEL_COMPLETE not in labels:
                return {"reason": "dependency_incomplete", "dependency": dependency}
        if self.github.has_open_issue_pr(issue["number"]):
            return {"reason": "open_pr_exists"}
        return None


class GitHubAdapter:
    """Read-only gh adapter.  It never executes Issue text or writes GitHub."""

    def __init__(self, repository: str, *, timeout_seconds: int = 30) -> None:
        if not REPOSITORY.fullmatch(repository):
            raise ValueError("repository must be owner/name")
        self.repository = repository
        self.timeout_seconds = timeout_seconds

    def _gh_json(self, *args: str) -> Any:
        try:
            result = subprocess.run(
                ["gh", *args],
                capture_output=True,
                text=True,
                timeout=self.timeout_seconds,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise LoopUnavailable("GitHub CLI is unavailable") from exc
        if result.returncode != 0:
            raise LoopUnavailable("GitHub query failed")
        try:
            return json.loads(result.stdout)
        except json.JSONDecodeError as exc:
            raise LoopUnavailable("GitHub returned invalid JSON") from exc

    def read_control_state(self) -> dict[str, Any]:
        try:
            return control_state.read_control_state(self.repository)
        except control_state.ControlStateError as exc:
            raise LoopUnavailable("control state is unavailable") from exc

    def repository_metadata(self) -> dict[str, Any]:
        value = self._gh_json(
            "repo",
            "view",
            self.repository,
            "--json",
            "nameWithOwner,isPrivate,owner,defaultBranchRef",
        )
        try:
            return {
                "name_with_owner": value["nameWithOwner"],
                "owner": value["owner"]["login"],
                "is_private": value["isPrivate"],
                "default_branch": value["defaultBranchRef"]["name"],
            }
        except (KeyError, TypeError) as exc:
            raise LoopUnavailable("repository metadata is malformed") from exc

    def current_user(self) -> str:
        value = self._gh_json("api", "user")
        login = value.get("login") if isinstance(value, dict) else None
        if not isinstance(login, str) or not login:
            raise LoopUnavailable("authenticated GitHub identity is unavailable")
        return login

    def accepted_main_sha(self, branch: str) -> str:
        if not BRANCH.fullmatch(branch):
            raise LoopUnavailable("default branch is invalid")
        value = self._gh_json("api", f"repos/{self.repository}/git/ref/heads/{branch}")
        sha = value.get("object", {}).get("sha") if isinstance(value, dict) else None
        if not isinstance(sha, str):
            raise LoopUnavailable("accepted main SHA is unavailable")
        return sha

    def list_ready_issues(self) -> list[dict[str, Any]]:
        value = self._gh_json(
            "issue",
            "list",
            "--repo",
            self.repository,
            "--state",
            "open",
            "--label",
            state_manager.LABEL_READY,
            "--limit",
            "100",
            "--json",
            "number,title,url,author,body,labels",
        )
        if not isinstance(value, list):
            raise LoopUnavailable("ready Issue list is malformed")
        result = []
        for item in value:
            if not isinstance(item, dict):
                raise LoopUnavailable("ready Issue list is malformed")
            author = item.get("author")
            labels = item.get("labels")
            result.append(
                {
                    **item,
                    "author": author.get("login") if isinstance(author, dict) else None,
                    "labels": [
                        label.get("name") for label in labels
                    ] if isinstance(labels, list) else None,
                }
            )
        return result

    def labels_for_issue(self, issue_number: int) -> set[str]:
        value = self._gh_json(
            "issue", "view", str(issue_number), "--repo", self.repository, "--json", "labels"
        )
        labels = value.get("labels") if isinstance(value, dict) else None
        if not isinstance(labels, list):
            raise LoopUnavailable("dependency label state is unavailable")
        names = {item.get("name") for item in labels if isinstance(item, dict)}
        if None in names:
            raise LoopUnavailable("dependency label state is malformed")
        return names

    def has_open_issue_pr(self, issue_number: int) -> bool:
        value = state_manager.has_open_issue_pr(issue_number, self.repository)
        if value is None:
            raise LoopUnavailable("Issue PR binding is unavailable")
        return value

    def active_issue_scopes(self) -> dict[int, list[str]]:
        active = state_manager.get_active_issue_numbers(self.repository)
        if active is None:
            raise LoopUnavailable("active capacity state is unavailable")
        # Only trusted claim-bound scopes count as active scope.  The mutable
        # active Issue bodies are never re-read here.
        scopes = state_manager.get_active_issue_scopes(active, self.repository)
        if scopes is None:
            raise LoopUnavailable("active Issue scope state is unavailable or invalid")
        return scopes

    def issue_snapshot(self, issue_number: int) -> dict[str, str]:
        value = self._gh_json(
            "issue", "view", str(issue_number), "--repo", self.repository,
            "--json", "title,body",
        )
        if not isinstance(value, dict):
            raise LoopUnavailable("Issue snapshot is unavailable")
        title, body = value.get("title"), value.get("body")
        if not isinstance(title, str) or not isinstance(body, str):
            raise LoopUnavailable("Issue snapshot is malformed")
        return {"title": title, "body": body}

    def dispatch_controller(self, command: str, fields: dict[str, object]) -> None:
        """Submit one allowlisted controller workflow request.

        The workflow's global concurrency group is the serialization point for
        claims and handoffs.  Values are passed as argv items, never through a
        shell, and the method deliberately returns no provider output.
        """

        if command not in {"claim-local", "handoff-local", "release-local"}:
            raise LoopUnavailable("controller command is not allowed")
        allowed = {
            "claim-local": {"issue", "attempt_id", "client_token"},
            "handoff-local": {"issue", "attempt_id", "client_token", "head_sha"},
            "release-local": {"issue", "attempt_id", "client_token", "reason_code"},
        }[command]
        if set(fields) != allowed:
            raise LoopUnavailable("controller fields are invalid")
        args = [
            "gh", "workflow", "run", "agent-controller.yml",
            "--repo", self.repository, "--ref", "main", "-f", f"command={command}",
        ]
        for key in sorted(fields):
            value = fields[key]
            if isinstance(value, bool) or not isinstance(value, (str, int)):
                raise LoopUnavailable("controller field value is invalid")
            text = str(value)
            if not text or "\x00" in text or len(text) > 256:
                raise LoopUnavailable("controller field value is invalid")
            args.extend(["-f", f"{key}={text}"])
        try:
            result = subprocess.run(
                args, capture_output=True, text=True, timeout=self.timeout_seconds
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise LoopUnavailable("controller workflow dispatch is unavailable") from exc
        if result.returncode != 0:
            raise LoopUnavailable("controller workflow dispatch failed")


class GitAdapter:
    def origin_main_sha(self, repo_path: Path, branch: str) -> str:
        if not repo_path.is_dir() or not BRANCH.fullmatch(branch):
            raise LoopUnavailable("local repository path or branch is invalid")
        try:
            result = subprocess.run(
                ["git", "rev-parse", f"refs/remotes/origin/{branch}"],
                cwd=repo_path,
                capture_output=True,
                text=True,
                timeout=30,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise LoopUnavailable("local Git state is unavailable") from exc
        sha = result.stdout.strip()
        if result.returncode != 0 or not HEX40.fullmatch(sha):
            raise LoopUnavailable("local origin main SHA is unavailable")
        return sha


def _canonical_attempt_id(value: object) -> str | None:
    if not isinstance(value, str):
        return None
    try:
        parsed = uuid.UUID(value)
    except ValueError:
        return None
    return value if value == str(parsed) else None


def _bounded_process(
    command: list[str], *, cwd: Path | None = None, timeout_seconds: int = 1800
) -> tuple[int, str, str]:
    """Run one local child in its own process group with bounded cleanup."""

    try:
        process = subprocess.Popen(
            command,
            cwd=cwd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            start_new_session=True,
        )
    except (OSError, ValueError) as exc:
        raise LoopUnavailable("local command could not start") from exc
    try:
        stdout, stderr = process.communicate(timeout=timeout_seconds)
        return process.returncode, stdout[-4000:], stderr[-4000:]
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            stdout, stderr = process.communicate(timeout=min(10, max(1, timeout_seconds // 10)))
        except subprocess.TimeoutExpired:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            try:
                stdout, stderr = process.communicate(timeout=10)
            except subprocess.TimeoutExpired:
                # A descendant may have inherited the pipes after the process
                # group was killed.  Close our read ends and return the
                # bounded timeout result instead of hanging indefinitely.
                if process.stdout is not None:
                    process.stdout.close()
                if process.stderr is not None:
                    process.stderr.close()
                return 124, "", ""
        return 124, stdout[-4000:], stderr[-4000:]


class LocalRunOnce:
    """Execute one claimed local implementation through existing owners.

    This is intentionally a thin orchestration layer.  GitHub comments and
    labels remain the durable claim/worker/CI authority; the local process only
    owns its temporary worktree and bounded child process.
    """

    def __init__(
        self,
        github: GitHubReader | None = None,
        git: GitReader | None = None,
        *,
        repository: str,
        repo_path: Path,
        claim_timeout_seconds: int = 120,
        command_timeout_seconds: int = 1800,
        poll_interval_seconds: float = 1.0,
        sleeper: Callable[[float], None] = time.sleep,
    ) -> None:
        if not REPOSITORY.fullmatch(repository):
            raise ValueError("repository must be owner/name")
        if claim_timeout_seconds < 0 or claim_timeout_seconds > 900:
            raise ValueError("claim_timeout_seconds is outside the bounded range")
        if command_timeout_seconds < 1 or command_timeout_seconds > 3600:
            raise ValueError("command_timeout_seconds is outside the bounded range")
        if poll_interval_seconds < 0 or poll_interval_seconds > 30:
            raise ValueError("poll_interval_seconds is outside the bounded range")
        self.github = github or GitHubAdapter(repository)
        self.git = git or GitAdapter()
        self.repository = repository
        self.repo_path = Path(repo_path).expanduser().resolve()
        self.claim_timeout_seconds = claim_timeout_seconds
        self.command_timeout_seconds = command_timeout_seconds
        self.poll_interval_seconds = poll_interval_seconds
        self.sleeper = sleeper

    def _result(self, status: str, issue: int, attempt: str, **details: Any) -> LocalRunOnceResult:
        return LocalRunOnceResult(status, issue, attempt, details)

    def _client_token(self, issue: int, attempt: str) -> str:
        return hashlib.sha256(
            f"local-run:{self.repository}:{issue}:{attempt}".encode("utf-8")
        ).hexdigest()[:32]

    def _dispatch_id(self, issue: int, attempt: str) -> str:
        return f"local-run:{issue}:{attempt}"

    def _wait_for_claim(self, issue: int, dispatch_id: str) -> dict[str, Any] | None:
        deadline = time.monotonic() + self.claim_timeout_seconds
        while True:
            try:
                state = state_manager.read_dispatch_state(
                    issue, dispatch_id, self.repository
                )
            except state_manager.StateUnavailableError:
                state = None
            if isinstance(state, dict):
                status = state.get("status")
                if status == "dispatched":
                    details = state.get("details")
                    return details if isinstance(details, dict) else None
                if status in {"failed", "rejected", "outcome_unknown"}:
                    return None
            if time.monotonic() >= deadline:
                return None
            self.sleeper(self.poll_interval_seconds)

    def _release(self, issue: int, attempt: str, token: str, reason: str) -> None:
        try:
            self.github.dispatch_controller(
                "release-local",
                {
                    "issue": issue,
                    "attempt_id": attempt,
                    "client_token": token,
                    "reason_code": reason,
                },
            )
        except LoopUnavailable:
            # The original claim remains the durable owner when compensation
            # cannot be submitted; never retry or mutate labels locally.
            return

    def _git_checked(self, worktree: Path, *args: str) -> str:
        try:
            result = subprocess.run(
                ["git", *args], cwd=worktree, capture_output=True, text=True, timeout=120
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise LoopUnavailable("local Git command is unavailable") from exc
        if result.returncode != 0:
            raise LoopUnavailable("local Git command failed")
        return result.stdout.strip()

    def run_once(self, issue_number: int, attempt_id: str) -> LocalRunOnceResult:
        """Run one exact Issue attempt; callers cannot provide derived inputs."""

        attempt = _canonical_attempt_id(attempt_id)
        if type(issue_number) is not int or issue_number <= 0:
            return self._result("rejected", issue_number, str(attempt_id), reason="invalid_issue")
        if attempt is None:
            return self._result("rejected", issue_number, str(attempt_id), reason="invalid_attempt_id")
        token = self._client_token(issue_number, attempt)
        dispatch_id = self._dispatch_id(issue_number, attempt)
        worktree_path: Path | None = None
        branch = f"agent/issue-{issue_number}"
        claimed = False
        pushed = False
        try:
            control = self.github.read_control_state()
            if control.get("emergency_stop") or not control.get("orchestrator_enabled"):
                return self._result("control_stopped", issue_number, attempt)
            metadata = self.github.repository_metadata()
            if str(metadata.get("name_with_owner", "")).casefold() != self.repository.casefold():
                return self._result("identity_rejected", issue_number, attempt, reason="repository_identity_mismatch")
            default_branch = metadata.get("default_branch")
            if not isinstance(default_branch, str) or not BRANCH.fullmatch(default_branch):
                return self._result("unavailable", issue_number, attempt, reason="default_branch_unavailable")
            accepted_main = self.github.accepted_main_sha(default_branch)
            local_main = self.git.origin_main_sha(self.repo_path, default_branch)
            if not HEX40.fullmatch(accepted_main) or accepted_main != local_main:
                return self._result("stale_checkout", issue_number, attempt, accepted_main_sha=accepted_main, local_origin_main_sha=local_main)

            # A retry must not treat an already durable claim as a fresh
            # execution.  Recovery of an existing exact branch/PR is handled
            # by the controller handoff; an in-flight claim remains owned
            # until its lease/terminal state is resolved by GitHub.
            try:
                existing_claim = state_manager.read_dispatch_state(
                    issue_number, dispatch_id, self.repository
                )
            except state_manager.StateUnavailableError:
                return self._result("claim_unavailable", issue_number, attempt)
            if isinstance(existing_claim, dict):
                existing_status = existing_claim.get("status")
                if existing_status in {"claimed", "dispatched"}:
                    return self._result(
                        "in_flight", issue_number, attempt,
                        dispatch_id=dispatch_id,
                    )
                if existing_status in {"failed", "rejected", "outcome_unknown"}:
                    return self._result(
                        "terminal", issue_number, attempt,
                        dispatch_id=dispatch_id,
                        claim_status=existing_status,
                    )
            self.github.dispatch_controller(
                "claim-local",
                {"issue": issue_number, "attempt_id": attempt, "client_token": token},
            )
            details = self._wait_for_claim(issue_number, dispatch_id)
            if details is None:
                return self._result("claim_unavailable", issue_number, attempt)
            claimed = True
            valid, reason = state_manager.local_claim_binding_valid(
                issue_number, details, attempt, token
            )
            if not valid:
                return self._result("claim_rejected", issue_number, attempt, reason=reason)
            claim_main = details["accepted_main_sha"]
            canonical_branch = details["canonical_branch"]
            if claim_main != accepted_main or canonical_branch != branch:
                return self._result("claim_rejected", issue_number, attempt, reason="claim_identity_mismatch")
            if self.github.accepted_main_sha(default_branch) != claim_main:
                return self._result("claim_rejected", issue_number, attempt, reason="accepted_main_moved")
            if self.git.origin_main_sha(self.repo_path, default_branch) != claim_main:
                return self._result("stale_checkout", issue_number, attempt, accepted_main_sha=claim_main)
            live_control = self.github.read_control_state()
            if live_control.get("emergency_stop") or not live_control.get("orchestrator_enabled"):
                return self._result("control_stopped", issue_number, attempt)
            labels = self.github.labels_for_issue(issue_number)
            if state_manager.LABEL_RUNNING not in labels:
                return self._result("claim_rejected", issue_number, attempt, reason="issue_not_running")
            snapshot = self.github.issue_snapshot(issue_number)
            binding = artifact_contract.build_issue_scope_binding(snapshot["body"])
            if binding != {
                "allowed_paths": details["allowed_paths"],
                "task_body_sha256": details["task_body_sha256"],
            }:
                return self._result("claim_rejected", issue_number, attempt, reason="task_body_changed")

            created = worktree_manager.create_worktree(
                issue_number, branch, str(self.repo_path), claim_main
            )
            if not created:
                return self._result("failed", issue_number, attempt, reason="worktree_failed")
            worktree_path = Path(created[0])
            base_sha, expected_remote_sha = created[2], created[3]
            if base_sha != claim_main:
                return self._result("failed", issue_number, attempt, reason="worktree_base_mismatch")
            with tempfile.TemporaryDirectory(prefix=f"agent-run-{issue_number}-") as temp:
                temp_dir = Path(temp)
                prompt_file = temp_dir / "implementation-prompt.txt"
                prompt_file.write_text(
                    prompt_builder.build_claim_bound_implementation_prompt(
                        issue_number,
                        snapshot["title"],
                        snapshot["body"],
                        details["allowed_paths"],
                        claim_main,
                        branch,
                        repo_root=self.repo_path,
                    ),
                    encoding="utf-8",
                )
                output_dir = temp_dir / "codex-output"
                wrapper = Path(__file__).resolve().parent / "codex_wrapper.sh"
                exit_code, _stdout, _stderr = _bounded_process(
                    ["bash", str(wrapper), "implement", str(prompt_file), str(output_dir), str(worktree_path)],
                    timeout_seconds=self.command_timeout_seconds,
                )
                if exit_code != 0:
                    return self._result("failed", issue_number, attempt, reason="codex_failed")
                exit_file = output_dir / "codex-exit-code.txt"
                if not exit_file.is_file() or exit_file.read_text().strip() != "0":
                    return self._result("failed", issue_number, attempt, reason="codex_result_invalid")
                artifact_dir = temp_dir / "artifact"
                manifest = artifact_contract.create_artifact(
                    repo=worktree_path,
                    artifact_dir=artifact_dir,
                    worker_type="implementation",
                    issue_number=issue_number,
                    pr_number=0,
                    base_sha=base_sha,
                    expected_remote_sha=expected_remote_sha,
                    branch=branch,
                    codex_exit_code=0,
                    local_checks=[{"command": "git diff --check", "exit_code": 0}],
                )
                artifact_contract.validate_artifact(
                    artifact_dir=artifact_dir,
                    expected_worker_type="implementation",
                    issue_number=issue_number,
                    pr_number=0,
                    base_sha=base_sha,
                    expected_remote_sha=expected_remote_sha,
                    branch=branch,
                )
                artifact_contract.validate_scope_binding(details, manifest)
                self._git_checked(worktree_path, "reset", "--hard", base_sha)
                self._git_checked(worktree_path, "clean", "-fd")
                self._git_checked(worktree_path, "apply", "--index", "--binary", str(artifact_dir / artifact_contract.PATCH_NAME))
                artifact_contract.validate_index(worktree_path, manifest)
                self._git_checked(worktree_path, "diff", "--check")
                self._git_checked(worktree_path, "commit", "-m", f"feat: implement issue #{issue_number}")
                head_sha = self._git_checked(worktree_path, "rev-parse", "HEAD")
                if not HEX40.fullmatch(head_sha):
                    return self._result("failed", issue_number, attempt, reason="commit_sha_invalid")
                push_args = ["push"]
                if expected_remote_sha:
                    push_args.append(f"--force-with-lease=refs/heads/{branch}:{expected_remote_sha}")
                push_args.extend(["origin", f"HEAD:refs/heads/{branch}"])
                push_code, _push_stdout, _push_stderr = _bounded_process(
                    ["git", *push_args], cwd=worktree_path, timeout_seconds=120
                )
                if push_code != 0:
                    return self._result("outcome_unknown", issue_number, attempt, reason="push_outcome_unknown")
                remote = self._git_checked(self.repo_path, "ls-remote", "origin", f"refs/heads/{branch}")
                if remote.split()[0] != head_sha:
                    return self._result("outcome_unknown", issue_number, attempt, reason="remote_head_unverified")
                pushed = True
                pr_body = (
                    f"<!-- agent-orchestrator-binding: {{\"issue_number\":{issue_number},\"branch\":\"{branch}\"}} -->\n\n"
                    f"Closes #{issue_number}\n\nLocal run attempt `{attempt}`."
                )
                pr = pr_binding.create_or_update_pr(
                    issue_number, branch, head_sha, snapshot["title"], pr_body, self.repository
                )
                pr_number = pr.get("number")
                if type(pr_number) is not int:
                    return self._result("outcome_unknown", issue_number, attempt, reason="pr_number_unavailable")
                pr_binding.verify_post_push_binding(
                    issue_number, pr_number, branch, head_sha, self.repository
                )
                self.github.dispatch_controller(
                    "handoff-local",
                    {"issue": issue_number, "attempt_id": attempt, "client_token": token, "head_sha": head_sha},
                )
                return self._result(
                    "handed_off", issue_number, attempt,
                    pr_number=pr_number, head_sha=head_sha, branch=branch,
                    accepted_main_sha=claim_main, ci_monitor="controller-handoff",
                )
        except (LoopUnavailable, artifact_contract.ArtifactContractError, pr_binding.PRBindingError, OSError, ValueError) as exc:
            if pushed:
                return self._result("outcome_unknown", issue_number, attempt, reason="external_outcome_unknown")
            return self._result("failed", issue_number, attempt, reason=str(exc)[:200])
        finally:
            if worktree_path is not None:
                worktree_manager.remove_worktree(issue_number, str(self.repo_path), branch)
            if claimed and not pushed:
                self._release(issue_number, attempt, token, "local_environment_failure")


class LocalSupervisor:
    """Stateless K=2 launcher built on the same run-once entrypoint."""

    def __init__(
        self,
        controller: LoopController,
        *,
        repository: str,
        repo_path: Path,
        max_active: int = state_manager.MAX_ACTIVE,
        task_timeout_seconds: int = 3600,
        sleeper: Callable[[float], None] = time.sleep,
    ) -> None:
        if max_active < 1 or max_active > state_manager.MAX_ACTIVE:
            raise ValueError(f"max_active must be between 1 and {state_manager.MAX_ACTIVE}")
        if task_timeout_seconds < 1 or task_timeout_seconds > 7200:
            raise ValueError("task_timeout_seconds is outside the bounded range")
        if not REPOSITORY.fullmatch(repository):
            raise ValueError("repository must be owner/name")
        self.controller = controller
        self.repository = repository
        self.repo_path = Path(repo_path).expanduser().resolve()
        self.max_active = max_active
        self.task_timeout_seconds = task_timeout_seconds
        self.sleeper = sleeper

    def _terminate(self, process: subprocess.Popen[str]) -> None:
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            return
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                return

    def run_batch(self) -> dict[str, Any]:
        decision = self.controller.poll()
        if decision.get("status") != "ready":
            return {"kind": "repo-agent-supervisor.v1", "decision": decision, "results": []}
        selected = decision.get("selected")
        if not isinstance(selected, list) or not selected or len(selected) > self.max_active:
            return {
                "kind": "repo-agent-supervisor.v1",
                "status": "unavailable",
                "reason": "poll_capacity_contract_violation",
                "results": [],
            }
        children: list[dict[str, Any]] = []
        script = Path(__file__).resolve().with_name("loopctl.py")
        for candidate in selected:
            issue = candidate.get("issue_number") if isinstance(candidate, dict) else None
            if type(issue) is not int or issue <= 0:
                return {
                    "kind": "repo-agent-supervisor.v1",
                    "status": "unavailable",
                    "reason": "poll_candidate_invalid",
                    "results": [],
                }
            attempt = str(uuid.uuid4())
            try:
                process = subprocess.Popen(
                    [
                        sys.executable,
                        str(script),
                        "run-once",
                        "--repo",
                        self.repository,
                        "--repo-path",
                        str(self.repo_path),
                        "--issue",
                        str(issue),
                        "--attempt-id",
                        attempt,
                    ],
                    cwd=self.repo_path,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                    start_new_session=True,
                )
            except (OSError, ValueError):
                for child in children:
                    self._terminate(child["process"])
                return {
                    "kind": "repo-agent-supervisor.v1",
                    "status": "unavailable",
                    "reason": "run_once_spawn_failed",
                    "results": [],
                }
            children.append({"process": process, "issue_number": issue, "attempt_id": attempt, "started": time.monotonic()})

        results: list[dict[str, Any]] = []
        while children:
            remaining: list[dict[str, Any]] = []
            for child in children:
                process = child["process"]
                elapsed = time.monotonic() - child["started"]
                if process.poll() is None and elapsed > self.task_timeout_seconds:
                    self._terminate(process)
                    results.append({
                        "issue_number": child["issue_number"],
                        "attempt_id": child["attempt_id"],
                        "status": "timeout",
                    })
                    continue
                if process.poll() is None:
                    remaining.append(child)
                    continue
                stdout, _stderr = process.communicate()
                parsed: dict[str, Any] | None = None
                for line in reversed(stdout.splitlines()):
                    try:
                        value = json.loads(line)
                    except json.JSONDecodeError:
                        continue
                    if isinstance(value, dict):
                        parsed = value
                        break
                results.append(parsed or {
                    "kind": "repo-agent-local-run-once.v1",
                    "status": "outcome_unknown",
                    "issue_number": child["issue_number"],
                    "attempt_id": child["attempt_id"],
                    "details": {"reason": "run_once_result_unreadable"},
                })
            children = remaining
            if children:
                self.sleeper(0.05)
        return {
            "kind": "repo-agent-supervisor.v1",
            "status": "completed",
            "selected_issue_numbers": [item["issue_number"] for item in selected],
            "results": results,
        }
