"""Provider-free tests for the accepted-main plan candidate contract."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import sys
import unittest
from unittest import mock


CONTROL = Path(__file__).resolve().parents[1] / "scripts" / "agent-control"
sys.path.insert(0, str(CONTROL))

import plan_lane  # noqa: E402
import local_loop  # noqa: E402
import state_manager  # noqa: E402
import control_state  # noqa: E402


MAIN = "a" * 40


def packet_payload(**overrides):
    value = {
        "schema_version": 1,
        "packet_id": "TOOL-PLAN-LANE-1",
        "state": "READY_FOR_EXECUTION",
        "source_main_sha": MAIN,
        "goal": "Implement one bounded plan lane.",
        "allowed_paths": ["scripts/agent-control/", "tests/"],
        "prerequisites": [],
        "forbidden_changes": ["default branch", "provider calls"],
        "verification": ["focused provider-free tests"],
        "rollback": ["disable the adapter and revert the packet"],
    }
    value.update(overrides)
    digest = hashlib.sha256(
        json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()
    return {**value, "task_spec_sha256": digest}


def document(*, packets=None, marker=True, route="TOOL-PLAN-LANE-1", marker_payload=None):
    packets = packets or [("TOOL-PLAN-LANE-1", "READY_FOR_EXECUTION")]
    blocks = []
    for packet_id, state in packets:
        block = [f"## Packet {packet_id}", f"**State:** `{state}`"]
        if marker and packet_id == "TOOL-PLAN-LANE-1":
            block.append(
                "<!-- agent-orchestrator-plan:v1 "
                + json.dumps(marker_payload or packet_payload(), sort_keys=True)
                + " -->"
            )
        blocks.append("\n".join(block))
    return "\n\n".join(["## Active Routing", f"1. `{route}`", *blocks])


class TestPlanLane(unittest.TestCase):
    def test_valid_marker_becomes_bounded_candidate(self):
        candidate = plan_lane.parse(document(), MAIN)
        self.assertEqual(candidate.packet_id, "TOOL-PLAN-LANE-1")
        self.assertEqual(candidate.source_main_sha, MAIN)
        self.assertEqual(candidate.branch, "agent/packet-tool-plan-lane-1")
        self.assertEqual(candidate.to_wire()["candidate_kind"], "plan")

    def test_absent_plan_marker_is_not_a_candidate(self):
        self.assertIsNone(
            plan_lane.parse_optional(
                document(packets=[("TOOL-PLAN-LANE-1", "IN_PROGRESS")], marker=False),
                MAIN,
            )
        )

    def test_multiple_ready_packets_fail_closed(self):
        with self.assertRaisesRegex(plan_lane.PlanLaneError, "multiple_plan_packets"):
            plan_lane.parse(
                document(
                    packets=[
                        ("TOOL-PLAN-LANE-1", "READY_FOR_EXECUTION"),
                        ("TOOL-PLAN-LANE-2", "READY_FOR_EXECUTION"),
                    ]
                ),
                MAIN,
            )

    def test_missing_fields_and_digest_mismatch_fail_closed(self):
        payload = packet_payload()
        del payload["allowed_paths"]
        with self.assertRaisesRegex(plan_lane.PlanLaneError, "fields_missing"):
            plan_lane.parse(document(marker_payload=payload), MAIN)
        payload = packet_payload(task_spec_sha256="0" * 64)
        with self.assertRaisesRegex(plan_lane.PlanLaneError, "digest_mismatch"):
            plan_lane.parse(document(marker_payload=payload), MAIN)

    def test_unmet_prerequisite_and_main_mismatch_fail_closed(self):
        payload = packet_payload(prerequisites=["TOOL-PREREQUISITE-1"])
        with self.assertRaisesRegex(plan_lane.PlanLaneError, "dependencies_not_ready"):
            plan_lane.parse(document(marker_payload=payload), MAIN)
        payload = packet_payload(source_main_sha="b" * 40)
        with self.assertRaisesRegex(plan_lane.PlanLaneError, "accepted_main_mismatch"):
            plan_lane.parse(document(marker_payload=payload), MAIN)

    def test_plan_marker_in_non_current_packet_is_rejected(self):
        with self.assertRaisesRegex(plan_lane.PlanLaneError, "current_route"):
            plan_lane.parse(document(route="TOOL-OTHER-1"), MAIN)

    def test_poll_defers_plan_candidates_until_terminal_owners_exist(self):
        class Git:
            def origin_main_sha(self, _repo_path, _branch):
                return MAIN

        class GitHub:
            def read_control_state(self):
                return {"orchestrator_enabled": True, "emergency_stop": False}

            def repository_metadata(self):
                return {"name_with_owner": "acme/repo", "owner": "acme", "default_branch": "main"}

            def current_user(self):
                return "acme"

            def accepted_main_sha(self, _branch):
                return MAIN

            def active_execution_scopes(self):
                return {"issue_numbers": set(), "plans": [], "scopes": {}}

            def accepted_plan_document(self, _sha):
                return document()

            def plan_ledger_issue(self):
                return 900

            def list_ready_issues(self):
                return []

        decision = local_loop.LoopController(
            GitHub(), Git(), repository="acme/repo", repo_path=Path("/tmp")
        ).poll()
        self.assertEqual(decision["status"], "no_eligible_task")
        rejected = decision.get("rejected") or []
        self.assertTrue(
            any(
                item.get("candidate_kind") == "plan"
                and item.get("subject_id") == "TOOL-PLAN-LANE-1"
                and item.get("reason") == "plan_lane_deferred_until_terminal_owners"
                for item in rejected
            ),
            rejected,
        )

    def test_active_plan_capacity_deduplicates_claim_generations(self):
        ledger_issue = 900
        dispatch_id = f"plan-run:TOOL-PLAN-LANE-1:{MAIN}:123e4567-e89b-12d3-a456-426614174000"
        details = {
            "ledger_issue_number": ledger_issue,
            "subject_kind": "plan-packet",
            "subject_id": "TOOL-PLAN-LANE-1",
            "source_main_sha": MAIN,
            "task_spec_sha256": packet_payload()["task_spec_sha256"],
            "allowed_paths": ["scripts/agent-control/", "tests/"],
            "canonical_branch": "agent/packet-tool-plan-lane-1",
            "attempt_id": "123e4567-e89b-12d3-a456-426614174000",
            "execution_token": "b" * 32,
            "claim_nonce": "c" * 32,
            "target_label": state_manager.LABEL_RUNNING,
        }

        def state(status):
            return {
                "kind": "agent-orchestrator-dispatch-state",
                "version": 1,
                "issue_number": ledger_issue,
                "dispatch_id": dispatch_id,
                "action": "plan-run",
                "status": status,
                "details": details,
            }

        comments = [
            {"author": {"login": "github-actions[bot]"}, "body": json.dumps(state("dispatched"))},
            {"author": {"login": "github-actions[bot]"}, "body": json.dumps(state("claimed"))},
        ]
        with mock.patch.object(
            control_state, "read_plan_ledger", return_value={"number": ledger_issue}
        ), mock.patch.object(state_manager, "get_issue_comments", return_value=comments):
            active = state_manager.get_active_plan_claims("acme/repo")
        self.assertEqual(len(active), 1)
        self.assertEqual(active[0]["dispatch_id"], dispatch_id)

    def test_terminal_plan_state_shadows_older_active_generation(self):
        ledger_issue = 900
        dispatch_id = f"plan-run:TOOL-PLAN-LANE-1:{MAIN}:123e4567-e89b-12d3-a456-426614174000"
        details = {
            "ledger_issue_number": ledger_issue,
            "subject_kind": "plan-packet",
            "subject_id": "TOOL-PLAN-LANE-1",
            "source_main_sha": MAIN,
            "task_spec_sha256": packet_payload()["task_spec_sha256"],
            "allowed_paths": ["scripts/agent-control/", "tests/"],
            "canonical_branch": "agent/packet-tool-plan-lane-1",
            "attempt_id": "123e4567-e89b-12d3-a456-426614174000",
            "execution_token": "b" * 32,
            "claim_nonce": "c" * 32,
            "target_label": state_manager.LABEL_RUNNING,
        }

        def state(status):
            return {
                "kind": "agent-orchestrator-dispatch-state",
                "version": 1,
                "issue_number": ledger_issue,
                "dispatch_id": dispatch_id,
                "action": "plan-run",
                "status": status,
                "details": details,
            }

        comments = [
            {"author": {"login": "github-actions[bot]"}, "body": json.dumps(state("failed"))},
            {"author": {"login": "github-actions[bot]"}, "body": json.dumps(state("dispatched"))},
        ]
        with mock.patch.object(
            control_state, "read_plan_ledger", return_value={"number": ledger_issue}
        ), mock.patch.object(state_manager, "get_issue_comments", return_value=comments):
            self.assertEqual(state_manager.get_active_plan_claims("acme/repo"), [])

    def test_empty_label_transition_does_not_send_an_empty_label(self):
        with mock.patch.object(state_manager, "_gh", return_value="") as gh:
            self.assertTrue(state_manager.set_labels(900, repo="acme/repo"))
        args = gh.call_args.args
        self.assertNotIn("--add-label", args)

    def test_generic_trusted_claim_verifier_accepts_plan_ledger_binding(self):
        ledger_issue = 900
        attempt = "123e4567-e89b-12d3-a456-426614174000"
        dispatch_id = f"plan-run:TOOL-PLAN-LANE-1:{MAIN}:{attempt}"
        state = {
            "kind": "agent-orchestrator-dispatch-state",
            "version": 1,
            "issue_number": ledger_issue,
            "dispatch_id": dispatch_id,
            "action": "plan-run",
            "status": "dispatched",
            "details": {
                "ledger_issue_number": ledger_issue,
                "subject_kind": "plan-packet",
                "subject_id": "TOOL-PLAN-LANE-1",
                "source_main_sha": MAIN,
                "task_spec_sha256": packet_payload()["task_spec_sha256"],
                "allowed_paths": ["scripts/agent-control/", "tests/"],
                "canonical_branch": "agent/packet-tool-plan-lane-1",
                "attempt_id": attempt,
                "execution_token": "b" * 32,
                "claim_nonce": "c" * 32,
                "target_label": state_manager.LABEL_RUNNING,
                "lease_deadline": "2999-01-01T00:00:00Z",
            },
        }
        with mock.patch.object(state_manager, "read_dispatch_state", return_value=state):
            ok, value = state_manager.verify_trusted_worker_claim(
                ledger_issue, dispatch_id, "c" * 32, "acme/repo"
            )
        self.assertTrue(ok)
        self.assertEqual(value["action"], "plan-run")


if __name__ == "__main__":
    unittest.main()
