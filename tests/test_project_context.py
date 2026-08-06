import importlib.util
from pathlib import Path
import sys
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "project_context.py"
SPEC = importlib.util.spec_from_file_location("project_context", SCRIPT)
assert SPEC and SPEC.loader
project_context = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = project_context
SPEC.loader.exec_module(project_context)


class TestProjectContextRouting(unittest.TestCase):
    def test_ready_live_packet_does_not_infer_pr_from_prerequisites(self):
        text = """\
## Active Routing

1. `PE7-PRODUCT-GOLDEN-PATH-DEEPSEEK-LIVE-SEAL-1` — `READY_FOR_EXECUTION`: satisfied by PRs #339/#340 and #342.

## Packet PE7-PRODUCT-GOLDEN-PATH-DEEPSEEK-LIVE-SEAL-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** Packets A and B are accepted by PR #342.
"""

        parsed = project_context.parse_first_routed_packet(text)

        self.assertEqual(
            parsed,
            {
                "packet": "PE7-PRODUCT-GOLDEN-PATH-DEEPSEEK-LIVE-SEAL-1",
                "state": "READY_FOR_EXECUTION",
                "pr_number": None,
            },
        )

    def test_explicit_owned_pr_remains_active_pr_binding(self):
        text = """\
## Active Routing

1. `PE7-TEST-1` — `IN_PROGRESS`

## Packet PE7-TEST-1

**State:** `IN_PROGRESS`

**Owned PR:** #342
"""

        parsed = project_context.parse_first_routed_packet(text)

        self.assertEqual(parsed["packet"], "PE7-TEST-1")
        self.assertEqual(parsed["state"], "IN_PROGRESS")
        self.assertEqual(parsed["pr_number"], "342")

    def test_ready_packet_without_pr_does_not_infer_implementation_pr(self):
        action = project_context.next_permitted_action(
            {
                "packet": "PE7-PRODUCT-GOLDEN-PATH-DEEPSEEK-LIVE-SEAL-1",
                "state": "READY_FOR_EXECUTION",
            },
            None,
        )

        self.assertIn("documented prerequisites", action)
        self.assertNotIn("create or continue one focused PR", action)

    def test_in_progress_prerequisite_prose_does_not_use_legacy_fallback(self):
        text = """\
## Active Routing

1. `PE7-TEST-1` — `IN_PROGRESS`

## Packet PE7-TEST-1

**State:** `IN_PROGRESS`

Prerequisite: PR #342 is accepted.
"""

        self.assertIsNone(project_context.parse_first_routed_packet(text)["pr_number"])


class TestReviewStateProjection(unittest.TestCase):
    def test_offline_projection_is_unavailable_with_bounded_keys(self):
        pr = project_context.load_pr(
            "Igzela/token-efficient-agent-harness-lab", 364, offline=True
        )
        projection = pr["review_state_projection"]
        self.assertEqual(projection["availability"], "unavailable")
        for key in (
            "review_protocol_version",
            "review_mode",
            "review_round",
            "prior_reviewed_head",
            "reviewed_head",
            "finding_ledger_digest",
            "open_blocker_ids",
            "deferred_note_ids",
            "autonomous_repairs_remaining",
            "stop_reason",
            "review_state",
        ):
            self.assertIn(key, projection)

    def test_linked_issue_not_found_marks_unavailable(self):
        projection = project_context._load_review_state_projection(
            "Igzela/token-efficient-agent-harness-lab",
            {"headRefOid": "a" * 40, "body": "no linked issue marker"},
        )
        self.assertEqual(projection["availability"], "unavailable")
        self.assertEqual(projection["unavailable_reason"], "linked_issue_not_found")

    def test_missing_head_marks_unavailable(self):
        projection = project_context._load_review_state_projection(
            "Igzela/token-efficient-agent-harness-lab",
            {"body": "Closes #1"},
        )
        self.assertEqual(projection["availability"], "unavailable")

    def test_capsule_binding_includes_review_state_projection(self):
        payload = project_context.build_capsule(
            offline=True,
            repository="Igzela/token-efficient-agent-harness-lab",
        )
        projection = payload["binding"]["review_state_projection"]
        self.assertIsInstance(projection, dict)
        self.assertIn("availability", projection)
        self.assertIn("review_state", projection)
        for forbidden in ("findings", "severity", "acceptance_condition", "disposition"):
            self.assertNotIn(forbidden, projection)

    def test_project_capsule_fields_conflict_and_legacy_paths(self):
        import sys as _sys

        _sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts" / "agent-control"))
        import review_convergence as rc

        v3_state = {
            "kind": "agent-orchestrator-review-state",
            "version": 3,
            "issue_number": 42,
            "pr_number": 207,
            "head_sha": "a" * 40,
            "verdict": "PASS",
            "summary": "ok",
            "base_sha": "c" * 40,
            "reviewed_range": f"{'c' * 40}...{'a' * 40}",
            "review_mode": "full",
            "review_round": 1,
            "findings": [],
            "finding_ledger_digest": "0" * 64,
            "open_blocker_ids": [],
            "deferred_note_ids": [],
            "decision_required_ids": [],
            "autonomous_repairs_remaining": 1,
            "stop_reason": "",
            "review_protocol_version": rc.REVIEW_PROTOCOL_VERSION,
        }
        confirmed = rc.project_capsule_fields(v3_state, expected_head="a" * 40)
        self.assertEqual(confirmed["availability"], "confirmed")
        conflict = rc.project_capsule_fields(v3_state, expected_head="b" * 40)
        self.assertEqual(conflict["availability"], "conflict")
        legacy = rc.project_capsule_fields({**v3_state, "version": 2}, expected_head="a" * 40)
        self.assertEqual(legacy["availability"], "legacy")
        self.assertIsNone(legacy["review_round"])


if __name__ == "__main__":
    unittest.main()
