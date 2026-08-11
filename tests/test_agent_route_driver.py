"""Provider-free tests for the promotion compiler and route driver core."""

from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys
import tempfile
from datetime import datetime, timedelta, timezone
import unittest
from unittest import mock

CONTROL = Path(__file__).resolve().parents[1] / "scripts" / "agent-control"
sys.path.insert(0, str(CONTROL))

import plan_lane  # noqa: E402
import route_driver  # noqa: E402

MAIN = "a" * 40
CLOSED = "PE7-LIFECYCLE-CONTROLLER-1"
SUCCESSOR = "PE7-SUCCESSOR-PROMOTION-ESCALATION-1"
EVIDENCE = "PR #389 exact head " + "b" * 40 + "; merge " + "c" * 40
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


def next_document(current=CLOSED, completed=("PE7-PLAN-LANE-ACTIVATION-1",)):
    blocks = []
    for packet_id in completed:
        blocks.append(
            f"## Completed ({packet_id})\n\n**Historical state:** `COMPLETE`\n"
        )
    blocks.append(
        f"## Packet {current}\n\n**State:** `READY_FOR_EXECUTION`\n\n"
        f"<!-- weak-agent-dispatch:v1 {json.dumps(_packet_payload(current), sort_keys=True)} -->\n"
    )
    return "\n\n".join(["## Active Routing", f"1. `{current}`", *blocks])


def status_document():
    return (
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

    def _successor(self, packet_class="IMPLEMENT"):
        sketch = route_driver.PacketSketch(
            packet_id="PE7-EXACT-PROMOTION-1",
            prerequisites=(CLOSED,),
            packet_class=packet_class,
            outcome="Use the existing repository-maintenance control-plane owner.",
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

    def _evidence(self, *, packet_id="PE7-EXACT-PROMOTION-1"):
        return route_driver.CurrentMainEvidence(
            packet_id=packet_id,
            accepted_main_sha=MAIN,
            owner_paths=("scripts/agent-control/plan_lifecycle.py",),
            caller_paths=("scripts/agent-control/local_run_once.py",),
            test_paths=("tests/test_agent_plan_lifecycle.py",),
            allowed_paths=(
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

    def test_static_future_paths_are_hints_not_promotion_evidence(self):
        result = route_driver.RoutePromotionPlanner().plan(
            self._successor(), MAIN, EVIDENCE, None
        )
        self.assertEqual(result.state, "DECISION_REQUIRED")
        self.assertEqual(result.reason, "promotion_current_main_evidence_missing")

    def test_current_main_evidence_owns_every_refreshed_contract_field(self):
        result = route_driver.RoutePromotionPlanner().plan(
            self._successor(), MAIN, EVIDENCE, self._evidence()
        )
        self.assertEqual(result.state, "READY_FOR_EXECUTION")
        self.assertIsNotNone(result.candidate)
        self.assertEqual(
            result.candidate.capsule["allowed_paths"],
            list(self._evidence().allowed_paths),
        )
        self.assertNotIn("docs/", result.candidate.capsule["allowed_paths"])
        self.assertEqual(
            result.candidate.contract["cleanup"],
            self._evidence().cleanup,
        )

    def test_effect_is_prepared_then_paused_for_t3_instead_of_skipped(self):
        successor = self._successor("EFFECT")
        evidence = self._evidence(packet_id=successor.packet_id)
        result = route_driver.RoutePromotionPlanner().plan(successor, MAIN, EVIDENCE, evidence)
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
            "issued_at": now.isoformat(),
            "expires_at": (now + timedelta(minutes=5)).isoformat(),
            "disposition": "GO",
        }
        validated, reason = route_driver.validate_t3_receipt(receipt, request, now=now)
        self.assertEqual(reason, "t3_receipt_valid")
        self.assertEqual(validated.disposition, "GO")
        receipt["candidate_digest"] = "f" * 64
        validated, reason = route_driver.validate_t3_receipt(receipt, request, now=now)
        self.assertIsNone(validated)
        self.assertEqual(reason, "t3_receipt_binding_mismatch")

    def test_compaction_keeps_one_current_window_for_116_crossings(self):
        document = "# Next\n\n## Common Execution Protocol\n\n- retained\n"
        for number in range(116):
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

    def test_compaction_traverses_the_current_canonical_portfolio_without_growth(self):
        """Use the accepted inventory, not a synthetic count, for the soak proof."""

        future = (Path(__file__).resolve().parents[1] / "docs" / "FUTURE_ROUTE.md").read_text(
            encoding="utf-8"
        )
        packet_ids = route_driver.inventory_manifest(future)["ordered_packet_ids"]
        self.assertGreaterEqual(len(packet_ids), 116)
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
            "docs/MODULE_MAP.md": "| Route | `scripts/agent-control/route_driver.py` | `route_driver.py` is the route promotion owner |\n",
            "docs/NEXT_DECISION.md": (
                "rollback-marker cleanup-marker retention-marker evidence-marker "
                "schema-marker evaluator-marker authority-marker recovery-marker\n"
            ),
            "scripts/agent-control/route_driver.py": "class RoutePromotionPlanner: pass\n",
            "scripts/agent-control/local_run_once.py": "from route_driver import RoutePromotionPlanner\nRoutePromotionPlanner()\n",
            "tests/test_route_driver.py": "from route_driver import RoutePromotionPlanner\nRoutePromotionPlanner()\n",
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

    def tearDown(self):
        self.temporary.cleanup()

    def _proposal(self):
        allowed = [
            "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md",
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

    def test_exact_tree_proves_all_refreshed_fields(self):
        result = route_driver.CurrentMainEvidenceVerifier(self.repo, self.main).verify(
            json.dumps(self._proposal()), self.successor, EVIDENCE
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

    def test_unproved_caller_is_a_decision_not_a_plausible_contract(self):
        proposal = self._proposal()
        proposal["caller_evidence"][0]["symbol"] = "ImaginaryCaller"
        result = route_driver.CurrentMainEvidenceVerifier(self.repo, self.main).verify(
            json.dumps(proposal), self.successor, EVIDENCE
        )
        self.assertEqual(result.state, "DECISION_REQUIRED")
        self.assertEqual(result.reason, "promotion_caller_not_proved")

    def test_unrelated_module_map_token_is_not_an_owner_proof(self):
        proposal = self._proposal()
        proposal["owner_evidence"][0]["path"] = "scripts/agent-control/local_run_once.py"
        result = route_driver.CurrentMainEvidenceVerifier(self.repo, self.main).verify(
            json.dumps(proposal), self.successor, EVIDENCE
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
            EVIDENCE,
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


class TestRepositoryRouteRunner(unittest.TestCase):
    class Result:
        def __init__(self, status, **details):
            self.status = status
            self.details = details
            self.attempt_id = ATTEMPT

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
        route = route_driver.RepositoryRouteRunner(
            repository="acme/repo", repo_path=Path("/tmp"), runner=mock.Mock(), github=github,
        )
        import local_loop
        with mock.patch.object(local_loop, "GitAdapter") as adapter:
            packet_id, accepted_main = route._current_packet()
        self.assertEqual((packet_id, accepted_main), (CLOSED, MAIN))
        adapter.return_value.refresh_origin_main.assert_called_once_with(Path("/tmp"), "main")
        github.accepted_plan_document.assert_called_once_with(MAIN)

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
            "issued_at": now.isoformat(), "expires_at": (now + timedelta(minutes=5)).isoformat(),
            "disposition": "GO",
        }
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


if __name__ == "__main__":
    unittest.main()
