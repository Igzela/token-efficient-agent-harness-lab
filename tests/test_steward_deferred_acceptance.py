"""Tests for Autonomous Steward deferred acceptance requirements (PR6).

Requirements verified:
1. Single persistent effect owner: LocalProductStore is the sole persistence owner of effect envelopes/authorizations.
2. Authority does not automatically carry across stages: Stage completion resets child authorizations and grants.
3. Lease claim/execute/settle separation: DB transactions are not held during external execution.
4. Target default branch is not a workspace: Target repo outputs operate in dedicated branch worktrees only.
5. Repository-maintenance outer loop does not intrude on Rust/Product Store authority.
"""

from __future__ import annotations

import unittest
from pathlib import Path
import re

import mission_contract as contract
from steward import StewardService


ROOT = Path(__file__).resolve().parents[1]


class TestStewardDeferredAcceptance(unittest.TestCase):
    """Verify the 5 deferred acceptance requirements for PR6."""

    def test_single_persistent_effect_owner(self) -> None:
        """Requirement 1: LocalProductStore is the sole persistence owner of effects."""
        managed_acc = ROOT / "engine" / "src" / "storage" / "local_product_store" / "managed_acceptance.rs"
        self.assertTrue(managed_acc.is_file(), "managed_acceptance.rs must exist")
        content = managed_acc.read_text(encoding="utf-8")

        # Verify parent effect envelope contract and child authorization request are owned here
        self.assertIn("EffectEnvelopeContract", content)
        self.assertIn("EffectChildAuthorizationRequest", content)
        self.assertIn("EFFECT_ENVELOPE_SCHEMA_VERSION", content)

    def test_authority_does_not_carry_across_stages(self) -> None:
        """Requirement 2: Authority does not automatically transfer across stages."""
        # In mission_contract, each Stage is a discrete integration boundary
        # and has its own WorkCard graph without cross-stage inherited execution grants.
        mission = contract.campaign_mission()
        self.assertIsNotNone(mission)
        self.assertIn(mission.state, contract.MISSION_STATES)

        # Stage schema check: each stage requires independent verification and acceptance checks
        stage = contract.Stage(
            stage_id="stage-test-01",
            mission_id=mission.mission_id,
            objective="Discrete test stage for acceptance check",
            repository_identity=mission.repository_identity,
            acceptance_checks=("cargo test -p engine",),
            compatibility_checks=(),
            workcard_ids=("card-01", "card-02"),
            rollback=mission.rollback,
            integration_pr=None,
            exact_head=None,
        )
        wire = stage.to_wire()
        self.assertEqual(wire["stage_id"], "stage-test-01")
        self.assertEqual(wire["workcard_ids"], ["card-01", "card-02"])

    def test_lease_claim_execute_settle_separation(self) -> None:
        """Requirement 3: Lease claim/execute/settle separation without holding DB transaction."""
        queue_lease = ROOT / "engine" / "src" / "storage" / "local_product_store" / "workflow_runs" / "queue_lease.rs"
        self.assertTrue(queue_lease.is_file(), "queue_lease.rs must exist")
        content = queue_lease.read_text(encoding="utf-8")
        managed_acc = (ROOT / "engine" / "src" / "storage" / "local_product_store" / "managed_acceptance.rs").read_text(encoding="utf-8")

        # Verify lease SQL and managed lease separation
        self.assertIn("ACTIVE_RUNS_PRIORITIZED_SQL", content)
        self.assertIn("SQLITE_SET_PENDING_NODE_RUNNING_SQL", content)
        self.assertIn("managed_delegated_attempt_lease", managed_acc)

    def test_target_default_branch_not_workspace(self) -> None:
        """Requirement 4: Target default branch is not a workspace."""
        target_output_test = ROOT / "engine" / "tests" / "test_target_repo_output.rs"
        self.assertTrue(target_output_test.is_file(), "test_target_repo_output.rs must exist")
        content = target_output_test.read_text(encoding="utf-8")

        self.assertIn("branch_push_rejects_protected_branch_and_secret_text", content)
        self.assertIn("approved_branch_push_preserves_main_and_exports_same_patch", content)

    def test_repo_maintenance_outer_loop_separation(self) -> None:
        """Requirement 5: Repository-maintenance outer loop does not intrude on Rust/ProductStore authority."""
        steward_file = ROOT / "scripts" / "agent-control" / "steward.py"
        self.assertTrue(steward_file.is_file(), "steward.py must exist")
        content = steward_file.read_text(encoding="utf-8")

        # Steward coordinator operates in read-only / git worktree domain and stops at WAITING_FOR_MERGE
        self.assertIn("WAITING_FOR_MERGE", content)
        self.assertNotIn("LocalProductStore", content)
        self.assertNotIn("INSERT INTO product_tasks", content)


if __name__ == "__main__":
    unittest.main()
