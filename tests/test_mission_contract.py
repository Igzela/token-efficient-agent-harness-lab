"""Provider-free positive and negative tests for the PR1 contract boundary."""

from __future__ import annotations

from dataclasses import replace
import json
from pathlib import Path
import sys
import unittest


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts" / "agent-control"))

import mission_contract as contract  # noqa: E402


class MissionContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.mission = contract.campaign_mission()

    def current_identity_kwargs(self):
        identity = self.mission.repository_identity
        return {
            "repository": identity.repository,
            "base_sha": identity.base_sha,
            "branch": identity.branch,
            "source_ref": identity.source_ref,
            "source_sha256": identity.source_sha256,
        }

    def test_campaign_is_one_immutable_round_trippable_mission(self):
        self.assertTrue(self.mission.__dataclass_params__.frozen)
        wire = self.mission.to_wire()
        self.assertEqual(
            contract.MaintenanceMission.from_wire(wire).to_wire(),
            wire,
        )
        self.assertEqual(
            self.mission.proposal_sha256,
            self.mission.owner_approval.proposal_sha256,
        )
        self.assertEqual(self.mission.budget.max_external_effects, 0)

    def test_proposal_digest_and_owner_approval_are_bound(self):
        wire = self.mission.to_wire()
        wire["objective"] = "Changed after owner approval."
        with self.assertRaisesRegex(
            contract.MissionContractError, "mission_proposal_digest_mismatch"
        ):
            contract.MaintenanceMission.from_wire(wire)

        approval_forged = self.mission.to_wire()
        approval_forged["owner_approval"]["proposal_sha256"] = "f" * 64
        with self.assertRaisesRegex(
            contract.MissionContractError, "owner_approval_digest_mismatch"
        ):
            contract.MaintenanceMission.from_wire(approval_forged)

    def test_unknown_fields_and_non_finite_json_fail_closed(self):
        wire = self.mission.to_wire()
        wire["unexpected_authority"] = True
        with self.assertRaisesRegex(contract.MissionContractError, "mission_fields_invalid"):
            contract.MaintenanceMission.from_wire(wire)
        with self.assertRaisesRegex(contract.MissionContractError, "canonical_json_invalid"):
            contract.json_sha256({"value": float("nan")})

    def test_stale_repository_and_base_identity_are_rejected(self):
        with self.assertRaisesRegex(contract.MissionContractError, "mission_repository_stale"):
            contract.validate_current_mission(
                self.mission,
                repository="other/repository",
                base_sha=self.mission.repository_identity.base_sha,
                **{
                    key: value
                    for key, value in self.current_identity_kwargs().items()
                    if key not in {"repository", "base_sha"}
                },
            )
        with self.assertRaisesRegex(contract.MissionContractError, "mission_base_sha_stale"):
            contract.validate_current_mission(
                self.mission,
                repository=self.mission.repository_identity.repository,
                base_sha="a" * 40,
                **{
                    key: value
                    for key, value in self.current_identity_kwargs().items()
                    if key not in {"repository", "base_sha"}
                },
            )

        for field, expected in (
            ("branch", "mission_branch_stale"),
            ("source_ref", "mission_source_ref_stale"),
            ("source_sha256", "mission_source_stale"),
        ):
            with self.subTest(field=field):
                current = self.current_identity_kwargs()
                current[field] = "other-identity" if field != "source_sha256" else "b" * 64
                with self.assertRaisesRegex(contract.MissionContractError, expected):
                    contract.validate_current_mission(self.mission, **current)

        self.assertEqual(
            contract.validate_current_mission(
                self.mission, **self.current_identity_kwargs()
            ),
            self.mission,
        )

        attacker = replace(
            self.mission,
            owner_approval=replace(
                self.mission.owner_approval, owner_identity="attacker"
            ),
        )
        with self.assertRaisesRegex(
            contract.MissionContractError, "owner_approval_identity_untrusted"
        ):
            contract.validate_current_mission(attacker, **self.current_identity_kwargs())
        attacker_wire = attacker.to_wire()
        with self.assertRaisesRegex(
            contract.MissionContractError, "owner_approval_identity_untrusted"
        ):
            contract.MaintenanceMission.from_wire(attacker_wire)

    def test_budget_grants_and_scope_cannot_widen_mission(self):
        wire = self.mission.to_wire()
        wire["budget"]["max_external_effects"] = 1
        wire["proposal_sha256"] = self.mission.proposal_sha256
        with self.assertRaisesRegex(contract.MissionContractError, "max_external_effects"):
            contract.MaintenanceMission.from_wire(wire)

        grant = self.mission.standing_grants[0]
        widened = replace(grant, allowed_paths=("src/",))
        forged_mission = replace(self.mission, standing_grants=(widened,))
        forged = forged_mission.to_wire()
        forged["proposal_sha256"] = forged_mission.computed_proposal_sha256
        forged["owner_approval"]["proposal_sha256"] = forged["proposal_sha256"]
        with self.assertRaisesRegex(contract.MissionContractError, "grant_scope_widens_mission"):
            contract.MaintenanceMission.from_wire(forged)

        bad_grant = grant.to_wire()
        bad_grant["grant_type"] = "provider"
        with self.assertRaisesRegex(contract.MissionContractError, "grant_type_forbidden"):
            contract.Grant.from_wire(bad_grant)

        read_only = grant.to_wire()
        read_only["grant_type"] = "read_only"
        read_only["allowed_operations"] = ["read", "write"]
        with self.assertRaisesRegex(
            contract.MissionContractError, "read_only_grant_writes"
        ):
            contract.Grant.from_wire(read_only)

        changed_types = self.mission.to_wire()
        changed_types["allowed_change_types"] = ["provider"]
        with self.assertRaisesRegex(
            contract.MissionContractError, "change_type_forbidden"
        ):
            contract.MaintenanceMission.from_wire(changed_types)

        sensitive = grant.to_wire()
        sensitive["allowed_paths"] = ["secrets/"]
        with self.assertRaisesRegex(
            contract.MissionContractError, "grant_allowed_paths_sensitive"
        ):
            contract.Grant.from_wire(sensitive)

    def test_paths_stops_and_rollbacks_are_bounded(self):
        with self.assertRaisesRegex(contract.MissionContractError, "candidate_path_invalid"):
            contract.path_in_scope(("scripts/",), "../secret")
        self.assertEqual(
            contract.stop_category("TEST_FAILED"), "ROUTINE_RECOVERY"
        )
        self.assertEqual(
            contract.stop_category("EXTERNAL_OUTCOME_UNKNOWN"), "PAUSED_FOR_OWNER"
        )
        with self.assertRaisesRegex(contract.MissionContractError, "stop_code_unknown"):
            contract.stop_category("GUESS_AND_CONTINUE")
        rollback = self.mission.rollback.to_wire()
        rollback["strategy"] = "shell_anything"
        with self.assertRaisesRegex(contract.MissionContractError, "rollback_strategy_invalid"):
            contract.RollbackBoundary.from_wire(rollback)
        rollback = self.mission.rollback.to_wire()
        rollback["reference"] = "arbitrary-not-an-accepted-sha"
        with self.assertRaisesRegex(
            contract.MissionContractError, "rollback_reference_invalid"
        ):
            contract.RollbackBoundary.from_wire(rollback)

    def test_current_validation_rejects_a_rehashed_unregistered_mission(self):
        widened_grant = replace(
            self.mission.standing_grants[0], allowed_paths=("engine/",)
        )
        forged = replace(
            self.mission,
            allowed_paths=("engine/",),
            standing_grants=(widened_grant,),
        )
        forged = replace(
            forged,
            proposal_sha256=forged.computed_proposal_sha256,
            owner_approval=replace(
                forged.owner_approval,
                proposal_sha256=forged.computed_proposal_sha256,
            ),
        )
        with self.assertRaisesRegex(
            contract.MissionContractError, "mission_registration_invalid"
        ):
            contract.validate_current_mission(
                forged,
                repository=self.mission.repository_identity.repository,
                base_sha=self.mission.repository_identity.base_sha,
                branch=self.mission.repository_identity.branch,
                source_ref=self.mission.repository_identity.source_ref,
                source_sha256=self.mission.repository_identity.source_sha256,
            )

    def test_stage_and_workcard_bind_exact_identity_scope_and_attempt_budget(self):
        stage = contract.Stage(
            "STAGE-PR1",
            self.mission.mission_id,
            "Freeze the provider-free contract.",
            self.mission.repository_identity,
            ("Run focused contract tests.",),
            ("Preserve the legacy controller and exact packet compatibility.",),
            ("CARD-PR1-CONTRACT",),
            self.mission.rollback,
            None,
            None,
        )
        card = contract.WorkCard(
            "CARD-PR1-CONTRACT",
            stage.stage_id,
            ("scripts/agent-control/mission_contract.py", "tests/test_mission_contract.py"),
            ("engine/",),
            ("Implement the immutable wire models.",),
            ("Run the focused contract test suite.",),
            ("Reject digest, scope, and authority tampering.",),
            ("A provider-free test receipt.",),
            (),
            ("scripts/agent-control/mission_contract.py",),
            4,
            "T2",
            self.mission.rollback,
            "PENDING",
        )
        self.assertEqual(contract.validate_stage(stage, self.mission, (card,)), stage)
        self.assertEqual(contract.validate_workcard(card, stage, self.mission), card)

        with self.assertRaisesRegex(
            contract.MissionContractError, "stage_workcard_graph_incomplete"
        ):
            contract.validate_stage(stage, self.mission)

        integrated = replace(stage, integration_pr=628, exact_head="a" * 40)
        with self.assertRaisesRegex(
            contract.MissionContractError, "stage_exact_head_mismatch"
        ):
            contract.validate_stage(integrated, self.mission, (card,))
        self.assertEqual(
            contract.validate_stage(
                integrated,
                self.mission,
                (card,),
                observed_integration_pr=628,
                observed_exact_head="a" * 40,
            ),
            integrated,
        )

        stale = replace(stage, repository_identity=replace(stage.repository_identity, base_sha="b" * 40))
        with self.assertRaisesRegex(contract.MissionContractError, "stage_repository_identity_invalid"):
            contract.validate_stage(stale, self.mission)
        too_many = replace(card, max_attempts=self.mission.budget.max_attempts + 1)
        with self.assertRaisesRegex(contract.MissionContractError, "workcard_budget_exceeded"):
            contract.validate_workcard(too_many, stage, self.mission)

        widened_stage = replace(
            stage,
            rollback=replace(
                stage.rollback,
                strategy="document_restore",
                reference="document:docs/ARCHITECTURE_BOOK.md:" + "a" * 64,
            ),
        )
        with self.assertRaisesRegex(
            contract.MissionContractError, "stage_rollback_widens_mission"
        ):
            contract.validate_stage(widened_stage, self.mission)

        widened_card = replace(
            card,
            rollback=replace(
                card.rollback,
                strategy="document_restore",
                reference="document:docs/ARCHITECTURE_BOOK.md:" + "a" * 64,
            ),
        )
        with self.assertRaisesRegex(
            contract.MissionContractError, "workcard_rollback_widens_stage"
        ):
            contract.validate_workcard(widened_card, stage, self.mission)

        orphan = replace(card, card_id="CARD-ORPHAN")
        with self.assertRaisesRegex(
            contract.MissionContractError, "workcard_not_in_stage"
        ):
            contract.validate_workcard(orphan, stage, self.mission)

        unknown_dependency = replace(card, dependencies=("CARD-MISSING",))
        with self.assertRaisesRegex(
            contract.MissionContractError, "workcard_dependency_unknown"
        ):
            contract.validate_stage(stage, self.mission, (unknown_dependency,))

        tier_three = card.to_wire()
        tier_three["model_tier"] = "T3"
        with self.assertRaisesRegex(
            contract.MissionContractError, "model_tier_invalid"
        ):
            contract.WorkCard.from_wire(tier_three)

    def test_workcard_dependency_cycles_are_rejected(self):
        stage = contract.Stage(
            "STAGE-PR1-GRAPH",
            self.mission.mission_id,
            "Validate the work-card graph.",
            self.mission.repository_identity,
            ("Run graph checks.",),
            ("Keep all dependencies in the stage.",),
            ("CARD-A", "CARD-B"),
            self.mission.rollback,
            None,
            None,
        )

        def card(card_id, dependencies):
            return contract.WorkCard(
                card_id,
                stage.stage_id,
                ("scripts/agent-control/mission_contract.py",),
                (),
                ("Run one graph step.",),
                ("Run one focused test.",),
                ("Reject a cycle.",),
                ("Graph evidence.",),
                dependencies,
                (),
                1,
                "T2",
                self.mission.rollback,
                "PENDING",
            )

        with self.assertRaisesRegex(
            contract.MissionContractError, "workcard_dependency_cycle"
        ):
            contract.validate_stage(
                stage,
                self.mission,
                (card("CARD-A", ("CARD-B",)), card("CARD-B", ("CARD-A",))),
            )

    def test_workcard_rejects_forbidden_overlap_and_unknown_result(self):
        base = {
            "schema_version": contract.WORKCARD_SCHEMA_VERSION,
            "card_id": "CARD-PR1-CONTRACT",
            "stage_id": "STAGE-PR1",
            "allowed_paths": ["scripts/"],
            "forbidden_paths": ["scripts/secret.py"],
            "steps": ["Do one bounded step."],
            "focused_tests": ["Run the focused test."],
            "negative_checks": ["Run the negative test."],
            "expected_evidence": ["A concrete evidence receipt."],
            "dependencies": [],
            "path_locks": [],
            "max_attempts": 1,
            "model_tier": "T2",
            "rollback": self.mission.rollback.to_wire(),
            "result_state": "PENDING",
        }
        with self.assertRaisesRegex(contract.MissionContractError, "forbidden_path_overlaps"):
            contract.WorkCard.from_wire(base)
        base["forbidden_paths"] = []
        base["result_state"] = "INVENTED"
        with self.assertRaisesRegex(contract.MissionContractError, "result_state_invalid"):
            contract.WorkCard.from_wire(base)


class LegacyCompatibilityTests(unittest.TestCase):
    def packet(self, **overrides):
        value = {
            "packet_id": "PE7-AUTONOMOUS-STEWARD-PR1",
            "state": "READY_FOR_EXECUTION",
            "source_path": "docs/NEXT_DECISION.md",
            "packet_sha256": "a" * 64,
            "allowed_paths": ["scripts/", "tests/"],
            "forbidden_next_actions": ["Do not start a successor packet."],
            "execution_authorized": True,
            "checkpoint_allowed": True,
            "dispatch_lane": "provider_free_repository_maintenance",
        }
        value.update(overrides)
        return value

    def capsule(self, **overrides):
        value = {
            "schema_version": "weak_agent_dispatch.v1",
            "packet_id": "PE7-AUTONOMOUS-STEWARD-PR1",
            "dispatch_lane": "provider_free_repository_maintenance",
            "external_effect_limit": 0,
            "authority_consumption_allowed": False,
            "secret_values_allowed": False,
            "private_paths_allowed": False,
            "allowed_paths": ["scripts/", "tests/"],
            "forbidden_next_actions": ["Do not start a successor packet."],
        }
        value.update(overrides)
        return value

    def test_projection_is_immutable_and_cannot_be_used_as_a_writer(self):
        projection = contract.validate_legacy_compatibility(self.packet(), self.capsule())
        self.assertTrue(projection.__dataclass_params__.frozen)
        self.assertFalse(projection.writes_lifecycle)
        self.assertFalse(projection.execution_authorized)
        self.assertFalse(projection.authority_consumption_allowed)
        self.assertEqual(projection.external_effect_limit, 0)
        self.assertEqual(projection.lifecycle_writer, contract.LEGACY_LIFECYCLE_WRITER)
        self.assertEqual(projection.mission_id, "legacy-packet:PE7-AUTONOMOUS-STEWARD-PR1")

    def test_projection_rejects_registered_scope_and_output_or_store_widening(self):
        with self.assertRaisesRegex(
            contract.MissionContractError, "legacy_scope_widens_safe_surface"
        ):
            contract.validate_legacy_compatibility(
                self.packet(allowed_paths=["src/"]),
                self.capsule(allowed_paths=["src/"]),
            )
        with self.assertRaisesRegex(
            contract.MissionContractError, "legacy_store_mutation_granted"
        ):
            contract.validate_legacy_compatibility(
                self.packet(), self.capsule(known_store_mutations=["write"])
            )
        with self.assertRaisesRegex(
            contract.MissionContractError, "legacy_output_surface_forbidden"
        ):
            contract.validate_legacy_compatibility(
                self.packet(), self.capsule(allowed_outputs=["Provider secret"])
            )

    def test_projection_rejects_effect_scope_or_sensitive_authority(self):
        for overrides, expected in (
            ({"external_effect_limit": 1}, "external_effect_granted"),
            ({"authority_consumption_allowed": True}, "authority_consumption_granted"),
            ({"private_paths_allowed": True}, "sensitive_surface_granted"),
            ({"allowed_paths": ["engine/"]}, "dispatch_scope_mismatch"),
            ({"packet_id": "PE7-AUTONOMOUS-STEWARD-PR2"}, "dispatch_binding_invalid"),
            ({"untrusted_writer": True}, "legacy_capsule_fields_invalid"),
        ):
            with self.subTest(expected=expected):
                with self.assertRaisesRegex(contract.MissionContractError, expected):
                    contract.validate_legacy_compatibility(self.packet(), self.capsule(**overrides))


if __name__ == "__main__":
    unittest.main()
