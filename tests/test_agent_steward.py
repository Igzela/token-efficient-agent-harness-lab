"""Provider-free Steward execution and bounded concurrency tests."""

from __future__ import annotations

from pathlib import Path
from dataclasses import replace
import hashlib
import json
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
        self.mission = contract.campaign_mission()
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

    def review(self, *, head=HEAD, reviewer="review-session", implementation="impl-session"):
        receipt_payload = {
            "schema_version": "steward_review_outcome.v1",
            "status": "PASS",
            "reviewer_session_id": reviewer,
            "implementation_session_id": implementation,
            "reviewed_head_sha": head,
            "blockers": [],
            "reviewed_base_sha": BASE,
            "reviewed_range_sha256": workers.review_range_digest(BASE, head),
            "review_axes": ["standards", "spec"],
            "review_round": 1,
            "review_mode": "full",
            "review_receipt_sha256": "",
        }
        return workers.ReviewOutcome(
            "PASS",
            reviewer,
            implementation,
            head,
            (),
            "",
            BASE,
            workers.review_range_digest(BASE, head),
            ("standards", "spec"),
            1,
            "full",
            hashlib.sha256(
                json.dumps(
                    receipt_payload, sort_keys=True, separators=(",", ":")
                ).encode("utf-8")
            ).hexdigest(),
        )

    def process_worker(self, callback):
        def command(context):
            outcome = callback(context)
            payload = json.dumps(outcome.to_wire())
            return ["/usr/bin/python3", "-c", f"print({payload!r})"]

        return workers.BoundedProcessWorker(command, timeout_seconds=5)

    def process_reviewer(self, callback):
        def command(context, outcome):
            review = callback(context, outcome)
            payload = json.dumps(review.to_wire())
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
        return steward.Steward(
            repository=contract.CAMPAIGN_REPOSITORY,
            repo_path=self.root,
            journal=StewardJournal(self.root / "journal.sqlite3"),
            github=steward_github.FakeGitHubReader(dict(self.facts.__dict__)),
            worker=worker,
            reviewer=reviewer,
            verifier=verifier
            or (lambda _worktree, _paths: [{"command": "git diff --check", "exit_code": 0}]),
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
        with mock.patch.object(
            worktree_manager,
            "create_steward_worktree",
            return_value=(
                str(self.mock_worktree_path), self.mock_worktree_branch, BASE, None
            ),
        ), head_patch, diff_patch, clean_patch, identity_patch, range_patch:
            return service.dispatch_card(
                self.mission,
                self.stage,
                self.card,
                base_sha=BASE,
                stage_pr=self.facts,
            ), service

    def test_approved_card_reaches_waiting_for_merge_without_merge(self):
        implementation = workers.WorkerOutcome("PASS", "impl-session", HEAD, (self.card.allowed_paths[0],))
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
        ):
            result = instance.dispatch_card(
                self.mission, self.stage, self.card, base_sha=BASE, stage_pr=self.facts
            )
        self.assertEqual(result.status, "BLOCKED")
        self.assertEqual(result.reason, "worker_path_outside_card")

    def test_default_worker_is_explicitly_provider_free_and_unconfigured(self):
        instance = steward.Steward(
            repository=contract.CAMPAIGN_REPOSITORY,
            repo_path=self.root,
            journal=StewardJournal(self.root / "journal.sqlite3"),
            github=steward_github.FakeGitHubReader(),
            reviewer=None,
            lock_dir=self.root / "locks",
        )
        with mock.patch.object(
            worktree_manager,
            "create_steward_worktree",
            return_value=(
                str(self.mock_worktree_path), self.mock_worktree_branch, BASE, None
            ),
        ), mock.patch.object(steward, "_git_repository_identity", return_value=True):
            result = instance.dispatch_card(self.mission, self.stage, self.card, base_sha=BASE)
        self.assertEqual(result.status, "BLOCKED")
        self.assertEqual(result.reason, "provider_free_worker_not_configured")


class StewardConcurrencyTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.mission = contract.campaign_mission()

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
