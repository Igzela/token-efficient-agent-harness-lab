"""Failure, restart, security, and GitHub-boundary tests for Steward."""

from __future__ import annotations

from pathlib import Path
from dataclasses import replace
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
import steward_service  # noqa: E402
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
            "detail": "",
            "reviewed_base_sha": BASE,
            "reviewed_range_sha256": workers.review_range_digest(BASE, HEAD),
            "review_axes": ["standards", "spec"],
            "review_round": 1,
            "review_mode": "full",
            "summary": "bounded independent review",
            "findings": None,
            "security_ok": True,
            "rollback_ok": True,
            "observed_ci_status": "unknown",
            "finding_ledger_digest": "",
            "review_receipt_sha256": "",
        }
        review = workers.ReviewOutcome.from_wire(
            workers.seal_review_outcome_wire(receipt_payload)
        )
        review_wire = review.to_wire()
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
                "review_receipt_sha256": review_wire["review_receipt_sha256"],
                "verdict": "PASS",
                "finding_ledger_digest": review_wire["finding_ledger_digest"],
                "open_blocker_ids": [],
                "deferred_note_ids": [],
                "security_ok": True,
                "rollback_ok": True,
                "observed_ci_status": "unknown",
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

    def test_review_binding_revalidates_pass_with_deferred_findings_after_restart(self):
        self.append("QUEUED", "queue:deferred")
        self.append("RUNNING", "run:deferred")
        self.append("VERIFYING", "verify:deferred")
        self.append("REVIEWING", "review:deferred")
        finding = {
            "id": "note-1",
            "axis": "standards",
            "evidence": "bounded residual note",
            "severity": "minor",
            "disposition": "defer",
            "scope_relation": "in_packet",
            "origin_head": HEAD,
            "acceptance_condition": "retain for later packet",
            "status": "deferred",
        }
        review_payload = {
            "schema_version": "steward_review_outcome.v1",
            "status": "PASS",
            "reviewer_session_id": "reviewer-session",
            "implementation_session_id": "implementation-session",
            "reviewed_head_sha": HEAD,
            "blockers": [],
            "detail": "",
            "reviewed_base_sha": BASE,
            "reviewed_range_sha256": workers.review_range_digest(BASE, HEAD),
            "review_axes": ["standards", "spec"],
            "review_round": 1,
            "review_mode": "full",
            "summary": "bounded independent review",
            "findings": [finding],
            "security_ok": True,
            "rollback_ok": True,
            "observed_ci_status": "unknown",
            "finding_ledger_digest": "",
            "review_receipt_sha256": "",
        }
        review = workers.ReviewOutcome.from_wire(
            workers.seal_review_outcome_wire(review_payload)
        )
        review_wire = review.to_wire()
        event = self.journal.append(
            event="REVIEW_PASSED",
            idempotency_key="review-pass:deferred",
            mission_id=MISSION,
            stage_id="stage-1",
            card_id="card-1",
            attempt=1,
            state="REVIEWING",
            detail="independent_review_passed",
            data={
                "implementation_session_id": review.implementation_session_id,
                "reviewer_session_id": review.reviewer_session_id,
                "base_sha": BASE,
                "head_sha": HEAD,
                "reviewed_range_sha256": review.reviewed_range_sha256,
                "review_axes": list(review.review_axes),
                "review_round": review.review_round,
                "review_mode": review.review_mode,
                "review_receipt_sha256": review_wire["review_receipt_sha256"],
                "verdict": "PASS",
                "finding_ledger_digest": review_wire["finding_ledger_digest"],
                "open_blocker_ids": [],
                "deferred_note_ids": ["note-1"],
                "security_ok": True,
                "rollback_ok": True,
                "observed_ci_status": "unknown",
            },
        )
        self.assertEqual(event.data["deferred_note_ids"], ["note-1"])
        self.assertEqual(event.data["review_receipt_sha256"], review_wire["review_receipt_sha256"])

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

    def test_reconciliation_observes_merge_from_reviewing_after_restart(self):
        self.make_waiting_journal()
        reader = steward_github.FakeGitHubReader(self.facts(merged=True))
        service = StewardService(mission_id=MISSION, journal=self.journal, github=reader)
        report = service.reconcile(stage_bindings={})
        self.assertEqual(report.items[0].outcome, "COMPLETE")
        self.assertEqual(self.journal.projection()["card_states"]["card-1"], "COMPLETE")

    def test_reconciliation_resolves_unknown_outcome_from_live_stage_facts(self):
        self.make_waiting_journal()
        self.append("OUTCOME_UNKNOWN", "unknown:stage-facts")
        reader = steward_github.FakeGitHubReader(self.facts(merged=True))
        service = StewardService(mission_id=MISSION, journal=self.journal, github=reader)
        report = service.reconcile(stage_bindings={})
        self.assertEqual(report.items[0].outcome, "COMPLETE")
        self.assertEqual(
            self.journal.projection()["card_states"]["card-1"],
            "COMPLETE",
        )

    def test_unknown_outcome_without_stage_binding_remains_paused(self):
        self.append("QUEUED", "queue:unknown-no-binding")
        self.append("RUNNING", "run:unknown-no-binding")
        self.append("OUTCOME_UNKNOWN", "unknown:no-binding")
        service = StewardService(
            mission_id=MISSION,
            journal=self.journal,
            github=steward_github.FakeGitHubReader(),
        )
        report = service.reconcile(stage_bindings={})
        self.assertEqual(report.items[0].outcome, "RECOVERY_REQUIRED")
        self.assertEqual(
            report.items[0].reason,
            "unknown_outcome_requires_read_only_reconciliation",
        )
        self.assertEqual(
            self.journal.projection()["card_states"]["card-1"],
            "OUTCOME_UNKNOWN",
        )

    def test_unknown_outcome_stays_paused_on_stage_identity_drift(self):
        self.make_waiting_journal()
        self.append("OUTCOME_UNKNOWN", "unknown:stage-drift")
        reader = steward_github.FakeGitHubReader(self.facts(head="c" * 40))
        service = StewardService(mission_id=MISSION, journal=self.journal, github=reader)
        report = service.reconcile(stage_bindings={})
        self.assertEqual(report.items[0].outcome, "RECOVERY_REQUIRED")
        self.assertEqual(
            report.items[0].reason,
            "unknown_outcome_requires_read_only_reconciliation",
        )
        self.assertEqual(
            self.journal.projection()["card_states"]["card-1"],
            "OUTCOME_UNKNOWN",
        )

    def test_unknown_outcome_with_insufficient_merged_facts_stays_paused(self):
        self.make_waiting_journal()
        self.append("OUTCOME_UNKNOWN", "unknown:merged-gates")
        facts = self.facts(merged=True)
        facts.update({"ci_state": "PENDING", "review_state": "PENDING"})
        reader = steward_github.FakeGitHubReader(facts)
        service = StewardService(mission_id=MISSION, journal=self.journal, github=reader)
        report = service.reconcile(stage_bindings={})
        self.assertEqual(report.items[0].outcome, "RECOVERY_REQUIRED")
        self.assertEqual(
            self.journal.projection()["card_states"]["card-1"],
            "OUTCOME_UNKNOWN",
        )

    def test_reconciliation_keeps_identity_drift_paused_for_reviewing_card(self):
        self.make_waiting_journal()
        reader = steward_github.FakeGitHubReader(self.facts(head="c" * 40))
        service = StewardService(mission_id=MISSION, journal=self.journal, github=reader)
        report = service.reconcile(stage_bindings={})
        self.assertEqual(report.items[0].outcome, "WAITING")
        self.assertEqual(
            self.journal.projection()["card_states"]["card-1"],
            "REVIEWING",
        )

    def test_reconciliation_revokes_waiting_for_merge_when_ci_regresses(self):
        self.make_waiting_journal()
        self.journal.append(
            event="STAGE_WAITING_FOR_MERGE",
            idempotency_key="waiting:regression",
            mission_id=MISSION,
            stage_id="stage-1",
            card_id="card-1",
            attempt=1,
            state="WAITING_FOR_MERGE",
            detail="gates_passed",
        )
        facts = self.facts()
        facts["ci_state"] = "PENDING"
        reader = steward_github.FakeGitHubReader(facts)
        service = StewardService(mission_id=MISSION, journal=self.journal, github=reader)
        report = service.reconcile(stage_bindings={})
        self.assertEqual(report.items[0].outcome, "WAITING")
        self.assertEqual(report.items[0].reason, "ci_pending")
        self.assertEqual(
            self.journal.projection()["card_states"]["card-1"],
            "REVIEWING",
        )

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

    def test_reconciliation_uses_canonical_facts_without_local_review_receipt(self):
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
        self.assertEqual(item.outcome, "WAITING_FOR_MERGE")
        self.assertEqual(item.reason, "exact_head_ci_and_review_pass")
        self.assertEqual(
            self.journal.projection()["card_states"]["unreviewed"],
            "WAITING_FOR_MERGE",
        )

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

    def test_recover_keeps_unknown_outcome_paused_for_reconciliation(self):
        expected_path, expected_branch = worktree_manager.steward_worktree_location(
            MISSION, "stage-1", "card-1", BASE
        )
        binding_digest = worktree_manager.steward_binding_digest(
            MISSION, "stage-1", "card-1", BASE
        )
        self.append("QUEUED", "queue:unknown")
        self.journal.append(
            event="WORKER_STARTED",
            idempotency_key="run:unknown",
            mission_id=MISSION,
            stage_id="stage-1",
            card_id="card-1",
            attempt=1,
            state="RUNNING",
            detail="isolated_worktree_bound",
            data={
                "base_sha": BASE,
                "worktree_binding_sha256": binding_digest,
                "branch": expected_branch,
            },
        )
        self.append("OUTCOME_UNKNOWN", "unknown:outcome")
        service = StewardService(
            mission_id=MISSION,
            journal=self.journal,
            github=steward_github.FakeGitHubReader(),
            repo_path=self.root,
        )
        with mock.patch.object(worktree_manager, "verify_worktree", return_value=True):
            report = service.recover()
        self.assertEqual(report.items[0].outcome, "RECOVERY_REQUIRED")
        self.assertEqual(
            report.items[0].reason,
            "unknown_outcome_requires_read_only_reconciliation",
        )

    def test_reviewer_sandbox_uses_read_only_mount_for_non_linked_worktree(self):
        command = workers._sandbox_command(
            ["/usr/bin/python3", "-c", "pass"],
            self.root,
            workers.child_environment({"PATH": "/usr/bin"}),
            worktree_writable=False,
        )
        worktree_index = command.index(str(self.root))
        self.assertEqual(command[worktree_index - 1], "--ro-bind")
        self.assertNotIn(("--ro-bind", "/etc", "/etc"), zip(command, command[1:], command[2:]))

    def test_service_wakeup_interrupts_periodic_wait(self):
        service = StewardService(
            mission_id=MISSION,
            journal=self.journal,
            github=steward_github.FakeGitHubReader(),
        )
        self.assertFalse(service.wait_for_wakeup(0))
        service.wakeup()
        self.assertTrue(service.wait_for_wakeup(0))
        self.assertFalse(service.wait_for_wakeup(0))

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

        merged = self.facts(merged=True)
        merged.update({"ci_state": "PENDING", "review_state": "PENDING"})
        status = steward_github.reconcile_stage_pr(
            merged,
            repository=merged["repository"],
            pr_number=merged["pr_number"],
            expected_base_sha=BASE,
            expected_head_sha=HEAD,
        )
        self.assertEqual(status.outcome, "WAITING")
        self.assertEqual(status.reason, "merged_pr_ci_pending")

    def test_github_reader_rejects_malformed_flags_and_mixed_check_states(self):
        malformed = self.facts()
        malformed["isDraft"] = "false"
        reader = steward_github.GhReadOnlyGitHub()
        with (
            mock.patch("steward_github.subprocess.run") as run,
            mock.patch.object(
                steward_github.state_manager,
                "current_effective_reviews",
                return_value={
                    "complete": True,
                    "review_decision": "APPROVED",
                    "requested_changes": [],
                    "effective_reviews": [
                        {"state": "APPROVED", "is_current_head": True}
                    ],
                },
            ),
            mock.patch.object(
                steward_github.state_manager,
                "review_threads_status",
                return_value={
                    "complete": True,
                    "unresolved_thread_ids": [],
                },
            ),
        ):
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
                        {"name": name, "conclusion": "SUCCESS", "status": "NOT_COMPLETED"}
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
            self.assertEqual(observed["ci_state"], "UNKNOWN")

    def test_github_reader_treats_empty_review_decision_as_pending(self):
        reader = steward_github.GhReadOnlyGitHub()
        payload = {
            "state": "OPEN",
            "isDraft": True,
            "mergedAt": None,
            "baseRefName": "main",
            "headRefName": "agent/steward-card",
            "baseRefOid": BASE,
            "headRefOid": HEAD,
            "statusCheckRollup": [],
            "reviewDecision": "",
        }
        with mock.patch("steward_github.subprocess.run") as run:
            run.return_value.returncode = 0
            run.return_value.stdout = json.dumps(payload)
            run.return_value.stderr = ""
            observed = reader.fetch_stage_pr("Igzela/token-efficient-agent-harness-lab", 7)
        self.assertEqual(observed["review_state"], "PENDING")

    def test_github_reader_rejects_aggregate_approval_without_current_head_review(self):
        reader = steward_github.GhReadOnlyGitHub()
        payload = {
            "state": "OPEN",
            "isDraft": False,
            "mergedAt": None,
            "baseRefName": "main",
            "headRefName": "agent/steward-card",
            "baseRefOid": BASE,
            "headRefOid": HEAD,
            "statusCheckRollup": [],
            "reviewDecision": "APPROVED",
        }
        with (
            mock.patch("steward_github.subprocess.run") as run,
            mock.patch.object(
                steward_github.state_manager,
                "current_effective_reviews",
                return_value={
                    "complete": True,
                    "review_decision": "APPROVED",
                    "requested_changes": [],
                    "effective_reviews": [
                        {"state": "APPROVED", "is_current_head": False}
                    ],
                },
            ),
            mock.patch.object(
                steward_github.state_manager,
                "review_threads_status",
                return_value={"complete": True, "unresolved_thread_ids": []},
            ),
        ):
            run.return_value.returncode = 0
            run.return_value.stdout = json.dumps(payload)
            run.return_value.stderr = ""
            with self.assertRaisesRegex(
                steward_github.GitHubReadError, "current_head_review_approval_missing"
            ):
                reader.fetch_stage_pr("Igzela/token-efficient-agent-harness-lab", 7)

    def test_service_execute_stage_runs_recovery_preflight_then_dispatcher(self):
        registered = contract.campaign_mission()
        mission = contract.activate_current_mission(
            repository=registered.repository_identity.repository,
            base_sha=registered.repository_identity.base_sha,
            branch=registered.repository_identity.branch,
            source_ref=registered.repository_identity.source_ref,
            source_sha256=registered.repository_identity.source_sha256,
            proposal_sha256=registered.proposal_sha256,
            owner_approval=registered.owner_approval,
            owner_authenticator=type("Authenticator", (), {"verify": lambda *_args: True})(),
        )
        service = StewardService(
            mission_id=MISSION,
            journal=self.journal,
            github=steward_github.FakeGitHubReader(),
        )
        calls = []

        def dispatch(received_mission, stage, cards, **kwargs):
            calls.append((received_mission, stage, cards, kwargs))
            return {"card-1": "WAITING_FOR_MERGE"}

        stage = mock.Mock(stage_id="stage-1")
        result = service.execute_stage(
            dispatch,
            mission=mission,
            stage=stage,
            cards=(),
            base_sha=BASE,
            stage_pr=None,
        )
        self.assertEqual(result, {"card-1": "WAITING_FOR_MERGE"})
        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0][0], mission)
        self.assertEqual(calls[0][3]["base_sha"], BASE)
        self.assertEqual(self.journal.projection()["event_count"], 1)

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

    def test_bounded_process_worker_imports_commit_from_private_git_view(self):
        repo = self.root / "repo"
        worktree = self.root / "worktree"
        subprocess.run(["git", "init", "-b", "main", str(repo)], check=True, capture_output=True)
        subprocess.run(["git", "config", "user.name", "Steward Test"], cwd=repo, check=True)
        subprocess.run(
            ["git", "config", "user.email", "steward-test@example.invalid"],
            cwd=repo,
            check=True,
        )
        (repo / "README.md").write_text("before\n", encoding="utf-8")
        subprocess.run(["git", "add", "README.md"], cwd=repo, check=True, capture_output=True)
        subprocess.run(["git", "commit", "-m", "initial"], cwd=repo, check=True, capture_output=True)
        base = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=repo, check=True, capture_output=True, text=True
        ).stdout.strip()
        branch = "agent/steward-sandbox-test"
        subprocess.run(
            ["git", "worktree", "add", "-b", branch, str(worktree), base],
            cwd=repo,
            check=True,
            capture_output=True,
        )

        canonical_ref = str(repo / ".git" / "refs" / "heads" / "main")
        script = (
            "from pathlib import Path; import json, subprocess; "
            "Path('README.md').write_text('after\\n'); "
            f"denied=subprocess.run(['/usr/bin/python3','-c',"
            f"\"from pathlib import Path; Path({canonical_ref!r}).write_text('x')\"], "
            "capture_output=True).returncode != 0; "
            "subprocess.run(['/usr/bin/git','add','README.md'], check=True); "
            "subprocess.run(['/usr/bin/git','commit','-m','worker'], check=True, "
            "capture_output=True); "
            "head=subprocess.run(['/usr/bin/git','rev-parse','HEAD'], check=True, "
            "capture_output=True, text=True).stdout.strip(); "
            "print(json.dumps({'schema_version':'steward_worker_outcome.v1',"
            "'status':'PASS','session_id':'steward-process:card-1:1',"
            "'head_sha':head,'changed_paths':['README.md'],"
            "'detail':'canonical_git_denied' if denied else 'canonical_git_writable'}))"
        )
        context = workers.WorkerContext(
            mission_id=MISSION,
            stage_id="stage-1",
            card_id="card-1",
            attempt=1,
            model_tier="T1",
            base_sha=base,
            worktree=worktree,
            allowed_paths=("README.md",),
            steps=("bounded",),
            focused_tests=("focused",),
            negative_checks=("negative",),
            expected_evidence=("receipt",),
            environment=workers.child_environment({"PATH": "/usr/bin"}),
            worktree_branch=branch,
        )
        result = workers.BoundedProcessWorker(
            lambda _context: ["/usr/bin/python3", "-c", script], timeout_seconds=10
        ).run(context)
        self.assertEqual(result.status, "PASS")
        self.assertEqual(result.detail, "canonical_git_denied")
        self.assertNotEqual(result.head_sha, base)
        self.assertEqual(
            subprocess.run(
                ["git", "rev-parse", "HEAD"], cwd=worktree, check=True, capture_output=True, text=True
            ).stdout.strip(),
            result.head_sha,
        )
        self.assertEqual(
            subprocess.run(
                ["git", "status", "--porcelain"], cwd=worktree, check=True, capture_output=True, text=True
            ).stdout,
            "",
        )

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
