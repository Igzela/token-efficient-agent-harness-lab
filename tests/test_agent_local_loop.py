"""Provider-free behavior tests for the outbound local loop controller."""

from __future__ import annotations

import os
from pathlib import Path
import sys
import unittest
from contextlib import redirect_stdout
from io import StringIO
from unittest import mock


sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "scripts", "agent-control"))
import local_loop
import loopctl
import state_manager


MAIN_SHA = "a" * 40
SCOPE = '<!-- agent-orchestrator-scope:v1 {"allowed_paths":["src/"]} -->'
TASK = f'<!-- repo-agent-task:v1 {{"accepted_main_sha":"{MAIN_SHA}"}} -->'


def scope(*paths):
    return '<!-- agent-orchestrator-scope:v1 ' + __import__("json").dumps(
        {"allowed_paths": list(paths)}, separators=(",", ":")
    ) + " -->"


class FakeGitHub:
    def __init__(self):
        self.control = {
            "orchestrator_enabled": True,
            "emergency_stop": False,
            "auto_merge_enabled": False,
        }
        self.metadata = {
            "name_with_owner": "Igzela/example",
            "owner": "Igzela",
            "is_private": False,
            "default_branch": "main",
        }
        self.user = "Igzela"
        self.main_sha = MAIN_SHA
        self.issues = [self.issue(12)]
        self.dependency_labels = {}
        self.open_pr_issues = set()
        self.active = set()
        self.active_scopes = {}

    @staticmethod
    def issue(number, *, author="Igzela", body=f"{SCOPE}\n{TASK}", labels=None):
        return {
            "number": number,
            "title": f"Task {number}",
            "url": f"https://example.invalid/issues/{number}",
            "author": author,
            "body": body,
            "labels": labels or ["agent-ready"],
        }

    def read_control_state(self):
        return self.control

    def repository_metadata(self):
        return self.metadata

    def current_user(self):
        return self.user

    def accepted_main_sha(self, branch):
        self.assert_branch = branch
        return self.main_sha

    def list_ready_issues(self):
        return self.issues

    def labels_for_issue(self, issue_number):
        return self.dependency_labels.get(issue_number, {"agent-complete"})

    def has_open_issue_pr(self, issue_number):
        return issue_number in self.open_pr_issues

    def active_issue_numbers(self):
        return self.active

    def active_issue_scopes(self):
        if self.active_scopes is None:
            return None
        return self.active_scopes or {
            issue_number: [f"active/{issue_number}/"]
            for issue_number in self.active
        }


class FakeGit:
    def __init__(self, main_sha=MAIN_SHA):
        self.main_sha = main_sha

    def origin_main_sha(self, repo_path, branch):
        self.repo_path = Path(repo_path)
        self.branch = branch
        return self.main_sha


class TestLoopControllerPoll(unittest.TestCase):
    def controller(self, github=None, git=None, *, max_active=state_manager.MAX_ACTIVE):
        return local_loop.LoopController(
            github or FakeGitHub(),
            git or FakeGit(),
            repository="Igzela/example",
            repo_path=Path("/workspace/example"),
            max_active=max_active,
        )

    def test_selects_a_deterministic_non_conflicting_batch_up_to_capacity(self):
        github = FakeGitHub()
        github.issues = [
            github.issue(20, body=f"{scope('docs/')}\n{TASK}"),
            github.issue(7, body=f"{scope('src/alpha/')}\n{TASK}"),
            github.issue(12, body=f"{scope('tests/')}\n{TASK}"),
        ]

        decision = self.controller(github).poll()

        self.assertEqual(decision["action"], "run_many")
        self.assertEqual(
            [candidate["issue_number"] for candidate in decision["selected"]],
            [7, 12],
        )
        self.assertEqual(decision["deferred_issue_numbers"], [20])

    def test_rejects_candidate_scope_that_conflicts_with_selected_candidate(self):
        github = FakeGitHub()
        github.issues = [
            github.issue(7, body=f"{scope('scripts/')}\n{TASK}"),
            github.issue(8, body=f"{scope('scripts/agent-control/local_loop.py')}\n{TASK}"),
            github.issue(9, body=f"{scope('docs/')}\n{TASK}"),
        ]

        decision = self.controller(github).poll()

        self.assertEqual(
            [candidate["issue_number"] for candidate in decision["selected"]],
            [7, 9],
        )
        self.assertIn(
            {"issue_number": 8, "reason": "scope_conflict", "conflicts_with": 7},
            decision["rejected"],
        )

    def test_rejects_candidate_scope_that_conflicts_with_active_issue(self):
        github = FakeGitHub()
        github.active = {3}
        github.active_scopes = {3: ["src/"]}
        github.issues = [
            github.issue(7, body=f"{scope('src/worker.py')}\n{TASK}"),
            github.issue(9, body=f"{scope('docs/')}\n{TASK}"),
        ]

        decision = self.controller(github).poll()

        self.assertEqual(
            [candidate["issue_number"] for candidate in decision["selected"]],
            [9],
        )
        self.assertIn(
            {"issue_number": 7, "reason": "scope_conflict", "conflicts_with": 3},
            decision["rejected"],
        )

    def test_active_scope_conflict_does_not_consume_a_slot(self):
        github = FakeGitHub()
        github.active = {3}
        github.active_scopes = {3: ["src/"]}
        github.issues = [
            github.issue(7, body=f"{scope('src/worker.py')}\n{TASK}"),
            github.issue(8, body=f"{scope('docs/')}\n{TASK}"),
            github.issue(9, body=f"{scope('tools/')}\n{TASK}"),
        ]

        decision = self.controller(github).poll()

        self.assertEqual(
            [candidate["issue_number"] for candidate in decision["selected"]],
            [8],
        )
        self.assertIn(
            {"issue_number": 7, "reason": "scope_conflict", "conflicts_with": 3},
            decision["rejected"],
        )
        self.assertEqual(decision["deferred_issue_numbers"], [9])

    def test_active_scope_unavailability_fails_closed(self):
        github = FakeGitHub()
        github.active = {3}

        def fail():
            raise local_loop.LoopUnavailable("active Issue scope is unavailable")

        github.active_issue_scopes = fail

        decision = self.controller(github).poll()

        self.assertEqual(decision["status"], "unavailable")
        self.assertEqual(decision["action"], "none")

    def test_missing_active_claim_scope_fails_closed(self):
        github = FakeGitHub()
        github.active = {3}
        github.active_scopes = None

        decision = self.controller(github).poll()

        self.assertEqual(decision["status"], "unavailable")
        self.assertEqual(decision["action"], "none")

    def test_selects_earliest_eligible_owner_task(self):
        github = FakeGitHub()
        github.issues = [github.issue(20), github.issue(7), github.issue(12)]

        decision = self.controller(github).poll()

        self.assertEqual(decision["kind"], "repo-agent-loop-poll.v1")
        self.assertEqual(decision["status"], "ready")
        self.assertEqual(decision["action"], "run_once")
        self.assertEqual(decision["accepted_main_sha"], MAIN_SHA)
        self.assertEqual(decision["selected"][0]["issue_number"], 7)
        self.assertEqual(decision["selected"][0]["allowed_paths"], ["src/"])

    def test_emergency_stop_blocks_without_inspecting_tasks(self):
        github = FakeGitHub()
        github.control["emergency_stop"] = True

        decision = self.controller(github).poll()

        self.assertEqual(decision["status"], "control_stopped")
        self.assertEqual(decision["action"], "none")
        self.assertEqual(decision["selected"], [])

    def test_disabled_control_blocks(self):
        github = FakeGitHub()
        github.control["orchestrator_enabled"] = False

        decision = self.controller(github).poll()

        self.assertEqual(decision["status"], "control_stopped")

    def test_rejects_checkout_or_issue_bound_to_stale_main(self):
        checkout_stale = self.controller(git=FakeGit("b" * 40)).poll()
        self.assertEqual(checkout_stale["status"], "stale_checkout")

        github = FakeGitHub()
        stale_task = '<!-- repo-agent-task:v1 {"accepted_main_sha":"' + "b" * 40 + '"} -->'
        github.issues = [github.issue(12, body=f"{SCOPE}\n{stale_task}")]
        decision = self.controller(github).poll()
        self.assertEqual(decision["status"], "no_eligible_task")
        self.assertEqual(decision["rejected"][0]["reason"], "accepted_main_mismatch")

    def test_rejects_untrusted_author_invalid_scope_and_existing_pr(self):
        github = FakeGitHub()
        github.issues = [
            github.issue(1, author="attacker"),
            github.issue(2, body=TASK),
            github.issue(3),
        ]
        github.open_pr_issues.add(3)

        decision = self.controller(github).poll()

        self.assertEqual(decision["status"], "no_eligible_task")
        reasons = {item["issue_number"]: item["reason"] for item in decision["rejected"]}
        self.assertEqual(reasons[1], "untrusted_author")
        self.assertEqual(reasons[2], "invalid_scope")
        self.assertEqual(reasons[3], "open_pr_exists")

    def test_rejects_incomplete_dependency(self):
        github = FakeGitHub()
        github.issues = [github.issue(12, body=f"Depends on #5\n{SCOPE}\n{TASK}")]
        github.dependency_labels[5] = {"agent-running"}

        decision = self.controller(github).poll()

        self.assertEqual(decision["status"], "no_eligible_task")
        self.assertEqual(decision["rejected"][0]["reason"], "dependency_incomplete")
        self.assertEqual(decision["rejected"][0]["dependency"], 5)

    def test_capacity_full_blocks_claim_selection(self):
        github = FakeGitHub()
        github.active = {3, 4}

        decision = self.controller(github).poll()

        self.assertEqual(decision["status"], "capacity_full")
        self.assertEqual(decision["active_issue_numbers"], [3, 4])
        self.assertEqual(decision["selected"], [])

    def test_authenticated_user_must_be_repository_owner(self):
        github = FakeGitHub()
        github.user = "someone-else"

        decision = self.controller(github).poll()

        self.assertEqual(decision["status"], "identity_rejected")
        self.assertEqual(decision["action"], "none")

    def test_adapter_failure_is_unavailable_not_empty_success(self):
        github = FakeGitHub()

        def fail():
            raise local_loop.LoopUnavailable("GitHub unavailable")

        github.list_ready_issues = fail
        decision = self.controller(github).poll()
        self.assertEqual(decision["status"], "unavailable")
        self.assertEqual(decision["action"], "none")
        self.assertIn("GitHub unavailable", decision["reason"])

    def test_loop_controller_rejects_max_active_outside_one_to_k(self):
        for value in (0, -1, state_manager.MAX_ACTIVE + 1, 99):
            with self.subTest(value=value):
                with self.assertRaisesRegex(ValueError, "max_active"):
                    self.controller(max_active=value)
        self.assertEqual(self.controller(max_active=1).max_active, 1)
        self.assertEqual(
            self.controller(max_active=state_manager.MAX_ACTIVE).max_active,
            state_manager.MAX_ACTIVE,
        )

    def test_loop_controller_rejects_max_active_expansion_beyond_canonical_k(self):
        github = FakeGitHub()
        github.issues = [
            github.issue(7, body=f"{scope('docs/')}\n{TASK}"),
            github.issue(8, body=f"{scope('src/')}\n{TASK}"),
            github.issue(9, body=f"{scope('tests/')}\n{TASK}"),
        ]
        decision = self.controller(github, max_active=1).poll()
        self.assertEqual(
            [candidate["issue_number"] for candidate in decision["selected"]],
            [7],
        )
        self.assertEqual(decision["deferred_issue_numbers"], [8, 9])


class TestGitHubAdapterActiveScopes(unittest.TestCase):
    def adapter(self):
        return local_loop.GitHubAdapter("Igzela/example")

    def test_active_scopes_come_from_trusted_claims_never_mutable_bodies(self):
        with mock.patch.object(state_manager, "get_active_issue_numbers", return_value={3, 4}), \
             mock.patch.object(
                 state_manager, "get_active_issue_scopes",
                 return_value={3: ["src/"], 4: ["docs/"]},
             ), \
             mock.patch.object(
                 state_manager, "get_issue_body",
                 side_effect=AssertionError("active Issue bodies must not be re-read"),
             ):
            scopes = self.adapter().active_issue_scopes()
        self.assertEqual(scopes, {3: ["src/"], 4: ["docs/"]})

    def test_active_scope_uses_claim_bound_scope_after_active_body_change(self):
        with mock.patch.object(state_manager, "get_active_issue_numbers", return_value={3}), \
             mock.patch.object(
                 state_manager, "get_active_issue_scopes",
                 return_value={3: ["src/"]},
             ):
            scopes = self.adapter().active_issue_scopes()
        self.assertEqual(scopes, {3: ["src/"]})

    def test_active_scope_unavailable_fails_closed(self):
        with mock.patch.object(state_manager, "get_active_issue_numbers", return_value={3}), \
             mock.patch.object(state_manager, "get_active_issue_scopes", return_value=None):
            with self.assertRaises(local_loop.LoopUnavailable):
                self.adapter().active_issue_scopes()
        with mock.patch.object(state_manager, "get_active_issue_numbers", return_value=None):
            with self.assertRaises(local_loop.LoopUnavailable):
                self.adapter().active_issue_scopes()


class TestLoopctl(unittest.TestCase):
    def run_cli(self, decision, *extra, factory=None):
        class Controller:
            def poll(self):
                return decision

        def default_factory(*args, **kwargs):
            return Controller()

        output = StringIO()
        with redirect_stdout(output):
            code = loopctl.main(
                [
                    "poll",
                    "--repo",
                    "Igzela/example",
                    "--repo-path",
                    "/workspace/example",
                    *extra,
                ],
                controller_factory=factory or default_factory,
            )
        return code, output.getvalue()

    def test_cli_emits_one_json_document_and_success_for_ready(self):
        decision = {
            "kind": local_loop.POLL_KIND,
            "status": "ready",
            "action": "run_once",
            "selected": [{"issue_number": 7}],
            "rejected": [],
        }
        code, output = self.run_cli(decision)
        self.assertEqual(code, 0)
        self.assertEqual(__import__("json").loads(output), decision)

    def test_cli_fail_closed_and_require_ready_exit_codes(self):
        unavailable = {
            "kind": local_loop.POLL_KIND,
            "status": "unavailable",
            "action": "none",
            "selected": [],
            "rejected": [],
        }
        self.assertEqual(self.run_cli(unavailable)[0], 2)

        idle = {
            "kind": local_loop.POLL_KIND,
            "status": "no_eligible_task",
            "action": "none",
            "selected": None,
            "rejected": [],
        }
        self.assertEqual(self.run_cli(idle)[0], 0)
        self.assertEqual(self.run_cli(idle, "--require-ready")[0], 3)

    def test_cli_rejects_max_active_outside_one_to_k(self):
        for value in ("0", "-1", str(state_manager.MAX_ACTIVE + 1), "99", "abc"):
            with self.subTest(value=value):
                with self.assertRaises(SystemExit) as ctx:
                    self.run_cli(
                        {"kind": local_loop.POLL_KIND, "status": "ready"},
                        "--max-active", value,
                    )
                self.assertEqual(ctx.exception.code, 2)

    def test_cli_defaults_max_active_to_canonical_k_and_accepts_one(self):
        captured = {}

        def factory(*args, **kwargs):
            captured.update(kwargs)
            class Controller:
                def poll(self):
                    return {
                        "kind": local_loop.POLL_KIND,
                        "status": "no_eligible_task",
                        "action": "none",
                        "selected": [],
                        "rejected": [],
                    }
            return Controller()

        code, _ = self.run_cli(
            {"kind": local_loop.POLL_KIND, "status": "ready"},
            factory=factory,
        )
        self.assertEqual(code, 0)
        self.assertEqual(captured["max_active"], state_manager.MAX_ACTIVE)
        code, _ = self.run_cli(
            {"kind": local_loop.POLL_KIND, "status": "ready"},
            "--max-active", "1",
            factory=factory,
        )
        self.assertEqual(code, 0)
        self.assertEqual(captured["max_active"], 1)


if __name__ == "__main__":
    unittest.main()
