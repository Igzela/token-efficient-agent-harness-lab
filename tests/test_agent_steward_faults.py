"""Failure, restart, security, and GitHub-boundary tests for Steward."""

from __future__ import annotations

from pathlib import Path
import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts" / "agent-control"))

import mission_contract as contract  # noqa: E402
import steward_github  # noqa: E402
from steward_journal import StewardJournal  # noqa: E402
from steward_service import StewardService, main as service_main  # noqa: E402
import steward_workers as workers  # noqa: E402
import worktree_manager  # noqa: E402


MISSION = contract.CAMPAIGN_MISSION_ID
BASE = contract.CAMPAIGN_BASE_SHA
HEAD = "b" * 40


class StewardFaultTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.journal = StewardJournal(Path(self.temp.name) / "steward.sqlite3")

    def append(self, state: str, key: str, *, attempt: int = 1, card: str = "card-1"):
        return self.journal.append(
            event=f"EVENT_{state}",
            idempotency_key=key,
            mission_id=MISSION,
            stage_id="stage-1",
            card_id=card,
            attempt=attempt,
            state=state,
            detail=state.lower(),
        )

    def make_waiting_journal(self, card: str = "card-1"):
        self.append("QUEUED", f"queue:{card}", card=card)
        self.append("RUNNING", f"run:{card}", card=card)
        self.append("VERIFYING", f"verify:{card}", card=card)
        self.append("REVIEWING", f"review:{card}", card=card)
        receipt_payload = {
            "schema_version": "steward_review_outcome.v1",
            "status": "PASS",
            "reviewer_session_id": "reviewer-session",
            "implementation_session_id": "implementation-session",
            "reviewed_head_sha": HEAD,
            "blockers": [],
            "reviewed_base_sha": BASE,
            "reviewed_range_sha256": workers.review_range_digest(BASE, HEAD),
            "review_axes": ["standards", "spec"],
            "review_round": 1,
            "review_mode": "full",
            "review_receipt_sha256": "",
        }
        receipt_sha = hashlib.sha256(
            json.dumps(receipt_payload, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest()
        self.journal.append(
            event="REVIEW_PASSED",
            idempotency_key=f"review-pass:{card}",
            mission_id=MISSION,
            stage_id="stage-1",
            card_id=card,
            attempt=1,
            state="REVIEWING",
            detail="independent_review_passed",
            data={
                "implementation_session_id": "implementation-session",
                "reviewer_session_id": "reviewer-session",
                "base_sha": BASE,
                "head_sha": HEAD,
                "reviewed_range_sha256": workers.review_range_digest(BASE, HEAD),
                "review_axes": ["standards", "spec"],
                "review_round": 1,
                "review_mode": "full",
                "review_receipt_sha256": receipt_sha,
                "verdict": "PASS",
            },
        )
        self.journal.append(
            event="STAGE_PR_BOUND",
            idempotency_key=f"stage-bind:{card}",
            mission_id=MISSION,
            stage_id="stage-1",
            card_id=card,
            attempt=1,
            state="REVIEWING",
            detail="stage_pr_binding_observed",
            data={
                "repository": "Igzela/token-efficient-agent-harness-lab",
                "pr_number": 7,
                "base_sha": BASE,
                "head_sha": HEAD,
                "base_branch": "main",
                "head_branch": "agent/steward-card",
            },
        )

    def facts(self, *, merged: bool = False, head: str = HEAD):
        return {
            "repository": "Igzela/token-efficient-agent-harness-lab",
            "pr_number": 7,
            "state": "OPEN" if not merged else "CLOSED",
            "draft": False,
            "merged": merged,
            "base_sha": BASE,
            "head_sha": head,
            "base_branch": "main",
            "head_branch": "agent/steward-card",
            "ci_state": "PASS",
            "review_state": "PASS",
        }

    def test_restart_rebuilds_projection_and_does_not_replay_inflight_card(self):
        self.append("QUEUED", "queue:1")
        self.append("RUNNING", "run:1")
        service = StewardService(
            mission_id=MISSION,
            journal=self.journal,
            github=steward_github.FakeGitHubReader(),
        )
        report = service.recover()
        self.assertEqual(report.items[0].outcome, "BLOCKED")
        self.assertEqual(report.items[0].reason, "worker_binding_missing_or_invalid")
        self.assertEqual(report.journal_projection["card_states"]["card-1"], "RUNNING")

    def test_service_rejects_unregistered_mission_identity(self):
        with self.assertRaisesRegex(ValueError, "mission_id_not_registered"):
            StewardService(
                mission_id="UNREGISTERED-MISSION",
                journal=self.journal,
                github=steward_github.FakeGitHubReader(),
            )

    def test_reconciliation_promotes_reviewing_card_from_live_read_only_facts(self):
        self.make_waiting_journal()
        reader = steward_github.FakeGitHubReader(self.facts())
        service = StewardService(mission_id=MISSION, journal=self.journal, github=reader)
        report = service.reconcile(
            stage_bindings={}
        )
        self.assertEqual(report.items[0].outcome, "WAITING_FOR_MERGE")
        self.assertEqual(self.journal.projection()["card_states"]["card-1"], "WAITING_FOR_MERGE")
        self.assertEqual(reader.reads, [("Igzela/token-efficient-agent-harness-lab", 7)])

    def test_reconciliation_ignores_caller_binding_projection(self):
        self.make_waiting_journal()
        reader = steward_github.FakeGitHubReader(self.facts())
        service = StewardService(mission_id=MISSION, journal=self.journal, github=reader)
        report = service.reconcile(
            stage_bindings={
                "card-1": {
                    "repository": "attacker/redirected",
                    "pr_number": 999,
                    "base_sha": "d" * 40,
                    "head_sha": "e" * 40,
                    "base_branch": "main",
                    "head_branch": "agent/redirected",
                }
            }
        )
        self.assertEqual(report.items[0].outcome, "WAITING_FOR_MERGE")
        self.assertEqual(reader.reads, [("Igzela/token-efficient-agent-harness-lab", 7)])

    def test_reconciliation_ignores_other_mission_cards(self):
        self.append("QUEUED", "queue:other", card="other")
        self.journal.append(
            event="CARD_QUEUED",
            idempotency_key="queue:other-mission",
            mission_id="OTHER-MISSION",
            stage_id="stage-other",
            card_id="other-mission-card",
            attempt=1,
            state="QUEUED",
            detail="queued",
        )
        service = StewardService(
            mission_id=MISSION,
            journal=self.journal,
            github=steward_github.FakeGitHubReader(),
        )
        report = service.recover()
        self.assertEqual([item.card_id for item in report.items], ["other"])

    def test_journal_transition_isolated_by_stage(self):
        for state, key in (
            ("QUEUED", "queue:stage-one"),
            ("RUNNING", "run:stage-one"),
            ("VERIFYING", "verify:stage-one"),
            ("REVIEWING", "review:stage-one"),
            ("WAITING_FOR_MERGE", "waiting:stage-one"),
            ("COMPLETE", "complete:stage-one"),
        ):
            self.append(state, key, card="same-card")
        self.journal.append(
            event="STAGE_TWO_QUEUE",
            idempotency_key="queue:stage-two",
            mission_id=MISSION,
            stage_id="stage-2",
            card_id="same-card",
            attempt=1,
            state="QUEUED",
            detail="queued",
        )
        self.journal.append(
            event="STAGE_TWO_RUN",
            idempotency_key="run:stage-two",
            mission_id=MISSION,
            stage_id="stage-2",
            card_id="same-card",
            attempt=1,
            state="RUNNING",
            detail="running",
        )
        projection = self.journal.projection(mission_id=MISSION)
        self.assertEqual(
            projection["card_states"],
            {
                f"{MISSION}:stage-1:same-card": "COMPLETE",
                f"{MISSION}:stage-2:same-card": "RUNNING",
            },
        )

    def test_reconciliation_requires_review_binding_for_exact_head(self):
        self.append("QUEUED", "queue:unreviewed", card="unreviewed")
        self.append("RUNNING", "run:unreviewed", card="unreviewed")
        self.append("VERIFYING", "verify:unreviewed", card="unreviewed")
        self.append("REVIEWING", "review:unreviewed", card="unreviewed")
        self.journal.append(
            event="STAGE_PR_BOUND",
            idempotency_key="stage-bind:unreviewed",
            mission_id=MISSION,
            stage_id="stage-1",
            card_id="unreviewed",
            attempt=1,
            state="REVIEWING",
            detail="stage_pr_binding_observed",
            data={
                "repository": "Igzela/token-efficient-agent-harness-lab",
                "pr_number": 7,
                "base_sha": BASE,
                "head_sha": HEAD,
                "base_branch": "main",
                "head_branch": "agent/steward-card",
            },
        )
        service = StewardService(
            mission_id=MISSION,
            journal=self.journal,
            github=steward_github.FakeGitHubReader(self.facts()),
        )
        report = service.reconcile(
            stage_bindings={
                "card-1": {
                    "repository": "Igzela/token-efficient-agent-harness-lab",
                    "pr_number": 7,
                "base_sha": BASE,
                "head_sha": HEAD,
                "base_branch": "main",
                "head_branch": "agent/steward-card",
            },
                "unreviewed": {
                    "repository": "Igzela/token-efficient-agent-harness-lab",
                    "pr_number": 7,
                    "base_sha": BASE,
                    "head_sha": HEAD,
                    "base_branch": "main",
                    "head_branch": "agent/steward-card",
                },
            }
        )
        item = next(item for item in report.items if item.card_id == "unreviewed")
        self.assertEqual(item.outcome, "BLOCKED")
        self.assertEqual(item.reason, "review_binding_missing")
        self.assertEqual(self.journal.projection()["card_states"]["unreviewed"], "BLOCKED")

    def test_reconciliation_records_observed_merge_but_never_requests_one(self):
        self.make_waiting_journal()
        self.journal.append(
            event="STAGE_WAITING_FOR_MERGE",
            idempotency_key="waiting:1",
            mission_id=MISSION,
            stage_id="stage-1",
            card_id="card-1",
            attempt=1,
            state="WAITING_FOR_MERGE",
            detail="gates_passed",
        )
        reader = steward_github.FakeGitHubReader(self.facts(merged=True))
        service = StewardService(mission_id=MISSION, journal=self.journal, github=reader)
        report = service.reconcile(
            stage_bindings={
                "card-1": {
                    "repository": "Igzela/token-efficient-agent-harness-lab",
                    "pr_number": 7,
                "base_sha": BASE,
                "head_sha": HEAD,
                "base_branch": "main",
                "head_branch": "agent/steward-card",
            }
            }
        )
        self.assertEqual(report.items[0].outcome, "COMPLETE")
        self.assertEqual(self.journal.projection()["card_states"]["card-1"], "COMPLETE")

    def test_github_read_failure_blocks_without_guessing_external_state(self):
        self.make_waiting_journal()
        reader = steward_github.FakeGitHubReader(self.facts())
        reader.fail = True
        service = StewardService(mission_id=MISSION, journal=self.journal, github=reader)
        report = service.reconcile(
            stage_bindings={
                "card-1": {
                    "repository": "Igzela/token-efficient-agent-harness-lab",
                    "pr_number": 7,
                "base_sha": BASE,
                "head_sha": HEAD,
                "base_branch": "main",
                "head_branch": "agent/steward-card",
            }
            }
        )
        self.assertEqual(report.items[0].outcome, "WAITING")
        self.assertEqual(
            self.journal.projection()["card_states"]["card-1"], "REVIEWING"
        )

    def test_github_binding_requires_exact_head_and_base(self):
        observed = steward_github.StagePRFacts.from_wire(self.facts())
        with self.assertRaisesRegex(steward_github.GitHubFactsError, "head_or_base"):
            steward_github.reconcile_stage_pr(
                observed,
                repository=observed.repository,
                pr_number=observed.pr_number,
                expected_base_sha="c" * 40,
                expected_head_sha=HEAD,
            )
        status = steward_github.reconcile_stage_pr(
            observed,
            repository=observed.repository,
            pr_number=observed.pr_number,
            expected_base_sha=BASE,
            expected_head_sha=HEAD,
        )
        self.assertTrue(status.waiting_for_merge)

    def test_pending_and_draft_gates_do_not_claim_waiting_for_merge(self):
        for change, reason in (
            ({"ci_state": "PENDING"}, "ci_pending"),
            ({"review_state": "FAIL"}, "review_fail"),
            ({"draft": True}, "pr_is_draft"),
        ):
            with self.subTest(change=change):
                payload = self.facts()
                payload.update(change)
                status = steward_github.reconcile_stage_pr(
                    payload,
                    repository=payload["repository"],
                    pr_number=7,
                    expected_base_sha=BASE,
                    expected_head_sha=HEAD,
                )
                self.assertFalse(status.waiting_for_merge)
                self.assertEqual(status.reason, reason)

    def test_github_reader_rejects_malformed_flags_and_mixed_check_states(self):
        malformed = self.facts()
        malformed["isDraft"] = "false"
        reader = steward_github.GhReadOnlyGitHub()
        with mock.patch("steward_github.subprocess.run") as run:
            run.return_value.returncode = 0
            run.return_value.stdout = json.dumps(
                {
                    "state": "OPEN",
                    "isDraft": "false",
                    "mergedAt": None,
                    "baseRefName": "main",
                    "headRefName": "agent/steward-card",
                    "baseRefOid": BASE,
                    "headRefOid": HEAD,
                    "statusCheckRollup": [],
                    "reviewDecision": "APPROVED",
                }
            )
            run.return_value.stderr = ""
            with self.assertRaises(steward_github.GitHubReadError):
                reader.fetch_stage_pr("Igzela/token-efficient-agent-harness-lab", 7)

            run.return_value.stdout = json.dumps(
                {
                    "state": "OPEN",
                    "isDraft": False,
                    "mergedAt": None,
                    "baseRefName": "main",
                    "headRefName": "agent/steward-card",
                    "baseRefOid": BASE,
                    "headRefOid": HEAD,
                    "statusCheckRollup": [
                        {"conclusion": "SUCCESS", "status": "COMPLETED"},
                        {"conclusion": None, "status": "IN_PROGRESS"},
                    ],
                    "reviewDecision": "APPROVED",
                }
            )
            observed = reader.fetch_stage_pr("Igzela/token-efficient-agent-harness-lab", 7)
            self.assertEqual(observed["ci_state"], "PENDING")

            run.return_value.stdout = json.dumps(
                {
                    "state": "OPEN",
                    "isDraft": False,
                    "mergedAt": None,
                    "baseRefName": "main",
                    "headRefName": "agent/steward-card",
                    "baseRefOid": BASE,
                    "headRefOid": HEAD,
                    "statusCheckRollup": [
                        {"name": name, "conclusion": "SUCCESS", "status": "COMPLETED"}
                        for name in (
                            "docker-build",
                            "native-runtime",
                            "pg-integration-tests",
                            "python-tests",
                            "rust-tests",
                            "rust-typescript-cutover",
                            "typescript-tests",
                            "context-capsule",
                        )
                    ],
                    "reviewDecision": "APPROVED",
                }
            )
            observed = reader.fetch_stage_pr("Igzela/token-efficient-agent-harness-lab", 7)
            self.assertEqual(observed["ci_state"], "PASS")

    def test_child_environment_drops_github_and_provider_credentials(self):
        environment = workers.child_environment(
            {
                "PATH": "/usr/bin",
                "HOME": "/tmp/safe",
                "GITHUB_TOKEN": "redacted",
                "OPENAI_API_KEY": "redacted",
                "PROVIDER_SECRET": "redacted",
            }
        )
        self.assertNotIn("GITHUB_TOKEN", environment)
        self.assertNotIn("OPENAI_API_KEY", environment)
        self.assertNotIn("PROVIDER_SECRET", environment)
        self.assertEqual(environment["PATH"], "/usr/bin:/bin")
        self.assertEqual(environment["HOME"], "/nonexistent")
        for key in ("CODEX_HOME", "HTTPS_PROXY", "HTTP_PROXY", "ALL_PROXY"):
            self.assertNotIn(key, environment)

    def test_bounded_command_policy_rejects_network_and_git_effects(self):
        with self.assertRaisesRegex(workers.WorkerError, "executable_not_allowlisted"):
            workers._validate_command(["gh", "pr", "merge", "7"])
        with self.assertRaisesRegex(workers.WorkerError, "git_effect_forbidden"):
            workers._validate_command(["git", "push", "origin", "HEAD"])

    def test_bounded_process_worker_produces_untrusted_wire_outcome(self):
        context = workers.WorkerContext(
            mission_id=MISSION,
            stage_id="stage-1",
            card_id="card-1",
            attempt=1,
            model_tier="T1",
            base_sha=BASE,
            worktree=self.root,
            allowed_paths=("docs/ARCHITECTURE_BOOK.md",),
            steps=("bounded",),
            focused_tests=("focused",),
            negative_checks=("negative",),
            expected_evidence=("receipt",),
            environment=workers.child_environment({"PATH": "/usr/bin"}),
        )
        payload = json.dumps(
            {
                "schema_version": "steward_worker_outcome.v1",
                "status": "PASS",
                "session_id": "steward-process:card-1:1",
                "head_sha": BASE,
                "changed_paths": [],
                "detail": "no_change",
            }
        )
        worker = workers.BoundedProcessWorker(
            lambda _context: ["/usr/bin/python3", "-c", f"print({payload!r})"],
            timeout_seconds=5,
        )
        result = worker.run(context)
        self.assertEqual(result.status, "PASS")
        self.assertEqual(result.session_id, "steward-process:card-1:1")

    def test_bounded_process_worker_timeout_is_not_success(self):
        context = workers.WorkerContext(
            mission_id=MISSION,
            stage_id="stage-1",
            card_id="card-1",
            attempt=1,
            model_tier="T1",
            base_sha=BASE,
            worktree=self.root,
            allowed_paths=("docs/ARCHITECTURE_BOOK.md",),
            steps=("bounded",),
            focused_tests=("focused",),
            negative_checks=("negative",),
            expected_evidence=("receipt",),
            environment=workers.child_environment({"PATH": "/usr/bin"}),
        )
        worker = workers.BoundedProcessWorker(
            lambda _context: ["/usr/bin/python3", "-c", "import time; time.sleep(2)"],
            timeout_seconds=1,
        )
        result = worker.run(context)
        self.assertEqual(result.status, "TIMEOUT")

    def test_path_lock_and_worktree_names_are_digest_bound(self):
        digest = hashlib.sha256(b"card:untrusted/../value").hexdigest()[:24]
        self.assertTrue(
            worktree_manager._is_steward_path(
                worktree_manager.WORKTREE_BASE / f"steward-{digest}"
            )
        )
        self.assertFalse(
            worktree_manager._is_steward_path(
                worktree_manager.WORKTREE_BASE / "steward-../escape"
            )
        )
        self.assertTrue(
            set(workers.lock_footprint(("scripts/",)))
            & set(workers.lock_footprint(("scripts/agent-control/steward.py",)))
        )
        with self.assertRaises(workers.PathConflict):
            workers.PathLockSet(Path(self.temp.name) / "locks", ("../outside",))

    def test_real_steward_worktree_is_exact_base_and_dirty_cleanup_is_retained(self):
        remote = self.root / "remote.git"
        repo = self.root / "repo"
        subprocess.run(["git", "init", "--bare", str(remote)], check=True, capture_output=True)
        subprocess.run(["git", "init", "-b", "main", str(repo)], check=True, capture_output=True)
        subprocess.run(["git", "config", "user.name", "Steward Test"], cwd=repo, check=True)
        subprocess.run(["git", "config", "user.email", "steward-test@example.invalid"], cwd=repo, check=True)
        (repo / "README.md").write_text("steward test\n", encoding="utf-8")
        subprocess.run(["git", "add", "README.md"], cwd=repo, check=True, capture_output=True)
        subprocess.run(["git", "commit", "-m", "initial"], cwd=repo, check=True, capture_output=True)
        subprocess.run(["git", "remote", "add", "origin", str(remote)], cwd=repo, check=True)
        subprocess.run(["git", "push", "-u", "origin", "main"], cwd=repo, check=True, capture_output=True)
        expected = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=repo, check=True, capture_output=True, text=True
        ).stdout.strip()

        old_base = worktree_manager.WORKTREE_BASE
        worktree_manager.WORKTREE_BASE = self.root / "worktrees"
        self.addCleanup(setattr, worktree_manager, "WORKTREE_BASE", old_base)
        created = worktree_manager.create_steward_worktree("card-real", str(repo), expected)
        self.assertIsNotNone(created)
        path, branch, base, _remote_sha = created
        self.assertEqual(base, expected)
        self.assertTrue(worktree_manager.verify_worktree(path, branch, str(repo), expected))

        (Path(path) / "README.md").write_text("dirty evidence\n", encoding="utf-8")
        self.assertFalse(worktree_manager.remove_steward_worktree("card-real", str(repo)))
        self.assertTrue(Path(path).exists())
        subprocess.run(["git", "checkout", "--", "README.md"], cwd=path, check=True, capture_output=True)
        self.assertTrue(worktree_manager.remove_steward_worktree("card-real", str(repo)))

    def test_heartbeat_service_cli_is_once_only_and_persists_no_raw_content(self):
        journal_path = Path(self.temp.name) / "heartbeat.sqlite3"
        self.assertEqual(
            service_main(
                ["--heartbeat-loop", "--once", "--journal", str(journal_path)]
            ),
            0,
        )
        projection = StewardJournal(journal_path).projection()
        self.assertEqual(projection["event_count"], 1)
        self.assertEqual(projection["card_states"], {})

    def test_worker_outcome_rejects_private_and_out_of_scope_paths(self):
        card = contract.WorkCard(
            "card-1",
            "stage-1",
            ("docs/ARCHITECTURE_BOOK.md",),
            (),
            ("bounded",),
            ("focused",),
            ("negative",),
            ("receipt",),
            (),
            ("docs/ARCHITECTURE_BOOK.md",),
            1,
            "T1",
            contract.campaign_mission().rollback,
            "PENDING",
        )
        outcome = workers.WorkerOutcome("PASS", "impl", HEAD, (".codex/secret",))
        with self.assertRaisesRegex(workers.WorkerError, "outside_card"):
            workers.validate_worker_outcome(card, outcome, expected_head_sha=HEAD)


if __name__ == "__main__":
    unittest.main()
