"""Provider-free Steward execution and bounded concurrency tests."""

from __future__ import annotations

from pathlib import Path
from dataclasses import replace
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts" / "agent-control"))

import mission_contract as contract  # noqa: E402
import steward  # noqa: E402
import steward_github  # noqa: E402
from steward_journal import StewardJournal  # noqa: E402
import steward_workers as workers  # noqa: E402
import worktree_manager  # noqa: E402


BASE = contract.CAMPAIGN_BASE_SHA
HEAD = "b" * 40


class ExplodingWorker(workers.BoundedProcessWorker):
    def run(self, context):
        raise RuntimeError("worker stopped after mutation boundary")


class StewardExecutionTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        registered = contract.campaign_mission()
        self.mission = contract.activate_current_mission(
            repository=registered.repository_identity.repository,
            base_sha=registered.repository_identity.base_sha,
            branch=registered.repository_identity.branch,
            source_ref=registered.repository_identity.source_ref,
            source_sha256=registered.repository_identity.source_sha256,
            proposal_sha256=registered.proposal_sha256,
            owner_approval=contract.OwnerApproval(
                "github:Igzela", registered.proposal_sha256, "fixture-owner-approval", "2026-08-30T00:00:00Z"
            ),
            owner_authenticator=type("Authenticator", (), {"verify": lambda *_args: True})(),
        )
        self.stage, self.card = self.make_stage_card()
        binding = worktree_manager.steward_binding_digest(
            self.mission.mission_id, self.stage.stage_id, self.card.card_id, BASE
        )
        self.mock_worktree_path = self.root / f"steward-{binding[:24]}"
        self.mock_worktree_path.mkdir()
        self.mock_worktree_branch = f"agent/steward-{binding[:24]}"
        self.facts = steward_github.StagePRFacts(
            contract.CAMPAIGN_REPOSITORY,
            42,
            "OPEN",
            False,
            False,
            BASE,
            HEAD,
            "PASS",
            "PASS",
            "main",
            self.mock_worktree_branch,
        )

    def test_outer_loop_capacity_is_canonical_k2(self):
        self.assertEqual(steward.MAX_CONCURRENCY, 2)
        self.assertEqual(workers.MAX_ACTIVE_WORKERS, 2)

    def review(
        self,
        *,
        head=HEAD,
        reviewer="review-session",
        implementation="impl-session",
        review_round=1,
        review_mode="full",
    ):
        receipt_payload = {
            "schema_version": "steward_review_outcome.v1",
            "status": "PASS",
            "reviewer_session_id": reviewer,
            "implementation_session_id": implementation,
            "reviewed_head_sha": head,
            "blockers": [],
            "detail": "",
            "reviewed_base_sha": BASE,
            "reviewed_range_sha256": workers.review_range_digest(BASE, head),
            "review_axes": ["standards", "spec"],
            "review_round": review_round,
            "review_mode": review_mode,
            "summary": "bounded independent review",
            "findings": None,
            "security_ok": True,
            "rollback_ok": True,
            "observed_ci_status": "unknown",
            "finding_ledger_digest": "",
            "review_receipt_sha256": "",
        }
        return workers.ReviewOutcome.from_wire(
            workers.seal_review_outcome_wire(receipt_payload)
        )

    def process_worker(self, callback):
        def command(context):
            outcome = callback(context)
            payload = outcome.to_wire()
            payload["session_id"] = f"steward-process:{context.card_id}:{context.attempt}"
            payload = json.dumps(payload)
            return ["/usr/bin/python3", "-c", f"print({payload!r})"]

        return workers.BoundedProcessWorker(command, timeout_seconds=5)

    def process_reviewer(self, callback):
        def command(context, outcome):
            review = callback(context, outcome)
            payload = review.to_wire()
            payload["reviewer_session_id"] = workers.reviewer_session_id(context, outcome)
            payload["implementation_session_id"] = outcome.session_id
            payload = workers.seal_review_outcome_wire(payload)
            payload = json.dumps(payload)
            return ["/usr/bin/python3", "-c", f"print({payload!r})"]

        return workers.BoundedProcessReviewer(command, timeout_seconds=5)

    def make_stage_card(self, *, second: bool = False, shared_path: bool = False):
        path = "tests/test_mission_contract.py" if second else "docs/ARCHITECTURE_BOOK.md"
        stage_id = "stage-1"
        card_id = "card-2" if second else "card-1"
        card = contract.WorkCard(
            card_id,
            stage_id,
            (path,),
            ("outside-approved/",),
            ("Apply one bounded change.",),
            ("focused checks",),
            ("reject scope expansion",),
            ("bounded receipt",),
            (),
            ("docs/ARCHITECTURE_BOOK.md" if shared_path else path,),
            2,
            "T1",
            self.mission.rollback,
            "PENDING",
        )
        stage = contract.Stage(
            stage_id,
            self.mission.mission_id,
            "One bounded repository maintenance stage.",
            self.mission.repository_identity,
            ("focused", "full", "independent review"),
            ("no external effects",),
            (card_id,),
            self.mission.rollback,
            None,
            None,
        )
        return stage, card

    def make_steward(self, worker, reviewer, verifier=None):
        with mock.patch.object(
            workers,
            "run_allowlisted_checks",
            return_value=[{"command": "git diff --check", "exit_code": 0}],
        ):
            return steward.Steward(
                repository=contract.CAMPAIGN_REPOSITORY,
                repo_path=self.root,
                journal=StewardJournal(self.root / "journal.sqlite3"),
                github=steward_github.FakeGitHubReader(dict(self.facts.__dict__)),
                worker=worker,
                reviewer=reviewer,
                lock_dir=self.root / "locks",
            )

    def run_with_facts(self, current_worker, current_reviewer, *, git_heads=None):
        service = self.make_steward(current_worker, current_reviewer)
        head_patch = (
            mock.patch.object(steward, "_git_head", return_value=HEAD)
            if git_heads is None
            else mock.patch.object(steward, "_git_head", side_effect=git_heads)
        )
        actual_paths = (
            [(), (self.card.allowed_paths[0],), (self.card.allowed_paths[0],)]
            if git_heads is not None
            else [(self.card.allowed_paths[0],), (self.card.allowed_paths[0],)]
        )
        diff_patch = mock.patch.object(steward, "_git_changed_paths", side_effect=actual_paths)
        clean_patch = mock.patch.object(steward, "_git_worktree_clean")
        identity_patch = mock.patch.object(steward, "_git_repository_identity", return_value=True)
        range_patch = mock.patch.object(
            workers,
            "review_range_digest",
            return_value=hashlib.sha256(f"{BASE}...{HEAD}".encode("ascii")).hexdigest(),
        )
        metadata_heads = [BASE, HEAD, HEAD] if git_heads is None else [BASE, BASE, BASE, HEAD, HEAD]
        metadata_patch = mock.patch.object(
            steward,
            "_git_metadata_snapshot",
                side_effect=lambda _worktree, **_kwargs: (
                {f"refs/heads/{self.mock_worktree_branch}": metadata_heads.pop(0)},
                "config",
            ),
        )
        with mock.patch.object(
            worktree_manager,
            "create_steward_worktree",
            return_value=(
                str(self.mock_worktree_path), self.mock_worktree_branch, BASE, None
            ),
        ), head_patch, diff_patch, clean_patch, identity_patch, range_patch, metadata_patch:
            return service.dispatch_card(
                self.mission,
                self.stage,
                self.card,
                base_sha=BASE,
                stage_pr=self.facts,
            ), service

    def run_stage_through_service(self, current_worker, current_reviewer):
        instance = self.make_steward(current_worker, current_reviewer)
        implementation_paths = [
            (self.card.allowed_paths[0],),
            (self.card.allowed_paths[0],),
        ]
        metadata_heads = [BASE, HEAD, HEAD]
        with (
            mock.patch.object(
                worktree_manager,
                "create_steward_worktree",
                return_value=(
                    str(self.mock_worktree_path), self.mock_worktree_branch, BASE, None
                ),
            ),
            mock.patch.object(steward, "_git_head", return_value=HEAD),
            mock.patch.object(
                steward, "_git_changed_paths", side_effect=implementation_paths
            ),
            mock.patch.object(steward, "_git_worktree_clean"),
            mock.patch.object(steward, "_git_repository_identity", return_value=True),
            mock.patch.object(
                workers,
                "review_range_digest",
                return_value=hashlib.sha256(f"{BASE}...{HEAD}".encode("ascii")).hexdigest(),
            ),
            mock.patch.object(
                steward,
                "_git_metadata_snapshot",
                side_effect=lambda _worktree, **_kwargs: (
                    {f"refs/heads/{self.mock_worktree_branch}": metadata_heads.pop(0)},
                    "config",
                ),
            ),
        ):
            return instance.execute_stage(
                self.mission,
                self.stage,
                (self.card,),
                base_sha=BASE,
                stage_pr=self.facts,
            ), instance

    def test_approved_card_reaches_waiting_for_merge_without_merge(self):
        implementation = workers.WorkerOutcome(
            "PASS", "impl-session", HEAD, (self.card.allowed_paths[0],)
        )
        review = self.review()
        result, service = self.run_with_facts(
            self.process_worker(lambda _context: implementation),
            self.process_reviewer(lambda _context, _outcome: review),
        )
        self.assertEqual(result.status, "WAITING_FOR_MERGE")
        self.assertEqual(result.pr_number, 42)
        self.assertFalse(result.to_wire()["automatic_merge"])
        self.assertEqual(
            service.journal.projection()["card_states"][self.card.card_id],
            "WAITING_FOR_MERGE",
        )
        events = service.journal.replay()
        self.assertIn("LOCAL_REVIEW_OBSERVED", [event.event for event in events])
        self.assertNotIn("REVIEW_PASSED", [event.event for event in events])

    def test_approved_stage_reaches_waiting_for_merge_through_service_entrypoint(self):
        implementation = workers.WorkerOutcome("PASS", "impl-session", HEAD, (self.card.allowed_paths[0],))
        review = self.review()
        results, instance = self.run_stage_through_service(
            self.process_worker(lambda _context: implementation),
            self.process_reviewer(lambda _context, _outcome: review),
        )
        self.assertEqual(results[self.card.card_id].status, "WAITING_FOR_MERGE")
        self.assertEqual(results[self.card.card_id].pr_number, 42)
        self.assertFalse(results[self.card.card_id].to_wire()["automatic_merge"])
        events = instance.journal.replay()
        self.assertEqual(events[0].event, "HEARTBEAT")
        self.assertEqual(
            instance.journal.projection()["card_states"][self.card.card_id],
            "WAITING_FOR_MERGE",
        )

    def test_parent_stage_promotion_stops_at_waiting_for_merge(self):
        instance = self.make_steward(None, None)
        integration = steward.StageIntegration(
            self.stage.stage_id,
            steward._stage_branch(self.mission, self.stage, BASE),
            BASE,
            HEAD,
            ((self.card.card_id, HEAD),),
        )
        bound_stage = replace(self.stage, integration_pr=42, exact_head=HEAD)
        waiting = steward.ExecutionResult(
            self.card.card_id, "WAITING_FOR_MERGE", 1, HEAD,
            "exact_head_ci_and_review_pass", None, 42,
        )
        with (
            mock.patch.object(instance, "execute_stage", return_value={
                self.card.card_id: steward.ExecutionResult(
                    self.card.card_id, "WAITING_FOR_PR", 1, HEAD,
                    "local_review_pass",
                )
            }) as execute,
            mock.patch.object(instance, "assemble_stage", return_value=integration) as assemble,
            mock.patch.object(instance, "publish_stage_branch") as publish,
            mock.patch.object(instance, "bind_stage_draft_pr", return_value=(
                bound_stage, {"number": 42, "head_sha": HEAD}
            )) as bind,
            mock.patch.object(
                instance.github, "fetch_stage_pr", return_value=self.facts.__dict__
            ) as fetch,
            mock.patch.object(instance, "reconcile_stage_pr", return_value={
                self.card.card_id: waiting
            }) as reconcile,
        ):
            result = instance.execute_stage_to_waiting_for_merge(
                self.mission,
                self.stage,
                (self.card,),
                base_sha=BASE,
                title="Stage",
                body="Body",
            )
        self.assertEqual(result["stage"], bound_stage)
        self.assertEqual(result["integration"], integration)
        self.assertEqual(result["results"][self.card.card_id].status, "WAITING_FOR_MERGE")
        execute.assert_called_once()
        assemble.assert_called_once()
        publish.assert_called_once_with(
            integration, mission=self.mission, stage=self.stage
        )
        bind.assert_called_once()
        fetch.assert_called_once_with(contract.CAMPAIGN_REPOSITORY, 42)
        reconcile.assert_called_once()

    def test_parent_stage_continuation_reuses_ready_pr_reconciliation(self):
        instance = self.make_steward(None, None)
        waiting = steward.ExecutionResult(
            self.card.card_id, "WAITING_FOR_MERGE", 1, HEAD,
            "exact_head_ci_and_review_pass", None, 42,
        )
        with mock.patch.object(instance, "reconcile_stage_pr", return_value={
            self.card.card_id: waiting
        }) as reconcile, mock.patch(
            "pr_binding.create_or_update_stage_pr",
            side_effect=AssertionError("Ready continuation must not update the PR"),
        ):
            result = instance.continue_stage_to_waiting_for_merge(
                self.mission,
                self.stage,
                (self.card,),
                stage_pr=self.facts,
            )
        self.assertEqual(result["status"], "waiting_for_merge")
        self.assertEqual(result["stage"].integration_pr, 42)
        self.assertEqual(result["stage"].exact_head, HEAD)
        reconcile.assert_called_once()

    def test_parent_stage_continuation_reports_merged_stage_complete(self):
        instance = self.make_steward(None, None)
        complete = steward.ExecutionResult(
            self.card.card_id, "COMPLETE", 1, HEAD,
            "pr_already_merged", None, 42,
        )
        with mock.patch.object(instance, "reconcile_stage_pr", return_value={
            self.card.card_id: complete
        }) as reconcile:
            result = instance.continue_stage_to_waiting_for_merge(
                self.mission,
                self.stage,
                (self.card,),
                stage_pr=self.facts,
            )
        self.assertEqual(result["status"], "complete")
        self.assertEqual(result["results"][self.card.card_id].status, "COMPLETE")
        reconcile.assert_called_once()

    def test_stage_publish_refuses_local_branch_head_drift_before_push(self):
        instance = self.make_steward(None, None)
        integration = steward.StageIntegration(
            self.stage.stage_id,
            steward._stage_branch(self.mission, self.stage, BASE),
            BASE,
            HEAD,
            ((self.card.card_id, HEAD),),
        )
        with mock.patch.object(instance, "_git_text", return_value=BASE) as git:
            with self.assertRaisesRegex(
                steward.StewardError, "stage_local_branch_head_mismatch"
            ):
                instance.publish_stage_branch(
                    integration, mission=self.mission, stage=self.stage
                )
        self.assertEqual(
            git.call_args_list,
            [
                mock.call(
                    "merge-base", "--is-ancestor", BASE, HEAD, allow_failure=True
                ),
                mock.call(
                    "rev-parse", "--verify",
                    f"refs/heads/{integration.branch}",
                    allow_failure=True,
                ),
            ],
        )

    def test_stage_publish_refuses_remote_read_failure_before_push(self):
        instance = self.make_steward(None, None)
        integration = steward.StageIntegration(
            self.stage.stage_id,
            steward._stage_branch(self.mission, self.stage, BASE),
            BASE,
            HEAD,
            ((self.card.card_id, HEAD),),
        )

        def git(*args, **_kwargs):
            if args[0] == "merge-base":
                return ""
            if args[0] == "rev-parse":
                return HEAD
            if args[0] == "ls-remote":
                raise steward.StewardError("stage_git_command_failed")
            raise AssertionError("publish must not run after remote read failure")

        with mock.patch.object(instance, "_git_text", side_effect=git):
            with self.assertRaisesRegex(
                steward.StewardError, "stage_remote_head_unavailable"
            ):
                instance.publish_stage_branch(
                    integration, mission=self.mission, stage=self.stage
                )

    def test_parent_assembly_cherry_picks_two_real_card_heads(self):
        repo = self.root / "Projects" / "repo"
        repo.mkdir(parents=True)
        (self.root / ".worktrees").mkdir()

        def git(*args):
            return subprocess.run(
                ["git", *args], cwd=repo, check=True,
                capture_output=True, text=True,
            ).stdout.strip()

        git("init", "-b", "main")
        git("config", "user.name", "Test")
        git("config", "user.email", "test@example.invalid")
        (repo / "docs").mkdir()
        (repo / "tests").mkdir()
        (repo / "docs" / "ARCHITECTURE_BOOK.md").write_text("base docs\n", encoding="utf-8")
        (repo / "docs" / "RUNBOOK.md").write_text("base runbook\n", encoding="utf-8")
        (repo / "tests" / "test_mission_contract.py").write_text("base tests\n", encoding="utf-8")
        (repo / "tests" / "test_review_loop.py").write_text("base review tests\n", encoding="utf-8")
        git("add", ".")
        git("commit", "-m", "base")
        base_sha = git("rev-parse", "HEAD")

        git("switch", "-c", "agent/card-a")
        (repo / "docs" / "ARCHITECTURE_BOOK.md").write_text("card a\n", encoding="utf-8")
        git("add", "docs/ARCHITECTURE_BOOK.md")
        git("commit", "-m", "card a")
        head_a = git("rev-parse", "HEAD")
        git("switch", "main")
        git("switch", "-c", "agent/card-b")
        (repo / "tests" / "test_mission_contract.py").write_text("card b\n", encoding="utf-8")
        git("add", "tests/test_mission_contract.py")
        git("commit", "-m", "card b")
        head_b = git("rev-parse", "HEAD")
        git("switch", "main")

        mission = replace(
            self.mission,
            repository_identity=replace(
                self.mission.repository_identity, base_sha=base_sha
            ),
        )
        stage_id = "stage-real"
        cards = (
            contract.WorkCard(
                "card-a", stage_id, ("docs/ARCHITECTURE_BOOK.md",),
                ("outside-approved/",), ("apply",), ("check",), ("negative",),
                ("receipt",), (), ("docs/ARCHITECTURE_BOOK.md",), 1, "T1",
                mission.rollback, "PENDING",
            ),
            contract.WorkCard(
                "card-b", stage_id, ("tests/test_mission_contract.py",),
                ("outside-approved/",), ("apply",), ("check",), ("negative",),
                ("receipt",), (), ("tests/test_mission_contract.py",), 1, "T1",
                mission.rollback, "PENDING",
            ),
        )
        stage = contract.Stage(
            stage_id, mission.mission_id, "real assembly", mission.repository_identity,
            ("checks",), ("no effects",), ("card-a", "card-b"),
            mission.rollback, None, None,
        )
        instance = steward.Steward(
            repository=contract.CAMPAIGN_REPOSITORY,
            repo_path=repo,
            journal=StewardJournal(repo / "journal.sqlite3"),
            github=steward_github.FakeGitHubReader(),
            lock_dir=repo / "locks",
        )
        results = {
            "card-a": steward.ExecutionResult("card-a", "WAITING_FOR_PR", 1, head_a, "reviewed"),
            "card-b": steward.ExecutionResult("card-b", "WAITING_FOR_PR", 1, head_b, "reviewed"),
        }
        integration = instance.assemble_stage(
            mission, stage, cards, results, base_sha=base_sha
        )
        self.assertEqual(integration.base_sha, base_sha)
        self.assertNotEqual(integration.head_sha, base_sha)
        self.assertEqual(
            set(git("diff", "--name-only", f"{base_sha}..{integration.head_sha}").splitlines()),
            {"docs/ARCHITECTURE_BOOK.md", "tests/test_mission_contract.py"},
        )

    def test_real_workcards_run_through_k_two_dependencies_and_assembly(self):
        repo = self.root / "Projects" / "repo"
        repo.mkdir(parents=True)
        origin = self.root / "origin.git"
        (self.root / ".worktrees").mkdir()
        (self.root / "worker-worktrees").mkdir()

        def git(*args):
            return subprocess.run(
                ["git", *args], cwd=repo, check=True,
                capture_output=True, text=True,
            ).stdout.strip()

        subprocess.run(["git", "init", "--bare", str(origin)], check=True, capture_output=True)
        git("init", "-b", "main")
        git("config", "user.name", "Test")
        git("config", "user.email", "test@example.invalid")
        (repo / "docs").mkdir()
        (repo / "tests").mkdir()
        (repo / "docs" / "ARCHITECTURE_BOOK.md").write_text("base docs\n", encoding="utf-8")
        (repo / "docs" / "RUNBOOK.md").write_text("base runbook\n", encoding="utf-8")
        (repo / "tests" / "test_mission_contract.py").write_text("base tests\n", encoding="utf-8")
        git("add", ".")
        git("commit", "-m", "base")
        base_sha = git("rev-parse", "HEAD")
        git("remote", "add", "origin", str(origin))
        git("push", "origin", "main")

        mission = replace(
            self.mission,
            repository_identity=replace(self.mission.repository_identity, base_sha=base_sha),
        )
        stage_id = "stage-real-execution"
        cards = (
            contract.WorkCard(
                "card-a", stage_id, ("docs/ARCHITECTURE_BOOK.md",),
                (), ("apply",), ("check",), ("negative",), ("receipt",),
                (), ("docs/ARCHITECTURE_BOOK.md",), 1, "T1", mission.rollback, "PENDING",
            ),
            contract.WorkCard(
                "card-b", stage_id, ("tests/test_mission_contract.py",),
                (), ("apply",), ("check",), ("negative",), ("receipt",),
                (), ("tests/test_mission_contract.py",), 1, "T1", mission.rollback, "PENDING",
            ),
            contract.WorkCard(
                "card-c", stage_id, ("docs/RUNBOOK.md", "docs/ARCHITECTURE_BOOK.md"),
                (), ("integrate",), ("check",), ("negative",), ("receipt",),
                ("card-a",), ("docs/ARCHITECTURE_BOOK.md",), 1, "T1", mission.rollback, "PENDING",
            ),
            contract.WorkCard(
                "card-d", stage_id, ("tests/test_review_loop.py", "docs/ARCHITECTURE_BOOK.md"),
                (), ("integrate",), ("check",), ("negative",), ("receipt",),
                ("card-b",), ("docs/ARCHITECTURE_BOOK.md",), 1, "T1", mission.rollback, "PENDING",
            ),
        )
        stage = contract.Stage(
            stage_id, mission.mission_id, "real worker execution", mission.repository_identity,
            ("focused",), ("no effects",), ("card-a", "card-b", "card-c", "card-d"),
            mission.rollback, None, None,
        )
        active = 0
        maximum = 0
        overlap_active = 0
        overlap_maximum = 0
        active_lock = threading.Lock()
        concurrency_barrier = threading.Barrier(2)

        class RealTestWorker(workers.BoundedProcessWorker):
            def __init__(self):
                super().__init__(lambda _context: ["/usr/bin/python3", "-c", "pass"])

            def run(self, context):
                nonlocal active, maximum, overlap_active, overlap_maximum
                if context.card_id in {"card-a", "card-b"}:
                    try:
                        concurrency_barrier.wait(timeout=1.0)
                    except threading.BrokenBarrierError:
                        pass
                with active_lock:
                    active += 1
                    maximum = max(maximum, active)
                    if context.card_id in {"card-c", "card-d"}:
                        overlap_active += 1
                        overlap_maximum = max(overlap_maximum, overlap_active)
                try:
                    relative = context.allowed_paths[0]
                    target = context.worktree / relative
                    target.write_text(f"real worker {context.card_id}\n", encoding="utf-8")
                    env = {
                        **os.environ,
                        "GIT_AUTHOR_NAME": "Test",
                        "GIT_AUTHOR_EMAIL": "test@example.invalid",
                        "GIT_COMMITTER_NAME": "Test",
                        "GIT_COMMITTER_EMAIL": "test@example.invalid",
                    }
                    subprocess.run(
                        ["git", "add", "--", relative], cwd=context.worktree,
                        check=True, capture_output=True, text=True,
                    )
                    subprocess.run(
                        ["git", "commit", "-m", f"worker {context.card_id}"],
                        cwd=context.worktree, check=True, capture_output=True, text=True,
                        env=env,
                    )
                    head = subprocess.run(
                        ["git", "rev-parse", "HEAD"], cwd=context.worktree,
                        check=True, capture_output=True, text=True,
                    ).stdout.strip()
                    return workers.WorkerOutcome(
                        "PASS", workers.process_session_id(context), head, (relative,),
                        "real_worker_commit",
                    )
                finally:
                    with active_lock:
                        active -= 1
                        if context.card_id in {"card-c", "card-d"}:
                            overlap_active -= 1

        class RealTestReviewer(workers.BoundedProcessReviewer):
            def __init__(self):
                super().__init__(lambda _context, _outcome: ["/usr/bin/python3", "-c", "pass"])

            def review(self, context, outcome):
                payload = {
                    "schema_version": "steward_review_outcome.v1",
                    "status": "PASS",
                    "reviewer_session_id": workers.reviewer_session_id(context, outcome),
                    "implementation_session_id": outcome.session_id,
                    "reviewed_head_sha": outcome.head_sha,
                    "blockers": [],
                    "detail": "real_independent_review",
                    "reviewed_base_sha": context.base_sha,
                    "reviewed_range_sha256": workers.review_range_digest(
                        context.base_sha, outcome.head_sha, worktree=context.worktree
                    ),
                    "review_axes": ["standards", "spec"],
                    "review_round": 1,
                    "review_mode": "full",
                    "review_receipt_sha256": "",
                    "summary": "real independent review",
                    "findings": None,
                    "security_ok": True,
                    "rollback_ok": True,
                    "observed_ci_status": "unknown",
                    "finding_ledger_digest": "",
                }
                return workers.ReviewOutcome.from_wire(
                    workers.seal_review_outcome_wire(payload)
                )

        with (
            mock.patch.object(worktree_manager, "WORKTREE_BASE", self.root / "worker-worktrees"),
            mock.patch.object(steward, "_git_repository_identity", return_value=True),
            mock.patch.object(
                workers,
                "run_allowlisted_checks",
                return_value=[{"command": "git diff --check", "exit_code": 0}],
            ),
        ):
            instance = steward.Steward(
                repository=contract.CAMPAIGN_REPOSITORY,
                repo_path=repo,
                journal=StewardJournal(repo / "journal.sqlite3"),
                github=steward_github.FakeGitHubReader(),
                worker=RealTestWorker(),
                reviewer=RealTestReviewer(),
                lock_dir=repo / "locks",
            )
            results = instance.dispatch_cards(mission, stage, cards, base_sha=base_sha)
            self.assertEqual(
                {card_id: result.status for card_id, result in results.items()},
                {
                    "card-a": "WAITING_FOR_PR",
                    "card-b": "WAITING_FOR_PR",
                    "card-c": "WAITING_FOR_PR",
                    "card-d": "WAITING_FOR_PR",
                },
                instance.journal.replay(),
            )
            integration = instance.assemble_stage(
                mission, stage, cards, results, base_sha=base_sha
            )

        self.assertEqual(
            set(git("diff", "--name-only", f"{base_sha}..{integration.head_sha}").splitlines()),
            {
                "docs/ARCHITECTURE_BOOK.md",
                "docs/RUNBOOK.md",
                "tests/test_mission_contract.py",
                "tests/test_review_loop.py",
            },
        )
        self.assertEqual(maximum, 2)
        self.assertEqual(overlap_maximum, 1)

    def test_worker_exception_is_outcome_unknown_and_is_never_retried(self):
        def explode(_context):
            raise RuntimeError("worker stopped after mutation boundary")

        result, service = self.run_with_facts(
            ExplodingWorker(lambda _context: []),
            None,
        )
        self.assertEqual(result.status, "OUTCOME_UNKNOWN")
        self.assertEqual(
            service.journal.projection()["card_states"][self.card.card_id],
            "OUTCOME_UNKNOWN",
        )

    def test_worker_exception_with_only_uncommitted_residue_replans(self):
        service = self.make_steward(
            ExplodingWorker(lambda _context: []),
            None,
        )
        metadata = ({f"refs/heads/{self.mock_worktree_branch}": BASE}, "config")
        with (
            mock.patch.object(
                worktree_manager,
                "create_steward_worktree",
                return_value=(
                    str(self.mock_worktree_path), self.mock_worktree_branch, BASE, None
                ),
            ),
            mock.patch.object(steward, "_git_head", return_value=BASE),
            mock.patch.object(steward, "_git_changed_paths", return_value=()),
            mock.patch.object(
                steward,
                "_git_worktree_clean",
                side_effect=workers.WorkerError("worktree_dirty_after_worker"),
            ),
            mock.patch.object(steward, "_git_repository_identity", return_value=True),
            mock.patch.object(steward, "_git_metadata_snapshot", return_value=metadata),
        ):
            result = service.dispatch_card(
                self.mission,
                self.stage,
                self.card,
                base_sha=BASE,
                stage_pr=self.facts,
            )
        self.assertEqual(result.status, "BLOCKED")
        self.assertEqual(
            result.reason, "worker_exception_with_local_uncommitted_residue"
        )
        self.assertNotIn(
            "WORKER_OUTCOME_UNKNOWN", [event.event for event in service.journal.replay()]
        )

    def test_worker_status_observation_failure_remains_outcome_unknown(self):
        service = self.make_steward(
            ExplodingWorker(lambda _context: []),
            None,
        )
        metadata = ({f"refs/heads/{self.mock_worktree_branch}": BASE}, "config")
        with (
            mock.patch.object(
                worktree_manager,
                "create_steward_worktree",
                return_value=(
                    str(self.mock_worktree_path), self.mock_worktree_branch, BASE, None
                ),
            ),
            mock.patch.object(steward, "_git_head", return_value=BASE),
            mock.patch.object(steward, "_git_changed_paths", return_value=()),
            mock.patch.object(
                steward,
                "_git_worktree_clean",
                side_effect=steward.StewardError("worktree_status_unavailable"),
            ),
            mock.patch.object(steward, "_git_repository_identity", return_value=True),
            mock.patch.object(steward, "_git_metadata_snapshot", return_value=metadata),
        ):
            result = service.dispatch_card(
                self.mission,
                self.stage,
                self.card,
                base_sha=BASE,
                stage_pr=self.facts,
            )
        self.assertEqual(result.status, "OUTCOME_UNKNOWN")
        self.assertEqual(
            service.journal.projection()["card_states"][self.card.card_id],
            "OUTCOME_UNKNOWN",
        )

    def test_retry_escalates_tier_and_then_waits_for_merge(self):
        seen: list[tuple[int, str]] = []
        attempts = [
            workers.WorkerOutcome("FAIL", "impl-1", BASE, (), "focused worker failed"),
            workers.WorkerOutcome("PASS", "impl-2", HEAD, (self.card.allowed_paths[0],)),
        ]

        def run(context):
            seen.append((context.attempt, context.model_tier))
            return attempts.pop(0)

        review = self.review(implementation="impl-2")
        result, _service = self.run_with_facts(
            self.process_worker(run),
            self.process_reviewer(lambda _context, _outcome: review),
            git_heads=[BASE, HEAD, HEAD],
        )
        self.assertEqual(result.status, "WAITING_FOR_MERGE")
        self.assertEqual(seen, [(1, "T1"), (2, "T2")])

    def test_failed_worker_with_mutation_is_outcome_unknown_and_not_retried(self):
        class MutatingFailureWorker(workers.BoundedProcessWorker):
            def __init__(self, path):
                super().__init__(lambda _context: [], timeout_seconds=5)
                self.path = path

            def run(self, _context):
                return workers.WorkerOutcome(
                    "FAIL", "impl-failure", HEAD, (self.path,), "worker failed after mutation"
                )

        service = self.make_steward(
            MutatingFailureWorker(self.card.allowed_paths[0]), None
        )
        before = {f"refs/heads/{self.mock_worktree_branch}": BASE}
        after = {f"refs/heads/{self.mock_worktree_branch}": HEAD}
        with (
            mock.patch.object(
                worktree_manager,
                "create_steward_worktree",
                return_value=(
                    str(self.mock_worktree_path), self.mock_worktree_branch, BASE, None
                ),
            ),
            mock.patch.object(steward, "_git_head", return_value=HEAD),
            mock.patch.object(
                steward, "_git_changed_paths", return_value=(self.card.allowed_paths[0],)
            ),
            mock.patch.object(
                steward,
                "_git_worktree_clean",
                side_effect=workers.WorkerError("worktree_dirty_after_worker"),
            ),
            mock.patch.object(steward, "_git_repository_identity", return_value=True),
            mock.patch.object(
                steward,
                "_git_metadata_snapshot",
                side_effect=[(before, "config"), (after, "config")],
            ),
        ):
            result = service.dispatch_card(
                self.mission,
                self.stage,
                self.card,
                base_sha=BASE,
                stage_pr=self.facts,
            )
        self.assertEqual(result.status, "OUTCOME_UNKNOWN")
        self.assertEqual(result.attempt, 1)
        self.assertNotIn("ATTEMPT_RETRY_SCHEDULED", [event.event for event in service.journal.replay()])

    def test_failed_worker_with_only_uncommitted_residue_replans_without_unknown(self):
        class DirtyFailureWorker(workers.BoundedProcessWorker):
            def __init__(self):
                super().__init__(lambda _context: [], timeout_seconds=5)

            def run(self, _context):
                return workers.WorkerOutcome(
                    "FAIL", "impl-failure", BASE, (), "worker execution failed"
                )

        service = self.make_steward(DirtyFailureWorker(), None)
        metadata = ({f"refs/heads/{self.mock_worktree_branch}": BASE}, "config")
        with (
            mock.patch.object(
                worktree_manager,
                "create_steward_worktree",
                return_value=(
                    str(self.mock_worktree_path), self.mock_worktree_branch, BASE, None
                ),
            ),
            mock.patch.object(steward, "_git_head", return_value=BASE),
            mock.patch.object(steward, "_git_changed_paths", return_value=()),
            mock.patch.object(
                steward,
                "_git_worktree_clean",
                side_effect=workers.WorkerError("worktree_dirty_after_worker"),
            ),
            mock.patch.object(steward, "_git_repository_identity", return_value=True),
            mock.patch.object(steward, "_git_metadata_snapshot", return_value=metadata),
        ):
            result = service.dispatch_card(
                self.mission,
                self.stage,
                self.card,
                base_sha=BASE,
                stage_pr=self.facts,
            )
        self.assertEqual(result.status, "BLOCKED")
        self.assertEqual(result.reason, "worker_failed_with_local_uncommitted_residue")
        self.assertNotIn(
            "WORKER_OUTCOME_UNKNOWN", [event.event for event in service.journal.replay()]
        )

    def test_self_review_is_rejected_before_execution(self):
        with self.assertRaisesRegex(workers.WorkerError, "self_review_forbidden"):
            workers.ReviewOutcome("PASS", "same-session", "same-session", HEAD)
        with self.assertRaisesRegex(workers.WorkerError, "review_pass_has_blockers"):
            workers.ReviewOutcome(
                "PASS", "review-session", "impl-session", HEAD, blockers=("open",)
            )

    def test_review_receipt_must_seal_the_review_outcome(self):
        review = self.review()
        with self.assertRaisesRegex(workers.WorkerError, "receipt_digest_mismatch"):
            workers.ReviewOutcome(
                review.status,
                review.reviewer_session_id,
                review.implementation_session_id,
                review.reviewed_head_sha,
                review.blockers,
                review.detail,
                review.reviewed_base_sha,
                review.reviewed_range_sha256,
                review.review_axes,
                review.review_round,
                review.review_mode,
                "d" * 64,
            )

    def test_r2_applies_canonical_prior_blocker_resolution(self):
        prior = {
            "base_sha": BASE,
            "head_sha": "a" * 40,
            "review_round": 1,
            "review_mode": "full",
            "verdict": "FAIL",
            "open_blocker_ids": ["blocker-1"],
            "deferred_note_ids": [],
            "decision_required_ids": [],
            "finding_ledger_digest": "c" * 64,
            "security_ok": True,
            "rollback_ok": True,
        }
        review = self.review(head=HEAD, review_round=2, review_mode="repair_verification")
        next_state = steward.review_convergence.apply_r2_decision(
            steward.Steward._prior_review_state(prior),
            workers.canonical_review_decision(review),
        )
        self.assertEqual(next_state.verdict, "PASS")
        self.assertEqual(next_state.open_blocker_ids, ())
        self.assertEqual(next_state.review_round, 2)

    def test_journal_keys_bind_stage_identity(self):
        first = steward._journal_key("queue", self.mission, self.stage, self.card, 1, BASE)
        second_stage = replace(self.stage, stage_id="stage-2")
        second = steward._journal_key("queue", self.mission, second_stage, self.card, 1, BASE)
        self.assertNotEqual(first, second)
        self.assertIn(self.mission.mission_id, first)
        self.assertIn(self.stage.stage_id, first)

    def test_review_head_drift_blocks_exact_head_delivery(self):
        implementation = workers.WorkerOutcome("PASS", "impl-session", HEAD, ("docs/ARCHITECTURE_BOOK.md",))
        review = self.review(head="c" * 40)
        result, service = self.run_with_facts(
            self.process_worker(lambda _context: implementation),
            self.process_reviewer(lambda _context, _outcome: review),
        )
        self.assertEqual(result.status, "BLOCKED")
        self.assertEqual(result.reason, "review_head_binding_mismatch")
        self.assertEqual(service.journal.projection()["card_states"]["card-1"], "BLOCKED")

    def test_stage_exact_head_is_checked_against_live_stage_pr(self):
        self.stage = replace(self.stage, integration_pr=42, exact_head="c" * 40)
        implementation = workers.WorkerOutcome(
            "PASS", "impl-session", HEAD, (self.card.allowed_paths[0],)
        )
        result, _service = self.run_with_facts(
            self.process_worker(lambda _context: implementation),
            self.process_reviewer(lambda _context, _outcome: self.review()),
        )
        self.assertEqual(result.status, "BLOCKED")
        self.assertEqual(result.reason, "stage_exact_head_mismatch")

    def test_observed_diff_is_checked_against_card_scope(self):
        implementation = workers.WorkerOutcome(
            "PASS", "impl-session", HEAD, ("docs/ARCHITECTURE_BOOK.md",)
        )
        review = self.review()
        instance = self.make_steward(
            self.process_worker(lambda _context: implementation),
            self.process_reviewer(lambda _context, _outcome: review),
        )
        with (
            mock.patch.object(
                worktree_manager,
                "create_steward_worktree",
                return_value=(
                    str(self.mock_worktree_path), self.mock_worktree_branch, BASE, None
                ),
            ),
            mock.patch.object(steward, "_git_head", return_value=HEAD),
            mock.patch.object(
                steward, "_git_changed_paths", return_value=("tests/test_mission_contract.py",)
            ),
            mock.patch.object(steward, "_git_worktree_clean"),
            mock.patch.object(steward, "_git_repository_identity", return_value=True),
            mock.patch.object(
                steward,
                "_git_metadata_snapshot",
                return_value=({f"refs/heads/{self.mock_worktree_branch}": BASE}, "config"),
            ),
        ):
            result = instance.dispatch_card(
                self.mission, self.stage, self.card, base_sha=BASE, stage_pr=self.facts
            )
        self.assertEqual(result.status, "BLOCKED")
        self.assertEqual(result.reason, "worker_path_outside_card")

    def test_default_worker_is_the_explicit_production_codex_adapter(self):
        instance = steward.Steward(
            repository=contract.CAMPAIGN_REPOSITORY,
            repo_path=self.root,
            journal=StewardJournal(self.root / "journal.sqlite3"),
            github=steward_github.FakeGitHubReader(),
            reviewer=None,
            lock_dir=self.root / "locks",
        )
        self.assertIsInstance(instance.worker, workers.CodexWorkCardWorker)
        self.assertIsInstance(instance.reviewer, workers.CodexWorkCardReviewer)

    def test_production_child_environment_allows_only_literal_loopback_proxy(self):
        environment = workers.child_environment(
            {
                "HOME": str(self.root),
                "PATH": "/caller-controlled-path",
                "HTTPS_PROXY": "http://127.0.0.1:7897",
                "HTTP_PROXY": "http://user:password@127.0.0.1:7897",
                "ALL_PROXY": "https://[::1]:7897/",
            },
            preserve_home=True,
        )
        self.assertEqual(environment["HTTPS_PROXY"], "http://127.0.0.1:7897")
        self.assertEqual(environment["ALL_PROXY"], "https://[::1]:7897/")
        self.assertNotIn("HTTP_PROXY", environment)

    def test_production_child_environment_bounds_optional_codex_model(self):
        environment = workers.child_environment(
            {"HOME": str(self.root), "AGENT_CODEX_MODEL": "gpt-5.2-codex"},
            preserve_home=True,
        )
        self.assertEqual(environment["AGENT_CODEX_MODEL"], "gpt-5.2-codex")
        with self.assertRaisesRegex(workers.WorkerError, "codex_model_invalid"):
            workers.child_environment(
                {"HOME": str(self.root), "AGENT_CODEX_MODEL": "--model=bad value"},
                preserve_home=True,
            )

    def test_production_reviewer_accepts_one_json_fence(self):
        raw = b'```json\n{"verdict":"PASS","blockers":[],"summary":"bounded"}\n```\n'
        self.assertEqual(
            workers.CodexWorkCardReviewer._decode_response(raw),
            {"verdict": "PASS", "blockers": [], "summary": "bounded"},
        )

    def test_production_reviewer_accepts_bounded_prose_around_one_json_object(self):
        raw = b'Review complete.\n{"verdict":"PASS","blockers":[],"summary":"bounded"}'
        self.assertEqual(
            workers.CodexWorkCardReviewer._decode_response(raw),
            {"verdict": "PASS", "blockers": [], "summary": "bounded"},
        )

    def test_production_reviewer_rejects_multiple_json_objects(self):
        raw = b'{"verdict":"PASS"}\n{"blockers":[],"summary":"bounded"}'
        with self.assertRaisesRegex(workers.WorkerError, "codex_review_output_invalid"):
            workers.CodexWorkCardReviewer._decode_response(raw)

    def test_production_reviewer_bounds_non_authoritative_summary(self):
        normalize = workers.CodexWorkCardReviewer._bounded_summary
        self.assertEqual(normalize("first line\n second\tline"), "first line second line")
        self.assertEqual(normalize(" \r\n "), "structured review verdict")
        self.assertEqual(len(normalize("x" * 700)), 512)

    def test_production_worker_retains_only_allowlisted_failure_category(self):
        output_dir = self.root / "failure-output"
        output_dir.mkdir()
        (output_dir / "failure_reason.json").write_text(
            json.dumps(
                {
                    "kind": "agent-orchestrator-failure",
                    "reason": "model_execution_failure",
                    "detail": "provider output must not reach the journal",
                }
            ),
            encoding="utf-8",
        )
        self.assertEqual(
            workers.CodexWorkCardWorker._bounded_failure_reason(output_dir),
            "model_execution_failure",
        )
        (output_dir / "failure_reason.json").write_text(
            json.dumps(
                {
                    "kind": "agent-orchestrator-failure",
                    "reason": "attacker-controlled-detail",
                    "detail": "private provider output",
                }
            ),
            encoding="utf-8",
        )
        self.assertIsNone(
            workers.CodexWorkCardWorker._bounded_failure_reason(output_dir)
        )

    def test_production_worker_returns_bounded_wrapper_failure_before_temp_cleanup(self):
        wrapper = self.root / "failing-wrapper"
        wrapper.write_text(
            """#!/usr/bin/env python3
import json
from pathlib import Path
import sys
output = Path(sys.argv[3])
output.mkdir(parents=True, exist_ok=True)
(output / "failure_reason.json").write_text(json.dumps({
    "kind": "agent-orchestrator-failure",
    "reason": "authentication_failure",
    "detail": "private provider output must be discarded",
}), encoding="utf-8")
raise SystemExit(1)
""",
            encoding="utf-8",
        )
        wrapper.chmod(0o755)
        worker = workers.CodexWorkCardWorker(
            wrapper_path=wrapper, timeout_seconds=5
        )
        exit_code, response_path, failure_reason = worker._invoke(
            "implement",
            "bounded workcard",
            self.root,
            environment={
                "HOME": str(self.root),
                "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
            },
        )
        self.assertEqual(exit_code, 1)
        self.assertEqual(failure_reason, "authentication_failure")
        self.assertFalse(response_path.exists())

    def test_production_worker_sandbox_exposes_minimal_codex_home(self):
        """The isolated HOME must not expose the host Codex tree."""
        bin_dir = self.root / "codex-bin"
        bin_dir.mkdir()
        fake = bin_dir / "codex"
        fake.write_text(
            """#!/usr/bin/env python3
import json
import os
from pathlib import Path
import sys

args = sys.argv[1:]
if args == ["--version"]:
    print("1.0.0")
elif args[:2] == ["exec", "--help"]:
    print("--json --ephemeral --ignore-user-config --skip-git-repo-check --sandbox --model --cd --output-last-message")
elif args and args[0] == "exec":
    codex_home = Path(os.environ["CODEX_HOME"])
    if codex_home != Path(os.environ["HOME"]) / ".codex":
        raise SystemExit(3)
    auth = codex_home / "auth.json"
    if auth.is_file() and auth.read_text(encoding="utf-8") != "runtime-auth":
        raise SystemExit(7)
    if (Path(os.environ["HOME"]) / ".codex" / "accounts").exists():
        raise SystemExit(4)
    resolver = Path("/etc/resolv.conf")
    if not resolver.is_file() or "nameserver" not in resolver.read_text(encoding="utf-8"):
        raise SystemExit(5)
    if not Path("/etc/hosts").is_file():
        raise SystemExit(6)
    output = Path(args[args.index("--output-last-message") + 1])
    output.write_text("done", encoding="utf-8")
    print(json.dumps({"type":"turn.completed"}))
else:
    raise SystemExit(2)
""",
            encoding="utf-8",
        )
        fake.chmod(0o755)
        (self.root / ".codex").mkdir()
        (self.root / ".codex" / "auth.json").write_text(
            "runtime-auth", encoding="utf-8"
        )
        worker = workers.CodexWorkCardWorker(timeout_seconds=5)
        exit_code, response_path, failure_reason = worker._invoke(
            "implement",
            "bounded workcard",
            self.root,
            environment={
                "HOME": str(self.root),
                "PATH": f"{bin_dir}:/usr/bin:/bin",
            },
        )
        self.assertEqual(exit_code, 0)
        self.assertIsNone(failure_reason)
        self.assertFalse(response_path.exists())

    def test_production_worker_uses_systemd_credential_and_fixed_service_binary(self):
        """The managed service must not depend on an interactive user's HOME."""
        credential_dir = self.root / "systemd-credentials"
        credential_dir.mkdir(mode=0o700)
        auth = credential_dir / "codex-auth"
        auth.write_text("service-runtime-auth", encoding="utf-8")
        auth.chmod(0o400)
        service_codex = self.root / "service-codex"
        service_codex.write_text(
            """#!/usr/bin/env python3
import json
import os
from pathlib import Path
import sys

args = sys.argv[1:]
if args == ["--version"]:
    print("1.0.0")
elif args[:2] == ["exec", "--help"]:
    print("--json --ephemeral --ignore-user-config --skip-git-repo-check --sandbox --model --cd --output-last-message")
elif args and args[0] == "exec":
    auth = Path(os.environ["CODEX_HOME"]) / "auth.json"
    if auth.read_text(encoding="utf-8") != "service-runtime-auth":
        raise SystemExit(7)
    output = Path(args[args.index("--output-last-message") + 1])
    output.write_text('{"verdict":"PASS","blockers":[],"summary":"bounded"}', encoding="utf-8")
    print(json.dumps({"type": "turn.completed"}))
else:
    raise SystemExit(2)
""",
            encoding="utf-8",
        )
        service_codex.chmod(0o755)
        worker = workers.CodexWorkCardWorker(timeout_seconds=5)
        with (
            mock.patch.dict(
                os.environ,
                {"CREDENTIALS_DIRECTORY": str(credential_dir)},
                clear=False,
            ),
            mock.patch.object(
                workers, "SERVICE_CODEX_BINARY", service_codex, create=True
            ),
        ):
            exit_code, response_path, failure_reason = worker._invoke(
                "review",
                "bounded review",
                self.root,
                environment={"HOME": "/nonexistent", "PATH": "/usr/bin:/bin"},
            )
        self.addCleanup(
            lambda: response_path.parent.rmdir()
            if response_path.parent.exists()
            and response_path.parent.name.startswith("steward-codex-review-")
            else None
        )
        self.addCleanup(
            lambda: response_path.unlink(missing_ok=True)
            if response_path.exists()
            else None
        )
        self.assertEqual(exit_code, 0)
        self.assertIsNone(failure_reason)
        self.assertIn('"verdict":"PASS"', response_path.read_text(encoding="utf-8"))

    def test_managed_service_declares_bounded_codex_runtime_sources(self):
        unit = (ROOT / "scripts" / "agent-control" / "steward.service").read_text(
            encoding="utf-8"
        )
        self.assertIn(
            "LoadCredentialEncrypted=codex-auth:/etc/credstore.encrypted/agent-steward.codex-auth",
            unit,
        )
        self.assertIn(
            "ReadOnlyPaths=/usr/local/libexec/agent-steward/codex", unit
        )

    def test_declared_systemd_credential_never_falls_back_to_interactive_home(self):
        credential_dir = self.root / "insecure-systemd-credentials"
        credential_dir.mkdir(mode=0o700)
        insecure_auth = credential_dir / "codex-auth"
        insecure_auth.write_text("insecure", encoding="utf-8")
        insecure_auth.chmod(0o644)
        interactive_auth = self.root / ".codex" / "auth.json"
        interactive_auth.parent.mkdir()
        interactive_auth.write_text("interactive-auth", encoding="utf-8")
        codex = self.root / "codex"
        codex.write_text(
            """#!/usr/bin/env python3
from pathlib import Path
import sys
args = sys.argv[1:]
if args == ["--version"]:
    print("1.0.0")
elif args[:2] == ["exec", "--help"]:
    print("--json --ephemeral --ignore-user-config --skip-git-repo-check --sandbox --model --cd --output-last-message")
elif args and args[0] == "exec":
    Path(args[args.index("--output-last-message") + 1]).write_text('{"verdict":"PASS","blockers":[],"summary":"bounded"}', encoding="utf-8")
else:
    raise SystemExit(2)
""",
            encoding="utf-8",
        )
        codex.chmod(0o755)
        worker = workers.CodexWorkCardWorker(timeout_seconds=5)
        with mock.patch.dict(
            os.environ,
            {"CREDENTIALS_DIRECTORY": str(credential_dir)},
            clear=False,
        ):
            exit_code, _response_path, failure_reason = worker._invoke(
                "review",
                "bounded review",
                self.root,
                environment={
                    "HOME": str(self.root),
                    "PATH": f"{self.root}:/usr/bin:/bin",
                },
            )
        self.assertNotEqual(exit_code, 0)
        self.assertEqual(failure_reason, "authentication_failure")

    def test_production_worker_enforces_workcard_contract_and_allowlisted_checks(self):
        context = type(
            "Contract",
            (),
            {
                "steps": ("implement the bounded change",),
                "focused_tests": ("focused_checks_required", "git diff --check"),
                "negative_checks": ("do not widen scope",),
                "expected_evidence": ("implementation head",),
            },
        )()
        workers.CodexWorkCardWorker._validate_workcard_contract(context)

        context.focused_tests = ("unallowlisted command",)
        with self.assertRaises(workers.WorkerError) as raised:
            workers.CodexWorkCardWorker._validate_workcard_contract(context)
        self.assertEqual(str(raised.exception), "codex_focused_check_not_allowlisted")

    def test_wrapper_retains_codex_last_message(self):
        root = self.root / "wrapper-last-message"
        bin_dir = root / "bin"
        output_dir = root / "output"
        workspace = root / "workspace"
        bin_dir.mkdir(parents=True)
        output_dir.mkdir()
        workspace.mkdir()
        prompt = root / "prompt.txt"
        prompt.write_text("bounded review", encoding="utf-8")
        final = '{"verdict":"PASS","blockers":[],"summary":"bounded"}'
        fake = bin_dir / "codex"
        fake.write_text(
            """#!/usr/bin/env python3
import json
from pathlib import Path
import sys
args = sys.argv[1:]
if args == ["--version"]:
    print("1.18.25")
elif args[:2] == ["exec", "--help"]:
    print("--json --ephemeral --ignore-user-config --skip-git-repo-check --sandbox --model --cd --output-last-message")
elif args and args[0] == "exec":
    if "--approve-for-me" in args:
        if "--sandbox" in args:
            raise SystemExit(8)
    elif "--sandbox" not in args or args[args.index("--sandbox") + 1] != "read-only":
        raise SystemExit(3)
    if "--model" in args and args[args.index("--model") + 1] != "gpt-5.3-codex":
        raise SystemExit(4)
    output = Path(args[args.index("--output-last-message") + 1])
    if output.parent.name == "output-default" and "--model" in args:
        raise SystemExit(9)
    output.write_text(%r, encoding="utf-8")
    print(json.dumps({"type":"turn.completed"}))
else:
    raise SystemExit(2)
""" % final,
            encoding="utf-8",
        )
        fake.chmod(0o755)
        environment = dict(os.environ)
        environment.update(
            HOME=str(root),
            PATH=f"{bin_dir}:{environment.get('PATH', '/usr/bin:/bin')}",
            AGENT_CODEX_TIMEOUT_SECONDS="30",
            AGENT_CODEX_MODEL_TIER="T2",
            AGENT_CODEX_MODEL="gpt-5.3-codex",
            CODEX_HOME=str(root / "isolated-codex-home"),
            CODEX_BIN=str(fake),
        )
        result = subprocess.run(
            [
                str(ROOT / "scripts" / "agent-control" / "codex_wrapper.sh"),
                "review",
                str(prompt),
                str(output_dir),
                str(workspace),
            ],
            capture_output=True,
            text=True,
            timeout=60,
            env=environment,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            (output_dir / "codex-last-message.txt").read_text(encoding="utf-8"),
            final,
        )
        environment.pop("AGENT_CODEX_MODEL")
        default_output = root / "output-default"
        default_output.mkdir()
        default_result = subprocess.run(
            [
                str(ROOT / "scripts" / "agent-control" / "codex_wrapper.sh"),
                "review",
                str(prompt),
                str(default_output),
                str(workspace),
            ],
            capture_output=True,
            text=True,
            timeout=60,
            env=environment,
            check=False,
        )
        self.assertEqual(default_result.returncode, 0, default_result.stderr)
        self.assertEqual(
            (default_output / "codex-last-message.txt").read_text(encoding="utf-8"),
            final,
        )
        implementation_output = root / "output-implementation"
        implementation_output.mkdir()
        implementation_result = subprocess.run(
            [
                str(ROOT / "scripts" / "agent-control" / "codex_wrapper.sh"),
                "implement",
                str(prompt),
                str(implementation_output),
                str(workspace),
            ],
            capture_output=True,
            text=True,
            timeout=60,
            env=environment,
            check=False,
        )
        self.assertEqual(
            implementation_result.returncode, 0, implementation_result.stderr
        )


class StewardConcurrencyTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        registered = contract.campaign_mission()
        self.mission = contract.activate_current_mission(
            repository=registered.repository_identity.repository,
            base_sha=registered.repository_identity.base_sha,
            branch=registered.repository_identity.branch,
            source_ref=registered.repository_identity.source_ref,
            source_sha256=registered.repository_identity.source_sha256,
            proposal_sha256=registered.proposal_sha256,
            owner_approval=contract.OwnerApproval(
                "github:Igzela", registered.proposal_sha256, "fixture-concurrency-approval", "2026-08-30T00:00:00Z"
            ),
            owner_authenticator=type("Authenticator", (), {"verify": lambda *_args: True})(),
        )

    def card(self, card_id: str, path: str, locks: tuple[str, ...], dependency=()):
        return contract.WorkCard(
            card_id,
            "stage-2",
            (path,),
            (),
            ("bounded",),
            ("focused",),
            ("negative",),
            ("receipt",),
            dependency,
            locks,
            1,
            "T1",
            self.mission.rollback,
            "PENDING",
        )

    def stage(self, cards):
        return contract.Stage(
            "stage-2",
            self.mission.mission_id,
            "bounded concurrent stage",
            self.mission.repository_identity,
            ("focused",),
            ("no effect",),
            tuple(card.card_id for card in cards),
            self.mission.rollback,
            None,
            None,
        )

    def test_disjoint_cards_are_capped_at_k_two(self):
        cards = (
            self.card("card-a", "docs/ARCHITECTURE_BOOK.md", ("docs/ARCHITECTURE_BOOK.md",)),
            self.card("card-b", "tests/test_mission_contract.py", ("tests/test_mission_contract.py",)),
        )
        stage = self.stage(cards)
        active = 0
        maximum = 0
        guard = threading.Lock()

        def fake_dispatch(_mission, _stage, card, **_kwargs):
            nonlocal active, maximum
            with guard:
                active += 1
                maximum = max(maximum, active)
            time.sleep(0.08)
            with guard:
                active -= 1
            return steward.ExecutionResult(card.card_id, "COMPLETE", 1, HEAD, "done")

        instance = steward.Steward(
            repository=contract.CAMPAIGN_REPOSITORY,
            repo_path=self.root,
            journal=StewardJournal(self.root / "journal.sqlite3"),
            github=steward_github.FakeGitHubReader(),
            lock_dir=self.root / "locks",
        )
        with mock.patch.object(instance, "dispatch_card", side_effect=fake_dispatch):
            results = instance.dispatch_cards(self.mission, stage, cards, base_sha=BASE)
        self.assertEqual(set(results), {"card-a", "card-b"})
        self.assertEqual(maximum, 2)

    def test_overlapping_cards_are_serialized(self):
        shared = ("docs/ARCHITECTURE_BOOK.md",)
        cards = (
            self.card("card-a", shared[0], shared),
            self.card("card-b", shared[0], shared),
        )
        stage = self.stage(cards)
        active = 0
        maximum = 0
        guard = threading.Lock()

        def fake_dispatch(_mission, _stage, card, **_kwargs):
            nonlocal active, maximum
            with guard:
                active += 1
                maximum = max(maximum, active)
            time.sleep(0.05)
            with guard:
                active -= 1
            return steward.ExecutionResult(card.card_id, "COMPLETE", 1, HEAD, "done")

        instance = steward.Steward(
            repository=contract.CAMPAIGN_REPOSITORY,
            repo_path=self.root,
            journal=StewardJournal(self.root / "journal.sqlite3"),
            github=steward_github.FakeGitHubReader(),
            lock_dir=self.root / "locks",
        )
        with mock.patch.object(instance, "dispatch_card", side_effect=fake_dispatch):
            results = instance.dispatch_cards(self.mission, stage, cards, base_sha=BASE)
        self.assertEqual(set(results), {"card-a", "card-b"})
        self.assertEqual(maximum, 1)
