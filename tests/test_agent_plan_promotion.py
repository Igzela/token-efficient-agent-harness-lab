"""Provider-free tests for plan-lane successor promotion and escalation."""

from __future__ import annotations

import json
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
                return_value=routing or (MAIN, routing_document()),
            ),
        ]
        if routing_error is not None:
            def _raise(_document, _closed, _main):
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


class TestPromotionWait(unittest.TestCase):
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


class TestRouteT3ReceiptTransport(unittest.TestCase):
    def _arguments(self):
        now = datetime.now(timezone.utc)
        return (
            CLOSED, MAIN, "b" * 64, "c" * 64, "d" * 64, "e" * 64,
            "existing-authority-owner", "authorized-operator", now.isoformat(),
            (now + timedelta(minutes=5)).isoformat(), "GO",
        )

    @staticmethod
    def _live_t3_document():
        request = {
            "schema_version": "route_t3_request.v1", "packet_id": CLOSED,
            "accepted_main_sha": MAIN, "candidate_digest": "b" * 64,
            "action_digest": "c" * 64, "scope_digest": "d" * 64,
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


if __name__ == "__main__":
    unittest.main()
