"""Provider-free tests for controller-owned plan lifecycle transitions."""

from __future__ import annotations

import json
from pathlib import Path
import sys
import unittest
from unittest import mock

CONTROL = Path(__file__).resolve().parents[1] / "scripts" / "agent-control"
sys.path.insert(0, str(CONTROL))

import dispatcher  # noqa: E402
import local_loop  # noqa: E402
import local_run_once  # noqa: E402
import plan_lane  # noqa: E402
import plan_lifecycle  # noqa: E402
import state_manager  # noqa: E402

LEDGER = 900
PR = 42
MAIN = "a" * 40
HEAD = "b" * 40
MERGE = "c" * 40
ATTEMPT = "123e4567-e89b-12d3-a456-426614174000"
PACKET = "PE7-LIFECYCLE-CONTROLLER-1"
DISPATCH_ID = f"plan-run:{PACKET}:{MAIN}:{ATTEMPT}"
CLOSEOUT_REFERENCE = (
    f"PR #{PR} exact head `{HEAD}`; merge `{MERGE}`; exact-head `PASS`; canonical workflow `7`"
)

DETAILS = {
    "ledger_issue_number": LEDGER,
    "subject_kind": "plan-packet",
    "subject_id": PACKET,
    "source_main_sha": MAIN,
    "task_spec_sha256": "d" * 64,
    "allowed_paths": ["scripts/agent-control/", "tests/"],
    "canonical_branch": "agent/packet-pe7-lifecycle-controller-1",
    "attempt_id": ATTEMPT,
    "execution_token": "b" * 32,
    "claim_nonce": "c" * 32,
    "target_label": state_manager.LABEL_RUNNING,
}


def candidate():
    return plan_lane.PlanCandidate(
        packet_id=PACKET,
        source_main_sha=MAIN,
        task_spec_sha256=DETAILS["task_spec_sha256"],
        goal="Wire plan lifecycle receipts through existing owners.",
        allowed_paths=["scripts/agent-control/", "tests/"],
        prerequisites=[],
        forbidden_changes=["default branch", "provider calls"],
        verification=["focused provider-free tests"],
        rollback=["disable the adapter and revert the packet"],
    )


def dispatch_state(status, details=None):
    return {
        "kind": "agent-orchestrator-dispatch-state",
        "version": 1,
        "issue_number": LEDGER,
        "dispatch_id": DISPATCH_ID,
        "action": "plan-run",
        "status": status,
        "details": details if details is not None else dict(DETAILS),
    }


def ci_wire(status="terminal_success", pr_number=PR, head_sha=HEAD, run_id=7):
    return {
        "kind": "agent-orchestrator-ci-state",
        "version": 2,
        "pr_number": pr_number,
        "head_sha": head_sha,
        "workflow_run_id": run_id,
        "workflow_name": "tests",
        "required_jobs": [],
        "successful_jobs": [],
        "status": status,
        "extra": {},
    }


def review_wire(verdict="PASS", pr_number=PR, head_sha=HEAD, run_id=9):
    return {
        "kind": "agent-orchestrator-review-state",
        "version": 3,
        "issue_number": LEDGER,
        "pr_number": pr_number,
        "head_sha": head_sha,
        "verdict": verdict,
        "summary": "exact-head plan review receipt",
        "blockers": [],
        "major_notes": [],
        "minor_notes": [],
        "artifact_sha256": "",
        "review_workflow_run_id": run_id,
        "base_sha": MAIN,
        "reviewed_range": f"{MAIN}...{HEAD}",
        "review_mode": "full",
        "review_round": 1,
        "prior_reviewed_head": "",
        "findings": [],
        "finding_ledger_digest": "",
        "open_blocker_ids": [],
        "deferred_note_ids": [],
        "decision_required_ids": [],
        "autonomous_repairs_remaining": 1,
        "stop_reason": "",
        "review_protocol_version": "v3",
    }


def merge_wire(merge_sha=MERGE, pr_number=PR, head_sha=HEAD):
    return {
        "kind": "agent-orchestrator-merge-state",
        "version": 1,
        "issue_number": LEDGER,
        "pr_number": pr_number,
        "expected_head_sha": head_sha,
        "merge_commit_sha": merge_sha,
        "status": "confirmed",
    }


def worker_wire():
    return {
        "kind": "agent-orchestrator-state",
        "version": 1,
        "pr_number": PR,
        "head_sha": HEAD,
        "worker_type": "plan-run",
        "extra": {"subject_id": PACKET, "attempt_id": ATTEMPT},
    }


def comment(body):
    return {"author": {"login": "github-actions[bot]"}, "body": body}


def patch_bodies(*bodies):
    def _bodies(_issue, _marker, _repo=""):
        return bodies[-1] if bodies else ""

    return _bodies


class TestPlanReceiptReadback(unittest.TestCase):
    def test_ci_receipt_absent_reads_none(self):
        with mock.patch.object(state_manager, "get_issue_comment_bodies", return_value=""):
            self.assertIsNone(plan_lifecycle.plan_ci_receipt(LEDGER, PR, HEAD))

    def test_ci_receipt_requires_successful_terminal_status(self):
        for status in ("in_progress", "terminal_failure"):
            with self.subTest(status=status), mock.patch.object(
                state_manager, "get_issue_comment_bodies",
                return_value=json.dumps(ci_wire(status=status)),
            ):
                self.assertIsNone(plan_lifecycle.plan_ci_receipt(LEDGER, PR, HEAD))

    def test_ci_receipt_requires_exact_binding(self):
        with mock.patch.object(
            state_manager, "get_issue_comment_bodies",
            return_value=json.dumps(ci_wire(head_sha="f" * 40)),
        ):
            self.assertIsNone(plan_lifecycle.plan_ci_receipt(LEDGER, PR, HEAD))

    def test_ci_receipt_matches_exact_terminal_binding(self):
        with mock.patch.object(
            state_manager, "get_issue_comment_bodies",
            return_value=json.dumps(ci_wire()),
        ):
            receipt = plan_lifecycle.plan_ci_receipt(LEDGER, PR, HEAD)
        self.assertIsNotNone(receipt)
        self.assertEqual(receipt["status"], "terminal_success")
        self.assertEqual(receipt["workflow_run_id"], 7)

    def test_review_receipt_requires_pass_verdict_and_binding(self):
        for body, pr_number, head_sha in (
            (json.dumps(review_wire(verdict="FAIL")), PR, HEAD),
            (json.dumps(review_wire()), 43, HEAD),
            (json.dumps(review_wire()), PR, "f" * 40),
            ("", PR, HEAD),
        ):
            with mock.patch.object(
                state_manager, "get_issue_comment_bodies", return_value=body
            ):
                self.assertIsNone(plan_lifecycle.plan_review_receipt(LEDGER, pr_number, head_sha))

    def test_review_receipt_matches_exact_pass_binding(self):
        with mock.patch.object(
            state_manager, "get_issue_comment_bodies",
            return_value=json.dumps(review_wire()),
        ):
            receipt = plan_lifecycle.plan_review_receipt(LEDGER, PR, HEAD)
        self.assertIsNotNone(receipt)
        self.assertEqual(receipt["verdict"], "PASS")

    def test_merge_receipt_requires_confirmed_binding_and_wellformed_sha(self):
        for body, pr_number, head_sha in (
            (json.dumps(merge_wire(merge_sha="z")), PR, HEAD),
            (json.dumps(merge_wire()), 43, HEAD),
            (json.dumps(merge_wire()), PR, "f" * 40),
            ("", PR, HEAD),
        ):
            with mock.patch.object(
                state_manager, "get_issue_comment_bodies", return_value=body
            ):
                self.assertIsNone(plan_lifecycle.plan_merge_receipt(LEDGER, pr_number, head_sha))

    def test_merge_receipt_matches_exact_confirmed_binding(self):
        with mock.patch.object(
            state_manager, "get_issue_comment_bodies",
            return_value=json.dumps(merge_wire()),
        ):
            receipt = plan_lifecycle.plan_merge_receipt(LEDGER, PR, HEAD)
        self.assertIsNotNone(receipt)
        self.assertEqual(receipt["merge_commit_sha"], MERGE)


class TestRecordPlanMergeReceipt(unittest.TestCase):
    def _patch(self, claim=None, merge_body="", record_ok=True):
        return [
            mock.patch.object(
                state_manager, "read_dispatch_state",
                return_value=claim if claim is not None else dispatch_state("dispatched"),
            ),
            mock.patch.object(
                state_manager, "get_issue_comment_bodies",
                return_value=merge_body,
            ),
            mock.patch.object(state_manager, "record_merge_state", return_value=record_ok),
        ]

    def _run(self, patches):
        for patch in patches:
            patch.start()
        self.addCleanup(lambda: [patch.stop() for patch in patches])
        return plan_lifecycle.record_plan_merge_receipt(
            LEDGER, PACKET, ATTEMPT, MAIN, PR, HEAD, MERGE
        )

    def test_invalid_identity_fails_closed_without_write(self):
        for packet_id, attempt, sha, pr in (
            ("bad packet", ATTEMPT, MERGE, PR),
            (PACKET, "not-an-attempt", MERGE, PR),
            (PACKET, ATTEMPT, "z", PR),
            (PACKET, ATTEMPT, MERGE, 0),
        ):
            with mock.patch.object(state_manager, "record_merge_state") as write:
                result = plan_lifecycle.record_plan_merge_receipt(
                    LEDGER, packet_id, attempt, MAIN, pr, HEAD, sha
                )
            self.assertFalse(result["recorded"], result)
            write.assert_not_called()

    def test_missing_claim_fails_closed(self):
        with mock.patch.object(state_manager, "read_dispatch_state", return_value=None):
            result = plan_lifecycle.record_plan_merge_receipt(
                LEDGER, PACKET, ATTEMPT, MAIN, PR, HEAD, MERGE
            )
        self.assertFalse(result["recorded"])
        self.assertEqual(result["reason"], "plan_claim_not_found")

    def test_claim_must_be_dispatched(self):
        with mock.patch.object(
            state_manager, "read_dispatch_state",
            return_value=dispatch_state("failed", {**DETAILS, "reason": "codex_failed"}),
        ):
            result = plan_lifecycle.record_plan_merge_receipt(
                LEDGER, PACKET, ATTEMPT, MAIN, PR, HEAD, MERGE
            )
        self.assertFalse(result["recorded"])
        self.assertEqual(result["reason"], "plan_claim_state_unexpected")

    def test_conflicting_existing_receipt_fails_closed(self):
        result = self._run(self._patch(merge_body=json.dumps(merge_wire(merge_sha="f" * 40))))
        self.assertFalse(result["recorded"])
        self.assertEqual(result["reason"], "conflicting_merge_receipt")
        state_manager.record_merge_state.assert_not_called()

    def test_identical_existing_receipt_is_idempotent(self):
        result = self._run(self._patch(merge_body=json.dumps(merge_wire())))
        self.assertTrue(result["recorded"])
        self.assertEqual(result["reason"], "already_recorded")
        state_manager.record_merge_state.assert_not_called()

    def test_fresh_receipt_records_through_existing_owner(self):
        result = self._run(self._patch())
        self.assertTrue(result["recorded"])
        state_manager.record_merge_state.assert_called_once_with(
            LEDGER, PR, HEAD, MERGE, ""
        )

    def test_write_failure_fails_closed(self):
        result = self._run(self._patch(record_ok=False))
        self.assertFalse(result["recorded"])
        self.assertEqual(result["reason"], "merge_receipt_write_failed")


class TestRecordPlanCloseoutReceipt(unittest.TestCase):
    def _patch(self, bodies, claim=None, labels=None, set_ok=True, remove_ok=True):
        return [
            mock.patch.object(
                state_manager, "get_issue_comment_bodies",
                side_effect=lambda _issue, marker, _repo="": {
                    "agent-orchestrator-ci-state": bodies.get("ci"),
                    "agent-orchestrator-review-state": bodies.get("review"),
                    "agent-orchestrator-merge-state": bodies.get("merge"),
                }.get(marker, ""),
            ),
            mock.patch.object(
                state_manager, "read_dispatch_state",
                return_value=claim if claim is not None else dispatch_state("dispatched"),
            ),
            mock.patch.object(state_manager, "record_dispatch_state", return_value=True),
            mock.patch.object(
                state_manager, "get_issue_labels_checked",
                return_value=labels if labels is not None else {state_manager.LABEL_RUNNING},
            ),
            mock.patch.object(state_manager, "set_labels", return_value=set_ok),
            mock.patch.object(state_manager, "remove_labels", return_value=remove_ok),
        ]

    def _run(self, patches):
        for patch in patches:
            patch.start()
        self.addCleanup(lambda: [patch.stop() for patch in patches])
        return plan_lifecycle.record_plan_closeout_receipt(
            LEDGER, PACKET, ATTEMPT, MAIN, PR, HEAD, "closed_out", CLOSEOUT_REFERENCE
        )

    def _full(self):
        return {
            "ci": json.dumps(ci_wire()),
            "review": json.dumps(review_wire()),
            "merge": json.dumps(merge_wire()),
        }

    def test_missing_transitions_fail_closed(self):
        for missing in ("ci", "review", "merge"):
            bodies = self._full()
            bodies[missing] = ""
            with mock.patch.object(
                state_manager, "get_issue_comment_bodies",
                side_effect=lambda _issue, marker, _repo="": {
                    "agent-orchestrator-ci-state": bodies.get("ci"),
                    "agent-orchestrator-review-state": bodies.get("review"),
                    "agent-orchestrator-merge-state": bodies.get("merge"),
                }.get(marker, ""),
            ), mock.patch.object(
                state_manager, "record_dispatch_state"
            ) as write:
                result = plan_lifecycle.record_plan_closeout_receipt(
                    LEDGER, PACKET, ATTEMPT, MAIN, PR, HEAD, "closed_out", CLOSEOUT_REFERENCE
                )
            self.assertFalse(result["recorded"], result)
            self.assertEqual(result["reason"], f"missing_transition:{missing}")
            write.assert_not_called()

    def test_closeout_records_terminal_claim_and_labels(self):
        result = self._run(self._patch(self._full()))
        self.assertTrue(result["recorded"])
        self.assertEqual(result["reason"], "closed_out")
        state_manager.record_dispatch_state.assert_called_once()
        write_args = state_manager.record_dispatch_state.call_args.args
        self.assertEqual(write_args[0], LEDGER)
        self.assertEqual(write_args[1], DISPATCH_ID)
        self.assertEqual(write_args[2], "plan-run")
        self.assertEqual(write_args[3], "closed_out")
        self.assertEqual(write_args[4]["terminal_packet_state"], "closed_out")
        self.assertEqual(write_args[4]["closeout_reference"], CLOSEOUT_REFERENCE)
        state_manager.set_labels.assert_called_once_with(
            LEDGER, state_manager.LABEL_COMPLETE, repo=""
        )
        state_manager.remove_labels.assert_called_once_with(
            LEDGER, state_manager.LABEL_RUNNING, repo=""
        )

    def test_closeout_reentry_is_idempotent(self):
        claim = dispatch_state("closed_out", {
            **DETAILS, "terminal_packet_state": "closed_out",
            "closeout_reference": CLOSEOUT_REFERENCE,
        })
        result = self._run(self._patch(self._full(), claim=claim))
        self.assertTrue(result["recorded"])
        self.assertEqual(result["reason"], "already_closed_out")
        state_manager.record_dispatch_state.assert_not_called()

    def test_closeout_rejects_failed_claim(self):
        claim = dispatch_state("failed", {**DETAILS, "reason": "codex_failed"})
        result = self._run(self._patch(self._full(), claim=claim))
        self.assertFalse(result["recorded"])
        self.assertEqual(result["reason"], "plan_claim_state_unexpected")
        state_manager.record_dispatch_state.assert_not_called()

    def test_closeout_label_failure_fails_closed(self):
        result = self._run(self._patch(self._full(), set_ok=False))
        self.assertFalse(result["recorded"])
        self.assertEqual(result["reason"], "closeout_label_failed")

    def test_closeout_identity_invalid_fails_closed(self):
        with mock.patch.object(state_manager, "record_dispatch_state") as write:
            result = plan_lifecycle.record_plan_closeout_receipt(
                LEDGER, PACKET, "bad", MAIN, PR, HEAD, "closed_out", CLOSEOUT_REFERENCE
            )
        self.assertFalse(result["recorded"])
        self.assertEqual(result["reason"], "closeout_identity_invalid")
        write.assert_not_called()

    def test_closeout_reference_must_bind_head_merge_and_canonical_ci(self):
        with mock.patch.object(state_manager, "record_dispatch_state") as write:
            result = plan_lifecycle.record_plan_closeout_receipt(
                LEDGER, PACKET, ATTEMPT, MAIN, PR, HEAD, "closed_out", f"PR #{PR}"
            )
        self.assertFalse(result["recorded"])
        self.assertEqual(result["reason"], "closeout_identity_invalid")
        write.assert_not_called()

    def test_closeout_reference_values_must_match_verified_ledger_receipts(self):
        wrong_merge_reference = plan_lifecycle.canonical_closeout_reference(
            PR, HEAD, "e" * 40, 7
        )
        assert wrong_merge_reference is not None
        patches = self._patch(self._full())
        for patch in patches:
            patch.start()
        self.addCleanup(lambda: [patch.stop() for patch in patches])
        result = plan_lifecycle.record_plan_closeout_receipt(
            LEDGER,
            PACKET,
            ATTEMPT,
            MAIN,
            PR,
            HEAD,
            "closed_out",
            wrong_merge_reference,
        )
        self.assertFalse(result["recorded"])
        self.assertEqual(result["reason"], "closeout_reference_binding_invalid")
        state_manager.record_dispatch_state.assert_not_called()

    def test_legacy_closeout_reference_reconciles_only_with_all_bound_receipts(self):
        lifecycle = {
            "claim_status": "closed_out",
            "pr_number": PR,
            "head_sha": HEAD,
            "stages": {"ci": True, "review": True, "merge": True, "closeout": True},
            "transitions": {
                "ci": ci_wire(),
                "review": review_wire(),
                "merge": merge_wire(),
                "closeout": {"closeout_reference": f"PR #{PR}"},
            },
        }
        with mock.patch.object(plan_lifecycle, "read_plan_lifecycle", return_value=lifecycle):
            reconciled = plan_lifecycle.reconcile_legacy_closeout_reference(
                LEDGER, PACKET, ATTEMPT, f"PR #{PR}"
            )
        self.assertEqual(reconciled, CLOSEOUT_REFERENCE)
        with mock.patch.object(plan_lifecycle, "read_plan_lifecycle", return_value=lifecycle):
            self.assertEqual(
                plan_lifecycle.reconcile_legacy_closeout_reference(
                    LEDGER, PACKET, ATTEMPT, CLOSEOUT_REFERENCE
                ),
                CLOSEOUT_REFERENCE,
            )
        mismatch = CLOSEOUT_REFERENCE.replace(f"PR #{PR}", "PR #999")
        with mock.patch.object(plan_lifecycle, "read_plan_lifecycle", return_value=lifecycle):
            self.assertIsNone(plan_lifecycle.reconcile_legacy_closeout_reference(
                LEDGER, PACKET, ATTEMPT, mismatch
            ))
        lifecycle["stages"]["review"] = False
        with mock.patch.object(plan_lifecycle, "read_plan_lifecycle", return_value=lifecycle):
            self.assertIsNone(plan_lifecycle.reconcile_legacy_closeout_reference(
                LEDGER, PACKET, ATTEMPT, f"PR #{PR}"
            ))
        lifecycle["stages"] = []
        with mock.patch.object(plan_lifecycle, "read_plan_lifecycle", return_value=lifecycle):
            self.assertIsNone(plan_lifecycle.reconcile_legacy_closeout_reference(
                LEDGER, PACKET, ATTEMPT, f"PR #{PR}"
            ))


class TestReadPlanLifecycle(unittest.TestCase):
    def _comments(self, states):
        return [comment(json.dumps(state)) for state in states]

    def test_invalid_identity_fails_closed(self):
        result = plan_lifecycle.read_plan_lifecycle(LEDGER, PACKET, "bad")
        self.assertFalse(result["stages"]["ci"])
        self.assertEqual(result["reason"], "lifecycle_identity_invalid")

    def test_missing_claim_fails_closed(self):
        with mock.patch.object(state_manager, "get_issue_comments", return_value=[]):
            result = plan_lifecycle.read_plan_lifecycle(LEDGER, PACKET, ATTEMPT)
        self.assertIsNone(result.get("claim"))
        self.assertFalse(result["stages"]["ci"])

    def test_ambiguous_claims_fail_closed(self):
        other_main = "e" * 40
        other = dict(DETAILS)
        other["source_main_sha"] = other_main
        states = [
            dispatch_state("dispatched"),
            {
                "kind": "agent-orchestrator-dispatch-state",
                "version": 1,
                "issue_number": LEDGER,
                "dispatch_id": f"plan-run:{PACKET}:{other_main}:{ATTEMPT}",
                "action": "plan-run",
                "status": "dispatched",
                "details": other,
            },
        ]
        with mock.patch.object(
            state_manager, "get_issue_comments", return_value=self._comments(states)
        ):
            result = plan_lifecycle.read_plan_lifecycle(LEDGER, PACKET, ATTEMPT)
        self.assertFalse(result["stages"]["ci"])
        self.assertEqual(result["reason"], "plan_claim_not_found")

    def test_partial_lifecycle_reads_only_done_stages(self):
        states = [dispatch_state("dispatched")]
        with mock.patch.object(
            state_manager, "get_issue_comments", return_value=self._comments(states)
        ), mock.patch.object(
            state_manager, "read_worker_state", return_value=worker_wire()
        ), mock.patch.object(
            state_manager, "get_issue_comment_bodies",
            return_value=json.dumps(ci_wire()),
        ):
            result = plan_lifecycle.read_plan_lifecycle(LEDGER, PACKET, ATTEMPT)
        self.assertEqual(result["stages"], {
            "ci": True, "review": False, "merge": False, "closeout": False,
        })

    def test_full_lifecycle_reads_all_four_transitions(self):
        claim = dispatch_state("closed_out", {
            **DETAILS,
            "terminal_packet_state": "closed_out",
            "closeout_reference": CLOSEOUT_REFERENCE,
        })
        with mock.patch.object(
            state_manager, "get_issue_comments", return_value=self._comments([claim])
        ), mock.patch.object(
            state_manager, "read_worker_state", return_value=worker_wire()
        ), mock.patch.object(
            state_manager, "get_issue_comment_bodies",
            side_effect=lambda _issue, marker, _repo="": {
                "agent-orchestrator-ci-state": json.dumps(ci_wire()),
                "agent-orchestrator-review-state": json.dumps(review_wire()),
                "agent-orchestrator-merge-state": json.dumps(merge_wire()),
            }.get(marker, ""),
        ):
            result = plan_lifecycle.read_plan_lifecycle(LEDGER, PACKET, ATTEMPT)
        self.assertTrue(all(result["stages"].values()))
        self.assertEqual(result["claim_status"], "closed_out")
        self.assertEqual(result["transitions"]["closeout"]["terminal_packet_state"], "closed_out")
        self.assertEqual(result["transitions"]["closeout"]["closeout_reference"], CLOSEOUT_REFERENCE)
        self.assertEqual(result["transitions"]["merge"]["merge_commit_sha"], MERGE)


class TestRecordPlanLifecycleDispatcher(unittest.TestCase):
    def _patch_dispatch(self, claim=None, worker=None, pr=None, merge_sha=None, bodies=None):
        patches = [
            mock.patch.object(dispatcher, "_repo", return_value="acme/repo"),
            mock.patch.object(dispatcher.control_state, "require_live", return_value=None),
            mock.patch.object(
                dispatcher, "_read_live_plan",
                return_value=(candidate(), LEDGER, None),
            ),
            mock.patch.object(
                state_manager, "read_dispatch_state",
                return_value=claim if claim is not None else dispatch_state("dispatched"),
            ),
            mock.patch.object(state_manager, "plan_claim_binding_valid", return_value=(True, "ok")),
            mock.patch.object(
                state_manager, "read_worker_state", return_value=worker if worker is not None else worker_wire()
            ),
            mock.patch.object(
                dispatcher, "_verified_plan_pr", return_value=True
            ),
            mock.patch.object(
                dispatcher, "_authoritative_plan_merge",
                return_value=merge_sha,
            ),
        ]
        if bodies is not None:
            patches.append(mock.patch.object(
                state_manager, "get_issue_comment_bodies",
                side_effect=lambda _issue, marker, _repo="": bodies.get(marker, ""),
            ))
        else:
            patches.append(mock.patch.object(
                state_manager, "get_issue_comment_bodies", return_value=""
            ))
        return patches

    def test_invalid_stage_fails_closed(self):
        with mock.patch.object(state_manager, "read_dispatch_state") as read:
            result = dispatcher.record_plan_lifecycle(PACKET, ATTEMPT, "deploy")
        self.assertFalse(result["recorded"])
        self.assertEqual(result["reason"], "stage_invalid")
        read.assert_not_called()

    def test_missing_claim_fails_closed(self):
        patches = self._patch_dispatch(claim=None)
        for patch in patches:
            patch.start()
        try:
            with mock.patch.object(state_manager, "read_dispatch_state", return_value=None):
                result = dispatcher.record_plan_lifecycle(PACKET, ATTEMPT, "ci")
        finally:
            for patch in patches:
                patch.stop()
        self.assertFalse(result["recorded"])
        self.assertEqual(result["reason"], "plan_claim_not_found")

    def test_unverified_pr_binding_fails_closed(self):
        patches = [p for p in self._patch_dispatch()
                   if not (isinstance(p, mock._patch) and p.attribute == "_verified_plan_pr")]
        for patch in patches:
            patch.start()
        try:
            with mock.patch.object(dispatcher, "_verified_plan_pr", return_value=False):
                result = dispatcher.record_plan_lifecycle(PACKET, ATTEMPT, "ci")
        finally:
            for patch in patches:
                patch.stop()
        self.assertFalse(result["recorded"])
        self.assertEqual(result["reason"], "plan_pr_binding_unverified")

    def test_ci_stage_pending_reads_fail_closed(self):
        patches = self._patch_dispatch()
        for patch in patches:
            patch.start()
        try:
            result = dispatcher.record_plan_lifecycle(PACKET, ATTEMPT, "ci")
        finally:
            for patch in patches:
                patch.stop()
        self.assertFalse(result["recorded"])
        self.assertEqual(result["reason"], "ci_receipt_pending")

    def test_ci_stage_verified_reports_existing_receipt(self):
        patches = self._patch_dispatch(bodies={"agent-orchestrator-ci-state": json.dumps(ci_wire())})
        for patch in patches:
            patch.start()
        try:
            result = dispatcher.record_plan_lifecycle(PACKET, ATTEMPT, "ci")
        finally:
            for patch in patches:
                patch.stop()
        self.assertTrue(result["recorded"])
        self.assertEqual(result["ci_run_id"], 7)

    def test_review_stage_verified_reports_existing_receipt(self):
        patches = self._patch_dispatch(bodies={"agent-orchestrator-review-state": json.dumps(review_wire())})
        for patch in patches:
            patch.start()
        try:
            result = dispatcher.record_plan_lifecycle(PACKET, ATTEMPT, "review")
        finally:
            for patch in patches:
                patch.stop()
        self.assertTrue(result["recorded"])
        self.assertEqual(result["review_workflow_run_id"], 9)

    def test_merge_stage_requires_authoritative_merge_evidence(self):
        patches = self._patch_dispatch(merge_sha=None)
        for patch in patches:
            patch.start()
        try:
            with mock.patch.object(plan_lifecycle, "record_plan_merge_receipt") as record:
                result = dispatcher.record_plan_lifecycle(PACKET, ATTEMPT, "merge")
                record.assert_not_called()
        finally:
            for patch in patches:
                patch.stop()
        self.assertFalse(result["recorded"])
        self.assertEqual(result["reason"], "merge_evidence_unavailable")

    def test_merge_stage_records_verified_merge(self):
        patches = self._patch_dispatch(merge_sha=MERGE)
        for patch in patches:
            patch.start()
        try:
            with mock.patch.object(
                plan_lifecycle, "record_plan_merge_receipt",
                return_value={"recorded": True, "reason": "recorded"},
            ) as record:
                result = dispatcher.record_plan_lifecycle(PACKET, ATTEMPT, "merge")
            record.assert_called_once()
            self.assertEqual(record.call_args.args[1], PACKET)
            self.assertEqual(record.call_args.args[2], ATTEMPT)
            self.assertEqual(record.call_args.args[6], MERGE)
        finally:
            for patch in patches:
                patch.stop()
        self.assertTrue(result["recorded"])
        self.assertEqual(result["merge_commit_sha"], MERGE)

    def test_closeout_stage_records_verified_transition(self):
        patches = self._patch_dispatch(bodies={
            "agent-orchestrator-ci-state": json.dumps(ci_wire()),
            "agent-orchestrator-merge-state": json.dumps(merge_wire()),
        })
        for patch in patches:
            patch.start()
        try:
            with mock.patch.object(
                plan_lifecycle, "record_plan_closeout_receipt",
                return_value={"recorded": True, "reason": "closed_out"},
            ) as record:
                result = dispatcher.record_plan_lifecycle(PACKET, ATTEMPT, "closeout")
            record.assert_called_once()
            self.assertEqual(record.call_args.args[1], PACKET)
            self.assertEqual(record.call_args.args[6], "closed_out")
            self.assertEqual(record.call_args.args[7], CLOSEOUT_REFERENCE)
        finally:
            for patch in patches:
                patch.stop()
        self.assertTrue(result["recorded"])
        self.assertEqual(result["reason"], "closed_out")

    def test_closeout_stage_requires_canonical_ci_and_merge_receipts(self):
        patches = self._patch_dispatch()
        for patch in patches:
            patch.start()
        try:
            with mock.patch.object(plan_lifecycle, "record_plan_closeout_receipt") as record:
                result = dispatcher.record_plan_lifecycle(PACKET, ATTEMPT, "closeout")
            record.assert_not_called()
        finally:
            for patch in patches:
                patch.stop()
        self.assertFalse(result["recorded"])
        self.assertEqual(result["reason"], "closeout_receipt_pending")

    def test_closed_out_claim_reports_idempotent_success(self):
        claim = dispatch_state("closed_out", {**DETAILS, "terminal_packet_state": "closed_out"})
        patches = self._patch_dispatch(claim=claim)
        for patch in patches:
            patch.start()
        try:
            result = dispatcher.record_plan_lifecycle(PACKET, ATTEMPT, "ci")
        finally:
            for patch in patches:
                patch.stop()
        self.assertTrue(result["recorded"])
        self.assertEqual(result["reason"], "already_closed_out")


class TestPlanLifecycleWait(unittest.TestCase):
    @staticmethod
    def _lifecycle(*, stages=None, transitions=None, packet_id=PACKET, pr_number=PR, head_sha=HEAD):
        if stages is None:
            stages = {"ci": True, "review": True, "merge": True, "closeout": True}
        if transitions is None:
            transitions = {
                "ci": ci_wire(pr_number=pr_number, head_sha=head_sha),
                "review": review_wire(pr_number=pr_number, head_sha=head_sha),
                "merge": merge_wire(pr_number=pr_number, head_sha=head_sha),
                "closeout": {
                    "terminal_packet_state": "closed_out",
                    "closeout_reference": plan_lifecycle.canonical_closeout_reference(
                        pr_number, head_sha, MERGE, 7
                    ),
                },
            }
        return {
            "packet_id": packet_id,
            "attempt_id": ATTEMPT,
            "ledger_issue": LEDGER,
            "pr_number": pr_number,
            "head_sha": head_sha,
            "stages": stages,
            "transitions": transitions,
        }

    def test_wait_returns_closed_out_when_all_stages_done(self):
        github = mock.Mock()
        git = mock.Mock()
        runner = local_run_once.LocalRunOnce(
            github, git, repository="acme/repo", repo_path=Path("/tmp"),
            lifecycle_timeout_seconds=60, sleeper=lambda _: None,
        )
        lifecycle = self._lifecycle()
        promotion = {
            "kind": "plan-promote", "status": "promoted",
            "details": {"successor_id": "PE7-SUCCESSOR-PROMOTION-ESCALATION-1"},
        }
        with mock.patch.object(
            plan_lifecycle, "read_plan_lifecycle", return_value=lifecycle
        ), mock.patch.object(
            runner, "_read_plan_promotion", return_value=promotion
        ):
            result = runner._wait_for_plan_terminal_receipts(LEDGER, PACKET, ATTEMPT, PR, HEAD)
        self.assertEqual(result.status, "closed_out")
        self.assertEqual(result.details.get("merge_commit_sha"), MERGE)
        self.assertEqual(result.details.get("terminal_packet_state"), "closed_out")
        self.assertEqual(result.details["promotion"]["status"], "promoted")
        self.assertFalse(result.details["promotion_pending"])
        github.dispatch_controller.assert_not_called()

    def test_evidence_missing_controller_escalation_returns_closeout_to_route_run(self):
        github = mock.Mock()
        runner = local_run_once.LocalRunOnce(
            github, mock.Mock(), repository="acme/repo", repo_path=Path("/tmp"),
            lifecycle_timeout_seconds=60, sleeper=lambda _: None,
        )
        lifecycle = self._lifecycle()
        escalation = {
            "kind": "plan-escalate", "status": "escalated",
            "details": {"reason": "promotion_current_main_evidence_missing"},
        }
        with mock.patch.object(
            plan_lifecycle, "read_plan_lifecycle", return_value=lifecycle
        ), mock.patch.object(
            runner, "_read_plan_promotion", return_value=escalation
        ):
            result = runner._wait_for_plan_terminal_receipts(LEDGER, PACKET, ATTEMPT, PR, HEAD)
        self.assertEqual(result.status, "closed_out")
        self.assertTrue(result.details["promotion_escalated"])
        self.assertTrue(result.details["promotion_pending"])
        self.assertEqual(result.details["promotion"], escalation)
        github.dispatch_controller.assert_not_called()

    def test_other_controller_escalation_remains_a_bounded_pause(self):
        github = mock.Mock()
        runner = local_run_once.LocalRunOnce(
            github, mock.Mock(), repository="acme/repo", repo_path=Path("/tmp"),
            lifecycle_timeout_seconds=60, sleeper=lambda _: None,
        )
        lifecycle = self._lifecycle()
        escalation = {
            "kind": "plan-escalate", "status": "escalated",
            "details": {"reason": "promotion_owner_ambiguous"},
        }
        with mock.patch.object(
            plan_lifecycle, "read_plan_lifecycle", return_value=lifecycle
        ), mock.patch.object(
            runner, "_read_plan_promotion", return_value=escalation
        ):
            result = runner._wait_for_plan_terminal_receipts(LEDGER, PACKET, ATTEMPT, PR, HEAD)
        self.assertEqual(result.status, "bounded_pause")
        self.assertEqual(result.details["reason"], "promotion_owner_ambiguous")
        github.dispatch_controller.assert_not_called()

    def test_wait_rejects_mismatched_lifecycle_before_promotion(self):
        github = mock.Mock()
        runner = local_run_once.LocalRunOnce(
            github, mock.Mock(), repository="acme/repo", repo_path=Path("/tmp"),
            lifecycle_timeout_seconds=60, sleeper=lambda _: None,
        )
        cases = {
            "top_level_pr": self._lifecycle(pr_number=PR + 1),
            "top_level_head": self._lifecycle(head_sha="f" * 40),
            "abbreviated_head": self._lifecycle(head_sha=HEAD[:12]),
            "ci_head": self._lifecycle(transitions={
                "ci": ci_wire(head_sha="f" * 40),
                "review": review_wire(), "merge": merge_wire(),
                "closeout": {"terminal_packet_state": "closed_out", "closeout_reference": f"PR #{PR}"},
            }),
            "review_pr": self._lifecycle(transitions={
                "ci": ci_wire(), "review": review_wire(pr_number=PR + 1),
                "merge": merge_wire(),
                "closeout": {"terminal_packet_state": "closed_out", "closeout_reference": f"PR #{PR}"},
            }),
            "merge_head": self._lifecycle(transitions={
                "ci": ci_wire(), "review": review_wire(),
                "merge": merge_wire(head_sha="f" * 40),
                "closeout": {
                    "terminal_packet_state": "closed_out",
                    "closeout_reference": CLOSEOUT_REFERENCE,
                },
            }),
        }
        for name, reference in {
            "closeout_head": CLOSEOUT_REFERENCE.replace(HEAD, "f" * 40),
            "closeout_merge": CLOSEOUT_REFERENCE.replace(MERGE, "f" * 40),
            "closeout_workflow": CLOSEOUT_REFERENCE.replace("workflow `7`", "workflow `8`"),
            "legacy_closeout": f"PR #{PR}",
        }.items():
            lifecycle = self._lifecycle()
            lifecycle["transitions"]["closeout"] = {
                "terminal_packet_state": "closed_out",
                "closeout_reference": reference,
            }
            cases[name] = lifecycle
        for key in ("packet_id", "attempt_id", "ledger_issue", "pr_number", "head_sha"):
            missing_binding = self._lifecycle()
            del missing_binding[key]
            cases[f"missing_{key}"] = missing_binding
        for name, value in (("string", "true"), ("integer", 1), ("none", None)):
            invalid_stages = self._lifecycle()
            invalid_stages["stages"]["ci"] = value
            cases[f"stage_{name}"] = invalid_stages
        missing_stage = self._lifecycle()
        del missing_stage["stages"]["ci"]
        cases["missing_stage"] = missing_stage
        for name, lifecycle in cases.items():
            with self.subTest(name=name), mock.patch.object(
                plan_lifecycle, "read_plan_lifecycle", return_value=lifecycle
            ), mock.patch.object(runner, "_read_plan_promotion", return_value=None) as promotion:
                result = runner._wait_for_plan_terminal_receipts(
                    LEDGER, PACKET, ATTEMPT, PR, HEAD
                )
                self.assertEqual(result.status, "rejected")
                self.assertEqual(result.details["reason"], "plan_lifecycle_binding_invalid")
                promotion.assert_not_called()
        github.dispatch_controller.assert_not_called()

    def test_wait_dispatches_controller_for_merge_and_closeout_stages(self):
        github = mock.Mock()
        runner = local_run_once.LocalRunOnce(
            github, mock.Mock(), repository="acme/repo", repo_path=Path("/tmp"),
            lifecycle_timeout_seconds=10, sleeper=lambda _: None,
        )
        readbacks = [
            self._lifecycle(
                stages={"ci": True, "review": True, "merge": False, "closeout": False},
                transitions={"ci": ci_wire(), "review": review_wire()},
            ),
            self._lifecycle(
                stages={"ci": True, "review": True, "merge": True, "closeout": False},
                transitions={"ci": ci_wire(), "review": review_wire(), "merge": merge_wire()},
            ),
            self._lifecycle(),
        ]
        with mock.patch.object(
            plan_lifecycle, "read_plan_lifecycle", side_effect=readbacks
        ), mock.patch.object(
            runner, "_read_plan_promotion", return_value=None
        ), mock.patch.object(
            local_run_once.time, "monotonic", side_effect=[0.0, 0.0, 0.0, 11.0]
        ):
            result = runner._wait_for_plan_terminal_receipts(LEDGER, PACKET, ATTEMPT, PR, HEAD)
        self.assertEqual(result.status, "closed_out")
        self.assertTrue(result.details["promotion_pending"])
        github.dispatch_controller.assert_has_calls([
            mock.call("lifecycle-plan", {"packet_id": PACKET, "attempt_id": ATTEMPT, "stage": "merge"}),
            mock.call("lifecycle-plan", {"packet_id": PACKET, "attempt_id": ATTEMPT, "stage": "closeout"}),
            mock.call("promote-plan", {"packet_id": PACKET, "attempt_id": ATTEMPT}),
        ])
        self.assertEqual(github.dispatch_controller.call_count, 3)

    def test_wait_timeout_never_treats_missing_receipts_as_success(self):
        github = mock.Mock()
        runner = local_run_once.LocalRunOnce(
            github, mock.Mock(), repository="acme/repo", repo_path=Path("/tmp"),
            lifecycle_timeout_seconds=10, sleeper=lambda _: None,
        )
        readback = self._lifecycle(
            stages={"ci": True, "review": True, "merge": True, "closeout": False},
            transitions={"ci": ci_wire(), "review": review_wire(), "merge": merge_wire()},
        )
        with mock.patch.object(
            plan_lifecycle, "read_plan_lifecycle", return_value=readback
        ), mock.patch.object(
            local_run_once.time, "monotonic", side_effect=[0.0, 11.0]
        ):
            result = runner._wait_for_plan_terminal_receipts(LEDGER, PACKET, ATTEMPT, PR, HEAD)
        self.assertEqual(result.status, "outcome_unknown")
        self.assertEqual(result.details.get("reason"), "lifecycle_timeout")
        self.assertEqual(result.details.get("stage"), "closeout")
        github.dispatch_controller.assert_called_once_with(
            "lifecycle-plan",
            {"packet_id": PACKET, "attempt_id": ATTEMPT, "stage": "closeout"},
        )

    def test_recovery_recognizes_closed_out_terminal_claim(self):
        github = mock.Mock()
        git = mock.Mock()
        runner = local_run_once.LocalRunOnce(
            github, git, repository="acme/repo", repo_path=Path("/tmp"),
        )
        claim = dispatch_state("closed_out", {
            **DETAILS, "terminal_packet_state": "closed_out",
            "closeout_reference": f"PR #{PR}",
        })
        with mock.patch.object(
            state_manager, "read_dispatch_state", return_value=claim
        ):
            result = runner._recover_existing_plan_claim(
                PACKET, ATTEMPT, candidate(), LEDGER
            )
        self.assertEqual(result.status, "terminal")
        self.assertEqual(result.details.get("claim_status"), "closed_out")
        self.assertEqual(result.details.get("terminal_packet_state"), "closed_out")
        github.dispatch_controller.assert_not_called()


class TestPlanLifecycleWorkflowTransport(unittest.TestCase):
    def test_existing_controller_exposes_lifecycle_and_promotion_with_bounded_inputs(self):
        workflow = (
            Path(__file__).resolve().parents[1]
            / ".github"
            / "workflows"
            / "agent-controller.yml"
        ).read_text(encoding="utf-8")
        self.assertIn("          - lifecycle-plan\n", workflow)
        self.assertIn("          - promote-plan\n", workflow)
        self.assertIn("      stage:\n", workflow)
        self.assertIn("      INPUT_STAGE: ${{ inputs.stage }}\n", workflow)
        self.assertIn("dispatcher.py lifecycle-plan", workflow)
        self.assertIn('"$INPUT_PACKET_ID" "$INPUT_ATTEMPT_ID" "$INPUT_STAGE"', workflow)
        self.assertIn("dispatcher.py promote-plan", workflow)
        self.assertIn("          - record-route-t3-receipt\n", workflow)
        self.assertLessEqual(
            sum(
                1
                for line in workflow.splitlines()
                if line.startswith("      ")
                and not line.startswith("        ")
                and line.rstrip().endswith(":")
            ),
            25,
        )
        self.assertIn("      route_payload:\n", workflow)
        self.assertIn("      INPUT_ROUTE_PAYLOAD: ${{ inputs.route_payload }}\n", workflow)
        self.assertEqual(workflow.count("${{ inputs.route_payload }}"), 1)
        self.assertNotIn('echo "$INPUT_ROUTE_PAYLOAD"', workflow)
        self.assertNotIn("set -x", workflow)
        for removed in (
            "accepted_main_sha", "candidate_digest", "action_digest", "scope_digest",
            "authority_receipt_digest", "outcome_receipt_digest", "owner_evidence_digest",
            "authority_owner_digest", "decision_source", "decision_evidence_digest",
            "issued_at", "expires_at", "disposition",
        ):
            self.assertNotIn(f"      {removed}:\n", workflow)
            self.assertNotIn(f"inputs.{removed}", workflow)
        self.assertIn("dispatcher.py record-route-t3-receipt", workflow)
        self.assertIn('"$INPUT_PACKET_ID" "$INPUT_ROUTE_PAYLOAD"', workflow)
        self.assertIn("          - record-route-owner-outcome\n", workflow)
        self.assertIn("dispatcher.py record-route-owner-outcome", workflow)


if __name__ == "__main__":
    unittest.main()
