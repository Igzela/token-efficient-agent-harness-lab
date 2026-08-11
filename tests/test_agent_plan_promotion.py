"""Provider-free tests for plan-lane successor promotion and escalation."""

from __future__ import annotations

import json
import os
from datetime import datetime, timedelta, timezone
from pathlib import Path
import re
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
import pr_binding  # noqa: E402
import route_driver  # noqa: E402
import state_manager  # noqa: E402

LEDGER = 900
MAIN = "a" * 40
ATTEMPT = "123e4567-e89b-12d3-a456-426614174000"
CLOSED = "PE7-LIFECYCLE-CONTROLLER-1"
SUCCESSOR = "PE7-SUCCESSOR-PROMOTION-ESCALATION-1"
DISPATCH_ID = f"plan-run:{CLOSED}:{MAIN}:{ATTEMPT}"

CLOSED_DETAILS = {
    "ledger_issue_number": LEDGER,
    "subject_kind": "plan-packet",
    "subject_id": CLOSED,
    "source_main_sha": MAIN,
    "task_spec_sha256": "d" * 64,
    "allowed_paths": ["scripts/agent-control/", "tests/"],
    "canonical_branch": "agent/packet-pe7-lifecycle-controller-1",
    "attempt_id": ATTEMPT,
    "execution_token": "b" * 32,
    "claim_nonce": "c" * 32,
    "target_label": state_manager.LABEL_RUNNING,
    "terminal_packet_state": "closed_out",
    "closeout_reference": "PR #42",
}


def closed_claim(status="closed_out"):
    return {
        "kind": "agent-orchestrator-dispatch-state",
        "version": 1,
        "issue_number": LEDGER,
        "dispatch_id": DISPATCH_ID,
        "action": "plan-run",
        "status": status,
        "details": dict(CLOSED_DETAILS),
    }


def packet_payload(packet_id=SUCCESSOR, state="READY_FOR_EXECUTION", lane=plan_lane.PLAN_LANE_ACTIVE):
    return {
        "schema_version": "weak_agent_dispatch.v1",
        "packet_id": packet_id,
        "packet_state": state,
        "dispatch_lane": "provider_free_repository_maintenance",
        "external_effect_limit": 0,
        "authority_consumption_allowed": False,
        "secret_values_allowed": False,
        "private_paths_allowed": False,
        "plan_lane_state": lane,
        "goal": "Promote exactly one successor.",
        "allowed_paths": ["scripts/agent-control/", "tests/"],
        "prerequisites": [],
        "forbidden_changes": ["default branch", "provider calls"],
        "verification": ["focused provider-free tests"],
        "rollback": "disable the adapter and revert the packet",
    }


def routing_document(closed=CLOSED, successor=SUCCESSOR, closed_historical=True):
    """Build a NEXT_DECISION-shaped routing document with one current packet."""

    blocks = []
    if closed_historical:
        blocks.append(
            "## Completed (PE7-LIFECYCLE-CONTROLLER-1)\n\n"
            "**Historical state:** `COMPLETE`\n"
        )
    else:
        blocks.append(
            "## Completed (PE7-PLAN-LANE-ACTIVATION-1)\n\n"
            "**Historical state:** `COMPLETE`\n"
        )
    blocks.append(
        f"## Packet {successor}\n\n**State:** `READY_FOR_EXECUTION`\n\n"
        f"<!-- weak-agent-dispatch:v1 {json.dumps(packet_payload(successor), sort_keys=True)} -->\n"
    )
    return "\n\n".join([
        "## Active Routing",
        f"1. `{successor}`",
        *blocks,
    ])


def _inventory_payload(future_text):
    import hashlib as _hashlib
    import route_driver

    ordered = []
    graph = []
    rows = []
    headings = list(plan_lane.PACKET_HEADING.finditer(future_text))
    for index, match in enumerate(headings):
        packet_id = match.group("packet")
        end = headings[index + 1].start() if index + 1 < len(headings) else len(future_text)
        block = future_text[match.start() : end]
        ordered.append(packet_id)
        prerequisite = re.search(r"^\*\*Prerequisite:\*\*\s*(?P<value>.+)$", block, re.MULTILINE)
        prerequisites = (
            re.findall(plan_lane.PACKET_TOKEN, prerequisite.group("value"))
            if prerequisite
            else []
        )
        graph.append({
            "packet_id": packet_id,
            "prerequisites": [item for item in dict.fromkeys(prerequisites) if item != packet_id],
        })
        packet_class = re.search(
            r"^\*\*Class:\*\*\s*`?(?P<value>[A-Z]+)`?\s*$", block, re.MULTILINE
        ).group("value")
        rows.append([
            packet_id,
            packet_class,
            route_driver.CLASS_DEFAULT_TIER[packet_class],
            route_driver.CLASS_DEFAULT_RISK.get(packet_class, "none"),
            route_driver.CLASS_DEFAULT_VERIFICATION[packet_class],
        ])

    def digest(value):
        return _hashlib.sha256(
            json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")
        ).hexdigest()

    return {
        "schema_version": "future_route_inventory.v1",
        "packet_count": len(ordered),
        "ordered_packet_ids": ordered,
        "ordered_packet_ids_sha256": digest(ordered),
        "dependency_graph_sha256": digest(graph),
        "profiles_sha256": digest(rows),
        "profiles": rows,
    }


FUTURE_SKETCHES = {
    SUCCESSOR: """### Packet PE7-SUCCESSOR-PROMOTION-ESCALATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-PLAN-LANE-ACTIVATION-1

**Class:** `IMPLEMENT`

**Outcome:** Wire plan-lane successor promotion and escalation through the existing owners with exactly-one promotion receipts and bounded pause escalation.

**Allowed delta:** scripts/agent-control/dispatcher.py, scripts/agent-control/local_run_once.py, scripts/agent-control/route_driver.py, scripts/session_context.py, tests/test_agent_plan_promotion.py.

**Exit:** The lane records exactly-one successor-promotion receipts and bounded pause escalation, controller-owned and idempotent.

**Stop:** Any second ledger/controller/store/state/routing owner, promotion of zero or multiple successors, stale or unprovable routing, or child authority.
""",
    "TOOL-OTHER-1": """### Packet TOOL-OTHER-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-SUCCESSOR-PROMOTION-ESCALATION-1

**Class:** `CONTRACT`

**Outcome:** Document the route contract only, with no product authority change.

**Allowed delta:** docs/ only.

**Exit:** Accepted documentation records the single route-controller owner and the current blocker truth.

**Stop:** Any second owner, unbound merge path, or requirement to choose a schema value.
""",
}


def future_route_document(promoted=SUCCESSOR, only_blocked=False):
    """Build a FUTURE_ROUTE-shaped index whose manifest matches its prose."""

    text = "## Portfolio Inventory Manifest\n\n"
    if only_blocked:
        sketches = [FUTURE_SKETCHES["TOOL-OTHER-1"]]
    else:
        sketches = [FUTURE_SKETCHES[promoted]]
        for packet_id in sorted(set(FUTURE_SKETCHES) - {promoted}):
            sketches.append(FUTURE_SKETCHES[packet_id])
    text += "\n".join(sketches)
    payload = _inventory_payload(text)
    marker = "<!-- future-route-inventory:v1\n" + json.dumps(payload, sort_keys=True) + "\n-->"
    return marker + "\n\n" + text


def status_document():
    return (
        "## Accepted Packet Receipts\n\n"
        "| Packet | State | Accepted evidence |\n"
        "|---|---|---|\n\n"
        "## Accepted Readiness Boundary\n\n"
        "| Capability | State | Entry or exit condition |\n"
        "|---|---|---|\n"
        "| Repository-maintenance route contract | `COMPLETE` | PR #380 accepted |\n"
    )


def derived_spec_digest(packet_id=SUCCESSOR):
    return plan_lane._canonical_spec({
        "schema_version": plan_lane.SCHEMA_VERSION,
        "packet_id": packet_id,
        "state": "READY_FOR_EXECUTION",
        "source_main_sha": MAIN,
        "goal": "Promote exactly one successor.",
        "allowed_paths": ["scripts/agent-control/", "tests/"],
        "prerequisites": [],
        "forbidden_changes": ["default branch", "provider calls"],
        "verification": ["focused provider-free tests"],
        "rollback": ["disable the adapter and revert the packet"],
    })


class TestSuccessorBinding(unittest.TestCase):
    def test_exactly_one_eligible_successor_binds_packet_and_digest(self):
        document = routing_document()
        packet_id, digest = plan_lane.successor_binding(document, CLOSED, MAIN)
        self.assertEqual(packet_id, SUCCESSOR)
        self.assertEqual(digest, derived_spec_digest())

    def test_closed_packet_still_current_fails_closed(self):
        document = routing_document(successor=CLOSED, closed_historical=False)
        with self.assertRaises(plan_lane.PlanLaneError) as ctx:
            plan_lane.successor_binding(document, CLOSED, MAIN)
        self.assertEqual(ctx.exception.reason, "successor_still_current")

    def test_absent_successor_fails_closed(self):
        document = "\n\n".join([
            "## Active Routing",
            "1. `" + CLOSED + "`",
            "## Completed (PE7-LIFECYCLE-CONTROLLER-1)\n\n"
            "**Historical state:** `COMPLETE`\n",
        ])
        with self.assertRaises(plan_lane.PlanLaneError) as ctx:
            plan_lane.successor_binding(document, CLOSED, MAIN)
        self.assertEqual(ctx.exception.reason, "plan_packet_absent")

    def test_multiple_candidates_fail_closed(self):
        document = "\n\n".join([
            "## Active Routing",
            "1. `" + SUCCESSOR + "`",
            f"## Packet {SUCCESSOR}\n\n**State:** `READY_FOR_EXECUTION`\n\n"
            f"<!-- weak-agent-dispatch:v1 {json.dumps(packet_payload(SUCCESSOR), sort_keys=True)} -->\n",
            f"## Packet TOOL-OTHER-1\n\n**State:** `READY_FOR_EXECUTION`\n\n"
            f"<!-- weak-agent-dispatch:v1 {json.dumps(packet_payload('TOOL-OTHER-1'), sort_keys=True)} -->\n",
        ])
        with self.assertRaises(plan_lane.PlanLaneError) as ctx:
            plan_lane.successor_binding(document, CLOSED, MAIN)
        self.assertEqual(ctx.exception.reason, "multiple_plan_packets")

    def test_invalid_closed_packet_id_fails_closed(self):
        with self.assertRaises(plan_lane.PlanLaneError) as ctx:
            plan_lane.successor_binding(routing_document(), "bad id", MAIN)
        self.assertEqual(ctx.exception.reason, "successor_closed_packet_invalid")


class TestPromotePlanDispatcher(unittest.TestCase):
    def _patch(self, claim=None, routing=None, routing_error=None):
        patches = [
            mock.patch.object(dispatcher, "_repo", return_value="acme/repo"),
            mock.patch.object(dispatcher.control_state, "require_live", return_value=None),
            mock.patch.object(
                dispatcher.control_state, "read_plan_ledger",
                return_value={"number": LEDGER},
            ),
            mock.patch.object(
                plan_lifecycle, "_exact_plan_claim",
                return_value=claim if claim is not None else closed_claim(),
            ),
            mock.patch.object(
                dispatcher, "_live_routing_document",
                return_value=routing or (MAIN, routing_document(), status_document()),
            ),
        ]
        if routing_error is not None:
            def _raise(_document, _closed, _main, **_kwargs):
                raise plan_lane.PlanLaneError(routing_error)

            patches.append(mock.patch.object(
                plan_lane, "successor_binding", side_effect=_raise
            ))
        return patches

    def _run(self, patches):
        for patch in patches:
            patch.start()
        self.addCleanup(lambda: [patch.stop() for patch in patches])
        return dispatcher.promote_plan(CLOSED, ATTEMPT)

    def test_invalid_inputs_fail_closed(self):
        with mock.patch.object(state_manager, "read_dispatch_state") as read:
            self.assertFalse(dispatcher.promote_plan("bad id", ATTEMPT)["promoted"])
            self.assertEqual(dispatcher.promote_plan(CLOSED, "bad")["reason"], "invalid_attempt_id")
        read.assert_not_called()

    def test_claim_must_be_closed_out(self):
        result = self._run(self._patch(claim=closed_claim("dispatched")))
        self.assertFalse(result["promoted"])
        self.assertEqual(result["reason"], "plan_claim_not_closed_out")

    def test_promotes_exactly_one_successor(self):
        patches = self._patch()
        for patch in patches:
            patch.start()
        try:
            with mock.patch.object(state_manager, "read_dispatch_state", return_value=None), \
                 mock.patch.object(state_manager, "record_dispatch_state", return_value=True) as write:
                result = dispatcher.promote_plan(CLOSED, ATTEMPT)
            write.assert_called_once()
            self.assertEqual(write.call_args.args[1], f"plan-promote:{CLOSED}:{ATTEMPT}")
            self.assertEqual(write.call_args.args[2], "plan-promote")
            self.assertEqual(write.call_args.args[3], "promoted")
        finally:
            for patch in patches:
                patch.stop()
        self.assertTrue(result["promoted"], result)
        self.assertEqual(result["successor_id"], SUCCESSOR)
        self.assertEqual(result["routing_main_sha"], MAIN)
        self.assertEqual(result["capsule_digest"], derived_spec_digest())

    def test_promote_is_idempotent(self):
        receipt = {
            "kind": "agent-orchestrator-dispatch-state",
            "version": 1,
            "issue_number": LEDGER,
            "dispatch_id": f"plan-promote:{CLOSED}:{ATTEMPT}",
            "action": "plan-promote",
            "status": "promoted",
            "details": {
                "subject_kind": "plan-packet",
                "subject_id": CLOSED,
                "attempt_id": ATTEMPT,
                "source_main_sha": MAIN,
                "routing_main_sha": MAIN,
                "successor_id": SUCCESSOR,
                "capsule_digest": derived_spec_digest(),
            },
        }
        patches = self._patch()
        for patch in patches:
            patch.start()
        try:
            with mock.patch.object(state_manager, "read_dispatch_state", return_value=receipt), \
                 mock.patch.object(state_manager, "record_dispatch_state") as write:
                result = dispatcher.promote_plan(CLOSED, ATTEMPT)
                write.assert_not_called()
        finally:
            for patch in patches:
                patch.stop()
        self.assertTrue(result["promoted"])
        self.assertEqual(result["reason"], "already_promoted")

    def test_conflicting_promotion_receipt_fails_closed(self):
        receipt = {
            "kind": "agent-orchestrator-dispatch-state",
            "version": 1,
            "issue_number": LEDGER,
            "dispatch_id": f"plan-promote:{CLOSED}:{ATTEMPT}",
            "action": "plan-promote",
            "status": "promoted",
            "details": {"successor_id": "TOOL-OTHER-1"},
        }
        patches = self._patch()
        for patch in patches:
            patch.start()
        try:
            with mock.patch.object(state_manager, "read_dispatch_state", return_value=receipt):
                result = dispatcher.promote_plan(CLOSED, ATTEMPT)
        finally:
            for patch in patches:
                patch.stop()
        self.assertFalse(result["promoted"])
        self.assertEqual(result["reason"], "conflicting_promotion_receipt")

    def test_absent_successor_escalates_when_compile_source_unavailable(self):
        with mock.patch.object(state_manager, "read_dispatch_state", return_value=None), \
             mock.patch.object(state_manager, "record_dispatch_state", return_value=True) as write:
            result = self._run(self._patch(routing_error="plan_packet_absent"))
        self.assertFalse(result["promoted"])
        self.assertTrue(result.get("escalated"))
        self.assertEqual(result["reason"], "promotion_current_main_evidence_missing")
        self.assertEqual(result["pause_owner"], "planning")
        self.assertEqual(write.call_args.args[1], f"plan-escalate:{CLOSED}:{ATTEMPT}")
        self.assertEqual(write.call_args.args[2], "plan-escalate")
        self.assertEqual(write.call_args.args[3], "escalated")

    def test_successor_still_current_escalates_when_compile_source_unavailable(self):
        with mock.patch.object(state_manager, "read_dispatch_state", return_value=None), \
             mock.patch.object(state_manager, "record_dispatch_state", return_value=True):
            result = self._run(self._patch(routing_error="successor_still_current"))
        self.assertTrue(result.get("escalated"))
        self.assertEqual(result["reason"], "promotion_current_main_evidence_missing")

    def _compile_documents(self, only_blocked=False):
        """Adapter documents for compile tests: CLOSED is the current packet."""

        import route_driver
        adapter = mock.Mock()
        next_doc = routing_document(successor=CLOSED, closed_historical=False)
        adapter.accepted_route_document.return_value = future_route_document(only_blocked=only_blocked)
        adapter.accepted_plan_document.return_value = next_doc
        adapter.accepted_status_document.return_value = status_document()
        return adapter, next_doc

    def test_no_eligible_successor_escalates_with_pause_receipt(self):
        import route_driver
        with mock.patch.object(dispatcher.local_loop, "GitHubAdapter") as adapter_cls:
            adapter, _next_doc = self._compile_documents(only_blocked=True)
            adapter_cls.return_value = adapter
            with mock.patch.object(state_manager, "read_dispatch_state", return_value=None), \
                 mock.patch.object(state_manager, "record_dispatch_state", return_value=True) as write:
                result = self._run(self._patch(routing_error="plan_packet_absent"))
        self.assertFalse(result["promoted"])
        self.assertTrue(result.get("escalated"))
        self.assertEqual(result["reason"], "promotion_current_main_evidence_missing")
        self.assertEqual(result["pause_owner"], "planning")
        self.assertEqual(write.call_args.args[1], f"plan-escalate:{CLOSED}:{ATTEMPT}")
        self.assertEqual(write.call_args.args[2], "plan-escalate")
        self.assertEqual(write.call_args.args[3], "escalated")

    def test_absent_successor_escalates_until_current_main_evidence_is_validated(self):
        import route_driver
        with mock.patch.object(dispatcher.local_loop, "GitHubAdapter") as adapter_cls:
            adapter, _next_doc = self._compile_documents()
            adapter_cls.return_value = adapter
            with mock.patch.object(state_manager, "read_dispatch_state", return_value=None), \
                 mock.patch.object(state_manager, "record_dispatch_state", return_value=True) as write:
                result = self._run(self._patch(routing_error="successor_still_current"))
            write.assert_called_once()
            self.assertEqual(write.call_args.args[1], f"plan-escalate:{CLOSED}:{ATTEMPT}")
            self.assertEqual(write.call_args.args[2], "plan-escalate")
            self.assertEqual(write.call_args.args[3], "escalated")
        self.assertFalse(result["promoted"], result)
        self.assertTrue(result.get("escalated"))
        self.assertEqual(result["reason"], "promotion_current_main_evidence_missing")

    def test_controller_never_invokes_the_compiler_without_validated_evidence(self):
        import route_driver
        with mock.patch.object(state_manager, "read_dispatch_state", return_value=None), \
             mock.patch.object(state_manager, "record_dispatch_state", return_value=True), \
             mock.patch.object(route_driver, "compile_successor") as compile_mock:
            result = self._run(self._patch(routing_error="successor_still_current"))
        compile_mock.assert_not_called()
        self.assertFalse(result["promoted"])
        self.assertEqual(result["reason"], "promotion_current_main_evidence_missing")

    def test_invalid_routing_fails_closed_without_receipt(self):
        patches = self._patch(routing_error="plan_packet_fields_invalid")
        for patch in patches:
            patch.start()
        try:
            with mock.patch.object(state_manager, "record_dispatch_state") as write:
                result = dispatcher.promote_plan(CLOSED, ATTEMPT)
                write.assert_not_called()
        finally:
            for patch in patches:
                patch.stop()
        self.assertFalse(result["promoted"])
        self.assertTrue(result["reason"].startswith("routing_invalid:"))


class TestBootstrapPromotionFallback(unittest.TestCase):
    def _runner(self):
        github = mock.Mock()
        github.read_control_state.return_value = {
            "emergency_stop": False,
            "orchestrator_enabled": True,
        }
        github.repository_metadata.return_value = {
            "name_with_owner": "acme/repo",
            "default_branch": "main",
        }
        github.accepted_main_sha.return_value = MAIN
        github.accepted_plan_document.return_value = "# Next Decision\n"
        github.accepted_route_document.return_value = "# Future Route\n"
        github.accepted_status_document.return_value = status_document()
        github.plan_ledger_issue.return_value = LEDGER
        git = mock.Mock()
        git.origin_main_sha.return_value = MAIN
        return local_run_once.LocalRunOnce(
            github, git, repository="acme/repo", repo_path=Path("/tmp")
        ), github

    def test_proved_bootstrap_scope_bridge_uses_existing_promotion_fallback(self):
        runner, github = self._runner()
        receipt = "PR #390 exact merge-backed COMPLETE receipt"
        successor = mock.Mock()
        planned = route_driver.PromotionPlanResult(
            "READY_FOR_EXECUTION", "proved", evidence=mock.Mock()
        )
        compiled = mock.Mock()
        expected = mock.Mock(status="promotion_pr")

        with mock.patch.object(
            route_driver, "accepted_complete_receipt", return_value=receipt
        ), mock.patch.object(
            route_driver, "retained_t3_request", return_value=None
        ), mock.patch.object(
            plan_lane,
            "successor_binding",
            side_effect=plan_lane.PlanLaneError("plan_allowed_paths_invalid"),
        ), mock.patch.object(
            route_driver, "eligible_successor", return_value=successor
        ), mock.patch.object(
            runner, "_plan_current_main_evidence", return_value=planned
        ) as planner, mock.patch.object(
            route_driver, "compile_successor", return_value=compiled
        ) as compiler, mock.patch.object(
            runner, "_drive_promotion_pr", return_value=expected
        ) as drive:
            result = runner.run_route_once(
                CLOSED, ATTEMPT, bootstrap_receipt=receipt
            )

        self.assertIs(result, expected)
        github.dispatch_controller.assert_not_called()
        planner.assert_called_once_with(successor, MAIN, receipt)
        compiler.assert_called_once_with(
            "# Future Route\n",
            "# Next Decision\n",
            status_document(),
            CLOSED,
            receipt,
            MAIN,
            planned.evidence,
        )
        drive.assert_called_once_with(CLOSED, ATTEMPT, MAIN, LEDGER, compiled, {})

    def test_planner_transport_unavailability_is_recoverable_not_a_controller_pause(self):
        runner, github = self._runner()
        receipt = "PR #390 exact merge-backed COMPLETE receipt"
        successor = mock.Mock()
        planned = route_driver.PromotionPlanResult(
            "DECISION_REQUIRED", "promotion_planner_unavailable"
        )

        with mock.patch.object(
            route_driver, "accepted_complete_receipt", return_value=receipt
        ), mock.patch.object(
            route_driver, "retained_t3_request", return_value=None
        ), mock.patch.object(
            plan_lane,
            "successor_binding",
            side_effect=plan_lane.PlanLaneError("plan_allowed_paths_invalid"),
        ), mock.patch.object(
            route_driver, "eligible_successor", return_value=successor
        ), mock.patch.object(
            runner, "_plan_current_main_evidence", return_value=planned
        ):
            result = runner.run_route_once(
                CLOSED, ATTEMPT, bootstrap_receipt=receipt
            )

        self.assertEqual(result.status, "unavailable")
        self.assertEqual(result.details["reason"], "promotion_planner_unavailable")
        github.dispatch_controller.assert_not_called()

    def test_ordinary_scope_error_does_not_enter_bootstrap_fallback(self):
        runner, _github = self._runner()

        with mock.patch.object(
            plan_lifecycle,
            "_exact_plan_claim",
            return_value={
                "status": "closed_out",
                "details": {"closeout_reference": "PR #42"},
            },
        ), mock.patch.object(
            route_driver, "retained_t3_request", return_value=None
        ), mock.patch.object(
            plan_lane,
            "successor_binding",
            side_effect=plan_lane.PlanLaneError("plan_allowed_paths_invalid"),
        ), mock.patch.object(runner, "_plan_current_main_evidence") as planner, mock.patch.object(
            runner, "_drive_promotion_pr"
        ) as drive:
            result = runner.run_route_once(CLOSED, ATTEMPT)

        self.assertEqual(result.status, "rejected")
        self.assertEqual(result.details["reason"], "routing_invalid:plan_allowed_paths_invalid")
        planner.assert_not_called()
        drive.assert_not_called()


class TestPromotionWait(unittest.TestCase):
    def test_reconcile_unknown_generation_never_mints_a_fresh_attempt(self):
        github = mock.Mock()
        runner = local_run_once.LocalRunOnce(
            github, mock.Mock(), repository="acme/repo", repo_path=Path("/tmp"),
        )
        candidate = mock.Mock(source_main_sha=MAIN)
        comment = {
            "author": {"login": "github-actions[bot]"},
            "body": json.dumps({
                "action": "plan-run",
                "dispatch_id": DISPATCH_ID,
                "status": "failed_unknown_output",
                "details": {
                    "subject_kind": "plan-packet",
                    "subject_id": CLOSED,
                    "source_main_sha": MAIN,
                },
            }),
        }
        with mock.patch.object(runner, "_live_plan", return_value=(candidate, LEDGER)), \
             mock.patch.object(state_manager, "get_issue_comments", return_value=[comment]):
            result = runner.reconcile_plan(CLOSED)
        self.assertEqual(result.status, "outcome_unknown")
        self.assertEqual(result.details["reason"], "plan_reconcile_outcome_unknown")

    def test_reconcile_closed_claim_reuses_exact_attempt_for_promotion(self):
        """A restart resumes promotion; it never recliams the closed packet."""

        github = mock.Mock()
        runner = local_run_once.LocalRunOnce(
            github, mock.Mock(), repository="acme/repo", repo_path=Path("/tmp"),
        )
        candidate = mock.Mock(source_main_sha=MAIN)
        comment = {
            "author": {"login": "github-actions[bot]"},
            "body": json.dumps(closed_claim()),
        }
        with mock.patch.object(
            runner, "_live_plan", return_value=(candidate, LEDGER)
        ), mock.patch.object(
            state_manager, "get_issue_comments", return_value=[comment]
        ), mock.patch.object(runner, "run_plan_once") as run_once:
            result = runner.reconcile_plan(CLOSED)
        self.assertEqual(result.status, "closed_out")
        self.assertEqual(result.attempt_id, ATTEMPT)
        self.assertTrue(result.details["reconciled"])
        run_once.assert_not_called()

    def test_wait_reads_promotion_and_returns_closed_out(self):
        github = mock.Mock()
        runner = local_run_once.LocalRunOnce(
            github, mock.Mock(), repository="acme/repo", repo_path=Path("/tmp"),
            lifecycle_timeout_seconds=10, sleeper=lambda _: None,
        )
        lifecycle = {
            "stages": {"ci": True, "review": True, "merge": True, "closeout": True},
            "transitions": {
                "merge": {"merge_commit_sha": "c" * 40},
                "closeout": {"terminal_packet_state": "closed_out", "closeout_reference": "PR #42"},
            },
        }
        promotion = {"kind": "plan-promote", "status": "promoted", "details": {"successor_id": SUCCESSOR}}
        with mock.patch.object(
            plan_lifecycle, "read_plan_lifecycle", return_value=lifecycle
        ), mock.patch.object(
            runner, "_read_plan_promotion", return_value=promotion
        ):
            result = runner._wait_for_plan_terminal_receipts(LEDGER, CLOSED, ATTEMPT, 42, "b" * 40)
        self.assertEqual(result.status, "closed_out")
        self.assertEqual(result.details["promotion"]["status"], "promoted")
        self.assertFalse(result.details["promotion_pending"])
        github.dispatch_controller.assert_not_called()

    def test_wait_dispatches_promote_once_and_reports_pending_on_timeout(self):
        github = mock.Mock()
        runner = local_run_once.LocalRunOnce(
            github, mock.Mock(), repository="acme/repo", repo_path=Path("/tmp"),
            lifecycle_timeout_seconds=10, sleeper=lambda _: None,
        )
        lifecycle = {
            "stages": {"ci": True, "review": True, "merge": True, "closeout": True},
            "transitions": {
                "merge": {"merge_commit_sha": "c" * 40},
                "closeout": {"terminal_packet_state": "closed_out", "closeout_reference": "PR #42"},
            },
        }
        with mock.patch.object(
            plan_lifecycle, "read_plan_lifecycle", return_value=lifecycle
        ), mock.patch.object(
            runner, "_read_plan_promotion", return_value=None
        ), mock.patch.object(
            local_run_once.time, "monotonic", side_effect=[0.0, 11.0]
        ):
            result = runner._wait_for_plan_terminal_receipts(LEDGER, CLOSED, ATTEMPT, 42, "b" * 40)
        self.assertEqual(result.status, "closed_out")
        self.assertTrue(result.details["promotion_pending"])
        github.dispatch_controller.assert_called_once_with(
            "promote-plan", {"packet_id": CLOSED, "attempt_id": ATTEMPT}
        )

    def test_promotion_read_prefers_promotion_then_escalation(self):
        github = mock.Mock()
        runner = local_run_once.LocalRunOnce(
            github, mock.Mock(), repository="acme/repo", repo_path=Path("/tmp"),
        )
        with mock.patch.object(
            state_manager, "read_dispatch_state", return_value=None
        ):
            self.assertIsNone(runner._read_plan_promotion(LEDGER, CLOSED, ATTEMPT))
        receipt = {
            "kind": "agent-orchestrator-dispatch-state",
            "version": 1,
            "issue_number": LEDGER,
            "dispatch_id": f"plan-escalate:{CLOSED}:{ATTEMPT}",
            "action": "plan-escalate",
            "status": "escalated",
            "details": {"reason": "plan_packet_absent", "pause_owner": "planning"},
        }
        with mock.patch.object(
            state_manager, "read_dispatch_state", return_value=receipt
        ):
            read = runner._read_plan_promotion(LEDGER, CLOSED, ATTEMPT)
        self.assertEqual(read["kind"], "plan-escalate")
        self.assertEqual(read["status"], "escalated")


class TestPromotionResume(unittest.TestCase):
    def _compiled(self):
        return mock.Mock(
            packet_id=SUCCESSOR,
            branch="agent/packet-pe7-successor-promotion-escalation-1",
            spec_digest="d" * 64,
            packet_state="READY_FOR_EXECUTION",
            t3_request=None,
        )

    def _runner(self):
        return local_run_once.LocalRunOnce(
            mock.Mock(), mock.Mock(), repository="acme/repo", repo_path=Path("/tmp"),
        )

    def _resume(self, runner):
        return runner._resume_promotion_pr(
            CLOSED, ATTEMPT, MAIN, LEDGER, self._compiled(), "b" * 40
        )

    def test_missing_review_is_a_recoverable_wait_not_a_decision(self):
        runner = self._runner()
        existing = {"number": 42, "head_sha": "b" * 40, "isDraft": True}
        with mock.patch.object(pr_binding, "find_plan_pr", return_value=existing), \
             mock.patch.object(plan_lifecycle, "plan_review_receipt", return_value=None), \
             mock.patch.object(local_run_once.subprocess, "run") as ready:
            result = self._resume(runner)
        self.assertEqual(result.status, "promotion_review_pending")
        ready.assert_not_called()

    def test_ready_failure_is_visible_and_never_proceeds_to_ci(self):
        runner = self._runner()
        existing = {"number": 42, "head_sha": "b" * 40, "isDraft": True}
        with mock.patch.object(pr_binding, "find_plan_pr", return_value=existing), \
             mock.patch.object(plan_lifecycle, "plan_review_receipt", return_value={"verdict": "PASS"}), \
             mock.patch.object(local_run_once.subprocess, "run", return_value=mock.Mock(returncode=1)), \
             mock.patch.object(runner, "_exact_head_canonical_ci") as ci:
            result = self._resume(runner)
        self.assertEqual(result.status, "promotion_ready_pending")
        self.assertEqual(result.details["reason"], "ready_transition_failed")
        ci.assert_not_called()

    def test_canonical_ci_wait_is_recoverable_after_a_successful_ready_transition(self):
        runner = self._runner()
        existing = {"number": 42, "head_sha": "b" * 40, "isDraft": True}
        with mock.patch.object(pr_binding, "find_plan_pr", return_value=existing), \
             mock.patch.object(plan_lifecycle, "plan_review_receipt", return_value={"verdict": "PASS"}), \
             mock.patch.object(local_run_once.subprocess, "run", return_value=mock.Mock(returncode=0)), \
             mock.patch.object(runner, "_exact_head_canonical_ci", return_value=False):
            result = self._resume(runner)
        self.assertEqual(result.status, "promotion_ci_pending")

    def test_escalation_receipt_is_a_bounded_pause_not_a_promotion(self):
        runner = self._runner()
        promotion = {
            "kind": "plan-escalate",
            "status": "escalated",
            "details": {"reason": "promotion_current_main_evidence_missing"},
        }
        with mock.patch.object(runner.github, "dispatch_controller"), \
             mock.patch.object(runner, "_read_plan_promotion", return_value=promotion):
            result = runner._settle_promotion(
                CLOSED, ATTEMPT, LEDGER, SUCCESSOR, 42, "b" * 40, "c" * 40,
            )
        self.assertEqual(result.status, "bounded_pause")
        self.assertEqual(result.details["reason"], "promotion_current_main_evidence_missing")


class TestRouteT3ReceiptTransport(unittest.TestCase):
    def setUp(self):
        self._actions_environment = mock.patch.dict(os.environ, {
            "GITHUB_ACTIONS": "true",
            "GITHUB_ACTOR": "authorized-operator",
        })
        self._actions_environment.start()
        self.addCleanup(self._actions_environment.stop)

    def _arguments(self):
        now = datetime.now(timezone.utc)
        return (
            CLOSED, MAIN, "b" * 64, "c" * 64, "d" * 64, "e" * 64,
            "f" * 64, "a" * 64, "authorized-operator", "local_sol_5_6_max", "1" * 64, now.isoformat(),
            (now + timedelta(minutes=5)).isoformat(), "GO",
        )

    @staticmethod
    def _live_t3_document():
        request = {
            "schema_version": "route_t3_request.v1", "packet_id": CLOSED,
            "accepted_main_sha": MAIN, "candidate_digest": "b" * 64,
            "action_digest": "c" * 64, "scope_digest": "d" * 64,
            "authority_owner_digest": "a" * 64,
            "requested_action": "one finite action",
        }
        return (
            f"## Active Routing\n\n1. `{CLOSED}` — `T3_REQUIRED`\n\n"
            f"## Packet {CLOSED}\n\n**State:** `T3_REQUIRED`\n\n"
            "<!-- route-t3-request:v1\n" + json.dumps(request) + "\n-->\n"
        )

    def _adapter(self):
        adapter = mock.Mock()
        adapter.repository_metadata.return_value = {"default_branch": "main"}
        adapter.accepted_main_sha.return_value = MAIN
        adapter.accepted_plan_document.return_value = self._live_t3_document()
        return adapter

    def test_operator_transport_records_only_one_exact_receipt(self):
        adapter = self._adapter()
        with mock.patch.object(dispatcher, "_repo", return_value="acme/repo"), \
             mock.patch.object(dispatcher.control_state, "require_live", return_value=None), \
             mock.patch.object(dispatcher.control_state, "read_plan_ledger", return_value={"number": LEDGER}), \
             mock.patch.object(local_loop, "GitHubAdapter", return_value=adapter), \
             mock.patch.object(state_manager, "read_dispatch_state", return_value=None), \
             mock.patch.object(state_manager, "record_dispatch_state", return_value=True) as write:
            result = dispatcher.record_route_t3_receipt(*self._arguments())
        self.assertTrue(result["authorized"])
        self.assertEqual(write.call_args.args[2:4], ("route-t3-receipt", "authorized"))
        self.assertEqual(write.call_args.args[1], f"route-t3:{CLOSED}:" + "b" * 64)
        self.assertEqual(
            write.call_args.args[4]["decision_digest"],
            route_driver.t3_decision_digest(
                route_driver.T3Request(
                    packet_id=CLOSED,
                    accepted_main_sha=MAIN,
                    candidate_digest="b" * 64,
                    action_digest="c" * 64,
                    scope_digest="d" * 64,
                    authority_owner_digest="a" * 64,
                    requested_action="one finite action",
                ),
                "local_sol_5_6_max",
                "1" * 64,
                "GO",
            ),
        )

    def test_conflicting_existing_t3_receipt_fails_closed(self):
        adapter = self._adapter()
        existing = {
            "action": "route-t3-receipt", "status": "authorized",
            "details": {"different": "binding"},
        }
        with mock.patch.object(dispatcher, "_repo", return_value="acme/repo"), \
             mock.patch.object(dispatcher.control_state, "require_live", return_value=None), \
             mock.patch.object(dispatcher.control_state, "read_plan_ledger", return_value={"number": LEDGER}), \
             mock.patch.object(local_loop, "GitHubAdapter", return_value=adapter), \
             mock.patch.object(state_manager, "read_dispatch_state", return_value=existing), \
             mock.patch.object(state_manager, "record_dispatch_state") as write:
            result = dispatcher.record_route_t3_receipt(*self._arguments())
        self.assertFalse(result["authorized"])
        self.assertEqual(result["reason"], "conflicting_t3_receipt")
        write.assert_not_called()

    def test_receipt_for_a_noncurrent_t3_request_is_rejected_before_ledger_write(self):
        adapter = self._adapter()
        adapter.accepted_main_sha.return_value = "f" * 40
        adapter.accepted_plan_document.return_value = self._live_t3_document().replace(
            MAIN, "f" * 40
        )
        with mock.patch.object(dispatcher, "_repo", return_value="acme/repo"), \
             mock.patch.object(dispatcher.control_state, "require_live", return_value=None), \
             mock.patch.object(local_loop, "GitHubAdapter", return_value=adapter), \
             mock.patch.object(state_manager, "record_dispatch_state") as write:
            result = dispatcher.record_route_t3_receipt(*self._arguments())
        self.assertFalse(result["authorized"])
        self.assertEqual(result["reason"], "route_t3_request_binding_mismatch")
        write.assert_not_called()

    def test_untrusted_or_bot_actor_is_rejected_before_ledger_write(self):
        adapter = self._adapter()
        with mock.patch.dict(os.environ, {
            "GITHUB_ACTIONS": "true",
            "GITHUB_ACTOR": "github-actions[bot]",
        }), \
             mock.patch.object(dispatcher, "_repo", return_value="acme/repo"), \
             mock.patch.object(dispatcher.control_state, "require_live", return_value=None), \
             mock.patch.object(local_loop, "GitHubAdapter", return_value=adapter) as adapter_cls, \
             mock.patch.object(state_manager, "record_dispatch_state") as write:
            result = dispatcher.record_route_t3_receipt(*self._arguments())
        self.assertFalse(result["authorized"])
        self.assertEqual(result["reason"], "route_t3_operator_unproved")
        adapter_cls.assert_not_called()
        write.assert_not_called()

    def test_owner_digest_must_match_the_current_t3_request_before_ledger_write(self):
        adapter = self._adapter()
        arguments = list(self._arguments())
        arguments[7] = "9" * 64
        with mock.patch.object(dispatcher, "_repo", return_value="acme/repo"), \
             mock.patch.object(dispatcher.control_state, "require_live", return_value=None), \
             mock.patch.object(local_loop, "GitHubAdapter", return_value=adapter), \
             mock.patch.object(state_manager, "record_dispatch_state") as write:
            result = dispatcher.record_route_t3_receipt(*arguments)
        self.assertFalse(result["authorized"])
        self.assertEqual(result["reason"], "route_t3_request_binding_mismatch")
        write.assert_not_called()

    def test_unallowlisted_decision_source_is_rejected_before_ledger_write(self):
        adapter = self._adapter()
        arguments = list(self._arguments())
        arguments[9] = "untrusted_model"
        with mock.patch.object(dispatcher, "_repo", return_value="acme/repo"), \
             mock.patch.object(dispatcher.control_state, "require_live", return_value=None), \
             mock.patch.object(local_loop, "GitHubAdapter", return_value=adapter), \
             mock.patch.object(state_manager, "record_dispatch_state") as write:
            result = dispatcher.record_route_t3_receipt(*arguments)
        self.assertFalse(result["authorized"])
        self.assertEqual(result["reason"], "t3_receipt_decision_source_invalid")
        write.assert_not_called()

    def test_malformed_decision_evidence_is_rejected_before_ledger_write(self):
        adapter = self._adapter()
        arguments = list(self._arguments())
        arguments[10] = "not-a-sha256"
        with mock.patch.object(dispatcher, "_repo", return_value="acme/repo"), \
             mock.patch.object(dispatcher.control_state, "require_live", return_value=None), \
             mock.patch.object(local_loop, "GitHubAdapter", return_value=adapter), \
             mock.patch.object(state_manager, "record_dispatch_state") as write:
            result = dispatcher.record_route_t3_receipt(*arguments)
        self.assertFalse(result["authorized"])
        self.assertEqual(result["reason"], "t3_receipt_decision_source_invalid")
        write.assert_not_called()


class TestOperatorEffectRouteResume(unittest.TestCase):
    def _request_and_receipt(self):
        now = datetime.now(timezone.utc)
        request = route_driver.T3Request(
            packet_id=CLOSED,
            accepted_main_sha=MAIN,
            candidate_digest="b" * 64,
            action_digest="c" * 64,
            scope_digest="d" * 64,
            authority_owner_digest="a" * 64,
            requested_action="one finite external action",
        )
        raw = {
            "schema_version": "route_t3_receipt.v1",
            "packet_id": request.packet_id,
            "accepted_main_sha": request.accepted_main_sha,
            "candidate_digest": request.candidate_digest,
            "action_digest": request.action_digest,
            "scope_digest": request.scope_digest,
            "authority_receipt_digest": "e" * 64,
            "outcome_receipt_digest": "f" * 64,
            "authority_owner_digest": request.authority_owner_digest,
            "operator": "authorized-operator",
            "decision_source": "local_sol_5_6_max",
            "decision_evidence_digest": "1" * 64,
            "decision_digest": "",
            "issued_at": now.isoformat(),
            "expires_at": (now + timedelta(minutes=5)).isoformat(),
            "disposition": "GO",
        }
        raw["decision_digest"] = route_driver.t3_decision_digest(
            request,
            raw["decision_source"],
            raw["decision_evidence_digest"],
            raw["disposition"],
        )
        receipt, reason = route_driver.validate_t3_receipt(raw, request, now=now)
        self.assertEqual(reason, "t3_receipt_valid")
        return request, receipt, raw

    @staticmethod
    def _t3_document(request):
        marker = {
            "schema_version": "route_t3_request.v1",
            "packet_id": request.packet_id,
            "accepted_main_sha": request.accepted_main_sha,
            "candidate_digest": request.candidate_digest,
            "action_digest": request.action_digest,
            "scope_digest": request.scope_digest,
            "authority_owner_digest": request.authority_owner_digest,
            "requested_action": request.requested_action,
        }
        return (
            f"## Active Routing\n\n1. `{request.packet_id}` — `T3_REQUIRED`\n\n"
            f"## Packet {request.packet_id}\n\n**State:** `T3_REQUIRED`\n\n"
            "<!-- route-t3-request:v1\n" + json.dumps(marker) + "\n-->\n"
        )

    def _runner(self, request, raw):
        github = mock.Mock()
        github.read_control_state.return_value = {"emergency_stop": False, "orchestrator_enabled": True}
        github.repository_metadata.return_value = {"name_with_owner": "acme/repo", "default_branch": "main"}
        github.accepted_main_sha.return_value = MAIN
        github.accepted_plan_document.return_value = self._t3_document(request)
        github.accepted_route_document.return_value = "future"
        github.accepted_status_document.return_value = status_document()
        github.plan_ledger_issue.return_value = LEDGER
        git = mock.Mock()
        git.origin_main_sha.return_value = MAIN
        runner = local_run_once.LocalRunOnce(
            github, git, repository="acme/repo", repo_path=Path("/tmp"),
        )
        state = {"action": "route-t3-receipt", "status": "authorized", "details": raw}
        return runner, github, state

    def test_valid_operator_completion_promotes_only_the_provider_free_closeout(self):
        request, receipt, raw = self._request_and_receipt()
        runner, _github, state = self._runner(request, raw)
        successor = mock.Mock(
            profile=("PE7-EFFECT-CLOSEOUT-1", "CLOSEOUT", "T2", "none", "evidence_review"),
        )
        successor.sketch.prerequisites = (CLOSED,)
        planned = route_driver.PromotionPlanResult(
            "READY_FOR_EXECUTION", "proved", evidence=mock.Mock()
        )
        compiled = mock.Mock()
        with mock.patch.object(state_manager, "read_dispatch_state", return_value=state), \
             mock.patch.object(route_driver, "eligible_successor", return_value=successor), \
             mock.patch.object(runner, "_plan_current_main_evidence", return_value=planned), \
             mock.patch.object(route_driver, "compile_successor", return_value=compiled) as compile_successor, \
             mock.patch.object(runner, "_drive_promotion_pr", return_value=mock.Mock(status="promotion_pr")) as drive:
            result = runner.run_effect_route_once(request, receipt)
            self.assertEqual(
                compile_successor.call_args.kwargs["closed_packet_state"],
                "IN_PROGRESS",
            )
            self.assertEqual(
                compile_successor.call_args.kwargs["retained_t3_request"], request
            )
        self.assertEqual(result.status, "promotion_pr")
        self.assertEqual(drive.call_args.args[0], CLOSED)
        self.assertEqual(drive.call_args.args[2], MAIN)
        self.assertEqual(drive.call_args.args[4], compiled)

    def test_effect_closeout_planner_transport_unavailability_is_recoverable(self):
        request, receipt, raw = self._request_and_receipt()
        runner, _github, state = self._runner(request, raw)
        successor = mock.Mock(
            profile=("PE7-EFFECT-CLOSEOUT-1", "CLOSEOUT", "T2", "none", "evidence_review"),
        )
        successor.sketch.prerequisites = (CLOSED,)
        planned = route_driver.PromotionPlanResult(
            "DECISION_REQUIRED", "promotion_planner_unavailable"
        )

        with mock.patch.object(state_manager, "read_dispatch_state", return_value=state), \
             mock.patch.object(route_driver, "eligible_successor", return_value=successor), \
             mock.patch.object(runner, "_plan_current_main_evidence", return_value=planned), \
             mock.patch.object(runner, "_drive_promotion_pr") as drive:
            result = runner.run_effect_route_once(request, receipt)

        self.assertEqual(result.status, "unavailable")
        self.assertEqual(result.details["reason"], "promotion_planner_unavailable")
        drive.assert_not_called()

    def test_receipt_without_a_direct_closeout_stops_outcome_unknown(self):
        request, receipt, raw = self._request_and_receipt()
        runner, _github, state = self._runner(request, raw)
        successor = mock.Mock(
            profile=("PE7-WRONG-SUCCESSOR-1", "IMPLEMENT", "T1", "none", "source_focused_full"),
        )
        successor.sketch.prerequisites = (CLOSED,)
        with mock.patch.object(state_manager, "read_dispatch_state", return_value=state), \
             mock.patch.object(route_driver, "eligible_successor", return_value=successor), \
             mock.patch.object(runner, "_drive_promotion_pr") as drive:
            result = runner.run_effect_route_once(request, receipt)
        self.assertEqual(result.status, "outcome_unknown")
        self.assertEqual(result.details["reason"], "route_effect_closeout_not_proved")
        drive.assert_not_called()

    def test_unproved_operator_completion_never_opens_a_promotion_pr(self):
        request, receipt, raw = self._request_and_receipt()
        runner, _github, _state = self._runner(request, raw)
        with mock.patch.object(state_manager, "read_dispatch_state", return_value=None), \
             mock.patch.object(runner, "_drive_promotion_pr") as drive:
            result = runner.run_effect_route_once(request, receipt)
        self.assertEqual(result.status, "rejected")
        self.assertEqual(result.details["reason"], "route_effect_receipt_unproved")
        drive.assert_not_called()

    def test_closeout_cannot_promote_after_effect_without_owner_validated_outcome_receipt(self):
        request, _receipt, _raw = self._request_and_receipt()
        runner, github, _state = self._runner(request, _raw)
        closeout = "PE7-EFFECT-CLOSEOUT-1"
        bridge = route_driver.compact_next_window(
            "# Next\n\n## Common Execution Protocol\n\n- retained\n",
            closed_packet_id=request.packet_id,
            predecessor_receipt="T3 receipt " + "f" * 64,
            active_packet_block=(
                f"## Packet {closeout}\n\n**State:** `READY_FOR_EXECUTION`\n\n"
                "**Class:** `CLOSEOUT`\n"
            ),
            closed_packet_state="IN_PROGRESS",
            retained_marker=route_driver._t3_request_marker(request),
        )
        github.accepted_plan_document.return_value = bridge
        with mock.patch.object(
            plan_lifecycle,
            "_exact_plan_claim",
            return_value={"status": "closed_out", "details": {"closeout_reference": "PR #42"}},
        ), mock.patch.object(runner, "_drive_promotion_pr") as drive:
            result = runner.run_route_once(closeout, ATTEMPT)
        self.assertEqual(result.status, "outcome_unknown")
        self.assertEqual(result.details["reason"], "route_effect_owner_outcome_unproved")
        drive.assert_not_called()


if __name__ == "__main__":
    unittest.main()
