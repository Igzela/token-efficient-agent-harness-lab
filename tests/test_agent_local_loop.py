"""Provider-free behavior tests for the outbound local loop controller."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
import uuid
from contextlib import redirect_stdout
from io import StringIO
from unittest import mock


sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "scripts", "agent-control"))
import local_loop
import local_run_once
import local_supervisor
import loopctl
import state_manager


MAIN_SHA = "a" * 40
PLAN_ID = "TOOL-LOCAL-PLAN-1"
ATTEMPT = "123e4567-e89b-12d3-a456-426614174000"
NONCE = "d" * 32
SCOPE = '<!-- agent-orchestrator-scope:v1 {"allowed_paths":["src/"]} -->'
TASK = f'<!-- repo-agent-task:v1 {{"accepted_main_sha":"{MAIN_SHA}"}} -->'


def plan_lane_fixture():
    import plan_lane

    return plan_lane.PlanCandidate(
        packet_id=PLAN_ID,
        source_main_sha=MAIN_SHA,
        task_spec_sha256="c" * 64,
        goal="Implement one bounded plan lane.",
        allowed_paths=["scripts/agent-control/", "tests/"],
        prerequisites=[],
        forbidden_changes=["default branch", "provider calls"],
        verification=["focused provider-free tests"],
        rollback=["disable the adapter and revert the packet"],
    )


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
    def test_git_adapter_refreshes_only_the_named_origin_branch(self):
        result = mock.Mock(returncode=0)
        with mock.patch.object(local_loop.subprocess, "run", return_value=result) as run:
            local_loop.GitAdapter().refresh_origin_main(Path("/tmp"), "main")
        self.assertEqual(
            run.call_args.args[0],
            [
                "git", "fetch", "--no-tags", "origin",
                "+refs/heads/main:refs/remotes/origin/main",
            ],
        )
        self.assertEqual(run.call_args.kwargs["cwd"], Path("/tmp"))

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

    def test_run_once_cli_does_not_accept_derived_inputs(self):
        for forbidden in ("--accepted-sha", "--branch", "--scope", "--prompt", "--shell", "--artifact"):
            with self.subTest(forbidden=forbidden):
                with self.assertRaises(SystemExit):
                    loopctl.main([
                        "run-once", "--repo", "Igzela/example", "--repo-path", "/tmp",
                        "--issue", "7", "--attempt-id", "123e4567-e89b-12d3-a456-426614174000",
                        forbidden, "value",
                    ])
    def test_route_run_cli_has_no_packet_selector_and_returns_typed_terminal_state(self):
        captured = {}

        class RouteRunner:
            def run(self):
                return {
                    "kind": "repo-agent-route-run.v1",
                    "state": "ROUTE_EXHAUSTED",
                    "reason": "no_routed_packet_remains",
                }

        def factory(*args, **kwargs):
            captured.update(kwargs)
            return RouteRunner()

        output = StringIO()
        with redirect_stdout(output):
            code = loopctl.main(
                [
                    "route-run",
                    "--repo", "Igzela/example",
                    "--repo-path", "/workspace/example",
                    "--max-transitions", "7",
                ],
                route_run_factory=factory,
            )
        self.assertEqual(code, 0)
        self.assertEqual(captured["max_transitions"], 7)
        self.assertNotIn("packet_id", captured)
        self.assertEqual(__import__("json").loads(output.getvalue())["state"], "ROUTE_EXHAUSTED")

    def test_route_run_transition_limit_is_bounded_before_runner_construction(self):
        for value in ("0", "257", "not-a-number"):
            with self.subTest(value=value), self.assertRaises(SystemExit):
                loopctl.main([
                    "route-run", "--repo", "Igzela/example", "--repo-path", "/workspace/example",
                    "--max-transitions", value,
                ])


class TestPlanDocumentDecode(unittest.TestCase):
    def test_github_base64_newlines_are_accepted(self):
        import base64
        body = "# Next Decision\n\nhello"
        encoded = base64.b64encode(body.encode()).decode()
        # GitHub inserts newlines every 60 chars
        wrapped = "\n".join(encoded[i:i+60] for i in range(0, len(encoded), 60))
        adapter = local_loop.GitHubAdapter.__new__(local_loop.GitHubAdapter)
        adapter.repository = "Igzela/example"
        adapter.timeout_seconds = 30
        adapter._gh_json = lambda *a, **k: {"encoding": "base64", "content": wrapped}
        self.assertEqual(adapter.accepted_plan_document("a" * 40), body)


class TestLocalRunOnce(unittest.TestCase):
    def test_stateful_plan_runner_keeps_one_identity_through_closeout_and_promotion(self):
        import plan_lane

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            subprocess.run(
                ["git", "clone", "-q", "--shared", str(Path(__file__).resolve().parents[1]), str(repo)],
                check=True,
            )
            subprocess.run(["git", "config", "user.name", "Test"], cwd=repo, check=True)
            subprocess.run(["git", "config", "user.email", "test@example.invalid"], cwd=repo, check=True)
            source_main = subprocess.run(
                ["git", "rev-parse", "HEAD"], cwd=repo, check=True,
                capture_output=True, text=True,
            ).stdout.strip()
            packet_id = "PE7-STATEFUL-SOAK-1"
            candidate = plan_lane.PlanCandidate(
                packet_id=packet_id,
                source_main_sha=source_main,
                task_spec_sha256="c" * 64,
                goal="Modify one repository script and synchronize one canonical document.",
                allowed_paths=["scripts/agent-control/codex_wrapper.sh", "docs/CURRENT_STATUS.md"],
                prerequisites=[],
                forbidden_changes=["provider calls", "external effects"],
                verification=["git diff --check", "python tools/check_security_baseline.py", "python scripts/check_agent_handoff.py"],
                rollback=["revert the packet commit"],
            )
            subprocess.run(["git", "switch", "-qc", candidate.branch], cwd=repo, check=True)
            transport = {
                "dispatches": [],
                "head_sha": None,
                "pr_number": 4901,
                "merge_sha": "e" * 40,
            }

            class StatefulGitHub:
                def read_control_state(self):
                    return {"emergency_stop": False, "orchestrator_enabled": True}

                def repository_metadata(self):
                    return {"name_with_owner": "Igzela/example", "default_branch": "main"}

                def accepted_main_sha(self, _branch):
                    return source_main

                def dispatch_controller(self, command, inputs):
                    transport["dispatches"].append((command, dict(inputs)))

            class StatefulGit:
                def origin_main_sha(self, _repo_path, _branch):
                    return source_main

            runner = local_run_once.LocalRunOnce(
                StatefulGitHub(), StatefulGit(), repository="Igzela/example",
                repo_path=repo, sleeper=lambda _: None, lifecycle_timeout_seconds=0,
            )
            real_bounded_process = local_run_once._bounded_process
            claim = {
                "claim_nonce": NONCE,
                "allowed_paths": candidate.allowed_paths,
            }
            artifact_dir = root / "artifact"

            def bounded(command, **kwargs):
                if command[:2] == ["bash", str(Path(local_run_once.__file__).resolve().parent / "codex_wrapper.sh")]:
                    output_dir = Path(command[4])
                    worktree = Path(command[5])
                    output_dir.mkdir(parents=True, exist_ok=True)
                    script = worktree / "scripts" / "agent-control" / "codex_wrapper.sh"
                    status = worktree / "docs" / "CURRENT_STATUS.md"
                    script.write_text(script.read_text(encoding="utf-8") + "\n# stateful soak candidate\n", encoding="utf-8")
                    status.write_text(status.read_text(encoding="utf-8") + "\nStateful soak candidate.\n", encoding="utf-8")
                    (output_dir / "codex-exit-code.txt").write_text("0", encoding="utf-8")
                    return 0, "", ""
                if command[:2] == ["git", "push"]:
                    return 0, "", ""
                return real_bounded_process(command, **kwargs)

            def git_checked(worktree, *args):
                if args[:2] == ("ls-remote", "origin"):
                    head = subprocess.run(
                        ["git", "rev-parse", "HEAD"], cwd=repo, check=True,
                        capture_output=True, text=True,
                    ).stdout.strip()
                    transport["head_sha"] = head
                    return f"{head}\trefs/heads/{candidate.branch}"
                result = subprocess.run(
                    ["git", *args], cwd=worktree, check=True,
                    capture_output=True, text=True,
                )
                return result.stdout.strip()

            lifecycle = {
                "stages": {"ci": True, "review": True, "merge": True, "closeout": True},
                "transitions": {
                    "ci": {"head_sha": None, "status": "success"},
                    "review": {"head_sha": None, "verdict": "PASS"},
                    "merge": {"head_sha": None, "merge_commit_sha": transport["merge_sha"]},
                    "closeout": {
                        "head_sha": None,
                        "terminal_packet_state": "COMPLETE",
                        "closeout_reference": "stateful-soak",
                    },
                },
            }
            promotion = {
                "kind": "plan-promote",
                "status": "promoted",
                "details": {"packet_id": packet_id, "attempt_id": ATTEMPT},
            }

            def create_pr(*args, **_kwargs):
                self.assertEqual(args[0], packet_id)
                self.assertEqual(args[2], transport["head_sha"])
                return {"number": transport["pr_number"]}

            with mock.patch.object(runner, "_live_plan", return_value=(candidate, 383)), \
                 mock.patch.object(runner, "_wait_for_plan_claim", return_value=claim), \
                 mock.patch.object(runner, "_owned_artifact_dir", return_value=artifact_dir), \
                 mock.patch.object(runner, "_git_checked", side_effect=git_checked), \
                 mock.patch.object(state_manager, "plan_claim_binding_valid", return_value=(True, "ok")), \
                 mock.patch.object(local_run_once.worktree_manager, "create_plan_worktree", return_value=(str(repo), candidate.branch, source_main, None)), \
                 mock.patch.object(local_run_once.worktree_manager, "remove_plan_worktree", return_value=True), \
                 mock.patch.object(local_run_once, "_bounded_process", side_effect=bounded), \
                 mock.patch.object(local_run_once.pr_binding, "create_or_update_plan_pr", side_effect=create_pr), \
                 mock.patch.object(local_run_once.pr_binding, "verify_post_push_plan_binding"), \
                 mock.patch.object(runner, "_wait_for_plan_handoff", return_value=(True, "handoff_proven")) as handoff, \
                 mock.patch.object(local_run_once.plan_lifecycle, "read_plan_lifecycle", side_effect=lambda *_args: lifecycle), \
                 mock.patch.object(runner, "_read_plan_promotion", return_value=promotion):
                result = runner._run_plan_once_authorized(packet_id, ATTEMPT)

            self.assertEqual(result.status, "closed_out")
            self.assertEqual(result.attempt_id, ATTEMPT)
            self.assertEqual(result.details["head_sha"], transport["head_sha"])
            self.assertEqual(result.details["merge_commit_sha"], transport["merge_sha"])
            handoff.assert_called_once_with(
                383, packet_id, ATTEMPT, NONCE, transport["pr_number"], transport["head_sha"]
            )
            self.assertEqual(transport["dispatches"][0], (
                "claim-plan", {"packet_id": packet_id, "attempt_id": ATTEMPT},
            ))
            self.assertEqual(transport["dispatches"][1][0], "handoff-plan")
            self.assertEqual(transport["dispatches"][1][1]["claim_nonce"], NONCE)
            self.assertEqual(transport["dispatches"][1][1]["head_sha"], transport["head_sha"])

    def test_bootstrap_route_binds_receipt_to_deterministic_attempt(self):
        runner = local_run_once.LocalRunOnce(
            mock.Mock(), mock.Mock(), repository="Igzela/example", repo_path=Path("/tmp")
        )
        receipt = "  merge-backed COMPLETE receipt  "
        digest = hashlib.sha256(receipt.strip().encode("utf-8")).hexdigest()
        expected_attempt = str(uuid.uuid5(
            uuid.NAMESPACE_URL,
            f"route-bootstrap:v1:Igzela/example:{PLAN_ID}:{digest}",
        ))
        expected = mock.Mock()

        with mock.patch.object(runner, "run_route_once", return_value=expected) as run:
            result = runner.bootstrap_route_once(PLAN_ID, receipt)

        self.assertIs(result, expected)
        run.assert_called_once_with(
            PLAN_ID, expected_attempt, bootstrap_receipt=receipt.strip()
        )

    def test_run_once_child_uses_isolated_session_not_caller_group(self):
        process = mock.Mock(pid=1234, returncode=0)
        process.communicate.return_value = ("stdout", "stderr")
        with mock.patch.object(local_run_once.subprocess, "Popen", return_value=process) as popen:
            result = local_run_once._bounded_process(["command"])
        self.assertEqual(result, (0, "stdout", "stderr"))
        self.assertTrue(popen.call_args.kwargs.get("start_new_session"))
        env = popen.call_args.kwargs.get("env") or {}
        self.assertNotIn("GH_TOKEN", env)
        self.assertNotIn("GITHUB_TOKEN", env)
        self.assertNotIn("OPENAI_API_KEY", env)

    def test_child_env_strips_credential_shaped_variables(self):
        env = local_run_once.child_env(
            {
                "HOME": "/home/u",
                "PATH": "/usr/bin",
                "GH_TOKEN": "secret",
                "GITHUB_TOKEN": "secret",
                "OPENAI_API_KEY": "sk-test",
                "DEEPSEEK_API_KEY": "secret",
                "LANG": "C",
                "HTTPS_PROXY": "http://127.0.0.1:7897",
                "NO_PROXY": "localhost,127.0.0.1",
            }
        )
        self.assertEqual(env["HOME"], "/home/u")
        self.assertEqual(env["HTTPS_PROXY"], "http://127.0.0.1:7897")
        self.assertEqual(env["NO_PROXY"], "localhost,127.0.0.1")
        self.assertNotIn("GH_TOKEN", env)
        self.assertNotIn("GITHUB_TOKEN", env)
        self.assertNotIn("OPENAI_API_KEY", env)
        self.assertNotIn("DEEPSEEK_API_KEY", env)

    def test_claim_wait_timeout_releases_same_attempt_capacity(self):
        github = mock.Mock()
        github.read_control_state.return_value = {
            "emergency_stop": False,
            "orchestrator_enabled": True,
        }
        github.repository_metadata.return_value = {
            "name_with_owner": "Igzela/example",
            "default_branch": "main",
        }
        github.accepted_main_sha.return_value = MAIN_SHA
        git = mock.Mock()
        git.origin_main_sha.return_value = MAIN_SHA
        token = local_loop.local_client_token("Igzela/example", 7, ATTEMPT)
        claimed_details = {
            "issue_number": 7,
            "attempt_id": ATTEMPT,
            "client_token": token,
            "accepted_main_sha": MAIN_SHA,
            "canonical_branch": "agent/issue-7",
            "allowed_paths": ["scripts/agent-control/"],
            "task_body_sha256": "a" * 64,
            "claim_nonce": NONCE,
            "lease_deadline": "2099-01-01T00:00:00Z",
        }

        def read_state(issue, dispatch_id, repo=""):
            del issue, dispatch_id, repo
            # First call (existing) → none; later reconcile reads claimed.
            if not hasattr(read_state, "n"):
                read_state.n = 0
            read_state.n += 1
            if read_state.n == 1:
                return None
            return {"status": "claimed", "details": claimed_details}

        with mock.patch.object(state_manager, "read_dispatch_state", side_effect=read_state):
            result = local_run_once.LocalRunOnce(
                github,
                git,
                repository="Igzela/example",
                repo_path=Path("/tmp"),
                claim_timeout_seconds=0,
                sleeper=lambda _: None,
            ).run_once(7, ATTEMPT)
        self.assertEqual(result.status, "claim_unavailable")
        self.assertEqual(result.details.get("reason"), "claim_wait_unproven_reconciled")
        release = [
            c for c in github.dispatch_controller.call_args_list
            if c.args and c.args[0] == "release-local"
        ]
        self.assertEqual(len(release), 1)

    def test_bounded_process_timeout_does_not_signal_caller_process_group(self):
        """Inner timeout must kill only the child tree so the receipt owner lives."""

        process = mock.Mock(pid=4242)
        process.communicate.side_effect = [
            local_run_once.subprocess.TimeoutExpired(cmd=["sleep"], timeout=1),
            ("partial", "err"),
        ]
        process.returncode = -15
        killed: list[tuple[int, int]] = []

        def fake_kill(pid, sig):
            killed.append((pid, sig))
            if pid == os.getpid():
                raise AssertionError("timeout must never signal the receipt owner")

        with mock.patch.object(local_run_once.subprocess, "Popen", return_value=process), \
             mock.patch.object(local_run_once.os, "kill", side_effect=fake_kill), \
             mock.patch.object(local_run_once, "_process_descendants", return_value=[]), \
             mock.patch.object(local_run_once, "_pid_exists", side_effect=lambda pid: pid == 4242):
            code, _out, _err = local_run_once._bounded_process(["sleep", "30"], timeout_seconds=1)
        self.assertEqual(code, 124)
        self.assertTrue(killed)
        self.assertNotIn(os.getpid(), [pid for pid, _sig in killed])
        self.assertIn((4242, signal.SIGTERM), killed)

    def test_terminate_task_process_group_kills_stubborn_descendants(self):
        leader = mock.Mock(pid=5000)
        leader.wait.side_effect = [local_run_once.subprocess.TimeoutExpired(cmd="x", timeout=0.2)]
        seen: list[tuple[int, int]] = []

        def fake_kill(pid, sig):
            seen.append((pid, sig))

        with mock.patch.object(
            local_run_once, "_process_descendants", side_effect=[[6001, 6002], [6001, 6002]]
        ), mock.patch.object(
            local_run_once, "_pid_exists", side_effect=lambda pid: pid in {5000, 6001, 6002}
        ), mock.patch.object(local_run_once.os, "kill", side_effect=fake_kill), \
             mock.patch.object(local_run_once.time, "sleep", return_value=None), \
             mock.patch.object(
                 local_run_once.time, "monotonic", side_effect=[0, 0.1, 10, 10.1, 20, 20.1]
             ):
            local_run_once.terminate_task_process_group(
                leader, term_timeout=0.01, kill_timeout=0.01
            )
        self.assertIn((6001, signal.SIGTERM), seen)
        self.assertIn((6002, signal.SIGTERM), seen)
        self.assertIn((5000, signal.SIGTERM), seen)
        self.assertTrue(any(sig == signal.SIGKILL for _pid, sig in seen))

    def test_claimed_not_dispatched_is_resumed_then_released_if_still_stuck(self):
        github = mock.Mock()
        github.read_control_state.return_value = {
            "emergency_stop": False,
            "orchestrator_enabled": True,
        }
        github.repository_metadata.return_value = {
            "name_with_owner": "Igzela/example",
            "default_branch": "main",
        }
        github.accepted_main_sha.return_value = MAIN_SHA
        git = mock.Mock()
        git.origin_main_sha.return_value = MAIN_SHA
        attempt = ATTEMPT
        token = local_loop.local_client_token("Igzela/example", 7, attempt)
        claim_details = {
            "issue_number": 7,
            "attempt_id": attempt,
            "client_token": token,
            "accepted_main_sha": MAIN_SHA,
            "canonical_branch": "agent/issue-7",
            "allowed_paths": ["scripts/agent-control/"],
            "task_body_sha256": "a" * 64,
            "claim_nonce": NONCE,
            "lease_deadline": "2099-01-01T00:00:00Z",
            "target_label": "agent-running",
        }
        claimed_state = {
            "status": "claimed",
            "details": claim_details,
        }
        with mock.patch.object(
            state_manager, "read_dispatch_state", return_value=claimed_state
        ), mock.patch.object(
            state_manager, "local_claim_binding_valid", return_value=(True, "ok")
        ):
            result = local_run_once.LocalRunOnce(
                github,
                git,
                repository="Igzela/example",
                repo_path=Path("/tmp"),
                claim_timeout_seconds=0,
                sleeper=lambda _: None,
            ).run_once(7, attempt)
        self.assertEqual(result.status, "failed")
        self.assertEqual(result.details.get("reason"), "claimed_not_dispatched_reconciled")
        github.dispatch_controller.assert_any_call(
            "claim-local",
            {"issue": 7, "attempt_id": attempt, "client_token": token},
        )
        release_calls = [
            call for call in github.dispatch_controller.call_args_list
            if call.args and call.args[0] == "release-local"
        ]
        self.assertEqual(len(release_calls), 1)

    def test_plan_run_once_rejects_when_terminal_owners_not_ready(self):
        github = mock.Mock()
        github.read_control_state.return_value = {
            "emergency_stop": False,
            "orchestrator_enabled": True,
        }
        github.repository_metadata.return_value = {
            "name_with_owner": "Igzela/example",
            "default_branch": "main",
        }
        github.accepted_main_sha.return_value = MAIN_SHA
        github.plan_ledger_issue.side_effect = local_loop.LoopUnavailable(
            "plan execution ledger is unavailable"
        )
        git = mock.Mock()
        git.origin_main_sha.return_value = MAIN_SHA
        result = local_run_once.LocalRunOnce(
            github,
            git,
            repository="Igzela/example",
            repo_path=Path("/tmp"),
        ).run_plan_once(PLAN_ID, ATTEMPT)
        self.assertEqual(result.status, "rejected")
        reason = result.details.get("reason")
        self.assertTrue(reason.startswith("plan_lane_not_ready:"), reason)
        self.assertIn("plan_execution_ledger", reason)
        self.assertIn("canonical_tests_workflow", reason)
        self.assertIn("ci_monitor_workflow", reason)
        # Readiness rejection must never dispatch claim-plan or write state.
        github.dispatch_controller.assert_not_called()

    def test_plan_run_once_rejects_when_workflows_missing(self):
        github = mock.Mock()
        github.read_control_state.return_value = {
            "emergency_stop": False,
            "orchestrator_enabled": True,
        }
        github.repository_metadata.return_value = {
            "name_with_owner": "Igzela/example",
            "default_branch": "main",
        }
        github.accepted_main_sha.return_value = MAIN_SHA
        github.plan_ledger_issue.return_value = 99
        git = mock.Mock()
        git.origin_main_sha.return_value = MAIN_SHA
        result = local_run_once.LocalRunOnce(
            github,
            git,
            repository="Igzela/example",
            repo_path=Path("/tmp"),
        ).run_plan_once(PLAN_ID, ATTEMPT)
        self.assertEqual(result.status, "rejected")
        reason = result.details.get("reason")
        self.assertTrue(reason.startswith("plan_lane_not_ready:"), reason)
        self.assertIn("canonical_tests_workflow", reason)
        self.assertIn("ci_monitor_workflow", reason)
        github.dispatch_controller.assert_not_called()

    def test_stale_token_on_claimed_state_does_not_release_capacity(self):
        github = mock.Mock()
        github.read_control_state.return_value = {
            "emergency_stop": False,
            "orchestrator_enabled": True,
        }
        github.repository_metadata.return_value = {
            "name_with_owner": "Igzela/example",
            "default_branch": "main",
        }
        github.accepted_main_sha.return_value = MAIN_SHA
        git = mock.Mock()
        git.origin_main_sha.return_value = MAIN_SHA
        token = local_loop.local_client_token("Igzela/example", 7, ATTEMPT)
        foreign = {
            "issue_number": 7,
            "attempt_id": ATTEMPT,
            "client_token": "e" * 32,
            "accepted_main_sha": MAIN_SHA,
            "canonical_branch": "agent/issue-7",
            "allowed_paths": ["scripts/agent-control/"],
            "task_body_sha256": "a" * 64,
            "claim_nonce": NONCE,
            "lease_deadline": "2099-01-01T00:00:00Z",
        }
        with mock.patch.object(
            state_manager,
            "read_dispatch_state",
            return_value={"status": "claimed", "details": foreign},
        ):
            result = local_run_once.LocalRunOnce(
                github,
                git,
                repository="Igzela/example",
                repo_path=Path("/tmp"),
                claim_timeout_seconds=0,
                sleeper=lambda _: None,
            ).run_once(7, ATTEMPT)
        self.assertEqual(result.status, "claim_rejected")
        self.assertEqual(result.details.get("reason"), "claim_token_mismatch")
        github.dispatch_controller.assert_not_called()

    def test_late_fork_descendant_is_included_in_kill_wave(self):
        leader = mock.Mock(pid=7000)
        leader.wait.side_effect = [None]
        killed: list[int] = []

        def fake_kill(pid, sig):
            del sig
            killed.append(pid)

        # First scan misses 7002; TERM wait rescans and finds late fork 7002.
        descendant_scans = [[7001], [7001, 7002]]

        def descendants(root):
            del root
            return descendant_scans.pop(0) if descendant_scans else [7001, 7002]

        exists = {7000, 7001, 7002}

        def pid_exists(pid):
            return pid in exists and pid not in {p for p in killed if p == pid}

        # Simpler: keep all alive until SIGKILL phase.
        with mock.patch.object(local_run_once, "_process_descendants", side_effect=descendants), \
             mock.patch.object(local_run_once, "_pid_exists", return_value=True), \
             mock.patch.object(local_run_once.os, "kill", side_effect=fake_kill), \
             mock.patch.object(local_run_once.time, "sleep", return_value=None), \
             mock.patch.object(
                 local_run_once.time, "monotonic", side_effect=[0, 1, 2, 3, 4, 5]
             ):
            local_run_once.terminate_task_process_group(
                leader, term_timeout=0.01, kill_timeout=0.01
            )
        self.assertIn(7001, killed)
        self.assertIn(7002, killed)
        self.assertIn(7000, killed)

    def test_dispatched_plan_claim_is_repaired_idempotently_not_reexecuted(self):
        github = mock.Mock()
        github.read_control_state.return_value = {
            "emergency_stop": False,
            "orchestrator_enabled": True,
        }
        github.repository_metadata.return_value = {
            "name_with_owner": "Igzela/example",
            "default_branch": "main",
        }
        github.accepted_main_sha.return_value = MAIN_SHA
        github.plan_ledger_issue.return_value = 99
        git = mock.Mock()
        git.origin_main_sha.return_value = MAIN_SHA
        candidate = plan_lane_fixture()
        dispatched = {
            "kind": "agent-orchestrator-dispatch-state",
            "version": 1,
            "issue_number": 99,
            "dispatch_id": f"plan-run:{PLAN_ID}:{MAIN_SHA}:{ATTEMPT}",
            "action": "plan-run",
            "status": "dispatched",
            "details": {
                "ledger_issue_number": 99,
                "subject_kind": "plan-packet",
                "subject_id": PLAN_ID,
                "source_main_sha": MAIN_SHA,
                "task_spec_sha256": candidate.task_spec_sha256,
                "allowed_paths": ["scripts/agent-control/", "tests/"],
                "canonical_branch": candidate.branch,
                "attempt_id": ATTEMPT,
                "execution_token": local_loop.plan_execution_token(
                    "Igzela/example", PLAN_ID, MAIN_SHA, ATTEMPT
                ),
                "claim_nonce": NONCE,
                "target_label": state_manager.LABEL_RUNNING,
                "lease_deadline": "2099-01-01T00:00:00Z",
            },
        }
        remote_head = "b" * 40

        class GitChecked:
            def __call__(self, _worktree, *args):
                self.args = args
                return f"{remote_head}\trefs/heads/{candidate.branch}"

        git_checked = GitChecked()
        runner = local_run_once.LocalRunOnce(
            github,
            git,
            repository="Igzela/example",
            repo_path=Path("/tmp"),
            claim_timeout_seconds=0,
            sleeper=lambda _: None,
        )
        with mock.patch.object(
            state_manager, "read_dispatch_state", return_value=dispatched
        ), mock.patch.object(
            runner, "_git_checked", side_effect=git_checked
        ), mock.patch.object(
            local_run_once.pr_binding,
            "find_plan_pr",
            return_value={"number": 4001},
        ), mock.patch.object(
            runner,
            "_request_plan_handoff",
            new=mock.Mock(return_value=(True, "handoff_proven")),
        ) as request_handoff:
            result = runner._recover_existing_plan_claim(
                PLAN_ID, ATTEMPT, candidate, 99
            )
        self.assertEqual(result.status, "handed_off")
        self.assertEqual(result.details.get("pr_number"), 4001)
        self.assertEqual(result.details.get("head_sha"), remote_head)
        request_handoff.assert_called_once_with(
            99, PLAN_ID, ATTEMPT, NONCE, 4001, remote_head
        )

    def test_plan_scope_violation_has_no_commit_push_pr_or_handoff(self):
        github = mock.Mock()
        github.read_control_state.return_value = {
            "emergency_stop": False,
            "orchestrator_enabled": True,
        }
        github.repository_metadata.return_value = {
            "name_with_owner": "Igzela/example",
            "default_branch": "main",
        }
        github.accepted_main_sha.return_value = MAIN_SHA
        git = mock.Mock()
        git.origin_main_sha.return_value = MAIN_SHA
        candidate = plan_lane_fixture()
        claim = {
            "claim_nonce": NONCE,
            "allowed_paths": candidate.allowed_paths,
        }
        runner = local_run_once.LocalRunOnce(
            github,
            git,
            repository="Igzela/example",
            repo_path=Path("/tmp/repo"),
            sleeper=lambda _: None,
        )

        process_calls = []

        def bounded(command, **_kwargs):
            process_calls.append(command)
            output_dir = Path(command[4])
            output_dir.mkdir(parents=True, exist_ok=True)
            (output_dir / "codex-exit-code.txt").write_text("0")
            return 0, "", ""

        git_checked = mock.Mock()
        create_pr = mock.Mock()
        with mock.patch.object(runner, "_live_plan", return_value=(candidate, 383)), \
             mock.patch.object(runner, "_wait_for_plan_claim", return_value=claim), \
             mock.patch.object(runner, "_owned_artifact_dir", return_value=Path("/tmp/fake-artifact")), \
             mock.patch.object(runner, "_git_checked", git_checked), \
             mock.patch.object(state_manager, "plan_claim_binding_valid", return_value=(True, "ok")), \
             mock.patch.object(local_run_once.worktree_manager, "create_plan_worktree", return_value=("/tmp/fake-plan-worktree", candidate.branch, MAIN_SHA, None)), \
             mock.patch.object(local_run_once.worktree_manager, "remove_plan_worktree", return_value=True), \
             mock.patch.object(local_run_once, "_bounded_process", side_effect=bounded), \
             mock.patch.object(local_run_once.local_verification, "run_plan_focused_checks", return_value=[]), \
             mock.patch.object(local_run_once.artifact_contract, "create_artifact", return_value={"changed_files": ["README.md"]}), \
             mock.patch.object(local_run_once.artifact_contract, "validate_artifact", return_value={"changed_files": ["README.md"]}), \
             mock.patch.object(local_run_once.pr_binding, "create_or_update_plan_pr", create_pr):
            result = runner._run_plan_once_authorized(PLAN_ID, ATTEMPT)

        self.assertEqual(result.status, "failed")
        self.assertEqual(result.details["reason"], "plan_scope_violation")
        self.assertIn("README.md", result.details["diagnostic"])
        self.assertEqual(len(process_calls), 1)
        git_checked.assert_not_called()
        create_pr.assert_not_called()
        actions = [call.args[0] for call in github.dispatch_controller.call_args_list]
        self.assertNotIn("handoff-plan", actions)

    def test_plan_check_mutation_has_no_artifact_commit_push_pr_or_handoff(self):
        github = mock.Mock()
        github.read_control_state.return_value = {
            "emergency_stop": False,
            "orchestrator_enabled": True,
        }
        github.repository_metadata.return_value = {
            "name_with_owner": "Igzela/example",
            "default_branch": "main",
        }
        github.accepted_main_sha.return_value = MAIN_SHA
        git = mock.Mock()
        git.origin_main_sha.return_value = MAIN_SHA
        candidate = plan_lane_fixture()
        claim = {"claim_nonce": NONCE, "allowed_paths": candidate.allowed_paths}
        runner = local_run_once.LocalRunOnce(
            github,
            git,
            repository="Igzela/example",
            repo_path=Path("/tmp/repo"),
            sleeper=lambda _: None,
        )

        def bounded(command, **_kwargs):
            output_dir = Path(command[4])
            output_dir.mkdir(parents=True, exist_ok=True)
            (output_dir / "codex-exit-code.txt").write_text("0")
            return 0, "", ""

        artifact = mock.Mock()
        git_checked = mock.Mock()
        create_pr = mock.Mock()
        with mock.patch.object(runner, "_live_plan", return_value=(candidate, 383)), \
             mock.patch.object(runner, "_wait_for_plan_claim", return_value=claim), \
             mock.patch.object(runner, "_owned_artifact_dir", return_value=Path("/tmp/fake-artifact")), \
             mock.patch.object(runner, "_git_checked", git_checked), \
             mock.patch.object(state_manager, "plan_claim_binding_valid", return_value=(True, "ok")), \
             mock.patch.object(local_run_once.worktree_manager, "create_plan_worktree", return_value=("/tmp/fake-plan-worktree", candidate.branch, MAIN_SHA, None)), \
             mock.patch.object(local_run_once.worktree_manager, "remove_plan_worktree", return_value=True), \
             mock.patch.object(local_run_once, "_bounded_process", side_effect=bounded), \
             mock.patch.object(
                 local_run_once.local_verification,
                 "run_plan_focused_checks",
                 side_effect=local_run_once.local_verification.LocalVerificationError(
                     "focused_check_mutated_candidate"
                 ),
             ), \
             mock.patch.object(local_run_once.artifact_contract, "create_artifact", artifact), \
             mock.patch.object(local_run_once.pr_binding, "create_or_update_plan_pr", create_pr):
            result = runner._run_plan_once_authorized(PLAN_ID, ATTEMPT)

        self.assertEqual(result.status, "failed")
        self.assertEqual(result.details["reason"], "focused_check_mutated_candidate")
        artifact.assert_not_called()
        git_checked.assert_not_called()
        create_pr.assert_not_called()
        actions = [call.args[0] for call in github.dispatch_controller.call_args_list]
        self.assertNotIn("handoff-plan", actions)

    def test_unknown_plan_output_requires_durable_ledger_terminal_readback(self):
        github = mock.Mock()
        github.plan_ledger_issue.return_value = 99
        terminal = {
            "kind": "agent-orchestrator-dispatch-state",
            "version": 1,
            "issue_number": 99,
            "dispatch_id": f"plan-run:{PLAN_ID}:{MAIN_SHA}:{ATTEMPT}",
            "action": "plan-run",
            "status": "failed_unknown_output",
            "details": {
                "subject_kind": "plan-packet",
                "subject_id": PLAN_ID,
                "source_main_sha": MAIN_SHA,
                "claim_nonce": NONCE,
                "reason": "local_unknown_output",
            },
        }
        with mock.patch.object(
            state_manager, "read_dispatch_state", return_value=terminal
        ):
            result = local_run_once.LocalRunOnce(
                github,
                repository="Igzela/example",
                repo_path=Path("/tmp"),
                sleeper=lambda _: None,
            )._unknown_plan_output(PLAN_ID, ATTEMPT, MAIN_SHA, NONCE, "push_outcome_unknown")
        self.assertEqual(result.status, "failed_unknown_output")
        self.assertEqual(result.details["subject_id"], PLAN_ID)
        github.dispatch_controller.assert_called_once_with(
            "block-plan",
            {
                "packet_id": PLAN_ID,
                "attempt_id": ATTEMPT,
                "source_main_sha": MAIN_SHA,
                "claim_nonce": NONCE,
            },
        )


class TestLocalSupervisor(unittest.TestCase):
    def test_two_selected_workers_start_with_real_overlap(self):
        class Poller:
            def poll(self):
                return {"status": "ready", "selected": [{"issue_number": 7}, {"issue_number": 8}]}

        real_popen = local_loop.subprocess.Popen
        starts = []

        def fake_popen(command, **kwargs):
            issue = command[command.index("--issue") + 1]
            attempt = command[command.index("--attempt-id") + 1]
            starts.append((issue, time.monotonic()))
            return real_popen([
                sys.executable, "-c",
                "import json,time,sys; time.sleep(.2); print(json.dumps({" \
                "'kind':'repo-agent-local-run-once.v1'," \
                "'status':'handed_off'," \
                "'issue_number':int(sys.argv[1])," \
                "'attempt_id':sys.argv[2]}))",
                "worker", issue, attempt,
            ], **kwargs)

        with mock.patch.object(local_loop.subprocess, "Popen", side_effect=fake_popen):
            result = local_supervisor.LocalSupervisor(
                Poller(), repository="Igzela/example", repo_path=Path("/tmp"),
                task_timeout_seconds=10, sleeper=lambda _: None,
            ).run_batch()
        self.assertEqual(result["status"], "completed")
        self.assertEqual(len(result["results"]), 2)
        self.assertEqual(len(starts), 2)
        self.assertLess(abs(starts[0][1] - starts[1][1]), .15)

    def test_malformed_later_candidate_does_not_leave_first_child(self):
        class Poller:
            def poll(self):
                return {"status": "ready", "selected": [{"issue_number": 7}, {"bad": True}]}

        with mock.patch.object(local_loop.subprocess, "Popen") as popen:
            result = local_supervisor.LocalSupervisor(
                Poller(), repository="Igzela/example", repo_path=Path("/tmp")
            ).run_batch()
        self.assertEqual(result["status"], "unavailable")
        self.assertEqual(result["reason"], "poll_candidate_invalid")
        popen.assert_not_called()

    def test_child_receipt_rejects_duplicate_json_and_exit_status_mismatch(self):
        supervisor = local_supervisor.LocalSupervisor(
            mock.Mock(), repository="Igzela/example", repo_path=Path("/tmp")
        )
        process = mock.Mock(returncode=0)
        process.communicate.return_value = (
            '{"kind":"repo-agent-local-run-once.v1","status":"handed_off",'
            '"issue_number":7,"attempt_id":"a"}\n'
            '{"kind":"repo-agent-local-run-once.v1","status":"handed_off",'
            '"issue_number":7,"attempt_id":"a"}\n',
            "",
        )
        duplicate = supervisor._child_receipt(process, 7, "a")
        self.assertEqual(duplicate["status"], "outcome_unknown")
        self.assertEqual(duplicate["details"]["reason"], "child_receipt_count_invalid")

        process.communicate.return_value = (
            '{"kind":"repo-agent-local-run-once.v1","status":"handed_off",'
            '"issue_number":7,"attempt_id":"a"}\n',
            "",
        )
        process.returncode = 2
        mismatch = supervisor._child_receipt(process, 7, "a")
        self.assertEqual(mismatch["status"], "outcome_unknown")
        self.assertEqual(mismatch["details"]["reason"], "child_receipt_binding_invalid")

    def test_timeout_reconciles_the_exact_attempt_without_releasing_it(self):
        controller = mock.Mock()
        controller.poll.return_value = {"status": "ready", "selected": [{"issue_number": 7}]}
        process = mock.Mock(pid=1234)
        process.poll.return_value = None
        controller.github.dispatch_controller.return_value = None
        with mock.patch.object(local_supervisor.subprocess, "Popen", return_value=process), \
             mock.patch.object(local_supervisor.os, "killpg"), \
             mock.patch.object(local_supervisor.time, "monotonic", side_effect=[0, 2, 3]), \
             mock.patch.object(
                 local_supervisor.state_manager,
                 "read_dispatch_state",
                  return_value={
                      "status": "failed_unknown_output",
                      "details": {"claim_nonce": "d" * 32},
                  },
             ):
            result = local_supervisor.LocalSupervisor(
                controller,
                repository="Igzela/example",
                repo_path=Path("/tmp"),
                task_timeout_seconds=1,
                sleeper=lambda _: None,
            ).run_batch()
        self.assertEqual(result["results"][0]["status"], "failed_unknown_output")
        controller.github.dispatch_controller.assert_called_once()
        fields = controller.github.dispatch_controller.call_args.args[1]
        self.assertEqual(fields["reason_code"], "local_unknown_output")

    def test_plan_timeout_reconciles_the_exact_ledger_claim(self):
        controller = mock.Mock()
        controller.github.plan_ledger_issue.return_value = 99
        dispatch_id = f"plan-run:{PLAN_ID}:{MAIN_SHA}:{ATTEMPT}"
        claim = {
            "kind": "agent-orchestrator-dispatch-state",
            "version": 1,
            "issue_number": 99,
            "dispatch_id": dispatch_id,
            "action": "plan-run",
            "status": "dispatched",
            "details": {
                "ledger_issue_number": 99,
                "subject_kind": "plan-packet",
                "subject_id": PLAN_ID,
                "attempt_id": ATTEMPT,
                "source_main_sha": MAIN_SHA,
                "claim_nonce": NONCE,
                "allowed_paths": ["src/"],
                "canonical_branch": f"agent/packet-{PLAN_ID.lower()}",
            },
        }
        terminal = {**claim, "status": "failed_unknown_output"}
        comments = [{
            "author": {"login": "github-actions[bot]"},
            "body": json.dumps(claim),
        }, {
            "author": {"login": "github-actions[bot]"},
            "body": json.dumps({**claim, "status": "claimed"}),
        }]
        with mock.patch.object(state_manager, "get_issue_comments", return_value=comments), \
             mock.patch.object(state_manager, "read_dispatch_state", return_value=terminal):
            result = local_supervisor.LocalSupervisor(
                controller,
                repository="Igzela/example",
                repo_path=Path("/tmp"),
                sleeper=lambda _: None,
            )._reconcile_unknown(None, ATTEMPT, PLAN_ID)
        self.assertEqual(result["status"], "failed_unknown_output")
        self.assertEqual(result["subject_id"], PLAN_ID)
        controller.github.dispatch_controller.assert_called_once_with(
            "block-plan",
            {
                "packet_id": PLAN_ID,
                "attempt_id": ATTEMPT,
                "source_main_sha": MAIN_SHA,
                "claim_nonce": NONCE,
            },
        )

    def test_plan_closed_out_claim_is_reported_terminal_not_blocked(self):
        controller = mock.Mock()
        controller.github.plan_ledger_issue.return_value = 99
        dispatch_id = f"plan-run:{PLAN_ID}:{MAIN_SHA}:{ATTEMPT}"
        claim = {
            "kind": "agent-orchestrator-dispatch-state",
            "version": 1,
            "issue_number": 99,
            "dispatch_id": dispatch_id,
            "action": "plan-run",
            "status": "closed_out",
            "details": {
                "ledger_issue_number": 99,
                "subject_kind": "plan-packet",
                "subject_id": PLAN_ID,
                "attempt_id": ATTEMPT,
                "source_main_sha": MAIN_SHA,
                "claim_nonce": NONCE,
                "allowed_paths": ["src/"],
                "canonical_branch": f"agent/packet-{PLAN_ID.lower()}",
                "terminal_packet_state": "closed_out",
                "closeout_reference": "PR #42",
            },
        }
        comments = [{
            "author": {"login": "github-actions[bot]"},
            "body": json.dumps(claim),
        }]
        with mock.patch.object(state_manager, "get_issue_comments", return_value=comments):
            result = local_supervisor.LocalSupervisor(
                controller,
                repository="Igzela/example",
                repo_path=Path("/tmp"),
                sleeper=lambda _: None,
            )._reconcile_unknown(None, ATTEMPT, PLAN_ID)
        self.assertEqual(result["status"], "closed_out")
        self.assertEqual(result["subject_id"], PLAN_ID)
        controller.github.dispatch_controller.assert_not_called()


if __name__ == "__main__":
    unittest.main()
