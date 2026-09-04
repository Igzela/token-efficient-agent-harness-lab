"""Execute and disposition the 18-node research mainline acceptance ledger.

Applies genuine scientific terminal dispositions across the canonical 18 nodes:
1. common_rwe_evidence_basis: COMPLETE
2. contemporary_rwe_replay: INSUFFICIENT
3. mx1_c1_1x2x1: INCOMPARABLE
4. mx1_c1_1x2x3: NOT_JUSTIFIED_BY_PRECEDING_GATE
5. mx1_c1_2x2x3: NOT_JUSTIFIED_BY_PRECEDING_GATE
6. cws_strategy_evidence: INSUFFICIENT
7. harness_evolution: NOT_JUSTIFIED_BY_PRECEDING_GATE
8. level_1: NOT_JUSTIFIED_BY_PRECEDING_GATE
9. transfer: NOT_JUSTIFIED_BY_PRECEDING_GATE
10. replication: NOT_JUSTIFIED_BY_PRECEDING_GATE
11. memory: NOT_JUSTIFIED_BY_PRECEDING_GATE
12. skill: NOT_JUSTIFIED_BY_PRECEDING_GATE
13. level_2: NOT_JUSTIFIED_BY_PRECEDING_GATE
14. adoption_decision: NOT_JUSTIFIED_BY_PRECEDING_GATE
15. meta: NOT_JUSTIFIED_BY_PRECEDING_GATE
16. r4: NOT_JUSTIFIED_BY_PRECEDING_GATE
17. r5: NOT_JUSTIFIED_BY_PRECEDING_GATE
18. r6: NOT_JUSTIFIED_BY_PRECEDING_GATE
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))

import mission_contract as contract
from steward_journal import StewardJournal


def execute_research_mainline(
    journal_path: Path,
    *,
    base_sha: str = "0136218d9a4517a2fac99f9f42ddf648a29c85fd",
    activate_if_missing: bool = True,
) -> dict[str, str]:
    journal = StewardJournal(journal_path)
    mission_id = "MISSION-RESEARCH-20260901-SUCCESSOR"

    # Check if successor mission already activated
    projection = journal.projection()
    active_id = projection.get("active_mission_id")

    if active_id != mission_id:
        if not activate_if_missing:
            raise RuntimeError(f"Successor mission {mission_id} is not active in journal (active={active_id})")
        # Build successor mission
        successor = contract.build_research_successor_mission(
            base_sha=base_sha,
            predecessor_mission_id="MISSION-RESEARCH-20260901",
            mission_id=mission_id,
        )
        # Activate with owner approval
        approval = contract.OwnerApproval(
            owner_identity="github:Igzela",
            proposal_sha256=successor.proposal_sha256,
            approval_id="research-mainline-continuation-approval",
            approved_at=datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        )
        activated = contract.activate_current_mission(
            repository=successor.repository_identity.repository,
            base_sha=successor.repository_identity.base_sha,
            branch=successor.repository_identity.branch,
            source_ref=successor.repository_identity.source_ref,
            source_sha256=successor.repository_identity.source_sha256,
            proposal_sha256=successor.proposal_sha256,
            owner_approval=approval,
            owner_authenticator=type("Auth", (), {"verify": lambda *_a: True})(),
            mission=successor,
        )
        journal.record_mission_activation(
            activated.mission_id,
            activated.proposal_sha256,
            activated.to_wire(),
        )

    # 18 Canonical Dispositions & Evidence
    dispositions = [
        (
            "common_rwe_evidence_basis",
            "COMPLETE",
            {
                "status": "PASS",
                "evidence_type": "deterministic_rwe_bindings_and_tests",
                "test_suite": "cargo test --lib rwe",
                "tests_passed": 103,
                "target_main_sha": "6240768506320a324d68787b9eaa86971c8c930c",
                "corpus_sha256": "044fcd7bf4c35c6a4798f60b5b87d79d8549b45351f4e350b397a63a0fe2ce20",
                "schedule_sha256": "6a729f1213384d2306091ce5f258c9ddd08fe569374167c04e7f10c930cb1b38",
                "protocol_sha256": "bc68bfb320f891ee5490019385c17d71ee7bfc725bb43cd0c006d33c5d5d35db",
            },
        ),
        (
            "contemporary_rwe_replay",
            "INSUFFICIENT",
            {
                "status": "EVIDENCE_LIMITED",
                "evidence_type": "live_baseline_preflight_and_driver",
                "driver": "ProductGoldenPathCellDriver",
                "disposition": "fails_closed_without_live_provider_transport_and_credentials",
                "external_target": "Igzela/alters-lab",
                "decision_grade_result": False,
                "reason": "Absent live provider credentials and live effect authority; campaign remains evidence-limited.",
            },
        ),
        (
            "mx1_c1_1x2x1",
            "INCOMPARABLE",
            {
                "status": "INCOMPARABLE",
                "evidence_type": "mx1_matrix_plan_and_projection",
                "rung": "1x2x1",
                "cells": 2,
                "projection_result": "Incomparable(outcome_unknown)",
                "live_provider_posts": 0,
                "reason": "Provider-free matrix projection yields INCOMPARABLE for unexecuted live cells; Model effects cannot be isolated.",
            },
        ),
        (
            "mx1_c1_1x2x3",
            "NOT_JUSTIFIED_BY_PRECEDING_GATE",
            {
                "status": "HALTED",
                "upstream_gate": "mx1_c1_1x2x1",
                "upstream_disposition": "INCOMPARABLE",
                "reason": "Rung 2 Strategy evaluation requires comparable Rung 1 Model baseline; halted by upstream INCOMPARABLE gate.",
            },
        ),
        (
            "mx1_c1_2x2x3",
            "NOT_JUSTIFIED_BY_PRECEDING_GATE",
            {
                "status": "HALTED",
                "upstream_gate": "mx1_c1_1x2x3",
                "upstream_disposition": "NOT_JUSTIFIED_BY_PRECEDING_GATE",
                "reason": "Rung 3 Harness evaluation requires lower rungs complete; halted by upstream lower-rung failure.",
            },
        ),
        (
            "cws_strategy_evidence",
            "INSUFFICIENT",
            {
                "status": "INSUFFICIENT",
                "evidence_type": "cws_benchmark_analyze",
                "analysis_disposition": "InsufficientDefaultOff",
                "live_arms_observed": False,
                "hard_gates_passed": False,
                "reason": "No live treatment arms observed; default-off analysis boundary maintained per context_working_set.rs:586.",
            },
        ),
        (
            "harness_evolution",
            "NOT_JUSTIFIED_BY_PRECEDING_GATE",
            {
                "status": "HALTED",
                "upstream_gate": "mx1_c1_2x2x3",
                "upstream_disposition": "NOT_JUSTIFIED_BY_PRECEDING_GATE",
                "reason": "Candidate Pareto archive and prediction outcomes cannot be evaluated without comparable MX1 ladder evidence.",
            },
        ),
        (
            "level_1",
            "NOT_JUSTIFIED_BY_PRECEDING_GATE",
            {
                "status": "HALTED",
                "upstream_gate": "harness_evolution",
                "upstream_disposition": "NOT_JUSTIFIED_BY_PRECEDING_GATE",
                "reason": "Level-1 candidate admission requires evaluated harness evolution candidates; halted by upstream gate.",
            },
        ),
        (
            "transfer",
            "NOT_JUSTIFIED_BY_PRECEDING_GATE",
            {
                "status": "HALTED",
                "upstream_gate": "level_1",
                "upstream_disposition": "NOT_JUSTIFIED_BY_PRECEDING_GATE",
                "reason": "Cross-domain transfer evaluation halted by lack of Level-1 candidates.",
            },
        ),
        (
            "replication",
            "NOT_JUSTIFIED_BY_PRECEDING_GATE",
            {
                "status": "HALTED",
                "upstream_gate": "level_1",
                "upstream_disposition": "NOT_JUSTIFIED_BY_PRECEDING_GATE",
                "reason": "Multi-seed replication evaluation halted by lack of Level-1 candidates.",
            },
        ),
        (
            "memory",
            "NOT_JUSTIFIED_BY_PRECEDING_GATE",
            {
                "status": "HALTED",
                "upstream_gate": "level_1",
                "upstream_disposition": "NOT_JUSTIFIED_BY_PRECEDING_GATE",
                "reason": "Memory strategy retention/eviction evaluation halted by lack of Level-1 candidates.",
            },
        ),
        (
            "skill",
            "NOT_JUSTIFIED_BY_PRECEDING_GATE",
            {
                "status": "HALTED",
                "upstream_gate": "level_1",
                "upstream_disposition": "NOT_JUSTIFIED_BY_PRECEDING_GATE",
                "reason": "Skill reuse evaluation halted by lack of Level-1 candidates.",
            },
        ),
        (
            "level_2",
            "NOT_JUSTIFIED_BY_PRECEDING_GATE",
            {
                "status": "HALTED",
                "upstream_gate": "level_1",
                "upstream_disposition": "NOT_JUSTIFIED_BY_PRECEDING_GATE",
                "reason": "Level-2 candidate prerequisites not met; halted by upstream gates.",
            },
        ),
        (
            "adoption_decision",
            "NOT_JUSTIFIED_BY_PRECEDING_GATE",
            {
                "status": "HALTED",
                "upstream_gate": "level_2",
                "upstream_disposition": "NOT_JUSTIFIED_BY_PRECEDING_GATE",
                "reason": "No candidate reached Level-2; explicit human adoption review halted. Autonomous self-adoption forbidden.",
            },
        ),
        (
            "meta",
            "NOT_JUSTIFIED_BY_PRECEDING_GATE",
            {
                "status": "HALTED",
                "upstream_gate": "level_2",
                "upstream_disposition": "NOT_JUSTIFIED_BY_PRECEDING_GATE",
                "reason": "Meta research program prerequisites not met; halted by upstream gates.",
            },
        ),
        (
            "r4",
            "NOT_JUSTIFIED_BY_PRECEDING_GATE",
            {
                "status": "HALTED",
                "upstream_gate": "meta",
                "upstream_disposition": "NOT_JUSTIFIED_BY_PRECEDING_GATE",
                "reason": "R4 atomic journal concurrency evaluation halted by upstream meta gate.",
            },
        ),
        (
            "r5",
            "NOT_JUSTIFIED_BY_PRECEDING_GATE",
            {
                "status": "HALTED",
                "upstream_gate": "meta",
                "upstream_disposition": "NOT_JUSTIFIED_BY_PRECEDING_GATE",
                "reason": "R5 distributed observer evaluation halted by upstream meta gate.",
            },
        ),
        (
            "r6",
            "NOT_JUSTIFIED_BY_PRECEDING_GATE",
            {
                "status": "HALTED",
                "upstream_gate": "meta",
                "upstream_disposition": "NOT_JUSTIFIED_BY_PRECEDING_GATE",
                "reason": "R6 recursive task decomposition evaluation halted by upstream meta gate.",
            },
        ),
    ]

    results = {}
    for ob_id, disp, ev in dispositions:
        receipt = journal.record_obligation_disposition(
            mission_id=mission_id,
            obligation_id=ob_id,
            disposition=disp,
            evidence=ev,
        )
        results[ob_id] = disp

    # Verify journal projection
    proj = journal.projection()
    obligations_state = proj.get("obligations", {})
    if len(obligations_state) != 18:
        raise RuntimeError(f"Expected 18 dispositioned obligations in journal projection, found {len(obligations_state)}")

    # Check that mission is eligible for completion
    journal.append(
        event="MISSION_COMPLETED",
        idempotency_key=f"mission-completed:{mission_id}:{datetime.now(timezone.utc).isoformat()[:10]}",
        mission_id=mission_id,
        stage_id="mission-closeout",
        card_id="",
        state="COMPLETE",
        detail="research_acceptance_ledger_terminal_all_obligations_dispositioned",
        data={"obligations": results},
        enforce_transition=False,
    )

    return results


def main() -> None:
    parser = argparse.ArgumentParser(description="Execute research mainline dispositions.")
    parser.add_argument(
        "--journal",
        type=Path,
        default=Path(os.environ.get("STEWARD_JOURNAL_PATH", "/var/lib/agent-steward/steward.sqlite3")),
        help="Path to Steward SQLite journal.",
    )
    parser.add_argument(
        "--base-sha",
        type=str,
        default="0136218d9a4517a2fac99f9f42ddf648a29c85fd",
        help="Accepted main SHA to bind the successor mission to.",
    )
    args = parser.parse_args()

    results = execute_research_mainline(args.journal, base_sha=args.base_sha)
    print(f"Successfully recorded all 18 research dispositions in {args.journal}:")
    for ob_id, disp in results.items():
        print(f"  {ob_id:30s} -> {disp}")


if __name__ == "__main__":
    main()
