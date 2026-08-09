from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "github_observer.py"
SPEC = importlib.util.spec_from_file_location("github_observer", SCRIPT)
assert SPEC and SPEC.loader
github_observer = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = github_observer
SPEC.loader.exec_module(github_observer)


class GitHubObserverTests(unittest.TestCase):
    def test_rejects_invalid_repository_and_sha(self) -> None:
        with self.assertRaises(ValueError):
            github_observer.GitHubObserver("not-a-repository")

        observer = github_observer.GitHubObserver(
            "owner/repository",
            fetcher=lambda *_args: github_observer.JsonResponse({}),
        )
        with self.assertRaises(ValueError):
            observer.check_runs("abc")

    def test_token_is_confined_to_headers(self) -> None:
        observed: list[tuple[str, dict[str, str], int]] = []

        def fetcher(url: str, headers: dict[str, str], timeout: int):
            observed.append((url, headers, timeout))
            return github_observer.JsonResponse([])

        observer = github_observer.GitHubObserver(
            "owner/repository", token="your-token", fetcher=fetcher
        )
        self.assertEqual(observer.list_open_pull_requests(), [])
        self.assertNotIn("your-token", observed[0][0])
        self.assertEqual(observed[0][1]["Authorization"], "Bearer your-token")

    def test_bounded_pagination_follows_only_next_links(self) -> None:
        calls: list[str] = []

        def fetcher(url: str, _headers: dict[str, str], _timeout: int):
            calls.append(url)
            if len(calls) == 1:
                return github_observer.JsonResponse(
                    [{"number": 1}], "https://api.github.com/next"
                )
            return github_observer.JsonResponse([{"number": 2}])

        observer = github_observer.GitHubObserver(
            "owner/repository", fetcher=fetcher
        )
        self.assertEqual(
            [item["number"] for item in observer.list_open_pull_requests()],
            [1, 2],
        )
        self.assertEqual(calls[1], "https://api.github.com/next")

    def test_pagination_cannot_forward_token_to_another_origin(self) -> None:
        calls: list[str] = []

        def fetcher(url: str, _headers: dict[str, str], _timeout: int):
            calls.append(url)
            return github_observer.JsonResponse([], "https://attacker.invalid/next")

        observer = github_observer.GitHubObserver(
            "owner/repository", token="your-token", fetcher=fetcher
        )
        with self.assertRaisesRegex(
            github_observer.GitHubObservationError,
            "github_pagination_origin_invalid",
        ):
            observer.list_open_pull_requests()
        self.assertEqual(len(calls), 1)

    def test_rejects_unbounded_pagination(self) -> None:
        observer = github_observer.GitHubObserver(
            "owner/repository",
            fetcher=lambda *_args: github_observer.JsonResponse(
                [], "https://api.github.com/next"
            ),
        )
        with self.assertRaisesRegex(
            github_observer.GitHubObservationError,
            "github_pagination_limit_exceeded",
        ):
            observer.list_open_pull_requests()

    def test_check_runs_requires_expected_shape(self) -> None:
        observer = github_observer.GitHubObserver(
            "owner/repository",
            fetcher=lambda *_args: github_observer.JsonResponse({"check_runs": []}),
        )
        self.assertEqual(observer.check_runs("a" * 40), [])

    def test_wrapped_endpoints_follow_every_bounded_page(self) -> None:
        calls: list[str] = []

        def fetcher(url: str, _headers: dict[str, str], _timeout: int):
            calls.append(url)
            if len(calls) == 1:
                return github_observer.JsonResponse(
                    {"total_count": 101, "check_runs": [{"id": i} for i in range(100)]},
                    "https://api.github.com/check-runs?page=2",
                )
            return github_observer.JsonResponse(
                {"total_count": 101, "check_runs": [{"id": 100}]}
            )

        observer = github_observer.GitHubObserver(
            "owner/repository", fetcher=fetcher
        )
        self.assertEqual(len(observer.check_runs("a" * 40)), 101)
        self.assertEqual(calls[-1], "https://api.github.com/check-runs?page=2")

    def test_wrapped_endpoint_rejects_incomplete_declared_total(self) -> None:
        observer = github_observer.GitHubObserver(
            "owner/repository",
            fetcher=lambda *_args: github_observer.JsonResponse(
                {"total_count": 101, "workflow_runs": [{"id": 1}]}
            ),
        )
        with self.assertRaisesRegex(
            github_observer.GitHubObservationError,
            "github_paginated_response_incomplete",
        ):
            observer.workflow_runs(head_sha="a" * 40, event="pull_request")

    def test_wrapped_endpoint_rejects_pagination_beyond_bound(self) -> None:
        observer = github_observer.GitHubObserver(
            "owner/repository",
            fetcher=lambda *_args: github_observer.JsonResponse(
                {"check_runs": []}, "https://api.github.com/next"
            ),
        )
        with self.assertRaisesRegex(
            github_observer.GitHubObservationError,
            "github_pagination_limit_exceeded",
        ):
            observer.check_runs("a" * 40)

    def test_workflow_observation_does_not_hide_nonterminal_runs(self) -> None:
        observed: list[str] = []

        def fetcher(url: str, _headers: dict[str, str], _timeout: int):
            observed.append(url)
            return github_observer.JsonResponse(
                {"total_count": 1, "workflow_runs": [{"id": 1, "status": "queued"}]}
            )

        observer = github_observer.GitHubObserver(
            "owner/repository", fetcher=fetcher
        )
        self.assertEqual(
            observer.workflow_runs(head_sha="a" * 40, event="pull_request")[0]["status"],
            "queued",
        )
        self.assertNotIn("status=completed", observed[0])

    def test_commit_requires_expected_shape(self) -> None:
        observer = github_observer.GitHubObserver(
            "owner/repository",
            fetcher=lambda *_args: github_observer.JsonResponse(
                {"sha": "a" * 40, "commit": {"tree": {"sha": "b" * 40}}}
            ),
        )
        self.assertEqual(observer.commit("a" * 40)["sha"], "a" * 40)

    def test_review_comments_use_the_pull_request_comment_surface(self) -> None:
        observed: list[str] = []

        def fetcher(url: str, _headers: dict[str, str], _timeout: int):
            observed.append(url)
            return github_observer.JsonResponse([])

        observer = github_observer.GitHubObserver(
            "owner/repository", fetcher=fetcher
        )
        self.assertEqual(observer.pull_request_comments(41), [])
        self.assertIn("/pulls/41/comments", observed[0])

    def test_observer_source_exposes_no_http_write_method(self) -> None:
        source = SCRIPT.read_text(encoding="utf-8")
        self.assertIn('method="GET"', source)
        for method in ("POST", "PUT", "PATCH", "DELETE"):
            self.assertNotIn(f'method="{method}"', source)


if __name__ == "__main__":
    unittest.main()
