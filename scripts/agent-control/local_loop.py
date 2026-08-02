"""Read-only front edge for a GitHub-controlled, locally executed agent loop.

The repository and GitHub remain authoritative.  This module performs one
bounded poll and returns a versioned decision; it owns no daemon, lease, task
state, approval, budget, output, review, merge, or retry authority.
"""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import re
import subprocess
from typing import Any, Protocol

import artifact_contract
import control_state
import state_manager


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


class GitReader(Protocol):
    def origin_main_sha(self, repo_path: Path, branch: str) -> str: ...


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
