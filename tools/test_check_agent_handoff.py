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
    def test_actual_start_here_has_a_valid_route_for_every_agent_role(self) -> None:
        checker = load_handoff_checker()
        start_here = (Path(__file__).resolve().parents[1] / "START_HERE.md").read_text(
            encoding="utf-8"
        )
        self.assertEqual(checker.session_context_route_failures(start_here), [])

    def test_start_here_uses_only_fixed_auto_checkpoint_commands(self) -> None:
        start_here = (Path(__file__).resolve().parents[1] / "START_HERE.md").read_text(
            encoding="utf-8"
        )
        self.assertIn("`checkpoint_write_commands`", start_here)
        self.assertIn(
            "stable command is permitted only after every declared verification command "
            "has actually passed",
            start_here,
        )
        self.assertNotIn("--completed-step", start_here)
        self.assertNotIn("--next-action", start_here)
        self.assertNotIn("repeat `--owned-path`", start_here)

    def test_next_decision_hygiene_rejects_size_and_append_only_history(self) -> None:
        checker = load_handoff_checker()
        oversized = "x\n" * (checker.NEXT_DECISION_MAX_LINES + 1)
        failures = checker.next_decision_hygiene_failures(oversized)
        self.assertTrue(any("line budget" in failure for failure in failures), failures)

        for heading in (
            "## Changelog",
            "## Progress Log",
            "### Session Notes",
            "## Handoff History",
        ):
            with self.subTest(heading=heading):
                failures = checker.next_decision_hygiene_failures(
                    "# Next Decision\n\n" + heading + "\n\nold session data\n"
                )
                self.assertTrue(
                    any("append-only history" in failure for failure in failures),
                    failures,
                )

    def test_next_decision_hygiene_accepts_bounded_replace_only_content(self) -> None:
        checker = load_handoff_checker()
        text = """# Next Decision

## Current Direction

Replace stale status in place.

## Active Routing

1. `TOOL-CONTEXT-1`
"""
        self.assertEqual(checker.next_decision_hygiene_failures(text), [])

    def test_handoff_guard_runs_full_secret_scan(self) -> None:
        checker = load_handoff_checker()
        commands: list[list[str]] = []

        def fake_run(command, **_kwargs):
            commands.append([str(part) for part in command])
            if commands[-1][:3] == ["git", "rev-parse", "origin/main"]:
                return completed(commands[-1], stdout="c3e58576cbba40dbcad666c39eefb6bbdc372434\n")
            if commands[-1][:2] == ["git", "cat-file"]:
                return completed(commands[-1])
            if commands[-1][:3] == ["git", "merge-base", "--is-ancestor"]:
                return completed(commands[-1])
            if commands[-1][:2] == ["git", "show"]:
                return completed(
                    commands[-1],
                    stdout="## Packet PE7-RWE-V2-VIABILITY-PREFLIGHT-1\n",
                )
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

    def test_accepted_complete_current_packet_requires_one_matching_bootstrap_marker(self) -> None:
        checker = load_handoff_checker()
        status = """## Accepted Packet Receipts
| Packet | State | Accepted evidence |
|---|---|---|
| `PE7-A-1` | `COMPLETE` | merge `aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa` |
"""
        current = """## Active Routing
1. `PE7-A-1` — `READY_FOR_EXECUTION`.
<!-- route-bootstrap-reconcile:v1 packet_id=PE7-A-1 -->
## Packet PE7-A-1
**State:** `READY_FOR_EXECUTION`
"""
        self.assertEqual(checker.active_state_failures(status, current), [])

    def test_bootstrap_marker_cannot_name_an_unaccepted_or_wrong_packet(self) -> None:
        checker = load_handoff_checker()
        current = """## Active Routing
1. `PE7-A-1` — `READY_FOR_EXECUTION`.
<!-- route-bootstrap-reconcile:v1 packet_id=PE7-B-1 -->
## Packet PE7-A-1
**State:** `READY_FOR_EXECUTION`
"""
        failures = checker.active_state_failures("", current)
        self.assertIn(
            "route-bootstrap-reconcile marker must name one accepted READY_FOR_EXECUTION current packet",
            failures,
        )

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

    def test_structural_guard_rejects_inline_route_state_mismatch(self) -> None:
        checker = load_handoff_checker()
        text = """## Packet PE3-A-1
**State:** `READY_FOR_EXECUTION`
## Active Routing
1. `PE3-A-1` — `BLOCKED_PREREQUISITE`.
"""
        failures = checker.active_state_failures(text, text)
        self.assertIn(
            "Active Routing says PE3-A-1 is BLOCKED_PREREQUISITE but its structural State is READY_FOR_EXECUTION",
            failures,
        )

    def test_historical_packet_can_satisfy_a_future_dependency_without_becoming_current(self) -> None:
        checker = load_handoff_checker()
        current = """## Packet PE7-A-1
**State:** `BLOCKED_PREREQUISITE`
## Retained Contract (historical: PE7-RWE-V2-VIABILITY-PREFLIGHT-1)
**Historical state:** `BLOCKED_PREREQUISITE`
**Historical source:** accepted main c3e58576cbba40dbcad666c39eefb6bbdc372434
## Active Routing
1. `PE7-A-1` — `BLOCKED_PREREQUISITE`.
"""
        future = """# Future Route
## Worker Tiers
## Known Planned-Seam Gaps
## Promotion Profile Contract
## Stop and Resume Protocol
## Portfolio Inventory Manifest
### Packet PE7-B-1
**State:** `BLOCKED_PREREQUISITE`
**Prerequisite:** PE7-RWE-V2-VIABILITY-PREFLIGHT-1
**Class:** `CONTRACT`
**Outcome:** Preserve a historical prerequisite identity.
**Allowed delta:** No implementation.
**Exit:** The successor is re-expanded later.
**Stop:** Stop on missing identity.
"""
        future = with_future_inventory(checker, future)
        self.assertEqual(checker.active_state_failures("", current, future), [])

        invalid = current.replace(
            "c3e58576cbba40dbcad666c39eefb6bbdc372434", "b" * 40
        )
        failures = checker.active_state_failures("", invalid, future)
        self.assertTrue(
            any("source is not a repository commit" in failure for failure in failures),
            failures,
        )

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

    def test_future_route_rejects_incomplete_packet_and_missing_manifest(self) -> None:
        checker = load_handoff_checker()
        future = """# Future Route

This document is routing-only.

### Packet PE7-B-1

**State:** `BLOCKED_PREREQUISITE`
**Prerequisite:** PE7-A-1
**Class:** `CONTRACT`
"""

        failures = checker.future_route_profile_failures(future)

        self.assertTrue(
            any("missing future-route section" in failure for failure in failures),
            failures,
        )
        self.assertTrue(
            any("missing future-route-inventory:v1 marker" in failure for failure in failures),
            failures,
        )
        self.assertTrue(
            any("PE7-B-1 is missing Outcome" in failure for failure in failures),
            failures,
        )

    def test_future_route_accepts_complete_weak_agent_profile(self) -> None:
        checker = load_handoff_checker()
        future = """# Future Route

This document is routing-only.

## Worker Tiers
## Known Planned-Seam Gaps
## Promotion Profile Contract
## Stop and Resume Protocol
## Portfolio Inventory Manifest

### Packet PE7-B-1

**State:** `BLOCKED_PREREQUISITE`
**Prerequisite:** PE7-A-1
**Class:** `CONTRACT`
**Outcome:** Freeze the exact RWE contract before any implementation or external effect.
**Allowed delta:** Versioned planning evidence only; runtime and authority remain unchanged.
**Exit:** One independently reviewed and hash-bound contract with explicit owners.
**Stop:** Any missing authority owner, unresolved value, or stale accepted prerequisite.
"""
        future = with_future_inventory(checker, future)

        self.assertEqual(checker.future_route_profile_failures(future), [])

    def test_future_route_requires_full_base_packet_contract(self) -> None:
        checker = load_handoff_checker()
        future = """# Future Route
## Worker Tiers
## Known Planned-Seam Gaps
## Promotion Profile Contract
## Stop and Resume Protocol
## Portfolio Inventory Manifest
### Packet PE7-B-1
**State:** `BLOCKED_PREREQUISITE`
**Prerequisite:** PE7-A-1
**Class:** `CONTRACT`
"""
        failures = checker.future_route_profile_failures(future)

        self.assertTrue(any("missing Outcome" in item for item in failures), failures)
        self.assertTrue(any("missing Allowed delta" in item for item in failures), failures)
        self.assertTrue(any("missing Exit" in item for item in failures), failures)
        self.assertTrue(any("missing Stop" in item for item in failures), failures)

    def test_future_route_inventory_rejects_tampered_packet_count(self) -> None:
        checker = load_handoff_checker()
        future = """# Future Route
## Worker Tiers
## Known Planned-Seam Gaps
## Promotion Profile Contract
## Stop and Resume Protocol
## Portfolio Inventory Manifest
### Packet PE7-B-1
**State:** `BLOCKED_PREREQUISITE`
**Prerequisite:** PE7-A-1
**Class:** `CONTRACT`
**Outcome:** Freeze the exact contract before implementation or external effects.
**Allowed delta:** Versioned planning evidence only; runtime and authority stay unchanged.
**Exit:** One independently reviewed and hash-bound contract with explicit owners.
**Stop:** Any missing authority owner, unresolved value, or stale prerequisite.
"""
        future = with_future_inventory(checker, future).replace(
            '"packet_count":1', '"packet_count":2', 1
        )

        failures = checker.future_route_profile_failures(future)

        self.assertTrue(any("packet_count is stale" in item for item in failures), failures)

    def test_future_route_inventory_rejects_profile_row_for_wrong_packet(self) -> None:
        checker = load_handoff_checker()
        future = """# Future Route
## Worker Tiers
## Known Planned-Seam Gaps
## Promotion Profile Contract
## Stop and Resume Protocol
## Portfolio Inventory Manifest
### Packet PE7-B-1
**State:** `BLOCKED_PREREQUISITE`
**Prerequisite:** PE7-A-1
**Class:** `CONTRACT`
**Outcome:** Freeze the exact contract before implementation or external effects.
**Allowed delta:** Versioned planning evidence only; runtime and authority stay unchanged.
**Exit:** One independently reviewed and hash-bound contract with explicit owners.
**Stop:** Any missing authority owner, unresolved value, or stale prerequisite.
"""
        future = with_future_inventory(checker, future).replace(
            '"PE7-B-1","CONTRACT","T2"',
            '"PE7-X-9","CONTRACT","T2"',
            1,
        )

        failures = checker.future_route_profile_failures(future)

        self.assertTrue(any("is malformed" in item for item in failures), failures)

    def test_future_route_inventory_rejects_effect_profile_without_t3(self) -> None:
        checker = load_handoff_checker()
        future = """# Future Route
## Worker Tiers
## Known Planned-Seam Gaps
## Promotion Profile Contract
## Stop and Resume Protocol
## Portfolio Inventory Manifest
### Packet PE7-B-1
**State:** `BLOCKED_PREREQUISITE`
**Prerequisite:** PE7-A-1
**Class:** `EFFECT`
**Outcome:** Execute one pre-registered effect under a finite one-use authority.
**Allowed delta:** Only the pre-authorized external effects may occur.
**Exit:** Honest terminal classification with complete evidence bindings.
**Stop:** Authority or hash mismatch, duplicate identity, or outcome unknown.
"""
        future = with_future_inventory(checker, future).replace(
            '"EFFECT","T3","external_effect"',
            '"EFFECT","T1","external_effect"',
            1,
        )

        failures = checker.future_route_profile_failures(future)

        self.assertTrue(
            any("EFFECT profile must use Worker tier T3" in item for item in failures),
            failures,
        )

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

    def test_weak_agent_dispatch_is_not_required_for_planning_parked_window(self) -> None:
        checker = load_handoff_checker()
        current = """## Packet PE7-A-1
**State:** `DECISION_REQUIRED`
"""
        packets = checker.parse_packet_contracts(current, [])
        self.assertEqual(checker.weak_agent_dispatch_failures(current, packets), [])

    def test_forward_order_window_mismatch_is_rejected(self) -> None:
        checker = load_handoff_checker()
        next_text = """## Authoritative Forward Order

```text
[window: viability preflight — DECISION_REQUIRED, planning must expand its contract]
→ separately authorized viability run
```

## Packet PE7-RWE-V2-VIABILITY-PREFLIGHT-1
**State:** `READY_FOR_EXECUTION`
## Active Routing
1. Execute PE7-RWE-V2-VIABILITY-PREFLIGHT-1.
"""
        failures = checker.active_state_failures(next_text, next_text)
        self.assertTrue(
            any("window projection" in failure for failure in failures), failures
        )

    def test_forward_order_window_match_is_accepted(self) -> None:
        checker = load_handoff_checker()
        next_text = """## Authoritative Forward Order

```text
[window: viability preflight — READY_FOR_EXECUTION, provider-free S1–S5 only]
→ separately authorized viability run
```

## Packet PE7-RWE-V2-VIABILITY-PREFLIGHT-1
**State:** `READY_FOR_EXECUTION`
## Active Routing
1. Execute PE7-RWE-V2-VIABILITY-PREFLIGHT-1.
"""
        self.assertEqual(checker.active_state_failures(next_text, next_text), [])

    def test_future_route_requires_promotion_profile_contract_section(self) -> None:
        checker = load_handoff_checker()
        future = """# Future Route
## Worker Tiers
## Known Planned-Seam Gaps
## Stop and Resume Protocol
"""

        failures = checker.future_route_profile_failures(future)

        self.assertIn(
            "FUTURE_ROUTE is missing future-route section '## Promotion Profile Contract'",
            failures,
        )

    def test_future_route_requires_planned_seam_gap_registry(self) -> None:
        checker = load_handoff_checker()
        future = """# Future Route
## Worker Tiers
## Promotion Profile Contract
## Stop and Resume Protocol
## Portfolio Inventory Manifest
"""

        failures = checker.future_route_profile_failures(future)

        self.assertIn(
            "FUTURE_ROUTE is missing future-route section '## Known Planned-Seam Gaps'",
            failures,
        )

    def test_future_route_rejects_placeholder_prerequisite_and_class(self) -> None:
        checker = load_handoff_checker()
        future = """# Future Route
## Worker Tiers
## Known Planned-Seam Gaps
## Promotion Profile Contract
## Stop and Resume Protocol
## Portfolio Inventory Manifest
### Packet PE7-B-1
**State:** `BLOCKED_PREREQUISITE`
**Prerequisite:** TBD
**Class:** `UNKNOWN`
"""
        failures = checker.future_route_profile_failures(future)

        self.assertTrue(any("placeholder Prerequisite" in item for item in failures), failures)
        self.assertTrue(
            any("has unsupported Class" in item for item in failures),
            failures,
        )

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
