"""Provider-free tests for the promotion compiler and route driver core."""

from __future__ import annotations

import dataclasses
import hashlib
import json
from pathlib import Path
import re
import shlex
import subprocess
import sys
import tempfile
from datetime import datetime, timedelta, timezone
import unittest
from unittest import mock

CONTROL = Path(__file__).resolve().parents[1] / "scripts" / "agent-control"
sys.path.insert(0, str(CONTROL))
sys.path.insert(0, str(CONTROL.parent))

import check_agent_handoff  # noqa: E402
import plan_lane  # noqa: E402
import route_driver  # noqa: E402

MAIN = "a" * 40
CLOSED = "PE7-LIFECYCLE-CONTROLLER-1"
SUCCESSOR = "PE7-SUCCESSOR-PROMOTION-ESCALATION-1"
EVIDENCE = (
    "PR #389 exact head `" + "b" * 40 + "`; merge `" + MAIN + "`; "
    "exact-head `PASS`; canonical workflow `31467821766`"
)
BOUND_EVIDENCE = route_driver.route_bound_closeout_reference(CLOSED, EVIDENCE)
MANIFEST = "d" * 64
ATTEMPT = "123e4567-e89b-12d3-a456-426614174000"

SKETCH = f"""### Packet {SUCCESSOR}

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-PLAN-LANE-ACTIVATION-1

**Class:** `IMPLEMENT`

**Outcome:** Wire plan-lane successor promotion and escalation through the existing owners with exactly-one promotion receipts and bounded pause escalation.

**Allowed delta:** scripts/agent-control/dispatcher.py, scripts/agent-control/local_run_once.py, scripts/agent-control/route_driver.py, scripts/session_context.py, tests/test_agent_plan_promotion.py.

**Exit:** The lane records exactly-one successor-promotion receipts and bounded pause escalation, controller-owned and idempotent.

**Stop:** Any second ledger/controller/store/state/routing owner, promotion of zero or multiple successors, stale or unprovable routing, or child authority.
"""

EFFECT_SKETCH = f"""### Packet PE7-RWE-DB-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** {SUCCESSOR}

**Class:** `EFFECT`

**Outcome:** Issue one new finite one-use authorization and execute exactly the accepted schedule once.

**Allowed delta:** Only the pre-authorized Provider effects and existing delegated lifecycle may occur.

**Exit:** All cells reach honest terminal classifications with complete evidence bindings.

**Stop:** Authority or hash mismatch, budget breach, or unknown outcome.
"""

BLOCKED_SKETCH = f"""### Packet TOOL-OTHER-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** {SUCCESSOR}

**Class:** `CONTRACT`

**Outcome:** Document the route contract only, with no product authority change.

**Allowed delta:** docs/ only.

**Exit:** Accepted documentation records the single route-controller owner.

**Stop:** Any second owner or unbound merge path.
"""


def _digest(value):
    import hashlib

    return hashlib.sha256(
        json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()


def _inventory_payload(future_text):
    ordered = []
    graph = []
    rows = []
    headings = list(plan_lane.PACKET_HEADING.finditer(future_text))
    for index, match in enumerate(headings):
        packet_id = match.group("packet")
        end = headings[index + 1].start() if index + 1 < len(headings) else len(future_text)
        block = future_text[match.start() : end]
        ordered.append(packet_id)
        prerequisite = __import__("re").search(
            r"^\*\*Prerequisite:\*\*\s*(?P<value>.+)$", block, __import__("re").MULTILINE
        )
        prerequisites = (
            __import__("re").findall(plan_lane.PACKET_TOKEN, prerequisite.group("value"))
            if prerequisite
            else []
        )
        graph.append({
            "packet_id": packet_id,
            "prerequisites": [item for item in dict.fromkeys(prerequisites) if item != packet_id],
        })
        packet_class = __import__("re").search(
            r"^\*\*Class:\*\*\s*`?(?P<value>[A-Z]+)`?\s*$", block, __import__("re").MULTILINE
        ).group("value")
        rows.append([
            packet_id,
            packet_class,
            route_driver.CLASS_DEFAULT_TIER[packet_class],
            route_driver.CLASS_DEFAULT_RISK.get(packet_class, "none"),
            route_driver.CLASS_DEFAULT_VERIFICATION[packet_class],
        ])
    return {
        "schema_version": "future_route_inventory.v1",
        "packet_count": len(ordered),
        "ordered_packet_ids": ordered,
        "ordered_packet_ids_sha256": _digest(ordered),
        "dependency_graph_sha256": _digest(graph),
        "profiles_sha256": _digest(rows),
        "profiles": rows,
    }


def future_document(sketches):
    text = "## Portfolio Inventory Manifest\n\n" + "\n\n".join(sketches)
    payload = _inventory_payload(text)
    marker = "<!-- future-route-inventory:v1\n" + json.dumps(payload, sort_keys=True) + "\n-->"
    return marker + "\n\n" + text


def _packet_payload(packet_id, lane="plan_lane_active"):
    return {
        "schema_version": "weak_agent_dispatch.v1",
        "packet_id": packet_id,
        "packet_state": "READY_FOR_EXECUTION",
        "dispatch_lane": "provider_free_repository_maintenance",
        "external_effect_limit": 0,
        "authority_consumption_allowed": False,
        "secret_values_allowed": False,
        "private_paths_allowed": False,
        "plan_lane_state": lane,
        "goal": "Execute the packet's bounded contract through the existing owners.",
        "allowed_paths": ["scripts/agent-control/", "tests/"],
        "prerequisites": [],
        "forbidden_changes": ["default branch", "provider calls"],
        "verification": ["focused provider-free tests"],
        "rollback": "disable the adapter and revert the packet",
    }


def next_document(current=CLOSED, completed=("PE7-PLAN-LANE-ACTIVATION-1",), marker_payload=None):
    blocks = []
    for packet_id in completed:
        blocks.append(
            f"## Completed ({packet_id})\n\n**Historical state:** `COMPLETE`\n"
        )
    blocks.append(
        f"## Packet {current}\n\n**State:** `READY_FOR_EXECUTION`\n\n"
        f"<!-- weak-agent-dispatch:v1 {json.dumps(marker_payload or _packet_payload(current), sort_keys=True)} -->\n"
    )
    return "\n\n".join(["## Active Routing", f"1. `{current}`", *blocks])


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


class TestInventoryManifest(unittest.TestCase):
    def test_valid_manifest_returns_checked_payload(self):
        doc = future_document([SKETCH])
        manifest = route_driver.inventory_manifest(doc)
        self.assertEqual(manifest["packet_count"], 1)
        self.assertEqual(manifest["ordered_packet_ids"], [SUCCESSOR])

    def test_missing_marker_fails_closed(self):
        with self.assertRaises(route_driver.RouteDriverError) as ctx:
            route_driver.inventory_manifest("## Portfolio Inventory Manifest\n\n" + SKETCH)
        self.assertEqual(ctx.exception.reason, "route_inventory_manifest_missing")

    def test_duplicated_marker_fails_closed(self):
        doc = future_document([SKETCH]) + future_document([SKETCH])
        with self.assertRaises(route_driver.RouteDriverError) as ctx:
            route_driver.inventory_manifest(doc)
        self.assertEqual(ctx.exception.reason, "route_inventory_manifest_duplicated")

    def test_stale_count_fails_closed(self):
        doc = future_document([SKETCH, BLOCKED_SKETCH])
        payload = json.loads(
            route_driver.INVENTORY_MARKER.search(doc).group(1)
        )
        payload["packet_count"] = 99
        doc = doc.replace(
            route_driver.INVENTORY_MARKER.search(doc).group(0),
            "<!-- future-route-inventory:v1\n" + json.dumps(payload, sort_keys=True) + "\n-->",
        )
        with self.assertRaises(route_driver.RouteDriverError) as ctx:
            route_driver.inventory_manifest(doc)
        self.assertEqual(ctx.exception.reason, "route_inventory_count_stale")

    def test_stale_ids_sha_fails_closed(self):
        doc = future_document([SKETCH])
        payload = json.loads(route_driver.INVENTORY_MARKER.search(doc).group(1))
        payload["ordered_packet_ids_sha256"] = "0" * 64
        doc = doc.replace(
            route_driver.INVENTORY_MARKER.search(doc).group(0),
            "<!-- future-route-inventory:v1\n" + json.dumps(payload, sort_keys=True) + "\n-->",
        )
        with self.assertRaises(route_driver.RouteDriverError) as ctx:
            route_driver.inventory_manifest(doc)
        self.assertEqual(ctx.exception.reason, "route_inventory_ids_sha_stale")

    def test_stale_graph_sha_fails_closed(self):
        doc = future_document([SKETCH])
        payload = json.loads(route_driver.INVENTORY_MARKER.search(doc).group(1))
        payload["dependency_graph_sha256"] = "1" * 64
        doc = doc.replace(
            route_driver.INVENTORY_MARKER.search(doc).group(0),
            "<!-- future-route-inventory:v1\n" + json.dumps(payload, sort_keys=True) + "\n-->",
        )
        with self.assertRaises(route_driver.RouteDriverError) as ctx:
            route_driver.inventory_manifest(doc)
        self.assertEqual(ctx.exception.reason, "route_inventory_graph_sha_stale")

    def test_stale_profiles_fail_closed(self):
        doc = future_document([SKETCH])
        payload = json.loads(route_driver.INVENTORY_MARKER.search(doc).group(1))
        payload["profiles"] = [[SUCCESSOR, "CONTRACT", "T2", "none", "docs_evidence_review"]]
        doc = doc.replace(
            route_driver.INVENTORY_MARKER.search(doc).group(0),
            "<!-- future-route-inventory:v1\n" + json.dumps(payload, sort_keys=True) + "\n-->",
        )
        with self.assertRaises(route_driver.RouteDriverError) as ctx:
            route_driver.inventory_manifest(doc)
        self.assertEqual(ctx.exception.reason, "route_inventory_profiles_stale")


class TestPacketSketches(unittest.TestCase):
    def test_parses_bounded_sketch(self):
        sketches = route_driver.packet_sketches(future_document([SKETCH]))
        self.assertEqual(set(sketches), {SUCCESSOR})
        self.assertEqual(sketches[SUCCESSOR].packet_class, "IMPLEMENT")
        self.assertEqual(sketches[SUCCESSOR].prerequisites, ("PE7-PLAN-LANE-ACTIVATION-1",))

    def test_missing_field_fails_closed(self):
        broken = SKETCH.replace("**Stop:**", "**Missing:**")
        with self.assertRaises(route_driver.RouteDriverError) as ctx:
            route_driver.packet_sketches(future_document([broken]))
        self.assertEqual(
            ctx.exception.reason, "route_sketch_field_missing_or_ambiguous:Stop"
        )

    def test_duplicate_field_fails_closed(self):
        broken = SKETCH + "**Stop:** duplicated\n"
        with self.assertRaises(route_driver.RouteDriverError) as ctx:
            route_driver.packet_sketches(future_document([broken]))
        self.assertTrue(
            ctx.exception.reason.startswith("route_sketch_field_missing_or_ambiguous")
        )

    def test_invalid_class_fails_closed(self):
        doc = future_document([SKETCH])
        marker = route_driver.INVENTORY_MARKER.search(doc)
        prose = doc[marker.end() :].replace("`IMPLEMENT`", "`FANCY`")
        broken = doc[: marker.end()] + prose
        with self.assertRaises(route_driver.RouteDriverError) as ctx:
            route_driver.packet_sketches(broken)
        self.assertEqual(ctx.exception.reason, "route_sketch_class_invalid")


class TestEligibleSuccessor(unittest.TestCase):
    def test_first_eligible_with_complete_prerequisites_wins(self):
        future = future_document([BLOCKED_SKETCH, SKETCH])
        successor = route_driver.eligible_successor(future, next_document(), CLOSED)
        self.assertEqual(successor.packet_id, SUCCESSOR)
        self.assertEqual(successor.profile[1], "IMPLEMENT")

    def test_effect_packets_are_never_auto_promoted(self):
        future = future_document([EFFECT_SKETCH, SKETCH])
        successor = route_driver.eligible_successor(future, next_document(), CLOSED)
        self.assertEqual(successor.packet_id, SUCCESSOR)

    def test_closed_packet_is_skipped(self):
        future = future_document([SKETCH])
        with self.assertRaises(route_driver.RouteDriverError) as ctx:
            route_driver.eligible_successor(future, next_document(), SUCCESSOR)
        self.assertEqual(ctx.exception.reason, "no_eligible_successor")

    def test_incomplete_prerequisites_skip_candidate(self):
        future = future_document([SKETCH, BLOCKED_SKETCH])
        with self.assertRaises(route_driver.RouteDriverError) as ctx:
            route_driver.eligible_successor(
                future, next_document(completed=("TOOL-OTHER-1",)), CLOSED
            )
        self.assertEqual(ctx.exception.reason, "no_eligible_successor")

    def test_invalid_closed_packet_fails_closed(self):
        with self.assertRaises(route_driver.RouteDriverError) as ctx:
            route_driver.eligible_successor(future_document([SKETCH]), next_document(), "bad id")
        self.assertEqual(ctx.exception.reason, "successor_closed_packet_invalid")

    def test_prose_order_drift_fails_closed(self):
        doc = future_document([SKETCH])
        doc = doc.replace("### Packet " + SUCCESSOR, "### Packet TOOL-OTHER-1")
        with self.assertRaises(route_driver.RouteDriverError) as ctx:
            route_driver.eligible_successor(doc, next_document(), CLOSED)
        self.assertEqual(ctx.exception.reason, "route_inventory_order_stale")


class TestEvidenceBackedPromotion(unittest.TestCase):
    def test_module_map_names_the_route_driver_as_the_single_promotion_boundary(self):
        module_map = (Path(__file__).resolve().parents[1] / "docs" / "MODULE_MAP.md").read_text(
            encoding="utf-8"
        )
        self.assertIn("`route_driver.py` is the deep promotion boundary", module_map)
        self.assertIn("cannot use FUTURE_ROUTE paths as authority", module_map)

    def test_bootstrap_receipt_reuses_its_existing_status_row_idempotently(self):
        evidence = (
            f"PR #390 exact head `{'b' * 40}`; merge `{'c' * 40}`; "
            "exact-head `PASS`; canonical workflow `31467821768`"
        )
        closed_row = f"| `{CLOSED}` | `COMPLETE` | {evidence} |\n"
        status = status_document().replace(
            "|---|---|---|\n\n",
            "|---|---|---|\n" + closed_row + "\n",
            1,
        )
        self.assertEqual(
            route_driver._with_status_rows(status, closed_row, ""),
            status,
        )

    def test_accepted_complete_receipt_keeps_only_the_canonical_status_prefix(self):
        evidence = (
            f"PR #390 exact head `{'b' * 40}`; merge `{'c' * 40}`; "
            "exact-head `PASS`; canonical workflow `31467821768`"
        )
        status = status_document().replace(
            "|---|---|---|\n\n",
            f"|---|---|---|\n| `{CLOSED}` | `COMPLETE` | {evidence}; controller-owned detail |\n\n",
            1,
        )
        self.assertEqual(route_driver.accepted_complete_receipt(status, CLOSED), evidence)

    def _successor(
        self,
        packet_class="IMPLEMENT",
        *,
        prerequisites=(CLOSED,),
        outcome="Use the existing repository-maintenance control-plane owner.",
    ):
        sketch = route_driver.PacketSketch(
            packet_id="PE7-EXACT-PROMOTION-1",
            prerequisites=prerequisites,
            packet_class=packet_class,
            outcome=outcome,
            allowed_delta="Static hints only: docs/ and scripts/agent-control/.",
            exit_statement="An independently accepted bounded contract exists.",
            stop="Stop on any unproved current-main authority.",
        )
        profile = (
            sketch.packet_id,
            packet_class,
            route_driver.CLASS_DEFAULT_TIER[packet_class],
            route_driver.CLASS_DEFAULT_RISK.get(packet_class, "none"),
            route_driver.CLASS_DEFAULT_VERIFICATION[packet_class],
        )
        return route_driver.EligibleSuccessor(sketch.packet_id, sketch, profile)

    def _evidence(self, *, packet_id="PE7-EXACT-PROMOTION-1", status_text=None):
        status_text = status_document() if status_text is None else status_text
        return route_driver.CurrentMainEvidence(
            packet_id=packet_id,
            accepted_main_sha=MAIN,
            status_document_sha256=hashlib.sha256(
                status_text.encode("utf-8")
            ).hexdigest(),
            owner_paths=("scripts/agent-control/plan_lifecycle.py",),
            caller_paths=("scripts/agent-control/local_run_once.py",),
            test_paths=("tests/test_agent_plan_lifecycle.py",),
            allowed_paths=(
                "scripts/agent-control/plan_lifecycle.py",
                "scripts/agent-control/local_run_once.py",
                "tests/test_agent_plan_lifecycle.py",
            ),
            read_paths=(
                "scripts/agent-control/plan_lifecycle.py",
                "scripts/agent-control/local_run_once.py",
                "tests/test_agent_plan_lifecycle.py",
            ),
            ordered_slices=("Add the bounded route transition through the existing owner.",),
            verification=("PYTHONPATH=src uv run --no-project python -m unittest tests.test_agent_plan_lifecycle",),
            rollback="Revert the bounded route transition and retain ledger receipts.",
            cleanup="Remove only the route-owned transient worktree after readback.",
            retention="Keep redacted receipt digests on the existing ledger.",
            evidence_destinations=("Plan Execution Ledger",),
            decisions=("No schema, evaluator, authority, or recovery decision is changed.",),
        )

    def test_compiled_successor_binds_the_remaining_route_inventory(self):
        successor_sketch = SKETCH.replace(
            "**Prerequisite:** PE7-PLAN-LANE-ACTIVATION-1",
            f"**Prerequisite:** {CLOSED}",
        )
        source_future = future_document([successor_sketch, BLOCKED_SKETCH])
        source_manifest_sha256 = route_driver._json_sha256(
            route_driver.inventory_manifest(source_future)
        )

        compiled = route_driver.compile_successor(
            source_future,
            next_document(),
            status_document(),
            CLOSED,
            BOUND_EVIDENCE,
            MAIN,
            self._evidence(packet_id=SUCCESSOR),
        )

        remaining_manifest_sha256 = route_driver._json_sha256(
            route_driver.inventory_manifest(compiled.future_document)
        )
        self.assertNotEqual(remaining_manifest_sha256, source_manifest_sha256)
        self.assertEqual(compiled.manifest_sha256, remaining_manifest_sha256)
        self.assertEqual(
            compiled.capsule["route_manifest_sha256"],
            remaining_manifest_sha256,
        )
        self.assertIn(
            f'"route_manifest_sha256": "{remaining_manifest_sha256}"',
            compiled.next_document,
        )

    def test_static_future_paths_are_hints_not_promotion_evidence(self):
        result = route_driver.RoutePromotionPlanner().plan(
            self._successor(), MAIN, EVIDENCE, None, MANIFEST
        )
        self.assertEqual(result.state, "DECISION_REQUIRED")
        self.assertEqual(result.reason, "promotion_current_main_evidence_missing")

    def test_current_main_evidence_owns_every_refreshed_contract_field(self):
        evidence = self._evidence()
        result = route_driver.RoutePromotionPlanner().plan(
            self._successor(), MAIN, BOUND_EVIDENCE, evidence, MANIFEST
        )
        self.assertEqual(result.state, "READY_FOR_EXECUTION")
        self.assertIsNotNone(result.candidate)
        assert result.candidate is not None
        capsule = result.candidate.capsule
        self.assertEqual(
            capsule["allowed_paths"], list(evidence.allowed_paths),
        )
        self.assertEqual(capsule["read_paths"], list(evidence.allowed_paths))
        self.assertEqual(capsule["prerequisite_receipts"], [EVIDENCE])
        self.assertEqual(capsule["ordered_steps"], list(evidence.ordered_slices))
        self.assertEqual(capsule["expected_artifacts"], list(evidence.evidence_destinations))
        for field in (
            "allowed_outputs",
            "forbidden_changes",
            "pause_gates",
            "forbidden_next_actions",
        ):
            self.assertTrue(capsule[field])
        self.assertNotIn("docs/", capsule["allowed_paths"])
        self.assertEqual(
            result.candidate.contract["cleanup"],
            evidence.cleanup,
        )
        self.assertEqual(result.candidate.manifest_sha256, MANIFEST)
        self.assertEqual(result.candidate.capsule["route_manifest_sha256"], MANIFEST)
        self.assertEqual(result.candidate.contract["manifest_sha256"], MANIFEST)

    def test_short_goal_and_rollback_are_rejected_before_handoff(self):
        short_goal = route_driver.RoutePromotionPlanner().plan(
            self._successor(outcome="Too short."), MAIN, BOUND_EVIDENCE, self._evidence(), MANIFEST
        )
        self.assertEqual(short_goal.state, "DECISION_REQUIRED")
        self.assertEqual(short_goal.reason, "promotion_goal_too_short")

        evidence = self._evidence()
        short_rollback = route_driver.CurrentMainEvidence(
            packet_id=evidence.packet_id,
            accepted_main_sha=evidence.accepted_main_sha,
            status_document_sha256=evidence.status_document_sha256,
            owner_paths=evidence.owner_paths,
            caller_paths=evidence.caller_paths,
            test_paths=evidence.test_paths,
            allowed_paths=evidence.allowed_paths,
            read_paths=evidence.read_paths,
            ordered_slices=evidence.ordered_slices,
            verification=evidence.verification,
            rollback="Revert.",
            cleanup=evidence.cleanup,
            retention=evidence.retention,
            evidence_destinations=evidence.evidence_destinations,
            decisions=evidence.decisions,
        )
        short_rollback_result = route_driver.RoutePromotionPlanner().plan(
            self._successor(), MAIN, BOUND_EVIDENCE, short_rollback, MANIFEST
        )
        self.assertEqual(short_rollback_result.state, "DECISION_REQUIRED")
        self.assertEqual(short_rollback_result.reason, "promotion_rollback_too_short")

    def test_multi_prerequisite_contract_requires_each_bound_receipt(self):
        successor = self._successor(
            prerequisites=(CLOSED, "PE7-OTHER-1"),
        )
        missing = route_driver.RoutePromotionPlanner().plan(
            successor, MAIN, EVIDENCE, self._evidence(), MANIFEST
        )
        self.assertEqual(missing.state, "DECISION_REQUIRED")
        self.assertEqual(
            missing.reason, "promotion_prerequisite_receipts_missing_or_invalid"
        )

        second_receipt = (
            f"PR #401 exact head `{'d' * 40}`; merge `{'e' * 40}`; "
            "exact-head `PASS`; canonical workflow `31467821768`"
        )
        closed_receipt = (
            f"PR #400 exact head `{'b' * 40}`; merge `{MAIN}`; "
            "exact-head `PASS`; canonical workflow `31467821767`"
        )
        status = status_document().replace(
            "|---|---|---|\n\n",
            f"|---|---|---|\n| `PE7-OTHER-1` | `COMPLETE` | {second_receipt} |\n\n",
            1,
        )
        bound_closed_receipt = route_driver.route_bound_closeout_reference(
            CLOSED, closed_receipt
        )
        complete = route_driver.RoutePromotionPlanner().plan(
            successor,
            MAIN,
            bound_closed_receipt,
            self._evidence(status_text=status),
            MANIFEST,
            closed_packet_id=CLOSED,
            status_document=status,
        )
        self.assertEqual(complete.state, "READY_FOR_EXECUTION")
        assert complete.candidate is not None
        self.assertEqual(
            complete.candidate.capsule["prerequisite_receipts"],
            [closed_receipt, second_receipt],
        )

    def test_prerequisite_receipts_use_current_status_for_every_prior_packet(self):
        successor = self._successor(prerequisites=(CLOSED, "PE7-OTHER-1"))
        second_receipt = (
            f"PR #401 exact head `{'d' * 40}`; merge `{'e' * 40}`; "
            "exact-head `PASS`; canonical workflow `31467821768`"
        )
        closed_receipt = (
            f"PR #400 exact head `{'b' * 40}`; merge `{MAIN}`; "
            "exact-head `PASS`; canonical workflow `31467821767`"
        )
        status = status_document().replace(
            "|---|---|---|\n\n",
            f"|---|---|---|\n| `PE7-OTHER-1` | `COMPLETE` | {second_receipt} |\n\n",
            1,
        )
        bound_closed_receipt = route_driver.route_bound_closeout_reference(
            CLOSED, closed_receipt
        )
        self.assertEqual(
            route_driver.bound_prerequisite_receipts(
                successor, CLOSED, bound_closed_receipt, status, MAIN
            ),
            (closed_receipt, second_receipt),
        )
        self.assertEqual(
            route_driver._status_readiness_rows(
                CLOSED,
                SUCCESSOR,
                route_driver.verified_predecessor_receipt(
                    status, CLOSED, bound_closed_receipt, MAIN
                ),
                "READY_FOR_EXECUTION",
            )[0],
            f"| `{CLOSED}` | `COMPLETE` | {closed_receipt} |\n",
        )

    def test_status_gap_rejects_an_unbound_or_wrongly_bound_receipt(self):
        successor = self._successor()
        receipt = (
            f"PR #400 exact head `{'b' * 40}`; merge `{MAIN}`; "
            "exact-head `PASS`; canonical workflow `31467821767`"
        )
        unbound = route_driver.RoutePromotionPlanner().plan(
            successor,
            MAIN,
            receipt,
            self._evidence(),
            MANIFEST,
            closed_packet_id=CLOSED,
            status_document=status_document(),
        )
        self.assertEqual(unbound.state, "DECISION_REQUIRED")
        self.assertEqual(
            unbound.reason, "promotion_prerequisite_receipts_missing_or_invalid"
        )
        wrong_packet = route_driver.route_bound_closeout_reference(
            "PE7-OTHER-1", receipt
        )
        wrongly_bound = route_driver.RoutePromotionPlanner().plan(
            successor,
            MAIN,
            wrong_packet,
            self._evidence(),
            MANIFEST,
            closed_packet_id=CLOSED,
            status_document=status_document(),
        )
        self.assertEqual(wrongly_bound.state, "DECISION_REQUIRED")
        self.assertEqual(
            wrongly_bound.reason, "promotion_prerequisite_receipts_missing_or_invalid"
        )

    def test_predecessor_and_status_bindings_fail_closed(self):
        successor = self._successor()
        unproved = route_driver.RoutePromotionPlanner().plan(
            successor,
            MAIN,
            "unproved predecessor receipt",
            self._evidence(),
            MANIFEST,
        )
        self.assertEqual(unproved.state, "DECISION_REQUIRED")
        self.assertEqual(unproved.reason, "promotion_predecessor_receipt_unproved")

        receipt = (
            f"PR #400 exact head `{'b' * 40}`; merge `{MAIN}`; "
            "exact-head `PASS`; canonical workflow `31467821767`"
        )
        mismatched_status = route_driver.RoutePromotionPlanner().plan(
            successor,
            MAIN,
            receipt,
            self._evidence(),
            MANIFEST,
            closed_packet_id=CLOSED,
            status_document=status_document() + "\nchanged",
        )
        self.assertEqual(mismatched_status.state, "DECISION_REQUIRED")
        self.assertEqual(
            mismatched_status.reason, "promotion_status_document_binding_invalid"
        )

    def test_t3_closeout_uses_only_the_validated_digest_bound_reference(self):
        successor = self._successor("CLOSEOUT")
        now = datetime.now(timezone.utc)
        request = route_driver.T3Request(
            packet_id=CLOSED,
            accepted_main_sha=MAIN,
            candidate_digest="b" * 64,
            action_digest="c" * 64,
            scope_digest="d" * 64,
            authority_owner_digest="e" * 64,
            requested_action="one finite authorized effect",
        )
        receipt = route_driver.T3Receipt(
            packet_id=CLOSED,
            accepted_main_sha=MAIN,
            candidate_digest=request.candidate_digest,
            action_digest=request.action_digest,
            scope_digest=request.scope_digest,
            authority_receipt_digest="f" * 64,
            outcome_receipt_digest="1" * 64,
            authority_owner_digest=request.authority_owner_digest,
            operator="existing-operator",
            decision_source="local_sol_5_6_max",
            decision_evidence_digest="2" * 64,
            decision_digest=route_driver.t3_decision_digest(
                request, "local_sol_5_6_max", "2" * 64, "GO"
            ),
            issued_at=now.isoformat(),
            expires_at=(now + timedelta(minutes=5)).isoformat(),
            disposition="GO",
        )
        reference = route_driver.t3_closeout_reference(receipt)
        ready = route_driver.RoutePromotionPlanner().plan(
            successor,
            MAIN,
            reference,
            self._evidence(),
            MANIFEST,
            closed_packet_id=CLOSED,
            status_document=status_document(),
            retained_t3_request=request,
            retained_t3_receipt=receipt,
        )
        self.assertEqual(ready.state, "READY_FOR_EXECUTION")
        invalid = route_driver.RoutePromotionPlanner().plan(
            successor,
            MAIN,
            reference + " changed",
            self._evidence(),
            MANIFEST,
            closed_packet_id=CLOSED,
            status_document=status_document(),
            retained_t3_request=request,
            retained_t3_receipt=receipt,
        )
        self.assertEqual(invalid.state, "DECISION_REQUIRED")
        self.assertEqual(invalid.reason, "promotion_t3_closeout_receipt_invalid")

        forged = dataclasses.replace(receipt, decision_digest="3" * 64)
        forged_result = route_driver.RoutePromotionPlanner().plan(
            successor,
            MAIN,
            route_driver.t3_closeout_reference(forged),
            self._evidence(),
            MANIFEST,
            closed_packet_id=CLOSED,
            status_document=status_document(),
            retained_t3_request=request,
            retained_t3_receipt=forged,
        )
        self.assertEqual(forged_result.state, "DECISION_REQUIRED")
        self.assertEqual(forged_result.reason, "promotion_t3_closeout_receipt_invalid")

        non_go = dataclasses.replace(receipt, disposition="NO_GO")
        non_go_result = route_driver.RoutePromotionPlanner().plan(
            successor,
            MAIN,
            route_driver.t3_closeout_reference(non_go),
            self._evidence(),
            MANIFEST,
            closed_packet_id=CLOSED,
            status_document=status_document(),
            retained_t3_request=request,
            retained_t3_receipt=non_go,
        )
        self.assertEqual(non_go_result.state, "DECISION_REQUIRED")
        self.assertEqual(non_go_result.reason, "promotion_t3_closeout_receipt_invalid")

    def test_serialized_promoted_capsule_round_trips_through_plan_and_handoff_validation(self):
        successor = self._successor()
        evidence = self._evidence()
        result = route_driver.RoutePromotionPlanner().plan(
            successor, MAIN, BOUND_EVIDENCE, evidence, MANIFEST
        )
        self.assertEqual(result.state, "READY_FOR_EXECUTION")
        assert result.candidate is not None
        capsule = result.candidate.capsule
        document = (
            "## Active Routing\n\n"
            f"1. `{successor.packet_id}` — `READY_FOR_EXECUTION`\n\n"
            f"## Packet {successor.packet_id}\n\n"
            "**State:** `READY_FOR_EXECUTION`\n\n"
            f"### 11. Weak-Agent Dispatch Capsule\n\n<!-- weak-agent-dispatch:v1\n"
            f"{json.dumps(capsule, sort_keys=True)}\n-->\n"
        )
        parsed = plan_lane.parse(
            document, MAIN, completed_packet_ids=frozenset({CLOSED})
        )
        self.assertEqual(parsed.packet_id, successor.packet_id)
        self.assertEqual(parsed.allowed_paths, list(evidence.allowed_paths))
        self.assertEqual(
            check_agent_handoff.weak_agent_dispatch_failures(
                document,
                {successor.packet_id: {"state": "READY_FOR_EXECUTION"}},
            ),
            [],
        )

    def test_manifest_is_required_and_changes_the_immutable_candidate_digest(self):
        missing = route_driver.RoutePromotionPlanner().plan(
            self._successor(), MAIN, EVIDENCE, self._evidence(), None
        )
        self.assertEqual(missing.state, "DECISION_REQUIRED")
        self.assertEqual(missing.reason, "promotion_manifest_missing_or_invalid")
        first = route_driver.RoutePromotionPlanner().plan(
            self._successor(), MAIN, BOUND_EVIDENCE, self._evidence(), MANIFEST
        )
        second = route_driver.RoutePromotionPlanner().plan(
            self._successor(), MAIN, BOUND_EVIDENCE, self._evidence(), "e" * 64
        )
        self.assertNotEqual(first.candidate.spec_digest, second.candidate.spec_digest)

    def test_effect_is_prepared_then_paused_for_t3_instead_of_skipped(self):
        successor = self._successor("EFFECT")
        evidence = self._evidence(packet_id=successor.packet_id)
        result = route_driver.RoutePromotionPlanner().plan(successor, MAIN, BOUND_EVIDENCE, evidence, MANIFEST)
        self.assertEqual(result.state, "T3_REQUIRED")
        self.assertIsNotNone(result.t3_request)
        self.assertEqual(result.t3_request.packet_id, successor.packet_id)
        self.assertEqual(result.candidate.capsule["external_effect_limit"], 0)

    def test_t3_request_must_be_bound_to_the_current_packet_and_main(self):
        packet = "PE7-EXACT-PROMOTION-1"
        request = {
            "schema_version": "route_t3_request.v1",
            "packet_id": packet,
            "accepted_main_sha": MAIN,
            "candidate_digest": "b" * 64,
            "action_digest": "c" * 64,
            "scope_digest": "d" * 64,
            "authority_owner_digest": "a" * 64,
            "requested_action": "Run exactly the accepted bounded effect once.",
        }
        document = (
            "## Active Routing\n\n1. `" + packet + "` — `T3_REQUIRED`\n\n"
            "## Packet " + packet + "\n\n**State:** `T3_REQUIRED`\n\n"
            "<!-- route-t3-request:v1\n" + json.dumps(request) + "\n-->\n"
        )
        parsed = route_driver.current_t3_request(document, MAIN)
        self.assertEqual(parsed.candidate_digest, "b" * 64)
        stale = document.replace(MAIN, "c" * 40)
        with self.assertRaises(route_driver.RouteDriverError) as ctx:
            route_driver.current_t3_request(stale, MAIN)
        self.assertEqual(ctx.exception.reason, "route_t3_request_invalid")

    def test_t3_receipt_requires_exact_finite_binding_and_expiry(self):
        request = route_driver.T3Request(
            packet_id="PE7-EXACT-PROMOTION-1", accepted_main_sha=MAIN,
            candidate_digest="b" * 64, action_digest="c" * 64,
            scope_digest="d" * 64, authority_owner_digest="a" * 64,
            requested_action="one finite action",
        )
        now = datetime(2026, 8, 11, tzinfo=timezone.utc)
        receipt = {
            "schema_version": "route_t3_receipt.v1",
            "packet_id": request.packet_id,
            "accepted_main_sha": request.accepted_main_sha,
            "candidate_digest": request.candidate_digest,
            "action_digest": "c" * 64,
            "scope_digest": "d" * 64,
            "authority_receipt_digest": "e" * 64,
            "outcome_receipt_digest": "f" * 64,
            "authority_owner_digest": "a" * 64,
            "operator": "authorized-operator",
            "decision_source": "local_sol_5_6_max",
            "decision_evidence_digest": "1" * 64,
            "decision_digest": "",
            "issued_at": now.isoformat(),
            "expires_at": (now + timedelta(minutes=5)).isoformat(),
            "disposition": "GO",
        }
        receipt["decision_digest"] = route_driver.t3_decision_digest(
            request,
            receipt["decision_source"],
            receipt["decision_evidence_digest"],
            receipt["disposition"],
        )
        validated, reason = route_driver.validate_t3_receipt(receipt, request, now=now)
        self.assertEqual(reason, "t3_receipt_valid")
        self.assertEqual(validated.disposition, "GO")
        receipt["decision_source"] = "gpt_web"
        receipt["decision_digest"] = route_driver.t3_decision_digest(
            request,
            receipt["decision_source"],
            receipt["decision_evidence_digest"],
            receipt["disposition"],
        )
        validated, reason = route_driver.validate_t3_receipt(receipt, request, now=now)
        self.assertEqual(reason, "t3_receipt_valid")
        self.assertEqual(validated.decision_source, "gpt_web")
        receipt["decision_digest"] = "0" * 64
        validated, reason = route_driver.validate_t3_receipt(receipt, request, now=now)
        self.assertIsNone(validated)
        self.assertEqual(reason, "t3_receipt_decision_binding_invalid")
        receipt["decision_digest"] = route_driver.t3_decision_digest(
            request,
            receipt["decision_source"],
            receipt["decision_evidence_digest"],
            receipt["disposition"],
        )
        receipt["candidate_digest"] = "f" * 64
        validated, reason = route_driver.validate_t3_receipt(receipt, request, now=now)
        self.assertIsNone(validated)
        self.assertEqual(reason, "t3_receipt_binding_mismatch")

        receipt["candidate_digest"] = request.candidate_digest
        receipt["issued_at"] = (now + timedelta(seconds=1)).isoformat()
        receipt["expires_at"] = (now + timedelta(minutes=2)).isoformat()
        validated, reason = route_driver.validate_t3_receipt(receipt, request, now=now)
        self.assertIsNone(validated)
        self.assertEqual(reason, "t3_receipt_issued_in_future")

        receipt["issued_at"] = now.isoformat()
        receipt["expires_at"] = (now + timedelta(minutes=16)).isoformat()
        validated, reason = route_driver.validate_t3_receipt(receipt, request, now=now)
        self.assertIsNone(validated)
        self.assertEqual(reason, "t3_receipt_window_exceeded")

    def test_compaction_keeps_one_current_window_for_many_crossings(self):
        document = "# Next\n\n## Common Execution Protocol\n\n- retained\n"
        for number in range(128):
            document = route_driver.compact_next_window(
                document,
                closed_packet_id=f"PE7-TEST-{number}-1",
                predecessor_receipt=f"PR #{number} exact head " + "a" * 40,
                active_packet_block=(
                    f"## Packet PE7-TEST-{number + 1}-1\n\n"
                    "**State:** `READY_FOR_EXECUTION`\n"
                ),
            )
        self.assertEqual(document.count("## Packet "), 1)
        self.assertEqual(document.count("## Completed"), 1)
        self.assertLess(len(document.encode("utf-8")), route_driver.NEXT_DECISION_MAX_BYTES)

    def test_effect_closeout_transition_keeps_the_effect_in_progress(self):
        effect_row, closeout_row = route_driver._status_readiness_rows(
            "PE7-EFFECT-1", "PE7-EFFECT-CLOSEOUT-1", "T3 outcome pending", "READY_FOR_EXECUTION",
            closed_packet_state="IN_PROGRESS",
        )
        self.assertEqual(effect_row, "")
        self.assertEqual(closeout_row, "")

    def test_compaction_retains_an_effect_in_progress_with_its_exact_t3_request(self):
        request = route_driver.T3Request(
            packet_id="PE7-EFFECT-1", accepted_main_sha=MAIN,
            candidate_digest="b" * 64, action_digest="c" * 64,
            scope_digest="d" * 64, authority_owner_digest="e" * 64,
            requested_action="one bounded effect",
        )
        document = route_driver.compact_next_window(
            "# Next\n\n## Common Execution Protocol\n\n- retained\n",
            closed_packet_id=request.packet_id,
            predecessor_receipt="T3 receipt " + "f" * 64,
            active_packet_block=(
                "## Packet PE7-EFFECT-CLOSEOUT-1\n\n"
                "**State:** `READY_FOR_EXECUTION`\n"
            ),
            closed_packet_state="IN_PROGRESS",
            retained_marker=route_driver._t3_request_marker(request),
        )
        self.assertIn("## Retained (PE7-EFFECT-1)", document)
        self.assertIn("**Historical state:** `IN_PROGRESS`", document)
        self.assertIn(request.candidate_digest, document)
        self.assertNotIn("## Completed (PE7-EFFECT-1)", document)

    def test_direct_effect_closeout_source_recovers_the_retained_request(self):
        request = route_driver.T3Request(
            packet_id="PE7-EFFECT-1", accepted_main_sha=MAIN,
            candidate_digest="b" * 64, action_digest="c" * 64,
            scope_digest="d" * 64, authority_owner_digest="e" * 64,
            requested_action="one bounded effect",
        )
        closeout = "PE7-EFFECT-CLOSEOUT-1"
        document = route_driver.compact_next_window(
            f"## Active Routing\n\n1. `{closeout}` — `READY_FOR_EXECUTION`\n\n"
            "## Common Execution Protocol\n\n- retained\n",
            closed_packet_id=request.packet_id,
            predecessor_receipt="T3 receipt " + "f" * 64,
            active_packet_block=(
                f"## Packet {closeout}\n\n**State:** `READY_FOR_EXECUTION`\n\n"
                f"**Prerequisite:** {request.packet_id} — IN_PROGRESS.\n\n"
                "**Class:** `CLOSEOUT`\n"
            ),
            closed_packet_state="IN_PROGRESS",
            retained_marker=route_driver._t3_request_marker(request),
        )
        self.assertEqual(
            route_driver.direct_effect_closeout_request(document, closeout, MAIN),
            request,
        )
        with self.assertRaises(route_driver.RouteDriverError):
            route_driver.direct_effect_closeout_request(
                document.replace("## Retained", "## Historical", 1), closeout, MAIN
            )

    def test_owner_outcome_proof_requires_an_accepted_digest_bound_receipt(self):
        now = datetime.now(timezone.utc)
        request = route_driver.T3Request(
            packet_id="PE7-EFFECT-1", accepted_main_sha=MAIN,
            candidate_digest="b" * 64, action_digest="c" * 64,
            scope_digest="d" * 64, authority_owner_digest="e" * 64,
            requested_action="one bounded effect",
        )
        raw = {
            "schema_version": "route_t3_receipt.v1", "packet_id": request.packet_id,
            "accepted_main_sha": MAIN, "candidate_digest": request.candidate_digest,
            "action_digest": request.action_digest, "scope_digest": request.scope_digest,
            "authority_receipt_digest": "f" * 64,
            "outcome_receipt_digest": "1" * 64,
            "authority_owner_digest": request.authority_owner_digest,
            "operator": "approved-model-decision-transport",
            "decision_source": "local_sol_5_6_max",
            "decision_evidence_digest": "2" * 64,
            "decision_digest": "",
            "issued_at": now.isoformat(), "expires_at": (now + timedelta(minutes=5)).isoformat(),
            "disposition": "GO",
        }
        raw["decision_digest"] = route_driver.t3_decision_digest(
            request,
            raw["decision_source"],
            raw["decision_evidence_digest"],
            raw["disposition"],
        )
        receipt, reason = route_driver.validate_recorded_t3_receipt(raw, request)
        self.assertEqual(reason, "t3_receipt_valid")
        status = (
            "## Accepted Packet Receipts\n\n| Packet | State | Accepted evidence |\n|---|---|---|\n"
            f"| `{request.packet_id}` | `COMPLETE` | owner-validated existing product evidence for `{receipt.outcome_receipt_digest}` |\n"
        )
        owner_actor = "product-owner"
        owner_evidence_digest = "3" * 64
        owner_receipt = {
            "action": "route-t3-owner-outcome",
            "status": "validated",
            "details": {
                "schema_version": "route_t3_owner_outcome.v1",
                "packet_id": request.packet_id,
                "accepted_main_sha": request.accepted_main_sha,
                "candidate_digest": request.candidate_digest,
                "outcome_receipt_digest": receipt.outcome_receipt_digest,
                "owner_actor": owner_actor,
                "owner_evidence_digest": owner_evidence_digest,
                "owner_receipt_digest": route_driver.owner_outcome_receipt_digest(
                    request.packet_id, request.accepted_main_sha,
                    request.candidate_digest, receipt.outcome_receipt_digest,
                    owner_actor, owner_evidence_digest,
                ),
            },
        }
        self.assertTrue(route_driver.owner_outcome_receipt_proved(status, request, receipt, owner_receipt))
        self.assertTrue(route_driver.owner_outcome_receipt_proved(
            status.replace("owner-validated", "operator asserted"), request, receipt, owner_receipt
        ))

    def test_compaction_refreshes_the_forward_order_window_projection(self):
        document = (
            "# Next\n\n## Authoritative Forward Order\n\n```text\n"
            "[window: Route automation — READY_FOR_EXECUTION, provider-free control-plane implementation]\n"
            "→ [route-autopilot adversarial soak — provider-free]\n"
            "→ [PREFLIGHT B1/B2/provenance contract → bounded repair → provider-free PREFLIGHT]\n"
            "→ remaining ordered FUTURE_ROUTE packets\n"
            "```\n\n## Active Routing\n\n1. `PE7-ROUTE-AUTOMATION-1` — `READY_FOR_EXECUTION`\n\n"
            "## Common Execution Protocol\n\n- retained\n"
        )
        compacted = route_driver.compact_next_window(
            document,
            closed_packet_id="PE7-ROUTE-AUTOMATION-1",
            predecessor_receipt="PR #390 exact head " + "a" * 40,
            active_packet_block=(
                "## Packet PE7-ROUTE-AUTOPILOT-SOAK-1\n\n"
                "**State:** `READY_FOR_EXECUTION`\n"
            ),
            active_risk_class="none",
        )
        window = compacted[
            compacted.find("## Authoritative Forward Order"):
            compacted.find("## Active Routing")
        ]
        self.assertIn(
            "[window: PE7-ROUTE-AUTOPILOT-SOAK-1 — READY_FOR_EXECUTION, provider-free]",
            window,
        )
        self.assertNotIn("Route automation — READY_FOR_EXECUTION", window)
        self.assertNotIn("route-autopilot adversarial soak — provider-free]", window)
        self.assertIn("→ [PREFLIGHT B1/B2/provenance contract", window)

    def test_compaction_leaves_an_unparseable_forward_order_untouched(self):
        document = (
            "# Next\n\n## Authoritative Forward Order\n\n```text\n"
            "[window: unparseable\n"
            "```\n\n## Active Routing\n\n1. `PE7-A` — `READY_FOR_EXECUTION`\n\n"
            "## Common Execution Protocol\n\n- retained\n"
        )
        compacted = route_driver.compact_next_window(
            document,
            closed_packet_id="PE7-ROUTE-AUTOMATION-1",
            predecessor_receipt="PR #390 exact head " + "a" * 40,
            active_packet_block=(
                "## Packet PE7-B\n\n**State:** `READY_FOR_EXECUTION`\n"
            ),
            active_risk_class="none",
        )
        self.assertIn("[window: unparseable", compacted)

    def test_compaction_traverses_the_current_canonical_portfolio_without_growth(self):
        """Use the accepted inventory, not a synthetic count, for the soak proof."""

        future = (Path(__file__).resolve().parents[1] / "docs" / "FUTURE_ROUTE.md").read_text(
            encoding="utf-8"
        )
        manifest = route_driver.inventory_manifest(future)
        packet_ids = manifest["ordered_packet_ids"]
        self.assertEqual(len(packet_ids), manifest["packet_count"])
        self.assertGreater(len(packet_ids), 0)
        document = "# Next\n\n## Common Execution Protocol\n\n- retained\n"
        for index, closed_packet_id in enumerate(packet_ids):
            active_packet_id = (
                packet_ids[index + 1]
                if index + 1 < len(packet_ids)
                else "PE7-ROUTE-EXHAUSTED-1"
            )
            document = route_driver.compact_next_window(
                document,
                closed_packet_id=closed_packet_id,
                predecessor_receipt=f"canonical receipt {index} " + "a" * 40,
                active_packet_block=(
                    f"## Packet {active_packet_id}\n\n"
                    "**State:** `READY_FOR_EXECUTION`\n"
                ),
            )
        self.assertEqual(document.count("## Packet "), 1)
        self.assertEqual(document.count("## Completed"), 1)
        self.assertLess(len(document.encode("utf-8")), route_driver.NEXT_DECISION_MAX_BYTES)


class TestCurrentMainEvidenceVerifier(unittest.TestCase):
    """The proposal is useful only when the accepted Git tree proves it."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="route-evidence-test-")
        self.repo = Path(self.temporary.name)
        for relative, content in {
            "docs/MODULE_MAP.md": (
                "| Route | `scripts/agent-control/route_driver.py` | "
                "`route_driver.py` is the route promotion owner |\n"
                "| Typed boundary | `engine/src/typed.rs` and `engine/src/caller.rs` | "
                "`typed.rs` is the Rust boundary owner |\n"
            ),
            "docs/CURRENT_STATUS.md": status_document(),
            "docs/NEXT_DECISION.md": (
                "rollback-marker cleanup-marker retention-marker evidence-marker "
                "schema-marker evaluator-marker authority-marker recovery-marker\n"
            ),
            "docs/FUTURE_ROUTE.md": future_document([SKETCH]),
            "scripts/agent-control/route_driver.py": "class RoutePromotionPlanner: pass\n",
            "scripts/agent-control/local_run_once.py": "from route_driver import RoutePromotionPlanner\nRoutePromotionPlanner()\n",
            "tests/test_route_driver.py": "from route_driver import RoutePromotionPlanner\nRoutePromotionPlanner()\n",
            "engine/src/typed.rs": (
                "pub struct TypedBoundary;\n"
                "impl TypedBoundary {\n"
                "    pub fn new() -> Self { Self }\n"
                "}\n"
            ),
            "engine/src/caller.rs": (
                "use crate::typed::TypedBoundary;\n"
                "pub fn build_boundary<'a>() -> TypedBoundary { TypedBoundary::new() }\n"
            ),
            "engine/tests/typed.rs": (
                "#[test]\n"
                "fn boundary_is_constructible() {\n"
                "    let _ = TypedBoundary::new();\n"
                "}\n"
            ),
        }.items():
            path = self.repo / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")
        for command in (
            ["git", "init", "-q"],
            ["git", "config", "user.email", "route@example.invalid"],
            ["git", "config", "user.name", "Route Test"],
            ["git", "add", "."],
            ["git", "commit", "-qm", "route evidence fixture"],
        ):
            subprocess.run(command, cwd=self.repo, check=True)
        self.main = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=self.repo, text=True
        ).strip()
        self.successor = route_driver.EligibleSuccessor(
            "PE7-EXACT-PROMOTION-1",
            route_driver.PacketSketch(
                packet_id="PE7-EXACT-PROMOTION-1",
                prerequisites=(CLOSED,),
                packet_class="IMPLEMENT",
                outcome="Prove the accepted tree, not a route hint.",
                allowed_delta="Static hint only.",
                exit_statement="Current-main evidence is independently validated.",
                stop="Any unproved fact is a decision.",
            ),
            ("PE7-EXACT-PROMOTION-1", "IMPLEMENT", "T1", "none", "source_focused_full"),
        )

    def _predecessor_receipt(self):
        receipt = (
            f"PR #389 exact head `{'b' * 40}`; merge `{self.main}`; "
            "exact-head `PASS`; canonical workflow `31467821766`"
        )
        return route_driver.route_bound_closeout_reference(CLOSED, receipt)

    def tearDown(self):
        self.temporary.cleanup()

    def _proposal(self):
        allowed = [
            "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "docs/FUTURE_ROUTE.md",
            "docs/CURRENT_STATUS.md",
            "scripts/agent-control/route_driver.py",
            "scripts/agent-control/local_run_once.py", "tests/test_route_driver.py",
        ]
        return {
            "schema_version": "route_promotion_evidence.v1",
            "packet_id": self.successor.packet_id,
            "accepted_main_sha": self.main,
            "owner_evidence": [{
                "path": "scripts/agent-control/route_driver.py",
                "module_map_token": "route_driver.py",
            }],
            "caller_evidence": [{
                "owner_path": "scripts/agent-control/route_driver.py",
                "caller_path": "scripts/agent-control/local_run_once.py",
                "symbol": "RoutePromotionPlanner",
            }],
            "test_evidence": [{
                "target_path": "scripts/agent-control/route_driver.py",
                "test_path": "tests/test_route_driver.py",
                "symbol": "RoutePromotionPlanner",
            }],
            "allowed_paths": allowed,
            "read_paths": allowed,
            "ordered_slices": [{
                "paths": ["scripts/agent-control/route_driver.py", "tests/test_route_driver.py"],
                "description": "Keep the promotion boundary and its exact test aligned.",
            }],
            "verification": ["git diff --check"],
            "operations": {
                name: {
                    "source_path": "docs/NEXT_DECISION.md",
                    "needle": f"{name}-marker",
                    "description": f"Keep the accepted {name} evidence.",
                }
                for name in ("rollback", "cleanup", "retention")
            },
            "evidence_destinations": [{
                "source_path": "docs/NEXT_DECISION.md",
                "needle": "evidence-marker",
                "description": "Plan Execution Ledger",
            }],
            "decisions": {
                kind: {
                    "state": "UNCHANGED",
                    "source_path": "docs/NEXT_DECISION.md",
                    "needle": f"{kind}-marker",
                }
                for kind in ("schema", "evaluator", "authority", "recovery")
            },
        }

    def _rust_proposal(self):
        proposal = self._proposal()
        proposal["owner_evidence"] = [{
            "path": "engine/src/typed.rs",
            "module_map_token": "typed.rs",
        }]
        proposal["caller_evidence"] = [{
            "owner_path": "engine/src/typed.rs",
            "caller_path": "engine/src/caller.rs",
            "symbol": "TypedBoundary",
        }]
        proposal["test_evidence"] = [{
            "target_path": "engine/src/typed.rs",
            "test_path": "engine/tests/typed.rs",
            "symbol": "TypedBoundary",
        }]
        proposal["allowed_paths"] = [
            "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "docs/FUTURE_ROUTE.md",
            "docs/CURRENT_STATUS.md", "engine/src/typed.rs", "engine/src/caller.rs",
            "engine/tests/typed.rs",
        ]
        proposal["read_paths"] = list(proposal["allowed_paths"])
        proposal["ordered_slices"] = [{
            "paths": ["engine/src/typed.rs", "engine/src/caller.rs", "engine/tests/typed.rs"],
            "description": "Keep the Rust boundary and its exact caller/test evidence aligned.",
        }]
        for operation in proposal["operations"].values():
            operation["source_path"] = "docs/NEXT_DECISION.md"
        for destination in proposal["evidence_destinations"]:
            destination["source_path"] = "docs/NEXT_DECISION.md"
        for decision in proposal["decisions"].values():
            decision["source_path"] = "docs/NEXT_DECISION.md"
        return proposal

    def test_read_only_evidence_paths_are_not_edit_scope(self):
        proposal = self._proposal()
        docs = [
            "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "docs/FUTURE_ROUTE.md",
            "docs/CURRENT_STATUS.md",
        ]
        evidence_paths = docs + [
            "scripts/agent-control/route_driver.py",
            "scripts/agent-control/local_run_once.py",
            "tests/test_route_driver.py",
        ]
        proposal["allowed_paths"] = docs
        proposal["read_paths"] = evidence_paths
        proposal["ordered_slices"] = [{
            "paths": docs,
            "description": "Update only the canonical route documents.",
        }]
        result = route_driver.CurrentMainEvidenceVerifier(self.repo, self.main).verify(
            json.dumps(proposal), self.successor, self._predecessor_receipt()
        )
        self.assertEqual(result.state, "READY_FOR_EXECUTION")
        self.assertIsNotNone(result.evidence)
        assert result.evidence is not None
        self.assertEqual(result.evidence.allowed_paths, tuple(sorted(docs)))
        self.assertEqual(result.evidence.read_paths, tuple(sorted(evidence_paths)))
        self.assertIsNotNone(result.candidate)
        assert result.candidate is not None
        self.assertEqual(result.candidate.capsule["allowed_paths"], sorted(docs))
        self.assertEqual(result.candidate.capsule["read_paths"], sorted(evidence_paths))

    def test_exact_tree_proves_all_refreshed_fields(self):
        result = route_driver.CurrentMainEvidenceVerifier(self.repo, self.main).verify(
            json.dumps(self._proposal()), self.successor, self._predecessor_receipt()
        )
        self.assertEqual(result.state, "READY_FOR_EXECUTION")
        self.assertIsNotNone(result.evidence)
        self.assertEqual(
            result.evidence.caller_paths,
            ("scripts/agent-control/local_run_once.py",),
        )
        self.assertEqual(
            result.evidence.decisions[0],
            "authority unchanged (docs/NEXT_DECISION.md:authority-marker)",
        )

    def test_exact_tree_proves_rust_owner_caller_and_test(self):
        result = route_driver.CurrentMainEvidenceVerifier(self.repo, self.main).verify(
            json.dumps(self._rust_proposal()), self.successor, self._predecessor_receipt()
        )
        self.assertEqual(result.state, "READY_FOR_EXECUTION")
        self.assertIsNotNone(result.evidence)
        self.assertEqual(result.evidence.owner_paths, ("engine/src/typed.rs",))
        self.assertEqual(result.evidence.caller_paths, ("engine/src/caller.rs",))
        self.assertEqual(result.evidence.test_paths, ("engine/tests/typed.rs",))

    def test_rust_comments_and_strings_do_not_prove_consumption(self):
        proposal = self._rust_proposal()
        caller = self.repo / "engine/src/caller.rs"
        caller.write_text(
            "// TypedBoundary::new()\n"
            "const NOTE: &str = \"TypedBoundary::new()\";\n",
            encoding="utf-8",
        )
        subprocess.run(["git", "add", "."], cwd=self.repo, check=True)
        subprocess.run(["git", "commit", "-qm", "replace caller fixture"], cwd=self.repo, check=True)
        self.main = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=self.repo, text=True
        ).strip()
        proposal["accepted_main_sha"] = self.main
        result = route_driver.CurrentMainEvidenceVerifier(self.repo, self.main).verify(
            json.dumps(proposal), self.successor, self._predecessor_receipt()
        )
        self.assertEqual(result.state, "DECISION_REQUIRED")
        self.assertEqual(result.reason, "promotion_caller_not_proved")

    def test_rust_constructor_and_macro_calls_count_but_declarations_do_not(self):
        self.assertTrue(
            route_driver.CurrentMainEvidenceVerifier._consumes_symbol(
                "fn build() { TypedBoundary::new(); }\n",
                "TypedBoundary",
                "rust",
            )
        )
        for macro_call in ("typed_boundary!();", "typed_boundary![];", "typed_boundary! {};"):
            self.assertTrue(
                route_driver.CurrentMainEvidenceVerifier._consumes_symbol(
                    f"fn build() {{ {macro_call} }}\n",
                    "typed_boundary",
                    "rust",
                )
            )
        for declaration in (
            "fn TypedBoundary() {}\n",
            "struct TypedBoundary {}\n",
            "pub struct TypedBoundary {}\n",
            "unsafe impl TypedBoundary {}\n",
            "macro_rules! TypedBoundary {}\n",
            "fn build() -> TypedBoundary {}\n",
            "fn build() -> TypedBoundary::Assoc {}\n",
            "struct\nTypedBoundary {}\n",
            "fn build() ->\nTypedBoundary {}\n",
            "fn build() ->\nTypedBoundary::Assoc {}\n",
            "fn build() -> Option<TypedBoundary::Assoc> {}\n",
            "type Alias = TypedBoundary::Assoc;\n",
            "fn build(value: TypedBoundary::Assoc) {}\n",
            "fn build() { let value: Option<TypedBoundary::Assoc> = value; }\n",
            "fn build() { let value = TypedBoundary {}; }\n",
            "enum Wrapper { TypedBoundary(i32) }\n",
            "match value { TypedBoundary::Exited(code) => {} }\n",
            "macro_rules! wrapper { (TypedBoundary::Exited($value:expr)) => {} }\n",
            "fn build() { TypedBoundary(code); }\n",
            "fn build() { TypedBoundary! {}; }\n",
            "match value { typed_boundary(code) => {} }\n",
            "macro_rules! wrapper { (typed_boundary($value:expr)) => {} }\n",
            "if let typed_boundary(code) = value {}\n",
            "while let typed_boundary(code) = value {}\n",
            "for typed_boundary(code) in values {}\n",
            "|typed_boundary(code)| value\n",
        ):
            self.assertFalse(
                route_driver.CurrentMainEvidenceVerifier._consumes_symbol(
                    declaration,
                    "typed_boundary",
                    "rust",
                )
            )
            self.assertFalse(
                route_driver.CurrentMainEvidenceVerifier._consumes_symbol(
                    declaration,
                    "TypedBoundary",
                    "rust",
                )
            )
        for pattern in (
            "match value { Some(typed_boundary(code)) => {} }\n",
            "for Some(typed_boundary(code)) in values {}\n",
            "|Some(typed_boundary(code))| value\n",
            "fn caller() { let f = |(typed_boundary(code), other)| code; }\n",
            "fn caller() { let f = move |Some(typed_boundary(code))| code; }\n",
            "fn caller() { let f = async |Some(typed_boundary(code))| code; }\n",
            "fn caller() { let f = async move |Some(typed_boundary(code))| code; }\n",
        ):
            self.assertFalse(
                route_driver.CurrentMainEvidenceVerifier._consumes_symbol(
                    pattern,
                    "typed_boundary",
                    "rust",
                )
            )
        for guard in (
            "match value { item if typed_boundary(code) => {} }\n",
            "match value { item if ProcessOutcome::exited(1).successful_exit() => {} }\n",
        ):
            symbol = "ProcessOutcome" if "ProcessOutcome" in guard else "typed_boundary"
            self.assertTrue(
                route_driver.CurrentMainEvidenceVerifier._consumes_symbol(
                    guard,
                    symbol,
                    "rust",
                )
            )
        self.assertTrue(
            route_driver.CurrentMainEvidenceVerifier._consumes_symbol(
                "fn build() { let _f: fn() -> TypedBoundary = TypedBoundary::new(); }\n",
                "TypedBoundary",
                "rust",
            )
        )
        self.assertFalse(
            route_driver.CurrentMainEvidenceVerifier._consumes_symbol(
                "impl TypedBoundary { fn new() -> Self { Self {} } }\n",
                "TypedBoundary",
                "rust",
            )
        )

    def test_equivalent_worker_path_orders_compile_one_canonical_candidate(self):
        baseline = self._proposal()
        permuted = self._proposal()
        permuted["allowed_paths"] = list(reversed(permuted["allowed_paths"]))

        verifier = route_driver.CurrentMainEvidenceVerifier(self.repo, self.main)
        first = verifier.verify(
            json.dumps(baseline), self.successor, self._predecessor_receipt()
        )
        second = verifier.verify(
            json.dumps(permuted), self.successor, self._predecessor_receipt()
        )

        expected = (
            "docs/CURRENT_STATUS.md",
            "docs/FUTURE_ROUTE.md",
            "docs/MODULE_MAP.md",
            "docs/NEXT_DECISION.md",
            "scripts/agent-control/local_run_once.py",
            "scripts/agent-control/route_driver.py",
            "tests/test_route_driver.py",
        )
        self.assertEqual(first.state, "READY_FOR_EXECUTION")
        self.assertEqual(second.state, "READY_FOR_EXECUTION")
        self.assertIsNotNone(first.evidence)
        self.assertIsNotNone(second.evidence)
        self.assertEqual(first.evidence.allowed_paths, expected)
        self.assertEqual(second.evidence.allowed_paths, expected)
        self.assertIsNotNone(first.candidate)
        self.assertIsNotNone(second.candidate)
        self.assertEqual(first.candidate.spec_digest, second.candidate.spec_digest)

    def test_allowed_paths_over_the_prompt_limit_are_a_decision(self):
        proposal = self._proposal()
        proposal["allowed_paths"] = [
            "docs/MODULE_MAP.md"
        ] * (route_driver._MAX_PROMOTION_LIST_ITEMS + 1)
        result = route_driver.CurrentMainEvidenceVerifier(self.repo, self.main).verify(
            json.dumps(proposal), self.successor, self._predecessor_receipt()
        )
        self.assertEqual(result.state, "DECISION_REQUIRED")
        self.assertEqual(result.reason, "promotion_allowed_paths_invalid")

    def test_unproved_caller_is_a_decision_not_a_plausible_contract(self):
        proposal = self._proposal()
        proposal["caller_evidence"][0]["symbol"] = "ImaginaryCaller"
        result = route_driver.CurrentMainEvidenceVerifier(self.repo, self.main).verify(
            json.dumps(proposal), self.successor, self._predecessor_receipt()
        )
        self.assertEqual(result.state, "DECISION_REQUIRED")
        self.assertEqual(result.reason, "promotion_caller_not_proved")

    def test_unrelated_module_map_token_is_not_an_owner_proof(self):
        proposal = self._proposal()
        proposal["owner_evidence"][0]["path"] = "scripts/agent-control/local_run_once.py"
        result = route_driver.CurrentMainEvidenceVerifier(self.repo, self.main).verify(
            json.dumps(proposal), self.successor, self._predecessor_receipt()
        )
        self.assertEqual(result.state, "DECISION_REQUIRED")
        self.assertEqual(result.reason, "promotion_owner_not_proved")

    def test_worker_can_explicitly_escalate_without_a_generic_fallback(self):
        result = route_driver.CurrentMainEvidenceVerifier(self.repo, self.main).verify(
            json.dumps({
                "schema_version": "route_promotion_evidence.v1",
                "state": "DECISION_REQUIRED",
                "reason": "owner_ambiguous",
            }),
            self.successor,
            self._predecessor_receipt(),
        )
        self.assertEqual(result.state, "DECISION_REQUIRED")
        self.assertEqual(result.reason, "promotion_planner:owner_ambiguous")

    def test_worker_prompt_excludes_static_allowed_delta_paths(self):
        prompt = route_driver.promotion_planner_prompt(
            self.successor, self.main, EVIDENCE
        )
        self.assertIn("git show " + self.main, prompt)
        self.assertNotIn(self.successor.sketch.allowed_delta, prompt)
        self.assertIn("Do not use FUTURE_ROUTE's Allowed delta", prompt)

    def test_worker_prompt_requires_machine_valid_allowed_paths(self):
        prompt = route_driver.promotion_planner_prompt(
            self.successor, self.main, EVIDENCE
        )
        self.assertIn("Each `allowed_paths` entry must be a literal repository-relative path", prompt)
        self.assertIn(
            f"at most {route_driver._MAX_PROMOTION_LIST_ITEMS} entries", prompt
        )
        self.assertIn("Never use placeholders such as `...` or `exact/file`", prompt)
        self.assertIn("existing regular, non-symlink accepted-tree file", prompt)
        self.assertIn("with no duplicates", prompt)
        self.assertIn("glob characters", prompt)
        self.assertIn("Every mutable path (ordered slice, operation, destination, or decision) must", prompt)
        self.assertIn("Treat `read_paths` as the closed, machine-validated read-only evidence scope.", prompt)
        self.assertIn(".github/workflows/", prompt)
        self.assertIn(".github/actions/", prompt)
        self.assertIn("row whose owner/caller/symbol cannot be proven is rejected", prompt)

    def test_worker_prompt_defines_machine_verifiable_caller_evidence(self):
        prompt = route_driver.promotion_planner_prompt(
            self.successor, self.main, EVIDENCE
        )
        self.assertIn("Each `caller_evidence` row must", prompt)
        self.assertIn("owner_path` must be in both `owner_evidence` and `read_paths`", prompt)
        self.assertIn("caller_path` must be in `read_paths`", prompt)
        self.assertIn("Python `def` or `class` declaration in `owner_path`", prompt)
        self.assertIn("verifier-recognized `symbol(` reference in `caller_path`", prompt)
        self.assertIn("git grep -nE", prompt)
        self.assertIn("git grep -nF", prompt)
        self.assertIn("Return the row only when both commands prove it", prompt)

    def test_worker_prompt_symbol_proof_commands_are_executable(self):
        prompt = route_driver.promotion_planner_prompt(
            self.successor, self.main, EVIDENCE
        )
        declaration = re.search(
            r"git grep -nE '[^']+' [0-9a-f]+ -- <owner_path>", prompt
        )
        self.assertIsNotNone(declaration)
        consumption = re.search(
            r"git grep -nF '[^']+' [0-9a-f]+ -- <caller_path>", prompt
        )
        self.assertIsNotNone(consumption)
        owner = "scripts/agent-control/route_driver.py"
        caller = "scripts/agent-control/local_run_once.py"
        declaration_cmd = declaration.group(0).replace(
            "<symbol>", "RoutePromotionPlanner"
        ).replace("<owner_path>", owner).replace(
            "git grep", "git -C " + str(self.repo) + " grep"
        )
        consumption_cmd = consumption.group(0).replace(
            "<symbol>", "RoutePromotionPlanner"
        ).replace("<caller_path>", caller).replace(
            "git grep", "git -C " + str(self.repo) + " grep"
        )
        negative_cmd = declaration.group(0).replace(
            "<symbol>", "NoSuchRouteSymbol"
        ).replace("<owner_path>", owner).replace(
            "git grep", "git -C " + str(self.repo) + " grep"
        )
        for command, expected in (
            (declaration_cmd, True),
            (consumption_cmd, True),
            (negative_cmd, False),
        ):
            result = subprocess.run(
                shlex.split(command), capture_output=True, text=True
            )
            self.assertEqual(
                (result.returncode == 0 and result.stdout.strip() != ""),
                expected,
                f"command {command} produced {result.returncode}: {result.stdout[:120]}",
            )


class TestRepositoryRouteRunner(unittest.TestCase):
    class Result:
        def __init__(self, status, **details):
            self.status = status
            self.details = details
            self.attempt_id = ATTEMPT

    def test_merge_backed_current_window_uses_bootstrap_not_a_duplicate_worker(self):
        runner = mock.Mock()
        runner.bootstrap_route_once.return_value = self.Result(
            "control_stopped", reason="operator_pause"
        )
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo", repo_path=Path("/tmp"), runner=runner,
        )
        receipt = (
            f"PR #390 exact head `{'b' * 40}`; merge `{'c' * 40}`; "
            "exact-head `PASS`; canonical workflow `31467821768`"
        )
        route._current_complete_receipt = receipt
        with mock.patch.object(route, "_current_packet", return_value=(CLOSED, MAIN)):
            result = route.run()
        self.assertEqual(result["state"], "DECISION_REQUIRED")
        runner.bootstrap_route_once.assert_called_once_with(CLOSED, receipt)
        runner.run_plan_once.assert_not_called()

    def test_ordinary_worker_failure_retries_without_a_packet_selector(self):
        runner = mock.Mock()
        runner.run_plan_once.side_effect = [
            self.Result("failed", reason="worker_failed"),
            self.Result("control_stopped", reason="operator_pause"),
        ]
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo",
            repo_path=Path("/tmp"),
            runner=runner,
            attempt_factory=lambda: ATTEMPT,
        )
        with mock.patch.object(route, "_current_packet", return_value=(CLOSED, MAIN)):
            result = route.run()
        self.assertEqual(result["state"], "DECISION_REQUIRED")
        self.assertEqual(runner.run_plan_once.call_count, 2)
        self.assertTrue(all(call.args[0] == CLOSED for call in runner.run_plan_once.call_args_list))

    def test_usage_exhaustion_stops_without_minting_another_generation(self):
        runner = mock.Mock()
        runner.run_plan_once.return_value = self.Result(
            "failed",
            reason="codex_failed",
            worker_failure_reason="usage_or_credit_exhaustion",
        )
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo",
            repo_path=Path("/tmp"),
            runner=runner,
            attempt_factory=lambda: ATTEMPT,
        )
        with mock.patch.object(route, "_current_packet", return_value=(CLOSED, MAIN)):
            result = route.run()
        self.assertEqual(result["state"], "UNRECOVERABLE_INFRASTRUCTURE_FAILURE")
        self.assertEqual(result["reason"], "route_worker_usage_or_credit_exhaustion")
        runner.run_plan_once.assert_called_once_with(CLOSED, ATTEMPT)

    def test_production_route_polls_recoverable_controller_state(self):
        runner = mock.Mock()
        runner.run_plan_once.side_effect = [
            self.Result("failed", reason="worker_failed"),
            self.Result("control_stopped", reason="operator_pause"),
        ]
        waits = []
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo",
            repo_path=Path("/tmp"),
            sleeper=waits.append,
            poll_interval_seconds=3,
        )
        route._runner = runner
        with mock.patch.object(route, "_current_packet", return_value=(CLOSED, MAIN)):
            result = route.run()
        self.assertEqual(result["state"], "DECISION_REQUIRED")
        self.assertEqual(waits, [3.0])

    def test_production_route_does_not_exhaust_its_budget_while_ci_state_is_unchanged(self):
        runner = mock.Mock()
        runner.run_plan_once.side_effect = [
            self.Result("promotion_ci_pending", reason="canonical_ci_wait"),
            self.Result("promotion_ci_pending", reason="canonical_ci_wait"),
            self.Result("control_stopped", reason="operator_pause"),
        ]
        waits = []
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo",
            repo_path=Path("/tmp"),
            max_transitions=2,
            sleeper=waits.append,
            poll_interval_seconds=3,
        )
        route._runner = runner
        with mock.patch.object(route, "_current_packet", return_value=(CLOSED, MAIN)):
            result = route.run()
        self.assertEqual(result["state"], "DECISION_REQUIRED")
        self.assertEqual(result["transitions"], 1)
        self.assertEqual(waits, [3.0, 3.0])
        self.assertEqual(runner.run_plan_once.call_count, 3)

    def test_outcome_unknown_is_terminal_and_never_retried(self):
        runner = mock.Mock()
        runner.run_plan_once.return_value = self.Result(
            "outcome_unknown", reason="effect_status_unknown"
        )
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo",
            repo_path=Path("/tmp"),
            runner=runner,
            attempt_factory=lambda: ATTEMPT,
        )
        with mock.patch.object(route, "_current_packet", return_value=(CLOSED, MAIN)):
            result = route.run()
        self.assertEqual(result["state"], "OUTCOME_UNKNOWN")
        runner.run_plan_once.assert_called_once()

    def test_failed_unknown_output_is_outcome_unknown_not_a_decision_pause(self):
        runner = mock.Mock()
        runner.run_plan_once.return_value = self.Result(
            "failed_unknown_output", reason="worker_output_unproved"
        )
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo",
            repo_path=Path("/tmp"),
            runner=runner,
            attempt_factory=lambda: ATTEMPT,
        )
        with mock.patch.object(route, "_current_packet", return_value=(CLOSED, MAIN)):
            result = route.run()
        self.assertEqual(result["state"], "OUTCOME_UNKNOWN")
        self.assertEqual(result["reason"], "worker_output_unproved")
        runner.run_plan_once.assert_called_once()

    def test_unavailable_controller_state_retries_without_a_decision_pause(self):
        runner = mock.Mock()
        runner.run_plan_once.side_effect = [
            self.Result("unavailable", reason="temporary_github_failure"),
            self.Result("control_stopped", reason="operator_pause"),
        ]
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo",
            repo_path=Path("/tmp"),
            runner=runner,
            attempt_factory=lambda: ATTEMPT,
        )
        with mock.patch.object(route, "_current_packet", return_value=(CLOSED, MAIN)):
            result = route.run()
        self.assertEqual(result["state"], "DECISION_REQUIRED")
        self.assertEqual(result["reason"], "operator_pause")
        self.assertEqual(runner.run_plan_once.call_count, 2)

    def test_persistent_controller_unavailability_becomes_typed_infrastructure_failure(self):
        runner = mock.Mock()
        runner.run_plan_once.side_effect = [
            self.Result("unavailable", reason="github_unavailable"),
            self.Result("unavailable", reason="github_unavailable"),
        ]
        waits = []
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo",
            repo_path=Path("/tmp"),
            max_transitions=2,
            poll_interval_seconds=3,
            recovery_timeout_seconds=60,
            sleeper=waits.append,
            clock=mock.Mock(side_effect=[0.0, 61.0]),
        )
        route._runner = runner
        with mock.patch.object(route, "_current_packet", return_value=(CLOSED, MAIN)):
            result = route.run()
        self.assertEqual(result["state"], "UNRECOVERABLE_INFRASTRUCTURE_FAILURE")
        self.assertEqual(result["reason"], "route_controller_unavailable_timeout")
        self.assertEqual(result["transitions"], 1)
        self.assertEqual(waits, [3.0])
        self.assertEqual(runner.run_plan_once.call_count, 2)

    def test_terminal_closed_out_claim_resumes_its_existing_promotion(self):
        runner = mock.Mock()
        runner.run_plan_once.return_value = self.Result(
            "terminal", claim_status="closed_out"
        )
        runner.run_route_once.return_value = self.Result("promoted")
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo",
            repo_path=Path("/tmp"),
            runner=runner,
            max_transitions=2,
            attempt_factory=lambda: ATTEMPT,
        )
        with mock.patch.object(
            route, "_current_packet", side_effect=[(CLOSED, MAIN), (None, MAIN)]
        ):
            result = route.run()
        self.assertEqual(result["state"], "ROUTE_EXHAUSTED")
        runner.run_route_once.assert_called_once_with(CLOSED, ATTEMPT)

    def test_unproved_promotion_is_decision_required_not_a_retry_loop(self):
        runner = mock.Mock()
        runner.run_plan_once.return_value = self.Result("closed_out")
        runner.run_route_once.return_value = self.Result(
            "bounded_pause", reason="promotion_caller_not_proved"
        )
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo", repo_path=Path("/tmp"), runner=runner,
            attempt_factory=lambda: ATTEMPT,
        )
        with mock.patch.object(route, "_current_packet", return_value=(CLOSED, MAIN)):
            result = route.run()
        self.assertEqual(result["state"], "DECISION_REQUIRED")
        self.assertEqual(result["reason"], "promotion_caller_not_proved")
        runner.run_route_once.assert_called_once()

    def test_empty_accepted_inventory_is_the_only_route_exhaustion(self):
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo", repo_path=Path("/tmp"), runner=mock.Mock(),
            t3_receipt_reader=lambda _request: None,
        )
        with mock.patch.object(route, "_current_packet", return_value=(None, MAIN)):
            result = route.run()
        self.assertEqual(result["state"], "ROUTE_EXHAUSTED")

    def test_current_packet_refreshes_exact_default_branch_before_reading_it(self):
        github = mock.Mock()
        github.repository_metadata.return_value = {"default_branch": "main"}
        github.accepted_main_sha.return_value = MAIN
        github.accepted_plan_document.return_value = next_document()
        github.accepted_status_document.return_value = status_document()
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo", repo_path=Path("/tmp"), runner=mock.Mock(), github=github,
        )
        import local_loop
        with mock.patch.object(local_loop, "GitAdapter") as adapter:
            packet_id, accepted_main = route._current_packet()
        self.assertEqual((packet_id, accepted_main), (CLOSED, MAIN))
        adapter.return_value.refresh_origin_main.assert_called_once_with(Path("/tmp"), "main")
        github.accepted_plan_document.assert_called_once_with(MAIN)

    def test_current_packet_accepts_only_one_merge_backed_completed_receipt(self):
        evidence = (
            f"PR #390 exact head `{'b' * 40}`; merge `{'c' * 40}`; "
            "exact-head `PASS`; canonical workflow `31467821768`"
        )
        receipt = f"| `{CLOSED}` | `COMPLETE` | {evidence} |"
        status = status_document().replace(
            "|---|---|---|\n\n",
            "|---|---|---|\n" + receipt + "\n\n",
            1,
        )
        github = mock.Mock()
        github.repository_metadata.return_value = {"default_branch": "main"}
        github.accepted_main_sha.return_value = MAIN
        payload = _packet_payload(CLOSED)
        payload["allowed_paths"] = [
            ".github/workflows/agent-controller.yml",
            "scripts/agent-control/",
            "tests/",
        ]
        github.accepted_plan_document.return_value = (
            next_document(marker_payload=payload)
            + f"\n<!-- route-bootstrap-reconcile:v1 packet_id={CLOSED} -->\n"
        )
        github.accepted_status_document.return_value = status
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo", repo_path=Path("/tmp"), runner=mock.Mock(), github=github,
        )
        import local_loop
        with mock.patch.object(local_loop, "GitAdapter"):
            self.assertEqual(route._current_packet(), (CLOSED, MAIN))
        self.assertEqual(route._current_complete_receipt, evidence)

    def test_current_packet_cannot_bootstrap_an_incomplete_workflow_scope(self):
        payload = _packet_payload(CLOSED)
        payload["allowed_paths"] = [".github/workflows/agent-controller.yml", "tests/"]
        github = mock.Mock()
        github.repository_metadata.return_value = {"default_branch": "main"}
        github.accepted_main_sha.return_value = MAIN
        github.accepted_plan_document.return_value = next_document(marker_payload=payload)
        github.accepted_status_document.return_value = status_document()
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo", repo_path=Path("/tmp"), runner=mock.Mock(), github=github,
        )
        import local_loop
        with mock.patch.object(local_loop, "GitAdapter"):
            with self.assertRaises(plan_lane.PlanLaneError) as ctx:
                route._current_packet()
        self.assertEqual(ctx.exception.reason, "plan_allowed_paths_invalid")

    def test_current_packet_rejects_a_completed_receipt_without_merge_evidence(self):
        receipt = f"| `{CLOSED}` | `COMPLETE` | PR #390 accepted |"
        status = status_document().replace(
            "|---|---|---|\n\n",
            "|---|---|---|\n" + receipt + "\n\n",
            1,
        )
        github = mock.Mock()
        github.repository_metadata.return_value = {"default_branch": "main"}
        github.accepted_main_sha.return_value = MAIN
        github.accepted_plan_document.return_value = (
            next_document() + f"\n<!-- route-bootstrap-reconcile:v1 packet_id={CLOSED} -->\n"
        )
        github.accepted_status_document.return_value = status
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo", repo_path=Path("/tmp"), runner=mock.Mock(), github=github,
        )
        import local_loop
        with mock.patch.object(local_loop, "GitAdapter"):
            with self.assertRaises(route_driver.RouteDriverError) as ctx:
                route._current_packet()
        self.assertEqual(ctx.exception.reason, "route_bootstrap_receipt_not_merge_backed")

    def test_current_packet_recognizes_a_compiled_t3_window_before_plan_dispatch(self):
        request = {
            "schema_version": "route_t3_request.v1", "packet_id": CLOSED,
            "accepted_main_sha": MAIN, "candidate_digest": "b" * 64,
            "action_digest": "c" * 64, "scope_digest": "d" * 64,
            "authority_owner_digest": "a" * 64,
            "requested_action": "one finite action",
        }
        document = (
            f"## Active Routing\n\n1. `{CLOSED}` — `T3_REQUIRED`\n\n"
            f"## Packet {CLOSED}\n\n**State:** `T3_REQUIRED`\n\n"
            "<!-- weak-agent-dispatch:v1 " + json.dumps({
                **_packet_payload(CLOSED), "packet_state": "T3_REQUIRED",
            }, sort_keys=True) + " -->\n"
            "<!-- route-t3-request:v1\n" + json.dumps(request) + "\n-->\n"
        )
        github = mock.Mock()
        github.repository_metadata.return_value = {"default_branch": "main"}
        github.accepted_main_sha.return_value = MAIN
        github.accepted_plan_document.return_value = document
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo", repo_path=Path("/tmp"), runner=mock.Mock(), github=github,
        )
        import local_loop
        with mock.patch.object(local_loop, "GitAdapter"):
            packet_id, accepted_main = route._current_packet()
        self.assertEqual((packet_id, accepted_main), (CLOSED, MAIN))
        self.assertEqual(
            route._current_t3_request,
            route_driver.T3Request(
                packet_id=CLOSED, accepted_main_sha=MAIN,
                candidate_digest="b" * 64, action_digest="c" * 64,
                scope_digest="d" * 64, authority_owner_digest="a" * 64,
                requested_action="one finite action",
            ),
        )

    def test_effect_pause_is_terminal_before_any_worker_dispatch(self):
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo", repo_path=Path("/tmp"), runner=mock.Mock(),
        )
        route._current_t3_request = route_driver.T3Request(
            packet_id=CLOSED, accepted_main_sha=MAIN,
            candidate_digest="b" * 64, action_digest="c" * 64,
            scope_digest="d" * 64, authority_owner_digest="a" * 64,
            requested_action="one finite action",
        )
        with mock.patch.object(route, "_current_packet", return_value=(CLOSED, MAIN)):
            result = route.run()
        self.assertEqual(result["state"], "T3_REQUIRED")
        route._runner.run_plan_once.assert_not_called()

    def test_valid_t3_receipt_resumes_through_the_production_closeout_owner(self):
        now = datetime.now(timezone.utc)
        request = route_driver.T3Request(
            packet_id=CLOSED, accepted_main_sha=MAIN,
            candidate_digest="b" * 64, action_digest="c" * 64,
            scope_digest="d" * 64, authority_owner_digest="a" * 64,
            requested_action="one finite action",
        )
        receipt = {
            "schema_version": "route_t3_receipt.v1", "packet_id": CLOSED,
            "accepted_main_sha": MAIN, "candidate_digest": "b" * 64,
            "action_digest": "c" * 64, "scope_digest": "d" * 64,
            "authority_receipt_digest": "e" * 64, "authority_owner_digest": "a" * 64,
            "outcome_receipt_digest": "f" * 64,
            "operator": "authorized-operator",
            "decision_source": "local_sol_5_6_max",
            "decision_evidence_digest": "1" * 64,
            "decision_digest": "",
            "issued_at": now.isoformat(), "expires_at": (now + timedelta(minutes=5)).isoformat(),
            "disposition": "GO",
        }
        receipt["decision_digest"] = route_driver.t3_decision_digest(
            request,
            receipt["decision_source"],
            receipt["decision_evidence_digest"],
            receipt["disposition"],
        )
        runner = mock.Mock()
        runner.run_effect_route_once.return_value = self.Result("promoted")
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo", repo_path=Path("/tmp"), runner=runner,
            max_transitions=2, t3_receipt_reader=lambda _request: receipt,
        )
        route._current_t3_request = request
        with mock.patch.object(route, "_current_packet", side_effect=[(CLOSED, MAIN), (None, MAIN)]):
            result = route.run()
        self.assertEqual(result["state"], "ROUTE_EXHAUSTED")
        runner.run_effect_route_once.assert_called_once()

    def test_clean_multi_packet_crossing_uses_each_closed_attempt_for_promotion(self):
        runner = mock.Mock()
        runner.run_plan_once.side_effect = [
            self.Result("closed_out"), self.Result("closed_out"),
        ]
        runner.run_route_once.side_effect = [
            self.Result("promoted"), self.Result("promoted"),
        ]
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo", repo_path=Path("/tmp"), runner=runner,
            max_transitions=3, attempt_factory=lambda: ATTEMPT,
        )
        with mock.patch.object(
            route, "_current_packet", side_effect=[(CLOSED, MAIN), ("PE7-CLEAN-2", MAIN), (None, MAIN)]
        ):
            result = route.run()
        self.assertEqual(result["state"], "ROUTE_EXHAUSTED")
        self.assertEqual(runner.run_route_once.call_args_list[0].args[1], ATTEMPT)
        self.assertEqual(runner.run_route_once.call_args_list[1].args[1], ATTEMPT)

    def test_restart_reconciles_the_exact_existing_attempt_before_a_new_claim(self):
        class RecoveringRunner:
            def __init__(self):
                self.reconciled = 0
                self.started = 0
                self.promoted = []

            def reconcile_plan(self, _packet):
                self.reconciled += 1
                return TestRepositoryRouteRunner.Result("closed_out")

            def run_plan_once(self, _packet, _attempt):
                self.started += 1
                raise AssertionError("fresh attempt would duplicate the recovered claim")

            def run_route_once(self, packet, attempt):
                self.promoted.append((packet, attempt))
                return TestRepositoryRouteRunner.Result("promoted")

        runner = RecoveringRunner()
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo", repo_path=Path("/tmp"), runner=runner,
            max_transitions=2,
        )
        with mock.patch.object(route, "_current_packet", side_effect=[(CLOSED, MAIN), (None, MAIN)]):
            result = route.run()
        self.assertEqual(result["state"], "ROUTE_EXHAUSTED")
        self.assertEqual(runner.started, 0)
        self.assertEqual(runner.promoted, [(CLOSED, ATTEMPT)])

    def test_adversarial_provider_free_soak_retries_routine_failures_before_a_typed_stop(self):
        """Exercise the canonical soak's routine recovery taxonomy in one run."""

        recoverable_cases = {
            "worker_failure": "worker_failed",
            "ci_repair": "canonical_ci_repair_pending",
            "review_repair": "review_repair_pending",
            "crash_restart": "in_flight",
            "main_drift": "stale_checkout",
            "stale_checkpoint": "claim_rejected",
        }
        for case, status in recoverable_cases.items():
            with self.subTest(case=case):
                runner = mock.Mock()
                runner.run_plan_once.side_effect = [
                    self.Result(status, reason=case),
                    self.Result("control_stopped", reason="operator_pause"),
                ]
                route = route_driver.RepositoryRouteRunner(
                    repository="acme/repo", repo_path=Path("/tmp"), runner=runner,
                    attempt_factory=lambda: ATTEMPT,
                )
                with mock.patch.object(route, "_current_packet", return_value=(CLOSED, MAIN)):
                    result = route.run()
                self.assertEqual(result["state"], "DECISION_REQUIRED")
                self.assertEqual(runner.run_plan_once.call_count, 2)

        for case, promotion_status in {
            "duplicate_dispatch_pr_prevention": "promotion_pr",
            "merge_before_closeout_crash": "promotion_pending",
            "promotion_crash": "failed",
            "review_wait": "promotion_review_pending",
            "ready_retry": "promotion_ready_pending",
            "canonical_ci_wait": "promotion_ci_pending",
        }.items():
            with self.subTest(case=case):
                runner = mock.Mock()
                runner.run_plan_once.side_effect = [
                    self.Result("closed_out"),
                    self.Result("control_stopped", reason="operator_pause"),
                ]
                runner.run_route_once.return_value = self.Result(promotion_status, reason=case)
                route = route_driver.RepositoryRouteRunner(
                    repository="acme/repo", repo_path=Path("/tmp"), runner=runner,
                    attempt_factory=lambda: ATTEMPT,
                )
                with mock.patch.object(route, "_current_packet", return_value=(CLOSED, MAIN)):
                    result = route.run()
                self.assertEqual(result["state"], "DECISION_REQUIRED")
                self.assertEqual(runner.run_route_once.call_count, 1)

        runner = mock.Mock()
        runner.run_plan_once.return_value = self.Result("closed_out", terminal_packet_state="NO_GO")
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo", repo_path=Path("/tmp"), runner=runner,
        )
        with mock.patch.object(route, "_current_packet", return_value=(CLOSED, MAIN)):
            result = route.run()
        self.assertEqual(result["reason"], "route_no_go_requires_canonical_rewrite")

    def test_stable_post_merge_in_flight_is_a_typed_unknown_not_an_unbounded_recover(self):
        """A repeated unrepairable in_flight after merge must stop with a typed result.

        Production poll mode does not increment the transition budget on an
        unchanged recover marker.  Crash-restart still recovers the first
        in_flight; a second identical post-merge in_flight is outcome-unknown.
        """

        class RecoveringRunner:
            def __init__(self):
                self.calls = 0

            def run_plan_once(self, _packet, _attempt):
                self.calls += 1
                return TestRepositoryRouteRunner.Result(
                    "in_flight", reason="dispatched_generation_unrepairable"
                )

        def sleeper(_seconds):
            if runner.calls > 8:
                raise AssertionError("stable in_flight recovered unbounded")

        runner = RecoveringRunner()
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo",
            repo_path=Path("/tmp"),
            max_transitions=8,
            sleeper=sleeper,
        )
        route._runner = runner
        self.assertTrue(route._poll_recoverable)
        with mock.patch.object(route, "_current_packet", return_value=(CLOSED, MAIN)):
            result = route.run()
        self.assertEqual(result["state"], "OUTCOME_UNKNOWN")
        self.assertEqual(result["reason"], "dispatched_generation_unrepairable")
        self.assertEqual(runner.calls, 2)
        self.assertLess(runner.calls, 8)

    def test_stable_post_merge_claim_unavailable_is_a_typed_unknown_not_an_unbounded_recover(self):
        """After merge, reconcile miss + claim_unavailable must not poll forever.

        The live dispatched generation still occupies capacity, so a fresh
        attempt returns claim_unavailable. Production poll mode would otherwise
        loop on that stable recoverable marker.
        """

        class MissRunner:
            def __init__(self):
                self.calls = 0

            def reconcile_plan(self, _packet):
                return None

            def run_plan_once(self, _packet, _attempt):
                self.calls += 1
                return TestRepositoryRouteRunner.Result(
                    "claim_unavailable", reason="capacity_occupied"
                )

        def sleeper(_seconds):
            if runner.calls > 8:
                raise AssertionError("stable claim_unavailable recovered unbounded")

        runner = MissRunner()
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo",
            repo_path=Path("/tmp"),
            max_transitions=8,
            sleeper=sleeper,
        )
        route._runner = runner
        self.assertTrue(route._poll_recoverable)
        with mock.patch.object(route, "_current_packet", return_value=(CLOSED, MAIN)):
            result = route.run()
        self.assertEqual(result["state"], "OUTCOME_UNKNOWN")
        self.assertEqual(result["reason"], "capacity_occupied")
        self.assertEqual(runner.calls, 2)
        self.assertLess(runner.calls, 8)


if __name__ == "__main__":
    unittest.main()
