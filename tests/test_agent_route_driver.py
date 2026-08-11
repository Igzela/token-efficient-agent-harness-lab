"""Provider-free tests for the promotion compiler and route driver core."""

from __future__ import annotations

import json
from pathlib import Path
import sys
import unittest
from unittest import mock

CONTROL = Path(__file__).resolve().parents[1] / "scripts" / "agent-control"
sys.path.insert(0, str(CONTROL))

import plan_lane  # noqa: E402
import route_driver  # noqa: E402

MAIN = "a" * 40
CLOSED = "PE7-LIFECYCLE-CONTROLLER-1"
SUCCESSOR = "PE7-SUCCESSOR-PROMOTION-ESCALATION-1"
EVIDENCE = "PR #388 exact head " + "b" * 40 + "; merge " + "c" * 40

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


class TestCompileSuccessor(unittest.TestCase):
    def test_compiles_packet_block_capsule_and_documents(self):
        compiled = route_driver.compile_successor(
            future_document([SKETCH]), next_document(), status_document(), CLOSED, EVIDENCE, MAIN
        )
        self.assertEqual(compiled.packet_id, SUCCESSOR)
        self.assertEqual(compiled.branch, f"agent/packet-{SUCCESSOR.lower()}")
        self.assertEqual(compiled.capsule["external_effect_limit"], 0)
        self.assertEqual(compiled.capsule["packet_state"], "READY_FOR_EXECUTION")
        self.assertEqual(compiled.capsule["plan_lane_state"], "plan_lane_active")
        self.assertTrue(compiled.capsule["allowed_paths"])
        self.assertIn("## Packet " + SUCCESSOR, compiled.next_document)
        self.assertNotIn("## Packet " + CLOSED, compiled.next_document)
        self.assertIn(f"## Completed {CLOSED} ({CLOSED})", compiled.next_document)
        self.assertIn("1. `" + SUCCESSOR + "`", compiled.next_document)
        self.assertNotIn(SUCCESSOR, compiled.future_document)
        self.assertNotIn(CLOSED, compiled.future_document)
        self.assertIn(SUCCESSOR + " | `READY_FOR_EXECUTION`", compiled.status_document)
        self.assertIn(CLOSED + " | `COMPLETE`", compiled.status_document)

    def test_compile_is_deterministic(self):
        first = route_driver.compile_successor(
            future_document([SKETCH]), next_document(), status_document(), CLOSED, EVIDENCE, MAIN
        )
        second = route_driver.compile_successor(
            future_document([SKETCH]), next_document(), status_document(), CLOSED, EVIDENCE, MAIN
        )
        self.assertEqual(first.spec_digest, second.spec_digest)
        self.assertEqual(first.next_document, second.next_document)
        self.assertEqual(first.future_document, second.future_document)

    def test_compiled_next_document_parses_as_the_single_current_packet(self):
        compiled = route_driver.compile_successor(
            future_document([SKETCH]), next_document(), status_document(), CLOSED, EVIDENCE, MAIN
        )
        candidate = plan_lane.parse(compiled.next_document, MAIN)
        self.assertEqual(candidate.packet_id, SUCCESSOR)
        self.assertEqual(candidate.task_spec_sha256, compiled.spec_digest)
        self.assertEqual(candidate.source_main_sha, MAIN)

    def test_compiled_future_document_manifest_is_refreshed(self):
        compiled = route_driver.compile_successor(
            future_document([SKETCH, BLOCKED_SKETCH]),
            next_document(), status_document(), CLOSED, EVIDENCE, MAIN,
        )
        manifest = route_driver.inventory_manifest(compiled.future_document)
        self.assertEqual(manifest["packet_count"], 1)
        self.assertEqual(manifest["ordered_packet_ids"], ["TOOL-OTHER-1"])

    def test_budget_exceeded_fails_closed(self):
        with mock.patch.object(route_driver, "NEXT_DECISION_MAX_BYTES", 200):
            with self.assertRaises(route_driver.RouteDriverError) as ctx:
                route_driver.compile_successor(
                    future_document([SKETCH]), next_document(), status_document(),
                    CLOSED, EVIDENCE, MAIN,
                )
        self.assertEqual(ctx.exception.reason, "route_compiled_next_document_too_large")

    def test_invalid_accepted_main_fails_closed(self):
        with self.assertRaises(route_driver.RouteDriverError) as ctx:
            route_driver.compile_successor(
                future_document([SKETCH]), next_document(), status_document(),
                CLOSED, EVIDENCE, "nope",
            )
        self.assertEqual(ctx.exception.reason, "route_accepted_main_invalid")

    def test_missing_evidence_fails_closed(self):
        with self.assertRaises(route_driver.RouteDriverError) as ctx:
            route_driver.compile_successor(
                future_document([SKETCH]), next_document(), status_document(),
                CLOSED, "   ", MAIN,
            )
        self.assertEqual(ctx.exception.reason, "route_predecessor_evidence_missing")

    def test_status_table_missing_fails_closed(self):
        with self.assertRaises(route_driver.RouteDriverError) as ctx:
            route_driver.compile_successor(
                future_document([SKETCH]), next_document(),
                "## Other Section\n\nnothing here", CLOSED, EVIDENCE, MAIN,
            )
        self.assertEqual(ctx.exception.reason, "route_status_readiness_table_missing")


class TestCapsuleCompilation(unittest.TestCase):
    def test_allowed_paths_underived_fails_closed(self):
        no_paths = SKETCH.replace(
            "scripts/agent-control/dispatcher.py, scripts/agent-control/local_run_once.py, "
            "scripts/agent-control/route_driver.py, scripts/session_context.py, "
            "tests/test_agent_plan_promotion.py.",
            "Canonical documentation only.",
        )
        with self.assertRaises(route_driver.RouteDriverError) as ctx:
            route_driver.compile_successor(
                future_document([no_paths]), next_document(), status_document(),
                CLOSED, EVIDENCE, MAIN,
            )
        self.assertEqual(ctx.exception.reason, "successor_allowed_paths_underived")

    def test_capsule_goal_is_the_sketch_outcome(self):
        compiled = route_driver.compile_successor(
            future_document([SKETCH]), next_document(), status_document(), CLOSED, EVIDENCE, MAIN
        )
        self.assertIn("successor promotion and escalation", compiled.capsule["goal"])


ATTEMPT = "123e4567-e89b-12d3-a456-426614174000"
LEDGER = 900


def _closed_claim():
    return {
        "kind": "agent-orchestrator-dispatch-state",
        "version": 1,
        "issue_number": LEDGER,
        "dispatch_id": f"plan-run:{CLOSED}:{MAIN}:{ATTEMPT}",
        "action": "plan-run",
        "status": "closed_out",
        "details": {
            "subject_kind": "plan-packet",
            "subject_id": CLOSED,
            "source_main_sha": MAIN,
            "attempt_id": ATTEMPT,
            "closeout_reference": EVIDENCE,
        },
    }


class TestRunRouteOnce(unittest.TestCase):
    def _runner(self, github, git=None):
        import local_run_once

        git = git or mock.Mock()
        git.origin_main_sha.return_value = MAIN
        return local_run_once.LocalRunOnce(
            github, git, repository="acme/repo",
            repo_path=Path("/tmp/route-driver-test"),
            lifecycle_timeout_seconds=10, sleeper=lambda _: None,
        )

    def _github(self, **overrides):
        github = mock.Mock()
        github.read_control_state.return_value = {"orchestrator_enabled": True}
        github.repository_metadata.return_value = {
            "name_with_owner": "acme/repo", "default_branch": "main",
        }
        github.accepted_main_sha.return_value = MAIN
        github.plan_ledger_issue.return_value = LEDGER
        github.accepted_plan_document.return_value = next_document()
        github.accepted_route_document.return_value = future_document([SKETCH])
        github.accepted_status_document.return_value = status_document()
        for key, value in overrides.items():
            setattr(github, key, mock.Mock(return_value=value) if callable(value) else value)
        return github

    def test_invalid_inputs_fail_closed(self):
        runner = self._runner(self._github())
        result = runner.run_route_once("bad id", ATTEMPT)
        self.assertEqual(result.status, "rejected")
        self.assertEqual(runner.run_route_once(CLOSED, "bad").details["reason"], "invalid_attempt_id")

    def test_claim_must_be_closed_out(self):
        import plan_lifecycle

        with mock.patch.object(plan_lifecycle, "_exact_plan_claim", return_value=_closed_claim() | {"status": "dispatched"}):
            result = self._runner(self._github()).run_route_once(CLOSED, ATTEMPT)
        self.assertEqual(result.status, "rejected")
        self.assertEqual(result.details["reason"], "plan_claim_not_closed_out")

    def test_successor_already_current_settles_promotion_receipt(self):
        import plan_lifecycle
        import state_manager

        github = self._github()
        github.accepted_plan_document.return_value = next_document(current=SUCCESSOR)
        with mock.patch.object(plan_lifecycle, "_exact_plan_claim", return_value=_closed_claim()), \
             mock.patch.object(state_manager, "read_dispatch_state", return_value={
                 "kind": "agent-orchestrator-dispatch-state",
                 "version": 1,
                 "issue_number": LEDGER,
                 "dispatch_id": f"plan-promote:{CLOSED}:{ATTEMPT}",
                 "action": "plan-promote",
                 "status": "promoted",
                 "details": {"successor_id": SUCCESSOR},
             }):
            result = self._runner(github).run_route_once(CLOSED, ATTEMPT)
        self.assertEqual(result.status, "promoted")
        self.assertEqual(result.details["successor_id"], SUCCESSOR)
        github.dispatch_controller.assert_called_once_with(
            "promote-plan", {"packet_id": CLOSED, "attempt_id": ATTEMPT}
        )

    def test_no_eligible_successor_dispatches_bounded_pause(self):
        import plan_lifecycle

        github = self._github()
        github.accepted_route_document.return_value = future_document([BLOCKED_SKETCH])
        with mock.patch.object(plan_lifecycle, "_exact_plan_claim", return_value=_closed_claim()):
            result = self._runner(github).run_route_once(CLOSED, ATTEMPT)
        self.assertEqual(result.status, "bounded_pause")
        self.assertEqual(result.details["reason"], "no_eligible_successor")
        github.dispatch_controller.assert_called_once_with(
            "promote-plan", {"packet_id": CLOSED, "attempt_id": ATTEMPT}
        )

    def test_compiles_and_opens_promotion_draft_pr(self):
        import plan_lifecycle
        import pr_binding
        import worktree_manager
        import local_run_once

        remote_ref = f"refs/heads/agent/packet-{SUCCESSOR.lower()}"
        ls_remote_calls = {"count": 0}

        def fake_git(_wt, *args):
            if args[0] == "commit":
                return ""
            if args[0] == "rev-parse":
                return "d" * 40
            if args[0] == "ls-remote":
                ls_remote_calls["count"] += 1
                if ls_remote_calls["count"] == 1:
                    return ""
                return f"{'d'*40}\t{remote_ref}"
            return ""

        github = self._github()
        with mock.patch.object(plan_lifecycle, "_exact_plan_claim", return_value=_closed_claim()), \
             mock.patch.object(
                 worktree_manager, "create_plan_worktree",
                 return_value=(str(Path("/tmp/route-drive-wt")), "ignored", MAIN, None),
             ), \
             mock.patch.object(
                 worktree_manager, "remove_plan_worktree", return_value=True,
             ), \
             mock.patch.object(
                 local_run_once, "_bounded_process", return_value=(0, "", ""),
             ), \
             mock.patch.object(
                 local_run_once.LocalRunOnce, "_git_checked", side_effect=fake_git,
             ), \
             mock.patch.object(
                 pr_binding, "create_or_update_plan_pr",
                 return_value={"number": 7, "head_sha": "d" * 40},
             ) as create_pr, \
             mock.patch.object(
                 pr_binding, "verify_post_push_plan_binding", return_value=None,
             ):
            result = self._runner(github).run_route_once(CLOSED, ATTEMPT)
        self.assertEqual(result.status, "promotion_pr", result.details)
        self.assertEqual(result.details["pr_number"], 7)
        self.assertEqual(result.details["successor_id"], SUCCESSOR)
        self.assertEqual(result.details["head_sha"], "d" * 40)
        create_pr.assert_called_once()
        marker_args = create_pr.call_args.args
        self.assertIn("agent-orchestrator-binding", marker_args[6])
        self.assertIn(SUCCESSOR, marker_args[6])

    def test_resume_pauses_when_review_receipt_pending(self):
        import plan_lifecycle
        import pr_binding
        import local_run_once

        remote_ref = f"refs/heads/agent/packet-{SUCCESSOR.lower()}"
        github = self._github()

        def fake_git(_wt, *args):
            if args[0] == "ls-remote":
                return f"{'d'*40}\t{remote_ref}"
            return ""

        with mock.patch.object(plan_lifecycle, "_exact_plan_claim", return_value=_closed_claim()), \
             mock.patch.object(
                 local_run_once.LocalRunOnce, "_git_checked", side_effect=fake_git,
             ), \
             mock.patch.object(
                 pr_binding, "find_plan_pr",
                 return_value={"number": 7, "head_sha": "d" * 40},
             ), \
             mock.patch.object(plan_lifecycle, "plan_review_receipt", return_value=None):
            result = self._runner(github).run_route_once(CLOSED, ATTEMPT)
        self.assertEqual(result.status, "bounded_pause")
        self.assertEqual(result.details["reason"], "review_receipt_pending")

    def test_resume_settles_after_eligible_merge(self):
        import plan_lifecycle
        import pr_binding
        import dispatcher
        import ci_verifier
        import state_manager
        import local_run_once

        remote_ref = f"refs/heads/agent/packet-{SUCCESSOR.lower()}"
        github = self._github()

        def fake_git(_wt, *args):
            if args[0] == "ls-remote":
                return f"{'d'*40}\t{remote_ref}"
            return ""

        pr = {"number": 7, "head_sha": "d" * 40}
        review = {"kind": "agent-orchestrator-review-state", "verdict": "PASS"}
        run = {"databaseId": 11, "conclusion": "success", "headSha": "d" * 40}
        with mock.patch.object(plan_lifecycle, "_exact_plan_claim", return_value=_closed_claim()), \
             mock.patch.object(
                 local_run_once.LocalRunOnce, "_git_checked", side_effect=fake_git,
             ), \
             mock.patch.object(pr_binding, "find_plan_pr", return_value=pr), \
             mock.patch.object(plan_lifecycle, "plan_review_receipt", return_value=review), \
             mock.patch.object(ci_verifier, "find_exact_runs", return_value=[run]), \
             mock.patch.object(ci_verifier, "select_canonical_run", return_value=run), \
             mock.patch.object(
                 dispatcher, "_authoritative_plan_merge", return_value="c" * 40,
             ), \
             mock.patch.object(
                 state_manager, "read_dispatch_state",
                 return_value={
                     "kind": "agent-orchestrator-dispatch-state",
                     "version": 1,
                     "issue_number": LEDGER,
                     "dispatch_id": f"plan-promote:{CLOSED}:{ATTEMPT}",
                     "action": "plan-promote",
                     "status": "promoted",
                     "details": {"successor_id": SUCCESSOR},
                 },
             ):
            result = self._runner(github).run_route_once(CLOSED, ATTEMPT)
        self.assertEqual(result.status, "promoted")
        self.assertEqual(result.details["successor_id"], SUCCESSOR)
        self.assertEqual(result.details["merge_commit_sha"], "c" * 40)
        github.dispatch_controller.assert_called_once_with(
            "promote-plan", {"packet_id": CLOSED, "attempt_id": ATTEMPT}
        )


if __name__ == "__main__":
    unittest.main()
