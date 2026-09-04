"""Regression tests for Mission completion semantics and research acceptance ledgers.

Enforces all 12 canonical requirements:
1. Fake OwnerApproval cannot activate a production research successor.
2. Permissive/fake authenticator cannot enter production activation path.
3. Arbitrary terminal labels with arbitrary evidence dictionaries cannot terminalize a ledger.
4. LACK_OF_PROVIDER_EXECUTION cannot become INSUFFICIENT.
5. Zero live posts / unexecuted cells cannot become INCOMPARABLE.
6. Actual canonical evaluator output can produce INSUFFICIENT.
7. Actual canonical comparison result can produce INCOMPARABLE.
8. NOT_JUSTIFIED requires a validated scientifically terminal upstream gate.
9. Direct journal MISSION_COMPLETED injection cannot bypass eligibility.
10. Restart preserves validated provenance.
11. Ordinary maintenance Missions still complete normally.
12. A real research Mission stays RESEARCH_PENDING while evidence acquisition is operationally blocked.
"""

from __future__ import annotations

from dataclasses import replace
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
from typing import Any
import unittest
from unittest.mock import MagicMock, patch

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts" / "agent-control"))

import mission_contract as contract
from steward_github import FakeGitHubWriter
from steward_journal import JournalError, StewardJournal
import steward_service as service


class TestMissionCompletionSemantics(unittest.TestCase):
    def setUp(self) -> None:
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = Path(self.temp_dir.name)
        self.journal_path = self.root / "steward.sqlite3"
        self.journal = StewardJournal(self.journal_path)

        self.repo_dir = self.root / "repo"
        self.repo_dir.mkdir()
        subprocess.run(["git", "init", "-b", "main"], cwd=self.repo_dir, check=True, capture_output=True)
        subprocess.run(["git", "config", "user.name", "Test Steward"], cwd=self.repo_dir, check=True, capture_output=True)
        subprocess.run(["git", "config", "user.email", "steward@localhost.invalid"], cwd=self.repo_dir, check=True, capture_output=True)

        readme = self.repo_dir / "README.md"
        readme.write_text("# Test Repo\nInitial content.\n")
        subprocess.run(["git", "add", "README.md"], cwd=self.repo_dir, check=True, capture_output=True)
        subprocess.run(["git", "commit", "-m", "Initial commit"], cwd=self.repo_dir, check=True, capture_output=True)

        rev = subprocess.run(["git", "rev-parse", "HEAD"], cwd=self.repo_dir, check=True, capture_output=True, text=True)
        self.base_sha = rev.stdout.strip()

        self.github_writer = FakeGitHubWriter(initial_pr_number=601)
        self.github_writer.remote_main_sha = self.base_sha

    def tearDown(self) -> None:
        self.temp_dir.cleanup()

    def _make_receipt(
        self,
        obligation_id: str,
        *,
        classification: str = "ACCEPTED_STATIC_BASIS",
        outcome: str = "PASS",
        disposition: str = "COMPLETE",
        **extra: Any,
    ) -> dict[str, Any]:
        return contract.make_provenance_receipt(
            obligation_id=obligation_id,
            accepted_main_sha=self.base_sha,
            evidence_producer_identity="test_producer",
            evaluator_identity="test_evaluator",
            provenance_classification=classification,
            hard_gate_outcome=outcome,
            missingness=False,
            **extra,
        )

    def _make_mission_with_ledger(
        self,
        obligations: tuple[contract.AcceptanceObligation, ...],
        *,
        allowed_paths: tuple[str, ...] = ("README.md", "docs/ROADMAP.md"),
    ) -> contract.MaintenanceMission:
        ledger = contract.MissionAcceptanceLedger(obligations)
        ledger.validate()
        req = (
            "Test research mission with bounded acceptance obligations "
            f"covering paths {' '.join(allowed_paths)} with documentation and tests."
        )
        proposal, digest = contract.compile_proposal_mission(
            req,
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
            acceptance_ledger=ledger,
        )
        evidence = contract.OwnerApprovalEvidence(
            transport="github_issue_comment",
            repository="Igzela/token-efficient-agent-harness-lab",
            mission_id=proposal.mission_id,
            approval_id="fixture-approval",
            owner_identity="github:Igzela",
            proposal_sha256=digest,
            accepted_main_sha=self.base_sha,
            evidence_id="fixture-comment-1",
        )
        return contract.activate_current_mission(
            repository=proposal.repository_identity.repository,
            base_sha=proposal.repository_identity.base_sha,
            branch=proposal.repository_identity.branch,
            source_ref=proposal.repository_identity.source_ref,
            source_sha256=proposal.repository_identity.source_sha256,
            proposal_sha256=digest,
            owner_approval=contract.OwnerApproval(
                "github:Igzela", digest, "fixture-approval", "2026-09-04T00:00:00Z"
            ),
            owner_authenticator=contract.AuthenticatedOwnerApprovalValidator(evidence),
            mission=proposal,
        )

    def test_01_fake_owner_approval_cannot_activate_production_research_successor(self):
        """Requirement 1: fake OwnerApproval cannot activate a production research successor."""
        successor = contract.build_research_successor_mission(base_sha=self.base_sha)
        self.journal.record_mission_proposal(successor.mission_id, successor.proposal_sha256, successor.to_wire())

        srv = service.StewardService(
            journal=self.journal,
            github=self.github_writer,
            repo_path=self.repo_dir,
        )
        # Attempt to activate with synthetic unauthenticated approval (no valid Issue comment)
        with self.assertRaises((contract.MissionContractError, service.StewardServiceError)):
            srv.approve(
                successor,
                control_issue_number=208,
                approval_comment_id=999999,  # Non-existent comment
            )

    def test_02_permissive_fake_authenticator_cannot_enter_production_activation_path(self):
        """Requirement 2: permissive/fake authenticator cannot enter production activation path."""
        proposal = contract.build_research_successor_mission(base_sha=self.base_sha)
        approval = contract.OwnerApproval(
            "github:Igzela", proposal.proposal_sha256, "fake-approval", "2026-09-04T00:00:00Z"
        )
        permissive_auth = type("FakeAuth", (), {"verify": lambda *_a: True})()

        with self.assertRaises(contract.MissionContractError) as ctx:
            contract.validate_authenticated_owner_approval(
                approval, proposal.proposal_sha256, permissive_auth, reject_permissive=True
            )
        self.assertEqual(str(ctx.exception), "permissive_authenticator_rejected")

    def test_03_arbitrary_terminal_labels_with_arbitrary_evidence_cannot_terminalize_ledger(self):
        """Requirement 3: arbitrary terminal labels with arbitrary evidence dictionaries cannot terminalize a ledger."""
        ob = contract.AcceptanceObligation(
            obligation_id="rwe_test",
            description="Test obligation",
            category="basis",
            dependencies=(),
            required_paths=("README.md",),
        )
        ledger = contract.MissionAcceptanceLedger((ob,))

        # Arbitrary caller dictionary without required provenance receipt fields
        arbitrary_evidence = {"result": "pass", "random_key": 12345}
        with self.assertRaises(contract.MissionContractError) as ctx:
            ledger.disposition_obligation("rwe_test", "COMPLETE", arbitrary_evidence)
        self.assertIn("evidence_receipt", str(ctx.exception))

        # Even if an unvalidated obligation object was constructed, is_terminal returns False
        unvalidated_ob = replace(ob, disposition="COMPLETE", evidence=arbitrary_evidence)
        unvalidated_ledger = contract.MissionAcceptanceLedger((unvalidated_ob,))
        self.assertFalse(unvalidated_ledger.is_terminal())

    def test_04_lack_of_provider_execution_cannot_become_insufficient(self):
        """Requirement 4: LACK_OF_PROVIDER_EXECUTION cannot become INSUFFICIENT."""
        ob = contract.AcceptanceObligation(
            obligation_id="rwe_replay",
            description="Replay obligation",
            category="evaluation",
            dependencies=(),
            required_paths=("README.md",),
        )
        ledger = contract.MissionAcceptanceLedger((ob,))

        # Evidence with lack_of_provider_execution or absent credentials
        evidence = contract.make_provenance_receipt(
            obligation_id="rwe_replay",
            accepted_main_sha=self.base_sha,
            evidence_producer_identity="ProductGoldenPathCellDriver",
            evaluator_identity="rwe_evaluator",
            provenance_classification="EXECUTED_EVIDENCE",
            hard_gate_outcome="INSUFFICIENT",
            missingness=True,
            execution_identity="run-1",
            lack_of_provider_execution=True,
            credentials_absent=True,
            reason="Absent live provider credentials and live effect authority; campaign remains evidence-limited.",
        )
        with self.assertRaises(contract.MissionContractError) as ctx:
            ledger.disposition_obligation("rwe_replay", "INSUFFICIENT", evidence)
        self.assertEqual(str(ctx.exception), "lack_of_provider_execution_cannot_produce_insufficient")

    def test_05_zero_live_posts_unexecuted_cells_cannot_become_incomparable(self):
        """Requirement 5: zero live posts/unexecuted cells cannot become INCOMPARABLE."""
        ob = contract.AcceptanceObligation(
            obligation_id="mx1_cell",
            description="Matrix rung",
            category="ladder",
            dependencies=(),
            required_paths=("README.md",),
        )
        ledger = contract.MissionAcceptanceLedger((ob,))

        evidence = contract.make_provenance_receipt(
            obligation_id="mx1_cell",
            accepted_main_sha=self.base_sha,
            evidence_producer_identity="mx1_matrix_plan",
            evaluator_identity="mx1_evaluator",
            provenance_classification="EXECUTED_EVIDENCE",
            hard_gate_outcome="INCOMPARABLE",
            missingness=True,
            execution_identity="matrix-cell-1",
            live_provider_posts=0,
            unexecuted_cells=True,
            projection_result="Incomparable(outcome_unknown)",
            reason="Provider-free matrix projection yields INCOMPARABLE for unexecuted live cells; Model effects cannot be isolated.",
        )
        with self.assertRaises(contract.MissionContractError) as ctx:
            ledger.disposition_obligation("mx1_cell", "INCOMPARABLE", evidence)
        self.assertEqual(str(ctx.exception), "unexecuted_cells_cannot_produce_incomparable")

    def test_06_actual_canonical_evaluator_output_can_produce_insufficient(self):
        """Requirement 6: actual canonical evaluator output can produce INSUFFICIENT."""
        ob = contract.AcceptanceObligation(
            obligation_id="cws_strategy",
            description="CWS benchmark",
            category="evaluation",
            dependencies=(),
            required_paths=("README.md",),
        )
        ledger = contract.MissionAcceptanceLedger((ob,))

        # Genuine executed run with live posts and authorized execution, but evaluator determined INSUFFICIENT
        valid_evidence = contract.make_provenance_receipt(
            obligation_id="cws_strategy",
            accepted_main_sha=self.base_sha,
            evidence_producer_identity="cws_benchmark_analyze",
            evaluator_identity="context_working_set_evaluator",
            provenance_classification="EXECUTED_EVIDENCE",
            hard_gate_outcome="INSUFFICIENT",
            missingness=False,
            execution_identity="cws-bench-exec-42",
            attempt_identity="attempt-1",
            executed=True,
            provider_posts=8,
            attempted_with_authority=True,
            disposition="InsufficientDefaultOff",
        )
        updated_ledger = ledger.disposition_obligation("cws_strategy", "INSUFFICIENT", valid_evidence)
        self.assertEqual(updated_ledger.get("cws_strategy").disposition, "INSUFFICIENT")
        self.assertTrue(updated_ledger.is_terminal())

    def test_07_actual_canonical_comparison_result_can_produce_incomparable(self):
        """Requirement 7: actual canonical comparison result can produce INCOMPARABLE."""
        ob = contract.AcceptanceObligation(
            obligation_id="mx1_matrix",
            description="Matrix comparison",
            category="ladder",
            dependencies=(),
            required_paths=("README.md",),
        )
        ledger = contract.MissionAcceptanceLedger((ob,))

        # Genuine executed cells with live posts, but comparison contract determines incomparable
        valid_evidence = contract.make_provenance_receipt(
            obligation_id="mx1_matrix",
            accepted_main_sha=self.base_sha,
            evidence_producer_identity="mx1_matrix_comparator",
            evaluator_identity="matrix_evaluator",
            provenance_classification="EXECUTED_EVIDENCE",
            hard_gate_outcome="INCOMPARABLE",
            missingness=False,
            execution_identity="rung-1x2x1-run",
            executed_cells=2,
            live_provider_posts=6,
            attempted_with_authority=True,
            reason="Executed treatment arms exhibited divergent variance exceeding protocol bound.",
        )
        updated_ledger = ledger.disposition_obligation("mx1_matrix", "INCOMPARABLE", valid_evidence)
        self.assertEqual(updated_ledger.get("mx1_matrix").disposition, "INCOMPARABLE")
        self.assertTrue(updated_ledger.is_terminal())

    def test_08_not_justified_requires_validated_scientifically_terminal_upstream_gate(self):
        """Requirement 8: NOT_JUSTIFIED requires a validated scientifically terminal upstream gate."""
        ob_up = contract.AcceptanceObligation(
            obligation_id="rung1",
            description="Upstream rung",
            category="gate",
            dependencies=(),
            required_paths=("README.md",),
        )
        ob_down = contract.AcceptanceObligation(
            obligation_id="rung2",
            description="Downstream rung",
            category="gate",
            dependencies=("rung1",),
            required_paths=("README.md",),
        )
        ledger = contract.MissionAcceptanceLedger((ob_up, ob_down))

        # Fails when upstream is unresolved
        down_ev = contract.make_provenance_receipt(
            obligation_id="rung2",
            accepted_main_sha=self.base_sha,
            evidence_producer_identity="dependency_resolver",
            evaluator_identity="gate_evaluator",
            provenance_classification="DEPENDENCY_DERIVED",
            hard_gate_outcome="HALTED",
            missingness=False,
            upstream_gate="rung1",
        )
        with self.assertRaises(contract.MissionContractError) as ctx:
            ledger.disposition_obligation("rung2", "NOT_JUSTIFIED_BY_PRECEDING_GATE", down_ev)
        self.assertEqual(str(ctx.exception), "upstream_gate_not_halting")

        # Fails when upstream completed with COMPLETE (non-halting)
        up_ev = contract.make_provenance_receipt(
            obligation_id="rung1",
            accepted_main_sha=self.base_sha,
            evidence_producer_identity="test_runner",
            evaluator_identity="test_evaluator",
            provenance_classification="ACCEPTED_STATIC_BASIS",
            hard_gate_outcome="PASS",
            missingness=False,
        )
        ledger_comp = ledger.disposition_obligation("rung1", "COMPLETE", up_ev)
        with self.assertRaises(contract.MissionContractError) as ctx:
            ledger_comp.disposition_obligation("rung2", "NOT_JUSTIFIED_BY_PRECEDING_GATE", down_ev)
        self.assertEqual(str(ctx.exception), "upstream_gate_not_halting")

        # Upstream genuinely halting with validated provenance receipt (INSUFFICIENT)
        halt_ev = contract.make_provenance_receipt(
            obligation_id="rung1",
            accepted_main_sha=self.base_sha,
            evidence_producer_identity="test_eval",
            evaluator_identity="test_evaluator",
            provenance_classification="EXECUTED_EVIDENCE",
            hard_gate_outcome="INSUFFICIENT",
            missingness=False,
            execution_identity="run-99",
            executed=True,
            provider_posts=4,
            attempted_with_authority=True,
        )
        ledger_halt = ledger.disposition_obligation("rung1", "INSUFFICIENT", halt_ev)
        # Downstream succeeds with validated DEPENDENCY_DERIVED provenance
        ledger_halt2 = ledger_halt.disposition_obligation("rung2", "NOT_JUSTIFIED_BY_PRECEDING_GATE", down_ev)
        self.assertEqual(ledger_halt2.get("rung2").disposition, "NOT_JUSTIFIED_BY_PRECEDING_GATE")
        self.assertTrue(ledger_halt2.is_terminal())

    def test_09_direct_journal_mission_completed_injection_cannot_bypass_eligibility(self):
        """Requirement 9: direct journal MISSION_COMPLETED injection cannot bypass eligibility."""
        ob1 = contract.AcceptanceObligation(
            obligation_id="ob1",
            description="Obligation 1",
            category="gate",
            dependencies=(),
            required_paths=("README.md",),
        )
        mission = self._make_mission_with_ledger((ob1,), allowed_paths=("README.md",))
        self.journal.record_mission_activation(
            mission.mission_id, mission.proposal_sha256, mission.to_wire()
        )

        # Attempt to inject MISSION_COMPLETED while ob1 is unresolved
        with self.assertRaises(JournalError):
            self.journal.append(
                event="MISSION_COMPLETED",
                idempotency_key="injected-completion-key",
                mission_id=mission.mission_id,
                stage_id="mission-closeout",
                card_id="",
                state="COMPLETE",
                detail="fake_injection",
                data={"obligations": {}},
                enforce_transition=False,
            )

        with self.assertRaises(JournalError):
            self.journal.record_mission_completion(mission.mission_id)

    def test_10_restart_preserves_validated_provenance(self):
        """Requirement 10: restart preserves validated provenance."""
        ob1 = contract.AcceptanceObligation(
            obligation_id="node_prov",
            description="Node with provenance",
            category="eval",
            dependencies=(),
            required_paths=("README.md",),
        )
        mission = self._make_mission_with_ledger((ob1,), allowed_paths=("README.md",))
        self.journal.record_mission_activation(
            mission.mission_id, mission.proposal_sha256, mission.to_wire()
        )

        srv1 = service.StewardService(
            mission_id=mission.mission_id,
            journal=self.journal,
            github=self.github_writer,
            repo_path=self.repo_dir,
        )
        ev = self._make_receipt("node_prov", classification="ACCEPTED_STATIC_BASIS", outcome="PASS", disposition="COMPLETE")
        srv1.disposition_mission_obligation("node_prov", "COMPLETE", ev)

        # Restart service
        srv2 = service.StewardService(
            mission_id=mission.mission_id,
            journal=self.journal,
            github=self.github_writer,
            repo_path=self.repo_dir,
        )
        self.assertIsNotNone(srv2.mission)
        self.assertIsNotNone(srv2.mission.acceptance_ledger)
        node = srv2.mission.acceptance_ledger.get("node_prov")
        self.assertIsNotNone(node)
        self.assertEqual(node.disposition, "COMPLETE")
        self.assertEqual(node.evidence["provenance_classification"], "ACCEPTED_STATIC_BASIS")
        self.assertEqual(node.evidence["accepted_main_sha"], self.base_sha)
        self.assertEqual(node.evidence["hard_gate_outcome"], "PASS")

    def test_11_ordinary_maintenance_missions_still_complete_normally(self):
        """Requirement 11: ordinary maintenance Missions still complete normally."""
        mission = contract.campaign_mission()
        self.journal.record_mission_activation(
            mission.mission_id, mission.proposal_sha256, mission.to_wire()
        )
        srv = service.StewardService(
            mission_id=mission.mission_id,
            journal=self.journal,
            github=self.github_writer,
            repo_path=self.repo_dir,
        )
        self.assertIsNone(srv.mission.acceptance_ledger)

        res = srv.complete_mission_if_eligible()
        self.assertEqual(res["status"], "COMPLETE")
        self.assertEqual(srv.mission.state, "COMPLETE")

    def test_12_real_research_mission_stays_research_pending_while_operationally_blocked(self):
        """Requirement 12: a real research Mission stays RESEARCH_PENDING while evidence acquisition is operationally blocked."""
        proposal = contract.build_research_successor_mission(base_sha=self.base_sha)
        evidence = contract.OwnerApprovalEvidence(
            transport="github_issue_comment",
            repository="Igzela/token-efficient-agent-harness-lab",
            mission_id=proposal.mission_id,
            approval_id="fixture-approval-research",
            owner_identity="github:Igzela",
            proposal_sha256=proposal.proposal_sha256,
            accepted_main_sha=self.base_sha,
            evidence_id="fixture-comment-research-1",
        )
        mission = contract.activate_current_mission(
            repository=proposal.repository_identity.repository,
            base_sha=proposal.repository_identity.base_sha,
            branch=proposal.repository_identity.branch,
            source_ref=proposal.repository_identity.source_ref,
            source_sha256=proposal.repository_identity.source_sha256,
            proposal_sha256=proposal.proposal_sha256,
            owner_approval=contract.OwnerApproval(
                "github:Igzela", proposal.proposal_sha256, "fixture-approval-research", "2026-09-04T00:00:00Z"
            ),
            owner_authenticator=contract.AuthenticatedOwnerApprovalValidator(evidence),
            mission=proposal,
        )
        self.journal.record_mission_activation(
            mission.mission_id, mission.proposal_sha256, mission.to_wire()
        )
        srv = service.StewardService(
            mission_id=mission.mission_id,
            journal=self.journal,
            github=self.github_writer,
            repo_path=self.repo_dir,
        )

        # Disposition 1 node (common_rwe_evidence_basis) as COMPLETE
        ev1 = self._make_receipt("common_rwe_evidence_basis", classification="ACCEPTED_STATIC_BASIS", outcome="PASS", disposition="COMPLETE")
        srv.disposition_mission_obligation("common_rwe_evidence_basis", "COMPLETE", ev1)

        # Other 17 nodes remain unresolved because live provider execution is blocked
        res = srv.complete_mission_if_eligible()
        self.assertEqual(res["status"], "RESEARCH_PENDING")
        self.assertEqual(srv.mission.state, "RUNNING")

    def test_13_duplicate_evidence_disposition_is_idempotent(self):
        """Duplicate evidence/disposition is idempotent."""
        ob1 = contract.AcceptanceObligation(
            obligation_id="node_idempotent",
            description="Node Idempotent",
            category="eval",
            dependencies=(),
            required_paths=("README.md",),
        )
        mission = self._make_mission_with_ledger((ob1,), allowed_paths=("README.md",))
        self.journal.record_mission_activation(
            mission.mission_id, mission.proposal_sha256, mission.to_wire()
        )
        srv = service.StewardService(
            mission_id=mission.mission_id,
            journal=self.journal,
            github=self.github_writer,
            repo_path=self.repo_dir,
        )
        ev = self._make_receipt("node_idempotent")
        evt1 = srv.disposition_mission_obligation("node_idempotent", "COMPLETE", ev)
        evt2 = srv.disposition_mission_obligation("node_idempotent", "COMPLETE", ev)
        self.assertEqual(evt1.seq, evt2.seq)
        self.assertEqual(evt1.sha256, evt2.sha256)

    def test_14_contradictory_evidence_fails_closed(self):
        """Contradictory evidence fails closed."""
        ob1 = contract.AcceptanceObligation(
            obligation_id="node_contradict",
            description="Node Contradict",
            category="eval",
            dependencies=(),
            required_paths=("README.md",),
        )
        mission = self._make_mission_with_ledger((ob1,), allowed_paths=("README.md",))
        self.journal.record_mission_activation(
            mission.mission_id, mission.proposal_sha256, mission.to_wire()
        )
        srv = service.StewardService(
            mission_id=mission.mission_id,
            journal=self.journal,
            github=self.github_writer,
            repo_path=self.repo_dir,
        )
        ev1 = self._make_receipt("node_contradict", extra_key="1")
        srv.disposition_mission_obligation("node_contradict", "COMPLETE", ev1)

        # Different evidence fails
        ev2 = self._make_receipt("node_contradict", extra_key="2")
        with self.assertRaises(contract.MissionContractError) as ctx:
            srv.disposition_mission_obligation("node_contradict", "COMPLETE", ev2)
        self.assertEqual(str(ctx.exception), "contradictory_obligation_evidence")



class TestCorrectiveContinuationAuthority(unittest.TestCase):
    def setUp(self) -> None:
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = Path(self.temp_dir.name)
        self.journal_path = self.root / "steward.sqlite3"
        self.journal = StewardJournal(self.journal_path)

        self.repo_dir = self.root / "repo"
        self.repo_dir.mkdir()
        subprocess.run(["git", "init", "-b", "main"], cwd=self.repo_dir, check=True, capture_output=True)
        subprocess.run(["git", "config", "user.name", "Test Steward"], cwd=self.repo_dir, check=True, capture_output=True)
        subprocess.run(["git", "config", "user.email", "steward@localhost.invalid"], cwd=self.repo_dir, check=True, capture_output=True)

        readme = self.repo_dir / "README.md"
        readme.write_text("# Test Repo\nInitial content.\n")
        subprocess.run(["git", "add", "README.md"], cwd=self.repo_dir, check=True, capture_output=True)
        subprocess.run(["git", "commit", "-m", "Initial commit"], cwd=self.repo_dir, check=True, capture_output=True)

        rev = subprocess.run(["git", "rev-parse", "HEAD"], cwd=self.repo_dir, check=True, capture_output=True, text=True)
        self.base_sha = rev.stdout.strip()

        self.github_writer = FakeGitHubWriter(initial_pr_number=601)
        self.github_writer.remote_main_sha = self.base_sha

    def tearDown(self) -> None:
        self.temp_dir.cleanup()

    def _make_receipt(
        self,
        obligation_id: str,
        *,
        classification: str = "ACCEPTED_STATIC_BASIS",
        outcome: str = "PASS",
        disposition: str = "COMPLETE",
        **extra: Any,
    ) -> dict[str, Any]:
        return contract.make_provenance_receipt(
            obligation_id=obligation_id,
            accepted_main_sha=self.base_sha,
            evidence_producer_identity="test_producer",
            evaluator_identity="test_evaluator",
            provenance_classification=classification,
            hard_gate_outcome=outcome,
            missingness=False,
            **extra,
        )

    def _setup_authenticated_mission(
        self,
        mission_id: str = "MISSION-RESEARCH-20260901",
        allowed_paths: tuple[str, ...] = ("README.md", "docs/ROADMAP.md"),
    ) -> contract.MaintenanceMission:
        req = (
            "Complete bounded closed loop research mainline and obtain actual RWE MX1 C1 CWS "
            f"with documentation and tests covering {' '.join(allowed_paths)}."
        )
        proposal, digest = contract.compile_proposal_mission(
            req,
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
            mission_id=mission_id,
        )
        approval_id = f"{mission_id}-owner-approval"
        self.journal.consume_owner_approval(
            repository="Igzela/token-efficient-agent-harness-lab",
            mission_id=mission_id,
            approval_id=approval_id,
            proposal_sha256=digest,
            accepted_main_sha=self.base_sha,
        )
        evidence = contract.OwnerApprovalEvidence(
            transport="github_issue_comment",
            repository="Igzela/token-efficient-agent-harness-lab",
            mission_id=mission_id,
            approval_id=approval_id,
            owner_identity="github:Igzela",
            proposal_sha256=digest,
            accepted_main_sha=self.base_sha,
            evidence_id=f"comment-{mission_id}",
        )
        activated = contract.activate_current_mission(
            repository=proposal.repository_identity.repository,
            base_sha=proposal.repository_identity.base_sha,
            branch=proposal.repository_identity.branch,
            source_ref=proposal.repository_identity.source_ref,
            source_sha256=proposal.repository_identity.source_sha256,
            proposal_sha256=digest,
            owner_approval=contract.OwnerApproval(
                "github:Igzela", digest, approval_id, "2026-09-04T00:00:00Z"
            ),
            owner_authenticator=contract.AuthenticatedOwnerApprovalValidator(evidence),
            mission=proposal,
        )
        self.journal.record_mission_activation(
            activated.mission_id, activated.proposal_sha256, activated.to_wire()
        )
        return activated

    def test_01_exact_original_approved_mission_can_be_correctively_continued(self):
        """1. Exact original approved Mission can be correctively continued without another OWNER prompt."""
        orig = self._setup_authenticated_mission()
        continuation = contract.build_corrective_continuation(orig, base_sha=self.base_sha)
        evt = self.journal.record_corrective_continuation(
            orig.mission_id, continuation.proposal_sha256, continuation.to_wire()
        )
        self.assertEqual(evt.event, "MISSION_CORRECTIVE_CONTINUATION")
        active = self.journal.active_mission_record()
        self.assertIsNotNone(active)
        self.assertEqual(active.mission_id, orig.mission_id)
        self.assertEqual(active.data.get("proposal_sha256"), orig.proposal_sha256)

    def test_02_changed_proposal_digest_is_rejected(self):
        """2. Continuation with changed proposal digest is rejected."""
        orig = self._setup_authenticated_mission()
        continuation = contract.build_corrective_continuation(orig, base_sha=self.base_sha)
        tampered_wire = continuation.to_wire()
        tampered_wire["proposal_sha256"] = "0" * 64
        with self.assertRaises(JournalError) as ctx:
            self.journal.record_corrective_continuation(
                orig.mission_id, "0" * 64, tampered_wire
            )
        self.assertIn("continuation_wire_decode_failed", str(ctx.exception))

    def test_03_changed_objective_is_rejected(self):
        """3. Continuation with changed objective is rejected."""
        orig = self._setup_authenticated_mission()
        tampered = replace(orig, objective="Altered unauthorized objective")
        with self.assertRaises(contract.MissionContractError) as ctx:
            contract.validate_corrective_continuation(orig, tampered)
        self.assertEqual(str(ctx.exception), "continuation_objective_mismatch")

    def test_04_widened_paths_are_rejected(self):
        """4. Continuation with widened paths is rejected (allowed_paths and ledger required_paths)."""
        orig = self._setup_authenticated_mission(allowed_paths=("README.md",))
        # 4a: Widened mission allowed_paths
        tampered = replace(orig, allowed_paths=("README.md", "unauthorized/secret.py"))
        with self.assertRaises(contract.MissionContractError) as ctx:
            contract.validate_corrective_continuation(orig, tampered)
        self.assertIn("continuation_allowed_paths_widened", str(ctx.exception))

        # 4b: Ledger required_paths outside mission allowed_paths
        ob = contract.AcceptanceObligation(
            obligation_id="ob_outside",
            description="Obligation outside allowed paths",
            category="basis",
            dependencies=(),
            required_paths=("unauthorized/path.py",),
        )
        bad_ledger = contract.MissionAcceptanceLedger((ob,))
        with self.assertRaises(contract.MissionContractError) as ctx:
            contract.build_corrective_continuation(orig, acceptance_ledger=bad_ledger)
        self.assertIn("continuation_ledger_path_outside_allowed_paths", str(ctx.exception))

    def test_05_widened_grants_budgets_or_weakened_forbidden_changes_rejected(self):
        """5. Continuation with widened grants, budgets, effect ceilings, or weakened forbidden changes is rejected."""
        orig = self._setup_authenticated_mission()
        # Budget widened
        bad_budget = replace(orig.budget, max_calls=orig.budget.max_calls + 1000)
        with self.assertRaises(contract.MissionContractError) as ctx:
            contract.validate_corrective_continuation(orig, replace(orig, budget=bad_budget))
        self.assertEqual(str(ctx.exception), "continuation_budget_calls_widened")

        # Effect ceilings widened
        bad_effects = replace(orig.budget, max_external_effects=1)
        with self.assertRaises(contract.MissionContractError) as ctx:
            contract.validate_corrective_continuation(orig, replace(orig, budget=bad_effects))
        self.assertEqual(str(ctx.exception), "continuation_budget_external_effects_widened")

        # Standing grant operations widened
        bad_grant = replace(orig.standing_grants[0], allowed_operations=orig.standing_grants[0].allowed_operations + ("forbidden_op",))
        with self.assertRaises(contract.MissionContractError) as ctx:
            contract.validate_corrective_continuation(orig, replace(orig, standing_grants=(bad_grant,)))
        self.assertEqual(str(ctx.exception), "continuation_standing_grant_operations_widened")

        # Forbidden changes weakened
        with self.assertRaises(contract.MissionContractError) as ctx:
            contract.validate_corrective_continuation(orig, replace(orig, forbidden_changes=()))
        self.assertIn("continuation_forbidden_changes_weakened", str(ctx.exception))

    def test_06_different_repository_is_rejected(self):
        """6. Continuation pointing to a different repository is rejected."""
        orig = self._setup_authenticated_mission()
        bad_repo = replace(orig.repository_identity, repository="attacker/repo")
        with self.assertRaises(contract.MissionContractError) as ctx:
            contract.validate_corrective_continuation(orig, replace(orig, repository_identity=bad_repo))
        self.assertEqual(str(ctx.exception), "continuation_repository_mismatch")

    def test_07_synthetic_or_invalidated_successor_cannot_supply_authority(self):
        """7. Synthetic or invalidated successor cannot supply authority."""
        self.journal.record_closeout_invalidation(
            mission_id="MISSION-RESEARCH-20260901-SUCCESSOR",
            reason="audit invalidation",
            details={"test": True},
        )
        self.assertIsNone(self.journal.get_mission_activation("MISSION-RESEARCH-20260901-SUCCESSOR"))
        with self.assertRaises(JournalError) as ctx:
            self.journal.record_corrective_continuation(
                mission_id="MISSION-RESEARCH-20260901-SUCCESSOR",
                proposal_sha256="0" * 64,
                mission_data={"mission_id": "MISSION-RESEARCH-20260901-SUCCESSOR"},
            )
        self.assertIn("mission_invalidated_cannot_continue", str(ctx.exception))

    def test_08_missing_historical_authenticated_activation_fails_closed(self):
        """8. Missing historical authenticated activation fails closed."""
        # 8a: Never activated mission
        with self.assertRaises(JournalError) as ctx:
            self.journal.record_corrective_continuation(
                mission_id="MISSION-NEVER-ACTIVATED",
                proposal_sha256="0" * 64,
                mission_data={"mission_id": "MISSION-NEVER-ACTIVATED"},
            )
        self.assertIn("mission_activation_missing", str(ctx.exception))

        # 8b: Activated without consumed approval
        req = "Mission without consumed approval"
        proposal, digest = contract.compile_proposal_mission(
            req,
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
            mission_id="MISSION-NO-CONSUMED",
        )
        evidence = contract.OwnerApprovalEvidence(
            transport="github_issue_comment",
            repository="Igzela/token-efficient-agent-harness-lab",
            mission_id=proposal.mission_id,
            approval_id="fixture-approval-no-consumed",
            owner_identity="github:Igzela",
            proposal_sha256=digest,
            accepted_main_sha=self.base_sha,
            evidence_id="fixture-comment-no-consumed",
        )
        activated = contract.activate_current_mission(
            repository=proposal.repository_identity.repository,
            base_sha=proposal.repository_identity.base_sha,
            branch=proposal.repository_identity.branch,
            source_ref=proposal.repository_identity.source_ref,
            source_sha256=proposal.repository_identity.source_sha256,
            proposal_sha256=digest,
            owner_approval=contract.OwnerApproval(
                "github:Igzela", digest, "fixture-approval-no-consumed", "2026-09-04T00:00:00Z"
            ),
            owner_authenticator=contract.AuthenticatedOwnerApprovalValidator(evidence),
            mission=proposal,
        )
        self.journal.record_mission_activation(
            activated.mission_id, activated.proposal_sha256, activated.to_wire()
        )
        continuation = contract.build_corrective_continuation(activated, base_sha=self.base_sha)
        with self.assertRaises(JournalError) as ctx:
            self.journal.record_corrective_continuation(
                activated.mission_id, continuation.proposal_sha256, continuation.to_wire()
            )
        self.assertIn("historical_approval_not_consumed", str(ctx.exception))

    def test_09_accepted_main_advancement_preserves_authority_envelope(self):
        """9. Accepted-main advancement caused by the Mission itself does not require new OWNER approval."""
        orig = self._setup_authenticated_mission()
        # Advance git main
        new_readme = self.repo_dir / "README.md"
        new_readme.write_text("# Updated README\n")
        subprocess.run(["git", "add", "README.md"], cwd=self.repo_dir, check=True, capture_output=True)
        subprocess.run(["git", "commit", "-m", "Advance main"], cwd=self.repo_dir, check=True, capture_output=True)
        new_rev = subprocess.run(["git", "rev-parse", "HEAD"], cwd=self.repo_dir, check=True, capture_output=True, text=True)
        new_base_sha = new_rev.stdout.strip()

        # Rebind to advanced main without needing a new owner prompt
        continuation = contract.build_corrective_continuation(orig, base_sha=new_base_sha)
        self.assertEqual(continuation.proposal_sha256, orig.proposal_sha256)
        evt = self.journal.record_corrective_continuation(
            orig.mission_id, continuation.proposal_sha256, continuation.to_wire()
        )
        self.assertEqual(evt.event, "MISSION_CORRECTIVE_CONTINUATION")
        active = self.journal.active_mission_record()
        self.assertEqual(active.data.get("repository_identity", {}).get("base_sha"), new_base_sha)

    def test_10_restart_idempotent_corrective_continuation_preserves_evidence(self):
        """10. Restart/idempotent corrective continuation restores running state without dropping evidence."""
        ob = contract.AcceptanceObligation(
            obligation_id="common_rwe_evidence_basis",
            description="Test obligation",
            category="basis",
            dependencies=(),
            required_paths=("README.md",),
        )
        ledger = contract.MissionAcceptanceLedger((ob,))
        orig = self._setup_authenticated_mission()
        continuation = contract.build_corrective_continuation(orig, base_sha=self.base_sha, acceptance_ledger=ledger)
        self.journal.record_corrective_continuation(
            orig.mission_id, continuation.proposal_sha256, continuation.to_wire()
        )
        srv = service.StewardService(
            mission_id=orig.mission_id,
            journal=self.journal,
            github=self.github_writer,
            repo_path=self.repo_dir,
        )
        receipt = self._make_receipt("common_rwe_evidence_basis", classification="ACCEPTED_STATIC_BASIS", outcome="PASS", disposition="COMPLETE")
        srv.disposition_mission_obligation("common_rwe_evidence_basis", "COMPLETE", receipt)

        # Restart / re-run continuation idempotently
        continuation2 = contract.build_corrective_continuation(orig, base_sha=self.base_sha, acceptance_ledger=ledger)
        evt = self.journal.record_corrective_continuation(
            orig.mission_id, continuation2.proposal_sha256, continuation2.to_wire()
        )
        self.assertEqual(evt.event, "MISSION_CORRECTIVE_CONTINUATION")

        srv2 = service.StewardService(
            mission_id=orig.mission_id,
            journal=self.journal,
            github=self.github_writer,
            repo_path=self.repo_dir,
        )
        self.assertIsNotNone(srv2.mission.acceptance_ledger)
        ob_map = {ob.obligation_id: ob for ob in srv2.mission.acceptance_ledger.obligations}
        self.assertIn("common_rwe_evidence_basis", ob_map)
        self.assertEqual(ob_map["common_rwe_evidence_basis"].disposition, "COMPLETE")

    def test_11_research_mission_stays_research_pending_after_repair(self):
        """11. MISSION-RESEARCH-20260901 remains RESEARCH_PENDING after repair."""
        import run_research_mainline as rrm
        orig = self._setup_authenticated_mission(
            mission_id="MISSION-RESEARCH-20260901",
            allowed_paths=(
                "docs/ARCHITECTURE.md",
                "docs/AUTONOMY.md",
                "docs/ROADMAP.md",
                "docs/RUNBOOK.md",
                "engine/src/harness_evolution.rs",
                "engine/src/harness_evolution_eval.rs",
                "engine/src/rwe",
                "engine/src/storage/local_product_store",
                "scripts/agent-control",
                "tests/test_agent_shadow_steward.py",
                "tests/test_mission_contract.py",
            ),
        )
        # Create unauthenticated successor in journal
        self.journal.record_mission_activation(
            "MISSION-RESEARCH-20260901-SUCCESSOR",
            "0" * 64,
            {"mission_id": "MISSION-RESEARCH-20260901-SUCCESSOR"},
        )

        orig_run = subprocess.run
        def mock_subp(*args, **kwargs):
            cmd = args[0] if args else kwargs.get("args")
            if cmd and "cargo" in cmd:
                return MagicMock(returncode=0)
            return orig_run(*args, **kwargs)

        with patch("subprocess.run", side_effect=mock_subp):
            outcome = rrm.execute_research_mainline(
                self.journal_path,
                base_sha=self.base_sha,
                repo_path=self.repo_dir,
            )

        self.assertEqual(outcome["mission_id"], "MISSION-RESEARCH-20260901")
        self.assertEqual(outcome["mission_status"], "RESEARCH_PENDING")
        self.assertEqual(outcome["dispositions"]["common_rwe_evidence_basis"], "COMPLETE")
        self.assertEqual(outcome["dispositions"]["contemporary_rwe_replay"], "UNRESOLVED_LACK_OF_PROVIDER_EXECUTION")
        self.assertEqual(outcome["dispositions"]["mx1_c1_1x2x1"], "UNRESOLVED_UNEXECUTED_CELLS")
        self.assertEqual(outcome["dispositions"]["level_1"], "UNRESOLVED_PREDECESSOR_GATE_PENDING")

    def test_12_ordinary_maintenance_mission_behavior_unchanged(self):
        """12. Ordinary maintenance Mission behavior is unchanged."""
        req = "Ordinary maintenance mission for testing"
        proposal, digest = contract.compile_proposal_mission(
            req,
            repository="Igzela/token-efficient-agent-harness-lab",
            base_sha=self.base_sha,
            mission_id="MISSION-MAINTENANCE-TEST",
        )
        self.journal.consume_owner_approval(
            repository="Igzela/token-efficient-agent-harness-lab",
            mission_id="MISSION-MAINTENANCE-TEST",
            approval_id="test-approval-maint",
            proposal_sha256=digest,
            accepted_main_sha=self.base_sha,
        )
        evidence = contract.OwnerApprovalEvidence(
            transport="github_issue_comment",
            repository="Igzela/token-efficient-agent-harness-lab",
            mission_id="MISSION-MAINTENANCE-TEST",
            approval_id="test-approval-maint",
            owner_identity="github:Igzela",
            proposal_sha256=digest,
            accepted_main_sha=self.base_sha,
            evidence_id="comment-maint-1",
        )
        activated = contract.activate_current_mission(
            repository=proposal.repository_identity.repository,
            base_sha=proposal.repository_identity.base_sha,
            branch=proposal.repository_identity.branch,
            source_ref=proposal.repository_identity.source_ref,
            source_sha256=proposal.repository_identity.source_sha256,
            proposal_sha256=digest,
            owner_approval=contract.OwnerApproval(
                "github:Igzela", digest, "test-approval-maint", "2026-09-04T00:00:00Z"
            ),
            owner_authenticator=contract.AuthenticatedOwnerApprovalValidator(evidence),
            mission=proposal,
        )
        self.journal.record_mission_activation(
            activated.mission_id, activated.proposal_sha256, activated.to_wire()
        )
        srv = service.StewardService(
            mission_id=activated.mission_id,
            journal=self.journal,
            github=self.github_writer,
            repo_path=self.repo_dir,
        )
        self.assertIsNone(srv.mission.acceptance_ledger)
        res = srv.complete_mission_if_eligible()
        self.assertEqual(res["status"], "COMPLETE")
        self.assertEqual(srv.mission.state, "COMPLETE")


if __name__ == "__main__":
    unittest.main()
