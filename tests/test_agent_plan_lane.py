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
import artifact_contract  # noqa: E402


MAIN = "a" * 40


def packet_payload(**overrides):
    value = {
        "schema_version": "weak_agent_dispatch.v1",
        "packet_id": "TOOL-PLAN-LANE-1",
        "packet_state": "READY_FOR_EXECUTION",
        "dispatch_lane": "provider_free_repository_maintenance",
        "external_effect_limit": 0,
        "authority_consumption_allowed": False,
        "secret_values_allowed": False,
        "private_paths_allowed": False,
        "plan_lane_state": plan_lane.PLAN_LANE_ACTIVE,
        "goal": "Implement one bounded plan lane.",
        "allowed_paths": ["scripts/agent-control/", "tests/"],
        "prerequisites": [],
        "forbidden_changes": ["default branch", "provider calls"],
        "verification": ["focused provider-free tests"],
        "rollback": "disable the adapter and revert the packet",
    }
    value.update(overrides)
    return value


def document(*, packets=None, marker=True, route="TOOL-PLAN-LANE-1", marker_payload=None):
    packets = packets or [("TOOL-PLAN-LANE-1", "READY_FOR_EXECUTION")]
    blocks = []
    for packet_id, state in packets:
        block = [f"## Packet {packet_id}", f"**State:** `{state}`"]
        if marker and packet_id == "TOOL-PLAN-LANE-1":
            block.append(
                "<!-- weak-agent-dispatch:v1 "
                + json.dumps(marker_payload or packet_payload(), sort_keys=True)
                + " -->"
            )
        blocks.append("\n".join(block))
    return "\n\n".join(["## Active Routing", f"1. `{route}`", *blocks])


def expected_task_spec_sha256() -> str:
    payload = packet_payload()
    spec = {
        "schema_version": plan_lane.SCHEMA_VERSION,
        "packet_id": payload["packet_id"],
        "state": "READY_FOR_EXECUTION",
        "source_main_sha": MAIN,
        "goal": payload["goal"],
        "allowed_paths": payload["allowed_paths"],
        "prerequisites": payload["prerequisites"],
        "forbidden_changes": payload["forbidden_changes"],
        "verification": payload["verification"],
        "rollback": [payload["rollback"]],
    }
    return hashlib.sha256(
        json.dumps(spec, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()


class TestPlanLane(unittest.TestCase):
    def test_valid_marker_becomes_bounded_candidate(self):
        candidate = plan_lane.parse(document(), MAIN)
        self.assertEqual(candidate.packet_id, "TOOL-PLAN-LANE-1")
        self.assertEqual(candidate.source_main_sha, MAIN)
        self.assertEqual(candidate.branch, "agent/packet-tool-plan-lane-1")
        self.assertEqual(candidate.to_wire()["candidate_kind"], "plan")

    def test_route_manifest_hash_is_bound_into_the_execution_candidate(self):
        payload = packet_payload(route_manifest_sha256="b" * 64)
        candidate = plan_lane.parse(document(marker_payload=payload), MAIN)
        self.assertEqual(candidate.route_manifest_sha256, "b" * 64)
        self.assertEqual(candidate.to_wire()["route_manifest_sha256"], "b" * 64)
        changed = plan_lane.parse(
            document(marker_payload=packet_payload(route_manifest_sha256="c" * 64)), MAIN
        )
        self.assertNotEqual(candidate.task_spec_sha256, changed.task_spec_sha256)
        with self.assertRaisesRegex(plan_lane.PlanLaneError, "route_manifest_sha256"):
            plan_lane.parse(document(marker_payload=packet_payload(route_manifest_sha256="not-a-digest")), MAIN)

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

    def test_completed_historical_predecessor_satisfies_prerequisite(self):
        payload = packet_payload(prerequisites=["TOOL-PREREQUISITE-1"])
        text = (
            "## Active Routing\n"
            "1. `TOOL-PLAN-LANE-1`\n"
            "## Completed Route Contract (TOOL-PREREQUISITE-1)\n"
            "**Historical state:** `COMPLETE`\n"
            "## Packet TOOL-PLAN-LANE-1\n"
            "**State:** `READY_FOR_EXECUTION`\n"
            "<!-- weak-agent-dispatch:v1 "
            + json.dumps(payload, sort_keys=True)
            + " -->"
        )
        candidate = plan_lane.parse(text, MAIN)
        self.assertEqual(candidate.packet_id, "TOOL-PLAN-LANE-1")
        self.assertEqual(candidate.prerequisites, ["TOOL-PREREQUISITE-1"])

    def test_durable_current_status_receipt_satisfies_prerequisite_without_history_growth(self):
        payload = packet_payload(prerequisites=["TOOL-PREREQUISITE-1"])
        status = (
            "## Accepted Packet Receipts\n\n"
            "| Packet | State | Accepted evidence |\n|---|---|---|\n"
            "| `TOOL-PREREQUISITE-1` | `COMPLETE` | exact accepted receipt |\n"
        )
        completed = plan_lane.accepted_completed_packet_ids(status)
        candidate = plan_lane.parse(
            document(marker_payload=payload), MAIN, completed_packet_ids=completed
        )
        self.assertEqual(candidate.prerequisites, ["TOOL-PREREQUISITE-1"])
        self.assertEqual(completed, frozenset({"TOOL-PREREQUISITE-1"}))

    def test_status_receipt_index_fails_closed_when_missing(self):
        with self.assertRaisesRegex(plan_lane.PlanLaneError, "status_receipt_index_missing"):
            plan_lane.accepted_completed_packet_ids("## Capability Status\n")

    def test_incomplete_historical_predecessor_blocks_prerequisite(self):
        payload = packet_payload(prerequisites=["TOOL-PREREQUISITE-1"])
        text = (
            "## Active Routing\n"
            "1. `TOOL-PLAN-LANE-1`\n"
            "## Completed Route Contract (TOOL-PREREQUISITE-1)\n"
            "**Historical state:** `BLOCKED_PREREQUISITE`\n"
            "## Packet TOOL-PLAN-LANE-1\n"
            "**State:** `READY_FOR_EXECUTION`\n"
            "<!-- weak-agent-dispatch:v1 "
            + json.dumps(payload, sort_keys=True)
            + " -->"
        )
        with self.assertRaisesRegex(plan_lane.PlanLaneError, "dependencies_not_ready"):
            plan_lane.parse(text, MAIN)

    def test_retained_in_progress_effect_admits_only_its_direct_provider_free_closeout(self):
        effect = "PE7-EFFECT-1"
        payload = packet_payload(
            packet_id="PE7-EFFECT-CLOSEOUT-1",
            prerequisites=[effect],
        )
        request = {
            "schema_version": "route_t3_request.v1",
            "packet_id": effect,
            "accepted_main_sha": MAIN,
            "candidate_digest": "b" * 64,
            "action_digest": "c" * 64,
            "scope_digest": "d" * 64,
            "authority_owner_digest": "e" * 64,
            "requested_action": "one bounded effect",
        }
        text = (
            "## Active Routing\n1. `PE7-EFFECT-CLOSEOUT-1`\n"
            f"## Retained ({effect})\n**Historical state:** `IN_PROGRESS`\n"
            "<!-- route-t3-request:v1\n" + json.dumps(request, sort_keys=True) + "\n-->\n"
            "## Packet PE7-EFFECT-CLOSEOUT-1\n**State:** `READY_FOR_EXECUTION`\n"
            "**Class:** `CLOSEOUT`\n"
            "<!-- weak-agent-dispatch:v1 " + json.dumps(payload, sort_keys=True) + " -->"
        )
        candidate = plan_lane.parse(text, MAIN)
        self.assertEqual(candidate.packet_id, "PE7-EFFECT-CLOSEOUT-1")
        blocked = text.replace("**Class:** `CLOSEOUT`", "**Class:** `IMPLEMENT`")
        with self.assertRaisesRegex(plan_lane.PlanLaneError, "dependencies_not_ready"):
            plan_lane.parse(blocked, MAIN)

    def test_missing_fields_and_invalid_digest_fail_closed(self):
        payload = packet_payload()
        del payload["allowed_paths"]
        with self.assertRaisesRegex(plan_lane.PlanLaneError, "allowed_paths"):
            plan_lane.parse(document(marker_payload=payload), MAIN)
        payload = packet_payload(plan_lane_state="plan_lane_unknown")
        with self.assertRaisesRegex(plan_lane.PlanLaneError, "lane_state"):
            plan_lane.parse(document(marker_payload=payload), MAIN)

    def test_bootstrap_reader_can_only_read_a_workflow_scope(self):
        payload = packet_payload(
            allowed_paths=[".github/workflows/agent-controller.yml", "tests/"]
        )
        with self.assertRaisesRegex(plan_lane.PlanLaneError, "allowed_paths"):
            plan_lane.parse(document(marker_payload=payload), MAIN)
        candidate = plan_lane.parse_bootstrap(document(marker_payload=payload), MAIN)
        self.assertEqual(candidate.allowed_paths, payload["allowed_paths"])
        with self.assertRaises(artifact_contract.ArtifactContractError):
            artifact_contract.validate_allowed_paths([".github/workflows/agent-controller.yml"])
        payload["allowed_paths"] = [".github/workflows/"]
        with self.assertRaisesRegex(plan_lane.PlanLaneError, "allowed_paths"):
            plan_lane.parse_bootstrap(document(marker_payload=payload), MAIN)
        payload["allowed_paths"] = [".github/workflows/../outside.yml"]
        with self.assertRaisesRegex(plan_lane.PlanLaneError, "allowed_paths"):
            plan_lane.parse_bootstrap(document(marker_payload=payload), MAIN)

    def test_deferred_lane_and_nonzero_effect_fail_closed(self):
        payload = packet_payload(plan_lane_state=plan_lane.PLAN_LANE_DEFERRED)
        with self.assertRaisesRegex(
            plan_lane.PlanLaneError, "plan_lane_deferred_until_terminal_owners"
        ):
            plan_lane.parse(document(marker_payload=payload), MAIN)
        payload = packet_payload(external_effect_limit=1)
        with self.assertRaisesRegex(plan_lane.PlanLaneError, "external_effect"):
            plan_lane.parse(document(marker_payload=payload), MAIN)

    def test_unmet_prerequisite_fails_closed(self):
        payload = packet_payload(prerequisites=["TOOL-PREREQUISITE-1"])
        with self.assertRaisesRegex(plan_lane.PlanLaneError, "dependencies_not_ready"):
            plan_lane.parse(document(marker_payload=payload), MAIN)

    def test_plan_marker_in_non_current_packet_is_rejected(self):
        with self.assertRaisesRegex(plan_lane.PlanLaneError, "current_route"):
            plan_lane.parse(document(route="TOOL-OTHER-1"), MAIN)

    def test_terminal_owner_readiness_fails_closed_on_missing_owners(self):
        ready, missing = plan_lane.terminal_owner_readiness(
            ledger_issue=0,
            canonical_tests_workflow_present=False,
            ci_monitor_workflow_present=False,
            review_owner_present=False,
            merge_owner_present=False,
            closeout_owner_present=False,
        )
        self.assertFalse(ready)
        self.assertEqual(
            missing,
            [
                "plan_execution_ledger",
                "canonical_tests_workflow",
                "ci_monitor_workflow",
                "review_owner",
                "merge_owner",
                "closeout_owner",
            ],
        )

    def test_terminal_owner_readiness_passes_only_when_all_owners_ready(self):
        ready, missing = plan_lane.terminal_owner_readiness(
            ledger_issue=900,
            canonical_tests_workflow_present=True,
            ci_monitor_workflow_present=True,
            review_owner_present=True,
            merge_owner_present=True,
            closeout_owner_present=True,
        )
        self.assertTrue(ready)
        self.assertEqual(missing, [])

    def test_poll_rejects_plan_candidates_until_terminal_owners_exist(self):
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

            def accepted_status_document(self, _sha):
                return (
                    "## Accepted Packet Receipts\n\n"
                    "| Packet | State | Accepted evidence |\n|---|---|---|\n"
                )

            def plan_ledger_issue(self):
                raise local_loop.LoopUnavailable("plan execution ledger is unavailable")

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
                and item.get("reason") == (
                    "plan_lane_not_ready:plan_execution_ledger,"
                    "canonical_tests_workflow,ci_monitor_workflow,"
                    "review_owner,merge_owner,closeout_owner"
                )
                for item in rejected
            ),
            rejected,
        )

    def test_poll_admits_plan_candidate_when_terminal_owners_ready(self):
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

            def accepted_status_document(self, _sha):
                return (
                    "## Accepted Packet Receipts\n\n"
                    "| Packet | State | Accepted evidence |\n|---|---|---|\n"
                )

            def plan_ledger_issue(self):
                return 900

            def list_ready_issues(self):
                return []

        worktree = Path("/tmp")
        for name in (
            ".github/workflows/tests.yml",
            ".github/workflows/agent-ci-monitor.yml",
            "scripts/agent-control/review_loop_cli.py",
            "docs/REAL_WORLD_TESTING_PLAYBOOK.md",
            "docs/CURRENT_STATUS.md",
        ):
            path = worktree / name
            path.parent.mkdir(parents=True, exist_ok=True)
            if not path.exists():
                path.write_text("", encoding="utf-8")
        try:
            decision = local_loop.LoopController(
                GitHub(), Git(), repository="acme/repo", repo_path=worktree
            ).poll()
            self.assertEqual(decision["status"], "ready")
            selected = decision.get("selected") or []
            self.assertTrue(
                any(
                    item.get("candidate_kind") == "plan"
                    and item.get("subject_id") == "TOOL-PLAN-LANE-1"
                    for item in selected
                ),
                decision,
            )
        finally:
            for name in (
                ".github/workflows/tests.yml",
                ".github/workflows/agent-ci-monitor.yml",
                "scripts/agent-control/review_loop_cli.py",
                "docs/REAL_WORLD_TESTING_PLAYBOOK.md",
                "docs/CURRENT_STATUS.md",
            ):
                path = worktree / name
                if path.exists():
                    path.unlink()

    def test_active_plan_capacity_deduplicates_claim_generations(self):
        ledger_issue = 900
        dispatch_id = f"plan-run:TOOL-PLAN-LANE-1:{MAIN}:123e4567-e89b-12d3-a456-426614174000"
        details = {
            "ledger_issue_number": ledger_issue,
            "subject_kind": "plan-packet",
            "subject_id": "TOOL-PLAN-LANE-1",
            "source_main_sha": MAIN,
            "task_spec_sha256": expected_task_spec_sha256(),
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
            "task_spec_sha256": expected_task_spec_sha256(),
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
                "task_spec_sha256": expected_task_spec_sha256(),
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

    def test_implement_packet_rejects_doc_only_allowed_paths(self):
        payload = packet_payload(
            packet_id="PE7-AC4-VIEWS-CORE-1",
            allowed_paths=["docs/NEXT_DECISION.md", "docs/CURRENT_STATUS.md"],
        )
        packet_block = [
            "## Packet PE7-AC4-VIEWS-CORE-1",
            "**State:** `READY_FOR_EXECUTION`",
            "**Class:** `IMPLEMENT`",
            "<!-- weak-agent-dispatch:v1 " + json.dumps(payload, sort_keys=True) + " -->",
        ]
        doc = "\n\n".join(["## Active Routing", "1. `PE7-AC4-VIEWS-CORE-1`", "\n".join(packet_block)])
        with self.assertRaisesRegex(plan_lane.PlanLaneError, "plan_implement_allowed_paths_lack_source"):
            plan_lane.parse(doc, MAIN)

    def test_generic_ac4_capsule_without_views_source_is_rejected(self):
        payload = packet_payload(
            packet_id="PE7-AC4-VIEWS-CORE-1",
            allowed_paths=["docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/NEXT_DECISION.md"],
        )
        packet_block = [
            "## Packet PE7-AC4-VIEWS-CORE-1",
            "**State:** `READY_FOR_EXECUTION`",
            "**Class:** `IMPLEMENT`",
            "<!-- weak-agent-dispatch:v1 " + json.dumps(payload, sort_keys=True) + " -->",
        ]
        doc = "\n\n".join(["## Active Routing", "1. `PE7-AC4-VIEWS-CORE-1`", "\n".join(packet_block)])
        with self.assertRaises(plan_lane.PlanLaneError) as ctx:
            plan_lane.parse(doc, MAIN)
        self.assertEqual(ctx.exception.reason, "plan_implement_allowed_paths_lack_source")

    def test_contract_packet_accepts_doc_allowed_paths(self):
        payload = packet_payload(
            packet_id="PE7-AC4-CONTRACT-1",
            allowed_paths=["docs/NEXT_DECISION.md", "docs/CURRENT_STATUS.md"],
        )
        packet_block = [
            "## Packet PE7-AC4-CONTRACT-1",
            "**State:** `READY_FOR_EXECUTION`",
            "**Class:** `CONTRACT`",
            "<!-- weak-agent-dispatch:v1 " + json.dumps(payload, sort_keys=True) + " -->",
        ]
        doc = "\n\n".join(["## Active Routing", "1. `PE7-AC4-CONTRACT-1`", "\n".join(packet_block)])
        candidate = plan_lane.parse(doc, MAIN)
        self.assertEqual(candidate.packet_id, "PE7-AC4-CONTRACT-1")


if __name__ == "__main__":
    unittest.main()
