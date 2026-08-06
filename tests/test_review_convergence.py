"""Provider-free tests for the Review Convergence Protocol state machine.

Covers the canonical owner scripts/agent-control/review_convergence.py:
R1/R2 rounds, the single autonomous repair batch, no autonomous R3,
severity/disposition separation, exact-PASS cross fields, ledger continuity,
retry-review derivation, and the non-authoritative capsule projection.
"""

from __future__ import annotations

import json
import pathlib
import sys
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts" / "agent-control"))

import review_convergence as rc  # noqa: E402

HEAD1 = "a" * 40
HEAD2 = "b" * 40
BASE = "c" * 40


def finding(**overrides):
    base = {
        "id": "F-1",
        "axis": "correctness",
        "evidence": "defect evidence",
        "severity": "blocker",
        "disposition": "block_current_head",
        "scope_relation": "in_packet",
        "origin_head": HEAD1,
        "acceptance_condition": "fixed with focused test",
        "status": "open",
    }
    base.update(overrides)
    return base


def decision(verdict="PASS", findings=(), review_mode="full", review_round=1, findings_structured=None, **overrides):
    if findings_structured is None:
        findings_structured = bool(findings)
    payload = {
        "verdict": verdict,
        "summary": "summary",
        "reviewed_base": BASE,
        "reviewed_head": HEAD2 if review_mode == "repair_verification" else HEAD1,
        "reviewed_range": "",
        "review_mode": review_mode,
        "review_round": review_round,
        "findings": list(findings),
        "security_ok": True,
        "rollback_ok": True,
        "observed_ci_status": "unknown",
    }
    payload.update(overrides)
    payload["reviewed_range"] = f"{payload['reviewed_base']}...{payload['reviewed_head']}"
    return rc.ReviewDecision(
        verdict=payload["verdict"],
        summary=payload["summary"],
        reviewed_base=payload["reviewed_base"],
        reviewed_head=payload["reviewed_head"],
        reviewed_range=payload["reviewed_range"],
        review_mode=payload["review_mode"],
        review_round=payload["review_round"],
        findings=tuple(
            f if isinstance(f, rc.ReviewFinding) else rc.normalize_finding(f)
            for f in payload["findings"]
        ),
        prior_reviewed_head=payload.get("prior_reviewed_head", ""),
        findings_structured=findings_structured,
        security_ok=payload["security_ok"],
        rollback_ok=payload["rollback_ok"],
        observed_ci_status=payload["observed_ci_status"],
    )


class TestCanonicalBudgets(unittest.TestCase):
    def test_rounds_and_batches_are_distinct_budgets(self):
        self.assertEqual(rc.MAX_SUBSTANTIVE_REVIEW_ROUNDS, 2)
        self.assertEqual(rc.MAX_AUTONOMOUS_REPAIR_BATCHES, 1)
        self.assertEqual(rc.INITIAL_AUTONOMOUS_REPAIRS_REMAINING, 1)
        self.assertNotEqual(
            rc.MAX_SUBSTANTIVE_REVIEW_ROUNDS, rc.MAX_AUTONOMOUS_REPAIR_BATCHES
        )
        self.assertNotIn("MAX_AUTONOMOUS_REVIEW_REPAIR_ROUNDS", dir(rc))


class TestVerdictCrossFields(unittest.TestCase):
    def test_pass_requires_no_open_blockers_or_decisions(self):
        with self.assertRaises(rc.ConvergenceError):
            rc.validate_decision_cross_fields(
                decision(
                    "PASS",
                    findings=(finding(status="open"),),
                )
            )
        with self.assertRaises(rc.ConvergenceError):
            rc.validate_decision_cross_fields(
                decision(
                    "PASS",
                    findings=(finding(disposition="decision_required"),),
                )
            )

    def test_pass_allows_deferred_notes(self):
        note = finding(
            id="N-1",
            severity="minor",
            disposition="defer",
            status="deferred",
        )
        d = decision("PASS", findings=(note,))
        rc.validate_decision_cross_fields(d)
        self.assertEqual(d.deferred_note_ids, ("N-1",))

    def test_pass_without_ci_green_observation_is_valid(self):
        d = decision("PASS", observed_ci_status="unknown")
        rc.validate_decision_cross_fields(d)

    def test_blocked_requires_open_blocker(self):
        with self.assertRaises(rc.ConvergenceError):
            rc.validate_decision_cross_fields(decision("BLOCKED"))
        d = decision("BLOCKED", findings=(finding(),))
        rc.validate_decision_cross_fields(d)

    def test_decision_required_requires_open_decision_or_r2_blocked(self):
        with self.assertRaises(rc.ConvergenceError):
            rc.validate_decision_cross_fields(decision("DECISION_REQUIRED"))
        d = decision(
            "DECISION_REQUIRED",
            findings=(finding(disposition="decision_required"),),
        )
        rc.validate_decision_cross_fields(d)
        r2 = decision(
            "DECISION_REQUIRED",
            review_mode="repair_verification",
            review_round=2,
            findings=(finding(status="open"),),
        )
        rc.validate_decision_cross_fields(r2)

    def test_pun_with_notes_cannot_carry_open_blockers(self):
        with self.assertRaises(rc.ConvergenceError):
            rc.validate_decision_cross_fields(
                decision("PASS_WITH_NOTES", findings=(finding(),))
            )

    def test_review_round_budget_enforced(self):
        with self.assertRaises(rc.ConvergenceError):
            rc.validate_decision_cross_fields(
                decision("BLOCKED", review_round=3, findings=(finding(),))
            )


class TestLegacyMapping(unittest.TestCase):
    def test_legacy_blockers_map_to_open_block_current_head(self):
        d = rc.decision_from_legacy_artifact(
            {
                "verdict": "BLOCKED",
                "summary": "blocked",
                "reviewed_head_sha": HEAD1,
                "blockers": ["broken"],
                "security_ok": True,
                "rollback_ok": True,
            }
        )
        self.assertEqual(d.open_blocker_ids, ("blocker-1",))
        self.assertEqual(d.findings[0].severity, "blocker")
        self.assertEqual(d.findings[0].disposition, "block_current_head")

    def test_legacy_notes_map_to_deferred(self):
        d = rc.decision_from_legacy_artifact(
            {
                "verdict": "PASS",
                "summary": "pass",
                "reviewed_head_sha": HEAD1,
                "blockers": [],
                "major_notes": ["rename later"],
                "minor_notes": ["polish"],
                "security_ok": True,
                "rollback_ok": True,
                "ci_green": False,
            }
        )
        self.assertEqual(d.verdict, "PASS")
        self.assertEqual(len(d.deferred_note_ids), 2)
        self.assertEqual(d.observed_ci_status, "model_reported_not_green")


class TestRoundTransitions(unittest.TestCase):
    def test_r1_pass_is_terminal(self):
        state = rc.initial_r1_state(decision("PASS"))
        self.assertEqual(state.verdict, "PASS")
        self.assertEqual(state.stop_reason, "")
        self.assertEqual(state.autonomous_repairs_remaining, 1)
        self.assertEqual(state.review_round, 1)
        self.assertEqual(state.review_mode, "full")

    def test_r1_blocked_leaves_one_repair_batch(self):
        state = rc.initial_r1_state(decision("BLOCKED", findings=(finding(),)))
        self.assertEqual(state.autonomous_repairs_remaining, 1)
        self.assertEqual(state.open_blocker_ids, ("F-1",))

    def test_r1_decision_required_stops(self):
        state = rc.initial_r1_state(
            decision("DECISION_REQUIRED", findings=(finding(disposition="decision_required"),))
        )
        self.assertEqual(state.stop_reason, "decision_required")
        self.assertEqual(state.autonomous_repairs_remaining, 0)

    def test_repair_batch_consumption_preserves_ledger_identity(self):
        prior = rc.initial_r1_state(decision("BLOCKED", findings=(finding(),)))
        after = rc.after_repair_batch_consumed(prior, new_head_sha=HEAD2)
        self.assertEqual(after.verdict, "INVALIDATED")
        self.assertEqual(after.autonomous_repairs_remaining, 0)
        self.assertEqual(after.review_round, 2)
        self.assertEqual(after.review_mode, "repair_verification")
        self.assertEqual(after.prior_reviewed_head, HEAD1)
        self.assertEqual(after.reviewed_head, HEAD2)
        self.assertEqual(after.finding_ledger_digest, prior.finding_ledger_digest)
        self.assertEqual(after.open_blocker_ids, ("F-1",))

    def test_no_repair_batch_available_rejected(self):
        prior = rc.initial_r1_state(decision("PASS"))
        prior = rc.ReviewRoundState(
            **{**prior.__dict__, "autonomous_repairs_remaining": 0}
        )
        with self.assertRaises(rc.ConvergenceError):
            rc.after_repair_batch_consumed(prior, new_head_sha=HEAD2)

    def test_repair_after_final_round_rejected(self):
        prior = rc.ReviewRoundState(
            **{
                **rc.initial_r1_state(decision("PASS")).__dict__,
                "review_round": rc.MAX_SUBSTANTIVE_REVIEW_ROUNDS,
                "autonomous_repairs_remaining": 1,
            }
        )
        with self.assertRaises(rc.ConvergenceError):
            rc.after_repair_batch_consumed(prior, new_head_sha=HEAD2)

    def test_r2_pass_is_terminal_with_resolved_prior_blockers(self):
        prior = rc.after_repair_batch_consumed(
            rc.initial_r1_state(decision("BLOCKED", findings=(finding(),))),
            new_head_sha=HEAD2,
        )
        resolved = finding(
            id="F-1",
            origin_head=HEAD1,
            status="resolved",
        )
        d = decision(
            "PASS",
            review_mode="repair_verification",
            review_round=2,
            findings=(resolved,),
            prior_reviewed_head=HEAD1,
        )
        state = rc.apply_r2_decision(prior, d)
        self.assertEqual(state.verdict, "PASS")
        self.assertEqual(state.stop_reason, "")
        self.assertEqual(state.review_round, 2)
        self.assertEqual(state.autonomous_repairs_remaining, 0)

    def test_r2_prior_blocker_cannot_silently_disappear(self):
        prior = rc.after_repair_batch_consumed(
            rc.initial_r1_state(decision("BLOCKED", findings=(finding(),))),
            new_head_sha=HEAD2,
        )
        d = decision(
            "PASS",
            review_mode="repair_verification",
            review_round=2,
            findings=(),
            findings_structured=True,
            prior_reviewed_head=HEAD1,
        )
        with self.assertRaises(rc.ConvergenceError):
            rc.apply_r2_decision(prior, d)

    def test_r2_prior_blocker_legacy_pass_expands_to_resolved(self):
        prior = rc.after_repair_batch_consumed(
            rc.initial_r1_state(decision("BLOCKED", findings=(finding(),))),
            new_head_sha=HEAD2,
        )
        d = decision(
            "PASS",
            review_mode="repair_verification",
            review_round=2,
            findings=(),
            prior_reviewed_head=HEAD1,
        )
        # Legacy path: empty findings on PASS expands missing prior blockers.
        legacy_prior = rc.ReviewRoundState(
            **{**prior.__dict__, "findings": (), "finding_ledger_digest": ""}
        )
        state = rc.apply_r2_decision(legacy_prior, d)
        self.assertEqual(state.verdict, "PASS")
        self.assertEqual(state.stop_reason, "")
        resolved_ids = [f["id"] for f in state.findings]
        self.assertIn("F-1", resolved_ids)

    def test_r2_new_blocker_requires_admission_reason(self):
        prior = rc.after_repair_batch_consumed(
            rc.initial_r1_state(decision("BLOCKED", findings=(finding(),))),
            new_head_sha=HEAD2,
        )
        resolved = finding(id="F-1", status="resolved")
        regression = finding(
            id="B-2",
            origin_head=HEAD2,
            status="open",
        )
        d = decision(
            "BLOCKED",
            review_mode="repair_verification",
            review_round=2,
            findings=(resolved, regression),
            prior_reviewed_head=HEAD1,
        )
        with self.assertRaises(rc.ConvergenceError):
            rc.apply_r2_decision(prior, d)
        admitted = finding(
            id="B-2",
            origin_head=HEAD2,
            status="open",
            admission_reason="repair_regression",
        )
        d = decision(
            "BLOCKED",
            review_mode="repair_verification",
            review_round=2,
            findings=(resolved, admitted),
            prior_reviewed_head=HEAD1,
        )
        state = rc.apply_r2_decision(prior, d)
        self.assertEqual(state.verdict, "DECISION_REQUIRED")
        self.assertEqual(state.stop_reason, "decision_required")

    def test_r2_still_blocked_becomes_decision_required_no_r3(self):
        prior = rc.after_repair_batch_consumed(
            rc.initial_r1_state(decision("BLOCKED", findings=(finding(),))),
            new_head_sha=HEAD2,
        )
        still_open = finding(id="F-1", status="open")
        d = decision(
            "BLOCKED",
            review_mode="repair_verification",
            review_round=2,
            findings=(still_open,),
            prior_reviewed_head=HEAD1,
        )
        state = rc.apply_r2_decision(prior, d)
        self.assertEqual(state.verdict, "DECISION_REQUIRED")
        self.assertEqual(state.review_round, 2)
        self.assertNotEqual(state.review_round, 3)
        self.assertEqual(state.autonomous_repairs_remaining, 0)


class TestDeriveNextReviewAttempt(unittest.TestCase):
    def test_first_review_is_full_round_one(self):
        attempt = rc.derive_next_review_attempt(None, HEAD1)
        self.assertTrue(attempt["allowed"])
        self.assertEqual(attempt["review_mode"], "full")
        self.assertEqual(attempt["review_round"], 1)
        self.assertEqual(attempt["autonomous_repairs_remaining"], 1)

    def test_decision_required_denies_retry(self):
        state = rc.initial_r1_state(
            decision("DECISION_REQUIRED", findings=(finding(disposition="decision_required"),))
        ).to_persistence_fields()
        state["verdict"] = "DECISION_REQUIRED"
        state["stop_reason"] = "decision_required"
        attempt = rc.derive_next_review_attempt(state, HEAD1)
        self.assertFalse(attempt["allowed"])
        self.assertIn("human_authority", attempt["deny_reason"])

    def test_new_head_after_repair_derives_r2(self):
        prior = rc.after_repair_batch_consumed(
            rc.initial_r1_state(decision("BLOCKED", findings=(finding(),))),
            new_head_sha=HEAD2,
        )
        attempt = rc.derive_next_review_attempt(
            prior.to_persistence_fields(), HEAD2
        )
        self.assertTrue(attempt["allowed"])
        self.assertEqual(attempt["review_mode"], "repair_verification")
        self.assertEqual(attempt["review_round"], 2)
        self.assertEqual(attempt["autonomous_repairs_remaining"], 0)
        self.assertEqual(attempt["prior_reviewed_head"], HEAD1)
        self.assertEqual(attempt["open_blocker_ids"], ["F-1"])

    def test_review_repair_head_without_invalidation_consumes_batch_and_derives_r2(self):
        # F-1: a changed head after an R1 BLOCKED record (no explicit
        # invalidate_evidence call) must consume the single repair batch and
        # route the next review to R2 repair verification.
        prior = rc.initial_r1_state(
            decision("BLOCKED", findings=(finding(),))
        ).to_persistence_fields()
        attempt = rc.derive_next_review_attempt(prior, HEAD2)
        self.assertTrue(attempt["allowed"])
        self.assertEqual(attempt["review_mode"], "repair_verification")
        self.assertEqual(attempt["review_round"], 2)
        self.assertEqual(attempt["autonomous_repairs_remaining"], 0)
        self.assertEqual(attempt["prior_reviewed_head"], HEAD1)
        self.assertEqual(attempt["open_blocker_ids"], ["F-1"])

    def test_new_head_after_terminal_pass_starts_fresh_r1(self):
        prior = rc.initial_r1_state(decision("PASS")).to_persistence_fields()
        attempt = rc.derive_next_review_attempt(prior, HEAD2)
        self.assertTrue(attempt["allowed"])
        self.assertEqual(attempt["review_mode"], "full")
        self.assertEqual(attempt["review_round"], 1)
        self.assertEqual(
            attempt["autonomous_repairs_remaining"],
            rc.INITIAL_AUTONOMOUS_REPAIRS_REMAINING,
        )

    def test_same_head_pass_cannot_be_reopened_automatically(self):
        prior = rc.initial_r1_state(decision("PASS")).to_persistence_fields()
        attempt = rc.derive_next_review_attempt(prior, HEAD1)
        self.assertFalse(attempt["allowed"])
        self.assertIn("human_authority", attempt["deny_reason"])

    def test_r2_decision_required_denies_retry_without_human_authority(self):
        prior = rc.after_repair_batch_consumed(
            rc.initial_r1_state(decision("BLOCKED", findings=(finding(),))),
            new_head_sha=HEAD2,
        )
        resolved = finding(id="F-1", status="resolved")
        regression = finding(
            id="B-2",
            origin_head=HEAD2,
            status="open",
            admission_reason="hard_stop_miss",
        )
        d = decision(
            "BLOCKED",
            review_mode="repair_verification",
            review_round=2,
            findings=(resolved, regression),
            prior_reviewed_head=HEAD1,
        )
        state = rc.apply_r2_decision(prior, d).to_persistence_fields()
        attempt = rc.derive_next_review_attempt(state, HEAD2)
        self.assertFalse(attempt["allowed"])

    def test_stale_r1_head_retry_within_budget_stays_round_one(self):
        state = rc.initial_r1_state(
            decision("BLOCKED", findings=(finding(),))
        ).to_persistence_fields()
        attempt = rc.derive_next_review_attempt(state, HEAD1)
        self.assertTrue(attempt["allowed"])
        self.assertEqual(attempt["review_round"], 1)

    def test_fresh_invalidation_restarts_r1_full(self):
        # Prior PASS invalidated by a new head is NOT an R2 repair flow.
        state = rc.initial_r1_state(decision("PASS")).to_persistence_fields()
        state["verdict"] = "INVALIDATED"
        state["stop_reason"] = "awaiting_review"
        state["head_sha"] = HEAD1
        attempt = rc.derive_next_review_attempt(state, HEAD2)
        self.assertTrue(attempt["allowed"])
        self.assertEqual(attempt["review_mode"], "full")
        self.assertEqual(attempt["review_round"], 1)
        self.assertEqual(
            attempt["autonomous_repairs_remaining"],
            rc.INITIAL_AUTONOMOUS_REPAIRS_REMAINING,
        )
        self.assertEqual(attempt["prior_reviewed_head"], HEAD1)

    def test_post_repair_invalidation_derives_r2(self):
        prior = rc.after_repair_batch_consumed(
            rc.initial_r1_state(decision("BLOCKED", findings=(finding(),))),
            new_head_sha=HEAD2,
        )
        attempt = rc.derive_next_review_attempt(
            prior.to_persistence_fields(), HEAD2
        )
        self.assertTrue(attempt["allowed"])
        self.assertEqual(attempt["review_mode"], "repair_verification")
        self.assertEqual(attempt["review_round"], 2)


class TestFindingLedgerDigest(unittest.TestCase):
    def test_digest_stable_and_order_independent(self):
        first = finding(id="Z-1", severity="note", disposition="defer", status="deferred")
        second = finding(id="A-1")
        d1 = rc.ledger_digest((rc.normalize_finding(first), rc.normalize_finding(second)))
        d2 = rc.ledger_digest((rc.normalize_finding(second), rc.normalize_finding(first)))
        self.assertEqual(d1, d2)
        self.assertRegex(d1, r"^[0-9a-f]{64}$")

    def test_normalize_rejects_unknown_fields_and_bad_enums(self):
        with self.assertRaises(rc.ConvergenceError):
            rc.normalize_finding({**finding(), "severity": "critical"})
        with self.assertRaises(rc.ConvergenceError):
            rc.normalize_finding({**finding(), "mystery": True})
        with self.assertRaises(rc.ConvergenceError):
            rc.normalize_finding({**finding(), "id": "bad id with spaces"})


class TestCapsuleProjection(unittest.TestCase):
    def _state(self, version=3, **overrides):
        payload = {
            "kind": "agent-orchestrator-review-state",
            "version": version,
            "issue_number": 42,
            "pr_number": 207,
            "head_sha": HEAD1,
            "verdict": "BLOCKED",
            "summary": "blocked",
            "blockers": ["x"],
            "major_notes": [],
            "minor_notes": [],
            "artifact_sha256": "0" * 64,
            "review_workflow_run_id": 9,
            "base_sha": BASE,
            "reviewed_range": f"{BASE}...{HEAD1}",
            "review_mode": "full",
            "review_round": 1,
            "prior_reviewed_head": "",
            "findings": [finding()],
            "finding_ledger_digest": "d" * 64,
            "open_blocker_ids": ["F-1"],
            "deferred_note_ids": [],
            "decision_required_ids": [],
            "autonomous_repairs_remaining": 1,
            "stop_reason": "",
            "review_protocol_version": rc.REVIEW_PROTOCOL_VERSION,
        }
        payload.update(overrides)
        return payload

    def test_projects_only_bounded_fields(self):
        projection = rc.project_capsule_fields(self._state(), expected_head=HEAD1)
        self.assertEqual(projection["availability"], "confirmed")
        self.assertEqual(projection["review_round"], 1)
        self.assertEqual(projection["reviewed_head"], HEAD1)
        self.assertEqual(projection["open_blocker_ids"], ["F-1"])
        self.assertEqual(projection["review_state"], "BLOCKED")
        self.assertNotIn("findings", projection)
        self.assertNotIn("severity", projection)
        self.assertNotIn("acceptance_condition", projection)
        self.assertNotIn("disposition", projection)

    def test_head_mismatch_marks_conflict(self):
        projection = rc.project_capsule_fields(self._state(), expected_head=HEAD2)
        self.assertEqual(projection["availability"], "conflict")
        self.assertEqual(projection["unavailable_reason"], "review_state_head_mismatch")

    def test_unsupported_version_marks_conflict(self):
        projection = rc.project_capsule_fields(
            self._state(version=99), expected_head=HEAD1
        )
        self.assertEqual(projection["availability"], "conflict")
        self.assertEqual(
            projection["unavailable_reason"], "unsupported_review_state_version"
        )

    def test_legacy_v2_state_projects_without_convergence_fields(self):
        projection = rc.project_capsule_fields(
            self._state(version=2), expected_head=HEAD1
        )
        self.assertEqual(projection["availability"], "legacy")
        self.assertIsNone(projection["review_round"])

    def test_missing_state_is_unavailable(self):
        projection = rc.project_capsule_fields(None, expected_head=HEAD1)
        self.assertEqual(projection["availability"], "unavailable")
        self.assertEqual(projection["review_state"], "unavailable")


class TestDurablePersistenceFields(unittest.TestCase):
    def test_round_state_maps_to_v3_persistence_fields(self):
        state = rc.initial_r1_state(decision("PASS"))
        fields = state.to_persistence_fields()
        for key in (
            "review_protocol_version",
            "review_mode",
            "review_round",
            "prior_reviewed_head",
            "base_sha",
            "head_sha",
            "reviewed_range",
            "verdict",
            "findings",
            "finding_ledger_digest",
            "open_blocker_ids",
            "deferred_note_ids",
            "decision_required_ids",
            "autonomous_repairs_remaining",
            "stop_reason",
        ):
            self.assertIn(key, fields)
        self.assertEqual(fields["autonomous_repairs_remaining"], 1)
        self.assertEqual(fields["review_round"], 1)
        self.assertEqual(fields["verdict"], "PASS")


if __name__ == "__main__":
    unittest.main()
