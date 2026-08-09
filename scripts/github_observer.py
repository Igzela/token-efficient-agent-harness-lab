#!/usr/bin/env python3
"""Bounded read-only GitHub observations for repository control-plane tools.

The observer deliberately owns no repository mutation or acceptance decision.
Callers validate the returned identities against their own trusted contracts.
"""

from __future__ import annotations

from dataclasses import dataclass
import json
import os
import re
from typing import Any, Callable
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode, urlsplit
from urllib.request import Request, urlopen


REPOSITORY_RE = re.compile(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+")
SHA_RE = re.compile(r"[0-9a-f]{40}")
DEFAULT_API_URL = "https://api.github.com"
DEFAULT_TIMEOUT_SECONDS = 15
MAX_PAGES = 10


class GitHubObservationError(RuntimeError):
    """Remote evidence is unavailable or malformed."""

    def __init__(self, reason: str, *, status: int | None = None) -> None:
        super().__init__(reason)
        self.reason = reason
        self.status = status


@dataclass(frozen=True)
class JsonResponse:
    payload: Any
    next_url: str | None = None


FetchJson = Callable[[str, dict[str, str], int], JsonResponse]


def token_from_environment() -> str | None:
    """Return an already-configured token without ever requiring one."""
    return os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN") or None


def _next_link(value: str | None) -> str | None:
    if not value:
        return None
    for item in value.split(","):
        match = re.match(r'\s*<([^>]+)>;\s*rel="([^"]+)"\s*$', item)
        if match and match.group(2) == "next":
            return match.group(1)
    return None


def fetch_json(url: str, headers: dict[str, str], timeout: int) -> JsonResponse:
    request = Request(url, headers=headers, method="GET")
    try:
        with urlopen(request, timeout=timeout) as response:
            raw = response.read()
            next_url = _next_link(response.headers.get("Link"))
    except HTTPError as error:
        raise GitHubObservationError(
            f"github_http_{error.code}", status=error.code
        ) from error
    except (URLError, TimeoutError, OSError) as error:
        raise GitHubObservationError("github_transport_unavailable") from error
    try:
        return JsonResponse(json.loads(raw.decode("utf-8")), next_url)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise GitHubObservationError("github_response_invalid_json") from error


class GitHubObserver:
    """Small REST client with bounded pagination and no write methods."""

    def __init__(
        self,
        repository: str,
        *,
        token: str | None = None,
        api_url: str = DEFAULT_API_URL,
        timeout: int = DEFAULT_TIMEOUT_SECONDS,
        fetcher: FetchJson = fetch_json,
    ) -> None:
        if not REPOSITORY_RE.fullmatch(repository):
            raise ValueError("repository must be owner/name")
        api_identity = urlsplit(api_url)
        if api_identity.scheme != "https" or not api_identity.netloc:
            raise ValueError("api_url must be an absolute HTTPS URL")
        self.repository = repository
        self.token = token
        self.api_url = api_url.rstrip("/")
        self.api_origin = (api_identity.scheme, api_identity.netloc)
        self.timeout = timeout
        self.fetcher = fetcher

    def _headers(self) -> dict[str, str]:
        headers = {
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "token-efficient-agent-harness-context",
        }
        if self.token:
            headers["Authorization"] = f"Bearer {self.token}"
        return headers

    def _url(self, path: str, query: dict[str, str | int] | None = None) -> str:
        if path.startswith("https://"):
            identity = urlsplit(path)
            if (identity.scheme, identity.netloc) != self.api_origin:
                raise GitHubObservationError("github_pagination_origin_invalid")
            return path
        url = f"{self.api_url}{path}"
        return f"{url}?{urlencode(query)}" if query else url

    def get(self, path: str, query: dict[str, str | int] | None = None) -> Any:
        return self.fetcher(
            self._url(path, query), self._headers(), self.timeout
        ).payload

    def get_all(
        self, path: str, query: dict[str, str | int] | None = None
    ) -> list[Any]:
        url = self._url(path, query)
        items: list[Any] = []
        for _ in range(MAX_PAGES):
            response = self.fetcher(url, self._headers(), self.timeout)
            if not isinstance(response.payload, list):
                raise GitHubObservationError("github_paginated_response_not_list")
            items.extend(response.payload)
            if not response.next_url:
                return items
            url = self._url(response.next_url)
        raise GitHubObservationError("github_pagination_limit_exceeded")

    def get_all_wrapped(
        self,
        path: str,
        key: str,
        query: dict[str, str | int] | None = None,
    ) -> list[Any]:
        """Read every bounded page from GitHub's object-wrapped list APIs.

        Check runs and Actions endpoints wrap their item arrays in an object,
        so ``get_all`` cannot safely consume them.  Preserve Link pagination
        and reject a declared total that does not match the complete result.
        """
        url = self._url(path, query)
        items: list[Any] = []
        declared_total: int | None = None
        for _ in range(MAX_PAGES):
            response = self.fetcher(url, self._headers(), self.timeout)
            payload = response.payload
            if not isinstance(payload, dict):
                raise GitHubObservationError("github_paginated_response_not_object")
            page = payload.get(key)
            if not isinstance(page, list):
                raise GitHubObservationError(f"github_{key}_not_list")
            total = payload.get("total_count")
            if total is not None:
                if not isinstance(total, int) or total < 0:
                    raise GitHubObservationError("github_total_count_invalid")
                if declared_total is None:
                    declared_total = total
                elif total != declared_total:
                    raise GitHubObservationError("github_total_count_changed")
            items.extend(page)
            if not response.next_url:
                if declared_total is not None and declared_total != len(items):
                    raise GitHubObservationError(
                        "github_paginated_response_incomplete"
                    )
                return items
            url = self._url(response.next_url)
        raise GitHubObservationError("github_pagination_limit_exceeded")

    def list_open_pull_requests(self, *, base: str = "main") -> list[dict[str, Any]]:
        items = self.get_all(
            f"/repos/{self.repository}/pulls",
            {"state": "open", "base": base, "per_page": 100},
        )
        return [item for item in items if isinstance(item, dict)]

    def pull_request(self, number: int) -> dict[str, Any]:
        payload = self.get(f"/repos/{self.repository}/pulls/{number}")
        if not isinstance(payload, dict):
            raise GitHubObservationError("github_pull_request_not_object")
        return payload

    def issue_comments(self, number: int) -> list[dict[str, Any]]:
        return [
            item
            for item in self.get_all(
                f"/repos/{self.repository}/issues/{number}/comments",
                {"per_page": 100},
            )
            if isinstance(item, dict)
        ]

    def pull_request_reviews(self, number: int) -> list[dict[str, Any]]:
        return [
            item
            for item in self.get_all(
                f"/repos/{self.repository}/pulls/{number}/reviews",
                {"per_page": 100},
            )
            if isinstance(item, dict)
        ]

    def pull_request_comments(self, number: int) -> list[dict[str, Any]]:
        """Return inline/diff review comments, separate from issue comments."""
        return [
            item
            for item in self.get_all(
                f"/repos/{self.repository}/pulls/{number}/comments",
                {"per_page": 100},
            )
            if isinstance(item, dict)
        ]

    def check_runs(self, sha: str) -> list[dict[str, Any]]:
        if not SHA_RE.fullmatch(sha):
            raise ValueError("sha must be a full lowercase commit identity")
        checks = self.get_all_wrapped(
            f"/repos/{self.repository}/commits/{sha}/check-runs",
            "check_runs",
            {"per_page": 100},
        )
        return [item for item in checks if isinstance(item, dict)]

    def commit_pull_requests(self, sha: str) -> list[dict[str, Any]]:
        if not SHA_RE.fullmatch(sha):
            raise ValueError("sha must be a full lowercase commit identity")
        return [
            item
            for item in self.get_all(
                f"/repos/{self.repository}/commits/{sha}/pulls",
                {"per_page": 100},
            )
            if isinstance(item, dict)
        ]

    def commit(self, sha: str) -> dict[str, Any]:
        if not SHA_RE.fullmatch(sha):
            raise ValueError("sha must be a full lowercase commit identity")
        payload = self.get(f"/repos/{self.repository}/commits/{sha}")
        if not isinstance(payload, dict):
            raise GitHubObservationError("github_commit_not_object")
        return payload

    def workflow_runs(
        self, *, head_sha: str, event: str | None = None
    ) -> list[dict[str, Any]]:
        if not SHA_RE.fullmatch(head_sha):
            raise ValueError("head_sha must be a full lowercase commit identity")
        query: dict[str, str | int] = {
            "head_sha": head_sha,
            "per_page": 100,
        }
        if event:
            query["event"] = event
        runs = self.get_all_wrapped(
            f"/repos/{self.repository}/actions/runs",
            "workflow_runs",
            query,
        )
        return [item for item in runs if isinstance(item, dict)]

    def workflow_jobs(self, run_id: int) -> list[dict[str, Any]]:
        if not isinstance(run_id, int) or run_id <= 0:
            raise ValueError("run_id must be a positive integer")
        jobs = self.get_all_wrapped(
            f"/repos/{self.repository}/actions/runs/{run_id}/jobs",
            "jobs",
            {"per_page": 100},
        )
        return [item for item in jobs if isinstance(item, dict)]
