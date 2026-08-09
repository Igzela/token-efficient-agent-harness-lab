from __future__ import annotations

import importlib.util
import io
import json
import subprocess
import sys
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import patch


def load_handoff_checker():
    repo_root = Path(__file__).resolve().parents[1]
    script = repo_root / "scripts" / "check_agent_handoff.py"
    spec = importlib.util.spec_from_file_location("check_agent_handoff", script)
    assert spec is not None
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def completed(command: list[str], returncode: int = 0, stdout: str = ""):
    return subprocess.CompletedProcess(command, returncode, stdout=stdout, stderr="")


def with_future_inventory(checker, future: str) -> str:
    payload = checker.future_route_inventory_payload(future)
    marker = "<!-- future-route-inventory:v1 " + json.dumps(
        payload, sort_keys=True, separators=(",", ":")
    ) + " -->"
    return future.replace(
        "## Portfolio Inventory Manifest",
        "## Portfolio Inventory Manifest\n\n" + marker,
        1,
    )


class CheckAgentHandoffTests(unittest.TestCase):
    def test_handoff_guard_runs_full_secret_scan(self) -> None:
        checker = load_handoff_checker()
        commands: list[list[str]] = []

        def fake_run(command, **_kwargs):
            commands.append([str(part) for part in command])
            return completed(commands[-1])

        with patch.object(checker.subprocess, "run", side_effect=fake_run):
            self.assertEqual(checker.main(), 0)

        self.assertTrue(
            any(command[-1].endswith("scripts/acp_secret_scan.py") for command in commands),
            commands,
        )

    def test_handoff_guard_fails_when_secret_scan_fails(self) -> None:
        checker = load_handoff_checker()

        def fake_run(command, **_kwargs):
            normalized = [str(part) for part in command]
            if normalized[-1].endswith("scripts/acp_secret_scan.py"):
                return completed(normalized, returncode=1, stdout="Secret scan findings:\n- x")
            return completed(normalized)

        output = io.StringIO()
        with patch.object(checker.subprocess, "run", side_effect=fake_run):
            with redirect_stdout(output):
                self.assertEqual(checker.main(), 1)

        self.assertIn("Agent handoff check FAILED — secret scan:", output.getvalue())


    def test_structural_guard_rejects_completed_active_route(self) -> None:
        checker = load_handoff_checker()
        next_text = """| Stage | Priority | Goal | Status |
|---|---|---|---|
| PE-3 | P1 | x | Complete |
### Packet PE3-A-1 — a
**State:** `COMPLETE`
## Active Routing
1. Execute PE3-A-1.
"""
        failures = checker.active_state_failures(next_text, next_text)
        self.assertIn("Active Routing points to completed packet PE3-A-1", failures)

    def test_structural_guard_accepts_terminal_objective_routing(self) -> None:
        checker = load_handoff_checker()
        text = """| Stage | Priority | Goal | Status |
|---|---|---|---|
| PE-3 | P1 | x | Complete |
### Packet PE3-A-1 — a
**State:** `COMPLETE`
## Active Routing
1. Terminal objective: PE3-A-1 is complete; no later packet is activated.
"""
        self.assertEqual(checker.active_state_failures(text, text), [])

    def test_structural_guard_rejects_duplicate_packet_state(self) -> None:
        checker = load_handoff_checker()
        next_text = """### Packet PE3-A-1 — a
**State:** `COMPLETE`
**State:** `IN_PROGRESS`
## Active Routing
1. Execute PE3-A-1.
"""
        failures = checker.active_state_failures(next_text, next_text)
        self.assertTrue(
            any("exactly one structural State" in failure for failure in failures),
            failures,
        )

    def test_structural_guard_rejects_incomplete_prerequisite(self) -> None:
        checker = load_handoff_checker()
        next_text = """| Stage | Priority | Goal | Status |
|---|---|---|---|
| PE-3 | P1 | x | In progress |
### Packet PE3-A-1 — a
**State:** `IN_PROGRESS`
### Packet PE3-B-1 — b
**State:** `READY_FOR_EXECUTION`
**Prerequisite:** PE3-A-1 complete.
## Active Routing
1. Execute PE3-B-1.
"""
        failures = checker.active_state_failures(next_text, next_text)
        self.assertTrue(
            any("prerequisites are not complete" in failure for failure in failures),
            failures,
        )

    def test_structural_guard_rejects_stage_summary_without_owner(self) -> None:
        checker = load_handoff_checker()
        next_text = """| Stage | Priority | Goal | Status |
|---|---|---|---|
| PE-3 | P1 | x | In progress |
### Packet PE3-A-1 — a
**State:** `COMPLETE`
### Packet PE3-B-1 — b
**State:** `BLOCKED_PREREQUISITE`
**Prerequisite:** PE3-A-1 complete.
## Active Routing
1. Execute PE3-B-1.
"""
        failures = checker.active_state_failures(next_text, next_text)
        self.assertIn("PE-3 summary says in progress but no packet is IN_PROGRESS", failures)

    def test_structural_guard_accepts_consistent_packet_routing(self) -> None:
        checker = load_handoff_checker()
        text = """## Active Tracks
| Track | Status |
|---|---|
| x | active |
## Planned Product Evolution Stages
| Stage | Priority | Goal | Status |
|---|---|---|---|
| PE-3 | P1 | x | In progress |
### Packet PE3-A-1 — a
**State:** `COMPLETE`
### Packet PE3-B-1 — b
**State:** `IN_PROGRESS`
**Prerequisite:** PE3-A-1 complete.
## Active Routing
1. Execute PE3-B-1.
"""
        self.assertEqual(checker.active_state_failures(text, text), [])

    def test_structural_guard_parses_current_level_packet_headings(self) -> None:
        checker = load_handoff_checker()
        text = """| Stage | Priority | Goal | Status |
|---|---|---|---|
| PE-5 | P1.5 | Release Provenance | Activated; first packet ready |
## Packet PE5-CONTRACT-1 — contract
**State:** `READY_FOR_EXECUTION`
## Packet PE5-SBOM-1 — sbom
**State:** `BLOCKED_PREREQUISITE`
**Prerequisite:** PE5-CONTRACT-1 complete.
## Active Routing
1. Start PE5-CONTRACT-1.
"""

        packets = checker.parse_packet_contracts(text, [])
        self.assertEqual(packets["PE5-CONTRACT-1"]["state"], "READY_FOR_EXECUTION")
        self.assertEqual(
            packets["PE5-SBOM-1"]["prerequisites"], ["PE5-CONTRACT-1"]
        )
        self.assertEqual(checker.active_state_failures(text, text), [])

    def test_future_route_must_remain_blocked(self) -> None:
        checker = load_handoff_checker()
        current = """## Active Routing
1. `PE7-A-1` — `READY_FOR_EXECUTION`.
## Packet PE7-A-1
**State:** `READY_FOR_EXECUTION`
"""
        future = """# Future Route
### Packet PE7-B-1
**State:** `READY_FOR_EXECUTION`
**Prerequisite:** PE7-A-1
"""
        failures = checker.active_state_failures("", current, future)
        self.assertIn(
            "FUTURE_ROUTE packet PE7-B-1 must remain BLOCKED_PREREQUISITE",
            failures,
        )

    def test_packet_cannot_exist_in_current_and_future_documents(self) -> None:
        checker = load_handoff_checker()
        current = """## Active Routing
1. `PE7-A-1` — `READY_FOR_EXECUTION`.
## Packet PE7-A-1
**State:** `READY_FOR_EXECUTION`
"""
        future = """# Future Route
### Packet PE7-A-1
**State:** `BLOCKED_PREREQUISITE`
"""
        failures = checker.active_state_failures("", current, future)
        self.assertIn(
            "PE7-A-1 is duplicated between NEXT_DECISION and FUTURE_ROUTE",
            failures,
        )

    def test_active_routing_cannot_point_to_future_route(self) -> None:
        checker = load_handoff_checker()
        current = """## Active Routing
1. `PE7-B-1` — `BLOCKED_PREREQUISITE`.
## Packet PE7-A-1
**State:** `READY_FOR_EXECUTION`
"""
        future = """# Future Route
### Packet PE7-B-1
**State:** `BLOCKED_PREREQUISITE`
**Prerequisite:** PE7-A-1
"""
        failures = checker.active_state_failures("", current, future)
        self.assertIn(
            "Active Routing references routing-only FUTURE_ROUTE packet PE7-B-1",
            failures,
        )

    def test_accepted_packet_receipt_satisfies_current_prerequisite(self) -> None:
        checker = load_handoff_checker()
        status = """## Accepted Packet Receipts
| Packet | State | Accepted evidence |
|---|---|---|
| `PE7-A-1` | `COMPLETE` | merge `aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa` |
"""
        current = """## Active Routing
1. `PE7-B-1` — `READY_FOR_EXECUTION`.
## Packet PE7-B-1
**State:** `READY_FOR_EXECUTION`
**Prerequisite:** PE7-A-1
"""

        self.assertEqual(checker.active_state_failures(status, current), [])

    def test_accepted_packet_receipt_is_scoped_to_owner_section(self) -> None:
        checker = load_handoff_checker()
        status = """## Unrelated Table
| Packet | State | Accepted evidence |
|---|---|---|
| `PE7-A-1` | `COMPLETE` | merge `aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa` |
"""

        self.assertEqual(checker.accepted_packet_receipts(status), set())

    def test_accepted_packet_receipt_requires_durable_evidence_identity(self) -> None:
        checker = load_handoff_checker()
        status = """## Accepted Packet Receipts
| Packet | State | Accepted evidence |
|---|---|---|
| `PE7-A-1` | `COMPLETE` | looks good |
"""
        failures: list[str] = []

        self.assertEqual(checker.accepted_packet_receipts(status, failures), set())
        self.assertTrue(any("durable evidence identity" in item for item in failures))

    def test_future_route_rejects_packet_without_execution_profile(self) -> None:
        checker = load_handoff_checker()
        future = """# Future Route

This document is routing-only.

### Packet PE7-B-1

**State:** `BLOCKED_PREREQUISITE`
**Prerequisite:** PE7-A-1
**Class:** `CONTRACT`
"""

        failures = checker.future_route_contract_failures(future)

        self.assertTrue(
            any("missing future-route section" in failure for failure in failures),
            failures,
        )
        self.assertTrue(
            any("PE7-B-1 is missing Execution profile" in failure for failure in failures),
            failures,
        )

    def test_future_route_accepts_complete_weak_agent_profile(self) -> None:
        checker = load_handoff_checker()
        future = """# Future Route

This document is routing-only.

## Weak-Agent Full-Course Contract
## Worker Tiers
## Cheap-Agent Dispatch Protocol
## Known Planned-Seam Gaps
## Promotion Contract
## Stop and Resume Protocol
## Execution Profile Field Contract
## Portfolio Inventory Manifest

### Packet PE7-B-1

**State:** `BLOCKED_PREREQUISITE`
**Prerequisite:** PE7-A-1
**Class:** `CONTRACT`
**Execution profile:** `PE7-B-1.v1`
**Worker tier:** `T2`
**Owner/seam:** Existing RWE authority; revalidate the exact symbol on promotion.
**Allowed paths at promotion:** `engine/src/rwe/**`, tests, and canonical docs only.
**Ordered work:** Revalidate owner -> freeze contract -> add negative tests -> verify.
**Verification:** Focused tests, baseline handoff checks, and exact diff review.
**Rollback/recovery:** Revert the packet; preserve prior accepted artifacts and receipts.
**Human/effect gate:** No external effect; primary approval is required for contract choices.
**Consolidation boundary:** Do not combine with implementation or effect packets.
**Negative-result route:** Record `DECISION_REQUIRED`; do not invent a contract value.
**Outcome:** Freeze the exact RWE contract before any implementation or external effect.
**Allowed delta:** Versioned planning evidence only; runtime and authority remain unchanged.
**Exit:** One independently reviewed and hash-bound contract with explicit owners.
**Stop:** Any missing authority owner, unresolved value, or stale accepted prerequisite.
"""
        future = with_future_inventory(checker, future)

        self.assertEqual(checker.future_route_contract_failures(future), [])

    def test_future_route_requires_full_base_packet_contract(self) -> None:
        checker = load_handoff_checker()
        future = """# Future Route
## Weak-Agent Full-Course Contract
## Worker Tiers
## Cheap-Agent Dispatch Protocol
## Known Planned-Seam Gaps
## Promotion Contract
## Stop and Resume Protocol
## Execution Profile Field Contract
## Portfolio Inventory Manifest
### Packet PE7-B-1
**State:** `BLOCKED_PREREQUISITE`
**Prerequisite:** PE7-A-1
**Class:** `CONTRACT`
**Execution profile:** `PE7-B-1.v1`
**Worker tier:** `T2`
**Owner/seam:** Existing RWE authority; revalidate exact symbols before promotion.
**Allowed paths at promotion:** Narrow exact RWE owner and test paths on promotion.
**Ordered work:** Inventory -> freeze -> test -> independently review the contract.
**Verification:** Run focused negative tests, handoff checks, and complete-diff review.
**Rollback/recovery:** Revert planning evidence and retain prior accepted receipts.
**Human/effect gate:** T2 accepts contract decisions; external effects stay forbidden.
**Consolidation boundary:** Keep this contract separate from implementation and effects.
**Negative-result route:** Stop DECISION_REQUIRED without inventing contract values.
"""
        failures = checker.future_route_contract_failures(future)

        self.assertTrue(any("missing Outcome" in item for item in failures), failures)
        self.assertTrue(any("missing Allowed delta" in item for item in failures), failures)
        self.assertTrue(any("missing Exit" in item for item in failures), failures)
        self.assertTrue(any("missing Stop" in item for item in failures), failures)

    def test_future_route_profile_id_must_bind_packet_id(self) -> None:
        checker = load_handoff_checker()
        future = """# Future Route
## Weak-Agent Full-Course Contract
## Worker Tiers
## Cheap-Agent Dispatch Protocol
## Known Planned-Seam Gaps
## Promotion Contract
## Stop and Resume Protocol
## Execution Profile Field Contract
## Portfolio Inventory Manifest
### Packet PE7-B-1
**State:** `BLOCKED_PREREQUISITE`
**Prerequisite:** PE7-A-1
**Class:** `CONTRACT`
**Execution profile:** `SOME-GENERIC-PROFILE.v1`
**Worker tier:** `T2`
"""

        failures = checker.future_route_contract_failures(future)

        self.assertTrue(any("must equal 'PE7-B-1.v1'" in item for item in failures), failures)

    def test_future_route_inventory_rejects_tampered_packet_count(self) -> None:
        checker = load_handoff_checker()
        future = """# Future Route
## Weak-Agent Full-Course Contract
## Worker Tiers
## Cheap-Agent Dispatch Protocol
## Known Planned-Seam Gaps
## Promotion Contract
## Stop and Resume Protocol
## Execution Profile Field Contract
## Portfolio Inventory Manifest
### Packet PE7-B-1
**State:** `BLOCKED_PREREQUISITE`
**Prerequisite:** PE7-A-1
**Class:** `CONTRACT`
**Execution profile:** `PE7-B-1.v1`
**Worker tier:** `T2`
**Owner/seam:** Existing RWE authority; revalidate exact symbols before promotion.
**Allowed paths at promotion:** Narrow exact RWE owner and test paths on promotion.
**Ordered work:** Inventory -> freeze -> test -> independently review the contract.
**Verification:** Run focused negative tests, handoff checks, and complete-diff review.
**Rollback/recovery:** Revert planning evidence and retain prior accepted receipts.
**Human/effect gate:** T2 accepts contract decisions; external effects stay forbidden.
**Consolidation boundary:** Keep this contract separate from implementation and effects.
**Negative-result route:** Stop DECISION_REQUIRED without inventing contract values.
**Outcome:** Freeze the exact RWE contract before implementation or external effects.
**Allowed delta:** Versioned planning evidence only; runtime and authority stay unchanged.
**Exit:** One independently reviewed and hash-bound contract with explicit owners.
**Stop:** Any missing authority owner, unresolved value, or stale prerequisite.
"""
        future = with_future_inventory(checker, future).replace(
            '"packet_count":1', '"packet_count":2', 1
        )

        failures = checker.future_route_contract_failures(future)

        self.assertTrue(any("inventory manifest mismatch" in item for item in failures), failures)

    def test_weak_agent_dispatch_capsule_rejects_missing_or_unsafe_payload(self) -> None:
        checker = load_handoff_checker()
        current = """## Packet PE7-A-1
**State:** `READY_FOR_EXECUTION`
"""
        packets = checker.parse_packet_contracts(current, [])

        missing = checker.weak_agent_dispatch_failures(current, packets)
        self.assertTrue(any("missing weak-agent-dispatch" in item for item in missing))

        unsafe = current + """
## Weak-Agent Dispatch Capsule
<!-- weak-agent-dispatch:v1 {"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-A-1","external_effect_limit":1,"authority_consumption_allowed":true} -->
"""
        failures = checker.weak_agent_dispatch_failures(unsafe, packets)
        self.assertTrue(any("external_effect_limit=0" in item for item in failures))
        self.assertTrue(any("authority consumption" in item for item in failures))

    def test_future_route_requires_cheap_agent_dispatch_protocol(self) -> None:
        checker = load_handoff_checker()
        future = """# Future Route
## Weak-Agent Full-Course Contract
## Worker Tiers
## Promotion Contract
## Stop and Resume Protocol
## Execution Profile Field Contract
"""

        failures = checker.future_route_contract_failures(future)

        self.assertIn(
            "FUTURE_ROUTE is missing future-route section '## Cheap-Agent Dispatch Protocol'",
            failures,
        )

    def test_future_route_requires_planned_seam_gap_registry(self) -> None:
        checker = load_handoff_checker()
        future = """# Future Route
## Weak-Agent Full-Course Contract
## Worker Tiers
## Cheap-Agent Dispatch Protocol
## Promotion Contract
## Stop and Resume Protocol
## Execution Profile Field Contract
"""

        failures = checker.future_route_contract_failures(future)

        self.assertIn(
            "FUTURE_ROUTE is missing future-route section '## Known Planned-Seam Gaps'",
            failures,
        )

    def test_future_route_rejects_duplicate_profile_ids_and_placeholders(self) -> None:
        checker = load_handoff_checker()
        header = """# Future Route
## Weak-Agent Full-Course Contract
## Worker Tiers
## Cheap-Agent Dispatch Protocol
## Known Planned-Seam Gaps
## Promotion Contract
## Stop and Resume Protocol
## Execution Profile Field Contract
"""
        packet = """### Packet {packet_id}
**State:** `BLOCKED_PREREQUISITE`
**Prerequisite:** PE7-A-1
**Class:** `CONTRACT`
**Execution profile:** `DUPLICATED.v1`
**Worker tier:** `T2`
**Owner/seam:** {owner}
**Allowed paths at promotion:** Exact accepted paths are narrowed on promotion.
**Ordered work:** Inventory -> freeze -> test -> independently review the contract.
**Verification:** Focused negative tests plus handoff and complete-diff review.
**Rollback/recovery:** Revert the contract while retaining accepted evidence.
**Human/effect gate:** T2 accepts contract choices; no external effect.
**Consolidation boundary:** Keep separate from implementation and effects.
**Negative-result route:** Stop DECISION_REQUIRED without inventing values.
"""
        future = header + packet.format(packet_id="PE7-B-1", owner="TBD") + packet.format(
            packet_id="PE7-C-1", owner="Existing owner revalidated on promotion."
        )

        failures = checker.future_route_contract_failures(future)

        self.assertTrue(any("placeholder Owner/seam" in item for item in failures), failures)
        self.assertTrue(any("duplicate Execution profile" in item for item in failures), failures)

    def test_future_route_rejects_unknown_prerequisite(self) -> None:
        checker = load_handoff_checker()
        current = """## Active Routing
1. `PE7-A-1` — `READY_FOR_EXECUTION`.
## Packet PE7-A-1
**State:** `READY_FOR_EXECUTION`
"""
        future = """# Future Route
### Packet PE7-B-1
**State:** `BLOCKED_PREREQUISITE`
**Prerequisite:** PE7-MISSING-1
"""

        failures = checker.active_state_failures("", current, future)

        self.assertIn(
            "PE7-B-1 references unknown prerequisites: ['PE7-MISSING-1']",
            failures,
        )

    def test_future_route_rejects_dependency_cycle(self) -> None:
        checker = load_handoff_checker()
        current = """## Active Routing
1. `PE7-A-1` — `READY_FOR_EXECUTION`.
## Packet PE7-A-1
**State:** `READY_FOR_EXECUTION`
"""
        future = """# Future Route
### Packet PE7-B-1
**State:** `BLOCKED_PREREQUISITE`
**Prerequisite:** PE7-C-1
### Packet PE7-C-1
**State:** `BLOCKED_PREREQUISITE`
**Prerequisite:** PE7-B-1
"""

        failures = checker.active_state_failures("", current, future)

        self.assertTrue(
            any("packet dependency cycle" in failure for failure in failures),
            failures,
        )


if __name__ == "__main__":
    unittest.main()
