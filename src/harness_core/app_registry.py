"""Repository registry for the local Harness app control plane.

The registry is application state, not target-project state. It may be written
by the local app server, but registered target repositories remain read-only.
Remote repositories are metadata-only in MVP2.
"""

from __future__ import annotations

from dataclasses import dataclass
import json
import os
from pathlib import Path
import re
from typing import Any


APP_REGISTRY_SCHEMA_VERSION = "app_registry.v1"
_VALID_REPO_ID = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.-]{0,63}$")
_VALID_KINDS = {"local", "remote"}


class AppRegistryError(ValueError):
    """Raised when registry data is invalid."""


@dataclass(frozen=True)
class RepoRef:
    """A registered repository reference."""

    id: str
    name: str
    kind: str
    path: str | None = None
    url: str | None = None
    branch: str | None = None
    description: str | None = None

    def to_dict(self) -> dict[str, Any]:
        data: dict[str, Any] = {
            "id": self.id,
            "name": self.name,
            "kind": self.kind,
        }
        if self.path is not None:
            data["path"] = self.path
        if self.url is not None:
            data["url"] = self.url
        if self.branch is not None:
            data["branch"] = self.branch
        if self.description is not None:
            data["description"] = self.description
        return data

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "RepoRef":
        return cls(
            id=_string_field(data, "id"),
            name=_string_field(data, "name"),
            kind=_string_field(data, "kind"),
            path=_optional_string_field(data, "path"),
            url=_optional_string_field(data, "url"),
            branch=_optional_string_field(data, "branch"),
            description=_optional_string_field(data, "description"),
        )


@dataclass(frozen=True)
class AppRegistry:
    """Immutable registry of app-visible repositories."""

    repos: tuple[RepoRef, ...] = ()
    schema_version: str = APP_REGISTRY_SCHEMA_VERSION

    @classmethod
    def empty(cls) -> "AppRegistry":
        return cls()

    @classmethod
    def load(cls, path: str | Path) -> "AppRegistry":
        registry_path = Path(path)
        if not registry_path.exists():
            return cls.empty()

        try:
            data = json.loads(registry_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            raise AppRegistryError("registry file is unreadable or invalid JSON") from exc

        schema_version = data.get("schema_version")
        if schema_version != APP_REGISTRY_SCHEMA_VERSION:
            raise AppRegistryError("unsupported registry schema version")

        raw_repos = data.get("repos")
        if not isinstance(raw_repos, list):
            raise AppRegistryError("registry repos must be a list")

        repos = tuple(validate_repo_ref(RepoRef.from_dict(item)) for item in raw_repos)
        _reject_duplicate_ids(repos)
        return cls(repos=repos)

    def save(self, path: str | Path) -> None:
        registry_path = Path(path)
        registry_path.parent.mkdir(parents=True, exist_ok=True)
        data = {
            "schema_version": self.schema_version,
            "repos": [repo.to_dict() for repo in self.repos],
        }
        registry_path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    def list_repos(self) -> list[RepoRef]:
        return list(self.repos)

    def get_repo(self, repo_id: str) -> RepoRef | None:
        for repo in self.repos:
            if repo.id == repo_id:
                return repo
        return None

    def add_repo(self, repo: RepoRef) -> "AppRegistry":
        normalized = validate_repo_ref(repo)
        if self.get_repo(normalized.id) is not None:
            raise AppRegistryError(f"duplicate repo id: {normalized.id}")
        return AppRegistry(repos=(*self.repos, normalized), schema_version=self.schema_version)


def validate_repo_ref(repo: RepoRef) -> RepoRef:
    """Validate and normalize one repo reference."""

    if not _VALID_REPO_ID.fullmatch(repo.id):
        raise AppRegistryError("repo id must be 1-64 chars: letters, numbers, dot, underscore, hyphen")
    if not repo.name.strip():
        raise AppRegistryError("repo name is required")
    if repo.kind not in _VALID_KINDS:
        raise AppRegistryError("repo kind must be local or remote")

    if repo.kind == "local":
        if not repo.path:
            raise AppRegistryError("local repo requires path")
        if repo.url:
            raise AppRegistryError("local repo must not include url")
        resolved = Path(repo.path).expanduser().resolve()
        validate_local_repo_path(resolved)
        return RepoRef(
            id=repo.id,
            name=repo.name.strip(),
            kind="local",
            path=str(resolved),
            branch=_clean_optional(repo.branch),
            description=_clean_optional(repo.description),
        )

    if not repo.url:
        raise AppRegistryError("remote repo requires url")
    if repo.path:
        raise AppRegistryError("remote repo must not include path")
    return RepoRef(
        id=repo.id,
        name=repo.name.strip(),
        kind="remote",
        url=repo.url.strip(),
        branch=_clean_optional(repo.branch),
        description=_clean_optional(repo.description),
    )


def validate_local_repo_path(path: Path) -> None:
    """Check that a local target path is readable without writing to it."""

    if not path.exists() or not path.is_dir():
        raise AppRegistryError("local repo path must exist and be a directory")
    if not os.access(path, os.R_OK):
        raise AppRegistryError("local repo path is not readable")


def registry_to_dict(registry: AppRegistry) -> dict[str, Any]:
    return {
        "schema_version": registry.schema_version,
        "repos": [repo.to_dict() for repo in registry.list_repos()],
    }


def _reject_duplicate_ids(repos: tuple[RepoRef, ...]) -> None:
    seen: set[str] = set()
    for repo in repos:
        if repo.id in seen:
            raise AppRegistryError(f"duplicate repo id: {repo.id}")
        seen.add(repo.id)


def _string_field(data: dict[str, Any], key: str) -> str:
    value = data.get(key)
    if not isinstance(value, str):
        raise AppRegistryError(f"{key} is required")
    return value


def _optional_string_field(data: dict[str, Any], key: str) -> str | None:
    value = data.get(key)
    if value is None:
        return None
    if not isinstance(value, str):
        raise AppRegistryError(f"{key} must be a string")
    return value


def _clean_optional(value: str | None) -> str | None:
    if value is None:
        return None
    stripped = value.strip()
    return stripped or None
